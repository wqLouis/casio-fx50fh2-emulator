//! End-to-end tests for constant propagation and dead-code pruning.
//!
//! These go through the public [`transpile`] entry point, so the pass runs in
//! its real position in the pipeline (after `simplify`, before the second
//! `fold`) and the assertions pin the emitted PRGM. Programs are wrapped in
//! `fn main()` by [`common::wrap`].

use fx_transpiler::{Options, transpile};

mod common;

/// Transpile `source` (wrapped in `fn main`) and return the glyph-mode PRGM.
#[track_caller]
fn out(source: &str) -> String {
    let source = common::wrap(source);
    transpile(&source).unwrap_or_else(|e| panic!("transpile failed: {e}\n{source}"))
}

/// Assert the exact emitted PRGM for `source`.
#[track_caller]
fn prgm(source: &str, expected: &str) {
    assert_eq!(out(source), expected, "for source:\n{source}");
}

/// Transpile `source` and expect a diagnostic.
#[track_caller]
fn err(source: &str) -> fx_transpiler::error::TranspileError {
    let source = common::wrap(source);
    transpile(&source).unwrap_err()
}

// ---------------------------------------------------------------------------
// Propagation into a loop bound and an arithmetic use

#[test]
fn a_constant_reaches_a_loop_bound() {
    // `n` is a literal, so `i <= n` becomes `i <= 3` and the native `For` is
    // emitted with the literal bound instead of `To A`.
    prgm(
        "let n = 3; for (let i = 0; i <= n; i = i + 1) { print(i); }",
        "3→A\nFor 0→B To 3 Step 1\nB◢\nNext\n",
    );
}

#[test]
fn the_less_than_form_still_subtracts_one() {
    // The emitter turns `i < n` into `To n-1`; the propagated literal lets it
    // fold to `To 2` instead of leaving `A-1`.
    prgm(
        "let n = 3; for (let i = 0; i < n; i = i + 1) { print(i); }",
        "3→A\nFor 0→B To 2 Step 1\nB◢\nNext\n",
    );
}

#[test]
fn a_constant_folds_into_an_arithmetic_use() {
    // `k * 3` becomes `2 * 3`, which the second fold collapses to `6`, and the
    // store then has no readers.
    prgm(
        "let k = 2; print(k * 3);",
        "6◢
",
    );
}

#[test]
fn a_store_goes_once_its_last_read_is_propagated_away() {
    // The read becomes `3`, and the store then has nothing reading it. Running
    // the passes to a fixpoint is what lets the second one see the first one's
    // result.
    prgm(
        "let n = 3; print(n);",
        "3◢
",
    );
    // A name that is *assigned* is never propagated, so its store stays.
    prgm(
        "let n = input(); n = 3; print(n);",
        "?→A
3→A
A◢
",
    );
}

// ---------------------------------------------------------------------------
// Constant conditions

#[test]
fn a_true_condition_keeps_only_the_then_branch() {
    prgm("if (1) { print(9); } else { print(8); }", "9◢\n");
}

#[test]
fn a_false_condition_keeps_only_the_else_branch() {
    prgm("if (0) { print(9); } else { print(8); }", "8◢\n");
}

#[test]
fn a_condition_made_constant_by_propagation_is_decided() {
    // `n > 0` is not a literal until `n` is replaced by 3.
    prgm(
        "let n = 3; if (n > 0) { print(1); } else { print(2); }",
        "1◢
",
    );
}

#[test]
fn a_zero_while_disappears() {
    prgm("while (0) { print(1); } print(2);", "2◢\n");
}

#[test]
fn a_while_emptied_by_propagation_disappears() {
    // `a` is read by nothing after propagation, so its store goes, and then the
    // loop it fed is empty and goes too.
    prgm(
        "let a = 0; while (0) { print(a); } print(2);",
        "2◢
",
    );
}

// ---------------------------------------------------------------------------
// Unreachable code after a jump

#[test]
fn code_after_goto_is_pruned() {
    prgm(
        "goto 1; print(1); label 1; print(2);",
        "Goto 1\nLbl 1\n2◢\n",
    );
}

#[test]
fn a_declaration_after_goto_is_kept_for_allocation() {
    // A store nothing reads is deleted even though it is unreachable…
    prgm(
        "let a = 5; goto 1; label 1; print(2);",
        "Goto 1
Lbl 1
2◢
",
    );
    // …but one that *is* read afterwards keeps its place, because the memory a
    // later variable receives is observable.
    prgm(
        "let a = 5; goto 1; label 1; print(a);",
        "5→A
Goto 1
Lbl 1
A◢
",
    );
}

// ---------------------------------------------------------------------------
// Must NOT propagate: anything assigned, freed, or read before declaration

#[test]
fn a_name_assigned_in_a_loop_is_not_propagated() {
    prgm(
        "let n = 3; while (n < 5) { n = n + 1; } print(n);",
        "3→A\nWhile A<5\nA+1→A\nWhileEnd\nA◢\n",
    );
}

#[test]
fn a_name_assigned_in_a_branch_is_not_propagated() {
    prgm(
        "let n = 3; let c = input(); if (c) { n = 4; } print(n);",
        "3→A\n?→B\nIf B\nThen\n4→A\nIfEnd\nA◢\n",
    );
}

#[test]
fn a_name_assigned_in_a_for_update_is_not_propagated() {
    // The counter is written by the update clause, so reading it must stay a
    // memory read (otherwise `i < 3` would become a constant true).
    prgm(
        "for (let i = 0; i < 3; i = i + 1) { print(i); }",
        "For 0→A To 2 Step 1\nA◢\nNext\n",
    );
}

#[test]
fn an_array_element_assignment_is_left_alone() {
    // `v[0] = 9` must survive; it is not a store that propagation may touch.
    prgm(
        "let v[2] = {1, 2}; v[0] = 9; print(v[1]);",
        "1→A\n2→B\n9→A\nB◢\n",
    );
}

#[test]
fn a_write_makes_a_value_unknown() {
    // `n` is assigned, so it is never propagated — the blunt rule that keeps a
    // write inside a loop or a branch from being ignored.
    prgm(
        "let n = 3; print(n); n = 5; print(n);",
        "3→A
A◢
5→A
A◢
",
    );
    // A reassigned name inside a loop must keep reading the memory, or every
    // iteration after the first would print the first value.
    prgm(
        "let n = 3; while (n < 6) { print(n); n = n + 1; }",
        "3→A
While A<6
A◢
A+1→A
WhileEnd
",
    );
}

#[test]
fn a_name_read_before_its_declaration_is_not_propagated() {
    // `n` is only 3 after the declaration runs; the earlier read is a read of
    // the memory as it stood then.
    prgm("print(n); let n = 3;", "A◢\n3→A\n");
}

#[test]
fn a_name_declared_in_a_branch_is_not_used_after_it() {
    // The `let n` may not have run, so the read after the `if` stays a memory
    // read even though `n` is never assigned.
    prgm(
        "let c = input(); if (c) { let n = 3; } print(n);",
        "?→A\nIf A\nThen\n3→B\nIfEnd\nB◢\n",
    );
}

#[test]
fn input_ran_and_ans_are_not_constants() {
    prgm("let a = input(); print(a);", "?→A\nA◢\n");
    prgm("let r = ran(); print(r);", "Ran#→A\nA◢\n");
    prgm("let q = ans(); print(q);", "Ans→A\nA◢\n");
}

#[test]
fn a_let_is_not_propagated_into_a_const() {
    // A `const` must name a value fixed while transpiling. A `let` that
    // happens to be constant is still a runtime value, so the emitter must go
    // on rejecting it; propagating would weaken that check and save no bytes.
    let source = common::wrap("let x = 1;\nconst k = x;\nprint(k);\n");
    let error = transpile(&source).unwrap_err();
    assert!(
        error.message.contains("must be a constant expression"),
        "{error}"
    );
}

// ---------------------------------------------------------------------------
// Pruning is blocked when it would change the memory plan

#[test]
fn a_dead_branch_that_declares_is_kept() {
    // The branch cannot run, but it declares `a` and `a` is read afterwards, so
    // dropping it would hand `a` a different memory. `prune` refuses, and the
    // `If` stays — even though `if (0)` can never take the branch.
    prgm(
        "if (0) { let a = input(); } print(a);",
        "If 0
Then
?→A
IfEnd
A◢
",
    );
    // When the branch's declaration has no reader left, it is deleted first and
    // the branch then prunes normally.
    prgm(
        "if (1) { print(9); } else { let a = 4; print(a); } print(1);",
        "9◢
1◢
",
    );
}

#[test]
fn a_zero_while_that_declares_is_kept() {
    // `while (0)` never runs, but the body declares `a` and `a` is read after the
    // loop, so removing it would move `a` to another memory.
    prgm(
        "while (0) { let a = input(); } print(a);",
        "While 0
?→A
WhileEnd
A◢
",
    );
    // With no reader left, the declaration and the loop both go.
    prgm(
        "let a = 0; while (0) { print(a); } print(2);",
        "2◢
",
    );
}

// ---------------------------------------------------------------------------
// Dead stores

#[test]
fn a_never_read_declaration_disappears() {
    // A program's contract is its output: `unused` is never read, so its store
    // cannot affect what is displayed and is deleted.
    prgm("let unused = 1; print(2);", "2◢\n");
    prgm("let k = 42; print(7);", "7◢\n");
}

#[test]
fn a_dead_store_of_a_plain_assignment_disappears() {
    prgm("let x = 1; x = 2; print(3);", "3◢\n");
}

#[test]
fn a_dead_chain_collapses_to_a_fixpoint() {
    // `b` is removed first, which leaves `a` unread; a second round removes it.
    prgm("let a = 1; let b = a; print(2);", "2◢\n");
}

#[test]
fn a_never_read_array_disappears() {
    prgm("let v[2] = {1, 2}; print(3);", "3◢\n");
}

#[test]
fn a_freed_name_can_be_a_dead_store() {
    // `t` is released and never read, so the declaration and the `free` that
    // released it both go. That is only sound because the lifetime checks run on
    // the program *as written*, before this pass — so the errors below are still
    // reported rather than being deleted along with the declaration.
    prgm(
        "let t = 1; free t; print(2);",
        "2◢
",
    );
    prgm(
        "let t = 1; unsafe_free t; print(2);",
        "2◢
",
    );

    // The guard rails: an invalid program stays invalid, however unread its
    // names are.
    let error = err("let t = 1; free t; free t;");
    assert!(error.message.contains("double free"), "{}", error.message);
    let error = err("let t = 1; let t = 2; print(3);");
    assert!(
        error.message.contains("already declared"),
        "{}",
        error.message
    );
    // And a name used in its own initializer, which folding would otherwise turn
    // into a plain literal (`v - v` becomes `0`) and hide.
    let error = err("let v = (v - v); print(v);");
    assert!(
        error.message.contains("its own initializer"),
        "{}",
        error.message
    );
}

#[test]
fn re_declaring_a_name_blocks_elimination() {
    // A second `let` of the same name is a re-declaration error. `t` is never
    // read, so the dead-store pass would otherwise remove both declarations and
    // silently accept the program; because `t` is declared more than once it is
    // left alone, and the error survives.
    let error = err("let t = 1; let t = 2; print(3);");
    assert!(
        error.message.contains("already declared"),
        "{}",
        error.message
    );
}

#[test]
fn a_name_declared_again_after_a_free_can_be_dead() {
    // Both lives are unread, so both stores and the `free` between them go.
    prgm(
        "let t = 1; free t; let t = 2; print(3);",
        "3◢
",
    );
}

#[test]
fn a_read_after_a_free_uses_the_second_binding() {
    // The read belongs to the *second* `t`, so it becomes `2` — not `1`, and not
    // an ambiguous "some `t`". Then the stores have no readers left and go.
    prgm(
        "let t = 1; free t; let t = 2; print(t);",
        "2◢
",
    );
}

#[test]
fn each_life_of_a_name_propagates_its_own_value() {
    // Propagation is per *binding*, not per name: the `free` ends the first
    // `n`'s life, so the second `let n` is a fresh variable with its own value.
    // A single value per name could not describe this.
    prgm(
        "let n = 3; print(n); free n; let n = 5; print(n);",
        "3◢
5◢
",
    );
}

#[test]
fn a_name_with_any_impure_store_is_left_entirely_alone() {
    // One life is `input()` (observable), so the name is not dead and the
    // binding pair must survive intact.
    prgm(
        "let t = input(); free t; let t = 1; print(2);",
        "?→A\n1→A\n2◢\n",
    );
}

#[test]
fn a_dead_store_inside_a_body_is_reached() {
    // Removing `x` empties the loop body, and then the constant `while (0)`
    // disappears entirely.
    prgm("while (0) { let x = 1; } print(2);", "2◢\n");
    // Likewise inside a branch: the empty taken branch replaces the `if`.
    prgm("if (1) { let x = 1; } print(2);", "2◢\n");
}

#[test]
fn an_empty_array_and_its_free_disappear_together() {
    prgm("let v[2]; free v; print(3);", "3◢\n");
}

#[test]
fn input_ran_and_a_call_are_never_removed() {
    // The prompt is observable.
    prgm("let a = input(); print(2);", "?→A\n2◢\n");
    // `ran()` advances the random sequence.
    prgm("let r = ran(); print(2);", "Ran#→A\n2◢\n");
    // Any call may raise `Math ERROR`, so it is kept even when unread.
    prgm("let s = sqrt(4); print(2);", "√(4)→A\n2◢\n");
}

#[test]
fn ans_and_mvalue_are_never_removed() {
    prgm("let q = ans(); print(2);", "Ans→A\n2◢\n");
    prgm("let m = mvalue(); print(2);", "M→A\n2◢\n");
}

#[test]
fn a_name_read_in_a_loop_is_not_removed() {
    // `n` is a loop bound, so it is read and its store survives even though
    // propagation later replaces the bound with the literal 3.
    prgm(
        "let n = 3; for (let i = 0; i < n; i = i + 1) { print(1); }",
        "3→A\nFor 0→B To 2 Step 1\n1◢\nNext\n",
    );
}

#[test]
fn a_name_read_in_a_branch_is_not_removed() {
    prgm(
        "let n = 3; let c = input(); if (c) { print(n); }",
        "3→A\n?→B\nIf B\nThen\n3◢\nIfEnd\n",
    );
}

// ---------------------------------------------------------------------------
// Diagnostics are settled before the optimiser runs
//
// The optimiser deletes code, and some of that code is what a diagnostic is
// about. These check the invariant that keeps that from hiding errors: **if the
// raw translation is rejected, the optimised one is rejected too**. Both were
// real bugs — each made an invalid program silently accepted.

/// A raw program that is rejected must stay rejected however the optimiser
/// changes it. This is the contract `Allocator::validate` buys by running on the
/// program as written.
#[test]
fn optimisation_never_legalises_an_invalid_program() {
    let cases: &[(&str, &str)] = &[
        ("a double free", "let t = 1; free t; free t;"),
        ("a re-declaration", "let t = 1; let t = 2; print(3);"),
        (
            "a self-referential initializer",
            "let v = (v - v); print(v);",
        ),
        (
            "a self-referential initializer inside a sum",
            "let v = (v / 1) - v;",
        ),
        ("reading a freed name", "let t = 1; free t; print(t);"),
        ("freeing an unknown name", "free nope;"),
        ("freeing a const", "const k = 1; free k;"),
        (
            "a checked free inside a loop",
            "while (1 < 2) { let t = 1; print(t); free t; }",
        ),
        ("indexing a scalar", "let x = 1; print(x[0]);"),
        (
            "an index past the end of an array",
            "let v[3] = {1,2,3}; print(v[3]);",
        ),
    ];
    for (what, source) in cases {
        let raw = fx_transpiler::transpile_with(
            &common::wrap(source),
            Options {
                ascii: false,
                mode: None,
                optimize: false,
            },
        );
        assert!(raw.is_err(), "{what} should be rejected unoptimised");
        let optimised = fx_transpiler::transpile(&common::wrap(source));
        assert!(
            optimised.is_err(),
            "{what} was accepted once optimised: {optimised:?}"
        );
    }
}

/// The counterpart: a program whose *memory plan* only fits after optimisation
/// must still compile, which is why the pre-check deliberately ignores running
/// out of memory.
#[test]
fn memory_pressure_is_still_checked_after_optimising() {
    // Eight names, but nothing reads them and the program ends in a display, so
    // the stores go and the program fits in one memory — none.
    prgm(
        "let a=1; let b=1; let c=1; let d=1; let x=1; let y=1; let z=1; print(9);",
        "9◢\n",
    );
    // Eight names that each have to occupy a memory cannot fit. `input()` keeps
    // them: it is not side-effect free, so neither the stores nor the reads can
    // be removed.
    let error = err(
        "let a=input(); print(a); let b=input(); print(b); let c=input(); print(c); \
         let d=input(); print(d); let x=input(); print(x); let y=input(); print(y); \
         let m=input(); print(m); let z=input(); print(z);",
    );
    assert!(
        error.message.contains("no free memory"),
        "{}",
        error.message
    );
}

// ---------------------------------------------------------------------------
// What the program displays

/// Removing statements can change which one the machine displays, when the
/// program has no `◢` of its own. The optimiser must leave such a program alone.
#[test]
fn an_implicitly_displayed_ending_is_left_alone() {
    // The last computed value is the (false) `if` condition, so the program
    // displays `0`. Deleting the unread store and pruning the dead `if` would
    // leave `11.25` as the last computed value instead.
    // Propagation still runs inside the branch — it is safe, because it
    // substitutes an *equal* value — but the store and the dead `if` stay, so
    // the value the machine computes last is still the condition, `0`.
    prgm(
        "let a = 11.25; let b = a - a; if (0 > 2) { print(a); }",
        "11.25→A\n0→B\nIf 0\nThen\n11.25◢\nIfEnd\n",
    );
    // Ending in a loop is the same problem.
    prgm(
        "let a = 1; while (0) { print(1); }",
        "1→A\nWhile 0\n1◢\nWhileEnd\n",
    );
    // Once the program ends in a display, trimming is allowed again.
    prgm(
        "let a = 11.25; let b = a - a; if (0 > 2) { print(a); } print(7);",
        "7◢\n",
    );
}

/// `Ans` reads the hidden result memory, which evaluating any expression
/// updates, so no store is local in a program that consults it.
#[test]
fn a_program_that_reads_ans_keeps_its_stores() {
    prgm("let a = 6 * 7; print(ans());", "42→A\nAns◢\n");
}

/// The display rule the guard above exists for.
///
/// PRGM shows the value of the last *value-producing* statement it executed —
/// a store, a bare expression or a `print` — and a control statement is not one.
/// So a trailing loop does **not** take over from the store before it, which is
/// why a program ending in a loop is left alone rather than trimmed.
#[test]
fn a_trailing_control_statement_does_not_take_over_the_display() {
    // `5→A` is the last value produced: the `While` is control flow. Removing
    // the store would leave the loop's `1◢` as the last value instead.
    prgm(
        "let a = 5; while (0) { print(1); }",
        "5→A\nWhile 0\n1◢\nWhileEnd\n",
    );
    // A trailing `if` with no `else`, likewise.
    prgm(
        "let a = 5; if (0) { print(1); }",
        "5→A\nIf 0\nThen\n1◢\nIfEnd\n",
    );
    // The loop *body* counts too: control can fall out of it into the end.
    prgm("while (0) { let a = 5; }", "While 0\n5→A\nWhileEnd\n");
    // Ending in a display is what makes trimming safe.
    prgm("let a = 5; while (0) { print(1); } print(2);", "2◢\n");
}

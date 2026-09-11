//! End-to-end tests for constant propagation and dead-code pruning.
//!
//! These go through the public [`transpile`] entry point, so the pass runs in
//! its real position in the pipeline (after `simplify`, before the second
//! `fold`) and the assertions pin the emitted PRGM. Programs are wrapped in
//! `fn main()` by [`common::wrap`].

use fx_transpiler::transpile;

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
    // `k * 3` becomes `2 * 3`, which the second fold collapses to `6`.
    prgm("let k = 2; print(k * 3);", "2→A\n6◢\n");
}

#[test]
fn a_store_to_a_read_name_is_kept() {
    // `n` is read, so its store stays; the read is then replaced by the value.
    prgm("let n = 3; print(n);", "3→A\n3◢\n");
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
        "3→A\n1◢\n",
    );
}

#[test]
fn a_zero_while_disappears() {
    prgm("while (0) { print(1); } print(2);", "2◢\n");
}

#[test]
fn a_while_emptied_by_propagation_disappears() {
    // `n` is the constant 0, so `while (n)` is `while (0)`.
    prgm("let n = 0; while (n) { print(1); } print(2);", "0→A\n2◢\n");
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
    // The store cannot run, but deleting `let x` would move every later
    // variable to a different memory, which the user can observe.
    prgm(
        "goto 1; let x = 5; print(x); label 1; print(2);",
        "Goto 1\n5→A\nLbl 1\n2◢\n",
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
fn a_name_redeclared_after_free_is_not_propagated() {
    // The second `let n` is a different binding from the first, so neither
    // life is treated as the constant 3/5.
    prgm(
        "let n = 3; print(n); free n; let n = 5; print(n);",
        "3→A\nA◢\n5→A\nA◢\n",
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
    // Dropping the `else` would hand a later variable a different memory, so
    // the whole `If` stays. The reads inside it are still propagated.
    prgm(
        "if (1) { print(9); } else { let dead = 4; print(dead); } print(1);",
        "If 1\nThen\n9◢\nElse\n4→A\n4◢\nIfEnd\n1◢\n",
    );
}

#[test]
fn a_zero_while_that_declares_is_kept() {
    prgm(
        "while (0) { let x = 1; print(x); } print(2);",
        "While 0\n1→A\n1◢\nWhileEnd\n2◢\n",
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
fn a_freed_name_is_not_a_dead_store() {
    // A `free` is a *claim* about a lifetime, so the declaration it refers to is
    // never eliminated: removing it would silently legalise a broken program.
    // The declaration, its store and the release all stay in place.
    prgm("let t = 1; free t; print(2);", "1→A\n2◢\n");
    prgm("let t = 1; unsafe_free t; print(2);", "1→A\n2◢\n");
    // The double free must still be reported, which is the whole point.
    let error = err("let t = 1; free t; free t;");
    assert!(error.message.contains("double free"), "{}", error.message);
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
fn a_name_freed_and_declared_again_is_not_a_dead_store() {
    // Two lives across a `free`: eliminating either would change which lifetime
    // the `free` releases.
    prgm("let t = 1; free t; let t = 2; print(3);", "1→A\n2→A\n3◢\n");
}

#[test]
fn a_free_keeps_its_declaration_while_the_name_is_read() {
    // `t` is read, so neither life of the binding may be removed, and the two
    // `free`/`let` pairs stay balanced.
    prgm("let t = 1; free t; let t = 2; print(t);", "1→A\n2→A\nA◢\n");
}

#[test]
fn both_lives_of_a_freed_name_are_kept() {
    // The name is declared twice across a `free`, so it is neither a single
    // declaration nor unreleased: the pass leaves it entirely alone.
    prgm("let t = 1; free t; let t = 2; print(3);", "1→A\n2→A\n3◢\n");
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

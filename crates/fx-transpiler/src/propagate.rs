//! Constant propagation and pruning of unreachable code.
//!
//! The machine has **680 bytes of program storage shared by all four program
//! areas**, one byte per key, so every value the transpiler can work out itself
//! is a value the calculator does not have to compute. This pass moves that
//! work to transpile time.
//!
//! ## What it propagates
//!
//! A `let` whose initialiser is already a literal, and whose name is **never
//! assigned anywhere**, holds the same value for the whole of that binding's
//! life. So every *read* of the name can be replaced by the literal, and the
//! surrounding arithmetic is then folded by the second [`crate::fold`] run:
//!
//! ```c
//! let n = 3;
//! for (let i = 0; i <= n; i = i + 1) { print(i); }
//! // For 0→A To 3 Step 1 | A◢ | Next
//!
//! let k = 2;
//! print(k * 3);
//! // 6◢                 (and the store goes too — nothing reads `k` any more)
//! ```
//!
//! Only `let` bindings whose initialiser has already been folded to a number
//! qualify. A symbolic initialiser (`2 * pi`) is left alone: folding it to a
//! decimal loses precision and costs *more* bytes, not fewer
//! ([ADR 0015](../../../docs/DECISIONS.md)).
//!
//! ### The tracking is per binding, not per name
//!
//! The value is carried in the flow state, so it follows the program's control
//! flow and ends when the binding does:
//!
//! ```c
//! let n = 3;
//! print(n);      // → 3◢
//! free n;        // the first `n` ends here
//! let n = 5;     // a *fresh* variable that happens to share the name
//! print(n);      // → 5◢
//! ```
//!
//! A single value per name could not describe that, and would have to refuse to
//! propagate any name that is freed or declared twice. Branching is handled by
//! keeping only what every path agrees on: an `if`/`else` keeps a name that both
//! branches still have *with the same value*, and a loop keeps only what the
//! body leaves untouched — so a value first established inside a loop, or
//! changed by one, is not known after it. A `label` clears the state, because a
//! `goto` can re-enter anywhere.
//!
//! ### A name that is ever assigned is never propagated
//!
//! This one rule is deliberately blunt, and it is what makes the rest safe. If a
//! name is the target of an assignment anywhere — including inside a loop or a
//! branch, and including the counter of a `for` — then no read of it is ever
//! replaced:
//!
//! ```c
//! let n = 3;
//! while (n < 6) { print(n); n = n + 1; }
//! // 3→A | While A<6 | A◢ | A+1→A | WhileEnd
//! ```
//!
//! Were `n` propagated, `print(n)` would become `print(3)` and every iteration
//! after the first would print `3`. A name that is only ever *declared* cannot
//! change value, so substituting it is always safe; a name that is assigned
//! would need a full reaching-definitions analysis, and the saving does not
//! justify one.
//!
//! ## A store to a name that is never read is removed
//!
//! **A program's contract is its output.** A store that no read can observe
//! cannot change a value the program displays, so it is deleted rather than
//! left to cost keys: `let unused = 1; print(2);` becomes just `2◢`. This is
//! the default — there is no flag to turn it on.
//!
//! A store is removed only when **both** of these hold:
//!
//! * its name is never *read* anywhere — no `Expr::Name` reference, no array
//!   index, no `for` bound or update, no argument to a call; and
//! * its value is side-effect free (see below).
//!
//! Because propagation runs first and to a fixpoint with this pass, replacing a
//! read with its value is often what makes the store removable — `let n = 3;
//! print(n);` loses both.
//!
//! ### Diagnostics are settled before any of this runs
//!
//! Removing the declaration an error is about would silently legalise the
//! program, and that has happened twice: `let t = 1; free t; free t;` stopped
//! being a double-free error once unread `t` was deleted, and `let v = (v - v);`
//! stopped being a self-reference error once `simplify` folded `v - v` to `0`.
//!
//! So the rule is not "be careful here" but **an ordering**:
//! [`crate::transpile`] runs [`crate::alloc::Allocator::validate`] — the
//! binding-validity half of the allocator, with memory pressure deliberately
//! ignored — on the program *as written*, before any optimisation. By the time
//! this pass runs, the program is known to be valid, so nothing it deletes can
//! hide a diagnostic.
//!
//! The invariant that falls out is worth stating: **if the unoptimised
//! translation is rejected, the optimised one is rejected too.** A test pins it
//! for a double free, a re-declaration, a self-referential initializer, a use
//! after free, freeing a `const`, a checked `free` inside a loop, and an array
//! index out of range.
//!
//! ### Two things still make a store observable
//!
//! `A program's contract is its output` is the licence to delete stores, so both
//! are checked before this pass will delete any:
//!
//! * **`Ans`.** Evaluating any expression updates the hidden result memory that
//!   `ans()` reads, so no store is local in a program that consults it.
//! * **What the program displays.** PRGM shows the last value it computed when a
//!   program ends without `◢`, and removing statements can change which one that
//!   is. [`ends_with_display`] therefore gates *both* this pass and pruning: if
//!   the program does not end in a display on every path, neither runs. The bug
//!   this prevents:
//!
//!   ```c
//!   let a = 11.25;
//!   let b = a - a;           // never read
//!   if (0 > 2) { print(a); } // false, so nothing is displayed
//!   // raw:        displays the condition, 0
//!   // over-eager: drops both statements and displays 11.25
//!   ```
//!
//!   Propagation stays allowed there: substituting an equal value cannot change
//!   what the machine computes last.
//!
//! Values that prompt or advance machine state are never removed: `input()`
//! (the prompt is observable), `ran()` (it advances the random sequence),
//! `ans`/`mvalue`, `mplus`/`mminus`, and any call that can raise `Math ERROR`
//! or has state. A literal, a `const`, a `#data` path, `pi`/`e`/`phys.*`,
//! `stat.*`, or pure arithmetic over those is removable.
//!
//! ## What it prunes
//!
//! * **A constant condition is decided now.** `if (1) { … } else { … }` emits
//!   only the taken branch, and `while (0) { … }` disappears entirely. The
//!   condition is folded first, so `let n = 3; if (n > 0) { … }` is decided
//!   too. Only statements the machine could never execute are removed.
//! * **Code after an unconditional jump is unreachable** — after `goto n;` or
//!   `return`, up to the next `label`/end of body.
//!
//! Pruning is guarded by [`affects_allocation`]: a dead branch is dropped only
//! when it declares nothing, frees nothing, and names no label or the fixed `M`
//! memory, so a read *outside* the branch cannot be left without its
//! declaration. (Stores to names that are never read are already gone by the
//! time a branch is judged, so an empty dead branch does disappear.) When a
//! branch cannot be dropped, the `If`/`While` structure is left exactly as it
//! was.
//!
//! ## What must NOT happen
//!
//! * Do not propagate a name that is assigned anywhere, including inside a
//!   loop, a branch, or a `for` update, and including an assignment to an
//!   *element* of an array of the same name. (The `assigned` gate above is how
//!   this is enforced.)
//! * Do not touch `input()`, `ran()`, `ans`, `mvalue`, or anything mode- or
//!   state-dependent; they are not constants and their stores are never
//!   removed.
//! * Do not remove a store whose value is not side-effect free, or while `Ans`
//!   is read, or when removing it could change what the program displays.
//! * Do not reorder anything side-effecting (`print`, `input`, `dt`, `mplus`,
//!   the setup statements).
//! * Anything involving a symbolic constant (`pi`, `e`, `phys.`) is not a value
//!   to propagate as a decimal ([ADR 0015](../../../docs/DECISIONS.md)).
//! * Do not assume this pass is the thing that checks the program. It runs
//!   *after* the check, and must stay that way.

use std::collections::{BTreeMap, BTreeSet};

use crate::ast::{Accessor, Expr, Program, Stmt};
use crate::fold;

/// Propagate constants and prune unreachable code in `program`, in place.
///
/// Runs after [`crate::simplify`] and before the second [`crate::fold`] run.
///
/// The passes run to a fixpoint, because they feed each other: replacing a read
/// with its value can leave a store with nothing reading it, and deleting that
/// store can leave an earlier read with nothing to feed.
pub(crate) fn propagate_program(program: &mut Program) {
    // Two properties of the program decide how much may be removed. Both are
    // checked on the program *as written*, and neither can be broken by a
    // removal, so they are computed once.
    //
    // 1. **What the program displays.** PRGM shows the value of the last
    //    *value-producing* statement it executed when a program ends without
    //    `◢` — so a store, a bare expression or a `print`, but not a control
    //    statement. `5→A:While 0:1◢:WhileEnd` therefore displays `5`: the loop
    //    is control flow, so the store before it is still the last value
    //    produced.
    //
    //    That makes *both* passes able to change the answer, because removing a
    //    store can hand the role to an earlier statement and pruning can remove
    //    the one that held it. So one condition gates both: the program must end
    //    in a display on every path, since a display is never removed.
    //
    //    This is deliberately conservative. A program ending in a loop that
    //    prints is *usually* fine to trim, but whether the loop body ran is a
    //    run-time question, so the transpiler does not try to answer it.
    // 2. **Whether `Ans` is read.** Evaluating any expression updates the hidden
    //    result memory `Ans` reads, so no store is purely local in a program
    //    that consults `Ans`. This gates dead-store elimination only: pruning a
    //    branch that cannot run evaluates nothing.
    //
    // Propagation is always allowed: it replaces a read with an *equal* value,
    // so the value the machine computes cannot change.
    let may_prune = ends_with_display(program);
    let may_drop_stores = may_prune && !uses_ans(program);

    loop {
        let mut changed = false;
        if may_drop_stores {
            changed |= eliminate_dead_stores(program);
        }
        changed |= propagate_reads(program);
        if may_prune {
            changed |= prune(program);
        }
        if !changed {
            return;
        }
    }
}

/// Whether every path through the program ends by displaying a value.
///
/// A program that ends in a display has a fixed answer whatever the optimiser
/// removes: `◢` is never deleted, so the displayed value is still the last one
/// computed. Anything else — a store, a bare expression, a loop, an `if` with no
/// `else` — leaves the last computed value implicit, and removing a statement
/// can change it. That is a real bug, not a theoretical one:
///
/// ```c
/// let a = 11.25;
/// let b = a - a;          // never read
/// if (0 > 2) { print(a); } // false, so nothing is displayed
/// // raw:       displays the condition, 0
/// // over-eager: drops both statements and displays `11.25`
/// ```
///
/// An `if` counts only when **both** branches are present and both end in a
/// display: with no `else`, the condition being false means no branch runs, so
/// the last value produced is from some earlier statement.
///
/// A loop never counts. Its body may run any number of times, including none, so
/// whether a display inside it was the last value produced is a run-time
/// question.
fn ends_with_display(stmts: &[Stmt]) -> bool {
    let Some(last) = stmts.last() else {
        return false;
    };
    match last {
        Stmt::Print(_) => true,
        Stmt::Block(body) => ends_with_display(body),
        Stmt::If {
            then_body,
            else_body,
            ..
        } => {
            !then_body.is_empty()
                && !else_body.is_empty()
                && ends_with_display(then_body)
                && ends_with_display(else_body)
        }
        _ => false,
    }
}

/// The number of statements in a program, nested bodies included.
///
/// The passes only ever remove statements, so comparing this before and after
/// tells them whether to loop again without threading a flag through every
/// function.
fn count_stmts(stmts: &[Stmt]) -> usize {
    stmts
        .iter()
        .map(|stmt| {
            1 + match stmt {
                Stmt::If {
                    then_body,
                    else_body,
                    ..
                } => count_stmts(then_body) + count_stmts(else_body),
                Stmt::While { body, .. } => count_stmts(body),
                Stmt::For(for_stmt) => count_stmts(&for_stmt.body),
                Stmt::Block(body) => count_stmts(body),
                Stmt::CondJump { target, .. } => count_stmts(std::slice::from_ref(target)),
                _ => 0,
            }
        })
        .sum()
}

/// Rewrite every read of a name whose value is known here.
///
/// Returns whether anything changed.
fn propagate_reads(program: &mut Program) -> bool {
    // A name that is *assigned* anywhere is never propagated, at any point in
    // the program. That is deliberately blunt, and it is what keeps a write
    // inside a loop or a branch from being ignored:
    //
    // ```c
    // let n = 3;
    // while (n < 5) { print(n); n = n + 1; }
    // ```
    //
    // Were `n` propagated, `print(n)` would become `print(3)` and the second
    // iteration would print `3` again. Because `n` is assigned somewhere it is
    // never substituted, and the loop is left alone. A name that is only ever
    // *declared* cannot change value, so substituting it is always safe.
    let assigned = assigned_names(program);
    let mut state = Values::new();
    propagate_stmts(program, &assigned, &mut state)
}

/// The names that are the target of an assignment anywhere in the program,
/// including a `for` counter or update clause.
fn assigned_names(program: &Program) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    collect_assigned(program, &mut out);
    out
}

fn collect_assigned(stmts: &[Stmt], out: &mut BTreeSet<String>) {
    for stmt in stmts {
        match stmt {
            Stmt::Assign { name, .. }
            | Stmt::AssignElement { name, .. }
            | Stmt::AssignElementExpr { name, .. } => {
                out.insert(name.clone());
            }
            Stmt::For(for_stmt) => {
                out.insert(for_stmt.init_name.clone());
                out.insert(for_stmt.update_name.clone());
                collect_assigned(&for_stmt.body, out);
            }
            Stmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_assigned(then_body, out);
                collect_assigned(else_body, out);
            }
            Stmt::While { body, .. } => collect_assigned(body, out),
            Stmt::Block(body) => collect_assigned(body, out),
            Stmt::CondJump { target, .. } => collect_assigned(std::slice::from_ref(target), out),
            _ => {}
        }
    }
}

/// The value known for a name at a point, if any.
type Values = BTreeMap<String, f64>;

/// Keep only the names `a` and `b` agree on, so a join cannot claim a value
/// that only one incoming path established.
fn intersect(a: &Values, b: &Values) -> Values {
    a.iter()
        .filter(|(name, value)| b.get(*name) == Some(*value))
        .map(|(name, value)| (name.clone(), *value))
        .collect()
}

/// The shared implementation: walk a statement list, rewriting reads and
/// tracking which values are known. Returns whether any read was rewritten.
fn propagate_stmts(stmts: &mut [Stmt], assigned: &BTreeSet<String>, state: &mut Values) -> bool {
    let mut changed = false;
    for stmt in stmts {
        changed |= propagate_stmt(stmt, assigned, state);
    }
    changed
}

fn propagate_stmt(stmt: &mut Stmt, assigned: &BTreeSet<String>, state: &mut Values) -> bool {
    match stmt {
        Stmt::Let { name, value, .. } => {
            // The initialiser runs before the name takes effect, so it is
            // rewritten against the state as it stands.
            let changed = propagate_expr(value, assigned, state);
            // A declaration binds a value of its own. `free x; let x = 60;` is a
            // *fresh* `x`, which is exactly why the tracking is per-binding
            // rather than per-name: the old value was removed by the `free`.
            match value {
                Expr::Number(number) if !assigned.contains(name) => {
                    state.insert(name.clone(), *number);
                }
                _ => {
                    state.remove(name);
                }
            }
            changed
        }
        Stmt::LetArray { values, .. } => {
            let mut changed = false;
            for value in values {
                changed |= propagate_expr(value, assigned, state);
            }
            changed
        }
        // A `const` is the programmer's promise that the value is fixed while
        // transpiling. Propagating a `let` into it would make the emitter accept
        // a `const` that names a runtime variable, weakening that check for no
        // byte saving — a `const` is inlined and uses no memory.
        Stmt::Const { .. } => false,
        Stmt::Assign { name, value, .. } => {
            let changed = propagate_expr(value, assigned, state);
            // A write makes the value unknown. Recording the new one instead
            // would be sound on a straight line but not across a jump, and the
            // saving is marginal.
            state.remove(name);
            changed
        }
        Stmt::AssignElement { name, value, .. } => {
            let changed = propagate_expr(value, assigned, state);
            state.remove(name);
            changed
        }
        Stmt::AssignElementExpr {
            name, index, value, ..
        } => {
            let mut changed = propagate_expr(index, assigned, state);
            changed |= propagate_expr(value, assigned, state);
            state.remove(name);
            changed
        }
        // Releasing a name ends that binding, so its value is no longer known:
        // the next `let` of the same name is a different variable.
        Stmt::Free { name, .. } => {
            state.remove(name);
            false
        }
        Stmt::Memory { value, .. } => propagate_expr(value, assigned, state),
        Stmt::Data { x, y, freq, .. } => {
            let mut changed = propagate_expr(x, assigned, state);
            if let Some(y) = y {
                changed |= propagate_expr(y, assigned, state);
            }
            if let Some(freq) = freq {
                changed |= propagate_expr(freq, assigned, state);
            }
            changed
        }
        // A `⇒` target runs only when the condition holds, so it is analysed in
        // a copy of the state and cannot affect what follows.
        Stmt::CondJump { cond, target, .. } => {
            let changed = propagate_expr(cond, assigned, state);
            let mut inner = state.clone();
            changed | propagate_stmt(target, assigned, &mut inner)
        }
        Stmt::Print(expr) | Stmt::ExprStmt(expr) => propagate_expr(expr, assigned, state),
        Stmt::If {
            cond,
            then_body,
            else_body,
        } => {
            let changed = propagate_expr(cond, assigned, state);
            let entry = state.clone();
            let mut then_state = entry.clone();
            let then_changed = propagate_stmts(then_body, assigned, &mut then_state);
            let mut else_state = entry.clone();
            let else_changed = propagate_stmts(else_body, assigned, &mut else_state);
            *state = intersect(&then_state, &else_state);
            changed | then_changed | else_changed
        }
        Stmt::While { cond, body } => {
            let changed = propagate_expr(cond, assigned, state);
            let entry = state.clone();
            let mut body_state = entry.clone();
            let body_changed = propagate_stmts(body, assigned, &mut body_state);
            // The body may run zero times, so nothing it established is known
            // afterwards — and anything it *released* is not known either, which
            // the intersection handles because `free` removes the name.
            *state = intersect(&entry, &body_state);
            changed | body_changed
        }
        Stmt::For(for_stmt) => {
            let mut changed = propagate_expr(&mut for_stmt.init_value, assigned, state);
            // The counter is rewritten by the update clause, so it is never a
            // known value inside the loop.
            state.remove(&for_stmt.init_name);
            changed |= propagate_expr(&mut for_stmt.cond, assigned, state);
            let entry = state.clone();
            let mut body_state = entry.clone();
            let mut body_changed = propagate_stmts(&mut for_stmt.body, assigned, &mut body_state);
            body_changed |= propagate_expr(&mut for_stmt.update_value, assigned, &body_state);
            *state = intersect(&entry, &body_state);
            changed | body_changed
        }
        Stmt::Block(stmts) => propagate_stmts(stmts, assigned, state),
        Stmt::Return { value, .. } => match value {
            Some(value) => propagate_expr(value, assigned, state),
            None => false,
        },
        // A jump can re-enter from anywhere, so nothing established before the
        // label can be trusted after it.
        Stmt::Label(..) => {
            state.clear();
            false
        }
        Stmt::Goto(..)
        | Stmt::Setup { .. }
        | Stmt::ClrMemory
        | Stmt::ClrStat
        | Stmt::FreqOn
        | Stmt::FreqOff
        | Stmt::Break
        | Stmt::Empty
        | Stmt::Function(_) => false,
    }
}

/// Replace a read of a known name with its value, recursing into every
/// subexpression. Returns whether anything changed.
fn propagate_expr(expr: &mut Expr, assigned: &BTreeSet<String>, state: &Values) -> bool {
    match expr {
        Expr::Name(name, _) => {
            if !assigned.contains(name)
                && let Some(value) = state.get(name)
            {
                *expr = Expr::Number(*value);
                return true;
            }
            false
        }
        Expr::Unary(_, inner) => propagate_expr(inner, assigned, state),
        Expr::Binary(_, left, right) => {
            let changed = propagate_expr(left, assigned, state);
            changed | propagate_expr(right, assigned, state)
        }
        Expr::Call(_, args, _) => {
            let mut changed = false;
            for arg in args {
                changed |= propagate_expr(arg, assigned, state);
            }
            changed
        }
        Expr::Data { accessors, .. } => {
            let mut changed = false;
            for accessor in accessors {
                if let Accessor::IndexExpr { expr, .. } = accessor {
                    changed |= propagate_expr(expr, assigned, state);
                }
            }
            changed
        }
        Expr::Number(_)
        | Expr::BaseLiteral { .. }
        | Expr::Pi(_)
        | Expr::E(_)
        | Expr::Ans(_)
        | Expr::StatVar(..)
        | Expr::Constant(..)
        | Expr::Input(_) => false,
    }
}

// ---------------------------------------------------------------------------
// Dead stores

/// Delete declarations and stores whose name is never read.
///
/// Removal is iterated to a fixpoint: dropping `let b = a;` can leave `a`
/// unread, so `let a = …;` becomes removable on the next round. A pass that
/// removes nothing ends the loop.
fn eliminate_dead_stores(program: &mut Program) -> bool {
    let before = count_stmts(program);
    loop {
        let dead = DeadStores::collect(program).dead();
        if dead.is_empty() {
            break;
        }
        let taken = std::mem::take(program);
        *program = remove_dead_stores(taken, &dead);
    }
    count_stmts(program) != before
}

/// Whether the program reads `Ans` anywhere.
///
/// Evaluating an expression stores its result in the hidden result memory that
/// `Ans` reads, so a store is never purely local in a program that consults
/// `Ans`: removing `6*7→A` also removes the `42` that a later `ans()` sees.
fn uses_ans(program: &Program) -> bool {
    program.iter().any(stmt_uses_ans)
}

fn stmt_uses_ans(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Let { value, .. }
        | Stmt::Const { value, .. }
        | Stmt::Assign { value, .. }
        | Stmt::AssignElement { value, .. }
        | Stmt::Print(value)
        | Stmt::ExprStmt(value)
        | Stmt::Memory { value, .. } => expr_uses_ans(value),
        Stmt::LetArray { values, .. } => values.iter().any(expr_uses_ans),
        Stmt::AssignElementExpr { index, value, .. } => {
            expr_uses_ans(index) || expr_uses_ans(value)
        }
        Stmt::Return { value, .. } => value.as_ref().is_some_and(expr_uses_ans),
        Stmt::Function(def) => def.body.iter().any(stmt_uses_ans),
        Stmt::Data { x, y, freq, .. } => {
            expr_uses_ans(x)
                || y.as_ref().is_some_and(expr_uses_ans)
                || freq.as_ref().is_some_and(expr_uses_ans)
        }
        Stmt::CondJump { cond, target, .. } => expr_uses_ans(cond) || stmt_uses_ans(target),
        Stmt::If {
            cond,
            then_body,
            else_body,
        } => {
            expr_uses_ans(cond)
                || then_body.iter().any(stmt_uses_ans)
                || else_body.iter().any(stmt_uses_ans)
        }
        Stmt::While { cond, body } => expr_uses_ans(cond) || body.iter().any(stmt_uses_ans),
        Stmt::For(for_stmt) => {
            expr_uses_ans(&for_stmt.init_value)
                || expr_uses_ans(&for_stmt.cond)
                || expr_uses_ans(&for_stmt.update_value)
                || for_stmt.body.iter().any(stmt_uses_ans)
        }
        Stmt::Block(body) => body.iter().any(stmt_uses_ans),
        Stmt::Free { .. }
        | Stmt::Setup { .. }
        | Stmt::ClrMemory
        | Stmt::ClrStat
        | Stmt::FreqOn
        | Stmt::FreqOff
        | Stmt::Break
        | Stmt::Goto(..)
        | Stmt::Label(..)
        | Stmt::Empty => false,
    }
}

fn expr_uses_ans(expr: &Expr) -> bool {
    match expr {
        Expr::Ans(_) => true,
        Expr::Unary(_, inner) => expr_uses_ans(inner),
        Expr::Binary(_, left, right) => expr_uses_ans(left) || expr_uses_ans(right),
        // `ans()` is a zero-argument builtin, so the read is the call itself
        // rather than anything inside it.
        Expr::Call(name, args, _) => name == "ans" || args.iter().any(expr_uses_ans),
        Expr::Data { accessors, .. } => accessors.iter().any(|accessor| match accessor {
            Accessor::IndexExpr { expr, .. } => expr_uses_ans(expr),
            _ => false,
        }),
        Expr::Number(_)
        | Expr::BaseLiteral { .. }
        | Expr::Name(..)
        | Expr::Pi(_)
        | Expr::E(_)
        | Expr::Constant(..)
        | Expr::StatVar(..)
        | Expr::Input(_) => false,
    }
}

/// Which names are dead stores, and why.
#[derive(Default)]
struct DeadStores {
    /// Names that are the target of at least one `let`/`let v[…]`/`=`.
    stores: BTreeSet<String>,
    /// Whether *every* store of the name has a side-effect-free value.
    droppable: BTreeMap<String, bool>,
    /// Names used in any other way: read, indexed, a `for` counter, …
    referenced: BTreeSet<String>,
}

impl DeadStores {
    fn collect(program: &Program) -> Self {
        let mut info = DeadStores::default();
        info.stmts(program);
        info
    }

    /// The names whose every store may be deleted: never read anywhere, and
    /// every store's value side-effect free.
    ///
    /// Nothing here has to defend the diagnostics, because
    /// [`crate::alloc::Allocator::validate`] has already run on the program *as
    /// written*. That is the point of the ordering: `let t = 1; free t;
    /// free t;` and `let x = 1; let x = 2;` are reported before this pass sees
    /// them, so a never-read `t`/`x` may be deleted honestly rather than being
    /// kept as a hostage to an error message.
    fn dead(&self) -> BTreeSet<String> {
        self.stores
            .iter()
            .filter(|name| {
                !self.referenced.contains(*name) && self.droppable.get(*name) == Some(&true)
            })
            .cloned()
            .collect()
    }

    fn note_store(&mut self, name: &str, value_droppable: bool) {
        self.stores.insert(name.to_string());
        self.droppable
            .entry(name.to_string())
            .and_modify(|current| *current &= value_droppable)
            .or_insert(value_droppable);
    }

    fn note_reference(&mut self, name: &str) {
        self.referenced.insert(name.to_string());
    }

    fn stmts(&mut self, stmts: &[Stmt]) {
        for stmt in stmts {
            self.stmt(stmt);
        }
    }

    fn stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let { name, value, .. } => {
                self.note_store(name, is_side_effect_free(value));
                self.expr(value);
            }
            Stmt::LetArray { name, values, .. } => {
                self.note_store(name, values.iter().all(is_side_effect_free));
                for value in values {
                    self.expr(value);
                }
            }
            Stmt::Assign { name, value, .. } => {
                self.note_store(name, is_side_effect_free(value));
                self.expr(value);
            }
            // An element assignment keeps the array alive: its memory cannot
            // go away while something still writes into it.
            Stmt::AssignElement { name, value, .. } => {
                self.note_reference(name);
                self.expr(value);
            }
            Stmt::AssignElementExpr {
                name, index, value, ..
            } => {
                self.note_reference(name);
                self.expr(index);
                self.expr(value);
            }
            Stmt::Const { value, .. } => self.expr(value),
            // A `free` is not a *read*. It does not block removal either: the
            // transform deletes a dead name's `free` along with its stores, so
            // no `free` is ever left pointing at a deleted declaration.
            Stmt::Free { .. } => {}
            Stmt::Memory { value, .. } => self.expr(value),
            Stmt::Data { x, y, freq, .. } => {
                self.expr(x);
                if let Some(y) = y {
                    self.expr(y);
                }
                if let Some(freq) = freq {
                    self.expr(freq);
                }
            }
            Stmt::CondJump { cond, target, .. } => {
                self.expr(cond);
                // A `=>` target is emitted on one line, so a store inside it
                // cannot be removed without removing the whole jump. Treat
                // every name it mentions as referenced.
                self.reference_all(target);
            }
            Stmt::Print(expr) | Stmt::ExprStmt(expr) => self.expr(expr),
            Stmt::If {
                cond,
                then_body,
                else_body,
            } => {
                self.expr(cond);
                self.stmts(then_body);
                self.stmts(else_body);
            }
            Stmt::While { cond, body } => {
                self.expr(cond);
                self.stmts(body);
            }
            Stmt::For(for_stmt) => {
                self.expr(&for_stmt.init_value);
                self.expr(&for_stmt.cond);
                self.expr(&for_stmt.update_value);
                // The counter is written every iteration and must exist.
                self.note_reference(&for_stmt.init_name);
                self.note_reference(&for_stmt.update_name);
                self.stmts(&for_stmt.body);
            }
            Stmt::Block(stmts) => self.stmts(stmts),
            Stmt::Return { value, .. } => {
                if let Some(value) = value {
                    self.expr(value);
                }
            }
            Stmt::Setup { .. }
            | Stmt::ClrMemory
            | Stmt::ClrStat
            | Stmt::FreqOn
            | Stmt::FreqOff
            | Stmt::Break
            | Stmt::Goto(..)
            | Stmt::Label(..)
            | Stmt::Empty
            | Stmt::Function(_) => {}
        }
    }

    fn expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Name(name, _) => self.note_reference(name),
            Expr::Data {
                name, accessors, ..
            } => {
                self.note_reference(name);
                for accessor in accessors {
                    if let Accessor::IndexExpr { expr, .. } = accessor {
                        self.expr(expr);
                    }
                }
            }
            Expr::Unary(_, inner) => self.expr(inner),
            Expr::Binary(_, left, right) => {
                self.expr(left);
                self.expr(right);
            }
            // An argument is a read even if the call itself is pure to drop.
            Expr::Call(_, args, _) => {
                for arg in args {
                    self.expr(arg);
                }
            }
            Expr::Number(_)
            | Expr::BaseLiteral { .. }
            | Expr::Pi(_)
            | Expr::E(_)
            | Expr::Ans(_)
            | Expr::StatVar(..)
            | Expr::Constant(..)
            | Expr::Input(_) => {}
        }
    }

    /// Mark every name a statement mentions — read, written or freed — as
    /// referenced, so nothing inside it is removed.
    fn reference_all(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let { name, value, .. } => {
                self.note_reference(name);
                self.expr(value);
            }
            Stmt::LetArray { name, values, .. } => {
                self.note_reference(name);
                for value in values {
                    self.expr(value);
                }
            }
            Stmt::Assign { name, value, .. } => {
                self.note_reference(name);
                self.expr(value);
            }
            Stmt::AssignElement { name, value, .. } => {
                self.note_reference(name);
                self.expr(value);
            }
            Stmt::AssignElementExpr {
                name, index, value, ..
            } => {
                self.note_reference(name);
                self.expr(index);
                self.expr(value);
            }
            Stmt::Const { value, .. } => self.expr(value),
            Stmt::Free { name, .. } => self.note_reference(name),
            Stmt::Memory { value, .. } => self.expr(value),
            Stmt::Data { x, y, freq, .. } => {
                self.expr(x);
                if let Some(y) = y {
                    self.expr(y);
                }
                if let Some(freq) = freq {
                    self.expr(freq);
                }
            }
            Stmt::CondJump { cond, target, .. } => {
                self.expr(cond);
                self.reference_all(target);
            }
            Stmt::Print(expr) | Stmt::ExprStmt(expr) => self.expr(expr),
            Stmt::If {
                cond,
                then_body,
                else_body,
            } => {
                self.expr(cond);
                for stmt in then_body.iter().chain(else_body) {
                    self.reference_all(stmt);
                }
            }
            Stmt::While { cond, body } => {
                self.expr(cond);
                for stmt in body {
                    self.reference_all(stmt);
                }
            }
            Stmt::For(for_stmt) => {
                self.expr(&for_stmt.init_value);
                self.expr(&for_stmt.cond);
                self.expr(&for_stmt.update_value);
                self.note_reference(&for_stmt.init_name);
                self.note_reference(&for_stmt.update_name);
                for stmt in &for_stmt.body {
                    self.reference_all(stmt);
                }
            }
            Stmt::Block(stmts) => {
                for stmt in stmts {
                    self.reference_all(stmt);
                }
            }
            Stmt::Return { value, .. } => {
                if let Some(value) = value {
                    self.expr(value);
                }
            }
            Stmt::Setup { .. }
            | Stmt::ClrMemory
            | Stmt::ClrStat
            | Stmt::FreqOn
            | Stmt::FreqOff
            | Stmt::Break
            | Stmt::Goto(..)
            | Stmt::Label(..)
            | Stmt::Empty
            | Stmt::Function(_) => {}
        }
    }
}

/// Whether deleting a store of `expr` cannot change the program's output.
///
/// Reads of memory, constants and statistical variables are all pure. What is
/// not: `input()` prompts, `ran()` advances the random sequence, `ans` and
/// `mvalue` read machine state, and *any* call may raise `Math ERROR` — the
/// transpiler cannot see the arguments' values, so every call is treated as
/// capable of erroring.
fn is_side_effect_free(expr: &Expr) -> bool {
    match expr {
        Expr::Number(_)
        | Expr::BaseLiteral { .. }
        | Expr::Pi(_)
        | Expr::E(_)
        | Expr::Constant(..)
        | Expr::StatVar(..)
        | Expr::Name(..) => true,
        Expr::Data { accessors, .. } => accessors.iter().all(|accessor| match accessor {
            Accessor::IndexExpr { expr, .. } => is_side_effect_free(expr),
            _ => true,
        }),
        Expr::Unary(_, inner) => is_side_effect_free(inner),
        Expr::Binary(_, left, right) => is_side_effect_free(left) && is_side_effect_free(right),
        Expr::Ans(_) | Expr::Input(_) | Expr::Call(..) => false,
    }
}

/// Drop every store and `free` for a dead name, recursing into every body.
fn remove_dead_stores(stmts: Vec<Stmt>, dead: &BTreeSet<String>) -> Vec<Stmt> {
    let mut out = Vec::with_capacity(stmts.len());
    for stmt in stmts {
        if is_dead_store(&stmt, dead) {
            continue;
        }
        out.push(rebuild_stmt(stmt, dead));
    }
    out
}

/// Whether `stmt` is a store (or the `free`) of a dead name.
fn is_dead_store(stmt: &Stmt, dead: &BTreeSet<String>) -> bool {
    match stmt {
        Stmt::Let { name, .. }
        | Stmt::LetArray { name, .. }
        | Stmt::Assign { name, .. }
        | Stmt::Free { name, .. } => dead.contains(name),
        _ => false,
    }
}

/// Rebuild `stmt`, pruning dead stores out of its nested bodies.
fn rebuild_stmt(stmt: Stmt, dead: &BTreeSet<String>) -> Stmt {
    match stmt {
        Stmt::If {
            cond,
            then_body,
            else_body,
        } => Stmt::If {
            cond,
            then_body: remove_dead_stores(then_body, dead),
            else_body: remove_dead_stores(else_body, dead),
        },
        Stmt::While { cond, body } => Stmt::While {
            cond,
            body: remove_dead_stores(body, dead),
        },
        Stmt::For(mut for_stmt) => {
            for_stmt.body = remove_dead_stores(for_stmt.body, dead);
            Stmt::For(for_stmt)
        }
        Stmt::Block(stmts) => Stmt::Block(remove_dead_stores(stmts, dead)),
        Stmt::CondJump { cond, target, pos } => Stmt::CondJump {
            cond,
            // A `=>` target was marked referenced, so nothing in it is dead;
            // recurse only for totality.
            target: Box::new(rebuild_stmt(*target, dead)),
            pos,
        },
        other => other,
    }
}

// ---------------------------------------------------------------------------
// Pruning

/// Prune nested bodies, decide constant conditions and drop unreachable
/// statements. Returns whether anything was removed.
fn prune(program: &mut Program) -> bool {
    let before = count_stmts(program);
    let taken = std::mem::take(program);
    *program = prune_stmts(taken);
    count_stmts(program) != before
}

/// Prune nested bodies, decide constant conditions, then drop unreachable
/// statements, all at one statement-list level.
fn prune_stmts(stmts: Vec<Stmt>) -> Vec<Stmt> {
    let nested: Vec<Stmt> = stmts.into_iter().map(prune_nested).collect();
    let decided: Vec<Stmt> = nested.into_iter().flat_map(decide_condition).collect();
    strip_unreachable(decided)
}

/// Recurse into the bodies of a single statement, leaving its own condition
/// alone for [`decide_condition`] to fold.
fn prune_nested(stmt: Stmt) -> Stmt {
    match stmt {
        Stmt::If {
            cond,
            then_body,
            else_body,
        } => Stmt::If {
            cond,
            then_body: prune_stmts(then_body),
            else_body: prune_stmts(else_body),
        },
        Stmt::While { cond, body } => Stmt::While {
            cond,
            body: prune_stmts(body),
        },
        Stmt::For(mut for_stmt) => {
            for_stmt.body = prune_stmts(for_stmt.body);
            Stmt::For(for_stmt)
        }
        Stmt::Block(stmts) => Stmt::Block(prune_stmts(stmts)),
        Stmt::CondJump { cond, target, pos } => Stmt::CondJump {
            cond,
            target: Box::new(prune_nested(*target)),
            pos,
        },
        other => other,
    }
}

/// Fold a condition and, when it is a literal, discard the branch that can
/// never run. The branch is only discarded when doing so cannot move a later
/// variable to a different memory.
fn decide_condition(stmt: Stmt) -> Vec<Stmt> {
    match stmt {
        Stmt::If {
            mut cond,
            then_body,
            else_body,
        } => {
            if let Some(value) = constant_condition(&mut cond) {
                let dead = if value != 0.0 { &else_body } else { &then_body };
                if !affects_allocation(dead) {
                    return if value != 0.0 { then_body } else { else_body };
                }
            }
            vec![Stmt::If {
                cond,
                then_body,
                else_body,
            }]
        }
        Stmt::While { mut cond, body } => {
            if let Some(value) = constant_condition(&mut cond)
                && value == 0.0
                && !affects_allocation(&body)
            {
                return Vec::new();
            }
            vec![Stmt::While { cond, body }]
        }
        other => vec![other],
    }
}

/// The literal value of `cond`, if folding makes it one.
fn constant_condition(cond: &mut Expr) -> Option<f64> {
    fold::fold_expr(cond);
    match cond {
        Expr::Number(value) => Some(*value),
        _ => None,
    }
}

/// Drop statements that follow a `goto`/`return`, up to the next `label`.
///
/// A statement that [`affects_allocation`] is kept even while unreachable, so
/// the memory plan of the reachable code is unchanged.
fn strip_unreachable(stmts: Vec<Stmt>) -> Vec<Stmt> {
    let mut out = Vec::with_capacity(stmts.len());
    let mut unreachable = false;
    for stmt in stmts {
        if unreachable {
            if matches!(stmt, Stmt::Label(..)) {
                unreachable = false;
                out.push(stmt);
            } else if affects_allocation_one(&stmt) {
                out.push(stmt);
            }
        } else {
            if matches!(stmt, Stmt::Goto(..) | Stmt::Return { .. }) {
                unreachable = true;
            }
            out.push(stmt);
        }
    }
    out
}

/// Whether dropping the statements in `stmts` could change the memory plan,
/// and therefore the observable state of a later, reachable statement.
///
/// A declaration or `free` moves every later variable to a different memory; a
/// `label` may be the target of a `goto`; and `mplus`/`mminus`/`mvalue` reserve
/// the fixed `M` memory, so dropping one can hand `M` to a later variable.
fn affects_allocation(stmts: &[Stmt]) -> bool {
    stmts.iter().any(affects_allocation_one)
}

fn affects_allocation_one(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Let { .. }
        | Stmt::Const { .. }
        | Stmt::Free { .. }
        | Stmt::Label(..)
        | Stmt::Memory { .. }
        | Stmt::Function(_) => true,
        Stmt::If {
            cond,
            then_body,
            else_body,
        } => {
            expr_mentions_mvalue(cond)
                || affects_allocation(then_body)
                || affects_allocation(else_body)
        }
        Stmt::While { cond, body } => expr_mentions_mvalue(cond) || affects_allocation(body),
        Stmt::For(for_stmt) => {
            expr_mentions_mvalue(&for_stmt.init_value)
                || expr_mentions_mvalue(&for_stmt.cond)
                || expr_mentions_mvalue(&for_stmt.update_value)
                || affects_allocation(&for_stmt.body)
        }
        Stmt::CondJump { cond, target, .. } => {
            expr_mentions_mvalue(cond) || affects_allocation_one(target)
        }
        Stmt::Assign { value, .. } | Stmt::AssignElement { value, .. } => {
            expr_mentions_mvalue(value)
        }
        Stmt::AssignElementExpr { index, value, .. } => {
            expr_mentions_mvalue(index) || expr_mentions_mvalue(value)
        }
        Stmt::Data { x, y, freq, .. } => {
            expr_mentions_mvalue(x)
                || y.as_ref().is_some_and(expr_mentions_mvalue)
                || freq.as_ref().is_some_and(expr_mentions_mvalue)
        }
        Stmt::Print(expr) | Stmt::ExprStmt(expr) => expr_mentions_mvalue(expr),
        Stmt::LetArray { values, .. } => values.iter().any(expr_mentions_mvalue),
        Stmt::Block(stmts) => affects_allocation(stmts),
        Stmt::Setup { .. }
        | Stmt::ClrMemory
        | Stmt::ClrStat
        | Stmt::FreqOn
        | Stmt::FreqOff
        | Stmt::Break
        | Stmt::Goto(..)
        | Stmt::Return { .. }
        | Stmt::Empty => false,
    }
}

/// Whether `expr` reads the fixed `M` memory through `mvalue()`.
fn expr_mentions_mvalue(expr: &Expr) -> bool {
    match expr {
        Expr::Call(name, args, _) => name == "mvalue" || args.iter().any(expr_mentions_mvalue),
        Expr::Unary(_, inner) => expr_mentions_mvalue(inner),
        Expr::Binary(_, left, right) => expr_mentions_mvalue(left) || expr_mentions_mvalue(right),
        Expr::Data { accessors, .. } => accessors.iter().any(|accessor| match accessor {
            Accessor::IndexExpr { expr, .. } => expr_mentions_mvalue(expr),
            _ => false,
        }),
        _ => false,
    }
}

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
//! assigned anywhere afterwards**, holds the same value for its whole life. So
//! every *read* of the name can be replaced by the literal, and the surrounding
//! arithmetic is then folded by the second [`crate::fold`] run:
//!
//! ```c
//! let n = 3;
//! for (let i = 0; i <= n; i = i + 1) { print(i); }
//! // 3→A | For 0→B To 3 Step 1 | B◢ | Next
//!
//! let k = 2;
//! print(k * 3);
//! // 2→A | 6◢          (not `A×3◢`)
//! ```
//!
//! Only `let` bindings whose initialiser has already been folded to a number
//! qualify. A symbolic initialiser (`2 * pi`) is left alone: folding it to a
//! decimal loses precision and costs *more* bytes, not fewer
//! ([ADR 0015](../../../docs/DECISIONS.md)).
//!
//! ## A store to a name that is never read is removed
//!
//! **A program's contract is its output.** A store that no read can observe
//! cannot change a value the program displays, so it is deleted rather than
//! left to cost keys: `let unused = 1; print(2);` becomes just `2◢`. This is
//! the default — there is no flag to turn it on.
//!
//! A store is removed only when **all** of the following hold:
//!
//! * its name is never *read* anywhere — no `Expr::Name` reference, no array
//!   index, no `for` bound or update, no argument to a call;
//! * its value is side-effect free (see below); and
//! * its name is neither released with `free`/`unsafe_free` nor declared more
//!   than once.
//!
//! The last condition is about **diagnostics**, not size. This pass runs after
//! the program has been checked, so deleting the declaration an error is about
//! would silently legalise the program: `let t = 1; free t; free t;` is a
//! double-free error and `let x = 1; let x = 2;` a re-declaration error, yet in
//! both cases the name is never *read*, so a naive dead-store pass would remove
//! the declaration (then the `free`s) and report success. A `free` is a claim
//! about a lifetime and a second `let` a claim about a name; checking the claim
//! is the point, so both are left alone.
//!
//! Values that prompt or advance machine state are never removed: `input()`
//! (the prompt is observable), `ran()` (it advances the random sequence),
//! `ans`/`mvalue`, `mplus`/`mminus`, and any call that can raise `Math ERROR`
//! or has state. A literal, a `const`, a `#data` path, `pi`/`e`/`phys.*`,
//! `stat.*`, or pure arithmetic over those is removable.
//!
//! Because "never read" means what the programmer wrote, this runs *before*
//! constant propagation: a name used only as a loop bound survives even though
//! folding later replaces that bound with a literal.
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
//!   *element* of an array of the same name.
//! * Do not propagate through a `free`: after `free x; let x = …` the second
//!   life is a different binding. (This pass is stricter still: any name that
//!   is ever freed, or declared more than once, is left alone.)
//! * Do not touch `input()`, `ran()`, `ans`, `mvalue`, or anything mode- or
//!   state-dependent; they are not constants and their stores are never
//!   removed.
//! * Do not remove a store whose value is not side-effect free, and never
//!   leave a `free`/`unsafe_free` pointing at a deleted declaration.
//! * Do not reorder anything side-effecting (`print`, `input`, `dt`, `mplus`,
//!   the setup statements).
//! * Anything involving a symbolic constant (`pi`, `e`, `phys.`) is not a value
//!   to propagate as a decimal ([ADR 0015](../../../docs/DECISIONS.md)).

use std::collections::{BTreeMap, BTreeSet};

use crate::ast::{Accessor, Expr, Program, Stmt};
use crate::fold;

/// Propagate constants and prune unreachable code in `program`, in place.
///
/// Runs after [`crate::simplify`] and before the second [`crate::fold`] run.
pub fn propagate_program(program: &mut Program) {
    // 1. Delete stores whose name is never read, before propagation has a
    //    chance to erase the reads that protect them.
    eliminate_dead_stores(program);
    // 2. Replace reads of constant `let`s with their values.
    let constants = collect_constants(program);
    let mut state = BTreeSet::new();
    propagate_stmts(program, &constants, &mut state);
    // 3. Decide constant conditions and drop unreachable statements.
    let taken = std::mem::take(program);
    *program = prune_stmts(taken);
}

// ---------------------------------------------------------------------------
// Which names are constant

/// The facts a whole-program scan needs before any read can be rewritten.
#[derive(Default)]
struct Facts {
    /// The value of every `let` whose initialiser folded to a number.
    values: BTreeMap<String, f64>,
    /// How many times each name is declared, over the whole program; a second
    /// declaration is a second binding and disqualifies the name.
    declarations: BTreeMap<String, usize>,
    /// Names that are ever the target of an assignment (`x = …`, `x[i] = …`,
    /// or a `for` counter), so their value is not fixed.
    assigned: BTreeSet<String>,
    /// Names that are ever released with `free`, ending one binding's life.
    freed: BTreeSet<String>,
}

impl Facts {
    fn collect(program: &Program) -> Self {
        let mut facts = Facts::default();
        facts.stmts(program);
        facts
    }

    /// The names whose value can be propagated: declared exactly once with a
    /// numeric literal, and never assigned or freed.
    fn constants(&self) -> BTreeMap<String, f64> {
        self.values
            .iter()
            .filter(|(name, _)| {
                self.declarations.get(*name) == Some(&1)
                    && !self.assigned.contains(*name)
                    && !self.freed.contains(*name)
            })
            .map(|(name, value)| (name.clone(), *value))
            .collect()
    }

    fn stmts(&mut self, stmts: &[Stmt]) {
        for stmt in stmts {
            self.stmt(stmt);
        }
    }

    fn stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let { name, value, .. } => {
                *self.declarations.entry(name.clone()).or_insert(0) += 1;
                if let Expr::Number(value) = value {
                    self.values.insert(name.clone(), *value);
                }
            }
            Stmt::LetArray { name, .. } => {
                *self.declarations.entry(name.clone()).or_insert(0) += 1;
            }
            Stmt::Assign { name, .. }
            | Stmt::AssignElement { name, .. }
            | Stmt::AssignElementExpr { name, .. } => {
                self.assigned.insert(name.clone());
            }
            Stmt::Free { name, .. } => {
                self.freed.insert(name.clone());
            }
            Stmt::For(for_stmt) => {
                if for_stmt.is_decl {
                    *self
                        .declarations
                        .entry(for_stmt.init_name.clone())
                        .or_insert(0) += 1;
                }
                // The update clause writes the counter every iteration, and a
                // bare `for (i = …)` writes an existing variable too, so both
                // names are treated as assigned.
                self.assigned.insert(for_stmt.init_name.clone());
                self.assigned.insert(for_stmt.update_name.clone());
                self.stmts(&for_stmt.body);
            }
            Stmt::If {
                then_body,
                else_body,
                ..
            } => {
                self.stmts(then_body);
                self.stmts(else_body);
            }
            Stmt::While { body, .. } => self.stmts(body),
            Stmt::Block(stmts) => self.stmts(stmts),
            Stmt::CondJump { target, .. } => self.stmt(target),
            // `Print`, `Assign` values and the rest carry no declarations, and
            // their expressions are never read for a constant name.
            _ => {}
        }
    }
}

/// The constants available to [`propagate_stmts`].
fn collect_constants(program: &Program) -> BTreeMap<String, f64> {
    Facts::collect(program).constants()
}

// ---------------------------------------------------------------------------
// Rewriting reads

/// Rewrite every read of a constant name, tracking which declarations have
/// definitely executed at each point.
///
/// A forward walk is enough because a constant name is, by construction, never
/// assigned: once its `let` has run, its value stays put along a straight-line
/// path. Where control flow joins, only names that ran on *every* incoming path
/// survive:
///
/// * an `if`/`else` keeps a name only when both branches declared it;
/// * a loop keeps a name only when it was already declared before the loop, so
///   a name first declared in the body (which may never run) is dropped.
///
/// A `label` clears the state, because a `goto` can re-enter from a point where
/// nothing has been declared yet.
fn propagate_stmts(
    stmts: &mut [Stmt],
    constants: &BTreeMap<String, f64>,
    state: &mut BTreeSet<String>,
) {
    for stmt in stmts {
        propagate_stmt(stmt, constants, state);
    }
}

fn propagate_stmt(
    stmt: &mut Stmt,
    constants: &BTreeMap<String, f64>,
    state: &mut BTreeSet<String>,
) {
    match stmt {
        Stmt::Let { name, value, .. } => {
            // The initialiser runs before the name takes effect, so a read of
            // the name here is a read of the *old* binding — and a name cannot
            // be used in its own initialiser anyway.
            propagate_expr(value, constants, state);
            if constants.contains_key(name) {
                state.insert(name.clone());
            }
        }
        Stmt::LetArray { values, .. } => {
            for value in values {
                propagate_expr(value, constants, state);
            }
        }
        // A `const` is the programmer's promise that the value is fixed while
        // transpiling. Propagating a `let` into it would make the emitter
        // accept a `const` that names a runtime variable, weakening that
        // check for no byte saving — a `const` is inlined and uses no memory.
        Stmt::Const { .. } => {}
        Stmt::Assign { value, .. } => propagate_expr(value, constants, state),
        Stmt::AssignElement { value, .. } => propagate_expr(value, constants, state),
        Stmt::AssignElementExpr { index, value, .. } => {
            propagate_expr(index, constants, state);
            propagate_expr(value, constants, state);
        }
        // A freed name is never a constant here (see `Facts::constants`), so
        // the release cannot change the tracked state.
        Stmt::Free { .. } => {}
        Stmt::Memory { value, .. } => propagate_expr(value, constants, state),
        Stmt::Data { x, y, freq, .. } => {
            propagate_expr(x, constants, state);
            if let Some(y) = y {
                propagate_expr(y, constants, state);
            }
            if let Some(freq) = freq {
                propagate_expr(freq, constants, state);
            }
        }
        // A `⇒` target runs only when the condition holds, so it is analysed
        // in a copy of the state and does not affect what follows.
        Stmt::CondJump { cond, target, .. } => {
            propagate_expr(cond, constants, state);
            let mut inner = state.clone();
            propagate_stmt(target, constants, &mut inner);
        }
        Stmt::Print(expr) | Stmt::ExprStmt(expr) => propagate_expr(expr, constants, state),
        Stmt::If {
            cond,
            then_body,
            else_body,
        } => {
            propagate_expr(cond, constants, state);
            let mut then_state = state.clone();
            propagate_stmts(then_body, constants, &mut then_state);
            let mut else_state = state.clone();
            propagate_stmts(else_body, constants, &mut else_state);
            *state = then_state.intersection(&else_state).cloned().collect();
        }
        Stmt::While { cond, body } => {
            propagate_expr(cond, constants, state);
            // The body may run zero times, so only names already constant
            // before the loop survive it.
            let before = state.clone();
            let mut body_state = before.clone();
            propagate_stmts(body, constants, &mut body_state);
            *state = before.intersection(&body_state).cloned().collect();
        }
        Stmt::For(for_stmt) => {
            propagate_expr(&mut for_stmt.init_value, constants, state);
            propagate_expr(&mut for_stmt.cond, constants, state);
            let before = state.clone();
            let mut body_state = before.clone();
            propagate_stmts(&mut for_stmt.body, constants, &mut body_state);
            // The update runs after the body, on every iteration that ran.
            propagate_expr(&mut for_stmt.update_value, constants, &body_state);
            *state = before.intersection(&body_state).cloned().collect();
        }
        Stmt::Block(stmts) => propagate_stmts(stmts, constants, state),
        Stmt::Return { value, .. } => {
            if let Some(value) = value {
                propagate_expr(value, constants, state);
            }
        }
        // A jump carries no value, but the code it skips may read a name that
        // was only *conditionally* declared, so the state is not trusted across
        // a label.
        Stmt::Label(..) => state.clear(),
        Stmt::Goto(..)
        | Stmt::Setup { .. }
        | Stmt::ClrMemory
        | Stmt::ClrStat
        | Stmt::FreqOn
        | Stmt::FreqOff
        | Stmt::Break
        | Stmt::Empty
        | Stmt::Function(_) => {}
    }
}

/// Replace a read of a constant name with its value, recursing into every
/// subexpression.
fn propagate_expr(expr: &mut Expr, constants: &BTreeMap<String, f64>, state: &BTreeSet<String>) {
    match expr {
        Expr::Name(name, _) => {
            if state.contains(name)
                && let Some(value) = constants.get(name)
            {
                *expr = Expr::Number(*value);
            }
        }
        Expr::Unary(_, inner) => propagate_expr(inner, constants, state),
        Expr::Binary(_, left, right) => {
            propagate_expr(left, constants, state);
            propagate_expr(right, constants, state);
        }
        Expr::Call(_, args, _) => {
            for arg in args {
                propagate_expr(arg, constants, state);
            }
        }
        Expr::Data { accessors, .. } => {
            for accessor in accessors {
                if let Accessor::IndexExpr { expr, .. } = accessor {
                    propagate_expr(expr, constants, state);
                }
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

// ---------------------------------------------------------------------------
// Dead stores

/// Delete declarations and stores whose name is never read.
///
/// Removal is iterated to a fixpoint: dropping `let b = a;` can leave `a`
/// unread, so `let a = …;` becomes removable on the next round. A pass that
/// removes nothing ends the loop.
fn eliminate_dead_stores(program: &mut Program) {
    // Two things make *any* store observable, however little the program reads:
    //
    // * `ans` reads the hidden result memory, which evaluating any expression
    //   updates — so dropping a store would change what `ans()` returns;
    // * a program that ends without `◢` displays the last value it computed, so
    //   a store in the final position is visible even though nothing reads it.
    //
    // Both are cheap to detect and turn the whole pass off, which is the only
    // sound answer: the pass reasons about *reads*, and neither of these is a
    // read in the AST.
    if uses_ans(program) || trails_with_a_store(program) {
        return;
    }
    loop {
        let dead = DeadStores::collect(program).dead();
        if dead.is_empty() {
            return;
        }
        let taken = std::mem::take(program);
        *program = remove_dead_stores(taken, &dead);
    }
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

/// Whether some execution path leaves a store or bare expression as the last
/// thing the machine computed.
///
/// PRGM displays that value when the program ends without `◢`, so it is
/// observable even when no name is ever read — `let a = 1;` on its own displays
/// `1`. Blocks and loop bodies count because control can fall out of them into
/// the end of the program.
fn trails_with_a_store(stmts: &[Stmt]) -> bool {
    let Some(last) = stmts.last() else {
        return false;
    };
    match last {
        Stmt::Let { .. }
        | Stmt::LetArray { .. }
        | Stmt::Assign { .. }
        | Stmt::AssignElement { .. }
        | Stmt::AssignElementExpr { .. }
        | Stmt::CondJump { .. }
        | Stmt::Memory { .. }
        | Stmt::ExprStmt(_) => true,
        Stmt::If {
            then_body,
            else_body,
            ..
        } => trails_with_a_store(then_body) || trails_with_a_store(else_body),
        Stmt::While { body, .. } => trails_with_a_store(body),
        Stmt::For(for_stmt) => trails_with_a_store(&for_stmt.body),
        Stmt::Block(body) => trails_with_a_store(body),
        _ => false,
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
    /// Names released with `free`/`unsafe_free`. A `free` is a *claim* about a
    /// lifetime, so removing the declaration it refers to would silently
    /// legalise the program — `let t = 1; free t; free t;` must still be a
    /// double-free error rather than vanishing.
    released: BTreeSet<String>,
    /// How many times each name is *declared* (`let`/`let v[…]`). A name
    /// declared twice is a re-declaration error, which removing one of the two
    /// declarations would hide.
    declares: BTreeMap<String, usize>,
}

impl DeadStores {
    fn collect(program: &Program) -> Self {
        let mut info = DeadStores::default();
        info.stmts(program);
        info
    }

    /// The names whose every store may be deleted.
    ///
    /// The restrictions are all about **diagnostics**: this pass runs after the
    /// program has been checked, so it must never delete the very thing an
    /// error is about. A declaration is only dead when the name is never read,
    /// every store of it is side-effect free, **and** it is neither released
    /// nor declared more than once.
    fn dead(&self) -> BTreeSet<String> {
        self.stores
            .iter()
            .filter(|name| {
                !self.referenced.contains(*name)
                    && self.droppable.get(*name) == Some(&true)
                    && !self.released.contains(*name)
                    && self.declares.get(*name).copied().unwrap_or(0) <= 1
            })
            .cloned()
            .collect()
    }

    fn note_declaration(&mut self, name: &str) {
        *self.declares.entry(name.to_string()).or_insert(0) += 1;
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
                self.note_declaration(name);
                self.note_store(name, is_side_effect_free(value));
                self.expr(value);
            }
            Stmt::LetArray { name, values, .. } => {
                self.note_declaration(name);
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
            // A `free` is not a *read*, but it does block removal: see
            // `DeadStores::released`.
            Stmt::Free { name, .. } => {
                self.released.insert(name.clone());
            }
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

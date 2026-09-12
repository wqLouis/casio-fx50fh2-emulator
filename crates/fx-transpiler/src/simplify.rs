//! Algebraic simplification of expressions.
//!
//! The machine has **680 bytes of program storage shared by all four program
//! areas**, and every operator left in the emitted PRGM costs bytes. After
//! [`crate::fold`] has pre-calculated the arithmetic that is built only from
//! numbers, this pass removes the operators that are provably redundant:
//!
//! ```
//! # use fx_transpiler::transpile;
//! // `A×1◢` costs two bytes more than `A◢`.
//! assert_eq!(transpile("fn main() { let a = input(); print(a * 1); }").unwrap(), "?→A\nA◢\n");
//! assert_eq!(transpile("fn main() { let a = input(); print(-(-a)); }").unwrap(), "?→A\nA◢\n");
//! ```
//!
//! ## Identities applied
//!
//! * An **identity element** is dropped: `x + 0`, `0 + x`, `x - 0`, `x * 1`,
//!   `1 * x`, `x / 1`, `x ^ 1` all become `x`. None of these operations can
//!   raise an error, and `x` is evaluated exactly once either way, so this is
//!   safe for *any* `x` — even a random-number call.
//! * **Double negation** is removed: `-(-x)` becomes `x`. Negation cannot
//!   error, and the two `-` signs wrap a single occurrence of `x`.
//! * An **absorbing element** wins: `x * 0`, `0 * x` become `0` and `x ^ 0`
//!   becomes `1`. These drop `x` entirely, so they are applied only when `x` is
//!   *inert* (see below): `0 * sqrt(-1)` must keep raising `Math ERROR`.
//! * A **self-operation** collapses: `x - x` is `0`, `x == x` is `1`, `x != x`
//!   is `0`. The machine compares or subtracts the same value with itself, so
//!   the result is fixed — but only an inert `x` may be duplicated.
//! * `x / x` is `1` **only when `x` is a non-zero literal**. A name may hold
//!   zero, where the machine reports `Math ERROR`, so a variable `x / x` is
//!   left alone.
//!
//! ## What "inert" means
//!
//! A leaf that is a value — a number, a base-tagged literal, a variable name, a
//! `stat.` variable, `Ans`, `pi`, `e`, a `phys.` constant or a `#data`/array
//! reference. Reading one cannot raise `Math ERROR` and cannot consume a random
//! number or a keystroke, so removing or duplicating it cannot change what a
//! run does. Everything else is treated as *effectful* and is never dropped or
//! duplicated: a call such as `sqrt(-1)` can error, and `ran()` advances the
//! random sequence.
//!
//! ## What must NOT be simplified
//!
//! * Anything whose evaluation could raise an error the machine would report.
//!   `0 * x` is only `0` when `x` cannot be `Math ERROR`, and `x / x` is only
//!   `1` when `x` cannot be zero. When in doubt the expression is left alone: a
//!   shorter program that reports the wrong error is worse than a longer one.
//! * Anything involving a **symbolic constant** (`pi`, `e`, `phys.`): folding
//!   `2 * pi` to a decimal loses precision and costs *more* bytes, not fewer
//!   ([ADR 0015](../../../docs/DECISIONS.md)). This pass never turns a symbol
//!   into a decimal; it only ever drops an expression that multiplies one by
//!   zero outright.
//! * Arithmetic on two literals, which is [`crate::fold`]'s job. This pass runs
//!   after `fold` and leaves literal-literal nodes to it.
//! * Anything the machine's 15-digit auto-correcting arithmetic would round.
//!   The only literals this pass produces by itself are `0` and `1`, which are
//!   exact; the predicate for more general values lives in [`crate::fold`].

use crate::ast::{Accessor, BinOp, Expr, ForStmt, Program, Stmt, UnOp};

/// Simplify every expression in `program`, in place.
///
/// The rewrite is bottom-up, so a simplification can expose another one at the
/// parent: `(a * 1) - a` first becomes `a - a` and then `0`.
pub(crate) fn simplify_program(program: &mut Program) {
    simplify_stmts(program);
}

/// Simplify the expressions of every statement in `stmts`, in order.
fn simplify_stmts(stmts: &mut [Stmt]) {
    for stmt in stmts {
        simplify_stmt(stmt);
    }
}

/// Simplify the expressions a statement owns, recursing into nested bodies.
fn simplify_stmt(stmt: &mut Stmt) {
    match stmt {
        Stmt::Let { value, .. } | Stmt::Assign { value, .. } => simplify_expr(value),
        Stmt::LetArray { values, .. } => {
            for value in values {
                simplify_expr(value);
            }
        }
        Stmt::Const { value, .. } => simplify_expr(value),
        Stmt::AssignElement { value, .. } => simplify_expr(value),
        Stmt::AssignElementExpr { index, value, .. } => {
            simplify_expr(index);
            simplify_expr(value);
        }
        Stmt::Memory { value, .. } => simplify_expr(value),
        Stmt::Data { x, y, freq, .. } => {
            simplify_expr(x);
            if let Some(y) = y {
                simplify_expr(y);
            }
            if let Some(freq) = freq {
                simplify_expr(freq);
            }
        }
        Stmt::CondJump { cond, target, .. } => {
            simplify_expr(cond);
            simplify_stmt(target);
        }
        Stmt::Print(expr) | Stmt::ExprStmt(expr) => simplify_expr(expr),
        Stmt::If {
            cond,
            then_body,
            else_body,
        } => {
            simplify_expr(cond);
            simplify_stmts(then_body);
            simplify_stmts(else_body);
        }
        Stmt::While { cond, body } => {
            simplify_expr(cond);
            simplify_stmts(body);
        }
        Stmt::For(for_stmt) => simplify_for(for_stmt),
        Stmt::Block(stmts) => simplify_stmts(stmts),
        // Gone before simplification; walked anyway so the pass is total.
        Stmt::Return { value, .. } => {
            if let Some(value) = value {
                simplify_expr(value);
            }
        }
        Stmt::Function(_) => {}
        Stmt::Free { .. }
        | Stmt::Setup { .. }
        | Stmt::ClrMemory
        | Stmt::ClrStat
        | Stmt::FreqOn
        | Stmt::FreqOff
        | Stmt::Break
        | Stmt::Goto(..)
        | Stmt::Label(..)
        | Stmt::Empty => {}
    }
}

/// Simplify every expression of a `for` header and body.
fn simplify_for(for_stmt: &mut ForStmt) {
    simplify_expr(&mut for_stmt.init_value);
    simplify_expr(&mut for_stmt.cond);
    simplify_expr(&mut for_stmt.update_value);
    simplify_stmts(&mut for_stmt.body);
}

/// Simplify `expr` and everything below it.
///
/// Children are simplified first, so a parent sees the rewritten form of each
/// operand.
fn simplify_expr(expr: &mut Expr) {
    match expr {
        Expr::Unary(_, inner) => simplify_expr(inner),
        Expr::Binary(_, left, right) => {
            simplify_expr(left);
            simplify_expr(right);
        }
        Expr::Call(_, args, _) => {
            for arg in args {
                simplify_expr(arg);
            }
        }
        // A computed index is an expression like any other; simplify it even
        // though the unroller will later make it literal.
        Expr::Data { accessors, .. } => {
            for accessor in accessors {
                if let Accessor::IndexExpr { expr, .. } = accessor {
                    simplify_expr(expr);
                }
            }
        }
        Expr::Number(_)
        | Expr::BaseLiteral { .. }
        | Expr::Name(..)
        | Expr::Pi(_)
        | Expr::E(_)
        | Expr::Ans(_)
        | Expr::StatVar(..)
        | Expr::Constant(..)
        | Expr::Input(_) => {}
    }

    if let Some(replacement) = simplify_node(expr) {
        *expr = replacement;
    }
}

/// The simplification of one expression node, if any.
///
/// Returns a replacement that is always smaller (it drops at least one
/// operator) or a literal; the caller has already simplified its children.
fn simplify_node(expr: &Expr) -> Option<Expr> {
    match expr {
        Expr::Unary(UnOp::Neg, inner) => {
            // `-(-x)` is `x`: negation cannot error, and `x` occurs once.
            if let Expr::Unary(UnOp::Neg, innermost) = inner.as_ref() {
                return Some((**innermost).clone());
            }
        }
        Expr::Binary(op, left, right) => return simplify_binary(*op, left, right),
        _ => {}
    }
    None
}

/// The simplification of `left op right`, if any.
fn simplify_binary(op: BinOp, left: &Expr, right: &Expr) -> Option<Expr> {
    match op {
        // Adding or subtracting zero cannot overflow, so the identity element
        // can be dropped for any operand.
        BinOp::Add => {
            if is_zero(right) {
                return Some(left.clone()); // x + 0
            }
            if is_zero(left) {
                return Some(right.clone()); // 0 + x
            }
        }
        BinOp::Sub => {
            if is_zero(right) {
                return Some(left.clone()); // x - 0
            }
            // `x - x` is always zero, but only an inert `x` may be duplicated
            // and dropped: `ran() - ran()` is not zero, and `(1/0) - (1/0)`
            // errors where `0` would not.
            if is_inert(left) && same(left, right) {
                return Some(Expr::Number(0.0)); // x - x
            }
        }
        BinOp::Mul => {
            if is_one(right) {
                return Some(left.clone()); // x * 1
            }
            if is_one(left) {
                return Some(right.clone()); // 1 * x
            }
            // Multiplication by zero discards the operand, so the operand must
            // be unable to error or have a side effect.
            if is_zero(right) && is_inert(left) {
                return Some(Expr::Number(0.0)); // x * 0
            }
            if is_zero(left) && is_inert(right) {
                return Some(Expr::Number(0.0)); // 0 * x
            }
        }
        BinOp::Div => {
            if is_one(right) {
                return Some(left.clone()); // x / 1
            }
            // `x / x` is `1` only when `x` is a non-zero literal. A variable
            // may be zero, where the machine reports `Math ERROR`; a symbolic
            // constant must keep its symbol. `fold` normally resolves
            // literal-literal division already, so this catches base-tagged
            // literals (`2h / 2h`).
            if is_inert(left) && same(left, right) && is_nonzero_literal(left) {
                return Some(Expr::Number(1.0)); // x / x, x a non-zero literal
            }
        }
        BinOp::Pow => {
            if is_one(right) {
                return Some(left.clone()); // x ^ 1
            }
            // `x ^ 0` is `1` for every non-erring `x` (including `0`), so the
            // base may be dropped once it is known to be inert.
            if is_zero(right) && is_inert(left) {
                return Some(Expr::Number(1.0)); // x ^ 0
            }
        }
        // The machine resolves `x == x` and `x != x` on the same value, so the
        // comparison is fixed — again only when `x` may be duplicated.
        BinOp::Eq | BinOp::Ne => {
            if is_inert(left) && same(left, right) {
                let value = if op == BinOp::Eq { 1.0 } else { 0.0 };
                return Some(Expr::Number(value)); // x == x, x != x
            }
        }
        // `<`, `<=`, `>`, `>=` are not written down as identities: the spec for
        // this pass lists only the equality pair, and a self-comparison of an
        // unordered value is not something to guess about.
        BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {}
        // Base-n bit operations depend on the word size, which lives in the
        // runtime; `x and x` is not obviously `x` on every machine, so it is
        // left for the emitter.
        BinOp::And | BinOp::Or | BinOp::Xor | BinOp::Xnor => {}
    }
    None
}

/// Whether `expr` is the literal `0`.
fn is_zero(expr: &Expr) -> bool {
    match expr {
        Expr::Number(value) => *value == 0.0,
        Expr::BaseLiteral { value, .. } => *value == 0,
        _ => false,
    }
}

/// Whether `expr` is the literal `1`.
fn is_one(expr: &Expr) -> bool {
    match expr {
        Expr::Number(value) => *value == 1.0,
        Expr::BaseLiteral { value, .. } => *value == 1,
        _ => false,
    }
}

/// Whether `expr` is a literal whose value is known not to be zero.
fn is_nonzero_literal(expr: &Expr) -> bool {
    match expr {
        Expr::Number(value) => *value != 0.0,
        Expr::BaseLiteral { value, .. } => *value != 0,
        _ => false,
    }
}

/// Whether `expr` is a value that cannot raise an error and has no side effect.
///
/// These are the leaves the pass may drop or duplicate. Everything else — every
/// call, every operator — is treated as effectful, even when a particular
/// operand happens to be harmless, because proving that in general would need
/// the full error model of the machine.
fn is_inert(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::Number(_)
            | Expr::BaseLiteral { .. }
            | Expr::Name(..)
            | Expr::Pi(_)
            | Expr::E(_)
            | Expr::Ans(_)
            | Expr::StatVar(..)
            | Expr::Constant(..)
            | Expr::Data { .. }
    )
}

/// Structural equality that ignores the byte offsets carried for diagnostics.
///
/// [`Expr`]'s derived `PartialEq` compares `Name("a", 4)` and `Name("a", 20)`
/// as unequal because the positions differ, but `a - a` is zero regardless of
/// where the two `a`s were written.
fn same(left: &Expr, right: &Expr) -> bool {
    match (left, right) {
        (Expr::Number(a), Expr::Number(b)) => a == b,
        (
            Expr::BaseLiteral {
                value: av,
                base: ab,
                ..
            },
            Expr::BaseLiteral {
                value: bv,
                base: bb,
                ..
            },
        ) => av == bv && ab == bb,
        (Expr::Name(a, _), Expr::Name(b, _)) => a == b,
        (Expr::Pi(_), Expr::Pi(_))
        | (Expr::E(_), Expr::E(_))
        | (Expr::Ans(_), Expr::Ans(_))
        | (Expr::Input(_), Expr::Input(_)) => true,
        (Expr::StatVar(a, _), Expr::StatVar(b, _)) => a == b,
        (Expr::Constant(a, _), Expr::Constant(b, _)) => a.code == b.code,
        (
            Expr::Data {
                name: an,
                accessors: aa,
                ..
            },
            Expr::Data {
                name: bn,
                accessors: ba,
                ..
            },
        ) => an == bn && same_accessors(aa, ba),
        (Expr::Unary(ao, ai), Expr::Unary(bo, bi)) => ao == bo && same(ai, bi),
        (Expr::Binary(ao, al, ar), Expr::Binary(bo, bl, br)) => {
            ao == bo && same(al, bl) && same(ar, br)
        }
        (Expr::Call(an, aa, _), Expr::Call(bn, ba, _)) => {
            an == bn && aa.len() == ba.len() && aa.iter().zip(ba).all(|(x, y)| same(x, y))
        }
        _ => false,
    }
}

/// Position-insensitive equality for two accessor paths.
fn same_accessors(left: &[Accessor], right: &[Accessor]) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(a, b)| match (a, b) {
            (Accessor::Field { name: an, .. }, Accessor::Field { name: bn, .. }) => an == bn,
            (Accessor::Index { index: ai, .. }, Accessor::Index { index: bi, .. }) => ai == bi,
            (Accessor::IndexExpr { expr: ae, .. }, Accessor::IndexExpr { expr: be, .. }) => {
                same(ae, be)
            }
            _ => false,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;
    use crate::parser::parse;

    /// Parse `source` and simplify it, returning the debug form of the program.
    fn simplified(source: &str) -> String {
        let tokens = lex(source).unwrap();
        let mut program = parse(&tokens, source).unwrap();
        simplify_program(&mut program);
        format!("{program:?}")
    }

    #[test]
    fn multiplicative_identity_folds_away() {
        assert!(simplified("let a = 1; print(a * 1);").contains("Print(Name(\"a\""));
        assert!(simplified("let a = 1; print(1 * a);").contains("Print(Name(\"a\""));
        assert!(simplified("let a = 1; print(a / 1);").contains("Print(Name(\"a\""));
        assert!(simplified("let a = 1; print(a ^ 1);").contains("Print(Name(\"a\""));
    }

    #[test]
    fn additive_identity_folds_away() {
        assert!(simplified("let a = 1; print(a + 0);").contains("Print(Name(\"a\""));
        assert!(simplified("let a = 1; print(0 + a);").contains("Print(Name(\"a\""));
        assert!(simplified("let a = 1; print(a - 0);").contains("Print(Name(\"a\""));
    }

    #[test]
    fn double_negation_is_removed() {
        assert!(simplified("let a = 1; print(-(-a));").contains("Print(Name(\"a\""));
    }

    #[test]
    fn absorbing_zero_replaces_an_inert_operand() {
        assert!(simplified("let a = 1; print(a * 0);").contains("Print(Number(0.0))"));
        assert!(simplified("let a = 1; print(0 * a);").contains("Print(Number(0.0))"));
        assert!(simplified("let a = 1; print(a ^ 0);").contains("Print(Number(1.0))"));
    }

    #[test]
    fn a_self_comparison_is_decided() {
        assert!(simplified("let a = 1; print(a == a);").contains("Print(Number(1.0))"));
        assert!(simplified("let a = 1; print(a != a);").contains("Print(Number(0.0))"));
    }

    #[test]
    fn a_self_subtraction_is_zero() {
        assert!(simplified("let a = 1; print(a - a);").contains("Print(Number(0.0))"));
    }

    #[test]
    fn a_call_that_can_error_is_never_dropped() {
        assert!(simplified("print(0 * sqrt(2));").contains("Binary(Mul"));
        assert!(simplified("print(sqrt(2) ^ 0);").contains("Binary(Pow"));
        assert!(simplified("print(sqrt(2) == sqrt(2));").contains("Binary(Eq"));
    }

    #[test]
    fn a_self_division_is_left_alone_for_a_name() {
        // A variable may be zero, where the machine reports `Math ERROR`.
        assert!(simplified("let a = 1; print(a / a);").contains("Binary(Div"));
    }

    #[test]
    fn a_symbolic_constant_keeps_its_symbol() {
        assert!(simplified("print(2 * pi);").contains("Pi"));
        assert!(simplified("print(2 * pi);").contains("Binary(Mul"));
    }
}

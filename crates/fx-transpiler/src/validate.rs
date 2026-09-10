//! Mode-capability checks for a parsed `.fxc` program.
//!
//! The `.fxc` language is real-number only: it has no syntax for complex
//! numbers, statistics values or base-n literals. The only meaningful check is
//! therefore **BASE**, which the calculator restricts to integer arithmetic —
//! so *every* floating-point built-in and the constants `π`/`e` are rejected
//! there.
//!
//! This lives in its own function so additional per-mode rules can be added
//! without touching the emitter.

use crate::ast::{Expr, Program, Stmt};
use crate::builtins;
use crate::error::TranspileError;
use crate::mode::Mode;

/// Reject constructs that the effective `mode` does not offer.
///
/// Returns the first violation in source order, pointing at the offending
/// built-in call or constant.
pub fn validate(program: &Program, mode: Mode, source: &str) -> Result<(), TranspileError> {
    if mode.allows_float_math() {
        return Ok(());
    }
    validate_stmts(program, source)
}

/// Is `name` a floating-point built-in that BASE mode does not offer?
///
/// Derived from [`builtins::BUILTINS`] rather than a hand-kept list, so the
/// transpiler rejects exactly the set the interpreter's own mode checker
/// rejects. A hardcoded subset once let `abs` (and others) pass `build` and
/// then fail `run`.
fn is_float_builtin(name: &str) -> bool {
    builtins::lookup(name).is_some()
}

fn validate_stmts(stmts: &[Stmt], source: &str) -> Result<(), TranspileError> {
    for stmt in stmts {
        validate_stmt(stmt, source)?;
    }
    Ok(())
}

fn validate_stmt(stmt: &Stmt, source: &str) -> Result<(), TranspileError> {
    match stmt {
        Stmt::Let { value, .. } | Stmt::Assign { value, .. } | Stmt::ExprStmt(value) => {
            validate_expr(value, source)
        }
        Stmt::Print(expr) => validate_expr(expr, source),
        Stmt::If {
            cond,
            then_body,
            else_body,
        } => {
            validate_expr(cond, source)?;
            validate_stmts(then_body, source)?;
            validate_stmts(else_body, source)
        }
        Stmt::While { cond, body } => {
            validate_expr(cond, source)?;
            validate_stmts(body, source)
        }
        Stmt::For(for_stmt) => {
            validate_expr(&for_stmt.init_value, source)?;
            validate_expr(&for_stmt.cond, source)?;
            validate_expr(&for_stmt.update_value, source)?;
            validate_stmts(&for_stmt.body, source)
        }
        Stmt::Block(stmts) => validate_stmts(stmts, source),
        Stmt::Break | Stmt::Goto(..) | Stmt::Label(..) | Stmt::Empty => Ok(()),
    }
}

fn validate_expr(expr: &Expr, source: &str) -> Result<(), TranspileError> {
    match expr {
        Expr::Pi(pos) => Err(forbidden(source, "`pi`", *pos)),
        Expr::E(pos) => Err(forbidden(source, "`e`", *pos)),
        // The call's name precedes its arguments in source order, so a
        // forbidden built-in is reported before anything nested inside it.
        Expr::Call(name, _, pos) if is_float_builtin(name) => {
            Err(forbidden(source, &format!("`{name}`"), *pos))
        }
        Expr::Unary(_, inner) => validate_expr(inner, source),
        Expr::Binary(_, left, right) => {
            validate_expr(left, source)?;
            validate_expr(right, source)
        }
        // Unknown names are a parser concern; their arguments still need
        // checking.
        Expr::Call(_, args, _) => {
            for arg in args {
                validate_expr(arg, source)?;
            }
            Ok(())
        }
        Expr::Number(_) | Expr::Name(..) | Expr::Input(_) => Ok(()),
    }
}

fn forbidden(source: &str, what: &str, pos: usize) -> TranspileError {
    TranspileError::at(
        source,
        format!("{what} is not available in BASE mode (switch to COMP, CMPLX, SD or REG)"),
        pos,
    )
}

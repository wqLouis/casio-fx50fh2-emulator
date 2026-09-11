//! Mode-capability checks for a parsed `.fxc` program.
//!
//! The fx-50FH II only offers certain constructs in certain modes, and the
//! transpiler mirrors the interpreter's own checker (`src/check.rs`): complex
//! numbers only exist in CMPLX, statistics only in SD/REG, and base-n only in
//! BASE. This pass rejects anything the declared mode does not offer, so
//! `fx50 build` fails at transpile time rather than leaving the program to
//! `Mode ERROR` on the calculator.
//!
//! It lives in its own function so per-mode rules can be added without touching
//! the emitter.

use crate::ast::{Expr, Program, Setup, StatVar, Stmt};
use crate::builtins;
use crate::error::TranspileError;
use crate::mode::Mode;

/// Reject constructs that the effective `mode` does not offer.
///
/// Returns the first violation in source order, pointing at the offending
/// construct.
pub fn validate(program: &Program, mode: Mode, source: &str) -> Result<(), TranspileError> {
    Checker { mode, source }.stmts(program)
}

struct Checker<'a> {
    mode: Mode,
    source: &'a str,
}

impl Checker<'_> {
    fn stmts(&self, stmts: &[Stmt]) -> Result<(), TranspileError> {
        for stmt in stmts {
            self.stmt(stmt)?;
        }
        Ok(())
    }

    fn stmt(&self, stmt: &Stmt) -> Result<(), TranspileError> {
        match stmt {
            Stmt::Let { value, .. }
            | Stmt::Const { value, .. }
            | Stmt::Assign { value, .. }
            | Stmt::AssignElement { value, .. } => self.expr(value),
            Stmt::AssignElementExpr { index, value, .. } => {
                self.expr(index)?;
                self.expr(value)
            }
            // Every element's initialiser is still an expression that has to
            // obey the mode; an array of numbers is fine in BASE.
            Stmt::LetArray { values, .. } => {
                for value in values {
                    self.expr(value)?;
                }
                Ok(())
            }
            // `free` names a variable, not a value; the allocator checks the
            // name.
            Stmt::Free { .. } => Ok(()),
            Stmt::Print(expr) | Stmt::ExprStmt(expr) => self.expr(expr),
            Stmt::Memory { value, .. } => self.expr(value),
            Stmt::Setup { setup, pos } => self.setup(*setup, *pos),
            Stmt::ClrMemory => Ok(()),
            Stmt::ClrStat => {
                self.require(self.mode.allows_stats(), "SD or REG", "`clrstat()`", None)
            }
            Stmt::FreqOn | Stmt::FreqOff => self.require(
                self.mode.allows_stats(),
                "SD or REG",
                "`freqon()`/`freqoff()`",
                None,
            ),
            Stmt::Data { x, y, freq, pos } => {
                self.require(self.mode.allows_stats(), "SD or REG", "`dt(…)`", Some(*pos))?;
                if y.is_some() {
                    self.require(
                        self.mode.allows_regression(),
                        "REG",
                        "paired data entry `dt(x, y)`",
                        Some(*pos),
                    )?;
                }
                self.expr(x)?;
                if let Some(y) = y {
                    self.expr(y)?;
                }
                if let Some(freq) = freq {
                    self.expr(freq)?;
                }
                Ok(())
            }
            Stmt::CondJump { cond, target, .. } => {
                self.expr(cond)?;
                self.stmt(target)
            }
            Stmt::If {
                cond,
                then_body,
                else_body,
            } => {
                self.expr(cond)?;
                self.stmts(then_body)?;
                self.stmts(else_body)
            }
            Stmt::While { cond, body } => {
                self.expr(cond)?;
                self.stmts(body)
            }
            Stmt::For(for_stmt) => {
                self.expr(&for_stmt.init_value)?;
                self.expr(&for_stmt.cond)?;
                self.expr(&for_stmt.update_value)?;
                self.stmts(&for_stmt.body)
            }
            Stmt::Block(stmts) => self.stmts(stmts),
            Stmt::Break | Stmt::Goto(..) | Stmt::Label(..) | Stmt::Empty => Ok(()),
            // Gone after function expansion; kept total for the compiler.
            Stmt::Return { value, .. } => match value {
                Some(value) => self.expr(value),
                None => Ok(()),
            },
            Stmt::Function(_) => Ok(()),
        }
    }

    fn setup(&self, setup: Setup, pos: usize) -> Result<(), TranspileError> {
        match setup {
            Setup::Deg
            | Setup::Rad
            | Setup::Gra
            | Setup::Fix(_)
            | Setup::Sci(_)
            | Setup::Norm(_) => self.require(
                self.mode.allows_setup(),
                "COMP, CMPLX, SD or REG",
                "a display/angle setup command",
                Some(pos),
            ),
            Setup::Dec | Setup::Hex | Setup::Bin | Setup::Oct => self.require(
                self.mode.allows_base(),
                "BASE",
                &format!("`{}()`", setup.call_name()),
                Some(pos),
            ),
            Setup::Cartesian | Setup::Polar => self.require(
                self.mode.allows_complex(),
                "CMPLX",
                "a complex format command",
                Some(pos),
            ),
            Setup::ReIm => {
                self.require(self.mode.allows_complex(), "CMPLX", "`re_im()`", Some(pos))
            }
            Setup::Reg(reg) => self.require(
                self.mode.allows_regression(),
                "REG",
                &format!("`{}()`", reg.call_name()),
                Some(pos),
            ),
            Setup::Sexagesimal => self.require(
                self.mode.allows_float_math(),
                "COMP, CMPLX, SD or REG",
                "`dms()`",
                Some(pos),
            ),
        }
    }

    fn expr(&self, expr: &Expr) -> Result<(), TranspileError> {
        match expr {
            Expr::Number(_) | Expr::Name(..) | Expr::Input(_) | Expr::Ans(_) => Ok(()),
            Expr::Pi(pos) => self.require(
                self.mode.allows_float_math(),
                "COMP, CMPLX, SD or REG",
                "`pi`",
                Some(*pos),
            ),
            Expr::E(pos) => self.require(
                self.mode.allows_float_math(),
                "COMP, CMPLX, SD or REG",
                "`e`",
                Some(*pos),
            ),
            // A scientific constant is a real number the BASE keypad cannot enter.
            Expr::Constant(constant, pos) => self.require(
                self.mode.allows_float_math(),
                "COMP, CMPLX, SD or REG",
                &format!("`{}` ({})", constant.name, constant.description),
                Some(*pos),
            ),
            Expr::BaseLiteral { base, pos, .. } => self.require(
                self.mode.allows_base(),
                "BASE",
                &format!("a `{}` literal", base.suffix()),
                Some(*pos),
            ),
            Expr::StatVar(var, pos) => {
                self.require(
                    self.mode.allows_stats(),
                    "SD or REG",
                    &format!("the statistical variable `stat.{}`", var.ascii()),
                    Some(*pos),
                )?;
                if is_regression_var(*var) {
                    self.require(
                        self.mode.allows_regression(),
                        "REG",
                        &format!("the regression variable `stat.{}`", var.ascii()),
                        Some(*pos),
                    )?;
                }
                Ok(())
            }
            Expr::Unary(_, inner) => self.expr(inner),
            Expr::Binary(op, left, right) => {
                use crate::ast::BinOp;
                if matches!(op, BinOp::And | BinOp::Or | BinOp::Xor | BinOp::Xnor) {
                    self.require(self.mode.allows_base(), "BASE", "a bitwise operator", None)?;
                }
                self.expr(left)?;
                self.expr(right)
            }
            // The call's name precedes its arguments in source order, so a
            // forbidden built-in is reported before anything nested inside it.
            Expr::Call(name, args, pos) => {
                self.call(name, *pos)?;
                for arg in args {
                    self.expr(arg)?;
                }
                Ok(())
            }
            Expr::Data { .. } => Ok(()),
        }
    }

    /// The mode a built-in requires, if any.
    fn call(&self, name: &str, pos: usize) -> Result<(), TranspileError> {
        match name {
            // `Ans` exists in every mode.
            "ans" => Ok(()),
            "not" | "neg" => self.require(
                self.mode.allows_base(),
                "BASE",
                &format!("`{name}`"),
                Some(pos),
            ),
            "arg" | "conjg" | "i" | "polar" => self.require(
                self.mode.allows_complex(),
                "CMPLX",
                &format!("`{name}`"),
                Some(pos),
            ),
            "pol" | "rec" => self.require(
                self.mode.allows_float_math() && !self.mode.allows_stats(),
                "COMP or CMPLX",
                &format!("`{name}`"),
                Some(pos),
            ),
            // `is_float_builtin`-style fallback: every remaining callable key is
            // a floating-point function BASE does not offer.
            _ if builtins::lookup(name).is_some() => self.require(
                self.mode.allows_float_math(),
                "COMP, CMPLX, SD or REG",
                &format!("`{name}`"),
                Some(pos),
            ),
            // Unknown names are a parser concern.
            _ => Ok(()),
        }
    }

    fn require(
        &self,
        allowed: bool,
        needed: &str,
        what: &str,
        pos: Option<usize>,
    ) -> Result<(), TranspileError> {
        if allowed {
            return Ok(());
        }
        let message = format!(
            "{what} is not available in {} mode (needs {needed})",
            self.mode
        );
        Err(match pos {
            Some(pos) => TranspileError::at(self.source, message, pos),
            None => TranspileError::at(self.source, message, 0),
        })
    }
}

/// The `y`-statistics and regression variables only exist in REG mode.
fn is_regression_var(var: StatVar) -> bool {
    matches!(
        var,
        StatVar::SumY
            | StatVar::SumY2
            | StatVar::SumXY
            | StatVar::MeanY
            | StatVar::SigmaY
            | StatVar::Sy
            | StatVar::MinY
            | StatVar::MaxY
            | StatVar::RegA
            | StatVar::RegB
            | StatVar::RegC
            | StatVar::RegR
    )
}

//! Static mode checking.
//!
//! The fx-50FH II only offers certain constructs in certain modes: complex
//! numbers exist in CMPLX, statistics in SD/REG, base-n in BASE.  A program
//! declares its mode with a `#mode` header (defaulting to COMP), and this pass
//! rejects anything the declared mode does not offer.
//!
//! The hardware never shows these errors — it prevents them by not offering
//! the key — so this is a source-level diagnostic rather than a faithful
//! error screen.  It is reported as [`CalcError::Mode`], labelled
//! `Mode ERROR`.

use crate::ast::{Expr, Setup, Stmt};
use crate::bases::Base;
use crate::error::CalcError;
use crate::mode::Mode;
use crate::stats::StatVar;
use crate::token::{BinOp, ConstName, FuncName};

/// The mode declared by a program's leading `#mode` directive.
///
/// Defaults to [`Mode::Comp`] when there is no directive.
pub fn declared_mode(program: &[Stmt]) -> Mode {
    program
        .iter()
        .find_map(|stmt| match stmt {
            Stmt::Mode(mode) => Some(*mode),
            _ => None,
        })
        .unwrap_or_default()
}

/// Reject any construct the program's declared mode does not offer.
pub fn check(program: &[Stmt]) -> Result<(), CalcError> {
    Checker::new(declared_mode(program)).check_program(program)
}

struct Checker {
    mode: Mode,
}

impl Checker {
    fn new(mode: Mode) -> Self {
        Checker { mode }
    }

    fn check_program(&self, program: &[Stmt]) -> Result<(), CalcError> {
        for stmt in program {
            self.check_stmt(stmt)?;
        }
        Ok(())
    }

    /// Report a construct that needs `needed`, unless the current mode is it.
    fn require(&self, allowed: bool, needed: &str, what: &str) -> Result<(), CalcError> {
        if allowed {
            return Ok(());
        }
        Err(CalcError::mode(
            self.mode,
            format!(
                "{what} is only available in {needed} mode; add `#mode {needed}` at the head of the program"
            ),
            None,
        ))
    }

    fn complex(&self, what: &str) -> Result<(), CalcError> {
        self.require(self.mode.allows_complex(), "CMPLX", what)
    }

    fn stats(&self, what: &str) -> Result<(), CalcError> {
        self.require(self.mode.allows_stats(), "SD or REG", what)
    }

    fn regression(&self, what: &str) -> Result<(), CalcError> {
        self.require(self.mode.allows_regression(), "REG", what)
    }

    fn base(&self, what: &str) -> Result<(), CalcError> {
        self.require(self.mode.allows_base(), "BASE", what)
    }

    fn float_math(&self, what: &str) -> Result<(), CalcError> {
        self.require(
            self.mode.allows_float_math(),
            "COMP, CMPLX, SD or REG",
            what,
        )
    }

    // -- statements ---------------------------------------------------------

    fn check_stmt(&self, stmt: &Stmt) -> Result<(), CalcError> {
        match stmt {
            Stmt::Mode(_) | Stmt::Noop | Stmt::Then | Stmt::Else | Stmt::IfEnd => Ok(()),
            Stmt::Next | Stmt::WhileEnd | Stmt::Break | Stmt::ClrMemory => Ok(()),
            Stmt::Goto(_) | Stmt::Label(_) => Ok(()),

            Stmt::Expr { expr, .. } => self.check_expr(expr),
            Stmt::Assign { value, .. } => self.check_expr(value),
            Stmt::Memory { expr, .. } => self.check_expr(expr),
            Stmt::CondJump { condition, target } => {
                self.check_expr(condition)?;
                self.check_stmt(target)
            }
            Stmt::If { condition } | Stmt::While { condition } => self.check_expr(condition),
            Stmt::For { from, to, step, .. } => {
                self.check_expr(from)?;
                self.check_expr(to)?;
                if let Some(step) = step {
                    self.check_expr(step)?;
                }
                Ok(())
            }
            Stmt::Setup(setup) => self.check_setup(*setup),
            Stmt::ClrStat => self.stats("`ClrStat`"),
            Stmt::FreqOn | Stmt::FreqOff => self.stats("`FreqOn`/`FreqOff`"),
            Stmt::DataEntry { x, y, freq } => {
                self.stats("data entry (`DT`)")?;
                if y.is_some() {
                    self.regression("paired-variable data entry (`x,y DT`)")?;
                }
                self.check_expr(x)?;
                if let Some(y) = y {
                    self.check_expr(y)?;
                }
                if let Some(freq) = freq {
                    self.check_expr(freq)?;
                }
                Ok(())
            }
        }
    }

    fn check_setup(&self, setup: Setup) -> Result<(), CalcError> {
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
            ),
            Setup::Dec | Setup::Hex | Setup::Bin | Setup::Oct => self.base("a number base"),
            Setup::ComplexCartesian | Setup::ComplexPolar => self.complex("a complex format"),
        }
    }

    // -- expressions --------------------------------------------------------

    fn check_expr(&self, expr: &Expr) -> Result<(), CalcError> {
        match expr {
            Expr::Number(_) | Expr::Var(_) | Expr::Input | Expr::Ran => {
                // `Ran#` is not offered in BASE, which works on integers.
                if matches!(expr, Expr::Ran) {
                    return self.float_math("`Ran#`");
                }
                Ok(())
            }
            Expr::BaseLiteral { base, .. } => {
                self.base(&format!("the `{}` base tag", base.suffix()))
            }
            Expr::Const(ConstName::I) => self.complex("the imaginary unit `i`"),
            Expr::Const(ConstName::Pi | ConstName::E) => self.float_math("`π`/`e`"),
            Expr::Const(ConstName::Physical(code)) => {
                // Scientific constants are real numbers, so BASE mode has no
                // way to enter them.
                let what = crate::constants::by_code(*code)
                    .map_or("a scientific constant", |c| c.description);
                self.float_math(what)
            }
            Expr::StatVar(var) => self.check_stat_var(*var),

            Expr::Unary { op, expr } => {
                use crate::ast::UnaryOp;
                match op {
                    UnaryOp::Inverse => self.float_math("`x⁻¹`"),
                    UnaryOp::Fact => self.float_math("factorial `!`"),
                    UnaryOp::Percent => self.float_math("percent `%`"),
                    UnaryOp::Neg | UnaryOp::Square | UnaryOp::Cube => Ok(()),
                }?;
                self.check_expr(expr)
            }

            Expr::ImplicitMul(a, b) => {
                self.check_expr(a)?;
                self.check_expr(b)
            }
            Expr::Binary { left, op, right } => {
                self.check_binop(*op)?;
                self.check_expr(left)?;
                self.check_expr(right)
            }
            Expr::Pow { base, exp } => {
                self.float_math("`^(`")?;
                self.check_expr(base)?;
                self.check_expr(exp)
            }
            Expr::Root { index, radicand } => {
                self.float_math("`x√(`")?;
                self.check_expr(index)?;
                self.check_expr(radicand)
            }
            Expr::Call { func, args } => {
                self.check_func(*func)?;
                for arg in args {
                    self.check_expr(arg)?;
                }
                Ok(())
            }
            Expr::Assign { value, .. } => self.check_expr(value),
        }
    }

    fn check_binop(&self, op: BinOp) -> Result<(), CalcError> {
        match op {
            BinOp::Frac => self.float_math("a fraction `┘`"),
            BinOp::Perm => self.float_math("`nPr`"),
            BinOp::Comb => self.float_math("`nCr`"),
            BinOp::Polar => self.complex("the polar form `r∠θ`"),
            BinOp::And | BinOp::Or | BinOp::Xor | BinOp::Xnor => self.base("a bitwise operator"),
            BinOp::Add
            | BinOp::Sub
            | BinOp::Mul
            | BinOp::Div
            | BinOp::Eq
            | BinOp::Ne
            | BinOp::Gt
            | BinOp::Lt
            | BinOp::Ge
            | BinOp::Le => Ok(()),
        }
    }

    fn check_func(&self, func: FuncName) -> Result<(), CalcError> {
        match func {
            FuncName::Arg | FuncName::Conjg => self.complex("`arg(`/`Conjg(`"),
            FuncName::Not | FuncName::Neg => self.base("`Not(`/`Neg(`"),
            FuncName::Pol | FuncName::Rec => {
                // Available in COMP and CMPLX, not in statistics or BASE.
                if self.mode.allows_float_math() && !self.mode.allows_stats() {
                    Ok(())
                } else {
                    Err(CalcError::mode(
                        self.mode,
                        "`Pol(`/`Rec(` is only available in COMP or CMPLX mode",
                        None,
                    ))
                }
            }
            FuncName::Sin
            | FuncName::Cos
            | FuncName::Tan
            | FuncName::Asin
            | FuncName::Acos
            | FuncName::Atan
            | FuncName::Sinh
            | FuncName::Cosh
            | FuncName::Tanh
            | FuncName::Asinh
            | FuncName::Acosh
            | FuncName::Atanh
            | FuncName::Log
            | FuncName::Ln
            | FuncName::Sqrt
            | FuncName::Cbrt
            | FuncName::TenPow
            | FuncName::EPow
            | FuncName::Abs
            | FuncName::Rnd => self.float_math("this function"),
        }
    }

    fn check_stat_var(&self, var: StatVar) -> Result<(), CalcError> {
        self.stats(&format!(
            "the statistical variable `{}`",
            stat_var_name(var)
        ))?;
        if is_regression_var(var) {
            self.regression(&format!("the regression variable `{}`", stat_var_name(var)))?;
        }
        Ok(())
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
            | StatVar::RegR
    )
}

fn stat_var_name(var: StatVar) -> &'static str {
    use StatVar::*;
    match var {
        N => "n",
        SumX => "Σx",
        SumX2 => "Σx²",
        SumY => "Σy",
        SumY2 => "Σy²",
        SumXY => "Σxy",
        MeanX => "x̄",
        MeanY => "ȳ",
        SigmaX => "σx",
        SigmaY => "σy",
        Sx => "sx",
        Sy => "sy",
        MinX => "minX",
        MaxX => "maxX",
        MinY => "minY",
        MaxY => "maxY",
        RegA => "regA",
        RegB => "regB",
        RegR => "regR",
    }
}

/// Validate that a `#mode` directive names a mode.  Used by the transpiler and
/// the CLI for their own headers.
pub fn parse_mode(name: &str) -> Option<Mode> {
    Mode::parse(name)
}

/// Convenience: the base a tagged literal requires, if any.
pub fn tagged_base(expr: &Expr) -> Option<Base> {
    match expr {
        Expr::BaseLiteral { base, .. } => Some(*base),
        _ => None,
    }
}

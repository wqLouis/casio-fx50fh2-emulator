//! Translation from the `.fxc` AST to fx-50FH II PRGM source.
//!
//! The emitter walks the AST once, in source order, and writes one PRGM
//! statement per line. Variable names are resolved through [`Allocator`] before
//! any output is produced, so an "eight variables" error never leaves a
//! half-written program behind.
//!
//! Glyph vs. ASCII output is controlled by [`crate::Options::ascii`]: the
//! default uses the calculator's `→ ◢ × ÷ ≠ ≤ ≥ π` glyphs, while ASCII mode
//! spells the same program with `-> disp * / <> <= >= pi`.

use crate::Options;
use crate::alloc::Allocator;
use crate::ast::{BinOp, Expr, ForStmt, Program, Stmt, UnOp};
use crate::builtins;
use crate::error::TranspileError;

/// Operator precedence levels for the `.fxc` grammar, mirroring
/// [`crate::parser`]. Higher binds tighter.
mod prec {
    pub const EQUALITY: u8 = 1;
    pub const COMPARISON: u8 = 2;
    pub const ADDITIVE: u8 = 3;
    pub const MULTIPLICATIVE: u8 = 4;
    pub const UNARY: u8 = 5;
    pub const POWER: u8 = 6;
    pub const ATOM: u8 = 7;
}

/// Transpile a parsed program into PRGM source.
pub fn emit(program: &Program, source: &str, opts: Options) -> Result<String, TranspileError> {
    let allocator = Allocator::collect(program, source)?;
    let mut emitter = Emitter {
        out: String::new(),
        allocator,
        opts,
        source,
    };
    emitter.stmts(program)?;
    Ok(emitter.out)
}

struct Emitter<'a> {
    out: String,
    allocator: Allocator,
    opts: Options,
    source: &'a str,
}

impl Emitter<'_> {
    // -- output helpers -----------------------------------------------------

    fn line(&mut self, text: impl AsRef<str>) {
        self.out.push_str(text.as_ref());
        self.out.push('\n');
    }

    /// The spelling of the `◢` display key for the selected output style.
    fn display(&self) -> &'static str {
        if self.opts.ascii { "disp" } else { "◢" }
    }

    fn arrow(&self) -> &'static str {
        if self.opts.ascii { "->" } else { "→" }
    }

    // -- statements ---------------------------------------------------------

    fn stmts(&mut self, stmts: &[Stmt]) -> Result<(), TranspileError> {
        for stmt in stmts {
            self.stmt(stmt)?;
        }
        Ok(())
    }

    fn stmt(&mut self, stmt: &Stmt) -> Result<(), TranspileError> {
        match stmt {
            Stmt::Let { name, value, pos } | Stmt::Assign { name, value, pos } => {
                self.assignment(name, *pos, value)?;
            }
            Stmt::Print(expr) => {
                let value = self.expr(expr, 0)?;
                let display = self.display();
                self.line(format!("{value}{display}"));
            }
            Stmt::ExprStmt(expr) => {
                let value = self.expr(expr, 0)?;
                self.line(value);
            }
            Stmt::If {
                cond,
                then_body,
                else_body,
            } => {
                let condition = self.expr(cond, 0)?;
                self.line(format!("If {condition}"));
                // Always emit `Then`: without it PRGM would guard only the
                // first statement, which is not what a `.fxc` block means.
                self.line("Then");
                self.stmts(then_body)?;
                if !else_body.is_empty() {
                    self.line("Else");
                    self.stmts(else_body)?;
                }
                self.line("IfEnd");
            }
            Stmt::While { cond, body } => {
                let condition = self.expr(cond, 0)?;
                self.line(format!("While {condition}"));
                self.stmts(body)?;
                self.line("WhileEnd");
            }
            Stmt::For(for_stmt) => self.for_statement(for_stmt)?,
            Stmt::Break => self.line("Break"),
            Stmt::Goto(label, _) => self.line(format!("Goto {label}")),
            Stmt::Label(label, _) => self.line(format!("Lbl {label}")),
            Stmt::Block(stmts) => self.stmts(stmts)?,
            Stmt::Empty => {}
        }
        Ok(())
    }

    /// `x = value` (and `let x = value`), including the `input()` special case.
    fn assignment(&mut self, name: &str, pos: usize, value: &Expr) -> Result<(), TranspileError> {
        let var = self.variable(name, pos)?;
        self.assign_to(value, var)
    }

    /// Emit `<value>→<var>` (or `?→<var>` when the value is `input()`).
    fn assign_to(&mut self, value: &Expr, var: char) -> Result<(), TranspileError> {
        if matches!(value, Expr::Input(_)) {
            let arrow = self.arrow();
            self.line(format!("?{arrow}{var}"));
            return Ok(());
        }
        let text = self.expr(value, 0)?;
        let arrow = self.arrow();
        self.line(format!("{text}{arrow}{var}"));
        Ok(())
    }

    // -- for loops ----------------------------------------------------------

    fn for_statement(&mut self, for_stmt: &ForStmt) -> Result<(), TranspileError> {
        match self.for_parts(for_stmt)? {
            Some((init, to, step)) => {
                let var = self.variable(&for_stmt.init_name, for_stmt.pos)?;
                let arrow = self.arrow();
                self.line(format!("For {init}{arrow}{var} To {to} Step {step}"));
                self.stmts(&for_stmt.body)?;
                self.line("Next");
            }
            // Unsupported shape: fall back to a hand-written `While` loop.
            None => self.for_as_while(for_stmt)?,
        }
        Ok(())
    }

    /// Try to express a `for` as a native PRGM `For`/`Next`.
    ///
    /// The canonical shape is
    /// `for (i = init; i <op> limit; i = i <update> step)` with the variable
    /// matching in all three places. Returns `(init, To-limit, Step)` when the
    /// loop can be translated, or `None` when it must fall back to `While`.
    fn for_parts(
        &self,
        for_stmt: &ForStmt,
    ) -> Result<Option<(String, String, String)>, TranspileError> {
        let Expr::Binary(cond_op, cond_left, limit) = &for_stmt.cond else {
            return Ok(None);
        };
        let Expr::Name(cond_name, _) = cond_left.as_ref() else {
            return Ok(None);
        };
        if cond_name != &for_stmt.init_name || cond_name != &for_stmt.update_name {
            return Ok(None);
        }

        let Expr::Binary(update_op, update_left, step_expr) = &for_stmt.update_value else {
            return Ok(None);
        };
        let Expr::Name(update_name, _) = update_left.as_ref() else {
            return Ok(None);
        };
        if update_name != &for_stmt.update_name {
            return Ok(None);
        }

        let init = self.expr(&for_stmt.init_value, 0)?;
        let (to, step) = match (cond_op, update_op) {
            // Ascending loops.
            (BinOp::Lt, BinOp::Add) => (self.offset(limit, -1.0)?, self.expr(step_expr, 0)?),
            (BinOp::Le, BinOp::Add) => (self.expr(limit, 0)?, self.expr(step_expr, 0)?),
            // Descending loops (the update subtracts, so negate the step).
            (BinOp::Gt, BinOp::Sub) => (self.offset(limit, 1.0)?, self.negated(step_expr)?),
            (BinOp::Ge, BinOp::Sub) => (self.expr(limit, 0)?, self.negated(step_expr)?),
            _ => return Ok(None),
        };
        Ok(Some((init, to, step)))
    }

    /// Emit the `While` fallback for a `for` loop that has an unusual shape.
    fn for_as_while(&mut self, for_stmt: &ForStmt) -> Result<(), TranspileError> {
        let init_var = self.variable(&for_stmt.init_name, for_stmt.pos)?;
        let init = self.expr(&for_stmt.init_value, 0)?;
        let arrow = self.arrow();
        self.line(format!("{init}{arrow}{init_var}"));

        let condition = self.expr(&for_stmt.cond, 0)?;
        self.line(format!("While {condition}"));
        self.stmts(&for_stmt.body)?;

        let update_var = self.variable(&for_stmt.update_name, for_stmt.pos)?;
        let update = self.expr(&for_stmt.update_value, 0)?;
        self.line(format!("{update}{arrow}{update_var}"));
        self.line("WhileEnd");
        Ok(())
    }

    /// `expr ± offset` as a PRGM expression, parenthesising only when needed.
    fn offset(&self, expr: &Expr, delta: f64) -> Result<String, TranspileError> {
        let op = if delta < 0.0 { BinOp::Sub } else { BinOp::Add };
        let combined = Expr::Binary(
            op,
            Box::new(expr.clone()),
            Box::new(Expr::Number(delta.abs())),
        );
        self.expr(&combined, 0)
    }

    /// `-expr`, parenthesising only when needed.
    fn negated(&self, expr: &Expr) -> Result<String, TranspileError> {
        let negated = Expr::Unary(UnOp::Neg, Box::new(expr.clone()));
        self.expr(&negated, 0)
    }

    // -- expressions --------------------------------------------------------

    /// Render `expr` for a context that binds at least as tightly as
    /// `parent_prec`, wrapping it in parentheses when it does not.
    fn expr(&self, expr: &Expr, parent_prec: u8) -> Result<String, TranspileError> {
        let (text, own_prec) = self.expr_prec(expr)?;
        if own_prec < parent_prec {
            Ok(format!("({text})"))
        } else {
            Ok(text)
        }
    }

    /// Render `expr` and report its precedence level.
    fn expr_prec(&self, expr: &Expr) -> Result<(String, u8), TranspileError> {
        match expr {
            Expr::Number(value) => Ok((format_number(*value), prec::ATOM)),
            Expr::Name(name, pos) => {
                let var = self.variable(name, *pos)?;
                Ok((var.to_string(), prec::ATOM))
            }
            Expr::Pi(_) => Ok((
                (if self.opts.ascii { "pi" } else { "π" }).to_string(),
                prec::ATOM,
            )),
            Expr::E(_) => Ok(("e".to_string(), prec::ATOM)),
            Expr::Input(pos) => Err(TranspileError::at(
                self.source,
                "`input()` can only be used directly as the right-hand side of an assignment",
                *pos,
            )),
            Expr::Unary(UnOp::Neg, inner) => {
                let operand = self.expr(inner, prec::UNARY + 1)?;
                Ok((format!("-{operand}"), prec::UNARY))
            }
            Expr::Binary(op, left, right) => self.binary(*op, left, right),
            Expr::Call(name, args, pos) => {
                let Some(builtin) = builtins::lookup(name) else {
                    return Err(TranspileError::at(
                        self.source,
                        format!("unknown function `{name}`"),
                        *pos,
                    ));
                };
                let mut rendered = Vec::with_capacity(args.len());
                for arg in args {
                    rendered.push(self.expr(arg, 0)?);
                }
                let spelling = builtin.spelling(self.opts.ascii);
                Ok((format!("{spelling}({})", rendered.join(",")), prec::ATOM))
            }
        }
    }

    fn binary(&self, op: BinOp, left: &Expr, right: &Expr) -> Result<(String, u8), TranspileError> {
        if op == BinOp::Pow {
            // `^(...)` is a parenthetical key: `base^(exponent)`.
            let base = self.expr(left, prec::POWER)?;
            let exponent = self.expr(right, 0)?;
            return Ok((format!("{base}^({exponent})"), prec::POWER));
        }

        let level = binary_precedence(op);
        let left_text = self.expr(left, level)?;
        // All `.fxc` binary operators are left-associative, so the right
        // operand needs parentheses at the same precedence.
        let right_text = self.expr(right, level + 1)?;
        let symbol = binary_symbol(op, self.opts.ascii);
        Ok((format!("{left_text}{symbol}{right_text}"), level))
    }

    /// Resolve a `.fxc` name to its calculator memory.
    fn variable(&self, name: &str, pos: usize) -> Result<char, TranspileError> {
        self.allocator.lookup(name).ok_or_else(|| {
            TranspileError::at(
                self.source,
                format!("internal error: variable `{name}` was not allocated"),
                pos,
            )
        })
    }
}

fn binary_precedence(op: BinOp) -> u8 {
    match op {
        BinOp::Eq | BinOp::Ne => prec::EQUALITY,
        BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => prec::COMPARISON,
        BinOp::Add | BinOp::Sub => prec::ADDITIVE,
        BinOp::Mul | BinOp::Div => prec::MULTIPLICATIVE,
        BinOp::Pow => prec::POWER,
    }
}

fn binary_symbol(op: BinOp, ascii: bool) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => {
            if ascii {
                "*"
            } else {
                "×"
            }
        }
        BinOp::Div => {
            if ascii {
                "/"
            } else {
                "÷"
            }
        }
        BinOp::Pow => "^",
        BinOp::Eq => "=",
        BinOp::Ne => {
            if ascii {
                "<>"
            } else {
                "≠"
            }
        }
        BinOp::Lt => "<",
        BinOp::Le => {
            if ascii {
                "<="
            } else {
                "≤"
            }
        }
        BinOp::Gt => ">",
        BinOp::Ge => {
            if ascii {
                ">="
            } else {
                "≥"
            }
        }
    }
}

/// Format an `f64` as PRGM source: shortest round-tripping decimal, no
/// scientific notation (which the calculator's keypad cannot enter).
fn format_number(value: f64) -> String {
    if value == 0.0 {
        // Collapse `-0` to `0`.
        return "0".to_string();
    }
    format!("{value}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_have_no_trailing_zero() {
        assert_eq!(format_number(5.0), "5");
        assert_eq!(format_number(0.5), "0.5");
        assert_eq!(format_number(-0.0), "0");
        assert_eq!(format_number(1e10), "10000000000");
    }

    #[test]
    fn symbols_switch_with_ascii() {
        assert_eq!(binary_symbol(BinOp::Mul, false), "×");
        assert_eq!(binary_symbol(BinOp::Mul, true), "*");
        assert_eq!(binary_symbol(BinOp::Ne, false), "≠");
        assert_eq!(binary_symbol(BinOp::Ne, true), "<>");
        assert_eq!(binary_symbol(BinOp::Le, true), "<=");
        assert_eq!(binary_symbol(BinOp::Ge, false), "≥");
    }
}

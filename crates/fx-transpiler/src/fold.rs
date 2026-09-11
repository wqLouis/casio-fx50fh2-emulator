//! Compile-time evaluation of constant expressions.
//!
//! The calculator has only **680 bytes of program storage shared by all four
//! program areas**, and every operator left in the emitted PRGM costs bytes. A
//! subexpression built only from number literals already has a value while
//! transpiling, so it is replaced by that value: `2×3+4` becomes `10`, saving
//! four keystrokes.
//!
//! ## What is folded
//!
//! * Arithmetic on number literals: `+ - × ÷ ^` and unary minus.
//! * Comparisons of number literals, which the machine evaluates to `1`/`0`.
//! * References to an earlier numeric `const`, which is inlined and then folded
//!   (`const k = 6; print(k + 1);` becomes `7◢`).
//!
//! ## What is deliberately *not* folded
//!
//! * **Symbolic constants.** `pi`, `e` and the `phys.` constants stay symbolic,
//!   because the calculator keys them in as their own symbols; folding `2 * pi`
//!   to a decimal would lose precision and cost *more* bytes, not fewer. This is
//!   [ADR 0015](../../../docs/DECISIONS.md).
//! * **Anything the machine would round.** The machine keeps 15 significant
//!   digits and applies an auto-correction pass after every operation, while a
//!   literal is *not* auto-corrected. So a value is folded only when the
//!   machine's own normalisation would leave it unchanged
//!   ([`normalize_stable`]); `1 / 3` and `0.1 + 0.2` therefore stay as written,
//!   since `0.333333333333333` and `0.30000000000000004` are not the values the
//!   machine would key in.

use crate::ast::{Accessor, BinOp, Expr, ForStmt, Program, Stmt, UnOp};

/// Fold every constant subexpression in `program`, in place.
///
/// Run after parsing and after [`crate::unroll`] has substituted loop
/// induction variables, then again on any expression built during emission
/// (the `for` offset).
pub fn fold_program(program: &mut Program) {
    Folder::default().stmts(program);
}

/// Fold one expression in place, leaving symbolic values alone.
pub fn fold_expr(expr: &mut Expr) {
    Folder::default().expr(expr);
}

#[derive(Default)]
struct Folder {
    /// `const` values declared so far, in order. A reference to a *numeric*
    /// `const` is replaced by its value; a symbolic one (`2 * pi`) is left for
    /// the emitter to inline, so its symbol survives.
    consts: Vec<(String, Expr)>,
}

impl Folder {
    fn stmts(&mut self, stmts: &mut [Stmt]) {
        for stmt in stmts {
            self.stmt(stmt);
        }
    }

    fn stmt(&mut self, stmt: &mut Stmt) {
        match stmt {
            Stmt::Let { value, .. } | Stmt::Assign { value, .. } => self.expr(value),
            Stmt::LetArray { values, .. } => {
                for value in values {
                    self.expr(value);
                }
            }
            Stmt::Const { name, value, .. } => {
                self.expr(value);
                // Recorded after folding, so a `const` cannot see itself.
                self.consts.push((name.clone(), value.clone()));
            }
            Stmt::AssignElement { value, .. } => self.expr(value),
            Stmt::AssignElementExpr { index, value, .. } => {
                self.expr(index);
                self.expr(value);
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
                self.stmt(target);
            }
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
            // Gone before folding; walked anyway so the pass is total.
            Stmt::Function(_) => {}
            Stmt::Return { value, .. } => {
                if let Some(value) = value {
                    self.expr(value);
                }
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
            Stmt::For(for_stmt) => self.for_stmt(for_stmt),
            Stmt::Block(stmts) => self.stmts(stmts),
        }
    }

    fn for_stmt(&mut self, for_stmt: &mut ForStmt) {
        self.expr(&mut for_stmt.init_value);
        self.expr(&mut for_stmt.cond);
        self.expr(&mut for_stmt.update_value);
        self.stmts(&mut for_stmt.body);
    }

    fn expr(&mut self, expr: &mut Expr) {
        // Children first, so `(2 + 3) * 4` folds the sum before the product.
        match expr {
            Expr::Unary(_, inner) => self.expr(inner),
            Expr::Binary(_, left, right) => {
                self.expr(left);
                self.expr(right);
            }
            Expr::Call(_, args, _) => {
                for arg in args {
                    self.expr(arg);
                }
            }
            Expr::Data { accessors, .. } => {
                for accessor in accessors {
                    if let Accessor::IndexExpr { expr, .. } = accessor {
                        self.expr(expr);
                    }
                }
            }
            // A reference to a numeric `const` is replaced by its value. The
            // value was itself folded when the declaration was walked, so it
            // needs no further folding.
            Expr::Name(name, _) => {
                if let Some(value) = self
                    .consts
                    .iter()
                    .rev()
                    .find(|(known, _)| known == name)
                    .map(|(_, value)| value.clone())
                {
                    *expr = value;
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
        if let Some(value) = evaluate(expr) {
            *expr = Expr::Number(value);
        }
    }
}

/// The value of a constant expression, or `None` when it is not one, or when
/// the machine's arithmetic would not produce exactly that value.
fn evaluate(expr: &Expr) -> Option<f64> {
    match expr {
        Expr::Number(value) => Some(*value),
        Expr::Unary(UnOp::Neg, inner) => {
            let value = evaluate(inner)?;
            let result = -value;
            normalize_stable(result).then_some(result)
        }
        Expr::Binary(op, left, right) => {
            let left = evaluate(left)?;
            let right = evaluate(right)?;
            apply(*op, left, right)
        }
        _ => None,
    }
}

fn apply(op: BinOp, left: f64, right: f64) -> Option<f64> {
    // Comparisons never round: the machine returns `1` or `0`.
    let comparison = match op {
        BinOp::Eq => Some(left == right),
        BinOp::Ne => Some(left != right),
        BinOp::Lt => Some(left < right),
        BinOp::Le => Some(left <= right),
        BinOp::Gt => Some(left > right),
        BinOp::Ge => Some(left >= right),
        _ => None,
    };
    if let Some(result) = comparison {
        return Some(if result { 1.0 } else { 0.0 });
    }

    let value = match op {
        BinOp::Add => left + right,
        BinOp::Sub => left - right,
        BinOp::Mul => left * right,
        BinOp::Div => {
            if right == 0.0 {
                return None;
            }
            left / right
        }
        BinOp::Pow => left.powf(right),
        // Bitwise keys depend on the base-n word size, which lives in the
        // runtime, so they are never folded.
        BinOp::And | BinOp::Or | BinOp::Xor | BinOp::Xnor => return None,
        BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => unreachable!(),
    };
    normalize_stable(value).then_some(value)
}

/// Whether the machine's normalisation leaves `value` unchanged.
///
/// The machine rounds every operation result to 15 significant digits and then
/// applies an auto-correction pass; a literal it reads from a program is *not*
/// corrected. So a value may be emitted as a literal only when those two steps
/// are the identity — otherwise the inlined literal would not be the number the
/// original expression produced.
///
/// This mirrors [`casio_fx50fh2::precision`]'s rules rather than duplicating
/// them: the value must already fit in 15 significant digits, and the last four
/// of them must be `0000` (rounding to 11 digits is a no-op) or in
/// `0010..=9990` (the band auto-correction leaves alone).
fn normalize_stable(value: f64) -> bool {
    if !value.is_finite() {
        return false;
    }
    if value == 0.0 {
        return true;
    }
    let text = format!("{value:.14e}");
    let Ok(rounded) = text.parse::<f64>() else {
        return false;
    };
    if rounded != value {
        return false;
    }
    let Some((mantissa, _)) = text.split_once('e') else {
        return false;
    };
    let digits: Vec<u32> = mantissa
        .bytes()
        .filter(u8::is_ascii_digit)
        .map(|byte| u32::from(byte - b'0'))
        .collect();
    if digits.len() != 15 {
        return false;
    }
    let lmno = digits[11] * 1000 + digits[12] * 100 + digits[13] * 10 + digits[14];
    lmno == 0 || (10..=9990).contains(&lmno)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;
    use crate::parser::parse;

    fn folded(source: &str) -> String {
        let tokens = lex(source).unwrap();
        let mut program = parse(&tokens, source).unwrap();
        fold_program(&mut program);
        format!("{program:?}")
    }

    #[test]
    fn integer_arithmetic_folds() {
        assert!(folded("print(2 * 3 + 4);").contains("Number(10.0)"));
    }

    #[test]
    fn nested_parentheses_fold() {
        assert!(folded("print((2 + 3) * 4);").contains("Number(20.0)"));
    }

    #[test]
    fn exact_division_folds() {
        assert!(folded("print(10 / 4);").contains("Number(2.5)"));
    }

    #[test]
    fn rounding_division_is_left_alone() {
        // 1/3 is 0.333333333333333 on the machine, so the division survives.
        assert!(folded("print(1 / 3);").contains("Binary(Div"));
    }

    #[test]
    fn an_inexact_sum_is_left_alone() {
        // 0.1 + 0.2 is 0.3 on the machine, not 0.30000000000000004.
        assert!(folded("print(0.1 + 0.2);").contains("Binary(Add"));
    }

    #[test]
    fn an_exact_sum_of_decimals_folds() {
        assert!(folded("print(0.5 + 0.25);").contains("Number(0.75)"));
    }

    #[test]
    fn symbolic_values_are_never_folded() {
        assert!(folded("print(2 * pi);").contains("Pi"));
        assert!(folded("print(e + 1);").contains("E"));
    }

    #[test]
    fn comparisons_become_one_or_zero() {
        assert!(folded("print(1 < 2);").contains("Number(1.0)"));
        assert!(folded("print(1 > 2);").contains("Number(0.0)"));
    }

    #[test]
    fn a_numeric_const_is_inlined_before_folding() {
        assert!(folded("const k = 2 * 3;\nprint(k + 1);").contains("Number(7.0)"));
    }

    #[test]
    fn a_symbolic_const_stays_symbolic() {
        assert!(folded("const c = 2 * pi;\nprint(c);").contains("Pi"));
    }

    #[test]
    fn normalize_stability_matches_the_machine() {
        assert!(normalize_stable(0.0));
        assert!(normalize_stable(6.0));
        assert!(normalize_stable(2.5));
        assert!(normalize_stable(0.2));
        assert!(normalize_stable(123456789012345.0));
        assert!(!normalize_stable(1.0 / 3.0));
        assert!(!normalize_stable(0.1 + 0.2));
        assert!(!normalize_stable(f64::INFINITY));
        // An integer whose last significant digits auto-correct away.
        assert!(!normalize_stable(100000000000001.0));
    }

    /// `normalize_stable` must be exactly `normalize(v) == v`, or folding would
    /// emit a literal the machine would never have produced.
    #[cfg(feature = "execute")]
    #[test]
    fn normalize_stability_agrees_with_the_interpreter() {
        let values = [
            0.0,
            1.0,
            -1.0,
            0.5,
            0.2,
            2.5,
            1.0 / 3.0,
            0.1 + 0.2,
            1e14,
            1e14 + 1.0,
            1.23456789012345,
            1.2345678901234,
            123456789012345.0,
            1e-7,
            123.456,
            6.02214076e23,
            f64::INFINITY,
        ];
        for value in values {
            // A non-finite value has no PRGM literal, so it is never folded.
            let expected = value.is_finite() && casio_fx50fh2::precision::normalize(value) == value;
            assert_eq!(normalize_stable(value), expected, "value {value}");
        }
    }
}

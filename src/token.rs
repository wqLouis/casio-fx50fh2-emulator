//! Tokens produced by the lexer.
//!
//! The fx-50FH II is a keystroke calculator: every key press is a token.
//! Program source is therefore a flat sequence of tokens where even the
//! `sin(` key is a single token.  This module models that vocabulary.

use std::fmt;

use crate::mode::Mode;
use crate::stats::{RegType, StatVar};
use crate::value::ComplexFormat;

/// The eight addressable memories of the calculator.
///
/// `Ans` is the "Answer" memory and is only ever read implicitly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VarName {
    A,
    B,
    C,
    D,
    X,
    Y,
    M,
    Ans,
}

impl VarName {
    /// Stable index used by [`crate::runtime::Environment`] for A..=M.
    pub fn slot(self) -> Option<usize> {
        Some(match self {
            VarName::A => 0,
            VarName::B => 1,
            VarName::C => 2,
            VarName::D => 3,
            VarName::X => 4,
            VarName::Y => 5,
            VarName::M => 6,
            VarName::Ans => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            VarName::A => "A",
            VarName::B => "B",
            VarName::C => "C",
            VarName::D => "D",
            VarName::X => "X",
            VarName::Y => "Y",
            VarName::M => "M",
            VarName::Ans => "Ans",
        }
    }
}

impl fmt::Display for VarName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Built-in mathematical constants, plus the calculator's 40 scientific
/// constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConstName {
    Pi,
    E,
    I,
    /// A scientific constant, identified by its menu number `1..=40`.
    ///
    /// The values live in [`crate::constants::CONSTANTS`] rather than in one
    /// enum variant per constant, so that adding or correcting a constant
    /// touches a single table.
    Physical(u8),
}

impl ConstName {
    /// The table entry behind a [`ConstName::Physical`], if there is one.
    pub fn physical(&self) -> Option<&'static crate::constants::PhysicalConstant> {
        match self {
            ConstName::Physical(code) => crate::constants::by_code(*code),
            _ => None,
        }
    }
}

/// Parenthetical functions (`sin(`, `log(`, …).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FuncName {
    Sin,
    Cos,
    Tan,
    Asin,
    Acos,
    Atan,
    Sinh,
    Cosh,
    Tanh,
    Asinh,
    Acosh,
    Atanh,
    Log,
    Ln,
    Sqrt,
    Cbrt,
    TenPow,
    EPow,
    Abs,
    Pol,
    Rec,
    Rnd,
    Arg,
    Conjg,
    /// `Not(` — bitwise complement (base-n mode).
    Not,
    /// `Neg(` — two's-complement negation (base-n mode).
    Neg,
}

impl FuncName {
    /// Every parenthetical function. Used by tests to guarantee the `.fxc`
    /// front end offers a spelling for each one.
    pub const ALL: [FuncName; 26] = [
        FuncName::Sin,
        FuncName::Cos,
        FuncName::Tan,
        FuncName::Asin,
        FuncName::Acos,
        FuncName::Atan,
        FuncName::Sinh,
        FuncName::Cosh,
        FuncName::Tanh,
        FuncName::Asinh,
        FuncName::Acosh,
        FuncName::Atanh,
        FuncName::Log,
        FuncName::Ln,
        FuncName::Sqrt,
        FuncName::Cbrt,
        FuncName::TenPow,
        FuncName::EPow,
        FuncName::Abs,
        FuncName::Pol,
        FuncName::Rec,
        FuncName::Rnd,
        FuncName::Arg,
        FuncName::Conjg,
        FuncName::Not,
        FuncName::Neg,
    ];
}

/// Postfix operators that bind to the value on their left.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Postfix {
    /// `x⁻¹`
    Inverse,
    /// `x²`
    Square,
    /// `x³`
    Cube,
    /// `x!`
    Fact,
    /// `x%`
    Percent,
}

impl Postfix {
    /// Every postfix key. Used by the `.fxc` coverage test.
    pub const ALL: [Postfix; 5] = [
        Postfix::Inverse,
        Postfix::Square,
        Postfix::Cube,
        Postfix::Fact,
        Postfix::Percent,
    ];
}

/// Infix binary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    /// The `┘` fraction key.  Binds tighter than `×`/`÷`.
    Frac,
    Perm,
    Comb,
    Eq,
    Ne,
    Gt,
    Lt,
    Ge,
    Le,
    And,
    Or,
    Xor,
    Xnor,
    /// The `∠` polar form `r∠θ`.
    Polar,
}

impl BinOp {
    /// Every infix operator. Used by the `.fxc` coverage test.
    pub const ALL: [BinOp; 18] = [
        BinOp::Add,
        BinOp::Sub,
        BinOp::Mul,
        BinOp::Div,
        BinOp::Frac,
        BinOp::Perm,
        BinOp::Comb,
        BinOp::Eq,
        BinOp::Ne,
        BinOp::Gt,
        BinOp::Lt,
        BinOp::Ge,
        BinOp::Le,
        BinOp::And,
        BinOp::Or,
        BinOp::Xor,
        BinOp::Xnor,
        BinOp::Polar,
    ];
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub lexeme: String,
    /// Byte offset into the source, for diagnostics.
    pub pos: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    /// A numeric literal, raw as written (may contain an `E` exponent or a
    /// base tag such as `1Fh`).
    Number(String),
    /// The leading `#mode NAME` directive.
    ModeDirective(Mode),
    Var(VarName),
    Const(ConstName),
    Func(FuncName),
    Postfix(Postfix),
    /// `^(`
    Pow,
    /// `x√(`
    Root,
    Op(BinOp),
    /// A statistical variable such as `Σx`, `x̄` or `σx`.
    StatVar(StatVar),
    LParen,
    RParen,
    Comma,
    /// `?`
    Input,
    /// `→`
    Assign,
    /// `:` (also produced for newlines)
    Colon,
    /// `◢`
    Display,
    /// `⇒`
    CondJump,
    Goto,
    Lbl,
    If,
    Then,
    Else,
    IfEnd,
    For,
    To,
    Step,
    Next,
    While,
    WhileEnd,
    Break,
    MPlus,
    MMinus,
    ClrMemory,
    ClrStat,
    FreqOn,
    FreqOff,
    Deg,
    Rad,
    Gra,
    Fix,
    Sci,
    Norm,
    /// `Dec`
    Dec,
    /// `Hex`
    Hex,
    /// `Bin`
    Bin,
    /// `Oct`
    Oct,
    /// `▶a+b𝑖` / `▶r∠θ`
    ComplexFormat(ComplexFormat),
    /// `Re⇔Im` — toggle which part of a complex result is displayed.
    ReIm,
    /// A regression model selected in REG mode (`Lin`, `Log`, …).
    Regression(RegType),
    /// A sexagesimal (degrees/minutes/seconds) literal such as `2°15′18″`,
    /// already reduced to `deg + min/60 + sec/3600`.
    Sexagesimal(f64),
    /// The bare `°′″` key: toggle a displayed value between decimal and
    /// sexagesimal.
    DmsToggle,
    /// `;` used by `x,y;f DT`
    Semicolon,
    DT,
    Ran,
    Eof,
}

impl TokenKind {
    /// Human-readable name used in parser errors.
    pub fn describe(&self) -> &'static str {
        match self {
            TokenKind::Number(_) => "number",
            TokenKind::ModeDirective(_) => "mode directive",
            TokenKind::Var(_) => "variable",
            TokenKind::Const(_) => "constant",
            TokenKind::Func(_) => "function",
            TokenKind::Postfix(_) => "postfix operator",
            TokenKind::Pow => "^(",
            TokenKind::Root => "x√(",
            TokenKind::Op(_) => "operator",
            TokenKind::LParen => "(",
            TokenKind::RParen => ")",
            TokenKind::Comma => ",",
            TokenKind::Input => "?",
            TokenKind::Assign => "→",
            TokenKind::Colon => ":",
            TokenKind::Display => "◢",
            TokenKind::CondJump => "⇒",
            TokenKind::Goto => "Goto",
            TokenKind::Lbl => "Lbl",
            TokenKind::If => "If",
            TokenKind::Then => "Then",
            TokenKind::Else => "Else",
            TokenKind::IfEnd => "IfEnd",
            TokenKind::For => "For",
            TokenKind::To => "To",
            TokenKind::Step => "Step",
            TokenKind::Next => "Next",
            TokenKind::While => "While",
            TokenKind::WhileEnd => "WhileEnd",
            TokenKind::Break => "Break",
            TokenKind::MPlus => "M+",
            TokenKind::MMinus => "M-",
            TokenKind::ClrMemory => "ClrMemory",
            TokenKind::ClrStat => "ClrStat",
            TokenKind::FreqOn => "FreqOn",
            TokenKind::FreqOff => "FreqOff",
            TokenKind::Deg => "Deg",
            TokenKind::Rad => "Rad",
            TokenKind::Gra => "Gra",
            TokenKind::Fix => "Fix",
            TokenKind::Sci => "Sci",
            TokenKind::Norm => "Norm",
            TokenKind::Dec => "Dec",
            TokenKind::Hex => "Hex",
            TokenKind::Bin => "Bin",
            TokenKind::Oct => "Oct",
            TokenKind::ComplexFormat(_) => "complex format",
            TokenKind::ReIm => "Re⇔Im",
            TokenKind::Regression(_) => "regression model",
            TokenKind::Sexagesimal(_) => "sexagesimal value",
            TokenKind::DmsToggle => "°′″",
            TokenKind::Semicolon => ";",
            TokenKind::StatVar(_) => "statistical variable",
            TokenKind::DT => "DT",
            TokenKind::Ran => "Ran#",
            TokenKind::Eof => "end of program",
        }
    }
}

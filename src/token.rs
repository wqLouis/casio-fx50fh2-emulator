//! Tokens produced by the lexer.
//!
//! The fx-50FH II is a keystroke calculator: every key press is a token.
//! Program source is therefore a flat sequence of tokens where even the
//! `sin(` key is a single token.  This module models that vocabulary.

use std::fmt;

use crate::stats::StatVar;
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

/// Built-in mathematical constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConstName {
    Pi,
    E,
    I,
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

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub lexeme: String,
    /// Byte offset into the source, for diagnostics.
    pub pos: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    /// A numeric literal, raw as written (may contain an `E` exponent).
    Number(String),
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
            TokenKind::Semicolon => ";",
            TokenKind::StatVar(_) => "statistical variable",
            TokenKind::DT => "DT",
            TokenKind::Ran => "Ran#",
            TokenKind::Eof => "end of program",
        }
    }
}

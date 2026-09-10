//! Abstract syntax tree for fx-50FH II programs.

use crate::bases::Base;
use crate::mode::Mode;
use crate::stats::StatVar;
use crate::token::{BinOp, ConstName, FuncName, VarName};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Inverse,
    Square,
    Cube,
    Fact,
    Percent,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Number(f64),
    /// A base-tagged literal such as `1Fh`, `1010b`, `17o` or `42d`.
    BaseLiteral {
        value: f64,
        base: Base,
    },
    Var(VarName),
    Const(ConstName),
    /// The `?` prompt; evaluates to the value entered by the user.
    Input,
    /// `Ran#`
    Ran,
    /// A statistical variable (`Σx`, `x̄`, …).
    StatVar(StatVar),
    /// `value → target`
    Assign {
        target: VarName,
        value: Box<Expr>,
    },
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    Binary {
        left: Box<Expr>,
        op: BinOp,
        right: Box<Expr>,
    },
    /// Multiplication written by juxtaposition, e.g. `2π` or `3(4+5)`.
    ImplicitMul(Box<Expr>, Box<Expr>),
    /// `base^(exp)`
    Pow {
        base: Box<Expr>,
        exp: Box<Expr>,
    },
    /// `index x√(radicand)`
    Root {
        index: Box<Expr>,
        radicand: Box<Expr>,
    },
    Call {
        func: FuncName,
        args: Vec<Expr>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemOp {
    Plus,
    Minus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setup {
    Deg,
    Rad,
    Gra,
    Fix(u8),
    Sci(u8),
    Norm(u8),
    /// `Dec`
    Dec,
    /// `Hex`
    Hex,
    /// `Bin`
    Bin,
    /// `Oct`
    Oct,
    /// `▶a+b𝑖`
    ComplexCartesian,
    /// `▶r∠θ`
    ComplexPolar,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    /// Evaluate an expression. `display` corresponds to a trailing `◢`.
    Expr {
        expr: Expr,
        display: bool,
    },
    Assign {
        target: VarName,
        value: Expr,
        display: bool,
    },
    /// `expr M+` / `expr M-`
    Memory {
        expr: Expr,
        op: MemOp,
        display: bool,
    },
    Goto(u8),
    Label(u8),
    /// `condition ⇒ statement`
    CondJump {
        condition: Expr,
        target: Box<Stmt>,
    },
    If {
        condition: Expr,
    },
    Then,
    Else,
    IfEnd,
    For {
        var: VarName,
        from: Expr,
        to: Expr,
        step: Option<Expr>,
    },
    Next,
    While {
        condition: Expr,
    },
    WhileEnd,
    Break,
    Setup(Setup),
    ClrMemory,
    ClrStat,
    FreqOn,
    FreqOff,
    /// `x DT` / `x,y DT` / `x,y;f DT`
    DataEntry {
        x: Expr,
        y: Option<Expr>,
        freq: Option<Expr>,
    },
    /// The leading `#mode NAME` directive.  Always the first statement when
    /// present.
    Mode(Mode),
    /// An empty statement, e.g. from a stray `:`.
    Noop,
}

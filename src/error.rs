//! Runtime and parse errors, named after the calculator's own error screens.

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum CalcError {
    /// `Syntax ERROR`
    Syntax { message: String, pos: Option<usize> },
    /// `Math ERROR`
    Math(String),
    /// `Stack ERROR`
    Stack,
    /// `Argument ERROR`
    Arg(String),
    /// `Go ERROR` — a `Goto` with no matching `Lbl`.
    Go(u8),
    /// `Nesting ERROR` — control structures nested too deeply / mismatched.
    Nesting(String),
    /// `Memory ERROR`
    Memory(String),
    /// `Data Full`
    DataFull,
}

impl CalcError {
    pub fn syntax(message: impl Into<String>, pos: usize) -> Self {
        CalcError::Syntax {
            message: message.into(),
            pos: Some(pos),
        }
    }

    pub fn syntax_here(message: impl Into<String>) -> Self {
        CalcError::Syntax {
            message: message.into(),
            pos: None,
        }
    }

    /// The error label the calculator would show on its lower line.
    pub fn label(&self) -> &'static str {
        match self {
            CalcError::Syntax { .. } => "Syntax ERROR",
            CalcError::Math(_) => "Math ERROR",
            CalcError::Stack => "Stack ERROR",
            CalcError::Arg(_) => "Argument ERROR",
            CalcError::Go(_) => "Go ERROR",
            CalcError::Nesting(_) => "Nesting ERROR",
            CalcError::Memory(_) => "Memory ERROR",
            CalcError::DataFull => "Data Full",
        }
    }
}

impl fmt::Display for CalcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CalcError::Syntax { message, pos } => match pos {
                Some(p) => write!(f, "Syntax ERROR at byte {p}: {message}"),
                None => write!(f, "Syntax ERROR: {message}"),
            },
            CalcError::Math(m) => write!(f, "Math ERROR: {m}"),
            CalcError::Stack => write!(f, "Stack ERROR"),
            CalcError::Arg(m) => write!(f, "Argument ERROR: {m}"),
            CalcError::Go(n) => write!(f, "Go ERROR: label {n} not found"),
            CalcError::Nesting(m) => write!(f, "Nesting ERROR: {m}"),
            CalcError::Memory(m) => write!(f, "Memory ERROR: {m}"),
            CalcError::DataFull => write!(f, "Data Full"),
        }
    }
}

impl std::error::Error for CalcError {}

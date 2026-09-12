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
    /// `Data Full`
    DataFull,
    /// A construct used in a mode that does not offer it.
    ///
    /// The hardware cannot produce this: it prevents the situation by not
    /// offering the key in the first place.  When running source, however, the
    /// mistake is worth reporting, so it gets its own diagnostic.
    Mode {
        message: String,
        mode: crate::mode::Mode,
        pos: Option<usize>,
    },
}

impl CalcError {
    pub(crate) fn syntax(message: impl Into<String>, pos: usize) -> Self {
        CalcError::Syntax {
            message: message.into(),
            pos: Some(pos),
        }
    }

    pub(crate) fn syntax_here(message: impl Into<String>) -> Self {
        CalcError::Syntax {
            message: message.into(),
            pos: None,
        }
    }

    /// A construct that the declared mode does not offer.
    pub(crate) fn mode(
        mode: crate::mode::Mode,
        message: impl Into<String>,
        pos: Option<usize>,
    ) -> Self {
        CalcError::Mode {
            message: message.into(),
            mode,
            pos,
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
            CalcError::DataFull => "Data Full",
            CalcError::Mode { .. } => "Mode ERROR",
        }
    }

    /// The byte offset this error points at, when it has one.
    pub fn pos(&self) -> Option<usize> {
        match self {
            CalcError::Syntax { pos, .. } | CalcError::Mode { pos, .. } => *pos,
            _ => None,
        }
    }

    /// The explanation without the leading error label, for use as a
    /// diagnostic note under a title that already carries the label.
    pub fn detail(&self) -> String {
        match self {
            CalcError::Syntax { message, pos } => match pos {
                Some(p) => format!("at byte {p}: {message}"),
                None => message.clone(),
            },
            CalcError::Math(m) => m.clone(),
            CalcError::Stack => "the numeric or command stack overflowed".to_string(),
            CalcError::Arg(m) => m.clone(),
            CalcError::Go(n) => format!("label {n} not found"),
            CalcError::Nesting(m) => m.clone(),
            CalcError::DataFull => "too many statistical data points".to_string(),
            CalcError::Mode { message, .. } => message.clone(),
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
            CalcError::DataFull => write!(f, "Data Full"),
            CalcError::Mode { message, mode, .. } => {
                write!(f, "Mode ERROR: {message} (mode {mode})")
            }
        }
    }
}

impl std::error::Error for CalcError {}

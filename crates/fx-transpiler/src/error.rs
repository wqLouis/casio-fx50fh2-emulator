//! Errors produced while transpiling `.fxc` source into PRGM.

use std::fmt;

/// A transpile error carrying the message plus both the byte offset and the
/// 1-based line/column of the offending source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranspileError {
    /// Human-readable description of what went wrong.
    pub message: String,
    /// Byte offset into the `.fxc` source.
    pub offset: usize,
    /// 1-based line number.
    pub line: usize,
    /// 1-based column number, counted in `char`s.
    pub column: usize,
}

impl TranspileError {
    /// Build an error from an explicit position.
    pub fn new(message: impl Into<String>, offset: usize, line: usize, column: usize) -> Self {
        TranspileError {
            message: message.into(),
            offset,
            line,
            column,
        }
    }

    /// Build an error, deriving line/column from `source` and `offset`.
    pub fn at(source: &str, message: impl Into<String>, offset: usize) -> Self {
        let (line, column) = line_col(source, offset);
        TranspileError::new(message, offset, line, column)
    }
}

impl fmt::Display for TranspileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} (line {}, column {})",
            self.message, self.line, self.column
        )
    }
}

impl std::error::Error for TranspileError {}

/// Map a byte offset to a 1-based `(line, column)` pair.
///
/// The column counts `char`s, not bytes, and offsets past the end of the
/// source clamp to the final position.
pub fn line_col(source: &str, offset: usize) -> (usize, usize) {
    let limit = offset.min(source.len());
    let mut line = 1usize;
    let mut column = 1usize;
    for (index, ch) in source.char_indices() {
        if index >= limit {
            break;
        }
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (line, column)
}

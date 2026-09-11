//! Errors produced while transpiling `.fxc` source into PRGM.

use std::fmt;
use std::path::Path;

/// A transpile error carrying the message plus both the byte offset and the
/// 1-based line/column of the offending source.
///
/// `file` is set when the error can be attributed to a specific file — an
/// included library, or the root file when transpiling from a path. It is
/// `None` for anonymous source, in which case `line`/`column` refer to that
/// text directly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranspileError {
    /// Human-readable description of what went wrong.
    pub message: String,
    /// Byte offset into the (include-expanded) `.fxc` source.
    pub offset: usize,
    /// 1-based line number, within [`TranspileError::file`] when that is set.
    pub line: usize,
    /// 1-based column number, counted in `char`s.
    pub column: usize,
    /// The file the position refers to, when known.
    pub file: Option<String>,
}

impl TranspileError {
    /// Build an error from an explicit position.
    pub fn new(message: impl Into<String>, offset: usize, line: usize, column: usize) -> Self {
        TranspileError {
            message: message.into(),
            offset,
            line,
            column,
            file: None,
        }
    }

    /// Build an error, deriving line/column from `source` and `offset`.
    pub fn at(source: &str, message: impl Into<String>, offset: usize) -> Self {
        let (line, column) = line_col(source, offset);
        TranspileError::new(message, offset, line, column)
    }

    /// Attribute the error to `file`, unless it already names one.
    pub fn in_file(mut self, file: Option<&Path>) -> Self {
        if self.file.is_none() {
            self.file = file.map(|p| p.display().to_string());
        }
        self
    }
}

impl fmt::Display for TranspileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.file {
            Some(file) => write!(
                f,
                "{} ({}:{}:{})",
                self.message, file, self.line, self.column
            ),
            None => write!(
                f,
                "{} (line {}, column {})",
                self.message, self.line, self.column
            ),
        }
    }
}

impl std::error::Error for TranspileError {}

/// The error for an array index that is still not a literal after folding and
/// unrolling.
///
/// PRGM has no indirect addressing, so an element is a fixed memory chosen
/// while transpiling; a run-time index cannot be expressed without an
/// `If`/`Else` chain over every element.
pub(crate) fn computed_index_error(source: &str, name: &str, pos: usize) -> TranspileError {
    TranspileError::at(
        source,
        format!(
            "`{name}[…]` needs a compile-time index: PRGM has no indirect addressing, so each \
             element is a fixed memory chosen while transpiling. Unroll the loop (write \
             `{name}[0]`, `{name}[1]`, …) so the index is a literal"
        ),
        pos,
    )
}

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

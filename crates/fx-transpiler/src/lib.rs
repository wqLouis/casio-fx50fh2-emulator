//! Transpiler from a small C-like language (`fxc`) to fx-50FH II PRGM source.
//!
//! The pipeline is [`lexer::lex`] → [`parser::parse`] → [`emit`] and is exposed
//! through [`transpile`] / [`transpile_with`]. Programs use ordinary names such
//! as `total` or `i`; the emitter assigns them to the calculator's seven
//! memories (`A B C D X Y M`) in first-seen order.
//!
//! ```
//! # use fx_transpiler::transpile;
//! let prgm = transpile("let a = input(); let b = a * 2; print(b);").unwrap();
//! assert_eq!(prgm, "?→A\nA×2→B\nB◢\n");
//! ```
//!
//! With [`Options::ascii`] the calculator glyphs are replaced by their ASCII
//! spellings (`→` becomes `->`, `◢` becomes `disp`, `≠` becomes `<>`, …):
//!
//! ```
//! # use fx_transpiler::{transpile_with, Options};
//! let prgm = transpile_with("print(a <= b);", Options { ascii: true }).unwrap();
//! assert_eq!(prgm, "A<=Bdisp\n");
//! ```

pub mod ast;
pub mod builtins;
pub mod error;
pub mod lexer;
pub mod parser;

mod alloc;
mod emit;

use error::TranspileError;

/// Output-style knobs for [`transpile_with`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Options {
    /// Emit ASCII aliases (`->`, `disp`, `<>`, `<=`, `>=`, `*`, `/`, `pi`)
    /// instead of the calculator's unicode glyphs.
    pub ascii: bool,
}

/// Transpile `.fxc` `source` into PRGM using the default (glyph) output.
pub fn transpile(source: &str) -> Result<String, TranspileError> {
    transpile_with(source, Options::default())
}

/// Transpile `.fxc` `source` into PRGM with explicit [`Options`].
pub fn transpile_with(source: &str, opts: Options) -> Result<String, TranspileError> {
    let tokens = lexer::lex(source)?;
    let program = parser::parse(&tokens, source)?;
    emit::emit(&program, source, opts)
}

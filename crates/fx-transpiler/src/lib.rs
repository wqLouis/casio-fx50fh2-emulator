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
//! let prgm = transpile_with("print(a <= b);", Options { ascii: true, ..Default::default() }).unwrap();
//! assert_eq!(prgm, "A<=Bdisp\n");
//! ```

pub mod ast;
pub mod builtins;
pub mod error;
pub mod lexer;
pub mod mode;
pub mod parser;
#[cfg(feature = "testing")]
pub mod testing;

mod alloc;
mod emit;
mod validate;

pub use mode::Mode;

use error::TranspileError;

/// Output-style knobs for [`transpile_with`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Options {
    /// Emit ASCII aliases (`->`, `disp`, `<>`, `<=`, `>=`, `*`, `/`, `pi`)
    /// instead of the calculator's unicode glyphs.
    pub ascii: bool,
    /// Force an operating mode, overriding any `#mode` header in the source
    /// (the transpiler's equivalent of the CLI's `--mode` flag). `None` means
    /// "use the header, or [`Mode::Comp`] when there is no header either".
    pub mode: Option<Mode>,
}

/// Transpile `.fxc` `source` into PRGM using the default (glyph) output.
pub fn transpile(source: &str) -> Result<String, TranspileError> {
    transpile_with(source, Options::default())
}

/// Transpile `.fxc` `source` into PRGM with explicit [`Options`].
///
/// A leading `#mode NAME` header selects the effective operating mode unless
/// [`Options::mode`] overrides it. The effective mode is re-emitted as the
/// first line whenever it came from the header or the override (never for the
/// implicit COMP default), and the program is validated against its
/// capabilities (currently only BASE rejects anything).
pub fn transpile_with(source: &str, opts: Options) -> Result<String, TranspileError> {
    let tokens = lexer::lex(source)?;
    let header = tokens.iter().find_map(|token| match &token.tok {
        lexer::Tok::Mode(mode) => Some(*mode),
        _ => None,
    });
    let effective = opts.mode.or(header).unwrap_or(Mode::Comp);

    let program = parser::parse(&tokens, source)?;
    validate::validate(&program, effective, source)?;
    let body = emit::emit(&program, source, opts)?;

    if opts.mode.is_some() || header.is_some() {
        Ok(format!("#mode {}\n{body}", effective.name()))
    } else {
        Ok(body)
    }
}

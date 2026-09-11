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
pub mod constants;
pub mod data;
pub mod error;
pub mod include;
pub mod json;
pub mod lexer;
pub mod mode;
pub mod parser;
#[cfg(feature = "testing")]
pub mod testing;

mod alloc;
mod emit;
mod fold;
mod unroll;
mod validate;

pub use alloc::Allocation;
pub use data::{Data, DataTable};
pub use mode::Mode;

use std::path::Path;

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
///
/// `#include` directives resolve relative to the current directory; use
/// [`transpile_file`] or [`transpile_with_base`] to resolve them relative to a
/// specific location instead.
pub fn transpile_with(source: &str, opts: Options) -> Result<String, TranspileError> {
    transpile_with_base(source, opts, Path::new("."))
}

/// Transpile a `.fxc` file, resolving `#include` relative to that file.
pub fn transpile_file(path: &Path, opts: Options) -> Result<String, TranspileError> {
    let source = std::fs::read_to_string(path).map_err(|e| {
        TranspileError::new(format!("cannot read `{}`: {e}", path.display()), 0, 1, 1)
            .in_file(Some(path))
    })?;
    transpile_named(
        &source,
        opts,
        Some(path),
        path.parent().unwrap_or(Path::new(".")),
    )
}

/// Transpile `source`, resolving `#include` relative to `base_dir`.
///
/// This is the entry point for callers that hold the text rather than a path
/// (an editor buffer, an inline test suite) but still know where relative
/// includes should be looked up.
pub fn transpile_with_base(
    source: &str,
    opts: Options,
    base_dir: &Path,
) -> Result<String, TranspileError> {
    transpile_named(source, opts, None, base_dir)
}

/// The shared implementation behind the public entry points.
fn transpile_named(
    source: &str,
    opts: Options,
    root: Option<&Path>,
    base_dir: &Path,
) -> Result<String, TranspileError> {
    let expanded = include::expand(source, root, base_dir)?;
    match transpile_expanded(&expanded, opts, base_dir) {
        Ok(prgm) => Ok(prgm),
        // Positions refer to the expanded text, so translate them back to the
        // file and line the user actually wrote.
        Err(error) => Err(attribute(error, &expanded)),
    }
}

/// Point an error at the file and line it came from after include expansion.
fn attribute(error: TranspileError, expanded: &include::Expanded) -> TranspileError {
    if error.file.is_some() {
        return error;
    }
    let (file, line) = expanded.origin(error.line);
    match file {
        Some(path) => TranspileError {
            file: Some(path.display().to_string()),
            line,
            ..error
        },
        // Anonymous source: leave the position as it is.
        None => error,
    }
}

/// Transpile include-expanded, data-extracted source.
fn transpile_expanded(
    expanded: &include::Expanded,
    opts: Options,
    base_dir: &Path,
) -> Result<String, TranspileError> {
    let (text, data) = data::extract(expanded, base_dir)?;
    let tokens = lexer::lex(&text)?;
    let header = tokens.iter().find_map(|token| match &token.tok {
        lexer::Tok::Mode(mode) => Some(*mode),
        _ => None,
    });
    let effective = opts.mode.or(header).unwrap_or(Mode::Comp);

    let mut program = parser::parse(&tokens, &text)?;
    // Pre-calculate constant expressions and unroll the loops that need it, so
    // the emitter sees plain literal array indices.
    fold::fold_program(&mut program);
    unroll::unroll_program(&mut program);
    validate::validate(&program, effective, &text)?;
    let body = emit::emit(&program, &text, opts, &data)?;

    if opts.mode.is_some() || header.is_some() {
        Ok(format!("#mode {}\n{body}", effective.name()))
    } else {
        Ok(body)
    }
}

/// What `fx50 regs` reports: how a program uses the calculator's memories.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Analysis {
    /// Variables and the memory each uses, plus each memory's occupants over
    /// time (a memory with more than one was released with `free` and re-used).
    pub allocation: Allocation,
    /// `const` names. These are inlined and use no memory.
    pub consts: Vec<String>,
    /// `#data` table names. These are compile-time numbers and use no memory.
    pub data: Vec<String>,
}

/// Analyse how a program would use the calculator's memories.
///
/// This runs the same front end as [`transpile_with_base`] — includes, data
/// extraction, lexing, parsing — and then stops short of emitting, so the
/// memory plan can be inspected (and failures reported) without a program.
/// `base_dir` resolves relative `#include` paths.
pub fn analyze(source: &str, base_dir: &Path) -> Result<Analysis, TranspileError> {
    let expanded = include::expand(source, None, base_dir)?;
    match analyze_expanded(&expanded, base_dir) {
        Ok(analysis) => Ok(analysis),
        Err(error) => Err(attribute(error, &expanded)),
    }
}

fn analyze_expanded(
    expanded: &include::Expanded,
    base_dir: &Path,
) -> Result<Analysis, TranspileError> {
    let (text, data) = data::extract(expanded, base_dir)?;
    let tokens = lexer::lex(&text)?;
    let mut program = parser::parse(&tokens, &text)?;
    // Mirror the transpiler front end, so `fx50 regs` and `fx50 build` never
    // disagree about the memory plan.
    fold::fold_program(&mut program);
    unroll::unroll_program(&mut program);
    let allocator = alloc::Allocator::collect(&program, &text, &data)?;
    Ok(Analysis {
        allocation: allocator.allocation(),
        consts: alloc::const_names(&program),
        data: data.names().map(str::to_string).collect(),
    })
}

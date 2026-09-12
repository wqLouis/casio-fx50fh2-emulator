//! A clean-room interpreter for the CASIO fx-50FH II programmable calculator.
//!
//! ```
//! use casio_fx50fh2::{compile, Interpreter, MockHost};
//!
//! let program = compile("?→A: A×2◢").unwrap();
//! let mut interp = Interpreter::new(program, MockHost::with_inputs([21.0]));
//! interp.run().unwrap();
//! assert_eq!(interp.host().output, vec!["42"]);
//! ```

pub mod ast;
pub mod bases;
pub mod check;
pub mod constants;
pub(crate) mod error;
pub mod format;
pub mod lexer;
pub(crate) mod mode;
pub(crate) mod parser;
pub mod precision;
pub(crate) mod runtime;
pub mod stats;
pub mod token;
pub mod value;

pub use constants::CONSTANTS;
pub use error::CalcError;
pub use mode::Mode;
pub use runtime::{AngleMode, DisplayMode, Environment, Host, Interpreter, MockHost};
pub use value::Value;

/// Lex, parse and mode-check a program into a flat list of statements.
///
/// A program may begin with a `#mode COMP|CMPLX|BASE|SD|REG` directive;
/// without one it runs in COMP.  Any construct the declared mode does not
/// offer is reported as [`CalcError::Mode`].
///
/// Use [`compile_with`] to supply the mode from outside instead of a header.
pub fn compile(source: &str) -> Result<Vec<ast::Stmt>, CalcError> {
    compile_with(source, None)
}

/// Like [`compile`], but an explicit `mode` overrides any `#mode` header.
///
/// This is what the CLI's `--mode` flag uses, and it is how a caller runs a
/// single CMPLX expression that has no header at all.
pub fn compile_with(source: &str, mode: Option<Mode>) -> Result<Vec<ast::Stmt>, CalcError> {
    let tokens = lexer::lex(source)?;
    let mut program = parser::parse(tokens)?;
    if let Some(mode) = mode {
        match program
            .iter_mut()
            .find(|stmt| matches!(stmt, ast::Stmt::Mode(_)))
        {
            Some(slot) => *slot = ast::Stmt::Mode(mode),
            None => program.insert(0, ast::Stmt::Mode(mode)),
        }
    }
    check::check(&program)?;
    Ok(program)
}

/// Run a program with the given host and return the interpreter (so callers can
/// inspect memory or the host afterwards).
fn run<H: Host>(source: &str, host: H) -> Result<Interpreter<H>, CalcError> {
    let program = compile(source)?;
    let mut interp = Interpreter::new(program, host);
    interp.run()?;
    Ok(interp)
}

/// Evaluate a single expression and return its real part.
///
/// Use [`evaluate_value`] when the result may be complex.
pub fn evaluate(source: &str) -> Result<f64, CalcError> {
    let interp = run(source, MockHost::default())?;
    Ok(interp.environment().ans())
}

/// Evaluate a single expression and return its full (possibly complex) value.
pub fn evaluate_value(source: &str) -> Result<Value, CalcError> {
    let interp = run(source, MockHost::default())?;
    Ok(interp.environment().ans_value())
}

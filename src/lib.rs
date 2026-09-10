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
pub mod error;
pub mod format;
pub mod lexer;
pub mod parser;
pub mod precision;
pub mod runtime;
pub mod stats;
pub mod token;
pub mod value;

pub use error::CalcError;
pub use runtime::{AngleMode, DisplayMode, Environment, Host, Interpreter, MockHost};
pub use stats::{RegType, StatVar, Stats};
pub use value::{ComplexFormat, Value};

/// Lex and parse a program into a flat list of statements.
pub fn compile(source: &str) -> Result<Vec<ast::Stmt>, CalcError> {
    let tokens = lexer::lex(source)?;
    parser::parse(tokens)
}

/// Run a program with the given host and return the interpreter (so callers can
/// inspect memory or the host afterwards).
pub fn run<H: Host>(source: &str, host: H) -> Result<Interpreter<H>, CalcError> {
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

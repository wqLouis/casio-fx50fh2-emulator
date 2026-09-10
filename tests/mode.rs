//! Tests for the `#mode` directive and mode-aware checking.
//!
//! The fx-50FH II only offers complex numbers in CMPLX, statistics in SD/REG,
//! and base-n in BASE. A program declares its mode in a header, exactly as the
//! hardware makes you pick a mode before the relevant keys appear.

use casio_fx50fh2::check::declared_mode;
use casio_fx50fh2::{CalcError, Interpreter, MockHost, Mode, compile, compile_with};

fn run(source: &str) -> Result<Interpreter<MockHost>, CalcError> {
    let program = compile(source)?;
    let mut interp = Interpreter::new(program, MockHost::default());
    interp.run()?;
    Ok(interp)
}

fn mode_error(source: &str) -> CalcError {
    compile(source).expect_err("expected a mode error")
}

// -- parsing the directive --------------------------------------------------

#[test]
fn directive_is_parsed_case_insensitively() {
    assert_eq!(declared_mode(&compile("#mode CMPLX").unwrap()), Mode::Cmplx);
    assert_eq!(declared_mode(&compile("#mode cmplx").unwrap()), Mode::Cmplx);
    assert_eq!(declared_mode(&compile("#mode=Reg").unwrap()), Mode::Reg);
    assert_eq!(declared_mode(&compile("#mode STAT").unwrap()), Mode::Sd);
    assert_eq!(declared_mode(&compile("#mode COMP").unwrap()), Mode::Comp);
}

#[test]
fn no_directive_defaults_to_comp() {
    assert_eq!(declared_mode(&compile("2+2").unwrap()), Mode::Comp);
}

#[test]
fn unknown_mode_name_is_a_syntax_error() {
    let err = compile("#mode FANCY: 1+1").unwrap_err();
    assert!(matches!(err, CalcError::Syntax { .. }), "{err:?}");
    assert!(err.to_string().contains("unknown mode"), "{err}");
}

#[test]
fn directive_must_be_first() {
    let err = compile("1+1: #mode CMPLX").unwrap_err();
    assert!(
        matches!(err, CalcError::Syntax { .. }),
        "expected a syntax error, got {err:?}"
    );
    assert!(err.to_string().contains("first statement"), "{err}");
}

#[test]
fn a_leading_comment_does_not_displace_the_directive() {
    assert_eq!(
        declared_mode(&compile("// a comment\n#mode CMPLX\n1+i").unwrap()),
        Mode::Cmplx
    );
}

// -- COMP is strict ---------------------------------------------------------

#[test]
fn comp_mode_rejects_complex_statistics_and_base() {
    assert!(matches!(mode_error("3+4i"), CalcError::Mode { .. }));
    assert!(matches!(mode_error("Conjg(1+i)"), CalcError::Mode { .. }));
    assert!(matches!(mode_error("1 DT: n"), CalcError::Mode { .. }));
    assert!(matches!(mode_error("Hex: 255"), CalcError::Mode { .. }));
    assert!(matches!(mode_error("1Fh"), CalcError::Mode { .. }));
}

/// Outside CMPLX the machine can never produce a complex value: `√(-4)` is a
/// `Math ERROR`, not `2i`.
#[test]
fn square_root_of_negative_is_an_error_in_comp() {
    let err = run("√(-4)").unwrap_err();
    assert!(matches!(err, CalcError::Mode { .. }), "{err:?}");
    // The same expression is fine once CMPLX is declared.
    assert!(run("#mode CMPLX\n√(-4)").is_ok());
}

#[test]
fn plain_arithmetic_still_works_in_comp() {
    assert_eq!(run("2+3×4").unwrap().environment().ans(), 14.0);
    assert_eq!(run("sin(30)").unwrap().environment().ans(), 0.5);
}

// -- each mode enables its own features -------------------------------------

#[test]
fn cmplx_mode_enables_complex_but_not_statistics() {
    assert!(run("#mode CMPLX\n(3+4i)×(1-2i)").is_ok());
    assert!(matches!(
        mode_error("#mode CMPLX\n1 DT: n"),
        CalcError::Mode { .. }
    ));
}

#[test]
fn sd_mode_enables_single_variable_statistics() {
    assert!(run("#mode SD\nClrStat: 1 DT: 2 DT: n").is_ok());
    // Regression variables belong to REG.
    assert!(matches!(
        mode_error("#mode SD\n1,2 DT: rega"),
        CalcError::Mode { .. }
    ));
    // Complex still needs CMPLX.
    assert!(matches!(mode_error("#mode SD\ni"), CalcError::Mode { .. }));
}

#[test]
fn reg_mode_enables_regression() {
    assert!(run("#mode REG\nClrStat: 1,3 DT: 2,5 DT: rega").is_ok());
}

#[test]
fn base_mode_enables_base_n() {
    assert!(run("#mode BASE\nHex: FFh").is_ok());
    // Fractions are not offered in BASE.
    assert!(matches!(
        mode_error("#mode BASE\n1┘2"),
        CalcError::Mode { .. }
    ));
}

// -- an explicit mode overrides the header ----------------------------------

#[test]
fn compile_with_overrides_the_header() {
    // Header says COMP, caller forces CMPLX.
    let program = compile_with("3+4i", Some(Mode::Cmplx)).unwrap();
    assert_eq!(declared_mode(&program), Mode::Cmplx);

    // Caller forces COMP for a program with no header.
    let err = compile_with("i", Some(Mode::Comp)).unwrap_err();
    assert!(matches!(err, CalcError::Mode { .. }), "{err:?}");
}

// -- the directive itself is silent -----------------------------------------

/// A program that only sets up state has no value to show, so a lone
/// directive must not print `0` (which it did while the REPL inherited
/// modes).
#[test]
fn a_lone_directive_produces_no_output() {
    let interp = run("#mode CMPLX").unwrap();
    assert!(
        interp.host().output.is_empty(),
        "{:?}",
        interp.host().output
    );
    assert_eq!(interp.environment().mode, Mode::Cmplx);
}

/// ...but a directive followed by an expression still shows the result.
#[test]
fn a_directive_does_not_suppress_a_later_result() {
    let interp = run("#mode CMPLX\n3+4i◢").unwrap();
    assert_eq!(interp.host().output, vec!["3+4𝑖"]);
}

//! End-to-end tests for base-n mode.

use casio_fx50fh2::bases::Base;
use casio_fx50fh2::{CalcError, Interpreter, MockHost, compile, evaluate};

/// Base-n work must be declared, just as the hardware requires you to select
/// the BASE mode before the base keys become available.
const MODE: &str = "#mode BASE\n";

fn run(source: &str) -> Result<Interpreter<MockHost>, CalcError> {
    let program = compile(&format!("{MODE}{source}"))?;
    let mut interp = Interpreter::new(program, MockHost::default());
    interp.run()?;
    Ok(interp)
}

fn output(source: &str) -> Vec<String> {
    run(source).unwrap().into_host().output
}

fn eval(source: &str) -> f64 {
    evaluate(&format!("{MODE}{source}")).unwrap()
}

#[test]
fn setup_commands_select_the_base() {
    assert_eq!(run("Hex").unwrap().environment().base, Some(Base::Hex));
    assert_eq!(run("Bin").unwrap().environment().base, Some(Base::Bin));
    assert_eq!(run("Oct").unwrap().environment().base, Some(Base::Oct));
    assert_eq!(run("Dec").unwrap().environment().base, Some(Base::Dec));
    assert_eq!(run("1+1").unwrap().environment().base, None);
}

#[test]
fn tagged_literals_convert_to_decimal() {
    assert_eq!(eval("1Fh"), 31.0);
    assert_eq!(eval("1010b"), 10.0);
    assert_eq!(eval("17o"), 15.0);
    assert_eq!(eval("42d"), 42.0);
}

#[test]
fn results_display_with_the_base_suffix() {
    assert_eq!(output("Hex: 1Fh◢"), vec!["1Fh"]);
    assert_eq!(output("Bin: 1010b◢"), vec!["1010b"]);
    assert_eq!(output("Oct: 17o◢"), vec!["17o"]);
    assert_eq!(output("Dec: 255◢"), vec!["255d"]);
}

#[test]
fn arithmetic_wraps_to_the_word_size() {
    // 10-bit binary: 1023 + 1 wraps back to zero.
    assert_eq!(output("Bin: 1111111111b+1◢"), vec!["0b"]);
    // 32-bit decimal: -1 + 1 = 0.
    assert_eq!(output("Dec: 0-1◢"), vec!["-1d"]);
    assert_eq!(output("Dec: 0-1+1◢"), vec!["0d"]);
}

#[test]
fn integer_division_truncates() {
    // BASE mode offers `÷`, not the fraction key `┘`.
    assert_eq!(output("Dec: 7÷2◢"), vec!["3d"]);
    assert_eq!(output("Dec: 0-7÷2◢"), vec!["-3d"]);
}

/// The fraction key is not offered in BASE mode, so using it is a mode error.
#[test]
fn fraction_key_is_not_available_in_base_mode() {
    let err = run("Dec: 7┘2").unwrap_err();
    assert!(matches!(err, CalcError::Mode { .. }), "{err:?}");
}

#[test]
fn bitwise_operators_in_base_mode() {
    assert_eq!(output("Bin: 1010b and 1100b◢"), vec!["1000b"]);
    assert_eq!(output("Bin: 1010b or 1100b◢"), vec!["1110b"]);
    assert_eq!(output("Bin: 1010b xor 1100b◢"), vec!["110b"]);
    assert_eq!(output("Bin: 1010b xnor 1010b◢"), vec!["1111111111b"]);
}

#[test]
fn bitwise_operators_need_a_base() {
    let err = run("1 and 1").unwrap_err();
    assert!(matches!(err, CalcError::Math(_)), "{err:?}");
}

#[test]
fn not_and_neg_helpers() {
    assert_eq!(output("Dec: Not(0)◢"), vec!["-1d"]);
    assert_eq!(output("Dec: Neg(1)◢"), vec!["-1d"]);
    let err = run("Not(0)").unwrap_err();
    assert!(matches!(err, CalcError::Math(_)), "{err:?}");
}

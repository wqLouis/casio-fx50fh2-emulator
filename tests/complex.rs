//! End-to-end tests for complex-number support.

use casio_fx50fh2::value::ComplexFormat;
use casio_fx50fh2::{CalcError, Interpreter, MockHost, Value, compile, evaluate_value};

fn run(source: &str) -> Result<Interpreter<MockHost>, CalcError> {
    let program = compile(source)?;
    let mut interp = Interpreter::new(program, MockHost::default());
    interp.run()?;
    Ok(interp)
}

fn output(source: &str) -> Vec<String> {
    run(source).unwrap().into_host().output
}

fn value(source: &str) -> Value {
    evaluate_value(source).unwrap()
}

#[test]
fn imaginary_unit_is_complex() {
    assert_eq!(value("i"), Value::Complex(0.0, 1.0));
    assert_eq!(value("i^2"), Value::Complex(-1.0, 0.0));
    assert_eq!(value("i^3"), Value::Complex(0.0, -1.0));
}

#[test]
fn complex_arithmetic() {
    assert_eq!(value("(2+3i)+(4-5i)"), Value::Complex(6.0, -2.0));
    assert_eq!(value("(1+2i)(3+4i)"), Value::Complex(-5.0, 10.0));
    assert_eq!(value("(1+i)┘(1-i)"), Value::Complex(0.0, 1.0));
    assert_eq!(value("(3+4i)^2"), Value::Complex(-7.0, 24.0));
    assert_eq!(value("(3+4i)^-1"), Value::Complex(0.12, -0.16));
}

#[test]
fn square_root_of_negative_is_imaginary() {
    assert_eq!(value("√(-4)"), Value::Complex(0.0, 2.0));
}

#[test]
fn abs_arg_and_conjg() {
    assert_eq!(value("Abs(3+4i)"), Value::Real(5.0));
    assert_eq!(value("arg(i)"), Value::Real(90.0));
    assert_eq!(value("Conjg(2+3i)"), Value::Complex(2.0, -3.0));
    assert_eq!(value("Conjg(7)"), Value::Real(7.0));
}

#[test]
fn polar_literal_follows_angle_mode() {
    let v = value("2∠30");
    assert!((v.re() - 3.0f64.sqrt()).abs() < 1e-12, "{v:?}");
    assert!((v.im() - 1.0).abs() < 1e-12, "{v:?}");

    let v = value("Rad: 1∠pi┘2");
    assert!(v.re().abs() < 1e-12, "{v:?}");
    assert!((v.im() - 1.0).abs() < 1e-12, "{v:?}");
}

#[test]
fn comparisons_equality_only() {
    assert_eq!(value("(1+i)=(1+i)"), Value::Real(1.0));
    assert_eq!(value("(1+i)≠(1+2i)"), Value::Real(1.0));
    let err = evaluate_value("(1+i)<(2+2i)").unwrap_err();
    assert!(matches!(err, CalcError::Math(_)), "{err:?}");
}

#[test]
fn real_only_functions_reject_complex() {
    let err = evaluate_value("sin(i)").unwrap_err();
    assert!(matches!(err, CalcError::Math(_)), "{err:?}");
    let err = evaluate_value("(3+4i)!").unwrap_err();
    assert!(matches!(err, CalcError::Math(_)), "{err:?}");
}

#[test]
fn cartesian_display_format() {
    assert_eq!(output("2+3i◢"), vec!["2+3𝑖"]);
    assert_eq!(output("2-3i◢"), vec!["2-3𝑖"]);
    assert_eq!(output("3i◢"), vec!["3𝑖"]);
}

#[test]
fn polar_display_format_command() {
    // `▶r∠θ` or the ASCII alias `>rangle` switches the format.
    let polar = output("1+i: ▶r∠θ: Ans◢");
    assert_eq!(polar, vec!["1.414213562∠45"]);
    let ascii = output("1+i: >rangle: Ans◢");
    assert_eq!(ascii, vec!["1.414213562∠45"]);
}

#[test]
fn cartesian_format_command_round_trips() {
    let out = output("▶r∠θ: 2∠45: ▶a+b𝑖: Ans◢");
    assert_eq!(out, vec!["1.414213562+1.414213562𝑖"]);
}

#[test]
fn environment_complex_format_is_public() {
    let interp = run("▶r∠θ").unwrap();
    assert_eq!(interp.environment().complex_format, ComplexFormat::Polar);
}

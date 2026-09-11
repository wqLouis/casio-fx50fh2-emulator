//! End-to-end tests for sexagesimal (degrees/minutes/seconds) values.
//!
//! The manual's 度分秒（六十进制）计算 section describes the `°′″` key.  It both
//! *enters* a sexagesimal literal (`2°15′18″`) and *converts* a displayed
//! value between the decimal and sexagesimal forms (`2.255` → `2°15′18″`).
//! As on the machine, the value is a single `f64`
//! (`deg + min/60 + sec/3600`); only the display differs.

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

fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-12,
        "expected {expected}, got {actual}"
    );
}

#[test]
fn literal_is_reduced_to_decimal() {
    close(value("2°15′18″").re(), 2.255);
    // Degrees and minutes must be entered even when they are zero.
    close(value("0°0′30″").re(), 30.0 / 3600.0);
}

#[test]
fn literal_is_marked_sexagesimal() {
    assert!(value("2°15′18″").is_sexagesimal());
    // An ordinary decimal is not.
    assert!(!value("2.255").is_sexagesimal());
}

#[test]
fn literal_displays_as_degrees_minutes_seconds() {
    assert_eq!(output("2°15′18″◢"), vec!["2°15′18″"]);
    assert_eq!(output("0°0′30″◢"), vec!["0°0′30″"]);
}

#[test]
fn conversion_key_toggles_display() {
    // `2.255` → `2°15′18″`.
    assert_eq!(output("2.255: °′″: Ans◢"), vec!["2°15′18″"]);
    // ...and back again.
    assert_eq!(output("2°15′18″: °′″: Ans◢"), vec!["2.255"]);
}

#[test]
fn ascii_alias_of_the_conversion_key() {
    assert_eq!(output("2.255: dms: Ans◢"), vec!["2°15′18″"]);
}

#[test]
fn comparison_ignores_the_display_flag() {
    // `2` and `2°0′0″` are the same number, so equality is true either way.
    assert_eq!(output("2°0′0″ = 2◢"), vec!["1"]);
    assert_eq!(output("2°0′0″ ≠ 3◢"), vec!["1"]);
    assert_eq!(output("2°0′0″ < 3◢"), vec!["1"]);
}

#[test]
fn adding_two_sexagesimal_values_stays_sexagesimal() {
    assert_eq!(output("1°30′0″ + 1°30′0″◢"), vec!["3°0′0″"]);
    assert_eq!(output("1°30′0″ - 0°30′0″◢"), vec!["1°0′0″"]);
}

#[test]
fn multiplying_or_dividing_by_a_decimal_stays_sexagesimal() {
    assert_eq!(output("2°0′0″ × 2◢"), vec!["4°0′0″"]);
    assert_eq!(output("2°0′0″ ÷ 2◢"), vec!["1°0′0″"]);
}

#[test]
fn a_decimal_expression_stays_decimal() {
    assert_eq!(output("1.5 + 1.5◢"), vec!["3"]);
}

#[test]
fn degrees_and_minutes_are_required() {
    assert!(compile("2°18″").is_err());
    assert!(compile("2°").is_err());
}

#[test]
fn bare_conversion_with_no_value_produces_no_output() {
    // The key only converts a displayed value; with nothing to show it is
    // silent.
    let interp = run("°′″").unwrap();
    assert!(
        interp.host().output.is_empty(),
        "{:?}",
        interp.host().output
    );
}

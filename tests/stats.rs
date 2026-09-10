//! End-to-end tests for statistics (`DT`, S-SUM and S-VAR).

use casio_fx50fh2::{CalcError, Interpreter, MockHost, compile, evaluate};

fn run(source: &str) -> Result<Interpreter<MockHost>, CalcError> {
    let program = compile(source)?;
    let mut interp = Interpreter::new(program, MockHost::default());
    interp.run()?;
    Ok(interp)
}

fn output(source: &str) -> Vec<String> {
    run(source).unwrap().into_host().output
}

fn value(source: &str) -> f64 {
    evaluate(source).unwrap()
}

fn assert_close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-9,
        "expected {expected}, got {actual}"
    );
}

#[test]
fn single_variable_data_entry() {
    let source = "ClrStat: 1 DT: 2 DT: 3 DT: n◢";
    assert_eq!(output(source), vec!["3"]);
    assert_eq!(value("ClrStat: 1 DT: 2 DT: 3 DT: sumx"), 6.0);
    assert_eq!(value("ClrStat: 1 DT: 2 DT: 3 DT: sumx2"), 14.0);
    assert_eq!(value("ClrStat: 1 DT: 2 DT: 3 DT: meanx"), 2.0);
    assert_eq!(value("ClrStat: 1 DT: 2 DT: 3 DT: minx"), 1.0);
    assert_eq!(value("ClrStat: 1 DT: 2 DT: 3 DT: maxx"), 3.0);
}

#[test]
fn glyph_stat_variables_lex() {
    assert_eq!(value("ClrStat: 1 DT: 2 DT: 3 DT: Σx"), 6.0);
    assert_eq!(value("ClrStat: 1 DT: 2 DT: 3 DT: x̄"), 2.0);
    assert_close(
        value("ClrStat: 1 DT: 2 DT: 3 DT: σx"),
        (2.0f64 / 3.0).sqrt(),
    );
    assert_close(value("ClrStat: 1 DT: 2 DT: 3 DT: sx"), 1.0);
}

#[test]
fn frequency_mode() {
    assert_eq!(value("ClrStat: FreqOn: 1;2 DT: 3;1 DT: n"), 3.0);
    assert_eq!(value("ClrStat: FreqOn: 1;2 DT: 3;1 DT: sumx"), 5.0);
    // FreqOff ignores the stored frequency.
    assert_eq!(value("ClrStat: FreqOff: 1;2 DT: n"), 1.0);
}

#[test]
fn two_variable_regression() {
    // y = 2x + 1
    let setup = "ClrStat: 1,3 DT: 2,5 DT: 3,7 DT: ";
    assert_close(value(&format!("{setup}rega")), 1.0);
    assert_close(value(&format!("{setup}regb")), 2.0);
    assert_close(value(&format!("{setup}regr")), 1.0);
    assert_eq!(
        value(&format!("{setup}sumxy")),
        1.0 * 3.0 + 2.0 * 5.0 + 3.0 * 7.0
    );
}

#[test]
fn clrstat_clears_data() {
    assert_eq!(value("ClrStat: 1 DT: 2 DT: ClrStat: n"), 0.0);
}

#[test]
fn complex_data_is_rejected() {
    let err = run("1+i DT").unwrap_err();
    assert!(matches!(err, CalcError::Math(_)), "{err:?}");
}

#[test]
fn statistics_are_available_on_the_interpreter() {
    let interp = run("ClrStat: 4 DT: 6 DT").unwrap();
    assert_eq!(interp.stats().n(), 2.0);
    assert_eq!(interp.stats().mean_x(), 5.0);
}

#[test]
fn mean_without_data_is_math_error() {
    let err = run("ClrStat: meanx").unwrap_err();
    assert!(matches!(err, CalcError::Math(_)), "{err:?}");
}

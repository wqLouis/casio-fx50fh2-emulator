//! End-to-end tests for statistics (`DT`, S-SUM and S-VAR).

use casio_fx50fh2::{CalcError, Interpreter, MockHost, compile, evaluate};

/// Statistics require SD or REG mode.  Most tests here use REG, which is a
/// superset of SD; the SD-specific behaviour is covered separately below.
const REG: &str = "#mode REG\n";
const SD: &str = "#mode SD\n";

fn run_with(mode: &str, source: &str) -> Result<Interpreter<MockHost>, CalcError> {
    let program = compile(&format!("{mode}{source}"))?;
    let mut interp = Interpreter::new(program, MockHost::default());
    interp.run()?;
    Ok(interp)
}

fn run(source: &str) -> Result<Interpreter<MockHost>, CalcError> {
    run_with(REG, source)
}

fn output(source: &str) -> Vec<String> {
    run(source).unwrap().into_host().output
}

fn value(source: &str) -> f64 {
    evaluate(&format!("{REG}{source}")).unwrap()
}

fn value_err(source: &str) -> CalcError {
    evaluate(&format!("{REG}{source}")).unwrap_err()
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

// -- the seven regression models -------------------------------------------
//
// Each model is fitted on data generated from that model exactly, so the
// coefficients are the generating parameters (up to floating point) and the
// correlation coefficient is 1.

/// y = 3 + 2·ln x, with x = 1, 2, 4.
#[test]
fn logarithmic_regression_model() {
    let setup = "ClrStat: Log: 1,3 DT: 2,4.386294361119891 DT: 4,5.772588722239781 DT: ";
    assert_close(value(&format!("{setup}rega")), 3.0);
    assert_close(value(&format!("{setup}regb")), 2.0);
    assert_close(value(&format!("{setup}regr")), 1.0);
}

/// y = 2·e^(0.5x), with x = 0, 1, 2.
#[test]
fn exponential_regression_model() {
    let setup = "ClrStat: Exp: 0,2 DT: 1,3.297442541400256 DT: 2,5.43656365691809 DT: ";
    assert_close(value(&format!("{setup}rega")), 2.0);
    assert_close(value(&format!("{setup}regb")), 0.5);
    assert_close(value(&format!("{setup}regr")), 1.0);
}

/// y = 3·x², with x = 1, 2, 3.
#[test]
fn power_regression_model() {
    let setup = "ClrStat: Pwr: 1,3 DT: 2,12 DT: 3,27 DT: ";
    assert_close(value(&format!("{setup}rega")), 3.0);
    assert_close(value(&format!("{setup}regb")), 2.0);
    assert_close(value(&format!("{setup}regr")), 1.0);
}

/// y = 1 + 2/x, with x = 1, 2, 4.
#[test]
fn inverse_regression_model() {
    let setup = "ClrStat: Inv: 1,3 DT: 2,2 DT: 4,1.5 DT: ";
    assert_close(value(&format!("{setup}rega")), 1.0);
    assert_close(value(&format!("{setup}regb")), 2.0);
    assert_close(value(&format!("{setup}regr")), 1.0);
}

/// y = 1 + 2x + 3x², with x = 0, 1, 2, 3.
#[test]
fn quadratic_regression_model() {
    let setup = "ClrStat: Quad: 0,1 DT: 1,6 DT: 2,17 DT: 3,34 DT: ";
    assert_close(value(&format!("{setup}rega")), 1.0);
    assert_close(value(&format!("{setup}regb")), 2.0);
    assert_close(value(&format!("{setup}regc")), 3.0);
    assert_close(value(&format!("{setup}regr")), 1.0);
}

/// y = 2·3^x, with x = 0, 1, 2.
#[test]
fn ab_exponential_regression_model() {
    let setup = "ClrStat: AB-Exp: 0,2 DT: 1,6 DT: 2,18 DT: ";
    assert_close(value(&format!("{setup}rega")), 2.0);
    assert_close(value(&format!("{setup}regb")), 3.0);
    assert_close(value(&format!("{setup}regr")), 1.0);
}

/// The model selector is a statement, so it takes effect for later data and
/// accessors, and `regc` is zero outside Quad.
#[test]
fn switching_the_regression_model() {
    let source =
        "ClrStat: Lin: 0,1 DT: 1,2 DT: rega◢: ClrStat: Quad: 0,1 DT: 1,6 DT: 2,17 DT: regc◢";
    assert_eq!(output(source), vec!["1", "3"]);
}

/// `Log` (and `Pwr`) need positive `x`; a non-positive one is a `Math ERROR`.
#[test]
fn logarithmic_regression_rejects_non_positive_x() {
    let err = value_err("ClrStat: Log: 0,1 DT: 1,2 DT: rega");
    assert!(matches!(err, CalcError::Math(_)), "{err:?}");
}

#[test]
fn clrstat_clears_data() {
    assert_eq!(value("ClrStat: 1 DT: 2 DT: ClrStat: n"), 0.0);
}

#[test]
fn complex_data_is_rejected() {
    // Complex values do not exist in a statistics mode, so this is caught
    // statically as a mode violation.
    let err = run("1+i DT").unwrap_err();
    assert!(matches!(err, CalcError::Mode { .. }), "{err:?}");
}

#[test]
fn sd_mode_allows_single_variable_stats() {
    let interp = run_with(SD, "ClrStat: 1 DT: 2 DT: 3 DT").unwrap();
    assert_eq!(interp.stats().n(), 3.0);
    assert_eq!(
        evaluate(&format!("{SD}ClrStat: 1 DT: 2 DT: 3 DT: sumx")).unwrap(),
        6.0
    );
}

#[test]
fn sd_mode_rejects_regression_variables() {
    let err = evaluate(&format!("{SD}1,2 DT: rega")).unwrap_err();
    assert!(matches!(err, CalcError::Mode { .. }), "{err:?}");
    // ...but the x-statistics are fine.
    assert_eq!(evaluate(&format!("{SD}1 DT: 2 DT: meanx")).unwrap(), 1.5);
}

#[test]
fn statistics_need_a_statistics_mode() {
    let err = compile("1 DT").unwrap_err();
    assert!(matches!(err, CalcError::Mode { .. }), "{err:?}");
    let err = compile("ClrStat: 1 DT: n").unwrap_err();
    assert!(matches!(err, CalcError::Mode { .. }), "{err:?}");
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

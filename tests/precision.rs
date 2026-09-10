//! End-to-end tests for the numeric-fidelity / autocorrection layer.

use casio_fx50fh2::precision::{autocorrect, normalize, round15};
use casio_fx50fh2::{Interpreter, MockHost, compile};

fn output(source: &str) -> Vec<String> {
    let program = compile(source).unwrap();
    let mut interp = Interpreter::new(program, MockHost::default());
    interp.run().unwrap();
    interp.into_host().output
}

#[test]
fn near_integer_fractions_collapse() {
    // 1/3 rounds to 0.999999999999999, whose LMNO = 9999 is autocorrected.
    assert_eq!(output("(1┘3)*3◢"), vec!["1"]);
    assert_eq!(output("0.1+0.2◢"), vec!["0.3"]);
}

#[test]
fn tiny_tails_are_dropped() {
    // `0.8 + 1E-15` has LMNO = 0001 and collapses back to 0.8.
    assert_eq!(output("0.8 + 1E-15◢"), vec!["0.8"]);
    assert_eq!(output("0.8 + 1E-14◢"), vec!["0.8"]);
}

#[test]
fn normal_values_are_untouched() {
    assert_eq!(normalize(1.2345678901234), round15(1.2345678901234));
    assert_eq!(autocorrect(round15(2.5)), 2.5);
}

#[test]
fn norm1_thresholds() {
    // Norm1 shows decimals only inside [1e-2, 1e10).
    assert_eq!(output("Norm 1: 0.01◢"), vec!["0.01"]);
    assert_eq!(output("Norm 1: 0.001◢"), vec!["1e-3"]);
    assert_eq!(output("Norm 1: 9999999999◢"), vec!["9999999999"]);
    assert_eq!(output("Norm 1: 10000000000◢"), vec!["1e10"]);
}

#[test]
fn norm2_thresholds() {
    // Norm2 shows decimals down to 1e-9.
    assert_eq!(output("Norm 2: 0.00000001◢"), vec!["0.00000001"]);
    assert_eq!(output("Norm 2: 0.000000001◢"), vec!["0.000000001"]);
    assert_eq!(output("Norm 2: 0.0000000001◢"), vec!["1e-10"]);
}

#[test]
fn literals_are_not_autocorrected() {
    // The literal itself keeps its 15 digits; only the display rounds.
    let program = compile("123456789.010005◢").unwrap();
    let mut interp = Interpreter::new(program, MockHost::default());
    interp.run().unwrap();
    assert_eq!(interp.environment().ans(), 123456789.010005);
}

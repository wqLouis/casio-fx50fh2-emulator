//! Regression tests for bugs found while wiring the CLI, transpiler and LSP
//! together. Each of these once produced a wrong answer or a spurious error.

use casio_fx50fh2::{Interpreter, MockHost, compile};

fn run(source: &str) -> Interpreter<MockHost> {
    let program = compile(source).expect("program should compile");
    let mut interp = Interpreter::new(program, MockHost::default());
    interp.run().expect("program should run");
    interp
}

fn display(source: &str) -> Vec<String> {
    run(source).into_host().output
}

/// The `^(...)` power operator used to fail because the lexer folded `^` and
/// `(` into one token while the parser expected them separately. The
/// transpiler emits exactly this spelling, so this guards both front ends.
#[test]
fn power_with_immediate_parenthesis() {
    assert_eq!(run("2^(3+1)").environment().ans(), 16.0);
    assert_eq!(run("2^(3)").environment().ans(), 8.0);
    assert_eq!(run("2^(3+1)").host().output, vec!["16"]);
}

/// The spaced and bare spellings must keep working too.
#[test]
fn power_other_spellings_unchanged() {
    assert_eq!(run("2^ (3+1)").environment().ans(), 16.0);
    // `^` binds tighter than `+`, so this is 2³ + 1, not 2⁴.
    assert_eq!(run("2^3+1").environment().ans(), 9.0);
}

/// `e^(x)` is Euler's number raised to `x`, whether `e^` is lexed as its own
/// key or as the constant followed by the power operator.
#[test]
fn euler_power() {
    let v = run("e^(1)").environment().ans();
    assert!((v - std::f64::consts::E).abs() < 1e-12, "e^(1) = {v}");
}

/// Hex literals may start with a letter (`FFh`), not just a digit.
#[test]
fn hex_literal_may_start_with_a_letter() {
    assert_eq!(run("Hex: FFh").environment().ans(), 255.0);
    assert_eq!(run("Hex: 1Fh").environment().ans(), 31.0);
    // Results are shown in the selected base.
    assert_eq!(display("Hex: FFh◢"), vec!["FFh"]);
}

/// Adding base-literal lexing must not swallow keywords or variables that also
/// consist of hex digits.
#[test]
fn base_literals_do_not_capture_identifiers() {
    // `AB` is `A × B`, not hexadecimal 0xAB.
    assert_eq!(run("3→A: 4→B: AB").environment().ans(), 12.0);
    // `Deg`, `Abs` and `cos` all begin with hex digits but are keywords.
    assert_eq!(display("Deg: 1+1◢"), vec!["2"]);
    assert_eq!(run("Abs(-3)").environment().ans(), 3.0);
    let v = run("cos(60)").environment().ans();
    assert!((v - 0.5).abs() < 1e-12, "cos(60) = {v}");
}

/// `x√` root takes its index from the preceding atom and its radicand from a
/// parenthesised expression.
#[test]
fn root_with_parentheses() {
    let v = run("3x√(27)").environment().ans();
    assert!((v - 3.0).abs() < 1e-12, "3√27 = {v}");
}

/// `disp` is the ASCII alias for the `◢` display token; transpiled `--ascii`
/// output relies on it round-tripping.
#[test]
fn disp_alias_matches_display_glyph() {
    assert_eq!(display("3+4disp"), display("3+4◢"));
    // A number immediately before `disp` must not lex as a decimal literal
    // (`4d`), which used to break ASCII-mode `print(4)` output.
    assert_eq!(display("4disp"), vec!["4"]);
    // ...while genuine decimal-tagged literals still work.
    assert_eq!(run("42d").environment().ans(), 42.0);
    assert_eq!(run("1Fh+1").environment().ans(), 32.0);
}

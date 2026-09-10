//! End-to-end interpreter tests: source text in, display lines / memory out.

use casio_fx50fh2::token::VarName;
use casio_fx50fh2::{CalcError, Interpreter, MockHost, compile};

fn run(source: &str, inputs: &[f64]) -> Result<Interpreter<MockHost>, CalcError> {
    let program = compile(source)?;
    let mut interp = Interpreter::new(program, MockHost::with_inputs(inputs.to_vec()));
    interp.run()?;
    Ok(interp)
}

/// Evaluate with no inputs and return the final answer memory.
fn value(source: &str) -> f64 {
    run(source, &[]).unwrap().environment().ans()
}

fn output(source: &str, inputs: &[f64]) -> Vec<String> {
    run(source, inputs).unwrap().into_host().output
}

#[test]
fn arithmetic_precedence() {
    assert_eq!(value("2+3×4"), 14.0);
    assert_eq!(value("2+3×4^2"), 50.0);
    // Prefix negation binds looser than the power key.
    assert_eq!(value("-2^2"), -4.0);
    // Implicit multiplication.
    assert_eq!(value("(1+2)(3+4)"), 21.0);
    let tau = value("2π");
    assert!((tau - std::f64::consts::TAU).abs() < 1e-12, "2π = {tau}");
}

#[test]
fn functions_and_angle_modes() {
    let v = value("sin(30)");
    assert!((v - 0.5).abs() < 1e-12, "sin(30) = {v}");
    let v = value("Rad: sin(pi┘2)");
    assert!((v - 1.0).abs() < 1e-12, "rad sin = {v}");
    let v = value("log(2,8)");
    assert!((v - 3.0).abs() < 1e-12);
    let v = value("√(2)");
    assert!((v - std::f64::consts::SQRT_2).abs() < 1e-12);
}

#[test]
fn combinatorics_and_factorial() {
    assert_eq!(value("5 nCr 2"), 10.0);
    assert_eq!(value("5 nPr 2"), 20.0);
    assert_eq!(value("6!"), 720.0);
}

#[test]
fn input_prompt_and_assignment() {
    let interp = run("?→A: A×2◢", &[21.0]).unwrap();
    assert_eq!(interp.host().output, vec!["42"]);
    assert_eq!(interp.environment().get(VarName::A), 21.0);
}

#[test]
fn for_loop_accumulates() {
    let interp = run("0→A: For 1→X To 5: A+X→A: Next: A◢", &[]).unwrap();
    assert_eq!(interp.environment().get(VarName::A), 15.0);
    assert_eq!(interp.host().output, vec!["15"]);
}

#[test]
fn for_loop_honours_step() {
    let interp = run("0→A: For 10→X To 1 Step -2: A+1→A: Next: A◢", &[]).unwrap();
    assert_eq!(interp.environment().get(VarName::A), 5.0);
}

#[test]
fn while_loop_and_break() {
    let out = output("For 1→X To 100: X>3⇒Break: X◢: Next: 9◢", &[]);
    assert_eq!(out, vec!["1", "2", "3", "9"]);
}

#[test]
fn if_else_branches() {
    assert_eq!(output("If 1: Then: 10◢: Else: 20◢: IfEnd", &[]), vec!["10"]);
    assert_eq!(output("If 0: Then: 10◢: Else: 20◢: IfEnd", &[]), vec!["20"]);
}

#[test]
fn nested_if() {
    let source = "If 1: Then: If 0: Then: 1◢: Else: 2◢: IfEnd: Else: 3◢: IfEnd";
    assert_eq!(output(source, &[]), vec!["2"]);
}

#[test]
fn goto_and_label() {
    let source = "0→A: 1→X: Lbl 1: A+X→A: X+1→X: X≤5⇒Goto 1: A◢";
    let interp = run(source, &[]).unwrap();
    assert_eq!(interp.environment().get(VarName::A), 15.0);
    assert_eq!(interp.host().output, vec!["15"]);
}

#[test]
fn conditional_jump_to_goto() {
    // A false condition skips the statement after `⇒`.
    assert_eq!(output("0⇒Goto 1: 5◢: Lbl 1: 7◢", &[]), vec!["5", "7"]);
    // A true condition performs the jump.
    assert_eq!(output("1⇒Goto 1: 5◢: Lbl 1: 7◢", &[]), vec!["7"]);
}

#[test]
fn memory_add_and_subtract() {
    let interp = run("5→A: A M+: 3 M+: 2 M-: M◢", &[]).unwrap();
    assert_eq!(interp.environment().get(VarName::M), 6.0);
    assert_eq!(interp.host().output, vec!["6"]);
}

#[test]
fn fix_and_sci_display() {
    assert_eq!(output("Fix 3: 10┘3◢", &[]), vec!["3.333"]);
    assert_eq!(output("Sci 2: 1234◢", &[]), vec!["1.23e3"]);
}

#[test]
fn division_by_zero_is_math_error() {
    let err = run("1┘0", &[]).unwrap_err();
    assert!(matches!(err, CalcError::Math(_)), "got {err:?}");
}

#[test]
fn missing_label_is_go_error() {
    let err = run("Goto 7", &[]).unwrap_err();
    assert_eq!(err, CalcError::Go(7));
}

#[test]
fn unmatched_if_is_nesting_error() {
    let err = run("If 1: 1◢", &[]).unwrap_err();
    assert!(matches!(err, CalcError::Nesting(_)), "got {err:?}");
}

#[test]
fn newlines_separate_statements() {
    let source = "3→A\n4→B\nA×B◢\n";
    assert_eq!(output(source, &[]), vec!["12"]);
}

#[test]
fn ascii_and_glyph_aliases_agree() {
    assert_eq!(output("5->A: A+1◢", &[]), output("5→A: A+1◢", &[]));
    assert_eq!(output("2=>3◢", &[]), output("2⇒3◢", &[]));
}

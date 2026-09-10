//! End-to-end tests for the `execute` feature: transpile `.fxc`, then run the
//! resulting PRGM on the real interpreter and inspect the displayed output.
//!
//! The whole file is compiled out when `execute` is disabled, which is what
//! keeps `cargo test -p fx-transpiler --no-default-features` independent of the
//! core interpreter crate.

#![cfg(feature = "execute")]

use casio_fx50fh2::{Interpreter, MockHost};
use fx_transpiler::transpile;

/// Transpile `source`, run it with the given `?` inputs and return the `◢`
/// displays in order.
#[track_caller]
fn run(source: &str, inputs: &[f64]) -> Vec<String> {
    let prgm = transpile(source).unwrap_or_else(|e| panic!("transpile failed: {e}"));
    let program = casio_fx50fh2::compile(&prgm)
        .unwrap_or_else(|e| panic!("transpiled PRGM failed to compile: {e}\n---\n{prgm}"));
    let mut interp = Interpreter::new(program, MockHost::with_inputs(inputs.iter().copied()));
    interp
        .run()
        .unwrap_or_else(|e| panic!("transpiled program failed to run: {e}\n---\n{prgm}"));
    interp.host().output.clone()
}

#[test]
fn factorial() {
    let source = "\
let n = input();
let result = 1;
for (let i = 1; i <= n; i = i + 1) { result = result * i; }
print(result);
";
    assert_eq!(run(source, &[5.0]), vec!["120"]);
    assert_eq!(run(source, &[0.0]), vec!["1"]);
    assert_eq!(run(source, &[1.0]), vec!["1"]);
    assert_eq!(run(source, &[10.0]), vec!["3628800"]);
}

#[test]
fn loop_sum() {
    let source = "\
let n = input();
let total = 0;
for (let i = 1; i <= n; i = i + 1) { total = total + i; }
print(total);
";
    assert_eq!(run(source, &[10.0]), vec!["55"]);
    assert_eq!(run(source, &[100.0]), vec!["5050"]);
}

#[test]
fn ascending_and_descending_for_loops() {
    let ascending = "\
let n = input();
for (let i = 0; i < n; i = i + 1) { print(i * i); }
";
    assert_eq!(run(ascending, &[4.0]), vec!["0", "1", "4", "9"]);

    let descending = "\
let n = input();
for (let i = n; i >= 1; i = i - 1) { print(i); }
";
    assert_eq!(run(descending, &[4.0]), vec!["4", "3", "2", "1"]);
}

#[test]
fn while_if_and_break() {
    let source = "\
let a = input();
while (a > 0) {
    if (a == 3) { break; }
    print(a);
    a = a - 1;
}
print(999);
";
    assert_eq!(run(source, &[5.0]), vec!["5", "4", "999"]);
}

#[test]
fn if_else_selects_the_right_branch() {
    let source = "\
let a = input();
if (a > 0) { print(1); } else if (a < 0) { print(2); } else { print(3); }
";
    assert_eq!(run(source, &[7.0]), vec!["1"]);
    assert_eq!(run(source, &[-7.0]), vec!["2"]);
    assert_eq!(run(source, &[0.0]), vec!["3"]);
}

#[test]
fn for_fallback_while_loop_runs() {
    // `!=` forces the emitter's `While` fallback. The final `print` keeps the
    // interpreter from auto-displaying the last computed value at program end.
    let source = "\
for (let i = 0; i != 3; i = i + 1) { print(i); }
print(-1);
";
    assert_eq!(run(source, &[]), vec!["0", "1", "2", "-1"]);
}

#[test]
fn goto_and_label_run() {
    let source = "\
let a = 1;
label 1;
print(a);
a = a + 1;
if (a <= 3) { goto 1; }
print(-1);
";
    assert_eq!(run(source, &[]), vec!["1", "2", "3", "-1"]);
}

#[test]
fn builtins_are_executable() {
    let source = "\
let a = input();
print(sqrt(a));
print(abs(a - 10));
print(log(a));
print(log(2, 8));
";
    assert_eq!(run(source, &[100.0]), vec!["10", "90", "2", "3"]);
}

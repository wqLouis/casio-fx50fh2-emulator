//! End-to-end tests for the `execute` feature: transpile `.fxc`, then run the
//! resulting PRGM on the real interpreter and inspect the displayed output.
//!
//! The whole file is compiled out when `execute` is disabled, which is what
//! keeps `cargo test -p fx-transpiler --no-default-features` independent of the
//! core interpreter crate.

#![cfg(feature = "execute")]

use casio_fx50fh2::{Interpreter, MockHost};
use fx_transpiler::transpile;

mod common;

/// Transpile `source`, run it with the given `?` inputs and return the `◢`
/// displays in order.
#[track_caller]
fn run(source: &str, inputs: &[f64]) -> Vec<String> {
    let source = &common::wrap(source);
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

#[test]
fn mode_header_runs_on_the_interpreter() {
    // The `#mode` first line we emit is accepted by the interpreter's compiler,
    // and ordinary real arithmetic still runs in the declared mode.
    let source = "#mode CMPLX\nlet a = 2; let b = 3; print(a + b);\n";
    assert_eq!(run(source, &[]), vec!["5"]);

    let source = "#mode SD\nlet a = 7; print(a * 2);\n";
    assert_eq!(run(source, &[]), vec!["14"]);
}

/// The transpiler's mode checker and the interpreter's must agree.
///
/// If the transpiler accepts a program, the PRGM it emits must compile and run
/// for the *same* mode. A hardcoded subset once let `abs`/`cbrt` pass `build`
/// in BASE and then fail `run`, which is exactly what this guards against.
#[test]
fn transpiler_mode_acceptance_implies_interpreter_acceptance() {
    use fx_transpiler::builtins::BUILTINS;
    use fx_transpiler::{Mode, Options, transpile_with};

    let modes = [Mode::Comp, Mode::Cmplx, Mode::Base, Mode::Sd, Mode::Reg];

    for mode in modes {
        for builtin in BUILTINS {
            let source = common::wrap(&format!("print({}(a));\n", builtin.name));
            let options = Options {
                ascii: false,
                mode: Some(mode),
                ..Default::default()
            };
            // Only the accepted cases matter: a rejection in both places is
            // also consistent, and is covered by the transpiler's own tests.
            let Ok(prgm) = transpile_with(&source, options) else {
                continue;
            };
            casio_fx50fh2::compile(&prgm).unwrap_or_else(|e| {
                panic!(
                    "transpiler accepted {}(...) in {mode} but the interpreter rejected the \
                     emitted PRGM: {e}\n---\n{prgm}",
                    builtin.name
                )
            });
        }
    }
}

// ---------------------------------------------------------------------------
// The full PRGM token vocabulary in `.fxc`

#[test]
fn ran_is_deterministic() {
    assert_eq!(run("print(ran());", &[]), vec!["0.6772111681"]);
    assert_eq!(
        run("print(ran()); print(ran());", &[]),
        vec!["0.6772111681", "0.7455286005"]
    );
}

#[test]
fn roots_powers_and_arithmetic_keys_run() {
    assert_eq!(run("print(root(3, 27));", &[]), vec!["3"]);
    assert_eq!(run("print(pow10(3));", &[]), vec!["1000"]);
    assert_eq!(run("print(exp(0));", &[]), vec!["1"]);
    assert_eq!(
        run("print(sqr(5)); print(cube(2)); print(inv(4));", &[]),
        vec!["25", "8", "0.25"]
    );
    assert_eq!(
        run("print(fact(5)); print(pct(50));", &[]),
        vec!["120", "0.5"]
    );
    assert_eq!(run("print(frac(1, 3));", &[]), vec!["0.3333333333"]);
    assert_eq!(
        run("print(ncr(5, 2)); print(npr(5, 2));", &[]),
        vec!["10", "20"]
    );
}

#[test]
fn base_n_operators_run() {
    let source = "\
#mode BASE
dec();
print(0b1010 and 0b1100);
print(0b1010 or 0b1100);
print(0b1010 xor 0b1100);
print(not(0));
print(neg(1));
";
    assert_eq!(run(source, &[]), vec!["8d", "14d", "6d", "-1d", "-1d"]);
}

#[test]
fn statistics_statements_run() {
    let source = "\
#mode REG
dt(1, 2);
dt(3, 4);
print(stat.meanx);
print(stat.sumxy);
freqon();
print(stat.n);
";
    assert_eq!(run(source, &[]), vec!["2", "14", "2"]);
}

#[test]
fn memory_accumulator_runs() {
    assert_eq!(run("mplus(3); mminus(1); print(mvalue());", &[]), vec!["2"]);
    assert_eq!(
        run("mplus(3); clrmemory(); print(mvalue());", &[]),
        vec!["0"]
    );
}

#[test]
fn conditional_jump_runs() {
    assert_eq!(
        run("let x = 5; x > 0 => print(1); x < 0 => print(2);", &[]),
        vec!["1"]
    );
}

#[test]
fn complex_builtins_run() {
    assert_eq!(run("#mode CMPLX\nprint(arg(i()));", &[]), vec!["90"]);
    assert_eq!(
        run("#mode CMPLX\nprint(conjg(1 + i()));", &[]),
        vec!["1-1𝑖"]
    );
    assert_eq!(
        run("#mode CMPLX\nprint(polar(2, 45));", &[]),
        vec!["1.414213562+1.414213562𝑖"]
    );
}

#[test]
fn fix_setup_rounds_the_display() {
    assert_eq!(run("fix(3); print(1 / 3);", &[]), vec!["0.333"]);
}

#[test]
fn ans_reads_the_previous_result() {
    assert_eq!(run("let a = 6 * 7; print(ans());", &[]), vec!["42"]);
}

// ---------------------------------------------------------------------------
// `rep` / `imp`: the real and imaginary parts
//
// The machine has no `ReP`/`ImP` key, so these lower to `conjg` arithmetic. The
// point of running them is that the *lowering* is what has to be right: the
// emitted expression must give back the part that was packed, and it must do so
// exactly.

/// The real and imaginary parts of a packed pair, on the interpreter.
#[test]
fn rep_and_imp_recover_a_packed_pair() {
    let source = "\
#mode CMPLX
let x = input();
let y = input();
let z = x + y * i();
print(rep(z));
print(imp(z));
";
    assert_eq!(run(source, &[3.0, 4.0]), vec!["3", "4"]);
    assert_eq!(run(source, &[-2.5, 7.25]), vec!["-2.5", "7.25"]);
    assert_eq!(run(source, &[0.0, 0.0]), vec!["0", "0"]);
    // Negative and fractional, to catch a sign error in either identity.
    assert_eq!(run(source, &[-0.5, -0.25]), vec!["-0.5", "-0.25"]);
}

/// The round trip is exact to 14 significant digits. The doubling is why there
/// is a bound at all: `2·x` may need one digit more than `x` did, and every
/// operation is normalised to 15.
#[test]
fn rep_and_imp_are_exact_to_fourteen_digits() {
    let source = "\
#mode CMPLX
let x = input();
let y = input();
let z = x + y * i();
print(rep(z) - x);
print(imp(z) - y);
";
    // A value that needs all 14 digits the identity preserves.
    let x = 1.2345678901234e8;
    let y = -9.8765432109876e-7;
    assert_eq!(run(source, &[x, y]), vec!["0", "0"]);
}

/// Arithmetic on a packed value is arithmetic on both parts, so a squared
/// complex number has the parts you would expect — which is the reason to pack
/// coordinates rather than two unrelated values.
#[test]
fn rep_and_imp_see_the_result_of_complex_arithmetic() {
    let source = "\
#mode CMPLX
let x = input();
let y = input();
let z = x + y * i();
print(rep(z * z));
print(imp(z * z));
";
    // (3+4i)² = -7+24i
    assert_eq!(run(source, &[3.0, 4.0]), vec!["-7", "24"]);
    // (0.1+0.2i)² = -0.03+0.04i
    assert_eq!(run(source, &[0.1, 0.2]), vec!["-0.03", "0.04"]);
}

/// The pieces compose: `rep` of a sum, and a part used inside arithmetic.
#[test]
fn rep_and_imp_compose_with_ordinary_arithmetic() {
    let source = "\
#mode CMPLX
let a = input();
let b = input();
let c = input();
let d = input();
let p = a + b * i();
let q = c + d * i();
print(rep(p + q));
print(imp(p + q));
print(rep(p) * 10 + imp(p));
";
    assert_eq!(run(source, &[1.0, 2.0, 3.0, 4.0]), vec!["4", "6", "12"]);
}

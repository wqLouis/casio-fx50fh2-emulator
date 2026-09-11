//! End-to-end tests for the algebraic simplification pass.
//!
//! Every test goes through the public `transpile` API, so the rewrite is
//! exercised alongside folding, propagation, allocation and emission. The
//! point of the pass is byte savings: each identity removed deletes at least
//! one PRGM key.

use fx_transpiler::{Options, transpile, transpile_with};

mod common;

/// Transpile `source` in glyph mode and return the PRGM.
#[track_caller]
fn out(source: &str) -> String {
    let source = &common::wrap(source);
    transpile(source).unwrap_or_else(|e| panic!("transpile failed: {e}\n{source}"))
}

/// Transpile `source` in ASCII mode and return the PRGM.
#[track_caller]
fn ascii_out(source: &str) -> String {
    let source = &common::wrap(source);
    transpile_with(
        source,
        Options {
            ascii: true,
            ..Default::default()
        },
    )
    .unwrap_or_else(|e| panic!("transpile failed: {e}\n{source}"))
}

// ---------------------------------------------------------------------------
// Identity elements: `x + 0`, `x * 1`, `x / 1`, `x ^ 1`, `-(-x)`

#[test]
fn multiplying_by_one_drops_the_operator() {
    // Both spellings, because both cost the same two bytes today.
    assert_eq!(out("let a = input(); print(a * 1);"), "?→A\nA◢\n");
    assert_eq!(out("let a = input(); print(1 * a);"), "?→A\nA◢\n");
}

#[test]
fn adding_zero_drops_the_operator() {
    assert_eq!(out("let a = input(); print(a + 0);"), "?→A\nA◢\n");
    assert_eq!(out("let a = input(); print(0 + a);"), "?→A\nA◢\n");
}

#[test]
fn subtracting_zero_drops_the_operator() {
    assert_eq!(out("let a = input(); print(a - 0);"), "?→A\nA◢\n");
}

#[test]
fn dividing_by_one_drops_the_operator() {
    assert_eq!(out("let a = input(); print(a / 1);"), "?→A\nA◢\n");
}

#[test]
fn raising_to_the_first_power_drops_the_operator() {
    assert_eq!(out("let a = input(); print(a ^ 1);"), "?→A\nA◢\n");
}

#[test]
fn double_negation_collapses() {
    assert_eq!(out("let a = input(); print(-(-a));"), "?→A\nA◢\n");
}

#[test]
fn an_identity_element_is_dropped_even_around_a_call() {
    // `+ 0` cannot error and evaluates the call once either way, so it is safe
    // even though the operand is not inert.
    assert_eq!(out("print(sqrt(2) * 1);"), "√(2)◢\n");
    assert_eq!(out("print(-(-sqrt(2)));"), "√(2)◢\n");
}

// ---------------------------------------------------------------------------
// Absorbing elements: `x * 0`, `0 * x`, `x ^ 0`

#[test]
fn multiplying_by_zero_replaces_an_inert_operand() {
    assert_eq!(out("let a = input(); print(a * 0);"), "?→A\n0◢\n");
    assert_eq!(out("let a = input(); print(0 * a);"), "?→A\n0◢\n");
}

#[test]
fn raising_to_the_zeroth_power_is_one() {
    assert_eq!(out("let a = input(); print(a ^ 0);"), "?→A\n1◢\n");
}

#[test]
fn a_call_inside_a_multiply_by_zero_survives() {
    // `0 * sqrt(-1)` must still report `Math ERROR`: dropping the call would
    // turn a failing run into a successful one.
    assert_eq!(out("print(0 * sqrt(2));"), "0×√(2)◢\n");
    assert_eq!(out("print(sqrt(2) ^ 0);"), "√(2)^(0)◢\n");
}

// ---------------------------------------------------------------------------
// Self-operations: `x - x`, `x == x`, `x != x`

#[test]
fn a_self_subtraction_is_zero() {
    assert_eq!(out("let a = input(); print(a - a);"), "?→A\n0◢\n");
}

#[test]
fn a_self_comparison_is_decided() {
    assert_eq!(out("let a = input(); print(a == a);"), "?→A\n1◢\n");
    assert_eq!(out("let a = input(); print(a != a);"), "?→A\n0◢\n");
}

#[test]
fn a_self_division_of_a_name_is_left_alone() {
    // The variable can be zero, where the machine reports `Math ERROR`, and
    // `1` would not.
    assert_eq!(out("let a = input(); print(a / a);"), "?→A\nA÷A◢\n");
}

#[test]
fn self_operations_around_calls_are_left_alone() {
    // A call may error, and `ran()` would be drawn twice rather than once.
    assert_eq!(out("print(sqrt(2) - sqrt(2));"), "√(2)-√(2)◢\n");
    assert_eq!(out("print(sqrt(2) == sqrt(2));"), "√(2)=√(2)◢\n");
}

#[test]
fn base_tagged_literals_are_simplified_too() {
    // `fold` does not evaluate base-tagged literals, so these reach the pass.
    assert_eq!(out("print(0x2 / 0x2);"), "1◢\n");
    assert_eq!(out("print(0x5 - 0x5);"), "0◢\n");
    assert_eq!(out("print(0x3 * 0);"), "0◢\n");
}

// ---------------------------------------------------------------------------
// Symbolic constants are never turned into decimals (ADR 0015)

#[test]
fn a_symbolic_product_keeps_its_symbol() {
    assert_eq!(out("print(2 * pi);"), "2×π◢\n");
    assert_eq!(out("print(2 * e);"), "2×e◢\n");
}

#[test]
fn a_symbolic_constant_may_be_absorbed_by_zero() {
    // `0 × π` is exactly zero and shorter, but `π` never becomes a decimal.
    assert_eq!(out("print(0 * pi);"), "0◢\n");
}

// ---------------------------------------------------------------------------
// The pass composes with folding, and keeps declarations/assignments

#[test]
fn simplifications_expose_each_other_bottom_up() {
    // `(a * 1) - a` first loses the multiply, then becomes `a - a`, then `0`.
    assert_eq!(out("let a = input(); print((a * 1) - a);"), "?→A\n0◢\n");
}

#[test]
fn removing_a_use_keeps_the_store() {
    // Memory is observable, so `b` is still assigned even though every read of
    // it was replaced by the constant zero.
    assert_eq!(
        out("let a = input(); let b = a * 0; print(b);"),
        "?→A\n0→B\n0◢\n"
    );
}

#[test]
fn ascii_mode_uses_the_same_rewrites() {
    assert_eq!(ascii_out("let a = input(); print(a * 1);"), "?->A\nAdisp\n");
    assert_eq!(ascii_out("let a = input(); print(a * 0);"), "?->A\n0disp\n");
}

#[test]
fn a_real_program_is_shortened_without_changing_its_shape() {
    // Every identity appears once, in a program that computes something.
    let source = "\
let a = input();
let b = input();
print(a * 1 + b * 0);
print((a - a) == 0);
print(-(-a) / 1);
print(a ^ 0);
";
    assert_eq!(out(source), "?→A\n?→B\nA◢\n1◢\nA◢\n1◢\n");
}

// ---------------------------------------------------------------------------
// The simplified program runs the same as one written without the identities

#[cfg(feature = "execute")]
mod running {
    use casio_fx50fh2::{Interpreter, MockHost};
    use fx_transpiler::transpile;

    use super::common;

    /// Transpile and run `source` with the given `?` inputs, returning either
    /// the `◢` displays or the error the run raised.
    #[track_caller]
    fn run(source: &str, inputs: &[f64]) -> Result<Vec<String>, String> {
        let source = &common::wrap(source);
        let prgm = transpile(source).unwrap_or_else(|e| panic!("transpile failed: {e}\n{source}"));
        let program = casio_fx50fh2::compile(&prgm)
            .unwrap_or_else(|e| panic!("transpiled PRGM failed to compile: {e}\n---\n{prgm}"));
        let mut interp = Interpreter::new(program, MockHost::with_inputs(inputs.iter().copied()));
        match interp.run() {
            Ok(()) => Ok(interp.host().output.clone()),
            Err(e) => Err(format!("{e}")),
        }
    }

    #[test]
    fn a_simplified_program_runs_identically() {
        // The left source leans on every identity; the right source is what the
        // identities mean. Both must display the same things for the same
        // inputs.
        let with_identities = "\
let a = input();
let b = input();
print(a * 1 + b * 0);
print(a - a);
print(-(-b) / 1);
print(a == a);
print(b != b);
print(a ^ 0);
";
        let hand_written = "\
let a = input();
let b = input();
print(a);
print(0);
print(b);
print(1);
print(0);
print(1);
";
        for inputs in [&[3.0, 4.0][..], &[-2.5, 0.0][..], &[0.0, 7.0][..]] {
            assert_eq!(
                run(with_identities, inputs),
                run(hand_written, inputs),
                "inputs {inputs:?}"
            );
        }
    }

    #[test]
    fn a_self_division_still_raises_math_error() {
        // The pass must leave `a / a` alone, so zero still errors rather than
        // quietly displaying `1`.
        let outcome = run("let a = input(); print(a / a);", &[0.0]);
        let message = outcome.expect_err("expected a Math ERROR for 0 / 0");
        assert!(message.contains("Math ERROR"), "{message}");
    }
}

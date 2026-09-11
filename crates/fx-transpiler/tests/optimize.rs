//! The `Options::optimize` switch, and what the optimiser is worth.
//!
//! The optional passes ([`simplify`](crate::simplify) and
//! [`propagate`](crate::propagate)) are **on by default**, because the machine
//! has only 680 bytes of program storage shared by all four program areas. The
//! golden tests compare against the *unoptimised* translation
//! (`Options::optimize = false`) so that each construct's own spelling is still
//! checked — the optimiser would otherwise fold `A≠B` to `1` and test nothing.
//! The optimiser itself is covered by `tests/simplify.rs` and
//! `tests/propagate.rs`.
//!
//! This file is about the switch: that it defaults on, that turning it off
//! gives the raw translation, and that **both forms compute the same thing**.

use fx_transpiler::size::measure;
use fx_transpiler::{Options, transpile, transpile_with};

mod common;

/// The same options with every optional pass turned off.
const RAW: Options = Options {
    ascii: false,
    mode: None,
    optimize: false,
};

#[track_caller]
fn raw(source: &str) -> String {
    transpile_with(&common::wrap(source), RAW)
        .unwrap_or_else(|e| panic!("transpile failed: {e}\n{source}"))
}

#[track_caller]
fn optimized(source: &str) -> String {
    transpile(&common::wrap(source)).unwrap_or_else(|e| panic!("transpile failed: {e}\n{source}"))
}

// ---------------------------------------------------------------------------
// The default is optimised

#[test]
fn the_default_is_optimised() {
    // `Options::default()` has to turn the passes *on*; a bool field defaulting
    // to `false` would silently disable them.
    assert!(Options::default().optimize);
    assert!(
        !transpile_with("fn main() { print(1); }", Options::default())
            .unwrap()
            .is_empty()
    );
}

#[test]
fn a_constant_expression_is_precalculated_by_default() {
    assert_eq!(optimized("print(2 * 3 + 4);"), "10◢\n");
    assert_eq!(raw("print(2 * 3 + 4);"), "10◢\n", "fold is not optional");
}

#[test]
fn propagation_reaches_a_use_by_default_and_not_when_disabled() {
    assert_eq!(optimized("let n = 3; print(n + 1);"), "3→A\n4◢\n");
    assert_eq!(raw("let n = 3; print(n + 1);"), "3→A\nA+1◢\n");
}

#[test]
fn simplification_applies_by_default_and_not_when_disabled() {
    assert_eq!(optimized("let a = input(); print(a * 1);"), "?→A\nA◢\n");
    assert_eq!(raw("let a = input(); print(a * 1);"), "?→A\nA×1◢\n");
}

#[test]
fn a_constant_condition_is_decided_by_default() {
    assert_eq!(optimized("if (1) { print(9); } else { print(8); }"), "9◢\n");
    assert_eq!(
        raw("if (1) { print(9); } else { print(8); }"),
        "If 1\nThen\n9◢\nElse\n8◢\nIfEnd\n"
    );
}

// ---------------------------------------------------------------------------
// Turning it off is safe: `fold` and `unroll` still run

#[test]
fn unrolling_still_happens_without_the_optimiser() {
    // An array element must name a memory while transpiling, so unrolling is
    // required for correctness and is not part of the optional passes.
    let prgm = raw("let v[3]; v[0] = 1; v[2] = 3; print(v[0] + v[2]);");
    assert_eq!(prgm, "1→A\n3→C\nA+C◢\n");
}

#[test]
fn a_const_is_still_inlined_without_the_optimiser() {
    // `const` is inlined by the emitter, and the value must be pre-calculated
    // for it to be emittable at all.
    assert_eq!(raw("const k = 2; print(k * 3);"), "6◢\n");
}

#[test]
fn a_function_is_still_inlined_without_the_optimiser() {
    // A function definition is top-level, so this must not be wrapped in
    // `fn main()`. The argument is a variable, so the inlined body is visible;
    // a literal argument would be folded to its value (`fold` is not optional).
    let prgm = transpile_with(
        "fn dbl(x) = x * 2;\nfn main() { let a = input(); print(dbl(a)); }",
        RAW,
    )
    .unwrap();
    assert_eq!(prgm, "?→A\nA×2◢\n");
}

// ---------------------------------------------------------------------------
// Both forms compute the same thing

#[test]
fn the_optimised_program_is_never_larger() {
    let programs = [
        "let a = 5; print(a * 2);",
        "let x = 1; let y = 2; print(x + y);",
        "let n = 3; for (let i = 0; i < n; i = i + 1) { print(i); }",
        "if (1) { print(9); } else { print(8); }",
        "let a = input(); print(a * 1 + 0);",
        "let unused = 1; print(2);",
        "const k = 6; print(k + 1);",
    ];
    for source in programs {
        let optimized_keys = measure(&optimized(source)).keys;
        let raw_keys = measure(&raw(source)).keys;
        assert!(
            optimized_keys <= raw_keys,
            "optimised was larger for {source:?}: {optimized_keys} > {raw_keys}"
        );
    }
}

#[cfg(feature = "execute")]
mod runs {
    use super::*;
    use casio_fx50fh2::{Interpreter, MockHost};

    /// Run a listing on the interpreter and collect its `◢` displays.
    fn displays(prgm: &str, inputs: &[f64]) -> Vec<String> {
        let program = casio_fx50fh2::compile(prgm)
            .unwrap_or_else(|e| panic!("transpiled PRGM failed to compile: {e}\n---\n{prgm}"));
        let mut interp = Interpreter::new(program, MockHost::with_inputs(inputs.iter().copied()));
        interp
            .run()
            .unwrap_or_else(|e| panic!("transpiled program failed to run: {e}\n---\n{prgm}"));
        interp.host().output.clone()
    }

    /// The optimiser must not change what a program computes.
    #[test]
    fn both_forms_compute_the_same_thing() {
        // This is the load-bearing test for the whole optimisation story: the
        // passes are on by default, so a rewrite that changes behaviour is a
        // silent wrong answer. The cases deliberately span every construct the
        // passes touch — arithmetic, comparisons, loops, arrays, functions,
        // statistics, sexagesimal, `ans`, and declarations nothing reads — and
        // both forms are run on the real interpreter and compared.
        let cases: &[(&str, &[f64])] = &[
            // Arithmetic and the identities `simplify` removes.
            ("let a = 5; print(a * 2);", &[]),
            ("let a = input(); print(a * 1 + 0);", &[7.0]),
            ("let a = input(); print(-(-a));", &[7.0]),
            ("let a = input(); print(a - a);", &[7.0]),
            ("let a = input(); print(a * 0);", &[7.0]),
            ("let a = input(); print(a / a);", &[3.0]),
            ("print(2 * 3 + 4);", &[]),
            ("print(2 * pi);", &[]),
            // Comparisons, which are folded when both sides are fixed.
            ("let a = 1; let b = 2; print(a < b); print(a == b);", &[]),
            ("let a = input(); print(a > 0); print(a <= 0);", &[5.0]),
            // Propagation into a loop bound, and the dead store left behind.
            (
                "let n = 3; for (let i = 0; i < n; i = i + 1) { print(i); }",
                &[],
            ),
            ("let unused = 1; print(2);", &[]),
            ("let k = 42; print(7);", &[]),
            // Constant conditions, both ways round.
            ("if (1) { print(9); } else { print(8); }", &[]),
            ("if (0) { print(9); } else { print(8); }", &[]),
            ("while (0) { print(1); } print(2);", &[]),
            (
                "let a = input(); if (a > 0) { print(1); } else { print(2); }",
                &[-1.0],
            ),
            // Loops that are *not* constant-foldable.
            (
                "let s = 0; let i = 1; while (i <= 5) { s = s + i; i = i + 1; } print(s);",
                &[],
            ),
            (
                "for (let i = 1; i <= 4; i = i + 1) { if (i == 2) { print(99); } }",
                &[],
            ),
            // Arrays, including the unrolling that makes an index literal.
            ("let v[3] = {1,2,3}; print(v[0] + v[2]);", &[]),
            ("let v[3]; v[0] = 1; v[1] = 2; v[2] = 3; print(v[1]);", &[]),
            (
                "let v[2] = {input(), input()}; print(v[0] + v[1]);",
                &[3.0, 4.0],
            ),
            // Functions are inlined, so the optimiser sees through the call.
            (
                "fn dbl(x) = x * 2;\nfn main() { let a = 4; print(dbl(a)); }",
                &[],
            ),
            (
                "fn sum_to(n) { let t = 0; for (let i = 1; i <= n; i = i + 1) { t = t + i; } return t; }\nfn main() { print(sum_to(4)); }",
                &[],
            ),
            // `const` is inlined either way.
            ("const k = 6; print(k + 1);", &[]),
            (
                "const scale = 3; let a = input(); print(a * scale);",
                &[2.0],
            ),
            // Sexagesimal and statistics must survive the passes intact.
            ("print(dms(2, 15, 18));", &[]),
            (
                "#mode SD\nfn main() { clrstat(); dt(2); dt(4); print(stat.meanx); }",
                &[],
            ),
            // Statements whose stores must **not** be removed: one that prompts,
            // one that is displayed because nothing follows it, and one whose
            // result a later `ans()` reads.
            ("let a = input(); print(3);", &[42.0]),
            ("let a = 1;", &[]),
            ("let a = input(); print(ans());", &[6.0]),
        ];
        for (source, inputs) in cases {
            let optimized = optimized(source);
            let raw = raw(source);
            assert_eq!(
                displays(&optimized, inputs),
                displays(&raw, inputs),
                "optimised and raw disagree for {source:?}\n--- optimised ---\n{optimized}\n--- raw ---\n{raw}"
            );
        }
    }

    /// A user-defined function is inlined either way, and computes the same.
    #[test]
    fn an_inlined_function_computes_the_same_either_way() {
        let source = "fn dbl(x) = x * 2;\nfn main() { let a = 4; print(dbl(a)); }";
        let optimized = transpile_with(source, Options::default()).unwrap();
        let raw = transpile_with(source, RAW).unwrap();
        assert_eq!(displays(&optimized, &[]), vec!["8"]);
        assert_eq!(displays(&optimized, &[]), displays(&raw, &[]));
        assert!(
            optimized.len() < raw.len(),
            "expected {optimized:?} < {raw:?}"
        );
    }
}

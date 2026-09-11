//! End-to-end tests for arrays.
//!
//! An array element is one memory and no program bytes: PRGM has no indirect
//! addressing, so `v[k]` must be resolved while transpiling, which is why an
//! index has to be a literal. These tests cover the whole path — parsing,
//! allocation, emission, and running the result on the real interpreter.

#![cfg(feature = "execute")]

use casio_fx50fh2::{Interpreter, MockHost};
use fx_transpiler::{Options, analyze, transpile, transpile_with};

mod common;

/// Transpile `source`, run it with the given `?` inputs and return the `◢`
/// displays in order.
#[track_caller]
fn run(source: &str, inputs: &[f64]) -> Vec<String> {
    let source = &common::wrap(source);
    let prgm = transpile(source).unwrap_or_else(|e| panic!("transpile failed: {e}\n{source}"));
    let program = casio_fx50fh2::compile(&prgm)
        .unwrap_or_else(|e| panic!("transpiled PRGM failed to compile: {e}\n---\n{prgm}"));
    let mut interp = Interpreter::new(program, MockHost::with_inputs(inputs.iter().copied()));
    interp
        .run()
        .unwrap_or_else(|e| panic!("transpiled program failed to run: {e}\n---\n{prgm}"));
    interp.host().output.clone()
}

#[track_caller]
fn out(source: &str) -> String {
    let source = &common::wrap(source);
    transpile(source).unwrap_or_else(|e| panic!("transpile failed: {e}\n{source}"))
}

#[track_caller]
fn err(source: &str) -> fx_transpiler::error::TranspileError {
    transpile(&common::wrap(source)).unwrap_err()
}

// ---------------------------------------------------------------------------
// What an array compiles to

#[test]
fn an_initialiser_list_becomes_one_assignment_per_element() {
    assert_eq!(out("let v[3] = {10, 20, 30};\n"), "10→A\n20→B\n30→C\n");
}

#[test]
fn indexing_costs_no_program_bytes() {
    // The whole point of requiring compile-time indices: `v[1]` is one memory
    // letter, not an if-chain.
    assert_eq!(out("let v[2] = {1, 2};\nprint(v[1]);\n"), "1→A\n2→B\nB◢\n");
}

#[test]
fn a_declaration_without_an_initialiser_emits_nothing() {
    assert_eq!(out("let v[3];\nprint(v[0]);\n"), "A◢\n");
}

#[test]
fn a_size_can_be_inferred_from_the_list() {
    assert_eq!(
        out("let v[] = {1, 2, 3};\nprint(v[2]);\n"),
        "1→A\n2→B\n3→C\nC◢\n"
    );
}

#[test]
fn input_can_fill_an_array() {
    assert_eq!(out("let v[2] = {input(), 7};\n"), "?→A\n7→B\n");
    assert_eq!(
        run(
            "let v[2] = {input(), input()};\nprint(v[0] + v[1]);\n",
            &[3.0, 4.0]
        ),
        vec!["7"]
    );
}

#[test]
fn elements_are_written_by_a_literal_index() {
    assert_eq!(
        out("let v[2] = {1, 2};\nv[0] = 9;\nprint(v[0]);\n"),
        "1→A\n2→B\n9→A\nA◢\n"
    );
}

#[test]
fn ascii_mode_applies_to_array_code_too() {
    let prgm = transpile_with(
        &common::wrap("let v[2] = {1, 2};\nv[1] = 3;\nprint(v[1]);\n"),
        Options {
            ascii: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(prgm, "1->A\n2->B\n3->B\nBdisp\n");
}

// ---------------------------------------------------------------------------
// Running

#[test]
fn an_array_holds_several_values_at_once() {
    let source = "let v[3] = {4, 8, 15};\nprint(v[0] + v[1] + v[2]);\n";
    assert_eq!(run(source, &[]), vec!["27"]);
}

#[test]
fn an_element_can_be_overwritten() {
    let source = "let v[3] = {4, 8, 15};\nv[1] = 16;\nprint(v[1]);\n";
    assert_eq!(run(source, &[]), vec!["16"]);
}

#[test]
fn only_the_addressed_element_changes() {
    let source = "\
let v[3] = {1, 2, 3};
v[1] = 9;
print(v[0]);
print(v[1]);
print(v[2]);
";
    assert_eq!(run(source, &[]), vec!["1", "9", "3"]);
}

#[test]
fn an_element_can_be_computed_from_the_others() {
    let source = "let v[2] = {1, 2};\nv[0] = v[0] + v[1];\nprint(v[0]);\n";
    assert_eq!(run(source, &[]), vec!["3"]);
}

#[test]
fn a_single_element_array_behaves_like_a_variable() {
    assert_eq!(run("let v[1] = {5};\nprint(v[0] * 2);\n", &[]), vec!["10"]);
}

#[test]
fn a_seven_element_array_fills_every_memory() {
    let source = "let v[7] = {1,2,3,4,5,6,7};\nprint(v[0] + v[6]);\n";
    assert_eq!(run(source, &[]), vec!["8"]);
}

#[test]
fn a_freed_array_lets_the_next_one_reuse_the_memories() {
    let source = "\
let first[3] = {1, 2, 3};
print(first[2]);
free first;
let second[3] = {4, 5, 6};
print(second[0]);
";
    assert_eq!(run(source, &[]), vec!["3", "4"]);
}

#[test]
fn an_array_can_be_redeclared_after_a_free() {
    let source = "\
let v[2] = {1, 2};
free v;
let v[2] = {8, 9};
print(v[1]);
";
    assert_eq!(run(source, &[]), vec!["9"]);
}

// ---------------------------------------------------------------------------
// Memory plan

fn used(source: &str) -> usize {
    analyze(&common::wrap(source), std::path::Path::new("."))
        .unwrap_or_else(|e| panic!("analyze failed: {e}"))
        .allocation
        .used()
}

#[test]
fn an_array_uses_one_memory_per_element() {
    assert_eq!(used("let v[3] = {1,2,3};\n"), 3);
    assert_eq!(used("let v[1] = {1};\n"), 1);
    assert_eq!(used("let v[7] = {1,2,3,4,5,6,7};\n"), 7);
}

#[test]
fn a_freed_array_returns_every_memory() {
    let source = "let v[3] = {1,2,3};\nfree v;\nlet w[1] = {1};\n";
    let source = common::wrap(source);
    let analysis = analyze(&source, std::path::Path::new(".")).unwrap();
    // `w` reuses A, the first memory the freed array gave back.
    assert_eq!(analysis.allocation.bindings[3].memory, 'A');
    // `used` counts the memories touched over the program's life, so the three
    // the array held still count even though only one is occupied at the end.
    assert_eq!(analysis.allocation.used(), 3);
    assert_eq!(analysis.allocation.free(), vec!['D', 'X', 'Y', 'M']);
}

#[test]
fn an_array_is_reported_element_by_element() {
    let analysis = analyze(
        &common::wrap("let v[2] = {1,2};\n"),
        std::path::Path::new("."),
    )
    .unwrap();
    let labels: Vec<String> = analysis
        .allocation
        .bindings
        .iter()
        .map(|binding| binding.label())
        .collect();
    assert_eq!(labels, vec!["v[0]", "v[1]"]);
    assert_eq!(analysis.allocation.bindings[0].element, Some(0));
    assert_eq!(analysis.allocation.bindings[1].element, Some(1));
}

#[test]
fn the_reuse_note_names_the_array_not_the_element() {
    let analysis = analyze(
        &common::wrap("let v[2] = {1,2};\nfree v;\nlet w[2] = {3,4};\n"),
        std::path::Path::new("."),
    )
    .unwrap();
    let reuses: Vec<Vec<String>> = analysis
        .allocation
        .reuses()
        .map(|(_, occupants)| {
            occupants
                .iter()
                .map(|binding| binding.name.clone())
                .collect()
        })
        .collect();
    assert_eq!(reuses, vec![vec!["v".to_string(), "w".to_string()]; 2]);
    assert_eq!(analysis.allocation.freed, vec!["v"]);
}

#[test]
fn an_array_declared_after_a_scalar_starts_at_the_next_memory() {
    let analysis = analyze(
        &common::wrap("let x = 1;\nlet v[2] = {1,2};\n"),
        std::path::Path::new("."),
    )
    .unwrap();
    assert_eq!(analysis.allocation.bindings[0].memory, 'A');
    assert_eq!(analysis.allocation.bindings[1].memory, 'B');
    assert_eq!(analysis.allocation.bindings[2].memory, 'C');
}

// ---------------------------------------------------------------------------
// Errors

#[test]
fn an_index_outside_the_array_is_rejected() {
    let error = err("let v[3] = {1,2,3};\nprint(v[3]);\n");
    assert!(
        error
            .message
            .contains("index 3 is out of range for `v` (length 3)"),
        "{}",
        error.message
    );
}

#[test]
fn a_negative_index_is_rejected_at_compile_time() {
    // `v[-1]` folds to a negative number, which is not a valid index; the
    // index must be a non-negative whole number known while transpiling.
    let error = err("let v[3] = {1,2,3};\nprint(v[-1]);\n");
    assert!(
        error.message.contains("compile-time index"),
        "{}",
        error.message
    );
}

#[test]
fn indexing_a_scalar_is_rejected() {
    let error = err("let x = 1;\nprint(x[0]);\n");
    assert!(
        error.message.contains("is not an array"),
        "{}",
        error.message
    );
}

#[test]
fn an_array_needs_declaring_before_use() {
    let error = err("print(v[0]);\n");
    assert!(error.message.contains("`let v[N]`"), "{}", error.message);
}

#[test]
fn an_array_cannot_be_used_without_an_index() {
    let error = err("let v[2] = {1,2};\nprint(v);\n");
    assert!(
        error.message.contains("is an array; index it"),
        "{}",
        error.message
    );
}

#[test]
fn an_array_name_cannot_be_assigned() {
    let error = err("let v[2] = {1,2};\nv = 1;\n");
    assert!(
        error.message.contains("assign to an element"),
        "{}",
        error.message
    );
}

#[test]
fn an_initialiser_count_must_match_the_size() {
    let error = err("let v[3] = {1,2};\n");
    assert!(
        error
            .message
            .contains("declared with 3 element(s) but has 2 initialiser(s)"),
        "{}",
        error.message
    );
}

#[test]
fn a_size_or_an_initialiser_is_required() {
    let error = err("let v[];\n");
    assert!(
        error
            .message
            .contains("needs a size or an initialiser list"),
        "{}",
        error.message
    );
}

#[test]
fn an_empty_array_is_rejected() {
    let error = err("let v[0] = {};\n");
    assert!(
        error.message.contains("at least one element"),
        "{}",
        error.message
    );
}

#[test]
fn a_const_cannot_be_an_array() {
    let error = err("const v[3] = {1,2,3};\n");
    assert!(
        error.message.contains("`const` cannot declare an array"),
        "{}",
        error.message
    );
}

#[test]
fn an_element_cannot_reference_itself_in_its_initialiser() {
    let error = err("let v[2] = {v[0], 2};\n");
    assert!(
        error.message.contains("its own initializer"),
        "{}",
        error.message
    );
}

#[test]
fn an_array_element_has_no_fields() {
    let error = err("let v[2] = {1,2};\nprint(v[0].size);\n");
    assert!(
        error.message.contains("array elements are numbers"),
        "{}",
        error.message
    );
}

#[test]
fn a_scalar_cannot_become_an_array() {
    let error = err("let x = 1;\nlet x[2] = {1,2};\n");
    assert!(
        error.message.contains("single variable"),
        "{}",
        error.message
    );
}

#[test]
fn an_array_cannot_become_a_scalar_without_a_free() {
    let error = err("let v[2] = {1,2};\nlet v = 1;\n");
    assert!(
        error.message.contains("already declared"),
        "{}",
        error.message
    );
}

#[test]
fn freeing_one_element_is_rejected() {
    let error = err("let v[2] = {1,2};\nfree v[0];\n");
    assert!(
        error.message.contains("releases the whole array"),
        "{}",
        error.message
    );
}

#[test]
fn double_free_of_an_array_is_rejected() {
    let error = err("let v[2] = {1,2};\nfree v;\nfree v;\n");
    assert!(error.message.contains("double free"), "{}", error.message);
}

#[test]
fn using_a_freed_array_is_rejected() {
    let error = err("let v[2] = {1,2};\nfree v;\nprint(v[0]);\n");
    assert!(
        error.message.contains("not defined here"),
        "{}",
        error.message
    );
    assert!(
        error.message.contains("let v[N]"),
        "the hint should suggest declaring an array again: {}",
        error.message
    );
}

#[test]
fn eight_elements_do_not_fit_in_seven_memories() {
    let error = err("let v[8] = {1,2,3,4,5,6,7,8};\n");
    assert!(
        error.message.contains("needs 8 memories"),
        "{}",
        error.message
    );
}

#[test]
fn an_array_that_does_not_fit_names_the_occupants() {
    let error = err("let a=1;let b=1;let c=1;let d=1;let e=1;\nlet v[3]={1,2,3};\n");
    assert!(
        error.message.contains("no room for array `v`"),
        "{}",
        error.message
    );
    assert!(error.message.contains("A (`a`)"), "{}", error.message);
    assert!(
        error.message.contains("only 2 are free"),
        "{}",
        error.message
    );
}

#[test]
fn a_full_array_is_grouped_in_the_error() {
    let error = err("let v[5] = {1,2,3,4,5};\nlet w[3] = {1,2,3};\n");
    assert!(
        error.message.contains("A B C D X (`v[…]`)"),
        "{}",
        error.message
    );
}

#[test]
fn an_array_with_jumps_still_needs_unsafe_free() {
    let error = err("let v[2] = {1,2};\nfree v;\ngoto 1;\nlabel 1;\n");
    assert!(error.message.contains("unsafe_free"), "{}", error.message);

    let prgm = out("let v[2] = {1,2};\nunsafe_free v;\nlet w[2] = {3,4};\ngoto 1;\nlabel 1;\n");
    assert!(prgm.contains("3→A"), "{prgm}");
}

#[test]
fn arrays_work_in_base_mode_when_they_are_integers() {
    let prgm = transpile_with(
        &common::wrap("#mode BASE\nlet v[2] = {1, 2};\nprint(v[0] + v[1]);\n"),
        Options::default(),
    )
    .unwrap();
    assert!(prgm.contains("A+B◢"), "{prgm}");
}

#[test]
fn a_float_builtin_in_a_base_mode_array_is_still_rejected() {
    let error = transpile_with(
        &common::wrap("#mode BASE\nlet v[2] = {sqrt(2), 1};\n"),
        Options::default(),
    )
    .unwrap_err();
    assert!(error.message.contains("BASE mode"), "{}", error.message);
}

// ---------------------------------------------------------------------------
// Loops that index an array
//
// PRGM cannot look an array element up from a run-time index, so a constant
// loop over an array is unrolled while transpiling and the index becomes a
// literal. These tests run the result to prove the expansion means the same as
// the loop.

#[test]
fn a_constant_loop_filling_an_array_runs() {
    let source = "\
let m[3] = {0, 0, 0};
for (let i = 0; i < 3; i = i + 1) { m[i] = input(); }
print(m[0] + m[1] + m[2]);
";
    assert_eq!(run(source, &[4.0, 5.0, 6.0]), vec!["15"]);
}

#[test]
fn a_constant_loop_reading_an_array_runs() {
    let source = "\
let m[3] = {10, 20, 30};
for (let i = 0; i < 3; i = i + 1) { print(m[i] * 2); }
";
    assert_eq!(run(source, &[]), vec!["20", "40", "60"]);
}

#[test]
fn a_descending_constant_loop_over_an_array_runs() {
    let source = "\
let m[3] = {1, 2, 3};
for (let i = 2; i >= 0; i = i - 1) { print(m[i]); }
";
    assert_eq!(run(source, &[]), vec!["3", "2", "1"]);
}

#[test]
fn the_counter_keeps_its_final_value_after_unrolling() {
    let source = "let m[2];\nfor (let i = 0; i < 2; i = i + 1) { m[i] = 1; }\nprint(i);";
    assert_eq!(run(source, &[]), vec!["2"]);
}

#[test]
fn a_run_time_index_is_a_transpile_error() {
    let error =
        err("let n = input();\nlet m[3];\nfor (let i = 0; i < n; i = i + 1) { m[i] = 1; }\n");
    assert!(
        error.message.contains("compile-time index"),
        "{}",
        error.message
    );
}

#[test]
fn an_unrollable_loop_explains_the_missing_index() {
    // A `free` in the body stops the unroll, so the computed index survives.
    let error = err("let m[3];\nfor (let i = 0; i < 3; i = i + 1) { m[i] = 1; free m; }\n");
    assert!(
        error.message.contains("compile-time index"),
        "{}",
        error.message
    );
}

#[test]
fn an_array_loop_with_an_ununrolled_inner_loop_is_rejected_cleanly() {
    // The outer loop cannot be unrolled without duplicating the inner `let j`,
    // so it is left alone and the computed index is reported rather than a
    // confusing re-declaration error.
    let error = err(
        "let m[2];\nfor (let i = 0; i < 2; i = i + 1) {\n  m[i] = 1;\n  \
         for (let j = 0; j < 3; j = j + 1) { print(j); }\n}\n",
    );
    assert!(
        error.message.contains("compile-time index"),
        "{}",
        error.message
    );
}

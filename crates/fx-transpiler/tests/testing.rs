//! End-to-end tests for the JSON test-suite runner: real files on disk,
//! transpiled and executed against the interpreter.
#![cfg(feature = "testing")]

use std::path::{Path, PathBuf};

use fx_transpiler::testing::{TestError, load_suite_file, parse_suite, run_suite, run_suite_file};

/// A scratch directory that cleans itself up.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "fx-transpiler-testing-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        TempDir(dir)
    }

    fn write(&self, name: &str, content: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, content).expect("write temp file");
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn examples_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

// ---------------------------------------------------------------------------
// The suites that ship with the repository must keep passing

#[test]
fn shipped_factorial_suite_passes() {
    let report = run_suite_file(&examples_dir().join("factorial.fxc"), None).unwrap();
    assert!(report.is_success(), "{report:#?}");
    assert_eq!(report.passed(), 5);
    assert_eq!(report.failed(), 0);
}

#[test]
fn shipped_quadratic_suite_passes() {
    let report = run_suite_file(&examples_dir().join("quadratic.fxc"), None).unwrap();
    assert!(report.is_success(), "{report:#?}");
    assert_eq!(report.passed(), 3);
}

#[test]
fn shipped_compiletime_suite_passes() {
    let report = run_suite_file(&examples_dir().join("compiletime.fxc"), None).unwrap();
    assert!(report.is_success(), "{report:#?}");
    assert_eq!(report.passed(), 1);
}

/// The cases travel inside the program: `fx50 test examples/factorial.fxc`.
#[test]
fn a_program_carries_its_own_tests() {
    let report = run_suite_file(&examples_dir().join("factorial.fxc"), None).unwrap();
    assert!(report.is_success(), "{report:#?}");
    // ...and the suite knows the program is the file it came from.
    assert!(report.name.contains("factorial.fxc"), "{}", report.name);
}

// ---------------------------------------------------------------------------
// Resolution and loading

#[test]
fn program_is_resolved_relative_to_the_suite() {
    let dir = TempDir::new("relative");
    // Nested layout: the JSON lives in `tests/`, the program in `src/`.
    std::fs::create_dir_all(dir.0.join("src")).unwrap();
    std::fs::create_dir_all(dir.0.join("tests")).unwrap();
    std::fs::write(
        dir.0.join("src/double.fxc"),
        "let a = input(); print(a * 2);",
    )
    .unwrap();
    let suite_path = dir.0.join("tests/double.tests.json");
    std::fs::write(
        &suite_path,
        r#"{"program": "../src/double.fxc", "cases": [{"input": [21], "output": ["42"]}]}"#,
    )
    .unwrap();

    let report = run_suite_file(&suite_path, None).unwrap();
    assert!(report.is_success(), "{report:#?}");
}

#[test]
fn a_suite_may_omit_program_and_use_the_fallback() {
    let dir = TempDir::new("fallback");
    let program = dir.write("prog.fxc", "print(7);");
    let suite = dir.write("prog.tests.json", r#"{"cases": [{"output": ["7"]}]}"#);

    // Without a fallback, the sibling `prog.fxc` is discovered automatically.
    let report = run_suite_file(&suite, None).unwrap();
    assert!(report.is_success(), "{report:#?}");

    // ...and an explicit fallback works too.
    let report = run_suite_file(&suite, Some(&program)).unwrap();
    assert!(report.is_success(), "{report:#?}");
}

#[test]
fn a_suite_with_neither_program_nor_source_is_rejected() {
    let dir = TempDir::new("nocase");
    let suite = dir.write("empty.tests.json", r#"{"cases": []}"#);
    let err = load_suite_file(&suite, None).unwrap_err();
    assert!(matches!(err, TestError::Schema(_)), "{err}");
    assert!(err.to_string().contains("`program`"), "{err}");
}

#[test]
fn inline_source_needs_no_file() {
    let suite = parse_suite(
        r#"{"source": "let a = input(); print(a + 1);",
            "cases": [{"input": [41], "output": ["42"]}]}"#,
        "inline",
        Path::new("."),
        None,
    )
    .unwrap();
    assert!(run_suite(&suite).is_success());
}

#[test]
fn the_json_name_field_labels_the_report() {
    let suite = parse_suite(
        r#"{"name": "my suite", "source": "", "cases": []}"#,
        "fallback-name",
        Path::new("."),
        None,
    )
    .unwrap();
    assert_eq!(suite.name, "my suite");
}

// ---------------------------------------------------------------------------
// Reporting

#[test]
fn a_failing_case_reports_expected_and_actual() {
    let dir = TempDir::new("failing");
    let suite = dir.write(
        "f.tests.json",
        r#"{"source": "print(2+3);", "cases": [{"name": "adds", "output": ["6"]}]}"#,
    );
    let report = run_suite_file(&suite, None).unwrap();
    assert!(!report.is_success());
    assert_eq!(report.failed(), 1);
    assert_eq!(report.cases[0].name, "adds");
    assert_eq!(report.cases[0].expected, "6");
    assert_eq!(report.cases[0].actual, "5");
}

#[test]
fn multi_line_output_is_rendered_line_by_line() {
    let suite = parse_suite(
        r#"{"source": "print(1); print(2);", "cases": [{"output": ["9", "9"]}]}"#,
        "s",
        Path::new("."),
        None,
    )
    .unwrap();
    let report = run_suite(&suite);
    assert_eq!(report.cases[0].actual, "1\n2");
    assert_eq!(report.cases[0].expected, "9\n9");
}

#[test]
fn the_json_report_round_trips_through_a_parser() {
    let dir = TempDir::new("json");
    let suite = dir.write(
        "j.tests.json",
        r#"{"source": "print(1);", "cases": [{"name": "one", "output": ["1"]}]}"#,
    );
    let report = run_suite_file(&suite, None).unwrap();
    let json = report.to_json_pretty();

    let value = fx_transpiler::json::parse(&json).expect("the report is valid JSON");
    assert_eq!(
        value
            .get("failed")
            .and_then(fx_transpiler::json::Json::as_f64),
        Some(0.0)
    );
    assert_eq!(
        value
            .get("cases")
            .and_then(|cases| cases.index(0))
            .and_then(|case| case.get("passed"))
            .and_then(fx_transpiler::json::Json::as_bool),
        Some(true)
    );
}

#[test]
fn a_transpile_error_is_reported_per_case() {
    let suite = parse_suite(
        r#"{"source": "print(nope(1));", "cases": [{"output": []}]}"#,
        "s",
        Path::new("."),
        None,
    )
    .unwrap();
    let report = run_suite(&suite);
    assert!(!report.is_success());
    assert!(
        report.cases[0].actual.contains("transpile error"),
        "{report:#?}"
    );
}

#[test]
fn every_case_is_run_even_when_one_fails() {
    let suite = parse_suite(
        r#"{"source": "print(1);",
            "cases": [{"output": ["9"]}, {"output": ["1"]}, {"output": ["9"]}]}"#,
        "s",
        Path::new("."),
        None,
    )
    .unwrap();
    let report = run_suite(&suite);
    assert_eq!(report.cases.len(), 3);
    assert_eq!(report.passed(), 1);
    assert_eq!(report.failed(), 2);
}

//! JSON-driven test suites for `.fxc` programs.
//!
//! A suite is a JSON document that pairs a program with cases, each giving the
//! values to feed to `?` prompts and the display lines (or the error) to
//! expect. It is the natural shape for regression testing a calculator
//! program, and it is easy for a tool — or an AI agent — to generate.
//!
//! ```json
//! {
//!   "program": "factorial.fxc",
//!   "cases": [
//!     { "name": "5!", "input": [5], "output": ["120"] },
//!     { "name": "0!", "input": [0], "output": ["1"] }
//!   ]
//! }
//! ```
//!
//! Run it from the CLI with `fx50 test <file.fxc>` (which finds the sibling
//! `<file>.tests.json`) or `fx50 test <file.tests.json>`.
//!
//! ```no_run
//! # use std::path::Path;
//! # use fx_transpiler::testing::{run_suite_file, TestError};
//! # fn main() -> Result<(), TestError> {
//! let report = run_suite_file(Path::new("examples/factorial.tests.json"), None)?;
//! assert!(report.is_success());
//! # Ok(())
//! # }
//! ```
//!
//! ## Schema
//!
//! | Field | Required | Meaning |
//! | --- | --- | --- |
//! | `name` | no | Suite label, used in output. |
//! | `program` | one of | Path to a `.fxc` file, resolved relative to the JSON. |
//! | `source` | one of | Inline `.fxc` source instead of a file. |
//! | `mode` | no | Operating mode override (`COMP`, `CMPLX`, `BASE`, `SD`, `REG`). |
//! | `ascii` | no | Transpile with ASCII aliases. Default `false`. |
//! | `cases` | yes | The test cases. |
//!
//! Each case takes:
//!
//! | Field | Required | Meaning |
//! | --- | --- | --- |
//! | `name` | no | Case label. Defaults to `case N`. |
//! | `input` | no | Numbers fed to `?` in order. Default `[]`. |
//! | `output` | one of | Expected `◢` display lines, in order. |
//! | `error` | one of | Expected error, matched against the calculator label. |
//!
//! A case must give exactly one of `output` or `error`. `"output": []` asserts
//! that the program displays nothing. `error` is matched case-insensitively as
//! a substring of the label, so `"Math"` matches `Math ERROR`.

use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use casio_fx50fh2::{Interpreter, MockHost};

use crate::{Mode, Options, transpile_with};

// ---------------------------------------------------------------------------
// Errors

/// Why a suite could not be loaded or run.
#[derive(Debug)]
pub enum TestError {
    /// The JSON itself is malformed.
    Json {
        message: String,
        line: usize,
        column: usize,
    },
    /// A referenced file could not be read.
    Io { path: PathBuf, message: String },
    /// The JSON parsed but does not describe a valid suite.
    Schema(String),
}

impl fmt::Display for TestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TestError::Json {
                message,
                line,
                column,
            } => write!(f, "invalid JSON at line {line}, column {column}: {message}"),
            TestError::Io { path, message } => {
                write!(f, "cannot read `{}`: {message}", path.display())
            }
            TestError::Schema(message) => write!(f, "invalid test suite: {message}"),
        }
    }
}

impl std::error::Error for TestError {}

// ---------------------------------------------------------------------------
// Suite model

/// A loaded test suite, ready to run.
#[derive(Debug, Clone, PartialEq)]
pub struct TestSuite {
    /// Suite label (from `name`, or the file name).
    pub name: String,
    /// The `.fxc` source under test.
    pub program: String,
    /// Operating mode override, if the suite specified one.
    pub mode: Option<Mode>,
    /// Whether to transpile with ASCII aliases.
    pub ascii: bool,
    pub cases: Vec<TestCase>,
}

/// One case: inputs in, and the expected result.
#[derive(Debug, Clone, PartialEq)]
pub struct TestCase {
    pub name: String,
    /// Values fed to `?` prompts, in order.
    pub input: Vec<f64>,
    /// Expected `◢` display lines. Empty when [`TestCase::error`] is set.
    pub output: Vec<String>,
    /// Expected error, matched against the calculator label.
    pub error: Option<String>,
}

// ---------------------------------------------------------------------------
// Raw JSON shapes

#[derive(Debug, Deserialize)]
struct RawSuite {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    program: Option<String>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    ascii: bool,
    cases: Vec<RawCase>,
}

#[derive(Debug, Deserialize)]
struct RawCase {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    input: Vec<f64>,
    /// `Option` so that "absent" is distinguishable from "empty".
    #[serde(default)]
    output: Option<Vec<String>>,
    #[serde(default)]
    error: Option<String>,
}

// ---------------------------------------------------------------------------
// Loading

/// `serde_json` appends ` at line N column M` to its messages; the position is
/// reported separately as structured fields, so drop the duplicate.
fn strip_position(message: &str) -> &str {
    match message.rfind(" at line ") {
        Some(index) => &message[..index],
        None => message,
    }
}

/// The conventional suite path for a program: `dir/stem.tests.json`.
pub fn sibling_suite_path(program: &Path) -> PathBuf {
    let mut name = program
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    name.push_str(".tests.json");
    program.with_file_name(name)
}

/// Parse a suite from JSON text.
///
/// `program` and `source` in the JSON are mutually exclusive. When neither is
/// present, `fallback_program` (if given) supplies the source, read relative
/// to `base_dir`. `base_dir` is normally the directory holding the JSON file.
pub fn parse_suite(
    json: &str,
    name: &str,
    base_dir: &Path,
    fallback_program: Option<&Path>,
) -> Result<TestSuite, TestError> {
    let raw: RawSuite = serde_json::from_str(json).map_err(|e| TestError::Json {
        message: strip_position(&e.to_string()).to_string(),
        line: e.line(),
        column: e.column(),
    })?;

    let program = match (raw.program.as_deref(), raw.source.as_deref()) {
        (Some(_), Some(_)) => {
            return Err(TestError::Schema(
                "give either `program` or `source`, not both".to_string(),
            ));
        }
        (Some(relative), None) => {
            let path = base_dir.join(relative);
            std::fs::read_to_string(&path).map_err(|e| TestError::Io {
                path,
                message: e.to_string(),
            })?
        }
        (None, Some(inline)) => inline.to_string(),
        (None, None) => match fallback_program {
            Some(path) => std::fs::read_to_string(path).map_err(|e| TestError::Io {
                path: path.to_path_buf(),
                message: e.to_string(),
            })?,
            None => {
                return Err(TestError::Schema(
                    "give `program` (a path) or `source` (inline .fxc text)".to_string(),
                ));
            }
        },
    };

    let mode = match raw.mode.as_deref() {
        Some(text) => Some(Mode::parse(text).ok_or_else(|| {
            TestError::Schema(format!(
                "unknown mode `{text}`; expected COMP, CMPLX, BASE, SD or REG"
            ))
        })?),
        None => None,
    };

    let mut cases = Vec::with_capacity(raw.cases.len());
    for (index, raw_case) in raw.cases.into_iter().enumerate() {
        let label = raw_case
            .name
            .unwrap_or_else(|| format!("case {}", index + 1));
        let case = match (raw_case.output, raw_case.error) {
            (Some(_), Some(_)) => {
                return Err(TestError::Schema(format!(
                    "case `{label}` gives both `output` and `error`"
                )));
            }
            (Some(output), None) => TestCase {
                name: label,
                input: raw_case.input,
                output,
                error: None,
            },
            (None, Some(error)) => TestCase {
                name: label,
                input: raw_case.input,
                output: Vec::new(),
                error: Some(error),
            },
            (None, None) => {
                return Err(TestError::Schema(format!(
                    "case `{label}` needs `output` or `error`"
                )));
            }
        };
        cases.push(case);
    }

    Ok(TestSuite {
        // An explicit `name` in the JSON wins; otherwise the caller's label
        // (usually the file name) is used.
        name: raw.name.unwrap_or_else(|| name.to_string()),
        program,
        mode,
        ascii: raw.ascii,
        cases,
    })
}

/// Load a suite from a `.tests.json` file.
///
/// `fallback_program` supplies the source when the JSON names neither
/// `program` nor `source`; the CLI passes the `.fxc` file it was handed so
/// that `fx50 test prog.fxc` works with a suite that omits `program`.
pub fn load_suite_file(
    suite_path: &Path,
    fallback_program: Option<&Path>,
) -> Result<TestSuite, TestError> {
    let json = std::fs::read_to_string(suite_path).map_err(|e| TestError::Io {
        path: suite_path.to_path_buf(),
        message: e.to_string(),
    })?;
    let fallback = match fallback_program {
        Some(explicit) => Some(explicit.to_path_buf()),
        None => {
            // `factorial.tests.json` implies `factorial.fxc` when present.
            let sibling = suite_path.with_extension("");
            let stem = suite_path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let candidate = if let Some(base) = stem.strip_suffix(".tests") {
                sibling.with_file_name(format!("{base}.fxc"))
            } else {
                sibling.with_extension("fxc")
            };
            candidate.is_file().then_some(candidate)
        }
    };
    let name = suite_path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| suite_path.display().to_string());
    let base_dir = suite_path.parent().unwrap_or(Path::new("."));
    parse_suite(&json, &name, base_dir, fallback.as_deref())
}

// ---------------------------------------------------------------------------
// Results

/// The outcome of one case.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CaseResult {
    pub name: String,
    pub passed: bool,
    /// What the case asked for, rendered for humans.
    pub expected: String,
    /// What the program actually did, rendered for humans.
    pub actual: String,
}

impl CaseResult {
    fn pass(name: String, expected: String) -> Self {
        CaseResult {
            name,
            passed: true,
            actual: expected.clone(),
            expected,
        }
    }

    fn fail(name: String, expected: String, actual: impl Into<String>) -> Self {
        CaseResult {
            name,
            passed: false,
            expected,
            actual: actual.into(),
        }
    }
}

/// The outcome of every case in a suite.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SuiteReport {
    pub name: String,
    pub cases: Vec<CaseResult>,
}

impl SuiteReport {
    pub fn passed(&self) -> usize {
        self.cases.iter().filter(|c| c.passed).count()
    }

    pub fn failed(&self) -> usize {
        self.cases.len() - self.passed()
    }

    pub fn is_success(&self) -> bool {
        self.failed() == 0
    }

    /// Render the report as pretty-printed JSON, for machine consumers.
    ///
    /// Kept here so callers (such as the CLI) need not depend on `serde_json`
    /// themselves.
    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
    }
}

// ---------------------------------------------------------------------------
// Running

/// Transpile and run every case in `suite`.
///
/// Transpile and compile failures are reported once per case rather than
/// aborting the suite, so a single report shows everything that is wrong.
pub fn run_suite(suite: &TestSuite) -> SuiteReport {
    let prepared = prepare(suite);
    let cases = suite
        .cases
        .iter()
        .map(|case| run_case(case, &prepared))
        .collect();
    SuiteReport {
        name: suite.name.clone(),
        cases,
    }
}

/// Load and run a suite in one step.
pub fn run_suite_file(
    suite_path: &Path,
    fallback_program: Option<&Path>,
) -> Result<SuiteReport, TestError> {
    let suite = load_suite_file(suite_path, fallback_program)?;
    Ok(run_suite(&suite))
}

/// The transpiled program, or the error that stopped it.
enum Prepared {
    Ready(Vec<casio_fx50fh2::ast::Stmt>),
    Failed(String),
}

fn prepare(suite: &TestSuite) -> Prepared {
    let options = Options {
        ascii: suite.ascii,
        mode: suite.mode,
    };
    let prgm = match transpile_with(&suite.program, options) {
        Ok(prgm) => prgm,
        Err(e) => return Prepared::Failed(format!("transpile error: {e}")),
    };
    match casio_fx50fh2::compile(&prgm) {
        Ok(program) => Prepared::Ready(program),
        Err(e) => Prepared::Failed(format!("{e}")),
    }
}

fn run_case(case: &TestCase, prepared: &Prepared) -> CaseResult {
    let expected = describe_expected(case);
    let program = match prepared {
        Prepared::Failed(message) => {
            return CaseResult::fail(case.name.clone(), expected, message.clone());
        }
        Prepared::Ready(program) => program.clone(),
    };

    let mut interp = Interpreter::new(program, MockHost::with_inputs(case.input.iter().copied()));
    match interp.run() {
        Ok(()) => {
            let lines = interp.host().output.clone();
            match &case.error {
                Some(_) => CaseResult::fail(
                    case.name.clone(),
                    expected,
                    format!("no error (displayed {})", describe_lines(&lines)),
                ),
                None if lines_match(&case.output, &lines) => {
                    CaseResult::pass(case.name.clone(), expected)
                }
                None => CaseResult::fail(case.name.clone(), expected, describe_lines(&lines)),
            }
        }
        Err(e) => {
            let label = e.label();
            match &case.error {
                Some(wanted) if label_matches(wanted, label) => {
                    CaseResult::pass(case.name.clone(), expected)
                }
                Some(_) => CaseResult::fail(case.name.clone(), expected, label),
                None => CaseResult::fail(case.name.clone(), expected, format!("{e}")),
            }
        }
    }
}

/// Does the expected error name match the calculator's label?
///
/// Case-insensitive substring, so `"math"` matches `Math ERROR`.
fn label_matches(wanted: &str, label: &str) -> bool {
    label
        .to_ascii_lowercase()
        .contains(&wanted.trim().to_ascii_lowercase())
}

/// Compare display lines, ignoring surrounding whitespace on each line.
fn lines_match(expected: &[String], actual: &[String]) -> bool {
    expected.len() == actual.len()
        && expected
            .iter()
            .zip(actual)
            .all(|(a, b)| a.trim() == b.trim())
}

fn describe_expected(case: &TestCase) -> String {
    match &case.error {
        Some(error) => format!("error matching \"{error}\""),
        None => describe_lines(&case.output),
    }
}

fn describe_lines(lines: &[String]) -> String {
    if lines.is_empty() {
        "<no output>".to_string()
    } else {
        lines.join("\n")
    }
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn suite(json: &str) -> TestSuite {
        parse_suite(json, "inline", Path::new("."), None).unwrap()
    }

    #[test]
    fn parses_inline_source_with_cases() {
        let s = suite(r#"{"source": "print(1);", "cases": [{"input": [], "output": ["1"]}]}"#);
        assert_eq!(s.cases.len(), 1);
        assert_eq!(s.cases[0].output, vec!["1"]);
        assert!(s.mode.is_none());
    }

    #[test]
    fn parses_mode_and_ascii() {
        let s = suite(r#"{"source": "print(1);", "mode": "cmplx", "ascii": true, "cases": []}"#);
        assert_eq!(s.mode, Some(Mode::Cmplx));
        assert!(s.ascii);
    }

    #[test]
    fn names_cases_by_position_when_unnamed() {
        let s = suite(r#"{"source": "", "cases": [{"output": []}, {"output": []}]}"#);
        assert_eq!(s.cases[0].name, "case 1");
        assert_eq!(s.cases[1].name, "case 2");
    }

    #[test]
    fn rejects_both_program_and_source() {
        let err = parse_suite(
            r#"{"program": "x.fxc", "source": "", "cases": []}"#,
            "s",
            Path::new("."),
            None,
        )
        .unwrap_err();
        assert!(matches!(err, TestError::Schema(_)), "{err}");
    }

    #[test]
    fn rejects_a_case_with_neither_output_nor_error() {
        let err = parse_suite(
            r#"{"source": "", "cases": [{}]}"#,
            "s",
            Path::new("."),
            None,
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("needs `output` or `error`"),
            "{err}"
        );
    }

    #[test]
    fn rejects_a_case_with_both_output_and_error() {
        let err = parse_suite(
            r#"{"source": "", "cases": [{"output": ["1"], "error": "Math"}]}"#,
            "s",
            Path::new("."),
            None,
        )
        .unwrap_err();
        assert!(err.to_string().contains("both"), "{err}");
    }

    #[test]
    fn reports_json_syntax_errors_with_a_position() {
        let err = parse_suite("{ nope }", "s", Path::new("."), None).unwrap_err();
        match err {
            TestError::Json { line, column, .. } => {
                assert_eq!(line, 1);
                assert!(column >= 1);
            }
            other => panic!("expected a JSON error, got {other}"),
        }
    }

    #[test]
    fn sibling_suite_path_swaps_the_extension() {
        assert_eq!(
            sibling_suite_path(Path::new("examples/factorial.fxc")),
            PathBuf::from("examples/factorial.tests.json")
        );
    }

    #[test]
    fn matches_errors_by_substring_and_case() {
        assert!(label_matches("Math", "Math ERROR"));
        assert!(label_matches("math error", "Math ERROR"));
        assert!(!label_matches("Go", "Math ERROR"));
    }

    #[test]
    fn output_comparison_ignores_surrounding_whitespace() {
        let expected = vec!["1".to_string(), " 2 ".to_string()];
        let actual = vec!["1".to_string(), "2".to_string()];
        assert!(lines_match(&expected, &actual));
        assert!(!lines_match(&expected, &["1".to_string()]));
    }

    // -- running ------------------------------------------------------------

    fn report(json: &str) -> SuiteReport {
        run_suite(&suite(json))
    }

    #[test]
    fn passing_case_reports_success() {
        let r = report(r#"{"source": "print(2+3);", "cases": [{"output": ["5"]}]}"#);
        assert!(r.is_success(), "{r:?}");
        assert_eq!(r.passed(), 1);
    }

    #[test]
    fn wrong_output_reports_both_sides() {
        let r = report(r#"{"source": "print(2+3);", "cases": [{"output": ["6"]}]}"#);
        assert!(!r.is_success());
        assert_eq!(r.cases[0].expected, "6");
        assert_eq!(r.cases[0].actual, "5");
    }

    #[test]
    fn inputs_are_fed_to_prompts_in_order() {
        let r = report(
            r#"{"source": "let a = input(); let b = input(); print(a*10+b);",
                "cases": [{"input": [4, 2], "output": ["42"]}]}"#,
        );
        assert!(r.is_success(), "{r:?}");
    }

    #[test]
    fn expected_error_is_matched_by_label() {
        let r = report(
            r#"{"source": "let a = input(); print(1/a);",
                "cases": [{"input": [0], "error": "Math ERROR"}]}"#,
        );
        assert!(r.is_success(), "{r:?}");
    }

    #[test]
    fn unexpected_error_fails_the_case() {
        let r = report(r#"{"source": "print(1/0);", "cases": [{"output": []}]}"#);
        assert!(!r.is_success());
        assert!(r.cases[0].actual.contains("Math ERROR"), "{r:?}");
    }

    #[test]
    fn missing_error_fails_the_case() {
        let r = report(r#"{"source": "print(1);", "cases": [{"error": "Math"}]}"#);
        assert!(!r.is_success());
        assert!(r.cases[0].actual.contains("no error"), "{r:?}");
    }

    #[test]
    fn empty_output_asserts_silence() {
        // `label 1;` computes nothing, so the program displays nothing.
        let r = report(r#"{"source": "label 1;", "cases": [{"output": []}]}"#);
        assert!(r.is_success(), "{r:?}");
    }

    #[test]
    fn a_trailing_assignment_still_displays() {
        // The calculator shows the last computed value when a program ends
        // without `◢`, so this is *not* silent.
        let r = report(r#"{"source": "let a = 1;", "cases": [{"output": ["1"]}]}"#);
        assert!(r.is_success(), "{r:?}");
        let r = report(r#"{"source": "let a = 1;", "cases": [{"output": []}]}"#);
        assert!(!r.is_success(), "{r:?}");
    }

    #[test]
    fn transpile_errors_are_reported_per_case() {
        let r = report(r#"{"source": "print(foo(1));", "cases": [{"output": []}]}"#);
        assert!(!r.is_success());
        assert!(r.cases[0].actual.contains("transpile error"), "{r:?}");
    }

    #[test]
    fn mode_override_reaches_the_program() {
        // `.fxc` cannot express complex numbers, so the observable effect of a
        // mode override is `BASE` rejecting floating-point built-ins.
        let r = report(
            r#"{"source": "print(sqrt(4));", "mode": "BASE",
                "cases": [{"output": []}]}"#,
        );
        assert!(!r.is_success(), "{r:?}");
        assert!(r.cases[0].actual.contains("BASE"), "{r:?}");
    }

    #[test]
    fn a_valid_program_passes_in_its_declared_mode() {
        let r = report(
            r#"{"source": "print(2+3);", "mode": "BASE",
                "cases": [{"output": ["5"]}]}"#,
        );
        assert!(r.is_success(), "{r:?}");
    }

    #[test]
    fn reports_are_serialisable_for_machine_consumers() {
        let r = report(r#"{"source": "print(1);", "cases": [{"output": ["1"]}]}"#);
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"passed\":true"), "{json}");
    }
}

//! JSON-driven test suites for `.fxc` programs.
//!
//! A suite pairs a program with cases, each giving the values to feed to `?`
//! prompts and the display lines (or the error) to expect. The cases live
//! **in the program**, in a `#tests` table — the same compile-time data
//! facility [`crate::data`] provides for anything else:
//!
//! ```text
//! // factorial.fxc
//! let n = input();
//! let r = 1;
//! for (let i = 2; i <= n; i = i + 1) { r = r * i; }
//! print(r);
//!
//! #tests = [
//!   { "name": "5!", "input": [5], "output": ["120"] },
//!   { "name": "0!", "input": [0], "output": ["1"] }
//! ];
//! ```
//!
//! Run it with `fx50 test factorial.fxc`. A separate `<name>.tests.json` file
//! still works for suites that prefer to live apart from their program; the
//! schema is the same, with `program` (a path) or `source` (inline text) in
//! place of the enclosing file.
//!
//! ```no_run
//! # use std::path::Path;
//! # use fx_transpiler::testing::{run_suite_file, TestError};
//! # fn main() -> Result<(), TestError> {
//! let report = run_suite_file(Path::new("examples/factorial.fxc"), None)?;
//! assert!(report.is_success());
//! # Ok(())
//! # }
//! ```
//!
//! ## `#tests` schema
//!
//! The table may be an array of cases, or an object:
//!
//! | Field | Required | Meaning |
//! | --- | --- | --- |
//! | `name` | no | Suite label, used in output. |
//! | `mode` | no | Operating mode override (`COMP`, `CMPLX`, `BASE`, `SD`, `REG`). |
//! | `ascii` | no | Transpile with ASCII aliases. Default `false`. |
//! | `cases` | yes | The test cases. |
//!
//! A standalone `.tests.json` additionally takes `program` **or** `source`.
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

use casio_fx50fh2::{Interpreter, MockHost};

use crate::json::{self, Json};
use crate::{Mode, Options, data, include, transpile_with_loader};

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
    /// Directory that `#include` and `#data` paths in the program resolve
    /// against: the program's own directory when it came from a file,
    /// otherwise the directory holding the suite.
    pub base_dir: PathBuf,
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
// Decoding

/// The pieces of a suite document, before the program text is resolved.
struct RawSuite {
    name: Option<String>,
    program: Option<String>,
    source: Option<String>,
    mode: Option<String>,
    ascii: bool,
    cases: Vec<RawCase>,
}

struct RawCase {
    name: Option<String>,
    input: Vec<f64>,
    output: Option<Vec<String>>,
    error: Option<String>,
}

fn schema(message: impl Into<String>) -> TestError {
    TestError::Schema(message.into())
}

/// Decode a suite document. `allow_program` is false for an embedded `#tests`
/// table, where the program is the file the table lives in.
fn decode_suite(value: &Json, allow_program: bool) -> Result<RawSuite, TestError> {
    // A bare array is a list of cases.
    if let Some(items) = value.as_array() {
        let mut cases = Vec::with_capacity(items.len());
        for (index, item) in items.iter().enumerate() {
            cases.push(decode_case(item, &format!("case {}", index + 1))?);
        }
        return Ok(RawSuite {
            name: None,
            program: None,
            source: None,
            mode: None,
            ascii: false,
            cases,
        });
    }

    let Json::Object(entries) = value else {
        return Err(schema(format!(
            "expected an object or an array of cases, found {}",
            value.type_name()
        )));
    };

    for (key, _) in entries {
        match key.as_str() {
            "name" | "program" | "source" | "mode" | "ascii" | "cases" => {}
            other => {
                return Err(schema(format!(
                    "unknown field `{other}`; expected one of name, mode, ascii, cases"
                )));
            }
        }
    }
    if !allow_program && (value.get("program").is_some() || value.get("source").is_some()) {
        return Err(schema(
            "`program`/`source` belong in a `.tests.json` file; an embedded `#tests` table \
             describes cases for the file it is written in",
        ));
    }

    let string_field = |key: &str| -> Result<Option<String>, TestError> {
        match value.get(key) {
            None => Ok(None),
            Some(Json::String(text)) => Ok(Some(text.clone())),
            Some(other) => Err(schema(format!(
                "`{key}` must be a string, found {}",
                other.type_name()
            ))),
        }
    };

    let ascii = match value.get("ascii") {
        None => false,
        Some(Json::Bool(flag)) => *flag,
        Some(other) => {
            return Err(schema(format!(
                "`ascii` must be a boolean, found {}",
                other.type_name()
            )));
        }
    };

    let cases_value = value
        .get("cases")
        .ok_or_else(|| schema("the suite has no `cases` array"))?;
    let items = cases_value.as_array().ok_or_else(|| {
        schema(format!(
            "`cases` must be an array, found {}",
            cases_value.type_name()
        ))
    })?;
    let mut cases = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        cases.push(decode_case(item, &format!("case {}", index + 1))?);
    }

    Ok(RawSuite {
        name: string_field("name")?,
        program: string_field("program")?,
        source: string_field("source")?,
        mode: string_field("mode")?,
        ascii,
        cases,
    })
}

fn decode_case(value: &Json, positional: &str) -> Result<RawCase, TestError> {
    let Json::Object(entries) = value else {
        return Err(schema(format!(
            "{positional} must be an object, found {}",
            value.type_name()
        )));
    };
    for (key, _) in entries {
        match key.as_str() {
            "name" | "input" | "output" | "error" => {}
            other => {
                return Err(schema(format!(
                    "{positional} has unknown field `{other}`; expected name, input, output or error"
                )));
            }
        }
    }

    let name = match value.get("name") {
        None => positional.to_string(),
        Some(Json::String(text)) => text.clone(),
        Some(other) => {
            return Err(schema(format!(
                "{positional}: `name` must be a string, found {}",
                other.type_name()
            )));
        }
    };

    let mut input = Vec::new();
    match value.get("input") {
        None => {}
        Some(Json::Array(items)) => {
            for (index, item) in items.iter().enumerate() {
                let number = item.as_f64().ok_or_else(|| {
                    schema(format!(
                        "{name}: `input[{index}]` must be a number, found {}",
                        item.type_name()
                    ))
                })?;
                input.push(number);
            }
        }
        Some(other) => {
            return Err(schema(format!(
                "{name}: `input` must be an array, found {}",
                other.type_name()
            )));
        }
    }

    let output = match value.get("output") {
        None => None,
        Some(Json::Array(items)) => {
            let mut lines = Vec::with_capacity(items.len());
            for (index, item) in items.iter().enumerate() {
                let line = item.as_str().ok_or_else(|| {
                    schema(format!(
                        "{name}: `output[{index}]` must be a string, found {}",
                        item.type_name()
                    ))
                })?;
                lines.push(line.to_string());
            }
            Some(lines)
        }
        Some(other) => {
            return Err(schema(format!(
                "{name}: `output` must be an array of strings, found {}",
                other.type_name()
            )));
        }
    };

    let error = match value.get("error") {
        None => None,
        Some(Json::String(text)) => Some(text.clone()),
        Some(other) => {
            return Err(schema(format!(
                "{name}: `error` must be a string, found {}",
                other.type_name()
            )));
        }
    };

    match (&output, &error) {
        (Some(_), Some(_)) => {
            return Err(schema(format!("{name} gives both `output` and `error`")));
        }
        (None, None) => {
            return Err(schema(format!("{name} needs `output` or `error`")));
        }
        _ => {}
    }

    Ok(RawCase {
        name: Some(name),
        input,
        output,
        error,
    })
}

/// Turn decoded cases into the suite model.
fn assemble(
    raw: RawSuite,
    name: String,
    program: String,
    base_dir: PathBuf,
) -> Result<TestSuite, TestError> {
    let mode = match raw.mode.as_deref() {
        Some(text) => Some(Mode::parse(text).ok_or_else(|| {
            schema(format!(
                "unknown mode `{text}`; expected COMP, CMPLX, BASE, SD or REG"
            ))
        })?),
        None => None,
    };

    let cases = raw
        .cases
        .into_iter()
        .enumerate()
        .map(|(index, case)| TestCase {
            name: case.name.unwrap_or_else(|| format!("case {}", index + 1)),
            input: case.input,
            output: case.output.unwrap_or_default(),
            error: case.error,
        })
        .collect();

    Ok(TestSuite {
        name: raw.name.unwrap_or(name),
        program,
        base_dir,
        mode,
        ascii: raw.ascii,
        cases,
    })
}

// ---------------------------------------------------------------------------
// Loading

/// The conventional suite path for a program: `dir/stem.tests.json`.
pub fn sibling_suite_path(program: &Path) -> PathBuf {
    let mut name = program
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    name.push_str(".tests.json");
    program.with_file_name(name)
}

/// Parse a standalone suite from JSON text.
///
/// `program` and `source` in the JSON are mutually exclusive. When neither is
/// present, `fallback_program` (if given) supplies the source, read relative
/// to `base_dir`. `base_dir` is normally the directory holding the JSON file.
pub fn parse_suite(
    json_text: &str,
    name: &str,
    base_dir: &Path,
    fallback_program: Option<&Path>,
) -> Result<TestSuite, TestError> {
    let value = json::parse(json_text).map_err(|e| TestError::Json {
        message: e.message,
        line: e.line,
        column: e.column,
    })?;
    let raw = decode_suite(&value, true)?;

    // The program's text, plus the directory its `#include`s resolve against.
    let (program, program_dir) = match (raw.program.as_deref(), raw.source.as_deref()) {
        (Some(_), Some(_)) => {
            return Err(schema("give either `program` or `source`, not both"));
        }
        (Some(relative), None) => {
            let path = base_dir.join(relative);
            let text = std::fs::read_to_string(&path).map_err(|e| TestError::Io {
                path: path.clone(),
                message: e.to_string(),
            })?;
            let dir = path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| base_dir.to_path_buf());
            (text, dir)
        }
        // Inline source has no directory of its own, so includes resolve
        // alongside the suite file.
        (None, Some(inline)) => (inline.to_string(), base_dir.to_path_buf()),
        (None, None) => match fallback_program {
            Some(path) => {
                let text = std::fs::read_to_string(path).map_err(|e| TestError::Io {
                    path: path.to_path_buf(),
                    message: e.to_string(),
                })?;
                let dir = path
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| base_dir.to_path_buf());
                (text, dir)
            }
            None => {
                return Err(schema(
                    "give `program` (a path) or `source` (inline .fxc text)",
                ));
            }
        },
    };

    assemble(raw, name.to_string(), program, program_dir)
}

/// Read the `#tests` table embedded in a `.fxc` program, if it has one.
///
/// `None` means the program declares no `#tests` table, which lets the caller
/// fall back to a sibling `.tests.json`. A malformed table, or one that is not
/// valid suite JSON, is an error.
pub fn parse_embedded_suite(
    source: &str,
    name: &str,
    base_dir: &Path,
) -> Result<Option<TestSuite>, TestError> {
    parse_embedded_suite_with_loader(source, name, base_dir, &crate::loader::FsLoader)
}

/// Like [`parse_embedded_suite`], but reads `#include`d libraries and `#data`
/// files through `loader`.
///
/// The suite that comes out is self-contained — [`assemble`] does no I/O — so a
/// host with no filesystem can run the embedded tests of a program it holds in
/// memory.
pub fn parse_embedded_suite_with_loader(
    source: &str,
    name: &str,
    base_dir: &Path,
    loader: &dyn crate::loader::FileLoader,
) -> Result<Option<TestSuite>, TestError> {
    let expanded =
        include::expand_with(source, None, base_dir, loader).map_err(|e| schema(format!("{e}")))?;
    let (_, tables) =
        data::extract_with(&expanded, base_dir, loader).map_err(|e| schema(format!("{e}")))?;
    let Some(tests) = tables.tests() else {
        return Ok(None);
    };
    let raw = decode_suite(tests, false)?;
    let program = source.to_string();
    let suite = assemble(raw, name.to_string(), program, base_dir.to_path_buf())?;
    Ok(Some(suite))
}

/// Load a suite from either a `.fxc` program (its embedded `#tests`) or a
/// `.tests.json` file.
///
/// `fallback_program` supplies the source when a JSON suite names neither
/// `program` nor `source`; the CLI passes the `.fxc` file it was handed so
/// that `fx50 test prog.fxc` also works with a sibling suite that omits
/// `program`.
pub fn load_suite_file(
    suite_path: &Path,
    fallback_program: Option<&Path>,
) -> Result<TestSuite, TestError> {
    let name = suite_path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| suite_path.display().to_string());
    let base_dir = suite_path.parent().unwrap_or(Path::new("."));

    // A `.fxc` program carries its own tests.
    if suite_path.extension().is_some_and(|ext| ext == "fxc") {
        let source = std::fs::read_to_string(suite_path).map_err(|e| TestError::Io {
            path: suite_path.to_path_buf(),
            message: e.to_string(),
        })?;
        if let Some(suite) = parse_embedded_suite(&source, &name, base_dir)? {
            return Ok(suite);
        }
        // Fall back to the conventional sibling file, if there is one.
        let sibling = sibling_suite_path(suite_path);
        if sibling.is_file() {
            return load_suite_file(&sibling, Some(suite_path));
        }
        return Err(schema(format!(
            "`{}` has no `#tests` table, and `{}` does not exist",
            suite_path.display(),
            sibling.display()
        )));
    }

    let json_text = std::fs::read_to_string(suite_path).map_err(|e| TestError::Io {
        path: suite_path.to_path_buf(),
        message: e.to_string(),
    })?;
    let fallback = match fallback_program {
        Some(explicit) => Some(explicit.to_path_buf()),
        None => {
            // `factorial.tests.json` implies `factorial.fxc` when present.
            let stem = suite_path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let candidate = if let Some(base) = stem.strip_suffix(".tests") {
                suite_path.with_file_name(format!("{base}.fxc"))
            } else {
                suite_path.with_extension("fxc")
            };
            candidate.is_file().then_some(candidate)
        }
    };
    parse_suite(&json_text, &name, base_dir, fallback.as_deref())
}

// ---------------------------------------------------------------------------
// Results

/// The outcome of one case.
#[derive(Debug, Clone, PartialEq)]
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
#[derive(Debug, Clone, PartialEq)]
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
    /// Kept here so callers (such as the CLI) need not depend on the JSON
    /// module or know the report's shape.
    pub fn to_json_pretty(&self) -> String {
        let cases = Json::array(self.cases.iter().map(|case| {
            Json::object([
                ("name", Json::string(case.name.as_str())),
                ("passed", Json::Bool(case.passed)),
                ("expected", Json::string(case.expected.as_str())),
                ("actual", Json::string(case.actual.as_str())),
            ])
        }));
        json::to_string_pretty(&Json::object([
            ("name", Json::string(self.name.as_str())),
            ("passed", Json::Number(self.passed() as f64)),
            ("failed", Json::Number(self.failed() as f64)),
            ("cases", cases),
        ]))
    }
}

// ---------------------------------------------------------------------------
// Running

/// Transpile and run every case in `suite`.
///
/// Transpile and compile failures are reported once per case rather than
/// aborting the suite, so a single report shows everything that is wrong.
pub fn run_suite(suite: &TestSuite) -> SuiteReport {
    run_suite_with_loader(suite, &crate::loader::FsLoader)
}

/// Like [`run_suite`], but reads `#include`d libraries through `loader`.
///
/// A suite parsed out of memory (an editor buffer, a browser) has no filesystem
/// to fall back on, so running it has to use the same loader that parsed it.
pub fn run_suite_with_loader(
    suite: &TestSuite,
    loader: &dyn crate::loader::FileLoader,
) -> SuiteReport {
    let prepared = prepare(suite, loader);
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

fn prepare(suite: &TestSuite, loader: &dyn crate::loader::FileLoader) -> Prepared {
    let options = Options {
        ascii: suite.ascii,
        mode: suite.mode,
        ..Default::default()
    };
    // Resolve the program's `#include` directives relative to wherever the
    // program actually lives, and through whichever loader the caller supplied
    // — a suite held in memory has no filesystem to fall back on.
    let prgm = match transpile_with_loader(&suite.program, options, None, &suite.base_dir, loader) {
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
        parse_suite(&with_main(json), "inline", Path::new("."), None).unwrap()
    }

    /// Wrap a suite's inline `"source"` value in its required `fn main()`.
    ///
    /// These tests exercise the suite *runner*, so they keep writing bare
    /// statement lists; this inserts the entry point the language now needs.
    /// `#data`/`#include` are resolved before lexing, so leaving one inside the
    /// wrapper is harmless — it is blanked or spliced either way.
    fn with_main(json: &str) -> String {
        const KEY: &str = "\"source\": \"";
        let Some(start) = json.find(KEY) else {
            return json.to_string();
        };
        let value_start = start + KEY.len();
        let Some(relative_end) = json[value_start..].find('"') else {
            return json.to_string();
        };
        let value_end = value_start + relative_end;
        let source = &json[value_start..value_end];
        let wrapped = format!("fn main() {{\\n{source}\\n}}");
        format!("{}{wrapped}{}", &json[..value_start], &json[value_end..])
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
    fn rejects_unknown_fields_and_wrong_types() {
        let err = parse_suite(
            r#"{"source": "", "nope": 1, "cases": []}"#,
            "s",
            Path::new("."),
            None,
        )
        .unwrap_err();
        assert!(err.to_string().contains("unknown field `nope`"), "{err}");

        let err = parse_suite(
            r#"{"source": "", "cases": [{"input": ["x"], "output": []}]}"#,
            "s",
            Path::new("."),
            None,
        )
        .unwrap_err();
        assert!(err.to_string().contains("must be a number"), "{err}");

        let err = parse_suite(
            r#"{"source": "", "cases": [{"output": [1]}]}"#,
            "s",
            Path::new("."),
            None,
        )
        .unwrap_err();
        assert!(err.to_string().contains("must be a string"), "{err}");
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

    // -- embedded `#tests` --------------------------------------------------

    #[test]
    fn reads_an_embedded_tests_table() {
        let source = "print(1);\n#tests = [{\"name\": \"one\", \"output\": [\"1\"]}];\n";
        let suite = parse_embedded_suite(source, "p.fxc", Path::new("."))
            .unwrap()
            .expect("a suite");
        assert_eq!(suite.cases.len(), 1);
        assert_eq!(suite.cases[0].name, "one");
        assert_eq!(suite.program, source);
    }

    #[test]
    fn an_embedded_table_may_be_an_object() {
        let source = "#tests = { \"name\": \"greet\", \"cases\": [{\"output\": []}] };\nprint(1);";
        let suite = parse_embedded_suite(source, "p.fxc", Path::new("."))
            .unwrap()
            .unwrap();
        assert_eq!(suite.name, "greet");
        assert_eq!(suite.cases.len(), 1);
    }

    #[test]
    fn embedded_tests_may_not_name_a_program() {
        let source = "#tests = { \"program\": \"other.fxc\", \"cases\": [] };\n";
        let err = parse_embedded_suite(source, "p.fxc", Path::new(".")).unwrap_err();
        assert!(
            err.to_string().contains("belong in a `.tests.json`"),
            "{err}"
        );
    }

    #[test]
    fn a_program_without_tests_yields_none() {
        assert!(
            parse_embedded_suite("print(1);\n", "p.fxc", Path::new("."))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn embedded_tests_bad_json_is_a_schema_error() {
        let err = parse_embedded_suite(
            "print(1);\n#tests = { \"cases\": [{}] };\n",
            "p",
            Path::new("."),
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("needs `output` or `error`"),
            "{err}"
        );
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
        let r = report(r#"{"source": "label 1;", "cases": [{"output": []}]}"#);
        assert!(r.is_success(), "{r:?}");
    }

    #[test]
    fn a_trailing_assignment_still_displays() {
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
    fn a_suite_may_use_compile_time_data() {
        let r =
            report(r##"{"source": "#data n = 3;\nprint(n * 2);", "cases": [{"output": ["6"]}]}"##);
        assert!(r.is_success(), "{r:?}");
    }

    #[test]
    fn reports_are_serialisable_for_machine_consumers() {
        let r = report(r#"{"source": "print(1);", "cases": [{"output": ["1"]}]}"#);
        let json = r.to_json_pretty();
        assert!(json.contains("\"passed\": true"), "{json}");
        // And the output is itself valid JSON with the expected shape.
        let value = json::parse(&json).unwrap();
        assert_eq!(value.get("failed").and_then(Json::as_f64), Some(0.0));
        assert_eq!(
            value
                .get("cases")
                .and_then(|c| c.index(0))
                .and_then(|c| c.get("name"))
                .and_then(Json::as_str),
            Some("case 1")
        );
    }
}

//! The request/response API, as ordinary Rust.
//!
//! Everything the wasm build can do is an operation on a JSON request, so the
//! whole surface is testable with plain `cargo test` on the host and
//! `lib.rs`/`abi.rs` stay a thin marshalling shell.
//!
//! The request shape is the *only* interface a host needs. It is deliberately
//! JSON rather than a set of typed exports, because the consumers are a web page
//! and an editor extension — neither of which can call into Rust types — and
//! because the transpiler already ships a zero-dependency JSON reader and writer
//! (ADR 0014). That is also why this crate needs no `wasm-bindgen`: strings and
//! numbers cross the boundary, and nothing else.
//!
//! # Operations
//!
//! | `op` | does |
//! |---|---|
//! | `version` | what this build is, plus the machine's limits |
//! | `transpile` | `.fxc` → PRGM, with its size and memory plan |
//! | `run` | transpile if needed, run, and report the displays |
//! | `tests` | run a program's embedded `#tests` table |
//! | `diagnostics` | errors for the editor (markers) |
//! | `completions` | the completion list for a language |
//! | `hover` | documentation at a position |
//! | `symbols` | the document outline |
//! | `constants` | the 40 scientific constants |
//! | `eval` | one expression |
//!
//! # The request
//!
//! Every operation accepts the same envelope, and uses what it needs:
//!
//! ```json
//! {
//!   "op": "transpile",
//!   "source": "fn main() { print(1 + 1); }",
//!   "entry": "main.fxc",
//!   "files": { "lib/pack.fxc": "fn pack(x, y) = x + y * i();" },
//!   "language": "fxc",
//!   "mode": "COMP",
//!   "ascii": false,
//!   "optimize": true,
//!   "inputs": [3, 4],
//!   "position": { "line": 0, "character": 4 }
//! }
//! ```
//!
//! * `source` is the document to work on. It may be omitted when `entry` names a
//!   file in `files`, which is how a multi-file project is transpiled:
//!   `{"op":"transpile","entry":"main.fxc","files":{…}}`.
//! * `entry` defaults to `"main.fxc"`. It is the root of the `#include` graph
//!   and the name diagnostics are reported against. Relative includes resolve
//!   against its directory, exactly as on disk.
//! * `files` replaces the filesystem: a map of path to text. The paths are
//!   matched forgivingly, so `/lib/x.fxc`, `lib/x.fxc` and `./lib/x.fxc` are the
//!   same file (see [`fx_transpiler::MemoryLoader`]).
//! * `language` is `"fxc"` or `"fx"`; when absent it is inferred from `entry`'s
//!   extension, matching the language server.
//! * `mode` overrides a `#mode` header; `ascii` selects the ASCII output style;
//!   `optimize` (default true) is the transpiler's `Options::optimize`.
//!
//! # The response
//!
//! Every response has `ok`. A failure adds `error`:
//!
//! ```json
//! { "ok": false, "error": { "message": "…", "file": "lib/x.fxc",
//!                           "range": { "start": {"line":2,"character":4}, … } } }
//! ```
//!
//! Positions are LSP-style — **0-based** line and character — because the
//! consumers are editors and Monaco, both of which are 0-based. This is the one
//! place the API deliberately disagrees with the CLI, which prints 1-based
//! columns for humans.

use std::path::{Path, PathBuf};

use casio_fx50fh2::token::VarName;
use casio_fx50fh2::{Environment, Interpreter, MockHost, Mode, Value, compile_with};
use fx_lsp::logic::{self, Language};
use fx_transpiler::json::{self, Json};
use fx_transpiler::{FileLoader, MemoryLoader, Options};
use lsp_types::{Diagnostic, DocumentSymbol, Position};

/// Handle one request and return one response, both JSON text.
///
/// This never fails: a malformed request produces a JSON error response, because
/// a host calling across the wasm boundary has no other way to be told.
pub fn call(request: &str) -> String {
    let response = match json::parse(request) {
        Ok(value) => dispatch(&value),
        Err(e) => failure(format!(
            "malformed request JSON at line {}, column {}: {}",
            e.line, e.column, e.message
        )),
    };
    json::to_string(&response)
}

/// The operations, by name. Unknown names are reported rather than ignored, so a
/// typo in a web page is visible instead of silently doing nothing.
fn dispatch(request: &Json) -> Json {
    let Some(op) = request.get("op").and_then(Json::as_str) else {
        return failure("request has no `op` string".to_string());
    };
    match op {
        "version" => version(),
        "transpile" => transpile(request),
        "run" => run(request),
        "tests" => tests(request),
        "diagnostics" => diagnostics(request),
        "completions" => completions(request),
        "hover" => hover(request),
        "symbols" => symbols(request),
        "constants" => constants(),
        "eval" => eval(request),
        other => failure(format!(
            "unknown op `{other}`; expected one of version, transpile, run, tests, \
             diagnostics, completions, hover, symbols, constants, eval"
        )),
    }
}

// ---------------------------------------------------------------------------
// Request helpers

/// The document to work on: its text, its path, and every file supplied.
///
/// The text comes from `source` when present, and otherwise from `entry` inside
/// `files` — which lets a page hold a whole project and name the file to build.
fn entry(request: &Json) -> Result<(String, PathBuf, MemoryLoader), String> {
    let files = loader(request);
    let root = PathBuf::from(
        request
            .get("entry")
            .and_then(Json::as_str)
            .unwrap_or("main.fxc"),
    );
    let source = match request.get("source").and_then(Json::as_str) {
        Some(text) => text.to_string(),
        None => files.read(&root).map_err(|e| {
            format!(
                "no `source` was given, and `{}` is not among the supplied files: {e}",
                root.display()
            )
        })?,
    };
    Ok((source, root, files))
}

/// The `files` map, as an in-memory replacement for the filesystem.
fn loader(request: &Json) -> MemoryLoader {
    let mut files = MemoryLoader::new();
    if let Some(Json::Object(entries)) = request.get("files") {
        for (path, value) in entries {
            if let Some(text) = value.as_str() {
                files.insert(path.as_str(), text);
            }
        }
    }
    files
}

/// The directory relative includes resolve against: the entry's own directory,
/// unless `base` says otherwise. Agrees with `transpile_file` on disk.
fn base_dir(request: &Json, root: &Path) -> PathBuf {
    if let Some(base) = request.get("base").and_then(Json::as_str) {
        return PathBuf::from(base);
    }
    match root.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

/// Which language the document is in: `language`, else the entry's extension.
fn language(request: &Json, root: &Path) -> Language {
    request
        .get("language")
        .and_then(Json::as_str)
        .and_then(Language::from_id)
        .unwrap_or_else(|| Language::from_path(&root.to_string_lossy()))
}

/// The transpiler options a request asks for.
fn options(request: &Json) -> Options {
    let mut options = Options::default();
    if let Some(ascii) = request.get("ascii").and_then(Json::as_bool) {
        options.ascii = ascii;
    }
    if let Some(optimize) = request.get("optimize").and_then(Json::as_bool) {
        options.optimize = optimize;
    }
    if let Some(mode) = request.get("mode").and_then(Json::as_str) {
        options.mode = fx_transpiler::Mode::parse(mode);
    }
    options
}

/// The calculator mode a request forces, if any.
fn forced_mode(request: &Json) -> Option<Mode> {
    request
        .get("mode")
        .and_then(Json::as_str)
        .and_then(Mode::parse)
}

/// The `?` inputs a request supplies.
///
/// The calculator's `?` reads one real number, so only numbers and strings that
/// parse as numbers are accepted; anything else is skipped rather than turned
/// into a surprising `NaN`.
fn inputs(request: &Json) -> Vec<f64> {
    let Some(items) = request.get("inputs").and_then(Json::as_array) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| match item {
            Json::Number(number) => Some(*number),
            Json::String(text) => text.trim().parse().ok(),
            _ => None,
        })
        .collect()
}

fn position(request: &Json) -> Option<Position> {
    let at = request.get("position")?;
    Some(Position {
        line: at.get("line")?.as_f64()? as u32,
        character: at.get("character")?.as_f64()? as u32,
    })
}

// ---------------------------------------------------------------------------
// Response helpers

fn success(entries: impl IntoIterator<Item = (&'static str, Json)>) -> Json {
    Json::object(std::iter::once(("ok", Json::Bool(true))).chain(entries))
}

/// A failure with no position — a bad request, or an operation that could not
/// start.
fn failure(message: impl Into<String>) -> Json {
    Json::object([
        ("ok", Json::Bool(false)),
        (
            "error",
            Json::object([("message", Json::string(message.into()))]),
        ),
    ])
}

/// A failure at a position, as an editor would report it.
fn failure_at(message: impl Into<String>, file: Option<&str>, range: Json) -> Json {
    Json::object([
        ("ok", Json::Bool(false)),
        (
            "error",
            Json::object([
                ("message", Json::string(message.into())),
                ("file", file.map_or(Json::Null, Json::string)),
                ("range", range),
            ]),
        ),
    ])
}

fn position_json(at: Position) -> Json {
    Json::object([
        ("line", Json::Number(at.line as f64)),
        ("character", Json::Number(at.character as f64)),
    ])
}

fn range_json(range: lsp_types::Range) -> Json {
    Json::object([
        ("start", position_json(range.start)),
        ("end", position_json(range.end)),
    ])
}

fn severity_json(severity: Option<lsp_types::DiagnosticSeverity>) -> Json {
    // `lsp-types` makes these newtypes over the wire number but renders them in
    // `Debug` as their PascalCase names, which is both what a page wants to read
    // and more portable than the LSP integers — Monaco's own marker severities
    // are numbered differently (Hint=1, Error=8), so a string cannot be
    // misread the way a bare number can.
    Json::string(
        severity
            .map(|severity| format!("{severity:?}").to_ascii_lowercase())
            .unwrap_or_else(|| "error".to_string()),
    )
}

fn diagnostic_json(diagnostic: &Diagnostic) -> Json {
    Json::object([
        ("message", Json::string(diagnostic.message.as_str())),
        ("severity", severity_json(diagnostic.severity)),
        ("range", range_json(diagnostic.range)),
        (
            "code",
            match &diagnostic.code {
                Some(lsp_types::NumberOrString::String(code)) => Json::string(code.as_str()),
                Some(lsp_types::NumberOrString::Number(code)) => Json::Number(*code as f64),
                None => Json::Null,
            },
        ),
    ])
}

/// A failed transpile, positioned for an editor.
fn transpile_failure(source: &str, error: &fx_transpiler::error::TranspileError) -> Json {
    let diagnostic = logic::transpile_diagnostic(source, error);
    failure_at(
        error.message.clone(),
        error.file.as_deref(),
        range_json(diagnostic.range),
    )
}

/// A failed run, positioned for an editor.
fn calc_failure(source: &str, error: &casio_fx50fh2::CalcError) -> Json {
    let diagnostic = logic::diagnostic(source, error);
    failure_at(error.to_string(), None, range_json(diagnostic.range))
}

// ---------------------------------------------------------------------------
// Operations

/// What this build is, and the limits of the machine it models.
///
/// A page needs the limits to render "412 of 680 bytes" without hard-coding
/// numbers that belong to the emulator, and the version to tell a stale cached
/// wasm module from a fresh one.
fn version() -> Json {
    success([
        ("version", Json::string(env!("CARGO_PKG_VERSION"))),
        (
            "modes",
            Json::array(
                [Mode::Comp, Mode::Cmplx, Mode::Base, Mode::Sd, Mode::Reg]
                    .into_iter()
                    .map(|mode| {
                        Json::object([
                            ("name", Json::string(mode.name())),
                            ("description", Json::string(describe_mode(mode))),
                        ])
                    }),
            ),
        ),
        (
            "limits",
            Json::object([
                (
                    "programKeys",
                    Json::Number(fx_transpiler::Size::CAPACITY as f64),
                ),
                ("memories", Json::Number(7.0)),
                (
                    "constants",
                    Json::Number(casio_fx50fh2::CONSTANTS.len() as f64),
                ),
            ]),
        ),
    ])
}

fn describe_mode(mode: Mode) -> &'static str {
    match mode {
        Mode::Comp => "general computation; real numbers only",
        Mode::Cmplx => "complex numbers",
        Mode::Base => "base-n (integer) computation",
        Mode::Sd => "single-variable statistics",
        Mode::Reg => "paired-variable statistics and regression",
    }
}

/// `.fxc` → PRGM, plus what the result costs and where it puts things.
///
/// Size is reported alongside the *unoptimised* size, as `fx50 size` does: a
/// program's size only means something next to what it would have been on this
/// machine, where 680 bytes are shared by all four program areas.
fn transpile(request: &Json) -> Json {
    let (source, root, files) = match entry(request) {
        Ok(parts) => parts,
        Err(message) => return failure(message),
    };
    let base = base_dir(request, &root);
    let options = options(request);

    let prgm =
        match fx_transpiler::transpile_with_loader(&source, options, Some(&root), &base, &files) {
            Ok(prgm) => prgm,
            Err(error) => return transpile_failure(&source, &error),
        };

    let mut fields = vec![
        ("prgm", Json::string(prgm.as_str())),
        (
            "size",
            size_json(&prgm, &source, options, &root, &base, &files),
        ),
    ];
    if let Some(plan) = regs_json(&source, &base, &files) {
        fields.push(("regs", plan));
    }
    success(fields)
}

/// The key cost of `prgm`, with the saving the optimiser made.
fn size_json(
    prgm: &str,
    source: &str,
    options: Options,
    root: &Path,
    base: &Path,
    files: &MemoryLoader,
) -> Json {
    let size = fx_transpiler::size::measure(prgm);
    let mut fields = vec![
        ("keys", Json::Number(size.keys as f64)),
        ("statements", Json::Number(size.statements as f64)),
        ("largest", Json::Number(size.largest as f64)),
        (
            "capacity",
            Json::Number(fx_transpiler::Size::CAPACITY as f64),
        ),
        ("fits", Json::Bool(size.fits())),
        (
            "remaining",
            match size.remaining() {
                Some(left) => Json::Number(left as f64),
                None => Json::Null,
            },
        ),
    ];

    // Only interesting when optimisation ran, and only when it is a fair
    // comparison — i.e. when the same source can be rebuilt without it.
    if options.optimize {
        let raw = Options {
            optimize: false,
            ..options
        };
        if let Ok(unoptimized) =
            fx_transpiler::transpile_with_loader(source, raw, Some(root), base, files)
        {
            let raw_size = fx_transpiler::size::measure(&unoptimized);
            fields.push(("unoptimizedKeys", Json::Number(raw_size.keys as f64)));
            fields.push((
                "savedKeys",
                Json::Number(raw_size.keys.saturating_sub(size.keys) as f64),
            ));
        }
    }
    Json::object(fields)
}

/// The memory plan, or `None` when the program cannot be analysed.
///
/// `transpile` has already succeeded by the time this runs, and analysis mirrors
/// the transpiler's front end exactly, so a failure here would be a bug —
/// reporting `null` keeps a page working rather than turning one into an error.
fn regs_json(source: &str, base: &Path, files: &MemoryLoader) -> Option<Json> {
    let analysis = fx_transpiler::analyze_with_loader(source, base, files).ok()?;
    let memories = analysis
        .allocation
        .registers
        .iter()
        .map(|(memory, occupants)| {
            Json::object([
                ("memory", Json::string(memory.to_string())),
                (
                    "holders",
                    Json::array(occupants.iter().map(|b| Json::string(b.label()))),
                ),
            ])
        })
        .collect::<Vec<_>>();
    Some(Json::object([
        ("memories", Json::array(memories)),
        ("used", Json::Number(analysis.allocation.used() as f64)),
        (
            "free",
            Json::array(
                analysis
                    .allocation
                    .free()
                    .into_iter()
                    .map(|memory| Json::string(memory.to_string())),
            ),
        ),
        (
            "bindings",
            Json::array(analysis.allocation.bindings.iter().map(|binding| {
                Json::object([
                    ("name", Json::string(binding.label())),
                    ("memory", Json::string(binding.memory.to_string())),
                ])
            })),
        ),
        (
            "freed",
            Json::array(
                analysis
                    .allocation
                    .freed
                    .iter()
                    .map(|name| Json::string(name.as_str())),
            ),
        ),
        (
            "consts",
            Json::array(
                analysis
                    .consts
                    .iter()
                    .map(|name| Json::string(name.as_str())),
            ),
        ),
        (
            "data",
            Json::array(analysis.data.iter().map(|name| Json::string(name.as_str()))),
        ),
    ]))
}

/// Transpile if needed, run, and report what the calculator would show.
fn run(request: &Json) -> Json {
    let (source, root, files) = match entry(request) {
        Ok(parts) => parts,
        Err(message) => return failure(message),
    };
    let base = base_dir(request, &root);
    let options = options(request);
    let language = language(request, &root);

    // A `.fxc` program is lowered first; a PRGM program is already what the
    // machine runs. `language` decides, exactly as `fx50 run` decides by
    // extension.
    let (prgm, transpiled) = match language {
        Language::Fxc => {
            match fx_transpiler::transpile_with_loader(&source, options, Some(&root), &base, &files)
            {
                Ok(prgm) => (prgm, true),
                Err(error) => return transpile_failure(&source, &error),
            }
        }
        Language::Prgm => (source.clone(), false),
    };

    // A `.fxc` file with no `fn main()` is a library: it builds — to nothing —
    // so it can be checked and edited on its own, but there is nothing to run.
    if transpiled && prgm.trim().is_empty() {
        return failure(format!(
            "nothing to run: `{}` has no `fn main()`; it defines functions for another \
             program to `#include`",
            root.display()
        ));
    }

    let program = match compile_with(&prgm, forced_mode(request)) {
        Ok(program) => program,
        Err(error) => return calc_failure(&prgm, &error),
    };
    let mut interpreter = Interpreter::new(program, MockHost::with_inputs(inputs(request)));
    if let Err(error) = interpreter.run() {
        return calc_failure(&prgm, &error);
    }

    success([
        ("prgm", Json::string(prgm.as_str())),
        ("transpiled", Json::Bool(transpiled)),
        (
            "outputs",
            Json::array(
                interpreter
                    .host()
                    .output
                    .iter()
                    .map(|line| Json::string(line.as_str())),
            ),
        ),
        ("state", state_json(interpreter.environment())),
        (
            "size",
            size_json(&prgm, &source, options, &root, &base, &files),
        ),
    ])
}

/// The calculator's display and memories after a run.
///
/// This is the "screen" a page draws beside the program: the memories the
/// program used, the value in `Ans`, and the display settings that decide how
/// numbers are rendered.
fn state_json(environment: &Environment) -> Json {
    let memories = [
        VarName::A,
        VarName::B,
        VarName::C,
        VarName::D,
        VarName::X,
        VarName::Y,
        VarName::M,
    ];
    Json::object([
        ("ans", value_json(environment.ans_value(), environment)),
        (
            "memories",
            Json::object(memories.into_iter().map(|var| {
                let value = environment.get_value(var);
                (format!("{var:?}"), value_json(value, environment))
            })),
        ),
        ("mode", Json::string(environment.mode.name())),
        ("angle", Json::string(format!("{:?}", environment.angle))),
        (
            "display",
            Json::string(format!("{:?}", environment.display)),
        ),
        (
            "base",
            match environment.base {
                Some(base) => Json::string(format!("{base:?}")),
                None => Json::Null,
            },
        ),
    ])
}

/// One value, both as the calculator would display it and as numbers.
///
/// The display string is what the hardware shows — including its 10-digit
/// rounding and its sexagesimal and `Re⇔Im` forms — so a page can show the real
/// thing rather than a reinterpretation of it. The numeric parts are there for
/// plotting and for tests that want to compare numbers.
fn value_json(value: Value, environment: &Environment) -> Json {
    Json::object([
        ("display", Json::string(environment.format_value(value))),
        ("re", number_json(value.re())),
        ("im", number_json(value.im())),
        ("complex", Json::Bool(value.is_complex())),
    ])
}

/// A number JSON can carry; a non-finite one becomes `null` rather than `inf`,
/// which is not JSON.
fn number_json(value: f64) -> Json {
    if value.is_finite() {
        Json::Number(value)
    } else {
        Json::Null
    }
}

/// Run a program's embedded `#tests` table.
///
/// The suite is parsed through the same loader as everything else, so a program
/// that `#include`s a library or reads a `#data` file can be tested in a browser.
fn tests(request: &Json) -> Json {
    let (source, root, files) = match entry(request) {
        Ok(parts) => parts,
        Err(message) => return failure(message),
    };
    let base = base_dir(request, &root);
    let name = root.display().to_string();

    let suite = match fx_transpiler::testing::parse_embedded_suite_with_loader(
        &source, &name, &base, &files,
    ) {
        Ok(Some(suite)) => suite,
        Ok(None) => {
            return failure(format!(
                "`{name}` has no `#tests` table; add one, or supply a suite another way"
            ));
        }
        Err(error) => return failure(error.to_string()),
    };

    let report = fx_transpiler::testing::run_suite_with_loader(&suite, &files);
    let cases = report.cases.iter().map(|case| {
        Json::object([
            ("name", Json::string(case.name.as_str())),
            ("passed", Json::Bool(case.passed)),
            ("expected", Json::string(case.expected.as_str())),
            ("actual", Json::string(case.actual.as_str())),
        ])
    });
    success([
        ("name", Json::string(report.name.as_str())),
        ("passed", Json::Number(report.passed() as f64)),
        ("failed", Json::Number(report.failed() as f64)),
        ("success", Json::Bool(report.is_success())),
        ("cases", Json::array(cases)),
    ])
}

/// Errors for the editor, through the same code the language server uses.
fn diagnostics(request: &Json) -> Json {
    let (source, root, files) = match entry(request) {
        Ok(parts) => parts,
        Err(message) => return failure(message),
    };
    let base = base_dir(request, &root);
    let language = language(request, &root);
    let diagnostics = logic::diagnostics_with_loader(&source, language, &base, &files);
    success([
        ("language", Json::string(language_id(language))),
        (
            "diagnostics",
            Json::array(diagnostics.iter().map(diagnostic_json)),
        ),
    ])
}

/// The completion list for a language.
///
/// Completions do not depend on the document, so a page can fetch them once and
/// cache them for the session.
fn completions(request: &Json) -> Json {
    let root = PathBuf::from(
        request
            .get("entry")
            .and_then(Json::as_str)
            .unwrap_or("main.fxc"),
    );
    let language = language(request, &root);
    let items = logic::completion_items(language);
    success([
        ("language", Json::string(language_id(language))),
        (
            "items",
            Json::array(items.iter().map(|item| {
                Json::object([
                    ("label", Json::string(item.label.as_str())),
                    (
                        "insertText",
                        match &item.insert_text {
                            Some(text) => Json::string(text.as_str()),
                            None => Json::string(item.label.as_str()),
                        },
                    ),
                    (
                        "detail",
                        match &item.detail {
                            Some(detail) => Json::string(detail.as_str()),
                            None => Json::Null,
                        },
                    ),
                    ("kind", Json::string(format!("{:?}", item.kind))),
                ])
            })),
        ),
    ])
}

/// Documentation at a position, through the language server's own hover.
fn hover(request: &Json) -> Json {
    let (source, root, _files) = match entry(request) {
        Ok(parts) => parts,
        Err(message) => return failure(message),
    };
    let Some(at) = position(request) else {
        return failure("`hover` needs a `position` object with `line` and `character`");
    };
    let language = language(request, &root);
    let found = logic::hover(&source, at, language);
    success([(
        "hover",
        match found {
            Some(hover) => {
                let text = match hover.contents {
                    lsp_types::HoverContents::Markup(markup) => markup.value,
                    lsp_types::HoverContents::Scalar(lsp_types::MarkedString::String(text)) => text,
                    lsp_types::HoverContents::Scalar(lsp_types::MarkedString::LanguageString(
                        marked,
                    )) => marked.value,
                    lsp_types::HoverContents::Array(items) => items
                        .into_iter()
                        .map(|item| match item {
                            lsp_types::MarkedString::String(text) => text,
                            lsp_types::MarkedString::LanguageString(marked) => marked.value,
                        })
                        .collect::<Vec<_>>()
                        .join("\n\n"),
                };
                Json::object([
                    ("contents", Json::string(text)),
                    (
                        "range",
                        match hover.range {
                            Some(range) => range_json(range),
                            None => Json::Null,
                        },
                    ),
                ])
            }
            None => Json::Null,
        },
    )])
}

/// The document outline.
fn symbols(request: &Json) -> Json {
    let (source, root, _files) = match entry(request) {
        Ok(parts) => parts,
        Err(message) => return failure(message),
    };
    let language = language(request, &root);
    let symbols = logic::document_symbols(&source, language);
    success([("symbols", Json::array(symbols.iter().map(symbol_json)))])
}

fn symbol_json(symbol: &DocumentSymbol) -> Json {
    let children = match &symbol.children {
        Some(children) => Json::array(children.iter().map(symbol_json)),
        None => Json::Null,
    };
    Json::object([
        ("name", Json::string(symbol.name.as_str())),
        (
            "detail",
            match &symbol.detail {
                Some(detail) => Json::string(detail.as_str()),
                None => Json::Null,
            },
        ),
        ("kind", Json::string(format!("{:?}", symbol.kind))),
        ("range", range_json(symbol.range)),
        ("selectionRange", range_json(symbol.selection_range)),
        ("children", children),
    ])
}

/// The 40 scientific constants, for a reference panel.
fn constants() -> Json {
    success([(
        "constants",
        Json::array(casio_fx50fh2::CONSTANTS.iter().map(|constant| {
            Json::object([
                ("code", Json::Number(constant.code as f64)),
                ("name", Json::string(constant.name)),
                ("symbol", Json::string(constant.symbol)),
                ("value", number_json(constant.value)),
                ("unit", Json::string(constant.unit)),
                ("description", Json::string(constant.description)),
            ])
        })),
    )])
}

/// One expression, evaluated and displayed.
///
/// This is `fx50 eval`: the operator-precedence calculator, without a program
/// around it. It is what makes a page usable as a scratchpad.
fn eval(request: &Json) -> Json {
    let Some(expression) = request.get("source").and_then(Json::as_str) else {
        return failure("`eval` needs `source`, holding the expression");
    };
    let program = match compile_with(expression, forced_mode(request)) {
        Ok(program) => program,
        Err(error) => return calc_failure(expression, &error),
    };
    let mut interpreter = Interpreter::new(program, MockHost::default());
    if let Err(error) = interpreter.run() {
        return calc_failure(expression, &error);
    }
    success([
        (
            "outputs",
            Json::array(
                interpreter
                    .host()
                    .output
                    .iter()
                    .map(|line| Json::string(line.as_str())),
            ),
        ),
        ("state", state_json(interpreter.environment())),
    ])
}

/// The language id a client asked for, spelled canonically.
fn language_id(language: Language) -> &'static str {
    match language {
        Language::Prgm => "fx",
        Language::Fxc => "fxc",
    }
}

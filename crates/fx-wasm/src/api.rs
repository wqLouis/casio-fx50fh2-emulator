//! The request/response API, as ordinary Rust.
//!
//! Everything the wasm build can do is an operation on a JSON request, so the
//! whole surface is testable with plain `cargo test` on the host and
//! `lib.rs`/`abi.rs` stay a thin marshalling shell.
//!
//! The request shape is the *only* interface a host needs. It is deliberately
//! JSON rather than a set of typed exports, because the consumers are a web page
//! and an editor extension — neither of which can call into Rust types. That is
//! also why this crate needs no `wasm-bindgen` (ADR 0031): strings and numbers
//! cross the boundary, and nothing else.
//!
//! Both directions are `serde`: the request deserializes into `Request`, and
//! the responses are plain `Serialize` structs. The editor operations return the
//! *actual* `lsp_types` values that [`fx_lsp::logic`] produces, so the protocol's
//! own integer enums and field names travel unchanged. There is no hand-rolled
//! JSON here, and no hand-written LSP translation.
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
//! * `inputs` are the values for the calculator's `?` prompts. They are JSON
//!   **numbers only**: a numeric string is a type error, not a silently skipped
//!   input. The calculator's `?` reads a real number, so a caller holding text
//!   parses it on its own side where it can report the failure itself.
//! * `position` is an LSP `Position`: `{"line": 0, "character": 4}`.
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
//! `file` and `range` are present only when the failure has a position.
//! Positions are LSP-style — **0-based** line and character — because the
//! consumers are editors and Monaco, both of which are 0-based. This is the one
//! place the API deliberately disagrees with the CLI, which prints 1-based
//! columns for humans.
//!
//! A non-finite number (`inf`, `NaN`) is written by `serde_json` as `null`, so
//! the response is always parseable JSON. The `re`/`im`/`value` fields can
//! therefore be `null`.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use casio_fx50fh2::token::VarName;
use casio_fx50fh2::{Environment, Interpreter, MockHost, Mode, Value, compile_with};
use fx_lsp::logic::{self, Language};
use fx_transpiler::{FileLoader, MemoryLoader, Options};
use lsp_types::{CompletionItem, Diagnostic, DocumentSymbol, Hover, Position};
use serde::{Deserialize, Serialize};

/// The operations, named for the error a misspelled `op` produces.
const OPERATIONS: &str = "version, transpile, run, tests, diagnostics, completions, hover, \
                          symbols, constants, eval";

// ---------------------------------------------------------------------------
// The request

/// One operation, and the fields it may carry.
///
/// `serde` reads the `op` tag and the fields in one pass, so a missing `op`, an
/// unknown `op`, a field of the wrong type, and malformed JSON all surface as
/// ordinary deserialization errors rather than hand-written lookups. A field
/// the operation does not use is simply ignored, exactly as the JSON envelope
/// always allowed.
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
enum Request {
    Version,
    Transpile(Fields),
    Run(Fields),
    Tests(Fields),
    Diagnostics(Fields),
    Completions(Fields),
    Hover(Fields),
    Symbols(Fields),
    Constants,
    Eval(Fields),
}

/// The fields any operation might read.
///
/// Every field is optional because the envelope is shared: `source` may come
/// from `files`, `position` is only meaningful to `hover`, and so on. An
/// operation that *requires* one of them says so with its own message rather
/// than through the deserializer.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct Fields {
    source: Option<String>,
    entry: Option<String>,
    files: HashMap<String, String>,
    base: Option<String>,
    language: Option<String>,
    mode: Option<String>,
    ascii: Option<bool>,
    optimize: Option<bool>,
    inputs: Option<Vec<f64>>,
    position: Option<Position>,
}

/// Handle one request and return one response, both JSON text.
///
/// This never fails: a malformed request produces a JSON error response, because
/// a host calling across the wasm boundary has no other way to be told.
pub fn call(request: &str) -> String {
    match serde_json::from_str::<Request>(request) {
        Ok(request) => dispatch(request),
        Err(error) => request_error(&error),
    }
}

/// The operations, by name. Unknown names are reported rather than ignored, so a
/// typo in a web page is visible instead of silently doing nothing.
fn dispatch(request: Request) -> String {
    match request {
        Request::Version => version(),
        Request::Transpile(fields) => transpile(&fields),
        Request::Run(fields) => run(&fields),
        Request::Tests(fields) => tests(&fields),
        Request::Diagnostics(fields) => diagnostics(&fields),
        Request::Completions(fields) => completions(&fields),
        Request::Hover(fields) => hover(&fields),
        Request::Symbols(fields) => symbols(&fields),
        Request::Constants => constants(),
        Request::Eval(fields) => eval(&fields),
    }
}

/// Turn a deserialization failure into the API's own error response.
///
/// `serde` already knows the operation list and the field names; this only keeps
/// the wording a caller wrote by hand (`op`, not "variant") and separates the
/// three mistakes: a bad `op`, a wrong-typed field, and JSON that does not parse.
fn request_error(error: &serde_json::Error) -> String {
    let detail = error.to_string();
    if let Some(rest) = detail.strip_prefix("unknown variant ") {
        return failure(format!("unknown op {rest}"));
    }
    if detail.contains("missing field `op`") {
        return failure(format!(
            "request has no `op` string; expected one of {OPERATIONS}"
        ));
    }
    // A well-formed document with a wrong-typed field is a different mistake
    // from JSON that does not parse; name it that way.
    if error.is_data() {
        return failure(format!(
            "invalid request at line {}, column {}: {detail}",
            error.line(),
            error.column()
        ));
    }
    failure(format!(
        "malformed request JSON at line {}, column {}: {detail}",
        error.line(),
        error.column()
    ))
}

// ---------------------------------------------------------------------------
// Request helpers

/// The document to work on: its text, its path, and every file supplied.
///
/// The text comes from `source` when present, and otherwise from `entry` inside
/// `files` — which lets a page hold a whole project and name the file to build.
fn entry(fields: &Fields) -> Result<(String, PathBuf, MemoryLoader), String> {
    let files = loader(fields);
    let root = PathBuf::from(fields.entry.as_deref().unwrap_or("main.fxc"));
    let source = match &fields.source {
        Some(text) => text.clone(),
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
fn loader(fields: &Fields) -> MemoryLoader {
    let mut files = MemoryLoader::new();
    for (path, text) in &fields.files {
        files.insert(path, text);
    }
    files
}

/// The directory relative includes resolve against: the entry's own directory,
/// unless `base` says otherwise. Agrees with `transpile_file` on disk.
fn base_dir(fields: &Fields, root: &Path) -> PathBuf {
    if let Some(base) = &fields.base {
        return PathBuf::from(base);
    }
    match root.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

/// Which language the document is in: `language`, else the entry's extension.
fn language(fields: &Fields, root: &Path) -> Language {
    fields
        .language
        .as_deref()
        .and_then(Language::from_id)
        .unwrap_or_else(|| Language::from_path(&root.to_string_lossy()))
}

/// The transpiler options a request asks for.
fn options(fields: &Fields) -> Options {
    let mut options = Options::default();
    if let Some(ascii) = fields.ascii {
        options.ascii = ascii;
    }
    if let Some(optimize) = fields.optimize {
        options.optimize = optimize;
    }
    if let Some(mode) = &fields.mode {
        options.mode = fx_transpiler::Mode::parse(mode);
    }
    options
}

/// The calculator mode a request forces, if any.
fn forced_mode(fields: &Fields) -> Option<Mode> {
    fields.mode.as_deref().and_then(Mode::parse)
}

/// The `?` inputs a request supplies. Numbers only; see the module docs.
fn inputs(fields: &Fields) -> Vec<f64> {
    fields.inputs.clone().unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Response envelope

/// The `{"ok":true, …}` half of the envelope, with the operation's own fields
/// flattened in beside `ok`.
#[derive(Serialize)]
struct Success<T> {
    ok: bool,
    #[serde(flatten)]
    data: T,
}

/// The `{"ok":false,"error":{…}}` half.
#[derive(Serialize)]
struct FailureEnvelope {
    ok: bool,
    error: FailureBody,
}

/// Why a request failed. `file`/`range` are omitted when there is no position.
#[derive(Serialize)]
struct FailureBody {
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    range: Option<lsp_types::Range>,
}

fn success<T: Serialize>(data: T) -> String {
    encode(Success { ok: true, data })
}

/// A failure with no position — a bad request, or an operation that could not
/// start.
fn failure(message: impl Into<String>) -> String {
    encode(FailureEnvelope {
        ok: false,
        error: FailureBody {
            message: message.into(),
            file: None,
            range: None,
        },
    })
}

/// A failure at a position, as an editor would report it.
fn failure_at(message: impl Into<String>, file: Option<&str>, range: lsp_types::Range) -> String {
    encode(FailureEnvelope {
        ok: false,
        error: FailureBody {
            message: message.into(),
            file: file.map(str::to_string),
            range: Some(range),
        },
    })
}

/// Serialize a response.
///
/// These shapes cannot fail — `serde_json` writes a non-finite `f64` as `null`
/// rather than rejecting it — but the ABI must never see a trap, so a
/// hypothetical failure is reported through the same envelope.
fn encode<T: Serialize>(value: T) -> String {
    match serde_json::to_string(&value) {
        Ok(text) => text,
        Err(error) => {
            let fallback = FailureEnvelope {
                ok: false,
                error: FailureBody {
                    message: format!("could not encode response: {error}"),
                    file: None,
                    range: None,
                },
            };
            // `to_string` on this shape is infallible; the last resort is still
            // JSON so the host can parse it.
            serde_json::to_string(&fallback).unwrap_or_else(|_| r#"{"ok":false}"#.to_string())
        }
    }
}

/// A failed transpile, positioned for an editor.
fn transpile_failure(source: &str, error: &fx_transpiler::error::TranspileError) -> String {
    let diagnostic = logic::transpile_diagnostic(source, error);
    failure_at(
        error.message.clone(),
        error.file.as_deref(),
        diagnostic.range,
    )
}

/// A failed run, positioned for an editor.
fn calc_failure(source: &str, error: &casio_fx50fh2::CalcError) -> String {
    let diagnostic = logic::diagnostic(source, error);
    failure_at(error.to_string(), None, diagnostic.range)
}

// ---------------------------------------------------------------------------
// Response shapes
//
// The four editor operations return real `lsp_types` values directly. The rest
// carry data from crates that deliberately do not derive `Serialize`, so these
// structs are a thin mapping from those types — not a second model of them.

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VersionInfo {
    version: &'static str,
    modes: Vec<ModeInfo>,
    limits: Limits,
}

#[derive(Serialize)]
struct ModeInfo {
    name: &'static str,
    description: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Limits {
    program_keys: usize,
    memories: usize,
    constants: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TranspileInfo {
    prgm: String,
    size: SizeInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    regs: Option<MemoryPlan>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SizeInfo {
    keys: usize,
    statements: usize,
    largest: usize,
    capacity: usize,
    fits: bool,
    remaining: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    unoptimized_keys: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    saved_keys: Option<usize>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MemoryPlan {
    memories: Vec<MemorySlot>,
    used: usize,
    free: Vec<String>,
    bindings: Vec<BindingInfo>,
    freed: Vec<String>,
    consts: Vec<String>,
    data: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MemorySlot {
    memory: String,
    holders: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BindingInfo {
    name: String,
    memory: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RunInfo {
    prgm: String,
    transpiled: bool,
    outputs: Vec<String>,
    state: StateInfo,
    size: SizeInfo,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StateInfo {
    ans: ValueInfo,
    memories: BTreeMap<String, ValueInfo>,
    mode: String,
    angle: String,
    display: String,
    base: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ValueInfo {
    display: String,
    re: f64,
    im: f64,
    complex: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TestsInfo {
    name: String,
    passed: usize,
    failed: usize,
    success: bool,
    cases: Vec<CaseInfo>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CaseInfo {
    name: String,
    passed: bool,
    expected: String,
    actual: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticsInfo {
    language: &'static str,
    diagnostics: Vec<Diagnostic>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CompletionsInfo {
    language: &'static str,
    items: Vec<CompletionItem>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HoverInfo {
    hover: Option<Hover>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SymbolsInfo {
    symbols: Vec<DocumentSymbol>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConstantsInfo {
    constants: Vec<ConstantInfo>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConstantInfo {
    code: u8,
    name: &'static str,
    symbol: &'static str,
    value: f64,
    unit: &'static str,
    description: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EvalInfo {
    outputs: Vec<String>,
    state: StateInfo,
}

// ---------------------------------------------------------------------------
// Operations

/// What this build is, and the limits of the machine it models.
///
/// A page needs the limits to render "412 of 680 bytes" without hard-coding
/// numbers that belong to the emulator, and the version to tell a stale cached
/// wasm module from a fresh one.
fn version() -> String {
    success(VersionInfo {
        version: env!("CARGO_PKG_VERSION"),
        modes: [Mode::Comp, Mode::Cmplx, Mode::Base, Mode::Sd, Mode::Reg]
            .into_iter()
            .map(|mode| ModeInfo {
                name: mode.name(),
                description: describe_mode(mode),
            })
            .collect(),
        limits: Limits {
            program_keys: fx_transpiler::Size::CAPACITY,
            memories: 7,
            constants: casio_fx50fh2::CONSTANTS.len(),
        },
    })
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
fn transpile(fields: &Fields) -> String {
    let (source, root, files) = match entry(fields) {
        Ok(parts) => parts,
        Err(message) => return failure(message),
    };
    let base = base_dir(fields, &root);
    let options = options(fields);

    let prgm =
        match fx_transpiler::transpile_with_loader(&source, options, Some(&root), &base, &files) {
            Ok(prgm) => prgm,
            Err(error) => return transpile_failure(&source, &error),
        };

    success(TranspileInfo {
        size: size_info(&prgm, &source, options, &root, &base, &files),
        regs: memory_plan(&source, &base, &files),
        prgm,
    })
}

/// The key cost of `prgm`, with the saving the optimiser made.
fn size_info(
    prgm: &str,
    source: &str,
    options: Options,
    root: &Path,
    base: &Path,
    files: &MemoryLoader,
) -> SizeInfo {
    let size = fx_transpiler::size::measure(prgm);
    let mut info = SizeInfo {
        keys: size.keys,
        statements: size.statements,
        largest: size.largest,
        capacity: fx_transpiler::Size::CAPACITY,
        fits: size.fits(),
        remaining: size.remaining(),
        unoptimized_keys: None,
        saved_keys: None,
    };

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
            info.unoptimized_keys = Some(raw_size.keys);
            info.saved_keys = Some(raw_size.keys.saturating_sub(size.keys));
        }
    }
    info
}

/// The memory plan, or `None` when the program cannot be analysed.
///
/// `transpile` has already succeeded by the time this runs, and analysis mirrors
/// the transpiler's front end exactly, so a failure here would be a bug —
/// reporting `null` keeps a page working rather than turning one into an error.
fn memory_plan(source: &str, base: &Path, files: &MemoryLoader) -> Option<MemoryPlan> {
    let analysis = fx_transpiler::analyze_with_loader(source, base, files).ok()?;
    let allocation = &analysis.allocation;
    Some(MemoryPlan {
        memories: allocation
            .registers
            .iter()
            .map(|(memory, occupants)| MemorySlot {
                memory: memory.to_string(),
                holders: occupants.iter().map(|b| b.label()).collect(),
            })
            .collect(),
        used: allocation.used(),
        free: allocation
            .free()
            .into_iter()
            .map(|m| m.to_string())
            .collect(),
        bindings: allocation
            .bindings
            .iter()
            .map(|binding| BindingInfo {
                name: binding.label(),
                memory: binding.memory.to_string(),
            })
            .collect(),
        freed: allocation.freed.clone(),
        consts: analysis.consts.clone(),
        data: analysis.data.clone(),
    })
}

/// Transpile if needed, run, and report what the calculator would show.
fn run(fields: &Fields) -> String {
    let (source, root, files) = match entry(fields) {
        Ok(parts) => parts,
        Err(message) => return failure(message),
    };
    let base = base_dir(fields, &root);
    let options = options(fields);
    let language = language(fields, &root);

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

    let program = match compile_with(&prgm, forced_mode(fields)) {
        Ok(program) => program,
        Err(error) => return calc_failure(&prgm, &error),
    };
    let mut interpreter = Interpreter::new(program, MockHost::with_inputs(inputs(fields)));
    if let Err(error) = interpreter.run() {
        return calc_failure(&prgm, &error);
    }

    success(RunInfo {
        size: size_info(&prgm, &source, options, &root, &base, &files),
        prgm,
        transpiled,
        outputs: interpreter.host().output.clone(),
        state: state_info(interpreter.environment()),
    })
}

/// The calculator's display and memories after a run.
///
/// This is the "screen" a page draws beside the program: the memories the
/// program used, the value in `Ans`, and the display settings that decide how
/// numbers are rendered.
fn state_info(environment: &Environment) -> StateInfo {
    let memories = [
        VarName::A,
        VarName::B,
        VarName::C,
        VarName::D,
        VarName::X,
        VarName::Y,
        VarName::M,
    ];
    StateInfo {
        ans: value_info(environment.ans_value(), environment),
        memories: memories
            .into_iter()
            .map(|var| {
                let value = environment.get_value(var);
                (format!("{var:?}"), value_info(value, environment))
            })
            .collect(),
        mode: environment.mode.name().to_string(),
        angle: format!("{:?}", environment.angle),
        display: format!("{:?}", environment.display),
        base: environment.base.map(|base| format!("{base:?}")),
    }
}

/// One value, both as the calculator would display it and as numbers.
///
/// The display string is what the hardware shows — including its 10-digit
/// rounding and its sexagesimal and `Re⇔Im` forms — so a page can show the real
/// thing rather than a reinterpretation of it. The numeric parts are there for
/// plotting and for tests that want to compare numbers; a non-finite one is
/// `null`, because JSON cannot carry `inf`.
fn value_info(value: Value, environment: &Environment) -> ValueInfo {
    ValueInfo {
        display: environment.format_value(value),
        re: value.re(),
        im: value.im(),
        complex: value.is_complex(),
    }
}

/// Run a program's embedded `#tests` table.
///
/// The suite is parsed through the same loader as everything else, so a program
/// that `#include`s a library or reads a `#data` file can be tested in a browser.
fn tests(fields: &Fields) -> String {
    let (source, root, files) = match entry(fields) {
        Ok(parts) => parts,
        Err(message) => return failure(message),
    };
    let base = base_dir(fields, &root);
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
    let passed = report.passed();
    let failed = report.failed();
    success(TestsInfo {
        success: report.is_success(),
        cases: report
            .cases
            .iter()
            .map(|case| CaseInfo {
                name: case.name.clone(),
                passed: case.passed,
                expected: case.expected.clone(),
                actual: case.actual.clone(),
            })
            .collect(),
        name: report.name,
        passed,
        failed,
    })
}

/// Errors for the editor, through the same code the language server uses.
fn diagnostics(fields: &Fields) -> String {
    let (source, root, files) = match entry(fields) {
        Ok(parts) => parts,
        Err(message) => return failure(message),
    };
    let base = base_dir(fields, &root);
    let language = language(fields, &root);
    success(DiagnosticsInfo {
        language: language.id(),
        diagnostics: logic::diagnostics_with_loader(&source, language, &base, &files),
    })
}

/// The completion list for a language.
///
/// Completions do not depend on the document, so a page can fetch them once and
/// cache them for the session.
fn completions(fields: &Fields) -> String {
    let root = PathBuf::from(fields.entry.as_deref().unwrap_or("main.fxc"));
    let language = language(fields, &root);
    success(CompletionsInfo {
        language: language.id(),
        items: logic::completion_items(language),
    })
}

/// Documentation at a position, through the language server's own hover.
fn hover(fields: &Fields) -> String {
    let (source, root, _files) = match entry(fields) {
        Ok(parts) => parts,
        Err(message) => return failure(message),
    };
    let Some(at) = fields.position else {
        return failure("`hover` needs a `position` object with `line` and `character`");
    };
    let language = language(fields, &root);
    success(HoverInfo {
        hover: logic::hover(&source, at, language),
    })
}

/// The document outline.
fn symbols(fields: &Fields) -> String {
    let (source, root, _files) = match entry(fields) {
        Ok(parts) => parts,
        Err(message) => return failure(message),
    };
    let language = language(fields, &root);
    success(SymbolsInfo {
        symbols: logic::document_symbols(&source, language),
    })
}

/// The 40 scientific constants, for a reference panel.
fn constants() -> String {
    success(ConstantsInfo {
        constants: casio_fx50fh2::CONSTANTS
            .iter()
            .map(|constant| ConstantInfo {
                code: constant.code,
                name: constant.name,
                symbol: constant.symbol,
                value: constant.value,
                unit: constant.unit,
                description: constant.description,
            })
            .collect(),
    })
}

/// One expression, evaluated and displayed.
///
/// This is `fx50 eval`: the operator-precedence calculator, without a program
/// around it. It is what makes a page usable as a scratchpad.
fn eval(fields: &Fields) -> String {
    let Some(expression) = fields.source.as_deref() else {
        return failure("`eval` needs `source`, holding the expression");
    };
    let program = match compile_with(expression, forced_mode(fields)) {
        Ok(program) => program,
        Err(error) => return calc_failure(expression, &error),
    };
    let mut interpreter = Interpreter::new(program, MockHost::default());
    if let Err(error) = interpreter.run() {
        return calc_failure(expression, &error);
    }
    success(EvalInfo {
        outputs: interpreter.host().output.clone(),
        state: state_info(interpreter.environment()),
    })
}

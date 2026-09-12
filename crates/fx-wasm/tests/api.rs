//! Tests for the request/response API, run on the host.
//!
//! These are the real tests for the browser build. `fx-wasm` compiles to
//! `wasm32-unknown-unknown`, where `std::fs` does not work, so anything that
//! passes here with no filesystem passes there too — and a failure shows up in
//! `cargo test` rather than only inside a browser.
//!
//! `abi.rs` has its own tests for the pointer marshalling; this file is about
//! behaviour.

use fx_wasm::api;
use serde_json::Value;

/// The tests speak in `serde_json` values now that the API does. `Value` has the
/// same `as_str`/`as_bool`/`as_f64`/`as_array` accessors the old reader had, so
/// the assertions read almost the same.
type Json = Value;

/// Build a JSON object from string keys, mirroring the old `Json::object`.
fn object<const N: usize>(entries: [(&str, Json); N]) -> Json {
    Value::Object(
        entries
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    )
}

/// Build a JSON string.
fn string(text: impl Into<String>) -> Json {
    Value::String(text.into())
}

/// Build a JSON array.
fn array<const N: usize>(items: [Json; N]) -> Json {
    Value::Array(items.into_iter().collect())
}

/// Send one request and return the parsed response.
#[track_caller]
fn call(request: Json) -> Json {
    let response = api::call(&serde_json::to_string(&request).expect("request serializes"));
    serde_json::from_str(&response)
        .unwrap_or_else(|e| panic!("response was not JSON: {e}\n{response}"))
}

/// Send one request, expecting success, and return the response.
#[track_caller]
fn ok(request: Json) -> Json {
    let response = call(request);
    assert_eq!(
        response.get("ok").and_then(Json::as_bool),
        Some(true),
        "expected success, got {}",
        serde_json::to_string_pretty(&response).unwrap_or_default()
    );
    response
}

/// Send one request, expecting failure, and return the error message.
#[track_caller]
fn err(request: Json) -> String {
    let response = call(request);
    assert_eq!(
        response.get("ok").and_then(Json::as_bool),
        Some(false),
        "expected failure, got {}",
        serde_json::to_string_pretty(&response).unwrap_or_default()
    );
    response
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(Json::as_str)
        .expect("a failure carries a message")
        .to_string()
}

/// A request with an op and a source, which is the common shape.
fn request(op: &str, source: &str) -> Json {
    object([
        ("op", string(op)),
        ("source", string(source)),
        ("entry", string("main.fxc")),
    ])
}

fn text(response: &Json, key: &str) -> String {
    response
        .get(key)
        .and_then(Json::as_str)
        .unwrap_or_else(|| panic!("no string `{key}` in {response:?}"))
        .to_string()
}

fn number(response: &Json, key: &str) -> f64 {
    response
        .get(key)
        .and_then(Json::as_f64)
        .unwrap_or_else(|| panic!("no number `{key}` in {response:?}"))
}

/// The strings of an array field.
fn strings(response: &Json, key: &str) -> Vec<String> {
    response
        .get(key)
        .and_then(Json::as_array)
        .unwrap_or_else(|| panic!("no array `{key}` in {response:?}"))
        .iter()
        .map(|item| item.as_str().unwrap_or_default().to_string())
        .collect()
}

// ---------------------------------------------------------------------------
// The envelope

#[test]
fn an_unknown_op_is_reported_with_the_list_of_known_ones() {
    let message = err(object([("op", string("nope"))]));
    assert!(message.contains("unknown op `nope`"), "{message}");
    assert!(message.contains("transpile"), "{message}");
}

#[test]
fn a_request_without_an_op_is_reported() {
    let message = err(object([("source", string("1"))]));
    assert!(message.contains("no `op`"), "{message}");
}

#[test]
fn a_malformed_request_names_the_position_of_the_problem() {
    let raw = api::call("{\"op\": }");
    assert!(raw.contains("\"ok\":false"), "{raw}");
    assert!(raw.contains("malformed"), "{raw}");
    // The JSON reader reports a line and column; a page can show them.
    assert!(raw.contains("line"), "{raw}");
}

#[test]
fn version_reports_the_machines_limits() {
    let response = ok(object([("op", string("version"))]));
    let limits = response.get("limits").expect("limits");
    assert_eq!(number(limits, "programKeys"), 680.0);
    assert_eq!(number(limits, "memories"), 7.0);
    assert_eq!(number(limits, "constants"), 40.0);
    let modes = response
        .get("modes")
        .and_then(Json::as_array)
        .expect("modes");
    assert_eq!(modes.len(), 5);
}

// ---------------------------------------------------------------------------
// transpile

#[test]
fn transpile_lowers_a_program_and_reports_its_size() {
    let response = ok(request("transpile", "fn main() { print(2 + 2); }"));
    let prgm = text(&response, "prgm");
    assert!(prgm.contains('◢'), "{prgm}");
    // `2 + 2` folds to `4`, so the program is two keys: the value and `◢`.
    let size = response.get("size").expect("size");
    assert_eq!(number(size, "keys"), 2.0);
    assert_eq!(number(size, "capacity"), 680.0);
    assert_eq!(size.get("fits").and_then(Json::as_bool), Some(true));
    assert_eq!(number(size, "remaining"), 678.0);
    // Constant folding is **not** one of the optional optimisations — the
    // emitter needs a `const`'s value — so there is nothing to save here and
    // the unoptimised build is the same size.
    assert_eq!(number(size, "unoptimizedKeys"), 2.0);
    assert_eq!(number(size, "savedKeys"), 0.0);
}

/// The saving is only worth reporting on a program the *optional* passes
/// change, which means `simplify`/`propagate` rather than `fold`.
#[test]
fn the_optimiser_reports_what_it_saved() {
    let source = "fn main() {
    let a = input();
    let b = 5;
    print(a + b);
    print(b * 2);
}";
    let response = ok(request("transpile", source));
    let prgm = text(&response, "prgm");
    // `b` is never assigned, so it is propagated into both uses and the memory
    // it would have occupied disappears.
    assert!(prgm.contains("A+5"), "{prgm}");
    assert!(!prgm.contains("\u{2192}B"), "{prgm}");
    let size = response.get("size").expect("size");
    assert_eq!(number(size, "keys"), 10.0);
    assert_eq!(number(size, "unoptimizedKeys"), 14.0);
    assert_eq!(number(size, "savedKeys"), 4.0);
}

#[test]
fn transpile_reports_the_memory_plan() {
    let response = ok(request(
        "transpile",
        "fn main() { let a = input(); let b = input(); print(a + b); }",
    ));
    let regs = response.get("regs").expect("regs");
    assert_eq!(number(regs, "used"), 2.0);
    let memories = regs
        .get("memories")
        .and_then(Json::as_array)
        .expect("memories");
    // All seven are listed, with the holders of the ones in use.
    assert_eq!(memories.len(), 7);
    let holders: Vec<String> = memories
        .iter()
        .flat_map(|memory| {
            memory
                .get("holders")
                .and_then(Json::as_array)
                .into_iter()
                .flatten()
                .map(|name| name.as_str().unwrap_or_default().to_string())
                .collect::<Vec<_>>()
        })
        .collect();
    assert_eq!(holders, vec!["a", "b"]);
}

#[test]
fn transpile_reads_included_libraries_from_the_request() {
    // The whole point: `#include` works with no filesystem.
    let response = ok(object([
        ("op", string("transpile")),
        ("entry", string("main.fxc")),
        (
            "source",
            string("#include \"lib/double.fxc\"\nfn main() { let a = input(); print(double(a)); }"),
        ),
        (
            "files",
            object([("lib/double.fxc", string("fn double(x) = x * 2;"))]),
        ),
    ]));
    let prgm = text(&response, "prgm");
    // `a` comes from `input()`, so it cannot be folded and the inlined
    // arithmetic survives into the output.
    assert!(
        prgm.contains("\u{00d7}2"),
        "the library should be inlined: {prgm}"
    );
    assert!(!prgm.contains("double"), "{prgm}");
}

#[test]
fn the_entry_document_can_come_from_the_files_map_instead_of_source() {
    // A page holding a whole project names the file to build rather than
    // passing its text separately.
    let response = ok(object([
        ("op", string("transpile")),
        ("entry", string("main.fxc")),
        (
            "files",
            object([
                (
                    "main.fxc",
                    string("#include \"lib/one.fxc\"\nfn main() { print(one()); }"),
                ),
                ("lib/one.fxc", string("fn one() = 1;")),
            ]),
        ),
    ]));
    assert!(text(&response, "prgm").contains('1'), "{response:?}");
}

#[test]
fn an_include_that_cannot_be_resolved_is_an_error_with_a_position() {
    let response = call(request(
        "transpile",
        "#include \"lib/missing.fxc\"\nfn main() {}",
    ));
    assert_eq!(response.get("ok").and_then(Json::as_bool), Some(false));
    let error = response.get("error").expect("error");
    let message = error.get("message").and_then(Json::as_str).unwrap_or("");
    assert!(message.contains("lib/missing.fxc"), "{message}");
    // Positions are 0-based, LSP-style, for the editor to use directly.
    let range = error.get("range").expect("range");
    assert_eq!(number(range.get("start").unwrap(), "line"), 0.0);
}

#[test]
fn a_syntax_error_points_at_the_offending_text() {
    let response = call(request("transpile", "fn main() { print(1 +); }"));
    let error = response.get("error").expect("error");
    let message = error.get("message").and_then(Json::as_str).unwrap_or("");
    assert!(!message.is_empty());
    let range = error.get("range").expect("range");
    // A single-line program can only fail on line 0.
    assert_eq!(number(range.get("start").unwrap(), "line"), 0.0);
}

#[test]
fn ascii_selects_the_other_output_style() {
    let glyph = ok(request(
        "transpile",
        "fn main() { let a = input(); print(a * 2); }",
    ));
    assert!(text(&glyph, "prgm").contains('×'));
    let ascii = ok(object([
        ("op", string("transpile")),
        (
            "source",
            string("fn main() { let a = input(); print(a * 2); }"),
        ),
        ("ascii", Value::Bool(true)),
    ]));
    assert!(text(&ascii, "prgm").contains('*'));
    assert!(!text(&ascii, "prgm").contains('×'));
}

#[test]
fn a_mode_override_is_honoured_and_enforced() {
    // `rep` needs CMPLX; asking for COMP must be refused rather than silently
    // emitting a key the mode does not have.
    let message = err(object([
        ("op", string("transpile")),
        ("source", string("fn main() { let z = 1; print(rep(z)); }")),
        ("mode", string("COMP")),
    ]));
    assert!(message.contains("CMPLX"), "{message}");

    let response = ok(object([
        ("op", string("transpile")),
        (
            "source",
            string("fn main() { let z = 1 + 2 * i(); print(rep(z)); }"),
        ),
        ("mode", string("CMPLX")),
    ]));
    assert!(text(&response, "prgm").contains("Conjg"), "{response:?}");
}

#[test]
fn optimize_false_gives_the_unpropagated_translation() {
    let source = "fn main() {
    let a = input();
    let b = 5;
    print(a + b);
    print(b * 2);
}";
    let optimized = ok(request("transpile", source));
    let raw = ok(object([
        ("op", string("transpile")),
        ("source", string(source)),
        ("optimize", Value::Bool(false)),
    ]));
    assert_eq!(number(optimized.get("size").unwrap(), "keys"), 10.0);
    assert_eq!(number(raw.get("size").unwrap(), "keys"), 14.0);
    // The unoptimised translation keeps `b` in a memory of its own.
    let raw_prgm = text(&raw, "prgm");
    assert!(raw_prgm.contains("\u{2192}B"), "{raw_prgm}");
    assert!(raw_prgm.contains("A+B"), "{raw_prgm}");
    // With optimisation off there is no saving to report against.
    assert!(raw.get("size").unwrap().get("unoptimizedKeys").is_none());
}

// ---------------------------------------------------------------------------
// run

#[test]
fn run_returns_the_displays_the_calculator_would_show() {
    let response = ok(request("run", "fn main() { print(2 + 2); print(10 / 4); }"));
    assert_eq!(strings(&response, "outputs"), vec!["4", "2.5"]);
    assert_eq!(
        response.get("transpiled").and_then(Json::as_bool),
        Some(true)
    );
}

#[test]
fn run_feeds_the_prompts_from_the_inputs_array() {
    let response = ok(object([
        ("op", string("run")),
        (
            "source",
            string("fn main() { let a = input(); let b = input(); print(a * b); }"),
        ),
        // Numbers only: a numeric string is a type error, not a skipped input.
        ("inputs", array([Value::from(6.0), Value::from(7.0)])),
    ]));
    assert_eq!(strings(&response, "outputs"), vec!["42"]);
}

#[test]
fn run_reports_the_memories_and_ans_as_the_display_would_show_them() {
    let response = ok(object([
        ("op", string("run")),
        (
            "source",
            string("fn main() { let a = input(); print(a + 1); }"),
        ),
        ("inputs", array([Value::from(41.0)])),
    ]));
    let state = response.get("state").expect("state");
    let memories = state.get("memories").expect("memories");
    let a = memories.get("A").expect("A");
    assert_eq!(text(a, "display"), "41");
    assert_eq!(number(a, "re"), 41.0);
    // `Ans` holds the last value the program computed.
    let ans = state.get("ans").expect("ans");
    assert_eq!(text(ans, "display"), "42");
}

#[test]
fn run_accepts_a_prgm_program_directly() {
    // `language: "fx"` means the text is already PRGM, so no transpiling and no
    // `fn main()` requirement.
    let response = ok(object([
        ("op", string("run")),
        ("source", string("?→A:A×2◢")),
        ("language", string("fx")),
        ("entry", string("prog.fx")),
        ("inputs", array([Value::from(21.0)])),
    ]));
    assert_eq!(strings(&response, "outputs"), vec!["42"]);
    assert_eq!(
        response.get("transpiled").and_then(Json::as_bool),
        Some(false)
    );
}

#[test]
fn a_library_without_main_says_so_instead_of_running_nothing() {
    let response = call(object([
        ("op", string("run")),
        ("source", string("fn double(x) = x * 2;")),
        ("entry", string("lib/double.fxc")),
    ]));
    assert_eq!(response.get("ok").and_then(Json::as_bool), Some(false));
    let error = response.get("error").expect("error");
    let message = error.get("message").and_then(Json::as_str).unwrap_or("");
    assert!(message.contains("no `fn main()`"), "{message}");
    assert!(message.contains("lib/double.fxc"), "{message}");
}

#[test]
fn a_runtime_error_is_reported_with_a_position() {
    // Divide by zero is the calculator's `Math ERROR`.
    let response = call(request("run", "fn main() { let a = 0; print(1 / a); }"));
    assert_eq!(response.get("ok").and_then(Json::as_bool), Some(false));
    let error = response.get("error").expect("error");
    let message = error.get("message").and_then(Json::as_str).unwrap_or("");
    assert!(message.to_ascii_lowercase().contains("math"), "{message}");
    assert!(error.get("range").is_some(), "{error:?}");
}

#[test]
fn a_complex_result_is_reported_both_ways() {
    let response = ok(object([
        ("op", string("run")),
        (
            "source",
            string("fn main() { let z = 3 + 4 * i(); print(rep(z)); print(imp(z)); }"),
        ),
        ("mode", string("CMPLX")),
    ]));
    assert_eq!(strings(&response, "outputs"), vec!["3", "4"]);
    let state = response.get("state").expect("state");
    let z = state.get("memories").unwrap().get("A").unwrap();
    // `A` still holds the complex `z`, displayed the way the screen would show
    // it — with the calculator's italic `𝑖`, not an ASCII `i`.
    assert_eq!(text(z, "display"), "3+4\u{1d456}");
    assert_eq!(number(z, "re"), 3.0);
    assert_eq!(number(z, "im"), 4.0);
    assert_eq!(z.get("complex").and_then(Json::as_bool), Some(true));
    // `Ans` is the last value the program computed, which was `imp(z)`. The
    // result of complex arithmetic stays a complex value even when its
    // imaginary part is zero, and the display still reads as the real `4`.
    let ans = state.get("ans").unwrap();
    assert_eq!(number(ans, "re"), 4.0);
    assert_eq!(text(ans, "display"), "4");
}

// ---------------------------------------------------------------------------
// tests

#[test]
fn embedded_tests_are_run_in_process() {
    let response = ok(request(
        "tests",
        r#"fn main() { let a = input(); print(a * 2); }
#tests = [
  { "name": "three", "input": [3], "output": ["6"] },
  { "name": "wrong", "input": [1], "output": ["99"] }
];"#,
    ));
    assert_eq!(number(&response, "passed"), 1.0);
    assert_eq!(number(&response, "failed"), 1.0);
    assert_eq!(response.get("success").and_then(Json::as_bool), Some(false));
    let cases = response
        .get("cases")
        .and_then(Json::as_array)
        .expect("cases");
    assert_eq!(cases.len(), 2);
    assert_eq!(text(&cases[0], "name"), "three");
    assert_eq!(cases[0].get("passed").and_then(Json::as_bool), Some(true));
    assert_eq!(text(&cases[1], "name"), "wrong");
    assert_eq!(cases[1].get("passed").and_then(Json::as_bool), Some(false));
    assert_eq!(text(&cases[1], "expected"), "99");
    assert_eq!(text(&cases[1], "actual"), "2");
}

#[test]
fn a_program_with_no_tests_says_so() {
    let message = err(request("tests", "fn main() { print(1); }"));
    assert!(message.contains("no `#tests` table"), "{message}");
}

// ---------------------------------------------------------------------------
// diagnostic publishing

#[test]
fn diagnostics_come_back_as_editor_markers() {
    let response = ok(request("diagnostics", "fn main() { print(1 +); }"));
    assert_eq!(text(&response, "language"), "fxc");
    let diagnostics = response
        .get("diagnostics")
        .and_then(Json::as_array)
        .expect("diagnostics");
    assert_eq!(diagnostics.len(), 1);
    let first = &diagnostics[0];
    // The protocol's own integer: `DiagnosticSeverity::ERROR` is 1. The old
    // shape invented the string `"error"`, which the page had to map back.
    assert_eq!(number(first, "severity"), 1.0);
    assert!(first.get("range").is_some(), "{first:?}");
}

#[test]
fn a_valid_program_has_no_diagnostics() {
    let response = ok(request("diagnostics", "fn main() { print(1); }"));
    assert!(strings(&response, "diagnostics").is_empty());
}

#[test]
fn diagnostics_resolve_includes_through_the_files_map() {
    // With no loader this would report a missing include that is not missing.
    let response = ok(object([
        ("op", string("diagnostics")),
        (
            "source",
            string("#include \"lib/one.fxc\"\nfn main() { print(one()); }"),
        ),
        ("files", object([("lib/one.fxc", string("fn one() = 1;"))])),
    ]));
    assert!(strings(&response, "diagnostics").is_empty(), "{response:?}");
}

#[test]
fn diagnostics_work_for_prgm_too() {
    let response = ok(object([
        ("op", string("diagnostics")),
        ("source", string("?→A:A×2◢")),
        ("language", string("fx")),
        ("entry", string("prog.fx")),
    ]));
    assert_eq!(text(&response, "language"), "fx");
    assert!(strings(&response, "diagnostics").is_empty());
}

// ---------------------------------------------------------------------------
// editor features

#[test]
fn completions_cover_both_languages() {
    let fxc = ok(object([
        ("op", string("completions")),
        ("language", string("fxc")),
    ]));
    let labels = strings_of_items(&fxc);
    for expected in ["sqrt(", "rep(", "phys.C0", "stat.sumx", "fn"] {
        assert!(
            labels.iter().any(|label| label.contains(expected)),
            "missing `{expected}`"
        );
    }

    let fx = ok(object([
        ("op", string("completions")),
        ("language", string("fx")),
    ]));
    let labels = strings_of_items(&fx);
    // PRGM-only keys are offered here and not for `.fxc`.
    assert!(labels.iter().any(|label| label == "While"), "{labels:?}");
    assert!(!labels.iter().any(|label| label == "sqrt("), "{labels:?}");
}

fn strings_of_items(response: &Json) -> Vec<String> {
    response
        .get("items")
        .and_then(Json::as_array)
        .expect("items")
        .iter()
        .map(|item| text(item, "label"))
        .collect()
}

#[test]
fn hover_describes_what_is_under_the_cursor() {
    // `wrap` is not used here, so line 0 is the body.
    let response = ok(object([
        ("op", string("hover")),
        ("source", string("fn main() { print(sqrt(4)); }")),
        ("language", string("fxc")),
        // `line`/`character` are the LSP `Position` integers (`u32`).
        (
            "position",
            object([("line", Value::from(0)), ("character", Value::from(18))]),
        ),
    ]));
    let hover = response.get("hover").expect("hover");
    // The real LSP `Hover`: `contents` is a `MarkupContent`, not a bare string.
    let contents = hover.get("contents").expect("contents");
    assert!(text(contents, "value").contains("sqrt"), "{hover:?}");
    assert_eq!(text(contents, "kind"), "markdown", "{hover:?}");
}

#[test]
fn hover_over_nothing_is_null_rather_than_an_error() {
    let response = ok(object([
        ("op", string("hover")),
        ("source", string("fn main() { }")),
        ("language", string("fxc")),
        (
            "position",
            object([("line", Value::from(0)), ("character", Value::from(14))]),
        ),
    ]));
    assert_eq!(response.get("hover"), Some(&Value::Null));
}

#[test]
fn hover_without_a_position_is_reported() {
    let message = err(request("hover", "fn main() { }"));
    assert!(message.contains("position"), "{message}");
}

#[test]
fn symbols_give_the_document_outline() {
    let response = ok(request(
        "symbols",
        "fn double(x) = x * 2;\nfn main() { print(double(2)); }",
    ));
    let symbols = response
        .get("symbols")
        .and_then(Json::as_array)
        .expect("symbols");
    let names: Vec<String> = symbols.iter().map(|s| text(s, "name")).collect();
    // A function's symbol is its signature.
    assert_eq!(names, vec!["double(x)", "main()"]);
}

// ---------------------------------------------------------------------------
// constants and eval

#[test]
fn constants_lists_all_forty_with_their_values() {
    let response = ok(object([("op", string("constants"))]));
    let constants = response
        .get("constants")
        .and_then(Json::as_array)
        .expect("constants");
    assert_eq!(constants.len(), 40);
    let speed_of_light = constants
        .iter()
        .find(|constant| text(constant, "name") == "C0")
        .expect("C0");
    assert_eq!(number(speed_of_light, "value"), 299_792_458.0);
    // Codes run 1..=40 in menu order.
    assert_eq!(number(&constants[0], "code"), 1.0);
    assert_eq!(number(&constants[39], "code"), 40.0);
}

#[test]
fn eval_works_on_an_expression_with_no_program_around_it() {
    let response = ok(request("eval", "2 + 3 × 4"));
    assert_eq!(strings(&response, "outputs"), vec!["14"]);
    let state = response.get("state").expect("state");
    assert_eq!(text(state.get("ans").unwrap(), "display"), "14");
}

#[test]
fn eval_honours_a_forced_mode() {
    let response = ok(object([
        ("op", string("eval")),
        ("source", string("(3 + 4i) × (1 - 2i)")),
        ("mode", string("CMPLX")),
    ]));
    // (3+4i)(1-2i) = 3 - 6i + 4i - 8i² = 11 - 2i
    assert_eq!(strings(&response, "outputs"), vec!["11-2\u{1d456}"]);
}

#[test]
fn eval_without_a_source_is_reported() {
    let message = err(object([("op", string("eval"))]));
    assert!(message.contains("needs `source`"), "{message}");
}

#[test]
fn a_failing_eval_reports_the_calculator_error() {
    let response = call(request("eval", "1 / 0"));
    assert_eq!(response.get("ok").and_then(Json::as_bool), Some(false));
    let message = response
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(Json::as_str)
        .unwrap_or("");
    assert!(message.to_ascii_lowercase().contains("math"), "{message}");
}

#[test]
fn the_response_is_always_parseable_json() {
    // A page calls `JSON.parse` on whatever comes back, so every path — success,
    // request error, program error — has to be valid JSON. A non-finite number
    // would break that, which is why the writer emits `null` for one.
    let cases: Vec<Json> = vec![
        object([("op", string("version"))]),
        object([("op", string("nope"))]),
        request("transpile", "fn main() { print(1); }"),
        request("transpile", "fn main() { print(1 +); }"),
        request("run", "fn main() { let a = 0; print(1 / a); }"),
        request("eval", "0 / 0"),
    ];
    for case in cases {
        let raw = api::call(&serde_json::to_string(&case).expect("request serializes"));
        serde_json::from_str::<Json>(&raw).unwrap_or_else(|e| panic!("not JSON ({e}): {raw}"));
        assert!(!raw.contains("NaN"), "{raw}");
        assert!(!raw.contains("inf"), "{raw}");
    }
}

/// A tested program whose `#include`d library is only in the files map.
///
/// The suite parser was made loader-aware before the runner was, which passed
/// locally and failed in wasm, so this pins the whole path.
#[test]
fn embedded_tests_may_include_a_library_from_the_files_map() {
    let response = ok(object([
        ("op", string("tests")),
        ("entry", string("main.fxc")),
        (
            "source",
            string(
                "#include \"lib/double.fxc\"\n\
                 fn main() { let a = input(); print(double(a)); }\n\
                 #tests = [ { \"name\": \"three\", \"input\": [3], \"output\": [\"6\"] } ];",
            ),
        ),
        (
            "files",
            object([("lib/double.fxc", string("fn double(x) = x * 2;"))]),
        ),
    ]));
    assert_eq!(number(&response, "passed"), 1.0);
    assert_eq!(number(&response, "failed"), 0.0);
}

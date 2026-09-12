//! End-to-end tests for transpiling with **no filesystem**: every file the
//! program refers to — `#include`d libraries and `#data` JSON values — is
//! supplied in memory through [`MemoryLoader`].
//!
//! This is the path a browser takes. `fx-wasm` compiles the same crates to
//! `wasm32-unknown-unknown`, where `std::fs` cannot work at all, so if these
//! tests pass the language's file-reading features survive the trip. The
//! `#include` tests in `include.rs` cover the on-disk behaviour; these cover the
//! identical behaviour with the disk removed.

use std::path::Path;

use fx_transpiler::{MemoryLoader, Options, transpile_with_loader};

mod common;

/// Transpile `source` as `main.fxc`, with `files` standing in for the disk.
fn transpile(source: &str, files: &MemoryLoader) -> Result<String, String> {
    let source = &common::wrap(source);
    transpile_with_loader(
        source,
        Options::default(),
        Some(Path::new("main.fxc")),
        Path::new("."),
        files,
    )
    .map_err(|e| e.to_string())
}

#[track_caller]
fn ok(source: &str, files: &MemoryLoader) -> String {
    transpile(source, files).unwrap_or_else(|e| panic!("transpile failed: {e}"))
}

#[track_caller]
fn err(source: &str, files: &MemoryLoader) -> String {
    match transpile(source, files) {
        Ok(prgm) => panic!("expected a failure, got:\n{prgm}"),
        Err(e) => e,
    }
}

#[test]
fn an_included_library_is_read_from_memory() {
    let mut files = MemoryLoader::new();
    files.insert("lib/double.fxc", "fn double(x) = x * 2;\n");
    let prgm = ok(
        "#include \"lib/double.fxc\"\nlet a = input();\nprint(double(a));\n",
        &files,
    );
    // The function is inlined, so the emitted program is the arithmetic itself.
    assert!(prgm.contains("×2"), "{prgm}");
    assert!(!prgm.contains("double"), "{prgm}");
}

#[test]
fn nested_includes_resolve_relative_to_the_including_file() {
    // `lib/outer.fxc` includes `inner.fxc`, which must be found in `lib/`, not
    // beside `main.fxc`. This is the case a naive "look it up by file name"
    // loader gets wrong.
    let mut files = MemoryLoader::new();
    files.insert("lib/inner.fxc", "fn inner(x) = x + 1;\n");
    files.insert(
        "lib/outer.fxc",
        "#include \"inner.fxc\"\nfn outer(x) = inner(x) * 10;\n",
    );
    let prgm = ok(
        "#include \"lib/outer.fxc\"\nlet a = input();\nprint(outer(a));\n",
        &files,
    );
    assert!(prgm.contains("+1"), "inner should be inlined: {prgm}");
    assert!(prgm.contains("×10"), "outer should be inlined: {prgm}");
}

#[test]
fn a_data_file_is_read_from_memory() {
    let mut files = MemoryLoader::new();
    files.insert("tables/weights.json", r#"{ "a": 12, "b": 30 }"#);
    files.insert("tables/more.json", r#"{ "a": 1, "b": 2 }"#);
    let prgm = ok(
        "#data weights = \"tables/weights.json\";\n\
         #data more = \"tables/more.json\";\n\
         print(weights.a + weights.b + more.b);\n",
        &files,
    );
    // The values are compile-time constants, so they fold into the output and
    // no memory is used.
    assert!(prgm.contains("\u{25e2}"), "expected a display: {prgm}");
    assert!(!prgm.contains("weights"), "data should be inlined: {prgm}");
}

#[test]
fn a_missing_include_says_which_file_was_wanted() {
    let files = MemoryLoader::new();
    let message = err("#include \"lib/nope.fxc\"\nlet a = 1;\nprint(a);\n", &files);
    assert!(message.contains("lib/nope.fxc"), "{message}");
    assert!(message.contains("was provided"), "{message}");
}

#[test]
fn a_missing_data_file_says_which_file_was_wanted() {
    let files = MemoryLoader::new();
    let message = err("#data t = \"nope.json\";\nprint(t.a);\n", &files);
    assert!(message.contains("nope.json"), "{message}");
}

#[test]
fn an_include_cycle_is_detected_without_a_filesystem() {
    let mut files = MemoryLoader::new();
    files.insert("a.fxc", "#include \"b.fxc\"\nfn a(x) = x;\n");
    files.insert("b.fxc", "#include \"a.fxc\"\nfn b(x) = x;\n");
    let message = err("#include \"a.fxc\"\nprint(a(1));\n", &files);
    assert!(message.contains("circular `#include`"), "{message}");
}

#[test]
fn a_cycle_is_detected_through_dot_dot_spellings() {
    // The same file reached by a different spelling must still be recognised as
    // the same file, which is why resolution is lexical rather than
    // filesystem-based.
    let mut files = MemoryLoader::new();
    files.insert("lib/a.fxc", "#include \"../lib/a.fxc\"\nfn a(x) = x;\n");
    let message = err("#include \"lib/a.fxc\"\nprint(a(1));\n", &files);
    assert!(message.contains("circular `#include`"), "{message}");
}

#[test]
fn the_root_document_may_be_keyed_with_a_leading_slash() {
    // A website might key its root as `/main.fxc` while resolving includes
    // relative to it; the loader must not care.
    let mut files = MemoryLoader::new();
    files.insert("/lib/one.fxc", "fn one() = 1;\n");
    let prgm = ok("#include \"lib/one.fxc\"\nprint(one());\n", &files);
    assert!(prgm.contains("1\u{25e2}"), "{prgm}");
}

#[test]
fn an_error_in_an_included_file_names_that_file() {
    let mut files = MemoryLoader::new();
    files.insert("lib/broken.fxc", "fn broken(x) = x +;\n");
    let message = err("#include \"lib/broken.fxc\"\nprint(broken(1));\n", &files);
    assert!(message.contains("broken.fxc"), "{message}");
}

#[test]
fn analysis_uses_the_loader_too() {
    // `fx50 regs` has to agree with `fx50 build`, in a browser as much as on a
    // command line.
    let mut files = MemoryLoader::new();
    files.insert("lib/one.fxc", "fn one() = 1;\n");
    let source = common::wrap("#include \"lib/one.fxc\"\nlet a = input();\nprint(one() + a);\n");
    let analysis = fx_transpiler::analyze_with_loader(&source, Path::new("."), &files)
        .unwrap_or_else(|e| panic!("analyze failed: {e}"));
    assert!(!analysis.allocation.bindings.is_empty(), "{analysis:?}");
    assert_eq!(analysis.allocation.used(), 1, "{analysis:?}");
}

/// A library `#include`d by a tested program must be read from memory too.
///
/// This is a regression test: the suite *parser* was made loader-aware first,
/// and the runner kept calling the filesystem, so a program with an `#include`
/// and a `#tests` table passed locally and failed in wasm with
/// "operation not supported on this platform".
///
/// `testing` is behind a feature (it needs the interpreter to run cases), so
/// this only exists when the feature is on — which is what keeps
/// `--no-default-features` dependency-free.
#[cfg(feature = "testing")]
#[test]
fn a_tested_program_can_include_a_library_from_memory() {
    use fx_transpiler::testing::{parse_embedded_suite_with_loader, run_suite_with_loader};

    let mut files = MemoryLoader::new();
    files.insert("lib/double.fxc", "fn double(x) = x * 2;\n");
    let source = "#include \"lib/double.fxc\"\n\
                  fn main() { let a = input(); print(double(a)); }\n\
                  #tests = [\n\
                    { \"name\": \"three\", \"input\": [3], \"output\": [\"6\"] },\n\
                    { \"name\": \"five\", \"input\": [5], \"output\": [\"10\"] }\n\
                  ];\n";

    let suite = parse_embedded_suite_with_loader(source, "main.fxc", Path::new("."), &files)
        .expect("the suite parses")
        .expect("it has a #tests table");
    let report = run_suite_with_loader(&suite, &files);

    assert_eq!(report.failed(), 0, "{:?}", report.cases);
    assert_eq!(report.passed(), 2);
}

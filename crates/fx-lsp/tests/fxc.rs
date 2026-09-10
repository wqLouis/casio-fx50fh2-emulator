//! Integration tests for the `.fxc` (C-like) half of `fx-lsp`.
//!
//! These drive the public [`fx_lsp::logic`] functions through the same entry
//! points the server uses, so they cover language selection, diagnostics
//! (including `#include` resolution), completion, hover and document symbols.

use std::path::{Path, PathBuf};

use fx_lsp::Language;
use fx_lsp::logic::{completion_items, diagnostics, document_symbols, hover, range_from_line_col};
use tower_lsp::lsp_types::{
    CompletionItemKind, Diagnostic, Hover, HoverContents, InsertTextFormat, NumberOrString,
    Position, Range,
};

/// A scratch directory that cleans itself up (the same pattern as the
/// transpiler's `tests/include.rs`).
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("fx-lsp-fxc-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        TempDir(dir)
    }

    fn write(&self, name: &str, content: &str) -> PathBuf {
        let path = self.0.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent dir");
        }
        std::fs::write(&path, content).expect("write temp file");
        path
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn code(diagnostic: &Diagnostic) -> String {
    match &diagnostic.code {
        Some(NumberOrString::String(code)) => code.clone(),
        other => panic!("unexpected diagnostic code: {other:?}"),
    }
}

fn markup(hover: &Hover) -> String {
    match &hover.contents {
        HoverContents::Markup(contents) => contents.value.clone(),
        other => panic!("unexpected hover contents: {other:?}"),
    }
}

fn labels(language: Language) -> Vec<String> {
    completion_items(language)
        .into_iter()
        .map(|item| item.label)
        .collect()
}

// ---------------------------------------------------------------------------
// Language selection

#[test]
fn language_from_id() {
    assert_eq!(Language::from_id("fx"), Some(Language::Prgm));
    assert_eq!(Language::from_id("prgm"), Some(Language::Prgm));
    assert_eq!(Language::from_id("fxc"), Some(Language::Fxc));
    assert_eq!(Language::from_id("FXC"), Some(Language::Fxc));
    assert_eq!(Language::from_id("Fx"), Some(Language::Prgm));
    assert_eq!(Language::from_id("rust"), None);
    assert_eq!(Language::from_id(""), None);
}

#[test]
fn language_from_path() {
    assert_eq!(Language::from_path("main.fxc"), Language::Fxc);
    assert_eq!(Language::from_path("MAIN.FXC"), Language::Fxc);
    assert_eq!(Language::from_path("main.fx"), Language::Prgm);
    assert_eq!(Language::from_path("main.FX"), Language::Prgm);
    assert_eq!(Language::from_path("main.txt"), Language::Prgm);
    assert_eq!(Language::from_path("no-extension"), Language::Prgm);
    assert_eq!(Language::from_path("file:///tmp/a.fxc"), Language::Fxc);
}

// ---------------------------------------------------------------------------
// Diagnostics

#[test]
fn valid_fxc_program_has_no_diagnostics() {
    let source = "let a = input();\nlet b = a * 2;\nprint(b);\n";
    assert!(diagnostics(source, Language::Fxc, None).is_empty());
}

#[test]
fn fxc_syntax_error_is_a_single_transpile_error() {
    let diags = diagnostics("let a = ;", Language::Fxc, None);
    assert_eq!(diags.len(), 1);
    assert_eq!(code(&diags[0]), "Transpile ERROR");
    assert_eq!(diags[0].range.start.line, 0);
    assert_eq!(diags[0].range.end.line, 0);
}

#[test]
fn fxc_base_mode_rejects_a_builtin() {
    let diags = diagnostics("#mode BASE\nprint(sqrt(4));", Language::Fxc, None);
    assert_eq!(diags.len(), 1);
    assert_eq!(code(&diags[0]), "Transpile ERROR");
    assert_eq!(diags[0].range.start.line, 1, "error should point at sqrt");
}

#[test]
fn fxc_unknown_function_is_reported() {
    let diags = diagnostics("print(nope(1));", Language::Fxc, None);
    assert_eq!(diags.len(), 1);
    assert!(
        diags[0].message.contains("unknown function"),
        "unexpected message: {}",
        diags[0].message
    );
}

#[test]
fn fxc_phys_constant_is_valid() {
    let source = "let f = input();\nlet energy = phys.h * f;\nprint(energy);\n";
    assert!(diagnostics(source, Language::Fxc, None).is_empty());
}

#[test]
fn fxc_compile_time_features_are_valid() {
    let source = "#data config = { \"n\": 3, \"xs\": [1, 2] };\n\
                  const k = config.n;\n\
                  let total = config.xs[1] * k;\n\
                  print(total);\n\
                  free total;\n\
                  let next = k;\n\
                  print(next);\n";
    assert!(
        diagnostics(source, Language::Fxc, None).is_empty(),
        "{:?}",
        diagnostics(source, Language::Fxc, None)
    );
}

#[test]
fn fxc_use_after_free_is_reported() {
    let diags = diagnostics("let a = 1;\nfree a;\nprint(a);\n", Language::Fxc, None);
    assert_eq!(diags.len(), 1);
    assert_eq!(code(&diags[0]), "Transpile ERROR");
    assert!(
        diags[0].message.contains("was freed"),
        "{}",
        diags[0].message
    );
    assert_eq!(diags[0].range.start.line, 2, "should point at the use");
}

#[test]
fn fxc_redeclaration_of_a_live_name_is_reported() {
    let diags = diagnostics("let x = 1;\nlet x = 2;\n", Language::Fxc, None);
    assert_eq!(diags.len(), 1);
    assert!(
        diags[0].message.contains("already declared"),
        "{}",
        diags[0].message
    );
    assert_eq!(diags[0].range.start.line, 1);
}

#[test]
fn fxc_redeclaration_after_free_is_clean() {
    let source = "let x = input();\nfree x;\nlet x = input();\nprint(x);\n";
    assert!(
        diagnostics(source, Language::Fxc, None).is_empty(),
        "{:?}",
        diagnostics(source, Language::Fxc, None)
    );
}

#[test]
fn fxc_checked_free_with_a_jump_is_reported() {
    let diags = diagnostics(
        "let a = 1;\nfree a;\ngoto 1;\nlabel 1;\n",
        Language::Fxc,
        None,
    );
    assert_eq!(diags.len(), 1);
    assert!(
        diags[0].message.contains("unsafe_free"),
        "{}",
        diags[0].message
    );
}

#[test]
fn fxc_double_free_is_reported() {
    let diags = diagnostics("let a = 1;\nfree a;\nfree a;\n", Language::Fxc, None);
    assert_eq!(diags.len(), 1);
    assert!(
        diags[0].message.contains("double free"),
        "{}",
        diags[0].message
    );
}

#[test]
fn fxc_const_of_a_variable_is_reported() {
    let diags = diagnostics("let a = 1;\nconst k = a;\n", Language::Fxc, None);
    assert_eq!(diags.len(), 1);
    assert_eq!(code(&diags[0]), "Transpile ERROR");
    assert!(
        diags[0].message.contains("constant expression"),
        "{}",
        diags[0].message
    );
}

#[test]
fn fxc_missing_data_file_is_reported() {
    let diags = diagnostics(
        "#data v = \"definitely-missing.json\";\nprint(v);\n",
        Language::Fxc,
        None,
    );
    assert_eq!(diags.len(), 1);
    assert_eq!(code(&diags[0]), "Transpile ERROR");
    assert!(
        diags[0].message.contains("cannot read data file"),
        "{}",
        diags[0].message
    );
}

#[test]
fn fxc_data_file_resolves_against_the_document_directory() {
    let dir = TempDir::new("data");
    dir.write("values.json", r#"{ "scale": 10 }"#);
    let source = "#data v = \"values.json\";\nprint(v.scale);\n";
    assert!(diagnostics(source, Language::Fxc, Some(dir.path())).is_empty());
}

#[test]
fn missing_include_is_an_include_error_naming_the_path() {
    let dir = TempDir::new("missing-include");
    let diags = diagnostics(
        "let a = 1;\n#include \"nope-missing.fxc\"\n",
        Language::Fxc,
        Some(dir.path()),
    );
    assert_eq!(diags.len(), 1, "expected exactly one diagnostic: {diags:?}");
    assert_eq!(code(&diags[0]), "Include ERROR");
    assert!(
        diags[0].message.contains("nope-missing.fxc"),
        "message should name the missing path: {}",
        diags[0].message
    );
    assert_eq!(diags[0].range.start.line, 1, "include is on line 2");
}

#[test]
fn include_resolves_relative_to_the_base_dir() {
    let dir = TempDir::new("valid-include");
    dir.write("lib/squares.fxc", "let squared = value * value;");
    let source = "let value = input();\n#include \"lib/squares.fxc\"\nprint(squared);\n";
    assert!(diagnostics(source, Language::Fxc, Some(dir.path())).is_empty());
}

#[test]
fn a_missing_base_dir_does_not_panic() {
    let missing = std::env::temp_dir().join("fx-lsp-base-dir-that-does-not-exist");
    let _ = std::fs::remove_dir_all(&missing);
    let diags = diagnostics("let a = 1;", Language::Fxc, Some(missing.as_path()));
    assert!(diags.is_empty());
}

// ---------------------------------------------------------------------------
// Completion

#[test]
fn fxc_completion_contains_the_expected_labels() {
    let labels = labels(Language::Fxc);
    for expected in ["let", "print", "sqrt(", "phys.h", "#include", "#mode CMPLX"] {
        assert!(
            labels.iter().any(|label| label == expected),
            "missing `{expected}` in {labels:?}"
        );
    }
}

#[test]
fn fxc_completion_omits_prgm_only_labels() {
    let labels = labels(Language::Fxc);
    for prgm_only in ["IfEnd", "WhileEnd", "Lbl"] {
        assert!(
            !labels.iter().any(|label| label == prgm_only),
            "PRGM-only `{prgm_only}` leaked into the .fxc list"
        );
    }
}

#[test]
fn fxc_completion_is_deduplicated() {
    let labels = labels(Language::Fxc);
    let mut unique = labels.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), labels.len(), "duplicate completion labels");
}

#[test]
fn fxc_completion_offers_every_scientific_constant() {
    let items = completion_items(Language::Fxc);
    let phys: Vec<_> = items
        .iter()
        .filter(|item| item.label.starts_with("phys."))
        .collect();
    // The 40 constants plus the bare `phys.` namespace item.
    assert_eq!(
        phys.len(),
        41,
        "expected 40 constants and one namespace item"
    );

    let planck = items
        .iter()
        .find(|item| item.label == "phys.h")
        .expect("phys.h");
    assert_eq!(planck.kind, Some(CompletionItemKind::CONSTANT));
    let detail = planck.detail.clone().unwrap_or_default();
    assert!(detail.contains("Planck constant"), "detail: {detail}");
    assert!(
        detail.contains("06"),
        "detail should name the menu code: {detail}"
    );
}

#[test]
fn fxc_completion_builtins_are_snippets() {
    let items = completion_items(Language::Fxc);
    let sqrt = items
        .iter()
        .find(|item| item.label == "sqrt(")
        .expect("sqrt(");
    assert_eq!(sqrt.kind, Some(CompletionItemKind::FUNCTION));
    assert_eq!(sqrt.insert_text.as_deref(), Some("sqrt($0)"));
    assert_eq!(sqrt.insert_text_format, Some(InsertTextFormat::SNIPPET));

    let log = items
        .iter()
        .find(|item| item.label == "log(")
        .expect("log(");
    assert!(
        log.detail.clone().unwrap_or_default().contains("1 to 2"),
        "log should mention its 1-or-2 arity"
    );
}

#[test]
fn prgm_completion_is_unchanged() {
    let labels = labels(Language::Prgm);
    for expected in ["IfEnd", "Lbl", "√(", "#mode"] {
        assert!(labels.contains(&expected.to_string()), "missing {expected}");
    }
    for fxc_only in ["let", "sqrt(", "#include", "phys.h"] {
        assert!(
            !labels.contains(&fxc_only.to_string()),
            "unexpected `.fxc` item `{fxc_only}` in the PRGM list"
        );
    }
}

// ---------------------------------------------------------------------------
// Hover

#[test]
fn fxc_hover_on_keyword() {
    let hover = hover("let a = 1;", Position::new(0, 1), Language::Fxc).expect("let");
    assert!(markup(&hover).contains("declare a variable"), "{hover:?}");
}

#[test]
fn fxc_hover_on_builtin() {
    let src = "sqrt(4)";
    let hover = hover(src, Position::new(0, 1), Language::Fxc).expect("sqrt");
    let value = markup(&hover);
    assert!(value.contains("built-in function"), "{value}");
    assert_eq!(
        hover.range,
        Some(Range::new(Position::new(0, 0), Position::new(0, 4)))
    );
}

#[test]
fn fxc_hover_on_pi() {
    let hover = hover("pi", Position::new(0, 1), Language::Fxc).expect("pi");
    assert!(markup(&hover).contains("constant"), "{hover:?}");
}

#[test]
fn fxc_hover_on_phys_constant_names_planck() {
    let planck = hover("phys.h", Position::new(0, 5), Language::Fxc).expect("phys.h");
    let value = markup(&planck);
    assert!(value.contains("Planck constant"), "{value}");
    assert!(value.contains("CONST 06"), "{value}");
    assert_eq!(
        planck.range,
        Some(Range::new(Position::new(0, 5), Position::new(0, 6)))
    );

    // Whitespace around the dot is ignored, matching the parser.
    let spaced = hover("phys . h", Position::new(0, 7), Language::Fxc).expect("phys . h");
    assert!(markup(&spaced).contains("Planck constant"));
}

#[test]
fn fxc_hover_on_whitespace_is_none() {
    assert!(hover("let a = 1;", Position::new(0, 3), Language::Fxc).is_none());
    assert!(hover("   ", Position::new(0, 1), Language::Fxc).is_none());
}

#[test]
fn fxc_bare_constant_name_is_described_as_a_variable() {
    // In `.fxc`, `hbar` without the `phys.` namespace is an ordinary variable.
    let hover = hover("let hbar = phys.h;", Position::new(0, 5), Language::Fxc).expect("hbar");
    let value = markup(&hover);
    assert!(value.contains("variable"), "{value}");
    assert!(
        value.contains("phys.hbar"),
        "a constant-named variable should hint at the namespace: {value}"
    );
}

// ---------------------------------------------------------------------------
// Document symbols

#[test]
fn fxc_document_symbols_list_labels_and_variables() {
    let source = "label 1;\nlet total = 1;\nfor (let i = 0; i < 3; i = i + 1) {\n  total = total + i;\n}\nlabel 2;\n";
    let symbols = document_symbols(source, Language::Fxc);
    let names: Vec<String> = symbols.iter().map(|symbol| symbol.name.clone()).collect();
    for expected in ["label 1", "label 2", "total", "i"] {
        assert!(
            names.contains(&expected.to_string()),
            "missing {expected}: {names:?}"
        );
    }

    // Sorted by position, with plausible ranges.
    let mut previous = (0u32, 0u32);
    for symbol in &symbols {
        let here = (symbol.range.start.line, symbol.range.start.character);
        assert!(here >= previous, "symbols out of order: {names:?}");
        previous = here;
        assert!(symbol.range.start.line < 6);
    }
}

#[test]
fn fxc_document_symbols_deduplicate_names() {
    let symbols = document_symbols("let a = 1;\nlet a = 2;\n", Language::Fxc);
    assert_eq!(
        symbols.iter().filter(|symbol| symbol.name == "a").count(),
        1
    );
}

#[test]
fn fxc_document_symbols_are_empty_for_malformed_source() {
    assert!(document_symbols("let a = ;", Language::Fxc).is_empty());
    assert!(document_symbols("@@@", Language::Fxc).is_empty());
}

// ---------------------------------------------------------------------------
// range_from_line_col

#[test]
fn range_from_line_col_edge_cases() {
    let source = "abc\ndef";
    // Line 0 and column 0 clamp to the first character.
    assert_eq!(
        range_from_line_col(source, 0, 0),
        Range::new(Position::new(0, 0), Position::new(0, 1))
    );
    // A normal position.
    assert_eq!(
        range_from_line_col(source, 2, 2),
        Range::new(Position::new(1, 1), Position::new(1, 2))
    );
    // A column past the end of a line clamps to a zero-width range there.
    let end = Position::new(1, 3);
    assert_eq!(range_from_line_col(source, 2, 99), Range::new(end, end));
    // A line past EOF clamps to a zero-width range at the end of the document.
    assert_eq!(range_from_line_col(source, 99, 1), Range::new(end, end));
}

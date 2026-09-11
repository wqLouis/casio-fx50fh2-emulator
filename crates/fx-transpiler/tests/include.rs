//! End-to-end tests for `#include`: real files on disk, expanded at transpile
//! time.

use std::path::{Path, PathBuf};

use fx_transpiler::error::TranspileError;
use fx_transpiler::{Options, transpile_file, transpile_with_base};

mod common;

/// A scratch directory that cleans itself up.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "fx-transpiler-include-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        TempDir(dir)
    }

    /// Write `content` to `name`, creating parent directories.
    fn write(&self, name: &str, content: &str) -> PathBuf {
        let path = self.0.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent dir");
        }
        std::fs::write(&path, content).expect("write temp file");
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The *unoptimised* translation. These tests are about `#include` resolution —
/// which file a fragment came from — so each construct is asserted as written;
/// the optimiser is covered by `tests/simplify.rs` and `tests/propagate.rs`.
const RAW: Options = Options {
    ascii: false,
    mode: None,
    optimize: false,
};

fn build(path: &Path) -> Result<String, TranspileError> {
    transpile_file(path, RAW)
}

// ---------------------------------------------------------------------------

#[test]
fn a_fragment_is_inlined_where_it_is_included() {
    let dir = TempDir::new("inline");
    dir.write("lib/math.fxc", "let r = x * x;");
    let main = dir.write(
        "main.fxc",
        &common::wrap("let x = input();\n#include \"lib/math.fxc\"\nprint(r);\n"),
    );

    let prgm = build(&main).unwrap();
    assert_eq!(prgm, "?→A\nA×A→B\nB◢\n");
}

#[test]
fn includes_nest() {
    let dir = TempDir::new("nested");
    dir.write("a/outer.fxc", "#include \"../b/inner.fxc\"\nlet y = r + 1;");
    dir.write("b/inner.fxc", "let r = 10;");
    // A statement fragment must be spliced into `main`'s body, so the include
    // sits inside it.
    let main = dir.write(
        "main.fxc",
        "fn main() {\n#include \"a/outer.fxc\"\nprint(y);\n}\n",
    );

    let prgm = build(&main).unwrap();
    assert_eq!(prgm, "10→A\nA+1→B\nB◢\n");
}

/// Paths are relative to the *including* file, so a fragment can include its
/// own neighbours without knowing who included it.
#[test]
fn paths_resolve_relative_to_the_including_file() {
    let dir = TempDir::new("relative");
    dir.write("lib/one.fxc", "#include \"two.fxc\"\nlet b = a + 1;");
    dir.write("lib/two.fxc", "let a = 1;");
    // The including file is at the top level, but `one.fxc`'s sibling is found.
    let main = dir.write(
        "main.fxc",
        "fn main() {\n#include \"lib/one.fxc\"\nprint(b);\n}\n",
    );

    assert_eq!(build(&main).unwrap(), "1→A\nA+1→B\nB◢\n");
}

#[test]
fn a_fragment_may_refer_to_names_from_its_includer() {
    let dir = TempDir::new("shared-names");
    dir.write("square.fxc", "let squared = value * value;");
    let main = dir.write(
        "main.fxc",
        &common::wrap("let value = input();\n#include \"square.fxc\"\nprint(squared);\n"),
    );

    assert_eq!(build(&main).unwrap(), "?→A\nA×A→B\nB◢\n");
}

#[test]
fn ascii_option_applies_after_expansion() {
    let dir = TempDir::new("ascii");
    dir.write("frag.fxc", "let b = a + 1;");
    let main = dir.write(
        "main.fxc",
        &common::wrap("let a = 1;\n#include \"frag.fxc\"\nprint(b);\n"),
    );

    let prgm = transpile_file(&main, Options { ascii: true, ..RAW }).unwrap();
    assert_eq!(prgm, "1->A\nA+1->B\nBdisp\n");
}

// ---------------------------------------------------------------------------
// Errors

#[test]
fn a_missing_fragment_names_the_offending_file_and_line() {
    let dir = TempDir::new("missing");
    let main = dir.write(
        "main.fxc",
        &common::wrap("let a = 1;\n#include \"nope.fxc\"\n"),
    );

    let err = build(&main).unwrap_err();
    assert!(err.message.contains("cannot include"), "{err}");
    assert!(err.message.contains("nope.fxc"), "{err}");
    // `common::wrap` adds the `fn main() {` line, so the include is line 3.
    assert_eq!(err.line, 3);
    assert_eq!(err.column, 1);
    assert_eq!(err.file.as_deref(), Some(main.to_string_lossy().as_ref()));
}

#[test]
fn a_cycle_is_reported_rather_than_looping() {
    let dir = TempDir::new("cycle");
    dir.write("a.fxc", "#include \"b.fxc\"\nlet a = 1;");
    dir.write("b.fxc", "#include \"a.fxc\"\nlet b = 2;");
    let main = dir.write("main.fxc", &common::wrap("#include \"a.fxc\"\n"));

    let err = build(&main).unwrap_err();
    assert!(err.message.contains("circular `#include`"), "{err}");
    // The chain is shown, so the cycle is obvious.
    assert!(err.message.contains("a.fxc"), "{err}");
    assert!(err.message.contains("b.fxc"), "{err}");
}

/// An error inside a fragment must point at the fragment, not at the expanded
/// text, or the position would be meaningless to the reader.
#[test]
fn errors_inside_a_fragment_are_attributed_to_it() {
    let dir = TempDir::new("attribution");
    let fragment = dir.write("frag.fxc", "let ok = 1;\nlet bad = nope(2);\n");
    let main = dir.write(
        "main.fxc",
        "fn main() {\n#include \"frag.fxc\"\nprint(ok);\n}\n",
    );

    let err = build(&main).unwrap_err();
    assert!(err.message.contains("unknown function"), "{err}");
    assert_eq!(err.line, 2, "line within the fragment");
    assert_eq!(err.column, 11);
    assert_eq!(
        err.file.as_deref(),
        Some(fragment.to_string_lossy().as_ref())
    );
}

/// Errors in the root file still point at the root.
#[test]
fn errors_in_the_root_are_attributed_to_the_root() {
    let dir = TempDir::new("root-error");
    dir.write("frag.fxc", "let a = 1;");
    let main = dir.write(
        "main.fxc",
        "fn main() {\n#include \"frag.fxc\"\nlet bad = nope(1);\n}\n",
    );

    let err = build(&main).unwrap_err();
    // `common::wrap` adds the `fn main() {` line, so the error is on line 3.
    assert_eq!(err.line, 3);
    assert_eq!(err.file.as_deref(), Some(main.to_string_lossy().as_ref()));
}

/// The mode applies to the whole program, so a fragment may not set it.
#[test]
fn a_fragment_may_not_declare_a_mode() {
    let dir = TempDir::new("fragment-mode");
    let fragment = dir.write("frag.fxc", "#mode CMPLX\nlet a = 1;");
    let main = dir.write(
        "main.fxc",
        &common::wrap("#include \"frag.fxc\"\nprint(a);\n"),
    );

    let err = build(&main).unwrap_err();
    assert!(err.message.contains("may only appear"), "{err}");
    assert_eq!(
        err.file.as_deref(),
        Some(fragment.to_string_lossy().as_ref())
    );
}

/// ...but the root file still may, and it still comes first in the output.
#[test]
fn the_root_file_may_declare_a_mode_alongside_includes() {
    let dir = TempDir::new("root-mode");
    dir.write("frag.fxc", "let b = a + 1;");
    let main = dir.write(
        "main.fxc",
        &common::wrap("#mode SD\nlet a = 1;\n#include \"frag.fxc\"\nprint(b);\n"),
    );

    let prgm = build(&main).unwrap();
    assert_eq!(prgm, "#mode SD\n1→A\nA+1→B\nB◢\n");
}

#[test]
fn an_include_directive_needs_to_start_the_line() {
    let dir = TempDir::new("not-first");
    dir.write("frag.fxc", "let a = 1;");
    // Mid-line, the `#` is not an include directive and the lexer rejects it.
    let main = dir.write(
        "main.fxc",
        &common::wrap("let a = 1 + #include \"frag.fxc\";\n"),
    );

    let err = build(&main).unwrap_err();
    assert!(
        err.message.contains("`#` directive"),
        "unexpected message: {err}"
    );
    // `common::wrap` adds the `fn main() {` line, so the error is on line 2.
    assert_eq!(err.line, 2);
}

// ---------------------------------------------------------------------------
// Transpiling text rather than a file

#[test]
fn base_dir_lets_inline_text_find_its_includes() {
    let dir = TempDir::new("base-dir");
    dir.write("frag.fxc", "let b = a * 3;");

    let prgm = transpile_with_base(
        &common::wrap("let a = 1;\n#include \"frag.fxc\"\nprint(b);\n"),
        RAW,
        &dir.0,
    )
    .unwrap();
    assert_eq!(prgm, "1→A\nA×3→B\nB◢\n");
}

#[test]
fn text_without_an_include_is_unaffected() {
    // The common case must not regress: no include, no change.
    let prgm = transpile_with_base(
        &common::wrap("print(1);"),
        Options::default(),
        Path::new("."),
    )
    .unwrap();
    assert_eq!(prgm, "1◢\n");
}

// ---------------------------------------------------------------------------
// Composition with the test runner

#[cfg(feature = "testing")]
#[test]
fn a_test_suite_can_test_a_program_that_uses_includes() {
    use fx_transpiler::testing::run_suite_file;

    let dir = TempDir::new("suite-include");
    dir.write("lib/double.fxc", "// doubles a into b\nlet b = a * 2;");
    dir.write(
        "prog.fxc",
        &common::wrap("let a = input();\n#include \"lib/double.fxc\"\nprint(b);\n"),
    );
    let suite = dir.write(
        "prog.tests.json",
        r#"{"program": "prog.fxc",
            "cases": [
              {"name": "21", "input": [21], "output": ["42"]},
              {"name": "0",  "input": [0],  "output": ["0"]}
            ]}"#,
    );

    let report = run_suite_file(&suite, None).unwrap();
    assert!(report.is_success(), "{report:#?}");
    assert_eq!(report.passed(), 2);
}

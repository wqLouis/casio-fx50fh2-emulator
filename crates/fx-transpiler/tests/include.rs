//! End-to-end tests for `#include`: real files on disk, expanded at transpile
//! time.
//!
//! An included file is a **library**: `fn` definitions (and optionally
//! compile-time `const`/`#data` values) that the including program calls. It is
//! not a fragment of statements spliced into the includer, so the directive sits
//! at the top level rather than inside a body, and a library file is buildable
//! on its own.

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
/// which file a definition came from — so each construct is asserted as written;
/// the optimiser is covered by `tests/simplify.rs` and `tests/propagate.rs`.
const RAW: Options = Options {
    ascii: false,
    mode: None,
    optimize: false,
};

/// Transpile a file with the unoptimised translation.
fn build(path: &Path) -> Result<String, TranspileError> {
    transpile_file(path, RAW)
}

// ---------------------------------------------------------------------------
// A library is inlined where it is included

#[test]
fn a_library_is_inlined_where_it_is_included() {
    let dir = TempDir::new("inline");
    dir.write("lib/math.fxc", "fn square(x) = x * x;");
    let main = dir.write(
        "main.fxc",
        "#include \"lib/math.fxc\"\nfn main() {\n    let x = input();\n    print(square(x));\n}\n",
    );

    assert_eq!(build(&main).unwrap(), "?→A\nA×A◢\n");
}

#[test]
fn includes_nest() {
    let dir = TempDir::new("nested");
    dir.write(
        "a/outer.fxc",
        "#include \"../b/inner.fxc\"\nfn outer(n) = inner(n) + 1;",
    );
    dir.write("b/inner.fxc", "fn inner(n) = n * 10;");
    let main = dir.write(
        "main.fxc",
        "#include \"a/outer.fxc\"\nfn main() {\n    let x = input();\n    print(outer(x));\n}\n",
    );

    assert_eq!(build(&main).unwrap(), "?→A\nA×10+1◢\n");
}

/// Paths are relative to the *including* file, so a library can include its own
/// neighbours without knowing who included it.
#[test]
fn paths_resolve_relative_to_the_including_file() {
    let dir = TempDir::new("relative");
    dir.write(
        "lib/one.fxc",
        "#include \"two.fxc\"\nfn one(n) = two(n) + 1;",
    );
    dir.write("lib/two.fxc", "fn two(n) = n + 2;");
    let main = dir.write(
        "main.fxc",
        "#include \"lib/one.fxc\"\nfn main() {\n    let x = input();\n    print(one(x));\n}\n",
    );

    assert_eq!(build(&main).unwrap(), "?→A\nA+2+1◢\n");
}

#[test]
fn a_library_may_export_a_compile_time_value() {
    let dir = TempDir::new("const");
    dir.write("lib/units.fxc", "const inches_per_foot = 12;");
    let main = dir.write(
        "main.fxc",
        "#include \"lib/units.fxc\"\nfn main() {\n    let feet = input();\n    \
         print(feet * inches_per_foot);\n}\n",
    );

    // A `const` is inlined, so it costs no memory and emits no statement of its
    // own.
    assert_eq!(build(&main).unwrap(), "?→A\nA×12◢\n");
}

// ---------------------------------------------------------------------------
// A library is a file in its own right

/// The reported bug: a library has no `fn main()`, and that is not an error.
#[test]
fn a_library_builds_to_an_empty_program() {
    let dir = TempDir::new("library");
    let lib = dir.write("lib.fxc", "fn square(x) = x * x;\nconst k = 2;\n");
    assert_eq!(build(&lib).unwrap(), "");
}

#[test]
fn an_empty_file_is_also_empty() {
    // Continuity with `golden.rs::empty_program_is_empty`: with nothing defined
    // there is no entry point to demand, and no statements to reject.
    let dir = TempDir::new("empty");
    let empty = dir.write("empty.fxc", "");
    assert_eq!(build(&empty).unwrap(), "");
}

#[test]
fn a_loose_statement_still_needs_an_entry_point() {
    let dir = TempDir::new("loose");
    let main = dir.write("main.fxc", "print(1);\n");
    let err = build(&main).unwrap_err();
    assert!(err.message.contains("needs an entry point"), "{err}");
}

/// A library cannot reach into the program that included it — which is the
/// whole reason statement fragments are gone. Its dependencies are in its
/// signature.
#[test]
fn a_library_cannot_read_the_includers_names() {
    let dir = TempDir::new("closed");
    dir.write("lib.fxc", "fn f(n) = n + caller_variable;");
    let main = dir.write(
        "main.fxc",
        "#include \"lib.fxc\"\nfn main() {\n    let caller_variable = 1;\n    \
         print(f(caller_variable));\n}\n",
    );

    let err = build(&main).unwrap_err();
    assert!(
        err.message.contains("caller_variable"),
        "the offending name should be named: {err}"
    );
}

// ---------------------------------------------------------------------------
// Where the include goes

#[test]
fn an_include_inside_a_body_is_rejected() {
    let dir = TempDir::new("in-body");
    dir.write("lib.fxc", "fn square(x) = x * x;");
    let main = dir.write(
        "main.fxc",
        "fn main() {\n#include \"lib.fxc\"\n    print(1);\n}\n",
    );

    let err = build(&main).unwrap_err();
    assert!(
        err.message.contains("must be at the top level"),
        "unexpected message: {err}"
    );
    assert!(err.message.contains("not inside a function body"), "{err}");
    // The directive is on line 2 of the file as written.
    assert_eq!(err.line, 2);
    assert_eq!(err.file.as_deref(), Some(main.to_string_lossy().as_ref()));
}

/// A brace in a comment or a string must not be mistaken for a function body,
/// or a legitimate top-level include after one would be rejected.
#[test]
fn braces_in_comments_and_strings_do_not_hide_a_top_level_include() {
    let dir = TempDir::new("braces");
    dir.write("lib.fxc", "fn f(n) = n + 1;");
    for (tag, prologue) in [
        ("line-comment", "// a brace in a comment: {\n"),
        ("block-comment", "/* a brace in a block comment: { */\n"),
        ("multi-line-comment", "/* spans\nlines and {\nbraces */\n"),
        ("brace-in-json-string", "#data s = { \"brace\": \"}\" };\n"),
        ("json", "#data c = { \"a\": { \"b\": 1 } };\n"),
        ("multi-line-json", "#data c = {\n  \"a\": 1\n};\n"),
    ] {
        let main = dir.write(
            &format!("{tag}.fxc"),
            &format!(
                "{prologue}#include \"lib.fxc\"\nfn main() {{\n    let x = input();\n    \
                 print(f(x));\n}}\n"
            ),
        );
        assert_eq!(build(&main).unwrap(), "?→A\nA+1◢\n", "for {tag}");
    }
}

#[test]
fn an_include_directive_needs_to_start_the_line() {
    let dir = TempDir::new("not-first");
    dir.write("frag.fxc", "fn f(n) = n;");
    // Mid-line, the `#` is not an include directive and the lexer rejects it.
    let main = dir.write(
        "main.fxc",
        "fn main() {\n    let a = 1 + #include \"frag.fxc\";\n}\n",
    );

    let err = build(&main).unwrap_err();
    assert!(
        err.message.contains("`#` directive"),
        "unexpected message: {err}"
    );
    assert_eq!(err.line, 2);
}

// ---------------------------------------------------------------------------
// Errors

#[test]
fn a_missing_include_names_the_offending_file_and_line() {
    let dir = TempDir::new("missing");
    let main = dir.write(
        "main.fxc",
        "#include \"nope.fxc\"\nfn main() {\n    print(1);\n}\n",
    );

    let err = build(&main).unwrap_err();
    assert!(err.message.contains("cannot include"), "{err}");
    assert!(err.message.contains("nope.fxc"), "{err}");
    assert_eq!(err.line, 1);
    assert_eq!(err.column, 1);
    assert_eq!(err.file.as_deref(), Some(main.to_string_lossy().as_ref()));
}

#[test]
fn a_cycle_is_reported_rather_than_looping() {
    let dir = TempDir::new("cycle");
    dir.write("a.fxc", "#include \"b.fxc\"\nfn fa(n) = n;");
    dir.write("b.fxc", "#include \"a.fxc\"\nfn fb(n) = n;");
    let main = dir.write(
        "main.fxc",
        "#include \"a.fxc\"\nfn main() {\n    print(fa(1));\n}\n",
    );

    let err = build(&main).unwrap_err();
    assert!(err.message.contains("circular `#include`"), "{err}");
    // The chain is shown, so the cycle is obvious.
    assert!(err.message.contains("a.fxc"), "{err}");
    assert!(err.message.contains("b.fxc"), "{err}");
}

/// An error inside a library must point at the library, not at the expanded
/// text, or the position would be meaningless to the reader.
#[test]
fn errors_inside_a_library_are_attributed_to_it() {
    let dir = TempDir::new("attribution");
    let lib = dir.write("lib.fxc", "fn ok(n) = n;\nfn bad(n) = nope(n);\n");
    let main = dir.write(
        "main.fxc",
        "#include \"lib.fxc\"\nfn main() {\n    print(bad(1));\n}\n",
    );

    let err = build(&main).unwrap_err();
    assert!(err.message.contains("unknown function"), "{err}");
    assert_eq!(err.line, 2, "line within the library");
    assert_eq!(err.file.as_deref(), Some(lib.to_string_lossy().as_ref()));
}

/// Errors in the root file still point at the root.
#[test]
fn errors_in_the_root_are_attributed_to_the_root() {
    let dir = TempDir::new("root-error");
    dir.write("lib.fxc", "fn f(n) = n;");
    let main = dir.write(
        "main.fxc",
        "#include \"lib.fxc\"\nfn main() {\n    let bad = nope(1);\n}\n",
    );

    let err = build(&main).unwrap_err();
    assert_eq!(err.line, 3);
    assert_eq!(err.file.as_deref(), Some(main.to_string_lossy().as_ref()));
}

/// The mode applies to the whole program, so a library may not set it.
#[test]
fn a_library_may_not_declare_a_mode() {
    let dir = TempDir::new("library-mode");
    let lib = dir.write("lib.fxc", "#mode CMPLX\nfn f(n) = n;");
    let main = dir.write(
        "main.fxc",
        "#include \"lib.fxc\"\nfn main() {\n    print(f(1));\n}\n",
    );

    let err = build(&main).unwrap_err();
    assert!(err.message.contains("may only appear"), "{err}");
    assert_eq!(err.file.as_deref(), Some(lib.to_string_lossy().as_ref()));
}

/// ...but the root file still may, and it still comes first in the output.
#[test]
fn the_root_file_may_declare_a_mode_alongside_includes() {
    let dir = TempDir::new("root-mode");
    dir.write("lib.fxc", "fn f(n) = n + 1;");
    let main = dir.write(
        "main.fxc",
        "#mode SD\n#include \"lib.fxc\"\nfn main() {\n    let a = input();\n    print(f(a));\n}\n",
    );

    assert_eq!(build(&main).unwrap(), "#mode SD\n?→A\nA+1◢\n");
}

#[test]
fn ascii_option_applies_after_expansion() {
    let dir = TempDir::new("ascii");
    dir.write("frag.fxc", "fn add_one(n) = n + 1;");
    let main = dir.write(
        "main.fxc",
        "#include \"frag.fxc\"\nfn main() {\n    let a = 1;\n    print(add_one(a));\n}\n",
    );

    let prgm = transpile_file(&main, Options { ascii: true, ..RAW }).unwrap();
    assert_eq!(prgm, "1->A\nA+1disp\n");
}

// ---------------------------------------------------------------------------
// Transpiling text rather than a file

#[test]
fn base_dir_lets_inline_text_find_its_includes() {
    let dir = TempDir::new("base-dir");
    dir.write("lib.fxc", "fn times_three(n) = n * 3;");

    let prgm = transpile_with_base(
        "#include \"lib.fxc\"\nfn main() {\n    let a = input();\n    print(times_three(a));\n}\n",
        RAW,
        &dir.0,
    )
    .unwrap();
    assert_eq!(prgm, "?→A\nA×3◢\n");
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
    dir.write("lib/double.fxc", "fn double(n) = n * 2;");
    dir.write(
        "prog.fxc",
        "#include \"lib/double.fxc\"\nfn main() {\n    let a = input();\n    print(double(a));\n}\n",
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

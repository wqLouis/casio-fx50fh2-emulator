//! End-to-end tests for `fx50 test`, driven through the real binary.
#![cfg(feature = "transpiler")]

use std::path::PathBuf;
use std::process::Command;

/// A scratch directory that cleans itself up.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("fx50-test-cmd-{tag}-{}", std::process::id()));
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
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn fx50() -> Command {
    Command::new(env!("CARGO_BIN_EXE_fx50"))
}

#[test]
fn passing_suite_exits_zero_and_summarises() {
    let dir = TempDir::new("pass");
    let program = dir.write("prog.fxc", "fn main() { let a = input(); print(a * 2); }");
    dir.write(
        "prog.tests.json",
        r#"{"cases": [{"name": "doubles", "input": [21], "output": ["42"]}]}"#,
    );

    let out = fx50().arg("test").arg(&program).output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "stdout: {stdout}");
    assert!(stdout.contains("ok    doubles"), "{stdout}");
    assert!(stdout.contains("1 passed, 0 failed"), "{stdout}");
}

#[test]
fn failing_suite_exits_nonzero_and_shows_both_sides() {
    let dir = TempDir::new("fail");
    let program = dir.write("prog.fxc", "fn main() { print(2+3); }");
    dir.write(
        "prog.tests.json",
        r#"{"cases": [{"name": "adds", "output": ["6"]}]}"#,
    );

    let out = fx50().arg("test").arg(&program).output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !out.status.success(),
        "expected a non-zero exit; stdout: {stdout}"
    );
    assert!(stdout.contains("FAIL  adds"), "{stdout}");
    assert!(stdout.contains("expected: 6"), "{stdout}");
    assert!(stdout.contains("actual: 5"), "{stdout}");
    assert!(stdout.contains("0 passed, 1 failed"), "{stdout}");
}

#[test]
fn json_report_is_emitted_on_request() {
    let dir = TempDir::new("json");
    let program = dir.write("prog.fxc", "fn main() { print(1); }");
    dir.write("prog.tests.json", r#"{"cases": [{"output": ["1"]}]}"#);

    let out = fx50()
        .args(["test", "--json"])
        .arg(&program)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success());
    assert!(stdout.contains("\"passed\": true"), "{stdout}");
    assert!(stdout.contains("\"cases\""), "{stdout}");
}

#[test]
fn a_program_without_tests_is_a_clear_error() {
    let dir = TempDir::new("missing");
    let program = dir.write("lonely.fxc", "fn main() { print(1); }");

    let out = fx50().arg("test").arg(&program).output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success());
    assert!(stderr.contains("has no `#tests` table"), "{stderr}");
    assert!(stderr.contains("lonely.tests.json"), "{stderr}");
}

#[test]
fn a_program_may_carry_its_own_tests() {
    let dir = TempDir::new("embedded");
    let program = dir.write(
        "prog.fxc",
        "fn main() {\n    let a = input();\n    print(a * 2);\n}\n\n#tests = [\n  { \"name\": \"doubles\", \"input\": [21], \"output\": [\"42\"] }\n];\n",
    );

    let out = fx50().arg("test").arg(&program).output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "stdout: {stdout}");
    assert!(stdout.contains("ok    doubles"), "{stdout}");
    assert!(stdout.contains("1 passed, 0 failed"), "{stdout}");
}

#[test]
fn regs_reports_the_memory_plan() {
    let dir = TempDir::new("regs");
    let program = dir.write(
        "prog.fxc",
        "#data config = { \"n\": 3 };\nconst k = config.n;\nfn main() {\n    let total = k;\n    free total;\n    let next = 1;\n    print(next);\n}\n",
    );

    let out = fx50().arg("regs").arg(&program).output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "stdout: {stdout}");
    assert!(stdout.contains("Memory plan for"), "{stdout}");
    assert!(
        stdout.contains("A  total → next"),
        "expected the reuse timeline: {stdout}"
    );
    assert!(stdout.contains("of 7 memories used"), "{stdout}");
    assert!(stdout.contains("released with `free`: total"), "{stdout}");
    assert!(stdout.contains("const (no memory): k"), "{stdout}");
    assert!(
        stdout.contains("data table(s) (no memory): config"),
        "{stdout}"
    );
}

#[test]
fn regs_reports_a_use_after_free() {
    let dir = TempDir::new("uaf");
    let program = dir.write(
        "prog.fxc",
        "fn main() {\n    let a = 1;\n    free a;\n    print(a);\n}\n",
    );

    let out = fx50().arg("regs").arg(&program).output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success());
    assert!(stderr.contains("was freed"), "{stderr}");
}

#[test]
fn regs_reports_a_program_that_does_not_fit() {
    let dir = TempDir::new("regs-full");
    let program = dir.write(
        "prog.fxc",
        "fn main() {\n    let a=1; let b=1; let c=1; let d=1; let x=1; let y=1; let m=1; let z=1;\n}\n",
    );

    let out = fx50().arg("regs").arg(&program).output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success());
    assert!(stderr.contains("no free memory for `z`"), "{stderr}");
}

#[test]
fn malformed_json_is_reported_with_a_position() {
    let dir = TempDir::new("badjson");
    let suite = dir.write("bad.tests.json", "{ nope }");

    let out = fx50().arg("test").arg(&suite).output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success());
    assert!(stderr.contains("invalid JSON at line 1"), "{stderr}");
}

#[test]
fn an_unknown_mode_in_a_suite_is_rejected() {
    let dir = TempDir::new("badmode");
    let suite = dir.write(
        "m.tests.json",
        r#"{"source": "fn main() { print(1); }", "mode": "FANCY", "cases": []}"#,
    );

    let out = fx50().arg("test").arg(&suite).output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success());
    assert!(stderr.contains("unknown mode `FANCY`"), "{stderr}");
}

#[test]
fn filter_selects_a_subset_of_cases() {
    let dir = TempDir::new("filter");
    let program = dir.write("prog.fxc", "fn main() { print(1); }");
    dir.write(
        "prog.tests.json",
        r#"{"cases": [
            {"name": "alpha", "output": ["1"]},
            {"name": "beta", "output": ["9"]}
        ]}"#,
    );

    let out = fx50()
        .args(["test", "--filter", "alpha"])
        .arg(&program)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{stdout}");
    assert!(stdout.contains("1 passed, 0 failed"), "{stdout}");
    assert!(!stdout.contains("beta"), "{stdout}");
}

#[test]
fn the_shipped_examples_pass_through_the_cli() {
    let examples = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    for name in [
        "factorial",
        "quadratic",
        "include",
        "constants",
        "compiletime",
        "arrays",
        "determinant",
        "functions",
    ] {
        let out = fx50()
            .arg("test")
            .arg(examples.join(format!("{name}.fxc")))
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(out.status.success(), "{name}: {stdout}");
        assert!(stdout.contains("0 failed"), "{name}: {stdout}");
    }
}

/// `fx50 build`/`run`/`test` must resolve `#include` relative to the program
/// file, not the process's working directory.
#[test]
fn includes_resolve_relative_to_the_program() {
    let dir = TempDir::new("includes");
    dir.write(
        "lib/double.fxc",
        "// doubles its argument\nfn double(n) = n * 2;",
    );
    let program = dir.write(
        "prog.fxc",
        "#include \"lib/double.fxc\"\nfn main() {\n    let a = input();\n    print(double(a));\n}\n",
    );

    // Build: the library is inlined.
    let out = fx50().arg("build").arg(&program).output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{stdout}");
    assert_eq!(stdout, "?→A\nA×2◢\n");

    // Run: end to end, feeding the `?` prompt.
    let out = fx50()
        .arg("run")
        .arg(&program)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child
                .stdin
                .as_mut()
                .expect("stdin")
                .write_all(b"21\n")
                .expect("write stdin");
            child.wait_with_output()
        })
        .expect("run fx50");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{stdout}");
    assert!(stdout.contains("42"), "{stdout}");

    // And through a suite.
    dir.write(
        "prog.tests.json",
        r#"{"program": "prog.fxc", "cases": [{"input": [21], "output": ["42"]}]}"#,
    );
    let out = fx50().arg("test").arg(&program).output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{stdout}");
    assert!(stdout.contains("1 passed, 0 failed"), "{stdout}");
}

/// A broken include is reported with the file and line that caused it.
#[test]
fn a_missing_include_is_reported_with_its_location() {
    let dir = TempDir::new("missing-include");
    let program = dir.write(
        "prog.fxc",
        "#include \"nope.fxc\"\nfn main() {\n    let a = 1;\n}\n",
    );

    let out = fx50().arg("build").arg(&program).output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success());
    assert!(stderr.contains("cannot include"), "{stderr}");
    assert!(stderr.contains("nope.fxc"), "{stderr}");
    assert!(stderr.contains(":1:1"), "{stderr}");
}

/// A library has no `fn main()`, so there is nothing to run — and saying so is
/// better than running an empty program silently.
#[test]
fn running_a_library_explains_there_is_no_entry_point() {
    let dir = TempDir::new("run-library");
    let library = dir.write("lib.fxc", "fn square(x) = x * x;\n");

    // Building it is fine: it is a valid, if empty, program.
    let out = fx50().arg("build").arg(&library).output().unwrap();
    assert!(
        out.status.success(),
        "{} has no main and must still build",
        library.display()
    );
    assert!(String::from_utf8_lossy(&out.stdout).trim().is_empty());

    // Running it is the error, and the message says why.
    let out = fx50().arg("run").arg(&library).output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success());
    assert!(stderr.contains("nothing to run"), "{stderr}");
    assert!(stderr.contains("fn main()"), "{stderr}");
}

/// `fx50 constants` lists every entry, formatted like the calculator's display.
#[test]
fn constants_lists_all_forty() {
    let out = fx50().arg("constants").output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{stdout}");
    assert_eq!(stdout.lines().count(), 40, "{stdout}");
    assert!(stdout.contains("Planck constant"), "{stdout}");
    assert!(stdout.contains("6.62606957e-34"), "{stdout}");
    // The elementary charge is shown under its own ASCII name.
    assert!(stdout.contains("eq"), "{stdout}");
    assert!(stdout.contains("elementary charge"), "{stdout}");
}

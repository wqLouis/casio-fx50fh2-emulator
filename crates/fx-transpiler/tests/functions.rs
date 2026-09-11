//! End-to-end tests for user-defined functions: inlining, call-by-name,
//! hygiene, `main`, and `#include`d libraries.
//!
//! Functions are expanded while transpiling, so the golden tests can pin the
//! exact PRGM, and the execution tests run it on the real interpreter.

use std::path::{Path, PathBuf};

use fx_transpiler::error::TranspileError;
use fx_transpiler::{
    Options, analyze, transpile, transpile_file, transpile_with, transpile_with_base,
};

/// The *unoptimised* translation, so each construct appears as written. The
/// optimiser is covered by `tests/simplify.rs` and `tests/propagate.rs`.
const RAW: Options = Options {
    ascii: false,
    mode: None,
    optimize: false,
};

#[track_caller]
fn glyph(source: &str, expected: &str) {
    let got =
        transpile_with(source, RAW).unwrap_or_else(|e| panic!("transpile failed: {e}\n{source}"));
    assert_eq!(got, expected, "glyph output for:\n{source}");
}

#[track_caller]
fn ascii(source: &str, expected: &str) {
    let got = transpile_with(source, Options { ascii: true, ..RAW })
        .unwrap_or_else(|e| panic!("transpile failed: {e}\n{source}"));
    assert_eq!(got, expected, "ASCII output for:\n{source}");
}

#[track_caller]
fn err(source: &str) -> TranspileError {
    transpile(source).unwrap_err()
}

// ---------------------------------------------------------------------------
// Inlining

#[test]
fn an_expression_function_inlines_to_its_expression() {
    glyph(
        "fn dbl(x) = x * 2;\nfn main() { let a = 3; print(dbl(a)); }",
        "3→A\nA×2◢\n",
    );
}

#[test]
fn arguments_are_substituted_where_they_are_used() {
    // Call by name: `x * x` with `x = 3` becomes `3 * 3`, which then folds.
    glyph("fn sq(x) = x * x;\nfn main() { print(sq(3)); }", "9◢\n");
    // A variable argument is emitted once per mention (and is not folded).
    glyph(
        "fn twice(x) = x + x;\nfn main() { let a = 1; print(twice(a)); }",
        "1→A\nA+A◢\n",
    );
}

#[test]
fn a_procedure_inlines_its_statements() {
    glyph(
        "fn show(n) { print(n); print(n + 1); }\nfn main() { show(4); }",
        "4◢\n5◢\n",
    );
}

#[test]
fn a_procedure_used_as_a_value_keeps_its_return() {
    glyph(
        "fn add(a, b) { let s = a + b; return s; }\nfn main() { print(add(2, 3)); }",
        "5→A\nA→B\nB◢\n",
    );
}

#[test]
fn nested_calls_inline_outside_in() {
    glyph(
        "fn sq(x) = x * x;\nfn add(a, b) = a + b;\nfn main() { print(sq(add(2, 3))); }",
        "25◢\n",
    );
}

#[test]
fn an_uncalled_function_emits_nothing() {
    glyph("fn unused(x) = x + 1;\nfn main() { print(1); }", "1◢\n");
}

#[test]
fn main_becomes_the_program() {
    glyph("fn main() { print(7); }", "7◢\n");
    ascii("fn main() { print(7); }", "7disp\n");
}

#[test]
fn functions_do_not_consume_memories() {
    // `sq` is inlined and never allocated; only `a` uses a memory.
    let analysis = analyze(
        "fn sq(x) = x * x;\nfn main() { let a = input(); print(sq(a)); }",
        Path::new("."),
    )
    .unwrap();
    assert_eq!(analysis.allocation.bindings.len(), 1);
    assert_eq!(analysis.allocation.bindings[0].name, "a");
}

// ---------------------------------------------------------------------------
// Call by name: pass-by-reference for variables

#[test]
fn assigning_a_parameter_writes_back_through_the_argument() {
    glyph(
        "fn inc(x) { x = x + 1; }\nfn main() { let a = 5; inc(a); print(a); }",
        "5→A\nA+1→A\nA◢\n",
    );
}

#[test]
fn output_parameters_give_a_procedure_two_results() {
    glyph(
        "fn split(a, b, lo, hi) { lo = a; hi = b; if (a > b) { lo = b; hi = a; } }\n\
         fn main() { let x = 3; let y = 7; let p = 0; let q = 0; split(x, y, p, q); print(p); }",
        "3→A\n7→B\n0→C\n0→D\nA→C\nB→D\nIf A>B\nThen\nB→C\nA→D\nIfEnd\nC◢\n",
    );
}

#[test]
fn assigning_a_parameter_needs_an_assignable_argument() {
    let e = err("fn inc(x) { x = x + 1; }\nfn main() { inc(1 + 2); }");
    assert!(
        e.message.contains("must be a variable or an array element"),
        "{}",
        e.message
    );
}

#[test]
fn a_parameter_may_be_an_array_element() {
    glyph(
        "fn bump(x) { x = x + 1; }\nfn main() { let v[2] = {1, 2}; bump(v[1]); print(v[1]); }",
        "1→A\n2→B\nB+1→B\nB◢\n",
    );
}

// ---------------------------------------------------------------------------
// Hygiene: locals cannot collide with the caller or with other calls

#[test]
fn a_local_does_not_clobber_a_caller_variable_of_the_same_name() {
    glyph(
        "fn dbl(n) { let t = n * 2; return t; }\nfn main() { let t = 100; print(dbl(3)); print(t); }",
        "100→A\n6→B\nB→C\nC◢\nA◢\n",
    );
}

#[test]
fn two_calls_have_independent_locals() {
    // Both expansions declare a local `s`; the emitter must not merge them.
    // Unoptimised, so the copy from the parameter (`A→B`) is visible; the
    // optimiser would fold the literal argument through it.
    let prgm = transpile_with(
        "fn add(a, b) { let s = a + b; return s; }\nfn main() { print(add(1, 2)); print(add(3, 4)); }",
        RAW,
    )
    .unwrap();
    assert_eq!(prgm, "3→A\nA→B\nB◢\n7→C\nC→D\nD◢\n");
}

#[test]
fn a_local_may_be_used_in_a_loop_inside_the_function() {
    glyph(
        "fn sum_to(n) { let total = 0; for (let i = 1; i <= n; i = i + 1) { total = total + i; } return total; }\nfn main() { print(sum_to(3)); }",
        "0→A\nFor 1→B To 3 Step 1\nA+B→A\nNext\nA→C\nC◢\n",
    );
}

// ---------------------------------------------------------------------------
// Errors

#[test]
fn recursion_is_rejected() {
    let e = err("fn f(n) = f(n);\nfn main() { print(f(1)); }");
    assert!(e.message.contains("recursive"), "{}", e.message);

    // Through another function, too.
    let e = err("fn a(n) = b(n);\nfn b(n) = a(n);\nfn main() { print(a(1)); }");
    assert!(e.message.contains("recursive"), "{}", e.message);
    assert!(e.message.contains("a → b → a"), "{}", e.message);
}

#[test]
fn a_return_must_be_the_last_statement() {
    let e = err("fn f(n) { if (n) { return 1; } return 2; }\nfn main() { print(f(1)); }");
    assert!(e.message.contains("last statement"), "{}", e.message);
}

#[test]
fn a_stray_return_is_rejected() {
    let e = err("return 1;");
    assert!(e.message.contains("only valid inside"), "{}", e.message);
}

#[test]
fn an_unknown_function_is_reported() {
    let e = err("fn main() { print(nope(1)); }");
    assert!(
        e.message.contains("unknown function `nope`"),
        "{}",
        e.message
    );
}

#[test]
fn the_wrong_number_of_arguments_is_rejected() {
    let e = err("fn f(a) = a;\nfn main() { print(f(1, 2)); }");
    assert!(e.message.contains("expects 1 argument(s)"), "{}", e.message);
}

#[test]
fn main_and_top_level_statements_cannot_be_mixed() {
    let e = err("fn main() { print(1); }\nprint(2);");
    assert!(e.message.contains("top level"), "{}", e.message);
}

#[test]
fn a_procedure_cannot_be_called_from_a_loop_condition() {
    let e = err(
        "fn f(n) { let t = n; return t; }\nfn main() { let a = 0; while (f(a) > 0) { a = a + 1; } }",
    );
    assert!(e.message.contains("loop condition"), "{}", e.message);
}

#[test]
fn a_procedure_without_a_return_cannot_be_used_as_a_value() {
    let e = err("fn f(n) { let t = n; }\nfn main() { print(f(1)); }");
    assert!(e.message.contains("never returns a value"), "{}", e.message);
}

#[test]
fn a_shadowing_local_is_rejected() {
    let e = err("fn f(a) { let a = 1; return a; }\nfn main() { print(f(2)); }");
    assert!(
        e.message.contains("both a parameter and a local"),
        "{}",
        e.message
    );
}

#[test]
fn a_duplicate_parameter_is_rejected() {
    let e = err("fn f(a, a) = a;\nfn main() { print(f(1, 2)); }");
    assert!(e.message.contains("listed twice"), "{}", e.message);
}

#[test]
fn a_builtin_name_cannot_be_redefined() {
    let e = err("fn sqrt(x) = x;\nfn main() { print(sqrt(1)); }");
    assert!(e.message.contains("built-in function"), "{}", e.message);
}

// ---------------------------------------------------------------------------
// `#include` carries functions

/// A scratch directory that cleans itself up.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "fx-transpiler-functions-{tag}-{}",
            std::process::id()
        ));
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

#[test]
fn an_included_library_provides_functions() {
    let dir = TempDir::new("library");
    dir.write(
        "math.fxc",
        "fn square(x) = x * x;\nfn cubed(x) = x * x * x;\n",
    );
    let main = dir.write(
        "main.fxc",
        "#include \"math.fxc\"\nfn main() { print(square(4)); print(cubed(2)); }\n",
    );
    let prgm = transpile_file(&main, Options::default()).unwrap();
    assert_eq!(prgm, "16◢\n8◢\n");
}

#[test]
fn a_library_function_may_call_another_library_function() {
    let dir = TempDir::new("nested-lib");
    dir.write("math.fxc", "fn square(x) = x * x;\n");
    dir.write(
        "main.fxc",
        "#include \"math.fxc\"\nfn quad(x) = square(square(x));\nfn main() { print(quad(2)); }\n",
    );
    let prgm = transpile_file(&dir.0.join("main.fxc"), Options::default()).unwrap();
    assert_eq!(prgm, "16◢\n");
}

#[test]
fn a_library_may_not_declare_a_mode_even_with_functions() {
    let dir = TempDir::new("mode-frag");
    dir.write("lib.fxc", "#mode CMPLX\nfn f(x) = x;\n");
    let main = dir.write(
        "main.fxc",
        "#include \"lib.fxc\"\nfn main() { print(f(1)); }\n",
    );
    let e = transpile_file(&main, Options::default()).unwrap_err();
    assert!(
        e.message.contains("`#mode` may only appear"),
        "{}",
        e.message
    );
}

#[test]
fn a_library_error_points_at_the_library_line() {
    let dir = TempDir::new("lib-error");
    dir.write("bad.fxc", "fn f(x) = x;\nfn g(x) = f(x, x);\n");
    let main = dir.write(
        "main.fxc",
        "#include \"bad.fxc\"\nfn main() { print(g(1)); }\n",
    );
    let e = transpile_file(&main, Options::default()).unwrap_err();
    assert!(e.message.contains("expects 1 argument(s)"), "{}", e.message);
    assert!(
        e.file.as_deref().is_some_and(|f| f.ends_with("bad.fxc")),
        "the error should name the library: {e:?}"
    );
}

// ---------------------------------------------------------------------------
// Execution

#[cfg(feature = "execute")]
mod run {
    use casio_fx50fh2::{Interpreter, MockHost};
    use fx_transpiler::transpile;

    #[track_caller]
    fn run(source: &str, inputs: &[f64]) -> Vec<String> {
        let prgm = transpile(source).unwrap_or_else(|e| panic!("transpile failed: {e}\n{source}"));
        let program = casio_fx50fh2::compile(&prgm)
            .unwrap_or_else(|e| panic!("PRGM failed to compile: {e}\n---\n{prgm}"));
        let mut interp = Interpreter::new(program, MockHost::with_inputs(inputs.iter().copied()));
        interp
            .run()
            .unwrap_or_else(|e| panic!("PRGM failed to run: {e}\n---\n{prgm}"));
        interp.host().output.clone()
    }

    #[test]
    fn an_expression_function_runs() {
        assert_eq!(
            run(
                "fn hypot(a, b) = sqrt(a*a + b*b);\nfn main() { print(hypot(3, 4)); }",
                &[]
            ),
            vec!["5"]
        );
    }

    #[test]
    fn a_procedure_with_a_loop_runs() {
        let source = "\
fn sum_to(n) {
    let total = 0;
    for (let i = 1; i <= n; i = i + 1) { total = total + i; }
    return total;
}
fn main() { print(sum_to(10)); }
";
        assert_eq!(run(source, &[]), vec!["55"]);
    }

    #[test]
    fn call_by_reference_through_a_parameter_runs() {
        let source = "\
fn inc(x) { x = x + 1; }
fn main() { let a = 5; inc(a); inc(a); print(a); }
";
        assert_eq!(run(source, &[]), vec!["7"]);
    }

    #[test]
    fn output_parameters_run() {
        let source = "\
fn minmax(a, b, lo, hi) {
    lo = a;
    hi = b;
    if (a > b) { lo = b; hi = a; }
}
fn main() {
    let x = 3;
    let y = 7;
    let small = 0;
    let big = 0;
    minmax(x, y, small, big);
    print(small);
    print(big);
}
";
        assert_eq!(run(source, &[]), vec!["3", "7"]);
    }

    #[test]
    fn a_call_by_name_argument_is_reevaluated() {
        // `ran()` is mentioned twice, so the two draws differ, exactly as if
        // the expression had been written out twice.
        let source = "fn twice(x) = x + x;\nfn main() { print(twice(ran())); }";
        let out = run(source, &[]);
        assert_eq!(out.len(), 1);
        assert_ne!(out[0], "1.354422336", "{out:?}");
    }

    #[test]
    fn hygiene_survives_execution() {
        let source = "\
fn dbl(n) { let t = n * 2; return t; }
fn main() { let t = 100; print(dbl(3)); print(t); }
";
        assert_eq!(run(source, &[]), vec!["6", "100"]);
    }

    #[test]
    fn a_function_can_use_input_and_print() {
        let source = "\
fn ask() { let x = input(); return x * 2; }
fn main() { print(ask()); }
";
        assert_eq!(run(source, &[21.0]), vec!["42"]);
    }

    #[test]
    fn a_function_can_be_called_from_a_loop_body() {
        let source = "\
fn sq(x) = x * x;
fn main() {
    for (let i = 1; i <= 3; i = i + 1) { print(sq(i)); }
}
";
        assert_eq!(run(source, &[]), vec!["1", "4", "9"]);
    }

    #[test]
    fn two_calls_to_the_same_procedure_do_not_share_locals() {
        let source = "\
fn add(a, b) { let s = a + b; return s; }
fn main() { print(add(1, 2)); print(add(10, 20)); }
";
        assert_eq!(run(source, &[]), vec!["3", "30"]);
    }
}

// ---------------------------------------------------------------------------
// `transpile_with_base` resolves includes for inline sources, too.

#[test]
fn inline_source_gets_functions_from_an_include() {
    let dir = TempDir::new("inline-base");
    dir.write("lib.fxc", "fn triple(x) = x * 3;\n");
    let prgm = transpile_with_base(
        "#include \"lib.fxc\"\nfn main() { print(triple(4)); }",
        Options::default(),
        &dir.0,
    )
    .unwrap();
    assert_eq!(prgm, "12◢\n");
}

// ---------------------------------------------------------------------------
// The entry point and scoping

#[test]
fn a_file_without_main_is_a_library() {
    // A library is not a program: it exists to be `#include`d, so it builds to
    // an empty program rather than reporting a missing entry point.
    assert_eq!(transpile("fn f(x) = x + 1;").unwrap(), "");
    assert_eq!(transpile("const k = 1;").unwrap(), "");
}

#[test]
fn a_library_is_checked_on_its_own() {
    // The scope check runs whether or not there is a `main`, so a typo in a
    // library is reported where it is written rather than only when another
    // program includes it.
    let e = err("fn square() = n * n;");
    assert!(e.message.contains("`n` is not defined"), "{}", e.message);
    // A function that names its dependency is fine.
    assert_eq!(transpile("fn square(n) = n * n;").unwrap(), "");
}

#[test]
fn a_loose_statement_still_needs_an_entry_point() {
    let e = err("print(1);");
    assert!(e.message.contains("needs an entry point"), "{}", e.message);
}

#[test]
fn top_level_statements_are_rejected_when_functions_exist() {
    let e = err("fn main() { print(1); }\nprint(2);");
    assert!(e.message.contains("top level"), "{}", e.message);
}

#[test]
fn loose_top_level_statements_need_a_main() {
    // Every program is a set of functions with an entry point, so the classic
    // loose-statement form is gone; `main` is where those statements live now.
    let e = err("print(a + 1);");
    assert!(e.message.contains("entry point"), "{}", e.message);

    // Inside `main` the classic order-free rules still apply: the wrapper adds
    // no declarations, so `a` is introduced by its first use exactly as before.
    glyph("fn main() { print(a + 1); }", "A+1◢\n");
}

#[test]
fn a_top_level_const_reaches_functions_but_costs_no_memory() {
    glyph(
        "const k = 2;\nfn scale(x) = x * k;\nfn main() { print(scale(3)); }",
        "6◢\n",
    );
    // `k` is inlined, so even the inlined body uses no memory for it.
    // `input()` keeps the store alive, so there is a plan to inspect.
    let analysis = analyze(
        "const k = 2;\nfn scale(x) = x * k;\nfn main() { let a = input(); print(scale(a)); }",
        Path::new("."),
    )
    .unwrap();
    assert_eq!(analysis.consts, vec!["k"]);
    assert_eq!(analysis.allocation.used(), 1);
}

#[test]
fn a_data_table_reaches_functions() {
    glyph(
        // `glyph` is the unoptimised translation, but the data path still folds
        // (folding is not optional), so the argument is multiplied out.
        "#data cfg = { \"n\": 4 };\nfn scale(x) = x * cfg.n;\nfn main() { print(scale(2)); }",
        "8◢\n",
    );
}

#[test]
fn a_function_cannot_read_an_undeclared_global() {
    let e = err("fn f(x) = x + a;\nfn main() { print(f(1)); }");
    assert!(
        e.message.contains("`a` is not defined in `f`"),
        "{}",
        e.message
    );
}

#[test]
fn a_function_cannot_create_an_implicit_global() {
    let e = err("fn f() { let x = 1; y = x + 1; }\nfn main() { f(); }");
    assert!(
        e.message.contains("`y` is not defined in `f`"),
        "{}",
        e.message
    );
}

#[test]
fn a_forward_reference_inside_a_function_still_works() {
    let prgm =
        transpile("fn f() { let a = b; let b = 1; return a; }\nfn main() { print(f()); }").unwrap();
    assert!(!prgm.is_empty(), "{prgm}");
}

#[test]
fn included_libraries_do_not_pollute_the_calling_namespace() {
    // The library declares a local `total`; the caller has its own `total`.
    let dir = TempDir::new("no-pollution");
    dir.write("lib.fxc", "fn sum_to(n) { let total = 0; for (let i = 1; i <= n; i = i + 1) { total = total + i; } return total; }\n");
    let main = dir.write(
        "main.fxc",
        "#include \"lib.fxc\"\nfn main() { let total = 100; print(sum_to(3)); print(total); }\n",
    );
    let prgm = transpile_file(&main, Options::default()).unwrap();
    // The two `total`s are separate: the library's accumulates in a memory, and
    // the caller's 100 is propagated as a literal and needs no memory at all —
    // which is the strongest possible proof that the library never touched it.
    assert!(prgm.ends_with("100◢\n"), "{prgm}");
    assert!(
        !prgm.contains("100→"),
        "the caller's 100 needs no memory: {prgm}"
    );
}

//! Tests for the compile-time data facility (`#data`, `#tests`) and `const`,
//! and for the memory plan (`#reg`, `analyze`).
//!
//! These are end-to-end: they go through the public `transpile*` entry points,
//! so includes, data extraction, lexing, parsing, allocation and emission are
//! all exercised together.

use fx_transpiler::{Options, analyze, transpile};

#[track_caller]
fn out(source: &str) -> String {
    transpile(source).unwrap_or_else(|e| panic!("transpile failed: {e}\n{source}"))
}

#[track_caller]
fn err(source: &str) -> fx_transpiler::error::TranspileError {
    transpile(source).unwrap_err()
}

// ---------------------------------------------------------------------------
// `#data`

#[test]
fn a_bare_data_name_becomes_a_literal() {
    assert_eq!(out("#data n = 3;\nprint(n);\n"), "3◢\n");
}

#[test]
fn data_paths_resolve_through_objects_and_arrays() {
    let source = "#data c = { \"a\": 2, \"xs\": [5, 6] };\nlet x = c.a + c.xs[1]; print(x);\n";
    assert_eq!(out(source), "2+6→A\nA◢\n");
}

#[test]
fn booleans_become_one_and_zero() {
    assert_eq!(out("#data on = true;\nprint(on);\n"), "1◢\n");
}

#[test]
fn data_uses_no_memory() {
    let source = "#data c = { \"a\": 1, \"b\": 2 };\nlet x = c.a; print(x + c.b);\n";
    let analysis = analyze(source, std::path::Path::new(".")).unwrap();
    assert_eq!(analysis.data, vec!["c"]);
    assert_eq!(analysis.allocation.bindings.len(), 1);
    assert_eq!(analysis.allocation.bindings[0].name, "x");
    assert_eq!(analysis.allocation.bindings[0].memory, 'A');
}

#[test]
fn a_data_file_is_read_relative_to_the_source() {
    let dir = std::env::temp_dir().join("fx_compiletime_data_file");
    std::fs::create_dir_all(&dir).unwrap();
    let data = dir.join("values.json");
    std::fs::write(&data, r#"{ "scale": 10 }"#).unwrap();
    let source = "#data v = \"values.json\";\nprint(v.scale * 2);\n";
    // `transpile_with_base` resolves the file against `dir`.
    let prgm = fx_transpiler::transpile_with_base(source, Options::default(), &dir).unwrap();
    assert_eq!(prgm, "10×2◢\n");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_missing_data_file_is_reported() {
    let e = err("#data v = \"no-such-file.json\";\nprint(1);\n");
    assert!(e.message.contains("cannot read data file"), "{e}");
}

#[test]
fn bad_data_json_is_reported_with_a_position() {
    let e = err("#data c = { \"a\": };\nprint(1);\n");
    assert!(e.message.contains("invalid JSON"), "{e}");
    assert_eq!(e.line, 1);
}

#[test]
fn unknown_paths_are_reported_precisely() {
    let e = err("#data c = { \"a\": 1 };\nprint(c.b);\n");
    assert!(e.message.contains("has no field `b`"), "{e}");
    assert!(e.message.contains("fields: a"), "{e}");

    let e = err("print(config.x);\n");
    assert!(e.message.contains("unknown data table `config`"), "{e}");

    let e = err("#data xs = [1, 2];\nprint(xs[9]);\n");
    assert!(e.message.contains("out of range"), "{e}");

    let e = err("#data c = { \"s\": \"hi\" };\nprint(c.s);\n");
    assert!(e.message.contains("only numbers and booleans"), "{e}");
}

#[test]
fn duplicate_data_names_are_rejected() {
    let e = err("#data a = 1;\n#data a = 2;\n");
    assert!(e.message.contains("already declared"), "{e}");
}

// ---------------------------------------------------------------------------
// `let` declares, and may declare again after a `free`

#[test]
fn let_declares_again_after_a_free() {
    // The same name, two lives: the memory is released and taken again.
    let source = "let x = input();\nprint(x);\nfree x;\nlet x = input();\nprint(x);\n";
    assert_eq!(out(source), "?→A\nA◢\n?→A\nA◢\n");
}

#[test]
fn let_of_a_live_name_is_an_error() {
    let e = err("let x = input();\nlet x = input();\n");
    assert!(e.message.contains("already declared"), "{e}");
    assert!(e.message.contains("free x"), "{e}");
}

#[test]
fn a_freed_name_may_be_revived_by_let_only() {
    // `let` redeclares; a bare assignment does not.
    let e = err("let x = 1;\nfree x;\nx = 2;\n");
    assert!(e.message.contains("not defined here"), "{e}");
}

#[test]
fn let_matches_the_documented_lifetime_rules() {
    // Allowed: two lives for one name, separated by a `free`.
    assert_eq!(
        out("let x = input();\nfree x;\nlet x = input();\nprint(x);\n"),
        "?→A\n?→A\nA◢\n"
    );

    // Forbidden: a second declaration while the first is still live.
    let e = err("let x = input();\nlet x = input();\n");
    assert!(e.message.contains("already declared"), "{e}");

    // Forbidden: a `const` of a live variable's name.
    let e = err("let x = 1;\nconst x = 2;\n");
    assert!(
        e.message.contains("`const`") || e.message.contains("already declared"),
        "{e}"
    );

    // Forbidden: the name is not in scope in its own initializer.
    let e = err("let x = input();\nfree x;\nlet x = x + 1;\n");
    assert!(e.message.contains("its own initializer"), "{e}");
}

#[test]
fn const_of_a_live_variable_is_an_error() {
    let e = err("let x = 1;\nconst x = 2;\nprint(x);\n");
    assert!(
        e.message.contains("`const`") || e.message.contains("already declared"),
        "{e}"
    );
}

#[test]
fn a_declaration_cannot_use_its_own_name() {
    let e = err("let x = 1;\nfree x;\nlet x = x + 1;\n");
    assert!(e.message.contains("its own initializer"), "{e}");
}

// ---------------------------------------------------------------------------
// `unsafe_free` and jumps

#[test]
fn a_checked_free_with_a_jump_is_an_error() {
    let e = err("let a = 1;\nfree a;\ngoto 1;\nlabel 1;\n");
    assert!(e.message.contains("`goto`/`label`"), "{e}");
    assert!(e.message.contains("unsafe_free"), "{e}");
}

#[test]
fn unsafe_free_is_allowed_with_a_jump() {
    let source = "let a = 1;\nunsafe_free a;\nlet b = 2;\ngoto 1;\nlabel 1;\nprint(b);\n";
    assert_eq!(out(source), "1→A\n2→A\nGoto 1\nLbl 1\nA◢\n");
}

#[test]
fn unsafe_free_still_checks_the_name() {
    let e = err("unsafe_free nope;\n");
    assert!(e.message.contains("is not a variable"), "{e}");

    let e = err("let a = 1;\nfree a;\nunsafe_free a;\n");
    assert!(e.message.contains("double free"), "{e}");
}

#[test]
fn a_jump_without_free_is_fine() {
    let source = "let a = 1;\ngoto 1;\nlabel 1;\nprint(a);\n";
    assert_eq!(out(source), "1→A\nGoto 1\nLbl 1\nA◢\n");
}

// ---------------------------------------------------------------------------
// `const`

#[test]
fn a_const_is_inlined_and_uses_no_memory() {
    assert_eq!(out("const k = 4;\nprint(k * 2);\n"), "4×2◢\n");
}

#[test]
fn a_const_may_build_on_an_earlier_const() {
    assert_eq!(out("const a = 2;\nconst b = a + 3;\nprint(b);\n"), "2+3◢\n");
}

#[test]
fn a_const_may_hold_a_calculator_constant_symbolically() {
    // `phys.h` stays symbolic, and `const` costs no memory either way.
    assert_eq!(out("const c = 2 * phys.h;\nprint(c);\n"), "2×h◢\n");
}

#[test]
fn a_const_frees_a_memory_for_a_variable() {
    let analysis = analyze(
        "const k = 1;\nlet a = k; print(a);\n",
        std::path::Path::new("."),
    )
    .unwrap();
    assert_eq!(analysis.consts, vec!["k"]);
    assert_eq!(analysis.allocation.bindings.len(), 1);
    assert_eq!(analysis.allocation.bindings[0].name, "a");
    assert_eq!(analysis.allocation.bindings[0].memory, 'A');
}

#[test]
fn a_const_must_be_a_constant_expression() {
    let e = err("let x = 1;\nconst k = x;\nprint(k);\n");
    assert!(e.message.contains("must be a constant expression"), "{e}");

    let e = err("const k = input();\nprint(k);\n");
    assert!(e.message.contains("cannot use `input()`"), "{e}");

    let e = err("const k = sqrt(4);\nprint(k);\n");
    assert!(e.message.contains("cannot call `sqrt`"), "{e}");
}

#[test]
fn a_const_must_be_declared_before_use() {
    let e = err("print(k);\nconst k = 2;\n");
    assert!(e.message.contains("declared later"), "{e}");
}

#[test]
fn a_const_may_not_be_redeclared() {
    let e = err("const k = 1;\nconst k = 2;\nprint(k);\n");
    assert!(e.message.contains("already declared as a `const`"), "{e}");
}

#[test]
fn a_const_name_is_not_a_variable() {
    let e = err("const k = 1;\nlet k = 2;\nprint(k);\n");
    assert!(e.message.contains("is a `const`"), "{e}");
}

#[test]
fn base_mode_rejects_a_float_const_value() {
    let e = err("#mode BASE\nconst k = pi;\nprint(k);\n");
    assert!(e.message.contains("not available in BASE"), "{e}");
}

// ---------------------------------------------------------------------------
// `free` and the memory plan

#[test]
fn free_releases_a_memory_for_a_later_variable() {
    let source = "let first = 5;\nprint(first);\nfree first;\nlet second = 7;\nprint(second);\n";
    // `second` is allocated to the memory `free first` released, so the program
    // needs only one of the seven.
    assert_eq!(out(source), "5→A\nA◢\n7→A\nA◢\n");
}

#[test]
fn free_emits_nothing() {
    assert_eq!(out("let a = 1;\nfree a;\n"), "1→A\n");
}

#[test]
fn free_is_tracked_by_the_allocator() {
    let source = "let t = 1;\nfree t;\nlet u = 2;\nprint(u);\n";
    let analysis = analyze(source, std::path::Path::new(".")).unwrap();
    assert_eq!(analysis.allocation.freed, vec!["t"]);
    assert_eq!(
        analysis
            .allocation
            .bindings
            .iter()
            .map(|b| (b.name.as_str(), b.memory))
            .collect::<Vec<_>>(),
        vec![("t", 'A'), ("u", 'A')]
    );
    assert_eq!(analysis.allocation.used(), 1);
}

#[test]
fn a_program_without_free_keeps_every_memory_separate() {
    let analysis = analyze(
        "let a = 1; let b = 2; print(a + b);\n",
        std::path::Path::new("."),
    )
    .unwrap();
    assert_eq!(analysis.allocation.used(), 2);
    assert_eq!(analysis.allocation.free(), vec!['C', 'D', 'X', 'Y', 'M']);
}

#[test]
fn double_free_is_a_transpile_error() {
    let e = err("let a = 1;\nfree a;\nfree a;\n");
    assert!(e.message.contains("double free"), "{e}");
}

#[test]
fn use_after_free_is_a_transpile_error() {
    let e = err("let a = 1;\nfree a;\nprint(a);\n");
    assert!(e.message.contains("was freed"), "{e}");
}

#[test]
fn freeing_a_compile_time_name_is_a_transpile_error() {
    let e = err("const k = 1;\nfree k;\nprint(k);\n");
    assert!(e.message.contains("uses no memory"), "{e}");
}

#[test]
fn free_with_a_jump_is_a_transpile_error() {
    let e = err("let a = 1;\nfree a;\ngoto 1;\nlabel 1;\n");
    assert!(e.message.contains("`goto`/`label`"), "{e}");
}

#[test]
fn eight_variables_do_not_fit_and_the_error_says_so() {
    let source = "let a=1; let b=1; let c=1; let d=1; let x=1; let y=1; let m=1; let z=1;";
    let e = err(source);
    assert!(e.message.contains("no free memory for `z`"), "{e}");
    assert!(e.message.contains("Use `const`"), "{e}");
    assert!(e.message.contains("`free`"), "{e}");
}

#[test]
fn replaying_the_same_program_gives_the_same_plan() {
    // Determinism matters: `regs` and `build` must agree.
    let source = "let b = 1; let a = 2;\nprint(a + b);\n";
    let first = analyze(source, std::path::Path::new(".")).unwrap();
    let second = analyze(source, std::path::Path::new(".")).unwrap();
    assert_eq!(first, second);
    assert_eq!(out(source), "1→A\n2→B\nB+A◢\n");
}

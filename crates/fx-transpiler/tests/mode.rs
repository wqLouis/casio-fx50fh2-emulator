//! Tests for the `#mode` header and [`fx_transpiler::Options::mode`] override.

use fx_transpiler::error::TranspileError;
use fx_transpiler::{
    Mode, Options, transpile as raw_transpile, transpile_with as raw_transpile_with,
};

mod common;

/// Transpile, wrapping the classic top-level statements in `fn main()`.
fn transpile(source: &str) -> Result<String, TranspileError> {
    raw_transpile(&common::wrap(source))
}

/// Transpile with options, wrapping the classic top-level statements in
/// `fn main()`.
fn transpile_with(source: &str, options: Options) -> Result<String, TranspileError> {
    raw_transpile_with(&common::wrap(source), options)
}

/// Transpile with default options and unwrap, for the success cases.
#[track_caller]
fn ok(source: &str) -> String {
    transpile(source).unwrap_or_else(|e| panic!("transpile failed: {e}\n{source}"))
}

/// Transpile with an explicit mode override.
#[track_caller]
fn ok_mode(source: &str, mode: Mode) -> String {
    transpile_with(
        source,
        Options {
            ascii: false,
            mode: Some(mode),
        },
    )
    .unwrap_or_else(|e| panic!("transpile failed: {e}\n{source}"))
}

#[track_caller]
fn err(source: &str) -> TranspileError {
    transpile(source).unwrap_err()
}

// ---------------------------------------------------------------------------
// Header parsing: every canonical name, case-insensitively

#[test]
fn all_canonical_names_are_accepted_and_re_emitted() {
    for (text, canonical) in [
        ("COMP", "COMP"),
        ("CMPLX", "CMPLX"),
        ("BASE", "BASE"),
        ("SD", "SD"),
        ("REG", "REG"),
    ] {
        assert_eq!(
            ok(&format!("#mode {text}\nprint(1);")),
            format!("#mode {canonical}\n1◢\n"),
            "header `{text}`"
        );
    }
}

#[test]
fn header_is_case_insensitive() {
    assert_eq!(ok("#mode cmplx\nprint(1);"), "#mode CMPLX\n1◢\n");
    assert_eq!(ok("#MoDe sD\nprint(1);"), "#mode SD\n1◢\n");
    assert_eq!(ok("#MODE reg\nprint(1);"), "#mode REG\n1◢\n");
}

#[test]
fn aliases_map_to_canonical_names() {
    assert_eq!(ok("#mode STAT\nprint(1);"), "#mode SD\n1◢\n");
    assert_eq!(ok("#mode CPLX\nprint(1);"), "#mode CMPLX\n1◢\n");
    assert_eq!(ok("#mode BASE-N\nprint(1);"), "#mode BASE\n1◢\n");
}

#[test]
fn optional_equals_and_spaces_are_allowed() {
    assert_eq!(ok("#mode=CMPLX\nprint(1);"), "#mode CMPLX\n1◢\n");
    assert_eq!(ok("#mode = cmplx\nprint(1);"), "#mode CMPLX\n1◢\n");
    assert_eq!(ok("#mode\t=\tBASE\nprint(1);"), "#mode BASE\n1◢\n");
}

#[test]
fn trailing_line_comment_is_allowed() {
    assert_eq!(
        ok("#mode CMPLX // complex from here\nprint(1);"),
        "#mode CMPLX\n1◢\n"
    );
}

#[test]
fn comments_and_blank_lines_may_precede_the_header() {
    // The comments sit above `fn main()`, which the lexer skips when it looks
    // for the first non-comment line.
    assert_eq!(
        ok("// a comment\n\n#mode CMPLX\nfn main() { print(1); }"),
        "#mode CMPLX\n1◢\n"
    );
    assert_eq!(
        ok("/* block */\n#mode SD\nfn main() { print(1); }"),
        "#mode SD\n1◢\n"
    );
}

#[test]
fn header_is_not_parsed_as_a_statement() {
    assert_eq!(ok("#mode CMPLX\nlet a = 1;"), "#mode CMPLX\n1→A\n");
}

#[test]
fn header_only_program_is_just_the_header() {
    assert_eq!(ok("#mode CMPLX\n"), "#mode CMPLX\n");
    assert_eq!(ok("#mode CMPLX"), "#mode CMPLX\n");
}

// ---------------------------------------------------------------------------
// Override precedence and the implicit default

#[test]
fn no_header_and_no_override_emits_nothing() {
    assert_eq!(ok("print(1);"), "1◢\n");
    assert_eq!(ok(""), "");
}

#[test]
fn override_wins_over_the_header() {
    assert_eq!(
        ok_mode("#mode CMPLX\nprint(1);", Mode::Base),
        "#mode BASE\n1◢\n"
    );
    assert_eq!(
        ok_mode("#mode COMP\nprint(1);", Mode::Reg),
        "#mode REG\n1◢\n"
    );
}

#[test]
fn override_is_emitted_without_a_header() {
    assert_eq!(ok_mode("print(1);", Mode::Cmplx), "#mode CMPLX\n1◢\n");
    assert_eq!(ok_mode("let a = 1;", Mode::Sd), "#mode SD\n1→A\n");
}

#[test]
fn explicit_comp_override_is_still_emitted() {
    // An explicit `--mode COMP` is distinguishable from "no mode at all".
    assert_eq!(ok_mode("print(1);", Mode::Comp), "#mode COMP\n1◢\n");
}

// ---------------------------------------------------------------------------
// Errors

#[test]
fn unknown_mode_points_at_the_name() {
    let e = err("#mode GRAPH\nprint(1);\n");
    assert!(e.message.contains("unknown mode `GRAPH`"), "{}", e.message);
    assert_eq!((e.line, e.column), (1, 7));
}

#[test]
fn missing_mode_name_is_an_error() {
    let e = err("#mode\nprint(1);\n");
    assert!(e.message.contains("expected a mode name"), "{}", e.message);
    assert_eq!((e.line, e.column), (1, 6));
}

#[test]
fn trailing_junk_after_the_name_is_an_error() {
    let e = err("#mode CMPLX print(1);\n");
    assert!(
        e.message.contains("unexpected text after the mode name"),
        "{}",
        e.message
    );
}

#[test]
fn header_must_be_first() {
    let e = err("print(1);\n#mode CMPLX\n");
    assert!(e.message.contains("must be the first"), "{}", e.message);
    // Wrapping puts the statement inside `fn main()`, so the stray header is
    // now on the third line.
    assert_eq!((e.line, e.column), (3, 1));

    // A second header is also "not first".
    let e = err("#mode COMP\n#mode CMPLX\n");
    assert!(e.message.contains("must be the first"), "{}", e.message);
    assert_eq!((e.line, e.column), (2, 1));
}

#[test]
fn unknown_directive_is_an_error() {
    let e = err("#nope CMPLX\nprint(1);\n");
    assert!(e.message.contains("unknown directive"), "{}", e.message);
    assert_eq!((e.line, e.column), (1, 1));
}

// ---------------------------------------------------------------------------
// BASE capability checks

#[test]
fn base_rejects_float_builtins() {
    for name in [
        "sqrt", "sin", "cos", "tan", "asin", "acos", "atan", "log", "ln", "rnd",
    ] {
        let source = format!("#mode BASE\nprint({name}(a));\n");
        let e = err(&source);
        assert!(
            e.message
                .contains(&format!("`{name}` is not available in BASE")),
            "{name}: {}",
            e.message
        );
    }
}

#[test]
fn base_rejects_pi_and_e() {
    let e = err("#mode BASE\nlet a = pi;\n");
    assert!(
        e.message.contains("`pi` is not available in BASE"),
        "{}",
        e.message
    );

    let e = err("#mode BASE\nprint(e);\n");
    assert!(
        e.message.contains("`e` is not available in BASE"),
        "{}",
        e.message
    );
}

#[test]
fn base_rejects_float_builtins_nested_in_other_expressions() {
    // The first forbidden built-in in source order is reported. `abs` comes
    // before `sqrt` here, so it is the one named.
    let e = err("#mode BASE\nlet a = 1; while (abs(x) < sqrt(a)) { print(a); }\n");
    assert!(
        e.message.contains("`abs` is not available in BASE"),
        "{}",
        e.message
    );
    // ...and `sqrt` on its own is caught in the same position.
    let e = err("#mode BASE\nlet a = 1; while (sqrt(a) < 2) { print(a); }\n");
    assert!(
        e.message.contains("`sqrt` is not available in BASE"),
        "{}",
        e.message
    );
}

#[test]
fn base_allows_integer_arithmetic_and_division() {
    assert_eq!(
        ok("#mode BASE\nlet a = 10; let b = 3; print(a / b);"),
        "#mode BASE\n10→A\n3→B\nA÷B◢\n"
    );
}

/// Every floating-point built-in is rejected in BASE, because the interpreter
/// rejects all of them. A hardcoded subset once let `abs` and `cbrt` pass
/// `build` and then fail at `run` time.
///
/// `not`/`neg` are the base-n keys and are offered in BASE; `ans`/`mvalue`
/// read fixed memories and exist everywhere. Everything else is rejected.
#[test]
fn base_rejects_every_float_builtin() {
    for builtin in fx_transpiler::builtins::BUILTINS {
        if matches!(builtin.name, "not" | "neg" | "ans" | "mvalue") {
            continue;
        }
        let args = vec!["a"; builtin.max_args].join(", ");
        let src = format!("#mode BASE\nlet a = 1;\nprint({}({args}));\n", builtin.name);
        let e = match transpile(&src) {
            Ok(prgm) => panic!(
                "`{}` should be rejected in BASE, got:\n{prgm}",
                builtin.name
            ),
            Err(e) => e,
        };
        assert!(
            e.message
                .contains(&format!("`{}` is not available in BASE", builtin.name)),
            "`{}` should be rejected in BASE, got: {}",
            builtin.name,
            e.message
        );
    }
}

#[test]
fn override_to_base_also_validates() {
    let e = transpile_with(
        "print(sqrt(a));",
        Options {
            ascii: false,
            mode: Some(Mode::Base),
        },
    )
    .unwrap_err();
    assert!(
        e.message.contains("`sqrt` is not available in BASE"),
        "{}",
        e.message
    );
}

#[test]
fn base_error_points_at_the_offending_expression() {
    let e = err("#mode BASE\nlet a = 1; let b = sqrt(a);\n");
    // `sqrt` starts at column 20 of the wrapped body's first line (line 3).
    assert_eq!((e.line, e.column), (3, 20));
}

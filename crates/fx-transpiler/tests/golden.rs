//! Golden tests: `.fxc` source in, exact PRGM out.
//!
//! Every construct of the language is covered at least once in glyph mode and
//! the operator/key spellings are re-checked in ASCII mode.

use fx_transpiler::{Options, transpile, transpile_with};

mod common;

/// Assert glyph-mode output.
#[track_caller]
fn glyph(source: &str, expected: &str) {
    let source = &common::wrap(source);
    let got = transpile(source).unwrap_or_else(|e| panic!("transpile failed: {e}\n{source}"));
    assert_eq!(got, expected, "glyph output for:\n{source}");
}

/// Assert ASCII-mode output.
#[track_caller]
fn ascii(source: &str, expected: &str) {
    let source = &common::wrap(source);
    let got = transpile_with(
        source,
        Options {
            ascii: true,
            ..Default::default()
        },
    )
    .unwrap_or_else(|e| panic!("transpile failed: {e}\n{source}"));
    assert_eq!(got, expected, "ASCII output for:\n{source}");
}

// ---------------------------------------------------------------------------
// Empty / trivial programs

#[test]
fn empty_program_is_empty() {
    glyph("", "");
    glyph("   \n\t", "");
    glyph("// just a comment\n", "");
    glyph("/* block */", "");
}

#[test]
fn stray_semicolons_are_dropped() {
    glyph(";", "");
    glyph(";;;", "");
    glyph("{ ; }", "");
}

// ---------------------------------------------------------------------------
// Assignments, input and display

#[test]
fn let_and_assignment() {
    glyph("let a = 1;", "1→A\n");
    glyph("a = 2;", "2→A\n");
    glyph("let a = 1; a = a + 1; print(a);", "1→A\nA+1→A\nA◢\n");
}

#[test]
fn input_is_read_into_the_target() {
    glyph("let a = input();", "?→A\n");
    glyph("a = input();", "?→A\n");
    glyph("let a = input(); let b = input();", "?→A\n?→B\n");
}

#[test]
fn print_emits_the_display_triangle() {
    glyph("print(a);", "A◢\n");
    glyph("print(a * 2);", "A×2◢\n");
    glyph("print a;", "A◢\n");
}

#[test]
fn bare_expression_statement_has_no_display() {
    glyph("a + b;", "A+B\n");
    glyph("a;", "A\n");
}

#[test]
fn blocks_are_flattened() {
    glyph("{ let a = 1; }", "1→A\n");
    glyph("let a = 1; { a = 2; { a = 3; } }", "1→A\n2→A\n3→A\n");
}

// ---------------------------------------------------------------------------
// Constants and variables

#[test]
fn constants_pi_and_e() {
    glyph("let a = pi; let b = e;", "π→A\ne→B\n");
    ascii("let a = pi; let b = e;", "pi->A\ne->B\n");
}

#[test]
fn names_are_allocated_in_first_seen_order() {
    glyph(
        "let total = 1; let count = 2; print(total + count);",
        "1→A\n2→B\nA+B◢\n",
    );
    // The declared name is bound before its initializer is walked, so `a` is
    // A even though `b` is the first name *read*; `b` then takes B.
    glyph("let a = b; let b = 1;", "B→A\n1→B\n");
}

#[test]
fn all_seven_memories_can_be_used() {
    glyph(
        "let a=1; let b=1; let c=1; let d=1; let x=1; let y=1; let m=1;",
        "1→A\n1→B\n1→C\n1→D\n1→X\n1→Y\n1→M\n",
    );
}

// ---------------------------------------------------------------------------
// Expressions: precedence and spelling

#[test]
fn arithmetic_operators() {
    glyph(
        "let a = 2; let b = 3; let c = a + b * 2 - a / b;",
        "2→A\n3→B\nA+B×2-A÷B→C\n",
    );
}

#[test]
fn power_is_parenthesised() {
    glyph("let a = b ^ c;", "B^(C)→A\n");
    glyph("let a = b ** c;", "B^(C)→A\n");
    glyph("let a = b ^ (c + d);", "B^(C+D)→A\n");
    glyph("let a = b ^ c + d;", "B^(C)+D→A\n");
}

#[test]
fn unary_minus() {
    glyph("let a = -5;", "-5→A\n");
    glyph("let a = -b * c;", "-B×C→A\n");
    glyph("let a = -(b + c);", "-(B+C)→A\n");
    glyph("let a = -(-b);", "-(-B)→A\n");
    glyph("let a = -b ^ c;", "-B^(C)→A\n");
}

#[test]
fn parentheses_are_only_added_when_needed() {
    glyph("let a = (b + c) * d;", "(B+C)×D→A\n");
    glyph("let a = b - (c - d);", "B-(C-D)→A\n");
    glyph("let a = b / (c * d);", "B÷(C×D)→A\n");
    glyph("let a = (b + c) + d;", "B+C+D→A\n");
}

#[test]
fn comparison_operators() {
    glyph(
        "let a = 1; let b = 2; print(a == b); print(a != b); print(a < b); print(a <= b); print(a > b); print(a >= b);",
        "1→A\n2→B\nA=B◢\nA≠B◢\nA<B◢\nA≤B◢\nA>B◢\nA≥B◢\n",
    );
}

#[test]
fn builtin_calls() {
    glyph("print(sqrt(a));", "√(A)◢\n");
    glyph("print(cbrt(a));", "∛(A)◢\n");
    glyph("print(abs(a));", "Abs(A)◢\n");
    glyph("print(sin(a));", "sin(A)◢\n");
    glyph("print(cos(a));", "cos(A)◢\n");
    glyph("print(tan(a));", "tan(A)◢\n");
    glyph("print(asin(a));", "sin⁻¹(A)◢\n");
    glyph("print(acos(a));", "cos⁻¹(A)◢\n");
    glyph("print(atan(a));", "tan⁻¹(A)◢\n");
    glyph("print(sinh(a));", "sinh(A)◢\n");
    glyph("print(asinh(a));", "sinh⁻¹(A)◢\n");
    glyph("print(ln(a));", "ln(A)◢\n");
    glyph("print(log(a));", "log(A)◢\n");
    glyph("print(log(a, b));", "log(A,B)◢\n");
    glyph("print(rnd(a));", "Rnd(A)◢\n");
}

#[test]
fn nested_calls_and_expressions() {
    glyph("print(sqrt(a + b) * 2);", "√(A+B)×2◢\n");
    glyph("print(abs(a - b) / c);", "Abs(A-B)÷C◢\n");
}

// ---------------------------------------------------------------------------
// Control flow

#[test]
fn if_without_else_always_emits_then() {
    glyph("if (a > 0) { print(1); }", "If A>0\nThen\n1◢\nIfEnd\n");
}

#[test]
fn if_with_else() {
    glyph(
        "if (a > 0) { print(1); } else { print(0); }",
        "If A>0\nThen\n1◢\nElse\n0◢\nIfEnd\n",
    );
}

#[test]
fn else_if_chains_nest() {
    glyph(
        "let a = 1;\nif (a > 0) { print(1); } else if (a < 0) { print(2); } else { print(3); }",
        "1→A\nIf A>0\nThen\n1◢\nElse\nIf A<0\nThen\n2◢\nElse\n3◢\nIfEnd\nIfEnd\n",
    );
}

#[test]
fn while_loop() {
    glyph(
        "while (a > 0) { a = a - 1; }",
        "While A>0\nA-1→A\nWhileEnd\n",
    );
}

#[test]
fn break_statement() {
    glyph("while (a > 0) { break; }", "While A>0\nBreak\nWhileEnd\n");
    glyph("break;", "Break\n");
}

#[test]
fn for_less_than_uses_offset_limit() {
    glyph(
        "for (let i = 0; i < 5; i = i + 1) { print(i); }",
        "For 0→A To 4 Step 1\nA◢\nNext\n",
    );
}

#[test]
fn for_less_or_equal_uses_the_limit() {
    glyph(
        "for (let i = 1; i <= 10; i = i + 2) { print(i); }",
        "For 1→A To 10 Step 2\nA◢\nNext\n",
    );
}

#[test]
fn for_greater_than_uses_a_negative_step() {
    glyph(
        "for (let i = 10; i > 0; i = i - 1) { print(i); }",
        "For 10→A To 1 Step -1\nA◢\nNext\n",
    );
}

#[test]
fn for_greater_or_equal_uses_a_negative_step() {
    glyph(
        "for (let i = 10; i >= 0; i = i - 1) { print(i); }",
        "For 10→A To 0 Step -1\nA◢\nNext\n",
    );
}

#[test]
fn for_with_a_variable_limit() {
    glyph(
        "let n = 5;\nfor (let i = 0; i < n; i = i + 1) { print(i); }",
        "5→A\nFor 0→B To A-1 Step 1\nB◢\nNext\n",
    );
}

#[test]
fn for_falls_back_to_while_when_shape_does_not_match() {
    // `!=` is not a supported bound.
    glyph(
        "for (let i = 0; i != 5; i = i + 2) { print(i); }",
        "0→A\nWhile A≠5\nA◢\nA+2→A\nWhileEnd\n",
    );
    // The update mutates a different variable.
    glyph(
        "for (let i = 0; i < 5; j = j + 1) { print(i); }",
        "0→A\nWhile A<5\nA◢\nB+1→B\nWhileEnd\n",
    );
}

#[test]
fn nested_loops() {
    glyph(
        "for (let i = 0; i < 2; i = i + 1) { for (let j = 0; j < 3; j = j + 1) { print(i * 10 + j); } }",
        "For 0→A To 1 Step 1\nFor 0→B To 2 Step 1\nA×10+B◢\nNext\nNext\n",
    );
}

#[test]
fn goto_and_label() {
    glyph("label 1; goto 1;", "Lbl 1\nGoto 1\n");
    glyph("goto 7; label 7;", "Goto 7\nLbl 7\n");
}

// ---------------------------------------------------------------------------
// Constant folding

#[test]
fn constant_expressions_are_pre_calculated() {
    glyph("print(2 * 3 + 4);", "10◢\n");
    glyph("print((2 + 3) * 4);", "20◢\n");
    glyph("print(10 / 4);", "2.5◢\n");
    glyph("print(0.5 + 0.25);", "0.75◢\n");
    glyph("print(1 < 2);", "1◢\n");
}

#[test]
fn inexact_arithmetic_is_left_as_written() {
    // The machine keeps 15 significant digits and auto-corrects after every
    // operation, so these are not the values it would produce.
    glyph("print(1 / 3);", "1÷3◢\n");
    glyph("print(0.1 + 0.2);", "0.1+0.2◢\n");
}

#[test]
fn a_const_is_folded_after_inlining() {
    glyph("const k = 6;\nprint(k * 2);", "12◢\n");
    glyph("print(2 * pi + 1);", "2×π+1◢\n");
}

// ---------------------------------------------------------------------------
// Unrolling a constant loop that indexes an array

#[test]
fn a_constant_loop_filling_an_array_is_unrolled() {
    let source = "\
let m[3] = {0, 0, 0};
for (let i = 0; i < 3; i = i + 1) { m[i] = input(); }
print(m[0] + m[1] + m[2]);
";
    glyph(source, "0→A\n0→B\n0→C\n?→A\n?→B\n?→C\nA+B+C◢\n");
    ascii(source, "0->A\n0->B\n0->C\n?->A\n?->B\n?->C\nA+B+Cdisp\n");
}

#[test]
fn a_loop_reading_an_array_with_the_counter_is_unrolled() {
    let source = "\
let m[3] = {10, 20, 30};
for (let i = 0; i < 3; i = i + 1) { print(m[i]); }
";
    glyph(source, "10→A\n20→B\n30→C\nA◢\nB◢\nC◢\n");
}

#[test]
fn a_counter_read_after_the_loop_is_kept() {
    let source = "let m[2];\nfor (let i = 0; i < 2; i = i + 1) { m[i] = 1; }\nprint(i);";
    // The loop becomes two assignments; the counter keeps its final value.
    glyph(source, "1→A\n1→B\n2→C\nC◢\n");
}

// ---------------------------------------------------------------------------
// A whole program

#[test]
fn full_program() {
    let source = "\
let a = input();
let b = input();
let sum = a + b;
print(sum);
if (sum > 10) { print(1); } else { print(0); }
while (sum > 0) { sum = sum - 1; }
for (let i = 0; i < 3; i = i + 1) { print(i); }
";
    glyph(
        source,
        "?→A\n?→B\nA+B→C\nC◢\nIf C>10\nThen\n1◢\nElse\n0◢\nIfEnd\nWhile C>0\nC-1→C\nWhileEnd\nFor 0→D To 2 Step 1\nD◢\nNext\n",
    );
    ascii(
        source,
        "?->A\n?->B\nA+B->C\nCdisp\nIf C>10\nThen\n1disp\nElse\n0disp\nIfEnd\nWhile C>0\nC-1->C\nWhileEnd\nFor 0->D To 2 Step 1\nDdisp\nNext\n",
    );
}

// ---------------------------------------------------------------------------
// ASCII mode spellings

#[test]
fn ascii_operator_spellings() {
    ascii("let a = input(); print(a * 2);", "?->A\nA*2disp\n");
    ascii(
        "let a = 1; let b = 2; print(a != b); print(a <= b); print(a >= b);",
        "1->A\n2->B\nA<>Bdisp\nA<=Bdisp\nA>=Bdisp\n",
    );
    ascii("print(a / b);", "A/Bdisp\n");
    ascii("print(sqrt(a));", "sqrt(A)disp\n");
}

#[test]
fn default_options_match_transpile() {
    assert_eq!(
        transpile_with(&common::wrap("print(a);"), Options::default()).unwrap(),
        transpile(&common::wrap("print(a);")).unwrap(),
    );
}

// ---------------------------------------------------------------------------
// Errors

#[test]
fn more_than_seven_names_is_an_error() {
    let source = "let a=1; let b=1; let c=1; let d=1; let x=1; let y=1; let m=1; let z=1;";
    let err = transpile(&common::wrap(source)).unwrap_err();
    assert!(
        err.message.contains("no free memory for `z`"),
        "unexpected message: {}",
        err.message
    );
    assert!(
        err.message.contains("Use `const`"),
        "the message should point at the way out: {}",
        err.message
    );
    // The wrapper adds one line before the body, so the column is unchanged
    // and the line is 2.
    assert_eq!((err.line, err.column), (2, 68));
    // `z` really is the eighth distinct name.
    assert!(
        err.message.contains("`z`"),
        "unexpected message: {}",
        err.message
    );
}

#[test]
fn input_in_an_expression_is_an_error() {
    let err = transpile(&common::wrap("let a = input() + 1;")).unwrap_err();
    assert!(err.message.contains("`input()`"), "{}", err.message);
    assert_eq!((err.line, err.column), (2, 9));

    let err = transpile(&common::wrap("print(input());")).unwrap_err();
    assert!(err.message.contains("`input()`"), "{}", err.message);
}

#[test]
fn unknown_functions_are_rejected_by_the_parser() {
    let err = transpile(&common::wrap("print(foo(1));")).unwrap_err();
    assert!(err.message.contains("unknown function"), "{}", err.message);
}

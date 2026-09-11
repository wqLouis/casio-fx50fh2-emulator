//! Every PRGM token has an `.fxc` spelling: golden output for the calculator
//! keys that are not plain arithmetic.
//!
//! The transpiler is a front end for the whole machine, so this file walks the
//! token vocabulary — prefix functions, postfix keys, infix keys, the `stat.`
//! namespace, setup/clear/data statements, `and`-family operators, `⇒`, and
//! base-tagged literals — and pins what each compiles to.

use fx_transpiler::{Options, transpile, transpile_with};

mod common;

#[track_caller]
fn glyph(source: &str, expected: &str) {
    let source = &common::wrap(source);
    let got = transpile(source).unwrap_or_else(|e| panic!("transpile failed: {e}\n{source}"));
    assert_eq!(got, expected, "glyph output for:\n{source}");
}

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

#[track_caller]
fn err(source: &str) -> fx_transpiler::error::TranspileError {
    transpile(&common::wrap(source)).unwrap_err()
}

// ---------------------------------------------------------------------------
// Prefix functions

#[test]
fn root_pow10_and_exp() {
    glyph("print(root(3, 27));", "3x√(27)◢\n");
    glyph("print(pow10(3));", "10^(3)◢\n");
    glyph("print(exp(0));", "e^(0)◢\n");
    // A compound index is parenthesised, the radicand is not.
    glyph(
        "let a = 1; print(root(a + 1, a + 2));",
        "1→A\n(A+1)x√(A+2)◢\n",
    );
}

#[test]
fn pol_rec_arg_and_conjg() {
    glyph("print(pol(1, 2));", "Pol(1,2)◢\n");
    glyph("print(rec(1, 2));", "Rec(1,2)◢\n");
    glyph("#mode CMPLX\nprint(arg(x));", "#mode CMPLX\narg(A)◢\n");
    glyph("#mode CMPLX\nprint(conjg(x));", "#mode CMPLX\nConjg(A)◢\n");
}

#[test]
fn not_and_neg_are_the_base_n_keys() {
    glyph("#mode BASE\nprint(not(x));", "#mode BASE\nNot(A)◢\n");
    glyph("#mode BASE\nprint(neg(x));", "#mode BASE\nNeg(A)◢\n");
}

// ---------------------------------------------------------------------------
// Postfix keys

#[test]
fn postfix_keys_are_one_arg_calls() {
    glyph("print(inv(x));", "A⁻¹◢\n");
    glyph("print(sqr(x));", "A²◢\n");
    glyph("print(cube(x));", "A³◢\n");
    glyph("print(fact(x));", "A!◢\n");
    glyph("print(pct(x));", "A%◢\n");
    // The operand binds tightly, so a sum is parenthesised.
    glyph("let a = 1; print(sqr(a + 1));", "1→A\n(A+1)²◢\n");
    ascii(
        "print(inv(x)); print(sqr(x)); print(cube(x));",
        "A^-1disp\nA^2disp\nA^3disp\n",
    );
}

// ---------------------------------------------------------------------------
// Infix keys

#[test]
fn infix_keys_are_two_arg_calls() {
    glyph("print(frac(1, 3));", "1┘3◢\n");
    glyph("print(ncr(5, 2));", "5nCr2◢\n");
    glyph("print(npr(5, 2));", "5nPr2◢\n");
    glyph("#mode CMPLX\nprint(polar(2, 45));", "#mode CMPLX\n2∠45◢\n");
}

// ---------------------------------------------------------------------------
// Random, fixed memories and constants

#[test]
fn random_and_fixed_memories() {
    glyph("print(ran());", "Ran#◢\n");
    glyph("print(ans());", "Ans◢\n");
    glyph("print(mvalue());", "M◢\n");
    glyph("#mode CMPLX\nprint(i());", "#mode CMPLX\ni◢\n");
    // `i` on its own is still a variable name.
    glyph("let i = 1; print(i);", "1→A\nA◢\n");
}

#[test]
fn using_the_fixed_m_memory_reserves_it() {
    // `mplus`/`mvalue` touch the calculator's fixed `M`, so the allocator keeps
    // `M` out of the pool and the variable takes `A`.
    let prgm = transpile(&common::wrap("let a = 1;\nmplus(a);\nprint(mvalue());")).unwrap();
    assert_eq!(prgm, "1→A\nA M+\nM◢\n");
    assert!(
        !prgm.contains("→M"),
        "a variable must not clobber M: {prgm}"
    );
}

// ---------------------------------------------------------------------------
// The `stat.` namespace

#[test]
fn statistical_variables_use_the_stat_namespace() {
    glyph(
        "#mode REG\nprint(stat.n + stat.sumx + stat.meanx + stat.sigmax);",
        "#mode REG\nn+Σx+x̄+σx◢\n",
    );
    glyph(
        "#mode REG\nprint(stat.sumx2 + stat.minx + stat.maxx);",
        "#mode REG\nΣx²+minX+maxX◢\n",
    );
    glyph(
        "#mode REG\nprint(stat.regA + stat.regB + stat.regR);",
        "#mode REG\nregA+regB+regR◢\n",
    );
    ascii(
        "#mode REG\nprint(stat.sumx2 + stat.meanx + stat.sigmax);",
        "#mode REG\nsumx2+meanx+sigmaxdisp\n",
    );
}

#[test]
fn an_unknown_statistical_variable_is_rejected() {
    let e = err("#mode REG\nprint(stat.sxy);");
    assert!(e.message.contains("unknown statistical variable"), "{e}");
}

// ---------------------------------------------------------------------------
// Setup, clear and data statements

#[test]
fn angle_and_display_setup() {
    glyph("deg();", "Deg\n");
    glyph("rad();", "Rad\n");
    glyph("gra();", "Gra\n");
    glyph("fix(3);", "Fix 3\n");
    glyph("sci(5);", "Sci 5\n");
    glyph("norm(2);", "Norm 2\n");
}

#[test]
fn base_selection_setup() {
    glyph("#mode BASE\ndec();", "#mode BASE\nDec\n");
    glyph("#mode BASE\nhex();", "#mode BASE\nHex\n");
    glyph("#mode BASE\nbin();", "#mode BASE\nBin\n");
    glyph("#mode BASE\noct();", "#mode BASE\nOct\n");
}

#[test]
fn complex_format_setup() {
    glyph("#mode CMPLX\nto_cartesian();", "#mode CMPLX\n▶a+b𝑖\n");
    glyph("#mode CMPLX\nto_polar();", "#mode CMPLX\n▶r∠θ\n");
    ascii("#mode CMPLX\nto_cartesian();", "#mode CMPLX\n>a+bi\n");
    ascii("#mode CMPLX\nto_polar();", "#mode CMPLX\n>rangle\n");
}

#[test]
fn clear_and_frequency_statements() {
    glyph("#mode SD\nclrmemory();", "#mode SD\nClrMemory\n");
    glyph("#mode SD\nclrstat();", "#mode SD\nClrStat\n");
    glyph("#mode SD\nfreqon();", "#mode SD\nFreqOn\n");
    glyph("#mode SD\nfreqoff();", "#mode SD\nFreqOff\n");
}

#[test]
fn data_entry_statements() {
    glyph("#mode REG\ndt(1);", "#mode REG\n1 DT\n");
    glyph("#mode REG\ndt(1, 2);", "#mode REG\n1,2 DT\n");
    glyph("#mode REG\ndt(1, 2, 3);", "#mode REG\n1,2;3 DT\n");
    // `dt(x, y)` needs REG; `dt(x)` works in SD too.
    glyph("#mode SD\ndt(1);", "#mode SD\n1 DT\n");
}

#[test]
fn memory_arithmetic_statements() {
    glyph("mplus(3);", "3 M+\n");
    glyph("mminus(3);", "3 M-\n");
    glyph("let a = 1;\nmplus(a + 2);", "1→A\nA+2 M+\n");
}

// ---------------------------------------------------------------------------
// Base-n operators and tagged literals

#[test]
fn base_literals_are_tagged() {
    glyph("#mode BASE\nprint(0xFF);", "#mode BASE\nFFh◢\n");
    glyph("#mode BASE\nprint(0b1010);", "#mode BASE\n1010b◢\n");
    glyph("#mode BASE\nprint(0o17);", "#mode BASE\n17o◢\n");
    glyph("#mode BASE\nprint(0Xff);", "#mode BASE\nFFh◢\n");
}

#[test]
fn bitwise_operators_are_words_with_spaces() {
    glyph(
        "#mode BASE\nprint(0b1010 and 0b1100);",
        "#mode BASE\n1010b and 1100b◢\n",
    );
    glyph(
        "#mode BASE\nprint(a or b xor c xnor d);",
        "#mode BASE\nA or B xor C xnor D◢\n",
    );
    // `and` binds tighter than `or`, and both looser than arithmetic.
    glyph(
        "#mode BASE\nprint(a + 1 and b);",
        "#mode BASE\nA+1 and B◢\n",
    );
}

// ---------------------------------------------------------------------------
// The `⇒` conditional jump

#[test]
fn conditional_jump() {
    glyph("let x = 5;\nx > 0 => print(1);", "5→A\nA>0⇒1◢\n");
    glyph("let x = 5;\nx > 0 => y = 1;", "5→A\nA>0⇒1→B\n");
    ascii("let x = 5;\nx > 0 => print(1);", "5->A\nA>0=>1disp\n");
}

#[test]
fn conditional_jump_rejects_a_compound_statement() {
    let e = err("let x = 1;\nx > 0 => while (x > 0) { x = x - 1; }");
    assert!(e.message.contains("`=>` can only guard"), "{e}");
}

// ---------------------------------------------------------------------------
// Mode validation

#[test]
fn modes_reject_what_they_do_not_offer() {
    // Complex-only keys need CMPLX.
    let e = err("print(arg(x));");
    assert!(e.message.contains("not available in COMP"), "{e}");
    // Statistics need SD or REG.
    let e = err("print(stat.sumx);");
    assert!(e.message.contains("not available in COMP"), "{e}");
    // Regression variables need REG.
    let e = err("#mode SD\nprint(stat.regA);");
    assert!(e.message.contains("needs REG"), "{e}");
    // Base-n keys need BASE.
    let e = err("print(not(x));");
    assert!(e.message.contains("not available in COMP"), "{e}");
    // pol/rec are COMP or CMPLX only.
    let e = err("#mode SD\nprint(pol(1, 2));");
    assert!(e.message.contains("needs COMP or CMPLX"), "{e}");
    // A base literal needs BASE.
    let e = err("print(0xFF);");
    assert!(e.message.contains("not available in COMP"), "{e}");
    // Setup needs a non-BASE mode.
    let e = err("#mode BASE\ndeg();");
    assert!(e.message.contains("not available in BASE"), "{e}");
}

#[test]
fn setup_digit_arguments_are_checked() {
    let e = err("norm(3);");
    assert!(e.message.contains("`norm` needs a literal 1"), "{e}");
    let e = err("fix(x);");
    assert!(e.message.contains("`fix` needs a literal 0"), "{e}");
}

// ---------------------------------------------------------------------------
// The whole vocabulary compiles to something the interpreter can lex again

#[cfg(feature = "execute")]
#[test]
fn ascii_output_round_trips_through_the_core_lexer() {
    // Every `.fxc` spelling in ASCII mode must be re-lexable by the
    // interpreter, which is what `--ascii` promises.
    for source in [
        "print(inv(x));",
        "print(sqr(x));",
        "print(ncr(5, 2));",
        "#mode REG\nprint(stat.meanx);",
        "#mode BASE\nprint(0b1010 and 0b1100);",
        "let x = 5;\nx > 0 => print(1);",
        "mplus(3);",
    ] {
        let source = &common::wrap(source);
        let prgm = transpile_with(
            source,
            Options {
                ascii: true,
                ..Default::default()
            },
        )
        .unwrap_or_else(|e| panic!("transpile failed: {e}\n{source}"));
        for line in prgm.lines() {
            if line.starts_with("#mode") {
                continue;
            }
            assert!(
                casio_fx50fh2::lexer::lex(line).is_ok(),
                "ASCII line {line:?} did not re-lex (from {source})"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Completeness: every PRGM token has an `.fxc` spelling
//
// The interpreter is the source of truth. Its `ALL` arrays are exhaustive, so
// this test fails to compile if a key is added to the machine without a front
// end for it — and fails at run time if the map points at a missing spelling.

/// The `.fxc` builtin that emits `func`, and the mode its call needs.
#[cfg(feature = "execute")]
fn func_spelling(func: casio_fx50fh2::token::FuncName) -> (&'static str, Option<&'static str>) {
    use casio_fx50fh2::token::FuncName::*;
    match func {
        Sin => ("sin", None),
        Cos => ("cos", None),
        Tan => ("tan", None),
        Asin => ("asin", None),
        Acos => ("acos", None),
        Atan => ("atan", None),
        Sinh => ("sinh", None),
        Cosh => ("cosh", None),
        Tanh => ("tanh", None),
        Asinh => ("asinh", None),
        Acosh => ("acosh", None),
        Atanh => ("atanh", None),
        Log => ("log", None),
        Ln => ("ln", None),
        Sqrt => ("sqrt", None),
        Cbrt => ("cbrt", None),
        TenPow => ("pow10", None),
        EPow => ("exp", None),
        Abs => ("abs", None),
        Pol => ("pol", None),
        Rec => ("rec", None),
        Rnd => ("rnd", None),
        Arg => ("arg", Some("CMPLX")),
        Conjg => ("conjg", Some("CMPLX")),
        Not => ("not", Some("BASE")),
        Neg => ("neg", Some("BASE")),
    }
}

#[cfg(feature = "execute")]
fn func_arity(func: casio_fx50fh2::token::FuncName) -> usize {
    use casio_fx50fh2::token::FuncName::*;
    match func {
        Pol | Rec => 2,
        _ => 1,
    }
}

#[cfg(feature = "execute")]
#[test]
fn every_prgm_function_has_an_fxc_spelling() {
    use casio_fx50fh2::token::FuncName;
    for func in FuncName::ALL {
        let (name, mode) = func_spelling(func);
        assert!(
            fx_transpiler::builtins::lookup(name).is_some(),
            "`{name}` (from {func:?}) is not a builtin"
        );
        let args = vec!["a"; func_arity(func)].join(", ");
        let header = mode.map(|m| format!("#mode {m}\n")).unwrap_or_default();
        let src = common::wrap(&format!("{header}let a = 1;\nprint({name}({args}));\n"));
        assert!(
            transpile(&src).is_ok(),
            "{func:?} should transpile as `{name}`, got: {:?}",
            transpile(&src).err().map(|e| e.message)
        );
    }
}

#[cfg(feature = "execute")]
#[test]
fn every_prgm_postfix_and_infix_key_has_an_fxc_spelling() {
    use casio_fx50fh2::token::{BinOp, Postfix};
    for postfix in Postfix::ALL {
        let name = match postfix {
            Postfix::Inverse => "inv",
            Postfix::Square => "sqr",
            Postfix::Cube => "cube",
            Postfix::Fact => "fact",
            Postfix::Percent => "pct",
        };
        assert!(fx_transpiler::builtins::lookup(name).is_some(), "{name}");
        let src = common::wrap(&format!("let a = 1;\nprint({name}(a));\n"));
        assert!(transpile(&src).is_ok(), "{postfix:?} -> {name}");
    }
    for op in BinOp::ALL {
        let (src, mode) = match op {
            BinOp::Add => ("let a = 1; print(a + a);", None),
            BinOp::Sub => ("let a = 1; print(a - a);", None),
            BinOp::Mul => ("let a = 1; print(a * a);", None),
            BinOp::Div => ("let a = 1; print(a / a);", None),
            BinOp::Frac => ("let a = 1; print(frac(a, a));", None),
            BinOp::Perm => ("let a = 1; print(npr(a, a));", None),
            BinOp::Comb => ("let a = 1; print(ncr(a, a));", None),
            BinOp::Eq => ("let a = 1; print(a == a);", None),
            BinOp::Ne => ("let a = 1; print(a != a);", None),
            BinOp::Gt => ("let a = 1; print(a > a);", None),
            BinOp::Lt => ("let a = 1; print(a < a);", None),
            BinOp::Ge => ("let a = 1; print(a >= a);", None),
            BinOp::Le => ("let a = 1; print(a <= a);", None),
            BinOp::And => ("#mode BASE\nlet a = 1; print(a and a);", Some("BASE")),
            BinOp::Or => ("#mode BASE\nlet a = 1; print(a or a);", Some("BASE")),
            BinOp::Xor => ("#mode BASE\nlet a = 1; print(a xor a);", Some("BASE")),
            BinOp::Xnor => ("#mode BASE\nlet a = 1; print(a xnor a);", Some("BASE")),
            BinOp::Polar => ("#mode CMPLX\nlet a = 1; print(polar(a, a));", None),
        };
        let _ = mode;
        let src = &common::wrap(src);
        assert!(
            transpile(src).is_ok(),
            "{op:?} should transpile, got: {:?}",
            transpile(src).err().map(|e| e.message)
        );
    }
}

#[cfg(feature = "execute")]
#[test]
fn every_prgm_statistical_variable_has_an_fxc_spelling() {
    use casio_fx50fh2::stats::StatVar;
    for var in StatVar::ALL {
        let name = match var {
            StatVar::N => "n",
            StatVar::SumX => "sumx",
            StatVar::SumX2 => "sumx2",
            StatVar::SumY => "sumy",
            StatVar::SumY2 => "sumy2",
            StatVar::SumXY => "sumxy",
            StatVar::MeanX => "meanx",
            StatVar::MeanY => "meany",
            StatVar::SigmaX => "sigmax",
            StatVar::SigmaY => "sigmay",
            StatVar::Sx => "sx",
            StatVar::Sy => "sy",
            StatVar::MinX => "minx",
            StatVar::MaxX => "maxx",
            StatVar::MinY => "miny",
            StatVar::MaxY => "maxy",
            StatVar::RegA => "rega",
            StatVar::RegB => "regb",
            StatVar::RegR => "regr",
        };
        let src = common::wrap(&format!("#mode REG\nprint(stat.{name});\n"));
        assert!(
            transpile(&src).is_ok(),
            "{var:?} should transpile as stat.{name}, got: {:?}",
            transpile(&src).err().map(|e| e.message)
        );
    }
}

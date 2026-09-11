//! Tests for the `phys.NAME` scientific-constant syntax.
//!
//! The pure-transpiler cases run under `--no-default-features`. The round-trip
//! and anti-drift cases need the interpreter, so they are compiled only with
//! the `execute` feature (which `testing`, on by default, implies).

use fx_transpiler::error::TranspileError;
use fx_transpiler::{Options, transpile as raw_transpile, transpile_with as raw_transpile_with};

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

/// Transpile with the ASCII output style.
#[track_caller]
fn ascii(source: &str) -> String {
    transpile_with(
        source,
        Options {
            ascii: true,
            ..Default::default()
        },
    )
    .unwrap_or_else(|e| panic!("transpile failed: {e}\n{source}"))
}

/// The glyph-mode PRGM for `print(phys.<spelling>)`.
#[track_caller]
fn glyph_constant(spelling: &str) -> String {
    transpile(&format!("print(phys.{spelling});"))
        .unwrap_or_else(|e| panic!("transpile `phys.{spelling}` failed: {e}"))
}

// ---------------------------------------------------------------------------
// Spelling and namespacing

#[test]
fn a_constant_transpiles_to_its_prgm_key() {
    assert_eq!(transpile("print(phys.h);").unwrap(), "h◢\n");
    assert_eq!(transpile("let a = phys.h;").unwrap(), "h→A\n");
    assert_eq!(transpile("print(phys.hbar);").unwrap(), "ħ◢\n");
}

#[test]
fn ascii_mode_uses_the_ascii_name() {
    assert_eq!(ascii("print(phys.h);"), "hdisp\n");
    assert_eq!(ascii("print(phys.hbar);"), "hbardisp\n");
    assert_eq!(ascii("print(phys.Rinf);"), "Rinfdisp\n");
    assert_eq!(ascii("print(phys.muN);"), "muNdisp\n");
}

#[test]
fn glyph_mode_uses_the_display_symbol() {
    assert_eq!(glyph_constant("hbar"), "ħ◢\n");
    assert_eq!(glyph_constant("Rinf"), "R∞◢\n");
    assert_eq!(glyph_constant("muN"), "μN◢\n");
    assert_eq!(glyph_constant("mumu"), "μμ◢\n");
}

#[test]
fn display_symbols_are_accepted_as_spellings() {
    assert_eq!(glyph_constant("ħ"), "ħ◢\n");
    assert_eq!(glyph_constant("μμ"), "μμ◢\n");
    assert_eq!(glyph_constant("R∞"), "R∞◢\n");
    assert_eq!(glyph_constant("α"), "α◢\n");
    assert_eq!(glyph_constant("λcp"), "λcp◢\n");
    assert_eq!(glyph_constant("C0"), "C0◢\n");
}

#[test]
fn bare_constant_names_are_not_constants() {
    // Without the `phys.` prefix `h` and `hbar` are ordinary variables, so
    // they are allocated a memory like any other name.
    assert_eq!(transpile("print(h);").unwrap(), "A◢\n");
    assert_eq!(transpile("print(hbar);").unwrap(), "A◢\n");
    assert_eq!(transpile("let h = 1; print(h);").unwrap(), "1→A\nA◢\n");
}

#[test]
fn the_elementary_charge_emits_eq_in_both_modes() {
    // `phys.e` is the elementary charge (the namespace removes the clash with
    // Euler's number). Its display symbol is `e`, which the emitted PRGM would
    // re-lex as Euler's number, so the emitter writes `eq` in both styles.
    assert_eq!(transpile("print(phys.e);").unwrap(), "eq◢\n");
    assert_eq!(ascii("print(phys.e);"), "eqdisp\n");
    assert_eq!(transpile("print(phys.eq);").unwrap(), "eq◢\n");
    assert_eq!(ascii("print(phys.eq);"), "eqdisp\n");
}

// ---------------------------------------------------------------------------
// Errors

#[test]
fn a_missing_name_after_phys_is_an_error() {
    let e = transpile("print(phys.);").unwrap_err();
    assert!(
        e.message.contains("a constant name after `phys.`"),
        "{}",
        e.message
    );
}

#[test]
fn an_unknown_constant_is_an_error() {
    let e = transpile("print(phys.nope);").unwrap_err();
    assert!(
        e.message.contains("unknown scientific constant `nope`"),
        "{}",
        e.message
    );
    assert_eq!((e.line, e.column), (2, 12));
}

#[test]
fn phys_without_a_dot_is_an_error() {
    let e = transpile("print(phys);").unwrap_err();
    assert!(
        e.message.contains("`phys` must be followed by `.`"),
        "{}",
        e.message
    );
}

#[test]
fn phys_cannot_be_a_variable() {
    let e = transpile("let phys = 1;").unwrap_err();
    assert!(e.message.contains("`phys`"), "{}", e.message);

    let e = transpile("phys = 1;").unwrap_err();
    assert!(e.message.contains("`phys`"), "{}", e.message);
}

/// A constant's display glyph is only a name directly after `phys.`.
///
/// Accepting Unicode identifiers everywhere would silently turn a misplaced
/// glyph into a variable — `print(π)` would allocate memory A and print it,
/// rather than being rejected — which is a miscompilation, not a
/// convenience.
#[test]
fn a_bare_display_glyph_is_rejected_not_treated_as_a_variable() {
    for source in ["print(π);", "print(ħ);", "print(μμ);", "let π = 1;"] {
        let e = transpile(source).unwrap_err();
        assert!(
            e.message.contains("unexpected character"),
            "`{source}` should be rejected, got: {}",
            e.message
        );
    }
}

#[test]
fn base_mode_rejects_a_constant_and_names_it() {
    let e = transpile("#mode BASE\nprint(phys.h);").unwrap_err();
    assert!(e.message.contains("`h` (Planck constant)"), "{}", e.message);
    assert!(
        e.message
            .contains("is not available in BASE mode (needs COMP, CMPLX, SD or REG)"),
        "{}",
        e.message
    );

    // A symbol spelling names the underlying constant, not the spelling.
    let e = transpile("#mode BASE\nprint(phys.ħ);").unwrap_err();
    assert!(
        e.message.contains("`hbar` (reduced Planck constant)"),
        "{}",
        e.message
    );
}

// ---------------------------------------------------------------------------
// Round trip / anti-drift (needs the interpreter)

#[cfg(feature = "execute")]
mod interpreter {
    use super::*;
    use casio_fx50fh2::ast::{Expr, Stmt};
    use casio_fx50fh2::token::ConstName;
    use casio_fx50fh2::{Interpreter, MockHost};
    use fx_transpiler::constants;

    /// The transpiler's local table must mirror the interpreter's exactly.
    #[test]
    fn the_transpiler_and_interpreter_tables_agree() {
        assert_eq!(
            constants::CONSTANTS.len(),
            casio_fx50fh2::CONSTANTS.len(),
            "table lengths differ"
        );
        for (local, core) in constants::CONSTANTS
            .iter()
            .zip(casio_fx50fh2::CONSTANTS.iter())
        {
            assert_eq!(local.code, core.code, "code for `{}`", local.name);
            assert_eq!(local.name, core.name, "name at code {}", local.code);
            assert_eq!(local.symbol, core.symbol, "symbol for `{}`", local.name);
            assert_eq!(
                local.description, core.description,
                "description for `{}`",
                local.name
            );
        }
    }

    /// Compile `prgm`, run it, and return the `◢` displays.
    #[track_caller]
    fn run(prgm: &str) -> Vec<String> {
        let program = casio_fx50fh2::compile(prgm)
            .unwrap_or_else(|e| panic!("emitted PRGM failed to compile: {e}\n---\n{prgm}"));
        let mut interp = Interpreter::new(program, MockHost::default());
        interp
            .run()
            .unwrap_or_else(|e| panic!("emitted PRGM failed to run: {e}\n---\n{prgm}"));
        interp.host().output.clone()
    }

    /// The menu code the interpreter sees in the compiled form of `prgm`.
    #[track_caller]
    fn compiled_code(prgm: &str) -> u8 {
        let program = casio_fx50fh2::compile(prgm)
            .unwrap_or_else(|e| panic!("emitted PRGM failed to compile: {e}\n---\n{prgm}"));
        let [
            Stmt::Expr {
                expr: Expr::Const(ConstName::Physical(code)),
                ..
            },
        ] = program.as_slice()
        else {
            panic!("expected exactly one constant expression in:\n{prgm}");
        };
        *code
    }

    /// The value the interpreter computes for a single constant spelling.
    #[track_caller]
    fn value_of(spelling: &str) -> f64 {
        let output = run(&glyph_constant(spelling));
        assert_eq!(output.len(), 1, "expected one display for `{spelling}`");
        output[0].parse::<f64>().unwrap_or_else(|e| {
            panic!(
                "display `{}` for `{spelling}` did not parse: {e}",
                output[0]
            )
        })
    }

    fn assert_close(got: f64, want: f64, label: &str) {
        let scale = got.abs().max(want.abs()).max(1.0);
        assert!(
            (got - want).abs() / scale < 1e-8,
            "{label}: got {got}, wanted {want}"
        );
    }

    /// Every entry, reached by both its name and its symbol, compiles, runs
    /// and resolves to the same menu entry.
    #[test]
    fn every_constant_round_trips_through_the_interpreter() {
        for c in &constants::CONSTANTS {
            for spelling in [c.name, c.symbol] {
                let prgm = glyph_constant(spelling);
                let _ = run(&prgm);
                assert_eq!(
                    compiled_code(&prgm),
                    c.code,
                    "`phys.{spelling}` should resolve to code {}",
                    c.code
                );
            }
        }
    }

    /// Spot-check a handful of values against the interpreter's own table.
    #[test]
    fn values_match_the_core_table() {
        let h = casio_fx50fh2::CONSTANTS
            .iter()
            .find(|c| c.name == "h")
            .unwrap();
        assert_close(value_of("h"), h.value, "Planck constant");

        for name in ["C0", "mumu", "gp", "atm", "G", "tK", "R∞"] {
            let core = casio_fx50fh2::CONSTANTS
                .iter()
                .find(|c| c.name == name || c.symbol == name)
                .unwrap_or_else(|| panic!("no core constant for `{name}`"));
            assert_close(value_of(name), core.value, name);
        }
    }

    /// `phys.e` is the elementary charge, not Euler's number.
    #[test]
    fn phys_e_runs_as_the_elementary_charge() {
        let prgm = glyph_constant("e");
        assert_eq!(prgm, "eq◢\n");
        assert_eq!(compiled_code(&prgm), 23);
        assert_close(value_of("e"), 1.602_176_565e-19, "elementary charge");
        assert!(
            (value_of("e") - std::f64::consts::E).abs() > 1e-10,
            "`phys.e` must not be Euler's number"
        );
    }
}

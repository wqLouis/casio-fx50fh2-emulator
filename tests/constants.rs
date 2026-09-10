//! End-to-end tests for the 40 built-in scientific constants.

use casio_fx50fh2::{CalcError, compile, evaluate};

/// Evaluate a one-line program and return the formatted display output.
fn run(source: &str) -> Result<Vec<String>, CalcError> {
    let program = compile(source)?;
    let mut interp = casio_fx50fh2::Interpreter::new(program, casio_fx50fh2::MockHost::default());
    interp.run()?;
    Ok(interp.host().output.clone())
}

fn first(source: &str) -> String {
    run(source)
        .unwrap_or_else(|e| panic!("`{source}` failed: {e}"))
        .into_iter()
        .next()
        .expect("a displayed value")
}

// ---------------------------------------------------------------------------
// Spellings

#[test]
fn an_ascii_name_reads_the_constant() {
    assert_eq!(first("h"), "6.62606957e-34");
    assert_eq!(first("atm"), "101325");
    assert_eq!(first("g"), "9.80665");
}

#[test]
fn a_display_symbol_reads_the_same_constant() {
    assert_eq!(first("ħ"), first("hbar"));
    assert_eq!(first("μμ"), first("mumu"));
    assert_eq!(first("R∞"), first("Rinf"));
    assert_eq!(first("mμ"), first("mmu"));
}

#[test]
fn e_is_still_eulers_number_and_eq_is_the_elementary_charge() {
    let euler = std::f64::consts::E;
    assert!((evaluate("e").unwrap() - euler).abs() < 1e-12);
    assert!((evaluate("eq").unwrap() - 1.602_176_565e-19).abs() < 1e-30);
}

#[test]
fn a_constant_participates_in_arithmetic() {
    // The Faraday constant is 96485.3365 C/mol, so two moles carry twice that.
    assert_eq!(evaluate("2 * F").unwrap(), 2.0 * 96_485.336_5);
    // And a speed of light squared, to check that `^` sees the whole constant.
    let c2 = evaluate("C0 ^ 2").unwrap();
    let want = 299_792_458.0f64.powi(2);
    assert!((c2 - want).abs() / want < 1e-15, "{c2} vs {want}");
}

#[test]
fn a_constant_can_be_stored_in_a_variable() {
    let out = run("hbar→A: A×2").unwrap();
    assert_eq!(out, vec!["2.109143452e-34"]);
}

// ---------------------------------------------------------------------------
// Modes

#[test]
fn constants_are_available_in_comp_and_cmplx() {
    assert!(run("h").is_ok());
    assert!(run("#mode CMPLX\nh").is_ok());
}

#[test]
fn constants_are_available_in_the_statistics_modes() {
    assert!(run("#mode SD\nh").is_ok());
    assert!(run("#mode REG\nh").is_ok());
}

/// BASE works on integers and has no floating point, so the constants, which
/// are all real numbers, cannot be entered there.
#[test]
fn base_mode_rejects_a_constant_with_a_mode_error() {
    let err = compile("#mode BASE\nh").unwrap_err();
    assert!(matches!(err, CalcError::Mode { .. }), "{err:?}");
    assert!(err.to_string().contains("Planck constant"), "{err}");
}

// ---------------------------------------------------------------------------
// The table itself

#[test]
fn unique_prefixes_do_not_shadow_longer_names() {
    // `h` is the Planck constant and `hbar` the reduced one; `g`/`G` and
    // `mue`/`mumu` likewise must not be confused.
    assert!((evaluate("h").unwrap() - 6.626_069_57e-34).abs() < 1e-44);
    assert!((evaluate("hbar").unwrap() - 1.054_571_726e-34).abs() < 1e-44);
    assert!((evaluate("g").unwrap() - 9.806_65).abs() < 1e-9);
    assert!((evaluate("G").unwrap() - 6.673_84e-11).abs() < 1e-19);
}

/// Every constant in the table must be lexable, by name and by symbol.
#[test]
fn every_entry_in_the_table_is_reachable() {
    for c in &casio_fx50fh2::CONSTANTS {
        assert!(run(c.name).is_ok(), "name `{}` is not lexable", c.name);
        if c.symbol != c.name && c.symbol != "e" {
            assert!(
                run(c.symbol).is_ok(),
                "symbol `{}` is not lexable",
                c.symbol
            );
        }
    }
}

/// A constant must never be mistaken for a variable or a keyword.
#[test]
fn a_constant_is_not_a_variable() {
    // `A` is a variable and `atm` a constant; both must survive side by side.
    let out = run("atm→A: A").unwrap();
    assert_eq!(out, vec!["101325"]);
}

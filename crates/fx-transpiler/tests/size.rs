//! Integration tests for PRGM key-size measurement.
//!
//! Every count here is derived from the key rules documented in
//! [`fx_transpiler::size`], which mirror the interpreter's PRGM lexer
//! (`src/lexer.rs`). The point of the module is to put a number on the
//! optimisation work, so the tests pin both the arithmetic and the
//! whitespace/formatting insensitivity the callers rely on.

use std::path::PathBuf;

use fx_transpiler::size::{Size, measure};
use fx_transpiler::{Options, transpile, transpile_with};

/// Read one of the repository's example programs.
fn example(name: &str) -> String {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("../../examples");
    path.push(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// The key count of a listing, for brevity.
fn keys(prgm: &str) -> usize {
    measure(prgm).keys
}

/// The five worked examples from the module documentation.
#[test]
fn worked_examples() {
    let cases = [
        ("1→A", 3),     // 1 + → + A
        ("10◢", 3),     // 1 + 0 + ◢
        ("A×2◢", 4),    // A + × + 2 + ◢
        ("log(2)◢", 4), // log( + 2 + ) + ◢
        ("?→A", 3),     // ? + → + A
    ];
    for (listing, expected) in cases {
        assert_eq!(keys(listing), expected, "listing {listing:?}");
    }
}

/// A prefix function and the `(` it inserts are a single key, in both glyph
/// and ASCII form.
#[test]
fn prefix_functions_own_their_parenthesis() {
    // `log(2)◢` is `log(` + `2` + `)` + `◢`.
    assert_eq!(keys("log(2)◢"), 4);
    // `sqrt(` is the ASCII alias of the glyph `√(`.
    assert_eq!(keys("sqrt(2)◢"), 4);
    assert_eq!(keys("√(2)◢"), 4);
    // `10^(` and `e^(` are single prefix keys.
    assert_eq!(keys("10^(3)◢"), 4);
    assert_eq!(keys("e^(0)◢"), 4);
    // `sin⁻¹(` is one key, with `asin(` as its ASCII spelling.
    assert_eq!(keys("sin⁻¹(2)◢"), 4);
    assert_eq!(keys("asin(2)◢"), 4);
    // The parenthetical binary `x√(` is one key too: index, key, two
    // radicand digits and `)`.
    assert_eq!(keys("3x√(27)◢"), 6);
}

/// The machine has no multi-digit number key: every digit is its own byte.
#[test]
fn digits_cost_one_key_each() {
    assert_eq!(keys("1◢"), 2);
    assert_eq!(keys("12◢"), 3);
    assert_eq!(keys("123◢"), 4);
    assert_eq!(keys("1.5◢"), 4);
    assert_eq!(keys("10◢"), 3);
    // An exponent marker is a key as well.
    assert_eq!(keys("1E3◢"), 4);
}

/// The two output styles of the same program cost the same, because every
/// ASCII alias stands for exactly one glyph key.
#[test]
fn ascii_and_glyph_cost_the_same() {
    let source = "fn main() { let a = input(); let b = a * 2; if (b <= 8) { print(b); } }";
    let glyph = transpile(source).unwrap();
    let ascii = transpile_with(
        source,
        Options {
            ascii: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(measure(&glyph), measure(&ascii));
}

/// Whitespace, indentation and blank lines are layout, not keys: they must not
/// move any of the three numbers.
#[test]
fn whitespace_and_blank_lines_are_free() {
    let tight = "A→B\nC→D\n";
    let loose = "\n\n  A→B  \n\n\t C→D  \n\n";
    assert_eq!(measure(tight), measure(loose));
    assert_eq!(
        measure(tight),
        Size {
            keys: 6,
            statements: 2,
            largest: 3,
        }
    );
}

/// A `:` is a real key and also ends the statement. The separator is charged
/// to the statement it terminates.
#[test]
fn colon_separator_is_a_key() {
    let size = measure("A→B:C→D");
    assert_eq!(size.keys, 7); // A→B(3) + :(1) + C→D(3)
    assert_eq!(size.statements, 2);
    assert_eq!(size.largest, 4); // the `:` belongs to the first statement
}

/// Comments never appear in emitted listings, but the contract promises they
/// are not counted.
#[test]
fn comments_are_free() {
    assert_eq!(keys("A→B // not stored\n"), 3);
    assert_eq!(measure("// only a comment\n"), Size::default());
}

/// A `#mode` header is a host directive the MODE key would have applied before
/// the program was typed in, so it is neither a key nor a statement.
#[test]
fn mode_header_is_free() {
    let size = measure("#mode BASE\nFFh◢\n");
    assert_eq!(
        size,
        Size {
            keys: 3, // F + F + ◢ (the base tag is not a keystroke)
            statements: 1,
            largest: 3,
        }
    );
}

/// Empty input is a valid, empty program.
#[test]
fn empty_input() {
    let size = measure("");
    assert_eq!(size, Size::default());
    assert!(size.fits());
    assert_eq!(size.remaining(), Some(Size::CAPACITY));
}

/// `fits` and `remaining` straddle the 680-byte capacity exactly.
#[test]
fn capacity_boundary() {
    let exact = Size {
        keys: Size::CAPACITY,
        ..Default::default()
    };
    assert!(exact.fits());
    assert_eq!(exact.remaining(), Some(0));

    let over = Size {
        keys: Size::CAPACITY + 1,
        ..Default::default()
    };
    assert!(!over.fits());
    assert_eq!(over.remaining(), None);
}

/// Transpile and measure a real example program. The exact numbers are pinned
/// so an accidental change in the emitted listing is caught.
#[test]
fn real_example_program() {
    let source = example("factorial.fxc");
    let prgm = transpile(&source).expect("factorial.fxc transpiles");
    let size = measure(&prgm);
    assert_eq!(
        size,
        Size {
            keys: 22,
            statements: 6,
            largest: 8,
        }
    );
    assert!(size.fits());
    assert_eq!(size.remaining(), Some(Size::CAPACITY - 22));
}

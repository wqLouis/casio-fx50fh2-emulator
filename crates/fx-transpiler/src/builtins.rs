//! The built-in functions of the `.fxc` language and how they map onto PRGM.
//!
//! Every parenthetical key on the calculator is reachable from `.fxc`. Most are
//! spelled as an ordinary call (`sin(x)`, `pol(x, y)`), but the calculator's
//! infix (`nPr`, `┘`, `∠`) and postfix (`x²`, `!`, `%`) keys are exposed as
//! two- and one-argument calls and re-emitted in their native form.

/// How a built-in is spelled in the emitted PRGM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Form {
    /// `SPELLING(args…)`, e.g. `sin(x)`, `√(x)`, `10^(x)`.
    Call,
    /// `a SPELLING b`, exactly two arguments: `nPr`, `nCr`, `┘`, `∠`.
    Infix,
    /// `aSPELLING`, exactly one argument: `²`, `³`, `⁻¹`, `!`, `%`.
    Postfix,
    /// Spelled by the emitter directly (`root`, `ran`, `i`, `ans`).
    Special,
}

/// Description of one callable built-in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Builtin {
    /// Name used in `.fxc` source.
    pub name: &'static str,
    pub min_args: usize,
    pub max_args: usize,
    /// Glyph spelling emitted by default.
    pub glyph: &'static str,
    /// ASCII-alias spelling emitted with [`crate::Options::ascii`].
    pub ascii: &'static str,
    /// How the arguments and the spelling are arranged.
    pub form: Form,
}

impl Builtin {
    /// The spelling to emit for the requested output style.
    pub fn spelling(&self, ascii: bool) -> &'static str {
        if ascii { self.ascii } else { self.glyph }
    }
}

macro_rules! builtin {
    ($name:literal, $min:literal, $max:literal, $glyph:literal, $ascii:literal, $form:ident) => {
        Builtin {
            name: $name,
            min_args: $min,
            max_args: $max,
            glyph: $glyph,
            ascii: $ascii,
            form: Form::$form,
        }
    };
}

/// Every callable built-in, in documentation order.
///
/// The table is also the parser's arity check and the evaluator's capability
/// check, so a key is added in one place.
pub const BUILTINS: &[Builtin] = &[
    // -- roots and powers ---------------------------------------------------
    builtin!("sqrt", 1, 1, "\u{221a}", "sqrt", Call),
    builtin!("cbrt", 1, 1, "\u{221b}", "cbrt", Call),
    // `root(index, radicand)` is the `x√(` key: the index precedes the radical.
    builtin!("root", 2, 2, "x\u{221a}", "x\u{221a}", Special),
    builtin!("pow10", 1, 1, "10^", "10^", Call),
    builtin!("exp", 1, 1, "e^", "e^", Call),
    // -- trigonometry and logarithms ---------------------------------------
    builtin!("sin", 1, 1, "sin", "sin", Call),
    builtin!("cos", 1, 1, "cos", "cos", Call),
    builtin!("tan", 1, 1, "tan", "tan", Call),
    builtin!("asin", 1, 1, "sin\u{207b}\u{00b9}", "asin", Call),
    builtin!("acos", 1, 1, "cos\u{207b}\u{00b9}", "acos", Call),
    builtin!("atan", 1, 1, "tan\u{207b}\u{00b9}", "atan", Call),
    builtin!("sinh", 1, 1, "sinh", "sinh", Call),
    builtin!("cosh", 1, 1, "cosh", "cosh", Call),
    builtin!("tanh", 1, 1, "tanh", "tanh", Call),
    builtin!("asinh", 1, 1, "sinh\u{207b}\u{00b9}", "asinh", Call),
    builtin!("acosh", 1, 1, "cosh\u{207b}\u{00b9}", "acosh", Call),
    builtin!("atanh", 1, 1, "tanh\u{207b}\u{00b9}", "atanh", Call),
    builtin!("log", 1, 2, "log", "log", Call),
    builtin!("ln", 1, 1, "ln", "ln", Call),
    // -- arithmetic helpers -------------------------------------------------
    builtin!("abs", 1, 1, "Abs", "Abs", Call),
    builtin!("rnd", 1, 1, "Rnd", "Rnd", Call),
    builtin!("inv", 1, 1, "\u{207b}\u{00b9}", "^-1", Postfix),
    builtin!("sqr", 1, 1, "\u{00b2}", "^2", Postfix),
    builtin!("cube", 1, 1, "\u{00b3}", "^3", Postfix),
    builtin!("fact", 1, 1, "!", "!", Postfix),
    builtin!("pct", 1, 1, "%", "%", Postfix),
    builtin!("frac", 2, 2, "\u{2518}", "\u{2518}", Infix),
    builtin!("npr", 2, 2, "nPr", "nPr", Infix),
    builtin!("ncr", 2, 2, "nCr", "nCr", Infix),
    // -- complex numbers ----------------------------------------------------
    builtin!("pol", 2, 2, "Pol", "Pol", Call),
    builtin!("rec", 2, 2, "Rec", "Rec", Call),
    builtin!("arg", 1, 1, "arg", "arg", Call),
    builtin!("conjg", 1, 1, "Conjg", "Conjg", Call),
    // `polar(r, theta)` is the `r∠θ` literal.
    builtin!("polar", 2, 2, "\u{2220}", "\u{2220}", Infix),
    // `dms(deg, min, sec)` is a sexagesimal literal.  It is emitted as the
    // machine's `d°m′s″` form rather than a call, so it is a special form.
    builtin!("dms", 3, 3, "\u{00b0}\u{2032}\u{2033}", "dms", Special),
    // -- base-n -------------------------------------------------------------
    builtin!("not", 1, 1, "Not", "Not", Call),
    builtin!("neg", 1, 1, "Neg", "Neg", Call),
    // -- constants and random ----------------------------------------------
    builtin!("ran", 0, 0, "Ran#", "Ran#", Special),
    builtin!("i", 0, 0, "i", "i", Special),
    builtin!("ans", 0, 0, "Ans", "Ans", Special),
    // The `M+`/`M-` accumulator is the calculator's fixed `M` memory; this key
    // reads it. Using it (or `mplus`/`mminus`) makes the allocator keep `M`
    // free so no `.fxc` variable clobbers the accumulator.
    builtin!("mvalue", 0, 0, "M", "M", Special),
];

/// Look up a built-in by its `.fxc` name.
pub fn lookup(name: &str) -> Option<&'static Builtin> {
    BUILTINS.iter().find(|b| b.name == name)
}

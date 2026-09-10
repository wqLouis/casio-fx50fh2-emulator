//! The built-in functions of the `.fxc` language and how they map onto PRGM.

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
}

impl Builtin {
    /// The spelling to emit for the requested output style.
    pub fn spelling(&self, ascii: bool) -> &'static str {
        if ascii { self.ascii } else { self.glyph }
    }
}

macro_rules! builtin {
    ($name:literal, $min:literal, $max:literal, $glyph:literal, $ascii:literal) => {
        Builtin {
            name: $name,
            min_args: $min,
            max_args: $max,
            glyph: $glyph,
            ascii: $ascii,
        }
    };
}

/// Every callable built-in, in documentation order.
pub const BUILTINS: &[Builtin] = &[
    builtin!("sqrt", 1, 1, "\u{221a}", "sqrt"),
    builtin!("cbrt", 1, 1, "\u{221b}", "cbrt"),
    builtin!("abs", 1, 1, "Abs", "Abs"),
    builtin!("sin", 1, 1, "sin", "sin"),
    builtin!("cos", 1, 1, "cos", "cos"),
    builtin!("tan", 1, 1, "tan", "tan"),
    builtin!("asin", 1, 1, "sin\u{207b}\u{00b9}", "asin"),
    builtin!("acos", 1, 1, "cos\u{207b}\u{00b9}", "acos"),
    builtin!("atan", 1, 1, "tan\u{207b}\u{00b9}", "atan"),
    builtin!("sinh", 1, 1, "sinh", "sinh"),
    builtin!("cosh", 1, 1, "cosh", "cosh"),
    builtin!("tanh", 1, 1, "tanh", "tanh"),
    builtin!("asinh", 1, 1, "sinh\u{207b}\u{00b9}", "asinh"),
    builtin!("acosh", 1, 1, "cosh\u{207b}\u{00b9}", "acosh"),
    builtin!("atanh", 1, 1, "tanh\u{207b}\u{00b9}", "atanh"),
    builtin!("log", 1, 2, "log", "log"),
    builtin!("ln", 1, 1, "ln", "ln"),
    builtin!("rnd", 1, 1, "Rnd", "Rnd"),
];

/// Look up a built-in by its `.fxc` name.
pub fn lookup(name: &str) -> Option<&'static Builtin> {
    BUILTINS.iter().find(|b| b.name == name)
}

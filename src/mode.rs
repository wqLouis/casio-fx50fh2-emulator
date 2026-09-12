//! Operating modes of the fx-50FH II.
//!
//! The hardware forces a mode before you can compute: complex numbers only
//! exist in **CMPLX**, statistics only in **SD**/**REG**, and base-n only in
//! **BASE**.  You cannot type an `i` in COMP mode because the key is not
//! offered there, and `√(-4)` is a `Math ERROR` rather than `2i`.
//!
//! This interpreter models that by letting a program declare its mode in a
//! header directive:
//!
//! ```text
//! #mode CMPLX
//! (3+4i)×(1-2i)◢
//! ```
//!
//! A program without a directive runs in [`Mode::Comp`], and any construct
//! that the declared mode does not offer is rejected with a
//! [`CalcError::Mode`](crate::CalcError::Mode).

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Mode {
    /// General computation.  Real numbers only.
    #[default]
    Comp,
    /// Complex-number computation.
    Cmplx,
    /// Base-n (integer) computation.
    Base,
    /// Single-variable statistics.
    Sd,
    /// Paired-variable statistics and regression.
    Reg,
}

impl Mode {
    /// Parse a mode name.  Case-insensitive, with a few common aliases.
    pub fn parse(text: &str) -> Option<Mode> {
        match text.to_ascii_uppercase().as_str() {
            "COMP" => Some(Mode::Comp),
            "CMPLX" | "CPLX" | "COMPLEX" => Some(Mode::Cmplx),
            "BASE" | "BASEN" | "BASE-N" => Some(Mode::Base),
            "SD" | "STAT" | "STATS" | "STATISTICS" => Some(Mode::Sd),
            "REG" | "REGRESSION" => Some(Mode::Reg),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Mode::Comp => "COMP",
            Mode::Cmplx => "CMPLX",
            Mode::Base => "BASE",
            Mode::Sd => "SD",
            Mode::Reg => "REG",
        }
    }

    /// Complex numbers (`i`, `∠`, complex results, `arg`, `Conjg`).
    pub(crate) fn allows_complex(self) -> bool {
        matches!(self, Mode::Cmplx)
    }

    /// Statistics data entry and statistical variables.
    pub(crate) fn allows_stats(self) -> bool {
        matches!(self, Mode::Sd | Mode::Reg)
    }

    /// Paired-variable statistics and regression (`y` statistics, `regA`…).
    pub(crate) fn allows_regression(self) -> bool {
        matches!(self, Mode::Reg)
    }

    /// Base-n: `Dec`/`Hex`/`Bin`/`Oct`, tagged literals, bitwise operators.
    pub(crate) fn allows_base(self) -> bool {
        matches!(self, Mode::Base)
    }

    /// Floating-point mathematics: trig, logs, powers, fractions, `!`, `%`,
    /// `nPr`/`nCr`, `π`, `e`, `Ran#`, `Pol`/`Rec`, `√`.
    ///
    /// These are offered in every mode except BASE, which works on integers.
    pub(crate) fn allows_float_math(self) -> bool {
        !matches!(self, Mode::Base)
    }

    /// Display and angle setup commands (`Fix`, `Sci`, `Norm`, `Deg`, …).
    pub(crate) fn allows_setup(self) -> bool {
        !matches!(self, Mode::Base)
    }
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

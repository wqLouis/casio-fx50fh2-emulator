//! Operating modes for the fx-50FH II.
//!
//! The calculator forces a mode before you can compute: complex numbers only
//! exist in **CMPLX**, statistics only in **SD**/**REG**, and base-n only in
//! **BASE**. A `.fxc` program may declare its mode with a leading directive:
//!
//! ```text
//! #mode CMPLX
//! ```
//!
//! This is the transpiler's own copy of the mode table. The crate deliberately
//! does **not** depend on the interpreter, so the enum is duplicated here (the
//! interpreter has an equivalent one in `casio_fx50fh2::Mode`).

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Mode {
    /// General computation. Real numbers only.
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
    /// Parse a mode name. Case-insensitive, with a few common aliases.
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

    /// The canonical uppercase name, as written after `#mode`.
    pub fn name(self) -> &'static str {
        match self {
            Mode::Comp => "COMP",
            Mode::Cmplx => "CMPLX",
            Mode::Base => "BASE",
            Mode::Sd => "SD",
            Mode::Reg => "REG",
        }
    }

    /// Whether the mode offers floating-point mathematics (trig, logs, `π`,
    /// `e`, …). Every mode except BASE does.
    pub fn allows_float_math(self) -> bool {
        !matches!(self, Mode::Base)
    }
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_canonical_names_case_insensitively() {
        assert_eq!(Mode::parse("comp"), Some(Mode::Comp));
        assert_eq!(Mode::parse("CmPlX"), Some(Mode::Cmplx));
        assert_eq!(Mode::parse("base"), Some(Mode::Base));
        assert_eq!(Mode::parse("sd"), Some(Mode::Sd));
        assert_eq!(Mode::parse("reg"), Some(Mode::Reg));
    }

    #[test]
    fn parses_aliases() {
        assert_eq!(Mode::parse("STAT"), Some(Mode::Sd));
        assert_eq!(Mode::parse("CPLX"), Some(Mode::Cmplx));
        assert_eq!(Mode::parse("BASE-N"), Some(Mode::Base));
        assert_eq!(Mode::parse("REGRESSION"), Some(Mode::Reg));
    }

    #[test]
    fn rejects_unknown_names() {
        assert_eq!(Mode::parse(""), None);
        assert_eq!(Mode::parse("GRAPH"), None);
    }

    #[test]
    fn names_are_canonical() {
        assert_eq!(Mode::Comp.name(), "COMP");
        assert_eq!(Mode::Cmplx.name(), "CMPLX");
        assert_eq!(Mode::Base.name(), "BASE");
        assert_eq!(Mode::Sd.name(), "SD");
        assert_eq!(Mode::Reg.name(), "REG");
        assert_eq!(Mode::default(), Mode::Comp);
    }
}

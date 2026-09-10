//! The fx-50FH II's 40 built-in scientific constants, for the `.fxc` front
//! end.
//!
//! This is a local, zero-dependency copy of the interpreter's table in
//! `casio-fx50fh2`'s `src/constants.rs`. The transpiler deliberately does not
//! depend on the core crate (it must keep building with `--no-default-features`),
//! so the two tables are kept in sync by the anti-drift test in
//! `tests/constants.rs`, which asserts that every code, name and symbol agrees.
//!
//! A `.fxc` program reaches a constant through the `phys` namespace: `phys.NAME`
//! where `NAME` is the ASCII [`name`](Constant::name) or the display
//! [`symbol`](Constant::symbol). The namespace is what keeps a bare `h` from
//! being a constant (and colliding with a variable), and it is why the
//! elementary charge can be written `phys.e` even though `e` alone is Euler's
//! number.

/// One of the calculator's built-in scientific constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Constant {
    /// Menu number, `1..=40`, as printed in the user's guide.
    pub code: u8,
    /// ASCII name used in program source (`mp`, `hbar`, `C0`).
    pub name: &'static str,
    /// The symbol the calculator's display shows (`mμ`, `ħ`, `R∞`).
    ///
    /// This is the same as `name` for the constants whose display spelling is
    /// already ASCII.
    pub symbol: &'static str,
    /// English name, used in BASE-mode errors.
    pub description: &'static str,
}

/// All 40 constants, in menu order.
pub const CONSTANTS: [Constant; 40] = [
    Constant {
        code: 1,
        name: "mp",
        symbol: "mp",
        description: "proton mass",
    },
    Constant {
        code: 2,
        name: "mn",
        symbol: "mn",
        description: "neutron mass",
    },
    Constant {
        code: 3,
        name: "me",
        symbol: "me",
        description: "electron mass",
    },
    Constant {
        code: 4,
        name: "mmu",
        symbol: "mμ",
        description: "muon mass",
    },
    Constant {
        code: 5,
        name: "a0",
        symbol: "a0",
        description: "Bohr radius",
    },
    Constant {
        code: 6,
        name: "h",
        symbol: "h",
        description: "Planck constant",
    },
    Constant {
        code: 7,
        name: "muN",
        symbol: "μN",
        description: "nuclear magneton",
    },
    Constant {
        code: 8,
        name: "muB",
        symbol: "μB",
        description: "Bohr magneton",
    },
    Constant {
        code: 9,
        name: "hbar",
        symbol: "ħ",
        description: "reduced Planck constant",
    },
    Constant {
        code: 10,
        name: "alpha",
        symbol: "α",
        description: "fine-structure constant",
    },
    Constant {
        code: 11,
        name: "re",
        symbol: "re",
        description: "classical electron radius",
    },
    Constant {
        code: 12,
        name: "lc",
        symbol: "λc",
        description: "Compton wavelength",
    },
    Constant {
        code: 13,
        name: "gp",
        symbol: "γp",
        description: "proton gyromagnetic ratio",
    },
    Constant {
        code: 14,
        name: "lcp",
        symbol: "λcp",
        description: "proton Compton wavelength",
    },
    Constant {
        code: 15,
        name: "lcn",
        symbol: "λcn",
        description: "neutron Compton wavelength",
    },
    Constant {
        code: 16,
        name: "Rinf",
        symbol: "R∞",
        description: "Rydberg constant",
    },
    Constant {
        code: 17,
        name: "u",
        symbol: "u",
        description: "atomic mass unit",
    },
    Constant {
        code: 18,
        name: "mup",
        symbol: "μp",
        description: "proton magnetic moment",
    },
    Constant {
        code: 19,
        name: "mue",
        symbol: "μe",
        description: "electron magnetic moment",
    },
    Constant {
        code: 20,
        name: "mun",
        symbol: "μn",
        description: "neutron magnetic moment",
    },
    Constant {
        code: 21,
        name: "mumu",
        symbol: "μμ",
        description: "muon magnetic moment",
    },
    Constant {
        code: 22,
        name: "F",
        symbol: "F",
        description: "Faraday constant",
    },
    Constant {
        code: 23,
        name: "eq",
        symbol: "e",
        description: "elementary charge",
    },
    Constant {
        code: 24,
        name: "NA",
        symbol: "NA",
        description: "Avogadro constant",
    },
    Constant {
        code: 25,
        name: "k",
        symbol: "k",
        description: "Boltzmann constant",
    },
    Constant {
        code: 26,
        name: "Vm",
        symbol: "Vm",
        description: "molar volume of ideal gas",
    },
    Constant {
        code: 27,
        name: "R",
        symbol: "R",
        description: "molar gas constant",
    },
    Constant {
        code: 28,
        name: "C0",
        symbol: "C0",
        description: "speed of light in vacuum",
    },
    Constant {
        code: 29,
        name: "C1",
        symbol: "C1",
        description: "first radiation constant",
    },
    Constant {
        code: 30,
        name: "C2",
        symbol: "C2",
        description: "second radiation constant",
    },
    Constant {
        code: 31,
        name: "sigma",
        symbol: "σ",
        description: "Stefan-Boltzmann constant",
    },
    Constant {
        code: 32,
        name: "eps0",
        symbol: "ε0",
        description: "electric constant",
    },
    Constant {
        code: 33,
        name: "mu0",
        symbol: "μ0",
        description: "magnetic constant",
    },
    Constant {
        code: 34,
        name: "phi0",
        symbol: "φ0",
        description: "magnetic flux quantum",
    },
    Constant {
        code: 35,
        name: "g",
        symbol: "g",
        description: "standard acceleration of gravity",
    },
    Constant {
        code: 36,
        name: "G0",
        symbol: "G0",
        description: "conductance quantum",
    },
    Constant {
        code: 37,
        name: "Z0",
        symbol: "Z0",
        description: "characteristic impedance of vacuum",
    },
    Constant {
        code: 38,
        name: "tK",
        symbol: "t",
        description: "Celsius temperature",
    },
    Constant {
        code: 39,
        name: "G",
        symbol: "G",
        description: "Newtonian constant of gravitation",
    },
    Constant {
        code: 40,
        name: "atm",
        symbol: "atm",
        description: "standard atmosphere",
    },
];

/// The constant with the given menu number, `1..=40`.
pub fn by_code(code: u8) -> Option<&'static Constant> {
    CONSTANTS.iter().find(|c| c.code == code)
}

/// The constant for a `phys.NAME` spelling: either its ASCII name or the
/// symbol the display shows.
///
/// Unlike the interpreter's own `lookup`, this accepts the elementary charge's
/// symbol `e`: `.fxc` only ever looks a constant up after a `phys.` prefix, so
/// the surrounding namespace already rules out a clash with Euler's number.
pub fn lookup(spelling: &str) -> Option<&'static Constant> {
    CONSTANTS
        .iter()
        .find(|c| c.name == spelling || c.symbol == spelling)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_holds_all_forty_constants_in_menu_order() {
        assert_eq!(CONSTANTS.len(), 40);
        for (i, c) in CONSTANTS.iter().enumerate() {
            assert_eq!(c.code as usize, i + 1, "{} is out of order", c.name);
        }
    }

    #[test]
    fn names_and_symbols_are_unique() {
        for (i, a) in CONSTANTS.iter().enumerate() {
            for b in &CONSTANTS[i + 1..] {
                assert_ne!(a.name, b.name, "duplicate name");
                assert_ne!(a.symbol, b.symbol, "duplicate symbol");
            }
        }
    }

    #[test]
    fn code_lookup_round_trips() {
        for c in &CONSTANTS {
            assert_eq!(by_code(c.code), Some(c));
        }
        assert_eq!(by_code(0), None);
        assert_eq!(by_code(41), None);
    }

    #[test]
    fn lookup_accepts_names_and_symbols() {
        assert_eq!(lookup("h").unwrap().code, 6);
        assert_eq!(lookup("hbar"), lookup("ħ"));
        assert_eq!(lookup("mumu"), lookup("μμ"));
        assert_eq!(lookup("Rinf"), lookup("R∞"));
        assert_eq!(lookup("nope"), None);
    }

    /// `phys.e` is the elementary charge: the namespace removes the clash with
    /// Euler's number, which the interpreter's bare-source lookup cannot do.
    #[test]
    fn the_elementary_charge_is_reachable_as_e_and_eq() {
        assert_eq!(lookup("e").unwrap().code, 23);
        assert_eq!(lookup("eq").unwrap().code, 23);
        assert_eq!(lookup("eq").unwrap().symbol, "e");
    }
}

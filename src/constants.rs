//! The fx-50FH II's 40 built-in scientific constants.
//!
//! The calculator stores these in a ten-page menu, four entries per page, and
//! a program inserts one with the `CONST` key followed by its two-digit number.
//! In source text a constant is written with its ASCII name (`mp`, `hbar`,
//! `C0`) or, when the font allows, with the symbol the display shows (`mμ`,
//! `ħ`, `R∞`).
//!
//! Everything lives in one table rather than forty free items so that the
//! constants occupy a single name — [`CONSTANTS`] — instead of polluting the
//! namespace with forty separate symbols. [`ConstName::Physical`] refers to an
//! entry by its menu number.
//!
//! The values are the 2010 CODATA revision, which is the one this calculator
//! shipped with.
//!
//! [`ConstName::Physical`]: crate::token::ConstName::Physical

/// One of the calculator's built-in scientific constants.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PhysicalConstant {
    /// Menu number, `1..=40`, as printed in the user's guide.
    pub code: u8,
    /// ASCII name used in program source (`mp`, `hbar`, `C0`).
    pub name: &'static str,
    /// The symbol the calculator's display shows (`mμ`, `ħ`, `R∞`).
    ///
    /// This is the same as `name` for the constants whose display spelling is
    /// already ASCII.
    pub symbol: &'static str,
    /// The stored value.
    pub value: f64,
    /// SI unit, for documentation only.
    pub unit: &'static str,
    /// English name, for documentation only.
    pub description: &'static str,
}

impl PhysicalConstant {
    /// A label such as `proton mass (mp)`.
    #[cfg(test)]
    pub(crate) fn label(&self) -> String {
        format!("{} ({})", self.description, self.name)
    }
}

/// All 40 constants, in menu order.
pub const CONSTANTS: [PhysicalConstant; 40] = [
    PhysicalConstant {
        code: 1,
        name: "mp",
        symbol: "mp",
        value: 1.672_621_777e-27,
        unit: "kg",
        description: "proton mass",
    },
    PhysicalConstant {
        code: 2,
        name: "mn",
        symbol: "mn",
        value: 1.674_927_351e-27,
        unit: "kg",
        description: "neutron mass",
    },
    PhysicalConstant {
        code: 3,
        name: "me",
        symbol: "me",
        value: 9.109_382_91e-31,
        unit: "kg",
        description: "electron mass",
    },
    PhysicalConstant {
        code: 4,
        name: "mmu",
        symbol: "mμ",
        value: 1.883_531_475e-28,
        unit: "kg",
        description: "muon mass",
    },
    PhysicalConstant {
        code: 5,
        name: "a0",
        symbol: "a0",
        value: 5.291_772_109_2e-11,
        unit: "m",
        description: "Bohr radius",
    },
    PhysicalConstant {
        code: 6,
        name: "h",
        symbol: "h",
        value: 6.626_069_57e-34,
        unit: "J s",
        description: "Planck constant",
    },
    PhysicalConstant {
        code: 7,
        name: "muN",
        symbol: "μN",
        value: 5.050_783_53e-27,
        unit: "J T⁻¹",
        description: "nuclear magneton",
    },
    PhysicalConstant {
        code: 8,
        name: "muB",
        symbol: "μB",
        value: 9.274_009_68e-24,
        unit: "J T⁻¹",
        description: "Bohr magneton",
    },
    PhysicalConstant {
        code: 9,
        name: "hbar",
        symbol: "ħ",
        value: 1.054_571_726e-34,
        unit: "J s",
        description: "reduced Planck constant",
    },
    PhysicalConstant {
        code: 10,
        name: "alpha",
        symbol: "α",
        value: 0.007_297_352_569_8,
        unit: "",
        description: "fine-structure constant",
    },
    PhysicalConstant {
        code: 11,
        name: "re",
        symbol: "re",
        value: 2.817_940_326_7e-15,
        unit: "m",
        description: "classical electron radius",
    },
    PhysicalConstant {
        code: 12,
        name: "lc",
        symbol: "λc",
        value: 2.426_310_238_9e-12,
        unit: "m",
        description: "Compton wavelength",
    },
    PhysicalConstant {
        code: 13,
        name: "gp",
        symbol: "γp",
        value: 267_522_200.5,
        unit: "s⁻¹ T⁻¹",
        description: "proton gyromagnetic ratio",
    },
    PhysicalConstant {
        code: 14,
        name: "lcp",
        symbol: "λcp",
        value: 1.321_409_856_23e-15,
        unit: "m",
        description: "proton Compton wavelength",
    },
    PhysicalConstant {
        code: 15,
        name: "lcn",
        symbol: "λcn",
        value: 1.319_590_906_8e-15,
        unit: "m",
        description: "neutron Compton wavelength",
    },
    PhysicalConstant {
        code: 16,
        name: "Rinf",
        symbol: "R∞",
        value: 10_973_731.568_539,
        unit: "m⁻¹",
        description: "Rydberg constant",
    },
    PhysicalConstant {
        code: 17,
        name: "u",
        symbol: "u",
        value: 1.660_538_921e-27,
        unit: "kg",
        description: "atomic mass unit",
    },
    PhysicalConstant {
        code: 18,
        name: "mup",
        symbol: "μp",
        value: 1.410_606_743e-26,
        unit: "J T⁻¹",
        description: "proton magnetic moment",
    },
    PhysicalConstant {
        code: 19,
        name: "mue",
        symbol: "μe",
        value: -9.284_764_3e-24,
        unit: "J T⁻¹",
        description: "electron magnetic moment",
    },
    PhysicalConstant {
        code: 20,
        name: "mun",
        symbol: "μn",
        value: -9.662_364_7e-27,
        unit: "J T⁻¹",
        description: "neutron magnetic moment",
    },
    PhysicalConstant {
        code: 21,
        name: "mumu",
        symbol: "μμ",
        value: -4.490_448_07e-26,
        unit: "J T⁻¹",
        description: "muon magnetic moment",
    },
    PhysicalConstant {
        code: 22,
        name: "F",
        symbol: "F",
        value: 96_485.336_5,
        unit: "C mol⁻¹",
        description: "Faraday constant",
    },
    PhysicalConstant {
        code: 23,
        name: "eq",
        symbol: "e",
        value: 1.602_176_565e-19,
        unit: "C",
        description: "elementary charge",
    },
    PhysicalConstant {
        code: 24,
        name: "NA",
        symbol: "NA",
        value: 6.022_141_29e23,
        unit: "mol⁻¹",
        description: "Avogadro constant",
    },
    PhysicalConstant {
        code: 25,
        name: "k",
        symbol: "k",
        value: 1.380_648_8e-23,
        unit: "J K⁻¹",
        description: "Boltzmann constant",
    },
    PhysicalConstant {
        code: 26,
        name: "Vm",
        symbol: "Vm",
        value: 0.022_413_968,
        unit: "m³ mol⁻¹",
        description: "molar volume of ideal gas",
    },
    PhysicalConstant {
        code: 27,
        name: "R",
        symbol: "R",
        value: 8.314_462_1,
        unit: "J mol⁻¹ K⁻¹",
        description: "molar gas constant",
    },
    PhysicalConstant {
        code: 28,
        name: "C0",
        symbol: "C0",
        value: 299_792_458.0,
        unit: "m s⁻¹",
        description: "speed of light in vacuum",
    },
    PhysicalConstant {
        code: 29,
        name: "C1",
        symbol: "C1",
        value: 3.741_771_53e-16,
        unit: "W m²",
        description: "first radiation constant",
    },
    PhysicalConstant {
        code: 30,
        name: "C2",
        symbol: "C2",
        value: 0.014_387_77,
        unit: "m K",
        description: "second radiation constant",
    },
    PhysicalConstant {
        code: 31,
        name: "sigma",
        symbol: "σ",
        value: 5.670_373e-8,
        unit: "W m⁻² K⁻⁴",
        description: "Stefan-Boltzmann constant",
    },
    PhysicalConstant {
        code: 32,
        name: "eps0",
        symbol: "ε0",
        value: 8.854_187_817e-12,
        unit: "F m⁻¹",
        description: "electric constant",
    },
    PhysicalConstant {
        code: 33,
        name: "mu0",
        symbol: "μ0",
        value: 1.256_637_061_4e-6,
        unit: "N A⁻²",
        description: "magnetic constant",
    },
    PhysicalConstant {
        code: 34,
        name: "phi0",
        symbol: "φ0",
        value: 2.067_833_758e-15,
        unit: "Wb",
        description: "magnetic flux quantum",
    },
    PhysicalConstant {
        code: 35,
        name: "g",
        symbol: "g",
        value: 9.806_65,
        unit: "m s⁻²",
        description: "standard acceleration of gravity",
    },
    PhysicalConstant {
        code: 36,
        name: "G0",
        symbol: "G0",
        value: 7.748_091_734_6e-5,
        unit: "S",
        description: "conductance quantum",
    },
    PhysicalConstant {
        code: 37,
        name: "Z0",
        symbol: "Z0",
        value: 376.730_313_461,
        unit: "Ω",
        description: "characteristic impedance of vacuum",
    },
    PhysicalConstant {
        code: 38,
        name: "tK",
        symbol: "t",
        value: 273.15,
        unit: "K",
        description: "Celsius temperature",
    },
    PhysicalConstant {
        code: 39,
        name: "G",
        symbol: "G",
        value: 6.673_84e-11,
        unit: "m³ kg⁻¹ s⁻²",
        description: "Newtonian constant of gravitation",
    },
    PhysicalConstant {
        code: 40,
        name: "atm",
        symbol: "atm",
        value: 101_325.0,
        unit: "Pa",
        description: "standard atmosphere",
    },
];

/// The constant with the given menu number, `1..=40`.
pub fn by_code(code: u8) -> Option<&'static PhysicalConstant> {
    CONSTANTS.iter().find(|c| c.code == code)
}

/// The constant with the given ASCII [`name`](PhysicalConstant::name).
#[cfg(test)]
pub(crate) fn by_name(name: &str) -> Option<&'static PhysicalConstant> {
    CONSTANTS.iter().find(|c| c.name == name)
}

/// The constant for a spelling used in program source: either its ASCII name
/// or the symbol its display shows.
///
/// The elementary charge is the one exception. Its symbol is `e`, which source
/// text reserves for Euler's number, so it can only be reached as `eq`.
pub(crate) fn lookup(spelling: &str) -> Option<&'static PhysicalConstant> {
    CONSTANTS
        .iter()
        .find(|c| c.name == spelling || (c.symbol == spelling && c.symbol != "e"))
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
    fn name_lookup_finds_the_constant() {
        assert_eq!(by_name("h").unwrap().value, 6.626_069_57e-34);
        assert_eq!(by_name("hbar").unwrap().code, 9);
        assert_eq!(by_name("atm").unwrap().value, 101_325.0);
        assert_eq!(by_name("nope"), None);
    }

    /// Source spellings accept the display symbol as well as the ASCII name.
    #[test]
    fn source_lookup_accepts_names_and_symbols() {
        assert_eq!(lookup("hbar"), lookup("ħ"));
        assert_eq!(lookup("mumu"), lookup("μμ"));
        assert_eq!(lookup("Rinf").unwrap().value, 10_973_731.568_539);
    }

    /// `e` is Euler's number in source, so the elementary charge needs `eq`.
    #[test]
    fn the_elementary_charge_is_not_reachable_as_e() {
        assert_eq!(lookup("e"), None);
        assert_eq!(lookup("eq").unwrap().code, 23);
        // Its display symbol is still reported.
        assert_eq!(by_code(23).unwrap().symbol, "e");
    }

    /// Two entries differ from the older figures an earlier revision printed.
    #[test]
    fn values_follow_the_2010_codata_revision() {
        assert_eq!(by_name("gp").unwrap().value, 267_522_200.5);
        assert_eq!(by_name("mumu").unwrap().value, -4.490_448_07e-26);
        assert_eq!(by_name("Vm").unwrap().value, 0.022_413_968);
    }

    #[test]
    fn label_reads_naturally() {
        assert_eq!(by_name("mp").unwrap().label(), "proton mass (mp)");
    }
}

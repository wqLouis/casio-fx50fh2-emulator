//! Base-n arithmetic and display.
//!
//! While a base is selected the calculator performs integer arithmetic that
//! wraps to a fixed word size and displays results with a base suffix
//! (`d`, `h`, `b`, `o`).

/// A number base selected by the `Dec`/`Hex`/`Bin`/`Oct` commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Base {
    Dec,
    Hex,
    Bin,
    Oct,
}

impl Base {
    /// The radix, for `u32::from_str_radix`.
    pub fn radix(self) -> u32 {
        match self {
            Base::Dec => 10,
            Base::Hex => 16,
            Base::Bin => 2,
            Base::Oct => 8,
        }
    }

    /// The word size in bits.  Binary uses 10 bits, octal 30, and
    /// decimal/hexadecimal 32.
    pub fn bits(self) -> u32 {
        match self {
            Base::Dec | Base::Hex => 32,
            Base::Bin => 10,
            Base::Oct => 30,
        }
    }

    /// The display suffix.
    pub fn suffix(self) -> char {
        match self {
            Base::Dec => 'd',
            Base::Hex => 'h',
            Base::Bin => 'b',
            Base::Oct => 'o',
        }
    }

    fn mask(self) -> u64 {
        let bits = self.bits();
        if bits >= 64 {
            u64::MAX
        } else {
            (1u64 << bits) - 1
        }
    }

    /// Convert a number to its unsigned word representation (truncating toward
    /// zero and masking to the word size).
    pub fn to_word(self, x: f64) -> u32 {
        let truncated = if x.is_finite() { x.trunc() } else { 0.0 };
        let as_int = truncated as i64 as u64;
        (as_int & self.mask()) as u32
    }

    /// Interpret a word as a signed value in this base.
    pub fn from_word(self, word: u32) -> f64 {
        let masked = (word as u64) & self.mask();
        let sign_bit = 1u64 << (self.bits() - 1);
        if masked & sign_bit != 0 {
            (masked as i64 - (1i64 << self.bits())) as f64
        } else {
            masked as f64
        }
    }

    /// Wrap a number into the signed range of this base.
    pub fn wrap(self, x: f64) -> f64 {
        self.from_word(self.to_word(x))
    }

    /// Format a number in this base with its suffix.
    pub fn format(self, x: f64) -> String {
        let word = self.to_word(x);
        match self {
            Base::Dec => format!("{}d", self.from_word(word) as i64),
            Base::Hex => format!("{:X}h", word),
            Base::Bin => format!("{:b}b", word),
            Base::Oct => format!("{:o}o", word),
        }
    }

    /// Parse a tagged literal such as `1Fh`, `1010b`, `17o` or `42d`.
    ///
    /// Returns `None` if the text does not carry a recognised suffix or the
    /// digits are not valid for that base.
    pub fn parse(text: &str) -> Option<f64> {
        let text = text.trim();
        let (body, base) = match text.chars().last()? {
            'd' | 'D' => (&text[..text.len() - 1], Base::Dec),
            'h' | 'H' => (&text[..text.len() - 1], Base::Hex),
            'b' => (&text[..text.len() - 1], Base::Bin),
            'o' => (&text[..text.len() - 1], Base::Oct),
            _ => return None,
        };
        if body.is_empty() {
            return None;
        }
        let negative = body.starts_with('-');
        let digits = body.trim_start_matches(['-', '+']);
        if digits.is_empty() {
            return None;
        }
        match base {
            Base::Dec => {
                let value: f64 = body.parse().ok()?;
                Some(base.wrap(value))
            }
            _ => {
                let magnitude = u64::from_str_radix(digits, base.radix()).ok()?;
                let word = if negative {
                    (!magnitude).wrapping_add(1)
                } else {
                    magnitude
                };
                Some(base.from_word((word & base.mask()) as u32))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tagged_literals() {
        assert_eq!(Base::parse("1Fh"), Some(31.0));
        assert_eq!(Base::parse("1010b"), Some(10.0));
        assert_eq!(Base::parse("17o"), Some(15.0));
        assert_eq!(Base::parse("42d"), Some(42.0));
        assert_eq!(Base::parse("10"), None);
    }

    #[test]
    fn wraps_signed_words() {
        let base = Base::Dec;
        assert_eq!(base.wrap(4294967295.0), -1.0);
        assert_eq!(base.to_word(-1.0), 0xFFFF_FFFF);
        assert_eq!(format!("{}", base.from_word(0xFFFF_FFFF)), "-1");
    }

    #[test]
    fn formats_each_base() {
        assert_eq!(Base::Dec.format(255.0), "255d");
        assert_eq!(Base::Hex.format(255.0), "FFh");
        assert_eq!(Base::Bin.format(10.0), "1010b");
        assert_eq!(Base::Oct.format(15.0), "17o");
    }
}

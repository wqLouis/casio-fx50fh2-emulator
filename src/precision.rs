//! Numeric fidelity: 15-significant-digit rounding and the fx-50FH II
//! auto-correction rules.
//!
//! The machine keeps a base-10 floating point number
//! `±A.BCDEFGHIJKLMNO × 10^n` with 15 significant digits.  After every
//! mathematical operation (but before the exponent is normalised) it applies
//! an auto-correction pass, reconstructed from
//! [`KeroppiMomo/calsimtor`](https://github.com/KeroppiMomo/calsimtor)'s
//! `DOC.md`:
//!
//! * if the last four significant digits `LMNO` satisfy `0 ≤ LMNO ≤ 9`, round
//!   down to 11 significant figures;
//! * if `9991 ≤ LMNO ≤ 9999`, round up to 11 significant figures (increment
//!   `K` and zero `LMNO`);
//! * if the first 13 significant digits are zero, the number collapses to
//!   zero.
//!
//! Literal numbers typed into a program are *not* auto-corrected; only the
//! results of operations are.  [`normalize`] is therefore applied to the
//! result of every arithmetic operation and never to a numeric literal.
//!
//! # Implementation notes
//!
//! The original implementation performed two `String` allocations and three
//! float↔string conversions per operation.  This version keeps the *identical*
//! `core::fmt` float formatting and `str::parse` code paths — so the results
//! are bit-for-bit unchanged — but formats into a fixed-size stack buffer
//! instead of the heap.  [`normalize`] additionally fuses the two round-trips
//! into one for normal inputs, which is provably exact (see the comment on
//! `normalize`).

use core::fmt::{self, Write as _};

/// Below this magnitude the fused [`normalize`] path is not used; the
/// reference two-round-trip path takes over.  The fusion's correctness proof
/// needs the intermediate `round15(x)` to be normal, so `1e-300` keeps the
/// fast path eight orders of magnitude clear of the subnormal regime
/// (`f64::MIN_POSITIVE ≈ 2.2e-308`).
const FUSED_MIN_MAGNITUDE: f64 = 1e-300;

/// A fixed-size stack buffer that implements [`core::fmt::Write`].
///
/// 48 bytes is comfortably more than any `{:.14e}` rendering of a finite
/// `f64` needs (sign, one leading digit, a dot, 14 fraction digits, `e`, a
/// sign and up to three exponent digits ≈ 22 bytes).
struct StackBuf {
    bytes: [u8; 48],
    len: usize,
}

impl StackBuf {
    const fn new() -> Self {
        Self {
            bytes: [0; 48],
            len: 0,
        }
    }

    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len]).unwrap_or("")
    }

    fn push(&mut self, byte: u8) {
        if self.len < self.bytes.len() {
            self.bytes[self.len] = byte;
            self.len += 1;
        }
    }
}

impl fmt::Write for StackBuf {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let bytes = s.as_bytes();
        if self.len + bytes.len() > self.bytes.len() {
            return Err(fmt::Error);
        }
        self.bytes[self.len..self.len + bytes.len()].copy_from_slice(bytes);
        self.len += bytes.len();
        Ok(())
    }
}

/// Render `x` with exactly 15 significant digits using std's formatter,
/// writing into a stack buffer.  `x` must be finite and non-zero.
fn format15(x: f64) -> StackBuf {
    let mut buf = StackBuf::new();
    // Cannot overflow: 48 bytes always suffice for `{:.14e}`.
    let _ = write!(buf, "{:.14e}", x);
    buf
}

/// Round to 15 significant decimal digits (the machine's working precision).
pub fn round15(x: f64) -> f64 {
    if !x.is_finite() || x == 0.0 {
        return x;
    }
    format15(x).as_str().parse().unwrap_or(x)
}

/// Apply the fx-50FH II auto-correction rules to an already 15-digit value.
pub fn autocorrect(x: f64) -> f64 {
    if !x.is_finite() || x == 0.0 {
        return x;
    }
    let negative = x < 0.0;
    let text = format15(x.abs());
    inspect_and_correct(negative, text.as_str()).unwrap_or(x)
}

/// Inspect a 15-significant-digit rendering of `|x|` and apply the
/// auto-correction rules.
///
/// Returns `Some(corrected)` when a rule fires, and `None` when the value is
/// left unchanged (in which case the caller decides what "unchanged" means:
/// [`autocorrect`] returns its input, [`normalize`] parses the rendering).
fn inspect_and_correct(negative: bool, text: &str) -> Option<f64> {
    let (mantissa, exp_text) = text.split_once('e')?;
    let exp: i32 = exp_text.parse().unwrap_or(0);

    let mut digits = [0u8; 15];
    let mut count = 0;
    for ch in mantissa.chars() {
        if let Some(d) = ch.to_digit(10)
            && count < digits.len()
        {
            digits[count] = d as u8;
            count += 1;
        }
    }
    if count != digits.len() {
        return None;
    }

    // "If the first 13 significant digits are zero, the number is corrected to
    // be zero."
    if digits[..13].iter().all(|&d| d == 0) {
        return Some(0.0);
    }

    let lmno = digits[11] as u32 * 1000
        + digits[12] as u32 * 100
        + digits[13] as u32 * 10
        + digits[14] as u32;

    if lmno <= 9 {
        digits[11..].fill(0);
        Some(render_mantissa(negative, &digits, exp))
    } else if (9991..=9999).contains(&lmno) {
        digits[11..].fill(0);
        let mut i = 10i32;
        while i >= 0 {
            if digits[i as usize] == 9 {
                digits[i as usize] = 0;
                i -= 1;
            } else {
                digits[i as usize] += 1;
                break;
            }
        }
        if i < 0 {
            // Carry out of the leading digit: the value becomes 1×10^(exp+1).
            let mut out = StackBuf::new();
            let _ = write!(out, "1e{}", exp + 1);
            Some(parse_signed(negative, out.as_str()))
        } else {
            Some(render_mantissa(negative, &digits, exp))
        }
    } else {
        None
    }
}

/// Parse `digits` as `d.dddddddddddddd × 10^exp`, applying the sign.
fn render_mantissa(negative: bool, digits: &[u8; 15], exp: i32) -> f64 {
    let mut out = StackBuf::new();
    out.push(b'0' + digits[0]);
    out.push(b'.');
    for &d in &digits[1..] {
        out.push(b'0' + d);
    }
    out.push(b'e');
    let _ = write!(out, "{exp}");
    parse_signed(negative, out.as_str())
}

fn parse_signed(negative: bool, text: &str) -> f64 {
    let value: f64 = text.parse().unwrap_or(0.0);
    if negative { -value } else { value }
}

/// The full operation-level normalisation: `autocorrect(round15(x))`.
///
/// This replaces the old `r15` helper and is applied after every arithmetic
/// operation.
///
/// # Why the fused path is exact
///
/// The reference implementation is `autocorrect(round15(x))`, i.e. it renders
/// `y = round15(x)` to 15 significant digits, applies the digit rule, and
/// (when a rule fires) parses the corrected rendering.  This version instead
/// renders `x` **once** and applies the digit rule to that rendering.
///
/// The two agree as long as `round15(x)` renders to the same 15-digit string
/// as `x` itself.  Write `s = format15(x)` and `y = parse(s)` (the nearest
/// `f64` to `s`), and suppose `y` is normal.  Since `y` is the correctly
/// rounded `f64` of `s`, `|y − s| ≤ ½·ulp(y)`.  With `e = ⌊log₁₀|y|⌋`:
///
/// ```text
/// ½·ulp(y) ≤ |y|·2⁻⁵³ < 10^(e+1)·2⁻⁵³ = 1.111e-15·10^e
/// ½·(15-digit spacing) = ½·10^(e-14)   = 5.000e-15·10^e
/// ```
///
/// and `1.111e-15 < 5.000e-15`, so `y` is strictly nearer to `s` than to any
/// other 15-digit decimal.  Hence `format15(y) = s`, the digit rule sees the
/// same `LMNO`, and parsing the corrected string yields the same `f64`.  The
/// same argument applies to the sign by symmetry.
///
/// The only escape hatch is `y` subnormal: then `ulp(y)` is fixed and can
/// dwarf the relative decimal spacing, so the round-trip can move the tail
/// digits.  That requires `|x|` to be within half a 15-digit ulp of
/// `f64::MIN_POSITIVE`; for anything below [`FUSED_MIN_MAGNITUDE`] we simply
/// fall back to the reference two-step path.
pub fn normalize(x: f64) -> f64 {
    if !x.is_finite() || x == 0.0 {
        return x;
    }
    let magnitude = x.abs();
    if magnitude < FUSED_MIN_MAGNITUDE {
        return autocorrect(round15(x));
    }
    let negative = x < 0.0;
    let text = format15(magnitude);
    match inspect_and_correct(negative, text.as_str()) {
        Some(corrected) => corrected,
        // No rule fired: the answer is exactly `round15(x)`, which is the
        // signed parse of the rendering we already produced.
        None => parse_signed(negative, text.as_str()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round15_keeps_fifteen_digits() {
        assert_eq!(round15(2.0 + 3.0 * 4.0), 14.0);
        assert_eq!(round15(1.0 / 3.0), 0.333333333333333);
    }

    #[test]
    fn small_tail_rounds_down() {
        // 0.8 + 1e-15 has LMNO = 0001, so it collapses back to 0.8.
        assert_eq!(normalize(0.8 + 1e-15), 0.8);
    }

    #[test]
    fn large_tail_rounds_up() {
        // The last four significant digits are 9995, so K is incremented and
        // LMNO zeroed.
        let value: f64 = "1.23456789019995".parse().unwrap();
        let corrected = autocorrect(round15(value));
        let expected: f64 = "1.2345678902".parse().unwrap();
        assert!((corrected - expected).abs() < 1e-15, "{corrected}");
    }

    #[test]
    fn uncorrected_values_are_unchanged() {
        let value = 1.2345678901234;
        assert_eq!(autocorrect(round15(value)), round15(value));
    }
}

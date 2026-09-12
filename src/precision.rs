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

/// `10^k` for `0 ≤ k ≤ 22`.
///
/// `f64` represents these exactly — `10^22` is the last power of ten with that
/// property (the next one, `10^23`, rounds to `99999999999999991611392`).
/// Scaling by an *exact* power of ten costs one correctly-rounded operation and
/// injects no error of its own, which is what makes the fast path below
/// provable. Above `10^22` the scaling would be lossy, so the caller falls back.
const POW10: [f64; 23] = [
    1e0, 1e1, 1e2, 1e3, 1e4, 1e5, 1e6, 1e7, 1e8, 1e9, 1e10, 1e11, 1e12, 1e13, 1e14, 1e15, 1e16,
    1e17, 1e18, 1e19, 1e20, 1e21, 1e22,
];

fn pow10(k: i32) -> Option<f64> {
    usize::try_from(k).ok().and_then(|k| POW10.get(k).copied())
}

/// `floor(log10(|x|))`, corrected so that `10^e ≤ |x| < 10^(e+1)` holds exactly.
///
/// `f64::log10` is not exact, and being off by one here would scale by the wrong
/// power of ten, so the estimate is checked against exactly-representable powers
/// and adjusted. Only the range where those powers are exact can be corrected,
/// so anything outside it reports `None` and takes the string path.
fn decimal_exponent(magnitude: f64) -> Option<i32> {
    let mut e = magnitude.log10().floor() as i32;
    // Adjust at most once in each direction; a `log10` result is never off by
    // more than one at these magnitudes.
    if pow10(e + 1).is_some_and(|p| magnitude >= p) {
        e += 1;
    }
    if pow10(e).is_some_and(|p| magnitude < p) {
        e -= 1;
    }
    // The bracketing powers are only used to *correct* the estimate, so their
    // absence is not fatal: the caller validates the result by requiring the
    // scaled value to land in `[10^14, 10^15)`, which an off-by-one exponent
    // cannot satisfy.
    Some(e)
}

/// The autocorrection rules applied to a 15-digit integer mantissa.
///
/// `y` is the 15 significant digits as an integer, so the rule's `LMNO` is
/// `y % 10_000` and digit `K` is `(y / 10_000) % 10`. Returns the corrected
/// mantissa, which may carry into a 16th digit (`y == 10^15`), in which case the
/// caller must bump the exponent.
fn autocorrect_mantissa(y: u64) -> u64 {
    let lmno = y % 10_000;
    if lmno <= 9 {
        // Round down to 11 significant figures.
        y - lmno
    } else if lmno >= 9991 {
        // Round up: increment K, zero LMNO.
        (y / 10_000 + 1) * 10_000
    } else {
        y
    }
}

/// The fast path for [`normalize`]: decimal arithmetic on an integer mantissa.
///
/// Returns `None` whenever the result cannot be guaranteed identical to the
/// string round-trip, in which case the caller uses that instead. Every `None`
/// is a *correctness* decision, never a performance one, so the conditions are
/// deliberately strict.
///
/// # Why this is exact
///
/// Let `y = round15(x)` and `e10 = floor(log10|x|)`. The 15 significant digits
/// of `x` are the integer `T = |x| · 10^(14 - e10)`, and `round15` is `T`
/// rounded to a whole number (ties to even, as decimal parsing does).
///
/// So the fast path computes `scaled = |x| · 10^m` for `m = 14 - e10` using a
/// single multiply or divide by an **exact** power of ten, and rounds it. That
/// differs from `T` only by the rounding of that one operation: at most half an
/// ulp, i.e. `|scaled - T| ≤ 0.0625` for `scaled ∈ [10^14, 10^15)`, where the
/// largest ulp is `0.125`. Two checks make the rounding certain:
///
/// * the value must land strictly inside `[10^14, 10^15)`, so no exponent drift
///   has occurred; and
/// * the true value's fractional part must be further than that error from the
///   `0.5` boundary ([`ROUNDING_MARGIN`]), so no rounding decision can flip.
///
/// The tie case is excluded by the same margin: an exact `.5` sits exactly on
/// the boundary, and Rust's `f64::round` breaks ties away from zero whereas
/// decimal parsing breaks them to even, so ties must take the string path.
///
/// When both checks pass, `scaled.round()` *is* `T` rounded, and from there on
/// all the arithmetic is on a `< 2^53` integer, which is exact.
/// # Panics
/// Never; every failure returns `None`.
const ROUNDING_MARGIN: f64 = 0.15;

fn fast_normalize(x: f64) -> Option<f64> {
    // Cheap pre-check, before any real work: the fast path needs
    // `10^|14 - e10|` to be exactly representable, so `e10` must lie in
    // `[-8, 36]`, i.e. `|x|` roughly in `[1e-8, 1e36]`. The binary exponent is
    // free to read from the bits, and rejecting out-of-range magnitudes here
    // means the common case of very large or very small numbers pays nothing
    // for a fast path it cannot use.
    const MIN_BIASED: u64 = 1023 - 26; // ≈ 1.5e-8
    const MAX_BIASED: u64 = 1023 + 118; // ≈ 3.3e35
    let biased = (x.to_bits() >> 52) & 0x7FF;
    if !(MIN_BIASED..=MAX_BIASED).contains(&biased) {
        return None;
    }

    let magnitude = x.abs();
    let negative = x < 0.0;

    let e10 = decimal_exponent(magnitude)?;
    let m = 14 - e10;
    let scale = pow10(m.unsigned_abs() as i32)?;

    // One correctly-rounded scaling by an exact power of ten.
    let scaled = if m >= 0 {
        magnitude * scale
    } else {
        magnitude / scale
    };

    // Exponent drift (the value is not the 15-digit integer we assumed) means
    // the caller's `e10` was wrong; let the string path sort it out.
    if !(1e14..1e15).contains(&scaled) {
        return None;
    }

    // Is the rounding decision unambiguous? `scaled` carries up to 0.0625 of
    // error, so anything near the `.5` boundary — including an exact tie — must
    // fall back.
    let fraction = scaled - scaled.floor();
    if (fraction - 0.5).abs() <= ROUNDING_MARGIN {
        return None;
    }

    let mantissa = scaled.round();
    if !(1e14..1e15).contains(&mantissa) {
        return None;
    }
    // `< 10^15 < 2^53`, so the conversion is exact and the integer arithmetic
    // below cannot lose a digit.
    let mut y = mantissa as u64;
    let mut e10 = e10;

    let corrected = autocorrect_mantissa(y);
    if corrected >= 1_000_000_000_000_000 {
        // The mantissa carried out of 15 digits: the value becomes
        // `1 × 10^(e10+1)`, whose 15-digit mantissa is `10^14` — *not* the
        // 16-digit `10^15` the rounding produced.
        y = 100_000_000_000_000;
        e10 += 1;
    } else {
        y = corrected;
    }

    // Scale back by the adjusted exponent, again by an exact power of ten.
    let m = 14 - e10;
    let scale = pow10(m.unsigned_abs() as i32)?;
    let value = if m >= 0 {
        y as f64 / scale
    } else {
        y as f64 * scale
    };
    if !value.is_finite() {
        return None;
    }
    Some(if negative { -value } else { value })
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
/// `f64::MIN_POSITIVE`; for anything below `FUSED_MIN_MAGNITUDE` we simply
/// fall back to the reference two-step path.
pub fn normalize(x: f64) -> f64 {
    if !x.is_finite() || x == 0.0 {
        return x;
    }
    let magnitude = x.abs();
    if magnitude < FUSED_MIN_MAGNITUDE {
        return autocorrect(round15(x));
    }
    // Try the integer-mantissa path first; it is provably identical wherever it
    // succeeds, and falls back to the decimal round-trip wherever it cannot be
    // sure.
    if let Some(value) = fast_normalize(x) {
        return value;
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

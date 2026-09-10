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

/// Round to 15 significant decimal digits (the machine's working precision).
pub fn round15(x: f64) -> f64 {
    if !x.is_finite() || x == 0.0 {
        return x;
    }
    let text = format!("{:.14e}", x);
    text.parse().unwrap_or(x)
}

/// Apply the fx-50FH II auto-correction rules to an already 15-digit value.
pub fn autocorrect(x: f64) -> f64 {
    if !x.is_finite() || x == 0.0 {
        return x;
    }
    let negative = x < 0.0;
    let magnitude = x.abs();
    let text = format!("{magnitude:.14e}");
    let Some((mantissa, exp_text)) = text.split_once('e') else {
        return x;
    };
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
        return x;
    }

    // "If the first 13 significant digits are zero, the number is corrected to
    // be zero."
    if digits[..13].iter().all(|&d| d == 0) {
        return 0.0;
    }

    let lmno = digits[11] as u32 * 1000
        + digits[12] as u32 * 100
        + digits[13] as u32 * 10
        + digits[14] as u32;

    let corrected = if lmno <= 9 {
        digits[11..].fill(0);
        Some(exp)
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
            return signed(negative, format!("1e{}", exp + 1));
        }
        Some(exp)
    } else {
        None
    };

    match corrected {
        Some(exp) => {
            let mantissa = format!(
                "{}.{}{}{}{}{}{}{}{}{}{}{}{}{}{}",
                digits[0],
                digits[1],
                digits[2],
                digits[3],
                digits[4],
                digits[5],
                digits[6],
                digits[7],
                digits[8],
                digits[9],
                digits[10],
                digits[11],
                digits[12],
                digits[13],
                digits[14]
            );
            signed(negative, format!("{mantissa}e{exp}"))
        }
        None => x,
    }
}

fn signed(negative: bool, text: String) -> f64 {
    let value: f64 = text.parse().unwrap_or(0.0);
    if negative { -value } else { value }
}

/// The full operation-level normalisation: `autocorrect(round15(x))`.
///
/// This replaces the old `r15` helper and is applied after every arithmetic
/// operation.
pub fn normalize(x: f64) -> f64 {
    autocorrect(round15(x))
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

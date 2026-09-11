//! End-to-end tests for the numeric-fidelity / autocorrection layer.

use casio_fx50fh2::precision::{autocorrect, normalize, round15};
use casio_fx50fh2::{Interpreter, MockHost, compile};

fn output(source: &str) -> Vec<String> {
    let program = compile(source).unwrap();
    let mut interp = Interpreter::new(program, MockHost::default());
    interp.run().unwrap();
    interp.into_host().output
}

#[test]
fn near_integer_fractions_collapse() {
    // 1/3 rounds to 0.999999999999999, whose LMNO = 9999 is autocorrected.
    assert_eq!(output("(1┘3)*3◢"), vec!["1"]);
    assert_eq!(output("0.1+0.2◢"), vec!["0.3"]);
}

#[test]
fn tiny_tails_are_dropped() {
    // `0.8 + 1E-15` has LMNO = 0001 and collapses back to 0.8.
    assert_eq!(output("0.8 + 1E-15◢"), vec!["0.8"]);
    assert_eq!(output("0.8 + 1E-14◢"), vec!["0.8"]);
}

#[test]
fn normal_values_are_untouched() {
    assert_eq!(normalize(1.2345678901234), round15(1.2345678901234));
    assert_eq!(autocorrect(round15(2.5)), 2.5);
}

#[test]
fn norm1_thresholds() {
    // Norm1 shows decimals only inside [1e-2, 1e10).
    assert_eq!(output("Norm 1: 0.01◢"), vec!["0.01"]);
    assert_eq!(output("Norm 1: 0.001◢"), vec!["1e-3"]);
    assert_eq!(output("Norm 1: 9999999999◢"), vec!["9999999999"]);
    assert_eq!(output("Norm 1: 10000000000◢"), vec!["1e10"]);
}

#[test]
fn norm2_thresholds() {
    // Norm2 shows decimals down to 1e-9.
    assert_eq!(output("Norm 2: 0.00000001◢"), vec!["0.00000001"]);
    assert_eq!(output("Norm 2: 0.000000001◢"), vec!["0.000000001"]);
    assert_eq!(output("Norm 2: 0.0000000001◢"), vec!["1e-10"]);
}

#[test]
fn literals_are_not_autocorrected() {
    // The literal itself keeps its 15 digits; only the display rounds.
    let program = compile("123456789.010005◢").unwrap();
    let mut interp = Interpreter::new(program, MockHost::default());
    interp.run().unwrap();
    assert_eq!(interp.environment().ans(), 123456789.010005);
}

// ---------------------------------------------------------------------------
// Reference oracle and differential corpus tests
// ---------------------------------------------------------------------------
//
// The functions below are verbatim copies of the original, allocation-heavy
// `precision.rs` implementation.  They are *the* specification: the optimised
// production code must agree with them bit for bit.

fn oracle_round15(x: f64) -> f64 {
    if !x.is_finite() || x == 0.0 {
        return x;
    }
    let text = format!("{:.14e}", x);
    text.parse().unwrap_or(x)
}

fn oracle_autocorrect(x: f64) -> f64 {
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
            return oracle_signed(negative, format!("1e{}", exp + 1));
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
            oracle_signed(negative, format!("{mantissa}e{exp}"))
        }
        None => x,
    }
}

fn oracle_signed(negative: bool, text: String) -> f64 {
    let value: f64 = text.parse().unwrap_or(0.0);
    if negative { -value } else { value }
}

fn oracle_normalize(x: f64) -> f64 {
    oracle_autocorrect(oracle_round15(x))
}

/// A tiny deterministic xorshift PRNG so the corpus is reproducible without
/// pulling in `rand`.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn next_f64_unit(&mut self) -> f64 {
        // Uniform in [0, 1).
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Assert that all three optimised functions agree with the oracle on `x`.
fn agree(x: f64) -> Result<(), String> {
    let r = round15(x);
    let or = oracle_round15(x);
    if r.to_bits() != or.to_bits() {
        return Err(format!(
            "round15({x:?}): got {:?} ({:#018x}), want {:?} ({:#018x})",
            r,
            r.to_bits(),
            or,
            or.to_bits()
        ));
    }

    let a = autocorrect(x);
    let oa = oracle_autocorrect(x);
    if a.to_bits() != oa.to_bits() {
        return Err(format!(
            "autocorrect({x:?}): got {:?} ({:#018x}), want {:?} ({:#018x})",
            a,
            a.to_bits(),
            oa,
            oa.to_bits()
        ));
    }

    let n = normalize(x);
    let on = oracle_normalize(x);
    if n.to_bits() != on.to_bits() {
        return Err(format!(
            "normalize({x:?}): got {:?} ({:#018x}), want {:?} ({:#018x})",
            n,
            n.to_bits(),
            on,
            on.to_bits()
        ));
    }
    Ok(())
}

/// Build a decimal value from a 15-to-17 digit mantissa string and an
/// exponent, e.g. `(prefix=12345678901, tail=9995, extra=None, exp=-3)`
/// represents `1.23456789019995e-3`.
fn decimal(prefix: u128, tail: u32, extra: &[u8], exp: i32) -> f64 {
    let mut mantissa = format!("{prefix}{tail:04}");
    for &e in extra {
        mantissa.push(char::from(b'0' + e));
    }
    let value = format!("{}.{}e{}", &mantissa[0..1], &mantissa[1..], exp);
    value.parse().unwrap()
}

#[test]
fn fast_path_matches_oracle_bit_for_bit() {
    let mut rng = Rng::new(0x9E37_79B9_7F4A_7C15);
    let mut checked = 0u64;

    // 1. Random raw f64 bit patterns: covers every exponent, subnormals,
    //    infinities and NaNs.
    for _ in 0..150_000 {
        let x = f64::from_bits(rng.next_u64());
        agree(x).unwrap();
        checked += 1;
    }

    // 2. Random values spread over the machine's working range ±1e-99..1e99.
    for _ in 0..150_000 {
        let exp = (rng.next_u64() % 199) as i32 - 99;
        let unit = rng.next_f64_unit();
        let sign = if rng.next_u64() & 1 == 0 { 1.0 } else { -1.0 };
        let x = sign * unit * 10f64.powi(exp);
        agree(x).unwrap();
        checked += 1;
    }

    // 3. Values whose 15th significant digits sit near the autocorrect
    //    boundaries, with 15, 16 and 17 significant digits.
    let tails: [u32; 16] = [
        0, 1, 5, 9, 10, 11, 9990, 9991, 9992, 9995, 9998, 9999, 1234, 5000, 9000, 9500,
    ];
    for &tail in &tails {
        for _ in 0..2_500 {
            let prefix = 10_000_000_000u128 + (rng.next_u64() as u128 % 90_000_000_000);
            let exp = (rng.next_u64() % 199) as i32 - 99;
            let x = decimal(prefix, tail, &[], exp);
            agree(x).unwrap();
            checked += 1;

            for extra in 0..=9u8 {
                let x = decimal(prefix, tail, &[extra], exp);
                agree(x).unwrap();
                checked += 1;
            }

            // 17 significant digits as well.
            let extra2 = (rng.next_u64() % 100) as u8;
            let x = decimal(prefix, tail, &[extra2 / 10, extra2 % 10], exp);
            agree(x).unwrap();
            checked += 1;
        }
    }

    // 4. Exact powers of ten across the whole exponent range, plus their
    //    neighbours.
    for n in -323..=308i32 {
        let parsed: f64 = format!("1e{n}").parse().unwrap();
        agree(parsed).unwrap();
        checked += 1;
        agree(f64::from_bits(parsed.to_bits().wrapping_add(1))).unwrap();
        checked += 1;
        if parsed > 0.0 {
            agree(-parsed).unwrap();
            checked += 1;
        }
    }

    // 5. Hand-picked special values, including the narrow band around
    //    MIN_POSITIVE where the intermediate `round15` result can be subnormal.
    let specials = [
        0.0,
        -0.0,
        f64::MIN_POSITIVE,
        -f64::MIN_POSITIVE,
        f64::from_bits(1),
        -f64::from_bits(1),
        f64::from_bits(0x000F_FFFF_FFFF_FFFF),
        -f64::from_bits(0x000F_FFFF_FFFF_FFFF),
        f64::MAX,
        -f64::MAX,
        f64::MIN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NAN,
        -f64::NAN,
        2.2250738585072e-308,
        2.22507385850719e-308,
        2.2250738585072014e-308,
        1e-300,
        1e-301,
        9.99999999999999e-301,
        1e99,
        -1e99,
        1e-99,
        9.99999999999999e99,
    ];
    for x in specials {
        agree(x).unwrap();
        checked += 1;
    }
    // Walk one ulp at a time through the subnormal-adjacent band.
    let base = f64::MIN_POSITIVE.to_bits();
    for delta in 0..64u64 {
        agree(f64::from_bits(base + delta)).unwrap();
        agree(f64::from_bits(base - delta)).unwrap();
        agree(-f64::from_bits(base + delta)).unwrap();
        agree(-f64::from_bits(base - delta)).unwrap();
        checked += 4;
    }

    // 6. Every f64 with a 15-digit decimal mantissa and small exponent is
    //    covered by steps 2-3; additionally stress the tie-to-even rounding of
    //    `round15` with 16-digit midpoints.
    for _ in 0..100_000 {
        let prefix = 10_000_000_000u128 + (rng.next_u64() as u128 % 90_000_000_000);
        let tail = (rng.next_u64() % 10_000) as u32;
        let final_digit = (rng.next_u64() % 10) as u8;
        let exp = (rng.next_u64() % 199) as i32 - 99;
        agree(decimal(prefix, tail, &[final_digit], exp)).unwrap();
        checked += 1;
    }

    assert!(checked >= 500_000, "corpus too small: {checked}");
    eprintln!("differential corpus: {checked} values, all bit-for-bit equal");
}

// ---------------------------------------------------------------------------
// The integer-mantissa fast path in `normalize`
//
// These do not test a public API change; they exist because the fast path is
// only *conditionally* taken, so "the corpus passes" is a weak statement unless
// the corpus actually reaches it — and the boundaries it can get wrong are
// precisely where the autocorrection rule bites.

/// Every 15-digit mantissa whose last four digits sit on a rule boundary,
/// across exponents spanning the fast path's window and well outside it.
#[test]
fn fast_path_matches_the_oracle_on_every_boundary() {
    // Every `LMNO` either side of both thresholds, plus the rounding midpoint
    // and the endpoints — 41 values, which with the exponents and prefixes
    // below gives well over 10 000 probes.
    let mut boundaries: Vec<u64> = (0..=20).collect();
    boundaries.push(4999);
    boundaries.push(5000);
    boundaries.push(5001);
    boundaries.extend(9980..=9999);
    let mut checked = 0usize;
    for e10 in -40i32..=60 {
        // An 11-digit prefix, so `prefix * 10^4 + lmno` is a 15-digit mantissa.
        for prefix in [10_000_000_000u64, 12_345_678_901, 99_999_999_999] {
            for &lmno in &boundaries {
                // A 15-digit mantissa: an 11-digit prefix followed by LMNO.
                let y = prefix * 10_000 + lmno;
                if !(100_000_000_000_000..=999_999_999_999_999).contains(&y) {
                    continue;
                }
                // The value is `y × 10^(e10 - 14)`.
                let x = y as f64 * 10f64.powi(e10 - 14);
                if !x.is_finite() || x == 0.0 {
                    continue;
                }
                assert_eq!(
                    normalize(x).to_bits(),
                    oracle_normalize(x).to_bits(),
                    "normalize disagreed for y={y} e10={e10} (x={x:.17e})"
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 12_000, "only {checked} values checked");
}

/// Values that round *up* out of 15 digits — the carry that turns
/// `0.999999999999999` into `1`. This is where an early version of the fast
/// path returned ten times the right answer.
#[test]
fn a_carry_out_of_fifteen_digits_is_handled() {
    // (1/3)*3 is exactly 1.0 in binary, but the interpreter reaches the
    // autocorrection through the 15-digit rounding of 0.999999999999999.
    for text in [
        "0.999999999999999",
        "0.9999999999999995",
        "9.99999999999999",
        "99999999999999.9",
        "1e10",
        "0.1",
        "0.5",
    ] {
        let x: f64 = text.parse().unwrap();
        assert_eq!(
            normalize(x).to_bits(),
            oracle_normalize(x).to_bits(),
            "carry/rounding disagreed for {text}"
        );
    }
    // The specific carry: 999999999999999 scaled by 10^-15.
    let x = 999_999_999_999_999f64 * 1e-15;
    assert_eq!(normalize(x), 1.0, "999999999999999e-15 must be 1");
}

/// Every decade of the machine's documented range agrees with the oracle,
/// whatever path each one takes (inside the fast path's window or outside it).
#[test]
fn every_decade_agrees_with_the_oracle() {
    let mut checked = 0usize;
    for e10 in -99i32..=99 {
        for factor in [1.0, 1.5, 9.99] {
            let x = factor * 10f64.powi(e10);
            if !x.is_finite() || x == 0.0 {
                continue;
            }
            assert_eq!(
                normalize(x).to_bits(),
                oracle_normalize(x).to_bits(),
                "normalize disagreed at 1e{e10} x {factor}"
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 199 * 3);
}

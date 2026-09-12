//! Real and complex values.
//!
//! Every expression evaluates to a [`Value`], which is either a real number or
//! a complex number.  Arithmetic helpers apply [`normalize`] to their results
//! so the interpreter automatically inherits the machine's 15-digit
//! auto-correction behaviour.

use std::fmt;

use crate::error::CalcError;
use crate::precision::normalize;

/// How the display renders a complex result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ComplexFormat {
    /// `a+b𝑖`
    #[default]
    Cartesian,
    /// `r∠θ`
    Polar,
}

/// Which part of a complex result the display shows.
///
/// The `Re⇔Im` key switches between the two parts; the interpreter starts in
/// [`ComplexPart::Both`], the machine's ordinary `a+b𝑖` result, and the first
/// press shows only the imaginary part (with the `𝑖` suffix the manual
/// describes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ComplexPart {
    /// Show the whole number (`a+b𝑖`, or `r∠θ` in polar format).
    #[default]
    Both,
    /// Show only the real part.
    Real,
    /// Show only the imaginary part, with the `𝑖` suffix.
    Imaginary,
}

/// A number that may be real or complex.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    Real(f64),
    /// `(real, imaginary)`
    Complex(f64, f64),
    /// A real number entered or converted in degrees/minutes/seconds.  It is
    /// numerically equal to [`Value::Real`] (`deg + min/60 + sec/3600`) and
    /// behaves like one in arithmetic; only the display differs, rendering it
    /// as `d°m′s″`.
    Sexagesimal(f64),
}

impl Value {
    // The plan fixes these helper names; they intentionally mirror the
    // arithmetic operators without implementing `std::ops` traits.
    #![allow(clippy::should_implement_trait)]
    /// `r∠θ`, with `theta` given in radians.
    pub(crate) fn from_polar(r: f64, theta: f64) -> Self {
        Value::Complex(normalize(r * theta.cos()), normalize(r * theta.sin()))
    }

    pub(crate) fn is_real(&self) -> bool {
        !self.is_complex()
    }

    /// A value is complex when it carries an imaginary component.  A
    /// `Complex` with a zero imaginary part still counts, because the machine
    /// only ever creates one in CMPLX mode.
    pub fn is_complex(&self) -> bool {
        matches!(self, Value::Complex(_, _))
    }

    pub(crate) fn is_zero(&self) -> bool {
        match self {
            Value::Real(x) | Value::Sexagesimal(x) => *x == 0.0,
            Value::Complex(re, im) => *re == 0.0 && *im == 0.0,
        }
    }

    /// Whether this value is a sexagesimal (degrees/minutes/seconds) real.
    pub fn is_sexagesimal(&self) -> bool {
        matches!(self, Value::Sexagesimal(_))
    }

    /// Flip the sexagesimal display flag.  A complex value is unchanged: the
    /// conversion only applies to real numbers.
    pub(crate) fn toggle_sexagesimal(self) -> Value {
        match self {
            Value::Real(x) => Value::Sexagesimal(x),
            Value::Sexagesimal(x) => Value::Real(x),
            Value::Complex(..) => self,
        }
    }

    /// Numeric equality, ignoring whether a real carries the sexagesimal
    /// display flag: `2` and `2°0′0″` are the same number.  A complex value
    /// compares part by part and is never equal to a real one.
    pub(crate) fn equals(self, other: Value) -> bool {
        match (self, other) {
            (Value::Complex(ar, ai), Value::Complex(br, bi)) => ar == br && ai == bi,
            (Value::Complex(..), _) | (_, Value::Complex(..)) => false,
            _ => self.re() == other.re(),
        }
    }

    /// The real part.
    pub fn re(&self) -> f64 {
        match self {
            Value::Real(x) | Value::Sexagesimal(x) => *x,
            Value::Complex(re, _) => *re,
        }
    }

    /// The imaginary part (`0` for a real value).
    pub fn im(&self) -> f64 {
        match self {
            Value::Real(_) | Value::Sexagesimal(_) => 0.0,
            Value::Complex(_, im) => *im,
        }
    }

    pub(crate) fn add(self, other: Value) -> Value {
        match (self, other) {
            (Value::Real(a), Value::Real(b)) => Value::Real(normalize(a + b)),
            (Value::Complex(..), _) | (_, Value::Complex(..)) => Value::Complex(
                normalize(self.re() + other.re()),
                normalize(self.im() + other.im()),
            ),
            _ => {
                let value = normalize(self.re() + other.re());
                if self.is_sexagesimal() || other.is_sexagesimal() {
                    Value::Sexagesimal(value)
                } else {
                    Value::Real(value)
                }
            }
        }
    }

    pub(crate) fn sub(self, other: Value) -> Value {
        match (self, other) {
            (Value::Real(a), Value::Real(b)) => Value::Real(normalize(a - b)),
            (Value::Complex(..), _) | (_, Value::Complex(..)) => Value::Complex(
                normalize(self.re() - other.re()),
                normalize(self.im() - other.im()),
            ),
            _ => {
                let value = normalize(self.re() - other.re());
                if self.is_sexagesimal() || other.is_sexagesimal() {
                    Value::Sexagesimal(value)
                } else {
                    Value::Real(value)
                }
            }
        }
    }

    pub(crate) fn mul(self, other: Value) -> Value {
        match (self, other) {
            (Value::Real(a), Value::Real(b)) => Value::Real(normalize(a * b)),
            (Value::Complex(..), _) | (_, Value::Complex(..)) => {
                let (ar, ai) = (self.re(), self.im());
                let (br, bi) = (other.re(), other.im());
                Value::Complex(normalize(ar * br - ai * bi), normalize(ar * bi + ai * br))
            }
            _ => {
                let value = normalize(self.re() * other.re());
                if self.is_sexagesimal() || other.is_sexagesimal() {
                    Value::Sexagesimal(value)
                } else {
                    Value::Real(value)
                }
            }
        }
    }

    pub(crate) fn div(self, other: Value) -> Result<Value, CalcError> {
        match (self, other) {
            (Value::Real(a), Value::Real(b)) => {
                if b == 0.0 {
                    return Err(CalcError::Math("division by zero".to_string()));
                }
                Ok(Value::Real(normalize(a / b)))
            }
            // A quotient in which either operand is sexagesimal stays
            // sexagesimal (the manual's `s ÷ d` rule).
            (Value::Sexagesimal(a), Value::Real(b) | Value::Sexagesimal(b))
            | (Value::Real(a), Value::Sexagesimal(b)) => {
                if b == 0.0 {
                    return Err(CalcError::Math("division by zero".to_string()));
                }
                Ok(Value::Sexagesimal(normalize(a / b)))
            }
            _ => {
                let (ar, ai) = (self.re(), self.im());
                let (br, bi) = (other.re(), other.im());
                let denom = br * br + bi * bi;
                if denom == 0.0 {
                    return Err(CalcError::Math("division by zero".to_string()));
                }
                Ok(Value::Complex(
                    normalize((ar * br + ai * bi) / denom),
                    normalize((ai * br - ar * bi) / denom),
                ))
            }
        }
    }

    pub(crate) fn neg(self) -> Value {
        match self {
            Value::Real(x) => Value::Real(normalize(-x)),
            Value::Sexagesimal(x) => Value::Sexagesimal(normalize(-x)),
            Value::Complex(re, im) => Value::Complex(normalize(-re), normalize(-im)),
        }
    }

    /// The modulus `|z|`.
    pub(crate) fn abs(self) -> f64 {
        match self {
            Value::Real(x) | Value::Sexagesimal(x) => x.abs(),
            Value::Complex(re, im) => (re * re + im * im).sqrt(),
        }
    }

    /// The argument in radians.
    pub(crate) fn arg(self) -> f64 {
        match self {
            Value::Real(x) | Value::Sexagesimal(x) => {
                if x >= 0.0 {
                    0.0
                } else {
                    std::f64::consts::PI
                }
            }
            Value::Complex(re, im) => {
                if re == 0.0 && im == 0.0 {
                    0.0
                } else {
                    im.atan2(re)
                }
            }
        }
    }

    pub(crate) fn conjg(self) -> Value {
        match self {
            Value::Real(x) => Value::Real(x),
            Value::Sexagesimal(x) => Value::Sexagesimal(x),
            Value::Complex(re, im) => Value::Complex(re, -im),
        }
    }

    pub(crate) fn square(self) -> Value {
        self.mul(self)
    }

    pub(crate) fn cube(self) -> Value {
        self.mul(self).mul(self)
    }

    pub(crate) fn inverse(self) -> Result<Value, CalcError> {
        match self {
            Value::Real(x) => {
                if x == 0.0 {
                    return Err(CalcError::Math("division by zero".to_string()));
                }
                Ok(Value::Real(normalize(1.0 / x)))
            }
            Value::Sexagesimal(x) => {
                if x == 0.0 {
                    return Err(CalcError::Math("division by zero".to_string()));
                }
                Ok(Value::Sexagesimal(normalize(1.0 / x)))
            }
            Value::Complex(re, im) => {
                let denom = re * re + im * im;
                if denom == 0.0 {
                    return Err(CalcError::Math("division by zero".to_string()));
                }
                Ok(Value::Complex(
                    normalize(re / denom),
                    normalize(-im / denom),
                ))
            }
        }
    }

    /// Principal square root.  A negative real yields a purely imaginary
    /// complex result.
    pub(crate) fn sqrt(self) -> Value {
        match self {
            Value::Real(x) | Value::Sexagesimal(x) if x >= 0.0 => Value::Real(normalize(x.sqrt())),
            Value::Real(x) | Value::Sexagesimal(x) => Value::Complex(0.0, normalize((-x).sqrt())),
            Value::Complex(re, im) => {
                let modulus = (re * re + im * im).sqrt();
                let mut real_part = ((modulus + re) / 2.0).max(0.0).sqrt();
                let mut imag_part = ((modulus - re) / 2.0).max(0.0).sqrt();
                if im < 0.0 {
                    imag_part = -imag_part;
                }
                if real_part == 0.0 {
                    real_part = 0.0;
                }
                Value::Complex(normalize(real_part), normalize(imag_part))
            }
        }
    }

    /// Principal natural logarithm `ln z = ln|z| + i·arg(z)` (radians).
    pub(crate) fn ln(self) -> Result<Value, CalcError> {
        if self.is_zero() {
            return Err(CalcError::Math("ln domain".to_string()));
        }
        Ok(Value::Complex(normalize(self.abs().ln()), self.arg()))
    }

    /// `e^z`.
    pub(crate) fn exp(self) -> Value {
        let magnitude = self.re().exp();
        Value::Complex(
            normalize(magnitude * self.im().cos()),
            normalize(magnitude * self.im().sin()),
        )
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Real(x) => write!(f, "{x}"),
            Value::Sexagesimal(x) => write!(f, "{}", format_sexagesimal(*x)),
            Value::Complex(re, im) => {
                if *im == 0.0 {
                    write!(f, "{re}")
                } else if *re == 0.0 {
                    write!(f, "{im}𝑖")
                } else if *im < 0.0 {
                    write!(f, "{re}-{}𝑖", -im)
                } else {
                    write!(f, "{re}+{im}𝑖")
                }
            }
        }
    }
}

/// The imaginary-unit glyph used by the display (`𝑖`, U+1D456).
pub(crate) const IMAGINARY_UNIT: &str = "𝑖";

/// Render a real number as `d°m′s″`.
///
/// `value` is `deg + min/60 + sec/3600`.  Seconds are rounded to nine decimal
/// places, so a value that was entered (or produced by the conversion key) as
/// whole degrees/minutes/seconds prints that way again despite binary floating
/// point, and a rounded `60` carries into the minutes and degrees.
pub(crate) fn format_sexagesimal(value: f64) -> String {
    if value.is_nan() {
        return "Math ERROR".to_string();
    }
    if value.is_infinite() {
        return if value < 0.0 { "-∞" } else { "∞" }.to_string();
    }
    let sign = if value < 0.0 { "-" } else { "" };
    let magnitude = value.abs();
    let degrees = magnitude.trunc();
    let after_degrees = (magnitude - degrees) * 60.0;
    let minutes = after_degrees.trunc();
    let seconds = round_to((after_degrees - minutes) * 60.0, 9);
    let (minutes, seconds) = if seconds >= 60.0 {
        (minutes + 1.0, seconds - 60.0)
    } else {
        (minutes, seconds)
    };
    let (degrees, minutes) = if minutes >= 60.0 {
        (degrees + 1.0, minutes - 60.0)
    } else {
        (degrees, minutes)
    };
    format!(
        "{sign}{degrees}\u{00b0}{minutes}\u{2032}{}\u{2033}",
        trim_number(seconds)
    )
}

/// Round `x` to `places` decimal places.
fn round_to(x: f64, places: u32) -> f64 {
    let factor = 10f64.powi(places as i32);
    (x * factor).round() / factor
}

/// Format a non-negative number without a trailing `.0` or trailing zeros.
fn trim_number(value: f64) -> String {
    let text = format!("{value}");
    match text.split_once('.') {
        Some((whole, fraction)) => {
            let fraction = fraction.trim_end_matches('0');
            if fraction.is_empty() {
                whole.to_string()
            } else {
                format!("{whole}.{fraction}")
            }
        }
        None => text,
    }
}

/// Render `a+b𝑖`, omitting the real part when it is zero.
pub(crate) fn format_cartesian(re: f64, im: f64, fmt: &impl Fn(f64) -> String) -> String {
    if im == 0.0 {
        return fmt(re);
    }
    if re == 0.0 {
        return format!("{}{IMAGINARY_UNIT}", fmt(im));
    }
    if im < 0.0 {
        format!("{}-{}{IMAGINARY_UNIT}", fmt(re), fmt(-im))
    } else {
        format!("{}+{}{IMAGINARY_UNIT}", fmt(re), fmt(im))
    }
}

/// Render `r∠θ`.
pub(crate) fn format_polar(r: f64, theta: f64, fmt: &impl Fn(f64) -> String) -> String {
    format!("{}∠{}", fmt(r), fmt(theta))
}

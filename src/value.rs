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

/// A number that may be real or complex.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    Real(f64),
    /// `(real, imaginary)`
    Complex(f64, f64),
}

impl Value {
    // The plan fixes these helper names; they intentionally mirror the
    // arithmetic operators without implementing `std::ops` traits.
    #![allow(clippy::should_implement_trait)]
    pub const fn real(x: f64) -> Self {
        Value::Real(x)
    }

    pub const fn complex(re: f64, im: f64) -> Self {
        Value::Complex(re, im)
    }

    /// `r∠θ`, with `theta` given in radians.
    pub fn from_polar(r: f64, theta: f64) -> Self {
        Value::Complex(normalize(r * theta.cos()), normalize(r * theta.sin()))
    }

    pub fn is_real(&self) -> bool {
        matches!(self, Value::Real(_))
    }

    pub fn is_zero(&self) -> bool {
        match self {
            Value::Real(x) => *x == 0.0,
            Value::Complex(re, im) => *re == 0.0 && *im == 0.0,
        }
    }

    /// The real part.
    pub fn re(&self) -> f64 {
        match self {
            Value::Real(x) => *x,
            Value::Complex(re, _) => *re,
        }
    }

    /// The imaginary part (`0` for a real value).
    pub fn im(&self) -> f64 {
        match self {
            Value::Real(_) => 0.0,
            Value::Complex(_, im) => *im,
        }
    }

    /// Extract the real part, or fail with a `Math ERROR` if this is complex.
    pub fn as_real(&self) -> Result<f64, CalcError> {
        match self {
            Value::Real(x) => Ok(*x),
            Value::Complex(..) => Err(CalcError::Math(
                "a complex value is not allowed here".to_string(),
            )),
        }
    }

    pub fn add(self, other: Value) -> Value {
        match (self, other) {
            (Value::Real(a), Value::Real(b)) => Value::Real(normalize(a + b)),
            _ => Value::Complex(
                normalize(self.re() + other.re()),
                normalize(self.im() + other.im()),
            ),
        }
    }

    pub fn sub(self, other: Value) -> Value {
        match (self, other) {
            (Value::Real(a), Value::Real(b)) => Value::Real(normalize(a - b)),
            _ => Value::Complex(
                normalize(self.re() - other.re()),
                normalize(self.im() - other.im()),
            ),
        }
    }

    pub fn mul(self, other: Value) -> Value {
        match (self, other) {
            (Value::Real(a), Value::Real(b)) => Value::Real(normalize(a * b)),
            _ => {
                let (ar, ai) = (self.re(), self.im());
                let (br, bi) = (other.re(), other.im());
                Value::Complex(normalize(ar * br - ai * bi), normalize(ar * bi + ai * br))
            }
        }
    }

    pub fn div(self, other: Value) -> Result<Value, CalcError> {
        match (self, other) {
            (Value::Real(a), Value::Real(b)) => {
                if b == 0.0 {
                    return Err(CalcError::Math("division by zero".to_string()));
                }
                Ok(Value::Real(normalize(a / b)))
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

    pub fn neg(self) -> Value {
        match self {
            Value::Real(x) => Value::Real(normalize(-x)),
            Value::Complex(re, im) => Value::Complex(normalize(-re), normalize(-im)),
        }
    }

    /// The modulus `|z|`.
    pub fn abs(self) -> f64 {
        match self {
            Value::Real(x) => x.abs(),
            Value::Complex(re, im) => (re * re + im * im).sqrt(),
        }
    }

    /// The argument in radians.
    pub fn arg(self) -> f64 {
        match self {
            Value::Real(x) => {
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

    pub fn conjg(self) -> Value {
        match self {
            Value::Real(x) => Value::Real(x),
            Value::Complex(re, im) => Value::Complex(re, -im),
        }
    }

    pub fn square(self) -> Value {
        self.mul(self)
    }

    pub fn cube(self) -> Value {
        self.mul(self).mul(self)
    }

    pub fn inverse(self) -> Result<Value, CalcError> {
        match self {
            Value::Real(x) => {
                if x == 0.0 {
                    return Err(CalcError::Math("division by zero".to_string()));
                }
                Ok(Value::Real(normalize(1.0 / x)))
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
    pub fn sqrt(self) -> Value {
        match self {
            Value::Real(x) if x >= 0.0 => Value::Real(normalize(x.sqrt())),
            Value::Real(x) => Value::Complex(0.0, normalize((-x).sqrt())),
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
    pub fn ln(self) -> Result<Value, CalcError> {
        if self.is_zero() {
            return Err(CalcError::Math("ln domain".to_string()));
        }
        Ok(Value::Complex(normalize(self.abs().ln()), self.arg()))
    }

    /// `e^z`.
    pub fn exp(self) -> Value {
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
pub const IMAGINARY_UNIT: &str = "𝑖";

/// Render `a+b𝑖`, omitting the real part when it is zero.
pub fn format_cartesian(re: f64, im: f64, fmt: &impl Fn(f64) -> String) -> String {
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
pub fn format_polar(r: f64, theta: f64, fmt: &impl Fn(f64) -> String) -> String {
    format!("{}∠{}", fmt(r), fmt(theta))
}

//! Statistics registers and the `DT` data-entry model.
//!
//! Data is stored as `(x, y, frequency)` triples so the same structure serves
//! both the single-variable (SD) and two-variable (REG) modes.  All accessors
//! return `f64`; the interpreter is responsible for raising `Math ERROR` when
//! an accessor is used without enough data.

use crate::error::CalcError;
use crate::precision::normalize;

/// The regression model selected in REG mode.
///
/// The manual's *kinds of regression calculation* (回归计算的种类) lists seven
/// models.  Except for [`RegType::Quad`], each is fitted by transforming the
/// data until it is linear in the transform, doing the usual least-squares
/// line fit, and mapping the coefficients back (the same approach the machine
/// uses).  Quad is a true least-squares quadratic fit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RegType {
    /// Linear regression `y = a + b·x`.
    Lin,
    /// Logarithmic regression `y = a + b·ln x`.
    Log,
    /// Exponential regression `y = a·e^(b·x)`.
    Exp,
    /// Power regression `y = a·x^b`.
    Pwr,
    /// Inverse regression `y = a + b/x`.
    Inv,
    /// Quadratic regression `y = a + b·x + c·x²`.
    Quad,
    /// `AB` exponential regression `y = a·b^x`.
    ABExp,
}

impl RegType {
    /// Every regression model, in the machine's menu order.  Used by the
    /// `.fxc` coverage test.
    pub const ALL: [RegType; 7] = [
        RegType::Lin,
        RegType::Log,
        RegType::Exp,
        RegType::Pwr,
        RegType::Inv,
        RegType::Quad,
        RegType::ABExp,
    ];

    /// The menu label (and the PRGM token) for this model.
    pub fn glyph(self) -> &'static str {
        use RegType::*;
        match self {
            Lin => "Lin",
            Log => "Log",
            Exp => "Exp",
            Pwr => "Pwr",
            Inv => "Inv",
            Quad => "Quad",
            ABExp => "AB-Exp",
        }
    }

    /// The ASCII spelling.  The regression menu is already ASCII, so this is
    /// the same as [`RegType::glyph`].
    pub fn ascii(self) -> &'static str {
        self.glyph()
    }

    /// Parse a regression-menu label.
    pub fn parse(name: &str) -> Option<RegType> {
        use RegType::*;
        Some(match name {
            "Lin" => Lin,
            "Log" => Log,
            "Exp" => Exp,
            "Pwr" => Pwr,
            "Inv" => Inv,
            "Quad" => Quad,
            "AB-Exp" | "ABExp" | "abexp" => ABExp,
            _ => return None,
        })
    }

    /// The `.fxc` statement that selects this model.
    pub fn call_name(self) -> &'static str {
        use RegType::*;
        match self {
            Lin => "reg_lin",
            Log => "reg_log",
            Exp => "reg_exp",
            Pwr => "reg_pwr",
            Inv => "reg_inv",
            Quad => "reg_quad",
            ABExp => "reg_abexp",
        }
    }

    /// Whether this model has the third coefficient `c`.
    pub fn has_c(self) -> bool {
        matches!(self, RegType::Quad)
    }
}

/// The statistical variables available as expression tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StatVar {
    N,
    SumX,
    SumX2,
    SumY,
    SumY2,
    SumXY,
    MeanX,
    MeanY,
    SigmaX,
    SigmaY,
    Sx,
    Sy,
    MinX,
    MaxX,
    MinY,
    MaxY,
    RegA,
    RegB,
    RegC,
    RegR,
}

impl StatVar {
    /// Every statistical variable. Used by the `.fxc` coverage test.
    pub const ALL: [StatVar; 20] = [
        StatVar::N,
        StatVar::SumX,
        StatVar::SumX2,
        StatVar::SumY,
        StatVar::SumY2,
        StatVar::SumXY,
        StatVar::MeanX,
        StatVar::MeanY,
        StatVar::SigmaX,
        StatVar::SigmaY,
        StatVar::Sx,
        StatVar::Sy,
        StatVar::MinX,
        StatVar::MaxX,
        StatVar::MinY,
        StatVar::MaxY,
        StatVar::RegA,
        StatVar::RegB,
        StatVar::RegC,
        StatVar::RegR,
    ];
}

/// A fixed-capacity list of data points.
#[derive(Debug, Clone)]
pub struct Stats {
    data: Vec<(f64, f64, f64)>,
    pub freq_on: bool,
    pub reg_type: RegType,
}

/// The machine accepts at most 40 SD/REG data points.
pub const MAX_DATA: usize = 40;

impl Default for Stats {
    fn default() -> Self {
        Stats {
            data: Vec::new(),
            freq_on: false,
            reg_type: RegType::Lin,
        }
    }
}

impl Stats {
    pub fn new() -> Self {
        Stats::default()
    }

    /// Append a data point. `freq` is stored regardless of `freq_on`; the
    /// accessors decide whether to honour it.
    pub fn add(&mut self, x: f64, y: f64, freq: f64) -> Result<(), CalcError> {
        if self.data.len() >= MAX_DATA {
            return Err(CalcError::DataFull);
        }
        if !x.is_finite() || !y.is_finite() || !freq.is_finite() {
            return Err(CalcError::Math("invalid statistical data".to_string()));
        }
        if self.freq_on && freq < 1.0 {
            return Err(CalcError::Math(
                "frequency must be a positive integer".to_string(),
            ));
        }
        self.data.push((x, y, freq));
        Ok(())
    }

    pub fn clr(&mut self) {
        self.data.clear();
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    fn weight(&self, freq: f64) -> f64 {
        if self.freq_on { freq } else { 1.0 }
    }

    pub fn n(&self) -> f64 {
        normalize(self.data.iter().map(|d| self.weight(d.2)).sum())
    }

    pub fn sum_x(&self) -> f64 {
        normalize(self.data.iter().map(|d| self.weight(d.2) * d.0).sum())
    }

    pub fn sum_x2(&self) -> f64 {
        normalize(self.data.iter().map(|d| self.weight(d.2) * d.0 * d.0).sum())
    }

    pub fn sum_y(&self) -> f64 {
        normalize(self.data.iter().map(|d| self.weight(d.2) * d.1).sum())
    }

    pub fn sum_y2(&self) -> f64 {
        normalize(self.data.iter().map(|d| self.weight(d.2) * d.1 * d.1).sum())
    }

    pub fn sum_xy(&self) -> f64 {
        normalize(self.data.iter().map(|d| self.weight(d.2) * d.0 * d.1).sum())
    }

    pub fn mean_x(&self) -> f64 {
        let n = self.n();
        if n == 0.0 {
            0.0
        } else {
            normalize(self.sum_x() / n)
        }
    }

    pub fn mean_y(&self) -> f64 {
        let n = self.n();
        if n == 0.0 {
            0.0
        } else {
            normalize(self.sum_y() / n)
        }
    }

    /// Population standard deviation `σx`.
    pub fn sigma_x(&self) -> f64 {
        self.population_sigma(self.sum_x2(), self.mean_x())
    }

    pub fn sigma_y(&self) -> f64 {
        self.population_sigma(self.sum_y2(), self.mean_y())
    }

    fn population_sigma(&self, sum_sq: f64, mean: f64) -> f64 {
        let n = self.n();
        if n == 0.0 {
            return 0.0;
        }
        let variance = (sum_sq / n - mean * mean).max(0.0);
        normalize(variance.sqrt())
    }

    /// Sample standard deviation `sx`.
    pub fn s_x(&self) -> f64 {
        self.sample_sigma(self.sum_x2(), self.mean_x())
    }

    pub fn s_y(&self) -> f64 {
        self.sample_sigma(self.sum_y2(), self.mean_y())
    }

    fn sample_sigma(&self, sum_sq: f64, mean: f64) -> f64 {
        let n = self.n();
        if n < 2.0 {
            return 0.0;
        }
        let variance = ((sum_sq - n * mean * mean) / (n - 1.0)).max(0.0);
        normalize(variance.sqrt())
    }

    pub fn min_x(&self) -> f64 {
        self.minimum(|d| d.0)
    }

    pub fn max_x(&self) -> f64 {
        self.maximum(|d| d.0)
    }

    pub fn min_y(&self) -> f64 {
        self.minimum(|d| d.1)
    }

    pub fn max_y(&self) -> f64 {
        self.maximum(|d| d.1)
    }

    fn minimum(&self, key: impl Fn(&(f64, f64, f64)) -> f64) -> f64 {
        if self.data.is_empty() {
            return 0.0;
        }
        self.data
            .iter()
            .fold(f64::INFINITY, |acc, d| acc.min(key(d)))
    }

    fn maximum(&self, key: impl Fn(&(f64, f64, f64)) -> f64) -> f64 {
        if self.data.is_empty() {
            return 0.0;
        }
        self.data
            .iter()
            .fold(f64::NEG_INFINITY, |acc, d| acc.max(key(d)))
    }

    /// Weighted sums of a transformed data set: `(n, ΣwX, ΣwY, ΣwX², ΣwY²,
    /// ΣwXY)`.
    fn transformed_sums(
        &self,
        tx: impl Fn(f64) -> f64,
        ty: impl Fn(f64) -> f64,
    ) -> (f64, f64, f64, f64, f64, f64) {
        let mut n = 0.0;
        let mut sum_x = 0.0;
        let mut sum_y = 0.0;
        let mut sum_xx = 0.0;
        let mut sum_yy = 0.0;
        let mut sum_xy = 0.0;
        for d in &self.data {
            let w = self.weight(d.2);
            let x = tx(d.0);
            let y = ty(d.1);
            n += w;
            sum_x += w * x;
            sum_y += w * y;
            sum_xx += w * x * x;
            sum_yy += w * y * y;
            sum_xy += w * x * y;
        }
        (n, sum_x, sum_y, sum_xx, sum_yy, sum_xy)
    }

    /// Weighted least-squares fit of `Y = a + b·X`, returning `(a, b, r)`.
    fn linear_fit(&self, tx: impl Fn(f64) -> f64, ty: impl Fn(f64) -> f64) -> (f64, f64, f64) {
        let (n, sum_x, sum_y, sum_xx, sum_yy, sum_xy) = self.transformed_sums(tx, ty);
        let covariance = n * sum_xy - sum_x * sum_y;
        let variance_x = n * sum_xx - sum_x * sum_x;
        let variance_y = n * sum_yy - sum_y * sum_y;
        let b = if variance_x == 0.0 {
            0.0
        } else {
            covariance / variance_x
        };
        let a = if n == 0.0 {
            0.0
        } else {
            (sum_y - b * sum_x) / n
        };
        let denominator = (variance_x * variance_y).sqrt();
        let r = if denominator == 0.0 {
            0.0
        } else {
            covariance / denominator
        };
        (normalize(a), normalize(b), normalize(r))
    }

    /// The fit of the transformed data for every model except Quad.
    fn transformed_fit(&self) -> (f64, f64, f64) {
        use RegType::*;
        match self.reg_type {
            Log => self.linear_fit(|x| x.ln(), |y| y),
            Exp => self.linear_fit(|x| x, |y| y.ln()),
            Pwr => self.linear_fit(|x| x.ln(), |y| y.ln()),
            Inv => self.linear_fit(|x| 1.0 / x, |y| y),
            ABExp => self.linear_fit(|x| x, |y| y.ln()),
            // Lin; Quad has its own fit.
            _ => self.linear_fit(|x| x, |y| y),
        }
    }

    /// The fitted coefficients `(a, b, c)` of the selected model.  `c` is `0`
    /// for every model except [`RegType::Quad`].
    fn coefficients(&self) -> (f64, f64, f64) {
        use RegType::*;
        if self.reg_type == Quad {
            return self.quadratic_coefficients();
        }
        let (a, b, _) = self.transformed_fit();
        match self.reg_type {
            // `ln y = A + b·x`, `ln y = A + b·ln x` and
            // `ln y = A + (ln b)·x` all exponentiate `a`.
            Exp | Pwr | ABExp => {
                let a = normalize(a.exp());
                let b = if self.reg_type == ABExp {
                    normalize(b.exp())
                } else {
                    b
                };
                (a, b, 0.0)
            }
            _ => (a, b, 0.0),
        }
    }

    /// Weighted least-squares quadratic fit `y = a + b·x + c·x²`.
    fn quadratic_coefficients(&self) -> (f64, f64, f64) {
        let mut n = 0.0;
        let mut sum_x = 0.0;
        let mut sum_x2 = 0.0;
        let mut sum_x3 = 0.0;
        let mut sum_x4 = 0.0;
        let mut sum_y = 0.0;
        let mut sum_xy = 0.0;
        let mut sum_x2y = 0.0;
        for d in &self.data {
            let w = self.weight(d.2);
            let x = d.0;
            let y = d.1;
            n += w;
            sum_x += w * x;
            sum_x2 += w * x * x;
            sum_x3 += w * x * x * x;
            sum_x4 += w * x * x * x * x;
            sum_y += w * y;
            sum_xy += w * x * y;
            sum_x2y += w * x * x * y;
        }
        // The normal equations, solved with Cramer's rule.
        let m = [
            [n, sum_x, sum_x2],
            [sum_x, sum_x2, sum_x3],
            [sum_x2, sum_x3, sum_x4],
        ];
        let rhs = [sum_y, sum_xy, sum_x2y];
        let determinant = determinant3(&m);
        if determinant == 0.0 {
            return (0.0, 0.0, 0.0);
        }
        let a = normalize(determinant3(&with_column(&m, 0, rhs)) / determinant);
        let b = normalize(determinant3(&with_column(&m, 1, rhs)) / determinant);
        let c = normalize(determinant3(&with_column(&m, 2, rhs)) / determinant);
        (a, b, c)
    }

    /// Intercept `a` of the selected regression model.
    pub fn reg_a(&self) -> f64 {
        self.coefficients().0
    }

    /// Slope (or exponent) `b` of the selected regression model.
    pub fn reg_b(&self) -> f64 {
        self.coefficients().1
    }

    /// Quadratic coefficient `c`; `0` for every model except
    /// [`RegType::Quad`].
    pub fn reg_c(&self) -> f64 {
        self.coefficients().2
    }

    /// Correlation coefficient `r`.  For [`RegType::Quad`] this is the multiple
    /// correlation coefficient.
    pub fn reg_r(&self) -> f64 {
        if self.reg_type == RegType::Quad {
            return self.quadratic_r();
        }
        self.transformed_fit().2
    }

    /// Multiple correlation coefficient of the quadratic fit.
    fn quadratic_r(&self) -> f64 {
        let (a, b, c) = self.quadratic_coefficients();
        let mean_y = self.mean_y();
        let mut residual = 0.0;
        let mut total = 0.0;
        for d in &self.data {
            let w = self.weight(d.2);
            let x = d.0;
            let y = d.1;
            let predicted = a + b * x + c * x * x;
            residual += w * (y - predicted) * (y - predicted);
            total += w * (y - mean_y) * (y - mean_y);
        }
        if total == 0.0 {
            return 0.0;
        }
        normalize((1.0 - residual / total).max(0.0).sqrt())
    }

    /// Estimated `y` for a given `x` under the selected model.
    pub fn est_y(&self, x: f64) -> f64 {
        use RegType::*;
        let (a, b, c) = self.coefficients();
        normalize(match self.reg_type {
            Lin => a + b * x,
            Log => a + b * x.ln(),
            Exp => a * (b * x).exp(),
            Pwr => a * x.powf(b),
            Inv => a + b / x,
            Quad => a + b * x + c * x * x,
            ABExp => a * b.powf(x),
        })
    }

    /// Estimated `x` for a given `y` under the selected model.
    ///
    /// Quadratic regression is not one-to-one, so the root nearer the mean `x`
    /// is returned; a negative discriminant gives `NaN`, which the interpreter
    /// reports as `Math ERROR`.
    pub fn est_x(&self, y: f64) -> f64 {
        use RegType::*;
        let (a, b, c) = self.coefficients();
        let value = match self.reg_type {
            Lin => {
                if b == 0.0 {
                    0.0
                } else {
                    (y - a) / b
                }
            }
            Log => {
                if b == 0.0 {
                    0.0
                } else {
                    ((y - a) / b).exp()
                }
            }
            Exp => {
                if a == 0.0 || b == 0.0 {
                    0.0
                } else {
                    (y / a).ln() / b
                }
            }
            Pwr => {
                if a == 0.0 || b == 0.0 {
                    0.0
                } else {
                    (y / a).powf(1.0 / b)
                }
            }
            Inv => {
                if y == a {
                    0.0
                } else {
                    b / (y - a)
                }
            }
            Quad => {
                if c == 0.0 {
                    if b == 0.0 { 0.0 } else { (y - a) / b }
                } else {
                    let discriminant = b * b - 4.0 * c * (a - y);
                    if discriminant < 0.0 {
                        f64::NAN
                    } else {
                        let root = discriminant.sqrt();
                        let first = (-b + root) / (2.0 * c);
                        let second = (-b - root) / (2.0 * c);
                        let mean = self.mean_x();
                        if (first - mean).abs() <= (second - mean).abs() {
                            first
                        } else {
                            second
                        }
                    }
                }
            }
            ABExp => {
                if a == 0.0 || b <= 0.0 || b == 1.0 {
                    0.0
                } else {
                    (y / a).ln() / b.ln()
                }
            }
        };
        normalize(value)
    }

    /// Evaluate a statistical variable.
    pub fn value(&self, var: StatVar) -> f64 {
        match var {
            StatVar::N => self.n(),
            StatVar::SumX => self.sum_x(),
            StatVar::SumX2 => self.sum_x2(),
            StatVar::SumY => self.sum_y(),
            StatVar::SumY2 => self.sum_y2(),
            StatVar::SumXY => self.sum_xy(),
            StatVar::MeanX => self.mean_x(),
            StatVar::MeanY => self.mean_y(),
            StatVar::SigmaX => self.sigma_x(),
            StatVar::SigmaY => self.sigma_y(),
            StatVar::Sx => self.s_x(),
            StatVar::Sy => self.s_y(),
            StatVar::MinX => self.min_x(),
            StatVar::MaxX => self.max_x(),
            StatVar::MinY => self.min_y(),
            StatVar::MaxY => self.max_y(),
            StatVar::RegA => self.reg_a(),
            StatVar::RegB => self.reg_b(),
            StatVar::RegC => self.reg_c(),
            StatVar::RegR => self.reg_r(),
        }
    }
}

/// Determinant of a 3×3 matrix given row-major.
fn determinant3(m: &[[f64; 3]; 3]) -> f64 {
    m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
}

/// Replace one column of a 3×3 matrix; Cramer's rule's numerator.
fn with_column(m: &[[f64; 3]; 3], column: usize, values: [f64; 3]) -> [[f64; 3]; 3] {
    let mut result = *m;
    for (row, value) in values.into_iter().enumerate() {
        result[row][column] = value;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sums_and_means() {
        let mut stats = Stats::new();
        stats.add(1.0, 0.0, 1.0).unwrap();
        stats.add(2.0, 0.0, 1.0).unwrap();
        stats.add(3.0, 0.0, 1.0).unwrap();
        assert_eq!(stats.n(), 3.0);
        assert_eq!(stats.sum_x(), 6.0);
        assert_eq!(stats.sum_x2(), 14.0);
        assert_eq!(stats.mean_x(), 2.0);
    }

    #[test]
    fn frequency_multiplies() {
        let mut stats = Stats::new();
        stats.freq_on = true;
        stats.add(1.0, 0.0, 2.0).unwrap();
        stats.add(2.0, 0.0, 3.0).unwrap();
        assert_eq!(stats.n(), 5.0);
        assert_eq!(stats.sum_x(), 8.0);
    }

    #[test]
    fn linear_regression() {
        let mut stats = Stats::new();
        // y = 2x + 1
        for x in 0..5 {
            stats.add(x as f64, 2.0 * x as f64 + 1.0, 1.0).unwrap();
        }
        assert!((stats.reg_a() - 1.0).abs() < 1e-12);
        assert!((stats.reg_b() - 2.0).abs() < 1e-12);
        assert!((stats.reg_r() - 1.0).abs() < 1e-12);
        assert!((stats.est_y(3.0) - 7.0).abs() < 1e-12);
    }

    /// A helper that fits `(x, y)` pairs in the given model.
    fn fit(reg_type: RegType, points: &[(f64, f64)]) -> Stats {
        let mut stats = Stats::new();
        stats.reg_type = reg_type;
        for &(x, y) in points {
            stats.add(x, y, 1.0).unwrap();
        }
        stats
    }

    /// Every transform fits an exact line, so `r` is `1` and the coefficients
    /// are the ones the data was generated from.
    #[test]
    fn logarithmic_regression() {
        // y = 3 + 2·ln x
        let stats = fit(
            RegType::Log,
            &[
                (1.0, 3.0),
                (2.0, 3.0 + 2.0 * 2.0_f64.ln()),
                (4.0, 3.0 + 2.0 * 4.0_f64.ln()),
            ],
        );
        assert!((stats.reg_a() - 3.0).abs() < 1e-12);
        assert!((stats.reg_b() - 2.0).abs() < 1e-12);
        assert!((stats.reg_r() - 1.0).abs() < 1e-12);
        assert!((stats.est_y(1.0) - 3.0).abs() < 1e-12);
        assert!((stats.est_x(3.0) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn exponential_regression() {
        // y = 2·e^(0.5x)
        let stats = fit(
            RegType::Exp,
            &[
                (0.0, 2.0),
                (1.0, 2.0 * 0.5_f64.exp()),
                (2.0, 2.0 * 1.0_f64.exp()),
            ],
        );
        assert!((stats.reg_a() - 2.0).abs() < 1e-12);
        assert!((stats.reg_b() - 0.5).abs() < 1e-12);
        assert!((stats.reg_r() - 1.0).abs() < 1e-12);
        assert!((stats.est_y(0.0) - 2.0).abs() < 1e-12);
        assert!((stats.est_x(2.0) - 0.0).abs() < 1e-12);
    }

    #[test]
    fn power_regression() {
        // y = 3·x²
        let stats = fit(RegType::Pwr, &[(1.0, 3.0), (2.0, 12.0), (3.0, 27.0)]);
        assert!((stats.reg_a() - 3.0).abs() < 1e-12);
        assert!((stats.reg_b() - 2.0).abs() < 1e-12);
        assert!((stats.reg_r() - 1.0).abs() < 1e-12);
        assert!((stats.est_y(1.0) - 3.0).abs() < 1e-12);
        assert!((stats.est_x(3.0) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn inverse_regression() {
        // y = 1 + 2/x
        let stats = fit(RegType::Inv, &[(1.0, 3.0), (2.0, 2.0), (4.0, 1.5)]);
        assert!((stats.reg_a() - 1.0).abs() < 1e-12);
        assert!((stats.reg_b() - 2.0).abs() < 1e-12);
        assert!((stats.reg_r() - 1.0).abs() < 1e-12);
        assert!((stats.est_y(1.0) - 3.0).abs() < 1e-12);
        assert!((stats.est_x(3.0) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn quadratic_regression() {
        // y = 1 + 2x + 3x²
        let stats = fit(
            RegType::Quad,
            &[(0.0, 1.0), (1.0, 6.0), (2.0, 17.0), (3.0, 34.0)],
        );
        assert!((stats.reg_a() - 1.0).abs() < 1e-12);
        assert!((stats.reg_b() - 2.0).abs() < 1e-12);
        assert!((stats.reg_c() - 3.0).abs() < 1e-12);
        assert!((stats.reg_r() - 1.0).abs() < 1e-12);
        assert!((stats.est_y(4.0) - 57.0).abs() < 1e-12);
        assert!((stats.est_x(57.0) - 4.0).abs() < 1e-12);
    }

    #[test]
    fn ab_exponential_regression() {
        // y = 2·3^x
        let stats = fit(RegType::ABExp, &[(0.0, 2.0), (1.0, 6.0), (2.0, 18.0)]);
        assert!((stats.reg_a() - 2.0).abs() < 1e-12);
        assert!((stats.reg_b() - 3.0).abs() < 1e-12);
        assert!((stats.reg_r() - 1.0).abs() < 1e-12);
        assert!((stats.est_y(0.0) - 2.0).abs() < 1e-12);
        assert!((stats.est_x(2.0) - 0.0).abs() < 1e-12);
    }
}

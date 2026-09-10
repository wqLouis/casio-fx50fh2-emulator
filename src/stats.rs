//! Statistics registers and the `DT` data-entry model.
//!
//! Data is stored as `(x, y, frequency)` triples so the same structure serves
//! both the single-variable (SD) and two-variable (REG) modes.  All accessors
//! return `f64`; the interpreter is responsible for raising `Math ERROR` when
//! an accessor is used without enough data.

use crate::error::CalcError;
use crate::precision::normalize;

/// The regression model selected in REG mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegType {
    /// Linear regression `y = a + b·x`.  The other Casio models are not
    /// implemented.
    Lin,
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
    RegR,
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

    fn covariance(&self) -> f64 {
        let n = self.n();
        if n == 0.0 {
            return 0.0;
        }
        normalize(n * self.sum_xy() - self.sum_x() * self.sum_y())
    }

    fn variance_x(&self) -> f64 {
        let n = self.n();
        normalize(n * self.sum_x2() - self.sum_x() * self.sum_x())
    }

    fn variance_y(&self) -> f64 {
        let n = self.n();
        normalize(n * self.sum_y2() - self.sum_y() * self.sum_y())
    }

    /// Linear-regression intercept `a`.
    pub fn reg_a(&self) -> f64 {
        let n = self.n();
        let denom = self.variance_x();
        if n == 0.0 || denom == 0.0 {
            return 0.0;
        }
        normalize((self.sum_y() - self.reg_b() * self.sum_x()) / n)
    }

    /// Linear-regression slope `b`.
    pub fn reg_b(&self) -> f64 {
        let denom = self.variance_x();
        if denom == 0.0 {
            return 0.0;
        }
        normalize(self.covariance() / denom)
    }

    /// Correlation coefficient `r`.
    pub fn reg_r(&self) -> f64 {
        let denom = (self.variance_x() * self.variance_y()).sqrt();
        if denom == 0.0 {
            return 0.0;
        }
        normalize(self.covariance() / denom)
    }

    /// Estimated `y` for a given `x`.
    pub fn est_y(&self, x: f64) -> f64 {
        normalize(self.reg_a() + self.reg_b() * x)
    }

    /// Estimated `x` for a given `y`.
    pub fn est_x(&self, y: f64) -> f64 {
        let b = self.reg_b();
        if b == 0.0 {
            return 0.0;
        }
        normalize((y - self.reg_a()) / b)
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
            StatVar::RegR => self.reg_r(),
        }
    }
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
}

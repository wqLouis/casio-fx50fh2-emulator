//! Number formatting that approximates the fx-50FH II two-line display.
//!
//! The real machine carries 15 significant digits internally and shows at most
//! 10 on the lower line.  `Norm1` switches to scientific notation outside
//! `[1e-2, 1e10)`, `Norm2` outside `[1e-9, 1e10)`; `Fix` and `Sci` are
//! explicit.

use crate::runtime::DisplayMode;

pub fn format_number(value: f64, mode: DisplayMode) -> String {
    if value.is_nan() {
        return "Math ERROR".to_string();
    }
    if value.is_infinite() {
        return if value < 0.0 { "-∞" } else { "∞" }.to_string();
    }
    if value == 0.0 {
        return "0".to_string();
    }
    match mode {
        DisplayMode::Fix(d) => fixed(value, d as usize),
        DisplayMode::Sci(d) => scientific(value, d as usize),
        DisplayMode::Norm(n) => {
            let lower = if n >= 2 { 1e-9 } else { 1e-2 };
            let abs = value.abs();
            if abs >= 1e10 || abs < lower {
                scientific(value, 9)
            } else {
                decimal_auto(value)
            }
        }
    }
}

fn decimal_auto(value: f64) -> String {
    let exp = value.abs().log10().floor() as i32;
    let decimals = (9 - exp).max(0) as usize;
    let text = format!("{:.*}", decimals, value);
    trim(text)
}

fn fixed(value: f64, decimals: usize) -> String {
    if value.abs() >= 1e10 {
        return scientific(value, decimals);
    }
    trim(format!("{:.*}", decimals, value))
}

fn scientific(value: f64, decimals: usize) -> String {
    let text = format!("{:.*e}", decimals, value);
    // Rust renders `1.23e5`; make the mantissa tidy and keep the exponent.
    let (mantissa, exp) = text.split_once('e').unwrap_or((text.as_str(), "0"));
    let mantissa = trim(mantissa.to_string());
    let exp: i32 = exp.parse().unwrap_or(0);
    format!("{mantissa}e{exp}")
}

/// Drop trailing zeros (and a trailing decimal point) from a decimal string.
fn trim(mut text: String) -> String {
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    if text == "-0" { "0".to_string() } else { text }
}

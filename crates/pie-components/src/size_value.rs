//! Canonical overlay size value: an absolute number or a percentage string.

use std::error::Error;
use std::fmt::{Display, Formatter};
use std::str::FromStr;

/// Rust representation of reference `number | `${number}%``.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SizeValue {
    Absolute(f64),
    Percent(f64),
}

impl SizeValue {
    /// Resolve using the reference rule. Absolute values remain fractional;
    /// percentages are floored after multiplying by the reference size.
    pub fn resolve(self, reference_size: usize) -> f64 {
        match self {
            Self::Absolute(value) => value,
            Self::Percent(percent) => ((reference_size as f64 * percent) / 100.0).floor(),
        }
    }
}

impl From<f64> for SizeValue {
    fn from(value: f64) -> Self {
        Self::Absolute(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseSizeValueError;

impl Display for ParseSizeValueError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("expected an unsigned number followed by %")
    }
}

impl Error for ParseSizeValueError {}

impl FromStr for SizeValue {
    type Err = ParseSizeValueError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let number = value.strip_suffix('%').ok_or(ParseSizeValueError)?;
        if number.is_empty()
            || number.starts_with('.')
            || number.ends_with('.')
            || number.chars().filter(|character| *character == '.').count() > 1
            || !number
                .chars()
                .all(|character| character.is_ascii_digit() || character == '.')
        {
            return Err(ParseSizeValueError);
        }
        let percent = number.parse::<f64>().map_err(|_| ParseSizeValueError)?;
        Ok(Self::Percent(percent))
    }
}

#[cfg(test)]
mod tests {
    use super::SizeValue;

    #[test]
    fn percentage_parser_and_resolution_match_reference_rule() {
        assert_eq!("33.3%".parse::<SizeValue>(), Ok(SizeValue::Percent(33.3)));
        assert_eq!(SizeValue::Percent(33.3).resolve(10), 3.0);
        assert_eq!(SizeValue::Absolute(3.75).resolve(10), 3.75);
        for invalid in ["", "%", ".5%", "5.%", "-1%", "+1%", "1e2%", "1%%"] {
            assert!(invalid.parse::<SizeValue>().is_err(), "{invalid:?}");
        }
    }
}

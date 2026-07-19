//! Positive decimal quantities with fixed three-place precision.

use std::fmt;

use thiserror::Error;

/// Positive decimal stored exactly as thousandths.
#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
pub struct Quantity(u64);

impl Quantity {
    /// Parse a positive base-10 decimal with at most three fractional digits.
    pub fn parse(value: &str) -> Result<Self, QuantityError> {
        if value.is_empty() || value.trim() != value || value.starts_with('+') {
            return Err(QuantityError::InvalidFormat);
        }
        let (whole, fraction) = value.split_once('.').map_or((value, ""), |parts| parts);
        if whole.is_empty()
            || !whole.bytes().all(|byte| byte.is_ascii_digit())
            || fraction.len() > 3
            || (value.contains('.') && fraction.is_empty())
            || !fraction.bytes().all(|byte| byte.is_ascii_digit())
            || value.matches('.').count() > 1
        {
            return Err(QuantityError::InvalidFormat);
        }
        let whole = whole
            .parse::<u64>()
            .map_err(|_| QuantityError::OutOfRange)?;
        let fractional = match fraction.len() {
            0 => 0,
            1 => {
                fraction
                    .parse::<u64>()
                    .map_err(|_| QuantityError::InvalidFormat)?
                    * 100
            }
            2 => {
                fraction
                    .parse::<u64>()
                    .map_err(|_| QuantityError::InvalidFormat)?
                    * 10
            }
            3 => fraction
                .parse::<u64>()
                .map_err(|_| QuantityError::InvalidFormat)?,
            _ => return Err(QuantityError::InvalidFormat),
        };
        let thousandths = whole
            .checked_mul(1_000)
            .and_then(|value| value.checked_add(fractional))
            .ok_or(QuantityError::OutOfRange)?;
        if thousandths == 0 {
            return Err(QuantityError::NotPositive);
        }
        Ok(Self(thousandths))
    }

    /// Construct from exact thousandths.
    pub fn from_thousandths(value: u64) -> Result<Self, QuantityError> {
        if value == 0 {
            Err(QuantityError::NotPositive)
        } else {
            Ok(Self(value))
        }
    }

    #[must_use]
    pub const fn thousandths(self) -> u64 {
        self.0
    }
}

impl fmt::Debug for Quantity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("Quantity")
            .field(&self.to_string())
            .finish()
    }
}

impl fmt::Display for Quantity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let whole = self.0 / 1_000;
        let fraction = self.0 % 1_000;
        if fraction == 0 {
            write!(formatter, "{whole}")
        } else {
            let fractional = format!("{fraction:03}");
            write!(formatter, "{whole}.{}", fractional.trim_end_matches('0'))
        }
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum QuantityError {
    #[error("quantity must be a base-10 decimal with at most three fractional digits")]
    InvalidFormat,
    #[error("quantity must be greater than zero")]
    NotPositive,
    #[error("quantity is outside the supported range")]
    OutOfRange,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_quantity_enforces_positive_three_place_decimal() {
        assert_eq!(Quantity::parse("0"), Err(QuantityError::NotPositive));
        assert_eq!(Quantity::parse("0.001").expect("valid").thousandths(), 1);
        assert_eq!(
            Quantity::parse("12.340").expect("valid").to_string(),
            "12.34"
        );
        for invalid in ["-1", "+1", "1.0001", "1e3", " 1", "1."] {
            assert_eq!(Quantity::parse(invalid), Err(QuantityError::InvalidFormat));
        }
    }
}

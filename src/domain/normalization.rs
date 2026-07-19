//! Validated vehicle lookup keys.

use std::fmt;

use thiserror::Error;

/// Canonical 17-character Vehicle Identification Number.
#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NormalizedVin(String);

impl NormalizedVin {
    /// Trim and uppercase a VIN, rejecting ambiguous or invalid VIN characters.
    pub fn parse(value: &str) -> Result<Self, NormalizationError> {
        let value = value.trim().to_ascii_uppercase();
        let valid = value.len() == 17
            && value.bytes().all(|byte| {
                byte.is_ascii_digit()
                    || (byte.is_ascii_uppercase() && !matches!(byte, b'I' | b'O' | b'Q'))
            });
        if valid {
            Ok(Self(value))
        } else {
            Err(NormalizationError::InvalidVin)
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for NormalizedVin {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("NormalizedVin([REDACTED])")
    }
}

/// Canonical registration lookup value.
#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NormalizedRegistration(String);

impl NormalizedRegistration {
    /// Remove common visual separators and uppercase a registration lookup value.
    pub fn parse(value: &str) -> Result<Self, NormalizationError> {
        let normalized = value
            .chars()
            .filter(|character| !character.is_ascii_whitespace() && !matches!(character, '-' | '.'))
            .flat_map(char::to_uppercase)
            .collect::<String>();
        let valid = (2..=16).contains(&normalized.len())
            && normalized.bytes().all(|byte| byte.is_ascii_alphanumeric());
        if valid {
            Ok(Self(normalized))
        } else {
            Err(NormalizationError::InvalidRegistration)
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for NormalizedRegistration {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("NormalizedRegistration([REDACTED])")
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum NormalizationError {
    #[error("VIN must contain 17 valid characters")]
    InvalidVin,
    #[error("registration lookup value has an invalid format")]
    InvalidRegistration,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_vehicle_lookup_values_are_normalized_and_redacted() {
        let vin = NormalizedVin::parse(" wvwzzz1jzxw000001 ").expect("valid VIN");
        assert_eq!(vin.as_str(), "WVWZZZ1JZXW000001");
        assert!(!format!("{vin:?}").contains(vin.as_str()));
        assert!(NormalizedVin::parse("WVWZZZ1JZXW00000I").is_err());

        let registration =
            NormalizedRegistration::parse(" 1-abc-234 ").expect("valid registration");
        assert_eq!(registration.as_str(), "1ABC234");
        assert!(!format!("{registration:?}").contains(registration.as_str()));
    }
}

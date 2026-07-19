//! Shared validation failures with stable field paths and machine-readable codes.

use std::fmt;

/// A dotted path identifying one invalid input field.
#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FieldPath(String);

impl FieldPath {
    /// Construct a non-empty dotted path made from identifier-like segments.
    ///
    /// # Errors
    ///
    /// Rejects empty paths and segments containing characters other than ASCII letters, digits,
    /// `_`, or `-`.
    pub fn parse(value: impl Into<String>) -> Result<Self, ValidationError> {
        let value = value.into();
        let valid = !value.is_empty()
            && value.split('.').all(|segment| {
                !segment.is_empty()
                    && segment
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
            });
        if valid {
            Ok(Self(value))
        } else {
            Err(ValidationError::new_unchecked(
                "field",
                ValidationCode::InvalidFormat,
                "field path has an invalid format",
            ))
        }
    }

    /// Stable dotted representation.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for FieldPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("FieldPath").field(&self.0).finish()
    }
}

/// Stable validation error codes used by services and API clients.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ValidationCode {
    Required,
    InvalidFormat,
    OutOfRange,
    TooLong,
    CurrencyMismatch,
}

impl ValidationCode {
    /// Stable snake-case wire value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Required => "required",
            Self::InvalidFormat => "invalid_format",
            Self::OutOfRange => "out_of_range",
            Self::TooLong => "too_long",
            Self::CurrencyMismatch => "currency_mismatch",
        }
    }
}

/// One presentation-safe domain validation failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidationError {
    field: FieldPath,
    code: ValidationCode,
    message: String,
}

impl ValidationError {
    /// Construct a validation failure.
    ///
    /// # Errors
    ///
    /// Rejects an invalid field path or an empty client-safe message.
    pub fn new(
        field: impl Into<String>,
        code: ValidationCode,
        message: impl Into<String>,
    ) -> Result<Self, Self> {
        let field = FieldPath::parse(field).map_err(|error| error)?;
        let message = message.into();
        if message.trim().is_empty() {
            return Err(Self::new_unchecked(
                "field",
                ValidationCode::Required,
                "validation message is required",
            ));
        }
        Ok(Self {
            field,
            code,
            message,
        })
    }

    pub(crate) fn new_unchecked(
        field: &str,
        code: ValidationCode,
        message: impl Into<String>,
    ) -> Self {
        Self {
            field: FieldPath(field.to_owned()),
            code,
            message: message.into(),
        }
    }

    #[must_use]
    pub fn field(&self) -> &FieldPath {
        &self.field
    }

    #[must_use]
    pub const fn code(&self) -> ValidationCode {
        self.code
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// Non-empty collection of field validation failures.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidationErrors(Vec<ValidationError>);

impl ValidationErrors {
    #[must_use]
    pub fn one(error: ValidationError) -> Self {
        Self(vec![error])
    }

    /// Construct from at least one error.
    #[must_use]
    pub fn from_vec(errors: Vec<ValidationError>) -> Option<Self> {
        (!errors.is_empty()).then_some(Self(errors))
    }

    #[must_use]
    pub fn as_slice(&self) -> &[ValidationError] {
        &self.0
    }
}

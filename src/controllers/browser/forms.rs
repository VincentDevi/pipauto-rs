//! Typed safe-value and field-error projections for server-rendered forms.

use std::collections::BTreeMap;

use axum::extract::DefaultBodyLimit;
use serde::Serialize;

use crate::{
    auth::csrf::AuthenticatedCsrfForm,
    domain::{FieldPath, ValidationCode, ValidationError, ValidationErrors},
};

/// Conservative default for URL-encoded authenticated browser forms.
pub const DEFAULT_FORM_BODY_LIMIT: usize = 16 * 1_024;

/// Authenticated URL-encoded form with session-bound CSRF validation.
pub type AuthenticatedForm<T> = AuthenticatedCsrfForm<T>;

/// Route layer required on each unsafe browser form handler.
pub fn body_limit() -> DefaultBodyLimit {
    DefaultBodyLimit::max(DEFAULT_FORM_BODY_LIMIT)
}

/// Parse a non-negative decimal currency input into minor units.
pub(crate) fn parse_minor_units(value: &str) -> Result<i64, ()> {
    if value.is_empty() || value.trim() != value || value.starts_with('+') {
        return Err(());
    }
    let (whole, fraction) = value.split_once('.').map_or((value, ""), |parts| parts);
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.len() > 2
        || (value.contains('.') && fraction.is_empty())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        || value.matches('.').count() > 1
    {
        return Err(());
    }
    let whole = whole.parse::<i64>().map_err(|_| ())?;
    let fraction = match fraction.len() {
        0 => 0,
        1 => fraction.parse::<i64>().map_err(|_| ())? * 10,
        2 => fraction.parse::<i64>().map_err(|_| ())?,
        _ => return Err(()),
    };
    whole
        .checked_mul(100)
        .and_then(|minor| minor.checked_add(fraction))
        .ok_or(())
}

/// Preserve non-empty submitted text while treating whitespace-only input as absent.
pub(crate) fn optional_text(value: &str) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.to_owned())
}

/// Construct a validation error for an invalid browser input format.
pub(crate) fn invalid_format_error(field: &str, message: &str) -> ValidationError {
    ValidationError::new(field, ValidationCode::InvalidFormat, message)
        .expect("static validation metadata is valid")
}

/// One stable, presentation-safe field failure.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FieldError {
    pub code: &'static str,
    pub message: String,
}

/// Validation failures indexed by their typed service field path.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct FieldErrors(BTreeMap<String, Vec<FieldError>>);

impl FieldErrors {
    /// Project service validation failures without inspecting message strings.
    #[must_use]
    pub fn from_validation(errors: &ValidationErrors) -> Self {
        let mut fields = BTreeMap::<String, Vec<FieldError>>::new();
        for error in errors.as_slice() {
            fields
                .entry(error.field().as_str().to_owned())
                .or_default()
                .push(FieldError {
                    code: error.code().as_str(),
                    message: error.message().to_owned(),
                });
        }
        Self(fields)
    }

    /// Failures for a typed field path.
    #[must_use]
    pub fn for_field(&self, field: &FieldPath) -> &[FieldError] {
        self.0.get(field.as_str()).map_or(&[], Vec::as_slice)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.values().all(Vec::is_empty)
    }
}

/// Safe submitted values and their typed service validation projection.
#[derive(Clone, Debug, Serialize)]
pub struct FormState<T> {
    pub values: T,
    pub errors: FieldErrors,
}

impl<T> FormState<T> {
    #[must_use]
    pub const fn new(values: T) -> Self {
        Self {
            values,
            errors: FieldErrors(BTreeMap::new()),
        }
    }

    /// Preserve safe submitted values while attaching service validation failures.
    #[must_use]
    pub fn with_validation(values: T, errors: &ValidationErrors) -> Self {
        Self {
            values,
            errors: FieldErrors::from_validation(errors),
        }
    }

    /// Ensure templates can read a known set of fields even when they have no failures.
    #[must_use]
    pub fn with_known_fields(mut self, fields: &[&str]) -> Self {
        for field in fields {
            self.errors.0.entry((*field).to_owned()).or_default();
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ValidationCode, ValidationError};

    #[derive(Clone, Debug, Eq, PartialEq, Serialize)]
    struct Values {
        display_name: String,
    }

    #[test]
    fn validation_projection_preserves_values_and_field_identity() {
        let error = ValidationError::new(
            "display_name",
            ValidationCode::Required,
            "Enter a display name.",
        )
        .expect("field error should be valid");
        let state = FormState::with_validation(
            Values {
                display_name: "  Filippo  ".to_owned(),
            },
            &ValidationErrors::one(error),
        );
        let field = FieldPath::parse("display_name").expect("field path should be valid");

        assert_eq!(state.values.display_name, "  Filippo  ");
        assert_eq!(state.errors.for_field(&field)[0].code, "required");
    }

    #[test]
    fn minor_unit_parser_preserves_strict_decimal_behavior() {
        assert_eq!(parse_minor_units("12.3"), Ok(1_230));
        assert_eq!(parse_minor_units("12.30"), Ok(1_230));
        for invalid in ["", " 12", "+12", "12.", "12.345", "-1"] {
            assert_eq!(parse_minor_units(invalid), Err(()));
        }
    }
}

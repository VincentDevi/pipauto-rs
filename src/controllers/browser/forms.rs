//! Typed safe-value and field-error projections for server-rendered forms.

use std::collections::BTreeMap;

use axum::extract::DefaultBodyLimit;
use serde::Serialize;

use crate::{
    auth::csrf::AuthenticatedCsrfForm,
    domain::{FieldPath, ValidationErrors},
};

/// Conservative default for URL-encoded authenticated browser forms.
pub const DEFAULT_FORM_BODY_LIMIT: usize = 16 * 1_024;

/// Authenticated URL-encoded form with session-bound CSRF validation.
pub type AuthenticatedForm<T> = AuthenticatedCsrfForm<T>;

/// Route layer required on each unsafe browser form handler.
pub fn body_limit() -> DefaultBodyLimit {
    DefaultBodyLimit::max(DEFAULT_FORM_BODY_LIMIT)
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
}

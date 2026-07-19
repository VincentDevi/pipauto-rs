//! Stable, persistence-opaque API error envelopes.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::domain::ValidationError;

/// Presentation-safe validation messages grouped by public DTO field path.
pub type FieldErrorsDto = BTreeMap<String, Vec<String>>;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ApiErrorBody {
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub fields: FieldErrorsDto,
    pub correlation_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ErrorEnvelope {
    pub error: ApiErrorBody,
}

impl ErrorEnvelope {
    #[must_use]
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            error: ApiErrorBody {
                code: code.into(),
                message: message.into(),
                fields: BTreeMap::new(),
                correlation_id: None,
            },
        }
    }

    /// Construct an opaque infrastructure failure carrying only a safe correlation identifier.
    #[must_use]
    pub fn correlated(
        code: impl Into<String>,
        message: impl Into<String>,
        correlation_id: impl Into<String>,
    ) -> Self {
        Self {
            error: ApiErrorBody {
                code: code.into(),
                message: message.into(),
                fields: BTreeMap::new(),
                correlation_id: Some(correlation_id.into()),
            },
        }
    }

    #[must_use]
    pub fn validation(errors: &[ValidationError]) -> Self {
        let mut fields = BTreeMap::<String, Vec<String>>::new();
        for error in errors {
            fields
                .entry(error.field().as_str().to_owned())
                .or_default()
                .push(error.message().to_owned());
        }
        Self {
            error: ApiErrorBody {
                code: "validation_failed".to_owned(),
                message: "Check the submitted values.".to_owned(),
                fields,
                correlation_id: None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::{ValidationCode, ValidationError};

    use super::*;

    #[test]
    fn api_foundation_validation_envelope_groups_safe_public_field_paths() {
        let errors = [
            ValidationError::new(
                "email",
                ValidationCode::InvalidFormat,
                "Enter a valid email.",
            )
            .expect("safe validation error"),
            ValidationError::new("email", ValidationCode::Required, "Email is required.")
                .expect("safe validation error"),
        ];
        let envelope = ErrorEnvelope::validation(&errors);

        assert_eq!(
            envelope.error.fields.get("email"),
            Some(&vec![
                "Enter a valid email.".to_owned(),
                "Email is required.".to_owned()
            ])
        );
        assert_eq!(envelope.error.correlation_id, None);
        let serialized = serde_json::to_value(ErrorEnvelope::new("not_found", "Not found."))
            .expect("error envelope should serialize");
        assert_eq!(serialized["error"]["fields"], serde_json::json!({}));
        assert!(serialized["error"]["correlation_id"].is_null());
    }
}

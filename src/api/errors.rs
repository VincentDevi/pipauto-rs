//! Stable API error envelopes.

use serde::{Deserialize, Serialize};

use crate::domain::ValidationError;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FieldErrorDto {
    pub field: String,
    pub code: String,
    pub message: String,
}

impl From<&ValidationError> for FieldErrorDto {
    fn from(value: &ValidationError) -> Self {
        Self {
            field: value.field().as_str().to_owned(),
            code: value.code().as_str().to_owned(),
            message: value.message().to_owned(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ApiErrorBody {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<FieldErrorDto>,
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
                fields: Vec::new(),
            },
        }
    }

    #[must_use]
    pub fn validation(errors: &[ValidationError]) -> Self {
        Self {
            error: ApiErrorBody {
                code: "validation_failed".to_owned(),
                message: "one or more fields are invalid".to_owned(),
                fields: errors.iter().map(FieldErrorDto::from).collect(),
            },
        }
    }
}

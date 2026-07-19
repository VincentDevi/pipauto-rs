//! Transport parsing for shared collection query parameters.

use serde::Deserialize;

use crate::{
    domain::{OpaqueCursor, PageLimit, ValidationCode, ValidationError, ValidationErrors},
    settings::BusinessSettings,
};

/// Optional pagination parameters accepted by collection controllers.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
pub struct PaginationQuery {
    pub limit: Option<u16>,
    pub cursor: Option<String>,
}

/// Syntax-checked pagination values passed from a controller to its service.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedPagination {
    pub limit: PageLimit,
    pub after: Option<OpaqueCursor>,
}

impl PaginationQuery {
    /// Apply the configured default and maximum and parse the opaque cursor shape.
    pub fn resolve(
        self,
        settings: &BusinessSettings,
    ) -> Result<ResolvedPagination, ValidationErrors> {
        let limit = self.limit.map_or_else(
            || Ok(settings.default_collection_limit()),
            |value| {
                PageLimit::new(value).and_then(|limit| {
                    if limit.value() <= settings.maximum_collection_limit().value() {
                        Ok(limit)
                    } else {
                        Err(crate::domain::PaginationError::InvalidLimit)
                    }
                })
            },
        );
        let limit = limit.map_err(|_| {
            ValidationErrors::one(public_validation(
                "limit",
                "Choose a page size within the allowed range.",
            ))
        })?;
        let after = self
            .cursor
            .map(OpaqueCursor::parse)
            .transpose()
            .map_err(|_| {
                ValidationErrors::one(public_validation(
                    "cursor",
                    "Use the cursor returned by the previous page.",
                ))
            })?;
        Ok(ResolvedPagination { limit, after })
    }
}

fn public_validation(field: &str, message: &str) -> ValidationError {
    ValidationError::new(field, ValidationCode::InvalidFormat, message)
        .expect("static API validation metadata is valid")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn settings() -> BusinessSettings {
        serde_json::from_value(json!({
            "default_collection_limit": 25,
            "maximum_collection_limit": 100
        }))
        .expect("settings should validate")
    }

    #[test]
    fn api_foundation_pagination_applies_default_and_configured_maximum() {
        let default = PaginationQuery::default()
            .resolve(&settings())
            .expect("default pagination should resolve");
        assert_eq!(default.limit.value(), 25);

        let error = PaginationQuery {
            limit: Some(101),
            cursor: None,
        }
        .resolve(&settings())
        .expect_err("configured maximum should be enforced");
        assert_eq!(error.as_slice()[0].field().as_str(), "limit");

        let error = PaginationQuery {
            limit: None,
            cursor: Some("raw:record".to_owned()),
        }
        .resolve(&settings())
        .expect_err("malformed cursor should be rejected");
        assert_eq!(error.as_slice()[0].field().as_str(), "cursor");
    }
}

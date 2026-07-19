//! Validated workshop business settings.

use std::fmt;

use loco_rs::config::Config;
use serde::{de::Error as _, Deserialize, Deserializer};
use thiserror::Error;

use crate::domain::{CurrencyCode, PageLimit, MAX_PAGE_LIMIT};

const SETTINGS_KEY: &str = "business";
pub const DEFAULT_COLLECTION_LIMIT: u16 = 25;

/// Business defaults shared by collection workflows.
#[derive(Clone)]
pub struct BusinessSettings {
    default_currency: CurrencyCode,
    default_collection_limit: PageLimit,
    maximum_collection_limit: PageLimit,
}

impl BusinessSettings {
    /// Read and validate only the `settings.business` section.
    pub fn from_config(config: &Config) -> Result<Self, BusinessSettingsError> {
        let settings = config
            .settings
            .as_ref()
            .and_then(|settings| settings.get(SETTINGS_KEY))
            .cloned()
            .ok_or(BusinessSettingsError::MissingSection)?;
        serde_json::from_value(settings).map_err(|_| BusinessSettingsError::InvalidFormat)
    }

    #[must_use]
    pub const fn default_currency(&self) -> CurrencyCode {
        self.default_currency
    }

    #[must_use]
    pub const fn default_collection_limit(&self) -> PageLimit {
        self.default_collection_limit
    }

    #[must_use]
    pub const fn maximum_collection_limit(&self) -> PageLimit {
        self.maximum_collection_limit
    }
}

impl fmt::Debug for BusinessSettings {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BusinessSettings")
            .field("default_currency", &self.default_currency)
            .field("default_collection_limit", &self.default_collection_limit)
            .field("maximum_collection_limit", &self.maximum_collection_limit)
            .finish()
    }
}

impl<'de> Deserialize<'de> for BusinessSettings {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawBusinessSettings::deserialize(deserializer)?;
        Self::try_from(raw).map_err(D::Error::custom)
    }
}

#[derive(Deserialize)]
struct RawBusinessSettings {
    #[serde(default = "default_currency")]
    default_currency: String,
    #[serde(default = "default_limit")]
    default_collection_limit: u16,
    #[serde(default = "maximum_limit")]
    maximum_collection_limit: u16,
}

impl TryFrom<RawBusinessSettings> for BusinessSettings {
    type Error = BusinessSettingsError;

    fn try_from(raw: RawBusinessSettings) -> Result<Self, Self::Error> {
        let default_currency = CurrencyCode::parse(&raw.default_currency)
            .map_err(|_| BusinessSettingsError::InvalidCurrency)?;
        let default_collection_limit = PageLimit::new(raw.default_collection_limit)
            .map_err(|_| BusinessSettingsError::InvalidDefaultLimit)?;
        let maximum_collection_limit = PageLimit::new(raw.maximum_collection_limit)
            .map_err(|_| BusinessSettingsError::InvalidMaximumLimit)?;
        if default_collection_limit.value() > maximum_collection_limit.value() {
            return Err(BusinessSettingsError::DefaultExceedsMaximum);
        }
        Ok(Self {
            default_currency,
            default_collection_limit,
            maximum_collection_limit,
        })
    }
}

fn default_currency() -> String {
    "EUR".to_owned()
}

const fn default_limit() -> u16 {
    DEFAULT_COLLECTION_LIMIT
}

const fn maximum_limit() -> u16 {
    MAX_PAGE_LIMIT
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum BusinessSettingsError {
    #[error("missing required setting section `settings.business`")]
    MissingSection,
    #[error("invalid `settings.business` configuration")]
    InvalidFormat,
    #[error("setting `business.default_currency` must be an uppercase ISO 4217 code")]
    InvalidCurrency,
    #[error("setting `business.default_collection_limit` must be between 1 and 200")]
    InvalidDefaultLimit,
    #[error("setting `business.maximum_collection_limit` must be between 1 and 200")]
    InvalidMaximumLimit,
    #[error("default collection limit cannot exceed maximum collection limit")]
    DefaultExceedsMaximum,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn business_settings_default_to_eur_and_documented_limits() {
        let settings: BusinessSettings = serde_json::from_value(json!({})).expect("defaults work");
        assert_eq!(settings.default_currency().as_str(), "EUR");
        assert_eq!(settings.default_collection_limit().value(), 25);
        assert_eq!(settings.maximum_collection_limit().value(), 200);
    }

    #[test]
    fn business_settings_reject_values_without_echoing_them() {
        let error = serde_json::from_value::<BusinessSettings>(json!({
            "default_currency": "secret-invalid-value"
        }))
        .expect_err("invalid currency is rejected");
        assert!(!error.to_string().contains("secret-invalid-value"));

        for value in [0, 201] {
            let error = serde_json::from_value::<BusinessSettings>(json!({
                "default_collection_limit": value
            }));
            assert!(error.is_err());
        }
    }
}

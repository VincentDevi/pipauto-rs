//! Validated workshop business and attachment settings.

use std::fmt;

use chrono_tz::Tz;
use loco_rs::config::Config;
use serde::{de::Error as _, Deserialize, Deserializer};
use thiserror::Error;

use crate::domain::{CurrencyCode, PageLimit, MAX_PAGE_LIMIT};

const BUSINESS_SETTINGS_KEY: &str = "business";
const ATTACHMENT_SETTINGS_KEY: &str = "attachments";
pub const DEFAULT_COLLECTION_LIMIT: u16 = 25;

/// Fixed initial-release maximum for one attachment (25 MiB).
pub const MAX_ATTACHMENT_FILE_BYTES: usize = 25 * 1_024 * 1_024;
/// Reserved multipart headers and text-field overhead above the file limit.
pub const MULTIPART_OVERHEAD_BYTES: usize = 64 * 1_024;
/// Global request ceiling needed for one maximum-size attachment and its multipart envelope.
pub const MULTIPART_ENVELOPE_BYTES: usize = MAX_ATTACHMENT_FILE_BYTES + MULTIPART_OVERHEAD_BYTES;

/// Validated maximum bytes accepted for one attachment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AttachmentFileLimit(usize);

impl AttachmentFileLimit {
    fn new(bytes: usize) -> Result<Self, AttachmentSettingsError> {
        if bytes == 0 || bytes > MAX_ATTACHMENT_FILE_BYTES {
            return Err(AttachmentSettingsError::InvalidFileLimit);
        }
        Ok(Self(bytes))
    }

    #[must_use]
    pub const fn bytes(self) -> usize {
        self.0
    }
}

/// Attachment transport settings validated before application startup completes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AttachmentSettings {
    maximum_file_bytes: AttachmentFileLimit,
}

impl AttachmentSettings {
    /// Read and validate only the `settings.attachments` section.
    pub fn from_config(config: &Config) -> Result<Self, AttachmentSettingsError> {
        let settings = config
            .settings
            .as_ref()
            .and_then(|settings| settings.get(ATTACHMENT_SETTINGS_KEY))
            .cloned()
            .ok_or(AttachmentSettingsError::MissingSection)?;
        serde_json::from_value(settings).map_err(|_| AttachmentSettingsError::InvalidFormat)
    }

    #[must_use]
    pub const fn maximum_file_bytes(self) -> AttachmentFileLimit {
        self.maximum_file_bytes
    }

    /// Maximum complete request body accepted by future multipart upload routes.
    #[must_use]
    pub const fn multipart_envelope_bytes(self) -> usize {
        MULTIPART_ENVELOPE_BYTES
    }
}

impl<'de> Deserialize<'de> for AttachmentSettings {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawAttachmentSettings::deserialize(deserializer)?;
        let maximum_file_bytes =
            AttachmentFileLimit::new(raw.maximum_file_bytes).map_err(D::Error::custom)?;
        Ok(Self { maximum_file_bytes })
    }
}

#[derive(Deserialize)]
struct RawAttachmentSettings {
    #[serde(default = "default_attachment_file_bytes")]
    maximum_file_bytes: usize,
}

const fn default_attachment_file_bytes() -> usize {
    MAX_ATTACHMENT_FILE_BYTES
}

/// Safe attachment-configuration failures which never include configured data.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum AttachmentSettingsError {
    #[error("missing required setting section `settings.attachments`")]
    MissingSection,
    #[error("invalid `settings.attachments` configuration")]
    InvalidFormat,
    #[error("setting `attachments.maximum_file_bytes` must be between 1 byte and 25 MiB")]
    InvalidFileLimit,
}

/// Business defaults shared by collection workflows.
#[derive(Clone)]
pub struct BusinessSettings {
    default_currency: CurrencyCode,
    default_collection_limit: PageLimit,
    maximum_collection_limit: PageLimit,
    workshop_timezone: Tz,
}

impl BusinessSettings {
    /// Read and validate only the `settings.business` section.
    pub fn from_config(config: &Config) -> Result<Self, BusinessSettingsError> {
        let settings = config
            .settings
            .as_ref()
            .and_then(|settings| settings.get(BUSINESS_SETTINGS_KEY))
            .cloned()
            .ok_or(BusinessSettingsError::MissingSection)?;
        let raw: RawBusinessSettings =
            serde_json::from_value(settings).map_err(|_| BusinessSettingsError::InvalidFormat)?;
        Self::try_from(raw)
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

    #[must_use]
    pub const fn workshop_timezone(&self) -> Tz {
        self.workshop_timezone
    }
}

impl fmt::Debug for BusinessSettings {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BusinessSettings")
            .field("default_currency", &self.default_currency)
            .field("default_collection_limit", &self.default_collection_limit)
            .field("maximum_collection_limit", &self.maximum_collection_limit)
            .field("workshop_timezone", &self.workshop_timezone)
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
    workshop_timezone: Option<String>,
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
        let workshop_timezone = raw
            .workshop_timezone
            .ok_or(BusinessSettingsError::MissingWorkshopTimezone)?
            .parse()
            .map_err(|_| BusinessSettingsError::InvalidWorkshopTimezone)?;
        Ok(Self {
            default_currency,
            default_collection_limit,
            maximum_collection_limit,
            workshop_timezone,
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
    #[error("missing required setting `business.workshop_timezone`")]
    MissingWorkshopTimezone,
    #[error("setting `business.workshop_timezone` must be a valid IANA timezone")]
    InvalidWorkshopTimezone,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn attachment_settings_default_to_the_fixed_25_mib_contract() {
        let settings: AttachmentSettings =
            serde_json::from_value(json!({})).expect("defaults should be valid");

        assert_eq!(settings.maximum_file_bytes().bytes(), 25 * 1_024 * 1_024);
        assert_eq!(
            settings.multipart_envelope_bytes(),
            settings.maximum_file_bytes().bytes() + 64 * 1_024
        );
    }

    #[test]
    fn attachment_settings_accept_smaller_limits_and_reject_zero_or_oversized_values_safely() {
        let settings: AttachmentSettings = serde_json::from_value(json!({
            "maximum_file_bytes": 1_024
        }))
        .expect("a smaller positive limit should be valid");
        assert_eq!(settings.maximum_file_bytes().bytes(), 1_024);

        for invalid in [0, MAX_ATTACHMENT_FILE_BYTES + 1] {
            let error = serde_json::from_value::<AttachmentSettings>(json!({
                "maximum_file_bytes": invalid
            }))
            .expect_err("invalid limits should be rejected");
            assert!(!error.to_string().contains(&invalid.to_string()));
        }
    }

    #[test]
    fn business_settings_default_to_eur_and_documented_limits() {
        let settings: BusinessSettings = serde_json::from_value(json!({
            "workshop_timezone": "Europe/Brussels"
        }))
        .expect("defaults work");
        assert_eq!(settings.default_currency().as_str(), "EUR");
        assert_eq!(settings.default_collection_limit().value(), 25);
        assert_eq!(settings.maximum_collection_limit().value(), 200);
        assert_eq!(settings.workshop_timezone(), chrono_tz::Europe::Brussels);
    }

    #[test]
    fn business_settings_accept_another_valid_iana_workshop_timezone() {
        let settings: BusinessSettings = serde_json::from_value(json!({
            "workshop_timezone": "America/New_York"
        }))
        .expect("a valid IANA timezone should load");

        assert_eq!(settings.workshop_timezone(), chrono_tz::America::New_York);
    }

    #[test]
    fn business_settings_reject_values_without_echoing_them() {
        let error = serde_json::from_value::<BusinessSettings>(json!({
            "default_currency": "secret-invalid-value",
            "workshop_timezone": "Europe/Brussels"
        }))
        .expect_err("invalid currency is rejected");
        assert!(!error.to_string().contains("secret-invalid-value"));

        for value in [0, 201] {
            let error = serde_json::from_value::<BusinessSettings>(json!({
                "default_collection_limit": value,
                "workshop_timezone": "Europe/Brussels"
            }));
            assert!(error.is_err());
        }
    }

    #[test]
    fn business_settings_require_a_valid_iana_workshop_timezone_without_echoing_it() {
        let missing = serde_json::from_value::<BusinessSettings>(json!({}))
            .expect_err("the workshop timezone is required");
        assert!(missing.to_string().contains("business.workshop_timezone"));

        let invalid_value = "secret-invalid-timezone";
        let invalid = serde_json::from_value::<BusinessSettings>(json!({
            "workshop_timezone": invalid_value
        }))
        .expect_err("invalid IANA zones should be rejected");
        assert!(invalid.to_string().contains("business.workshop_timezone"));
        assert!(!invalid.to_string().contains(invalid_value));
    }
}

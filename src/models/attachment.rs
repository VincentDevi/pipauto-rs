//! Honest metadata for a file that has not been stored yet.

use chrono::{DateTime, Utc};

use crate::domain::{AttachmentId, InterventionId, VehicleId};

pub const DISPLAY_NAME_MAX_CHARS: usize = 255;
pub const CAPTION_MAX_CHARS: usize = 1_000;
pub const METADATA_ONLY_STORAGE_STATE: &str = "metadata_only";

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AttachmentOwner {
    Vehicle(VehicleId),
    Intervention(InterventionId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttachmentMediaType {
    Pdf,
    Heic,
    Heif,
    Jpeg,
    Png,
    Webp,
}

impl AttachmentMediaType {
    /// Parse the closed media-type set supported by metadata-only records.
    ///
    /// # Errors
    ///
    /// Rejects types not explicitly supported by the schema.
    pub fn parse(value: &str) -> Result<Self, AttachmentMetadataError> {
        match value {
            "application/pdf" => Ok(Self::Pdf),
            "image/heic" => Ok(Self::Heic),
            "image/heif" => Ok(Self::Heif),
            "image/jpeg" => Ok(Self::Jpeg),
            "image/png" => Ok(Self::Png),
            "image/webp" => Ok(Self::Webp),
            _ => Err(AttachmentMetadataError::UnsupportedMediaType),
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pdf => "application/pdf",
            Self::Heic => "image/heic",
            Self::Heif => "image/heif",
            Self::Jpeg => "image/jpeg",
            Self::Png => "image/png",
            Self::Webp => "image/webp",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewAttachmentMetadata {
    pub owner: AttachmentOwner,
    pub display_name: String,
    pub media_type: AttachmentMediaType,
    pub byte_size: Option<u64>,
    pub caption: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttachmentMetadata {
    pub id: AttachmentId,
    pub owner: AttachmentOwner,
    pub display_name: String,
    pub media_type: AttachmentMediaType,
    pub byte_size: Option<u64>,
    pub caption: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl AttachmentMetadata {
    #[must_use]
    pub const fn storage_state(&self) -> &'static str {
        METADATA_ONLY_STORAGE_STATE
    }
}

impl NewAttachmentMetadata {
    /// Validate metadata without accepting or implying binary content.
    ///
    /// Ownership is exactly one vehicle or intervention by construction. The persisted storage
    /// state is always [`METADATA_ONLY_STORAGE_STATE`] and is deliberately not caller-supplied.
    ///
    /// # Errors
    ///
    /// Rejects blank or oversized display text.
    pub fn new(
        owner: AttachmentOwner,
        display_name: String,
        media_type: AttachmentMediaType,
        byte_size: Option<u64>,
        caption: Option<String>,
    ) -> Result<Self, AttachmentMetadataError> {
        Ok(Self {
            owner,
            display_name: required_text(display_name, DISPLAY_NAME_MAX_CHARS)?,
            media_type,
            byte_size,
            caption: optional_text(caption, CAPTION_MAX_CHARS)?,
        })
    }

    #[must_use]
    pub const fn storage_state(&self) -> &'static str {
        METADATA_ONLY_STORAGE_STATE
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum AttachmentMetadataError {
    #[error("attachment display text is required")]
    Required,
    #[error("attachment display text exceeds its maximum length")]
    TooLong,
    #[error("attachment media type is unsupported")]
    UnsupportedMediaType,
}

fn required_text(value: String, maximum: usize) -> Result<String, AttachmentMetadataError> {
    let value = value.trim().to_owned();
    if value.is_empty() {
        return Err(AttachmentMetadataError::Required);
    }
    if value.chars().count() > maximum {
        return Err(AttachmentMetadataError::TooLong);
    }
    Ok(value)
}

fn optional_text(
    value: Option<String>,
    maximum: usize,
) -> Result<Option<String>, AttachmentMetadataError> {
    value
        .map(|value| {
            let value = value.trim().to_owned();
            if value.is_empty() {
                return Ok(None);
            }
            if value.chars().count() > maximum {
                return Err(AttachmentMetadataError::TooLong);
            }
            Ok(Some(value))
        })
        .transpose()
        .map(Option::flatten)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attachment_metadata_has_one_owner_and_never_claims_storage() {
        let metadata = NewAttachmentMetadata::new(
            AttachmentOwner::Vehicle(VehicleId::parse("golf").expect("valid vehicle id")),
            " Water pump photo.jpg ".into(),
            AttachmentMediaType::Jpeg,
            Some(24_512),
            Some("  Before replacement  ".into()),
        )
        .expect("valid attachment metadata");

        assert_eq!(metadata.display_name, "Water pump photo.jpg");
        assert_eq!(metadata.caption.as_deref(), Some("Before replacement"));
        assert_eq!(metadata.media_type.as_str(), "image/jpeg");
        assert_eq!(metadata.storage_state(), "metadata_only");
        assert!(matches!(metadata.owner, AttachmentOwner::Vehicle(_)));
    }

    #[test]
    fn attachment_metadata_rejects_unsupported_media_and_invalid_text() {
        assert_eq!(
            AttachmentMediaType::parse("application/octet-stream"),
            Err(AttachmentMetadataError::UnsupportedMediaType)
        );
        assert_eq!(
            NewAttachmentMetadata::new(
                AttachmentOwner::Intervention(
                    InterventionId::parse("job-42").expect("valid intervention id")
                ),
                " ".into(),
                AttachmentMediaType::Pdf,
                None,
                None,
            ),
            Err(AttachmentMetadataError::Required)
        );
    }
}

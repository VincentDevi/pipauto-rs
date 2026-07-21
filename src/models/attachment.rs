//! Stored-attachment domain values and closed byte-signature detection.

use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};

use crate::domain::{AttachmentId, InterventionId, TechnicalNoteId, VehicleId};

pub const ATTACHMENT_BUCKET_NAME: &str = "pipauto_attachments";
pub const DISPLAY_NAME_MAX_CHARS: usize = 255;
pub const CAPTION_MAX_CHARS: usize = 1_000;
pub const MAX_ATTACHMENT_BYTES: usize = 25 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AttachmentOwner {
    Vehicle(VehicleId),
    Intervention(InterventionId),
    TechnicalNote(TechnicalNoteId),
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
    /// Parse the closed persisted media-type set.
    ///
    /// # Errors
    ///
    /// Rejects types not explicitly supported by the stored-attachment contract.
    pub fn parse(value: &str) -> Result<Self, AttachmentModelError> {
        match value {
            "application/pdf" => Ok(Self::Pdf),
            "image/heic" => Ok(Self::Heic),
            "image/heif" => Ok(Self::Heif),
            "image/jpeg" => Ok(Self::Jpeg),
            "image/png" => Ok(Self::Png),
            "image/webp" => Ok(Self::Webp),
            _ => Err(AttachmentModelError::UnsupportedMediaType),
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

    /// Detect an approved media type from bytes alone.
    ///
    /// # Errors
    ///
    /// Rejects empty, truncated, unsupported, AVIF, and ambiguous ISO-BMFF content.
    pub fn detect(bytes: &[u8]) -> Result<Self, AttachmentModelError> {
        if bytes.is_empty() {
            return Err(AttachmentModelError::EmptyContent);
        }
        if bytes.len() > MAX_ATTACHMENT_BYTES {
            return Err(AttachmentModelError::ContentTooLarge);
        }
        if bytes.len() >= 8
            && bytes.starts_with(b"%PDF-")
            && bytes[5].is_ascii_digit()
            && bytes[6] == b'.'
            && bytes[7].is_ascii_digit()
        {
            return Ok(Self::Pdf);
        }
        if bytes.len() >= 4
            && bytes[0..3] == [0xff, 0xd8, 0xff]
            && (0xc0..=0xfe).contains(&bytes[3])
        {
            return Ok(Self::Jpeg);
        }
        if bytes.len() >= 33
            && bytes.starts_with(b"\x89PNG\r\n\x1a\n")
            && bytes[8..12] == 13_u32.to_be_bytes()
            && &bytes[12..16] == b"IHDR"
        {
            return Ok(Self::Png);
        }
        if bytes.len() >= 20
            && &bytes[0..4] == b"RIFF"
            && &bytes[8..12] == b"WEBP"
            && matches!(&bytes[12..16], b"VP8 " | b"VP8L" | b"VP8X")
            && usize::try_from(u32::from_le_bytes(
                bytes[4..8]
                    .try_into()
                    .map_err(|_| AttachmentModelError::UnsupportedMediaType)?,
            ))
            .is_ok_and(|declared| {
                declared
                    .checked_add(8)
                    .is_some_and(|size| size <= bytes.len())
            })
        {
            return Ok(Self::Webp);
        }
        detect_iso_bmff(bytes)
    }
}

fn detect_iso_bmff(bytes: &[u8]) -> Result<AttachmentMediaType, AttachmentModelError> {
    if bytes.len() < 16 || &bytes[4..8] != b"ftyp" {
        return Err(AttachmentModelError::UnsupportedMediaType);
    }
    let box_size = usize::try_from(u32::from_be_bytes(
        bytes[0..4]
            .try_into()
            .map_err(|_| AttachmentModelError::UnsupportedMediaType)?,
    ))
    .map_err(|_| AttachmentModelError::UnsupportedMediaType)?;
    if box_size < 16 || box_size > bytes.len() || (box_size - 16) % 4 != 0 {
        return Err(AttachmentModelError::UnsupportedMediaType);
    }

    let brands = std::iter::once(&bytes[8..12]).chain(bytes[16..box_size].chunks_exact(4));
    let mut heic = false;
    let mut heif = false;
    for brand in brands {
        if matches!(brand, b"avif" | b"avis") {
            return Err(AttachmentModelError::UnsupportedMediaType);
        }
        heic |= matches!(brand, b"heic" | b"heix" | b"hevc" | b"hevx");
        heif |= matches!(brand, b"mif1" | b"msf1");
    }
    if heic {
        Ok(AttachmentMediaType::Heic)
    } else if heif {
        Ok(AttachmentMediaType::Heif)
    } else {
        Err(AttachmentModelError::UnsupportedMediaType)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttachmentStorageState {
    Pending,
    Stored,
    Deleting,
}

impl AttachmentStorageState {
    /// Decode a persisted lifecycle state.
    ///
    /// # Errors
    ///
    /// Rejects legacy and unknown states.
    pub fn parse(value: &str) -> Result<Self, AttachmentModelError> {
        match value {
            "pending" => Ok(Self::Pending),
            "stored" => Ok(Self::Stored),
            "deleting" => Ok(Self::Deleting),
            _ => Err(AttachmentModelError::InvalidStorageState),
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Stored => "stored",
            Self::Deleting => "deleting",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttachmentFilePointer {
    key: String,
}

impl AttachmentFilePointer {
    /// Construct a pointer known to belong to the one approved bucket.
    ///
    /// # Errors
    ///
    /// Rejects non-opaque or malformed keys.
    pub fn new(bucket: &str, key: impl Into<String>) -> Result<Self, AttachmentModelError> {
        let key = key.into();
        let key = key.strip_prefix('/').unwrap_or(&key);
        let valid = bucket == ATTACHMENT_BUCKET_NAME
            && key.len() == 48
            && key
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase());
        if !valid {
            return Err(AttachmentModelError::InvalidFilePointer);
        }
        Ok(Self {
            key: key.to_owned(),
        })
    }

    #[must_use]
    pub const fn bucket(&self) -> &'static str {
        ATTACHMENT_BUCKET_NAME
    }

    #[must_use]
    pub fn key(&self) -> &str {
        &self.key
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttachmentDigest(String);

impl AttachmentDigest {
    /// Decode a lowercase SHA-256 value.
    ///
    /// # Errors
    ///
    /// Rejects values outside the exact persisted representation.
    pub fn parse(value: impl Into<String>) -> Result<Self, AttachmentModelError> {
        let value = value.into();
        if value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err(AttachmentModelError::InvalidDigest);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn calculate(bytes: &[u8]) -> Self {
        let digest = Sha256::digest(bytes);
        Self(hex(&digest))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttachmentRecord {
    pub id: AttachmentId,
    pub owner: AttachmentOwner,
    pub display_name: String,
    pub media_type: AttachmentMediaType,
    pub byte_size: Option<u64>,
    pub caption: Option<String>,
    pub digest: Option<AttachmentDigest>,
    pub file: AttachmentFilePointer,
    pub storage_state: AttachmentStorageState,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl AttachmentRecord {
    /// Validate a defensively decoded persistence record.
    ///
    /// # Errors
    ///
    /// Rejects invalid text or storage-field/state combinations.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: AttachmentId,
        owner: AttachmentOwner,
        display_name: String,
        media_type: AttachmentMediaType,
        byte_size: Option<u64>,
        caption: Option<String>,
        digest: Option<AttachmentDigest>,
        file: AttachmentFilePointer,
        storage_state: AttachmentStorageState,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, AttachmentModelError> {
        let display_name = required_text(display_name, DISPLAY_NAME_MAX_CHARS)?;
        let caption = optional_text(caption, CAPTION_MAX_CHARS)?;
        let size_valid = byte_size.is_some_and(|size| {
            size >= 1 && size <= u64::try_from(MAX_ATTACHMENT_BYTES).unwrap_or(u64::MAX)
        });
        let storage_valid = match storage_state {
            AttachmentStorageState::Pending => byte_size.is_none() && digest.is_none(),
            AttachmentStorageState::Stored => size_valid && digest.is_some(),
            AttachmentStorageState::Deleting => {
                (byte_size.is_none() && digest.is_none()) || (size_valid && digest.is_some())
            }
        };
        if !storage_valid {
            return Err(AttachmentModelError::InvalidStorageFields);
        }
        Ok(Self {
            id,
            owner,
            display_name,
            media_type,
            byte_size,
            caption,
            digest,
            file,
            storage_state,
            created_at,
            updated_at,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredAttachment {
    pub id: AttachmentId,
    pub owner: AttachmentOwner,
    pub display_name: String,
    pub media_type: AttachmentMediaType,
    pub byte_size: u64,
    pub caption: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub(crate) digest: AttachmentDigest,
    pub(crate) file: AttachmentFilePointer,
}

impl StoredAttachment {
    #[must_use]
    pub const fn storage_state(&self) -> &'static str {
        "stored"
    }

    #[must_use]
    pub(crate) fn file(&self) -> &AttachmentFilePointer {
        &self.file
    }
}

impl TryFrom<AttachmentRecord> for StoredAttachment {
    type Error = AttachmentModelError;

    fn try_from(value: AttachmentRecord) -> Result<Self, Self::Error> {
        if value.storage_state != AttachmentStorageState::Stored {
            return Err(AttachmentModelError::InvalidStorageState);
        }
        Ok(Self {
            id: value.id,
            owner: value.owner,
            display_name: value.display_name,
            media_type: value.media_type,
            byte_size: value
                .byte_size
                .ok_or(AttachmentModelError::InvalidStorageFields)?,
            caption: value.caption,
            created_at: value.created_at,
            updated_at: value.updated_at,
            digest: value
                .digest
                .ok_or(AttachmentModelError::InvalidStorageFields)?,
            file: value.file,
        })
    }
}

/// Compatibility name used by transports until VIN-67 replaces their metadata-only contract.
pub type AttachmentMetadata = StoredAttachment;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewAttachmentReservation {
    pub id: AttachmentId,
    pub owner: AttachmentOwner,
    pub display_name: String,
    pub media_type: AttachmentMediaType,
    pub caption: Option<String>,
    pub file: AttachmentFilePointer,
}

impl NewAttachmentReservation {
    /// Validate the immutable values reserved before a bucket write.
    ///
    /// # Errors
    ///
    /// Rejects blank or oversized user-facing text.
    pub fn new(
        id: AttachmentId,
        owner: AttachmentOwner,
        display_name: String,
        media_type: AttachmentMediaType,
        caption: Option<String>,
        file: AttachmentFilePointer,
    ) -> Result<Self, AttachmentModelError> {
        Ok(Self {
            id,
            owner,
            display_name: required_text(display_name, DISPLAY_NAME_MAX_CHARS)?,
            media_type,
            caption: optional_text(caption, CAPTION_MAX_CHARS)?,
            file,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdateAttachmentMetadata {
    pub display_name: String,
    pub caption: Option<String>,
}

impl UpdateAttachmentMetadata {
    /// Validate the only mutable attachment fields.
    ///
    /// # Errors
    ///
    /// Rejects blank or oversized values.
    pub fn new(
        display_name: String,
        caption: Option<String>,
    ) -> Result<Self, AttachmentModelError> {
        Ok(Self {
            display_name: required_text(display_name, DISPLAY_NAME_MAX_CHARS)?,
            caption: optional_text(caption, CAPTION_MAX_CHARS)?,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum AttachmentModelError {
    #[error("attachment display text is required")]
    Required,
    #[error("attachment display text exceeds its maximum length")]
    TooLong,
    #[error("attachment content is empty")]
    EmptyContent,
    #[error("attachment content exceeds 25 MiB")]
    ContentTooLarge,
    #[error("attachment media type is unsupported")]
    UnsupportedMediaType,
    #[error("attachment storage state is invalid")]
    InvalidStorageState,
    #[error("attachment storage fields are invalid")]
    InvalidStorageFields,
    #[error("attachment file pointer is invalid")]
    InvalidFilePointer,
    #[error("attachment digest is invalid")]
    InvalidDigest,
}

/// Legacy error name retained until VIN-67 migrates transport imports.
pub type AttachmentMetadataError = AttachmentModelError;

fn required_text(value: String, maximum: usize) -> Result<String, AttachmentModelError> {
    let value = value.trim().to_owned();
    if value.is_empty() {
        return Err(AttachmentModelError::Required);
    }
    if value.chars().count() > maximum {
        return Err(AttachmentModelError::TooLong);
    }
    Ok(value)
}

fn optional_text(
    value: Option<String>,
    maximum: usize,
) -> Result<Option<String>, AttachmentModelError> {
    value
        .map(|value| {
            let value = value.trim().to_owned();
            if value.is_empty() {
                return Ok(None);
            }
            if value.chars().count() > maximum {
                return Err(AttachmentModelError::TooLong);
            }
            Ok(Some(value))
        })
        .transpose()
        .map(Option::flatten)
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        value.push(char::from(HEX[usize::from(byte >> 4)]));
        value.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bmff(major: &[u8; 4], compatible: &[[u8; 4]]) -> Vec<u8> {
        let size = 16 + compatible.len() * 4;
        let mut bytes = Vec::with_capacity(size);
        bytes.extend_from_slice(&u32::try_from(size).expect("small fixture").to_be_bytes());
        bytes.extend_from_slice(b"ftyp");
        bytes.extend_from_slice(major);
        bytes.extend_from_slice(&0_u32.to_be_bytes());
        for brand in compatible {
            bytes.extend_from_slice(brand);
        }
        bytes
    }

    #[test]
    fn attachment_media_detection_accepts_closed_signature_set() {
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.resize(33, 0);
        assert_eq!(
            AttachmentMediaType::detect(b"%PDF-1.7\n"),
            Ok(AttachmentMediaType::Pdf)
        );
        assert_eq!(
            AttachmentMediaType::detect(b"\xff\xd8\xff\xe0JFIF"),
            Ok(AttachmentMediaType::Jpeg)
        );
        assert_eq!(
            AttachmentMediaType::detect(&png),
            Ok(AttachmentMediaType::Png)
        );
        assert_eq!(
            AttachmentMediaType::detect(b"RIFF\x0c\0\0\0WEBPVP8 \0\0\0\0"),
            Ok(AttachmentMediaType::Webp)
        );
        assert_eq!(
            AttachmentMediaType::detect(&bmff(b"heic", &[b"mif1".to_owned()])),
            Ok(AttachmentMediaType::Heic)
        );
        assert_eq!(
            AttachmentMediaType::detect(&bmff(b"mif1", &[])),
            Ok(AttachmentMediaType::Heif)
        );
    }

    #[test]
    fn attachment_media_detection_rejects_spoofed_and_ambiguous_bytes() {
        assert_eq!(
            AttachmentMediaType::detect(b"not a png but named photo.png"),
            Err(AttachmentModelError::UnsupportedMediaType)
        );
        assert_eq!(
            AttachmentMediaType::detect(&[]),
            Err(AttachmentModelError::EmptyContent)
        );
        assert_eq!(
            AttachmentMediaType::detect(&bmff(b"avif", &[b"mif1".to_owned()])),
            Err(AttachmentModelError::UnsupportedMediaType)
        );
        assert_eq!(
            AttachmentMediaType::detect(&bmff(b"isom", &[])),
            Err(AttachmentModelError::UnsupportedMediaType)
        );
        assert_eq!(
            AttachmentMediaType::detect(b"\0\0\0\x10ftypmif1"),
            Err(AttachmentModelError::UnsupportedMediaType)
        );
        assert_eq!(
            AttachmentMediaType::detect(b"\x89PNG\r\n\x1a\n"),
            Err(AttachmentModelError::UnsupportedMediaType)
        );
        assert_eq!(
            AttachmentMediaType::detect(b"RIFF\x04\0\0\0WEBP"),
            Err(AttachmentModelError::UnsupportedMediaType)
        );
    }

    #[test]
    fn attachment_model_derives_size_and_digest_without_caller_metadata() {
        let bytes = b"%PDF-1.7\n";
        let size = u64::try_from(bytes.len()).expect("fixture size");
        let digest = AttachmentDigest::calculate(bytes);
        assert_eq!(size, 9);
        assert_eq!(digest.as_str().len(), 64);
        assert!(AttachmentDigest::parse(digest.as_str()).is_ok());
        assert!(AttachmentDigest::parse("A".repeat(64)).is_err());
    }
}

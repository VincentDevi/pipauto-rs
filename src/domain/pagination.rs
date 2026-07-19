//! Typed collection filters, bounded page requests, and authenticated opaque cursors.

use std::fmt;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use thiserror::Error;

type HmacSha256 = Hmac<Sha256>;

pub const MIN_PAGE_LIMIT: u16 = 1;
pub const MAX_PAGE_LIMIT: u16 = 200;

/// Validated number of records requested from a collection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PageLimit(u16);

impl PageLimit {
    pub fn new(value: u16) -> Result<Self, PaginationError> {
        if (MIN_PAGE_LIMIT..=MAX_PAGE_LIMIT).contains(&value) {
            Ok(Self(value))
        } else {
            Err(PaginationError::InvalidLimit)
        }
    }

    #[must_use]
    pub const fn value(self) -> u16 {
        self.0
    }
}

/// Typed filters provide a stable fingerprint used to bind cursors to one collection query.
pub trait CollectionFilter {
    /// Stable, versioned bytes. Implementations must include every field affecting membership or
    /// ordering and must not include HTTP query-string syntax.
    fn fingerprint_bytes(&self) -> Vec<u8>;
}

/// Typed page input passed from services to repositories.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PageRequest<F> {
    pub filter: F,
    pub limit: PageLimit,
    pub after: Option<OpaqueCursor>,
}

/// Page returned by a repository or service.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<OpaqueCursor>,
}

/// Opaque authenticated cursor safe to pass through an API boundary.
#[derive(Clone, Eq, PartialEq)]
pub struct OpaqueCursor(String);

impl OpaqueCursor {
    pub fn parse(value: impl Into<String>) -> Result<Self, CursorError> {
        let value = value.into();
        if value.is_empty() || value.len() > 1_024 || !value.bytes().all(is_base64url_byte) {
            return Err(CursorError::Malformed);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for OpaqueCursor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OpaqueCursor([REDACTED])")
    }
}

/// Persistence-neutral tuple used for deterministic `(updated_at, id)` cursor ordering.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CursorTuple {
    pub timestamp: DateTime<Utc>,
    pub entity_key: String,
}

/// Codec authenticating cursor tuples and binding them to the exact typed filter.
#[derive(Clone)]
pub struct CursorCodec {
    secret: Vec<u8>,
}

impl CursorCodec {
    /// Construct with at least 32 bytes of application secret material.
    pub fn new(secret: impl AsRef<[u8]>) -> Result<Self, CursorError> {
        let secret = secret.as_ref();
        if secret.len() < 32 {
            return Err(CursorError::WeakSecret);
        }
        Ok(Self {
            secret: secret.to_vec(),
        })
    }

    /// Encode a deterministic ordering tuple for one typed filter.
    pub fn encode<F: CollectionFilter>(
        &self,
        tuple: &CursorTuple,
        filter: &F,
    ) -> Result<OpaqueCursor, CursorError> {
        if tuple.entity_key.is_empty() || tuple.entity_key.len() > 256 {
            return Err(CursorError::Malformed);
        }
        let timestamp = tuple.timestamp.timestamp_micros();
        let key = URL_SAFE_NO_PAD.encode(tuple.entity_key.as_bytes());
        let filter_hash = filter_hash(filter);
        let body = format!("1.{timestamp}.{key}.{filter_hash}");
        let signature = self.sign(body.as_bytes())?;
        OpaqueCursor::parse(URL_SAFE_NO_PAD.encode(format!("{body}.{signature}")))
    }

    /// Authenticate, parse, and confirm that a cursor belongs to the supplied typed filter.
    pub fn decode<F: CollectionFilter>(
        &self,
        cursor: &OpaqueCursor,
        filter: &F,
    ) -> Result<CursorTuple, CursorError> {
        let decoded = URL_SAFE_NO_PAD
            .decode(cursor.as_str())
            .map_err(|_| CursorError::Malformed)?;
        let decoded = std::str::from_utf8(&decoded).map_err(|_| CursorError::Malformed)?;
        let mut fields = decoded.split('.');
        let version = fields.next().ok_or(CursorError::Malformed)?;
        let timestamp = fields.next().ok_or(CursorError::Malformed)?;
        let key = fields.next().ok_or(CursorError::Malformed)?;
        let encoded_filter = fields.next().ok_or(CursorError::Malformed)?;
        let signature = fields.next().ok_or(CursorError::Malformed)?;
        if fields.next().is_some() || version != "1" {
            return Err(CursorError::Malformed);
        }
        let body = format!("{version}.{timestamp}.{key}.{encoded_filter}");
        self.verify(body.as_bytes(), signature)?;
        if encoded_filter != filter_hash(filter) {
            return Err(CursorError::FilterMismatch);
        }
        let timestamp = timestamp
            .parse::<i64>()
            .map_err(|_| CursorError::Malformed)?;
        let timestamp = DateTime::from_timestamp_micros(timestamp).ok_or(CursorError::Malformed)?;
        let key = URL_SAFE_NO_PAD
            .decode(key)
            .map_err(|_| CursorError::Malformed)?;
        let entity_key = String::from_utf8(key).map_err(|_| CursorError::Malformed)?;
        if entity_key.is_empty() || entity_key.len() > 256 {
            return Err(CursorError::Malformed);
        }
        Ok(CursorTuple {
            timestamp,
            entity_key,
        })
    }

    fn sign(&self, message: &[u8]) -> Result<String, CursorError> {
        let mut mac =
            HmacSha256::new_from_slice(&self.secret).map_err(|_| CursorError::WeakSecret)?;
        mac.update(message);
        Ok(URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes()))
    }

    fn verify(&self, message: &[u8], signature: &str) -> Result<(), CursorError> {
        let signature = URL_SAFE_NO_PAD
            .decode(signature)
            .map_err(|_| CursorError::Malformed)?;
        let mut mac =
            HmacSha256::new_from_slice(&self.secret).map_err(|_| CursorError::WeakSecret)?;
        mac.update(message);
        mac.verify_slice(&signature)
            .map_err(|_| CursorError::Tampered)
    }
}

impl fmt::Debug for CursorCodec {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CursorCodec")
            .field("secret", &"[REDACTED]")
            .finish()
    }
}

fn filter_hash<F: CollectionFilter>(filter: &F) -> String {
    let digest = Sha256::digest(filter.fingerprint_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

const fn is_base64url_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum PaginationError {
    #[error("page limit must be between 1 and 200")]
    InvalidLimit,
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum CursorError {
    #[error("cursor is malformed")]
    Malformed,
    #[error("cursor authentication failed")]
    Tampered,
    #[error("cursor does not belong to these filters")]
    FilterMismatch,
    #[error("cursor secret must contain at least 32 bytes")]
    WeakSecret,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct VehicleFilter {
        archived: bool,
        query: &'static str,
    }

    impl CollectionFilter for VehicleFilter {
        fn fingerprint_bytes(&self) -> Vec<u8> {
            format!("vehicle-filter:v1:{}:{}", self.archived, self.query).into_bytes()
        }
    }

    fn codec() -> CursorCodec {
        CursorCodec::new(b"0123456789abcdef0123456789abcdef").expect("strong test secret")
    }

    fn tuple() -> CursorTuple {
        CursorTuple {
            timestamp: DateTime::from_timestamp(1_700_000_000, 123_000_000)
                .expect("valid timestamp"),
            entity_key: "vehicle_123".to_owned(),
        }
    }

    #[test]
    fn pagination_limits_enforce_documented_bounds() {
        assert!(PageLimit::new(MIN_PAGE_LIMIT).is_ok());
        assert!(PageLimit::new(MAX_PAGE_LIMIT).is_ok());
        assert_eq!(PageLimit::new(0), Err(PaginationError::InvalidLimit));
        assert_eq!(PageLimit::new(201), Err(PaginationError::InvalidLimit));
    }

    #[test]
    fn pagination_cursor_round_trips_for_matching_filter() {
        let filter = VehicleFilter {
            archived: false,
            query: "golf",
        };
        let encoded = codec().encode(&tuple(), &filter).expect("cursor encodes");
        assert_eq!(
            codec().decode(&encoded, &filter).expect("cursor decodes"),
            tuple()
        );
        assert!(!format!("{encoded:?}").contains(encoded.as_str()));
    }

    #[test]
    fn pagination_cursor_detects_tampering_malformed_data_and_filter_mismatch() {
        let filter = VehicleFilter {
            archived: false,
            query: "golf",
        };
        let cursor = codec().encode(&tuple(), &filter).expect("cursor encodes");
        let decoded = URL_SAFE_NO_PAD
            .decode(cursor.as_str())
            .expect("generated cursor is valid base64url");
        let mut decoded = String::from_utf8(decoded).expect("generated cursor is UTF-8");
        let last = decoded.pop().expect("cursor payload is non-empty");
        decoded.push(if last == 'A' { 'B' } else { 'A' });
        let tampered = OpaqueCursor::parse(URL_SAFE_NO_PAD.encode(decoded))
            .expect("tampered cursor remains opaque");
        assert_eq!(
            codec().decode(&tampered, &filter),
            Err(CursorError::Tampered)
        );
        assert_eq!(
            OpaqueCursor::parse("not+a+cursor"),
            Err(CursorError::Malformed)
        );
        let other_filter = VehicleFilter {
            archived: true,
            query: "golf",
        };
        assert_eq!(
            codec().decode(&cursor, &other_filter),
            Err(CursorError::FilterMismatch)
        );
    }
}

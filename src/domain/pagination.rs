//! Typed collection filters, bounded page requests, and authenticated opaque cursors.

use std::fmt;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{DateTime, NaiveDate, Utc};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

type HmacSha256 = Hmac<Sha256>;

pub const MIN_PAGE_LIMIT: u16 = 1;
pub const MAX_PAGE_LIMIT: u16 = 200;
const MAX_CURSOR_BYTES: usize = 1_024;
const MAX_SORT_VALUES: usize = 4;

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

/// Typed page input passed from model operations to private persistence.
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
        if value.is_empty()
            || value.len() > MAX_CURSOR_BYTES
            || !value.bytes().all(is_base64url_byte)
        {
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

/// Stable API resource discriminator authenticated inside every cursor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CursorResource(String);

impl CursorResource {
    pub fn parse(value: impl Into<String>) -> Result<Self, CursorError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        {
            return Err(CursorError::InvalidResource);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Persistence-neutral values supported in deterministic collection sort tuples.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum CursorSortValue {
    Timestamp(DateTime<Utc>),
    Date(NaiveDate),
    Integer(i64),
    Text(String),
}

/// Final deterministic sort tuple, ending in a database-independent entity key.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CursorTuple {
    sort_values: Vec<CursorSortValue>,
    entity_key: String,
}

impl CursorTuple {
    pub fn new(
        sort_values: Vec<CursorSortValue>,
        entity_key: impl Into<String>,
    ) -> Result<Self, CursorError> {
        let entity_key = entity_key.into();
        let valid_key = !entity_key.is_empty()
            && entity_key.len() <= 128
            && entity_key
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
        let valid_values = (1..=MAX_SORT_VALUES).contains(&sort_values.len())
            && sort_values.iter().all(valid_sort_value);
        if !valid_key || !valid_values {
            return Err(CursorError::Malformed);
        }
        Ok(Self {
            sort_values,
            entity_key,
        })
    }

    #[must_use]
    pub fn sort_values(&self) -> &[CursorSortValue] {
        &self.sort_values
    }

    #[must_use]
    pub fn entity_key(&self) -> &str {
        &self.entity_key
    }
}

#[derive(Deserialize, Serialize)]
struct CursorPayload {
    version: u8,
    resource: String,
    filter: String,
    sort_values: Vec<CursorSortValue>,
    entity_key: String,
}

/// Codec authenticating cursor tuples and binding them to a resource and exact typed filter.
#[derive(Clone)]
pub struct CursorCodec {
    secret: Vec<u8>,
}

impl CursorCodec {
    /// Construct with at least 32 bytes of purpose-separated application secret material.
    pub fn new(secret: impl AsRef<[u8]>) -> Result<Self, CursorError> {
        let secret = secret.as_ref();
        if secret.len() < 32 {
            return Err(CursorError::WeakSecret);
        }
        Ok(Self {
            secret: secret.to_vec(),
        })
    }

    /// Encode a deterministic ordering tuple for one resource and typed filter.
    pub fn encode<F: CollectionFilter>(
        &self,
        resource: &CursorResource,
        tuple: &CursorTuple,
        filter: &F,
    ) -> Result<OpaqueCursor, CursorError> {
        let payload = CursorPayload {
            version: 1,
            resource: resource.as_str().to_owned(),
            filter: filter_hash(filter),
            sort_values: tuple.sort_values.clone(),
            entity_key: tuple.entity_key.clone(),
        };
        let payload = serde_json::to_vec(&payload).map_err(|_| CursorError::Malformed)?;
        let payload = URL_SAFE_NO_PAD.encode(payload);
        let signature = self.sign(payload.as_bytes())?;
        OpaqueCursor::parse(URL_SAFE_NO_PAD.encode(format!("{payload}.{signature}")))
    }

    /// Authenticate, parse, and confirm cursor resource and filter ownership.
    pub fn decode<F: CollectionFilter>(
        &self,
        cursor: &OpaqueCursor,
        resource: &CursorResource,
        filter: &F,
    ) -> Result<CursorTuple, CursorError> {
        let signed = URL_SAFE_NO_PAD
            .decode(cursor.as_str())
            .map_err(|_| CursorError::Malformed)?;
        let signed = std::str::from_utf8(&signed).map_err(|_| CursorError::Malformed)?;
        let (payload, signature) = signed.split_once('.').ok_or(CursorError::Malformed)?;
        if signature.contains('.') {
            return Err(CursorError::Malformed);
        }
        self.verify(payload.as_bytes(), signature)?;
        let payload = URL_SAFE_NO_PAD
            .decode(payload)
            .map_err(|_| CursorError::Malformed)?;
        let payload: CursorPayload =
            serde_json::from_slice(&payload).map_err(|_| CursorError::Malformed)?;
        if payload.version != 1 {
            return Err(CursorError::UnsupportedVersion);
        }
        if payload.resource != resource.as_str() {
            return Err(CursorError::ResourceMismatch);
        }
        if payload.filter != filter_hash(filter) {
            return Err(CursorError::FilterMismatch);
        }
        CursorTuple::new(payload.sort_values, payload.entity_key)
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

fn valid_sort_value(value: &CursorSortValue) -> bool {
    !matches!(value, CursorSortValue::Text(text) if text.is_empty() || text.len() > 256)
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
    #[error("cursor does not belong to this resource")]
    ResourceMismatch,
    #[error("cursor version is unsupported")]
    UnsupportedVersion,
    #[error("cursor resource kind is invalid")]
    InvalidResource,
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

    fn resource() -> CursorResource {
        CursorResource::parse("vehicles").expect("valid resource")
    }

    fn tuple() -> CursorTuple {
        CursorTuple::new(
            vec![CursorSortValue::Timestamp(
                DateTime::from_timestamp(1_700_000_000, 123_000_000).expect("valid timestamp"),
            )],
            "vehicle_123",
        )
        .expect("valid tuple")
    }

    #[test]
    fn cursor_pagination_limits_enforce_documented_bounds() {
        assert!(PageLimit::new(MIN_PAGE_LIMIT).is_ok());
        assert!(PageLimit::new(MAX_PAGE_LIMIT).is_ok());
        assert_eq!(PageLimit::new(0), Err(PaginationError::InvalidLimit));
        assert_eq!(PageLimit::new(201), Err(PaginationError::InvalidLimit));
    }

    #[test]
    fn cursor_round_trips_resource_filter_and_final_sort_tuple() {
        let filter = VehicleFilter {
            archived: false,
            query: "golf",
        };
        let encoded = codec()
            .encode(&resource(), &tuple(), &filter)
            .expect("cursor encodes");
        assert_eq!(
            codec()
                .decode(&encoded, &resource(), &filter)
                .expect("cursor decodes"),
            tuple()
        );
        assert!(!format!("{encoded:?}").contains(encoded.as_str()));
    }

    #[test]
    fn cursor_detects_tampering_filter_and_resource_reuse() {
        let filter = VehicleFilter {
            archived: false,
            query: "golf",
        };
        let cursor = codec()
            .encode(&resource(), &tuple(), &filter)
            .expect("cursor encodes");
        let signed = URL_SAFE_NO_PAD
            .decode(cursor.as_str())
            .expect("generated cursor is base64url");
        let mut signed = String::from_utf8(signed).expect("generated cursor is UTF-8");
        let last = signed.pop().expect("cursor is non-empty");
        signed.push(if last == 'A' { 'B' } else { 'A' });
        let tampered = OpaqueCursor::parse(URL_SAFE_NO_PAD.encode(signed))
            .expect("tampered cursor remains opaque");
        assert_eq!(
            codec().decode(&tampered, &resource(), &filter),
            Err(CursorError::Tampered)
        );

        let other_filter = VehicleFilter {
            archived: true,
            query: "golf",
        };
        assert_eq!(
            codec().decode(&cursor, &resource(), &other_filter),
            Err(CursorError::FilterMismatch)
        );
        let customers = CursorResource::parse("customers").expect("valid resource");
        assert_eq!(
            codec().decode(&cursor, &customers, &filter),
            Err(CursorError::ResourceMismatch)
        );
    }

    #[test]
    fn cursor_tuple_rejects_serialized_database_record_ids() {
        assert_eq!(
            CursorTuple::new(vec![CursorSortValue::Integer(1)], "vehicle:record"),
            Err(CursorError::Malformed)
        );
    }
}

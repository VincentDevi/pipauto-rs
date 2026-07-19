//! Opaque table-specific entity identifiers.

use std::{fmt, marker::PhantomData};

use thiserror::Error;

const MAX_KEY_BYTES: usize = 64;

/// Invalid opaque entity key.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
#[error("entity identifier has an invalid format")]
pub struct EntityIdError;

/// Opaque entity key parameterized by a private table marker.
#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EntityId<T> {
    key: String,
    marker: PhantomData<fn() -> T>,
}

impl<T> EntityId<T> {
    /// Parse a persistence-independent opaque key.
    ///
    /// # Errors
    ///
    /// Rejects empty, oversized, or non-portable keys.
    pub fn parse(key: impl Into<String>) -> Result<Self, EntityIdError> {
        let key = key.into();
        let valid = !key.is_empty()
            && key.len() <= MAX_KEY_BYTES
            && key
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
        if !valid {
            return Err(EntityIdError);
        }
        Ok(Self {
            key,
            marker: PhantomData,
        })
    }

    /// Persistence-independent key.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.key
    }
}

impl<T> fmt::Debug for EntityId<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("EntityId([REDACTED])")
    }
}

macro_rules! entity_id {
    ($name:ident, $marker:ident) => {
        #[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
        enum $marker {}

        #[doc = concat!("Opaque `", stringify!($name), "` value.")]
        #[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(EntityId<$marker>);

        impl $name {
            /// Parse a persistence-independent key.
            pub fn parse(key: impl Into<String>) -> Result<Self, EntityIdError> {
                EntityId::parse(key).map(Self)
            }

            /// Persistence-independent key.
            #[must_use]
            pub fn as_str(&self) -> &str {
                self.0.as_str()
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter
                    .debug_tuple(stringify!($name))
                    .field(&"[REDACTED]")
                    .finish()
            }
        }
    };
}

entity_id!(CustomerId, Customer);
entity_id!(VehicleId, Vehicle);
entity_id!(InterventionId, Intervention);
entity_id!(InterventionLineId, InterventionLine);
entity_id!(TechnicalNoteId, TechnicalNote);
entity_id!(AttachmentId, Attachment);
entity_id!(InvoiceId, Invoice);
entity_id!(InvoiceLineId, InvoiceLine);
entity_id!(PaymentId, Payment);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_entity_ids_are_table_specific_and_redacted() {
        let customer = CustomerId::parse("01J-workshop_7").expect("valid key");
        assert_eq!(customer.as_str(), "01J-workshop_7");
        assert!(!format!("{customer:?}").contains("01J-workshop_7"));
        assert!(VehicleId::parse("vehicle:one").is_err());
        assert!(CustomerId::parse("").is_err());
    }
}

//! Shared success response shapes used by API controllers.

use serde::{Deserialize, Serialize};

/// Envelope for a single successful resource or command result.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DataEnvelope<T> {
    pub data: T,
}

impl<T> DataEnvelope<T> {
    #[must_use]
    pub const fn new(data: T) -> Self {
        Self { data }
    }
}

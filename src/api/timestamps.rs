//! UTC RFC 3339 timestamp DTO.

use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct TimestampDto(pub String);

impl From<DateTime<Utc>> for TimestampDto {
    fn from(value: DateTime<Utc>) -> Self {
        Self(value.to_rfc3339_opts(SecondsFormat::Micros, true))
    }
}

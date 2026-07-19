//! Exact decimal quantity DTO.

use serde::{Deserialize, Serialize};

use crate::domain::Quantity;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct QuantityDto(pub String);

impl From<Quantity> for QuantityDto {
    fn from(value: Quantity) -> Self {
        Self(value.to_string())
    }
}

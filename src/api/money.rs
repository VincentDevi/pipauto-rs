//! Money response DTO.

use serde::{Deserialize, Serialize};

use crate::domain::Money;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MoneyDto {
    pub minor_units: i64,
    pub currency: String,
}

impl From<Money> for MoneyDto {
    fn from(value: Money) -> Self {
        Self {
            minor_units: value.minor_units(),
            currency: value.currency().as_str().to_owned(),
        }
    }
}

//! Intervention line validation and persisted totals.

use chrono::{DateTime, Utc};

use crate::domain::{
    CurrencyCode, InterventionId, InterventionLineId, Money, MoneyError, Quantity,
};

pub const DESCRIPTION_MAX_CHARS: usize = 500;
pub const UNIT_LABEL_MAX_CHARS: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InterventionLineCategory {
    Labour,
    Part,
    Material,
    Other,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewInterventionLine {
    pub intervention_id: InterventionId,
    pub category: InterventionLineCategory,
    pub description: String,
    pub quantity: Quantity,
    pub unit_label: String,
    pub unit_price: Money,
    pub unit_cost: Option<Money>,
    pub total_price: Money,
    pub total_cost: Option<Money>,
    pub position: u32,
}

impl NewInterventionLine {
    /// Validate a line and calculate its persisted revenue and cost totals.
    ///
    /// # Errors
    ///
    /// Rejects blank or oversized labels, currency mismatches, and overflowing totals.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        intervention_id: InterventionId,
        category: InterventionLineCategory,
        description: String,
        quantity: Quantity,
        unit_label: String,
        unit_price: Money,
        unit_cost: Option<Money>,
        position: u32,
        intervention_currency: CurrencyCode,
    ) -> Result<Self, InterventionLineError> {
        let description = required_text(description, DESCRIPTION_MAX_CHARS)?;
        let unit_label = required_text(unit_label, UNIT_LABEL_MAX_CHARS)?;
        if unit_price.currency() != intervention_currency
            || unit_cost.is_some_and(|money| money.currency() != intervention_currency)
        {
            return Err(InterventionLineError::CurrencyMismatch);
        }
        let total_price = unit_price.checked_mul_quantity(quantity)?;
        let total_cost = unit_cost
            .map(|money| money.checked_mul_quantity(quantity))
            .transpose()?;
        Ok(Self {
            intervention_id,
            category,
            description,
            quantity,
            unit_label,
            unit_price,
            unit_cost,
            total_price,
            total_cost,
            position,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InterventionLine {
    pub id: InterventionLineId,
    pub intervention_id: InterventionId,
    pub category: InterventionLineCategory,
    pub description: String,
    pub quantity: Quantity,
    pub unit_label: String,
    pub unit_price: Money,
    pub unit_cost: Option<Money>,
    pub total_price: Money,
    pub total_cost: Option<Money>,
    pub position: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum InterventionLineError {
    #[error("required intervention line text is blank")]
    Required,
    #[error("intervention line text exceeds its maximum length")]
    TooLong,
    #[error("line currency must match intervention currency")]
    CurrencyMismatch,
    #[error(transparent)]
    Money(#[from] MoneyError),
}

fn required_text(value: String, maximum: usize) -> Result<String, InterventionLineError> {
    let value = value.trim().to_owned();
    if value.is_empty() {
        return Err(InterventionLineError::Required);
    }
    if value.chars().count() > maximum {
        return Err(InterventionLineError::TooLong);
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eur() -> CurrencyCode {
        CurrencyCode::parse("EUR").expect("valid currency")
    }

    #[test]
    fn intervention_line_calculates_persisted_totals_with_shared_rounding() {
        let line = NewInterventionLine::new(
            InterventionId::parse("front-pads").expect("valid intervention id"),
            InterventionLineCategory::Part,
            " Front brake pads ".into(),
            Quantity::parse("1.500").expect("valid quantity"),
            " set ".into(),
            Money::new(101, eur()).expect("valid unit price"),
            Some(Money::new(51, eur()).expect("valid unit cost")),
            0,
            eur(),
        )
        .expect("valid line");

        assert_eq!(line.description, "Front brake pads");
        assert_eq!(line.unit_label, "set");
        assert_eq!(line.total_price.minor_units(), 152);
        assert_eq!(line.total_cost.map(Money::minor_units), Some(77));
    }

    #[test]
    fn intervention_line_rejects_currency_mismatch() {
        let error = NewInterventionLine::new(
            InterventionId::parse("front-pads").expect("valid intervention id"),
            InterventionLineCategory::Part,
            "Front brake pads".into(),
            Quantity::parse("1").expect("valid quantity"),
            "set".into(),
            Money::new(100, CurrencyCode::parse("USD").expect("valid currency"))
                .expect("valid unit price"),
            None,
            0,
            eur(),
        );

        assert_eq!(error, Err(InterventionLineError::CurrencyMismatch));
    }
}

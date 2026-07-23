//! Invoice-line snapshots and persisted totals.

use chrono::{DateTime, Utc};

use crate::domain::{
    CurrencyCode, InterventionLineId, InvoiceId, InvoiceLineId, Money, MoneyError, Quantity,
};

pub const DESCRIPTION_MAX_CHARS: usize = 500;
pub const UNIT_LABEL_MAX_CHARS: usize = 32;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceLine {
    pub invoice_id: InvoiceId,
    pub source_intervention_line_id: Option<InterventionLineId>,
    pub description: String,
    pub quantity: Quantity,
    pub unit_label: String,
    pub unit_price: Money,
    pub line_total: Money,
    pub position: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceLineRecord {
    pub id: InvoiceLineId,
    pub line: InvoiceLine,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl InvoiceLine {
    /// Create an immutable-value snapshot and calculate its persisted total.
    ///
    /// # Errors
    ///
    /// Rejects blank or oversized labels, currency mismatches, and overflowing totals.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        invoice_id: InvoiceId,
        source_intervention_line_id: Option<InterventionLineId>,
        description: String,
        quantity: Quantity,
        unit_label: String,
        unit_price: Money,
        position: u32,
        invoice_currency: CurrencyCode,
    ) -> Result<Self, InvoiceLineError> {
        if unit_price.currency() != invoice_currency {
            return Err(InvoiceLineError::CurrencyMismatch);
        }
        let line_total = unit_price.checked_mul_quantity(quantity)?;
        Ok(Self {
            invoice_id,
            source_intervention_line_id,
            description: required_text(description, DESCRIPTION_MAX_CHARS)?,
            quantity,
            unit_label: required_text(unit_label, UNIT_LABEL_MAX_CHARS)?,
            unit_price,
            line_total,
            position,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum InvoiceLineError {
    #[error("required invoice line text is blank")]
    Required,
    #[error("invoice line text exceeds its maximum length")]
    TooLong,
    #[error("invoice line currency must match invoice currency")]
    CurrencyMismatch,
    #[error(transparent)]
    Money(#[from] MoneyError),
}

fn required_text(value: String, maximum: usize) -> Result<String, InvoiceLineError> {
    let value = value.trim().to_owned();
    if value.is_empty() {
        return Err(InvoiceLineError::Required);
    }
    if value.chars().count() > maximum {
        return Err(InvoiceLineError::TooLong);
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invoice_line_model_snapshots_values_and_calculates_total() {
        let eur = CurrencyCode::parse("EUR").expect("valid currency");
        let line = InvoiceLine::new(
            InvoiceId::parse("draft").expect("valid invoice id"),
            Some(InterventionLineId::parse("labour").expect("valid source id")),
            " Workshop labour ".into(),
            Quantity::parse("1.500").expect("valid quantity"),
            " hour ".into(),
            Money::new(5_001, eur).expect("valid money"),
            0,
            eur,
        )
        .expect("valid line");

        assert_eq!(line.description, "Workshop labour");
        assert_eq!(line.line_total.minor_units(), 7_502);
    }
}

//! Append-only payment validation.

use chrono::{DateTime, Utc};

use crate::{
    domain::{InvoiceId, Money},
    models::{
        auth::UserId,
        invoice::{payment_summary, InvoiceError, InvoiceStatus},
    },
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PaymentMethod {
    Cash,
    BankTransfer,
    Card,
    Other,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Payment {
    pub invoice_id: InvoiceId,
    pub amount: Money,
    pub received_at: DateTime<Utc>,
    pub method: PaymentMethod,
    pub reference: Option<String>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub created_by: UserId,
}

impl Payment {
    /// Validate a new payment against the current invoice balance.
    ///
    /// Persistence must perform this validation and creation atomically.
    ///
    /// # Errors
    ///
    /// Rejects zero, wrong-currency, draft/void, and overpaying payments.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        invoice_id: InvoiceId,
        invoice_status: InvoiceStatus,
        invoice_total: Money,
        existing_payments: &[Money],
        amount: Money,
        received_at: DateTime<Utc>,
        method: PaymentMethod,
        reference: Option<String>,
        notes: Option<String>,
        created_at: DateTime<Utc>,
        created_by: UserId,
    ) -> Result<Self, PaymentError> {
        if invoice_status != InvoiceStatus::Issued {
            return Err(PaymentError::InvoiceNotIssued);
        }
        if amount.minor_units() == 0 {
            return Err(PaymentError::NotPositive);
        }
        if amount.currency() != invoice_total.currency() {
            return Err(PaymentError::CurrencyMismatch);
        }
        let mut candidate = Vec::with_capacity(existing_payments.len() + 1);
        candidate.extend_from_slice(existing_payments);
        candidate.push(amount);
        payment_summary(invoice_total, &candidate)?;
        Ok(Self {
            invoice_id,
            amount,
            received_at,
            method,
            reference: optional_text(reference),
            notes: optional_text(notes),
            created_at,
            created_by,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum PaymentError {
    #[error("payment amount must be positive")]
    NotPositive,
    #[error("payment currency must match invoice currency")]
    CurrencyMismatch,
    #[error("payments require an issued invoice")]
    InvoiceNotIssued,
    #[error(transparent)]
    Invoice(#[from] InvoiceError),
}

fn optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim().to_owned();
        (!value.is_empty()).then_some(value)
    })
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone as _;

    use super::*;
    use crate::domain::CurrencyCode;

    fn at(second: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 19, 10, 0, second)
            .single()
            .expect("valid fixture timestamp")
    }

    fn eur(amount: i64) -> Money {
        Money::new(amount, CurrencyCode::parse("EUR").expect("valid currency"))
            .expect("valid money")
    }

    fn create(amount: Money, existing: &[Money]) -> Result<Payment, PaymentError> {
        Payment::new(
            InvoiceId::parse("issued").expect("valid invoice id"),
            InvoiceStatus::Issued,
            eur(10_000),
            existing,
            amount,
            at(1),
            PaymentMethod::BankTransfer,
            Some(" transfer-42 ".into()),
            None,
            at(2),
            UserId::parse("user:mechanic").expect("valid user id"),
        )
    }

    #[test]
    fn payment_model_accepts_exact_outstanding_amount() {
        let payment = create(eur(6_000), &[eur(4_000)]).expect("exact payment is valid");
        assert_eq!(payment.reference.as_deref(), Some("transfer-42"));
    }

    #[test]
    fn payment_model_rejects_zero_and_overpayment() {
        assert_eq!(create(eur(0), &[]), Err(PaymentError::NotPositive));
        assert_eq!(
            create(eur(6_001), &[eur(4_000)]),
            Err(PaymentError::Invoice(InvoiceError::Overpayment))
        );
    }
}

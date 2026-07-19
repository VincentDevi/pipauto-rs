//! Database-independent invoice lifecycle and payment-state rules.

use chrono::{DateTime, Datelike as _, NaiveDate, Utc};

use crate::domain::{CurrencyCode, CustomerId, InterventionId, Money, MoneyError, VehicleId};

pub const CUSTOMER_DISPLAY_MAX_CHARS: usize = 160;
pub const NOTES_MAX_CHARS: usize = 10_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvoiceStatus {
    Draft,
    Issued,
    Void,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PaymentStatus {
    Unpaid,
    PartiallyPaid,
    Paid,
}

/// Final number produced from the database-owned issue sequence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceNumber(String);

impl InvoiceNumber {
    /// Format `YYYY-NNNNN` from the UTC issue year and a positive sequence value.
    ///
    /// The sequence portion expands beyond five digits rather than wrapping.
    ///
    /// # Errors
    ///
    /// Rejects zero because issue sequence values start at one.
    pub fn from_sequence(issued_at: DateTime<Utc>, sequence: u64) -> Result<Self, InvoiceError> {
        if sequence == 0 {
            return Err(InvoiceError::InvalidIssueNumber);
        }
        Ok(Self(format!("{}-{sequence:05}", issued_at.year())))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BillingAddressSnapshot {
    pub line_1: String,
    pub line_2: Option<String>,
    pub postal_code: String,
    pub city: String,
    pub country_code: String,
}

impl BillingAddressSnapshot {
    /// Validate a billing-address snapshot without linking it to later customer changes.
    ///
    /// # Errors
    ///
    /// Rejects blank required values and malformed country codes.
    pub fn new(
        line_1: String,
        line_2: Option<String>,
        postal_code: String,
        city: String,
        country_code: String,
    ) -> Result<Self, InvoiceError> {
        let country_code = country_code.trim().to_owned();
        if country_code.len() != 2 || !country_code.bytes().all(|byte| byte.is_ascii_uppercase()) {
            return Err(InvoiceError::InvalidCountryCode);
        }
        Ok(Self {
            line_1: required_text(line_1, 160)?,
            line_2: optional_text(line_2, 160)?,
            postal_code: required_text(postal_code, 32)?,
            city: required_text(city, 120)?,
            country_code,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Invoice {
    pub customer_id: CustomerId,
    pub vehicle_id: Option<VehicleId>,
    pub intervention_id: Option<InterventionId>,
    pub status: InvoiceStatus,
    pub currency: CurrencyCode,
    pub number: Option<InvoiceNumber>,
    pub issue_date: Option<NaiveDate>,
    pub due_date: Option<NaiveDate>,
    pub customer_display_snapshot: Option<String>,
    pub billing_address_snapshot: Option<BillingAddressSnapshot>,
    pub notes: Option<String>,
    pub void_reason: Option<String>,
    pub subtotal: Money,
    pub total: Money,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub issued_at: Option<DateTime<Utc>>,
    pub voided_at: Option<DateTime<Utc>>,
}

impl Invoice {
    /// Create an unnumbered draft invoice with zero totals.
    ///
    /// # Errors
    ///
    /// Rejects invalid optional text.
    pub fn new(
        customer_id: CustomerId,
        vehicle_id: Option<VehicleId>,
        intervention_id: Option<InterventionId>,
        currency: CurrencyCode,
        notes: Option<String>,
        now: DateTime<Utc>,
    ) -> Result<Self, InvoiceError> {
        let zero = Money::new(0, currency)?;
        Ok(Self {
            customer_id,
            vehicle_id,
            intervention_id,
            status: InvoiceStatus::Draft,
            currency,
            number: None,
            issue_date: None,
            due_date: None,
            customer_display_snapshot: None,
            billing_address_snapshot: None,
            notes: optional_text(notes, NOTES_MAX_CHARS)?,
            void_reason: None,
            subtotal: zero,
            total: zero,
            created_at: now,
            updated_at: now,
            issued_at: None,
            voided_at: None,
        })
    }

    /// Replace the tax-neutral persisted totals while the invoice is a draft.
    ///
    /// # Errors
    ///
    /// Rejects terminal invoices and currency mismatches.
    pub fn set_subtotal(
        &mut self,
        subtotal: Money,
        now: DateTime<Utc>,
    ) -> Result<(), InvoiceError> {
        if self.status != InvoiceStatus::Draft {
            return Err(InvoiceError::Immutable);
        }
        if subtotal.currency() != self.currency {
            return Err(InvoiceError::CurrencyMismatch);
        }
        self.subtotal = subtotal;
        self.total = subtotal;
        self.updated_at = now;
        Ok(())
    }

    /// Issue a draft using a number already allocated from the database sequence.
    ///
    /// # Errors
    ///
    /// Rejects invalid transitions, a mismatched number year, blank snapshots, or a due date before
    /// the issue date.
    pub fn issue(
        &mut self,
        number: InvoiceNumber,
        issue_date: NaiveDate,
        due_date: Option<NaiveDate>,
        customer_display_snapshot: String,
        billing_address_snapshot: Option<BillingAddressSnapshot>,
        now: DateTime<Utc>,
    ) -> Result<(), InvoiceError> {
        if self.status != InvoiceStatus::Draft {
            return Err(InvoiceError::InvalidTransition);
        }
        if !number.as_str().starts_with(&format!("{}-", now.year())) {
            return Err(InvoiceError::InvalidIssueNumber);
        }
        if due_date.is_some_and(|due_date| due_date < issue_date) {
            return Err(InvoiceError::InvalidDueDate);
        }
        let customer_display_snapshot =
            required_text(customer_display_snapshot, CUSTOMER_DISPLAY_MAX_CHARS)?;
        self.status = InvoiceStatus::Issued;
        self.number = Some(number);
        self.issue_date = Some(issue_date);
        self.due_date = due_date;
        self.customer_display_snapshot = Some(customer_display_snapshot);
        self.billing_address_snapshot = billing_address_snapshot;
        self.issued_at = Some(now);
        self.updated_at = now;
        Ok(())
    }

    /// Void a draft or issued invoice when no payment has been recorded.
    ///
    /// # Errors
    ///
    /// Rejects repeated voiding, blank reasons, currency mismatches, and invoices with payments.
    pub fn void(
        &mut self,
        paid: Money,
        reason: String,
        now: DateTime<Utc>,
    ) -> Result<(), InvoiceError> {
        if self.status == InvoiceStatus::Void {
            return Err(InvoiceError::InvalidTransition);
        }
        if paid.currency() != self.currency {
            return Err(InvoiceError::CurrencyMismatch);
        }
        if paid.minor_units() != 0 {
            return Err(InvoiceError::PaymentsRecorded);
        }
        self.status = InvoiceStatus::Void;
        self.void_reason = Some(required_text(reason, NOTES_MAX_CHARS)?);
        self.voided_at = Some(now);
        self.updated_at = now;
        Ok(())
    }
}

/// Derive payment status and outstanding balance from immutable payment amounts.
///
/// # Errors
///
/// Rejects currency mismatches, overflow, and overpayment.
pub fn payment_summary(
    total: Money,
    payments: &[Money],
) -> Result<(PaymentStatus, Money), InvoiceError> {
    let mut paid = Money::new(0, total.currency())?;
    for payment in payments {
        paid = paid.checked_add(*payment)?;
    }
    let outstanding = total.checked_sub(paid).map_err(|error| match error {
        MoneyError::Negative => InvoiceError::Overpayment,
        other => InvoiceError::Money(other),
    })?;
    let status = if paid.minor_units() == 0 {
        PaymentStatus::Unpaid
    } else if outstanding.minor_units() == 0 {
        PaymentStatus::Paid
    } else {
        PaymentStatus::PartiallyPaid
    };
    Ok((status, outstanding))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum InvoiceError {
    #[error("required invoice text is blank")]
    Required,
    #[error("invoice text exceeds its maximum length")]
    TooLong,
    #[error("invoice state transition is not allowed")]
    InvalidTransition,
    #[error("issued invoice snapshots are immutable")]
    Immutable,
    #[error("invoice issue number is invalid")]
    InvalidIssueNumber,
    #[error("invoice due date cannot precede its issue date")]
    InvalidDueDate,
    #[error("billing country code must be two uppercase ASCII letters")]
    InvalidCountryCode,
    #[error("money currency must match invoice currency")]
    CurrencyMismatch,
    #[error("an invoice with recorded payments cannot be voided")]
    PaymentsRecorded,
    #[error("payment amount exceeds invoice total")]
    Overpayment,
    #[error(transparent)]
    Money(#[from] MoneyError),
}

fn required_text(value: String, maximum: usize) -> Result<String, InvoiceError> {
    let value = value.trim().to_owned();
    if value.is_empty() {
        return Err(InvoiceError::Required);
    }
    if value.chars().count() > maximum {
        return Err(InvoiceError::TooLong);
    }
    Ok(value)
}

fn optional_text(value: Option<String>, maximum: usize) -> Result<Option<String>, InvoiceError> {
    value
        .map(|value| {
            let value = value.trim().to_owned();
            if value.is_empty() {
                return Ok(None);
            }
            required_text(value, maximum).map(Some)
        })
        .transpose()
        .map(Option::flatten)
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone as _;

    use super::*;

    fn at(second: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 19, 10, 0, second)
            .single()
            .expect("valid fixture timestamp")
    }

    fn eur(amount: i64) -> Money {
        Money::new(amount, CurrencyCode::parse("EUR").expect("valid currency"))
            .expect("valid money")
    }

    #[test]
    fn invoice_model_issues_once_with_database_sequence_number() {
        let mut invoice = Invoice::new(
            CustomerId::parse("customer").expect("valid customer id"),
            None,
            None,
            CurrencyCode::parse("EUR").expect("valid currency"),
            None,
            at(0),
        )
        .expect("valid invoice");
        invoice.set_subtotal(eur(12_500), at(1)).expect("draft");
        let number = InvoiceNumber::from_sequence(at(2), 42).expect("valid sequence");
        invoice
            .issue(
                number,
                at(2).date_naive(),
                Some(NaiveDate::from_ymd_opt(2026, 8, 19).expect("valid due date")),
                "Filippo Example".into(),
                None,
                at(2),
            )
            .expect("draft issues");

        assert_eq!(
            invoice.number.as_ref().map(InvoiceNumber::as_str),
            Some("2026-00042")
        );
        assert_eq!(invoice.status, InvoiceStatus::Issued);
        assert_eq!(
            invoice.set_subtotal(eur(1), at(3)),
            Err(InvoiceError::Immutable)
        );
    }

    #[test]
    fn invoice_model_derives_payment_status_and_rejects_overpayment() {
        assert_eq!(
            payment_summary(eur(10_000), &[]),
            Ok((PaymentStatus::Unpaid, eur(10_000)))
        );
        assert_eq!(
            payment_summary(eur(10_000), &[eur(2_500)]),
            Ok((PaymentStatus::PartiallyPaid, eur(7_500)))
        );
        assert_eq!(
            payment_summary(eur(10_000), &[eur(4_000), eur(6_000)]),
            Ok((PaymentStatus::Paid, eur(0)))
        );
        assert_eq!(
            payment_summary(eur(10_000), &[eur(10_001)]),
            Err(InvoiceError::Overpayment)
        );
    }

    #[test]
    fn invoice_model_void_requires_no_recorded_payments() {
        let mut invoice = Invoice::new(
            CustomerId::parse("customer").expect("valid customer id"),
            None,
            None,
            CurrencyCode::parse("EUR").expect("valid currency"),
            None,
            at(0),
        )
        .expect("valid invoice");
        assert_eq!(
            invoice.void(eur(1), "Duplicate".into(), at(1)),
            Err(InvoiceError::PaymentsRecorded)
        );
        invoice
            .void(eur(0), "Duplicate".into(), at(2))
            .expect("unpaid draft can be voided");
        assert_eq!(invoice.status, InvoiceStatus::Void);
    }
}

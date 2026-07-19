//! Database-independent intervention state and service-history validation.

use chrono::{DateTime, NaiveDate, Utc};

use crate::domain::{CurrencyCode, InterventionId, Money, VehicleId};

pub const NARRATIVE_MAX_CHARS: usize = 10_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InterventionStatus {
    Draft,
    Completed,
    Cancelled,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewIntervention {
    pub vehicle_id: VehicleId,
    pub service_date: NaiveDate,
    pub status: InterventionStatus,
    pub mileage: Option<u64>,
    pub customer_reported_problem: Option<String>,
    pub diagnostics: Option<String>,
    pub performed_work: Option<String>,
    pub recommendations: Option<String>,
    pub notes: Option<String>,
    pub currency: CurrencyCode,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub cancelled_at: Option<DateTime<Utc>>,
}

impl NewIntervention {
    /// Create a mutable draft intervention.
    ///
    /// # Errors
    ///
    /// Rejects oversized workshop narrative fields.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        vehicle_id: VehicleId,
        service_date: NaiveDate,
        mileage: Option<u64>,
        customer_reported_problem: Option<String>,
        diagnostics: Option<String>,
        performed_work: Option<String>,
        recommendations: Option<String>,
        notes: Option<String>,
        currency: CurrencyCode,
        now: DateTime<Utc>,
    ) -> Result<Self, InterventionModelError> {
        Ok(Self {
            vehicle_id,
            service_date,
            status: InterventionStatus::Draft,
            mileage,
            customer_reported_problem: optional_text(customer_reported_problem)?,
            diagnostics: optional_text(diagnostics)?,
            performed_work: optional_text(performed_work)?,
            recommendations: optional_text(recommendations)?,
            notes: optional_text(notes)?,
            currency,
            created_at: now,
            updated_at: now,
            completed_at: None,
            cancelled_at: None,
        })
    }

    /// Complete a draft and freeze its ordinary workshop data.
    ///
    /// # Errors
    ///
    /// Completion requires a non-empty performed-work narrative and is only allowed from draft.
    pub fn complete(&mut self, now: DateTime<Utc>) -> Result<(), InterventionModelError> {
        if self.status != InterventionStatus::Draft {
            return Err(InterventionModelError::InvalidTransition);
        }
        if self.performed_work.is_none() {
            return Err(InterventionModelError::PerformedWorkRequired);
        }
        self.status = InterventionStatus::Completed;
        self.completed_at = Some(now);
        self.updated_at = now;
        Ok(())
    }

    /// Cancel a draft while retaining it in service history.
    ///
    /// # Errors
    ///
    /// Cancellation is only allowed from draft.
    pub fn cancel(&mut self, now: DateTime<Utc>) -> Result<(), InterventionModelError> {
        if self.status != InterventionStatus::Draft {
            return Err(InterventionModelError::InvalidTransition);
        }
        self.status = InterventionStatus::Cancelled;
        self.cancelled_at = Some(now);
        self.updated_at = now;
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Intervention {
    pub id: InterventionId,
    pub vehicle_id: VehicleId,
    pub service_date: NaiveDate,
    pub status: InterventionStatus,
    pub mileage: Option<u64>,
    pub customer_reported_problem: Option<String>,
    pub diagnostics: Option<String>,
    pub performed_work: Option<String>,
    pub recommendations: Option<String>,
    pub notes: Option<String>,
    pub currency: CurrencyCode,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub cancelled_at: Option<DateTime<Utc>>,
}

impl Intervention {
    #[must_use]
    pub fn history_entry(&self) -> ServiceHistoryEntry {
        ServiceHistoryEntry {
            id: self.id.clone(),
            service_date: self.service_date,
            created_at: self.created_at,
            status: self.status,
            mileage: self.mileage,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InterventionTotals {
    pub price: Money,
    pub cost: Money,
}

impl InterventionTotals {
    pub fn zero(currency: CurrencyCode) -> Result<Self, InterventionModelError> {
        Ok(Self {
            price: Money::new(0, currency)?,
            cost: Money::new(0, currency)?,
        })
    }

    pub fn checked_add(
        self,
        price: Money,
        cost: Option<Money>,
    ) -> Result<Self, InterventionModelError> {
        Ok(Self {
            price: self.price.checked_add(price)?,
            cost: self
                .cost
                .checked_add(cost.unwrap_or(Money::new(0, self.cost.currency())?))?,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceHistorySummary {
    pub intervention: Intervention,
    pub totals: InterventionTotals,
}

/// Chronology values needed to validate a proposed intervention mileage.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceHistoryEntry {
    pub id: InterventionId,
    pub service_date: NaiveDate,
    pub created_at: DateTime<Utc>,
    pub status: InterventionStatus,
    pub mileage: Option<u64>,
}

impl ServiceHistoryEntry {
    fn chronology_key(&self) -> (NaiveDate, DateTime<Utc>, &str) {
        (self.service_date, self.created_at, self.id.as_str())
    }
}

/// Validate a candidate against its non-cancelled mileage-bearing chronological neighbours.
///
/// The caller supplies the candidate's stable creation time and identifier so equal service dates
/// follow the same deterministic order as service-history queries.
///
/// # Errors
///
/// Rejects mileage lower than the previous neighbour or higher than the next neighbour.
pub fn validate_service_history_mileage(
    candidate: &ServiceHistoryEntry,
    history: &[ServiceHistoryEntry],
) -> Result<(), InterventionModelError> {
    if candidate.status == InterventionStatus::Cancelled || candidate.mileage.is_none() {
        return Ok(());
    }
    let candidate_key = candidate.chronology_key();
    let mut previous: Option<&ServiceHistoryEntry> = None;
    let mut next: Option<&ServiceHistoryEntry> = None;

    for entry in history.iter().filter(|entry| {
        entry.id != candidate.id
            && entry.status != InterventionStatus::Cancelled
            && entry.mileage.is_some()
    }) {
        let entry_key = entry.chronology_key();
        if entry_key < candidate_key
            && previous.is_none_or(|current| current.chronology_key() < entry_key)
        {
            previous = Some(entry);
        } else if entry_key > candidate_key
            && next.is_none_or(|current| entry_key < current.chronology_key())
        {
            next = Some(entry);
        }
    }

    let mileage = candidate
        .mileage
        .expect("candidate mileage was checked above");
    if previous
        .and_then(|entry| entry.mileage)
        .is_some_and(|previous| mileage < previous)
        || next
            .and_then(|entry| entry.mileage)
            .is_some_and(|next| mileage > next)
    {
        return Err(InterventionModelError::MileageRegression);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum InterventionModelError {
    #[error("intervention narrative exceeds its maximum length")]
    NarrativeTooLong,
    #[error("performed work is required before completion")]
    PerformedWorkRequired,
    #[error("intervention state transition is not allowed")]
    InvalidTransition,
    #[error("intervention mileage conflicts with service-history chronology")]
    MileageRegression,
    #[error(transparent)]
    Money(#[from] crate::domain::MoneyError),
}

fn optional_text(value: Option<String>) -> Result<Option<String>, InterventionModelError> {
    value
        .map(|value| {
            let value = value.trim().to_owned();
            if value.is_empty() {
                return Ok(None);
            }
            if value.chars().count() > NARRATIVE_MAX_CHARS {
                return Err(InterventionModelError::NarrativeTooLong);
            }
            Ok(Some(value))
        })
        .transpose()
        .map(Option::flatten)
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone as _;

    use super::*;

    fn at(day: u32, second: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, day, 10, 0, second)
            .single()
            .expect("valid fixture timestamp")
    }

    fn entry(id: &str, day: u32, created_second: u32, mileage: u64) -> ServiceHistoryEntry {
        ServiceHistoryEntry {
            id: InterventionId::parse(id).expect("valid intervention id"),
            service_date: NaiveDate::from_ymd_opt(2026, 7, day).expect("valid date"),
            created_at: at(day, created_second),
            status: InterventionStatus::Completed,
            mileage: Some(mileage),
        }
    }

    #[test]
    fn intervention_model_timestamps_terminal_transitions_and_prevents_reopening() {
        let mut intervention = NewIntervention::new(
            VehicleId::parse("golf").expect("valid vehicle id"),
            NaiveDate::from_ymd_opt(2026, 7, 19).expect("valid date"),
            Some(120_000),
            Some(" Noise under braking ".into()),
            None,
            Some(" Replaced front pads ".into()),
            None,
            None,
            CurrencyCode::parse("EUR").expect("valid currency"),
            at(19, 0),
        )
        .expect("valid intervention");
        intervention.complete(at(19, 1)).expect("draft completes");

        assert_eq!(intervention.status, InterventionStatus::Completed);
        assert_eq!(intervention.completed_at, Some(at(19, 1)));
        assert_eq!(
            intervention.cancel(at(19, 2)),
            Err(InterventionModelError::InvalidTransition)
        );
    }

    #[test]
    fn intervention_model_requires_performed_work_for_completion() {
        let mut intervention = NewIntervention::new(
            VehicleId::parse("golf").expect("valid vehicle id"),
            NaiveDate::from_ymd_opt(2026, 7, 19).expect("valid date"),
            None,
            None,
            None,
            Some("  ".into()),
            None,
            None,
            CurrencyCode::parse("EUR").expect("valid currency"),
            at(19, 0),
        )
        .expect("valid draft");

        assert_eq!(
            intervention.complete(at(19, 1)),
            Err(InterventionModelError::PerformedWorkRequired)
        );
    }

    #[test]
    fn service_history_accepts_current_backdated_and_equal_mileage() {
        let history = [
            entry("earlier", 10, 0, 100_000),
            entry("later", 20, 0, 120_000),
        ];
        for candidate in [
            entry("current", 25, 0, 125_000),
            entry("backdated", 15, 0, 110_000),
            entry("equal", 15, 0, 100_000),
        ] {
            validate_service_history_mileage(&candidate, &history)
                .expect("chronological mileage is valid");
        }
    }

    #[test]
    fn service_history_rejects_current_and_backdated_regressions() {
        let history = [
            entry("earlier", 10, 0, 100_000),
            entry("later", 20, 0, 120_000),
        ];
        for candidate in [
            entry("current", 25, 0, 99_999),
            entry("backdated-low", 15, 0, 99_999),
            entry("backdated-high", 15, 0, 120_001),
        ] {
            assert_eq!(
                validate_service_history_mileage(&candidate, &history),
                Err(InterventionModelError::MileageRegression)
            );
        }
    }
}

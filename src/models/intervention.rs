//! Database-independent intervention state and service-history validation.

use chrono::{DateTime, Utc};

use crate::{
    domain::{CurrencyCode, CustomerId, InterventionId, Money, VehicleId},
    models::{
        customer::DISPLAY_NAME_MAX_CHARS,
        vehicle::{MAKE_MAX_CHARS, MODEL_MAX_CHARS, REGISTRATION_MAX_CHARS},
    },
};

pub const NARRATIVE_MAX_CHARS: usize = 10_000;
pub const MIN_ESTIMATED_DURATION_MINUTES: u16 = 30;
pub const MAX_ESTIMATED_DURATION_MINUTES: u16 = 1_440;
pub const ESTIMATED_DURATION_STEP_MINUTES: u16 = 30;

/// A complete planning duration that is always valid for an intervention schedule.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EstimatedDuration(u16);

impl EstimatedDuration {
    /// Validate an estimated duration in whole minutes.
    ///
    /// # Errors
    ///
    /// Rejects values outside 30 minutes through 24 hours or not aligned to a 30-minute step.
    pub const fn new(minutes: u16) -> Result<Self, InterventionModelError> {
        if minutes < MIN_ESTIMATED_DURATION_MINUTES
            || minutes > MAX_ESTIMATED_DURATION_MINUTES
            || !minutes.is_multiple_of(ESTIMATED_DURATION_STEP_MINUTES)
        {
            return Err(InterventionModelError::InvalidEstimatedDuration);
        }
        Ok(Self(minutes))
    }

    #[must_use]
    pub const fn minutes(self) -> u16 {
        self.0
    }
}

/// Historical customer and vehicle display identity captured when an intervention is created.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InterventionIdentitySnapshot {
    pub customer_id: CustomerId,
    pub customer_name: String,
    pub vehicle_registration: Option<String>,
    pub vehicle_make: String,
    pub vehicle_model: String,
}

impl InterventionIdentitySnapshot {
    /// Validate identity values before they become immutable intervention history.
    ///
    /// # Errors
    ///
    /// Rejects blank, untrimmed, or oversized displayed identity values.
    pub fn new(
        customer_id: CustomerId,
        customer_name: String,
        vehicle_registration: Option<String>,
        vehicle_make: String,
        vehicle_model: String,
    ) -> Result<Self, InterventionModelError> {
        Ok(Self {
            customer_id,
            customer_name: snapshot_required(customer_name, DISPLAY_NAME_MAX_CHARS)?,
            vehicle_registration: snapshot_optional(vehicle_registration, REGISTRATION_MAX_CHARS)?,
            vehicle_make: snapshot_required(vehicle_make, MAKE_MAX_CHARS)?,
            vehicle_model: snapshot_required(vehicle_model, MODEL_MAX_CHARS)?,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InterventionStatus {
    Draft,
    Completed,
    Cancelled,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewIntervention {
    pub vehicle_id: VehicleId,
    pub service_date: DateTime<Utc>,
    pub estimated_duration: EstimatedDuration,
    pub identity_snapshot: InterventionIdentitySnapshot,
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
        service_date: DateTime<Utc>,
        estimated_duration: EstimatedDuration,
        identity_snapshot: InterventionIdentitySnapshot,
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
            estimated_duration,
            identity_snapshot,
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
    pub service_date: DateTime<Utc>,
    pub estimated_duration: EstimatedDuration,
    pub identity_snapshot: InterventionIdentitySnapshot,
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
    pub service_date: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub status: InterventionStatus,
    pub mileage: Option<u64>,
}

impl ServiceHistoryEntry {
    fn chronology_key(&self) -> (DateTime<Utc>, DateTime<Utc>, &str) {
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
    #[error("estimated duration must be 30-minute steps from 30 minutes through 24 hours")]
    InvalidEstimatedDuration,
    #[error("intervention identity snapshot is invalid")]
    InvalidIdentitySnapshot,
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

fn snapshot_required(value: String, maximum: usize) -> Result<String, InterventionModelError> {
    if value.is_empty() || value.trim() != value || value.chars().count() > maximum {
        return Err(InterventionModelError::InvalidIdentitySnapshot);
    }
    Ok(value)
}

fn snapshot_optional(
    value: Option<String>,
    maximum: usize,
) -> Result<Option<String>, InterventionModelError> {
    value
        .map(|value| snapshot_required(value, maximum))
        .transpose()
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone as _, Timelike as _};

    use super::*;

    fn at(day: u32, second: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, day, 10, 0, second)
            .single()
            .expect("valid fixture timestamp")
    }

    fn snapshot() -> InterventionIdentitySnapshot {
        InterventionIdentitySnapshot::new(
            CustomerId::parse("owner").expect("valid customer id"),
            "Owner".into(),
            Some("1-ABC-234".into()),
            "Volkswagen".into(),
            "Golf".into(),
        )
        .expect("valid snapshot")
    }

    fn entry(id: &str, day: u32, created_second: u32, mileage: u64) -> ServiceHistoryEntry {
        ServiceHistoryEntry {
            id: InterventionId::parse(id).expect("valid intervention id"),
            service_date: at(day, 0),
            created_at: at(day, created_second),
            status: InterventionStatus::Completed,
            mileage: Some(mileage),
        }
    }

    #[test]
    fn intervention_model_timestamps_terminal_transitions_and_prevents_reopening() {
        let mut intervention = NewIntervention::new(
            VehicleId::parse("golf").expect("valid vehicle id"),
            at(19, 0),
            EstimatedDuration::new(60).expect("valid duration"),
            snapshot(),
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
            at(19, 0),
            EstimatedDuration::new(60).expect("valid duration"),
            snapshot(),
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

    #[test]
    fn intervention_model_duration_cannot_represent_invalid_steps_or_ranges() {
        for minutes in [30, 60, 1_440] {
            assert_eq!(
                EstimatedDuration::new(minutes)
                    .expect("valid duration")
                    .minutes(),
                minutes
            );
        }
        for minutes in [0, 29, 45, 1_441] {
            assert_eq!(
                EstimatedDuration::new(minutes),
                Err(InterventionModelError::InvalidEstimatedDuration)
            );
        }
    }

    #[test]
    fn service_history_orders_distinct_times_on_the_same_civil_date() {
        let mut morning = entry("morning", 19, 0, 100_000);
        morning.service_date = morning.service_date.with_hour(9).expect("valid hour");
        let mut afternoon = entry("afternoon", 19, 0, 120_000);
        afternoon.service_date = afternoon.service_date.with_hour(15).expect("valid hour");
        let mut midday = entry("midday", 19, 0, 110_000);
        midday.service_date = midday.service_date.with_hour(12).expect("valid hour");

        validate_service_history_mileage(&midday, &[morning, afternoon])
            .expect("full timestamp chronology is preserved");
    }
}

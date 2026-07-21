//! Workshop-local time conversion and calendar boundaries.

use std::sync::Arc;

use chrono::{
    DateTime, Datelike as _, Days, LocalResult, Months, NaiveDate, NaiveDateTime, TimeZone as _,
    Utc,
};
use chrono_tz::Tz;
use thiserror::Error;

const LOCAL_MINUTE_FORMAT: &str = "%Y-%m-%dT%H:%M";

/// Time source seam used by workshop-local date behavior.
pub trait Clock: Send + Sync {
    /// Return the current UTC instant.
    fn now(&self) -> DateTime<Utc>;
}

/// Production wall clock.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// Half-open UTC range derived from workshop-local calendar boundaries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UtcRange {
    start: DateTime<Utc>,
    end: DateTime<Utc>,
}

impl UtcRange {
    #[must_use]
    pub const fn start(self) -> DateTime<Utc> {
        self.start
    }

    #[must_use]
    pub const fn end(self) -> DateTime<Utc> {
        self.end
    }
}

/// Shared application-wide workshop timezone behavior.
#[derive(Clone)]
pub struct WorkshopTime {
    timezone: Tz,
    clock: Arc<dyn Clock>,
}

impl WorkshopTime {
    #[must_use]
    pub fn new(timezone: Tz, clock: Arc<dyn Clock>) -> Self {
        Self { timezone, clock }
    }

    #[must_use]
    pub fn system(timezone: Tz) -> Self {
        Self::new(timezone, Arc::new(SystemClock))
    }

    #[must_use]
    pub const fn timezone(&self) -> Tz {
        self.timezone
    }

    /// Derive today from the injected UTC clock and configured workshop timezone.
    #[must_use]
    pub fn current_local_date(&self) -> NaiveDate {
        self.utc_to_local(self.clock.now()).date_naive()
    }

    /// Resolve an exact workshop-local minute to one unambiguous UTC instant.
    pub fn local_to_utc(&self, value: &str) -> Result<DateTime<Utc>, WorkshopTimeError> {
        let local = NaiveDateTime::parse_from_str(value, LOCAL_MINUTE_FORMAT)
            .map_err(|_| WorkshopTimeError::MalformedLocalDateTime)?;
        self.resolve(local)
    }

    /// Convert a stored UTC instant for workshop-local presentation.
    #[must_use]
    pub fn utc_to_local(&self, value: DateTime<Utc>) -> DateTime<Tz> {
        value.with_timezone(&self.timezone)
    }

    /// Return the half-open UTC range for the workshop-local month containing `anchor`.
    pub fn month_boundaries(&self, anchor: NaiveDate) -> Result<UtcRange, WorkshopTimeError> {
        let start = NaiveDate::from_ymd_opt(anchor.year(), anchor.month(), 1)
            .ok_or(WorkshopTimeError::CalendarBoundaryOutOfRange)?;
        let end = start
            .checked_add_months(Months::new(1))
            .ok_or(WorkshopTimeError::CalendarBoundaryOutOfRange)?;
        self.date_range(start, end)
    }

    /// Return the half-open Monday-through-Monday UTC range containing `anchor`.
    pub fn week_boundaries(&self, anchor: NaiveDate) -> Result<UtcRange, WorkshopTimeError> {
        let days_from_monday = u64::from(anchor.weekday().num_days_from_monday());
        let start = anchor
            .checked_sub_days(Days::new(days_from_monday))
            .ok_or(WorkshopTimeError::CalendarBoundaryOutOfRange)?;
        let end = start
            .checked_add_days(Days::new(7))
            .ok_or(WorkshopTimeError::CalendarBoundaryOutOfRange)?;
        self.date_range(start, end)
    }

    fn date_range(&self, start: NaiveDate, end: NaiveDate) -> Result<UtcRange, WorkshopTimeError> {
        let start = start
            .and_hms_opt(0, 0, 0)
            .ok_or(WorkshopTimeError::CalendarBoundaryOutOfRange)?;
        let end = end
            .and_hms_opt(0, 0, 0)
            .ok_or(WorkshopTimeError::CalendarBoundaryOutOfRange)?;
        Ok(UtcRange {
            start: self.resolve(start)?,
            end: self.resolve(end)?,
        })
    }

    fn resolve(&self, local: NaiveDateTime) -> Result<DateTime<Utc>, WorkshopTimeError> {
        match self.timezone.from_local_datetime(&local) {
            LocalResult::Single(value) => Ok(value.with_timezone(&Utc)),
            LocalResult::None => Err(WorkshopTimeError::NonexistentLocalDateTime),
            LocalResult::Ambiguous(_, _) => Err(WorkshopTimeError::AmbiguousLocalDateTime),
        }
    }
}

/// Actionable, typed failures for workshop-local input and calendar boundaries.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum WorkshopTimeError {
    #[error("enter a local date and time in YYYY-MM-DDTHH:MM format")]
    MalformedLocalDateTime,
    #[error("this local time does not exist because the clock changes; choose another time")]
    NonexistentLocalDateTime,
    #[error("this local time occurs twice because the clock changes; choose another time")]
    AmbiguousLocalDateTime,
    #[error("the requested local calendar boundary is outside the supported date range")]
    CalendarBoundaryOutOfRange,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct FixedClock(DateTime<Utc>);

    impl Clock for FixedClock {
        fn now(&self) -> DateTime<Utc> {
            self.0
        }
    }

    fn utc(value: &str) -> DateTime<Utc> {
        value.parse().expect("fixture UTC instant should parse")
    }

    fn date(value: &str) -> NaiveDate {
        value.parse().expect("fixture date should parse")
    }

    fn brussels_at(now: &str) -> WorkshopTime {
        WorkshopTime::new(chrono_tz::Europe::Brussels, Arc::new(FixedClock(utc(now))))
    }

    #[test]
    fn workshop_time_uses_the_injected_clock_for_the_current_local_date() {
        let time = brussels_at("2026-01-01T23:30:00Z");

        assert_eq!(time.current_local_date(), date("2026-01-02"));
    }

    #[test]
    fn workshop_time_resolves_brussels_winter_and_summer_offsets() {
        let time = brussels_at("2026-01-01T00:00:00Z");

        assert_eq!(
            time.local_to_utc("2026-01-15T09:30"),
            Ok(utc("2026-01-15T08:30:00Z"))
        );
        assert_eq!(
            time.local_to_utc("2026-07-15T09:30"),
            Ok(utc("2026-07-15T07:30:00Z"))
        );
        assert_eq!(
            time.utc_to_local(utc("2026-07-15T07:30:00Z"))
                .format("%Y-%m-%dT%H:%M%:z")
                .to_string(),
            "2026-07-15T09:30+02:00"
        );
    }

    #[test]
    fn workshop_time_rejects_malformed_gaps_and_overlaps_distinctly() {
        let time = brussels_at("2026-01-01T00:00:00Z");

        assert_eq!(
            time.local_to_utc("2026-07-15"),
            Err(WorkshopTimeError::MalformedLocalDateTime)
        );
        assert_eq!(
            time.local_to_utc("2026-03-29T02:30"),
            Err(WorkshopTimeError::NonexistentLocalDateTime)
        );
        assert_eq!(
            time.local_to_utc("2026-10-25T02:30"),
            Err(WorkshopTimeError::AmbiguousLocalDateTime)
        );
    }

    #[test]
    fn workshop_time_month_boundaries_follow_offset_changes() {
        let time = brussels_at("2026-01-01T00:00:00Z");
        let range = time
            .month_boundaries(date("2026-03-15"))
            .expect("March boundaries should resolve");

        assert_eq!(range.start(), utc("2026-02-28T23:00:00Z"));
        assert_eq!(range.end(), utc("2026-03-31T22:00:00Z"));
    }

    #[test]
    fn workshop_time_week_boundaries_are_monday_first_across_dst() {
        let time = brussels_at("2026-01-01T00:00:00Z");
        let range = time
            .week_boundaries(date("2026-03-29"))
            .expect("DST-transition week boundaries should resolve");

        assert_eq!(range.start(), utc("2026-03-22T23:00:00Z"));
        assert_eq!(range.end(), utc("2026-03-29T22:00:00Z"));
        assert_eq!(
            time.utc_to_local(range.start())
                .weekday()
                .number_from_monday(),
            1
        );
        assert_eq!(
            time.utc_to_local(range.end())
                .weekday()
                .number_from_monday(),
            1
        );
    }
}

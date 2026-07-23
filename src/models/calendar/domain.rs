//! Persistence-neutral calendar range and entry projection.

use chrono::{DateTime, NaiveDate, TimeDelta, Utc};

use crate::domain::InterventionId;

use crate::models::intervention::{
    EstimatedDuration, InterventionIdentitySnapshot, InterventionStatus,
};

/// Calendar periods supported by the initial read-only workshop calendar.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CalendarView {
    Month,
    Week,
}

/// A validated half-open UTC range with its workshop-local presentation dates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CalendarRange {
    view: CalendarView,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    local_start: NaiveDate,
    local_end: NaiveDate,
}

impl CalendarRange {
    pub(crate) fn new(
        view: CalendarView,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        local_start: NaiveDate,
        local_end: NaiveDate,
    ) -> Option<Self> {
        (start < end && local_start < local_end).then_some(Self {
            view,
            start,
            end,
            local_start,
            local_end,
        })
    }

    #[must_use]
    pub const fn view(self) -> CalendarView {
        self.view
    }

    #[must_use]
    pub const fn start(self) -> DateTime<Utc> {
        self.start
    }

    #[must_use]
    pub const fn end(self) -> DateTime<Utc> {
        self.end
    }

    #[must_use]
    pub const fn local_start(self) -> NaiveDate {
        self.local_start
    }

    /// Exclusive workshop-local end date.
    #[must_use]
    pub const fn local_end(self) -> NaiveDate {
        self.local_end
    }
}

/// Immutable intervention data required by calendar presentation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CalendarEntry {
    pub id: InterventionId,
    pub start: DateTime<Utc>,
    pub estimated_duration: EstimatedDuration,
    pub status: InterventionStatus,
    pub identity_snapshot: InterventionIdentitySnapshot,
}

impl CalendarEntry {
    /// Calculate the half-open end instant without wrapping corrupt extreme timestamps.
    #[must_use]
    pub fn end(&self) -> Option<DateTime<Utc>> {
        self.start.checked_add_signed(TimeDelta::minutes(i64::from(
            self.estimated_duration.minutes(),
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(value: &str) -> NaiveDate {
        value.parse().expect("fixture date")
    }

    fn instant(value: &str) -> DateTime<Utc> {
        value.parse().expect("fixture instant")
    }

    #[test]
    fn calendar_range_rejects_empty_or_reversed_bounds() {
        let start = instant("2026-07-01T00:00:00Z");
        let end = instant("2026-08-01T00:00:00Z");

        assert!(CalendarRange::new(
            CalendarView::Month,
            start,
            end,
            date("2026-07-01"),
            date("2026-08-01")
        )
        .is_some());
        assert!(CalendarRange::new(
            CalendarView::Month,
            end,
            start,
            date("2026-07-01"),
            date("2026-08-01")
        )
        .is_none());
    }
}

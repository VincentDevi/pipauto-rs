//! Bounded Month and Week calendar read workflow.

use std::sync::Arc;

use chrono::NaiveDate;

use crate::{
    domain::{ValidationCode, ValidationError, ValidationErrors, WorkshopTime},
    models::calendar::{CalendarEntry, CalendarRange, CalendarView},
    repositories::calendar::CalendarRepository,
};

use super::WorkflowError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CalendarSchedule {
    pub anchor: NaiveDate,
    pub range: CalendarRange,
    pub entries: Vec<CalendarEntry>,
}

#[derive(Clone)]
pub struct CalendarService {
    repository: Arc<dyn CalendarRepository>,
    workshop_time: WorkshopTime,
}

impl CalendarService {
    #[must_use]
    pub fn new(repository: Arc<dyn CalendarRepository>, workshop_time: WorkshopTime) -> Self {
        Self {
            repository,
            workshop_time,
        }
    }

    /// Read the month containing `anchor`, defaulting through the injected workshop clock.
    pub async fn month(
        &self,
        anchor: Option<NaiveDate>,
    ) -> Result<CalendarSchedule, WorkflowError> {
        self.read(CalendarView::Month, anchor).await
    }

    /// Read the Monday-first week containing `anchor`, defaulting through the injected clock.
    pub async fn week(&self, anchor: Option<NaiveDate>) -> Result<CalendarSchedule, WorkflowError> {
        self.read(CalendarView::Week, anchor).await
    }

    async fn read(
        &self,
        view: CalendarView,
        anchor: Option<NaiveDate>,
    ) -> Result<CalendarSchedule, WorkflowError> {
        let anchor = anchor.unwrap_or_else(|| self.workshop_time.current_local_date());
        let utc = match view {
            CalendarView::Month => self.workshop_time.month_boundaries(anchor),
            CalendarView::Week => self.workshop_time.week_boundaries(anchor),
        }
        .map_err(|_| invalid_calendar_date())?;
        let local_start = self.workshop_time.utc_to_local(utc.start()).date_naive();
        let local_end = self.workshop_time.utc_to_local(utc.end()).date_naive();
        let range = CalendarRange::new(view, utc.start(), utc.end(), local_start, local_end)
            .ok_or(WorkflowError::Internal)?;
        let entries = self.repository.entries(&range).await?;
        Ok(CalendarSchedule {
            anchor,
            range,
            entries,
        })
    }

    #[must_use]
    pub fn workshop_time(&self) -> &WorkshopTime {
        &self.workshop_time
    }
}

fn invalid_calendar_date() -> WorkflowError {
    WorkflowError::Validation(ValidationErrors::one(ValidationError::new_unchecked(
        "date",
        ValidationCode::OutOfRange,
        "choose a calendar date within the supported range",
    )))
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use async_trait::async_trait;
    use chrono::{DateTime, Utc};

    use super::*;
    use crate::{domain::Clock, repositories::RepositoryError};

    #[derive(Debug)]
    struct FixedClock(DateTime<Utc>);

    impl Clock for FixedClock {
        fn now(&self) -> DateTime<Utc> {
            self.0
        }
    }

    #[derive(Default)]
    struct RecordingRepository {
        ranges: Mutex<Vec<CalendarRange>>,
        failure: Mutex<Option<RepositoryError>>,
    }

    #[async_trait]
    impl CalendarRepository for RecordingRepository {
        async fn entries(
            &self,
            range: &CalendarRange,
        ) -> Result<Vec<CalendarEntry>, RepositoryError> {
            self.ranges.lock().expect("ranges lock").push(*range);
            if let Some(error) = *self.failure.lock().expect("failure lock") {
                return Err(error);
            }
            Ok(Vec::new())
        }
    }

    fn instant(value: &str) -> DateTime<Utc> {
        value.parse().expect("fixture instant")
    }

    fn date(value: &str) -> NaiveDate {
        value.parse().expect("fixture date")
    }

    fn service(now: &str) -> (CalendarService, Arc<RecordingRepository>) {
        let repository = Arc::new(RecordingRepository::default());
        let time = WorkshopTime::new(
            chrono_tz::Europe::Brussels,
            Arc::new(FixedClock(instant(now))),
        );
        (CalendarService::new(repository.clone(), time), repository)
    }

    #[tokio::test]
    async fn calendar_service_defaults_to_the_current_workshop_local_month() {
        let (service, repository) = service("2026-01-31T23:30:00Z");

        let schedule = service.month(None).await.expect("calendar month");

        assert_eq!(schedule.anchor, date("2026-02-01"));
        assert_eq!(schedule.range.local_start(), date("2026-02-01"));
        assert_eq!(schedule.range.local_end(), date("2026-03-01"));
        assert_eq!(repository.ranges.lock().expect("ranges lock").len(), 1);
    }

    #[tokio::test]
    async fn calendar_service_handles_leap_days_year_boundaries_and_dst() {
        let (service, _) = service("2026-01-01T00:00:00Z");

        let leap = service
            .month(Some(date("2024-02-29")))
            .await
            .expect("leap month");
        assert_eq!(leap.range.local_end(), date("2024-03-01"));

        let year = service
            .week(Some(date("2025-12-31")))
            .await
            .expect("year boundary week");
        assert_eq!(year.range.local_start(), date("2025-12-29"));
        assert_eq!(year.range.local_end(), date("2026-01-05"));

        let dst = service
            .month(Some(date("2026-03-29")))
            .await
            .expect("DST month");
        assert_eq!(dst.range.start(), instant("2026-02-28T23:00:00Z"));
        assert_eq!(dst.range.end(), instant("2026-03-31T22:00:00Z"));
    }

    #[tokio::test]
    async fn calendar_service_maps_repository_failures_to_workflow_errors() {
        let (service, repository) = service("2026-01-01T00:00:00Z");
        *repository.failure.lock().expect("failure lock") = Some(RepositoryError::CorruptData);

        assert_eq!(service.week(None).await, Err(WorkflowError::Internal));
    }
}

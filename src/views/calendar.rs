//! Presentation-safe calendar page, day, entry, segment, and geometry types.

use chrono::{DateTime, Days, NaiveDate, Timelike as _, Utc};
use serde::Serialize;
use thiserror::Error;

use crate::{
    domain::WorkshopTime,
    models::{
        calendar::{CalendarEntry, CalendarView},
        intervention::InterventionStatus,
    },
    services::calendar::CalendarSchedule,
};

#[derive(Debug, Serialize)]
pub struct CalendarPage {
    pub view: &'static str,
    pub anchor_date: String,
    pub range_start_date: String,
    pub range_end_date: String,
    pub entries: Vec<CalendarEntryView>,
    pub days: Vec<CalendarDay>,
}

#[derive(Debug, Serialize)]
pub struct CalendarDay {
    pub date: String,
    pub segments: Vec<CalendarSegment>,
}

#[derive(Debug, Serialize)]
pub struct CalendarEntryView {
    pub id: String,
    pub start_label: String,
    pub end_label: String,
    pub duration_label: String,
    pub customer_name: String,
    pub registration: String,
    pub vehicle: String,
    pub status: &'static str,
}

#[derive(Debug, Serialize)]
pub struct CalendarSegment {
    pub entry_id: String,
    pub date: String,
    pub start_label: String,
    pub end_label: String,
    pub duration_label: String,
    pub customer_name: String,
    pub registration: String,
    pub vehicle: String,
    pub status: &'static str,
    pub continuation_before: bool,
    pub continuation_after: bool,
    pub continuation_label: Option<&'static str>,
    pub lane: u16,
    pub lane_count: u16,
    pub geometry: Option<CalendarGeometry>,
    #[serde(skip)]
    interval_start: DateTime<Utc>,
    #[serde(skip)]
    interval_end: DateTime<Utc>,
}

/// Validated ordinary-day Week positioning values. All serialized values are numeric.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct CalendarGeometry {
    pub start_minute: u16,
    pub span_minutes: u16,
    pub lane: u16,
    pub lane_count: u16,
}

impl CalendarGeometry {
    fn new(
        start_minute: u16,
        span_minutes: u16,
        lane: u16,
        lane_count: u16,
    ) -> Result<Self, CalendarPresentationError> {
        let end_minute = start_minute
            .checked_add(span_minutes)
            .ok_or(CalendarPresentationError::InvalidGeometry)?;
        if start_minute >= 1_440
            || span_minutes == 0
            || end_minute > 1_440
            || lane_count == 0
            || lane >= lane_count
        {
            return Err(CalendarPresentationError::InvalidGeometry);
        }
        Ok(Self {
            start_minute,
            span_minutes,
            lane,
            lane_count,
        })
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum CalendarPresentationError {
    #[error("calendar entry duration overflows its start instant")]
    DurationOverflow,
    #[error("calendar date boundary is outside the supported range")]
    BoundaryOutOfRange,
    #[error("calendar geometry is outside its validated numeric bounds")]
    InvalidGeometry,
}

impl CalendarPage {
    /// Split all entries by workshop-local date and assign deterministic overlap lanes.
    pub fn build(
        schedule: CalendarSchedule,
        workshop_time: &WorkshopTime,
    ) -> Result<Self, CalendarPresentationError> {
        let entry_views = schedule
            .entries
            .iter()
            .map(|entry| entry_view(entry, workshop_time))
            .collect::<Result<Vec<_>, _>>()?;
        let mut days = Vec::new();
        let mut date = schedule.range.local_start();
        while date < schedule.range.local_end() {
            let day_range = workshop_time
                .day_boundaries(date)
                .map_err(|_| CalendarPresentationError::BoundaryOutOfRange)?;
            let ordinary_day = (day_range.end() - day_range.start()).num_minutes() == 1_440;
            let mut segments = schedule
                .entries
                .iter()
                .filter_map(|entry| {
                    segment_for_day(entry, date, day_range, ordinary_day, workshop_time).transpose()
                })
                .collect::<Result<Vec<_>, _>>()?;
            assign_lanes(&mut segments)?;
            days.push(CalendarDay {
                date: date.to_string(),
                segments,
            });
            date = date
                .checked_add_days(Days::new(1))
                .ok_or(CalendarPresentationError::BoundaryOutOfRange)?;
        }
        Ok(Self {
            view: match schedule.range.view() {
                CalendarView::Month => "month",
                CalendarView::Week => "week",
            },
            anchor_date: schedule.anchor.to_string(),
            range_start_date: schedule.range.local_start().to_string(),
            range_end_date: schedule.range.local_end().to_string(),
            entries: entry_views,
            days,
        })
    }
}

fn entry_view(
    entry: &CalendarEntry,
    workshop_time: &WorkshopTime,
) -> Result<CalendarEntryView, CalendarPresentationError> {
    let end = entry
        .end()
        .ok_or(CalendarPresentationError::DurationOverflow)?;
    Ok(CalendarEntryView {
        id: entry.id.as_str().to_owned(),
        start_label: workshop_time
            .utc_to_local(entry.start)
            .format("%Y-%m-%d %H:%M")
            .to_string(),
        end_label: workshop_time
            .utc_to_local(end)
            .format("%Y-%m-%d %H:%M")
            .to_string(),
        duration_label: duration_label(entry.estimated_duration.minutes()),
        customer_name: entry.identity_snapshot.customer_name.clone(),
        registration: entry
            .identity_snapshot
            .vehicle_registration
            .clone()
            .unwrap_or_else(|| "No registration".to_owned()),
        vehicle: format!(
            "{} {}",
            entry.identity_snapshot.vehicle_make, entry.identity_snapshot.vehicle_model
        ),
        status: status_label(entry.status),
    })
}

fn segment_for_day(
    entry: &CalendarEntry,
    date: NaiveDate,
    day_range: crate::domain::UtcRange,
    ordinary_day: bool,
    workshop_time: &WorkshopTime,
) -> Result<Option<CalendarSegment>, CalendarPresentationError> {
    let entry_end = entry
        .end()
        .ok_or(CalendarPresentationError::DurationOverflow)?;
    let interval_start = entry.start.max(day_range.start());
    let interval_end = entry_end.min(day_range.end());
    if interval_start >= interval_end {
        return Ok(None);
    }
    let continuation_before = entry.start < interval_start;
    let continuation_after = entry_end > interval_end;
    let geometry = ordinary_day
        .then(|| ordinary_geometry(interval_start, interval_end, date, workshop_time))
        .transpose()?;
    Ok(Some(CalendarSegment {
        entry_id: entry.id.as_str().to_owned(),
        date: date.to_string(),
        start_label: workshop_time
            .utc_to_local(interval_start)
            .format("%H:%M")
            .to_string(),
        end_label: workshop_time
            .utc_to_local(interval_end)
            .format("%H:%M")
            .to_string(),
        duration_label: duration_label(entry.estimated_duration.minutes()),
        customer_name: entry.identity_snapshot.customer_name.clone(),
        registration: entry
            .identity_snapshot
            .vehicle_registration
            .clone()
            .unwrap_or_else(|| "No registration".to_owned()),
        vehicle: format!(
            "{} {}",
            entry.identity_snapshot.vehicle_make, entry.identity_snapshot.vehicle_model
        ),
        status: status_label(entry.status),
        continuation_before,
        continuation_after,
        continuation_label: match (continuation_before, continuation_after) {
            (true, true) => Some("Continues from the previous day and into the next day"),
            (true, false) => Some("Continues from the previous day"),
            (false, true) => Some("Continues into the next day"),
            (false, false) => None,
        },
        lane: 0,
        lane_count: 1,
        geometry,
        interval_start,
        interval_end,
    }))
}

fn ordinary_geometry(
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    date: NaiveDate,
    workshop_time: &WorkshopTime,
) -> Result<CalendarGeometry, CalendarPresentationError> {
    let local_start = workshop_time.utc_to_local(start);
    let local_end = workshop_time.utc_to_local(end);
    let start_minute = u16::try_from(local_start.hour() * 60 + local_start.minute())
        .map_err(|_| CalendarPresentationError::InvalidGeometry)?;
    let end_minute = if local_end.date_naive() > date {
        1_440
    } else {
        u16::try_from(local_end.hour() * 60 + local_end.minute())
            .map_err(|_| CalendarPresentationError::InvalidGeometry)?
    };
    let span_minutes = end_minute
        .checked_sub(start_minute)
        .ok_or(CalendarPresentationError::InvalidGeometry)?;
    CalendarGeometry::new(start_minute, span_minutes, 0, 1)
}

fn assign_lanes(segments: &mut [CalendarSegment]) -> Result<(), CalendarPresentationError> {
    segments.sort_by(|left, right| {
        left.interval_start
            .cmp(&right.interval_start)
            .then_with(|| left.interval_end.cmp(&right.interval_end))
            .then_with(|| left.entry_id.cmp(&right.entry_id))
    });
    let mut group_start = 0;
    while group_start < segments.len() {
        let mut group_end = segments[group_start].interval_end;
        let mut group_stop = group_start + 1;
        while group_stop < segments.len() && segments[group_stop].interval_start < group_end {
            group_end = group_end.max(segments[group_stop].interval_end);
            group_stop += 1;
        }
        assign_group_lanes(&mut segments[group_start..group_stop])?;
        group_start = group_stop;
    }
    Ok(())
}

fn assign_group_lanes(group: &mut [CalendarSegment]) -> Result<(), CalendarPresentationError> {
    let mut lane_ends: Vec<DateTime<Utc>> = Vec::new();
    let mut lanes = Vec::with_capacity(group.len());
    for segment in group.iter() {
        let lane = lane_ends
            .iter()
            .position(|end| *end <= segment.interval_start)
            .unwrap_or(lane_ends.len());
        if lane == lane_ends.len() {
            lane_ends.push(segment.interval_end);
        } else {
            lane_ends[lane] = segment.interval_end;
        }
        lanes.push(u16::try_from(lane).map_err(|_| CalendarPresentationError::InvalidGeometry)?);
    }
    let lane_count =
        u16::try_from(lane_ends.len()).map_err(|_| CalendarPresentationError::InvalidGeometry)?;
    for (segment, lane) in group.iter_mut().zip(lanes) {
        if lane >= lane_count || lane_count == 0 {
            return Err(CalendarPresentationError::InvalidGeometry);
        }
        segment.lane = lane;
        segment.lane_count = lane_count;
        if let Some(geometry) = segment.geometry {
            segment.geometry = Some(CalendarGeometry::new(
                geometry.start_minute,
                geometry.span_minutes,
                lane,
                lane_count,
            )?);
        }
    }
    Ok(())
}

const fn status_label(status: InterventionStatus) -> &'static str {
    match status {
        InterventionStatus::Draft => "Draft",
        InterventionStatus::Completed => "Completed",
        InterventionStatus::Cancelled => "Cancelled",
    }
}

fn duration_label(minutes: u16) -> String {
    match (minutes / 60, minutes % 60) {
        (0, minutes) => format!("{minutes} min"),
        (hours, 0) => format!("{hours} h"),
        (hours, minutes) => format!("{hours} h {minutes} min"),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::{DateTime, Utc};

    use super::*;
    use crate::{
        domain::{CustomerId, InterventionId},
        models::{
            calendar::CalendarRange,
            intervention::{EstimatedDuration, InterventionIdentitySnapshot},
        },
    };

    fn instant(value: &str) -> DateTime<Utc> {
        value.parse().expect("fixture instant")
    }

    fn date(value: &str) -> NaiveDate {
        value.parse().expect("fixture date")
    }

    fn time() -> WorkshopTime {
        WorkshopTime::new(
            chrono_tz::Europe::Brussels,
            Arc::new(FixedClock(instant("2026-01-01T00:00:00Z"))),
        )
    }

    #[derive(Debug)]
    struct FixedClock(DateTime<Utc>);

    impl crate::domain::Clock for FixedClock {
        fn now(&self) -> DateTime<Utc> {
            self.0
        }
    }

    fn entry(id: &str, local_start: &str, duration: u16) -> CalendarEntry {
        CalendarEntry {
            id: InterventionId::parse(id).expect("entry id"),
            start: time().local_to_utc(local_start).expect("local start"),
            estimated_duration: EstimatedDuration::new(duration).expect("duration"),
            status: InterventionStatus::Draft,
            identity_snapshot: InterventionIdentitySnapshot::new(
                CustomerId::parse("customer").expect("customer id"),
                "Mario Rossi".to_owned(),
                Some("1-ABC-234".to_owned()),
                "Volkswagen".to_owned(),
                "Golf".to_owned(),
            )
            .expect("identity"),
        }
    }

    fn schedule(entries: Vec<CalendarEntry>, start: &str, end: &str) -> CalendarSchedule {
        let workshop_time = time();
        let local_start = date(start);
        let local_end = date(end);
        let utc_start = workshop_time
            .day_boundaries(local_start)
            .expect("start boundary")
            .start();
        let utc_end = workshop_time
            .day_boundaries(local_end)
            .expect("end boundary")
            .start();
        CalendarSchedule {
            anchor: local_start,
            range: CalendarRange::new(
                CalendarView::Week,
                utc_start,
                utc_end,
                local_start,
                local_end,
            )
            .expect("range"),
            entries,
        }
    }

    #[test]
    fn calendar_segments_split_midnight_with_visible_continuations() {
        let page = CalendarPage::build(
            schedule(
                vec![entry("overnight", "2026-07-20T23:30", 120)],
                "2026-07-20",
                "2026-07-22",
            ),
            &time(),
        )
        .expect("calendar page");

        let first = &page.days[0].segments[0];
        let second = &page.days[1].segments[0];
        assert_eq!(first.start_label, "23:30");
        assert_eq!(first.end_label, "00:00");
        assert!(first.continuation_after);
        assert_eq!(
            first.continuation_label,
            Some("Continues into the next day")
        );
        assert_eq!(second.start_label, "00:00");
        assert_eq!(second.end_label, "01:30");
        assert!(second.continuation_before);
        assert_eq!(
            second.continuation_label,
            Some("Continues from the previous day")
        );
    }

    #[test]
    fn calendar_overlap_lanes_are_lowest_free_and_half_open() {
        let page = CalendarPage::build(
            schedule(
                vec![
                    entry("a", "2026-07-20T09:00", 120),
                    entry("b", "2026-07-20T10:00", 120),
                    entry("c", "2026-07-20T11:00", 120),
                    entry("adjacent", "2026-07-20T13:00", 60),
                ],
                "2026-07-20",
                "2026-07-21",
            ),
            &time(),
        )
        .expect("calendar page");
        let geometries = page.days[0]
            .segments
            .iter()
            .map(|segment| segment.geometry.expect("ordinary-day geometry"))
            .collect::<Vec<_>>();

        assert_eq!((geometries[0].lane, geometries[0].lane_count), (0, 2));
        assert_eq!((geometries[1].lane, geometries[1].lane_count), (1, 2));
        assert_eq!((geometries[2].lane, geometries[2].lane_count), (0, 2));
        assert_eq!((geometries[3].lane, geometries[3].lane_count), (0, 1));
    }

    #[test]
    fn calendar_segments_keep_dst_labels_without_ordinary_day_geometry() {
        let page = CalendarPage::build(
            schedule(
                vec![entry("dst", "2026-03-29T01:30", 120)],
                "2026-03-29",
                "2026-03-30",
            ),
            &time(),
        )
        .expect("DST calendar page");
        let segment = &page.days[0].segments[0];

        assert_eq!(segment.start_label, "01:30");
        assert_eq!(segment.end_label, "04:30");
        assert_eq!(segment.duration_label, "2 h");
        assert!(segment.geometry.is_none());
    }

    #[test]
    fn calendar_segments_reject_duration_overflow() {
        let mut corrupt = entry("overflow", "2026-07-20T09:00", 30);
        corrupt.start = DateTime::<Utc>::MAX_UTC;

        assert!(matches!(
            entry_view(&corrupt, &time()),
            Err(CalendarPresentationError::DurationOverflow)
        ));
    }
}

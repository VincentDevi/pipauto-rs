//! Presentation-safe calendar page, day, entry, segment, and geometry types.

use std::collections::BTreeMap;

use chrono::{DateTime, Datelike as _, Days, Months, NaiveDate, Timelike as _, Utc};
use loco_rs::{
    controller::views::{engines::TeraView, ViewRenderer},
    Result as LocoResult,
};
use serde::Serialize;
use thiserror::Error;

use crate::{
    domain::WorkshopTime,
    models::{
        calendar::{CalendarEntry, CalendarSchedule, CalendarView},
        intervention::InterventionStatus,
    },
};

use super::layout::AuthenticatedLayout;

const BROWSER_PAGE_TEMPLATE: &str = "pages/calendar.html";
const BROWSER_REGION_TEMPLATE: &str = "fragments/calendar.html";
const MONTH_VISIBLE_ENTRY_LIMIT: usize = 3;

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

#[derive(Clone, Debug, Serialize)]
pub struct CalendarSegment {
    pub entry_id: String,
    pub date: String,
    pub date_label: String,
    pub start_datetime: String,
    pub end_datetime: String,
    pub accessible_label: String,
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

/// Complete authenticated Calendar page and replaceable region presentation.
#[derive(Debug, Serialize)]
pub struct CalendarBrowserPage<'page> {
    #[serde(flatten)]
    layout: AuthenticatedLayout<'page>,
    title: &'static str,
    view: &'static str,
    month_selected: bool,
    week_selected: bool,
    period_label: String,
    timezone_label: String,
    previous_href: String,
    previous_label: String,
    today_href: String,
    today_label: String,
    next_href: String,
    next_label: String,
    month_href: String,
    month_label: String,
    week_href: String,
    week_label: String,
    days: Vec<CalendarMonthDay>,
    week_days: Vec<CalendarWeekDay>,
    time_rows: Vec<CalendarTimeRow>,
    selected_day_label: String,
    selected_entries: Vec<CalendarSegment>,
    selected_entry_count: usize,
    has_entries: bool,
    state: &'static str,
    state_heading: Option<&'static str>,
    state_message: Option<&'static str>,
    recovery_href: Option<String>,
    recovery_label: Option<&'static str>,
    correlation_reference: Option<String>,
}

#[derive(Debug, Serialize)]
struct CalendarMonthDay {
    date: String,
    number: u32,
    full_label: String,
    href: String,
    in_month: bool,
    selected: bool,
    today: bool,
    visible_entries: Vec<CalendarSegment>,
    hidden_entries: Vec<CalendarSegment>,
    hidden_count: usize,
    entry_count: usize,
}

#[derive(Debug, Serialize)]
struct CalendarWeekDay {
    date: String,
    weekday_label: String,
    short_weekday_label: String,
    day_label: String,
    full_label: String,
    href: String,
    selected: bool,
    today: bool,
    segments: Vec<CalendarSegment>,
    ordinary_segments: Vec<CalendarSegment>,
    transition_segments: Vec<CalendarSegment>,
    entry_count: usize,
}

#[derive(Debug, Serialize)]
struct CalendarTimeRow {
    label: String,
}

/// Copy-only metadata for a Calendar-owned non-ready response.
pub struct CalendarState {
    pub view: &'static str,
    pub name: &'static str,
    pub heading: &'static str,
    pub message: &'static str,
    pub recovery: Option<(&'static str, String)>,
    pub correlation_reference: Option<String>,
}

impl<'page> CalendarBrowserPage<'page> {
    /// Build the responsive Month presentation from the shared calendar projection.
    pub fn month(
        layout: AuthenticatedLayout<'page>,
        page: CalendarPage,
        today: NaiveDate,
        timezone_label: String,
    ) -> Result<Self, CalendarPresentationError> {
        let anchor = parse_presentation_date(&page.anchor_date)?;
        let month_start = parse_presentation_date(&page.range_start_date)?;
        let month_end = parse_presentation_date(&page.range_end_date)?;
        let grid_start = month_start
            .checked_sub_days(Days::new(u64::from(
                month_start.weekday().num_days_from_monday(),
            )))
            .ok_or(CalendarPresentationError::BoundaryOutOfRange)?;
        let trailing_days = (7 - month_end.weekday().num_days_from_monday()) % 7;
        let grid_end = month_end
            .checked_add_days(Days::new(u64::from(trailing_days)))
            .ok_or(CalendarPresentationError::BoundaryOutOfRange)?;
        let mut segments_by_date = page
            .days
            .into_iter()
            .map(|day| (day.date, day.segments))
            .collect::<BTreeMap<_, _>>();
        let mut days = Vec::new();
        let mut date = grid_start;
        while date < grid_end {
            let segments = segments_by_date
                .remove(&date.to_string())
                .unwrap_or_default();
            let split = segments.len().min(MONTH_VISIBLE_ENTRY_LIMIT);
            days.push(CalendarMonthDay {
                date: date.to_string(),
                number: date.day(),
                full_label: date.format("%A %-d %B %Y").to_string(),
                href: calendar_href("month", date),
                in_month: date >= month_start && date < month_end,
                selected: date == anchor,
                today: date == today,
                visible_entries: segments[..split].to_vec(),
                hidden_entries: segments[split..].to_vec(),
                hidden_count: segments.len().saturating_sub(split),
                entry_count: segments.len(),
            });
            date = date
                .checked_add_days(Days::new(1))
                .ok_or(CalendarPresentationError::BoundaryOutOfRange)?;
        }
        let selected_entries = days
            .iter()
            .find(|day| day.selected)
            .map(|day| {
                day.visible_entries
                    .iter()
                    .chain(&day.hidden_entries)
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let selected_entry_count = selected_entries.len();
        let has_entries = !page.entries.is_empty();
        let navigation = CalendarNavigation::new("month", anchor, today)?;
        Ok(Self {
            layout,
            title: "Calendar · Pipauto",
            view: "month",
            month_selected: true,
            week_selected: false,
            period_label: month_start.format("%B %Y").to_string(),
            timezone_label,
            previous_href: navigation.previous_href,
            previous_label: navigation.previous_label,
            today_href: navigation.today_href,
            today_label: navigation.today_label,
            next_href: navigation.next_href,
            next_label: navigation.next_label,
            month_href: calendar_href("month", anchor),
            month_label: format!("Month view for {}", anchor.format("%-d %B %Y")),
            week_href: calendar_href("week", anchor),
            week_label: format!("Week view containing {}", anchor.format("%-d %B %Y")),
            days,
            week_days: Vec::new(),
            time_rows: time_rows(),
            selected_day_label: anchor.format("%A %-d %B %Y").to_string(),
            selected_entries,
            selected_entry_count,
            has_entries,
            state: "ready",
            state_heading: None,
            state_message: None,
            recovery_href: None,
            recovery_label: None,
            correlation_reference: None,
        })
    }

    /// Build the complete Monday-first Week presentation and focused-day timeline.
    pub fn week(
        layout: AuthenticatedLayout<'page>,
        page: CalendarPage,
        today: NaiveDate,
        timezone_label: String,
    ) -> Result<Self, CalendarPresentationError> {
        let anchor = parse_presentation_date(&page.anchor_date)?;
        let week_start = parse_presentation_date(&page.range_start_date)?;
        let week_end = parse_presentation_date(&page.range_end_date)?;
        if week_end.signed_duration_since(week_start).num_days() != 7
            || week_start.weekday().num_days_from_monday() != 0
        {
            return Err(CalendarPresentationError::InvalidWeekRange);
        }
        let mut segments_by_date = page
            .days
            .into_iter()
            .map(|day| (day.date, day.segments))
            .collect::<BTreeMap<_, _>>();
        let mut week_days = Vec::with_capacity(7);
        let mut date = week_start;
        while date < week_end {
            let segments = segments_by_date
                .remove(&date.to_string())
                .unwrap_or_default();
            let ordinary_segments = segments
                .iter()
                .filter(|segment| segment.geometry.is_some())
                .cloned()
                .collect();
            let transition_segments = segments
                .iter()
                .filter(|segment| segment.geometry.is_none())
                .cloned()
                .collect();
            week_days.push(CalendarWeekDay {
                date: date.to_string(),
                weekday_label: date.format("%A").to_string(),
                short_weekday_label: date.format("%a").to_string(),
                day_label: date.format("%-d %b").to_string(),
                full_label: date.format("%A %-d %B %Y").to_string(),
                href: calendar_href("week", date),
                selected: date == anchor,
                today: date == today,
                entry_count: segments.len(),
                segments,
                ordinary_segments,
                transition_segments,
            });
            date = date
                .checked_add_days(Days::new(1))
                .ok_or(CalendarPresentationError::BoundaryOutOfRange)?;
        }
        let selected_entries = week_days
            .iter()
            .find(|day| day.selected)
            .map(|day| day.segments.clone())
            .unwrap_or_default();
        let selected_entry_count = selected_entries.len();
        let has_entries = !page.entries.is_empty();
        let navigation = CalendarNavigation::new("week", anchor, today)?;
        let sunday = week_end
            .checked_sub_days(Days::new(1))
            .ok_or(CalendarPresentationError::BoundaryOutOfRange)?;
        Ok(Self {
            layout,
            title: "Calendar · Pipauto",
            view: "week",
            month_selected: false,
            week_selected: true,
            period_label: format!(
                "{}–{}",
                week_start.format("%-d %B %Y"),
                sunday.format("%-d %B %Y")
            ),
            timezone_label,
            previous_href: navigation.previous_href,
            previous_label: navigation.previous_label,
            today_href: navigation.today_href,
            today_label: navigation.today_label,
            next_href: navigation.next_href,
            next_label: navigation.next_label,
            month_href: calendar_href("month", anchor),
            month_label: format!("Month view for {}", anchor.format("%-d %B %Y")),
            week_href: calendar_href("week", anchor),
            week_label: format!("Week view containing {}", anchor.format("%-d %B %Y")),
            days: Vec::new(),
            week_days,
            time_rows: time_rows(),
            selected_day_label: anchor.format("%A %-d %B %Y").to_string(),
            selected_entries,
            selected_entry_count,
            has_entries,
            state: "ready",
            state_heading: None,
            state_message: None,
            recovery_href: None,
            recovery_label: None,
            correlation_reference: None,
        })
    }

    /// Build a Calendar-owned recovery state while preserving authenticated navigation.
    pub fn state(
        layout: AuthenticatedLayout<'page>,
        anchor: NaiveDate,
        today: NaiveDate,
        timezone_label: String,
        state: CalendarState,
    ) -> Result<Self, CalendarPresentationError> {
        let navigation = CalendarNavigation::new(state.view, anchor, today)?;
        let period_label = if state.view == "month" {
            anchor.format("%B %Y").to_string()
        } else {
            let monday = anchor
                .checked_sub_days(Days::new(u64::from(
                    anchor.weekday().num_days_from_monday(),
                )))
                .ok_or(CalendarPresentationError::BoundaryOutOfRange)?;
            let sunday = monday
                .checked_add_days(Days::new(6))
                .ok_or(CalendarPresentationError::BoundaryOutOfRange)?;
            format!(
                "{}–{}",
                monday.format("%-d %B %Y"),
                sunday.format("%-d %B %Y")
            )
        };
        let (recovery_label, recovery_href) = state
            .recovery
            .map(|(label, href)| (Some(label), Some(href)))
            .unwrap_or((None, None));
        Ok(Self {
            layout,
            title: "Calendar · Pipauto",
            view: state.view,
            month_selected: state.view == "month",
            week_selected: state.view == "week",
            period_label,
            timezone_label,
            previous_href: navigation.previous_href,
            previous_label: navigation.previous_label,
            today_href: navigation.today_href,
            today_label: navigation.today_label,
            next_href: navigation.next_href,
            next_label: navigation.next_label,
            month_href: calendar_href("month", anchor),
            month_label: format!("Month view for {}", anchor.format("%-d %B %Y")),
            week_href: calendar_href("week", anchor),
            week_label: format!("Week view containing {}", anchor.format("%-d %B %Y")),
            days: Vec::new(),
            week_days: Vec::new(),
            time_rows: time_rows(),
            selected_day_label: anchor.format("%A %-d %B %Y").to_string(),
            selected_entries: Vec::new(),
            selected_entry_count: 0,
            has_entries: false,
            state: state.name,
            state_heading: Some(state.heading),
            state_message: Some(state.message),
            recovery_href,
            recovery_label,
            correlation_reference: state.correlation_reference,
        })
    }

    pub fn render_page(&self, engine: &TeraView) -> LocoResult<String> {
        engine.render(BROWSER_PAGE_TEMPLATE, self)
    }

    pub fn render_region(&self, engine: &TeraView) -> LocoResult<String> {
        engine.render(BROWSER_REGION_TEMPLATE, self)
    }
}

fn time_rows() -> Vec<CalendarTimeRow> {
    (0..48)
        .map(|row| {
            let minutes = row * 30;
            CalendarTimeRow {
                label: format!("{:02}:{:02}", minutes / 60, minutes % 60),
            }
        })
        .collect()
}

struct CalendarNavigation {
    previous_href: String,
    previous_label: String,
    today_href: String,
    today_label: String,
    next_href: String,
    next_label: String,
}

impl CalendarNavigation {
    fn new(
        view: &'static str,
        anchor: NaiveDate,
        today: NaiveDate,
    ) -> Result<Self, CalendarPresentationError> {
        let (previous, next) = if view == "month" {
            (
                anchor.checked_sub_months(Months::new(1)),
                anchor.checked_add_months(Months::new(1)),
            )
        } else {
            (
                anchor.checked_sub_days(Days::new(7)),
                anchor.checked_add_days(Days::new(7)),
            )
        };
        let previous = previous.ok_or(CalendarPresentationError::BoundaryOutOfRange)?;
        let next = next.ok_or(CalendarPresentationError::BoundaryOutOfRange)?;
        Ok(Self {
            previous_href: calendar_href(view, previous),
            previous_label: format!(
                "Previous {view}, {}",
                navigation_date_label(view, previous)?
            ),
            today_href: calendar_href(view, today),
            today_label: format!("Today, {}", navigation_date_label(view, today)?),
            next_href: calendar_href(view, next),
            next_label: format!("Next {view}, {}", navigation_date_label(view, next)?),
        })
    }
}

fn navigation_date_label(
    view: &str,
    anchor: NaiveDate,
) -> Result<String, CalendarPresentationError> {
    if view == "month" {
        return Ok(anchor.format("%B %Y").to_string());
    }
    let monday = anchor
        .checked_sub_days(Days::new(u64::from(
            anchor.weekday().num_days_from_monday(),
        )))
        .ok_or(CalendarPresentationError::BoundaryOutOfRange)?;
    let sunday = monday
        .checked_add_days(Days::new(6))
        .ok_or(CalendarPresentationError::BoundaryOutOfRange)?;
    Ok(format!(
        "{} to {}",
        monday.format("%-d %B %Y"),
        sunday.format("%-d %B %Y")
    ))
}

fn parse_presentation_date(value: &str) -> Result<NaiveDate, CalendarPresentationError> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| CalendarPresentationError::BoundaryOutOfRange)
}

fn calendar_href(view: &str, date: NaiveDate) -> String {
    format!("/calendar?view={view}&date={date}")
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
    #[error("calendar Week range is not exactly Monday through Sunday")]
    InvalidWeekRange,
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
    let local_start = workshop_time.utc_to_local(interval_start);
    let local_end = workshop_time.utc_to_local(interval_end);
    let date_label = date.format("%A %-d %B %Y").to_string();
    let start_label = local_start.format("%H:%M").to_string();
    let end_label = local_end.format("%H:%M").to_string();
    let duration_label = duration_label(entry.estimated_duration.minutes());
    let customer_name = entry.identity_snapshot.customer_name.clone();
    let registration = entry
        .identity_snapshot
        .vehicle_registration
        .clone()
        .unwrap_or_else(|| "No registration".to_owned());
    let vehicle = format!(
        "{} {}",
        entry.identity_snapshot.vehicle_make, entry.identity_snapshot.vehicle_model
    );
    let status = status_label(entry.status);
    let continuation_label = match (continuation_before, continuation_after) {
        (true, true) => Some("Continues from the previous day and into the next day"),
        (true, false) => Some("Continues from the previous day"),
        (false, true) => Some("Continues into the next day"),
        (false, false) => None,
    };
    let accessible_label = format!(
        "{date_label}, {start_label} to {end_label}, {registration}, {vehicle}, \
         {customer_name}, {status}, {duration_label}{}",
        continuation_label.map_or(String::new(), |label| format!(", {label}"))
    );
    Ok(Some(CalendarSegment {
        entry_id: entry.id.as_str().to_owned(),
        date: date.to_string(),
        date_label,
        start_datetime: local_start.format("%Y-%m-%dT%H:%M:%S%:z").to_string(),
        end_datetime: local_end.format("%Y-%m-%dT%H:%M:%S%:z").to_string(),
        accessible_label,
        start_label,
        end_label,
        duration_label,
        customer_name,
        registration,
        vehicle,
        status,
        continuation_before,
        continuation_after,
        continuation_label,
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
        views::context::PresentationUser,
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
    fn calendar_week_view_renders_monday_sunday_and_complete_time_axes() {
        let page = CalendarPage::build(
            schedule(
                vec![
                    entry("early", "2026-07-20T00:00", 30),
                    entry("overlap", "2026-07-21T09:15", 120),
                    entry("overnight", "2026-07-26T23:30", 60),
                ],
                "2026-07-20",
                "2026-07-27",
            ),
            &time(),
        )
        .expect("calendar page");
        let user = PresentationUser {
            display_name: "Filippo".to_owned(),
        };
        let view = CalendarBrowserPage::week(
            AuthenticatedLayout::new(&user, "csrf", "/calendar"),
            page,
            date("2026-07-21"),
            "Europe/Brussels".to_owned(),
        )
        .expect("Week view");

        assert_eq!(view.time_rows.len(), 48);
        assert_eq!(view.week_days.len(), 7);
        assert_eq!(view.week_days[0].weekday_label, "Monday");
        assert_eq!(view.week_days[6].weekday_label, "Sunday");
        assert_eq!(view.selected_day_label, "Monday 20 July 2026");
        let html = view
            .render_region(&TeraView::build().expect("view engine"))
            .expect("Week template");
        assert!(html.contains("Scrollable 24-hour Week timeline"));
        assert!(html.contains("2026-07-26"));
        assert!(html.contains("view=week"));
        assert!(html.contains("--calendar-start: 555; --calendar-span: 120;"));
        assert!(html.contains("Continues into the next day"));
        assert_eq!(html.matches("class=\"calendar-time-row\"").count(), 96);
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

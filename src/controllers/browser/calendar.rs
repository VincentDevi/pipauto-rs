//! Authenticated Calendar query parsing and server-rendered Month and Week responses.

use axum::{
    extract::RawQuery,
    http::{HeaderValue, StatusCode},
    response::Response,
};
use chrono::NaiveDate;
use loco_rs::{
    controller::{
        extractor::shared_store::SharedStore, views::engines::TeraView, views::ViewEngine, Routes,
    },
    prelude::get,
    Result,
};

use crate::{
    controllers::browser::{context::BrowserRequestContext, responses},
    services::{calendar::CalendarService, WorkflowError},
    views::{
        calendar::{CalendarBrowserPage, CalendarPage, CalendarState},
        layout::AuthenticatedLayout,
    },
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RequestedView {
    Month,
    Week,
}

impl RequestedView {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Month => "month",
            Self::Week => "week",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CalendarQuery {
    view: RequestedView,
    date: Option<NaiveDate>,
}

async fn show(
    context: BrowserRequestContext,
    SharedStore(service): SharedStore<CalendarService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    RawQuery(raw_query): RawQuery,
) -> Result<Response> {
    let today = service.workshop_time().current_local_date();
    let timezone = service.workshop_time().timezone().to_string();
    let query = match parse_query(raw_query.as_deref()) {
        Ok(query) => query,
        Err(()) => {
            let view = CalendarBrowserPage::state(
                layout(&context),
                today,
                today,
                timezone,
                CalendarState {
                    view: "month",
                    name: "invalid",
                    heading: "Check the Calendar link",
                    message: "Use only Month or Week and a date written as YYYY-MM-DD.",
                    recovery: Some((
                        "Open current month",
                        calendar_href(RequestedView::Month, today),
                    )),
                    correlation_reference: None,
                },
            )
            .map_err(loco_rs::Error::msg)?;
            return render(&context, &engine, &view, StatusCode::UNPROCESSABLE_ENTITY);
        }
    };
    let anchor = query.date.unwrap_or(today);
    let schedule = match query.view {
        RequestedView::Month => service.month(query.date).await,
        RequestedView::Week => service.week(query.date).await,
    };
    let schedule = match schedule {
        Ok(schedule) => schedule,
        Err(error) => {
            return render_workflow_error(
                &context, &engine, &service, query.view, anchor, today, error,
            );
        }
    };
    let page = match CalendarPage::build(schedule, service.workshop_time()) {
        Ok(page) => page,
        Err(_) => {
            return render_unexpected(&context, &engine, &service, query.view, anchor, today);
        }
    };
    let view = match query.view {
        RequestedView::Month => CalendarBrowserPage::month(layout(&context), page, today, timezone),
        RequestedView::Week => CalendarBrowserPage::week(layout(&context), page, today, timezone),
    };
    let view = match view {
        Ok(view) => view,
        Err(_) => {
            return render_unexpected(&context, &engine, &service, query.view, anchor, today);
        }
    };
    render(&context, &engine, &view, StatusCode::OK)
}

#[allow(clippy::too_many_arguments)]
fn render_workflow_error(
    context: &BrowserRequestContext,
    engine: &TeraView,
    service: &CalendarService,
    requested_view: RequestedView,
    anchor: NaiveDate,
    today: NaiveDate,
    error: WorkflowError,
) -> Result<Response> {
    match error {
        WorkflowError::Validation(_) => {
            let view = CalendarBrowserPage::state(
                layout(context),
                anchor,
                today,
                service.workshop_time().timezone().to_string(),
                CalendarState {
                    view: requested_view.as_str(),
                    name: "invalid",
                    heading: "This Calendar date is not supported",
                    message: "Choose another date to continue.",
                    recovery: Some((
                        "Open current month",
                        calendar_href(RequestedView::Month, today),
                    )),
                    correlation_reference: None,
                },
            )
            .map_err(loco_rs::Error::msg)?;
            render(context, engine, &view, StatusCode::UNPROCESSABLE_ENTITY)
        }
        WorkflowError::Unavailable => {
            let view = CalendarBrowserPage::state(
                layout(context),
                anchor,
                today,
                service.workshop_time().timezone().to_string(),
                CalendarState {
                    view: requested_view.as_str(),
                    name: "unavailable",
                    heading: "Calendar is temporarily unavailable",
                    message:
                        "Try this Calendar view again shortly. No intervention data was changed.",
                    recovery: Some(("Try again", calendar_href(requested_view, anchor))),
                    correlation_reference: None,
                },
            )
            .map_err(loco_rs::Error::msg)?;
            render(context, engine, &view, StatusCode::SERVICE_UNAVAILABLE)
        }
        _ => render_unexpected(context, engine, service, requested_view, anchor, today),
    }
}

fn render_unexpected(
    context: &BrowserRequestContext,
    engine: &TeraView,
    service: &CalendarService,
    requested_view: RequestedView,
    anchor: NaiveDate,
    today: NaiveDate,
) -> Result<Response> {
    let reference = responses::correlation_reference();
    tracing::error!(
        correlation_reference = reference,
        view = requested_view.as_str(),
        %anchor,
        "calendar browser rendering failed"
    );
    let view = CalendarBrowserPage::state(
        layout(context),
        anchor,
        today,
        service.workshop_time().timezone().to_string(),
        CalendarState {
            view: requested_view.as_str(),
            name: "unexpected",
            heading: "Something went wrong",
            message: "Try this Calendar view again. No intervention data was changed.",
            recovery: Some(("Try again", calendar_href(requested_view, anchor))),
            correlation_reference: Some(reference.clone()),
        },
    )
    .map_err(loco_rs::Error::msg)?;
    let mut response = render(context, engine, &view, StatusCode::INTERNAL_SERVER_ERROR)?;
    if let Ok(value) = HeaderValue::from_str(&reference) {
        response.headers_mut().insert("X-Correlation-ID", value);
    }
    Ok(response)
}

fn render(
    context: &BrowserRequestContext,
    engine: &TeraView,
    view: &CalendarBrowserPage<'_>,
    status: StatusCode,
) -> Result<Response> {
    Ok(responses::render(
        context.response_preference,
        status,
        view.render_page(engine)?,
        view.render_region(engine)?,
    ))
}

fn parse_query(raw_query: Option<&str>) -> Result<CalendarQuery, ()> {
    let mut view = None;
    let mut date = None;
    for (key, value) in url::form_urlencoded::parse(raw_query.unwrap_or_default().as_bytes()) {
        match key.as_ref() {
            "view" if view.is_none() => {
                view = Some(match value.as_ref() {
                    "month" => RequestedView::Month,
                    "week" => RequestedView::Week,
                    _ => return Err(()),
                });
            }
            "date" if date.is_none() => {
                let value = value.as_ref();
                if value.len() != 10
                    || value.as_bytes().get(4) != Some(&b'-')
                    || value.as_bytes().get(7) != Some(&b'-')
                {
                    return Err(());
                }
                date = Some(NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| ())?);
            }
            _ => return Err(()),
        }
    }
    Ok(CalendarQuery {
        view: view.unwrap_or(RequestedView::Month),
        date,
    })
}

fn calendar_href(view: RequestedView, date: NaiveDate) -> String {
    format!("/calendar?view={}&date={date}", view.as_str())
}

fn layout(context: &BrowserRequestContext) -> AuthenticatedLayout<'_> {
    AuthenticatedLayout::new(
        &context.current_user,
        context.csrf_token.expose(),
        &context.current_path,
    )
}

#[must_use]
pub fn routes() -> Routes {
    Routes::new().add("/calendar", get(show))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar_query_accepts_only_reproducible_view_and_date_values() {
        assert_eq!(
            parse_query(None),
            Ok(CalendarQuery {
                view: RequestedView::Month,
                date: None,
            })
        );
        assert_eq!(
            parse_query(Some("view=week&date=2026-07-21")),
            Ok(CalendarQuery {
                view: RequestedView::Week,
                date: NaiveDate::from_ymd_opt(2026, 7, 21),
            })
        );
        for invalid in [
            "view=day",
            "date=2026-7-21",
            "date=2026-02-30",
            "date=2026-07-21&date=2026-07-22",
            "view=month&cursor=opaque",
        ] {
            assert!(parse_query(Some(invalid)).is_err(), "accepted {invalid}");
        }
    }
}

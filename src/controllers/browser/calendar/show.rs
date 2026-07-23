use super::query::*;
use super::*;
pub(super) async fn show(
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
                context.layout(),
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
        RequestedView::Month => CalendarBrowserPage::month(context.layout(), page, today, timezone),
        RequestedView::Week => CalendarBrowserPage::week(context.layout(), page, today, timezone),
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
pub(super) fn render_workflow_error(
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
                context.layout(),
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
                context.layout(),
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

pub(super) fn render_unexpected(
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
        context.layout(),
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

pub(super) fn render(
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

use super::forms::*;
use super::*;
pub(super) async fn history(
    context: BrowserRequestContext,
    SharedStore(vehicles): SharedStore<VehicleService>,
    SharedStore(interventions): SharedStore<InterventionService>,
    SharedStore(settings): SharedStore<BusinessSettings>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path(raw_id): Path<String>,
    Query(filters): Query<HistoryFilterValues>,
) -> Result<Response> {
    let id = match VehicleId::parse(raw_id) {
        Ok(value) => value,
        Err(_) => return Ok(responses::not_found(context.response_preference, "vehicle")),
    };
    let vehicle = match vehicles.get(&id).await {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(&context, error, "vehicle")),
    };
    let filter = match parse_history_filter(&filters, &settings) {
        Ok(value) => value,
        Err(message) => {
            return render_history(
                &context,
                &engine,
                vehicle,
                filters,
                Page {
                    items: Vec::new(),
                    next_cursor: None,
                },
                None,
                Some(message),
                StatusCode::UNPROCESSABLE_ENTITY,
            )
        }
    };
    let cursor = match parse_cursor(&filters.cursor) {
        Ok(value) => value,
        Err(message) => {
            return render_history(
                &context,
                &engine,
                vehicle,
                filters,
                Page {
                    items: Vec::new(),
                    next_cursor: None,
                },
                None,
                Some(message),
                StatusCode::UNPROCESSABLE_ENTITY,
            )
        }
    };
    let page = match interventions
        .service_history(
            &id,
            PageRequest {
                filter,
                limit: settings.default_collection_limit(),
                after: cursor,
            },
        )
        .await
    {
        Ok(value) => value,
        Err(WorkflowError::Validation(_)) => {
            return render_history(
                &context,
                &engine,
                vehicle,
                filters,
                Page {
                    items: Vec::new(),
                    next_cursor: None,
                },
                None,
                Some("This page link does not match the current history filters.".to_owned()),
                StatusCode::UNPROCESSABLE_ENTITY,
            )
        }
        Err(error) => return Ok(workflow_response(&context, error, "service history")),
    };
    let next_href = page.next_cursor.as_ref().map(|cursor| {
        let mut next = filters.clone();
        next.cursor = cursor.as_str().to_owned();
        history_href(&id, &next)
    });
    render_history(
        &context,
        &engine,
        vehicle,
        filters,
        page,
        next_href,
        None,
        StatusCode::OK,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render_history(
    context: &BrowserRequestContext,
    engine: &TeraView,
    vehicle: crate::models::vehicle::Vehicle,
    mut filters: HistoryFilterValues,
    page: Page<crate::models::intervention::ServiceHistorySummary>,
    next_href: Option<String>,
    filter_error: Option<String>,
    status: StatusCode,
) -> Result<Response> {
    filters.cursor.clear();
    let view = ServiceHistoryPage::new(
        context.layout(),
        vehicle,
        filters,
        page,
        next_href,
        filter_error,
    );
    Ok(responses::render(
        context.response_preference,
        status,
        view.render_page(engine)?,
        view.render_content(engine)?,
    ))
}

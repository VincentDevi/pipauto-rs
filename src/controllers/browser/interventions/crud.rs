use super::forms::*;
use super::*;
pub(super) async fn list(
    context: BrowserRequestContext,
    SharedStore(interventions): SharedStore<InterventionService>,
    SharedStore(vehicles): SharedStore<VehicleService>,
    SharedStore(settings): SharedStore<BusinessSettings>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Query(filters): Query<InterventionFilterValues>,
) -> Result<Response> {
    let filter_vehicles = match all_vehicles(&vehicles).await {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(&context, error, "vehicle selection")),
    };
    let filter = match parse_filter(&filters, &settings) {
        Ok(value) => value,
        Err(message) => {
            return render_list(
                &context,
                &engine,
                filters,
                empty_page(),
                filter_vehicles,
                None,
                Some(message),
                StatusCode::UNPROCESSABLE_ENTITY,
            )
            .await;
        }
    };
    let cursor = match parse_cursor(&filters.cursor) {
        Ok(value) => value,
        Err(message) => {
            return render_list(
                &context,
                &engine,
                filters,
                empty_page(),
                filter_vehicles,
                None,
                Some(message),
                StatusCode::UNPROCESSABLE_ENTITY,
            )
            .await;
        }
    };
    let page = match interventions
        .list(PageRequest {
            filter,
            limit: settings.default_collection_limit(),
            after: cursor,
        })
        .await
    {
        Ok(value) => value,
        Err(WorkflowError::Validation(_)) => {
            return render_list(
                &context,
                &engine,
                filters,
                empty_page(),
                filter_vehicles,
                None,
                Some("This page link does not match the current intervention filters.".to_owned()),
                StatusCode::UNPROCESSABLE_ENTITY,
            )
            .await;
        }
        Err(error) => return Ok(workflow_response(&context, error, "intervention list")),
    };
    let next_href = page.next_cursor.as_ref().map(|cursor| {
        let mut next = filters.clone();
        next.cursor = cursor.as_str().to_owned();
        list_href(&next)
    });
    render_list(
        &context,
        &engine,
        filters,
        page,
        filter_vehicles,
        next_href,
        None,
        StatusCode::OK,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn render_list(
    context: &BrowserRequestContext,
    engine: &TeraView,
    mut filters: InterventionFilterValues,
    page: Page<crate::models::intervention::ServiceHistorySummary>,
    filter_vehicles: Vec<Vehicle>,
    next_href: Option<String>,
    filter_error: Option<String>,
    status: StatusCode,
) -> Result<Response> {
    filters.cursor.clear();
    let view = InterventionListPage::new(
        context.layout(),
        filters,
        page,
        filter_vehicles,
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

pub(super) async fn new_form(
    context: BrowserRequestContext,
    SharedStore(vehicles): SharedStore<VehicleService>,
    SharedStore(customers): SharedStore<CustomerService>,
    SharedStore(settings): SharedStore<BusinessSettings>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path(raw_id): Path<String>,
) -> Result<Response> {
    let vehicle = match vehicle(&vehicles, raw_id, &context).await {
        Ok(value) => value,
        Err(response) => return Ok(response),
    };
    if vehicle.is_archived() {
        return Ok(responses::redirect(
            context.response_preference,
            &format!("/vehicles/{}", vehicle.id.as_str()),
        ));
    }
    let owner = match customers.get(&vehicle.customer_id).await {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(&context, error, "vehicle owner")),
    };
    let values = InterventionFormValues {
        service_date: WorkshopTime::system(settings.workshop_timezone())
            .current_local_date()
            .to_string(),
        ..InterventionFormValues::default()
    };
    render_create_form(
        &context,
        &engine,
        &vehicle,
        &owner,
        settings.default_currency().as_str(),
        FormState::new(values),
        None,
        StatusCode::OK,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn create(
    context: BrowserRequestContext,
    SharedStore(interventions): SharedStore<InterventionService>,
    SharedStore(vehicles): SharedStore<VehicleService>,
    SharedStore(customers): SharedStore<CustomerService>,
    SharedStore(settings): SharedStore<BusinessSettings>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path(raw_id): Path<String>,
    form: AuthenticatedForm<InterventionFormValues>,
) -> Result<Response> {
    let vehicle = match vehicle(&vehicles, raw_id, &context).await {
        Ok(value) => value,
        Err(response) => return Ok(response),
    };
    let owner = match customers.get(&vehicle.customer_id).await {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(&context, error, "vehicle owner")),
    };
    let values = form.fields;
    let command = match create_command(
        &values,
        vehicle.id.clone(),
        settings.default_currency(),
        &settings,
    ) {
        Ok(value) => value,
        Err(errors) => {
            return render_create_form(
                &context,
                &engine,
                &vehicle,
                &owner,
                settings.default_currency().as_str(),
                FormState::with_validation(values, &errors),
                None,
                StatusCode::UNPROCESSABLE_ENTITY,
            )
        }
    };
    match interventions.create(command).await {
        Ok(intervention) => Ok(responses::redirect(
            context.response_preference,
            &format!("/interventions/{}", intervention.id.as_str()),
        )),
        Err(WorkflowError::Validation(errors)) if mileage_error(&errors) => render_create_form(
            &context,
            &engine,
            &vehicle,
            &owner,
            settings.default_currency().as_str(),
            FormState::with_validation(values, &errors),
            Some("This mileage does not fit the vehicle's dated service history. Review the neighboring records; no intervention was changed.".to_owned()),
            StatusCode::CONFLICT,
        ),
        Err(WorkflowError::Validation(errors)) => render_create_form(
            &context,
            &engine,
            &vehicle,
            &owner,
            settings.default_currency().as_str(),
            FormState::with_validation(values, &errors),
            None,
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        Err(WorkflowError::Conflict) => render_create_form(
            &context,
            &engine,
            &vehicle,
            &owner,
            settings.default_currency().as_str(),
            FormState::new(values),
            Some("The vehicle was archived before this draft could be saved. The submitted values are preserved.".to_owned()),
            StatusCode::CONFLICT,
        ),
        Err(error) => Ok(workflow_response(&context, error, "intervention")),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render_create_form(
    context: &BrowserRequestContext,
    engine: &TeraView,
    vehicle: &Vehicle,
    owner: &crate::models::customer::Customer,
    currency: &str,
    form: FormState<InterventionFormValues>,
    conflict: Option<String>,
    status: StatusCode,
) -> Result<Response> {
    let view = InterventionFormPage::create(
        context.layout(),
        vehicle,
        owner,
        currency,
        form.with_known_fields(FORM_FIELDS),
        conflict,
    );
    Ok(responses::render(
        context.response_preference,
        status,
        view.render_page(engine)?,
        view.render_form(engine)?,
    ))
}

pub(super) async fn show(
    context: BrowserRequestContext,
    SharedStore(interventions): SharedStore<InterventionService>,
    SharedStore(vehicles): SharedStore<VehicleService>,
    SharedStore(customers): SharedStore<CustomerService>,
    SharedStore(attachments): SharedStore<AttachmentService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path(raw_id): Path<String>,
) -> Result<Response> {
    let id = match intervention_id(raw_id, &context) {
        Ok(value) => value,
        Err(response) => return Ok(response),
    };
    render_detail(
        &context,
        &engine,
        &interventions,
        &vehicles,
        &customers,
        &attachments,
        &id,
        None,
        StatusCode::OK,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn render_detail(
    context: &BrowserRequestContext,
    engine: &TeraView,
    interventions: &InterventionService,
    vehicles: &VehicleService,
    customers: &CustomerService,
    attachments: &AttachmentService,
    id: &InterventionId,
    conflict: Option<String>,
    status: StatusCode,
) -> Result<Response> {
    let intervention = match interventions.get(id).await {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(context, error, "intervention")),
    };
    let vehicle = match vehicles.get(&intervention.vehicle_id).await {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(context, error, "intervention vehicle")),
    };
    let owner = match customers.get(&vehicle.customer_id).await {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(context, error, "vehicle owner")),
    };
    let workspace = match interventions.line_workspace(id).await {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(context, error, "intervention lines")),
    };
    let metadata = match attachments
        .list(&AttachmentOwner::Intervention(id.clone()))
        .await
    {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(context, error, "attachment metadata")),
    };
    let view = InterventionDetailPage::new(
        context.layout(),
        intervention,
        vehicle,
        owner,
        workspace.lines,
        metadata,
        workspace.totals,
        conflict,
    );
    Ok(responses::render(
        context.response_preference,
        status,
        view.render_page(engine)?,
        view.render_content(engine)?,
    ))
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn edit_form(
    context: BrowserRequestContext,
    SharedStore(interventions): SharedStore<InterventionService>,
    SharedStore(vehicles): SharedStore<VehicleService>,
    SharedStore(customers): SharedStore<CustomerService>,
    SharedStore(attachments): SharedStore<AttachmentService>,
    SharedStore(settings): SharedStore<BusinessSettings>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path(raw_id): Path<String>,
) -> Result<Response> {
    let id = match intervention_id(raw_id, &context) {
        Ok(value) => value,
        Err(response) => return Ok(response),
    };
    let intervention = match interventions.get(&id).await {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(&context, error, "intervention")),
    };
    if intervention.status != InterventionStatus::Draft {
        return render_detail(
            &context,
            &engine,
            &interventions,
            &vehicles,
            &customers,
            &attachments,
            &id,
            Some(
                "This intervention is locked and is shown in its authoritative read-only state."
                    .to_owned(),
            ),
            StatusCode::OK,
        )
        .await;
    }
    let vehicle = match vehicles.get(&intervention.vehicle_id).await {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(&context, error, "intervention vehicle")),
    };
    let owner = match customers.get(&vehicle.customer_id).await {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(&context, error, "vehicle owner")),
    };
    render_edit_form(
        &context,
        &engine,
        &intervention,
        &vehicle,
        &owner,
        FormState::new(form_values(&intervention, &settings)),
        None,
        StatusCode::OK,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn update(
    context: BrowserRequestContext,
    SharedStore(interventions): SharedStore<InterventionService>,
    SharedStore(vehicles): SharedStore<VehicleService>,
    SharedStore(customers): SharedStore<CustomerService>,
    SharedStore(attachments): SharedStore<AttachmentService>,
    SharedStore(settings): SharedStore<BusinessSettings>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path(raw_id): Path<String>,
    form: AuthenticatedForm<InterventionFormValues>,
) -> Result<Response> {
    let id = match intervention_id(raw_id, &context) {
        Ok(value) => value,
        Err(response) => return Ok(response),
    };
    let current = match interventions.get(&id).await {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(&context, error, "intervention")),
    };
    if current.status != InterventionStatus::Draft {
        return render_detail(
            &context,
            &engine,
            &interventions,
            &vehicles,
            &customers,
            &attachments,
            &id,
            Some("This intervention changed state before the edit was submitted. Authoritative read-only details are shown; the update was not repeated.".to_owned()),
            StatusCode::CONFLICT,
        )
        .await;
    }
    let vehicle = match vehicles.get(&current.vehicle_id).await {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(&context, error, "intervention vehicle")),
    };
    let owner = match customers.get(&vehicle.customer_id).await {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(&context, error, "vehicle owner")),
    };
    let values = form.fields;
    let command = match update_command(&values, &settings) {
        Ok(value) => value,
        Err(errors) => {
            return render_edit_form(
                &context,
                &engine,
                &current,
                &vehicle,
                &owner,
                FormState::with_validation(values, &errors),
                None,
                StatusCode::UNPROCESSABLE_ENTITY,
            )
        }
    };
    match interventions.update(&id, command).await {
        Ok(_) => Ok(responses::redirect(
            context.response_preference,
            &format!("/interventions/{}", id.as_str()),
        )),
        Err(WorkflowError::Validation(errors)) if mileage_error(&errors) => render_edit_form(
            &context,
            &engine,
            &current,
            &vehicle,
            &owner,
            FormState::with_validation(values, &errors),
            Some("This mileage does not fit the vehicle's dated service history. Review the neighboring records; no intervention was changed.".to_owned()),
            StatusCode::CONFLICT,
        ),
        Err(WorkflowError::Validation(errors)) => render_edit_form(
            &context,
            &engine,
            &current,
            &vehicle,
            &owner,
            FormState::with_validation(values, &errors),
            None,
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        Err(WorkflowError::Conflict) => render_detail(
            &context,
            &engine,
            &interventions,
            &vehicles,
            &customers,
            &attachments,
            &id,
            Some("This intervention changed while it was being saved. Authoritative details are shown; the update was not repeated.".to_owned()),
            StatusCode::CONFLICT,
        )
        .await,
        Err(error) => Ok(workflow_response(&context, error, "intervention")),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render_edit_form(
    context: &BrowserRequestContext,
    engine: &TeraView,
    intervention: &Intervention,
    vehicle: &Vehicle,
    owner: &crate::models::customer::Customer,
    form: FormState<InterventionFormValues>,
    conflict: Option<String>,
    status: StatusCode,
) -> Result<Response> {
    let view = InterventionFormPage::edit(
        context.layout(),
        intervention.id.as_str(),
        vehicle,
        owner,
        intervention.currency.as_str(),
        form.with_known_fields(FORM_FIELDS),
        conflict,
    );
    Ok(responses::render(
        context.response_preference,
        status,
        view.render_page(engine)?,
        view.render_form(engine)?,
    ))
}

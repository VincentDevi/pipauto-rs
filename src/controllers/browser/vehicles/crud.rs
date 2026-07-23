use super::forms::*;
use super::*;
pub(super) async fn list(
    context: BrowserRequestContext,
    SharedStore(vehicles): SharedStore<VehicleService>,
    SharedStore(customers): SharedStore<CustomerService>,
    SharedStore(settings): SharedStore<BusinessSettings>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Query(filters): Query<VehicleFilterValues>,
) -> Result<Response> {
    let filter_customers = match customers_for_filter(&customers).await {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(&context, error, "vehicle list")),
    };
    let parsed = parse_vehicle_filter(&filters);
    let filter = match parsed {
        Ok(filter) => filter,
        Err(message) => {
            return render_vehicle_list(
                &context,
                &engine,
                &customers,
                filters,
                Page {
                    items: Vec::new(),
                    next_cursor: None,
                },
                filter_customers,
                Some(message),
                StatusCode::UNPROCESSABLE_ENTITY,
            )
            .await;
        }
    };
    let cursor = match parse_cursor(&filters.cursor) {
        Ok(value) => value,
        Err(message) => {
            return render_vehicle_list(
                &context,
                &engine,
                &customers,
                filters,
                Page {
                    items: Vec::new(),
                    next_cursor: None,
                },
                filter_customers,
                Some(message),
                StatusCode::UNPROCESSABLE_ENTITY,
            )
            .await;
        }
    };
    let page = match vehicles
        .list(PageRequest {
            filter,
            limit: settings.default_collection_limit(),
            after: cursor,
        })
        .await
    {
        Ok(value) => value,
        Err(WorkflowError::Validation(_)) => {
            return render_vehicle_list(
                &context,
                &engine,
                &customers,
                filters,
                Page {
                    items: Vec::new(),
                    next_cursor: None,
                },
                filter_customers,
                Some("This page link does not match the current vehicle filters.".to_owned()),
                StatusCode::UNPROCESSABLE_ENTITY,
            )
            .await;
        }
        Err(error) => return Ok(workflow_response(&context, error, "vehicle list")),
    };
    render_vehicle_list(
        &context,
        &engine,
        &customers,
        filters,
        page,
        filter_customers,
        None,
        StatusCode::OK,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn render_vehicle_list(
    context: &BrowserRequestContext,
    engine: &TeraView,
    customers: &CustomerService,
    mut filters: VehicleFilterValues,
    page: Page<crate::models::vehicle::Vehicle>,
    active_customers: Vec<crate::models::customer::Customer>,
    filter_error: Option<String>,
    status: StatusCode,
) -> Result<Response> {
    let mut owners = Vec::with_capacity(page.items.len());
    for vehicle in &page.items {
        match customers.get(&vehicle.customer_id).await {
            Ok(owner) => owners.push(owner),
            Err(error) => return Ok(workflow_response(context, error, "vehicle owner")),
        }
    }
    let next_href = page.next_cursor.as_ref().map(|cursor| {
        filters.cursor = cursor.as_str().to_owned();
        vehicle_list_href(&filters)
    });
    filters.cursor.clear();
    let view = VehicleListPage::new(
        context.layout(),
        filters,
        page,
        owners,
        next_href,
        filter_error,
        active_customers,
    );
    Ok(responses::render(
        context.response_preference,
        status,
        view.render_page(engine)?,
        view.render_content(engine)?,
    ))
}

pub(super) async fn generic_new_form(
    context: BrowserRequestContext,
    SharedStore(customers): SharedStore<CustomerService>,
    ViewEngine(engine): ViewEngine<TeraView>,
) -> Result<Response> {
    let customers = match active_customers(&customers).await {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(&context, error, "customer selection")),
    };
    render_create_form(
        &context,
        &engine,
        FormState::new(VehicleFormValues::default()),
        customers,
        false,
        "/vehicles".to_owned(),
        None,
        StatusCode::OK,
    )
}

pub(super) async fn customer_new_form(
    context: BrowserRequestContext,
    SharedStore(customers): SharedStore<CustomerService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path(raw_id): Path<String>,
) -> Result<Response> {
    let id = match CustomerId::parse(raw_id) {
        Ok(value) => value,
        Err(_) => {
            return Ok(responses::not_found(
                context.response_preference,
                "customer",
            ))
        }
    };
    let customer = match customers.get(&id).await {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(&context, error, "customer")),
    };
    if customer.is_archived() {
        return Ok(responses::redirect(
            context.response_preference,
            &format!("/customers/{}", id.as_str()),
        ));
    }
    let values = VehicleFormValues {
        customer_id: id.as_str().to_owned(),
        ..VehicleFormValues::default()
    };
    render_create_form(
        &context,
        &engine,
        FormState::new(values),
        vec![customer],
        true,
        format!("/customers/{}", id.as_str()),
        None,
        StatusCode::OK,
    )
}

pub(super) async fn create(
    context: BrowserRequestContext,
    SharedStore(vehicles): SharedStore<VehicleService>,
    SharedStore(customers): SharedStore<CustomerService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    form: AuthenticatedForm<VehicleFormValues>,
) -> Result<Response> {
    let values = form.fields;
    let customer_options = match active_customers(&customers).await {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(&context, error, "customer selection")),
    };
    let owner_locked =
        customer_options.len() == 1 && customer_options[0].id.as_str() == values.customer_id;
    let cancel_href = if owner_locked {
        format!("/customers/{}", values.customer_id)
    } else {
        "/vehicles".to_owned()
    };
    let command = match vehicle_create_command(&values) {
        Ok(value) => value,
        Err(errors) => {
            return render_create_form(
                &context,
                &engine,
                FormState::with_validation(values, &errors),
                customer_options,
                owner_locked,
                cancel_href,
                None,
                StatusCode::UNPROCESSABLE_ENTITY,
            )
        }
    };
    match vehicles.create(command).await {
        Ok(vehicle) => Ok(responses::redirect(
            context.response_preference,
            &format!("/vehicles/{}", vehicle.id.as_str()),
        )),
        Err(WorkflowError::Validation(errors)) => render_create_form(
            &context,
            &engine,
            FormState::with_validation(values, &errors),
            customer_options,
            owner_locked,
            cancel_href,
            None,
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        Err(WorkflowError::Conflict) => render_create_form(
            &context,
            &engine,
            FormState::new(values),
            customer_options,
            owner_locked,
            cancel_href,
            Some("The selected customer is no longer active, or the registration/VIN is already used. Review the preserved values.".to_owned()),
            StatusCode::CONFLICT,
        ),
        Err(error) => Ok(workflow_response(&context, error, "vehicle")),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render_create_form(
    context: &BrowserRequestContext,
    engine: &TeraView,
    form: FormState<VehicleFormValues>,
    customers: Vec<crate::models::customer::Customer>,
    owner_locked: bool,
    cancel_href: String,
    conflict: Option<String>,
    status: StatusCode,
) -> Result<Response> {
    let view = VehicleFormPage::create(
        context.layout(),
        form.with_known_fields(VEHICLE_FORM_FIELDS),
        customers,
        owner_locked,
        cancel_href,
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
    SharedStore(vehicles): SharedStore<VehicleService>,
    SharedStore(customers): SharedStore<CustomerService>,
    SharedStore(interventions): SharedStore<InterventionService>,
    SharedStore(attachments): SharedStore<AttachmentService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path(raw_id): Path<String>,
) -> Result<Response> {
    let id = match VehicleId::parse(raw_id) {
        Ok(value) => value,
        Err(_) => return Ok(responses::not_found(context.response_preference, "vehicle")),
    };
    render_detail(
        &context,
        &engine,
        &vehicles,
        &customers,
        &interventions,
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
    vehicles: &VehicleService,
    customers: &CustomerService,
    interventions: &InterventionService,
    attachments: &AttachmentService,
    id: &VehicleId,
    lifecycle_message: Option<String>,
    status: StatusCode,
) -> Result<Response> {
    let vehicle = match vehicles.get(id).await {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(context, error, "vehicle")),
    };
    let owner = match customers.get(&vehicle.customer_id).await {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(context, error, "vehicle owner")),
    };
    let history = match interventions
        .service_history(
            id,
            PageRequest {
                filter: InterventionFilter::default(),
                limit: PageLimit::new(5).expect("five is a valid page limit"),
                after: None,
            },
        )
        .await
    {
        Ok(value) => value.items,
        Err(error) => return Ok(workflow_response(context, error, "service history")),
    };
    let attachment_values = match attachments
        .list(&AttachmentOwner::Vehicle(id.clone()))
        .await
    {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(context, error, "attachment metadata")),
    };
    let view = VehicleDetailPage::new(
        context.layout(),
        vehicle,
        owner,
        history,
        attachment_values,
        lifecycle_message,
    );
    Ok(responses::render(
        context.response_preference,
        status,
        view.render_page(engine)?,
        view.render_content(engine)?,
    ))
}

pub(super) async fn edit_form(
    context: BrowserRequestContext,
    SharedStore(vehicles): SharedStore<VehicleService>,
    SharedStore(customers): SharedStore<CustomerService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path(raw_id): Path<String>,
) -> Result<Response> {
    let id = match VehicleId::parse(raw_id) {
        Ok(value) => value,
        Err(_) => return Ok(responses::not_found(context.response_preference, "vehicle")),
    };
    let vehicle = match vehicles.get(&id).await {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(&context, error, "vehicle")),
    };
    if vehicle.is_archived() {
        return Ok(responses::redirect(
            context.response_preference,
            &format!("/vehicles/{}", id.as_str()),
        ));
    }
    let owner = match customers.get(&vehicle.customer_id).await {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(&context, error, "vehicle owner")),
    };
    render_edit_form(
        &context,
        &engine,
        id.as_str(),
        FormState::new(vehicle.into()),
        owner,
        None,
        StatusCode::OK,
    )
}

pub(super) async fn update(
    context: BrowserRequestContext,
    SharedStore(vehicles): SharedStore<VehicleService>,
    SharedStore(customers): SharedStore<CustomerService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path(raw_id): Path<String>,
    form: AuthenticatedForm<VehicleFormValues>,
) -> Result<Response> {
    let id = match VehicleId::parse(raw_id) {
        Ok(value) => value,
        Err(_) => return Ok(responses::not_found(context.response_preference, "vehicle")),
    };
    let current = match vehicles.get(&id).await {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(&context, error, "vehicle")),
    };
    if current.is_archived() {
        return Ok(responses::redirect(
            context.response_preference,
            &format!("/vehicles/{}", id.as_str()),
        ));
    }
    let owner = match customers.get(&current.customer_id).await {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(&context, error, "vehicle owner")),
    };
    let values = form.fields;
    let command = match vehicle_update_command(&values) {
        Ok(value) => value,
        Err(errors) => {
            return render_edit_form(
                &context,
                &engine,
                id.as_str(),
                FormState::with_validation(values, &errors),
                owner,
                None,
                StatusCode::UNPROCESSABLE_ENTITY,
            )
        }
    };
    match vehicles.update(&id, command).await {
        Ok(_) => Ok(responses::redirect(
            context.response_preference,
            &format!("/vehicles/{}", id.as_str()),
        )),
        Err(WorkflowError::Validation(errors)) => render_edit_form(
            &context,
            &engine,
            id.as_str(),
            FormState::with_validation(values, &errors),
            owner,
            None,
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        Err(WorkflowError::Conflict) => render_edit_form(
            &context,
            &engine,
            id.as_str(),
            FormState::new(values),
            owner,
            Some(
                "The registration or VIN is already used. Review the preserved values.".to_owned(),
            ),
            StatusCode::CONFLICT,
        ),
        Err(error) => Ok(workflow_response(&context, error, "vehicle")),
    }
}

pub(super) fn render_edit_form(
    context: &BrowserRequestContext,
    engine: &TeraView,
    id: &str,
    form: FormState<VehicleFormValues>,
    owner: crate::models::customer::Customer,
    conflict: Option<String>,
    status: StatusCode,
) -> Result<Response> {
    let view = VehicleFormPage::edit(
        context.layout(),
        id,
        form.with_known_fields(VEHICLE_FORM_FIELDS),
        owner,
        conflict,
    );
    Ok(responses::render(
        context.response_preference,
        status,
        view.render_page(engine)?,
        view.render_form(engine)?,
    ))
}

pub(super) async fn archive(
    context: BrowserRequestContext,
    SharedStore(vehicles): SharedStore<VehicleService>,
    Path(raw_id): Path<String>,
    _form: AuthenticatedForm<EmptyForm>,
) -> Result<Response> {
    lifecycle(context, vehicles, raw_id, true).await
}

pub(super) async fn restore(
    context: BrowserRequestContext,
    SharedStore(vehicles): SharedStore<VehicleService>,
    Path(raw_id): Path<String>,
    _form: AuthenticatedForm<EmptyForm>,
) -> Result<Response> {
    lifecycle(context, vehicles, raw_id, false).await
}

pub(super) async fn lifecycle(
    context: BrowserRequestContext,
    vehicles: VehicleService,
    raw_id: String,
    archiving: bool,
) -> Result<Response> {
    let id = match VehicleId::parse(raw_id) {
        Ok(value) => value,
        Err(_) => return Ok(responses::not_found(context.response_preference, "vehicle")),
    };
    let result = if archiving {
        vehicles.archive(&id).await
    } else {
        vehicles.restore(&id).await
    };
    match result {
        Ok(_) => Ok(responses::redirect(
            context.response_preference,
            &format!("/vehicles/{}", id.as_str()),
        )),
        Err(WorkflowError::Conflict) => Ok(responses::redirect(
            context.response_preference,
            &format!("/vehicles/{}", id.as_str()),
        )),
        Err(error) => Ok(workflow_response(&context, error, "vehicle")),
    }
}

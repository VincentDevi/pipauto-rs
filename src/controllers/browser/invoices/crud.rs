use super::forms::*;
use super::*;
pub(super) async fn list(
    context: BrowserRequestContext,
    SharedStore(invoices): SharedStore<InvoiceService>,
    SharedStore(customers): SharedStore<CustomerService>,
    SharedStore(settings): SharedStore<BusinessSettings>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Query(filters): Query<InvoiceFilterValues>,
) -> Result<Response> {
    let filter = match invoice_filter(&filters.status) {
        Ok(value) => value,
        Err(message) => {
            return render_list(
                &context,
                &engine,
                &customers,
                filters,
                empty_page(),
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
                &customers,
                filters,
                empty_page(),
                None,
                Some(message),
                StatusCode::UNPROCESSABLE_ENTITY,
            )
            .await;
        }
    };
    let page = match invoices
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
                &customers,
                filters,
                empty_page(),
                None,
                Some("This page link does not match the current invoice lifecycle filter.".into()),
                StatusCode::UNPROCESSABLE_ENTITY,
            )
            .await;
        }
        Err(error) => return Ok(workflow_response(&context, error, "invoice list")),
    };
    let next_href = page.next_cursor.as_ref().map(|cursor| {
        let mut next = filters.clone();
        next.cursor = cursor.as_str().to_owned();
        list_href(&next)
    });
    render_list(
        &context,
        &engine,
        &customers,
        filters,
        page,
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
    customers: &CustomerService,
    filters: InvoiceFilterValues,
    page: Page<InvoiceView>,
    next_href: Option<String>,
    filter_error: Option<String>,
    status: StatusCode,
) -> Result<Response> {
    let mut draft_customers = Vec::with_capacity(page.items.len());
    for item in &page.items {
        if item.invoice.invoice.status == InvoiceStatus::Draft {
            match customers.get(&item.invoice.invoice.customer_id).await {
                Ok(customer) => draft_customers.push(Some(customer)),
                Err(error) => return Ok(workflow_response(context, error, "invoice customer")),
            }
        } else {
            draft_customers.push(None);
        }
    }
    let view = InvoiceListPage::new(
        context.layout(),
        filters,
        page,
        draft_customers,
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
    SharedStore(customers): SharedStore<CustomerService>,
    SharedStore(vehicles): SharedStore<VehicleService>,
    SharedStore(interventions): SharedStore<InterventionService>,
    SharedStore(settings): SharedStore<BusinessSettings>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Query(query): Query<NewInvoiceQuery>,
) -> Result<Response> {
    let (values, conflict) = prefill(&customers, &vehicles, &interventions, &settings, query).await;
    render_form(
        &context,
        &engine,
        &customers,
        &vehicles,
        &interventions,
        None,
        FormState::new(values),
        conflict,
        StatusCode::OK,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn create(
    context: BrowserRequestContext,
    SharedStore(invoices): SharedStore<InvoiceService>,
    SharedStore(customers): SharedStore<CustomerService>,
    SharedStore(vehicles): SharedStore<VehicleService>,
    SharedStore(interventions): SharedStore<InterventionService>,
    SharedStore(settings): SharedStore<BusinessSettings>,
    ViewEngine(engine): ViewEngine<TeraView>,
    form: AuthenticatedForm<InvoiceFormValues>,
) -> Result<Response> {
    let values = form.fields;
    let command = match create_command(&values, settings.default_currency()) {
        Ok(value) => value,
        Err(errors) => {
            return render_form(
                &context,
                &engine,
                &customers,
                &vehicles,
                &interventions,
                None,
                FormState::with_validation(values, &errors),
                None,
                StatusCode::UNPROCESSABLE_ENTITY,
            )
            .await;
        }
    };
    match invoices.create(command).await {
        Ok(invoice) => Ok(responses::redirect(
            context.response_preference,
            &format!("/invoices/{}", invoice.invoice.id.as_str()),
        )),
        Err(WorkflowError::Validation(errors)) => {
            render_form(
                &context,
                &engine,
                &customers,
                &vehicles,
                &interventions,
                None,
                FormState::with_validation(values, &errors),
                None,
                StatusCode::UNPROCESSABLE_ENTITY,
            )
            .await
        }
        Err(WorkflowError::Conflict | WorkflowError::NotFound) => {
            render_form(
                &context,
                &engine,
                &customers,
                &vehicles,
                &interventions,
                None,
                FormState::new(values),
                Some("The selected customer, vehicle, or intervention relationship changed. Your selections were preserved; choose a currently valid combination.".into()),
                StatusCode::CONFLICT,
            )
            .await
        }
        Err(error) => Ok(workflow_response(&context, error, "invoice draft")),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn render_form(
    context: &BrowserRequestContext,
    engine: &TeraView,
    customers: &CustomerService,
    vehicles: &VehicleService,
    interventions: &InterventionService,
    invoice_id: Option<&str>,
    form: FormState<InvoiceFormValues>,
    conflict: Option<String>,
    status: StatusCode,
) -> Result<Response> {
    let customers = match all_customers(customers).await {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(context, error, "customer choices")),
    };
    let vehicles = match all_vehicles(vehicles).await {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(context, error, "vehicle choices")),
    };
    let interventions = match all_interventions(interventions).await {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(context, error, "intervention choices")),
    };
    let view = InvoiceFormPage::new(
        context.layout(),
        invoice_id,
        form.with_known_fields(FORM_FIELDS),
        customers,
        vehicles,
        interventions,
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
    SharedStore(invoices): SharedStore<InvoiceService>,
    SharedStore(customers): SharedStore<CustomerService>,
    SharedStore(vehicles): SharedStore<VehicleService>,
    SharedStore(interventions): SharedStore<InterventionService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path(raw_id): Path<String>,
) -> Result<Response> {
    let id = match invoice_id(raw_id, &context) {
        Ok(value) => value,
        Err(response) => return Ok(response),
    };
    render_detail(
        &context,
        &engine,
        &invoices,
        &customers,
        &vehicles,
        &interventions,
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
    invoices: &InvoiceService,
    customers: &CustomerService,
    vehicles: &VehicleService,
    interventions: &InterventionService,
    id: &InvoiceId,
    conflict: Option<String>,
    status: StatusCode,
) -> Result<Response> {
    let view = match invoices.get(id).await {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(context, error, "invoice")),
    };
    let customer = match customers.get(&view.invoice.invoice.customer_id).await {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(context, error, "invoice customer")),
    };
    let vehicle = match &view.invoice.invoice.vehicle_id {
        Some(id) => match vehicles.get(id).await {
            Ok(value) => Some(value),
            Err(error) => return Ok(workflow_response(context, error, "invoice vehicle")),
        },
        None => None,
    };
    let intervention = match &view.invoice.invoice.intervention_id {
        Some(id) => match interventions.get(id).await {
            Ok(value) => Some(value),
            Err(error) => return Ok(workflow_response(context, error, "invoice intervention")),
        },
        None => None,
    };
    let page = InvoiceDetailPage::new(
        context.layout(),
        view,
        customer,
        vehicle,
        intervention,
        &context.actor_id,
        &context.current_user.display_name,
        conflict,
    );
    Ok(responses::render(
        context.response_preference,
        status,
        page.render_page(engine)?,
        page.render_content(engine)?,
    ))
}

pub(super) async fn edit_form(
    context: BrowserRequestContext,
    SharedStore(invoices): SharedStore<InvoiceService>,
    SharedStore(customers): SharedStore<CustomerService>,
    SharedStore(vehicles): SharedStore<VehicleService>,
    SharedStore(interventions): SharedStore<InterventionService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path(raw_id): Path<String>,
) -> Result<Response> {
    let id = match invoice_id(raw_id, &context) {
        Ok(value) => value,
        Err(response) => return Ok(response),
    };
    let invoice = match invoices.get(&id).await {
        Ok(value) if value.invoice.invoice.status == InvoiceStatus::Draft => value,
        Ok(_) => {
            return Ok(responses::redirect(
                context.response_preference,
                &format!("/invoices/{}", id.as_str()),
            ))
        }
        Err(error) => return Ok(workflow_response(&context, error, "invoice")),
    };
    let values = InvoiceFormValues {
        customer_id: invoice.invoice.invoice.customer_id.as_str().to_owned(),
        vehicle_id: invoice
            .invoice
            .invoice
            .vehicle_id
            .as_ref()
            .map_or_else(String::new, |value| value.as_str().to_owned()),
        intervention_id: invoice
            .invoice
            .invoice
            .intervention_id
            .as_ref()
            .map_or_else(String::new, |value| value.as_str().to_owned()),
        currency: invoice.invoice.invoice.currency.as_str().to_owned(),
        notes: invoice.invoice.invoice.notes.unwrap_or_default(),
    };
    render_form(
        &context,
        &engine,
        &customers,
        &vehicles,
        &interventions,
        Some(id.as_str()),
        FormState::new(values),
        None,
        StatusCode::OK,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn update(
    context: BrowserRequestContext,
    SharedStore(invoices): SharedStore<InvoiceService>,
    SharedStore(customers): SharedStore<CustomerService>,
    SharedStore(vehicles): SharedStore<VehicleService>,
    SharedStore(interventions): SharedStore<InterventionService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path(raw_id): Path<String>,
    form: AuthenticatedForm<InvoiceFormValues>,
) -> Result<Response> {
    let id = match invoice_id(raw_id, &context) {
        Ok(value) => value,
        Err(response) => return Ok(response),
    };
    let values = form.fields;
    let command = match update_command(&values) {
        Ok(value) => value,
        Err(errors) => {
            return render_form(
                &context,
                &engine,
                &customers,
                &vehicles,
                &interventions,
                Some(id.as_str()),
                FormState::with_validation(values, &errors),
                None,
                StatusCode::UNPROCESSABLE_ENTITY,
            )
            .await;
        }
    };
    match invoices.update(&id, command).await {
        Ok(_) => Ok(detail_redirect(&context, &id)),
        Err(WorkflowError::Validation(errors)) => {
            render_form(
                &context,
                &engine,
                &customers,
                &vehicles,
                &interventions,
                Some(id.as_str()),
                FormState::with_validation(values, &errors),
                None,
                StatusCode::UNPROCESSABLE_ENTITY,
            )
            .await
        }
        Err(WorkflowError::Conflict | WorkflowError::NotFound) => {
            render_form(
                &context,
                &engine,
                &customers,
                &vehicles,
                &interventions,
                Some(id.as_str()),
                FormState::new(values),
                Some("The selected relationship, currency, or draft state changed. Your selections were preserved; choose authoritative values and submit again.".into()),
                StatusCode::CONFLICT,
            )
            .await
        }
        Err(error) => Ok(workflow_response(&context, error, "invoice draft")),
    }
}

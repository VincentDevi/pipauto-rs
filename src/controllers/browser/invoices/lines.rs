use super::forms::*;
use super::*;
pub(super) async fn new_line_form(
    context: BrowserRequestContext,
    SharedStore(invoices): SharedStore<InvoiceService>,
    SharedStore(interventions): SharedStore<InterventionService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path(raw_id): Path<String>,
) -> Result<Response> {
    let id = match draft_invoice(&invoices, raw_id, &context).await {
        Ok(value) => value,
        Err(response) => return Ok(response),
    };
    let lines = match invoices.list_lines(&id.invoice.id).await {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(&context, error, "invoice lines")),
    };
    let next_position = lines
        .iter()
        .map(|line| line.line.position)
        .max()
        .and_then(|position| position.checked_add(1))
        .unwrap_or(0);
    render_line_form(
        &context,
        &engine,
        &interventions,
        &id,
        None,
        FormState::new(InvoiceLineFormValues {
            quantity: "1".into(),
            position: next_position.to_string(),
            ..InvoiceLineFormValues::default()
        }),
        None,
        StatusCode::OK,
    )
    .await
}

pub(super) async fn create_line(
    context: BrowserRequestContext,
    SharedStore(invoices): SharedStore<InvoiceService>,
    SharedStore(interventions): SharedStore<InterventionService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path(raw_id): Path<String>,
    form: AuthenticatedForm<InvoiceLineFormValues>,
) -> Result<Response> {
    let invoice = match draft_invoice(&invoices, raw_id, &context).await {
        Ok(value) => value,
        Err(response) => return Ok(response),
    };
    let values = form.fields;
    let command = match line_command(&values) {
        Ok(value) => value,
        Err(errors) => {
            return render_line_form(
                &context,
                &engine,
                &interventions,
                &invoice,
                None,
                FormState::with_validation(values, &errors),
                None,
                StatusCode::UNPROCESSABLE_ENTITY,
            )
            .await;
        }
    };
    match invoices.create_line(&invoice.invoice.id, command).await {
        Ok(_) => Ok(detail_redirect(&context, &invoice.invoice.id)),
        Err(WorkflowError::Validation(errors)) => render_line_form(
            &context,
            &engine,
            &interventions,
            &invoice,
            None,
            FormState::with_validation(values, &errors),
            None,
            StatusCode::UNPROCESSABLE_ENTITY,
        )
        .await,
        Err(WorkflowError::Conflict | WorkflowError::NotFound) => render_line_form(
            &context,
            &engine,
            &interventions,
            &invoice,
            None,
            FormState::new(values),
            Some("The source line, invoice relationship, currency, position, or draft total changed. Authoritative choices were reloaded; review and submit again.".into()),
            StatusCode::CONFLICT,
        )
        .await,
        Err(error) => Ok(workflow_response(&context, error, "invoice line")),
    }
}

pub(super) async fn edit_line_form(
    context: BrowserRequestContext,
    SharedStore(invoices): SharedStore<InvoiceService>,
    SharedStore(interventions): SharedStore<InterventionService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path((raw_id, raw_line_id)): Path<(String, String)>,
) -> Result<Response> {
    let (invoice, line) = match invoice_line(&invoices, raw_id, raw_line_id, &context).await {
        Ok(value) => value,
        Err(response) => return Ok(response),
    };
    render_line_form(
        &context,
        &engine,
        &interventions,
        &invoice,
        Some(line.id.as_str()),
        FormState::new(InvoiceLineFormValues::from(&line)),
        None,
        StatusCode::OK,
    )
    .await
}

pub(super) async fn update_line(
    context: BrowserRequestContext,
    SharedStore(invoices): SharedStore<InvoiceService>,
    SharedStore(interventions): SharedStore<InterventionService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path((raw_id, raw_line_id)): Path<(String, String)>,
    form: AuthenticatedForm<InvoiceLineFormValues>,
) -> Result<Response> {
    let (invoice, line) = match invoice_line(&invoices, raw_id, raw_line_id, &context).await {
        Ok(value) => value,
        Err(response) => return Ok(response),
    };
    let values = form.fields;
    let command = match line_command(&values) {
        Ok(value) => value,
        Err(errors) => {
            return render_line_form(
                &context,
                &engine,
                &interventions,
                &invoice,
                Some(line.id.as_str()),
                FormState::with_validation(values, &errors),
                None,
                StatusCode::UNPROCESSABLE_ENTITY,
            )
            .await;
        }
    };
    match invoices
        .update_line(&invoice.invoice.id, line.id.clone(), command)
        .await
    {
        Ok(_) => Ok(detail_redirect(&context, &invoice.invoice.id)),
        Err(WorkflowError::Validation(errors)) => render_line_form(
            &context,
            &engine,
            &interventions,
            &invoice,
            Some(line.id.as_str()),
            FormState::with_validation(values, &errors),
            None,
            StatusCode::UNPROCESSABLE_ENTITY,
        )
        .await,
        Err(WorkflowError::Conflict | WorkflowError::NotFound) => render_line_form(
            &context,
            &engine,
            &interventions,
            &invoice,
            Some(line.id.as_str()),
            FormState::new(values),
            Some("The source line, invoice relationship, currency, position, or draft total changed. Authoritative choices were reloaded; review and submit again.".into()),
            StatusCode::CONFLICT,
        )
        .await,
        Err(error) => Ok(workflow_response(&context, error, "invoice line")),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn render_line_form(
    context: &BrowserRequestContext,
    engine: &TeraView,
    interventions: &InterventionService,
    invoice: &InvoiceView,
    line_id: Option<&str>,
    form: FormState<InvoiceLineFormValues>,
    conflict: Option<String>,
    status: StatusCode,
) -> Result<Response> {
    let source_lines = match &invoice.invoice.invoice.intervention_id {
        Some(id) => match interventions.list_lines(id).await {
            Ok(value) => value,
            Err(error) => {
                return Ok(workflow_response(
                    context,
                    error,
                    "source intervention lines",
                ))
            }
        },
        None => Vec::new(),
    };
    let view = InvoiceLineFormPage::new(
        context.layout(),
        invoice.invoice.id.as_str(),
        line_id,
        invoice.invoice.invoice.currency.as_str(),
        form.with_known_fields(LINE_FORM_FIELDS),
        source_lines,
        conflict,
    );
    Ok(responses::render(
        context.response_preference,
        status,
        view.render_page(engine)?,
        view.render_form(engine)?,
    ))
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn delete_line(
    context: BrowserRequestContext,
    SharedStore(invoices): SharedStore<InvoiceService>,
    SharedStore(customers): SharedStore<CustomerService>,
    SharedStore(vehicles): SharedStore<VehicleService>,
    SharedStore(interventions): SharedStore<InterventionService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path((raw_id, raw_line_id)): Path<(String, String)>,
    _form: AuthenticatedForm<EmptyForm>,
) -> Result<Response> {
    mutate_line_action(
        &context,
        &engine,
        &invoices,
        &customers,
        &vehicles,
        &interventions,
        raw_id,
        raw_line_id,
        LineAction::Delete,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn move_line_up(
    context: BrowserRequestContext,
    SharedStore(invoices): SharedStore<InvoiceService>,
    SharedStore(customers): SharedStore<CustomerService>,
    SharedStore(vehicles): SharedStore<VehicleService>,
    SharedStore(interventions): SharedStore<InterventionService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path((raw_id, raw_line_id)): Path<(String, String)>,
    _form: AuthenticatedForm<EmptyForm>,
) -> Result<Response> {
    mutate_line_action(
        &context,
        &engine,
        &invoices,
        &customers,
        &vehicles,
        &interventions,
        raw_id,
        raw_line_id,
        LineAction::Move(InvoiceLineMoveDirection::Up),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn move_line_down(
    context: BrowserRequestContext,
    SharedStore(invoices): SharedStore<InvoiceService>,
    SharedStore(customers): SharedStore<CustomerService>,
    SharedStore(vehicles): SharedStore<VehicleService>,
    SharedStore(interventions): SharedStore<InterventionService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path((raw_id, raw_line_id)): Path<(String, String)>,
    _form: AuthenticatedForm<EmptyForm>,
) -> Result<Response> {
    mutate_line_action(
        &context,
        &engine,
        &invoices,
        &customers,
        &vehicles,
        &interventions,
        raw_id,
        raw_line_id,
        LineAction::Move(InvoiceLineMoveDirection::Down),
    )
    .await
}

#[derive(Clone, Copy)]
pub(super) enum LineAction {
    Delete,
    Move(InvoiceLineMoveDirection),
}

#[derive(Deserialize)]
pub(super) struct EmptyForm {}

#[allow(clippy::too_many_arguments)]
pub(super) async fn mutate_line_action(
    context: &BrowserRequestContext,
    engine: &TeraView,
    invoices: &InvoiceService,
    customers: &CustomerService,
    vehicles: &VehicleService,
    interventions: &InterventionService,
    raw_id: String,
    raw_line_id: String,
    action: LineAction,
) -> Result<Response> {
    let id = match invoice_id(raw_id, context) {
        Ok(value) => value,
        Err(response) => return Ok(response),
    };
    let line_id = match InvoiceLineId::parse(raw_line_id) {
        Ok(value) => value,
        Err(_) => {
            return Ok(responses::not_found(
                context.response_preference,
                "invoice line",
            ))
        }
    };
    let result = match action {
        LineAction::Delete => invoices.delete_line(&id, line_id).await,
        LineAction::Move(direction) => invoices.move_line(&id, line_id, direction).await,
    };
    match result {
        Ok(_) if context.response_preference == ResponsePreference::FullPage => {
            Ok(detail_redirect(context, &id))
        }
        Ok(_) => render_line_region(
            context,
            engine,
            invoices,
            customers,
            vehicles,
            interventions,
            &id,
            None,
            StatusCode::OK,
        )
        .await,
        Err(WorkflowError::Conflict | WorkflowError::NotFound) => render_line_region(
            context,
            engine,
            invoices,
            customers,
            vehicles,
            interventions,
            &id,
            Some("The invoice order, draft state, or authoritative total changed. Current values were reloaded.".into()),
            StatusCode::CONFLICT,
        )
        .await,
        Err(error) => Ok(workflow_response(context, error, "invoice line")),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn render_line_region(
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
    Ok(responses::fragment(status, page.render_lines(engine)?))
}

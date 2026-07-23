use super::forms::*;
use super::*;
pub(super) async fn issue_form(
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
    render_issue_form(
        &context,
        &engine,
        &invoices,
        &customers,
        &vehicles,
        &interventions,
        &id,
        FormState::new(IssueInvoiceFormValues {
            issue_date: chrono::Utc::now().date_naive().to_string(),
            due_date: String::new(),
        }),
        None,
        StatusCode::OK,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn issue_invoice(
    context: BrowserRequestContext,
    SharedStore(invoices): SharedStore<InvoiceService>,
    SharedStore(customers): SharedStore<CustomerService>,
    SharedStore(vehicles): SharedStore<VehicleService>,
    SharedStore(interventions): SharedStore<InterventionService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path(raw_id): Path<String>,
    form: AuthenticatedForm<IssueInvoiceFormValues>,
) -> Result<Response> {
    let id = match invoice_id(raw_id, &context) {
        Ok(value) => value,
        Err(response) => return Ok(response),
    };
    let values = form.fields;
    let command = match issue_command(&values) {
        Ok(value) => value,
        Err(errors) => {
            return render_issue_form(
                &context,
                &engine,
                &invoices,
                &customers,
                &vehicles,
                &interventions,
                &id,
                FormState::with_validation(values, &errors),
                None,
                StatusCode::UNPROCESSABLE_ENTITY,
            )
            .await;
        }
    };
    match invoices.issue(&id, command).await {
        Ok(_) => Ok(detail_redirect(&context, &id)),
        Err(WorkflowError::Conflict | WorkflowError::NotFound) => {
            render_detail(
                &context,
                &engine,
                &invoices,
                &customers,
                &vehicles,
                &interventions,
                &id,
                Some("The invoice lifecycle, lines, or authoritative total changed. Current state was reloaded; no invoice number was requested again.".into()),
                StatusCode::CONFLICT,
            )
            .await
        }
        Err(WorkflowError::Validation(errors)) => {
            render_issue_form(
                &context,
                &engine,
                &invoices,
                &customers,
                &vehicles,
                &interventions,
                &id,
                FormState::with_validation(values, &errors),
                None,
                StatusCode::UNPROCESSABLE_ENTITY,
            )
            .await
        }
        Err(error) => Ok(workflow_response(&context, error, "invoice issuance")),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn render_issue_form(
    context: &BrowserRequestContext,
    engine: &TeraView,
    invoices: &InvoiceService,
    customers: &CustomerService,
    vehicles: &VehicleService,
    interventions: &InterventionService,
    id: &InvoiceId,
    form: FormState<IssueInvoiceFormValues>,
    conflict: Option<String>,
    status: StatusCode,
) -> Result<Response> {
    let view = match invoices.get(id).await {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(context, error, "invoice")),
    };
    if view.invoice.invoice.status != InvoiceStatus::Draft || view.lines.is_empty() {
        return render_detail(
            context,
            engine,
            invoices,
            customers,
            vehicles,
            interventions,
            id,
            Some("Issuance is unavailable because the invoice is no longer an eligible non-empty draft. Current lifecycle and totals were reloaded.".into()),
            StatusCode::CONFLICT,
        )
        .await;
    }
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
    let page = IssueInvoicePage::new(
        context.layout(),
        view,
        customer,
        vehicle,
        intervention,
        form.with_known_fields(ISSUE_FORM_FIELDS),
        conflict,
    );
    Ok(responses::render(
        context.response_preference,
        status,
        page.render_page(engine)?,
        page.render_form(engine)?,
    ))
}

pub(super) async fn void_form(
    context: BrowserRequestContext,
    SharedStore(invoices): SharedStore<InvoiceService>,
    SharedStore(customers): SharedStore<CustomerService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path(raw_id): Path<String>,
) -> Result<Response> {
    let id = match invoice_id(raw_id, &context) {
        Ok(value) => value,
        Err(response) => return Ok(response),
    };
    render_void_form(
        &context,
        &engine,
        &invoices,
        &customers,
        &id,
        FormState::new(VoidInvoiceFormValues::default()),
        None,
        StatusCode::OK,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn void_invoice(
    context: BrowserRequestContext,
    SharedStore(invoices): SharedStore<InvoiceService>,
    SharedStore(customers): SharedStore<CustomerService>,
    SharedStore(vehicles): SharedStore<VehicleService>,
    SharedStore(interventions): SharedStore<InterventionService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path(raw_id): Path<String>,
    form: AuthenticatedForm<VoidInvoiceFormValues>,
) -> Result<Response> {
    let id = match invoice_id(raw_id, &context) {
        Ok(value) => value,
        Err(response) => return Ok(response),
    };
    let values = form.fields;
    let reason = match void_reason(&values) {
        Ok(value) => value,
        Err(errors) => {
            return render_void_form(
                &context,
                &engine,
                &invoices,
                &customers,
                &id,
                FormState::with_validation(values, &errors),
                None,
                StatusCode::UNPROCESSABLE_ENTITY,
            )
            .await;
        }
    };
    match invoices.void(&id, reason).await {
        Ok(_) => Ok(detail_redirect(&context, &id)),
        Err(WorkflowError::Conflict | WorkflowError::NotFound) => {
            render_detail(
                &context,
                &engine,
                &invoices,
                &customers,
                &vehicles,
                &interventions,
                &id,
                Some("Void eligibility changed. Current lifecycle, payment history, and balance were reloaded; the invoice was not voided.".into()),
                StatusCode::CONFLICT,
            )
            .await
        }
        Err(WorkflowError::Validation(errors)) => {
            render_void_form(
                &context,
                &engine,
                &invoices,
                &customers,
                &id,
                FormState::with_validation(values, &errors),
                None,
                StatusCode::UNPROCESSABLE_ENTITY,
            )
            .await
        }
        Err(error) => Ok(workflow_response(&context, error, "invoice void")),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn render_void_form(
    context: &BrowserRequestContext,
    engine: &TeraView,
    invoices: &InvoiceService,
    customers: &CustomerService,
    id: &InvoiceId,
    form: FormState<VoidInvoiceFormValues>,
    conflict: Option<String>,
    status: StatusCode,
) -> Result<Response> {
    let view = match invoices.get(id).await {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(context, error, "invoice")),
    };
    if view.invoice.invoice.status == InvoiceStatus::Void || view.paid.minor_units() != 0 {
        return Ok(detail_redirect(context, id));
    }
    let customer = match customers.get(&view.invoice.invoice.customer_id).await {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(context, error, "invoice customer")),
    };
    let page = VoidInvoicePage::new(
        context.layout(),
        view,
        customer,
        form.with_known_fields(VOID_FORM_FIELDS),
        conflict,
    );
    Ok(responses::render(
        context.response_preference,
        status,
        page.render_page(engine)?,
        page.render_form(engine)?,
    ))
}

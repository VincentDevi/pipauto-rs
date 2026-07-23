use super::forms::*;
use super::*;
pub(super) async fn payment_form(
    context: BrowserRequestContext,
    SharedStore(invoices): SharedStore<InvoiceService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path(raw_id): Path<String>,
) -> Result<Response> {
    let id = match invoice_id(raw_id, &context) {
        Ok(value) => value,
        Err(response) => return Ok(response),
    };
    render_payment_form(
        &context,
        &engine,
        &invoices,
        &id,
        FormState::new(PaymentFormValues {
            received_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M").to_string(),
            method: "cash".into(),
            ..PaymentFormValues::default()
        }),
        None,
        StatusCode::OK,
    )
    .await
}

pub(super) async fn record_payment(
    context: BrowserRequestContext,
    SharedStore(invoices): SharedStore<InvoiceService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path(raw_id): Path<String>,
    form: AuthenticatedForm<PaymentFormValues>,
) -> Result<Response> {
    let id = match invoice_id(raw_id, &context) {
        Ok(value) => value,
        Err(response) => return Ok(response),
    };
    let values = form.fields;
    let invoice = match invoices.get(&id).await {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(&context, error, "invoice")),
    };
    let command = match payment_command(&values, invoice.invoice.invoice.currency) {
        Ok(value) => value,
        Err(errors) => {
            return render_payment_form(
                &context,
                &engine,
                &invoices,
                &id,
                FormState::with_validation(values, &errors),
                None,
                StatusCode::UNPROCESSABLE_ENTITY,
            )
            .await;
        }
    };
    match invoices
        .record_payment(&id, command, context.actor_id.clone())
        .await
    {
        Ok(_) => Ok(detail_redirect(&context, &id)),
        Err(WorkflowError::Conflict | WorkflowError::NotFound) => {
            render_payment_form(
                &context,
                &engine,
                &invoices,
                &id,
                FormState::new(values),
                Some("Another payment or lifecycle change updated this invoice. The latest outstanding balance is shown; correct the amount and explicitly submit again.".into()),
                StatusCode::CONFLICT,
            )
            .await
        }
        Err(WorkflowError::Validation(errors)) => {
            render_payment_form(
                &context,
                &engine,
                &invoices,
                &id,
                FormState::with_validation(values, &errors),
                None,
                StatusCode::UNPROCESSABLE_ENTITY,
            )
            .await
        }
        Err(WorkflowError::Unavailable) => Ok(responses::unavailable(
            context.response_preference,
            "Payment outcome is uncertain. No automatic retry was made. Reload this invoice before recording another payment.",
        )),
        Err(error) => Ok(workflow_response(&context, error, "payment")),
    }
}

pub(super) async fn render_payment_form(
    context: &BrowserRequestContext,
    engine: &TeraView,
    invoices: &InvoiceService,
    id: &InvoiceId,
    form: FormState<PaymentFormValues>,
    conflict: Option<String>,
    status: StatusCode,
) -> Result<Response> {
    let view = match invoices.get(id).await {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(context, error, "invoice")),
    };
    if view.invoice.invoice.status != InvoiceStatus::Issued || view.outstanding.minor_units() == 0 {
        return Ok(detail_redirect(context, id));
    }
    let page = PaymentFormPage::new(
        context.layout(),
        view,
        form.with_known_fields(PAYMENT_FORM_FIELDS),
        conflict,
    );
    Ok(responses::render(
        context.response_preference,
        status,
        page.render_page(engine)?,
        page.render_form(engine)?,
    ))
}

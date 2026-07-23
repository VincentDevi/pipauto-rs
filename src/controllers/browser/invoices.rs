//! Server-rendered invoice discovery, draft, and ordered line workflows.

use axum::{extract::Query, http::StatusCode, response::Response};
use chrono::{NaiveDate, NaiveDateTime};
use loco_rs::{
    controller::{
        extractor::shared_store::SharedStore, views::engines::TeraView, views::ViewEngine, Routes,
    },
    prelude::{get, post, Path},
    Result,
};
use serde::Deserialize;

use crate::{
    controllers::browser::{
        context::{BrowserRequestContext, ResponsePreference},
        forms::{body_limit, AuthenticatedForm, FormState},
        responses,
    },
    domain::{
        CurrencyCode, CustomerId, InterventionId, InterventionLineId, InvoiceId, InvoiceLineId,
        OpaqueCursor, Page, PageLimit, PageRequest, Quantity, ValidationCode, ValidationError,
        ValidationErrors, VehicleId,
    },
    models::{
        customer::{ArchiveFilter, Customer, CustomerFilter, CustomerModel as CustomerService},
        intervention::{
            Intervention, InterventionFilter, InterventionModel as InterventionService,
        },
        invoice::{
            CreateInvoice, InvoiceFilter, InvoiceLineMoveDirection, InvoiceModel as InvoiceService,
            InvoiceStatus, InvoiceView, IssueInvoiceCommand, RecordPayment, UpdateInvoice,
            WriteInvoiceLine, NOTES_MAX_CHARS,
        },
        invoice_line::{DESCRIPTION_MAX_CHARS, UNIT_LABEL_MAX_CHARS},
        payment::PaymentMethod,
        vehicle::{Vehicle, VehicleFilter, VehicleModel as VehicleService},
        ModelError as WorkflowError,
    },
    settings::BusinessSettings,
    views::{
        invoice::{
            InvoiceDetailPage, InvoiceFilterValues, InvoiceFormPage, InvoiceFormValues,
            InvoiceLineFormPage, InvoiceLineFormValues, InvoiceListPage, IssueInvoiceFormValues,
            IssueInvoicePage, PaymentFormPage, PaymentFormValues, VoidInvoiceFormValues,
            VoidInvoicePage,
        },
        layout::AuthenticatedLayout,
    },
};

const FORM_FIELDS: &[&str] = &[
    "customer_id",
    "vehicle_id",
    "intervention_id",
    "currency",
    "notes",
];
const LINE_FORM_FIELDS: &[&str] = &[
    "source_intervention_line_id",
    "description",
    "quantity",
    "unit_label",
    "unit_price",
    "position",
];
const ISSUE_FORM_FIELDS: &[&str] = &["issue_date", "due_date"];
const PAYMENT_FORM_FIELDS: &[&str] = &["amount", "received_at", "method", "reference", "notes"];
const VOID_FORM_FIELDS: &[&str] = &["reason"];

#[derive(Clone, Debug, Default, Deserialize)]
struct NewInvoiceQuery {
    customer: Option<String>,
    vehicle: Option<String>,
    intervention: Option<String>,
}

async fn list(
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
async fn render_list(
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
        layout(context),
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

async fn new_form(
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
async fn create(
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
async fn render_form(
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
        layout(context),
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

async fn show(
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
async fn render_detail(
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
        layout(context),
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

async fn edit_form(
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
async fn update(
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

async fn issue_form(
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
async fn issue_invoice(
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
async fn render_issue_form(
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
        layout(context),
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

async fn payment_form(
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

async fn record_payment(
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

async fn render_payment_form(
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
        layout(context),
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

async fn void_form(
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
async fn void_invoice(
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
async fn render_void_form(
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
        layout(context),
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

async fn new_line_form(
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

async fn create_line(
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

async fn edit_line_form(
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

async fn update_line(
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
async fn render_line_form(
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
        layout(context),
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
async fn delete_line(
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
async fn move_line_up(
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
async fn move_line_down(
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
enum LineAction {
    Delete,
    Move(InvoiceLineMoveDirection),
}

#[derive(Deserialize)]
struct EmptyForm {}

#[allow(clippy::too_many_arguments)]
async fn mutate_line_action(
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
async fn render_line_region(
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
        layout(context),
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

fn create_command(
    values: &InvoiceFormValues,
    authoritative_currency: CurrencyCode,
) -> std::result::Result<CreateInvoice, ValidationErrors> {
    let (customer_id, vehicle_id, intervention_id, currency) = parsed_header(values)?;
    if currency != authoritative_currency {
        return Err(ValidationErrors::one(validation_error(
            "currency",
            "Use the authoritative workshop currency.",
        )));
    }
    Ok(CreateInvoice {
        customer_id,
        vehicle_id,
        intervention_id,
        currency,
        notes: optional_text(&values.notes),
    })
}

fn issue_command(
    values: &IssueInvoiceFormValues,
) -> std::result::Result<IssueInvoiceCommand, ValidationErrors> {
    let mut errors = Vec::new();
    let issue_date = NaiveDate::parse_from_str(&values.issue_date, "%Y-%m-%d").map_err(|_| {
        errors.push(validation_error("issue_date", "Enter a valid issue date."));
    });
    let due_date = if values.due_date.trim().is_empty() {
        Ok(None)
    } else {
        NaiveDate::parse_from_str(&values.due_date, "%Y-%m-%d")
            .map(Some)
            .map_err(|_| {
                errors.push(validation_error("due_date", "Enter a valid due date."));
            })
    };
    if let (Ok(issue_date), Ok(Some(due_date))) = (&issue_date, &due_date) {
        if due_date < issue_date {
            errors.push(validation_error(
                "due_date",
                "Due date cannot precede the issue date.",
            ));
        }
    }
    if let Some(errors) = ValidationErrors::from_vec(errors) {
        return Err(errors);
    }
    Ok(IssueInvoiceCommand {
        issue_date: issue_date.expect("validated issue date"),
        due_date: due_date.expect("validated due date"),
    })
}

fn payment_command(
    values: &PaymentFormValues,
    currency: CurrencyCode,
) -> std::result::Result<RecordPayment, ValidationErrors> {
    let mut errors = Vec::new();
    let amount_minor =
        parse_money_input(&values.amount).and_then(
            |amount| {
                if amount > 0 {
                    Ok(amount)
                } else {
                    Err(())
                }
            },
        );
    if amount_minor.is_err() {
        errors.push(validation_error(
            "amount",
            "Enter a positive amount with at most two decimal places.",
        ));
    }
    let received_at = NaiveDateTime::parse_from_str(&values.received_at, "%Y-%m-%dT%H:%M")
        .map(|received_at| received_at.and_utc());
    if received_at.is_err() {
        errors.push(validation_error(
            "received_at",
            "Enter the received date and time in UTC.",
        ));
    }
    let method = match values.method.as_str() {
        "cash" => Ok(PaymentMethod::Cash),
        "bank_transfer" => Ok(PaymentMethod::BankTransfer),
        "card" => Ok(PaymentMethod::Card),
        "other" => Ok(PaymentMethod::Other),
        _ => Err(()),
    };
    if method.is_err() {
        errors.push(validation_error("method", "Choose a payment method."));
    }
    if let Some(errors) = ValidationErrors::from_vec(errors) {
        return Err(errors);
    }
    Ok(RecordPayment {
        amount_minor: amount_minor.expect("validated amount"),
        currency,
        received_at: received_at.expect("validated received time"),
        method: method.expect("validated payment method"),
        reference: optional_text(&values.reference),
        notes: optional_text(&values.notes),
    })
}

fn void_reason(values: &VoidInvoiceFormValues) -> std::result::Result<String, ValidationErrors> {
    let value = values.reason.trim();
    if value.is_empty() {
        return Err(ValidationErrors::one(validation_error(
            "reason",
            "Enter the reason for voiding this invoice.",
        )));
    }
    if value.chars().count() > NOTES_MAX_CHARS {
        return Err(ValidationErrors::one(validation_error(
            "reason",
            "Use 10,000 characters or fewer.",
        )));
    }
    Ok(value.to_owned())
}

fn update_command(
    values: &InvoiceFormValues,
) -> std::result::Result<UpdateInvoice, ValidationErrors> {
    let (customer_id, vehicle_id, intervention_id, currency) = parsed_header(values)?;
    Ok(UpdateInvoice {
        customer_id: Some(customer_id),
        vehicle_id: Some(vehicle_id),
        intervention_id: Some(intervention_id),
        currency: Some(currency),
        notes: Some(optional_text(&values.notes)),
    })
}

fn parsed_header(
    values: &InvoiceFormValues,
) -> std::result::Result<
    (
        CustomerId,
        Option<VehicleId>,
        Option<InterventionId>,
        CurrencyCode,
    ),
    ValidationErrors,
> {
    let mut errors = Vec::new();
    let customer_id = CustomerId::parse(values.customer_id.clone()).map_err(|_| {
        errors.push(validation_error(
            "customer_id",
            "Choose an active customer.",
        ));
    });
    let vehicle_id = optional_id(&values.vehicle_id, VehicleId::parse).map_err(|_| {
        errors.push(validation_error("vehicle_id", "Choose a valid vehicle."));
    });
    let intervention_id =
        optional_id(&values.intervention_id, InterventionId::parse).map_err(|_| {
            errors.push(validation_error(
                "intervention_id",
                "Choose a valid intervention.",
            ));
        });
    let currency = CurrencyCode::parse(&values.currency).map_err(|_| {
        errors.push(validation_error(
            "currency",
            "Use the authoritative workshop currency.",
        ));
    });
    if values.notes.trim().chars().count() > 10_000 {
        errors.push(validation_error("notes", "Use 10,000 characters or fewer."));
    }
    if let Some(errors) = ValidationErrors::from_vec(errors) {
        return Err(errors);
    }
    Ok((
        customer_id.expect("validated customer"),
        vehicle_id.expect("validated vehicle"),
        intervention_id.expect("validated intervention"),
        currency.expect("validated currency"),
    ))
}

fn line_command(
    values: &InvoiceLineFormValues,
) -> std::result::Result<WriteInvoiceLine, ValidationErrors> {
    let mut errors = Vec::new();
    let source_intervention_line_id = optional_id(
        &values.source_intervention_line_id,
        InterventionLineId::parse,
    )
    .map_err(|_| {
        errors.push(validation_error(
            "source_intervention_line_id",
            "Choose a source line from the related intervention.",
        ));
    });
    validate_required_length(
        &mut errors,
        "description",
        &values.description,
        DESCRIPTION_MAX_CHARS,
        "Enter a line description.",
        "Use 500 characters or fewer.",
    );
    validate_required_length(
        &mut errors,
        "unit_label",
        &values.unit_label,
        UNIT_LABEL_MAX_CHARS,
        "Enter a unit label.",
        "Use 32 characters or fewer.",
    );
    let quantity = Quantity::parse(&values.quantity).map_err(|_| {
        errors.push(validation_error(
            "quantity",
            "Enter a positive quantity with up to three decimal places.",
        ));
    });
    let unit_price_minor = parse_money_input(&values.unit_price).map_err(|_| {
        errors.push(validation_error(
            "unit_price",
            "Enter a non-negative amount with at most two decimal places.",
        ));
    });
    let position = values.position.parse::<u32>().map_err(|_| {
        errors.push(validation_error(
            "position",
            "Enter a non-negative whole-number position.",
        ));
    });
    if let Some(errors) = ValidationErrors::from_vec(errors) {
        return Err(errors);
    }
    Ok(WriteInvoiceLine {
        source_intervention_line_id: source_intervention_line_id.expect("validated source line"),
        description: values.description.clone(),
        quantity: quantity.expect("validated quantity"),
        unit_label: values.unit_label.clone(),
        unit_price_minor: unit_price_minor.expect("validated unit price"),
        position: position.expect("validated position"),
    })
}

async fn prefill(
    customers: &CustomerService,
    vehicles: &VehicleService,
    interventions: &InterventionService,
    settings: &BusinessSettings,
    query: NewInvoiceQuery,
) -> (InvoiceFormValues, Option<String>) {
    let mut values = InvoiceFormValues {
        customer_id: query.customer.unwrap_or_default(),
        vehicle_id: query.vehicle.unwrap_or_default(),
        intervention_id: query.intervention.unwrap_or_default(),
        currency: settings.default_currency().as_str().to_owned(),
        notes: String::new(),
    };
    let result = async {
        if !values.intervention_id.is_empty() {
            let id = InterventionId::parse(values.intervention_id.clone()).map_err(|_| ())?;
            let intervention = interventions.get(&id).await.map_err(|_| ())?;
            values.vehicle_id = intervention.vehicle_id.as_str().to_owned();
        }
        if !values.vehicle_id.is_empty() {
            let id = VehicleId::parse(values.vehicle_id.clone()).map_err(|_| ())?;
            let vehicle = vehicles.get(&id).await.map_err(|_| ())?;
            values.customer_id = vehicle.customer_id.as_str().to_owned();
        }
        if !values.customer_id.is_empty() {
            let id = CustomerId::parse(values.customer_id.clone()).map_err(|_| ())?;
            let customer = customers.get(&id).await.map_err(|_| ())?;
            if customer.is_archived() {
                return Err(());
            }
        }
        Ok::<(), ()>(())
    }
    .await;
    let conflict = result.err().map(|()| {
        "The requested prefill is no longer an active, consistent relationship. Review the preserved references and choose valid records.".to_owned()
    });
    (values, conflict)
}

async fn all_customers(
    service: &CustomerService,
) -> std::result::Result<Vec<Customer>, WorkflowError> {
    Ok(service
        .list(PageRequest {
            filter: CustomerFilter {
                query: None,
                archive: ArchiveFilter::All,
            },
            limit: maximum_limit(),
            after: None,
        })
        .await?
        .items)
}

async fn all_vehicles(
    service: &VehicleService,
) -> std::result::Result<Vec<Vehicle>, WorkflowError> {
    Ok(service
        .list(PageRequest {
            filter: VehicleFilter {
                archive: ArchiveFilter::All,
                ..VehicleFilter::default()
            },
            limit: maximum_limit(),
            after: None,
        })
        .await?
        .items)
}

async fn all_interventions(
    service: &InterventionService,
) -> std::result::Result<Vec<Intervention>, WorkflowError> {
    Ok(service
        .list(PageRequest {
            filter: InterventionFilter::default(),
            limit: maximum_limit(),
            after: None,
        })
        .await?
        .items
        .into_iter()
        .map(|summary| summary.intervention)
        .collect())
}

async fn draft_invoice(
    invoices: &InvoiceService,
    raw_id: String,
    context: &BrowserRequestContext,
) -> std::result::Result<InvoiceView, Response> {
    let id = invoice_id(raw_id, context)?;
    let invoice = invoices
        .get(&id)
        .await
        .map_err(|error| workflow_response(context, error, "invoice"))?;
    if invoice.invoice.invoice.status != InvoiceStatus::Draft {
        return Err(responses::redirect(
            context.response_preference,
            &format!("/invoices/{}", id.as_str()),
        ));
    }
    Ok(invoice)
}

async fn invoice_line(
    invoices: &InvoiceService,
    raw_id: String,
    raw_line_id: String,
    context: &BrowserRequestContext,
) -> std::result::Result<(InvoiceView, crate::models::invoice_line::InvoiceLineRecord), Response> {
    let invoice = draft_invoice(invoices, raw_id, context).await?;
    let line_id = InvoiceLineId::parse(raw_line_id)
        .map_err(|_| responses::not_found(context.response_preference, "invoice line"))?;
    let line = invoice
        .lines
        .iter()
        .find(|line| line.id == line_id)
        .cloned()
        .ok_or_else(|| responses::not_found(context.response_preference, "invoice line"))?;
    Ok((invoice, line))
}

fn invoice_filter(value: &str) -> std::result::Result<InvoiceFilter, String> {
    let status = match value {
        "" | "all" => None,
        "draft" => Some(InvoiceStatus::Draft),
        "issued" => Some(InvoiceStatus::Issued),
        "void" => Some(InvoiceStatus::Void),
        _ => return Err("Choose Draft, Issued, Void, or All.".into()),
    };
    Ok(InvoiceFilter { status })
}

fn parse_cursor(value: &str) -> std::result::Result<Option<OpaqueCursor>, String> {
    if value.is_empty() {
        Ok(None)
    } else {
        OpaqueCursor::parse(value.to_owned())
            .map(Some)
            .map_err(|_| "Use a valid invoice page link.".to_owned())
    }
}

fn list_href(filters: &InvoiceFilterValues) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    if !filters.status.is_empty() {
        serializer.append_pair("status", &filters.status);
    }
    if !filters.cursor.is_empty() {
        serializer.append_pair("cursor", &filters.cursor);
    }
    format!("/invoices?{}", serializer.finish())
}

fn optional_id<T, E>(
    value: &str,
    parse: impl FnOnce(String) -> std::result::Result<T, E>,
) -> std::result::Result<Option<T>, E> {
    if value.trim().is_empty() {
        Ok(None)
    } else {
        parse(value.to_owned()).map(Some)
    }
}

fn parse_money_input(value: &str) -> std::result::Result<i64, ()> {
    if value.is_empty() || value.trim() != value || value.starts_with('+') {
        return Err(());
    }
    let (whole, fraction) = value.split_once('.').map_or((value, ""), |parts| parts);
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.len() > 2
        || (value.contains('.') && fraction.is_empty())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        || value.matches('.').count() > 1
    {
        return Err(());
    }
    let whole = whole.parse::<i64>().map_err(|_| ())?;
    let fraction = match fraction.len() {
        0 => 0,
        1 => fraction.parse::<i64>().map_err(|_| ())? * 10,
        2 => fraction.parse::<i64>().map_err(|_| ())?,
        _ => return Err(()),
    };
    whole
        .checked_mul(100)
        .and_then(|minor| minor.checked_add(fraction))
        .ok_or(())
}

fn validate_required_length(
    errors: &mut Vec<ValidationError>,
    field: &str,
    value: &str,
    maximum: usize,
    required_message: &str,
    length_message: &str,
) {
    if value.trim().is_empty() {
        errors.push(validation_error(field, required_message));
    } else if value.trim().chars().count() > maximum {
        errors.push(validation_error(field, length_message));
    }
}

fn validation_error(field: &str, message: &str) -> ValidationError {
    ValidationError::new(field, ValidationCode::InvalidFormat, message)
        .expect("static validation metadata is valid")
}

fn optional_text(value: &str) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.to_owned())
}

fn maximum_limit() -> PageLimit {
    PageLimit::new(200).expect("maximum page limit is valid")
}

fn empty_page() -> Page<InvoiceView> {
    Page {
        items: Vec::new(),
        next_cursor: None,
    }
}

fn detail_redirect(context: &BrowserRequestContext, id: &InvoiceId) -> Response {
    responses::redirect(
        context.response_preference,
        &format!("/invoices/{}", id.as_str()),
    )
}

#[allow(clippy::result_large_err)]
fn invoice_id(
    raw_id: String,
    context: &BrowserRequestContext,
) -> std::result::Result<InvoiceId, Response> {
    InvoiceId::parse(raw_id)
        .map_err(|_| responses::not_found(context.response_preference, "invoice"))
}

fn layout(context: &BrowserRequestContext) -> AuthenticatedLayout<'_> {
    AuthenticatedLayout::new(
        &context.current_user,
        context.csrf_token.expose(),
        &context.current_path,
    )
}

fn workflow_response(
    context: &BrowserRequestContext,
    error: WorkflowError,
    resource: &str,
) -> Response {
    match error {
        WorkflowError::NotFound => responses::not_found(context.response_preference, resource),
        WorkflowError::Unavailable => responses::unavailable(
            context.response_preference,
            "Invoice information is temporarily unavailable. Try again shortly.",
        ),
        WorkflowError::Validation(_) | WorkflowError::Conflict | WorkflowError::Internal => {
            responses::unexpected(context.response_preference)
        }
    }
}

#[must_use]
pub fn routes() -> Routes {
    Routes::new()
        .add("/invoices", get(list))
        .add("/invoices", post(create).layer(body_limit()))
        .add("/invoices/new", get(new_form))
        .add("/invoices/{id}", get(show))
        .add("/invoices/{id}/issue", get(issue_form))
        .add(
            "/invoices/{id}/issue",
            post(issue_invoice).layer(body_limit()),
        )
        .add("/invoices/{id}/payments/new", get(payment_form))
        .add(
            "/invoices/{id}/payments",
            post(record_payment).layer(body_limit()),
        )
        .add("/invoices/{id}/void", get(void_form))
        .add(
            "/invoices/{id}/void",
            post(void_invoice).layer(body_limit()),
        )
        .add("/invoices/{id}/edit", get(edit_form))
        .add("/invoices/{id}/edit", post(update).layer(body_limit()))
        .add("/invoices/{id}/lines/new", get(new_line_form))
        .add(
            "/invoices/{id}/lines",
            post(create_line).layer(body_limit()),
        )
        .add("/invoices/{id}/lines/{line_id}/edit", get(edit_line_form))
        .add(
            "/invoices/{id}/lines/{line_id}/edit",
            post(update_line).layer(body_limit()),
        )
        .add(
            "/invoices/{id}/lines/{line_id}/delete",
            post(delete_line).layer(body_limit()),
        )
        .add(
            "/invoices/{id}/lines/{line_id}/move-up",
            post(move_line_up).layer(body_limit()),
        )
        .add(
            "/invoices/{id}/lines/{line_id}/move-down",
            post(move_line_down).layer(body_limit()),
        )
}

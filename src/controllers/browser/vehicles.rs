//! Server-rendered vehicle, service-history, and vehicle attachment workflows.

use axum::{
    extract::{DefaultBodyLimit, Query},
    http::StatusCode,
    response::Response,
};
use chrono::{Datelike, NaiveDate, Utc};
use loco_rs::{
    controller::{
        extractor::shared_store::SharedStore, views::engines::TeraView, views::ViewEngine, Routes,
    },
    prelude::{get, post, Path},
    Result,
};
use serde::Deserialize;

use crate::{
    auth::{csrf::AuthenticatedAttachmentMultipart, extractors::CurrentUser},
    controllers::browser::{
        context::BrowserRequestContext,
        forms::{body_limit, AuthenticatedForm, FormState},
        responses,
    },
    domain::{
        AttachmentId, CustomerId, NormalizedRegistration, NormalizedVin, OpaqueCursor, Page,
        PageLimit, PageRequest, ValidationCode, ValidationError, ValidationErrors, VehicleId,
        WorkshopTime,
    },
    errors::AppError,
    models::{
        attachment::{
            AttachmentMetadata, AttachmentOwner, CAPTION_MAX_CHARS, DISPLAY_NAME_MAX_CHARS,
        },
        intervention::InterventionStatus,
        vehicle::{
            EARLIEST_VEHICLE_YEAR, ENGINE_TYPE_MAX_CHARS, MAKE_MAX_CHARS, MODEL_MAX_CHARS,
            NOTES_MAX_CHARS, REGISTRATION_MAX_CHARS, VIN_DISPLAY_MAX_CHARS,
        },
    },
    repositories::{
        customer::{ArchiveFilter, CustomerFilter},
        intervention::InterventionFilter,
        vehicle::VehicleFilter,
    },
    services::{
        attachment::{AttachmentService, UploadAttachment, WriteAttachmentMetadata},
        customer::CustomerService,
        intervention::InterventionService,
        vehicle::{CreateVehicle, UpdateVehicle, VehicleService},
        WorkflowError,
    },
    settings::{BusinessSettings, MULTIPART_ENVELOPE_BYTES},
    views::{
        layout::AuthenticatedLayout,
        vehicle::{
            AttachmentFormPage, AttachmentFormValues, HistoryFilterValues, ReassignPage,
            ServiceHistoryPage, VehicleDetailPage, VehicleFilterValues, VehicleFormPage,
            VehicleFormValues, VehicleListPage,
        },
    },
};

const VEHICLE_FORM_FIELDS: &[&str] = &[
    "customer_id",
    "make",
    "model",
    "year",
    "registration",
    "vin",
    "current_mileage",
    "engine_type",
    "notes",
];
const ATTACHMENT_FORM_FIELDS: &[&str] = &["file", "display_name", "caption"];

async fn list(
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
async fn render_vehicle_list(
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
        layout(context),
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

async fn generic_new_form(
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

async fn customer_new_form(
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

async fn create(
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
fn render_create_form(
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
        layout(context),
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

async fn show(
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
async fn render_detail(
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
        layout(context),
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

async fn edit_form(
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

async fn update(
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

fn render_edit_form(
    context: &BrowserRequestContext,
    engine: &TeraView,
    id: &str,
    form: FormState<VehicleFormValues>,
    owner: crate::models::customer::Customer,
    conflict: Option<String>,
    status: StatusCode,
) -> Result<Response> {
    let view = VehicleFormPage::edit(
        layout(context),
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

#[derive(Deserialize)]
struct EmptyForm {}

async fn archive(
    context: BrowserRequestContext,
    SharedStore(vehicles): SharedStore<VehicleService>,
    Path(raw_id): Path<String>,
    _form: AuthenticatedForm<EmptyForm>,
) -> Result<Response> {
    lifecycle(context, vehicles, raw_id, true).await
}

async fn restore(
    context: BrowserRequestContext,
    SharedStore(vehicles): SharedStore<VehicleService>,
    Path(raw_id): Path<String>,
    _form: AuthenticatedForm<EmptyForm>,
) -> Result<Response> {
    lifecycle(context, vehicles, raw_id, false).await
}

async fn lifecycle(
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

#[derive(Default, Deserialize)]
struct ReassignQuery {
    #[serde(default)]
    customer_id: String,
}

async fn reassign_form(
    context: BrowserRequestContext,
    SharedStore(vehicles): SharedStore<VehicleService>,
    SharedStore(customers): SharedStore<CustomerService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path(raw_id): Path<String>,
    Query(query): Query<ReassignQuery>,
) -> Result<Response> {
    render_reassign(
        &context,
        &engine,
        &vehicles,
        &customers,
        raw_id,
        query.customer_id,
        None,
        StatusCode::OK,
    )
    .await
}

#[derive(Deserialize)]
struct ReassignForm {
    customer_id: String,
}

async fn reassign(
    context: BrowserRequestContext,
    SharedStore(vehicles): SharedStore<VehicleService>,
    SharedStore(customers): SharedStore<CustomerService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path(raw_id): Path<String>,
    form: AuthenticatedForm<ReassignForm>,
) -> Result<Response> {
    let id = match VehicleId::parse(raw_id.clone()) {
        Ok(value) => value,
        Err(_) => return Ok(responses::not_found(context.response_preference, "vehicle")),
    };
    let customer_id = match CustomerId::parse(form.fields.customer_id.clone()) {
        Ok(value) => value,
        Err(_) => {
            return render_reassign(
                &context,
                &engine,
                &vehicles,
                &customers,
                raw_id,
                form.fields.customer_id,
                Some("Choose an active customer.".to_owned()),
                StatusCode::UNPROCESSABLE_ENTITY,
            )
            .await
        }
    };
    match vehicles.reassign(&id, &customer_id).await {
        Ok(_) => Ok(responses::redirect(
            context.response_preference,
            &format!("/vehicles/{}", id.as_str()),
        )),
        Err(WorkflowError::Conflict) => {
            render_reassign(
                &context,
                &engine,
                &vehicles,
                &customers,
                raw_id,
                form.fields.customer_id,
                Some(
                    "The selected customer is no longer active. Ownership was not changed."
                        .to_owned(),
                ),
                StatusCode::CONFLICT,
            )
            .await
        }
        Err(error) => Ok(workflow_response(&context, error, "vehicle")),
    }
}

#[allow(clippy::too_many_arguments)]
async fn render_reassign(
    context: &BrowserRequestContext,
    engine: &TeraView,
    vehicles: &VehicleService,
    customers: &CustomerService,
    raw_id: String,
    selected_id: String,
    message: Option<String>,
    status: StatusCode,
) -> Result<Response> {
    let id = match VehicleId::parse(raw_id) {
        Ok(value) => value,
        Err(_) => return Ok(responses::not_found(context.response_preference, "vehicle")),
    };
    let vehicle = match vehicles.get(&id).await {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(context, error, "vehicle")),
    };
    if vehicle.is_archived() {
        return Ok(responses::redirect(
            context.response_preference,
            &format!("/vehicles/{}", id.as_str()),
        ));
    }
    let old_owner = match customers.get(&vehicle.customer_id).await {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(context, error, "vehicle owner")),
    };
    let options = match active_customers(customers).await {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(context, error, "customer selection")),
    };
    let selected = options
        .iter()
        .find(|customer| customer.id.as_str() == selected_id)
        .cloned();
    let message = if !selected_id.is_empty() && selected.is_none() && message.is_none() {
        Some("The selected customer is not active. Choose another customer.".to_owned())
    } else {
        message
    };
    let response_status = if message.is_some() && status == StatusCode::OK {
        StatusCode::UNPROCESSABLE_ENTITY
    } else {
        status
    };
    let view = ReassignPage::new(
        layout(context),
        &vehicle,
        old_owner,
        selected,
        options,
        message,
    );
    Ok(responses::render(
        context.response_preference,
        response_status,
        view.render_page(engine)?,
        view.render_content(engine)?,
    ))
}

async fn history(
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
fn render_history(
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
        layout(context),
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

async fn new_attachment_form(
    context: BrowserRequestContext,
    SharedStore(vehicles): SharedStore<VehicleService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path(raw_id): Path<String>,
) -> Result<Response> {
    let vehicle = match get_active_vehicle(&vehicles, raw_id, &context).await {
        Ok(value) => value,
        Err(response) => return Ok(response),
    };
    render_attachment_form(
        &context,
        &engine,
        &vehicle,
        None,
        FormState::new(AttachmentFormValues::default()),
        None,
        StatusCode::OK,
    )
}

async fn create_attachment(
    context: BrowserRequestContext,
    SharedStore(vehicles): SharedStore<VehicleService>,
    SharedStore(attachments): SharedStore<AttachmentService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path(raw_id): Path<String>,
    upload: AuthenticatedAttachmentMultipart,
) -> Result<Response> {
    let vehicle = match get_active_vehicle(&vehicles, raw_id, &context).await {
        Ok(value) => value,
        Err(response) => return Ok(response),
    };
    let values = AttachmentFormValues {
        display_name: upload.display_name.clone().unwrap_or_default(),
        caption: upload.caption.clone().unwrap_or_default(),
    };
    let command = UploadAttachment {
        bytes: upload.bytes,
        display_name: upload.display_name,
        original_filename: upload.original_filename,
        caption: upload.caption,
    };
    match attachments
        .upload(AttachmentOwner::Vehicle(vehicle.id.clone()), command)
        .await
    {
        Ok(_) => Ok(responses::redirect(
            context.response_preference,
            &format!("/vehicles/{}", vehicle.id.as_str()),
        )),
        Err(WorkflowError::Validation(errors)) => render_attachment_form(
            &context,
            &engine,
            &vehicle,
            None,
            FormState::with_validation(values, &errors),
            None,
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        Err(WorkflowError::Conflict) => render_attachment_form(
            &context,
            &engine,
            &vehicle,
            None,
            FormState::new(values),
            Some("The vehicle was archived before this file could be stored. Restore it, reselect the file, and try again.".to_owned()),
            StatusCode::CONFLICT,
        ),
        Err(error) => Ok(workflow_response(&context, error, "attachment")),
    }
}

async fn edit_attachment_form(
    context: BrowserRequestContext,
    SharedStore(vehicles): SharedStore<VehicleService>,
    SharedStore(attachments): SharedStore<AttachmentService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path(raw_id): Path<String>,
) -> Result<Response> {
    let (attachment, vehicle) =
        match vehicle_attachment(&attachments, &vehicles, raw_id, &context).await {
            Ok(value) => value,
            Err(response) => return Ok(response),
        };
    if vehicle.is_archived() {
        return Ok(responses::redirect(
            context.response_preference,
            &format!("/vehicles/{}", vehicle.id.as_str()),
        ));
    }
    let attachment_id = attachment.id.as_str().to_owned();
    render_attachment_form(
        &context,
        &engine,
        &vehicle,
        Some(&attachment_id),
        FormState::new(attachment.into()),
        None,
        StatusCode::OK,
    )
}

async fn update_attachment(
    context: BrowserRequestContext,
    SharedStore(vehicles): SharedStore<VehicleService>,
    SharedStore(attachments): SharedStore<AttachmentService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path(raw_id): Path<String>,
    form: AuthenticatedForm<AttachmentFormValues>,
) -> Result<Response> {
    let (attachment, vehicle) =
        match vehicle_attachment(&attachments, &vehicles, raw_id, &context).await {
            Ok(value) => value,
            Err(response) => return Ok(response),
        };
    if vehicle.is_archived() {
        return Ok(responses::redirect(
            context.response_preference,
            &format!("/vehicles/{}", vehicle.id.as_str()),
        ));
    }
    let attachment_id = attachment.id.as_str().to_owned();
    let values = form.fields;
    let command = match attachment_update_command(&values, &attachment) {
        Ok(value) => value,
        Err(errors) => {
            return render_attachment_form(
                &context,
                &engine,
                &vehicle,
                Some(&attachment_id),
                FormState::with_validation(values, &errors),
                None,
                StatusCode::UNPROCESSABLE_ENTITY,
            )
        }
    };
    match attachments.update(&attachment.id, command).await {
        Ok(_) => Ok(responses::redirect(
            context.response_preference,
            &format!("/vehicles/{}", vehicle.id.as_str()),
        )),
        Err(WorkflowError::Validation(errors)) => render_attachment_form(
            &context,
            &engine,
            &vehicle,
            Some(&attachment_id),
            FormState::with_validation(values, &errors),
            None,
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        Err(WorkflowError::Conflict) => render_attachment_form(
            &context,
            &engine,
            &vehicle,
            Some(&attachment_id),
            FormState::new(values),
            Some(
                "The vehicle was archived before these attachment details could be saved."
                    .to_owned(),
            ),
            StatusCode::CONFLICT,
        ),
        Err(error) => Ok(workflow_response(&context, error, "attachment")),
    }
}

async fn delete_attachment(
    context: BrowserRequestContext,
    SharedStore(vehicles): SharedStore<VehicleService>,
    SharedStore(attachments): SharedStore<AttachmentService>,
    Path(raw_id): Path<String>,
    _form: AuthenticatedForm<EmptyForm>,
) -> Result<Response> {
    let (attachment, vehicle) =
        match vehicle_attachment(&attachments, &vehicles, raw_id, &context).await {
            Ok(value) => value,
            Err(response) => return Ok(response),
        };
    if vehicle.is_archived() {
        return Ok(responses::redirect(
            context.response_preference,
            &format!("/vehicles/{}", vehicle.id.as_str()),
        ));
    }
    match attachments.delete(&attachment.id).await {
        Ok(()) => Ok(responses::redirect(
            context.response_preference,
            &format!("/vehicles/{}", vehicle.id.as_str()),
        )),
        Err(error) => Ok(workflow_response(&context, error, "attachment")),
    }
}

async fn attachment_content(
    CurrentUser(_): CurrentUser,
    SharedStore(attachments): SharedStore<AttachmentService>,
    Path(raw_id): Path<String>,
) -> std::result::Result<Response, AppError> {
    attachment_bytes(&attachments, raw_id, false).await
}

async fn attachment_download(
    CurrentUser(_): CurrentUser,
    SharedStore(attachments): SharedStore<AttachmentService>,
    Path(raw_id): Path<String>,
) -> std::result::Result<Response, AppError> {
    attachment_bytes(&attachments, raw_id, true).await
}

async fn attachment_bytes(
    attachments: &AttachmentService,
    raw_id: String,
    force_download: bool,
) -> std::result::Result<Response, AppError> {
    let id = AttachmentId::parse(raw_id).map_err(|_| AppError::NotFound)?;
    let content = attachments.content(&id).await?;
    crate::controllers::attachments::content_response(content, force_download)
}

fn render_attachment_form(
    context: &BrowserRequestContext,
    engine: &TeraView,
    vehicle: &crate::models::vehicle::Vehicle,
    attachment_id: Option<&str>,
    form: FormState<AttachmentFormValues>,
    conflict: Option<String>,
    status: StatusCode,
) -> Result<Response> {
    let view = AttachmentFormPage::new(
        layout(context),
        vehicle,
        attachment_id,
        form.with_known_fields(ATTACHMENT_FORM_FIELDS),
        conflict,
    );
    Ok(responses::render(
        context.response_preference,
        status,
        view.render_page(engine)?,
        view.render_form(engine)?,
    ))
}

async fn vehicle_attachment(
    attachments: &AttachmentService,
    vehicles: &VehicleService,
    raw_id: String,
    context: &BrowserRequestContext,
) -> std::result::Result<
    (
        crate::models::attachment::AttachmentMetadata,
        crate::models::vehicle::Vehicle,
    ),
    Response,
> {
    let id = AttachmentId::parse(raw_id)
        .map_err(|_| responses::not_found(context.response_preference, "attachment metadata"))?;
    let attachment = attachments
        .get(&id)
        .await
        .map_err(|error| workflow_response(context, error, "attachment metadata"))?;
    let AttachmentOwner::Vehicle(vehicle_id) = &attachment.owner else {
        return Err(responses::not_found(
            context.response_preference,
            "vehicle attachment metadata",
        ));
    };
    let vehicle = vehicles
        .get(vehicle_id)
        .await
        .map_err(|error| workflow_response(context, error, "vehicle"))?;
    Ok((attachment, vehicle))
}

async fn get_active_vehicle(
    vehicles: &VehicleService,
    raw_id: String,
    context: &BrowserRequestContext,
) -> std::result::Result<crate::models::vehicle::Vehicle, Response> {
    let id = VehicleId::parse(raw_id)
        .map_err(|_| responses::not_found(context.response_preference, "vehicle"))?;
    let vehicle = vehicles
        .get(&id)
        .await
        .map_err(|error| workflow_response(context, error, "vehicle"))?;
    if vehicle.is_archived() {
        return Err(responses::redirect(
            context.response_preference,
            &format!("/vehicles/{}", id.as_str()),
        ));
    }
    Ok(vehicle)
}

async fn active_customers(
    customers: &CustomerService,
) -> std::result::Result<Vec<crate::models::customer::Customer>, WorkflowError> {
    Ok(customers
        .list(PageRequest {
            filter: CustomerFilter {
                query: None,
                archive: ArchiveFilter::Active,
            },
            limit: PageLimit::new(200).expect("maximum page limit is valid"),
            after: None,
        })
        .await?
        .items)
}

async fn customers_for_filter(
    customers: &CustomerService,
) -> std::result::Result<Vec<crate::models::customer::Customer>, WorkflowError> {
    Ok(customers
        .list(PageRequest {
            filter: CustomerFilter {
                query: None,
                archive: ArchiveFilter::All,
            },
            limit: PageLimit::new(200).expect("maximum page limit is valid"),
            after: None,
        })
        .await?
        .items)
}

fn parse_vehicle_filter(
    values: &VehicleFilterValues,
) -> std::result::Result<VehicleFilter, String> {
    let archive = match values.archived.as_str() {
        "active" | "" => ArchiveFilter::Active,
        "archived" => ArchiveFilter::Archived,
        _ => return Err("Choose Active or Archived vehicles.".to_owned()),
    };
    let customer_id = optional_parse(&values.customer, CustomerId::parse)
        .map_err(|_| "Choose a valid customer.".to_owned())?;
    let registration = optional_parse(&values.registration, |value| {
        NormalizedRegistration::parse(&value)
    })
    .map_err(|_| "Enter a valid exact registration filter.".to_owned())?;
    let vin = optional_parse(&values.vin, |value| NormalizedVin::parse(&value))
        .map_err(|_| "Enter a valid 17-character VIN filter.".to_owned())?;
    Ok(VehicleFilter {
        query: some_text(&values.q),
        archive,
        customer_id,
        registration,
        vin,
        make: some_text(&values.make),
        model: some_text(&values.model),
    })
}

fn parse_history_filter(
    values: &HistoryFilterValues,
    settings: &BusinessSettings,
) -> std::result::Result<InterventionFilter, String> {
    let status = match values.status.as_str() {
        "" | "all" => None,
        "draft" => Some(InterventionStatus::Draft),
        "completed" => Some(InterventionStatus::Completed),
        "cancelled" => Some(InterventionStatus::Cancelled),
        _ => return Err("Choose All, Draft, Completed, or Cancelled history.".to_owned()),
    };
    let from = parse_date(&values.from, "Enter a valid From date.")?;
    let to = parse_date(&values.to, "Enter a valid To date.")?;
    if from.zip(to).is_some_and(|(from, to)| from > to) {
        return Err("The From date must be on or before the To date.".to_owned());
    }
    let workshop_time = WorkshopTime::system(settings.workshop_timezone());
    let service_date_from = from
        .map(|date| workshop_time.local_to_utc(&format!("{date}T00:00")))
        .transpose()
        .map_err(|error| error.to_string())?;
    let service_date_until = to
        .map(|date| {
            date.checked_add_days(chrono::Days::new(1))
                .ok_or(crate::domain::WorkshopTimeError::CalendarBoundaryOutOfRange)
                .and_then(|date| workshop_time.local_to_utc(&format!("{date}T00:00")))
        })
        .transpose()
        .map_err(|error| error.to_string())?;
    Ok(InterventionFilter {
        vehicle_id: None,
        status,
        service_date_from,
        service_date_until,
    })
}

fn parse_date(value: &str, message: &str) -> std::result::Result<Option<NaiveDate>, String> {
    if value.trim().is_empty() {
        Ok(None)
    } else {
        NaiveDate::parse_from_str(value, "%Y-%m-%d")
            .map(Some)
            .map_err(|_| message.to_owned())
    }
}

fn vehicle_create_command(
    values: &VehicleFormValues,
) -> std::result::Result<CreateVehicle, ValidationErrors> {
    let (customer_id, year, mileage) = validate_vehicle_values(values)?;
    Ok(CreateVehicle {
        customer_id,
        make: values.make.clone(),
        model: values.model.clone(),
        year,
        registration: Some(values.registration.clone()),
        vin: Some(values.vin.clone()),
        current_mileage: mileage,
        engine_type: Some(values.engine_type.clone()),
        notes: Some(values.notes.clone()),
    })
}

fn vehicle_update_command(
    values: &VehicleFormValues,
) -> std::result::Result<UpdateVehicle, ValidationErrors> {
    let (_, year, mileage) = validate_vehicle_values(values)?;
    Ok(UpdateVehicle {
        customer_id: None,
        make: Some(values.make.clone()),
        model: Some(values.model.clone()),
        year: Some(year),
        registration: Some(Some(values.registration.clone())),
        vin: Some(Some(values.vin.clone())),
        current_mileage: Some(mileage),
        engine_type: Some(Some(values.engine_type.clone())),
        notes: Some(Some(values.notes.clone())),
    })
}

fn validate_vehicle_values(
    values: &VehicleFormValues,
) -> std::result::Result<(CustomerId, Option<i32>, Option<u64>), ValidationErrors> {
    let mut errors = Vec::new();
    required(
        &mut errors,
        "customer_id",
        &values.customer_id,
        "Choose an active customer.",
    );
    required(&mut errors, "make", &values.make, "Enter the vehicle make.");
    required(
        &mut errors,
        "model",
        &values.model,
        "Enter the vehicle model.",
    );
    for (field, value, maximum) in [
        ("make", values.make.as_str(), MAKE_MAX_CHARS),
        ("model", values.model.as_str(), MODEL_MAX_CHARS),
        (
            "registration",
            values.registration.as_str(),
            REGISTRATION_MAX_CHARS,
        ),
        ("vin", values.vin.as_str(), VIN_DISPLAY_MAX_CHARS),
        (
            "engine_type",
            values.engine_type.as_str(),
            ENGINE_TYPE_MAX_CHARS,
        ),
        ("notes", values.notes.as_str(), NOTES_MAX_CHARS),
    ] {
        if value.trim().chars().count() > maximum {
            push_error(
                &mut errors,
                field,
                ValidationCode::TooLong,
                "Shorten this value.",
            );
        }
    }
    let customer_id = CustomerId::parse(values.customer_id.clone()).map_err(|_| ());
    if customer_id.is_err() && !values.customer_id.trim().is_empty() {
        push_error(
            &mut errors,
            "customer_id",
            ValidationCode::InvalidFormat,
            "Choose an active customer.",
        );
    }
    let year = if values.year.trim().is_empty() {
        Ok(None)
    } else {
        values
            .year
            .trim()
            .parse::<i32>()
            .ok()
            .filter(|year| (EARLIEST_VEHICLE_YEAR..=Utc::now().year() + 1).contains(year))
            .map(Some)
            .ok_or(())
    };
    if year.is_err() {
        push_error(
            &mut errors,
            "year",
            ValidationCode::OutOfRange,
            "Enter a year from 1886 through next year.",
        );
    }
    let mileage = parse_optional_u64(&values.current_mileage);
    if mileage.is_err() {
        push_error(
            &mut errors,
            "current_mileage",
            ValidationCode::OutOfRange,
            "Enter a non-negative whole-number mileage.",
        );
    }
    if !values.vin.trim().is_empty() && NormalizedVin::parse(&values.vin).is_err() {
        push_error(
            &mut errors,
            "vin",
            ValidationCode::InvalidFormat,
            "Enter a 17-character VIN without I, O, or Q.",
        );
    }
    if !values.registration.trim().is_empty()
        && NormalizedRegistration::parse(&values.registration).is_err()
    {
        push_error(
            &mut errors,
            "registration",
            ValidationCode::InvalidFormat,
            "Enter a valid registration.",
        );
    }
    if let Some(errors) = ValidationErrors::from_vec(errors) {
        return Err(errors);
    }
    Ok((
        customer_id.expect("validated customer id"),
        year.expect("validated year"),
        mileage.expect("validated mileage"),
    ))
}

fn attachment_update_command(
    values: &AttachmentFormValues,
    attachment: &AttachmentMetadata,
) -> std::result::Result<WriteAttachmentMetadata, ValidationErrors> {
    let mut errors = Vec::new();
    required(
        &mut errors,
        "display_name",
        &values.display_name,
        "Enter a display name.",
    );
    if values.display_name.trim().chars().count() > DISPLAY_NAME_MAX_CHARS {
        push_error(
            &mut errors,
            "display_name",
            ValidationCode::TooLong,
            "Use 255 characters or fewer.",
        );
    }
    if values.caption.trim().chars().count() > CAPTION_MAX_CHARS {
        push_error(
            &mut errors,
            "caption",
            ValidationCode::TooLong,
            "Use 1,000 characters or fewer.",
        );
    }
    if let Some(errors) = ValidationErrors::from_vec(errors) {
        return Err(errors);
    }
    Ok(WriteAttachmentMetadata {
        display_name: values.display_name.clone(),
        media_type: attachment.media_type.as_str().to_owned(),
        byte_size: Some(attachment.byte_size),
        caption: Some(values.caption.clone()),
    })
}

fn parse_optional_u64(value: &str) -> std::result::Result<Option<u64>, ()> {
    if value.trim().is_empty() {
        Ok(None)
    } else {
        value.trim().parse::<u64>().map(Some).map_err(|_| ())
    }
}

fn required(errors: &mut Vec<ValidationError>, field: &str, value: &str, message: &str) {
    if value.trim().is_empty() {
        push_error(errors, field, ValidationCode::Required, message);
    }
}

fn push_error(errors: &mut Vec<ValidationError>, field: &str, code: ValidationCode, message: &str) {
    errors.push(
        ValidationError::new(field, code, message)
            .expect("vehicle browser validation metadata is static and valid"),
    );
}

fn parse_cursor(value: &str) -> std::result::Result<Option<OpaqueCursor>, String> {
    if value.is_empty() {
        Ok(None)
    } else {
        OpaqueCursor::parse(value.to_owned())
            .map(Some)
            .map_err(|_| "This page link is no longer valid. Start from the first page.".to_owned())
    }
}

fn optional_parse<T, E>(
    value: &str,
    parser: impl FnOnce(String) -> std::result::Result<T, E>,
) -> std::result::Result<Option<T>, E> {
    if value.trim().is_empty() {
        Ok(None)
    } else {
        parser(value.to_owned()).map(Some)
    }
}

fn some_text(value: &str) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.to_owned())
}

fn vehicle_list_href(filters: &VehicleFilterValues) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    for (key, value) in [
        ("q", filters.q.as_str()),
        ("customer", filters.customer.as_str()),
        ("registration", filters.registration.as_str()),
        ("vin", filters.vin.as_str()),
        ("make", filters.make.as_str()),
        ("model", filters.model.as_str()),
        ("archived", filters.archived.as_str()),
        ("cursor", filters.cursor.as_str()),
    ] {
        if !value.is_empty() {
            serializer.append_pair(key, value);
        }
    }
    format!("/vehicles?{}", serializer.finish())
}

fn history_href(id: &VehicleId, filters: &HistoryFilterValues) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    for (key, value) in [
        ("status", filters.status.as_str()),
        ("from", filters.from.as_str()),
        ("to", filters.to.as_str()),
        ("cursor", filters.cursor.as_str()),
    ] {
        if !value.is_empty() {
            serializer.append_pair(key, value);
        }
    }
    format!("/vehicles/{}/history?{}", id.as_str(), serializer.finish())
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
            "Vehicle information is temporarily unavailable. Try again shortly.",
        ),
        WorkflowError::Validation(_) | WorkflowError::Conflict | WorkflowError::Internal => {
            responses::unexpected(context.response_preference)
        }
    }
}

#[must_use]
pub fn routes() -> Routes {
    Routes::new()
        .add("/vehicles", get(list))
        .add("/vehicles", post(create).layer(body_limit()))
        .add("/vehicles/new", get(generic_new_form))
        .add("/customers/{id}/vehicles/new", get(customer_new_form))
        .add("/vehicles/{id}", get(show))
        .add("/vehicles/{id}/edit", get(edit_form))
        .add("/vehicles/{id}/edit", post(update).layer(body_limit()))
        .add("/vehicles/{id}/archive", post(archive).layer(body_limit()))
        .add("/vehicles/{id}/restore", post(restore).layer(body_limit()))
        .add("/vehicles/{id}/reassign", get(reassign_form))
        .add(
            "/vehicles/{id}/reassign",
            post(reassign).layer(body_limit()),
        )
        .add("/vehicles/{id}/history", get(history))
        .add("/vehicles/{id}/attachments/new", get(new_attachment_form))
        .add(
            "/vehicles/{id}/attachments",
            post(create_attachment).layer(DefaultBodyLimit::max(MULTIPART_ENVELOPE_BYTES)),
        )
        .add("/attachments/{id}/content", get(attachment_content))
        .add("/attachments/{id}/download", get(attachment_download))
        .add("/attachments/{id}/edit", get(edit_attachment_form))
        .add(
            "/attachments/{id}/edit",
            post(update_attachment).layer(body_limit()),
        )
        .add(
            "/attachments/{id}/delete",
            post(delete_attachment).layer(body_limit()),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vehicle_cursor_links_preserve_all_filters() {
        let href = vehicle_list_href(&VehicleFilterValues {
            q: "Golf & GTE".to_owned(),
            registration: "1-abc-234".to_owned(),
            archived: "archived".to_owned(),
            cursor: "opaque_cursor".to_owned(),
            ..VehicleFilterValues::default()
        });
        assert_eq!(
            href,
            "/vehicles?q=Golf+%26+GTE&registration=1-abc-234&archived=archived&cursor=opaque_cursor"
        );
    }

    #[test]
    fn vehicle_form_rejects_negative_mileage_and_invalid_vin() {
        let values = VehicleFormValues {
            customer_id: "customer-1".to_owned(),
            make: "Volkswagen".to_owned(),
            model: "Golf".to_owned(),
            vin: "WVWZZZ1JZXW00000I".to_owned(),
            current_mileage: "-1".to_owned(),
            ..VehicleFormValues::default()
        };
        let errors = validate_vehicle_values(&values).expect_err("values should be invalid");
        let fields = errors
            .as_slice()
            .iter()
            .map(|error| error.field().as_str())
            .collect::<Vec<_>>();
        assert!(fields.contains(&"vin"));
        assert!(fields.contains(&"current_mileage"));
    }

    #[test]
    fn service_history_links_keep_status_and_date_cursor_binding() {
        let href = history_href(
            &VehicleId::parse("golf").expect("valid id"),
            &HistoryFilterValues {
                status: "cancelled".to_owned(),
                from: "2026-01-01".to_owned(),
                to: "2026-07-20".to_owned(),
                cursor: "opaque_cursor".to_owned(),
            },
        );
        assert_eq!(
            href,
            "/vehicles/golf/history?status=cancelled&from=2026-01-01&to=2026-07-20&cursor=opaque_cursor"
        );
    }
}

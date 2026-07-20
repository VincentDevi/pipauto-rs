//! Server-rendered intervention discovery, draft, detail, and transition workflows.

use axum::{extract::Query, http::StatusCode, response::Response};
use chrono::{NaiveDate, Utc};
use loco_rs::{
    controller::{
        extractor::shared_store::SharedStore, views::engines::TeraView, views::ViewEngine, Routes,
    },
    prelude::{get, post, Path},
    Result,
};

use crate::{
    controllers::browser::{
        context::{BrowserRequestContext, ResponsePreference},
        forms::{body_limit, AuthenticatedForm, FormState},
        responses,
    },
    domain::{
        AttachmentId, InterventionId, InterventionLineId, OpaqueCursor, Page, PageLimit,
        PageRequest, Quantity, ValidationCode, ValidationError, ValidationErrors, VehicleId,
    },
    models::{
        attachment::{AttachmentOwner, CAPTION_MAX_CHARS, DISPLAY_NAME_MAX_CHARS},
        intervention::{Intervention, InterventionStatus},
        intervention_line::{
            InterventionLine, InterventionLineCategory, DESCRIPTION_MAX_CHARS, UNIT_LABEL_MAX_CHARS,
        },
        vehicle::Vehicle,
    },
    repositories::{
        customer::ArchiveFilter,
        intervention::{InterventionFilter, LineMoveDirection, LineMutationResult},
        vehicle::VehicleFilter,
    },
    services::{
        attachment::{AttachmentService, WriteAttachmentMetadata},
        customer::CustomerService,
        intervention::{CreateIntervention, InterventionService, UpdateIntervention, WriteLine},
        vehicle::VehicleService,
        WorkflowError,
    },
    settings::BusinessSettings,
    views::{
        intervention::{
            InterventionDetailPage, InterventionFilterValues, InterventionFormPage,
            InterventionFormValues, InterventionLineFormPage, InterventionLineFormValues,
            InterventionLineRegion, InterventionListPage, InterventionTransitionPage,
        },
        layout::AuthenticatedLayout,
        vehicle::{AttachmentFormPage, AttachmentFormValues},
    },
};

const FORM_FIELDS: &[&str] = &[
    "service_date",
    "mileage",
    "customer_reported_problem",
    "diagnostics",
    "performed_work",
    "recommendations",
    "notes",
    "intervention",
];
const LINE_FORM_FIELDS: &[&str] = &[
    "category",
    "description",
    "quantity",
    "unit_label",
    "unit_price",
    "unit_cost",
    "position",
];
const ATTACHMENT_FORM_FIELDS: &[&str] = &["display_name", "media_type", "byte_size", "caption"];

async fn list(
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
    let filter = match parse_filter(&filters) {
        Ok(value) => value,
        Err(message) => {
            return render_list(
                &context,
                &engine,
                &vehicles,
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
                &vehicles,
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
                &vehicles,
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
        &vehicles,
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
async fn render_list(
    context: &BrowserRequestContext,
    engine: &TeraView,
    vehicles: &VehicleService,
    mut filters: InterventionFilterValues,
    page: Page<crate::models::intervention::ServiceHistorySummary>,
    filter_vehicles: Vec<Vehicle>,
    next_href: Option<String>,
    filter_error: Option<String>,
    status: StatusCode,
) -> Result<Response> {
    let mut row_vehicles = Vec::with_capacity(page.items.len());
    for item in &page.items {
        match vehicles.get(&item.intervention.vehicle_id).await {
            Ok(vehicle) => row_vehicles.push(vehicle),
            Err(error) => return Ok(workflow_response(context, error, "intervention vehicle")),
        }
    }
    filters.cursor.clear();
    let view = InterventionListPage::new(
        layout(context),
        filters,
        page,
        row_vehicles,
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

async fn new_form(
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
        service_date: Utc::now().date_naive().to_string(),
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
async fn create(
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
    let command = match create_command(&values, vehicle.id.clone(), settings.default_currency()) {
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
fn render_create_form(
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
        layout(context),
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

async fn show(
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
async fn render_detail(
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
        layout(context),
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

async fn edit_form(
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
        FormState::new((&intervention).into()),
        None,
        StatusCode::OK,
    )
}

#[allow(clippy::too_many_arguments)]
async fn update(
    context: BrowserRequestContext,
    SharedStore(interventions): SharedStore<InterventionService>,
    SharedStore(vehicles): SharedStore<VehicleService>,
    SharedStore(customers): SharedStore<CustomerService>,
    SharedStore(attachments): SharedStore<AttachmentService>,
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
    let command = match update_command(&values) {
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
fn render_edit_form(
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
        layout(context),
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

async fn new_line_form(
    context: BrowserRequestContext,
    SharedStore(interventions): SharedStore<InterventionService>,
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
        return Ok(responses::redirect(
            context.response_preference,
            &format!("/interventions/{}", id.as_str()),
        ));
    }
    let workspace = match interventions.line_workspace(&id).await {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(&context, error, "intervention lines")),
    };
    let position = (0..=u32::MAX)
        .find(|position| {
            workspace
                .lines
                .iter()
                .all(|line| line.position != *position)
        })
        .unwrap_or_default();
    render_line_form(
        &context,
        &engine,
        &intervention,
        None,
        FormState::new(InterventionLineFormValues {
            quantity: "1".to_owned(),
            position: position.to_string(),
            ..InterventionLineFormValues::default()
        }),
        None,
        StatusCode::OK,
    )
}

async fn create_line(
    context: BrowserRequestContext,
    SharedStore(interventions): SharedStore<InterventionService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path(raw_id): Path<String>,
    form: AuthenticatedForm<InterventionLineFormValues>,
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
        return Ok(responses::redirect(
            context.response_preference,
            &format!("/interventions/{}", id.as_str()),
        ));
    }
    let values = form.fields;
    let command = match line_command(&values) {
        Ok(value) => value,
        Err(errors) => {
            return render_line_form(
                &context,
                &engine,
                &intervention,
                None,
                FormState::with_validation(values, &errors),
                None,
                StatusCode::UNPROCESSABLE_ENTITY,
            )
        }
    };
    match interventions.create_line(&id, command).await {
        Ok(_) => Ok(detail_redirect(&context, &id)),
        Err(WorkflowError::Validation(errors)) => render_line_form(
            &context,
            &engine,
            &intervention,
            None,
            FormState::with_validation(values, &errors),
            None,
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        Err(WorkflowError::Conflict) => render_line_form(
            &context,
            &engine,
            &intervention,
            None,
            FormState::new(values),
            Some("The intervention or line order changed before this line was saved. Review the authoritative workspace and try again.".to_owned()),
            StatusCode::CONFLICT,
        ),
        Err(error) => Ok(workflow_response(&context, error, "intervention line")),
    }
}

async fn edit_line_form(
    context: BrowserRequestContext,
    SharedStore(interventions): SharedStore<InterventionService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path((raw_id, raw_line_id)): Path<(String, String)>,
) -> Result<Response> {
    let (intervention, line) =
        match intervention_line(&interventions, raw_id, raw_line_id, &context).await {
            Ok(value) => value,
            Err(response) => return Ok(response),
        };
    if intervention.status != InterventionStatus::Draft {
        return Ok(detail_redirect(&context, &intervention.id));
    }
    let line_id = line.id.as_str().to_owned();
    render_line_form(
        &context,
        &engine,
        &intervention,
        Some(&line_id),
        FormState::new((&line).into()),
        None,
        StatusCode::OK,
    )
}

async fn update_line(
    context: BrowserRequestContext,
    SharedStore(interventions): SharedStore<InterventionService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path((raw_id, raw_line_id)): Path<(String, String)>,
    form: AuthenticatedForm<InterventionLineFormValues>,
) -> Result<Response> {
    let (intervention, line) =
        match intervention_line(&interventions, raw_id, raw_line_id, &context).await {
            Ok(value) => value,
            Err(response) => return Ok(response),
        };
    if intervention.status != InterventionStatus::Draft {
        return Ok(detail_redirect(&context, &intervention.id));
    }
    let line_id = line.id.as_str().to_owned();
    let values = form.fields;
    let command = match line_command(&values) {
        Ok(value) => value,
        Err(errors) => {
            return render_line_form(
                &context,
                &engine,
                &intervention,
                Some(&line_id),
                FormState::with_validation(values, &errors),
                None,
                StatusCode::UNPROCESSABLE_ENTITY,
            )
        }
    };
    match interventions.update_line(&intervention.id, line.id, command).await {
        Ok(_) => Ok(detail_redirect(&context, &intervention.id)),
        Err(WorkflowError::Validation(errors)) => render_line_form(
            &context,
            &engine,
            &intervention,
            Some(&line_id),
            FormState::with_validation(values, &errors),
            None,
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        Err(WorkflowError::Conflict) => render_line_form(
            &context,
            &engine,
            &intervention,
            Some(&line_id),
            FormState::new(values),
            Some("The intervention or line order changed while this line was being saved. Authoritative values were not overwritten.".to_owned()),
            StatusCode::CONFLICT,
        ),
        Err(error) => Ok(workflow_response(&context, error, "intervention line")),
    }
}

async fn delete_line(
    context: BrowserRequestContext,
    SharedStore(interventions): SharedStore<InterventionService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path((raw_id, raw_line_id)): Path<(String, String)>,
    _form: AuthenticatedForm<EmptyForm>,
) -> Result<Response> {
    let (intervention, line) =
        match intervention_line(&interventions, raw_id, raw_line_id, &context).await {
            Ok(value) => value,
            Err(response) => return Ok(response),
        };
    match interventions.delete_line(&intervention.id, line.id).await {
        Ok(result) => line_mutation_response(&context, &engine, &intervention, result),
        Err(WorkflowError::Conflict) => Ok(detail_redirect(&context, &intervention.id)),
        Err(error) => Ok(workflow_response(&context, error, "intervention line")),
    }
}

async fn move_line_up(
    context: BrowserRequestContext,
    SharedStore(interventions): SharedStore<InterventionService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path((raw_id, raw_line_id)): Path<(String, String)>,
    _form: AuthenticatedForm<EmptyForm>,
) -> Result<Response> {
    move_line(
        context,
        interventions,
        engine,
        raw_id,
        raw_line_id,
        LineMoveDirection::Up,
    )
    .await
}

async fn move_line_down(
    context: BrowserRequestContext,
    SharedStore(interventions): SharedStore<InterventionService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path((raw_id, raw_line_id)): Path<(String, String)>,
    _form: AuthenticatedForm<EmptyForm>,
) -> Result<Response> {
    move_line(
        context,
        interventions,
        engine,
        raw_id,
        raw_line_id,
        LineMoveDirection::Down,
    )
    .await
}

async fn move_line(
    context: BrowserRequestContext,
    interventions: InterventionService,
    engine: TeraView,
    raw_id: String,
    raw_line_id: String,
    direction: LineMoveDirection,
) -> Result<Response> {
    let (intervention, line) =
        match intervention_line(&interventions, raw_id, raw_line_id, &context).await {
            Ok(value) => value,
            Err(response) => return Ok(response),
        };
    match interventions
        .move_line(&intervention.id, line.id, direction)
        .await
    {
        Ok(result) => line_mutation_response(&context, &engine, &intervention, result),
        Err(WorkflowError::Conflict) => Ok(detail_redirect(&context, &intervention.id)),
        Err(error) => Ok(workflow_response(
            &context,
            error,
            "intervention line order",
        )),
    }
}

fn line_mutation_response(
    context: &BrowserRequestContext,
    engine: &TeraView,
    intervention: &Intervention,
    result: LineMutationResult,
) -> Result<Response> {
    if context.response_preference == ResponsePreference::FullPage {
        return Ok(detail_redirect(context, &intervention.id));
    }
    let view =
        InterventionLineRegion::new(layout(context), intervention, result.lines, result.totals);
    Ok(responses::fragment(StatusCode::OK, view.render(engine)?))
}

fn render_line_form(
    context: &BrowserRequestContext,
    engine: &TeraView,
    intervention: &Intervention,
    line_id: Option<&str>,
    form: FormState<InterventionLineFormValues>,
    conflict: Option<String>,
    status: StatusCode,
) -> Result<Response> {
    let view = InterventionLineFormPage::new(
        layout(context),
        intervention,
        line_id,
        form.with_known_fields(LINE_FORM_FIELDS),
        conflict,
    );
    Ok(responses::render(
        context.response_preference,
        status,
        view.render_page(engine)?,
        view.render_form(engine)?,
    ))
}

async fn new_attachment_form(
    context: BrowserRequestContext,
    SharedStore(interventions): SharedStore<InterventionService>,
    SharedStore(vehicles): SharedStore<VehicleService>,
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
        return Ok(detail_redirect(&context, &id));
    }
    let vehicle = match vehicles.get(&intervention.vehicle_id).await {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(&context, error, "intervention vehicle")),
    };
    render_intervention_attachment_form(
        &context,
        &engine,
        &intervention,
        &vehicle,
        None,
        FormState::new(AttachmentFormValues::default()),
        None,
        StatusCode::OK,
    )
}

async fn create_attachment(
    context: BrowserRequestContext,
    SharedStore(interventions): SharedStore<InterventionService>,
    SharedStore(vehicles): SharedStore<VehicleService>,
    SharedStore(attachments): SharedStore<AttachmentService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path(raw_id): Path<String>,
    form: AuthenticatedForm<AttachmentFormValues>,
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
        return Ok(detail_redirect(&context, &id));
    }
    let vehicle = match vehicles.get(&intervention.vehicle_id).await {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(&context, error, "intervention vehicle")),
    };
    let values = form.fields;
    let command = match attachment_command(&values) {
        Ok(value) => value,
        Err(errors) => {
            return render_intervention_attachment_form(
                &context,
                &engine,
                &intervention,
                &vehicle,
                None,
                FormState::with_validation(values, &errors),
                None,
                StatusCode::UNPROCESSABLE_ENTITY,
            )
        }
    };
    match attachments
        .create(AttachmentOwner::Intervention(id.clone()), command)
        .await
    {
        Ok(_) => Ok(detail_redirect(&context, &id)),
        Err(WorkflowError::Validation(errors)) => render_intervention_attachment_form(
            &context,
            &engine,
            &intervention,
            &vehicle,
            None,
            FormState::with_validation(values, &errors),
            None,
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        Err(WorkflowError::Conflict) => render_intervention_attachment_form(
            &context,
            &engine,
            &intervention,
            &vehicle,
            None,
            FormState::new(values),
            Some("The intervention changed state before this metadata was saved.".to_owned()),
            StatusCode::CONFLICT,
        ),
        Err(error) => Ok(workflow_response(&context, error, "attachment metadata")),
    }
}

async fn edit_attachment_form(
    context: BrowserRequestContext,
    SharedStore(interventions): SharedStore<InterventionService>,
    SharedStore(vehicles): SharedStore<VehicleService>,
    SharedStore(attachments): SharedStore<AttachmentService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path((raw_id, raw_attachment_id)): Path<(String, String)>,
) -> Result<Response> {
    let (intervention, attachment) = match intervention_attachment(
        &interventions,
        &attachments,
        raw_id,
        raw_attachment_id,
        &context,
    )
    .await
    {
        Ok(value) => value,
        Err(response) => return Ok(response),
    };
    if intervention.status != InterventionStatus::Draft {
        return Ok(detail_redirect(&context, &intervention.id));
    }
    let vehicle = match vehicles.get(&intervention.vehicle_id).await {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(&context, error, "intervention vehicle")),
    };
    let attachment_id = attachment.id.as_str().to_owned();
    render_intervention_attachment_form(
        &context,
        &engine,
        &intervention,
        &vehicle,
        Some(&attachment_id),
        FormState::new(attachment.into()),
        None,
        StatusCode::OK,
    )
}

async fn update_attachment(
    context: BrowserRequestContext,
    SharedStore(interventions): SharedStore<InterventionService>,
    SharedStore(vehicles): SharedStore<VehicleService>,
    SharedStore(attachments): SharedStore<AttachmentService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path((raw_id, raw_attachment_id)): Path<(String, String)>,
    form: AuthenticatedForm<AttachmentFormValues>,
) -> Result<Response> {
    let (intervention, attachment) = match intervention_attachment(
        &interventions,
        &attachments,
        raw_id,
        raw_attachment_id,
        &context,
    )
    .await
    {
        Ok(value) => value,
        Err(response) => return Ok(response),
    };
    if intervention.status != InterventionStatus::Draft {
        return Ok(detail_redirect(&context, &intervention.id));
    }
    let vehicle = match vehicles.get(&intervention.vehicle_id).await {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(&context, error, "intervention vehicle")),
    };
    let attachment_id = attachment.id.as_str().to_owned();
    let values = form.fields;
    let command = match attachment_command(&values) {
        Ok(value) => value,
        Err(errors) => {
            return render_intervention_attachment_form(
                &context,
                &engine,
                &intervention,
                &vehicle,
                Some(&attachment_id),
                FormState::with_validation(values, &errors),
                None,
                StatusCode::UNPROCESSABLE_ENTITY,
            )
        }
    };
    match attachments.update(&attachment.id, command).await {
        Ok(_) => Ok(detail_redirect(&context, &intervention.id)),
        Err(WorkflowError::Validation(errors)) => render_intervention_attachment_form(
            &context,
            &engine,
            &intervention,
            &vehicle,
            Some(&attachment_id),
            FormState::with_validation(values, &errors),
            None,
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        Err(WorkflowError::Conflict) => render_intervention_attachment_form(
            &context,
            &engine,
            &intervention,
            &vehicle,
            Some(&attachment_id),
            FormState::new(values),
            Some("The intervention changed state before this metadata was saved.".to_owned()),
            StatusCode::CONFLICT,
        ),
        Err(error) => Ok(workflow_response(&context, error, "attachment metadata")),
    }
}

async fn delete_attachment(
    context: BrowserRequestContext,
    SharedStore(interventions): SharedStore<InterventionService>,
    SharedStore(attachments): SharedStore<AttachmentService>,
    Path((raw_id, raw_attachment_id)): Path<(String, String)>,
    _form: AuthenticatedForm<EmptyForm>,
) -> Result<Response> {
    let (intervention, attachment) = match intervention_attachment(
        &interventions,
        &attachments,
        raw_id,
        raw_attachment_id,
        &context,
    )
    .await
    {
        Ok(value) => value,
        Err(response) => return Ok(response),
    };
    match attachments.delete(&attachment.id).await {
        Ok(()) | Err(WorkflowError::Conflict) => Ok(detail_redirect(&context, &intervention.id)),
        Err(error) => Ok(workflow_response(&context, error, "attachment metadata")),
    }
}

#[allow(clippy::too_many_arguments)]
fn render_intervention_attachment_form(
    context: &BrowserRequestContext,
    engine: &TeraView,
    intervention: &Intervention,
    vehicle: &Vehicle,
    attachment_id: Option<&str>,
    form: FormState<AttachmentFormValues>,
    conflict: Option<String>,
    status: StatusCode,
) -> Result<Response> {
    let view = AttachmentFormPage::for_intervention(
        layout(context),
        intervention,
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

async fn complete_confirmation(
    context: BrowserRequestContext,
    SharedStore(interventions): SharedStore<InterventionService>,
    SharedStore(vehicles): SharedStore<VehicleService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path(raw_id): Path<String>,
) -> Result<Response> {
    transition_confirmation(context, interventions, vehicles, engine, raw_id, true).await
}

async fn cancel_confirmation(
    context: BrowserRequestContext,
    SharedStore(interventions): SharedStore<InterventionService>,
    SharedStore(vehicles): SharedStore<VehicleService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path(raw_id): Path<String>,
) -> Result<Response> {
    transition_confirmation(context, interventions, vehicles, engine, raw_id, false).await
}

async fn transition_confirmation(
    context: BrowserRequestContext,
    interventions: InterventionService,
    vehicles: VehicleService,
    engine: TeraView,
    raw_id: String,
    completion: bool,
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
        return Ok(responses::redirect(
            context.response_preference,
            &format!("/interventions/{}", id.as_str()),
        ));
    }
    let vehicle = match vehicles.get(&intervention.vehicle_id).await {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(&context, error, "intervention vehicle")),
    };
    let workspace = match interventions.line_workspace(&id).await {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(&context, error, "intervention lines")),
    };
    let view = InterventionTransitionPage::new(
        layout(&context),
        &intervention,
        &vehicle,
        workspace.totals.price,
        completion,
    );
    Ok(responses::render(
        context.response_preference,
        StatusCode::OK,
        view.render_page(&engine)?,
        view.render_content(&engine)?,
    ))
}

#[allow(clippy::too_many_arguments)]
async fn complete(
    context: BrowserRequestContext,
    SharedStore(interventions): SharedStore<InterventionService>,
    SharedStore(vehicles): SharedStore<VehicleService>,
    SharedStore(customers): SharedStore<CustomerService>,
    SharedStore(attachments): SharedStore<AttachmentService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path(raw_id): Path<String>,
    _form: AuthenticatedForm<EmptyForm>,
) -> Result<Response> {
    transition(
        &context,
        &engine,
        &interventions,
        &vehicles,
        &customers,
        &attachments,
        raw_id,
        true,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn cancel(
    context: BrowserRequestContext,
    SharedStore(interventions): SharedStore<InterventionService>,
    SharedStore(vehicles): SharedStore<VehicleService>,
    SharedStore(customers): SharedStore<CustomerService>,
    SharedStore(attachments): SharedStore<AttachmentService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path(raw_id): Path<String>,
    _form: AuthenticatedForm<EmptyForm>,
) -> Result<Response> {
    transition(
        &context,
        &engine,
        &interventions,
        &vehicles,
        &customers,
        &attachments,
        raw_id,
        false,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn transition(
    context: &BrowserRequestContext,
    engine: &TeraView,
    interventions: &InterventionService,
    vehicles: &VehicleService,
    customers: &CustomerService,
    attachments: &AttachmentService,
    raw_id: String,
    completion: bool,
) -> Result<Response> {
    let id = match intervention_id(raw_id, context) {
        Ok(value) => value,
        Err(response) => return Ok(response),
    };
    let result = if completion {
        interventions.complete(&id).await
    } else {
        interventions.cancel(&id).await
    };
    match result {
        Ok(_) => Ok(responses::redirect(
            context.response_preference,
            &format!("/interventions/{}", id.as_str()),
        )),
        Err(WorkflowError::Validation(errors)) => render_detail(
            context,
            engine,
            interventions,
            vehicles,
            customers,
            attachments,
            &id,
            Some(
                errors
                    .as_slice()
                    .first()
                    .map_or("Check the intervention before completing it.", |error| error.message())
                    .to_owned(),
            ),
            StatusCode::UNPROCESSABLE_ENTITY,
        )
        .await,
        Err(WorkflowError::Conflict) => render_detail(
            context,
            engine,
            interventions,
            vehicles,
            customers,
            attachments,
            &id,
            Some("Another request changed this intervention. Authoritative state has been reloaded; the transition was not repeated.".to_owned()),
            StatusCode::CONFLICT,
        )
        .await,
        Err(error) => Ok(workflow_response(context, error, "intervention")),
    }
}

#[derive(serde::Deserialize)]
struct EmptyForm {
    #[serde(default)]
    _unused: Option<String>,
}

fn parse_filter(
    values: &InterventionFilterValues,
) -> std::result::Result<InterventionFilter, String> {
    let vehicle_id = if values.vehicle.trim().is_empty() {
        None
    } else {
        Some(
            VehicleId::parse(values.vehicle.clone())
                .map_err(|_| "Choose a valid vehicle.".to_owned())?,
        )
    };
    let status = match values.status.as_str() {
        "" | "all" => None,
        "draft" => Some(InterventionStatus::Draft),
        "completed" => Some(InterventionStatus::Completed),
        "cancelled" => Some(InterventionStatus::Cancelled),
        _ => return Err("Choose All, Draft, Completed, or Cancelled interventions.".to_owned()),
    };
    let from = parse_optional_date(&values.from, "Enter a valid From date.")?;
    let to = parse_optional_date(&values.to, "Enter a valid To date.")?;
    if from.zip(to).is_some_and(|(from, to)| from > to) {
        return Err("The From date must be on or before the To date.".to_owned());
    }
    Ok(InterventionFilter {
        vehicle_id,
        status,
        service_date_from: from,
        service_date_to: to,
    })
}

fn create_command(
    values: &InterventionFormValues,
    vehicle_id: VehicleId,
    currency: crate::domain::CurrencyCode,
) -> std::result::Result<CreateIntervention, ValidationErrors> {
    let (service_date, mileage) = validate_form(values)?;
    Ok(CreateIntervention {
        vehicle_id,
        service_date,
        mileage,
        customer_reported_problem: optional_text(&values.customer_reported_problem),
        diagnostics: optional_text(&values.diagnostics),
        performed_work: optional_text(&values.performed_work),
        recommendations: optional_text(&values.recommendations),
        notes: optional_text(&values.notes),
        currency,
    })
}

fn update_command(
    values: &InterventionFormValues,
) -> std::result::Result<UpdateIntervention, ValidationErrors> {
    let (service_date, mileage) = validate_form(values)?;
    Ok(UpdateIntervention {
        service_date: Some(service_date),
        mileage: Some(mileage),
        customer_reported_problem: Some(optional_text(&values.customer_reported_problem)),
        diagnostics: Some(optional_text(&values.diagnostics)),
        performed_work: Some(optional_text(&values.performed_work)),
        recommendations: Some(optional_text(&values.recommendations)),
        notes: Some(optional_text(&values.notes)),
        currency: None,
    })
}

fn validate_form(
    values: &InterventionFormValues,
) -> std::result::Result<(NaiveDate, Option<u64>), ValidationErrors> {
    let mut errors = Vec::new();
    let service_date = NaiveDate::parse_from_str(&values.service_date, "%Y-%m-%d").map_err(|_| {
        errors.push(validation_error(
            "service_date",
            "Enter a valid service date.",
        ));
    });
    let mileage = if values.mileage.trim().is_empty() {
        Ok(None)
    } else {
        values.mileage.parse::<u64>().map(Some).map_err(|_| {
            errors.push(validation_error(
                "mileage",
                "Enter a non-negative whole mileage.",
            ));
        })
    };
    match (service_date, mileage) {
        (Ok(date), Ok(mileage)) => Ok((date, mileage)),
        _ => Err(ValidationErrors::from_vec(errors).expect("form validation errors are non-empty")),
    }
}

fn line_command(
    values: &InterventionLineFormValues,
) -> std::result::Result<WriteLine, ValidationErrors> {
    let mut errors = Vec::new();
    let category = match values.category.as_str() {
        "labour" => Some(InterventionLineCategory::Labour),
        "part" => Some(InterventionLineCategory::Part),
        "material" => Some(InterventionLineCategory::Material),
        "other" => Some(InterventionLineCategory::Other),
        _ => {
            errors.push(validation_error(
                "category",
                "Choose Labour, Part, Material, or Other.",
            ));
            None
        }
    };
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
    let unit_price = parse_money_input(&values.unit_price).map_err(|_| {
        errors.push(validation_error(
            "unit_price",
            "Enter a non-negative amount with at most two decimal places.",
        ));
    });
    let unit_cost = if values.unit_cost.is_empty() {
        Ok(None)
    } else {
        parse_money_input(&values.unit_cost).map(Some).map_err(|_| {
            errors.push(validation_error(
                "unit_cost",
                "Enter a non-negative amount with at most two decimal places.",
            ));
        })
    };
    let position = values.position.parse::<u32>().map_err(|_| {
        errors.push(validation_error(
            "position",
            "Enter a non-negative whole-number position.",
        ));
    });
    if let Some(errors) = ValidationErrors::from_vec(errors) {
        return Err(errors);
    }
    Ok(WriteLine {
        category: category.expect("validated category"),
        description: values.description.clone(),
        quantity: quantity.expect("validated quantity"),
        unit_label: values.unit_label.clone(),
        unit_price_minor: unit_price.expect("validated unit price"),
        unit_cost_minor: unit_cost.expect("validated unit cost"),
        position: position.expect("validated position"),
    })
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

fn attachment_command(
    values: &AttachmentFormValues,
) -> std::result::Result<WriteAttachmentMetadata, ValidationErrors> {
    let mut errors = Vec::new();
    validate_required_length(
        &mut errors,
        "display_name",
        &values.display_name,
        DISPLAY_NAME_MAX_CHARS,
        "Enter a display name.",
        "Use 255 characters or fewer.",
    );
    if values.caption.trim().chars().count() > CAPTION_MAX_CHARS {
        errors.push(validation_error(
            "caption",
            "Use 1,000 characters or fewer.",
        ));
    }
    if !matches!(
        values.media_type.as_str(),
        "application/pdf" | "image/heic" | "image/heif" | "image/jpeg" | "image/png" | "image/webp"
    ) {
        errors.push(validation_error(
            "media_type",
            "Choose a supported PDF or image content type.",
        ));
    }
    let byte_size = if values.byte_size.trim().is_empty() {
        Ok(None)
    } else {
        values.byte_size.parse::<u64>().map(Some).map_err(|_| {
            errors.push(validation_error(
                "byte_size",
                "Enter a non-negative whole-number byte size.",
            ));
        })
    };
    if let Some(errors) = ValidationErrors::from_vec(errors) {
        return Err(errors);
    }
    Ok(WriteAttachmentMetadata {
        display_name: values.display_name.clone(),
        media_type: values.media_type.clone(),
        byte_size: byte_size.expect("validated byte size"),
        caption: (!values.caption.trim().is_empty()).then(|| values.caption.clone()),
    })
}

fn validation_error(field: &str, message: &str) -> ValidationError {
    ValidationError::new(field, ValidationCode::InvalidFormat, message)
        .expect("static validation metadata is valid")
}

fn optional_text(value: &str) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.to_owned())
}

fn parse_optional_date(
    value: &str,
    message: &str,
) -> std::result::Result<Option<NaiveDate>, String> {
    if value.trim().is_empty() {
        Ok(None)
    } else {
        NaiveDate::parse_from_str(value, "%Y-%m-%d")
            .map(Some)
            .map_err(|_| message.to_owned())
    }
}

fn parse_cursor(value: &str) -> std::result::Result<Option<OpaqueCursor>, String> {
    if value.is_empty() {
        Ok(None)
    } else {
        OpaqueCursor::parse(value.to_owned())
            .map(Some)
            .map_err(|_| "Use a valid intervention page link.".to_owned())
    }
}

fn list_href(filters: &InterventionFilterValues) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    for (key, value) in [
        ("vehicle", filters.vehicle.as_str()),
        ("status", filters.status.as_str()),
        ("from", filters.from.as_str()),
        ("to", filters.to.as_str()),
        ("cursor", filters.cursor.as_str()),
    ] {
        if !value.is_empty() {
            serializer.append_pair(key, value);
        }
    }
    format!("/interventions?{}", serializer.finish())
}

async fn all_vehicles(
    vehicles: &VehicleService,
) -> std::result::Result<Vec<Vehicle>, WorkflowError> {
    Ok(vehicles
        .list(PageRequest {
            filter: VehicleFilter {
                archive: ArchiveFilter::All,
                ..VehicleFilter::default()
            },
            limit: PageLimit::new(200).expect("maximum page limit is valid"),
            after: None,
        })
        .await?
        .items)
}

fn empty_page() -> Page<crate::models::intervention::ServiceHistorySummary> {
    Page {
        items: Vec::new(),
        next_cursor: None,
    }
}

fn mileage_error(errors: &ValidationErrors) -> bool {
    errors
        .as_slice()
        .iter()
        .any(|error| error.field().as_str() == "mileage")
}

async fn intervention_line(
    interventions: &InterventionService,
    raw_id: String,
    raw_line_id: String,
    context: &BrowserRequestContext,
) -> std::result::Result<(Intervention, InterventionLine), Response> {
    let id = intervention_id(raw_id, context)?;
    let line_id = InterventionLineId::parse(raw_line_id)
        .map_err(|_| responses::not_found(context.response_preference, "intervention line"))?;
    let intervention = interventions
        .get(&id)
        .await
        .map_err(|error| workflow_response(context, error, "intervention"))?;
    let workspace = interventions
        .line_workspace(&id)
        .await
        .map_err(|error| workflow_response(context, error, "intervention lines"))?;
    let line = workspace
        .lines
        .into_iter()
        .find(|line| line.id == line_id)
        .ok_or_else(|| responses::not_found(context.response_preference, "intervention line"))?;
    Ok((intervention, line))
}

async fn intervention_attachment(
    interventions: &InterventionService,
    attachments: &AttachmentService,
    raw_id: String,
    raw_attachment_id: String,
    context: &BrowserRequestContext,
) -> std::result::Result<(Intervention, crate::models::attachment::AttachmentMetadata), Response> {
    let id = intervention_id(raw_id, context)?;
    let attachment_id = AttachmentId::parse(raw_attachment_id)
        .map_err(|_| responses::not_found(context.response_preference, "attachment metadata"))?;
    let intervention = interventions
        .get(&id)
        .await
        .map_err(|error| workflow_response(context, error, "intervention"))?;
    let attachment = attachments
        .get(&attachment_id)
        .await
        .map_err(|error| workflow_response(context, error, "attachment metadata"))?;
    if attachment.owner != AttachmentOwner::Intervention(id) {
        return Err(responses::not_found(
            context.response_preference,
            "intervention attachment metadata",
        ));
    }
    Ok((intervention, attachment))
}

fn detail_redirect(context: &BrowserRequestContext, id: &InterventionId) -> Response {
    responses::redirect(
        context.response_preference,
        &format!("/interventions/{}", id.as_str()),
    )
}

async fn vehicle(
    vehicles: &VehicleService,
    raw_id: String,
    context: &BrowserRequestContext,
) -> std::result::Result<Vehicle, Response> {
    let id = VehicleId::parse(raw_id)
        .map_err(|_| responses::not_found(context.response_preference, "vehicle"))?;
    vehicles
        .get(&id)
        .await
        .map_err(|error| workflow_response(context, error, "vehicle"))
}

#[allow(clippy::result_large_err)]
fn intervention_id(
    raw_id: String,
    context: &BrowserRequestContext,
) -> std::result::Result<InterventionId, Response> {
    InterventionId::parse(raw_id)
        .map_err(|_| responses::not_found(context.response_preference, "intervention"))
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
            "Intervention information is temporarily unavailable. Try again shortly.",
        ),
        WorkflowError::Validation(_) | WorkflowError::Conflict | WorkflowError::Internal => {
            responses::unexpected(context.response_preference)
        }
    }
}

#[must_use]
pub fn routes() -> Routes {
    Routes::new()
        .add("/interventions", get(list))
        .add("/vehicles/{id}/interventions/new", get(new_form))
        .add(
            "/vehicles/{id}/interventions",
            post(create).layer(body_limit()),
        )
        .add("/interventions/{id}", get(show))
        .add("/interventions/{id}/edit", get(edit_form))
        .add("/interventions/{id}/edit", post(update).layer(body_limit()))
        .add("/interventions/{id}/lines/new", get(new_line_form))
        .add(
            "/interventions/{id}/lines",
            post(create_line).layer(body_limit()),
        )
        .add(
            "/interventions/{id}/lines/{line_id}/edit",
            get(edit_line_form),
        )
        .add(
            "/interventions/{id}/lines/{line_id}/edit",
            post(update_line).layer(body_limit()),
        )
        .add(
            "/interventions/{id}/lines/{line_id}/delete",
            post(delete_line).layer(body_limit()),
        )
        .add(
            "/interventions/{id}/lines/{line_id}/move-up",
            post(move_line_up).layer(body_limit()),
        )
        .add(
            "/interventions/{id}/lines/{line_id}/move-down",
            post(move_line_down).layer(body_limit()),
        )
        .add(
            "/interventions/{id}/attachments/new",
            get(new_attachment_form),
        )
        .add(
            "/interventions/{id}/attachments",
            post(create_attachment).layer(body_limit()),
        )
        .add(
            "/interventions/{id}/attachments/{attachment_id}/edit",
            get(edit_attachment_form),
        )
        .add(
            "/interventions/{id}/attachments/{attachment_id}/edit",
            post(update_attachment).layer(body_limit()),
        )
        .add(
            "/interventions/{id}/attachments/{attachment_id}/delete",
            post(delete_attachment).layer(body_limit()),
        )
        .add("/interventions/{id}/complete", get(complete_confirmation))
        .add(
            "/interventions/{id}/complete",
            post(complete).layer(body_limit()),
        )
        .add("/interventions/{id}/cancel", get(cancel_confirmation))
        .add(
            "/interventions/{id}/cancel",
            post(cancel).layer(body_limit()),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_links_preserve_supported_filters_only() {
        let href = list_href(&InterventionFilterValues {
            vehicle: "vehicle-1".to_owned(),
            status: "completed".to_owned(),
            from: "2026-01-01".to_owned(),
            to: "2026-07-20".to_owned(),
            cursor: "opaque_cursor".to_owned(),
        });
        assert_eq!(
            href,
            "/interventions?vehicle=vehicle-1&status=completed&from=2026-01-01&to=2026-07-20&cursor=opaque_cursor"
        );
    }
}

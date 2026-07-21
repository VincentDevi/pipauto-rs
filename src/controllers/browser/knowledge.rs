//! Server-rendered technical-knowledge search, authoring, detail, and lifecycle workflows.

use axum::{
    extract::{DefaultBodyLimit, Query},
    http::StatusCode,
    response::Response,
};
use loco_rs::{
    controller::{
        extractor::shared_store::SharedStore, views::engines::TeraView, views::ViewEngine, Routes,
    },
    prelude::{get, post, Path},
    Result,
};
use serde::Deserialize;

use crate::{
    auth::csrf::AuthenticatedAttachmentMultipart,
    controllers::browser::{
        context::BrowserRequestContext,
        forms::{body_limit, AuthenticatedForm, FormState},
        responses,
    },
    domain::{
        normalize_search_text, AttachmentId, InterventionId, OpaqueCursor, Page, PageLimit,
        PageRequest, TechnicalNoteId, ValidationCode, ValidationError, ValidationErrors, VehicleId,
    },
    models::{
        attachment::{
            AttachmentMetadata, AttachmentOwner, CAPTION_MAX_CHARS, DISPLAY_NAME_MAX_CHARS,
        },
        intervention::Intervention,
        technical_note::{
            NewTechnicalNote, BODY_MAX_CHARS, ENGINE_MAX_CHARS, MAKE_MAX_CHARS, MODEL_MAX_CHARS,
            TAG_MAX_CHARS, TAG_MAX_COUNT, TITLE_MAX_CHARS,
        },
        vehicle::Vehicle,
    },
    repositories::{
        customer::ArchiveFilter, intervention::InterventionFilter,
        technical_note::TechnicalNoteFilter, vehicle::VehicleFilter,
    },
    services::{
        attachment::{AttachmentService, UploadAttachment, WriteAttachmentMetadata},
        intervention::InterventionService,
        technical_note::{validate_write, TechnicalNoteService},
        vehicle::VehicleService,
        WorkflowError,
    },
    settings::{BusinessSettings, MULTIPART_ENVELOPE_BYTES},
    views::{
        knowledge::{
            KnowledgeDetailPage, KnowledgeFilterValues, KnowledgeFormPage, KnowledgeFormValues,
            KnowledgeListPage,
        },
        layout::AuthenticatedLayout,
        vehicle::{AttachmentFormPage, AttachmentFormValues},
    },
};

const FORM_FIELDS: &[&str] = &[
    "title",
    "body",
    "tags",
    "make",
    "model",
    "engine",
    "vehicle_id",
    "source_intervention_id",
];
// Preserve the former global 64 KiB ceiling now that multipart raises the global middleware.
const FORM_BODY_LIMIT: usize = 64 * 1_024;
const ATTACHMENT_FORM_FIELDS: &[&str] = &["file", "display_name", "caption"];

#[derive(Debug, Default, Deserialize)]
struct NewNoteQuery {
    #[serde(default)]
    vehicle: String,
    #[serde(default)]
    source_intervention: String,
}

async fn list(
    context: BrowserRequestContext,
    SharedStore(notes): SharedStore<TechnicalNoteService>,
    SharedStore(settings): SharedStore<BusinessSettings>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Query(filters): Query<KnowledgeFilterValues>,
) -> Result<Response> {
    let filter = match parse_filter(&filters) {
        Ok(value) => value,
        Err(message) => {
            return render_list(
                &context,
                &engine,
                filters,
                empty_page(),
                None,
                Some(message),
                StatusCode::UNPROCESSABLE_ENTITY,
            )
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
                None,
                Some(message),
                StatusCode::UNPROCESSABLE_ENTITY,
            )
        }
    };
    let page = match notes
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
                None,
                Some("This page link does not match the current knowledge filters.".to_owned()),
                StatusCode::UNPROCESSABLE_ENTITY,
            )
        }
        Err(error) => return Ok(workflow_response(&context, error, "technical-note list")),
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
        next_href,
        None,
        StatusCode::OK,
    )
}

fn render_list(
    context: &BrowserRequestContext,
    engine: &TeraView,
    mut filters: KnowledgeFilterValues,
    page: Page<crate::models::technical_note::TechnicalNote>,
    next_href: Option<String>,
    filter_error: Option<String>,
    status: StatusCode,
) -> Result<Response> {
    filters.cursor.clear();
    let view = KnowledgeListPage::new(layout(context), filters, page, next_href, filter_error);
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
    SharedStore(interventions): SharedStore<InterventionService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Query(query): Query<NewNoteQuery>,
) -> Result<Response> {
    let mut values = KnowledgeFormValues::default();
    let mut conflict = None;
    if !query.source_intervention.is_empty() {
        let source_id = match InterventionId::parse(query.source_intervention) {
            Ok(value) => value,
            Err(_) => {
                return Ok(responses::not_found(
                    context.response_preference,
                    "source intervention",
                ))
            }
        };
        let source = match interventions.get(&source_id).await {
            Ok(value) => value,
            Err(error) => return Ok(workflow_response(&context, error, "source intervention")),
        };
        let vehicle = match vehicles.get(&source.vehicle_id).await {
            Ok(value) => value,
            Err(error) => return Ok(workflow_response(&context, error, "source vehicle")),
        };
        if !query.vehicle.is_empty() && query.vehicle != vehicle.id.as_str() {
            conflict = Some(
                "The requested vehicle does not match the source intervention. Choose the source vehicle or remove the source before saving."
                    .to_owned(),
            );
        }
        prefill_vehicle(&mut values, &vehicle);
        values.source_intervention_id = source.id.as_str().to_owned();
    } else if !query.vehicle.is_empty() {
        let vehicle_id = match VehicleId::parse(query.vehicle) {
            Ok(value) => value,
            Err(_) => return Ok(responses::not_found(context.response_preference, "vehicle")),
        };
        let vehicle = match vehicles.get(&vehicle_id).await {
            Ok(value) => value,
            Err(error) => return Ok(workflow_response(&context, error, "vehicle")),
        };
        prefill_vehicle(&mut values, &vehicle);
    }
    render_form(
        &context,
        &engine,
        &vehicles,
        &interventions,
        false,
        "/knowledge".to_owned(),
        "/knowledge".to_owned(),
        FormState::new(values),
        conflict,
        StatusCode::OK,
    )
    .await
}

async fn create(
    context: BrowserRequestContext,
    SharedStore(notes): SharedStore<TechnicalNoteService>,
    SharedStore(vehicles): SharedStore<VehicleService>,
    SharedStore(interventions): SharedStore<InterventionService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    form: AuthenticatedForm<KnowledgeFormValues>,
) -> Result<Response> {
    let mut values = form.fields;
    if let Err(message) = apply_resolution(&mut values, &interventions).await {
        return render_form(
            &context,
            &engine,
            &vehicles,
            &interventions,
            false,
            "/knowledge".to_owned(),
            "/knowledge".to_owned(),
            FormState::new(values),
            Some(message),
            StatusCode::CONFLICT,
        )
        .await;
    }
    let command = match command(&values) {
        Ok(value) => value,
        Err(errors) => {
            return render_form(
                &context,
                &engine,
                &vehicles,
                &interventions,
                false,
                "/knowledge".to_owned(),
                "/knowledge".to_owned(),
                FormState::with_validation(values, &errors),
                None,
                StatusCode::UNPROCESSABLE_ENTITY,
            )
            .await
        }
    };
    match notes.create(command).await {
        Ok(note) => Ok(responses::redirect(
            context.response_preference,
            &format!("/knowledge/{}", note.id.as_str()),
        )),
        Err(WorkflowError::Validation(errors)) => {
            render_form(
                &context,
                &engine,
                &vehicles,
                &interventions,
                false,
                "/knowledge".to_owned(),
                "/knowledge".to_owned(),
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
                &vehicles,
                &interventions,
                false,
                "/knowledge".to_owned(),
                "/knowledge".to_owned(),
                FormState::new(values),
                Some(source_conflict()),
                StatusCode::CONFLICT,
            )
            .await
        }
        Err(error) => Ok(workflow_response(&context, error, "technical note")),
    }
}

async fn show(
    context: BrowserRequestContext,
    SharedStore(notes): SharedStore<TechnicalNoteService>,
    SharedStore(vehicles): SharedStore<VehicleService>,
    SharedStore(interventions): SharedStore<InterventionService>,
    SharedStore(attachments): SharedStore<AttachmentService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path(raw_id): Path<String>,
) -> Result<Response> {
    let id = match note_id(raw_id, &context) {
        Ok(value) => value,
        Err(response) => return Ok(response),
    };
    let note = match notes.get(&id).await {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(&context, error, "technical note")),
    };
    let vehicle = if let Some(id) = &note.vehicle_id {
        match vehicles.get(id).await {
            Ok(value) => Some(value),
            Err(error) => return Ok(workflow_response(&context, error, "related vehicle")),
        }
    } else {
        None
    };
    let source = if let Some(id) = &note.source_intervention_id {
        match interventions.get(id).await {
            Ok(value) => Some(value),
            Err(error) => return Ok(workflow_response(&context, error, "source intervention")),
        }
    } else {
        None
    };
    let attachment_metadata = match attachments.list(&AttachmentOwner::TechnicalNote(id)).await {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(&context, error, "attachment metadata")),
    };
    let view =
        KnowledgeDetailPage::new(layout(&context), note, vehicle, source, attachment_metadata);
    Ok(responses::render(
        context.response_preference,
        StatusCode::OK,
        view.render_page(&engine)?,
        view.render_content(&engine)?,
    ))
}

async fn edit_form(
    context: BrowserRequestContext,
    SharedStore(notes): SharedStore<TechnicalNoteService>,
    SharedStore(vehicles): SharedStore<VehicleService>,
    SharedStore(interventions): SharedStore<InterventionService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path(raw_id): Path<String>,
) -> Result<Response> {
    let id = match note_id(raw_id, &context) {
        Ok(value) => value,
        Err(response) => return Ok(response),
    };
    let note = match notes.get(&id).await {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(&context, error, "technical note")),
    };
    if note.is_archived() {
        return Ok(responses::redirect(
            context.response_preference,
            &format!("/knowledge/{}", id.as_str()),
        ));
    }
    render_form(
        &context,
        &engine,
        &vehicles,
        &interventions,
        true,
        format!("/knowledge/{}/edit", id.as_str()),
        format!("/knowledge/{}", id.as_str()),
        FormState::new(KnowledgeFormValues::from(&note)),
        None,
        StatusCode::OK,
    )
    .await
}

async fn update(
    context: BrowserRequestContext,
    SharedStore(notes): SharedStore<TechnicalNoteService>,
    SharedStore(vehicles): SharedStore<VehicleService>,
    SharedStore(interventions): SharedStore<InterventionService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path(raw_id): Path<String>,
    form: AuthenticatedForm<KnowledgeFormValues>,
) -> Result<Response> {
    let id = match note_id(raw_id, &context) {
        Ok(value) => value,
        Err(response) => return Ok(response),
    };
    let current = match notes.get(&id).await {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(&context, error, "technical note")),
    };
    let action = format!("/knowledge/{}/edit", id.as_str());
    let cancel = format!("/knowledge/{}", id.as_str());
    if current.is_archived() {
        return render_form(
            &context,
            &engine,
            &vehicles,
            &interventions,
            true,
            action,
            cancel,
            FormState::new(KnowledgeFormValues::from(&current)),
            Some("This technical note was archived. Reload latest or restore it from the detail page."
                .to_owned()),
            StatusCode::CONFLICT,
        )
        .await;
    }
    let mut values = form.fields;
    if let Err(message) = apply_resolution(&mut values, &interventions).await {
        return render_form(
            &context,
            &engine,
            &vehicles,
            &interventions,
            true,
            action,
            cancel,
            FormState::new(values),
            Some(message),
            StatusCode::CONFLICT,
        )
        .await;
    }
    let command = match command(&values) {
        Ok(value) => value,
        Err(errors) => {
            return render_form(
                &context,
                &engine,
                &vehicles,
                &interventions,
                true,
                action,
                cancel,
                FormState::with_validation(values, &errors),
                None,
                StatusCode::UNPROCESSABLE_ENTITY,
            )
            .await
        }
    };
    match notes.update(&id, command).await {
        Ok(_) => Ok(responses::redirect(context.response_preference, &cancel)),
        Err(WorkflowError::Validation(errors)) => {
            render_form(
                &context,
                &engine,
                &vehicles,
                &interventions,
                true,
                action,
                cancel,
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
                &vehicles,
                &interventions,
                true,
                action,
                cancel,
                FormState::new(values),
                Some(source_conflict()),
                StatusCode::CONFLICT,
            )
            .await
        }
        Err(error) => Ok(workflow_response(&context, error, "technical note")),
    }
}

async fn archive(
    context: BrowserRequestContext,
    SharedStore(notes): SharedStore<TechnicalNoteService>,
    Path(raw_id): Path<String>,
    _form: AuthenticatedForm<LifecycleForm>,
) -> Result<Response> {
    lifecycle(context, notes, raw_id, false).await
}

async fn restore(
    context: BrowserRequestContext,
    SharedStore(notes): SharedStore<TechnicalNoteService>,
    Path(raw_id): Path<String>,
    _form: AuthenticatedForm<LifecycleForm>,
) -> Result<Response> {
    lifecycle(context, notes, raw_id, true).await
}

#[derive(Debug, Deserialize)]
struct LifecycleForm {}

async fn lifecycle(
    context: BrowserRequestContext,
    notes: TechnicalNoteService,
    raw_id: String,
    restoring: bool,
) -> Result<Response> {
    let id = match note_id(raw_id, &context) {
        Ok(value) => value,
        Err(response) => return Ok(response),
    };
    let result = if restoring {
        notes.restore(&id).await
    } else {
        notes.archive(&id).await
    };
    match result {
        Ok(_) => Ok(responses::redirect(
            context.response_preference,
            &format!("/knowledge/{}", id.as_str()),
        )),
        Err(error) => Ok(workflow_response(&context, error, "technical note")),
    }
}

async fn new_attachment_form(
    context: BrowserRequestContext,
    SharedStore(notes): SharedStore<TechnicalNoteService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path(raw_id): Path<String>,
) -> Result<Response> {
    let note = match get_active_note(&notes, raw_id, &context).await {
        Ok(value) => value,
        Err(response) => return Ok(response),
    };
    render_attachment_form(
        &context,
        &engine,
        &note,
        None,
        FormState::new(AttachmentFormValues::default()),
        None,
        StatusCode::OK,
    )
}

async fn create_attachment(
    context: BrowserRequestContext,
    SharedStore(notes): SharedStore<TechnicalNoteService>,
    SharedStore(attachments): SharedStore<AttachmentService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path(raw_id): Path<String>,
    upload: AuthenticatedAttachmentMultipart,
) -> Result<Response> {
    let note = match get_active_note(&notes, raw_id, &context).await {
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
        .upload(AttachmentOwner::TechnicalNote(note.id.clone()), command)
        .await
    {
        Ok(_) => Ok(responses::redirect(
            context.response_preference,
            &format!("/knowledge/{}", note.id.as_str()),
        )),
        Err(WorkflowError::Validation(errors)) => render_attachment_form(
            &context,
            &engine,
            &note,
            None,
            FormState::with_validation(values, &errors),
            None,
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        Err(WorkflowError::Conflict) => render_attachment_form(
            &context,
            &engine,
            &note,
            None,
            FormState::new(values),
            Some("The technical note was archived before this file could be stored. Restore it, reselect the file, and try again.".to_owned()),
            StatusCode::CONFLICT,
        ),
        Err(error) => Ok(workflow_response(&context, error, "attachment")),
    }
}

async fn edit_attachment_form(
    context: BrowserRequestContext,
    SharedStore(notes): SharedStore<TechnicalNoteService>,
    SharedStore(attachments): SharedStore<AttachmentService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path((raw_id, raw_attachment_id)): Path<(String, String)>,
) -> Result<Response> {
    let (note, attachment) =
        match technical_note_attachment(&notes, &attachments, raw_id, raw_attachment_id, &context)
            .await
        {
            Ok(value) => value,
            Err(response) => return Ok(response),
        };
    if note.is_archived() {
        return Ok(note_redirect(&context, &note.id));
    }
    let attachment_id = attachment.id.as_str().to_owned();
    render_attachment_form(
        &context,
        &engine,
        &note,
        Some(&attachment_id),
        FormState::new(attachment.into()),
        None,
        StatusCode::OK,
    )
}

async fn update_attachment(
    context: BrowserRequestContext,
    SharedStore(notes): SharedStore<TechnicalNoteService>,
    SharedStore(attachments): SharedStore<AttachmentService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path((raw_id, raw_attachment_id)): Path<(String, String)>,
    form: AuthenticatedForm<AttachmentFormValues>,
) -> Result<Response> {
    let (note, attachment) =
        match technical_note_attachment(&notes, &attachments, raw_id, raw_attachment_id, &context)
            .await
        {
            Ok(value) => value,
            Err(response) => return Ok(response),
        };
    if note.is_archived() {
        return Ok(note_redirect(&context, &note.id));
    }
    let attachment_id = attachment.id.as_str().to_owned();
    let values = form.fields;
    let command = match attachment_update_command(&values, &attachment) {
        Ok(value) => value,
        Err(errors) => {
            return render_attachment_form(
                &context,
                &engine,
                &note,
                Some(&attachment_id),
                FormState::with_validation(values, &errors),
                None,
                StatusCode::UNPROCESSABLE_ENTITY,
            )
        }
    };
    match attachments.update(&attachment.id, command).await {
        Ok(_) => Ok(note_redirect(&context, &note.id)),
        Err(WorkflowError::Validation(errors)) => render_attachment_form(
            &context,
            &engine,
            &note,
            Some(&attachment_id),
            FormState::with_validation(values, &errors),
            None,
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        Err(WorkflowError::Conflict) => render_attachment_form(
            &context,
            &engine,
            &note,
            Some(&attachment_id),
            FormState::new(values),
            Some(
                "The technical note was archived before these attachment details could be saved."
                    .to_owned(),
            ),
            StatusCode::CONFLICT,
        ),
        Err(error) => Ok(workflow_response(&context, error, "attachment")),
    }
}

async fn delete_attachment(
    context: BrowserRequestContext,
    SharedStore(notes): SharedStore<TechnicalNoteService>,
    SharedStore(attachments): SharedStore<AttachmentService>,
    Path((raw_id, raw_attachment_id)): Path<(String, String)>,
    _form: AuthenticatedForm<LifecycleForm>,
) -> Result<Response> {
    let (note, attachment) =
        match technical_note_attachment(&notes, &attachments, raw_id, raw_attachment_id, &context)
            .await
        {
            Ok(value) => value,
            Err(response) => return Ok(response),
        };
    if note.is_archived() {
        return Ok(note_redirect(&context, &note.id));
    }
    match attachments.delete(&attachment.id).await {
        Ok(()) | Err(WorkflowError::Conflict) => Ok(note_redirect(&context, &note.id)),
        Err(error) => Ok(workflow_response(&context, error, "attachment")),
    }
}

fn render_attachment_form(
    context: &BrowserRequestContext,
    engine: &TeraView,
    note: &crate::models::technical_note::TechnicalNote,
    attachment_id: Option<&str>,
    form: FormState<AttachmentFormValues>,
    conflict: Option<String>,
    status: StatusCode,
) -> Result<Response> {
    let view = AttachmentFormPage::for_technical_note(
        layout(context),
        note,
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

async fn technical_note_attachment(
    notes: &TechnicalNoteService,
    attachments: &AttachmentService,
    raw_id: String,
    raw_attachment_id: String,
    context: &BrowserRequestContext,
) -> std::result::Result<
    (
        crate::models::technical_note::TechnicalNote,
        AttachmentMetadata,
    ),
    Response,
> {
    let note_id = note_id(raw_id, context)?;
    let note = notes
        .get(&note_id)
        .await
        .map_err(|error| workflow_response(context, error, "technical note"))?;
    let attachment_id = AttachmentId::parse(raw_attachment_id)
        .map_err(|_| responses::not_found(context.response_preference, "attachment metadata"))?;
    let attachment = attachments
        .get(&attachment_id)
        .await
        .map_err(|error| workflow_response(context, error, "attachment metadata"))?;
    if attachment.owner != AttachmentOwner::TechnicalNote(note_id) {
        return Err(responses::not_found(
            context.response_preference,
            "technical note attachment metadata",
        ));
    }
    Ok((note, attachment))
}

async fn get_active_note(
    notes: &TechnicalNoteService,
    raw_id: String,
    context: &BrowserRequestContext,
) -> std::result::Result<crate::models::technical_note::TechnicalNote, Response> {
    let id = note_id(raw_id, context)?;
    let note = notes
        .get(&id)
        .await
        .map_err(|error| workflow_response(context, error, "technical note"))?;
    if note.is_archived() {
        return Err(note_redirect(context, &id));
    }
    Ok(note)
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
        DISPLAY_NAME_MAX_CHARS,
    );
    optional(&mut errors, "caption", &values.caption, CAPTION_MAX_CHARS);
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

fn note_redirect(context: &BrowserRequestContext, id: &TechnicalNoteId) -> Response {
    responses::redirect(
        context.response_preference,
        &format!("/knowledge/{}", id.as_str()),
    )
}

#[allow(clippy::too_many_arguments)]
async fn render_form(
    context: &BrowserRequestContext,
    engine: &TeraView,
    vehicles: &VehicleService,
    interventions: &InterventionService,
    editing: bool,
    action: String,
    cancel_href: String,
    form: FormState<KnowledgeFormValues>,
    conflict: Option<String>,
    status: StatusCode,
) -> Result<Response> {
    let selected_vehicle = form.values.vehicle_id.clone();
    let selected_source = form.values.source_intervention_id.clone();
    let (mut vehicle_options, mut source_options) = match options(vehicles, interventions).await {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(context, error, "knowledge context")),
    };
    if !selected_vehicle.is_empty()
        && !vehicle_options
            .iter()
            .any(|vehicle| vehicle.id.as_str() == selected_vehicle)
    {
        if let Ok(id) = VehicleId::parse(selected_vehicle) {
            if let Ok(vehicle) = vehicles.get(&id).await {
                vehicle_options.push(vehicle);
            }
        }
    }
    if !selected_source.is_empty()
        && !source_options
            .iter()
            .any(|(source, _)| source.id.as_str() == selected_source)
    {
        if let Ok(id) = InterventionId::parse(selected_source) {
            if let Ok(source) = interventions.get(&id).await {
                if let Ok(vehicle) = vehicles.get(&source.vehicle_id).await {
                    source_options.push((source, vehicle));
                }
            }
        }
    }
    let view = KnowledgeFormPage::new(
        layout(context),
        editing,
        action,
        cancel_href,
        form.with_known_fields(FORM_FIELDS),
        vehicle_options,
        source_options,
        conflict,
    );
    Ok(responses::render(
        context.response_preference,
        status,
        view.render_page(engine)?,
        view.render_form(engine)?,
    ))
}

async fn options(
    vehicles: &VehicleService,
    interventions: &InterventionService,
) -> std::result::Result<(Vec<Vehicle>, Vec<(Intervention, Vehicle)>), WorkflowError> {
    let vehicle_options = vehicles
        .list(PageRequest {
            filter: VehicleFilter {
                archive: ArchiveFilter::All,
                ..VehicleFilter::default()
            },
            limit: maximum_limit(),
            after: None,
        })
        .await?
        .items;
    let summaries = interventions
        .list(PageRequest {
            filter: InterventionFilter::default(),
            limit: maximum_limit(),
            after: None,
        })
        .await?
        .items;
    let mut source_options = Vec::with_capacity(summaries.len());
    for summary in summaries {
        let intervention = interventions.get(&summary.intervention.id).await?;
        let vehicle = vehicles.get(&intervention.vehicle_id).await?;
        source_options.push((intervention, vehicle));
    }
    Ok((vehicle_options, source_options))
}

async fn apply_resolution(
    values: &mut KnowledgeFormValues,
    interventions: &InterventionService,
) -> std::result::Result<(), String> {
    if values.source_intervention_id.is_empty() {
        return Ok(());
    }
    if values.resolution == "remove_source" {
        values.source_intervention_id.clear();
        values.resolution.clear();
        return Ok(());
    }
    if values.vehicle_id.is_empty() || values.resolution == "source_vehicle" {
        if values.resolution != "source_vehicle" {
            return Err(source_conflict());
        }
        let id = InterventionId::parse(values.source_intervention_id.clone())
            .map_err(|_| source_conflict())?;
        let source = interventions
            .get(&id)
            .await
            .map_err(|_| source_conflict())?;
        values.vehicle_id = source.vehicle_id.as_str().to_owned();
        values.resolution.clear();
    }
    Ok(())
}

fn command(
    values: &KnowledgeFormValues,
) -> std::result::Result<NewTechnicalNote, ValidationErrors> {
    let tags = parse_tags(&values.tags)?;
    let mut errors = Vec::new();
    required(&mut errors, "title", &values.title, TITLE_MAX_CHARS);
    required(&mut errors, "body", &values.body, BODY_MAX_CHARS);
    optional(&mut errors, "make", &values.make, MAKE_MAX_CHARS);
    optional(&mut errors, "model", &values.model, MODEL_MAX_CHARS);
    optional(&mut errors, "engine", &values.engine, ENGINE_MAX_CHARS);
    let vehicle_id = parse_optional_id(
        &values.vehicle_id,
        "vehicle_id",
        "Choose a valid related vehicle.",
        VehicleId::parse,
        &mut errors,
    );
    let source_id = parse_optional_id(
        &values.source_intervention_id,
        "source_intervention_id",
        "Choose a valid source intervention.",
        InterventionId::parse,
        &mut errors,
    );
    if let Some(errors) = ValidationErrors::from_vec(errors) {
        return Err(errors);
    }
    validate_write(
        values.title.clone(),
        values.body.clone(),
        tags,
        vehicle_id,
        source_id,
        optional_value(&values.make),
        optional_value(&values.model),
        optional_value(&values.engine),
    )
    .map_err(|error| match error {
        WorkflowError::Validation(errors) => errors,
        _ => validation_errors("title", "Check the technical-note values."),
    })
}

fn parse_tags(value: &str) -> std::result::Result<Vec<String>, ValidationErrors> {
    let raw = value
        .lines()
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .collect::<Vec<_>>();
    let mut tags = Vec::with_capacity(raw.len());
    for tag in raw {
        if tag.chars().count() > TAG_MAX_CHARS {
            return Err(validation_errors(
                "tags",
                "Each tag must be 64 characters or fewer.",
            ));
        }
        let normalized = normalize_search_text(tag);
        if !tags.contains(&normalized) {
            tags.push(normalized);
        }
    }
    if tags.len() > TAG_MAX_COUNT {
        return Err(validation_errors("tags", "Use no more than 20 tags."));
    }
    Ok(tags)
}

fn required(errors: &mut Vec<ValidationError>, field: &str, value: &str, maximum: usize) {
    if value.trim().is_empty() {
        errors.push(error(field, ValidationCode::Required, "Enter a value."));
    } else if value.trim().chars().count() > maximum {
        errors.push(error(
            field,
            ValidationCode::TooLong,
            format!("Use {maximum} characters or fewer."),
        ));
    }
}

fn optional(errors: &mut Vec<ValidationError>, field: &str, value: &str, maximum: usize) {
    if value.trim().chars().count() > maximum {
        errors.push(error(
            field,
            ValidationCode::TooLong,
            format!("Use {maximum} characters or fewer."),
        ));
    }
}

fn parse_optional_id<T, E>(
    value: &str,
    field: &str,
    message: &str,
    parser: impl FnOnce(String) -> std::result::Result<T, E>,
    errors: &mut Vec<ValidationError>,
) -> Option<T> {
    if value.is_empty() {
        return None;
    }
    match parser(value.to_owned()) {
        Ok(value) => Some(value),
        Err(_) => {
            errors.push(error(field, ValidationCode::InvalidFormat, message));
            None
        }
    }
}

fn optional_value(value: &str) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.to_owned())
}

fn error(field: &str, code: ValidationCode, message: impl Into<String>) -> ValidationError {
    ValidationError::new(field, code, message).expect("static field path is valid")
}

fn validation_errors(field: &str, message: &str) -> ValidationErrors {
    ValidationErrors::one(error(field, ValidationCode::InvalidFormat, message))
}

fn parse_filter(
    values: &KnowledgeFilterValues,
) -> std::result::Result<TechnicalNoteFilter, String> {
    let tags =
        parse_tags(&values.tags).map_err(|errors| errors.as_slice()[0].message().to_owned())?;
    let archive = match values.archived.as_str() {
        "" | "active" => ArchiveFilter::Active,
        "archived" => ArchiveFilter::Archived,
        "all" => ArchiveFilter::All,
        _ => return Err("Choose Active, Archived, or All notes.".to_owned()),
    };
    Ok(TechnicalNoteFilter {
        query: normalized_optional(&values.q),
        tags,
        make: normalized_optional(&values.make),
        model: normalized_optional(&values.model),
        engine: normalized_optional(&values.engine),
        archive,
    })
}

fn normalized_optional(value: &str) -> Option<String> {
    let value = normalize_search_text(value);
    (!value.is_empty()).then_some(value)
}

fn parse_cursor(value: &str) -> std::result::Result<Option<OpaqueCursor>, String> {
    if value.is_empty() {
        Ok(None)
    } else {
        OpaqueCursor::parse(value.to_owned())
            .map(Some)
            .map_err(|_| "Use a page link returned by this knowledge search.".to_owned())
    }
}

fn list_href(values: &KnowledgeFilterValues) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    for (key, value) in [
        ("q", values.q.as_str()),
        ("tags", values.tags.as_str()),
        ("make", values.make.as_str()),
        ("model", values.model.as_str()),
        ("engine", values.engine.as_str()),
        ("archived", values.archived.as_str()),
        ("cursor", values.cursor.as_str()),
    ] {
        if !value.is_empty() {
            serializer.append_pair(key, value);
        }
    }
    format!("/knowledge?{}", serializer.finish())
}

fn prefill_vehicle(values: &mut KnowledgeFormValues, vehicle: &Vehicle) {
    values.vehicle_id = vehicle.id.as_str().to_owned();
    values.make.clone_from(&vehicle.make);
    values.model.clone_from(&vehicle.model);
    values.engine = vehicle.engine_type.clone().unwrap_or_default();
}

fn source_conflict() -> String {
    "The selected source intervention and vehicle are inconsistent. Use the source vehicle, remove or change the source, or Reload latest."
        .to_owned()
}

fn empty_page() -> Page<crate::models::technical_note::TechnicalNote> {
    Page {
        items: Vec::new(),
        next_cursor: None,
    }
}

fn maximum_limit() -> PageLimit {
    PageLimit::new(200).expect("maximum page limit is valid")
}

#[allow(clippy::result_large_err)]
fn note_id(raw_id: String, context: &BrowserRequestContext) -> Result<TechnicalNoteId, Response> {
    TechnicalNoteId::parse(raw_id)
        .map_err(|_| responses::not_found(context.response_preference, "technical note"))
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
            "Technical knowledge is temporarily unavailable. Try again shortly.",
        ),
        WorkflowError::Validation(_) | WorkflowError::Conflict | WorkflowError::Internal => {
            responses::unexpected(context.response_preference)
        }
    }
}

#[must_use]
pub fn routes() -> Routes {
    Routes::new()
        .add("/knowledge", get(list))
        .add(
            "/knowledge",
            post(create).layer(DefaultBodyLimit::max(FORM_BODY_LIMIT)),
        )
        .add("/knowledge/new", get(new_form))
        .add("/knowledge/{id}", get(show))
        .add("/knowledge/{id}/edit", get(edit_form))
        .add(
            "/knowledge/{id}/edit",
            post(update).layer(DefaultBodyLimit::max(FORM_BODY_LIMIT)),
        )
        .add("/knowledge/{id}/archive", post(archive).layer(body_limit()))
        .add("/knowledge/{id}/restore", post(restore).layer(body_limit()))
        .add("/knowledge/{id}/attachments/new", get(new_attachment_form))
        .add(
            "/knowledge/{id}/attachments",
            post(create_attachment).layer(DefaultBodyLimit::max(MULTIPART_ENVELOPE_BYTES)),
        )
        .add(
            "/knowledge/{id}/attachments/{attachment_id}/edit",
            get(edit_attachment_form),
        )
        .add(
            "/knowledge/{id}/attachments/{attachment_id}/edit",
            post(update_attachment).layer(body_limit()),
        )
        .add(
            "/knowledge/{id}/attachments/{attachment_id}/delete",
            post(delete_attachment).layer(body_limit()),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knowledge_cursor_link_preserves_every_filter() {
        let href = list_href(&KnowledgeFilterValues {
            q: "water pump".to_owned(),
            tags: "cooling\nVolkswagen".to_owned(),
            make: "Volkswagen".to_owned(),
            model: "Golf".to_owned(),
            engine: "1.4 TSI".to_owned(),
            archived: "archived".to_owned(),
            cursor: "opaque_cursor".to_owned(),
        });
        assert!(href.contains("q=water+pump"));
        assert!(href.contains("tags=cooling%0AVolkswagen"));
        assert!(href.contains("cursor=opaque_cursor"));
    }

    #[test]
    fn browser_tags_preserve_normalized_unique_order_and_limits() {
        assert_eq!(
            parse_tags(" Cooling \nVW\nvw\n Brakes ").expect("valid tags"),
            vec!["cooling", "vw", "brakes"]
        );
        let too_many = (0..=TAG_MAX_COUNT)
            .map(|index| format!("tag-{index}"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(parse_tags(&too_many).is_err());
    }
}

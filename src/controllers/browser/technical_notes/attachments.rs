use super::forms::*;
use super::*;
use crate::controllers::browser::attachments::forms::ATTACHMENT_FORM_FIELDS;
pub(super) async fn new_attachment_form(
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

pub(super) async fn create_attachment(
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

pub(super) async fn edit_attachment_form(
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

pub(super) async fn update_attachment(
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

pub(super) async fn delete_attachment(
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

pub(super) fn render_attachment_form(
    context: &BrowserRequestContext,
    engine: &TeraView,
    note: &crate::models::technical_note::TechnicalNote,
    attachment_id: Option<&str>,
    form: FormState<AttachmentFormValues>,
    conflict: Option<String>,
    status: StatusCode,
) -> Result<Response> {
    let view = AttachmentFormPage::for_technical_note(
        context.layout(),
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

pub(super) async fn technical_note_attachment(
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

pub(super) async fn get_active_note(
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

pub(super) fn attachment_update_command(
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

pub(super) fn note_redirect(context: &BrowserRequestContext, id: &TechnicalNoteId) -> Response {
    responses::redirect(
        context.response_preference,
        &format!("/knowledge/{}", id.as_str()),
    )
}

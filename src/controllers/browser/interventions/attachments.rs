use super::forms::*;
use super::*;
use crate::controllers::browser::attachments::forms::ATTACHMENT_FORM_FIELDS;
pub(super) async fn new_attachment_form(
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
    if vehicle.is_archived() {
        return Ok(detail_redirect(&context, &id));
    }
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

pub(super) async fn create_attachment(
    context: BrowserRequestContext,
    SharedStore(interventions): SharedStore<InterventionService>,
    SharedStore(vehicles): SharedStore<VehicleService>,
    SharedStore(attachments): SharedStore<AttachmentService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path(raw_id): Path<String>,
    upload: AuthenticatedAttachmentMultipart,
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
        .upload(AttachmentOwner::Intervention(id.clone()), command)
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
            Some("The intervention or vehicle changed state before this file could be stored. Return to the record, reselect the file, and try again if it is still editable.".to_owned()),
            StatusCode::CONFLICT,
        ),
        Err(error) => Ok(workflow_response(&context, error, "attachment")),
    }
}

pub(super) async fn edit_attachment_form(
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
    if vehicle.is_archived() {
        return Ok(detail_redirect(&context, &intervention.id));
    }
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

pub(super) async fn update_attachment(
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
    let command = match attachment_update_command(&values, &attachment) {
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
            Some("The intervention or vehicle changed state before these attachment details were saved.".to_owned()),
            StatusCode::CONFLICT,
        ),
        Err(error) => Ok(workflow_response(&context, error, "attachment")),
    }
}

pub(super) async fn delete_attachment(
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
        Err(error) => Ok(workflow_response(&context, error, "attachment")),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render_intervention_attachment_form(
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
        context.layout(),
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

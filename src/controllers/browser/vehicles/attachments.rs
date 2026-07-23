use super::forms::*;
use super::*;
use crate::controllers::browser::attachments::forms::ATTACHMENT_FORM_FIELDS;
pub(super) async fn new_attachment_form(
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

pub(super) async fn create_attachment(
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

pub(super) async fn edit_attachment_form(
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

pub(super) async fn update_attachment(
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

pub(super) async fn delete_attachment(
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

pub(super) fn render_attachment_form(
    context: &BrowserRequestContext,
    engine: &TeraView,
    vehicle: &crate::models::vehicle::Vehicle,
    attachment_id: Option<&str>,
    form: FormState<AttachmentFormValues>,
    conflict: Option<String>,
    status: StatusCode,
) -> Result<Response> {
    let view = AttachmentFormPage::new(
        context.layout(),
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

pub(super) async fn vehicle_attachment(
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

pub(super) async fn get_active_vehicle(
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

use super::forms::*;
use super::*;
pub(super) async fn list(
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

pub(super) fn render_list(
    context: &BrowserRequestContext,
    engine: &TeraView,
    mut filters: KnowledgeFilterValues,
    page: Page<crate::models::technical_note::TechnicalNote>,
    next_href: Option<String>,
    filter_error: Option<String>,
    status: StatusCode,
) -> Result<Response> {
    filters.cursor.clear();
    let view = KnowledgeListPage::new(context.layout(), filters, page, next_href, filter_error);
    Ok(responses::render(
        context.response_preference,
        status,
        view.render_page(engine)?,
        view.render_content(engine)?,
    ))
}

pub(super) async fn new_form(
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

pub(super) async fn create(
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

pub(super) async fn show(
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
        KnowledgeDetailPage::new(context.layout(), note, vehicle, source, attachment_metadata);
    Ok(responses::render(
        context.response_preference,
        StatusCode::OK,
        view.render_page(&engine)?,
        view.render_content(&engine)?,
    ))
}

pub(super) async fn edit_form(
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

pub(super) async fn update(
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

use super::forms::*;
use super::*;
pub(super) async fn new_line_form(
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

pub(super) async fn create_line(
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

pub(super) async fn edit_line_form(
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

pub(super) async fn update_line(
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

pub(super) async fn delete_line(
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

pub(super) async fn move_line_up(
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

pub(super) async fn move_line_down(
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

pub(super) async fn move_line(
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

pub(super) fn line_mutation_response(
    context: &BrowserRequestContext,
    engine: &TeraView,
    intervention: &Intervention,
    result: LineMutationResult,
) -> Result<Response> {
    if context.response_preference == ResponsePreference::FullPage {
        return Ok(detail_redirect(context, &intervention.id));
    }
    let view =
        InterventionLineRegion::new(context.layout(), intervention, result.lines, result.totals);
    Ok(responses::fragment(StatusCode::OK, view.render(engine)?))
}

pub(super) fn render_line_form(
    context: &BrowserRequestContext,
    engine: &TeraView,
    intervention: &Intervention,
    line_id: Option<&str>,
    form: FormState<InterventionLineFormValues>,
    conflict: Option<String>,
    status: StatusCode,
) -> Result<Response> {
    let view = InterventionLineFormPage::new(
        context.layout(),
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

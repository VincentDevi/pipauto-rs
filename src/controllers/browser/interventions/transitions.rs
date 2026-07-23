use super::forms::*;
use super::*;
pub(super) async fn complete_confirmation(
    context: BrowserRequestContext,
    SharedStore(interventions): SharedStore<InterventionService>,
    SharedStore(vehicles): SharedStore<VehicleService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path(raw_id): Path<String>,
) -> Result<Response> {
    transition_confirmation(context, interventions, vehicles, engine, raw_id, true).await
}

pub(super) async fn cancel_confirmation(
    context: BrowserRequestContext,
    SharedStore(interventions): SharedStore<InterventionService>,
    SharedStore(vehicles): SharedStore<VehicleService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path(raw_id): Path<String>,
) -> Result<Response> {
    transition_confirmation(context, interventions, vehicles, engine, raw_id, false).await
}

pub(super) async fn transition_confirmation(
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
        context.layout(),
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
pub(super) async fn complete(
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
pub(super) async fn cancel(
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
pub(super) async fn transition(
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

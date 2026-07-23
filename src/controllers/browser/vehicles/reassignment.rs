use super::forms::*;
use super::*;
#[derive(Default, Deserialize)]
pub(super) struct ReassignQuery {
    #[serde(default)]
    customer_id: String,
}

pub(super) async fn reassign_form(
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
pub(super) struct ReassignForm {
    customer_id: String,
}

pub(super) async fn reassign(
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
pub(super) async fn render_reassign(
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
        context.layout(),
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

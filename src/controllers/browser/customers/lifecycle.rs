use super::*;
use super::{crud::*, forms::*};
pub(super) async fn archive(
    context: BrowserRequestContext,
    SharedStore(customers): SharedStore<CustomerService>,
    SharedStore(vehicles): SharedStore<VehicleService>,
    SharedStore(settings): SharedStore<BusinessSettings>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path(id): Path<String>,
    _form: AuthenticatedForm<LifecycleForm>,
) -> Result<Response> {
    lifecycle(
        context,
        customers,
        vehicles,
        settings,
        engine,
        id,
        Lifecycle::Archive,
    )
    .await
}

pub(super) async fn restore(
    context: BrowserRequestContext,
    SharedStore(customers): SharedStore<CustomerService>,
    SharedStore(vehicles): SharedStore<VehicleService>,
    SharedStore(settings): SharedStore<BusinessSettings>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path(id): Path<String>,
    _form: AuthenticatedForm<LifecycleForm>,
) -> Result<Response> {
    lifecycle(
        context,
        customers,
        vehicles,
        settings,
        engine,
        id,
        Lifecycle::Restore,
    )
    .await
}

#[derive(Deserialize)]
pub(super) struct LifecycleForm {}

#[derive(Clone, Copy)]
pub(super) enum Lifecycle {
    Archive,
    Restore,
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn lifecycle(
    context: BrowserRequestContext,
    customers: CustomerService,
    vehicles: VehicleService,
    settings: BusinessSettings,
    engine: TeraView,
    raw_id: String,
    action: Lifecycle,
) -> Result<Response> {
    let id = match CustomerId::parse(raw_id) {
        Ok(id) => id,
        Err(_) => {
            return Ok(responses::not_found(
                context.response_preference,
                "customer",
            ))
        }
    };
    let result = match action {
        Lifecycle::Archive => customers.archive(&id).await,
        Lifecycle::Restore => customers.restore(&id).await,
    };
    match result {
        Ok(_) => Ok(responses::redirect(
            context.response_preference,
            &format!("/customers/{}", id.as_str()),
        )),
        Err(WorkflowError::Conflict) => {
            render_detail(
                &context,
                &engine,
                &customers,
                &vehicles,
                &settings,
                &id,
                None,
                Some(
                    "The customer changed before this action completed. The latest state is shown.",
                ),
                StatusCode::CONFLICT,
            )
            .await
        }
        Err(error) => Ok(workflow_response(&context, error, "customer")),
    }
}

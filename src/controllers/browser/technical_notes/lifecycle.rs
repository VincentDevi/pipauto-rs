use super::forms::*;
use super::*;
pub(super) async fn archive(
    context: BrowserRequestContext,
    SharedStore(notes): SharedStore<TechnicalNoteService>,
    Path(raw_id): Path<String>,
    _form: AuthenticatedForm<LifecycleForm>,
) -> Result<Response> {
    lifecycle(context, notes, raw_id, false).await
}

pub(super) async fn restore(
    context: BrowserRequestContext,
    SharedStore(notes): SharedStore<TechnicalNoteService>,
    Path(raw_id): Path<String>,
    _form: AuthenticatedForm<LifecycleForm>,
) -> Result<Response> {
    lifecycle(context, notes, raw_id, true).await
}

#[derive(Debug, Deserialize)]
pub(super) struct LifecycleForm {}

pub(super) async fn lifecycle(
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

use super::*;
pub(super) async fn status(
    CurrentUser(_user): CurrentUser,
    SharedStore(models): SharedStore<ModelContext>,
    ViewEngine(engine): ViewEngine<TeraView>,
) -> Result<Response> {
    let status = if models.database().health().await.is_ok() {
        SetupStatus::connected()
    } else {
        SetupStatus::unavailable()
    };
    let mut response = format::html(&status.render(&engine)?)?;
    response
        .headers_mut()
        .insert(VARY, HeaderValue::from_static("HX-Request"));
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

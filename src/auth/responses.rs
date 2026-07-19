//! Cache policy shared by browser authentication and authenticated application routes.

use axum::{
    extract::Request,
    http::{header::CACHE_CONTROL, HeaderValue},
    middleware::Next,
    response::Response,
};

/// Force sensitive route responses, including extractor and framework errors, to remain uncached.
pub async fn no_store_layer(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

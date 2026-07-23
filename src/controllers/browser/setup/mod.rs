//! Authenticated database-status fragment retained for development diagnostics.

use axum::{
    http::{
        header::{CACHE_CONTROL, VARY},
        HeaderValue,
    },
    response::Response,
};
use loco_rs::{
    controller::extractor::shared_store::SharedStore,
    controller::{format, views::engines::TeraView, views::ViewEngine, Routes},
    prelude::get,
    Result,
};

use crate::{auth::extractors::CurrentUser, models::ModelContext, views::setup::SetupStatus};

mod status;

use status::*;

pub fn routes() -> Routes {
    Routes::new().add("/setup/status", get(status))
}

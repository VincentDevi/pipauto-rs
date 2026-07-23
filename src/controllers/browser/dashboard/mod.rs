//! Authenticated workshop dashboard backed by existing intervention collection capabilities.

use axum::{http::StatusCode, response::Response};
use loco_rs::{
    controller::{
        extractor::shared_store::SharedStore, views::engines::TeraView, views::ViewEngine, Routes,
    },
    prelude::get,
    Result,
};

use crate::{
    controllers::browser::{
        context::{BrowserRequestContext, ResponsePreference},
        responses,
    },
    domain::{PageLimit, PageRequest},
    models::intervention::{
        InterventionFilter, InterventionModel as InterventionService, InterventionStatus,
    },
    views::dashboard::{DashboardPage, InterventionSection},
};

mod index;

use index::*;

pub fn routes() -> Routes {
    Routes::new()
        .add("/", get(show))
        .add("/dashboard/recent-interventions", get(recent))
        .add("/dashboard/draft-interventions", get(drafts))
}

//! Authenticated Calendar query parsing and server-rendered Month and Week responses.

use axum::{
    extract::RawQuery,
    http::{HeaderValue, StatusCode},
    response::Response,
};
use chrono::NaiveDate;
use loco_rs::{
    controller::{
        extractor::shared_store::SharedStore, views::engines::TeraView, views::ViewEngine, Routes,
    },
    prelude::get,
    Result,
};

use crate::{
    controllers::browser::{context::BrowserRequestContext, responses},
    models::{calendar::CalendarModel as CalendarService, ModelError as WorkflowError},
    views::calendar::{CalendarBrowserPage, CalendarPage, CalendarState},
};

mod query;
mod show;

use show::*;

pub fn routes() -> Routes {
    Routes::new().add("/calendar", get(show))
}

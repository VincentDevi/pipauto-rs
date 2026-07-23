//! Pipauto's application library and architectural module boundaries.

pub mod api;
pub mod app;
pub mod auth;
pub mod controllers;
pub mod database;
pub mod domain;
pub mod errors;
pub mod initializers;
pub mod models;
pub mod settings;
pub mod tasks;
#[doc(hidden)]
pub mod testing;
pub mod views;

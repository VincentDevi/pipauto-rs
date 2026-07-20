//! Public HTTP behavior tests.
//!
//! Request tests may boot the application through `tests::support` and make HTTP requests against
//! public routes. They must not call private workflow functions or infrastructure adapters directly.

mod api_foundation;
mod auth;
mod browser_foundation;
mod browser_security;
mod customer_browser;
mod customers_vehicles;
mod dashboard;
mod html_rendering;
mod intervention_browser;
mod interventions;
mod invoice_browser;
mod invoices;
mod setup;
mod surrealdb_health;
mod technical_note_browser;
mod technical_notes_attachments;

//! Infrastructure integration tests, including database connectivity.
//!
//! Integration tests may use real or in-memory infrastructure configured through `tests::support`.
//! They must not depend on production credentials or duplicate public request assertions.

mod attachment_reconciliation;
mod attachment_schema;
mod attachment_service;
mod auth;
mod auth_repositories;
mod customer_vehicle_repositories;
mod database;
mod interventions;
mod invoices;
mod migration;
mod surrealdb_bucket_capability;
mod technical_note_attachment_repositories;

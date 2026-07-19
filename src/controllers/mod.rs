//! HTTP boundary for parsing requests and selecting responses.
//!
//! Controllers may depend on Axum and Loco HTTP types, services, and views. They must not contain
//! business rules, issue database queries, or depend directly on persistence adapters.

pub mod auth;
pub mod setup;
pub mod surrealdb_health;

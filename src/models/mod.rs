//! Database-independent domain values and invariants.
//!
//! Models may depend on the Rust standard library and domain-focused utility crates. They must not
//! depend on Loco, Axum, Tera, `SurrealDB`, controllers, views, or concrete repositories.

pub mod auth;
pub mod customer;
pub mod intervention;
pub mod intervention_line;
pub mod vehicle;

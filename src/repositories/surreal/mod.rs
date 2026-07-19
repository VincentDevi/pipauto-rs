//! `SurrealDB` implementations of repository contracts.
//!
//! This adapter may depend on `SurrealDB`, database infrastructure, models, and repository
//! contracts. It must not contain HTTP handling, template rendering, or business workflow decisions.

pub mod attachment;
pub mod auth;
pub mod customer;
pub mod health;
pub mod intervention;
pub mod invoice;
pub mod support;
pub mod technical_note;
pub mod vehicle;

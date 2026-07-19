//! `SurrealDB` implementations of repository contracts.
//!
//! This adapter may depend on `SurrealDB`, database infrastructure, models, and repository
//! contracts. It must not contain HTTP handling, template rendering, or business workflow decisions.

pub mod auth;
pub mod support;

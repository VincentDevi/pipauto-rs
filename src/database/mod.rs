//! `SurrealDB` settings, client creation, authentication, selection, and health checks.
//!
//! Database infrastructure may depend on `SurrealDB` and configuration types. It must not define
//! business repository contracts, application workflows, HTTP behavior, or template behavior.

pub mod client;
pub mod settings;

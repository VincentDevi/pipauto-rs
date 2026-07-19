//! Loco lifecycle adapters for installing infrastructure in the shared application store.
//!
//! Initializers may depend on Loco and concrete infrastructure from `database` and `views`. They
//! must not implement business workflows, HTTP handling, or persistence rules.

pub mod auth;
pub mod surrealdb;
pub mod view_engine;

//! Public HTTP behavior tests.
//!
//! Request tests may boot the application through `tests::support` and make HTTP requests against
//! public routes. They must not call private workflow functions or infrastructure adapters directly.

mod api_foundation;
mod auth;
mod setup;
mod surrealdb_health;

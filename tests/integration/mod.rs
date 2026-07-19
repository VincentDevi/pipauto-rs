//! Infrastructure integration tests, including database connectivity.
//!
//! Integration tests may use real or in-memory infrastructure configured through `tests::support`.
//! They must not depend on production credentials or duplicate public request assertions.

mod auth;
mod auth_repositories;
mod database;

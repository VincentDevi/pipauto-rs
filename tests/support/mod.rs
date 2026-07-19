//! Reusable test application bootstrapping, settings, fixtures, and helpers.
//!
//! Support code may depend on the public application API and test-only crates. It must not contain
//! test assertions, production behavior, or environment-specific credentials.

use surrealdb::{engine::any::Any, Surreal};

/// Apply the committed authentication schema to an isolated test database.
pub async fn apply_authentication_schema(client: &Surreal<Any>) {
    let schema = [
        include_str!("../../database/schema/authentication/user.surql"),
        include_str!("../../database/schema/authentication/auth_session.surql"),
        include_str!("../../database/schema/authentication/login_throttle.surql"),
    ]
    .join("\n");
    let response = client
        .query(schema)
        .await
        .expect("committed authentication schema should execute");
    response
        .check()
        .expect("committed authentication definitions should be valid");
}

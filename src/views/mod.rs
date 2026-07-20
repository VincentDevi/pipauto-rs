//! Server-rendered presentation types and Tera template invocation.
//!
//! Views may depend on Tera and read-only presentation data supplied by controllers. They must not
//! parse HTTP requests, implement business rules, or query repositories and databases.

pub mod auth;
pub mod context;
pub mod layout;
pub mod setup;
pub mod unavailable;

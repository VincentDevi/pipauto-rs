//! Server-rendered presentation types and Tera template invocation.
//!
//! Views may depend on Tera and read-only presentation data supplied by controllers. They must not
//! parse HTTP requests, implement business rules, or query model persistence and databases.

pub mod auth;
pub mod calendar;
pub mod context;
pub mod customer;
pub mod dashboard;
pub mod intervention;
pub mod invoice;
pub mod knowledge;
pub mod layout;
pub mod setup;
pub mod unavailable;
pub mod vehicle;

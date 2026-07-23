//! Cohesive business models, their workflows, and private persistence implementations.
//!
//! HTTP and presentation concerns stay outside this module. SurrealDB-specific query and row
//! details are private to the model that owns them.

pub mod attachment;
pub mod auth;
pub mod calendar;
pub mod context;
pub mod customer;
pub mod error;
pub mod intervention;
pub mod invoice;
pub(crate) mod persistence_error;
pub mod technical_note;
pub mod vehicle;

pub use context::ModelContext;
pub use error::ModelError;
pub use intervention::line as intervention_line;
pub use invoice::{line as invoice_line, payment};

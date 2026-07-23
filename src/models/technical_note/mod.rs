//! Technical-note data, validation, operations, associations, and private persistence.

mod domain;
mod operations;
pub(crate) mod persistence;
pub(crate) mod repository;

pub(crate) use crate::models::ModelError as WorkflowError;
pub use domain::*;
pub use operations::{validate_write, TechnicalNoteModel, WriteTechnicalNote};
pub use repository::TechnicalNoteFilter;

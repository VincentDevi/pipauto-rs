//! Calendar projections owned by the intervention model.

mod domain;
mod operations;

pub(crate) use crate::models::ModelError as WorkflowError;
pub use domain::*;
pub use operations::{CalendarModel, CalendarSchedule};

//! Shared HTTP DTOs. DTO construction is explicit and never derives from persistence row types.

pub mod errors;
pub mod ids;
pub mod money;
pub mod pagination;
pub mod quantity;
pub mod timestamps;

pub use errors::{ApiErrorBody, ErrorEnvelope, FieldErrorDto};
pub use money::MoneyDto;
pub use pagination::PaginationEnvelope;
pub use quantity::QuantityDto;
pub use timestamps::TimestampDto;

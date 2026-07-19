//! Shared HTTP DTOs. DTO construction is explicit and never derives from persistence row types.

pub mod errors;
pub mod ids;
pub mod money;
pub mod pagination;
pub mod quantity;
pub mod query;
pub mod responses;
pub mod timestamps;

pub use errors::{ApiErrorBody, ErrorEnvelope, FieldErrorsDto};
pub use money::MoneyDto;
pub use pagination::PaginationEnvelope;
pub use quantity::QuantityDto;
pub use query::{PaginationQuery, ResolvedPagination};
pub use responses::DataEnvelope;
pub use timestamps::TimestampDto;

#[cfg(test)]
mod tests {
    #[test]
    fn api_foundation_dtos_do_not_reference_surrealdb_types() {
        let api_sources = [
            include_str!("errors.rs"),
            include_str!("ids.rs"),
            include_str!("money.rs"),
            include_str!("pagination.rs"),
            include_str!("quantity.rs"),
            include_str!("query.rs"),
            include_str!("responses.rs"),
            include_str!("timestamps.rs"),
        ]
        .join("\n");

        assert!(!api_sources.contains("surrealdb"));
        assert!(!api_sources.contains("RecordId"));
    }
}

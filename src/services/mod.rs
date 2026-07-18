//! Application workflows coordinating domain models and repository contracts.
//!
//! Services may depend on `models`, repository contracts, and application error types. They must
//! not depend on HTTP frameworks, templates, database clients, or concrete persistence adapters.

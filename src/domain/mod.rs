//! Database- and transport-independent business primitives.
//!
//! This module deliberately has no Loco, Axum, Tera, or SurrealDB dependency. Values can only be
//! constructed through APIs that enforce their invariants.

pub mod archive;
pub mod id;
pub mod money;
pub mod normalization;
pub mod pagination;
pub mod quantity;
pub mod validation;

pub use archive::{ArchiveState, EntityTimestamps};
pub use id::{
    AttachmentId, CustomerId, EntityId, InterventionId, InvoiceId, PaymentId, TechnicalNoteId,
    VehicleId,
};
pub use money::{CurrencyCode, Money, MoneyError};
pub use normalization::{
    normalize_email, normalize_phone, normalize_search_text, NormalizedRegistration, NormalizedVin,
};
pub use pagination::{
    CollectionFilter, CursorCodec, CursorError, CursorTuple, OpaqueCursor, Page, PageLimit,
    PageRequest, PaginationError, MAX_PAGE_LIMIT, MIN_PAGE_LIMIT,
};
pub use quantity::{Quantity, QuantityError};
pub use validation::{FieldPath, ValidationCode, ValidationError, ValidationErrors};

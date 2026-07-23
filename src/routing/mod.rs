//! Route composition and auditable access metadata.

mod access;

pub(crate) use access::inventory_for;
pub use access::{AccessClass, ClassifiedRoutes, RouteAccess};

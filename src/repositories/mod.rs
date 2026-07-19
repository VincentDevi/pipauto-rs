//! Persistence contracts and their technology-specific adapters.
//!
//! Repository contracts may depend on domain models and persistence-neutral error types. Adapter
//! submodules may depend on their technology, but this boundary must not contain HTTP, templates,
//! or application workflow policy. Business-domain repository traits are intentionally deferred.

pub mod auth;
pub mod surreal;

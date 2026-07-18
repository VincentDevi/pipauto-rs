//! Integration-test entry point for Pipauto's public and infrastructure boundaries.
//!
//! This crate may depend on the public application API and reusable `support` helpers. It must not
//! duplicate production workflows or hide assertions inside shared fixtures.

mod integration;
mod requests;
mod support;

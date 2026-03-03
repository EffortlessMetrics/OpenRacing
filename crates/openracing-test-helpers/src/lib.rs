//! Shared test utilities for OpenRacing.
//!
//! This crate provides common test helpers, assertions, and fixtures
//! to reduce code duplication across the test suite.
//!
//! # Modules
//!
//! - [`mod@must`] - Unwrap helpers with good error messages and `#[track_caller]`
//! - [`assertions`] - Custom assertion macros for testing
//! - [`tracking`] - Allocation tracking for RT safety tests
//! - [`mock`] - Mock implementations for testing
//! - [`fixtures`] - Test fixture builders
//! - [`prelude`] - Convenience re-exports
//!
//! # Usage
//!
//! Add to your `Cargo.toml`:
//!
//! ```toml
//! [dev-dependencies]
//! openracing-test-helpers = { path = "crates/openracing-test-helpers" }
//! ```
//!
//! Then import the prelude:
//!
//! ```rust,ignore
//! use openracing_test_helpers::prelude::*;
//! ```

#![deny(unsafe_op_in_unsafe_fn)]
#![allow(clippy::unwrap_used, clippy::panic)]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod assertions;
pub mod must;
pub mod prelude;

#[cfg(feature = "tracking")]
#[cfg_attr(docsrs, doc(cfg(feature = "tracking")))]
pub mod tracking;

#[cfg(all(test, feature = "tracking"))]
#[global_allocator]
static GLOBAL_TEST: tracking::TrackingAllocator = tracking::TrackingAllocator;

#[cfg(feature = "mock")]
#[cfg_attr(docsrs, doc(cfg(feature = "mock")))]
pub mod mock;

#[cfg(feature = "fixtures")]
#[cfg_attr(docsrs, doc(cfg(feature = "fixtures")))]
pub mod fixtures;

pub use must::*;

#[cfg(feature = "tracking")]
pub use tracking::track;

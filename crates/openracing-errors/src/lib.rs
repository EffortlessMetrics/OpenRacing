//! Centralized error types for OpenRacing
//!
//! This crate provides a unified error handling system for the OpenRacing project,
//! supporting both real-time (RT) and non-RT code paths with appropriate safety
//! guarantees.
//!
//! # Architecture
//!
//! The error system is organized into several modules:
//!
//! - [`common`]: Top-level error types and classifications used across all crates
//! - [`rt`]: Real-time specific errors with RT-safe semantics
//! - [`device`]: Hardware and device-related errors
//! - [`profile`]: Profile and configuration errors
//! - [`validation`]: Input validation errors
//!
//! # RT Safety
//!
//! RT-specific error types are designed for use in real-time code paths:
//! - No heap allocations after initialization
//! - Copy semantics where possible
//! - Pre-allocated error codes
//!
//! # Example
//!
//! ```
//! use openracing_errors::prelude::*;
//!
//! fn process_torque(value: f32) -> Result<f32> {
//!     if value.abs() > 1.0 {
//!         return Err(ValidationError::out_of_range("torque", value, -1.0, 1.0).into());
//!     }
//!     Ok(value)
//! }
//! ```

#![deny(unsafe_op_in_unsafe_fn, clippy::unwrap_used)]
#![warn(missing_docs, rust_2018_idioms)]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod common;
pub mod device;
pub mod prelude;
pub mod profile;
pub mod rt;
pub mod validation;

pub use common::{ErrorCategory, ErrorContext, ErrorSeverity, OpenRacingError, ResultExt};
pub use device::DeviceError;
pub use profile::ProfileError;
pub use rt::RTError;
pub use validation::ValidationError;

/// A specialized `Result` type for OpenRacing operations.
pub type Result<T> = std::result::Result<T, OpenRacingError>;

/// A specialized `Result` type for real-time operations.
pub type RTResult<T = ()> = std::result::Result<T, RTError>;

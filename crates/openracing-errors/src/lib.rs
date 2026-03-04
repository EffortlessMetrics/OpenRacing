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
//!
//! # Pattern Matching on Errors
//!
//! ```
//! use openracing_errors::prelude::*;
//!
//! let err: OpenRacingError = RTError::TimingViolation.into();
//!
//! let message = match &err {
//!     OpenRacingError::RT(RTError::TimingViolation) => "timing issue",
//!     OpenRacingError::RT(rt) if rt.requires_safety_action() => "safety action needed",
//!     OpenRacingError::Device(_) => "device problem",
//!     _ => "other error",
//! };
//! assert_eq!(message, "timing issue");
//! ```
//!
//! # Error Category Classification
//!
//! ```
//! use openracing_errors::{OpenRacingError, RTError, DeviceError, ErrorCategory};
//!
//! let rt_err: OpenRacingError = RTError::PipelineFault.into();
//! assert_eq!(rt_err.category(), ErrorCategory::RT);
//!
//! let dev_err: OpenRacingError = DeviceError::not_found("wheel").into();
//! assert_eq!(dev_err.category(), ErrorCategory::Device);
//!
//! let cfg_err = OpenRacingError::config("missing section");
//! assert_eq!(cfg_err.category(), ErrorCategory::Config);
//! ```
//!
//! # Severity-Based Error Handling
//!
//! ```
//! use openracing_errors::{OpenRacingError, RTError, ErrorSeverity};
//!
//! // Critical errors require immediate attention
//! let critical: OpenRacingError = RTError::DeviceDisconnected.into();
//! assert_eq!(critical.severity(), ErrorSeverity::Critical);
//! assert!(!critical.is_recoverable());
//!
//! // Non-critical errors are recoverable
//! let recoverable = OpenRacingError::config("bad value");
//! assert_eq!(recoverable.severity(), ErrorSeverity::Error);
//! assert!(recoverable.is_recoverable());
//! ```
//!
//! # Error Context Macro
//!
//! ```
//! use openracing_errors::prelude::*;
//! use openracing_errors::error_context;
//!
//! let ctx = error_context!("apply_profile", "profile" => "gt3", "device" => "moza-r9");
//! assert!(ctx.to_string().contains("apply_profile"));
//! assert!(ctx.to_string().contains("gt3"));
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
///
/// # Examples
///
/// ```
/// use openracing_errors::{RTResult, RTError};
///
/// fn check_timing(jitter_us: u32) -> RTResult {
///     if jitter_us > 250 {
///         return Err(RTError::TimingViolation);
///     }
///     Ok(())
/// }
///
/// assert!(check_timing(100).is_ok());
/// assert!(check_timing(300).is_err());
/// ```
pub type RTResult<T = ()> = std::result::Result<T, RTError>;

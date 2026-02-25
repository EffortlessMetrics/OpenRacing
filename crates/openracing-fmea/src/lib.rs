//! FMEA (Failure Mode & Effects Analysis) fault management for OpenRacing.
//!
//! This crate provides a comprehensive fault detection, isolation, and recovery system
//! for safety-critical real-time force feedback systems.
//!
//! # Architecture
//!
//! The FMEA system is built around several key components:
//!
//! - **FaultTypes**: Enumeration of all detectable fault conditions
//! - **FaultThresholds**: Configurable detection thresholds
//! - **FmeaSystem**: Central coordinator for fault management
//! - **SoftStopController**: Graceful torque ramping for safe shutdown
//! - **RecoveryProcedures**: Fault recovery strategies
//!
//! # RT-Safety
//!
//! All fault detection methods in this crate are RT-safe:
//! - No heap allocations in hot paths
//! - No blocking operations
//! - Bounded execution time
//! - Deterministic behavior
//!
//! # State Machine
//!
//! ```text
//! ┌─────────────┐
//! │   Normal    │
//! └──────┬──────┘
//!        │ fault detected
//!        ▼
//! ┌─────────────┐     recovery successful
//! │   Faulted   │─────────────────────────┐
//! └──────┬──────┘                         │
//!        │                                │
//!        │ soft-stop active               │
//!        ▼                                │
//! ┌─────────────┐                         │
//! │  SoftStop   │─────────────────────────┘
//! └─────────────┘
//! ```
//!
//! # Example
//!
//! ```rust
//! use openracing_fmea::{FmeaSystem, FaultType, FaultThresholds};
//! use core::time::Duration;
//!
//! let mut fmea = FmeaSystem::new();
//!
//! // Detect USB faults
//! let fault = fmea.detect_usb_fault(3, Some(Duration::ZERO));
//! if let Some(fault_type) = fault {
//!     // Handle the detected fault
//!     println!("Detected fault: {:?}", fault_type);
//! }
//! ```

#![deny(unsafe_op_in_unsafe_fn, clippy::unwrap_used)]
#![warn(missing_docs, missing_debug_implementations, rust_2018_idioms)]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]

mod alerts;
mod error;
mod faults;
mod fmea;
mod recovery;
mod soft_stop;

pub mod prelude;

pub use alerts::{AudioAlert, AudioAlertSystem};
pub use error::{FmeaError, FmeaResult};
pub use faults::{
    FaultAction, FaultDetectionState, FaultMarker, FaultThresholds, FaultType, PostMortemConfig,
};
pub use fmea::{FmeaEntry, FmeaMatrix, FmeaSystem};
pub use recovery::{RecoveryContext, RecoveryProcedure, RecoveryResult, RecoveryStatus};
pub use soft_stop::SoftStopController;

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;

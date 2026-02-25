//! # openracing-hardware-watchdog
//!
//! Hardware watchdog management for safety-critical torque control.
//!
//! This crate provides a `#![no_std]`-compatible hardware watchdog system with:
//! - `HardwareWatchdog` trait for RT-safe implementations
//! - `SoftwareWatchdog` for testing and hardware-free environments
//! - State machine with deterministic transitions
//! - WCET-bounded operations for real-time safety
//!
//! ## Safety Guarantees
//!
//! - **No heap allocations** after initialization
//! - **No blocking operations** in RT methods
//! - **Deterministic execution** with bounded WCET
//! - **Atomic state transitions** for thread safety
//!
//! ## Real-Time Safety
//!
//! All RT-safe methods are documented with WCET bounds:
//! - `HardwareWatchdog::feed()` - WCET: < 100ns
//! - `HardwareWatchdog::is_armed()` - WCET: < 50ns
//! - `HardwareWatchdog::has_timed_out()` - WCET: < 100ns
//! - `WatchdogState::transition()` - WCET: < 50ns
//!
//! ## State Machine
//!
//! ```text
//! ┌─────────┐    arm()    ┌─────────┐
//! │ Disarmed│───────────►│  Armed  │
//! └─────────┘            └─────────┘
//!      ▲                      │
//!      │                      │
//!      │ reset()         feed()│ timeout()
//!      │                      ▼
//!      │                ┌─────────┐
//!      └────────────────│ TimedOut│
//!                       └─────────┘
//! ```
//!
//! ## Example
//!
//! ```rust
//! use openracing_hardware_watchdog::prelude::*;
//!
//! // Create a software watchdog with 100ms timeout
//! let config = WatchdogConfig::new(100).expect("Valid config");
//! let mut watchdog = SoftwareWatchdog::new(config);
//!
//! // Arm the watchdog
//! watchdog.arm().expect("Failed to arm");
//!
//! // Feed the watchdog (RT-safe)
//! watchdog.feed().expect("Failed to feed");
//!
//! // Check state
//! assert!(watchdog.is_armed());
//! assert!(!watchdog.has_timed_out());
//! ```

#![no_std]
#![deny(
    unsafe_op_in_unsafe_fn,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic_in_result_fn,
    clippy::panic,
    missing_docs,
    missing_debug_implementations
)]
#![warn(clippy::pedantic)]
#![cfg_attr(docsrs, feature(doc_cfg))]

#[cfg(feature = "std")]
extern crate std;

extern crate alloc;

pub mod config;
pub mod error;
pub mod prelude;
pub mod software_impl;
pub mod state;
pub mod watchdog;

pub use config::WatchdogConfig;
pub use error::{HardwareWatchdogError, HardwareWatchdogResult};
pub use software_impl::SoftwareWatchdog;
pub use state::{WatchdogMetrics, WatchdogState, WatchdogStatus};
pub use watchdog::HardwareWatchdog;

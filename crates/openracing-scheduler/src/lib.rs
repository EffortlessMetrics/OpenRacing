//! Absolute scheduling with PLL drift correction and jitter tracking for real-time systems.
//!
//! This crate provides precise timing control for real-time applications operating at
//! fixed frequencies (e.g., 1kHz force feedback loops). It includes:
//!
//! - **PLL (Phase-Locked Loop)**: Drift correction to maintain accurate timing over time
//! - **JitterMetrics**: Comprehensive jitter tracking with percentile calculations
//! - **AbsoluteScheduler**: Platform-specific high-precision sleep with busy-spin tail
//! - **AdaptiveScheduling**: Dynamic period adjustment based on system load
//! - **RTSetup**: Real-time thread configuration
//!
//! # RT-Safety Guarantees
//!
//! - **No heap allocations** in the hot path after initialization
//! - **No blocking operations** in timing-critical code
//! - **Bounded execution time** for all operations
//! - **Deterministic behavior** under all conditions
//!
//! # Example
//!
//! ```no_run
//! use openracing_scheduler::{AbsoluteScheduler, RTSetup};
//!
//! let mut scheduler = AbsoluteScheduler::new_1khz();
//! let setup = RTSetup::default();
//! scheduler.apply_rt_setup(&setup).expect("RT setup failed");
//!
//! loop {
//!     let tick = scheduler.wait_for_tick().expect("Timing violation");
//!     // Process real-time work here
//! }
//! ```

#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::unwrap_used)]
#![deny(static_mut_refs)]
#![deny(unused_must_use)]

pub mod adaptive;
pub mod error;
pub mod jitter;
pub mod pll;
pub mod rt_setup;
pub mod scheduler;

#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
mod linux;

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
mod fallback;

pub mod prelude;

pub use adaptive::{AdaptiveSchedulingConfig, AdaptiveSchedulingState};
pub use error::{RTError, RTResult};
pub use jitter::JitterMetrics;
pub use pll::PLL;
pub use rt_setup::RTSetup;
pub use scheduler::AbsoluteScheduler;

/// Target period for 1kHz operation in nanoseconds (1ms)
pub const PERIOD_1KHZ_NS: u64 = 1_000_000;

/// Maximum allowed jitter in nanoseconds for production (0.25ms)
pub const MAX_JITTER_NS: u64 = 250_000;

/// Maximum allowed jitter in nanoseconds for testing (5ms)
#[cfg(test)]
pub const MAX_JITTER_TEST_NS: u64 = 5_000_000;

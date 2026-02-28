//! # openracing-watchdog
//!
//! Watchdog systems for monitoring plugin execution and system health in `OpenRacing`.
//!
//! This crate provides comprehensive monitoring capabilities for the real-time force feedback
//! pipeline, ensuring plugins and system components meet their timing and health requirements.
//!
//! ## Safety Guarantees
//!
//! - **No heap allocations** after initialization in RT-safe methods
//! - **No blocking operations** in monitoring hot paths
//! - **Thread-safe** access to statistics and health status
//! - **Deterministic execution** for health check operations
//!
//! ## Architecture
//!
//! The crate is organized into several focused modules:
//!
//! - [`watchdog`] - Core watchdog system for monitoring plugins and components
//! - [`stats`] - Plugin execution statistics tracking
//! - [`health`] - Component health status and check management
//! - [`quarantine`] - Plugin quarantine management
//! - [`error`] - Watchdog-specific error types
//!
//! ## RT Safety Notes
//!
//! The following operations are RT-safe (no allocations, no blocking):
//! - `WatchdogSystem::record_plugin_execution()` - Records execution metrics
//! - `WatchdogSystem::heartbeat()` - Updates component heartbeats
//! - `WatchdogSystem::is_plugin_quarantined()` - Quarantine status check
//!
//! Operations that may allocate or block:
//! - `WatchdogSystem::perform_health_checks()` - Periodic cleanup
//! - `WatchdogSystem::get_quarantined_plugins()` - Returns `Vec`
//! - `WatchdogSystem::get_plugin_performance_metrics()` - Returns `HashMap`
//!
//! ## Example
//!
//! ```rust
//! use openracing_watchdog::prelude::*;
//! use std::time::Duration;
//!
//! // Create watchdog with configuration
//! let config = WatchdogConfig {
//!     plugin_timeout_us: 100,
//!     plugin_max_timeouts: 5,
//!     plugin_quarantine_duration: Duration::from_secs(300),
//!     ..Default::default()
//! };
//! let watchdog = WatchdogSystem::new(config);
//!
//! // Record plugin execution
//! let fault = watchdog.record_plugin_execution("my_plugin", 50);
//! assert!(fault.is_none());
//!
//! // Check plugin health
//! let stats = watchdog.get_plugin_stats("my_plugin");
//! assert!(stats.is_some());
//! ```

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

pub mod error;
pub mod health;
pub mod quarantine;
pub mod stats;
pub mod watchdog;

pub mod prelude;

pub use error::{WatchdogError, WatchdogResult};
pub use health::{HealthCheck, HealthStatus, SystemComponent};
pub use quarantine::{QuarantineEntry, QuarantineManager, QuarantineReason};
pub use stats::PluginStats;
pub use watchdog::{FaultCallback, WatchdogConfig, WatchdogConfigBuilder, WatchdogSystem};

// Re-export FaultType from openracing-fmea for convenience
pub use openracing_fmea::FaultType;

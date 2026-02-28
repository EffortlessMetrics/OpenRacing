//! Prelude for openracing-watchdog.
//!
//! This module re-exports the most commonly used types for convenient importing.
//!
//! # Example
//!
//! ```rust
//! use openracing_watchdog::prelude::*;
//!
//! let config = WatchdogConfig::default();
//! let watchdog = WatchdogSystem::new(config);
//!
//! watchdog.heartbeat(SystemComponent::RtThread);
//! watchdog.record_plugin_execution("my_plugin", 50);
//! ```

pub use crate::FaultType;
pub use crate::error::{WatchdogError, WatchdogResult};
pub use crate::health::{HealthCheck, HealthStatus, SystemComponent};
pub use crate::quarantine::{QuarantineEntry, QuarantineManager, QuarantineReason};
pub use crate::stats::PluginStats;
pub use crate::watchdog::{FaultCallback, WatchdogConfig, WatchdogConfigBuilder, WatchdogSystem};

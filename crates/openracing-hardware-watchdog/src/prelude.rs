//! Prelude for openracing-hardware-watchdog.
//!
//! This module re-exports the most commonly used types for convenient importing.
//!
//! # Example
//!
//! ```rust
//! use openracing_hardware_watchdog::prelude::*;
//!
//! let mut watchdog = SoftwareWatchdog::with_default_timeout();
//! watchdog.arm().expect("Failed to arm");
//! watchdog.feed().expect("Failed to feed");
//! ```

pub use crate::config::{WatchdogConfig, WatchdogConfigBuilder};
pub use crate::error::{HardwareWatchdogError, HardwareWatchdogResult};
pub use crate::software_impl::SoftwareWatchdog;
pub use crate::state::{WatchdogMetrics, WatchdogState, WatchdogStatus};
pub use crate::watchdog::HardwareWatchdog;

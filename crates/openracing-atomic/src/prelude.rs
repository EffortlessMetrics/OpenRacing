//! Prelude for openracing-atomic.
//!
//! This module re-exports the most commonly used types for convenient importing.
//!
//! # Example
//!
//! ```rust
//! use openracing_atomic::prelude::*;
//!
//! let counters = AtomicCounters::new();
//! counters.inc_tick();
//!
//! let jitter = JitterStats::from_values(100, 200, 500);
//! ```

pub use crate::counters::{AtomicCounters, CounterSnapshot};
pub use crate::stats::{
    AppMetricsSnapshot, AppThresholds, JitterStats, LatencyStats, RTMetricsSnapshot, RTThresholds,
    StreamingStats,
};

#[cfg(feature = "queues")]
#[cfg_attr(docsrs, doc(cfg(feature = "queues")))]
pub use crate::queues::{DEFAULT_QUEUE_CAPACITY, QueueStats, RTSampleQueues};

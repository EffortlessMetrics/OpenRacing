//! # openracing-atomic
//!
//! RT-safe atomic counters and metrics primitives for `OpenRacing`.
//!
//! This crate provides real-time safe primitives for metrics collection that can be
//! used in the hot path of the RT loop without allocations, blocking, or syscalls.
//!
//! ## Safety Guarantees
//!
//! - **No heap allocations** after initialization
//! - **No blocking operations** - all methods are lock-free
//! - **No syscalls** in RT hot paths
//! - **Deterministic execution time** for all operations
//!
//! ## Architecture
//!
//! The crate is organized into three modules:
//!
//! - [`counters`] - Atomic counter types for RT-safe metric accumulation
//! - [`stats`] - Statistics structures for latency, jitter, and metrics
//! - [`queues`] - Lock-free sample queues (optional, requires `queues` feature)
//!
//! ## Usage
//!
//! ```rust
//! use openracing_atomic::{AtomicCounters, JitterStats, LatencyStats};
//!
//! // Create counters (done once at initialization)
//! let counters = AtomicCounters::new();
//!
//! // RT-safe operations (no allocations, no blocking)
//! counters.inc_tick();
//! counters.inc_missed_tick();
//! counters.record_torque_saturation(true);
//!
//! // Read and reset (non-RT path, typically in metrics collector)
//! let values = counters.snapshot();
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

extern crate alloc;

pub mod counters;
pub mod stats;

#[cfg(feature = "queues")]
#[cfg_attr(docsrs, doc(cfg(feature = "queues")))]
pub mod queues;

pub mod prelude;

pub use counters::{AtomicCounters, CounterSnapshot};
pub use stats::{
    AppMetricsSnapshot, AppThresholds, JitterStats, LatencyStats, RTMetricsSnapshot, RTThresholds,
    StreamingStats,
};

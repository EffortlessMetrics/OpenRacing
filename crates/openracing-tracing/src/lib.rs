//! RT-safe tracing and observability for OpenRacing
//!
//! This crate provides platform-specific tracing capabilities:
//! - **Windows**: ETW (Event Tracing for Windows) provider
//! - **Linux**: Tracepoints via trace_marker
//! - **Other platforms**: Structured logging fallback
//!
//! # RT-Safety Guarantees
//!
//! All [`RTTraceEvent`] emissions are designed to be RT-safe:
//! - No heap allocations after initialization
//! - No blocking operations
//! - No syscalls that could block
//! - Bounded execution time
//!
//! # Example
//!
//! ```rust,ignore
//! use openracing_tracing::{TracingManager, RTTraceEvent, trace_rt_tick_start};
//!
//! let mut manager = TracingManager::new()?;
//! manager.initialize()?;
//!
//! // Using the macro
//! trace_rt_tick_start!(manager, 1, 1_000_000);
//!
//! // Or directly
//! manager.emit_rt_event(RTTraceEvent::TickStart {
//!     tick_count: 1,
//!     timestamp_ns: 1_000_000,
//! });
//! ```

#![deny(unsafe_op_in_unsafe_fn, clippy::unwrap_used)]
#![warn(missing_docs, missing_debug_implementations)]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod error;
pub mod events;
pub mod macros;
pub mod manager;
pub mod metrics;
pub mod platform;
pub mod prelude;
pub mod provider;

pub use error::TracingError;
pub use events::{AppEventCategory, AppTraceEvent, RTEventCategory, RTTraceEvent};
pub use manager::TracingManager;
pub use metrics::TracingMetrics;
pub use provider::TracingProvider;

//! Prelude for openracing-tracing
//!
//! This module re-exports the most commonly used types and macros.
//!
//! # Example
//!
//! ```rust,ignore
//! use openracing_tracing::prelude::*;
//!
//! let mut manager = TracingManager::new()?;
//! manager.initialize()?;
//!
//! trace_rt_tick_start!(manager, 1, 1_000_000);
//! ```

pub use crate::{
    AppTraceEvent, RTTraceEvent, TracingError, TracingManager, TracingMetrics, TracingProvider,
    events::{AppEventCategory, RTEventCategory},
    trace_rt_deadline_miss, trace_rt_hid_write, trace_rt_if_enabled, trace_rt_pipeline_fault,
    trace_rt_tick_end, trace_rt_tick_start,
};

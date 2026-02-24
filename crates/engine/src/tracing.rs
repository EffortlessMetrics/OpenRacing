//! Real-time observability and tracing for the FFB engine
//!
//! This module re-exports types from the `openracing-tracing` crate.
//! See the `openracing-tracing` crate documentation for details.

pub use openracing_tracing::{
    AppEventCategory, AppTraceEvent, RTEventCategory, RTTraceEvent, TracingError, TracingManager,
    TracingMetrics, TracingProvider, trace_rt_deadline_miss, trace_rt_hid_write,
    trace_rt_pipeline_fault, trace_rt_tick_end, trace_rt_tick_start,
};

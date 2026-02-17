//! Racing Wheel Engine - Real-time Force Feedback Core
//!
//! This crate contains the real-time force feedback engine that operates at 1kHz
//! with strict timing requirements and zero-allocation hot paths.

#![deny(static_mut_refs)]
#![deny(unused_must_use)]
#![deny(clippy::unwrap_used)]

#[cfg(all(not(test), feature = "rt-allocator"))]
use mimalloc::MiMalloc;

#[cfg(test)]
#[global_allocator]
static GLOBAL: crate::allocation_tracker::TrackingAllocator =
    crate::allocation_tracker::TrackingAllocator;

#[cfg(all(not(test), feature = "rt-hardening"))]
#[global_allocator]
static GLOBAL: crate::allocation_tracker::TrackingAllocator =
    crate::allocation_tracker::TrackingAllocator;

#[cfg(all(not(test), not(feature = "rt-hardening"), feature = "rt-allocator"))]
#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

pub mod allocation_tracker;
pub mod benchmark_types;
#[cfg(test)]
pub mod compat_impl;
pub mod curves;
#[cfg(test)]
pub mod curves_property_tests;
pub mod device;
pub mod diagnostic;
pub mod engine;
pub mod ffb;
pub mod filters;
pub mod hid;
pub mod hil_tests;
pub mod led_haptics;
pub mod metrics;
#[cfg(test)]
pub mod metrics_tests;
pub mod pipeline;
pub mod policies;
pub mod ports;
pub mod prelude;
pub mod profile_merge;
pub mod profile_service;
pub mod protocol;
pub mod rt;
pub mod safety;
pub mod scheduler;
#[cfg(any(test, feature = "harness"))]
pub mod test_harness;
pub mod tracing;
pub mod tracing_test;
pub mod two_phase_apply;

// Explicit exports from rt module (canonical FFBMode and Frame)
pub use rt::{FFBMode, Frame, PerformanceMetrics, RTError, RTResult};

// Pipeline for FFB processing
pub use pipeline::Pipeline;

// Explicit exports from ffb module (no FFBMode to avoid conflict)
pub use ffb::{CapabilityNegotiator, GameCompatibility, ModeSelectionPolicy, NegotiationResult};

// Explicit exports from other modules - only export what actually exists
pub use engine::{BlackboxFrame, Engine, EngineCommand, EngineConfig, EngineStats, GameInput};
pub use scheduler::{
    AbsoluteScheduler, AdaptiveSchedulingConfig, AdaptiveSchedulingState, JitterMetrics, PLL,
    RTSetup,
};
#[cfg(any(test, feature = "harness"))]
pub use test_harness::{
    ExpectedResponse, FaultInjection, RTLoopTestHarness, ResponseValidationResult,
    TestHarnessConfig, TestResult, TestScenario, TimingValidation, TorquePattern,
};
pub use two_phase_apply::{ApplyOperationStats, ApplyResult, ApplyStats, TwoPhaseApplyCoordinator};

// Re-export specific items to avoid conflicts
pub use device::{DeviceEvent, DeviceInfo, TelemetryData, VirtualDevice, VirtualHidPort};
pub use metrics::{
    AlertingThresholds, AppMetrics, AtomicCounters, HealthEvent, HealthEventStreamer,
    HealthEventType, HealthSeverity, MetricsCollector, MetricsValidator, PrometheusMetrics,
    RTMetrics,
};
pub use policies::{ProfileHierarchyError, ProfileHierarchyPolicy, SafetyPolicy, SafetyViolation};
pub use ports::{
    ConfigChange, ConfigurationStatus, DeviceHealthStatus, HidDevice, HidPort, NormalizedTelemetry,
    ProfileContext, ProfileRepo, ProfileRepoError, RepositoryStatus, TelemetryFlags, TelemetryPort,
    TelemetryStatistics,
};
pub use protocol::{DeviceCapabilitiesReport, DeviceTelemetryReport, TorqueCommand};
pub use tracing::{
    AppTraceEvent, RTTraceEvent, TracingError, TracingManager, TracingMetrics, TracingProvider,
};

// Curve-based FFB effects
pub use curves::{BezierCurve, CurveError, CurveLut, CurveType};

// Benchmark result types for JSON output
pub use benchmark_types::{
    BenchmarkEntry, BenchmarkResult, BenchmarkResults, CustomMetrics, Percentiles,
};

//! Racing Wheel Engine - Real-time Force Feedback Core
//!
//! This crate contains the real-time force feedback engine that operates at 1kHz
//! with strict timing requirements and zero-allocation hot paths.

#![deny(static_mut_refs)]
#![deny(unused_must_use)]
#![deny(clippy::unwrap_used)]

#[cfg(feature = "rt-allocator")]
use mimalloc::MiMalloc;

#[cfg(feature = "rt-allocator")]
#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

pub mod rt;
pub mod pipeline;
pub mod scheduler;
pub mod safety;
pub mod device;
pub mod ffb;
pub mod protocol;
#[cfg(any(test, feature = "harness"))]
pub mod test_harness;
pub mod ports;
pub mod policies;
pub mod profile_service;
pub mod profile_merge;
pub mod two_phase_apply;
pub mod allocation_tracker;
pub mod filters;
pub mod hid;
pub mod tracing;
pub mod tracing_test;
pub mod engine;
pub mod hil_tests;
pub mod led_haptics;
pub mod diagnostic;
pub mod metrics;
pub mod prelude;
#[cfg(test)]
pub mod metrics_tests;
#[cfg(test)]
pub mod compat_impl;

// Explicit exports from rt module (canonical FFBMode and Frame)
pub use rt::{Frame, FFBMode, RTError, RTResult, PerformanceMetrics};

// Explicit exports from ffb module (no FFBMode to avoid conflict)
pub use ffb::{
    GameCompatibility, ModeSelectionPolicy, CapabilityNegotiator, NegotiationResult
};

// Explicit exports from other modules - only export what actually exists
pub use scheduler::{PLL, RTSetup, JitterMetrics};
#[cfg(any(test, feature = "harness"))]
pub use test_harness::{
    TestHarnessConfig, TestScenario, TorquePattern, ExpectedResponse, 
    FaultInjection, TestResult, TimingValidation, ResponseValidationResult, RTLoopTestHarness
};
pub use two_phase_apply::{TwoPhaseApplyCoordinator, ApplyResult, ApplyOperationStats, ApplyStats};
pub use engine::{
    Engine, EngineConfig, EngineStats, EngineCommand, GameInput, BlackboxFrame
};

// Re-export specific items to avoid conflicts
pub use device::{VirtualDevice, VirtualHidPort, DeviceEvent, TelemetryData, DeviceInfo};
pub use ports::{
    HidDevice, HidPort, TelemetryPort, ProfileRepo, ProfileRepoError, 
    NormalizedTelemetry, TelemetryFlags, ProfileContext, DeviceHealthStatus,
    TelemetryStatistics, ConfigurationStatus, ConfigChange, RepositoryStatus
};
pub use policies::{SafetyPolicy, ProfileHierarchyPolicy, SafetyViolation, ProfileHierarchyError};
pub use protocol::{TorqueCommand, DeviceTelemetryReport, DeviceCapabilitiesReport};
pub use tracing::{
    TracingManager, TracingProvider, RTTraceEvent, AppTraceEvent, TracingMetrics, TracingError
};
pub use metrics::{
    MetricsCollector, PrometheusMetrics, AtomicCounters, HealthEventStreamer, HealthEvent,
    HealthEventType, HealthSeverity, RTMetrics, AppMetrics, AlertingThresholds, MetricsValidator
};
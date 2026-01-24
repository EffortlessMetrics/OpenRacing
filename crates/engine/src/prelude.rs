//! Prelude module for common engine types
//!
//! This module provides a convenient way to import the most commonly used
//! types from the racing wheel engine.

// Core RT types (canonical exports)
pub use crate::rt::{FFBMode, Frame, PerformanceMetrics, RTError, RTResult};

// Engine types
pub use crate::engine::{BlackboxFrame, Engine, EngineConfig, GameInput};

// Device and port types
pub use crate::device::{DeviceInfo, TelemetryData, VirtualDevice};
pub use crate::ports::{HidDevice, NormalizedTelemetry, TelemetryFlags};

// FFB capability negotiation
pub use crate::ffb::{
    CapabilityNegotiator, GameCompatibility, ModeSelectionPolicy, NegotiationResult,
};

// Test harness for development
#[cfg(test)]
pub use crate::test_harness::{RTLoopTestHarness, TestHarnessConfig, TestResult};

// Scheduler and RT setup
pub use crate::scheduler::{JitterMetrics, PLL, RTSetup};

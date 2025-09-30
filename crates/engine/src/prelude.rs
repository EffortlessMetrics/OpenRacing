//! Prelude module for common engine types
//!
//! This module provides a convenient way to import the most commonly used
//! types from the racing wheel engine.

// Core RT types (canonical exports)
pub use crate::rt::{Frame, FFBMode, RTError, RTResult, PerformanceMetrics};

// Engine types
pub use crate::engine::{Engine, EngineConfig, GameInput, BlackboxFrame};

// Device and port types
pub use crate::device::{VirtualDevice, TelemetryData, DeviceInfo};
pub use crate::ports::{HidDevice, NormalizedTelemetry, TelemetryFlags};

// FFB capability negotiation
pub use crate::ffb::{GameCompatibility, ModeSelectionPolicy, CapabilityNegotiator, NegotiationResult};

// Test harness for development
#[cfg(test)]
pub use crate::test_harness::{TestHarnessConfig, RTLoopTestHarness, TestResult};

// Scheduler and RT setup
pub use crate::scheduler::{PLL, RTSetup, JitterMetrics};
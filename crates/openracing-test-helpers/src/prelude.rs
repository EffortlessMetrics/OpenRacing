//! Convenience re-exports for common test utilities.
//!
//! Import this module to get access to the most commonly used test helpers:
//!
//! ```rust,ignore
//! use openracing_test_helpers::prelude::*;
//! ```

pub use crate::must::{must, must_or_else, must_parse, must_some, must_some_or, must_with};

#[cfg(feature = "mock")]
pub use crate::must::{must_async, must_some_async};

#[cfg(feature = "tracking")]
pub use crate::tracking::{AllocationGuard, AllocationReport, track};

#[cfg(feature = "fixtures")]
pub use crate::fixtures::{
    DeviceCapabilitiesFixture, LoadLevel, PerformanceFixture, ProfileFixture, TelemetryFixture,
    get_device_fixtures, get_performance_fixtures, get_profile_fixtures, get_telemetry_fixtures,
};

#[cfg(feature = "mock")]
pub use crate::mock::{
    MockDeviceWriter, MockHidDevice, MockProfile, MockProfileId, MockTelemetryData,
    MockTelemetryPort,
};

pub type TestResult = Result<(), Box<dyn std::error::Error>>;

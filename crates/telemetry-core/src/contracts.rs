//! Normalized telemetry domain contracts for OpenRacing.
//!
//! # Deprecation Notice
//!
//! The types in this module are re-exported from `racing_wheel_schemas::telemetry`.
//! New code should use that crate directly.

// Re-export from the canonical location
pub use racing_wheel_schemas::telemetry::{
    NormalizedTelemetry, NormalizedTelemetryBuilder, TelemetryFlags, TelemetryFrame,
    TelemetrySnapshot, TelemetryValue,
};

use serde::{Deserialize, Serialize};

/// Telemetry field coverage information for documentation and docs generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryFieldCoverage {
    pub game_id: String,
    pub game_version: String,
    pub ffb_scalar: bool,
    pub rpm: bool,
    pub speed: bool,
    pub slip_ratio: bool,
    pub gear: bool,
    pub flags: FlagCoverage,
    pub car_id: bool,
    pub track_id: bool,
    pub extended_fields: Vec<String>,
}

/// Flag coverage information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlagCoverage {
    pub yellow_flag: bool,
    pub red_flag: bool,
    pub blue_flag: bool,
    pub checkered_flag: bool,
    pub green_flag: bool,
    pub pit_limiter: bool,
    pub in_pits: bool,
    pub drs_available: bool,
    pub drs_active: bool,
    pub ers_available: bool,
    pub launch_control: bool,
    pub traction_control: bool,
    pub abs_active: bool,
}

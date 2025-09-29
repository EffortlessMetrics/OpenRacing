//! Plugin SDK for developing racing wheel plugins

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Plugin SDK version
pub const SDK_VERSION: &str = "1.0.0";

/// Telemetry data structure for plugins
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkTelemetry {
    /// Force feedback scalar (-1.0 to 1.0)
    pub ffb_scalar: f32,
    /// Engine RPM
    pub rpm: f32,
    /// Vehicle speed (m/s)
    pub speed_ms: f32,
    /// Slip ratio (0.0 to 1.0)
    pub slip_ratio: f32,
    /// Current gear (-1 = reverse, 0 = neutral, 1+ = forward gears)
    pub gear: i8,
    /// Race flags
    pub flags: TelemetryFlags,
    /// Car identifier
    pub car_id: Option<String>,
    /// Track identifier
    pub track_id: Option<String>,
    /// Custom data from other plugins
    pub custom_data: HashMap<String, serde_json::Value>,
}

/// Race flags
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryFlags {
    pub green_flag: bool,
    pub yellow_flag: bool,
    pub red_flag: bool,
    pub checkered_flag: bool,
    pub blue_flag: bool,
    pub white_flag: bool,
    pub pit_limiter: bool,
    pub drs_enabled: bool,
    pub ers_available: bool,
}

/// LED mapping input for plugins
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkLedInput {
    /// Current telemetry
    pub telemetry: SdkTelemetry,
    /// Number of available LEDs
    pub led_count: u32,
    /// Current LED state
    pub current_leds: Vec<SdkLedColor>,
}

/// LED color representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkLedColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// DSP filter input for plugins
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkDspInput {
    /// Input force feedback signal (-1.0 to 1.0)
    pub ffb_input: f32,
    /// Wheel angular velocity (rad/s)
    pub wheel_speed: f32,
    /// Wheel angle (radians)
    pub wheel_angle: f32,
    /// Sample rate (Hz)
    pub sample_rate: f32,
    /// Time delta since last sample (seconds)
    pub dt: f32,
}
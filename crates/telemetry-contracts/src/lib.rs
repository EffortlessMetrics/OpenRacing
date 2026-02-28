//! Normalized telemetry domain contracts for OpenRacing.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Normalized telemetry data structure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct NormalizedTelemetry {
    /// Force feedback scalar value (-1.0 to 1.0)
    /// Represents the force feedback strength requested by the game.
    pub ffb_scalar: Option<f32>,

    /// Engine RPM (revolutions per minute).
    pub rpm: Option<f32>,

    /// Vehicle speed in meters per second.
    pub speed_ms: Option<f32>,

    /// Tire slip ratio (0.0 = no slip, 1.0 = full slip).
    pub slip_ratio: Option<f32>,

    /// Current gear (-1 = reverse, 0 = neutral, 1+ = forward gears).
    pub gear: Option<i8>,

    /// Racing flags and status information.
    pub flags: TelemetryFlags,

    /// Car identifier (if available).
    pub car_id: Option<String>,

    /// Track identifier (if available).
    pub track_id: Option<String>,

    /// Additional game-specific data.
    pub extended: HashMap<String, TelemetryValue>,
}

/// Racing flags and status information.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TelemetryFlags {
    /// Yellow flag (caution).
    pub yellow_flag: bool,

    /// Red flag (session stopped).
    pub red_flag: bool,

    /// Blue flag (being lapped).
    pub blue_flag: bool,

    /// Checkered flag (race finished).
    pub checkered_flag: bool,

    /// Green flag (racing).
    pub green_flag: bool,

    /// Pit limiter active.
    pub pit_limiter: bool,

    /// In pit lane.
    pub in_pits: bool,

    /// DRS (Drag Reduction System) available.
    pub drs_available: bool,

    /// DRS currently active.
    pub drs_active: bool,

    /// ERS (Energy Recovery System) available.
    pub ers_available: bool,

    /// Launch control active.
    pub launch_control: bool,

    /// Traction control active.
    pub traction_control: bool,

    /// ABS active.
    pub abs_active: bool,
}

impl Default for TelemetryFlags {
    fn default() -> Self {
        Self {
            yellow_flag: false,
            red_flag: false,
            blue_flag: false,
            checkered_flag: false,
            green_flag: true,
            pit_limiter: false,
            in_pits: false,
            drs_available: false,
            drs_active: false,
            ers_available: false,
            launch_control: false,
            traction_control: false,
            abs_active: false,
        }
    }
}

/// Extended telemetry value for game-specific data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TelemetryValue {
    Float(f32),
    Integer(i32),
    Boolean(bool),
    String(String),
}

impl NormalizedTelemetry {
    /// Create a default telemetry instance.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set FFB scalar value with clamping.
    pub fn with_ffb_scalar(mut self, value: f32) -> Self {
        self.ffb_scalar = Some(value.clamp(-1.0, 1.0));
        self
    }

    /// Set RPM value with validation.
    pub fn with_rpm(mut self, value: f32) -> Self {
        if value >= 0.0 && value.is_finite() {
            self.rpm = Some(value);
        }
        self
    }

    /// Set speed value with validation.
    pub fn with_speed_ms(mut self, value: f32) -> Self {
        if value >= 0.0 && value.is_finite() {
            self.speed_ms = Some(value);
        }
        self
    }

    /// Set slip ratio with validation.
    pub fn with_slip_ratio(mut self, value: f32) -> Self {
        if value.is_finite() {
            self.slip_ratio = Some(value.clamp(0.0, 1.0));
        }
        self
    }

    /// Set gear value.
    pub fn with_gear(mut self, value: i8) -> Self {
        self.gear = Some(value);
        self
    }

    /// Set car ID.
    pub fn with_car_id(mut self, id: String) -> Self {
        if !id.is_empty() {
            self.car_id = Some(id);
        }
        self
    }

    /// Set track ID.
    pub fn with_track_id(mut self, id: String) -> Self {
        if !id.is_empty() {
            self.track_id = Some(id);
        }
        self
    }

    /// Set flags.
    pub fn with_flags(mut self, flags: TelemetryFlags) -> Self {
        self.flags = flags;
        self
    }

    /// Add extended telemetry value.
    pub fn with_extended(mut self, key: String, value: TelemetryValue) -> Self {
        self.extended.insert(key, value);
        self
    }

    /// Check if telemetry has valid FFB data.
    pub fn has_ffb_data(&self) -> bool {
        self.ffb_scalar.is_some()
    }

    /// Check if telemetry has valid RPM data for LED display.
    pub fn has_rpm_data(&self) -> bool {
        self.rpm.is_some()
    }

    /// Get RPM as fraction of redline (0.0-1.0).
    pub fn rpm_fraction(&self, redline_rpm: f32) -> Option<f32> {
        self.rpm.map(|rpm| (rpm / redline_rpm).clamp(0.0, 1.0))
    }

    /// Check if any racing flags are active.
    pub fn has_active_flags(&self) -> bool {
        self.flags.yellow_flag
            || self.flags.red_flag
            || self.flags.blue_flag
            || self.flags.checkered_flag
    }

    /// Get speed in km/h.
    pub fn speed_kmh(&self) -> Option<f32> {
        self.speed_ms.map(|speed| speed * 3.6)
    }

    /// Get speed in mph.
    pub fn speed_mph(&self) -> Option<f32> {
        self.speed_ms.map(|speed| speed * 2.237)
    }
}

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

/// Telemetry frame with timing information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryFrame {
    /// Normalized telemetry data.
    pub data: NormalizedTelemetry,

    /// Timestamp when frame was received (monotonic).
    pub timestamp_ns: u64,

    /// Sequence number for ordering.
    pub sequence: u64,

    /// Raw data size for diagnostics.
    pub raw_size: usize,
}

impl TelemetryFrame {
    /// Create a new telemetry frame.
    pub fn new(
        data: NormalizedTelemetry,
        timestamp_ns: u64,
        sequence: u64,
        raw_size: usize,
    ) -> Self {
        Self {
            data,
            timestamp_ns,
            sequence,
            raw_size,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{NormalizedTelemetry, TelemetryFlags, TelemetryFrame, TelemetryValue};

    #[test]
    fn normalized_telemetry_new_returns_default() {
        let t = NormalizedTelemetry::new();
        assert!(t.ffb_scalar.is_none());
        assert!(t.rpm.is_none());
        assert!(t.speed_ms.is_none());
        assert!(t.extended.is_empty());
    }
    #[test]
    fn with_ffb_scalar_clamps_above_one() {
        let t = NormalizedTelemetry::new().with_ffb_scalar(2.5);
        assert_eq!(t.ffb_scalar, Some(1.0));
    }
    #[test]
    fn with_ffb_scalar_clamps_below_negative_one() {
        let t = NormalizedTelemetry::new().with_ffb_scalar(-3.0);
        assert_eq!(t.ffb_scalar, Some(-1.0));
    }
    #[test]
    fn with_ffb_scalar_preserves_valid_value() {
        let t = NormalizedTelemetry::new().with_ffb_scalar(0.5);
        assert_eq!(t.ffb_scalar, Some(0.5));
    }
    #[test]
    fn with_rpm_rejects_negative_value() {
        let t = NormalizedTelemetry::new().with_rpm(-100.0);
        assert!(t.rpm.is_none());
    }
    #[test]
    fn with_rpm_accepts_valid_value() {
        let t = NormalizedTelemetry::new().with_rpm(7000.0);
        assert_eq!(t.rpm, Some(7000.0));
    }
    #[test]
    fn with_speed_ms_rejects_negative() {
        let t = NormalizedTelemetry::new().with_speed_ms(-10.0);
        assert!(t.speed_ms.is_none());
    }
    #[test]
    fn speed_kmh_and_mph_conversions() {
        let t = NormalizedTelemetry::new().with_speed_ms(10.0);
        let kmh = t.speed_kmh().expect("kmh should be Some");
        let mph = t.speed_mph().expect("mph should be Some");
        assert!((kmh - 36.0).abs() < 0.01, "expected 36 km/h, got {}", kmh);
        assert!(
            (mph - 22.37).abs() < 0.01,
            "expected ~22.37 mph, got {}",
            mph
        );
    }
    #[test]
    fn telemetry_flags_default_has_green_flag_true() {
        let flags = TelemetryFlags::default();
        assert!(flags.green_flag);
        assert!(!flags.yellow_flag);
        assert!(!flags.checkered_flag);
    }
    #[test]
    fn has_active_flags_returns_false_for_default() {
        let t = NormalizedTelemetry::new();
        assert!(!t.has_active_flags());
    }
    #[test]
    fn has_active_flags_returns_true_for_yellow() {
        let flags = TelemetryFlags { yellow_flag: true, ..TelemetryFlags::default() };
        let t = NormalizedTelemetry::new().with_flags(flags);
        assert!(t.has_active_flags());
    }
    #[test]
    fn telemetry_frame_new_stores_fields() {
        let data = NormalizedTelemetry::new().with_rpm(5000.0);
        let frame = TelemetryFrame::new(data.clone(), 123_456_789, 42, 64);
        assert_eq!(frame.timestamp_ns, 123_456_789);
        assert_eq!(frame.sequence, 42);
        assert_eq!(frame.raw_size, 64);
        assert_eq!(frame.data.rpm, Some(5000.0));
    }
    #[test]
    fn telemetry_value_variants_are_distinct() {
        let f = TelemetryValue::Float(1.0);
        let i = TelemetryValue::Integer(1);
        let b = TelemetryValue::Boolean(true);
        let s = TelemetryValue::String("x".to_string());
        assert_ne!(f, i);
        assert_ne!(b, s);
    }
    #[test]
    fn normalized_telemetry_serde_round_trip() -> Result<(), Box<dyn std::error::Error>> {
        let t = NormalizedTelemetry::new()
            .with_ffb_scalar(0.75)
            .with_rpm(6500.0)
            .with_speed_ms(50.0)
            .with_gear(3);
        let json = serde_json::to_string(&t)?;
        let decoded: NormalizedTelemetry = serde_json::from_str(&json)?;
        assert_eq!(decoded.ffb_scalar, t.ffb_scalar);
        assert_eq!(decoded.rpm, t.rpm);
        Ok(())
    }
}

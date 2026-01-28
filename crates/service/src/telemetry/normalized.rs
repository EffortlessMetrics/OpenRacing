//! Normalized telemetry data structures
//!
//! Defines the common telemetry format that all adapters normalize to.
//! Requirements: GI-03

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Normalized telemetry data structure (GI-03)
///
/// This represents the common format that all game-specific adapters
/// normalize their telemetry data to. Fields are optional to handle
/// games that don't provide all data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct NormalizedTelemetry {
    /// Force feedback scalar value (-1.0 to 1.0)
    /// Represents the force feedback strength requested by the game
    pub ffb_scalar: Option<f32>,

    /// Engine RPM (revolutions per minute)
    pub rpm: Option<f32>,

    /// Vehicle speed in meters per second
    pub speed_ms: Option<f32>,

    /// Tire slip ratio (0.0 = no slip, 1.0 = full slip)
    /// Average of all tires or most relevant tire
    pub slip_ratio: Option<f32>,

    /// Current gear (-1 = reverse, 0 = neutral, 1+ = forward gears)
    pub gear: Option<i8>,

    /// Racing flags and status information
    pub flags: TelemetryFlags,

    /// Car identifier (if available)
    pub car_id: Option<String>,

    /// Track identifier (if available)
    pub track_id: Option<String>,

    /// Additional game-specific data
    pub extended: HashMap<String, TelemetryValue>,
}

/// Racing flags and status information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TelemetryFlags {
    /// Yellow flag (caution)
    pub yellow_flag: bool,

    /// Red flag (session stopped)
    pub red_flag: bool,

    /// Blue flag (being lapped)
    pub blue_flag: bool,

    /// Checkered flag (race finished)
    pub checkered_flag: bool,

    /// Green flag (racing)
    pub green_flag: bool,

    /// Pit limiter active
    pub pit_limiter: bool,

    /// In pit lane
    pub in_pits: bool,

    /// DRS (Drag Reduction System) available
    pub drs_available: bool,

    /// DRS currently active
    pub drs_active: bool,

    /// ERS (Energy Recovery System) available
    pub ers_available: bool,

    /// Launch control active
    pub launch_control: bool,

    /// Traction control active
    pub traction_control: bool,

    /// ABS active
    pub abs_active: bool,
}

/// Extended telemetry value for game-specific data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TelemetryValue {
    Float(f32),
    Integer(i32),
    Boolean(bool),
    String(String),
}

impl Default for TelemetryFlags {
    fn default() -> Self {
        Self {
            yellow_flag: false,
            red_flag: false,
            blue_flag: false,
            checkered_flag: false,
            green_flag: true, // Default to green (racing)
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

impl NormalizedTelemetry {
    /// Set FFB scalar value with validation
    pub fn with_ffb_scalar(mut self, value: f32) -> Self {
        // Clamp to valid range
        self.ffb_scalar = Some(value.clamp(-1.0, 1.0));
        self
    }

    /// Set RPM value with validation
    pub fn with_rpm(mut self, value: f32) -> Self {
        // Ensure non-negative
        if value >= 0.0 && value.is_finite() {
            self.rpm = Some(value);
        }
        self
    }

    /// Set speed value with validation
    pub fn with_speed_ms(mut self, value: f32) -> Self {
        // Ensure non-negative and finite
        if value >= 0.0 && value.is_finite() {
            self.speed_ms = Some(value);
        }
        self
    }

    /// Set slip ratio with validation
    pub fn with_slip_ratio(mut self, value: f32) -> Self {
        // Clamp to valid range
        if value.is_finite() {
            self.slip_ratio = Some(value.clamp(0.0, 1.0));
        }
        self
    }

    /// Set gear value
    pub fn with_gear(mut self, value: i8) -> Self {
        self.gear = Some(value);
        self
    }

    /// Set car ID
    pub fn with_car_id(mut self, id: String) -> Self {
        if !id.is_empty() {
            self.car_id = Some(id);
        }
        self
    }

    /// Set track ID
    pub fn with_track_id(mut self, id: String) -> Self {
        if !id.is_empty() {
            self.track_id = Some(id);
        }
        self
    }

    /// Set flags
    pub fn with_flags(mut self, flags: TelemetryFlags) -> Self {
        self.flags = flags;
        self
    }

    /// Add extended telemetry value
    pub fn with_extended(mut self, key: String, value: TelemetryValue) -> Self {
        self.extended.insert(key, value);
        self
    }

    /// Check if telemetry has valid FFB data
    pub fn has_ffb_data(&self) -> bool {
        self.ffb_scalar.is_some()
    }

    /// Check if telemetry has valid RPM data for LED display
    pub fn has_rpm_data(&self) -> bool {
        self.rpm.is_some()
    }

    /// Get RPM as fraction of redline (0.0-1.0)
    /// Requires redline_rpm to be provided
    pub fn rpm_fraction(&self, redline_rpm: f32) -> Option<f32> {
        self.rpm.map(|rpm| (rpm / redline_rpm).clamp(0.0, 1.0))
    }

    /// Check if any racing flags are active
    pub fn has_active_flags(&self) -> bool {
        self.flags.yellow_flag
            || self.flags.red_flag
            || self.flags.blue_flag
            || self.flags.checkered_flag
    }

    /// Get speed in km/h
    pub fn speed_kmh(&self) -> Option<f32> {
        self.speed_ms.map(|speed| speed * 3.6)
    }

    /// Get speed in mph
    pub fn speed_mph(&self) -> Option<f32> {
        self.speed_ms.map(|speed| speed * 2.237)
    }
}

/// Telemetry field coverage information for documentation
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

/// Flag coverage information
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

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn test_normalized_telemetry_creation() -> TestResult {
        let telemetry = NormalizedTelemetry::default()
            .with_ffb_scalar(0.75)
            .with_rpm(6500.0)
            .with_speed_ms(45.0)
            .with_slip_ratio(0.15)
            .with_gear(4)
            .with_car_id("gt3_bmw".to_string())
            .with_track_id("spa".to_string());

        assert_eq!(telemetry.ffb_scalar, Some(0.75));
        assert_eq!(telemetry.rpm, Some(6500.0));
        assert_eq!(telemetry.speed_ms, Some(45.0));
        assert_eq!(telemetry.slip_ratio, Some(0.15));
        assert_eq!(telemetry.gear, Some(4));
        assert_eq!(telemetry.car_id, Some("gt3_bmw".to_string()));
        assert_eq!(telemetry.track_id, Some("spa".to_string()));
        Ok(())
    }

    #[test]
    fn test_ffb_scalar_clamping() -> TestResult {
        let telemetry1 = NormalizedTelemetry::default().with_ffb_scalar(1.5);
        assert_eq!(telemetry1.ffb_scalar, Some(1.0));

        let telemetry2 = NormalizedTelemetry::default().with_ffb_scalar(-1.5);
        assert_eq!(telemetry2.ffb_scalar, Some(-1.0));
        Ok(())
    }

    #[test]
    fn test_slip_ratio_clamping() -> TestResult {
        let telemetry1 = NormalizedTelemetry::default().with_slip_ratio(1.5);
        assert_eq!(telemetry1.slip_ratio, Some(1.0));

        let telemetry2 = NormalizedTelemetry::default().with_slip_ratio(-0.5);
        assert_eq!(telemetry2.slip_ratio, Some(0.0));
        Ok(())
    }

    #[test]
    fn test_invalid_values_rejected() -> TestResult {
        let telemetry = NormalizedTelemetry::default()
            .with_rpm(-100.0) // Negative RPM should be rejected
            .with_speed_ms(f32::NAN); // NaN should be rejected

        assert_eq!(telemetry.rpm, None);
        assert_eq!(telemetry.speed_ms, None);
        Ok(())
    }

    #[test]
    fn test_speed_conversions() -> TestResult {
        let telemetry = NormalizedTelemetry::default().with_speed_ms(27.78); // 100 km/h

        let speed_kmh = telemetry
            .speed_kmh()
            .ok_or("expected speed_kmh to be Some")?;
        let speed_mph = telemetry
            .speed_mph()
            .ok_or("expected speed_mph to be Some")?;
        assert!((speed_kmh - 100.0).abs() < 0.1);
        assert!((speed_mph - 62.14).abs() < 0.1);
        Ok(())
    }

    #[test]
    fn test_rpm_fraction() -> TestResult {
        let telemetry = NormalizedTelemetry::default().with_rpm(6000.0);

        let fraction = telemetry
            .rpm_fraction(8000.0)
            .ok_or("expected rpm_fraction to be Some")?;
        assert!((fraction - 0.75).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn test_flags() -> TestResult {
        let flags = TelemetryFlags {
            yellow_flag: true,
            pit_limiter: true,
            ..Default::default()
        };

        let telemetry = NormalizedTelemetry::default().with_flags(flags);

        assert!(telemetry.has_active_flags());
        assert!(telemetry.flags.yellow_flag);
        assert!(telemetry.flags.pit_limiter);
        Ok(())
    }

    #[test]
    fn test_extended_data() -> TestResult {
        let telemetry = NormalizedTelemetry::default()
            .with_extended("fuel_level".to_string(), TelemetryValue::Float(45.5))
            .with_extended("lap_count".to_string(), TelemetryValue::Integer(12))
            .with_extended(
                "session_type".to_string(),
                TelemetryValue::String("Race".to_string()),
            );

        assert_eq!(telemetry.extended.len(), 3);

        let fuel_value = telemetry
            .extended
            .get("fuel_level")
            .ok_or("expected fuel_level key")?;
        match fuel_value {
            TelemetryValue::Float(fuel) => assert_eq!(*fuel, 45.5),
            _ => return Err("Expected fuel_level to be a float".into()),
        }
        Ok(())
    }
}

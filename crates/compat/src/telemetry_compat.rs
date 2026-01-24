//! Telemetry compatibility trait for schema migration
//!
//! This module provides the TelemetryCompat trait that maps old field names
//! to new field names in TelemetryData structs. This allows test code to
//! gradually migrate from old field names to new ones.
//!
//! Field mappings:
//! - wheel_angle_mdeg → wheel_angle_deg (with unit conversion)
//! - wheel_speed_mrad_s → wheel_speed_rad_s (with unit conversion)  
//! - temp_c → temperature_c (direct mapping)
//! - faults → fault_flags (direct mapping)
//! - sequence → (removed field, returns 0)

/// Compatibility trait for TelemetryData field access
///
/// This trait provides methods with old field names that forward to the new
/// field names, enabling gradual migration of test code.
#[cfg(test)]
pub trait TelemetryCompat {
    /// Get temperature in Celsius (old field name: temp_c)
    fn temp_c(&self) -> u8;

    /// Get fault flags (old field name: faults)
    fn faults(&self) -> u8;

    /// Get wheel angle in millidegrees (old field name: wheel_angle_mdeg)
    /// Converts from degrees to millidegrees
    fn wheel_angle_mdeg(&self) -> i32;

    /// Get wheel speed in milliradians per second (old field name: wheel_speed_mrad_s)
    /// Converts from radians/s to milliradians/s
    fn wheel_speed_mrad_s(&self) -> i32;

    /// Get sequence number (removed field, always returns 0)
    /// This field was removed from the schema
    fn sequence(&self) -> u32;
}

// Implementation for engine's TelemetryData (uses new field names)
#[cfg(test)]
impl TelemetryCompat for racing_wheel_engine::TelemetryData {
    #[inline]
    fn temp_c(&self) -> u8 {
        self.temperature_c
    }

    #[inline]
    fn faults(&self) -> u8 {
        self.fault_flags
    }

    #[inline]
    fn wheel_angle_mdeg(&self) -> i32 {
        (self.wheel_angle_deg * 1000.0) as i32
    }

    #[inline]
    fn wheel_speed_mrad_s(&self) -> i32 {
        (self.wheel_speed_rad_s * 1000.0) as i32
    }

    #[inline]
    fn sequence(&self) -> u32 {
        0 // Field was removed
    }
}

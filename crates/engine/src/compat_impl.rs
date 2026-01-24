//! Compatibility trait implementation for engine TelemetryData
//!
//! This module implements the TelemetryCompat trait for the engine crate's
//! TelemetryData struct, providing backward compatibility for old field names.

use crate::device::TelemetryData;
use compat::TelemetryCompat;

impl TelemetryCompat for TelemetryData {
    /// Get temperature in Celsius (old field name: temp_c)
    #[inline]
    fn temp_c(&self) -> u8 {
        self.temperature_c
    }

    /// Get fault flags (old field name: faults)
    #[inline]
    fn faults(&self) -> u8 {
        self.fault_flags
    }

    /// Get wheel angle in millidegrees (old field name: wheel_angle_mdeg)
    /// Converts from degrees to millidegrees
    #[inline]
    fn wheel_angle_mdeg(&self) -> i32 {
        (self.wheel_angle_deg * 1000.0) as i32
    }

    /// Get wheel speed in milliradians per second (old field name: wheel_speed_mrad_s)
    /// Converts from radians/s to milliradians/s
    #[inline]
    fn wheel_speed_mrad_s(&self) -> i32 {
        (self.wheel_speed_rad_s * 1000.0) as i32
    }

    /// Get sequence number (removed field, always returns 0)
    /// This field was removed from the schema
    #[inline]
    fn sequence(&self) -> u32 {
        0 // Field was removed, return default value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn test_telemetry_compat_trait() {
        let telemetry = TelemetryData {
            wheel_angle_deg: 45.5,
            wheel_speed_rad_s: 2.5,
            temperature_c: 65,
            fault_flags: 0x04,
            hands_on: true,
            timestamp: Instant::now(),
        };

        // Test old field name access through compat trait
        assert_eq!(telemetry.temp_c(), 65);
        assert_eq!(telemetry.faults(), 0x04);
        assert_eq!(telemetry.wheel_angle_mdeg(), 45500); // 45.5 * 1000
        assert_eq!(telemetry.wheel_speed_mrad_s(), 2500); // 2.5 * 1000
        assert_eq!(telemetry.sequence(), 0); // Removed field
    }

    #[test]
    fn test_unit_conversions() {
        let telemetry = TelemetryData {
            wheel_angle_deg: -180.0,
            wheel_speed_rad_s: -10.5,
            temperature_c: 25,
            fault_flags: 0,
            hands_on: false,
            timestamp: Instant::now(),
        };

        // Test negative values and precision
        assert_eq!(telemetry.wheel_angle_mdeg(), -180000);
        assert_eq!(telemetry.wheel_speed_mrad_s(), -10500);
    }
}

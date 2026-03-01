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

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal mock that implements TelemetryCompat for isolated trait testing.
    #[derive(Debug, Clone)]
    struct MockTelemetry {
        temperature_c: u8,
        fault_flags: u8,
        wheel_angle_deg: f32,
        wheel_speed_rad_s: f32,
    }

    impl TelemetryCompat for MockTelemetry {
        fn temp_c(&self) -> u8 {
            self.temperature_c
        }
        fn faults(&self) -> u8 {
            self.fault_flags
        }
        fn wheel_angle_mdeg(&self) -> i32 {
            (self.wheel_angle_deg * 1000.0) as i32
        }
        fn wheel_speed_mrad_s(&self) -> i32 {
            (self.wheel_speed_rad_s * 1000.0) as i32
        }
        fn sequence(&self) -> u32 {
            0
        }
    }

    fn mock(angle: f32, speed: f32, temp: u8, faults: u8) -> MockTelemetry {
        MockTelemetry {
            temperature_c: temp,
            fault_flags: faults,
            wheel_angle_deg: angle,
            wheel_speed_rad_s: speed,
        }
    }

    // -- Direct field mapping tests -----------------------------------------

    #[test]
    fn temp_c_maps_to_temperature_c() {
        let t = mock(0.0, 0.0, 42, 0);
        assert_eq!(t.temp_c(), 42);
        assert_eq!(t.temp_c(), t.temperature_c);
    }

    #[test]
    fn faults_maps_to_fault_flags() {
        let t = mock(0.0, 0.0, 0, 0xAB);
        assert_eq!(t.faults(), 0xAB);
        assert_eq!(t.faults(), t.fault_flags);
    }

    // -- Unit-conversion tests: wheel_angle_mdeg ----------------------------

    #[test]
    fn wheel_angle_mdeg_positive() {
        let t = mock(45.5, 0.0, 0, 0);
        assert_eq!(t.wheel_angle_mdeg(), 45500);
    }

    #[test]
    fn wheel_angle_mdeg_negative() {
        let t = mock(-180.0, 0.0, 0, 0);
        assert_eq!(t.wheel_angle_mdeg(), -180000);
    }

    #[test]
    fn wheel_angle_mdeg_zero() {
        let t = mock(0.0, 0.0, 0, 0);
        assert_eq!(t.wheel_angle_mdeg(), 0);
    }

    #[test]
    fn wheel_angle_mdeg_fractional() {
        let t = mock(0.001, 0.0, 0, 0);
        assert_eq!(t.wheel_angle_mdeg(), 1);
    }

    #[test]
    fn wheel_angle_mdeg_large_positive() {
        let t = mock(900.0, 0.0, 0, 0);
        assert_eq!(t.wheel_angle_mdeg(), 900_000);
    }

    #[test]
    fn wheel_angle_mdeg_large_negative() {
        let t = mock(-900.0, 0.0, 0, 0);
        assert_eq!(t.wheel_angle_mdeg(), -900_000);
    }

    // -- Unit-conversion tests: wheel_speed_mrad_s --------------------------

    #[test]
    fn wheel_speed_mrad_s_positive() {
        let t = mock(0.0, 2.5, 0, 0);
        assert_eq!(t.wheel_speed_mrad_s(), 2500);
    }

    #[test]
    fn wheel_speed_mrad_s_negative() {
        let t = mock(0.0, -10.5, 0, 0);
        assert_eq!(t.wheel_speed_mrad_s(), -10500);
    }

    #[test]
    fn wheel_speed_mrad_s_zero() {
        let t = mock(0.0, 0.0, 0, 0);
        assert_eq!(t.wheel_speed_mrad_s(), 0);
    }

    #[test]
    fn wheel_speed_mrad_s_fractional() {
        let t = mock(0.0, 0.001, 0, 0);
        assert_eq!(t.wheel_speed_mrad_s(), 1);
    }

    #[test]
    fn wheel_speed_mrad_s_large() {
        let t = mock(0.0, 1000.0, 0, 0);
        assert_eq!(t.wheel_speed_mrad_s(), 1_000_000);
    }

    // -- Removed field: sequence --------------------------------------------

    #[test]
    fn sequence_always_returns_zero() {
        let t1 = mock(0.0, 0.0, 0, 0);
        assert_eq!(t1.sequence(), 0);

        let t2 = mock(90.0, 5.0, 100, 0xFF);
        assert_eq!(t2.sequence(), 0);
    }

    // -- Boundary values for u8 fields --------------------------------------

    #[test]
    fn temperature_boundary_values() {
        assert_eq!(mock(0.0, 0.0, u8::MIN, 0).temp_c(), 0);
        assert_eq!(mock(0.0, 0.0, u8::MAX, 0).temp_c(), 255);
    }

    #[test]
    fn fault_flags_boundary_values() {
        assert_eq!(mock(0.0, 0.0, 0, u8::MIN).faults(), 0);
        assert_eq!(mock(0.0, 0.0, 0, u8::MAX).faults(), 255);
    }

    // -- Special float values -----------------------------------------------

    #[test]
    fn negative_zero_angle_yields_zero() {
        let t = mock(-0.0, 0.0, 0, 0);
        assert_eq!(t.wheel_angle_mdeg(), 0);
    }

    #[test]
    fn negative_zero_speed_yields_zero() {
        let t = mock(0.0, -0.0, 0, 0);
        assert_eq!(t.wheel_speed_mrad_s(), 0);
    }

    #[test]
    fn very_small_angle_truncates_to_zero() {
        let t = mock(0.0001, 0.0, 0, 0);
        // 0.0001 * 1000 = 0.1 → truncates to 0 as i32
        assert_eq!(t.wheel_angle_mdeg(), 0);
    }

    #[test]
    fn very_small_speed_truncates_to_zero() {
        let t = mock(0.0, 0.0001, 0, 0);
        assert_eq!(t.wheel_speed_mrad_s(), 0);
    }

    // -- Consistency: compat methods match manual conversion -----------------

    #[test]
    fn compat_matches_manual_conversion_angle() {
        let angles = [0.0_f32, 1.0, -1.0, 45.5, -180.0, 360.0, -720.0];
        for &deg in &angles {
            let t = mock(deg, 0.0, 0, 0);
            let expected = (deg * 1000.0) as i32;
            assert_eq!(
                t.wheel_angle_mdeg(),
                expected,
                "mismatch for wheel_angle_deg={deg}"
            );
        }
    }

    #[test]
    fn compat_matches_manual_conversion_speed() {
        let speeds = [0.0_f32, 1.0, -1.0, 2.5, -10.5, 100.0, -500.0];
        for &rad_s in &speeds {
            let t = mock(0.0, rad_s, 0, 0);
            let expected = (rad_s * 1000.0) as i32;
            assert_eq!(
                t.wheel_speed_mrad_s(),
                expected,
                "mismatch for wheel_speed_rad_s={rad_s}"
            );
        }
    }

    // -- Full snapshot: all compat methods on a realistic sample -------------

    #[test]
    fn full_compat_snapshot() {
        let t = mock(90.0, 5.0, 45, 0x02);
        assert_eq!(t.temp_c(), 45);
        assert_eq!(t.faults(), 0x02);
        assert_eq!(t.wheel_angle_mdeg(), 90_000);
        assert_eq!(t.wheel_speed_mrad_s(), 5_000);
        assert_eq!(t.sequence(), 0);

        assert_eq!(t.temp_c(), t.temperature_c);
        assert_eq!(t.faults(), t.fault_flags);
    }

    // -- Trait object safety -------------------------------------------------

    #[test]
    fn trait_object_usage() {
        let t = mock(30.0, 3.0, 50, 0x10);
        let dyn_ref: &dyn TelemetryCompat = &t;
        assert_eq!(dyn_ref.temp_c(), 50);
        assert_eq!(dyn_ref.faults(), 0x10);
        assert_eq!(dyn_ref.wheel_angle_mdeg(), 30_000);
        assert_eq!(dyn_ref.wheel_speed_mrad_s(), 3_000);
        assert_eq!(dyn_ref.sequence(), 0);
    }

    // -- Independent instances do not interfere -----------------------------

    #[test]
    fn independent_instances() {
        let t1 = mock(10.0, 1.0, 20, 0x01);
        let t2 = mock(20.0, 2.0, 40, 0x02);
        assert_eq!(t1.temp_c(), 20);
        assert_eq!(t2.temp_c(), 40);
        assert_eq!(t1.faults(), 0x01);
        assert_eq!(t2.faults(), 0x02);
        assert_eq!(t1.wheel_angle_mdeg(), 10_000);
        assert_eq!(t2.wheel_angle_mdeg(), 20_000);
        assert_eq!(t1.wheel_speed_mrad_s(), 1_000);
        assert_eq!(t2.wheel_speed_mrad_s(), 2_000);
    }

    // -- Common fault flag bit patterns ------------------------------------

    #[test]
    fn common_fault_flag_patterns() {
        // Single bit flags
        assert_eq!(mock(0.0, 0.0, 0, 0x01).faults(), 0x01);
        assert_eq!(mock(0.0, 0.0, 0, 0x02).faults(), 0x02);
        assert_eq!(mock(0.0, 0.0, 0, 0x04).faults(), 0x04);
        assert_eq!(mock(0.0, 0.0, 0, 0x80).faults(), 0x80);
        // Combined flags
        assert_eq!(mock(0.0, 0.0, 0, 0x05).faults(), 0x05);
    }

    // -- Symmetry: positive and negative yield opposite signs ---------------

    #[test]
    fn angle_sign_symmetry() {
        let pos = mock(45.0, 0.0, 0, 0).wheel_angle_mdeg();
        let neg = mock(-45.0, 0.0, 0, 0).wheel_angle_mdeg();
        assert_eq!(pos, -neg);
    }

    #[test]
    fn speed_sign_symmetry() {
        let pos = mock(0.0, 7.0, 0, 0).wheel_speed_mrad_s();
        let neg = mock(0.0, -7.0, 0, 0).wheel_speed_mrad_s();
        assert_eq!(pos, -neg);
    }
}

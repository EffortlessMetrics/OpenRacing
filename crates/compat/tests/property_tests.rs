//! Property-based tests for the compat crate.
//!
//! Tests cover: TelemetryCompat conversion round-trips via proptest,
//! deprecated API shim consistency, and edge cases around f32 precision.

#![allow(clippy::redundant_closure)]

use compat::TelemetryCompat;
use proptest::prelude::*;
use racing_wheel_engine::TelemetryData;
use std::time::Instant;

// ---------------------------------------------------------------------------
// Newtype wrapper (orphan rule)
// ---------------------------------------------------------------------------

struct Compat(TelemetryData);

impl TelemetryCompat for Compat {
    fn temp_c(&self) -> u8 {
        self.0.temperature_c
    }
    fn faults(&self) -> u8 {
        self.0.fault_flags
    }
    fn wheel_angle_mdeg(&self) -> i32 {
        (self.0.wheel_angle_deg * 1000.0) as i32
    }
    fn wheel_speed_mrad_s(&self) -> i32 {
        (self.0.wheel_speed_rad_s * 1000.0) as i32
    }
    fn sequence(&self) -> u32 {
        0
    }
}

fn sample(angle_deg: f32, speed_rad_s: f32, temp: u8, faults: u8) -> Compat {
    Compat(TelemetryData {
        wheel_angle_deg: angle_deg,
        wheel_speed_rad_s: speed_rad_s,
        temperature_c: temp,
        fault_flags: faults,
        hands_on: false,
        timestamp: Instant::now(),
    })
}

// ---------------------------------------------------------------------------
// Proptest: conversion consistency
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn prop_temp_c_identity(temp in any::<u8>()) {
        let t = sample(0.0, 0.0, temp, 0);
        prop_assert_eq!(t.temp_c(), temp);
        prop_assert_eq!(t.temp_c(), t.0.temperature_c);
    }

    #[test]
    fn prop_faults_identity(faults in any::<u8>()) {
        let t = sample(0.0, 0.0, 0, faults);
        prop_assert_eq!(t.faults(), faults);
        prop_assert_eq!(t.faults(), t.0.fault_flags);
    }

    #[test]
    fn prop_sequence_always_zero(
        angle in -1000.0f32..1000.0f32,
        speed in -500.0f32..500.0f32,
        temp in any::<u8>(),
        faults in any::<u8>(),
    ) {
        let t = sample(angle, speed, temp, faults);
        prop_assert_eq!(t.sequence(), 0);
    }

    #[test]
    fn prop_wheel_angle_mdeg_matches_manual(angle in -900.0f32..900.0f32) {
        let t = sample(angle, 0.0, 0, 0);
        let expected = (angle * 1000.0) as i32;
        prop_assert_eq!(t.wheel_angle_mdeg(), expected);
    }

    #[test]
    fn prop_wheel_speed_mrad_s_matches_manual(speed in -500.0f32..500.0f32) {
        let t = sample(0.0, speed, 0, 0);
        let expected = (speed * 1000.0) as i32;
        prop_assert_eq!(t.wheel_speed_mrad_s(), expected);
    }

    // -----------------------------------------------------------------------
    // Proptest: sign symmetry for conversions
    // -----------------------------------------------------------------------

    #[test]
    fn prop_angle_sign_symmetry(angle in 0.001f32..900.0f32) {
        let pos = sample(angle, 0.0, 0, 0).wheel_angle_mdeg();
        let neg = sample(-angle, 0.0, 0, 0).wheel_angle_mdeg();
        prop_assert_eq!(pos, -neg);
    }

    #[test]
    fn prop_speed_sign_symmetry(speed in 0.001f32..500.0f32) {
        let pos = sample(0.0, speed, 0, 0).wheel_speed_mrad_s();
        let neg = sample(0.0, -speed, 0, 0).wheel_speed_mrad_s();
        prop_assert_eq!(pos, -neg);
    }

    // -----------------------------------------------------------------------
    // Proptest: cross-field isolation
    // -----------------------------------------------------------------------

    #[test]
    fn prop_temp_independent_of_angle_and_speed(
        temp in any::<u8>(),
        angle in -900.0f32..900.0f32,
        speed in -500.0f32..500.0f32,
    ) {
        let t1 = sample(0.0, 0.0, temp, 0);
        let t2 = sample(angle, speed, temp, 0);
        prop_assert_eq!(t1.temp_c(), t2.temp_c());
    }

    #[test]
    fn prop_faults_independent_of_angle_and_speed(
        faults in any::<u8>(),
        angle in -900.0f32..900.0f32,
        speed in -500.0f32..500.0f32,
    ) {
        let t1 = sample(0.0, 0.0, 0, faults);
        let t2 = sample(angle, speed, 0, faults);
        prop_assert_eq!(t1.faults(), t2.faults());
    }

    // -----------------------------------------------------------------------
    // Proptest: idempotent access
    // -----------------------------------------------------------------------

    #[test]
    fn prop_repeated_access_is_idempotent(
        angle in -900.0f32..900.0f32,
        speed in -500.0f32..500.0f32,
        temp in any::<u8>(),
        faults in any::<u8>(),
    ) {
        let t = sample(angle, speed, temp, faults);
        prop_assert_eq!(t.temp_c(), t.temp_c());
        prop_assert_eq!(t.faults(), t.faults());
        prop_assert_eq!(t.wheel_angle_mdeg(), t.wheel_angle_mdeg());
        prop_assert_eq!(t.wheel_speed_mrad_s(), t.wheel_speed_mrad_s());
        prop_assert_eq!(t.sequence(), t.sequence());
    }
}

// ---------------------------------------------------------------------------
// Trait object tests (deprecated API shim via dynamic dispatch)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod deprecated_api_shims {
    use super::*;

    #[test]
    fn test_dyn_dispatch_preserves_values() -> Result<(), Box<dyn std::error::Error>> {
        let t = sample(45.0, 3.0, 50, 0x10);
        let dyn_ref: &dyn TelemetryCompat = &t;
        assert_eq!(dyn_ref.temp_c(), 50);
        assert_eq!(dyn_ref.faults(), 0x10);
        assert_eq!(dyn_ref.wheel_angle_mdeg(), 45000);
        assert_eq!(dyn_ref.wheel_speed_mrad_s(), 3000);
        assert_eq!(dyn_ref.sequence(), 0);
        Ok(())
    }

    #[test]
    fn test_zero_values_through_compat() -> Result<(), Box<dyn std::error::Error>> {
        let t = sample(0.0, 0.0, 0, 0);
        assert_eq!(t.temp_c(), 0);
        assert_eq!(t.faults(), 0);
        assert_eq!(t.wheel_angle_mdeg(), 0);
        assert_eq!(t.wheel_speed_mrad_s(), 0);
        assert_eq!(t.sequence(), 0);
        Ok(())
    }

    #[test]
    fn test_boundary_u8_values() -> Result<(), Box<dyn std::error::Error>> {
        let t_min = sample(0.0, 0.0, u8::MIN, u8::MIN);
        assert_eq!(t_min.temp_c(), 0);
        assert_eq!(t_min.faults(), 0);

        let t_max = sample(0.0, 0.0, u8::MAX, u8::MAX);
        assert_eq!(t_max.temp_c(), 255);
        assert_eq!(t_max.faults(), 255);
        Ok(())
    }

    #[test]
    fn test_negative_zero_produces_zero() -> Result<(), Box<dyn std::error::Error>> {
        let t = sample(-0.0, -0.0, 0, 0);
        assert_eq!(t.wheel_angle_mdeg(), 0);
        assert_eq!(t.wheel_speed_mrad_s(), 0);
        Ok(())
    }
}

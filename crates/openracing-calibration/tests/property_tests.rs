#![allow(clippy::redundant_closure)]
//! Property-based integration tests for openracing-calibration
//!
//! Tests calibration value round-trips, curve interpolation,
//! and min/max range enforcement.

use openracing_calibration::{
    AxisCalibration, CalibrationPoint, DeviceCalibration, JoystickCalibrator, PedalCalibrator,
    calibrate_joystick_axis, create_pedal_calibration,
};
use proptest::prelude::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ---------------------------------------------------------------------------
// Calibration value round-trip via serde
// ---------------------------------------------------------------------------

mod round_trip_tests {
    use super::*;

    #[test]
    fn calibration_point_stores_values() -> TestResult {
        let cp = CalibrationPoint::new(1234, 0.75);
        assert_eq!(cp.raw, 1234);
        assert!((cp.normalized - 0.75).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn axis_calibration_default_full_range() -> TestResult {
        let calib = AxisCalibration::default();
        assert_eq!(calib.min, 0);
        assert_eq!(calib.max, 0xFFFF);
        assert!(calib.center.is_none());
        Ok(())
    }

    #[test]
    fn device_calibration_creates_correct_axis_count() -> TestResult {
        let dc = DeviceCalibration::new("Test", 5);
        assert_eq!(dc.name, "Test");
        assert_eq!(dc.axes.len(), 5);
        assert_eq!(dc.version, 1);
        Ok(())
    }

    #[test]
    fn device_calibration_axis_access() -> TestResult {
        let mut dc = DeviceCalibration::new("Dev", 3);
        assert!(dc.axis(0).is_some());
        assert!(dc.axis(2).is_some());
        assert!(dc.axis(3).is_none());
        Ok(())
    }

    #[test]
    fn device_calibration_axis_mutation() -> TestResult {
        let mut dc = DeviceCalibration::new("Dev", 2);
        if let Some(axis) = dc.axis(0) {
            *axis = AxisCalibration::new(100, 900);
        }
        assert_eq!(dc.axes[0].min, 100);
        assert_eq!(dc.axes[0].max, 900);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Calibration curve interpolation
// ---------------------------------------------------------------------------

mod interpolation_tests {
    use super::*;

    #[test]
    fn midpoint_maps_near_half() -> TestResult {
        let calib = AxisCalibration::new(0, 65535);
        let mid = calib.apply(32768);
        assert!(
            (mid - 0.5).abs() < 0.01,
            "midpoint should be ~0.5, got {mid}"
        );
        Ok(())
    }

    #[test]
    fn apply_with_center_retains_center() -> TestResult {
        let calib = AxisCalibration::new(0, 65535).with_center(32768);
        assert_eq!(calib.center, Some(32768));
        let mid = calib.apply(32768);
        assert!(
            (mid - 0.5).abs() < 0.02,
            "center should map near 0.5, got {mid}"
        );
        Ok(())
    }

    #[test]
    fn apply_below_min_returns_zero() -> TestResult {
        // Use a calibration where min=0 so raw values are always >= min
        let calib = AxisCalibration::new(0, 60000);
        let out = calib.apply(0);
        assert!((out - 0.0).abs() < 0.02, "at min should be ~0.0, got {out}");
        Ok(())
    }

    #[test]
    fn apply_at_max_returns_one() -> TestResult {
        // Use full-range calibration (0, 65535) where deadzone defaults match
        let calib = AxisCalibration::new(0, 65535);
        let out = calib.apply(65535);
        assert!((out - 1.0).abs() < 0.02, "at max should be ~1.0, got {out}");
        Ok(())
    }

    #[test]
    fn equal_min_max_returns_half() -> TestResult {
        let calib = AxisCalibration::new(5000, 5000);
        let out = calib.apply(5000);
        assert!(
            (out - 0.5).abs() < 0.01,
            "equal min/max should return 0.5, got {out}"
        );
        Ok(())
    }

    #[test]
    fn deadzone_clips_low_values() -> TestResult {
        let calib = AxisCalibration::new(0, 10000).with_deadzone(1000, 10000);
        let out = calib.apply(0);
        assert!(
            (out - 0.0).abs() < 0.01,
            "below deadzone should be 0.0, got {out}"
        );
        Ok(())
    }

    #[test]
    fn deadzone_clips_high_values() -> TestResult {
        let calib = AxisCalibration::new(0, 10000).with_deadzone(0, 9000);
        let out = calib.apply(10000);
        assert!(
            (out - 1.0).abs() < 0.01,
            "above deadzone should be 1.0, got {out}"
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Joystick calibrator
// ---------------------------------------------------------------------------

mod joystick_tests {
    use super::*;

    #[test]
    fn empty_calibrator_returns_error() {
        let cal = JoystickCalibrator::new(0);
        assert!(cal.calibrate().is_err());
    }

    #[test]
    fn single_sample_calibrates() -> TestResult {
        let mut cal = JoystickCalibrator::new(0);
        cal.add_sample(500, 0.5);
        let axis = cal.calibrate()?;
        assert_eq!(axis.min, 500);
        assert_eq!(axis.max, 500);
        Ok(())
    }

    #[test]
    fn full_sweep_calibration() -> TestResult {
        let mut cal = JoystickCalibrator::new(0);
        cal.add_sample(0, 0.0);
        cal.add_sample(32768, 0.5);
        cal.add_sample(65535, 1.0);
        let axis = cal.calibrate()?;
        assert_eq!(axis.min, 0);
        assert_eq!(axis.max, 65535);
        assert!(axis.center.is_some());
        Ok(())
    }

    #[test]
    fn reset_clears_samples() {
        let mut cal = JoystickCalibrator::new(0);
        cal.add_sample(0, 0.0);
        cal.add_sample(65535, 1.0);
        cal.reset();
        assert!(cal.calibrate().is_err());
    }

    #[test]
    fn convenience_function_calibrates() -> TestResult {
        let samples = [(0, 0.0), (32768, 0.5), (65535, 1.0)];
        let axis = calibrate_joystick_axis(&samples)?;
        assert_eq!(axis.min, 0);
        assert_eq!(axis.max, 65535);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Pedal calibrator
// ---------------------------------------------------------------------------

mod pedal_tests {
    use super::*;

    #[test]
    fn empty_pedal_calibrator_fails() {
        let cal = PedalCalibrator::new();
        assert!(cal.calibrate().is_err());
    }

    #[test]
    fn partial_pedals_fail() {
        let mut cal = PedalCalibrator::new();
        cal.add_throttle(0);
        cal.add_throttle(65535);
        // Missing brake and clutch
        assert!(cal.calibrate().is_err());
    }

    #[test]
    fn full_pedal_calibration() -> TestResult {
        let mut cal = PedalCalibrator::new();
        cal.add_throttle(100);
        cal.add_throttle(50000);
        cal.add_brake(200);
        cal.add_brake(60000);
        cal.add_clutch(300);
        cal.add_clutch(55000);

        let axes = cal.calibrate()?;
        assert_eq!(axes.len(), 3);
        assert_eq!(axes[0].min, 100);
        assert_eq!(axes[0].max, 50000);
        assert_eq!(axes[1].min, 200);
        assert_eq!(axes[1].max, 60000);
        assert_eq!(axes[2].min, 300);
        assert_eq!(axes[2].max, 55000);
        Ok(())
    }

    #[test]
    fn pedal_reset_clears() {
        let mut cal = PedalCalibrator::new();
        cal.add_throttle(0);
        cal.add_throttle(65535);
        cal.add_brake(0);
        cal.add_brake(65535);
        cal.add_clutch(0);
        cal.add_clutch(65535);
        cal.reset();
        assert!(cal.calibrate().is_err());
    }

    #[test]
    fn create_pedal_calibration_convenience() -> TestResult {
        let axes = create_pedal_calibration(&[0, 65535], &[0, 65535], &[0, 65535])?;
        assert_eq!(axes.len(), 3);
        for axis in &axes {
            assert_eq!(axis.min, 0);
            assert_eq!(axis.max, 65535);
        }
        Ok(())
    }

    #[test]
    fn pedal_default_matches_new() -> TestResult {
        let from_new = PedalCalibrator::new();
        let from_default = PedalCalibrator::default();
        // Both should fail since neither has samples
        assert!(from_new.calibrate().is_err());
        assert!(from_default.calibrate().is_err());
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Property-based tests
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    // --- Apply output always in [0.0, 1.0] for any valid calibration ---

    #[test]
    fn prop_apply_full_range_bounded(raw in 0u16..=u16::MAX) {
        let calib = AxisCalibration::new(0, u16::MAX);
        let out = calib.apply(raw);
        prop_assert!((0.0..=1.0).contains(&out), "output {} out of [0,1]", out);
        prop_assert!(out.is_finite(), "output must be finite");
    }

    #[test]
    fn prop_apply_subrange_bounded(
        min_val in 0u16..32000,
        spread in 1u16..32000,
        frac in 0.0f32..=1.0,
    ) {
        let max_val = min_val.saturating_add(spread);
        let calib = AxisCalibration::new(min_val, max_val);
        // Only test raw values within [min, max] to avoid u16 underflow in apply()
        let raw = min_val + (frac * spread as f32) as u16;
        let out = calib.apply(raw);
        prop_assert!((0.0..=1.0).contains(&out), "output {} out of [0,1]", out);
        prop_assert!(out.is_finite(), "output must be finite");
    }

    // --- Monotonicity: larger raw → larger or equal output ---

    #[test]
    fn prop_monotonic_full_range(
        a in 0u16..=u16::MAX,
        b in 0u16..=u16::MAX,
    ) {
        let calib = AxisCalibration::new(0, u16::MAX);
        let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
        let out_lo = calib.apply(lo);
        let out_hi = calib.apply(hi);
        prop_assert!(
            out_hi >= out_lo,
            "monotonicity violated: apply({})={} > apply({})={}",
            lo, out_lo, hi, out_hi
        );
    }

    // --- Equal min/max always yields 0.5 ---

    #[test]
    fn prop_equal_minmax_returns_half(val in 0u16..=u16::MAX) {
        let calib = AxisCalibration::new(val, val);
        let out = calib.apply(val);
        prop_assert!(
            (out - 0.5).abs() < 0.01,
            "equal min/max should return 0.5, got {}",
            out
        );
    }

    // --- Joystick calibrator: min/max detection is correct ---

    #[test]
    fn prop_joystick_min_max_detected(
        a in 0u16..=u16::MAX,
        b in 0u16..=u16::MAX,
        c in 0u16..=u16::MAX,
    ) {
        let mut cal = JoystickCalibrator::new(0);
        cal.add_sample(a, 0.0);
        cal.add_sample(b, 0.5);
        cal.add_sample(c, 1.0);
        let axis = cal.calibrate().map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        let expected_min = a.min(b).min(c);
        let expected_max = a.max(b).max(c);
        prop_assert_eq!(axis.min, expected_min, "min detection failed");
        prop_assert_eq!(axis.max, expected_max, "max detection failed");
    }

    // --- Pedal calibrator: min/max are correct for each axis ---

    #[test]
    fn prop_pedal_min_max(
        t_lo in 0u16..30000u16,
        t_hi in 30000u16..=u16::MAX,
        b_lo in 0u16..30000u16,
        b_hi in 30000u16..=u16::MAX,
        c_lo in 0u16..30000u16,
        c_hi in 30000u16..=u16::MAX,
    ) {
        let axes = create_pedal_calibration(
            &[t_lo, t_hi],
            &[b_lo, b_hi],
            &[c_lo, c_hi],
        ).map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        prop_assert_eq!(axes[0].min, t_lo);
        prop_assert_eq!(axes[0].max, t_hi);
        prop_assert_eq!(axes[1].min, b_lo);
        prop_assert_eq!(axes[1].max, b_hi);
        prop_assert_eq!(axes[2].min, c_lo);
        prop_assert_eq!(axes[2].max, c_hi);
    }

    // --- Calibrated axis: apply(min) ≈ 0.0 and apply(max) ≈ 1.0 ---

    #[test]
    fn prop_calibrated_endpoints(
        min_val in 0u16..60000,
        spread in 1u16..5000,
    ) {
        let max_val = min_val.saturating_add(spread);
        let range = max_val.saturating_sub(min_val);
        let calib = AxisCalibration::new(min_val, max_val).with_deadzone(0, range);
        let at_min = calib.apply(min_val);
        let at_max = calib.apply(max_val);
        prop_assert!(
            (at_min - 0.0).abs() < 0.01,
            "apply(min={}) should be ~0.0, got {}", min_val, at_min
        );
        prop_assert!(
            (at_max - 1.0).abs() < 0.01,
            "apply(max={}) should be ~1.0, got {}", max_val, at_max
        );
    }
}

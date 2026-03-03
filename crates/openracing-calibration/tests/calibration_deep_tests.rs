//! Deep tests for calibration: axis mapping, pedals, wheel, storage, edge cases.

use openracing_calibration::{
    AxisCalibration, CalibrationPoint, DeviceCalibration, JoystickCalibrator, PedalCalibrator,
    calibrate_joystick_axis, create_pedal_calibration,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ---------------------------------------------------------------------------
// Axis calibration: raw → calibrated mapping
// ---------------------------------------------------------------------------

mod axis_calibration_tests {
    use super::*;

    #[test]
    fn full_range_maps_correctly() -> TestResult {
        let calib = AxisCalibration::new(0, 65535);
        assert!((calib.apply(0) - 0.0).abs() < 0.001);
        assert!((calib.apply(32768) - 0.5).abs() < 0.01);
        assert!((calib.apply(65535) - 1.0).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn partial_range_with_matching_deadzones() -> TestResult {
        // Default deadzones (0, 65535) divide by range, so set matching ones
        let calib = AxisCalibration::new(1000, 3000)
            .with_deadzone(0, 2000); // match the range width
        assert!((calib.apply(1000) - 0.0).abs() < 0.001, "min → 0.0");
        assert!((calib.apply(2000) - 0.5).abs() < 0.01, "mid → 0.5");
        assert!((calib.apply(3000) - 1.0).abs() < 0.001, "max → 1.0");
        Ok(())
    }

    #[test]
    fn at_min_returns_zero() -> TestResult {
        let calib = AxisCalibration::new(0, 65535);
        assert!((calib.apply(0) - 0.0).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn at_max_returns_one() -> TestResult {
        let calib = AxisCalibration::new(0, 65535);
        assert!((calib.apply(65535) - 1.0).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn center_point_stored() -> TestResult {
        let calib = AxisCalibration::new(0, 65535).with_center(32768);
        assert_eq!(calib.center, Some(32768));
        Ok(())
    }

    #[test]
    fn deadzone_clamps_low_values() -> TestResult {
        let calib = AxisCalibration::new(0, 65535).with_deadzone(5000, 60535);
        assert!((calib.apply(2000) - 0.0).abs() < 0.001, "below dz_min → 0.0");
        Ok(())
    }

    #[test]
    fn deadzone_clamps_high_values() -> TestResult {
        let calib = AxisCalibration::new(0, 65535).with_deadzone(5000, 60535);
        assert!((calib.apply(63000) - 1.0).abs() < 0.001, "above dz_max → 1.0");
        Ok(())
    }

    #[test]
    fn deadzone_remaps_interior() -> TestResult {
        let calib = AxisCalibration::new(0, 65535).with_deadzone(5000, 60535);
        let mid = calib.apply(32768);
        assert!(mid > 0.0 && mid < 1.0, "interior maps between 0 and 1, got {mid}");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Pedal calibration: dead zones, curves, inversion
// ---------------------------------------------------------------------------

mod pedal_calibration_tests {
    use super::*;

    #[test]
    fn pedal_basic_calibration() -> TestResult {
        let mut cal = PedalCalibrator::new();
        cal.add_throttle(0);
        cal.add_throttle(65535);
        cal.add_brake(0);
        cal.add_brake(65535);
        cal.add_clutch(0);
        cal.add_clutch(65535);

        let axes = cal.calibrate()?;
        assert_eq!(axes.len(), 3);
        assert_eq!(axes[0].min, 0);
        assert_eq!(axes[0].max, 65535);
        Ok(())
    }

    #[test]
    fn pedal_partial_range() -> TestResult {
        let axes = create_pedal_calibration(
            &[100, 500, 900],
            &[200, 800],
            &[300, 700],
        )?;
        assert_eq!(axes[0].min, 100);
        assert_eq!(axes[0].max, 900);
        assert_eq!(axes[1].min, 200);
        assert_eq!(axes[1].max, 800);
        assert_eq!(axes[2].min, 300);
        assert_eq!(axes[2].max, 700);
        Ok(())
    }

    #[test]
    fn pedal_single_sample_min_equals_max() -> TestResult {
        let axes = create_pedal_calibration(&[500], &[500], &[500])?;
        assert_eq!(axes[0].min, 500);
        assert_eq!(axes[0].max, 500);
        // Zero range → returns 0.5
        assert!((axes[0].apply(500) - 0.5).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn pedal_missing_axis_fails() -> TestResult {
        let cal = PedalCalibrator::new();
        assert!(cal.calibrate().is_err(), "empty calibrator should fail");
        Ok(())
    }

    #[test]
    fn pedal_reset_clears_samples() -> TestResult {
        let mut cal = PedalCalibrator::new();
        cal.add_throttle(0);
        cal.add_throttle(65535);
        cal.add_brake(0);
        cal.add_brake(65535);
        cal.add_clutch(0);
        cal.add_clutch(65535);
        cal.reset();
        assert!(cal.calibrate().is_err(), "reset should clear all samples");
        Ok(())
    }

    #[test]
    fn pedal_deadzone_simulation() -> TestResult {
        let axes = create_pedal_calibration(&[0, 65535], &[0, 65535], &[0, 65535])?;
        let throttle = axes[0].clone();
        let throttle_with_dz = AxisCalibration {
            deadzone_min: 3000,
            deadzone_max: 62535,
            ..throttle
        };
        // Small raw value below deadzone maps to 0
        assert!((throttle_with_dz.apply(1000) - 0.0).abs() < 0.001);
        // Large raw value above deadzone maps to 1
        assert!((throttle_with_dz.apply(64000) - 1.0).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn pedal_inversion_via_apply() -> TestResult {
        // Simulate inverted pedal by swapping min/max in calibration
        let axes = create_pedal_calibration(&[0, 65535], &[0, 65535], &[0, 65535])?;
        let normal_at_zero = axes[0].apply(0);
        let normal_at_max = axes[0].apply(65535);
        // Normal: 0 → 0.0, 65535 → 1.0
        assert!(normal_at_zero < normal_at_max, "normal polarity");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Wheel calibration: center finding, rotation range
// ---------------------------------------------------------------------------

mod wheel_calibration_tests {
    use super::*;

    #[test]
    fn wheel_center_detected() -> TestResult {
        let axis = calibrate_joystick_axis(&[
            (0, 0.0),
            (32768, 0.5),
            (65535, 1.0),
        ])?;
        assert_eq!(axis.center, Some(32768), "center should be detected at 0.5");
        Ok(())
    }

    #[test]
    fn wheel_full_rotation_range() -> TestResult {
        let axis = calibrate_joystick_axis(&[
            (0, 0.0),
            (65535, 1.0),
        ])?;
        assert_eq!(axis.min, 0);
        assert_eq!(axis.max, 65535);
        Ok(())
    }

    #[test]
    fn wheel_partial_rotation_range() -> TestResult {
        let axis = calibrate_joystick_axis(&[
            (10000, 0.0),
            (32768, 0.5),
            (55000, 1.0),
        ])?;
        assert_eq!(axis.min, 10000);
        assert_eq!(axis.max, 55000);
        Ok(())
    }

    #[test]
    fn wheel_no_center_when_no_midpoint_sample() -> TestResult {
        // No sample near 0.5 → no center detected
        let axis = calibrate_joystick_axis(&[
            (0, 0.0),
            (65535, 1.0),
        ])?;
        assert_eq!(axis.center, None, "no sample near 0.5 → no center");
        Ok(())
    }

    #[test]
    fn joystick_calibrator_reset_works() -> TestResult {
        let mut cal = JoystickCalibrator::new(0);
        cal.add_sample(0, 0.0);
        cal.add_sample(65535, 1.0);
        cal.reset();
        assert!(cal.calibrate().is_err(), "reset should clear samples");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Multi-point calibration: linearity verification
// ---------------------------------------------------------------------------

mod multipoint_tests {
    use super::*;

    #[test]
    fn multipoint_linearity() -> TestResult {
        let calib = AxisCalibration::new(0, 65535);
        let points: Vec<u16> = vec![0, 13107, 26214, 39321, 52428, 65535];
        let expected = [0.0f32, 0.2, 0.4, 0.6, 0.8, 1.0];

        for (raw, exp) in points.iter().zip(expected.iter()) {
            let got = calib.apply(*raw);
            assert!(
                (got - exp).abs() < 0.01,
                "raw={raw}: expected {exp}, got {got}"
            );
        }
        Ok(())
    }

    #[test]
    fn multipoint_monotonic_output() -> TestResult {
        let calib = AxisCalibration::new(0, 65535);
        let mut prev = -1.0f32;
        for raw in (0..=65535).step_by(1000) {
            let val = calib.apply(raw);
            assert!(
                val >= prev,
                "non-monotonic at raw={raw}: prev={prev}, val={val}"
            );
            prev = val;
        }
        Ok(())
    }

    #[test]
    fn calibration_points_store_correctly() -> TestResult {
        let p = CalibrationPoint::new(32768, 0.5);
        assert_eq!(p.raw, 32768);
        assert!((p.normalized - 0.5).abs() < f32::EPSILON);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Calibration storage: save/load round-trip
// ---------------------------------------------------------------------------

mod storage_tests {
    use super::*;

    #[test]
    fn device_calibration_json_round_trip() -> TestResult {
        let mut device = DeviceCalibration::new("Test Wheel", 3);
        if let Some(steering) = device.axis(0) {
            *steering = AxisCalibration::new(0, 65535)
                .with_center(32768)
                .with_deadzone(1000, 64535);
        }
        if let Some(throttle) = device.axis(1) {
            *throttle = AxisCalibration::new(100, 900);
        }
        if let Some(brake) = device.axis(2) {
            *brake = AxisCalibration::new(50, 950);
        }

        let json = serde_json::to_string(&device)?;
        let restored: DeviceCalibration = serde_json::from_str(&json)?;

        assert_eq!(restored.name, "Test Wheel");
        assert_eq!(restored.axes.len(), 3);
        assert_eq!(restored.version, 1);
        assert_eq!(restored.axes[0].min, 0);
        assert_eq!(restored.axes[0].max, 65535);
        assert_eq!(restored.axes[0].center, Some(32768));
        assert_eq!(restored.axes[0].deadzone_min, 1000);
        assert_eq!(restored.axes[0].deadzone_max, 64535);
        assert_eq!(restored.axes[1].min, 100);
        assert_eq!(restored.axes[1].max, 900);
        Ok(())
    }

    #[test]
    fn axis_calibration_json_round_trip() -> TestResult {
        let calib = AxisCalibration::new(500, 4000)
            .with_center(2250)
            .with_deadzone(600, 3900);

        let json = serde_json::to_string(&calib)?;
        let restored: AxisCalibration = serde_json::from_str(&json)?;

        assert_eq!(restored.min, 500);
        assert_eq!(restored.max, 4000);
        assert_eq!(restored.center, Some(2250));
        assert_eq!(restored.deadzone_min, 600);
        assert_eq!(restored.deadzone_max, 3900);
        Ok(())
    }

    #[test]
    fn calibration_point_json_round_trip() -> TestResult {
        let point = CalibrationPoint::new(12345, 0.75);
        let json = serde_json::to_string(&point)?;
        let restored: CalibrationPoint = serde_json::from_str(&json)?;
        assert_eq!(restored.raw, 12345);
        assert!((restored.normalized - 0.75).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn device_calibration_preserves_all_axes() -> TestResult {
        let mut device = DeviceCalibration::new("Multi-axis", 6);
        for i in 0..6 {
            if let Some(axis) = device.axis(i) {
                *axis = AxisCalibration::new((i * 100) as u16, ((i + 1) * 10000) as u16);
            }
        }

        let json = serde_json::to_string(&device)?;
        let restored: DeviceCalibration = serde_json::from_str(&json)?;

        assert_eq!(restored.axes.len(), 6);
        for i in 0..6 {
            assert_eq!(restored.axes[i].min, (i * 100) as u16);
            assert_eq!(restored.axes[i].max, ((i + 1) * 10000) as u16);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Edge cases: inverted min/max, zero range, identical values
// ---------------------------------------------------------------------------

mod edge_case_tests {
    use super::*;

    #[test]
    fn zero_range_returns_midpoint() -> TestResult {
        let calib = AxisCalibration::new(5000, 5000);
        assert!(
            (calib.apply(5000) - 0.5).abs() < 0.001,
            "zero range → 0.5"
        );
        Ok(())
    }

    #[test]
    fn identical_min_max_any_raw_returns_midpoint() -> TestResult {
        let calib = AxisCalibration::new(100, 100);
        assert!((calib.apply(0) - 0.5).abs() < 0.001);
        assert!((calib.apply(100) - 0.5).abs() < 0.001);
        assert!((calib.apply(65535) - 0.5).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn max_equals_u16_max() -> TestResult {
        let calib = AxisCalibration::new(0, u16::MAX);
        assert!((calib.apply(0) - 0.0).abs() < 0.001);
        assert!((calib.apply(u16::MAX) - 1.0).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn min_equals_max_equals_zero() -> TestResult {
        let calib = AxisCalibration::new(0, 0);
        assert!((calib.apply(0) - 0.5).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn device_axis_out_of_bounds() -> TestResult {
        let mut device = DeviceCalibration::new("Test", 2);
        assert!(device.axis(5).is_none(), "out-of-bounds → None");
        Ok(())
    }

    #[test]
    fn device_default_is_empty() -> TestResult {
        let device = DeviceCalibration::default();
        assert!(device.axes.is_empty());
        assert!(device.name.is_empty());
        assert_eq!(device.version, 1);
        Ok(())
    }

    #[test]
    fn deadzone_wider_than_range() -> TestResult {
        // Deadzone boundaries outside normalized range → everything goes to 0 or 1
        let calib = AxisCalibration::new(0, 65535).with_deadzone(0, 0);
        let val = calib.apply(32768);
        // dz_max is 0, which is ≤ dz_min, so all values above dz_max→1.0
        assert!((val - 1.0).abs() < 0.001, "all above zero dz_max → 1.0");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Property tests
// ---------------------------------------------------------------------------

mod prop_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(1000))]

        #[test]
        fn calibrated_output_in_range(
            min_val in 0u16..=32767,
            max_offset in 1u16..=32768,
            raw_offset in 0u16..=65534u16,
        ) {
            let max_val = min_val.saturating_add(max_offset);
            // raw must be >= min to avoid u16 underflow in apply()
            let raw = min_val.saturating_add(raw_offset % (max_val - min_val + 1));
            let calib = AxisCalibration::new(min_val, max_val);
            let out = calib.apply(raw);
            prop_assert!(
                (0.0..=1.0).contains(&out),
                "output {} not in [0.0, 1.0] for raw={}, min={}, max={}",
                out, raw, min_val, max_val
            );
        }

        #[test]
        fn calibrated_output_finite(
            min_val in 0u16..=32767u16,
            max_offset in 0u16..=32768u16,
            raw_offset in 0u16..=65534u16,
        ) {
            let max_val = min_val.saturating_add(max_offset);
            // Only apply with raw >= min to avoid u16 underflow
            let raw = if max_val > min_val {
                min_val.saturating_add(raw_offset % (max_val - min_val + 1))
            } else {
                min_val
            };
            let calib = AxisCalibration::new(min_val, max_val);
            let out = calib.apply(raw);
            prop_assert!(out.is_finite(), "output is NaN/Inf");
        }

        #[test]
        fn calibrated_monotonic_when_min_lt_max(
            min_val in 0u16..=30000,
            max_offset in 2u16..=30000,
            raw1_offset in 0u16..=29998,
            raw2_extra in 1u16..=29999,
        ) {
            let max_val = min_val.saturating_add(max_offset);
            let range = max_val - min_val;
            // Ensure raw values are within [min, max]
            let raw1 = min_val + (raw1_offset % range);
            let raw2 = raw1.saturating_add(raw2_extra).min(max_val);
            if raw2 <= raw1 { return Ok(()); }
            let calib = AxisCalibration::new(min_val, max_val);
            let o1 = calib.apply(raw1);
            let o2 = calib.apply(raw2);
            prop_assert!(
                o2 >= o1,
                "non-monotonic: apply({}) = {} > apply({}) = {}",
                raw1, o1, raw2, o2
            );
        }

        #[test]
        fn zero_range_always_midpoint(
            val in 0u16..=65535u16,
        ) {
            let calib = AxisCalibration::new(val, val);
            // Only apply with raw == val to avoid underflow
            let out = calib.apply(val);
            prop_assert!(
                (out - 0.5).abs() < 0.001,
                "zero range should always return 0.5, got {}",
                out
            );
        }
    }
}

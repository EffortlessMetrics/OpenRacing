//! Additional coverage tests for `openracing-calibration`.
//!
//! Pins paths the existing suite leaves uncovered. These tests
//! deliberately avoid raw values below `min` and inverted
//! `min > max` calibrations because both currently trigger a
//! `u16` underflow inside `AxisCalibration::apply` — that is a
//! separate, architecturally-significant bug worth its own PR.

use openracing_calibration::{
    AxisCalibration, CalibrationError, CalibrationPoint, DeviceCalibration, JoystickCalibrator,
    PedalCalibrator, calibrate_joystick_axis,
};

// ---------------------------------------------------------------------------
// AxisCalibration::apply boundary at the deadzone edges
// ---------------------------------------------------------------------------

#[test]
fn apply_at_exact_deadzone_min_boundary_returns_zero() {
    let cal = AxisCalibration::new(0, 1000).with_deadzone(200, 800);
    // normalized = 0.2, dz_min = 0.2; comparison is strict `<`, so equality
    // proceeds to remap → (0.2 - 0.2) / (0.8 - 0.2) = 0.0.
    assert!((cal.apply(200) - 0.0).abs() < 1e-5);
}

#[test]
fn apply_at_exact_deadzone_max_boundary_returns_one() {
    let cal = AxisCalibration::new(0, 1000).with_deadzone(200, 800);
    // normalized = 0.8, dz_max = 0.8; strict `>` means equality remaps to
    // (0.8 - 0.2) / (0.8 - 0.2) = 1.0.
    assert!((cal.apply(800) - 1.0).abs() < 1e-5);
}

#[test]
fn apply_just_inside_deadzone_min_clamps_to_zero() {
    let cal = AxisCalibration::new(0, 1000).with_deadzone(200, 800);
    // 199 → normalized 0.199 < dz_min 0.2 → 0.0 short-circuit.
    assert!((cal.apply(199) - 0.0).abs() < 1e-5);
}

#[test]
fn apply_just_inside_deadzone_max_clamps_to_one() {
    let cal = AxisCalibration::new(0, 1000).with_deadzone(200, 800);
    // 801 → normalized 0.801 > dz_max 0.8 → 1.0 short-circuit.
    assert!((cal.apply(801) - 1.0).abs() < 1e-5);
}

// ---------------------------------------------------------------------------
// AxisCalibration: Clone independence
// ---------------------------------------------------------------------------

#[test]
fn axis_calibration_clone_is_independent_of_original() {
    let original = AxisCalibration::new(0, 1000)
        .with_center(500)
        .with_deadzone(50, 950);
    let mut cloned = original.clone();

    cloned.deadzone_min = 999;
    cloned.deadzone_max = 999;

    assert_eq!(original.deadzone_min, 50);
    assert_eq!(original.deadzone_max, 950);
    assert_eq!(cloned.deadzone_min, 999);
    assert_eq!(cloned.deadzone_max, 999);
}

// ---------------------------------------------------------------------------
// CalibrationError: Debug + Display for every variant
// ---------------------------------------------------------------------------

#[test]
fn calibration_error_debug_format_contains_variant_name() {
    let invalid = CalibrationError::InvalidData;
    let not_complete = CalibrationError::NotComplete;
    let device = CalibrationError::DeviceError("timeout".to_string());

    assert!(format!("{invalid:?}").contains("InvalidData"));
    assert!(format!("{not_complete:?}").contains("NotComplete"));
    let device_dbg = format!("{device:?}");
    assert!(device_dbg.contains("DeviceError"));
    assert!(device_dbg.contains("timeout"));
}

#[test]
fn calibration_error_invalid_data_display_message_pinned() {
    assert_eq!(
        CalibrationError::InvalidData.to_string(),
        "Invalid calibration data"
    );
}

#[test]
fn calibration_error_device_error_carries_message_in_display() {
    let err = CalibrationError::DeviceError("usb stall".to_string());
    assert!(err.to_string().contains("usb stall"));
    assert!(err.to_string().contains("Device error"));
}

// ---------------------------------------------------------------------------
// DeviceCalibration: axis() mutation persists across calls
// ---------------------------------------------------------------------------

#[test]
fn device_calibration_axis_mutation_persists_across_calls() {
    let mut device = DeviceCalibration::new("test", 3);
    if let Some(axis) = device.axis(1) {
        axis.min = 100;
        axis.max = 900;
        axis.deadzone_min = 110;
        axis.deadzone_max = 890;
    }
    // Re-fetch the same axis — the mutation must have persisted.
    let axis_again = device.axis(1).expect("axis 1 exists");
    assert_eq!(axis_again.min, 100);
    assert_eq!(axis_again.max, 900);
    assert_eq!(axis_again.deadzone_min, 110);
    assert_eq!(axis_again.deadzone_max, 890);
}

#[test]
fn device_calibration_axis_and_direct_index_observe_same_state() {
    let mut device = DeviceCalibration::new("d", 2);
    if let Some(axis) = device.axis(0) {
        axis.max = 1234;
    }
    // Direct index access should observe the same value.
    assert_eq!(device.axes[0].max, 1234);
}

// ---------------------------------------------------------------------------
// PedalCalibrator: partial fill across two axes still rejected
// ---------------------------------------------------------------------------

#[test]
fn pedal_calibrator_throttle_and_brake_only_fails() {
    let mut cal = PedalCalibrator::new();
    cal.add_throttle(0);
    cal.add_throttle(1000);
    cal.add_brake(0);
    cal.add_brake(1000);
    // No clutch samples — must error.
    let result = cal.calibrate();
    assert!(matches!(result, Err(CalibrationError::NotComplete)));
}

#[test]
fn pedal_calibrator_brake_only_fails() {
    let mut cal = PedalCalibrator::new();
    cal.add_brake(0);
    cal.add_brake(1000);
    assert!(matches!(
        cal.calibrate(),
        Err(CalibrationError::NotComplete)
    ));
}

#[test]
fn pedal_calibrator_clutch_only_fails() {
    let mut cal = PedalCalibrator::new();
    cal.add_clutch(0);
    cal.add_clutch(1000);
    assert!(matches!(
        cal.calibrate(),
        Err(CalibrationError::NotComplete)
    ));
}

#[test]
fn pedal_calibrator_reset_clears_all_three_axes() {
    let mut cal = PedalCalibrator::new();
    cal.add_throttle(500);
    cal.add_brake(500);
    cal.add_clutch(500);
    cal.reset();
    assert!(matches!(
        cal.calibrate(),
        Err(CalibrationError::NotComplete)
    ));
}

// ---------------------------------------------------------------------------
// JoystickCalibrator: empty + reset
// ---------------------------------------------------------------------------

#[test]
fn joystick_calibrator_empty_calibrate_errors() {
    let cal = JoystickCalibrator::new(0);
    assert!(matches!(
        cal.calibrate(),
        Err(CalibrationError::NotComplete)
    ));
}

#[test]
fn joystick_calibrator_reset_drops_samples() {
    let mut cal = JoystickCalibrator::new(0);
    cal.add_sample(100, 0.0);
    cal.add_sample(900, 1.0);
    cal.reset();
    assert!(matches!(
        cal.calibrate(),
        Err(CalibrationError::NotComplete)
    ));
}

#[test]
fn calibrate_joystick_axis_empty_slice_errors() {
    let result = calibrate_joystick_axis(&[]);
    assert!(matches!(result, Err(CalibrationError::NotComplete)));
}

// ---------------------------------------------------------------------------
// CalibrationPoint Debug
// ---------------------------------------------------------------------------

#[test]
fn calibration_point_debug_contains_fields() {
    let p = CalibrationPoint::new(123, 0.25);
    let dbg = format!("{p:?}");
    assert!(dbg.contains("CalibrationPoint"));
    assert!(dbg.contains("123"));
    assert!(dbg.contains("0.25"));
}

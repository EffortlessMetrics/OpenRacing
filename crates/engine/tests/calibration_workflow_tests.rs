//! Deep tests for calibration workflows: auto-calibration, manual calibration,
//! persistence, interpolation, noisy data, pedals, and wheel center detection.

use openracing_calibration::{
    AxisCalibration, CalibrationPoint, DeviceCalibration, JoystickCalibrator, PedalCalibrator,
    calibrate_joystick_axis, create_pedal_calibration,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Tolerance for floating-point comparisons.
const TOL: f32 = 0.02;

fn assert_near(actual: f32, expected: f32, label: &str) {
    assert!(
        (actual - expected).abs() < TOL,
        "{label}: expected {expected}, got {actual}"
    );
}

// ---------------------------------------------------------------------------
// Auto-calibration sequence (full range detection)
// ---------------------------------------------------------------------------

#[test]
fn cal_01_auto_calibration_full_range_sweep() -> Result<(), Box<dyn std::error::Error>> {
    let mut cal = JoystickCalibrator::new(0);

    // Simulate a smooth sweep from 0 to 65535 in steps of ~6553
    for i in 0..=10 {
        let raw = (i as u32 * 65535 / 10) as u16;
        let norm = i as f32 / 10.0;
        cal.add_sample(raw, norm);
    }

    let axis = cal.calibrate()?;
    assert_eq!(axis.min, 0);
    assert_eq!(axis.max, 65535);
    assert!(axis.center.is_some(), "center should be detected near 0.5");
    Ok(())
}

#[test]
fn cal_02_auto_calibration_partial_range() -> Result<(), Box<dyn std::error::Error>> {
    let mut cal = JoystickCalibrator::new(0);

    cal.add_sample(10000, 0.0);
    cal.add_sample(30000, 0.5);
    cal.add_sample(50000, 1.0);

    let axis = cal.calibrate()?;
    assert_eq!(axis.min, 10000);
    assert_eq!(axis.max, 50000);
    Ok(())
}

#[test]
fn cal_03_auto_calibration_progressive_refinement() -> Result<(), Box<dyn std::error::Error>> {
    // First pass: coarse sweep
    let mut cal = JoystickCalibrator::new(0);
    cal.add_sample(1000, 0.0);
    cal.add_sample(32000, 0.5);
    cal.add_sample(64000, 1.0);
    let first = cal.calibrate()?;

    // Second pass: finer boundaries
    let mut cal2 = JoystickCalibrator::new(0);
    cal2.add_sample(500, 0.0);
    cal2.add_sample(32000, 0.5);
    cal2.add_sample(65000, 1.0);
    let second = cal2.calibrate()?;

    // Second pass widens the range
    assert!(second.min <= first.min);
    assert!(second.max >= first.max);
    Ok(())
}

// ---------------------------------------------------------------------------
// Manual calibration points
// ---------------------------------------------------------------------------

#[test]
fn cal_04_manual_calibration_explicit_range() {
    let axis = AxisCalibration::new(100, 900);
    assert_eq!(axis.min, 100);
    assert_eq!(axis.max, 900);
    assert!(axis.center.is_none());
}

#[test]
fn cal_05_manual_calibration_with_center() {
    let axis = AxisCalibration::new(0, 65535).with_center(32768);
    assert_eq!(axis.center, Some(32768));
}

#[test]
fn cal_06_manual_calibration_with_deadzone() {
    let axis = AxisCalibration::new(0, 65535).with_deadzone(1000, 64535);
    assert_eq!(axis.deadzone_min, 1000);
    assert_eq!(axis.deadzone_max, 64535);
}

#[test]
fn cal_07_manual_calibration_builder_chaining() {
    let axis = AxisCalibration::new(0, 65535)
        .with_center(32768)
        .with_deadzone(500, 65000);
    assert_eq!(axis.center, Some(32768));
    assert_eq!(axis.deadzone_min, 500);
    assert_eq!(axis.deadzone_max, 65000);
}

// ---------------------------------------------------------------------------
// Persistence and loading (JSON round-trip)
// ---------------------------------------------------------------------------

#[test]
fn cal_08_persistence_json_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    let mut device = DeviceCalibration::new("Test Wheel", 3);
    if let Some(steering) = device.axis(0) {
        *steering = AxisCalibration::new(0, 65535).with_center(32768);
    }
    if let Some(throttle) = device.axis(1) {
        *throttle = AxisCalibration::new(100, 60000);
    }

    let json = serde_json::to_string(&device)?;
    let restored: DeviceCalibration = serde_json::from_str(&json)?;

    assert_eq!(restored.name, "Test Wheel");
    assert_eq!(restored.axes.len(), 3);
    assert_eq!(restored.version, 1);
    assert_eq!(restored.axes[0].center, Some(32768));
    assert_eq!(restored.axes[1].min, 100);
    assert_eq!(restored.axes[1].max, 60000);
    Ok(())
}

#[test]
fn cal_09_persistence_preserves_deadzones() -> Result<(), Box<dyn std::error::Error>> {
    let axis = AxisCalibration::new(0, 65535).with_deadzone(2000, 63000);
    let json = serde_json::to_string(&axis)?;
    let restored: AxisCalibration = serde_json::from_str(&json)?;

    assert_eq!(restored.deadzone_min, 2000);
    assert_eq!(restored.deadzone_max, 63000);
    Ok(())
}

#[test]
fn cal_10_persistence_calibration_point() -> Result<(), Box<dyn std::error::Error>> {
    let point = CalibrationPoint::new(32768, 0.5);
    let json = serde_json::to_string(&point)?;
    let restored: CalibrationPoint = serde_json::from_str(&json)?;

    assert_eq!(restored.raw, 32768);
    assert_near(restored.normalized, 0.5, "normalized");
    Ok(())
}

// ---------------------------------------------------------------------------
// Calibration interpolation accuracy
// ---------------------------------------------------------------------------

#[test]
fn cal_11_interpolation_full_range_midpoint() {
    let axis = AxisCalibration::new(0, 65535);
    assert_near(axis.apply(32768), 0.5, "midpoint");
}

#[test]
fn cal_12_interpolation_quarter_points() {
    let axis = AxisCalibration::new(0, 65535);
    assert_near(axis.apply(16384), 0.25, "25%");
    assert_near(axis.apply(49151), 0.75, "75%");
}

#[test]
fn cal_13_interpolation_endpoints() {
    let axis = AxisCalibration::new(0, 65535);
    assert_near(axis.apply(0), 0.0, "min");
    assert_near(axis.apply(65535), 1.0, "max");
}

#[test]
fn cal_14_interpolation_with_deadzone() {
    let axis = AxisCalibration::new(0, 65535).with_deadzone(6553, 58982);

    // Below deadzone_min → 0.0
    assert_near(axis.apply(0), 0.0, "below dz");

    // Above deadzone_max → 1.0
    assert_near(axis.apply(65535), 1.0, "above dz");

    // Midpoint of the active range should be near 0.5
    let mid_raw = (6553 + 58982) / 2;
    let result = axis.apply(mid_raw);
    assert!(
        (result - 0.5).abs() < 0.05,
        "mid active range expected ~0.5, got {result}"
    );
}

#[test]
fn cal_15_interpolation_equal_min_max_returns_half() {
    let axis = AxisCalibration::new(500, 500);
    assert_near(axis.apply(500), 0.5, "equal min/max");
    assert_near(axis.apply(0), 0.5, "equal min/max low");
    assert_near(axis.apply(65535), 0.5, "equal min/max high");
}

// ---------------------------------------------------------------------------
// Noisy input data
// ---------------------------------------------------------------------------

#[test]
fn cal_16_noisy_input_still_finds_extremes() -> Result<(), Box<dyn std::error::Error>> {
    let mut cal = JoystickCalibrator::new(0);

    // Jittery samples around the true min/max
    let noisy_samples: Vec<(u16, f32)> = vec![
        (105, 0.01),
        (98, 0.0),
        (110, 0.02),
        (32500, 0.49),
        (32700, 0.5),
        (32600, 0.48),
        (64900, 0.98),
        (65100, 1.0),
        (65000, 0.99),
    ];

    for (raw, norm) in &noisy_samples {
        cal.add_sample(*raw, *norm);
    }

    let axis = cal.calibrate()?;
    assert_eq!(axis.min, 98, "min should be the lowest raw sample");
    assert_eq!(axis.max, 65100, "max should be the highest raw sample");
    Ok(())
}

#[test]
fn cal_17_noisy_pedal_samples() -> Result<(), Box<dyn std::error::Error>> {
    // Simulate noisy throttle pedal with jitter
    let throttle: Vec<u16> = (0..50).map(|i| (i * 1300 + (i % 7) * 50) as u16).collect();
    let brake = vec![0u16, 100, 200, 65000, 65200, 65535];
    let clutch = vec![1000u16, 2000, 3000, 60000, 61000];

    let axes = create_pedal_calibration(&throttle, &brake, &clutch)?;
    assert_eq!(axes.len(), 3);

    // Min/max should capture the full range of noisy data
    let t_min = *throttle.iter().min().ok_or("no min")?;
    let t_max = *throttle.iter().max().ok_or("no max")?;
    assert_eq!(axes[0].min, t_min);
    assert_eq!(axes[0].max, t_max);
    Ok(())
}

// ---------------------------------------------------------------------------
// Pedal calibration
// ---------------------------------------------------------------------------

#[test]
fn cal_18_pedal_calibration_full_range() -> Result<(), Box<dyn std::error::Error>> {
    let axes = create_pedal_calibration(&[0, 65535], &[0, 65535], &[0, 65535])?;
    assert_eq!(axes.len(), 3);
    for (i, axis) in axes.iter().enumerate() {
        assert_eq!(axis.min, 0, "axis {i} min");
        assert_eq!(axis.max, 65535, "axis {i} max");
    }
    Ok(())
}

#[test]
fn cal_19_pedal_calibration_non_linear_curve() -> Result<(), Box<dyn std::error::Error>> {
    // Throttle with non-linear sensor distribution: many samples near the bottom
    let throttle: Vec<u16> = vec![0, 100, 200, 400, 800, 1600, 5000, 20000, 50000, 65535];
    let brake = vec![0u16, 65535];
    let clutch = vec![0u16, 65535];

    let axes = create_pedal_calibration(&throttle, &brake, &clutch)?;
    assert_eq!(axes[0].min, 0);
    assert_eq!(axes[0].max, 65535);
    Ok(())
}

#[test]
fn cal_20_pedal_calibration_missing_axis_fails() {
    let result = create_pedal_calibration(&[0, 65535], &[], &[0, 65535]);
    assert!(result.is_err(), "missing brake samples should fail");
}

#[test]
fn cal_21_pedal_calibration_single_sample_per_axis() -> Result<(), Box<dyn std::error::Error>> {
    let axes = create_pedal_calibration(&[32768], &[32768], &[32768])?;
    assert_eq!(axes.len(), 3);
    // Single sample: min == max
    assert_eq!(axes[0].min, axes[0].max);
    Ok(())
}

#[test]
fn cal_22_pedal_calibrator_reset_and_recalibrate() -> Result<(), Box<dyn std::error::Error>> {
    let mut cal = PedalCalibrator::new();
    cal.add_throttle(0);
    cal.add_throttle(1000);
    cal.add_brake(0);
    cal.add_brake(1000);
    cal.add_clutch(0);
    cal.add_clutch(1000);

    let first = cal.calibrate()?;
    assert_eq!(first[0].max, 1000);

    cal.reset();
    assert!(cal.calibrate().is_err(), "after reset should fail");

    // Re-calibrate with different range
    cal.add_throttle(500);
    cal.add_throttle(60000);
    cal.add_brake(500);
    cal.add_brake(60000);
    cal.add_clutch(500);
    cal.add_clutch(60000);

    let second = cal.calibrate()?;
    assert_eq!(second[0].min, 500);
    assert_eq!(second[0].max, 60000);
    Ok(())
}

// ---------------------------------------------------------------------------
// Wheel center detection
// ---------------------------------------------------------------------------

#[test]
fn cal_23_wheel_center_detected_at_midpoint() -> Result<(), Box<dyn std::error::Error>> {
    let axis = calibrate_joystick_axis(&[(0, 0.0), (32768, 0.5), (65535, 1.0)])?;
    assert_eq!(axis.center, Some(32768));
    Ok(())
}

#[test]
fn cal_24_wheel_center_detected_near_half() -> Result<(), Box<dyn std::error::Error>> {
    // 0.45 is within ±0.1 of 0.5, so center should still be detected
    let axis = calibrate_joystick_axis(&[(0, 0.0), (30000, 0.45), (65535, 1.0)])?;
    assert_eq!(axis.center, Some(30000));
    Ok(())
}

#[test]
fn cal_25_wheel_no_center_when_far_from_half() -> Result<(), Box<dyn std::error::Error>> {
    // No sample within ±0.1 of 0.5
    let axis = calibrate_joystick_axis(&[(0, 0.0), (10000, 0.15), (65535, 1.0)])?;
    assert!(axis.center.is_none(), "no sample near 0.5 → no center");
    Ok(())
}

#[test]
fn cal_26_wheel_center_first_matching_sample() -> Result<(), Box<dyn std::error::Error>> {
    // Multiple samples near 0.5 — first match is used
    let axis = calibrate_joystick_axis(&[(0, 0.0), (30000, 0.46), (33000, 0.50), (65535, 1.0)])?;
    // First sample within ±0.1 of 0.5 is (30000, 0.46)
    assert_eq!(axis.center, Some(30000));
    Ok(())
}

// ---------------------------------------------------------------------------
// Joystick convenience function
// ---------------------------------------------------------------------------

#[test]
fn cal_27_joystick_convenience_function() -> Result<(), Box<dyn std::error::Error>> {
    let axis = calibrate_joystick_axis(&[(5000, 0.0), (60000, 1.0)])?;
    assert_eq!(axis.min, 5000);
    assert_eq!(axis.max, 60000);
    assert!(axis.center.is_none());
    Ok(())
}

// ---------------------------------------------------------------------------
// Device calibration multi-axis
// ---------------------------------------------------------------------------

#[test]
fn cal_28_device_calibration_multi_axis() {
    let mut device = DeviceCalibration::new("Multi-Axis Rig", 5);
    assert_eq!(device.axes.len(), 5);

    // Configure each axis differently
    if let Some(a) = device.axis(0) {
        *a = AxisCalibration::new(0, 65535).with_center(32768);
    }
    if let Some(a) = device.axis(1) {
        *a = AxisCalibration::new(100, 900);
    }
    if let Some(a) = device.axis(4) {
        *a = AxisCalibration::new(2000, 62000).with_deadzone(2500, 61500);
    }

    assert_eq!(device.axes[0].center, Some(32768));
    assert_eq!(device.axes[1].min, 100);
    assert_eq!(device.axes[4].deadzone_min, 2500);

    // Out of bounds
    assert!(device.axis(10).is_none());
}

#[test]
fn cal_29_device_calibration_default() {
    let device = DeviceCalibration::default();
    assert!(device.name.is_empty());
    assert!(device.axes.is_empty());
    assert_eq!(device.version, 1);
}

// ---------------------------------------------------------------------------
// Monotonicity property
// ---------------------------------------------------------------------------

#[test]
fn cal_30_monotonic_output_full_range() {
    let axis = AxisCalibration::new(0, 65535);
    let mut prev = axis.apply(0);
    for raw in (1000..=65535).step_by(1000) {
        let current = axis.apply(raw);
        assert!(
            current >= prev,
            "non-monotonic at raw={raw}: prev={prev}, current={current}"
        );
        prev = current;
    }
}

// ---------------------------------------------------------------------------
// Calibration with dense samples
// ---------------------------------------------------------------------------

#[test]
fn cal_31_dense_sampling_joystick() -> Result<(), Box<dyn std::error::Error>> {
    let mut cal = JoystickCalibrator::new(0);
    for i in 0..=100 {
        let raw = (i as u32 * 65535 / 100) as u16;
        let norm = i as f32 / 100.0;
        cal.add_sample(raw, norm);
    }

    let axis = cal.calibrate()?;
    assert_eq!(axis.min, 0);
    assert_eq!(axis.max, 65535);
    // With 101 samples, center should be found at the sample nearest 0.5
    assert!(axis.center.is_some());
    Ok(())
}

// ---------------------------------------------------------------------------
// Multi-device persistence
// ---------------------------------------------------------------------------

#[test]
fn cal_32_multiple_devices_json() -> Result<(), Box<dyn std::error::Error>> {
    let devices = vec![
        DeviceCalibration::new("Wheel A", 1),
        DeviceCalibration::new("Pedals B", 3),
    ];

    let json = serde_json::to_string(&devices)?;
    let restored: Vec<DeviceCalibration> = serde_json::from_str(&json)?;

    assert_eq!(restored.len(), 2);
    assert_eq!(restored[0].name, "Wheel A");
    assert_eq!(restored[1].name, "Pedals B");
    assert_eq!(restored[1].axes.len(), 3);
    Ok(())
}

// ---------------------------------------------------------------------------
// Pedal calibrator incremental sampling
// ---------------------------------------------------------------------------

#[test]
fn cal_33_pedal_incremental_sampling() -> Result<(), Box<dyn std::error::Error>> {
    let mut cal = PedalCalibrator::new();

    // Simulate gradual pedal press/release
    for i in 0..=20 {
        let raw = (i as u32 * 65535 / 20) as u16;
        cal.add_throttle(raw);
        cal.add_brake(raw);
        cal.add_clutch(raw);
    }

    let axes = cal.calibrate()?;
    assert_eq!(axes[0].min, 0);
    assert_eq!(axes[0].max, 65535);
    assert_eq!(axes[1].min, 0);
    assert_eq!(axes[2].max, 65535);
    Ok(())
}

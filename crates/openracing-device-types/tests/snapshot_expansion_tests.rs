//! Snapshot tests for device types — ensure Debug output is stable.

use openracing_device_types::{DeviceInputs, TelemetryData};

#[test]
fn snapshot_device_inputs_default() {
    insta::assert_debug_snapshot!("device_inputs_default", DeviceInputs::default());
}

#[test]
fn snapshot_device_inputs_with_steering() {
    let inputs = DeviceInputs::new().with_steering(32768);
    insta::assert_debug_snapshot!("device_inputs_with_steering", inputs);
}

#[test]
fn snapshot_device_inputs_with_pedals() {
    let inputs = DeviceInputs::new().with_pedals(65535, 32000, 0);
    insta::assert_debug_snapshot!("device_inputs_with_pedals", inputs);
}

#[test]
fn snapshot_device_inputs_full() {
    let inputs = DeviceInputs::new()
        .with_steering(32768)
        .with_pedals(50000, 40000, 10000)
        .with_buttons([1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])
        .with_handbrake(8000);
    insta::assert_debug_snapshot!("device_inputs_full", inputs);
}

#[test]
fn snapshot_telemetry_data_typical() {
    let telemetry = TelemetryData {
        wheel_angle_deg: 45.5,
        wheel_speed_rad_s: 2.1,
        temperature_c: 42,
        fault_flags: 0,
        hands_on: true,
    };
    insta::assert_debug_snapshot!("telemetry_data_typical", telemetry);
}

#[test]
fn snapshot_telemetry_data_faulted() {
    let telemetry = TelemetryData {
        wheel_angle_deg: 0.0,
        wheel_speed_rad_s: 0.0,
        temperature_c: 85,
        fault_flags: 0x03,
        hands_on: false,
    };
    insta::assert_debug_snapshot!("telemetry_data_faulted", telemetry);
}

//! Insta snapshot tests for the Forza telemetry adapter.
//!
//! Tests the public API surface exposed by the `racing-wheel-telemetry-forza`
//! crate: adapter metadata, normalized telemetry defaults, Sled packet parsing,
//! and telemetry frame construction.

use insta::assert_debug_snapshot;
use racing_wheel_telemetry_forza::{
    ForzaAdapter, NormalizedTelemetry, TelemetryAdapter, TelemetryFrame, TelemetryValue,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn snapshot_forza_adapter_metadata() -> TestResult {
    let adapter = ForzaAdapter::new();
    assert_debug_snapshot!(format!(
        "game_id={}, update_rate={:?}",
        adapter.game_id(),
        adapter.expected_update_rate()
    ));
    Ok(())
}

#[test]
fn snapshot_normalized_telemetry_default() {
    let t = NormalizedTelemetry::default();
    assert_debug_snapshot!(format!(
        "rpm={}, speed_ms={}, gear={}, throttle={}, brake={}, steering_angle={}",
        t.rpm, t.speed_ms, t.gear, t.throttle, t.brake, t.steering_angle
    ));
}

#[test]
fn snapshot_forza_sled_parse() -> TestResult {
    let adapter = ForzaAdapter::new();
    let mut data = vec![0u8; 232];
    // is_race_on = 1 (i32 LE at offset 0)
    data[0..4].copy_from_slice(&1i32.to_le_bytes());
    // engine_max_rpm = 8500.0 (f32 LE at offset 8)
    data[8..12].copy_from_slice(&8500.0f32.to_le_bytes());
    // engine_idle_rpm = 800.0 (f32 LE at offset 12)
    data[12..16].copy_from_slice(&800.0f32.to_le_bytes());
    // current_engine_rpm = 6200.0 (f32 LE at offset 16)
    data[16..20].copy_from_slice(&6200.0f32.to_le_bytes());
    // velocity_x = 30.0 m/s (f32 LE at offset 32)
    data[32..36].copy_from_slice(&30.0f32.to_le_bytes());

    let telemetry = adapter.normalize(&data)?;
    assert_debug_snapshot!(format!(
        "rpm={:.1}, speed_ms={:.1}",
        telemetry.rpm, telemetry.speed_ms
    ));
    Ok(())
}

#[test]
fn snapshot_telemetry_frame_construction() {
    let telemetry = NormalizedTelemetry::builder()
        .rpm(5000.0)
        .speed_ms(44.4)
        .gear(4)
        .throttle(0.85)
        .brake(0.0)
        .build();
    let frame = TelemetryFrame::new(telemetry, 999_999_000, 7, 232);
    assert_debug_snapshot!(format!(
        "rpm={:.1}, speed={:.1}, gear={}, seq={}, raw_size={}",
        frame.data.rpm, frame.data.speed_ms, frame.data.gear, frame.sequence, frame.raw_size
    ));
}

#[test]
fn snapshot_telemetry_value_variants() {
    let variants = [
        ("float", format!("{:?}", TelemetryValue::Float(1.5))),
        ("int", format!("{:?}", TelemetryValue::Integer(42))),
        ("bool", format!("{:?}", TelemetryValue::Boolean(true))),
        (
            "string",
            format!("{:?}", TelemetryValue::String("test".into())),
        ),
    ];
    assert_debug_snapshot!(format!("{variants:?}"));
}

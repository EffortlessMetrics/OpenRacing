//! Insta snapshot tests for the F1 telemetry adapter.
//!
//! Tests the public API surface exposed by the `racing-wheel-telemetry-f1`
//! crate: adapter metadata, normalized telemetry builder, and frame creation.

use insta::assert_debug_snapshot;
use racing_wheel_telemetry_f1::{
    F1NativeAdapter, NormalizedTelemetry, TelemetryAdapter, TelemetryFrame,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn snapshot_f1_adapter_metadata() -> TestResult {
    let adapter = F1NativeAdapter::new();
    assert_debug_snapshot!(format!(
        "game_id={}, update_rate={:?}",
        adapter.game_id(),
        adapter.expected_update_rate()
    ));
    Ok(())
}

#[test]
fn snapshot_normalized_telemetry_builder() {
    let t = NormalizedTelemetry::builder()
        .rpm(12500.0)
        .speed_ms(83.3)
        .gear(7)
        .throttle(1.0)
        .brake(0.0)
        .build();
    assert_debug_snapshot!(format!(
        "rpm={:.1}, speed_ms={:.1}, gear={}, throttle={:.2}, brake={:.2}, steering_angle={:.2}",
        t.rpm, t.speed_ms, t.gear, t.throttle, t.brake, t.steering_angle
    ));
}

#[test]
fn snapshot_telemetry_frame_from_builder() {
    let t = NormalizedTelemetry::builder()
        .rpm(3200.0)
        .speed_ms(22.0)
        .gear(2)
        .build();
    let frame = TelemetryFrame::new(t, 1_000_000, 1, 1347);
    assert_debug_snapshot!(format!(
        "rpm={:.1}, speed={:.1}, gear={}, ts_ns={}, seq={}, raw={}",
        frame.data.rpm,
        frame.data.speed_ms,
        frame.data.gear,
        frame.timestamp_ns,
        frame.sequence,
        frame.raw_size
    ));
}

#[test]
fn snapshot_normalized_telemetry_default() {
    let t = NormalizedTelemetry::default();
    assert_debug_snapshot!(format!(
        "rpm={}, speed_ms={}, gear={}, throttle={}, brake={}, steering_angle={}",
        t.rpm, t.speed_ms, t.gear, t.throttle, t.brake, t.steering_angle
    ));
}

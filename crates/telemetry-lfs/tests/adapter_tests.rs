//! Integration tests for the `racing-wheel-telemetry-lfs` crate.
//!
//! These tests verify OutGauge packet parsing via the public `LFSAdapter` API.

use racing_wheel_telemetry_lfs::{LFSAdapter, TelemetryAdapter};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Minimum valid OutGauge packet size.
const OUTGAUGE_PACKET_SIZE: usize = 96;

/// Field byte offsets in the OutGauge packet.
const OFF_GEAR: usize = 10;
const OFF_SPEED: usize = 12;
const OFF_RPM: usize = 16;
const OFF_FUEL: usize = 28;
const OFF_THROTTLE: usize = 48;
const OFF_BRAKE: usize = 52;
const OFF_CLUTCH: usize = 56;

fn make_outgauge_packet(
    speed: f32,
    rpm: f32,
    gear: u8,
    throttle: f32,
    brake: f32,
    clutch: f32,
    fuel: f32,
) -> Vec<u8> {
    let mut data = vec![0u8; OUTGAUGE_PACKET_SIZE];
    data[OFF_GEAR] = gear;
    data[OFF_SPEED..OFF_SPEED + 4].copy_from_slice(&speed.to_le_bytes());
    data[OFF_RPM..OFF_RPM + 4].copy_from_slice(&rpm.to_le_bytes());
    data[OFF_FUEL..OFF_FUEL + 4].copy_from_slice(&fuel.to_le_bytes());
    data[OFF_THROTTLE..OFF_THROTTLE + 4].copy_from_slice(&throttle.to_le_bytes());
    data[OFF_BRAKE..OFF_BRAKE + 4].copy_from_slice(&brake.to_le_bytes());
    data[OFF_CLUTCH..OFF_CLUTCH + 4].copy_from_slice(&clutch.to_le_bytes());
    data
}

#[test]
fn test_game_id() {
    let adapter = LFSAdapter::new();
    assert_eq!(adapter.game_id(), "live_for_speed");
}

#[test]
fn test_parse_speed_and_rpm() -> TestResult {
    let adapter = LFSAdapter::new();
    let data = make_outgauge_packet(30.0, 4500.0, 3, 0.7, 0.0, 0.0, 0.5);
    let result = adapter.normalize(&data)?;
    assert!((result.speed_ms - 30.0).abs() < 0.01, "speed_ms mismatch");
    assert!((result.rpm - 4500.0).abs() < 0.01, "rpm mismatch");
    Ok(())
}

#[test]
fn test_gear_encoding_reverse() -> TestResult {
    let adapter = LFSAdapter::new();
    let data = make_outgauge_packet(5.0, 2000.0, 0, 0.0, 0.5, 0.0, 0.8);
    let result = adapter.normalize(&data)?;
    assert_eq!(result.gear, -1, "gear 0 (reverse) should normalize to -1");
    Ok(())
}

#[test]
fn test_gear_encoding_neutral() -> TestResult {
    let adapter = LFSAdapter::new();
    let data = make_outgauge_packet(0.0, 800.0, 1, 0.0, 0.0, 0.0, 0.9);
    let result = adapter.normalize(&data)?;
    assert_eq!(result.gear, 0, "gear 1 (neutral) should normalize to 0");
    Ok(())
}

#[test]
fn test_gear_encoding_first() -> TestResult {
    let adapter = LFSAdapter::new();
    let data = make_outgauge_packet(10.0, 3000.0, 2, 0.5, 0.0, 0.0, 0.6);
    let result = adapter.normalize(&data)?;
    assert_eq!(result.gear, 1, "gear 2 (1st) should normalize to 1");
    Ok(())
}

#[test]
fn test_throttle_brake_clutch() -> TestResult {
    let adapter = LFSAdapter::new();
    let data = make_outgauge_packet(20.0, 3500.0, 3, 0.75, 0.25, 0.1, 0.6);
    let result = adapter.normalize(&data)?;
    assert!((result.throttle - 0.75).abs() < 0.001);
    assert!((result.brake - 0.25).abs() < 0.001);
    Ok(())
}

#[test]
fn test_truncated_packet_returns_error() {
    let adapter = LFSAdapter::new();
    assert!(
        adapter.normalize(&vec![0u8; 50]).is_err(),
        "truncated packet must return error"
    );
}

#[test]
fn test_empty_packet_returns_error() {
    let adapter = LFSAdapter::new();
    assert!(adapter.normalize(&[]).is_err(), "empty packet must return error");
}

#[test]
fn test_with_port_builder_is_chainable() {
    // with_port() must return Self and not panic; port is private so we just
    // verify the method compiles and the adapter remains usable.
    let adapter = LFSAdapter::new().with_port(4444);
    assert_eq!(adapter.game_id(), "live_for_speed");
}

#[test]
fn test_fuel_converted_to_percent() -> TestResult {
    let adapter = LFSAdapter::new();
    let data = make_outgauge_packet(0.0, 0.0, 1, 0.0, 0.0, 0.0, 0.75);
    let result = adapter.normalize(&data)?;
    assert!(
        (result.fuel_percent - 75.0).abs() < 0.01,
        "fuel 0.75 should be 75%"
    );
    Ok(())
}

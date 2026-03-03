//! Integration tests for the `racing-wheel-telemetry-rennsport` crate.
//!
//! Tests verify Rennsport UDP packet parsing via the public API.

use racing_wheel_telemetry_rennsport::{NormalizedTelemetry, RennsportAdapter, TelemetryAdapter};
use std::time::Duration;

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Rennsport packet layout constants (mirroring the adapter's protocol spec).
const RENNSPORT_MIN_PACKET_SIZE: usize = 24;
const RENNSPORT_IDENTIFIER: u8 = 0x52; // 'R'
const OFF_IDENTIFIER: usize = 0;
const OFF_SPEED_KMH: usize = 4;
const OFF_RPM: usize = 8;
const OFF_GEAR: usize = 12;
const OFF_FFB_SCALAR: usize = 16;
const OFF_SLIP_RATIO: usize = 20;

fn make_rennsport_packet(
    speed_kmh: f32,
    rpm: f32,
    gear: i8,
    ffb_scalar: f32,
    slip_ratio: f32,
) -> Vec<u8> {
    let mut data = vec![0u8; RENNSPORT_MIN_PACKET_SIZE];
    data[OFF_IDENTIFIER] = RENNSPORT_IDENTIFIER;
    data[OFF_SPEED_KMH..OFF_SPEED_KMH + 4].copy_from_slice(&speed_kmh.to_le_bytes());
    data[OFF_RPM..OFF_RPM + 4].copy_from_slice(&rpm.to_le_bytes());
    data[OFF_GEAR] = gear as u8;
    data[OFF_FFB_SCALAR..OFF_FFB_SCALAR + 4].copy_from_slice(&ffb_scalar.to_le_bytes());
    data[OFF_SLIP_RATIO..OFF_SLIP_RATIO + 4].copy_from_slice(&slip_ratio.to_le_bytes());
    data
}

#[test]
fn test_packet_size_matches_protocol() {
    // Rennsport packets must be at least 24 bytes to contain all documented fields.
    assert_eq!(RENNSPORT_MIN_PACKET_SIZE, 24);
    // Verify the last field (slip_ratio f32) ends exactly at byte 24.
    assert_eq!(OFF_SLIP_RATIO + 4, RENNSPORT_MIN_PACKET_SIZE);
}

#[test]
fn test_game_id() {
    let adapter = RennsportAdapter::new();
    assert_eq!(adapter.game_id(), "rennsport");
}

#[test]
fn test_update_rate_60hz() {
    let adapter = RennsportAdapter::new();
    assert_eq!(adapter.expected_update_rate(), Duration::from_millis(16));
}

#[test]
fn test_parse_valid_packet() -> TestResult {
    let adapter = RennsportAdapter::new();
    let data = make_rennsport_packet(180.0, 7500.0, 4, 0.6, 0.1);
    let result = adapter.normalize(&data)?;
    // 180 km/h → 50 m/s
    assert!(
        (result.speed_ms - 50.0).abs() < 0.01,
        "speed_ms={}",
        result.speed_ms
    );
    assert!((result.rpm - 7500.0).abs() < 0.1);
    assert_eq!(result.gear, 4);
    assert!((result.ffb_scalar - 0.6).abs() < 0.001);
    assert!((result.slip_ratio - 0.1).abs() < 0.001);
    Ok(())
}

#[test]
fn test_wrong_identifier_rejected() {
    let adapter = RennsportAdapter::new();
    let mut data = make_rennsport_packet(100.0, 5000.0, 3, 0.0, 0.0);
    data[OFF_IDENTIFIER] = 0x41; // 'A' instead of 'R'
    assert!(
        adapter.normalize(&data).is_err(),
        "wrong identifier must be rejected"
    );
}

#[test]
fn test_short_packet_rejected() {
    let adapter = RennsportAdapter::new();
    assert!(
        adapter.normalize(&[0u8; 8]).is_err(),
        "short packet must be rejected"
    );
}

#[test]
fn test_empty_packet_rejected() {
    let adapter = RennsportAdapter::new();
    assert!(
        adapter.normalize(&[]).is_err(),
        "empty packet must be rejected"
    );
}

#[test]
fn test_reverse_gear() -> TestResult {
    let adapter = RennsportAdapter::new();
    let data = make_rennsport_packet(0.0, 1000.0, -1, -0.1, 0.0);
    let result = adapter.normalize(&data)?;
    assert_eq!(result.gear, -1);
    Ok(())
}

#[test]
fn test_ffb_scalar_clamped_to_one() -> TestResult {
    let adapter = RennsportAdapter::new();
    let data = make_rennsport_packet(200.0, 8000.0, 5, 5.0, 0.0);
    let result = adapter.normalize(&data)?;
    assert!(
        result.ffb_scalar <= 1.0,
        "ffb_scalar not clamped: {}",
        result.ffb_scalar
    );
    Ok(())
}

#[test]
fn test_slip_ratio_clamped_to_one() -> TestResult {
    let adapter = RennsportAdapter::new();
    let data = make_rennsport_packet(50.0, 6000.0, 3, 0.3, 2.0);
    let result = adapter.normalize(&data)?;
    assert!(
        result.slip_ratio <= 1.0,
        "slip_ratio not clamped: {}",
        result.slip_ratio
    );
    Ok(())
}

#[test]
fn test_speed_nonnegative() -> TestResult {
    let adapter = RennsportAdapter::new();
    let data = make_rennsport_packet(144.0, 6500.0, 3, 0.4, 0.05);
    let result = adapter.normalize(&data)?;
    assert!(
        result.speed_ms >= 0.0,
        "speed_ms must be non-negative, got {}",
        result.speed_ms
    );
    Ok(())
}

#[test]
fn test_with_port_builder() {
    let adapter = RennsportAdapter::new().with_port(9999);
    assert_eq!(adapter.game_id(), "rennsport");
}

#[test]
fn test_default_trait() {
    let adapter = RennsportAdapter::default();
    assert_eq!(adapter.game_id(), "rennsport");
}

#[test]
fn test_normalized_telemetry_default_is_zero() {
    let t = NormalizedTelemetry::default();
    assert_eq!(t.speed_ms, 0.0);
    assert_eq!(t.rpm, 0.0);
    assert_eq!(t.gear, 0);
    assert_eq!(t.ffb_scalar, 0.0);
    assert_eq!(t.slip_ratio, 0.0);
}

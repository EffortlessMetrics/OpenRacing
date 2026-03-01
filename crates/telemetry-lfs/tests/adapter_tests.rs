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
        adapter.normalize(&[0u8; 50]).is_err(),
        "truncated packet must return error"
    );
}

#[test]
fn test_empty_packet_returns_error() {
    let adapter = LFSAdapter::new();
    assert!(
        adapter.normalize(&[]).is_err(),
        "empty packet must return error"
    );
}

#[test]
fn test_with_port_builder_is_chainable() {
    // with_port() must return Self and not panic; port is private so we just
    // verify the method compiles and the adapter remains usable.
    let adapter = LFSAdapter::new().with_port(4444);
    assert_eq!(adapter.game_id(), "live_for_speed");
}

#[test]
fn test_fuel_percent_in_valid_range() -> TestResult {
    let adapter = LFSAdapter::new();
    let data = make_outgauge_packet(0.0, 0.0, 1, 0.0, 0.0, 0.0, 0.75);
    let result = adapter.normalize(&data)?;
    assert!(
        result.fuel_percent >= 0.0 && result.fuel_percent <= 1.0,
        "fuel_percent {} must be in [0.0, 1.0]",
        result.fuel_percent
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Struct size / layout verification
// ---------------------------------------------------------------------------

/// The OutGauge packet size constant must be 96 bytes.
#[test]
fn test_outgauge_packet_size_is_96() {
    assert_eq!(OUTGAUGE_PACKET_SIZE, 96);
}

/// Packet exactly at the minimum size must parse without error.
#[test]
fn test_exact_minimum_packet_size_succeeds() -> TestResult {
    let adapter = LFSAdapter::new();
    let data = vec![0u8; OUTGAUGE_PACKET_SIZE];
    let _result = adapter.normalize(&data)?;
    Ok(())
}

/// One byte less than the minimum must return an error.
#[test]
fn test_one_byte_short_returns_error() {
    let adapter = LFSAdapter::new();
    let data = vec![0u8; OUTGAUGE_PACKET_SIZE - 1];
    assert!(
        adapter.normalize(&data).is_err(),
        "95 bytes must return error"
    );
}

// ---------------------------------------------------------------------------
// Higher gear encodings
// ---------------------------------------------------------------------------

/// Gears 3–8 should map to normalized 2–7.
#[test]
fn test_gear_encoding_higher_gears() -> TestResult {
    let adapter = LFSAdapter::new();
    for raw_gear in 3u8..=8u8 {
        let data = make_outgauge_packet(40.0, 5000.0, raw_gear, 0.5, 0.0, 0.0, 0.5);
        let result = adapter.normalize(&data)?;
        let expected = (raw_gear - 1) as i8;
        assert_eq!(
            result.gear, expected,
            "raw gear {raw_gear} should normalize to {expected}, got {}",
            result.gear
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Edge cases and boundary values
// ---------------------------------------------------------------------------

/// All-zero packet (idle car) should return sensible defaults.
#[test]
fn test_all_zeros_packet() -> TestResult {
    let adapter = LFSAdapter::new();
    let data = vec![0u8; OUTGAUGE_PACKET_SIZE];
    let result = adapter.normalize(&data)?;
    // gear 0 = reverse → -1
    assert_eq!(result.gear, -1);
    assert!(result.speed_ms.abs() < 0.001);
    assert!(result.rpm.abs() < 0.001);
    assert!(result.throttle.abs() < 0.001);
    assert!(result.brake.abs() < 0.001);
    Ok(())
}

/// An oversized packet (> 96 bytes) should still parse successfully using
/// the first 96 bytes.
#[test]
fn test_oversized_packet_still_parses() -> TestResult {
    let adapter = LFSAdapter::new();
    let mut data = make_outgauge_packet(55.0, 6000.0, 4, 0.8, 0.1, 0.0, 0.3);
    data.extend_from_slice(&[0xFF; 64]); // extra trailing bytes
    let result = adapter.normalize(&data)?;
    assert!((result.speed_ms - 55.0).abs() < 0.01);
    assert!((result.rpm - 6000.0).abs() < 0.01);
    Ok(())
}

/// Zero speed is a valid value.
#[test]
fn test_zero_speed_is_valid() -> TestResult {
    let adapter = LFSAdapter::new();
    let data = make_outgauge_packet(0.0, 800.0, 1, 0.0, 0.0, 0.0, 1.0);
    let result = adapter.normalize(&data)?;
    assert!(result.speed_ms.abs() < 0.001, "zero speed should be 0.0");
    Ok(())
}

/// Full throttle and full brake should both be 1.0.
#[test]
fn test_full_throttle_and_brake() -> TestResult {
    let adapter = LFSAdapter::new();
    let data = make_outgauge_packet(0.0, 3000.0, 1, 1.0, 1.0, 1.0, 0.5);
    let result = adapter.normalize(&data)?;
    assert!(
        (result.throttle - 1.0).abs() < 0.001,
        "full throttle should be 1.0"
    );
    assert!(
        (result.brake - 1.0).abs() < 0.001,
        "full brake should be 1.0"
    );
    Ok(())
}

/// Clutch value is parsed from the packet.
#[test]
fn test_clutch_value_parsed() -> TestResult {
    let adapter = LFSAdapter::new();
    let data = make_outgauge_packet(10.0, 2000.0, 2, 0.0, 0.0, 0.65, 0.5);
    let result = adapter.normalize(&data)?;
    assert!(
        (result.clutch - 0.65).abs() < 0.001,
        "clutch should be 0.65, got {}",
        result.clutch
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// NaN / Infinity handling
// ---------------------------------------------------------------------------

/// NaN in the speed field should default to 0.0 (read_f32_le filters non-finite).
#[test]
fn test_nan_speed_defaults_to_zero() -> TestResult {
    let adapter = LFSAdapter::new();
    let mut data = vec![0u8; OUTGAUGE_PACKET_SIZE];
    data[OFF_SPEED..OFF_SPEED + 4].copy_from_slice(&f32::NAN.to_le_bytes());
    let result = adapter.normalize(&data)?;
    assert!(
        result.speed_ms.is_finite(),
        "NaN speed should be replaced with finite default"
    );
    Ok(())
}

/// Infinity in the RPM field should default to 0.0 (read_f32_le filters non-finite).
#[test]
fn test_infinity_rpm_defaults_to_zero() -> TestResult {
    let adapter = LFSAdapter::new();
    let mut data = vec![0u8; OUTGAUGE_PACKET_SIZE];
    data[OFF_RPM..OFF_RPM + 4].copy_from_slice(&f32::INFINITY.to_le_bytes());
    let result = adapter.normalize(&data)?;
    assert!(
        result.rpm.is_finite(),
        "Infinity RPM should be replaced with finite default"
    );
    Ok(())
}

/// Negative infinity in throttle should default to 0.0.
#[test]
fn test_neg_infinity_throttle_defaults_to_zero() -> TestResult {
    let adapter = LFSAdapter::new();
    let mut data = vec![0u8; OUTGAUGE_PACKET_SIZE];
    data[OFF_THROTTLE..OFF_THROTTLE + 4].copy_from_slice(&f32::NEG_INFINITY.to_le_bytes());
    let result = adapter.normalize(&data)?;
    assert!(
        result.throttle.is_finite(),
        "NEG_INFINITY throttle should be replaced with finite default"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Adapter metadata
// ---------------------------------------------------------------------------

/// Default update rate is 16ms (~60Hz).
#[test]
fn test_default_update_rate_is_60hz() {
    let adapter = LFSAdapter::new();
    assert_eq!(
        adapter.expected_update_rate(),
        std::time::Duration::from_millis(16)
    );
}

/// LFSAdapter implements Default.
#[test]
fn test_adapter_default_impl() {
    let a = LFSAdapter::new();
    let b = LFSAdapter::default();
    assert_eq!(a.game_id(), b.game_id());
    assert_eq!(a.expected_update_rate(), b.expected_update_rate());
}

/// Normalizing junk bytes at full packet size must not panic.
#[test]
fn test_junk_full_size_packet_no_panic() {
    let adapter = LFSAdapter::new();
    let data = vec![0xFF; OUTGAUGE_PACKET_SIZE];
    let _ = adapter.normalize(&data);
}

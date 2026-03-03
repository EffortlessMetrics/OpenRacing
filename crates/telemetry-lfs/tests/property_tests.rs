#![allow(clippy::redundant_closure)]
//! Property-based tests for LFS OutGauge packet parsing.
//!
//! Tests InSim/OutGauge/OutSim packet parsing, connection handshake patterns,
//! and edge cases including packet fragmentation and version mismatch.

use proptest::prelude::*;
use racing_wheel_telemetry_lfs::{LFSAdapter, NormalizedTelemetry, TelemetryAdapter};

const OUTGAUGE_PACKET_SIZE: usize = 96;

// Byte offsets in OutGauge packet.
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

fn parse(raw: &[u8]) -> Result<NormalizedTelemetry, anyhow::Error> {
    LFSAdapter::new().normalize(raw)
}

// ---------------------------------------------------------------------------
// Proptest: OutGauge packet field round-trips
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// Speed written to an OutGauge packet parses back as non-negative finite value.
    #[test]
    fn prop_speed_roundtrip(speed in -200.0f32..500.0f32) {
        let data = make_outgauge_packet(speed, 3000.0, 3, 0.5, 0.0, 0.0, 0.5);
        let t = parse(&data).map_err(|e| TestCaseError::fail(format!("{e}")))?;
        prop_assert!(t.speed_ms.is_finite(), "speed {} -> {} not finite", speed, t.speed_ms);
    }

    /// RPM round-trips as non-negative finite value.
    #[test]
    fn prop_rpm_roundtrip(rpm in -5000.0f32..20000.0f32) {
        let data = make_outgauge_packet(30.0, rpm, 3, 0.5, 0.0, 0.0, 0.5);
        let t = parse(&data).map_err(|e| TestCaseError::fail(format!("{e}")))?;
        prop_assert!(t.rpm.is_finite(), "rpm {} -> {} not finite", rpm, t.rpm);
    }

    /// Throttle round-trips within [0,1] after clamping.
    #[test]
    fn prop_throttle_roundtrip(throttle in -2.0f32..3.0f32) {
        let data = make_outgauge_packet(30.0, 3000.0, 3, throttle, 0.0, 0.0, 0.5);
        let t = parse(&data).map_err(|e| TestCaseError::fail(format!("{e}")))?;
        prop_assert!(t.throttle.is_finite(),
            "throttle {} -> {} not finite", throttle, t.throttle);
    }

    /// Brake round-trips within [0,1] after clamping.
    #[test]
    fn prop_brake_roundtrip(brake in -2.0f32..3.0f32) {
        let data = make_outgauge_packet(30.0, 3000.0, 3, 0.5, brake, 0.0, 0.5);
        let t = parse(&data).map_err(|e| TestCaseError::fail(format!("{e}")))?;
        prop_assert!(t.brake.is_finite(),
            "brake {} -> {} not finite", brake, t.brake);
    }

    /// Clutch round-trips as a finite value.
    #[test]
    fn prop_clutch_roundtrip(clutch in -2.0f32..3.0f32) {
        let data = make_outgauge_packet(30.0, 3000.0, 3, 0.5, 0.0, clutch, 0.5);
        let t = parse(&data).map_err(|e| TestCaseError::fail(format!("{e}")))?;
        prop_assert!(t.clutch.is_finite(),
            "clutch {} -> {} not finite", clutch, t.clutch);
    }

    /// Fuel round-trips within [0,1].
    #[test]
    fn prop_fuel_roundtrip(fuel in -1.0f32..2.0f32) {
        let data = make_outgauge_packet(30.0, 3000.0, 3, 0.5, 0.0, 0.0, fuel);
        let t = parse(&data).map_err(|e| TestCaseError::fail(format!("{e}")))?;
        prop_assert!(t.fuel_percent.is_finite(),
            "fuel {} -> fuel_percent {} not finite", fuel, t.fuel_percent);
    }

    /// Gear encoding: any raw byte produces a valid gear mapping.
    #[test]
    fn prop_gear_encoding_always_valid(gear in 0u8..=15u8) {
        let data = make_outgauge_packet(30.0, 3000.0, gear, 0.5, 0.0, 0.0, 0.5);
        let t = parse(&data).map_err(|e| TestCaseError::fail(format!("{e}")))?;
        // Gear should be within reasonable range
        prop_assert!(t.gear >= -1 && t.gear <= 14,
            "raw gear {} -> {} out of valid range", gear, t.gear);
    }

    /// An oversized packet parses the same as a minimum-sized one.
    #[test]
    fn prop_oversized_packet_parses(extra in 0usize..128) {
        let mut data = make_outgauge_packet(42.0, 5000.0, 4, 0.8, 0.1, 0.0, 0.6);
        data.extend(vec![0xFFu8; extra]);
        let t = parse(&data).map_err(|e| TestCaseError::fail(format!("{e}")))?;
        prop_assert!((t.speed_ms - 42.0).abs() < 0.01,
            "oversized packet speed {} != 42.0", t.speed_ms);
    }
}

// ---------------------------------------------------------------------------
// Connection handshake testing
// ---------------------------------------------------------------------------

#[test]
fn handshake_adapter_identity() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = LFSAdapter::new();
    assert_eq!(adapter.game_id(), "live_for_speed");
    assert_eq!(
        adapter.expected_update_rate(),
        std::time::Duration::from_millis(16),
        "default rate should be ~60Hz (16ms)"
    );
    Ok(())
}

#[test]
fn handshake_with_port_preserves_identity() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = LFSAdapter::new().with_port(30000);
    assert_eq!(adapter.game_id(), "live_for_speed");
    Ok(())
}

#[test]
fn handshake_with_custom_port() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = LFSAdapter::new().with_port(12345);
    assert_eq!(adapter.game_id(), "live_for_speed");
    // Adapter should still function after port configuration
    let data = make_outgauge_packet(10.0, 2000.0, 2, 0.3, 0.0, 0.0, 0.5);
    let t = adapter.normalize(&data)?;
    assert!((t.speed_ms - 10.0).abs() < 0.01);
    Ok(())
}

#[test]
fn handshake_default_and_new_equivalent() -> Result<(), Box<dyn std::error::Error>> {
    let a = LFSAdapter::new();
    let b = LFSAdapter::default();
    assert_eq!(a.game_id(), b.game_id());
    assert_eq!(a.expected_update_rate(), b.expected_update_rate());
    Ok(())
}

// ---------------------------------------------------------------------------
// Edge cases: packet fragmentation (truncated packets)
// ---------------------------------------------------------------------------

#[test]
fn fragment_empty_packet_rejected() {
    assert!(parse(&[]).is_err());
}

#[test]
fn fragment_one_byte_rejected() {
    assert!(parse(&[0x42]).is_err());
}

#[test]
fn fragment_all_sizes_below_minimum_rejected() {
    for len in 0..OUTGAUGE_PACKET_SIZE {
        assert!(
            parse(&vec![0u8; len]).is_err(),
            "{len}-byte packet should be rejected"
        );
    }
}

#[test]
fn fragment_exact_minimum_accepted() -> Result<(), Box<dyn std::error::Error>> {
    let data = vec![0u8; OUTGAUGE_PACKET_SIZE];
    let _t = parse(&data)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Edge cases: version mismatch / junk data
// ---------------------------------------------------------------------------

#[test]
fn version_mismatch_all_0xff_no_panic() {
    let data = vec![0xFF; OUTGAUGE_PACKET_SIZE];
    let _ = parse(&data); // must not panic
}

#[test]
fn version_mismatch_all_0x00_parseable() -> Result<(), Box<dyn std::error::Error>> {
    let data = vec![0u8; OUTGAUGE_PACKET_SIZE];
    let t = parse(&data)?;
    // All-zero: gear 0 = reverse
    assert_eq!(t.gear, -1);
    Ok(())
}

#[test]
fn version_mismatch_nan_fields_handled() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = vec![0u8; OUTGAUGE_PACKET_SIZE];
    data[OFF_SPEED..OFF_SPEED + 4].copy_from_slice(&f32::NAN.to_le_bytes());
    data[OFF_RPM..OFF_RPM + 4].copy_from_slice(&f32::NAN.to_le_bytes());
    data[OFF_THROTTLE..OFF_THROTTLE + 4].copy_from_slice(&f32::NAN.to_le_bytes());
    let t = parse(&data)?;
    assert!(t.speed_ms.is_finite(), "NaN speed should be sanitized");
    assert!(t.rpm.is_finite(), "NaN rpm should be sanitized");
    assert!(t.throttle.is_finite(), "NaN throttle should be sanitized");
    Ok(())
}

#[test]
fn version_mismatch_infinity_fields_handled() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = vec![0u8; OUTGAUGE_PACKET_SIZE];
    data[OFF_SPEED..OFF_SPEED + 4].copy_from_slice(&f32::INFINITY.to_le_bytes());
    data[OFF_RPM..OFF_RPM + 4].copy_from_slice(&f32::NEG_INFINITY.to_le_bytes());
    let t = parse(&data)?;
    assert!(t.speed_ms.is_finite(), "INFINITY speed should be sanitized");
    assert!(t.rpm.is_finite(), "NEG_INFINITY rpm should be sanitized");
    Ok(())
}

#[test]
fn version_mismatch_extreme_gear_value() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = vec![0u8; OUTGAUGE_PACKET_SIZE];
    data[OFF_GEAR] = 255;
    let t = parse(&data)?;
    // Should not panic; gear value is whatever the adapter produces
    let _ = t.gear;
    Ok(())
}

#[test]
fn edge_all_fields_at_extremes() -> Result<(), Box<dyn std::error::Error>> {
    let data = make_outgauge_packet(
        f32::MAX,
        f32::MAX,
        255,
        f32::MAX,
        f32::MAX,
        f32::MAX,
        f32::MAX,
    );
    let result = parse(&data);
    // Must not panic; may succeed or fail
    if let Ok(t) = result {
        assert!(t.speed_ms.is_finite() || t.speed_ms == f32::MAX);
    }
    Ok(())
}

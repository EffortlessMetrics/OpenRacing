//! Property-based tests for the Rennsport telemetry adapter.
//!
//! Exercises binary packet parsing, normalization invariants, session state
//! edge cases, and ensures the adapter never panics on arbitrary input.

#![allow(clippy::redundant_closure)]

use proptest::prelude::*;
use racing_wheel_telemetry_rennsport::{RennsportAdapter, TelemetryAdapter};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ---------------------------------------------------------------------------
// Constants (must match adapter protocol spec)
// ---------------------------------------------------------------------------

const RENNSPORT_MIN_PACKET_SIZE: usize = 24;
const RENNSPORT_IDENTIFIER: u8 = 0x52; // 'R'
const OFF_IDENTIFIER: usize = 0;
const OFF_SPEED_KMH: usize = 4;
const OFF_RPM: usize = 8;
const OFF_GEAR: usize = 12;
const OFF_FFB_SCALAR: usize = 16;
const OFF_SLIP_RATIO: usize = 20;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Fuzz: arbitrary bytes must never panic
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Arbitrary random bytes of any length must never cause a panic.
    #[test]
    fn prop_random_bytes_no_panic(
        data in proptest::collection::vec(any::<u8>(), 0..8192)
    ) {
        let adapter = RennsportAdapter::new();
        let _ = adapter.normalize(&data);
    }

    /// A packet of exactly the minimum valid size with random content must not panic.
    #[test]
    fn prop_valid_size_random_content(
        data in proptest::collection::vec(any::<u8>(), RENNSPORT_MIN_PACKET_SIZE..=RENNSPORT_MIN_PACKET_SIZE)
    ) {
        let adapter = RennsportAdapter::new();
        let _ = adapter.normalize(&data);
    }
}

// ---------------------------------------------------------------------------
// Round-trip: generated packets → normalize → verify invariants
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// Valid packets with finite values must produce finite, correctly bounded output.
    #[test]
    fn prop_valid_packet_round_trip(
        speed_kmh in 0.0f32..500.0,
        rpm in 0.0f32..20000.0,
        gear in -1i8..=6,
        ffb_scalar in 0.0f32..1.0,
        slip_ratio in 0.0f32..1.0,
    ) {
        let data = make_rennsport_packet(speed_kmh, rpm, gear, ffb_scalar, slip_ratio);
        let adapter = RennsportAdapter::new();
        let result = adapter.normalize(&data);
        prop_assert!(result.is_ok(), "valid packet must parse: {:?}", result);
        let t = result.map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;

        // Speed: km/h → m/s (divide by 3.6)
        let expected_speed = speed_kmh / 3.6;
        prop_assert!((t.speed_ms - expected_speed).abs() < 0.1,
            "speed_ms mismatch: {} vs expected {}", t.speed_ms, expected_speed);

        // RPM preserved
        prop_assert!((t.rpm - rpm).abs() < 0.1, "rpm mismatch: {} vs {}", t.rpm, rpm);

        // Gear preserved
        prop_assert_eq!(t.gear, gear, "gear mismatch");

        // FFB and slip clamped to -1..1
        prop_assert!(t.ffb_scalar >= -1.0 && t.ffb_scalar <= 1.0,
            "ffb_scalar out of range: {}", t.ffb_scalar);
        prop_assert!(t.slip_ratio >= -1.0 && t.slip_ratio <= 1.0,
            "slip_ratio out of range: {}", t.slip_ratio);

        // All fields finite
        prop_assert!(t.speed_ms.is_finite());
        prop_assert!(t.rpm.is_finite());
        prop_assert!(t.ffb_scalar.is_finite());
        prop_assert!(t.slip_ratio.is_finite());
    }

    /// Over-range FFB and slip values must be clamped.
    #[test]
    fn prop_clamping_invariants(
        ffb_scalar in -10.0f32..10.0,
        slip_ratio in -10.0f32..10.0,
    ) {
        let data = make_rennsport_packet(100.0, 5000.0, 3, ffb_scalar, slip_ratio);
        let adapter = RennsportAdapter::new();
        let result = adapter.normalize(&data);
        prop_assert!(result.is_ok());
        let t = result.map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;

        prop_assert!(t.ffb_scalar >= -1.0 && t.ffb_scalar <= 1.0,
            "ffb_scalar not clamped: {}", t.ffb_scalar);
        prop_assert!(t.slip_ratio >= -1.0 && t.slip_ratio <= 1.0,
            "slip_ratio not clamped: {}", t.slip_ratio);
    }
}

// ---------------------------------------------------------------------------
// Session state / disconnect edge cases
// ---------------------------------------------------------------------------

#[test]
fn test_mid_session_disconnect_short_packet() {
    let adapter = RennsportAdapter::new();
    // Simulate a truncated packet (connection lost mid-transmission)
    let full = make_rennsport_packet(180.0, 7500.0, 4, 0.6, 0.1);
    for len in 0..RENNSPORT_MIN_PACKET_SIZE {
        assert!(
            adapter.normalize(&full[..len]).is_err(),
            "truncated packet of len {len} must be rejected"
        );
    }
}

#[test]
fn test_incomplete_packet_with_correct_identifier() {
    let adapter = RennsportAdapter::new();
    // Just the identifier byte and a few more
    let mut data = vec![0u8; 10];
    data[OFF_IDENTIFIER] = RENNSPORT_IDENTIFIER;
    assert!(
        adapter.normalize(&data).is_err(),
        "packet shorter than 24 bytes must be rejected"
    );
}

#[test]
fn test_oversized_packet_accepted() -> TestResult {
    let adapter = RennsportAdapter::new();
    let mut data = make_rennsport_packet(100.0, 5000.0, 3, 0.5, 0.05);
    data.extend_from_slice(&[0u8; 512]); // extra trailing bytes
    let t = adapter.normalize(&data)?;
    assert!((t.speed_ms - (100.0 / 3.6)).abs() < 0.1);
    Ok(())
}

#[test]
fn test_nan_speed_handling() -> TestResult {
    let adapter = RennsportAdapter::new();
    let data = make_rennsport_packet(f32::NAN, 5000.0, 3, 0.5, 0.05);
    // Should not panic; may produce 0 or error
    let _ = adapter.normalize(&data);
    Ok(())
}

#[test]
fn test_infinity_rpm_handling() -> TestResult {
    let adapter = RennsportAdapter::new();
    let data = make_rennsport_packet(100.0, f32::INFINITY, 3, 0.5, 0.05);
    let _ = adapter.normalize(&data);
    Ok(())
}

#[test]
fn test_neg_infinity_ffb_handling() -> TestResult {
    let adapter = RennsportAdapter::new();
    let data = make_rennsport_packet(100.0, 5000.0, 3, f32::NEG_INFINITY, 0.05);
    let _ = adapter.normalize(&data);
    Ok(())
}

#[test]
fn test_all_zeros_except_identifier() -> TestResult {
    let adapter = RennsportAdapter::new();
    let mut data = vec![0u8; RENNSPORT_MIN_PACKET_SIZE];
    data[OFF_IDENTIFIER] = RENNSPORT_IDENTIFIER;
    let t = adapter.normalize(&data)?;
    assert!(t.speed_ms.abs() < 0.001);
    assert!(t.rpm.abs() < 0.001);
    assert_eq!(t.gear, 0);
    assert!(t.ffb_scalar.abs() < 0.001);
    assert!(t.slip_ratio.abs() < 0.001);
    Ok(())
}

#[test]
fn test_max_gear_values() -> TestResult {
    let adapter = RennsportAdapter::new();
    // High positive gear
    let data = make_rennsport_packet(100.0, 5000.0, 127, 0.5, 0.05);
    let t = adapter.normalize(&data)?;
    assert_eq!(t.gear, 127);

    // Max negative gear
    let data = make_rennsport_packet(100.0, 5000.0, -128, 0.5, 0.05);
    let t = adapter.normalize(&data)?;
    assert_eq!(t.gear, -128);
    Ok(())
}

#[test]
fn test_normalize_is_deterministic() -> TestResult {
    let adapter = RennsportAdapter::new();
    let data = make_rennsport_packet(144.0, 6500.0, 3, 0.4, 0.05);
    let a = adapter.normalize(&data)?;
    let b = adapter.normalize(&data)?;
    assert!((a.speed_ms - b.speed_ms).abs() < f32::EPSILON);
    assert!((a.rpm - b.rpm).abs() < f32::EPSILON);
    assert_eq!(a.gear, b.gear);
    assert!((a.ffb_scalar - b.ffb_scalar).abs() < f32::EPSILON);
    assert!((a.slip_ratio - b.slip_ratio).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn test_negative_speed_kmh() -> TestResult {
    let adapter = RennsportAdapter::new();
    let data = make_rennsport_packet(-50.0, 3000.0, -1, 0.1, 0.0);
    // Should not panic
    let _ = adapter.normalize(&data);
    Ok(())
}

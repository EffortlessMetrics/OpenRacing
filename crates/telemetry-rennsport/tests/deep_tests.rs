//! Deep individual tests for the Rennsport telemetry adapter.
//!
//! Covers binary packet parsing, speed conversion (km/h → m/s),
//! identifier validation, clamping, gear handling, and edge cases.

use racing_wheel_telemetry_rennsport::{RennsportAdapter, TelemetryAdapter};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// Packet layout constants (mirror the adapter's protocol spec).
const MIN_PACKET_SIZE: usize = 24;
const IDENTIFIER: u8 = 0x52; // 'R'
const OFF_IDENTIFIER: usize = 0;
const OFF_SPEED_KMH: usize = 4;
const OFF_RPM: usize = 8;
const OFF_GEAR: usize = 12;
const OFF_FFB_SCALAR: usize = 16;
const OFF_SLIP_RATIO: usize = 20;

fn make_packet(speed_kmh: f32, rpm: f32, gear: i8, ffb: f32, slip: f32) -> Vec<u8> {
    let mut data = vec![0u8; MIN_PACKET_SIZE];
    data[OFF_IDENTIFIER] = IDENTIFIER;
    data[OFF_SPEED_KMH..OFF_SPEED_KMH + 4].copy_from_slice(&speed_kmh.to_le_bytes());
    data[OFF_RPM..OFF_RPM + 4].copy_from_slice(&rpm.to_le_bytes());
    data[OFF_GEAR] = gear as u8;
    data[OFF_FFB_SCALAR..OFF_FFB_SCALAR + 4].copy_from_slice(&ffb.to_le_bytes());
    data[OFF_SLIP_RATIO..OFF_SLIP_RATIO + 4].copy_from_slice(&slip.to_le_bytes());
    data
}

// ── Packet parsing: valid ────────────────────────────────────────────────────

#[test]
fn deep_valid_high_speed_packet() -> TestResult {
    let adapter = RennsportAdapter::new();
    let data = make_packet(288.0, 9500.0, 6, 0.8, 0.15);
    let t = adapter.normalize(&data)?;
    // 288 km/h = 80.0 m/s
    assert!((t.speed_ms - 80.0).abs() < 0.01, "speed_ms={}", t.speed_ms);
    assert!((t.rpm - 9500.0).abs() < 0.1);
    assert_eq!(t.gear, 6);
    assert!((t.ffb_scalar - 0.8).abs() < 0.001);
    assert!((t.slip_ratio - 0.15).abs() < 0.001);
    Ok(())
}

#[test]
fn deep_stationary_idle_packet() -> TestResult {
    let adapter = RennsportAdapter::new();
    let data = make_packet(0.0, 850.0, 0, 0.0, 0.0);
    let t = adapter.normalize(&data)?;
    assert_eq!(t.speed_ms, 0.0);
    assert!((t.rpm - 850.0).abs() < 0.1);
    assert_eq!(t.gear, 0);
    assert_eq!(t.ffb_scalar, 0.0);
    assert_eq!(t.slip_ratio, 0.0);
    Ok(())
}

#[test]
fn deep_reverse_gear_with_negative_ffb() -> TestResult {
    let adapter = RennsportAdapter::new();
    let data = make_packet(10.0, 2000.0, -1, -0.6, 0.0);
    let t = adapter.normalize(&data)?;
    assert_eq!(t.gear, -1);
    assert!((t.ffb_scalar - (-0.6)).abs() < 0.001);
    Ok(())
}

// ── Packet parsing: malformed ────────────────────────────────────────────────

#[test]
fn deep_empty_packet_rejected() -> TestResult {
    let adapter = RennsportAdapter::new();
    assert!(adapter.normalize(&[]).is_err());
    Ok(())
}

#[test]
fn deep_23_byte_packet_rejected() -> TestResult {
    let adapter = RennsportAdapter::new();
    let data = vec![0u8; 23];
    assert!(adapter.normalize(&data).is_err(), "23 bytes < 24 minimum");
    Ok(())
}

#[test]
fn deep_wrong_identifier_rejected() -> TestResult {
    let adapter = RennsportAdapter::new();
    let mut data = make_packet(100.0, 5000.0, 3, 0.0, 0.0);
    data[OFF_IDENTIFIER] = 0x00;
    assert!(adapter.normalize(&data).is_err());
    Ok(())
}

#[test]
fn deep_all_wrong_identifiers() -> TestResult {
    let adapter = RennsportAdapter::new();
    for id in [0x00u8, 0x41, 0x53, 0xFF] {
        let mut data = make_packet(0.0, 0.0, 0, 0.0, 0.0);
        data[OFF_IDENTIFIER] = id;
        assert!(
            adapter.normalize(&data).is_err(),
            "identifier 0x{id:02X} should be rejected"
        );
    }
    Ok(())
}

// ── Normalization: speed conversion ──────────────────────────────────────────

#[test]
fn deep_speed_conversion_accuracy() -> TestResult {
    let adapter = RennsportAdapter::new();
    let test_cases: Vec<(f32, f32)> = vec![
        (0.0, 0.0),
        (3.6, 1.0),
        (36.0, 10.0),
        (100.0, 100.0 / 3.6),
        (180.0, 50.0),
        (360.0, 100.0),
    ];
    for (kmh, expected_ms) in test_cases {
        let data = make_packet(kmh, 0.0, 0, 0.0, 0.0);
        let t = adapter.normalize(&data)?;
        assert!(
            (t.speed_ms - expected_ms).abs() < 0.02,
            "kmh={kmh} → expected {expected_ms}, got {}",
            t.speed_ms
        );
    }
    Ok(())
}

// ── Normalization: clamping ──────────────────────────────────────────────────

#[test]
fn deep_ffb_scalar_positive_overflow_clamped() -> TestResult {
    let adapter = RennsportAdapter::new();
    let data = make_packet(0.0, 0.0, 0, 10.0, 0.0);
    let t = adapter.normalize(&data)?;
    assert!(t.ffb_scalar <= 1.0, "ffb={}", t.ffb_scalar);
    Ok(())
}

#[test]
fn deep_ffb_scalar_negative_overflow_clamped() -> TestResult {
    let adapter = RennsportAdapter::new();
    let data = make_packet(0.0, 0.0, 0, -5.0, 0.0);
    let t = adapter.normalize(&data)?;
    assert!(t.ffb_scalar >= -1.0, "ffb={}", t.ffb_scalar);
    Ok(())
}

#[test]
fn deep_slip_ratio_overflow_clamped() -> TestResult {
    let adapter = RennsportAdapter::new();
    let data = make_packet(0.0, 0.0, 0, 0.0, 3.5);
    let t = adapter.normalize(&data)?;
    assert!(t.slip_ratio <= 1.0, "slip={}", t.slip_ratio);
    assert!(t.slip_ratio >= 0.0, "slip={}", t.slip_ratio);
    Ok(())
}

#[test]
fn deep_speed_nonnegative_even_for_negative_input() -> TestResult {
    let adapter = RennsportAdapter::new();
    // Negative km/h → speed clamps to 0 (after /3.6 the builder guards >=0).
    let data = make_packet(-50.0, 0.0, 0, 0.0, 0.0);
    let t = adapter.normalize(&data)?;
    assert!(t.speed_ms >= 0.0, "speed={}", t.speed_ms);
    Ok(())
}

#[test]
fn deep_rpm_nonnegative() -> TestResult {
    let adapter = RennsportAdapter::new();
    let data = make_packet(0.0, -100.0, 0, 0.0, 0.0);
    let t = adapter.normalize(&data)?;
    assert!(t.rpm >= 0.0, "rpm={}", t.rpm);
    Ok(())
}

// ── Oversized packet ─────────────────────────────────────────────────────────

#[test]
fn deep_oversized_packet_accepted() -> TestResult {
    let adapter = RennsportAdapter::new();
    let mut data = make_packet(72.0, 3000.0, 2, 0.3, 0.05);
    data.extend_from_slice(&[0xABu8; 128]); // extra trailing bytes
    let t = adapter.normalize(&data)?;
    assert!((t.speed_ms - 20.0).abs() < 0.1, "speed_ms={}", t.speed_ms);
    assert_eq!(t.gear, 2);
    Ok(())
}

// ── NaN / Infinity handling ──────────────────────────────────────────────────

#[test]
fn deep_nan_speed_defaults_to_zero() -> TestResult {
    let adapter = RennsportAdapter::new();
    let mut data = make_packet(0.0, 0.0, 0, 0.0, 0.0);
    // Write NaN into the speed field
    data[OFF_SPEED_KMH..OFF_SPEED_KMH + 4].copy_from_slice(&f32::NAN.to_le_bytes());
    let t = adapter.normalize(&data)?;
    assert!(t.speed_ms.is_finite(), "NaN speed → finite default");
    assert!(t.speed_ms >= 0.0);
    Ok(())
}

#[test]
fn deep_infinity_rpm_defaults_to_zero() -> TestResult {
    let adapter = RennsportAdapter::new();
    let mut data = make_packet(0.0, 0.0, 0, 0.0, 0.0);
    data[OFF_RPM..OFF_RPM + 4].copy_from_slice(&f32::INFINITY.to_le_bytes());
    let t = adapter.normalize(&data)?;
    assert!(t.rpm.is_finite(), "Inf rpm → finite default");
    Ok(())
}

// ── Determinism ──────────────────────────────────────────────────────────────

#[test]
fn deep_deterministic_output() -> TestResult {
    let adapter = RennsportAdapter::new();
    let data = make_packet(200.0, 7000.0, 4, 0.5, 0.2);
    let t1 = adapter.normalize(&data)?;
    let t2 = adapter.normalize(&data)?;
    assert_eq!(t1.speed_ms, t2.speed_ms, "deterministic speed");
    assert_eq!(t1.rpm, t2.rpm, "deterministic rpm");
    assert_eq!(t1.gear, t2.gear, "deterministic gear");
    assert_eq!(t1.ffb_scalar, t2.ffb_scalar, "deterministic ffb");
    assert_eq!(t1.slip_ratio, t2.slip_ratio, "deterministic slip");
    Ok(())
}

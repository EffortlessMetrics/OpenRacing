//! Property-based tests for the KartKraft telemetry adapter.
//!
//! Exercises FlatBuffers packet parsing, kart-specific field normalization,
//! and ensures the adapter never panics on arbitrary input.

#![allow(clippy::redundant_closure)]

use proptest::prelude::*;
use racing_wheel_telemetry_kartkraft::{KartKraftAdapter, NormalizedTelemetry, TelemetryAdapter};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ---------------------------------------------------------------------------
// Helpers (mirrors adapter_tests.rs make_test_packet)
// ---------------------------------------------------------------------------

/// Build a minimal valid KartKraft FlatBuffer with a Dashboard sub-table.
fn make_test_packet(
    speed: f32,
    rpm: f32,
    steer_deg: f32,
    throttle: f32,
    brake: f32,
    gear: i8,
) -> Vec<u8> {
    let mut buf: Vec<u8> = Vec::new();
    let push_u16 = |buf: &mut Vec<u8>, v: u16| buf.extend_from_slice(&v.to_le_bytes());
    let push_i32 = |buf: &mut Vec<u8>, v: i32| buf.extend_from_slice(&v.to_le_bytes());
    let push_u32 = |buf: &mut Vec<u8>, v: u32| buf.extend_from_slice(&v.to_le_bytes());
    let push_f32 = |buf: &mut Vec<u8>, v: f32| buf.extend_from_slice(&v.to_le_bytes());

    // Root offset placeholder + "KKFB" identifier
    push_u32(&mut buf, 0);
    buf.extend_from_slice(b"KKFB");

    // Frame vtable
    let vt_frame_start = buf.len();
    push_u16(&mut buf, 10); // vtable_size
    push_u16(&mut buf, 12); // object_size
    push_u16(&mut buf, 0); // field 0 absent
    push_u16(&mut buf, 0); // field 1 absent
    push_u16(&mut buf, 4); // field 2 (dash) at offset 4

    // Frame table
    let frame_table_pos = buf.len();
    push_i32(&mut buf, (frame_table_pos - vt_frame_start) as i32);
    push_u32(&mut buf, 0); // dash UOffset placeholder
    push_u32(&mut buf, 0); // padding

    // Patch root_offset
    buf[0..4].copy_from_slice(&(frame_table_pos as u32).to_le_bytes());

    // Dashboard vtable (6 fields)
    let vt_dash_start = buf.len();
    push_u16(&mut buf, 16); // vtable_size = 4 + 6*2
    push_u16(&mut buf, 28); // object_size = 4 + 6*4
    push_u16(&mut buf, 4); // speed
    push_u16(&mut buf, 8); // rpm
    push_u16(&mut buf, 12); // steer
    push_u16(&mut buf, 16); // throttle
    push_u16(&mut buf, 20); // brake
    push_u16(&mut buf, 24); // gear

    // Dashboard table
    let dash_table_pos = buf.len();
    push_i32(&mut buf, (dash_table_pos - vt_dash_start) as i32);
    push_f32(&mut buf, speed);
    push_f32(&mut buf, rpm);
    push_f32(&mut buf, steer_deg);
    push_f32(&mut buf, throttle);
    push_f32(&mut buf, brake);
    buf.push(gear as u8);
    buf.push(0);
    buf.push(0);
    buf.push(0);

    // Patch dash UOffset: ref_pos = frame_table_pos + 4
    let ref_pos = frame_table_pos + 4;
    let dash_uoffset = (dash_table_pos - ref_pos) as u32;
    buf[ref_pos..ref_pos + 4].copy_from_slice(&dash_uoffset.to_le_bytes());

    buf
}

// ---------------------------------------------------------------------------
// Fuzz: arbitrary bytes must never panic
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Arbitrary random bytes of any length must never cause a panic.
    #[test]
    fn prop_random_bytes_no_panic(
        data in proptest::collection::vec(any::<u8>(), 0..4096)
    ) {
        let adapter = KartKraftAdapter::new();
        let _ = adapter.normalize(&data);
    }

    /// Random bytes with a valid "KKFB" header must not cause a panic.
    #[test]
    fn prop_valid_header_random_body(
        body in proptest::collection::vec(any::<u8>(), 8..512)
    ) {
        let mut data = body;
        // Ensure the first 8 bytes contain root_offset + "KKFB"
        if data.len() >= 8 {
            data[4] = b'K';
            data[5] = b'K';
            data[6] = b'F';
            data[7] = b'B';
        }
        let adapter = KartKraftAdapter::new();
        let _ = adapter.normalize(&data);
    }
}

// ---------------------------------------------------------------------------
// Round-trip: generated packets → normalize → verify invariants
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// Valid packets with finite values must produce correct, bounded output.
    #[test]
    fn prop_valid_packet_round_trip(
        speed in 0.0f32..200.0,
        rpm in 0.0f32..20000.0,
        steer_deg in -90.0f32..90.0,
        throttle in 0.0f32..1.0,
        brake in 0.0f32..1.0,
        gear in -1i8..=6,
    ) {
        let data = make_test_packet(speed, rpm, steer_deg, throttle, brake, gear);
        let adapter = KartKraftAdapter::new();
        let result = adapter.normalize(&data);
        prop_assert!(result.is_ok(), "valid packet must parse: {:?}", result);
        let t = result.map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;

        // Speed preserved
        prop_assert!((t.speed_ms - speed).abs() < 0.01,
            "speed_ms mismatch: {} vs {}", t.speed_ms, speed);

        // RPM preserved
        prop_assert!((t.rpm - rpm).abs() < 0.1,
            "rpm mismatch: {} vs {}", t.rpm, rpm);

        // Steering: degrees / 90 → -1..1
        let expected_steer = steer_deg / 90.0;
        prop_assert!((t.steering_angle - expected_steer).abs() < 0.01,
            "steering mismatch: {} vs expected {}", t.steering_angle, expected_steer);

        // Throttle and brake preserved (already in 0..1 range)
        prop_assert!((t.throttle - throttle).abs() < 0.001,
            "throttle mismatch: {} vs {}", t.throttle, throttle);
        prop_assert!((t.brake - brake).abs() < 0.001,
            "brake mismatch: {} vs {}", t.brake, brake);

        // Gear preserved
        prop_assert_eq!(t.gear, gear, "gear mismatch");

        // All fields finite
        prop_assert!(t.speed_ms.is_finite());
        prop_assert!(t.rpm.is_finite());
        prop_assert!(t.steering_angle.is_finite());
        prop_assert!(t.throttle.is_finite());
        prop_assert!(t.brake.is_finite());
    }

    /// Throttle and brake over-range values must be clamped to [0, 1].
    #[test]
    fn prop_clamping_invariants(
        throttle in -5.0f32..5.0,
        brake in -5.0f32..5.0,
        steer_deg in -360.0f32..360.0,
    ) {
        let data = make_test_packet(50.0, 8000.0, steer_deg, throttle, brake, 3);
        let adapter = KartKraftAdapter::new();
        let result = adapter.normalize(&data);
        prop_assert!(result.is_ok());
        let t = result.map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;

        prop_assert!(t.throttle >= 0.0 && t.throttle <= 1.0,
            "throttle not clamped: {}", t.throttle);
        prop_assert!(t.brake >= 0.0 && t.brake <= 1.0,
            "brake not clamped: {}", t.brake);
        // Steering clamped to [-1, 1]
        prop_assert!(t.steering_angle >= -1.0 && t.steering_angle <= 1.0,
            "steering_angle not clamped: {}", t.steering_angle);
    }
}

// ---------------------------------------------------------------------------
// Kart-specific edge cases
// ---------------------------------------------------------------------------

#[test]
fn test_pre_race_state_neutral_idle() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let data = make_test_packet(0.0, 1200.0, 0.0, 0.0, 0.0, 0);
    let t = adapter.normalize(&data)?;
    assert!(t.speed_ms.abs() < 0.001, "pre-race speed should be 0");
    assert_eq!(t.gear, 0, "pre-race gear should be neutral");
    assert!(t.throttle.abs() < 0.001, "pre-race throttle should be 0");
    assert!(t.brake.abs() < 0.001, "pre-race brake should be 0");
    Ok(())
}

#[test]
fn test_full_throttle_start() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let data = make_test_packet(0.0, 12000.0, 0.0, 1.0, 0.0, 1);
    let t = adapter.normalize(&data)?;
    assert!((t.throttle - 1.0).abs() < 0.001);
    assert_eq!(t.gear, 1);
    Ok(())
}

#[test]
fn test_steering_boundary_values() -> TestResult {
    let adapter = KartKraftAdapter::new();
    // Exactly at ±90° boundaries
    let data = make_test_packet(50.0, 8000.0, 90.0, 0.5, 0.0, 3);
    let t = adapter.normalize(&data)?;
    assert!(
        (t.steering_angle - 1.0).abs() < 0.001,
        "90° should be 1.0, got {}",
        t.steering_angle
    );

    let data = make_test_packet(50.0, 8000.0, -90.0, 0.5, 0.0, 3);
    let t = adapter.normalize(&data)?;
    assert!(
        (t.steering_angle + 1.0).abs() < 0.001,
        "-90° should be -1.0, got {}",
        t.steering_angle
    );
    Ok(())
}

#[test]
fn test_over_range_steering_clamped() -> TestResult {
    let adapter = KartKraftAdapter::new();
    // Beyond ±90° should clamp to ±1.0
    let data = make_test_packet(50.0, 8000.0, 180.0, 0.5, 0.0, 3);
    let t = adapter.normalize(&data)?;
    assert!(
        (t.steering_angle - 1.0).abs() < 0.001,
        "180° should clamp to 1.0, got {}",
        t.steering_angle
    );
    Ok(())
}

#[test]
fn test_nan_fields_handled() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let data = make_test_packet(f32::NAN, f32::NAN, f32::NAN, f32::NAN, f32::NAN, 0);
    // Should not panic
    let _ = adapter.normalize(&data);
    Ok(())
}

#[test]
fn test_infinity_fields_handled() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let data = make_test_packet(
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        3,
    );
    let _ = adapter.normalize(&data);
    Ok(())
}

#[test]
fn test_empty_packet_rejected() {
    let adapter = KartKraftAdapter::new();
    assert!(adapter.normalize(&[]).is_err());
}

#[test]
fn test_too_short_for_header() {
    let adapter = KartKraftAdapter::new();
    assert!(adapter.normalize(&[0u8; 7]).is_err());
}

#[test]
fn test_wrong_identifier() {
    let adapter = KartKraftAdapter::new();
    let mut data = make_test_packet(0.0, 0.0, 0.0, 0.0, 0.0, 0);
    data[4] = b'X'; // corrupt "KKFB"
    assert!(adapter.normalize(&data).is_err());
}

#[test]
fn test_reverse_gear() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let data = make_test_packet(5.0, 3000.0, 0.0, 0.1, 0.0, -1);
    let t = adapter.normalize(&data)?;
    assert_eq!(t.gear, -1, "reverse gear should be -1");
    Ok(())
}

#[test]
fn test_normalize_is_deterministic() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let data = make_test_packet(25.0, 8000.0, 45.0, 0.8, 0.1, 3);
    let a = adapter.normalize(&data)?;
    let b = adapter.normalize(&data)?;
    assert!((a.speed_ms - b.speed_ms).abs() < f32::EPSILON);
    assert!((a.rpm - b.rpm).abs() < f32::EPSILON);
    assert_eq!(a.gear, b.gear);
    assert!((a.steering_angle - b.steering_angle).abs() < f32::EPSILON);
    assert!((a.throttle - b.throttle).abs() < f32::EPSILON);
    assert!((a.brake - b.brake).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn test_normalized_telemetry_default_invariants() {
    let t = NormalizedTelemetry::default();
    assert_eq!(t.speed_ms, 0.0);
    assert_eq!(t.rpm, 0.0);
    assert_eq!(t.gear, 0);
    assert_eq!(t.throttle, 0.0);
    assert_eq!(t.brake, 0.0);
    assert_eq!(t.steering_angle, 0.0);
}

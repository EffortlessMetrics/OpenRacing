//! Property-based tests for the RaceRoom Experience telemetry adapter.
//!
//! Exercises shared memory parsing, R3E-specific field handling, and ensures
//! the adapter never panics on arbitrary or malformed input.

#![allow(clippy::redundant_closure)]

use proptest::prelude::*;
use racing_wheel_telemetry_raceroom::{RaceRoomAdapter, TelemetryAdapter};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ---------------------------------------------------------------------------
// R3E shared memory constants
// ---------------------------------------------------------------------------

const R3E_VIEW_SIZE: usize = 4096;
const R3E_VERSION_MAJOR: i32 = 3;

const OFF_VERSION_MAJOR: usize = 0;
const OFF_GAME_PAUSED: usize = 20;
const OFF_GAME_IN_MENUS: usize = 24;
const OFF_SPEED: usize = 1392;
const OFF_ENGINE_RPS: usize = 1396;
const OFF_MAX_ENGINE_RPS: usize = 1400;
const OFF_GEAR: usize = 1408;
const OFF_FUEL_LEFT: usize = 1456;
const OFF_FUEL_CAPACITY: usize = 1460;
const OFF_THROTTLE: usize = 1500;
const OFF_BRAKE: usize = 1508;
const OFF_CLUTCH: usize = 1516;
const OFF_STEER_INPUT: usize = 1524;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn write_i32(buf: &mut [u8], offset: usize, value: i32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_f32(buf: &mut [u8], offset: usize, value: f32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

#[allow(clippy::too_many_arguments)]
fn make_r3e_memory(
    rpm: f32,
    speed: f32,
    steering: f32,
    throttle: f32,
    brake: f32,
    clutch: f32,
    gear: i32,
    fuel_left: f32,
    fuel_capacity: f32,
) -> Vec<u8> {
    let mut data = vec![0u8; R3E_VIEW_SIZE];
    write_i32(&mut data, OFF_VERSION_MAJOR, R3E_VERSION_MAJOR);
    write_i32(&mut data, OFF_GAME_PAUSED, 0);
    write_i32(&mut data, OFF_GAME_IN_MENUS, 0);
    let rps = rpm * (std::f32::consts::PI / 30.0);
    write_f32(&mut data, OFF_ENGINE_RPS, rps);
    let max_rps = 8000.0f32 * (std::f32::consts::PI / 30.0);
    write_f32(&mut data, OFF_MAX_ENGINE_RPS, max_rps);
    write_f32(&mut data, OFF_SPEED, speed);
    write_f32(&mut data, OFF_STEER_INPUT, steering);
    write_f32(&mut data, OFF_THROTTLE, throttle);
    write_f32(&mut data, OFF_BRAKE, brake);
    write_f32(&mut data, OFF_CLUTCH, clutch);
    write_i32(&mut data, OFF_GEAR, gear);
    write_f32(&mut data, OFF_FUEL_LEFT, fuel_left);
    write_f32(&mut data, OFF_FUEL_CAPACITY, fuel_capacity);
    data
}

fn make_default_memory(
    rpm: f32,
    speed: f32,
    steering: f32,
    throttle: f32,
    brake: f32,
    gear: i32,
) -> Vec<u8> {
    make_r3e_memory(rpm, speed, steering, throttle, brake, 0.0, gear, 30.0, 60.0)
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
        let adapter = RaceRoomAdapter::new();
        let _ = adapter.normalize(&data);
    }

    /// A buffer of exactly 4096 bytes (R3E shared memory view size) with
    /// random content must not panic.
    #[test]
    fn prop_valid_size_random_content(
        data in proptest::collection::vec(any::<u8>(), R3E_VIEW_SIZE..=R3E_VIEW_SIZE)
    ) {
        let adapter = RaceRoomAdapter::new();
        let _ = adapter.normalize(&data);
    }

    /// Valid version header + random body must not panic.
    #[test]
    fn prop_valid_version_random_body(
        mut data in proptest::collection::vec(any::<u8>(), R3E_VIEW_SIZE..=R3E_VIEW_SIZE)
    ) {
        // Set valid version
        data[OFF_VERSION_MAJOR..OFF_VERSION_MAJOR + 4]
            .copy_from_slice(&R3E_VERSION_MAJOR.to_le_bytes());
        let adapter = RaceRoomAdapter::new();
        let _ = adapter.normalize(&data);
    }
}

// ---------------------------------------------------------------------------
// Round-trip: generated shared memory → normalize → verify invariants
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// Valid shared memory with finite values must produce correct, bounded output.
    #[test]
    fn prop_valid_memory_round_trip(
        rpm in 0.0f32..20000.0,
        speed in 0.0f32..500.0,
        steering in -1.0f32..1.0,
        throttle in 0.0f32..1.0,
        brake in 0.0f32..1.0,
        clutch in 0.0f32..1.0,
        gear in -1i32..=6,
        fuel_left in 0.0f32..60.0,
    ) {
        let data = make_r3e_memory(rpm, speed, steering, throttle, brake, clutch, gear, fuel_left, 60.0);
        let adapter = RaceRoomAdapter::new();
        let result = adapter.normalize(&data);
        prop_assert!(result.is_ok(), "valid memory must parse: {:?}", result);
        let t = result.map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;

        // RPM round-trip (rps * 30/π)
        prop_assert!((t.rpm - rpm).abs() < 1.0,
            "rpm mismatch: {} vs expected {}", t.rpm, rpm);

        // Speed preserved
        prop_assert!((t.speed_ms - speed).abs() < 0.01,
            "speed mismatch: {} vs {}", t.speed_ms, speed);

        // Steering clamped to [-1, 1]
        prop_assert!(t.steering_angle >= -1.0 && t.steering_angle <= 1.0,
            "steering out of range: {}", t.steering_angle);

        // Throttle clamped to [0, 1]
        prop_assert!(t.throttle >= 0.0 && t.throttle <= 1.0,
            "throttle out of range: {}", t.throttle);

        // Brake clamped to [0, 1]
        prop_assert!(t.brake >= 0.0 && t.brake <= 1.0,
            "brake out of range: {}", t.brake);

        // Clutch clamped to [0, 1]
        prop_assert!(t.clutch >= 0.0 && t.clutch <= 1.0,
            "clutch out of range: {}", t.clutch);

        // Fuel percent in [0, 1]
        prop_assert!(t.fuel_percent >= 0.0 && t.fuel_percent <= 1.0,
            "fuel_percent out of range: {}", t.fuel_percent);

        // Gear preserved (clamped to i8 range)
        let expected_gear = gear.clamp(-128, 127) as i8;
        prop_assert_eq!(t.gear, expected_gear, "gear mismatch");

        // All fields finite
        prop_assert!(t.speed_ms.is_finite());
        prop_assert!(t.rpm.is_finite());
        prop_assert!(t.steering_angle.is_finite());
        prop_assert!(t.throttle.is_finite());
        prop_assert!(t.brake.is_finite());
        prop_assert!(t.fuel_percent.is_finite());
    }

    /// Over-range input values must be properly clamped.
    #[test]
    fn prop_clamping_invariants(
        steering in -10.0f32..10.0,
        throttle in -5.0f32..5.0,
        brake in -5.0f32..5.0,
        clutch in -5.0f32..5.0,
    ) {
        let data = make_r3e_memory(5000.0, 50.0, steering, throttle, brake, clutch, 3, 30.0, 60.0);
        let adapter = RaceRoomAdapter::new();
        let result = adapter.normalize(&data);
        prop_assert!(result.is_ok());
        let t = result.map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;

        prop_assert!(t.steering_angle >= -1.0 && t.steering_angle <= 1.0,
            "steering not clamped: {}", t.steering_angle);
        prop_assert!(t.throttle >= 0.0 && t.throttle <= 1.0,
            "throttle not clamped: {}", t.throttle);
        prop_assert!(t.brake >= 0.0 && t.brake <= 1.0,
            "brake not clamped: {}", t.brake);
        prop_assert!(t.clutch >= 0.0 && t.clutch <= 1.0,
            "clutch not clamped: {}", t.clutch);
    }

    /// Fuel percentage must be correctly computed from fuel_left / fuel_capacity.
    #[test]
    fn prop_fuel_computation(
        fuel_left in 0.0f32..100.0,
        fuel_capacity in 1.0f32..100.0,
    ) {
        let data = make_r3e_memory(3000.0, 20.0, 0.0, 0.5, 0.0, 0.0, 2, fuel_left, fuel_capacity);
        let adapter = RaceRoomAdapter::new();
        let t = adapter.normalize(&data)
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;

        prop_assert!(t.fuel_percent >= 0.0 && t.fuel_percent <= 1.0,
            "fuel_percent out of range: {} (left={}, cap={})", t.fuel_percent, fuel_left, fuel_capacity);
    }
}

// ---------------------------------------------------------------------------
// Memory mapping failures / version mismatches
// ---------------------------------------------------------------------------

#[test]
fn test_version_0_rejected() {
    let adapter = RaceRoomAdapter::new();
    let data = vec![0u8; R3E_VIEW_SIZE];
    assert!(adapter.normalize(&data).is_err(), "version 0 must be rejected");
}

#[test]
fn test_version_1_rejected() {
    let adapter = RaceRoomAdapter::new();
    let mut data = vec![0u8; R3E_VIEW_SIZE];
    write_i32(&mut data, OFF_VERSION_MAJOR, 1);
    assert!(adapter.normalize(&data).is_err(), "version 1 must be rejected");
}

#[test]
fn test_version_2_rejected() {
    let adapter = RaceRoomAdapter::new();
    let mut data = vec![0u8; R3E_VIEW_SIZE];
    write_i32(&mut data, OFF_VERSION_MAJOR, 2);
    assert!(adapter.normalize(&data).is_err(), "version 2 must be rejected");
}

#[test]
fn test_version_4_rejected() {
    let adapter = RaceRoomAdapter::new();
    let mut data = vec![0u8; R3E_VIEW_SIZE];
    write_i32(&mut data, OFF_VERSION_MAJOR, 4);
    assert!(adapter.normalize(&data).is_err(), "future version 4 must be rejected");
}

#[test]
fn test_negative_version_rejected() {
    let adapter = RaceRoomAdapter::new();
    let mut data = vec![0u8; R3E_VIEW_SIZE];
    write_i32(&mut data, OFF_VERSION_MAJOR, -1);
    assert!(adapter.normalize(&data).is_err(), "negative version must be rejected");
}

#[test]
fn test_undersized_buffers() {
    let adapter = RaceRoomAdapter::new();
    for size in [0, 1, 4, 100, 1000, 2048, 4095] {
        assert!(
            adapter.normalize(&vec![0u8; size]).is_err(),
            "buffer size {size} must be rejected"
        );
    }
}

#[test]
fn test_oversized_buffer_accepted() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut data = make_default_memory(3000.0, 30.0, 0.0, 0.5, 0.0, 3);
    data.extend_from_slice(&[0u8; 2048]);
    let t = adapter.normalize(&data)?;
    assert!((t.rpm - 3000.0).abs() < 1.0);
    Ok(())
}

// ---------------------------------------------------------------------------
// NaN / Infinity handling
// ---------------------------------------------------------------------------

#[test]
fn test_nan_in_all_float_fields() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut data = make_default_memory(0.0, 0.0, 0.0, 0.0, 0.0, 0);
    write_f32(&mut data, OFF_SPEED, f32::NAN);
    write_f32(&mut data, OFF_ENGINE_RPS, f32::NAN);
    write_f32(&mut data, OFF_MAX_ENGINE_RPS, f32::NAN);
    write_f32(&mut data, OFF_THROTTLE, f32::NAN);
    write_f32(&mut data, OFF_BRAKE, f32::NAN);
    write_f32(&mut data, OFF_CLUTCH, f32::NAN);
    write_f32(&mut data, OFF_STEER_INPUT, f32::NAN);
    write_f32(&mut data, OFF_FUEL_LEFT, f32::NAN);
    write_f32(&mut data, OFF_FUEL_CAPACITY, f32::NAN);

    let t = adapter.normalize(&data)?;
    assert!(t.speed_ms.abs() < 0.001, "NaN speed should default to 0");
    assert!(t.rpm.abs() < 0.001, "NaN rpm should default to 0");
    assert!(t.throttle.abs() < 0.001, "NaN throttle should default to 0");
    assert!(t.brake.abs() < 0.001, "NaN brake should default to 0");
    Ok(())
}

#[test]
fn test_infinity_in_float_fields() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut data = make_default_memory(0.0, 0.0, 0.0, 0.0, 0.0, 0);
    write_f32(&mut data, OFF_SPEED, f32::INFINITY);
    write_f32(&mut data, OFF_ENGINE_RPS, f32::NEG_INFINITY);
    write_f32(&mut data, OFF_THROTTLE, f32::INFINITY);

    let t = adapter.normalize(&data)?;
    assert!(t.speed_ms.abs() < 0.001, "infinite speed should default to 0");
    assert!(t.rpm.abs() < 0.001, "neg-inf rpm should default to 0");
    assert!(t.throttle.abs() < 0.001 || (t.throttle - 1.0).abs() < 0.001,
        "infinite throttle should be sanitized");
    Ok(())
}

// ---------------------------------------------------------------------------
// Paused / In-menus state
// ---------------------------------------------------------------------------

#[test]
fn test_paused_zeroes_all_telemetry() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut data = make_default_memory(5000.0, 80.0, 0.5, 1.0, 0.5, 4);
    write_i32(&mut data, OFF_GAME_PAUSED, 1);
    let t = adapter.normalize(&data)?;
    assert!(t.rpm.abs() < 0.001, "paused: rpm should be 0");
    assert!(t.speed_ms.abs() < 0.001, "paused: speed should be 0");
    assert_eq!(t.gear, 0, "paused: gear should be 0");
    Ok(())
}

#[test]
fn test_in_menus_zeroes_all_telemetry() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut data = make_default_memory(5000.0, 80.0, 0.5, 1.0, 0.5, 4);
    write_i32(&mut data, OFF_GAME_IN_MENUS, 1);
    let t = adapter.normalize(&data)?;
    assert!(t.rpm.abs() < 0.001, "in menus: rpm should be 0");
    assert!(t.speed_ms.abs() < 0.001, "in menus: speed should be 0");
    Ok(())
}

#[test]
fn test_fuel_zero_capacity() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let data = make_r3e_memory(3000.0, 20.0, 0.0, 0.3, 0.0, 0.0, 2, 10.0, 0.0);
    let t = adapter.normalize(&data)?;
    assert!(
        (t.fuel_percent - 0.0).abs() < 0.01,
        "zero capacity should give 0% fuel, got {}",
        t.fuel_percent
    );
    Ok(())
}

#[test]
fn test_negative_speed_becomes_positive() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let data = make_default_memory(2000.0, -30.0, 0.0, 0.0, 0.0, -1);
    let t = adapter.normalize(&data)?;
    assert!(
        (t.speed_ms - 30.0).abs() < 0.01,
        "negative speed should be abs()'d, got {}",
        t.speed_ms
    );
    Ok(())
}

#[test]
fn test_normalize_is_deterministic() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let data = make_default_memory(4500.0, 33.0, -0.2, 0.6, 0.1, 3);
    let a = adapter.normalize(&data)?;
    let b = adapter.normalize(&data)?;
    assert!((a.rpm - b.rpm).abs() < f32::EPSILON);
    assert!((a.speed_ms - b.speed_ms).abs() < f32::EPSILON);
    assert!((a.throttle - b.throttle).abs() < f32::EPSILON);
    assert!((a.brake - b.brake).abs() < f32::EPSILON);
    assert!((a.steering_angle - b.steering_angle).abs() < f32::EPSILON);
    assert_eq!(a.gear, b.gear);
    Ok(())
}

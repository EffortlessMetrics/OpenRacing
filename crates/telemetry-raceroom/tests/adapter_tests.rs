//! Integration tests for the `racing-wheel-telemetry-raceroom` crate.
//!
//! Tests exercise the public `RaceRoomAdapter` API with synthetic R3E shared
//! memory buffers.  All offsets are from the Sector3 SDK r3e.h (version 3.4).

use racing_wheel_telemetry_raceroom::{NormalizedTelemetry, RaceRoomAdapter, TelemetryAdapter};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ---------------------------------------------------------------------------
// R3E shared memory constants (must match adapter internals exactly)
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

/// Build a valid R3E shared memory buffer with the given telemetry values.
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
    // engine_rps in rad/s: RPM = rps * 30/π → rps = RPM * π/30
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

/// Convenience wrapper with default fuel values.
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
// Shared memory layout verification
// ---------------------------------------------------------------------------

#[test]
fn view_size_is_4096_bytes() {
    assert_eq!(R3E_VIEW_SIZE, 4096);
}

#[test]
fn field_offsets_are_non_overlapping() {
    let offsets: &[(usize, usize)] = &[
        (OFF_VERSION_MAJOR, 4),
        (OFF_GAME_PAUSED, 4),
        (OFF_GAME_IN_MENUS, 4),
        (OFF_SPEED, 4),
        (OFF_ENGINE_RPS, 4),
        (OFF_MAX_ENGINE_RPS, 4),
        (OFF_GEAR, 4),
        (OFF_FUEL_LEFT, 4),
        (OFF_FUEL_CAPACITY, 4),
        (OFF_THROTTLE, 4),
        (OFF_BRAKE, 4),
        (OFF_CLUTCH, 4),
        (OFF_STEER_INPUT, 4),
    ];
    for (i, (a_off, a_sz)) in offsets.iter().enumerate() {
        for (b_off, b_sz) in &offsets[i + 1..] {
            let a_end = a_off + a_sz;
            let b_end = b_off + b_sz;
            assert!(
                a_end <= *b_off || b_end <= *a_off,
                "offsets {a_off}..{a_end} and {b_off}..{b_end} overlap"
            );
        }
    }
}

#[test]
fn all_offsets_fit_within_view() {
    let max_offset = [
        OFF_VERSION_MAJOR,
        OFF_GAME_PAUSED,
        OFF_GAME_IN_MENUS,
        OFF_SPEED,
        OFF_ENGINE_RPS,
        OFF_MAX_ENGINE_RPS,
        OFF_GEAR,
        OFF_FUEL_LEFT,
        OFF_FUEL_CAPACITY,
        OFF_THROTTLE,
        OFF_BRAKE,
        OFF_CLUTCH,
        OFF_STEER_INPUT,
    ]
    .into_iter()
    .max()
    .unwrap_or(0);
    // Every field is 4 bytes, so last byte is max_offset + 3.
    assert!(
        max_offset + 4 <= R3E_VIEW_SIZE,
        "highest field end {} exceeds view size {}",
        max_offset + 4,
        R3E_VIEW_SIZE
    );
}

// ---------------------------------------------------------------------------
// Adapter identity
// ---------------------------------------------------------------------------

#[test]
fn game_id_is_raceroom() {
    let adapter = RaceRoomAdapter::new();
    assert_eq!(adapter.game_id(), "raceroom");
}

#[test]
fn update_rate_is_10ms() {
    let adapter = RaceRoomAdapter::new();
    assert_eq!(
        adapter.expected_update_rate(),
        std::time::Duration::from_millis(10)
    );
}

#[test]
fn default_produces_same_as_new() {
    let a = RaceRoomAdapter::new();
    let b = RaceRoomAdapter::default();
    assert_eq!(a.game_id(), b.game_id());
    assert_eq!(a.expected_update_rate(), b.expected_update_rate());
}

// ---------------------------------------------------------------------------
// Valid parsing
// ---------------------------------------------------------------------------

#[test]
fn parse_typical_driving_data() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let data = make_default_memory(5000.0, 50.0, 0.3, 0.7, 0.0, 3);
    let t = adapter.normalize(&data)?;
    assert!((t.rpm - 5000.0).abs() < 1.0, "rpm mismatch: {}", t.rpm);
    assert!(
        (t.speed_ms - 50.0).abs() < 0.01,
        "speed mismatch: {}",
        t.speed_ms
    );
    assert!(
        (t.steering_angle - 0.3).abs() < 0.001,
        "steering mismatch: {}",
        t.steering_angle
    );
    assert!(
        (t.throttle - 0.7).abs() < 0.001,
        "throttle mismatch: {}",
        t.throttle
    );
    assert!(t.brake.abs() < 0.001, "brake should be ~0");
    assert_eq!(t.gear, 3);
    Ok(())
}

#[test]
fn parse_braking_data() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let data = make_default_memory(3000.0, 25.0, 0.0, 0.0, 0.9, 2);
    let t = adapter.normalize(&data)?;
    assert!((t.brake - 0.9).abs() < 0.001);
    assert!(t.throttle.abs() < 0.001);
    Ok(())
}

#[test]
fn parse_clutch_value() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let data = make_r3e_memory(2000.0, 10.0, 0.0, 0.3, 0.0, 0.6, 1, 30.0, 60.0);
    let t = adapter.normalize(&data)?;
    assert!(
        (t.clutch - 0.6).abs() < 0.001,
        "clutch mismatch: {}",
        t.clutch
    );
    Ok(())
}

#[test]
fn rpm_conversion_rad_to_rpm() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let data = make_default_memory(1000.0, 0.0, 0.0, 0.0, 0.0, 0);
    let t = adapter.normalize(&data)?;
    assert!(
        (t.rpm - 1000.0).abs() < 1.0,
        "1000 RPM roundtrip failed: {}",
        t.rpm
    );
    Ok(())
}

#[test]
fn max_rpm_populated() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let data = make_default_memory(3000.0, 20.0, 0.0, 0.5, 0.0, 2);
    let t = adapter.normalize(&data)?;
    // max_rps = 8000 * PI/30 → max_rpm ≈ 8000
    assert!(
        (t.max_rpm - 8000.0).abs() < 1.0,
        "max_rpm mismatch: {}",
        t.max_rpm
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Gear encoding
// ---------------------------------------------------------------------------

#[test]
fn gear_reverse() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let data = make_default_memory(1000.0, 2.0, 0.0, 0.1, 0.0, -1);
    let t = adapter.normalize(&data)?;
    assert_eq!(t.gear, -1, "gear -1 should map to reverse");
    Ok(())
}

#[test]
fn gear_neutral() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let data = make_default_memory(800.0, 0.0, 0.0, 0.0, 0.0, 0);
    let t = adapter.normalize(&data)?;
    assert_eq!(t.gear, 0, "gear 0 should map to neutral");
    Ok(())
}

#[test]
fn gear_forward() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    for g in 1..=6 {
        let data = make_default_memory(4000.0, 30.0, 0.0, 0.5, 0.0, g);
        let t = adapter.normalize(&data)?;
        assert_eq!(t.gear, g as i8, "gear {g} roundtrip failed");
    }
    Ok(())
}

#[test]
fn gear_clamped_to_i8_range() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let data = make_default_memory(4000.0, 30.0, 0.0, 0.5, 0.0, 200);
    let t = adapter.normalize(&data)?;
    assert_eq!(t.gear, 127, "large gear should clamp to 127");
    Ok(())
}

// ---------------------------------------------------------------------------
// Fuel
// ---------------------------------------------------------------------------

#[test]
fn fuel_percent_half_tank() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let data = make_r3e_memory(3000.0, 20.0, 0.0, 0.3, 0.0, 0.0, 2, 30.0, 60.0);
    let t = adapter.normalize(&data)?;
    let ext = format!("{:?}", t.get_extended("fuel_percent"));
    assert!(
        ext.contains("0.5"),
        "expected fuel_percent ~0.5, got: {ext}"
    );
    Ok(())
}

#[test]
fn fuel_percent_full_tank() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let data = make_r3e_memory(3000.0, 20.0, 0.0, 0.3, 0.0, 0.0, 2, 60.0, 60.0);
    let t = adapter.normalize(&data)?;
    let ext = format!("{:?}", t.get_extended("fuel_percent"));
    assert!(ext.contains("1.0"), "expected fuel_percent 1.0, got: {ext}");
    Ok(())
}

#[test]
fn fuel_percent_zero_capacity() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let data = make_r3e_memory(3000.0, 20.0, 0.0, 0.3, 0.0, 0.0, 2, 10.0, 0.0);
    let t = adapter.normalize(&data)?;
    let ext = format!("{:?}", t.get_extended("fuel_percent"));
    assert!(
        ext.contains("0.0"),
        "expected fuel_percent 0.0 when capacity is zero, got: {ext}"
    );
    Ok(())
}

#[test]
fn fuel_percent_overfilled_clamped() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let data = make_r3e_memory(3000.0, 20.0, 0.0, 0.3, 0.0, 0.0, 2, 100.0, 60.0);
    let t = adapter.normalize(&data)?;
    let ext = format!("{:?}", t.get_extended("fuel_percent"));
    assert!(
        ext.contains("1.0"),
        "overfilled fuel should clamp to 1.0, got: {ext}"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Clamping and edge cases
// ---------------------------------------------------------------------------

#[test]
fn steering_clamped_high() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let data = make_default_memory(3000.0, 30.0, 2.0, 0.5, 0.0, 2);
    let t = adapter.normalize(&data)?;
    assert!(
        (t.steering_angle - 1.0).abs() < 0.001,
        "steering >1 should clamp to 1.0"
    );
    Ok(())
}

#[test]
fn steering_clamped_low() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let data = make_default_memory(3000.0, 30.0, -5.0, 0.5, 0.0, 2);
    let t = adapter.normalize(&data)?;
    assert!(
        (t.steering_angle + 1.0).abs() < 0.001,
        "steering <-1 should clamp to -1.0"
    );
    Ok(())
}

#[test]
fn throttle_clamped() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let data = make_default_memory(3000.0, 30.0, 0.0, 1.5, 0.0, 2);
    let t = adapter.normalize(&data)?;
    assert!(
        (t.throttle - 1.0).abs() < 0.001,
        "throttle >1 should clamp to 1.0"
    );
    Ok(())
}

#[test]
fn brake_clamped() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let data = make_default_memory(3000.0, 30.0, 0.0, 0.0, -0.5, 2);
    let t = adapter.normalize(&data)?;
    assert!(t.brake.abs() < 0.001, "negative brake should clamp to 0.0");
    Ok(())
}

#[test]
fn negative_speed_becomes_positive() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let data = make_default_memory(2000.0, -30.0, 0.0, 0.0, 0.0, -1);
    let t = adapter.normalize(&data)?;
    assert!(
        (t.speed_ms - 30.0).abs() < 0.01,
        "negative speed should be abs()'d"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Default / zero initialization
// ---------------------------------------------------------------------------

#[test]
fn zero_filled_buffer_with_valid_version_returns_default() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut data = vec![0u8; R3E_VIEW_SIZE];
    write_i32(&mut data, OFF_VERSION_MAJOR, R3E_VERSION_MAJOR);
    let t = adapter.normalize(&data)?;
    assert!(t.rpm.abs() < 0.001);
    assert!(t.speed_ms.abs() < 0.001);
    assert!(t.throttle.abs() < 0.001);
    assert!(t.brake.abs() < 0.001);
    assert_eq!(t.gear, 0);
    Ok(())
}

#[test]
fn paused_game_returns_zeroed_telemetry() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut data = make_default_memory(5000.0, 80.0, 0.5, 1.0, 0.0, 4);
    write_i32(&mut data, OFF_GAME_PAUSED, 1);
    let t = adapter.normalize(&data)?;
    assert!(t.rpm.abs() < 0.001, "paused: rpm should be 0");
    assert!(t.speed_ms.abs() < 0.001, "paused: speed should be 0");
    assert_eq!(t.gear, 0, "paused: gear should be 0");
    Ok(())
}

#[test]
fn in_menus_returns_zeroed_telemetry() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut data = make_default_memory(5000.0, 80.0, 0.5, 1.0, 0.0, 4);
    write_i32(&mut data, OFF_GAME_IN_MENUS, 1);
    let t = adapter.normalize(&data)?;
    assert!(t.rpm.abs() < 0.001, "in menus: rpm should be 0");
    assert!(t.speed_ms.abs() < 0.001, "in menus: speed should be 0");
    Ok(())
}

#[test]
fn paused_and_in_menus_returns_zeroed_telemetry() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut data = make_default_memory(5000.0, 80.0, 0.5, 1.0, 0.0, 4);
    write_i32(&mut data, OFF_GAME_PAUSED, 1);
    write_i32(&mut data, OFF_GAME_IN_MENUS, 1);
    let t = adapter.normalize(&data)?;
    assert!(t.rpm.abs() < 0.001);
    Ok(())
}

// ---------------------------------------------------------------------------
// Error cases
// ---------------------------------------------------------------------------

#[test]
fn empty_buffer_returns_error() {
    let adapter = RaceRoomAdapter::new();
    assert!(adapter.normalize(&[]).is_err(), "empty buffer must error");
}

#[test]
fn undersized_buffer_returns_error() {
    let adapter = RaceRoomAdapter::new();
    assert!(
        adapter.normalize(&[0u8; 100]).is_err(),
        "undersized buffer must error"
    );
}

#[test]
fn wrong_version_returns_error() {
    let adapter = RaceRoomAdapter::new();
    let mut data = vec![0u8; R3E_VIEW_SIZE];
    write_i32(&mut data, OFF_VERSION_MAJOR, 1);
    assert!(
        adapter.normalize(&data).is_err(),
        "wrong version must error"
    );
}

#[test]
fn version_zero_returns_error() {
    let adapter = RaceRoomAdapter::new();
    let data = vec![0u8; R3E_VIEW_SIZE];
    assert!(adapter.normalize(&data).is_err(), "version 0 must error");
}

#[test]
fn future_version_returns_error() {
    let adapter = RaceRoomAdapter::new();
    let mut data = vec![0u8; R3E_VIEW_SIZE];
    write_i32(&mut data, OFF_VERSION_MAJOR, 4);
    assert!(adapter.normalize(&data).is_err(), "version 4 must error");
}

#[test]
fn exactly_view_size_is_accepted() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let data = make_default_memory(1000.0, 10.0, 0.0, 0.2, 0.0, 1);
    assert_eq!(data.len(), R3E_VIEW_SIZE);
    let _t = adapter.normalize(&data)?;
    Ok(())
}

#[test]
fn larger_than_view_size_is_accepted() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut data = make_default_memory(1000.0, 10.0, 0.0, 0.2, 0.0, 1);
    data.extend_from_slice(&[0u8; 1024]);
    let t = adapter.normalize(&data)?;
    assert!((t.rpm - 1000.0).abs() < 1.0);
    Ok(())
}

// ---------------------------------------------------------------------------
// NaN / Infinity handling
// ---------------------------------------------------------------------------

#[test]
fn nan_speed_defaults_to_zero() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut data = make_default_memory(3000.0, 0.0, 0.0, 0.5, 0.0, 2);
    write_f32(&mut data, OFF_SPEED, f32::NAN);
    let t = adapter.normalize(&data)?;
    assert!(t.speed_ms.abs() < 0.001, "NaN speed should default to 0.0");
    Ok(())
}

#[test]
fn infinity_rpm_defaults_to_zero() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut data = make_default_memory(0.0, 20.0, 0.0, 0.5, 0.0, 2);
    write_f32(&mut data, OFF_ENGINE_RPS, f32::INFINITY);
    let t = adapter.normalize(&data)?;
    assert!(t.rpm.abs() < 0.001, "infinity RPM should default to 0.0");
    Ok(())
}

#[test]
fn neg_infinity_throttle_defaults_to_zero() -> TestResult {
    let adapter = RaceRoomAdapter::new();
    let mut data = make_default_memory(3000.0, 20.0, 0.0, 0.0, 0.0, 2);
    write_f32(&mut data, OFF_THROTTLE, f32::NEG_INFINITY);
    let t = adapter.normalize(&data)?;
    assert!(
        t.throttle.abs() < 0.001,
        "neg infinity throttle should default to 0.0"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// TelemetryFrame construction (compile-time type check)
// ---------------------------------------------------------------------------

#[test]
fn telemetry_frame_constructible() {
    let nt = NormalizedTelemetry::builder().build();
    let frame = racing_wheel_telemetry_raceroom::TelemetryFrame::new(nt, 42, 1, R3E_VIEW_SIZE);
    assert_eq!(frame.timestamp_ns, 42);
    assert_eq!(frame.sequence, 1);
    assert_eq!(frame.raw_size, R3E_VIEW_SIZE);
}

// ---------------------------------------------------------------------------
// normalize() is consistent (same input → same output)
// ---------------------------------------------------------------------------

#[test]
fn normalize_is_deterministic() -> TestResult {
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

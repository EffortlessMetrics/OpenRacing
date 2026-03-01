//! Integration tests for the `racing-wheel-telemetry-kartkraft` crate.
//!
//! Tests verify KartKraft FlatBuffers UDP parsing via the public API.

use racing_wheel_telemetry_kartkraft::{KartKraftAdapter, NormalizedTelemetry, TelemetryAdapter};
use std::time::Duration;

type TestResult = Result<(), Box<dyn std::error::Error>>;

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

#[test]
fn test_game_id() {
    let adapter = KartKraftAdapter::new();
    assert_eq!(adapter.game_id(), "kartkraft");
}

#[test]
fn test_update_rate_60hz() {
    let adapter = KartKraftAdapter::new();
    assert_eq!(adapter.expected_update_rate(), Duration::from_millis(16));
}

#[test]
fn test_default_trait() {
    let adapter = KartKraftAdapter::default();
    assert_eq!(adapter.game_id(), "kartkraft");
}

#[test]
fn test_parse_valid_packet() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let data = make_test_packet(25.0, 8000.0, 45.0, 0.8, 0.1, 3);
    let t = adapter.normalize(&data)?;
    assert!((t.speed_ms - 25.0).abs() < 0.001, "speed_ms {}", t.speed_ms);
    assert!((t.rpm - 8000.0).abs() < 0.1, "rpm {}", t.rpm);
    assert!((t.throttle - 0.8).abs() < 0.001, "throttle {}", t.throttle);
    assert!((t.brake - 0.1).abs() < 0.001, "brake {}", t.brake);
    assert_eq!(t.gear, 3);
    // steer: 45° / 90° max = 0.5
    assert!(
        (t.steering_angle - 0.5).abs() < 0.001,
        "steering_angle {}",
        t.steering_angle
    );
    Ok(())
}

#[test]
fn test_too_short_rejected() {
    let adapter = KartKraftAdapter::new();
    assert!(adapter.normalize(&[]).is_err(), "empty must be rejected");
    assert!(
        adapter.normalize(&[0u8; 7]).is_err(),
        "7-byte packet must be rejected"
    );
}

#[test]
fn test_wrong_identifier_rejected() {
    let adapter = KartKraftAdapter::new();
    let mut data = make_test_packet(0.0, 0.0, 0.0, 0.0, 0.0, 0);
    data[4] = b'X'; // corrupt "KKFB" identifier
    assert!(
        adapter.normalize(&data).is_err(),
        "wrong identifier must be rejected"
    );
}

#[test]
fn test_reverse_gear() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let data = make_test_packet(5.0, 3000.0, 0.0, 0.1, 0.0, -1);
    let t = adapter.normalize(&data)?;
    assert_eq!(t.gear, -1);
    Ok(())
}

#[test]
fn test_steering_normalisation_full_lock() -> TestResult {
    let adapter = KartKraftAdapter::new();
    // Full right lock (90°) → 1.0
    let data = make_test_packet(0.0, 0.0, 90.0, 0.0, 0.0, 0);
    let t = adapter.normalize(&data)?;
    assert!(
        (t.steering_angle - 1.0).abs() < 0.001,
        "right lock={}",
        t.steering_angle
    );

    // Full left lock (-90°) → -1.0
    let data = make_test_packet(0.0, 0.0, -90.0, 0.0, 0.0, 0);
    let t = adapter.normalize(&data)?;
    assert!(
        (t.steering_angle + 1.0).abs() < 0.001,
        "left lock={}",
        t.steering_angle
    );
    Ok(())
}

#[test]
fn test_throttle_brake_clamped() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let data = make_test_packet(0.0, 0.0, 0.0, 2.0, -1.0, 0);
    let t = adapter.normalize(&data)?;
    assert!(
        (t.throttle - 1.0).abs() < 0.001,
        "throttle should clamp to 1"
    );
    assert!(t.brake.abs() < 0.001, "brake should clamp to 0");
    Ok(())
}

#[test]
fn test_with_port_builder() {
    let adapter = KartKraftAdapter::new().with_port(6000);
    assert_eq!(adapter.game_id(), "kartkraft");
}

#[test]
fn test_speed_nonnegative() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let data = make_test_packet(50.0, 5000.0, 0.0, 0.5, 0.0, 2);
    let t = adapter.normalize(&data)?;
    assert!(t.speed_ms >= 0.0, "speed_ms must be non-negative");
    assert!(t.rpm >= 0.0, "rpm must be non-negative");
    Ok(())
}

#[test]
fn test_normalized_telemetry_default() {
    let t = NormalizedTelemetry::default();
    assert_eq!(t.speed_ms, 0.0);
    assert_eq!(t.rpm, 0.0);
    assert_eq!(t.gear, 0);
}

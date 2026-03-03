//! Deep individual tests for the KartKraft telemetry adapter.
//!
//! Covers FlatBuffers packet parsing, identifier validation, steering
//! normalization, clamping, gear handling, and edge cases.

use racing_wheel_telemetry_kartkraft::{KartKraftAdapter, TelemetryAdapter};
use std::time::Duration;

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Build a minimal valid KartKraft FlatBuffer packet with Dashboard fields.
///
/// Matches the FlatBuffers layout: root_offset + "KKFB" identifier, then
/// Frame vtable/table referencing a Dashboard vtable/table.
fn make_packet(
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

    // Root offset (u32) + file identifier "KKFB"
    push_u32(&mut buf, 0); // placeholder
    buf.extend_from_slice(b"KKFB");

    // Frame vtable
    let vt_frame = buf.len();
    push_u16(&mut buf, 10); // vtable_size = 4 + 3*2
    push_u16(&mut buf, 12); // object_size
    push_u16(&mut buf, 0); // field 0 (timestamp) absent
    push_u16(&mut buf, 0); // field 1 (motion) absent
    push_u16(&mut buf, 4); // field 2 (dash) at offset 4

    // Frame table
    let frame_pos = buf.len();
    push_i32(&mut buf, (frame_pos - vt_frame) as i32); // soffset
    push_u32(&mut buf, 0); // dash UOffset placeholder
    push_u32(&mut buf, 0); // padding

    // Patch root_offset
    buf[0..4].copy_from_slice(&(frame_pos as u32).to_le_bytes());

    // Dashboard vtable (6 fields)
    let vt_dash = buf.len();
    push_u16(&mut buf, 16); // vtable_size = 4 + 6*2
    push_u16(&mut buf, 28); // object_size = 4 + 6*4
    push_u16(&mut buf, 4); // field 0 (speed)
    push_u16(&mut buf, 8); // field 1 (rpm)
    push_u16(&mut buf, 12); // field 2 (steer)
    push_u16(&mut buf, 16); // field 3 (throttle)
    push_u16(&mut buf, 20); // field 4 (brake)
    push_u16(&mut buf, 24); // field 5 (gear)

    // Dashboard table
    let dash_pos = buf.len();
    push_i32(&mut buf, (dash_pos - vt_dash) as i32);
    push_f32(&mut buf, speed);
    push_f32(&mut buf, rpm);
    push_f32(&mut buf, steer_deg);
    push_f32(&mut buf, throttle);
    push_f32(&mut buf, brake);
    buf.push(gear as u8);
    buf.push(0);
    buf.push(0);
    buf.push(0);

    // Patch dash UOffset: ref_pos = frame_pos + 4
    let ref_pos = frame_pos + 4;
    let dash_uoffset = (dash_pos - ref_pos) as u32;
    buf[ref_pos..ref_pos + 4].copy_from_slice(&dash_uoffset.to_le_bytes());

    buf
}

// ── Adapter metadata ─────────────────────────────────────────────────────────

#[test]
fn deep_game_id() {
    assert_eq!(KartKraftAdapter::new().game_id(), "kartkraft");
}

#[test]
fn deep_update_rate_60hz() {
    let adapter = KartKraftAdapter::new();
    assert_eq!(adapter.expected_update_rate(), Duration::from_millis(16));
}

#[test]
fn deep_default_trait() {
    let adapter = KartKraftAdapter::default();
    assert_eq!(adapter.game_id(), "kartkraft");
}

// ── Valid packet parsing ─────────────────────────────────────────────────────

#[test]
fn deep_parse_race_pace() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let data = make_packet(25.0, 8000.0, 30.0, 0.9, 0.05, 3);
    let t = adapter.normalize(&data)?;
    assert!((t.speed_ms - 25.0).abs() < 0.001, "speed={}", t.speed_ms);
    assert!((t.rpm - 8000.0).abs() < 0.1, "rpm={}", t.rpm);
    assert_eq!(t.gear, 3);
    assert!((t.throttle - 0.9).abs() < 0.001);
    assert!((t.brake - 0.05).abs() < 0.001);
    // steer: 30/90 = 0.333...
    assert!((t.steering_angle - (30.0 / 90.0)).abs() < 0.001);
    Ok(())
}

#[test]
fn deep_parse_idle_neutral() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let data = make_packet(0.0, 1500.0, 0.0, 0.0, 0.0, 0);
    let t = adapter.normalize(&data)?;
    assert_eq!(t.speed_ms, 0.0);
    assert!((t.rpm - 1500.0).abs() < 0.1);
    assert_eq!(t.gear, 0);
    assert_eq!(t.throttle, 0.0);
    assert_eq!(t.steering_angle, 0.0);
    Ok(())
}

#[test]
fn deep_parse_reverse_gear() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let data = make_packet(5.0, 3000.0, 0.0, 0.1, 0.0, -1);
    let t = adapter.normalize(&data)?;
    assert_eq!(t.gear, -1);
    Ok(())
}

// ── Steering normalization ───────────────────────────────────────────────────

#[test]
fn deep_steer_full_right_90deg() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let data = make_packet(0.0, 0.0, 90.0, 0.0, 0.0, 0);
    let t = adapter.normalize(&data)?;
    assert!((t.steering_angle - 1.0).abs() < 0.001, "90°=1.0");
    Ok(())
}

#[test]
fn deep_steer_full_left_neg90deg() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let data = make_packet(0.0, 0.0, -90.0, 0.0, 0.0, 0);
    let t = adapter.normalize(&data)?;
    assert!((t.steering_angle - (-1.0)).abs() < 0.001, "-90°=-1.0");
    Ok(())
}

#[test]
fn deep_steer_half_right() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let data = make_packet(0.0, 0.0, 45.0, 0.0, 0.0, 0);
    let t = adapter.normalize(&data)?;
    assert!((t.steering_angle - 0.5).abs() < 0.001, "45°=0.5");
    Ok(())
}

#[test]
fn deep_steer_overrange_clamped() -> TestResult {
    let adapter = KartKraftAdapter::new();
    // 180° → 180/90 = 2.0 → clamp to 1.0
    let data = make_packet(0.0, 0.0, 180.0, 0.0, 0.0, 0);
    let t = adapter.normalize(&data)?;
    assert!((t.steering_angle - 1.0).abs() < 0.001, "overrange clamped");
    Ok(())
}

// ── Clamping tests ───────────────────────────────────────────────────────────

#[test]
fn deep_throttle_overclamped() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let data = make_packet(0.0, 0.0, 0.0, 2.0, 0.0, 0);
    let t = adapter.normalize(&data)?;
    assert!(t.throttle <= 1.0, "throttle={}", t.throttle);
    Ok(())
}

#[test]
fn deep_brake_negative_clamped() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let data = make_packet(0.0, 0.0, 0.0, 0.0, -1.0, 0);
    let t = adapter.normalize(&data)?;
    assert!(t.brake >= 0.0, "brake={}", t.brake);
    Ok(())
}

#[test]
fn deep_speed_nonnegative() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let data = make_packet(-10.0, 0.0, 0.0, 0.0, 0.0, 0);
    let t = adapter.normalize(&data)?;
    assert!(t.speed_ms >= 0.0, "speed clamped");
    Ok(())
}

#[test]
fn deep_rpm_nonnegative() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let data = make_packet(0.0, -500.0, 0.0, 0.0, 0.0, 0);
    let t = adapter.normalize(&data)?;
    assert!(t.rpm >= 0.0, "rpm clamped");
    Ok(())
}

// ── Malformed packet tests ───────────────────────────────────────────────────

#[test]
fn deep_empty_packet_rejected() -> TestResult {
    let adapter = KartKraftAdapter::new();
    assert!(adapter.normalize(&[]).is_err());
    Ok(())
}

#[test]
fn deep_7_byte_packet_rejected() -> TestResult {
    let adapter = KartKraftAdapter::new();
    assert!(adapter.normalize(&[0u8; 7]).is_err());
    Ok(())
}

#[test]
fn deep_wrong_identifier_rejected() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let mut data = make_packet(0.0, 0.0, 0.0, 0.0, 0.0, 0);
    // Corrupt the "KKFB" identifier
    data[4] = b'X';
    data[5] = b'X';
    assert!(adapter.normalize(&data).is_err());
    Ok(())
}

#[test]
fn deep_8_bytes_with_wrong_ident_rejected() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let data = vec![0u8; 8]; // correct size but no KKFB
    assert!(adapter.normalize(&data).is_err());
    Ok(())
}

// ── Gear range tests ─────────────────────────────────────────────────────────

#[test]
fn deep_gear_range_neg1_to_6() -> TestResult {
    let adapter = KartKraftAdapter::new();
    for g in -1i8..=6 {
        let data = make_packet(10.0, 5000.0, 0.0, 0.5, 0.0, g);
        let t = adapter.normalize(&data)?;
        assert_eq!(t.gear, g, "gear={g}");
    }
    Ok(())
}

// ── NaN handling ─────────────────────────────────────────────────────────────

#[test]
fn deep_nan_speed_defaults() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let mut data = make_packet(0.0, 1000.0, 0.0, 0.0, 0.0, 0);
    // Overwrite speed field with NaN in the dashboard table.
    // The dashboard table starts after the frame vtable/table and dash vtable,
    // find where the f32 speed is and overwrite it.
    // Since the packet is constructed dynamically, we find the NaN bytes.
    // The speed is the first f32 after the dash soffset (i32).
    // From make_packet: dash_pos = after vtable_dash in the buffer, then soffset + speed f32.
    // We wrote 0.0f32 for speed. Find the dash table position.
    // Actually let's just search for the 0.0f32 that's speed and overwrite it.
    // Speed is at dash_pos + 4 in the buffer. From the layout:
    //   0..4: root offset
    //   4..8: KKFB
    //   8..18: frame vtable (10 bytes)
    //   18..30: frame table (12 bytes)
    //   30..46: dash vtable (16 bytes)
    //   46..50: dash soffset
    //   50..54: speed (f32)
    data[50..54].copy_from_slice(&f32::NAN.to_le_bytes());
    let t = adapter.normalize(&data)?;
    assert!(t.speed_ms.is_finite());
    assert!(t.speed_ms >= 0.0);
    Ok(())
}

// ── Determinism ──────────────────────────────────────────────────────────────

#[test]
fn deep_deterministic_output() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let data = make_packet(20.0, 7000.0, -15.0, 0.7, 0.2, 2);
    let t1 = adapter.normalize(&data)?;
    let t2 = adapter.normalize(&data)?;
    assert_eq!(t1.speed_ms, t2.speed_ms);
    assert_eq!(t1.rpm, t2.rpm);
    assert_eq!(t1.gear, t2.gear);
    assert_eq!(t1.throttle, t2.throttle);
    assert_eq!(t1.steering_angle, t2.steering_angle);
    Ok(())
}

// ── Oversized packet accepted ────────────────────────────────────────────────

#[test]
fn deep_oversized_packet_accepted() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let mut data = make_packet(15.0, 6000.0, 10.0, 0.5, 0.1, 1);
    data.extend_from_slice(&[0xAAu8; 512]);
    let t = adapter.normalize(&data)?;
    assert!((t.speed_ms - 15.0).abs() < 0.001);
    Ok(())
}

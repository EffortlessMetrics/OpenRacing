//! Insta snapshot tests for the KartKraft FlatBuffers telemetry adapter.
//!
//! Three scenarios: normal mid-race kart, standing start / neutral, and
//! edge case with reverse gear and saturated inputs.

use racing_wheel_telemetry_kartkraft::{KartKraftAdapter, TelemetryAdapter};

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

// ─── Scenario 1: Normal mid-race kart ───────────────────────────────────────
// 3rd gear at 25 m/s, moderate RPM, partial throttle, light braking, slight
// right steer (45° → 0.5 normalised).

#[test]
fn kartkraft_normal_mid_race() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let data = make_test_packet(25.0, 8000.0, 45.0, 0.8, 0.1, 3);
    let normalized = adapter.normalize(&data)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── Scenario 2: Standing start / neutral ───────────────────────────────────
// Kart stationary on the grid in neutral, zero inputs across the board.

#[test]
fn kartkraft_standing_start_neutral() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let data = make_test_packet(0.0, 900.0, 0.0, 0.0, 0.0, 0);
    let normalized = adapter.normalize(&data)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

// ─── Scenario 3: Reverse gear with saturated inputs ─────────────────────────
// Reverse gear, full left lock (-90° → -1.0), throttle and brake both over-
// range (should be clamped to 0..1), low speed.

#[test]
fn kartkraft_reverse_saturated_inputs() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let data = make_test_packet(3.0, 4500.0, -90.0, 2.0, -0.5, -1);
    let normalized = adapter.normalize(&data)?;
    insta::assert_yaml_snapshot!(normalized);
    Ok(())
}

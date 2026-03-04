//! Extended deep tests for the KartKraft FlatBuffers UDP telemetry adapter.
//!
//! Focuses on optional FlatBuffer subtable parsing (Motion → slip_ratio,
//! VehicleConfig → max_rpm, TrackConfig → track_id), combined subtable
//! scenarios, NaN / Inf injection at individual field offsets, and
//! FlatBuffer structural edge cases not covered by the existing deep_tests.

use racing_wheel_telemetry_kartkraft::{KartKraftAdapter, TelemetryAdapter};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ── Helpers ──────────────────────────────────────────────────────────────────

fn push_u16(buf: &mut Vec<u8>, v: u16) {
    buf.extend_from_slice(&v.to_le_bytes());
}
fn push_i32(buf: &mut Vec<u8>, v: i32) {
    buf.extend_from_slice(&v.to_le_bytes());
}
fn push_u32(buf: &mut Vec<u8>, v: u32) {
    buf.extend_from_slice(&v.to_le_bytes());
}
fn push_f32(buf: &mut Vec<u8>, v: f32) {
    buf.extend_from_slice(&v.to_le_bytes());
}

/// Build a dash-only KartKraft FlatBuffer (same layout as existing tests).
fn make_packet(
    speed: f32,
    rpm: f32,
    steer_deg: f32,
    throttle: f32,
    brake: f32,
    gear: i8,
) -> Vec<u8> {
    let mut buf: Vec<u8> = Vec::new();

    push_u32(&mut buf, 0); // root offset placeholder
    buf.extend_from_slice(b"KKFB");

    // Frame vtable (3 field slots: timestamp, motion, dash)
    let vt_frame = buf.len();
    push_u16(&mut buf, 10); // vtable_size = 4 + 3*2
    push_u16(&mut buf, 12); // object_size
    push_u16(&mut buf, 0); // field 0 (timestamp) absent
    push_u16(&mut buf, 0); // field 1 (motion) absent
    push_u16(&mut buf, 4); // field 2 (dash) at offset 4

    let frame_pos = buf.len();
    push_i32(&mut buf, (frame_pos - vt_frame) as i32);
    push_u32(&mut buf, 0); // dash UOffset placeholder
    push_u32(&mut buf, 0); // padding

    buf[0..4].copy_from_slice(&(frame_pos as u32).to_le_bytes());

    // Dashboard vtable (6 fields)
    let vt_dash = buf.len();
    push_u16(&mut buf, 16);
    push_u16(&mut buf, 28);
    push_u16(&mut buf, 4); // speed
    push_u16(&mut buf, 8); // rpm
    push_u16(&mut buf, 12); // steer
    push_u16(&mut buf, 16); // throttle
    push_u16(&mut buf, 20); // brake
    push_u16(&mut buf, 24); // gear

    let dash_pos = buf.len();
    push_i32(&mut buf, (dash_pos - vt_dash) as i32);
    push_f32(&mut buf, speed);
    push_f32(&mut buf, rpm);
    push_f32(&mut buf, steer_deg);
    push_f32(&mut buf, throttle);
    push_f32(&mut buf, brake);
    buf.push(gear as u8);
    buf.extend_from_slice(&[0; 3]);

    let ref_pos = frame_pos + 4;
    let dash_uoffset = (dash_pos - ref_pos) as u32;
    buf[ref_pos..ref_pos + 4].copy_from_slice(&dash_uoffset.to_le_bytes());

    buf
}

/// Build a KartKraft FlatBuffer with *all* optional subtables present:
/// Dashboard, Motion (traction_loss → slip_ratio), VehicleConfig (max_rpm),
/// and TrackConfig (track name string).
///
/// Frame vtable slots 0–5:
///   0: timestamp (absent)
///   1: motion   (present → UOffset at offset 4)
///   2: dash     (present → UOffset at offset 8)
///   3: unused   (absent)
///   4: vcfg     (present → UOffset at offset 12)
///   5: trkfg    (present → UOffset at offset 16)
#[allow(clippy::too_many_arguments)]
fn make_full_packet(
    speed: f32,
    rpm: f32,
    steer_deg: f32,
    throttle: f32,
    brake: f32,
    gear: i8,
    traction_loss: f32,
    max_rpm: f32,
    track_name: &str,
) -> Vec<u8> {
    let mut buf: Vec<u8> = Vec::new();

    push_u32(&mut buf, 0); // root offset placeholder
    buf.extend_from_slice(b"KKFB");

    // Frame vtable (6 field slots: 0–5)
    let vt_frame = buf.len();
    push_u16(&mut buf, 16); // vtable_size = 4 + 6*2
    push_u16(&mut buf, 24); // object_size = 4(soffset) + 4*5(UOffsets)
    push_u16(&mut buf, 0); // field 0 (timestamp) absent
    push_u16(&mut buf, 4); // field 1 (motion) at offset 4
    push_u16(&mut buf, 8); // field 2 (dash)   at offset 8
    push_u16(&mut buf, 0); // field 3 absent
    push_u16(&mut buf, 12); // field 4 (vcfg)   at offset 12
    push_u16(&mut buf, 16); // field 5 (trkfg)  at offset 16

    // Frame table
    let frame_pos = buf.len();
    push_i32(&mut buf, (frame_pos - vt_frame) as i32);
    // UOffset slots (placeholders)
    let motion_ref = buf.len();
    push_u32(&mut buf, 0);
    let dash_ref = buf.len();
    push_u32(&mut buf, 0);
    let vcfg_ref = buf.len();
    push_u32(&mut buf, 0);
    let trkfg_ref = buf.len();
    push_u32(&mut buf, 0);
    push_u32(&mut buf, 0); // padding to match object_size = 24

    buf[0..4].copy_from_slice(&(frame_pos as u32).to_le_bytes());

    // ── Dashboard subtable ───────────────────────────────────────────────
    let vt_dash = buf.len();
    push_u16(&mut buf, 16);
    push_u16(&mut buf, 28);
    push_u16(&mut buf, 4);
    push_u16(&mut buf, 8);
    push_u16(&mut buf, 12);
    push_u16(&mut buf, 16);
    push_u16(&mut buf, 20);
    push_u16(&mut buf, 24);

    let dash_pos = buf.len();
    push_i32(&mut buf, (dash_pos - vt_dash) as i32);
    push_f32(&mut buf, speed);
    push_f32(&mut buf, rpm);
    push_f32(&mut buf, steer_deg);
    push_f32(&mut buf, throttle);
    push_f32(&mut buf, brake);
    buf.push(gear as u8);
    buf.extend_from_slice(&[0; 3]);

    buf[dash_ref..dash_ref + 4].copy_from_slice(&((dash_pos - dash_ref) as u32).to_le_bytes());

    // ── Motion subtable (field 6 = traction_loss) ────────────────────────
    // vtable: 7 field slots (0–6), only field 6 present
    let vt_motion = buf.len();
    push_u16(&mut buf, 18); // vtable_size = 4 + 7*2
    push_u16(&mut buf, 32); // object_size = 4 + 7*4
    push_u16(&mut buf, 0); // field 0 absent
    push_u16(&mut buf, 0); // field 1 absent
    push_u16(&mut buf, 0); // field 2 absent
    push_u16(&mut buf, 0); // field 3 absent
    push_u16(&mut buf, 0); // field 4 absent
    push_u16(&mut buf, 0); // field 5 absent
    push_u16(&mut buf, 28); // field 6 (traction_loss) at offset 28

    let motion_pos = buf.len();
    push_i32(&mut buf, (motion_pos - vt_motion) as i32);
    // Fields 0–5 absent (zeros)
    push_f32(&mut buf, 0.0); // offset 4
    push_f32(&mut buf, 0.0); // offset 8
    push_f32(&mut buf, 0.0); // offset 12
    push_f32(&mut buf, 0.0); // offset 16
    push_f32(&mut buf, 0.0); // offset 20
    push_f32(&mut buf, 0.0); // offset 24
    push_f32(&mut buf, traction_loss); // offset 28

    buf[motion_ref..motion_ref + 4]
        .copy_from_slice(&((motion_pos - motion_ref) as u32).to_le_bytes());

    // ── VehicleConfig subtable (field 1 = rpm_max) ───────────────────────
    let vt_vcfg = buf.len();
    push_u16(&mut buf, 8); // vtable_size = 4 + 2*2
    push_u16(&mut buf, 12); // object_size = 4 + 2*4
    push_u16(&mut buf, 0); // field 0 absent
    push_u16(&mut buf, 8); // field 1 (rpm_max) at offset 8

    let vcfg_pos = buf.len();
    push_i32(&mut buf, (vcfg_pos - vt_vcfg) as i32);
    push_f32(&mut buf, 0.0); // field 0 placeholder
    push_f32(&mut buf, max_rpm); // field 1

    buf[vcfg_ref..vcfg_ref + 4].copy_from_slice(&((vcfg_pos - vcfg_ref) as u32).to_le_bytes());

    // ── TrackConfig subtable (field 0 = name string) ─────────────────────
    let vt_trkfg = buf.len();
    push_u16(&mut buf, 6); // vtable_size = 4 + 1*2
    push_u16(&mut buf, 8); // object_size = 4 + 1*4
    push_u16(&mut buf, 4); // field 0 (name) at offset 4

    let trkfg_pos = buf.len();
    push_i32(&mut buf, (trkfg_pos - vt_trkfg) as i32);
    let name_ref_pos = buf.len();
    push_u32(&mut buf, 0); // name UOffset placeholder

    buf[trkfg_ref..trkfg_ref + 4].copy_from_slice(&((trkfg_pos - trkfg_ref) as u32).to_le_bytes());

    // String data: [u32 length][utf8 bytes][null terminator]
    let str_pos = buf.len();
    push_u32(&mut buf, track_name.len() as u32);
    buf.extend_from_slice(track_name.as_bytes());
    buf.push(0); // null terminator
    // pad to 4-byte alignment
    while !buf.len().is_multiple_of(4) {
        buf.push(0);
    }

    buf[name_ref_pos..name_ref_pos + 4]
        .copy_from_slice(&((str_pos - name_ref_pos) as u32).to_le_bytes());

    buf
}

// ═══════════════════════════════════════════════════════════════════════════════
// Optional subtable: Motion → slip_ratio from traction_loss
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn motion_traction_loss_maps_to_slip_ratio() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let data = make_full_packet(20.0, 8000.0, 0.0, 0.7, 0.0, 2, 0.4, 15000.0, "TestTrack");
    let t = adapter.normalize(&data)?;
    // slip_ratio = abs(traction_loss).clamp(0, 1) = 0.4
    assert!(
        (t.slip_ratio - 0.4).abs() < 0.001,
        "slip_ratio={}",
        t.slip_ratio
    );
    Ok(())
}

#[test]
fn motion_traction_loss_negative_uses_abs() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let data = make_full_packet(20.0, 8000.0, 0.0, 0.7, 0.0, 2, -0.6, 15000.0, "T");
    let t = adapter.normalize(&data)?;
    assert!((t.slip_ratio - 0.6).abs() < 0.001, "abs(-0.6)=0.6");
    Ok(())
}

#[test]
fn motion_traction_loss_clamped_to_one() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let data = make_full_packet(20.0, 8000.0, 0.0, 0.7, 0.0, 2, 3.0, 15000.0, "T");
    let t = adapter.normalize(&data)?;
    assert!(
        (t.slip_ratio - 1.0).abs() < 0.001,
        "clamped to 1.0, got {}",
        t.slip_ratio
    );
    Ok(())
}

#[test]
fn motion_traction_loss_zero() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let data = make_full_packet(20.0, 8000.0, 0.0, 0.7, 0.0, 2, 0.0, 15000.0, "T");
    let t = adapter.normalize(&data)?;
    assert_eq!(t.slip_ratio, 0.0);
    Ok(())
}

#[test]
fn motion_absent_slip_ratio_defaults_zero() -> TestResult {
    let adapter = KartKraftAdapter::new();
    // make_packet() has no Motion subtable
    let data = make_packet(20.0, 8000.0, 0.0, 0.7, 0.0, 2);
    let t = adapter.normalize(&data)?;
    assert_eq!(t.slip_ratio, 0.0, "no motion → slip_ratio=0");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Optional subtable: VehicleConfig → max_rpm
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn vcfg_max_rpm_extracted() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let data = make_full_packet(10.0, 5000.0, 0.0, 0.5, 0.0, 1, 0.0, 14000.0, "T");
    let t = adapter.normalize(&data)?;
    assert!((t.max_rpm - 14000.0).abs() < 0.1, "max_rpm={}", t.max_rpm);
    Ok(())
}

#[test]
fn vcfg_max_rpm_zero_treated_as_absent() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let data = make_full_packet(10.0, 5000.0, 0.0, 0.5, 0.0, 1, 0.0, 0.0, "T");
    let t = adapter.normalize(&data)?;
    // When max_rpm=0 the builder skips setting it, so it remains default
    assert_eq!(t.max_rpm, 0.0);
    Ok(())
}

#[test]
fn vcfg_absent_max_rpm_defaults_zero() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let data = make_packet(10.0, 5000.0, 0.0, 0.5, 0.0, 1);
    let t = adapter.normalize(&data)?;
    assert_eq!(t.max_rpm, 0.0, "no vcfg → max_rpm=0");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Optional subtable: TrackConfig → track_id
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn trkfg_track_name_extracted() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let data = make_full_packet(10.0, 5000.0, 0.0, 0.5, 0.0, 1, 0.0, 14000.0, "Rye House");
    let t = adapter.normalize(&data)?;
    assert_eq!(t.track_id.as_deref(), Some("Rye House"));
    Ok(())
}

#[test]
fn trkfg_empty_name() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let data = make_full_packet(10.0, 5000.0, 0.0, 0.5, 0.0, 1, 0.0, 14000.0, "");
    let t = adapter.normalize(&data)?;
    // An empty FlatBuffer string resolves to None (zero-length string not preserved)
    assert!(t.track_id.is_none() || t.track_id.as_deref() == Some(""));
    Ok(())
}

#[test]
fn trkfg_absent_track_id_none() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let data = make_packet(10.0, 5000.0, 0.0, 0.5, 0.0, 1);
    let t = adapter.normalize(&data)?;
    assert!(t.track_id.is_none(), "no trkfg → track_id=None");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Combined scenario with all subtables
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn full_packet_all_subtables() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let data = make_full_packet(
        28.0,    // speed m/s
        11500.0, // rpm
        -30.0,   // steer degrees
        0.92,    // throttle
        0.0,     // brake
        3,       // gear
        0.25,    // traction_loss
        13500.0, // max_rpm
        "Buckmore Park",
    );
    let t = adapter.normalize(&data)?;

    assert!((t.speed_ms - 28.0).abs() < 0.001);
    assert!((t.rpm - 11500.0).abs() < 0.1);
    assert_eq!(t.gear, 3);
    // steer: -30°/90° = -0.333…
    assert!((t.steering_angle - (-30.0 / 90.0)).abs() < 0.001);
    assert!((t.throttle - 0.92).abs() < 0.001);
    assert_eq!(t.brake, 0.0);
    // slip_ratio = abs(0.25) = 0.25
    assert!((t.slip_ratio - 0.25).abs() < 0.001);
    assert!((t.max_rpm - 13500.0).abs() < 0.1);
    assert_eq!(t.track_id.as_deref(), Some("Buckmore Park"));
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// NaN injection at individual dashboard field offsets
// ═══════════════════════════════════════════════════════════════════════════════

/// Dashboard field byte offsets in a make_packet() buffer.
/// Layout: root(4)+KKFB(4)+frame_vt(10)+frame_tbl(12)+dash_vt(16)+dash_soff(4)
/// Speed starts at offset 50 (= 4+4+10+12+16+4).
const DASH_SPEED_OFF: usize = 50;
const DASH_RPM_OFF: usize = 54;
const DASH_STEER_OFF: usize = 58;
const DASH_THROTTLE_OFF: usize = 62;
const DASH_BRAKE_OFF: usize = 66;

fn write_nan(buf: &mut [u8], off: usize) {
    buf[off..off + 4].copy_from_slice(&f32::NAN.to_le_bytes());
}

fn write_inf(buf: &mut [u8], off: usize) {
    buf[off..off + 4].copy_from_slice(&f32::INFINITY.to_le_bytes());
}

#[test]
fn nan_speed_defaults_to_zero() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let mut data = make_packet(100.0, 5000.0, 0.0, 0.5, 0.0, 1);
    write_nan(&mut data, DASH_SPEED_OFF);
    let t = adapter.normalize(&data)?;
    assert!(t.speed_ms.is_finite());
    assert_eq!(t.speed_ms, 0.0, "NaN speed → 0.0");
    Ok(())
}

#[test]
fn nan_rpm_defaults_to_zero() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let mut data = make_packet(10.0, 5000.0, 0.0, 0.5, 0.0, 1);
    write_nan(&mut data, DASH_RPM_OFF);
    let t = adapter.normalize(&data)?;
    assert!(t.rpm.is_finite());
    assert_eq!(t.rpm, 0.0, "NaN rpm → 0.0");
    Ok(())
}

#[test]
fn nan_steer_defaults_to_zero() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let mut data = make_packet(10.0, 5000.0, 45.0, 0.5, 0.0, 1);
    write_nan(&mut data, DASH_STEER_OFF);
    let t = adapter.normalize(&data)?;
    assert!(t.steering_angle.is_finite());
    assert_eq!(t.steering_angle, 0.0, "NaN steer → 0.0");
    Ok(())
}

#[test]
fn nan_throttle_defaults_to_zero() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let mut data = make_packet(10.0, 5000.0, 0.0, 0.9, 0.0, 1);
    write_nan(&mut data, DASH_THROTTLE_OFF);
    let t = adapter.normalize(&data)?;
    assert!(t.throttle.is_finite());
    assert_eq!(t.throttle, 0.0, "NaN throttle → 0.0");
    Ok(())
}

#[test]
fn nan_brake_defaults_to_zero() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let mut data = make_packet(10.0, 5000.0, 0.0, 0.0, 0.8, 1);
    write_nan(&mut data, DASH_BRAKE_OFF);
    let t = adapter.normalize(&data)?;
    assert!(t.brake.is_finite());
    assert_eq!(t.brake, 0.0, "NaN brake → 0.0");
    Ok(())
}

#[test]
fn inf_speed_clamped_nonneg() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let mut data = make_packet(10.0, 5000.0, 0.0, 0.5, 0.0, 1);
    write_inf(&mut data, DASH_SPEED_OFF);
    let t = adapter.normalize(&data)?;
    // Inf is not finite → read_f32_le returns None → default 0.0
    assert!(t.speed_ms.is_finite());
    Ok(())
}

#[test]
fn neg_inf_steer_clamped() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let mut data = make_packet(10.0, 5000.0, 0.0, 0.5, 0.0, 1);
    data[DASH_STEER_OFF..DASH_STEER_OFF + 4].copy_from_slice(&f32::NEG_INFINITY.to_le_bytes());
    let t = adapter.normalize(&data)?;
    assert!(t.steering_angle.is_finite());
    assert_eq!(t.steering_angle, 0.0, "NegInf steer → default 0.0");
    Ok(())
}

#[test]
fn all_nan_fields_still_produce_valid_output() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let mut data = make_packet(0.0, 0.0, 0.0, 0.0, 0.0, 0);
    for off in [
        DASH_SPEED_OFF,
        DASH_RPM_OFF,
        DASH_STEER_OFF,
        DASH_THROTTLE_OFF,
        DASH_BRAKE_OFF,
    ] {
        write_nan(&mut data, off);
    }
    let t = adapter.normalize(&data)?;
    assert!(t.speed_ms.is_finite());
    assert!(t.rpm.is_finite());
    assert!(t.steering_angle.is_finite());
    assert!(t.throttle.is_finite());
    assert!(t.brake.is_finite());
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Intermediate steering angles (fractional degree values)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn steer_one_degree() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let data = make_packet(0.0, 0.0, 1.0, 0.0, 0.0, 0);
    let t = adapter.normalize(&data)?;
    assert!((t.steering_angle - (1.0 / 90.0)).abs() < 0.0001, "1°/90°");
    Ok(())
}

#[test]
fn steer_60_degrees() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let data = make_packet(0.0, 0.0, 60.0, 0.0, 0.0, 0);
    let t = adapter.normalize(&data)?;
    assert!((t.steering_angle - (60.0 / 90.0)).abs() < 0.001, "60°/90°");
    Ok(())
}

#[test]
fn steer_neg_15_degrees() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let data = make_packet(0.0, 0.0, -15.0, 0.0, 0.0, 0);
    let t = adapter.normalize(&data)?;
    assert!((t.steering_angle - (-15.0 / 90.0)).abs() < 0.001);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Adapter clone and identity
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn adapter_clone_preserves_identity() -> TestResult {
    let adapter = KartKraftAdapter::new().with_port(7777);
    let cloned = adapter.clone();
    assert_eq!(adapter.game_id(), cloned.game_id());
    assert_eq!(
        adapter.expected_update_rate(),
        cloned.expected_update_rate()
    );
    Ok(())
}

#[tokio::test]
async fn async_is_game_running_false_by_default() -> TestResult {
    let adapter = KartKraftAdapter::new();
    assert!(!adapter.is_game_running().await?);
    Ok(())
}

#[tokio::test]
async fn async_stop_monitoring_succeeds() -> TestResult {
    let adapter = KartKraftAdapter::new();
    adapter.stop_monitoring().await?;
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenarios
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn scenario_hot_lap_with_traction_loss() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let data = make_full_packet(
        30.0,    // speed
        12500.0, // rpm
        -40.0,   // steer
        1.0,     // throttle
        0.0,     // brake
        3,       // gear
        0.55,    // traction_loss → slip_ratio
        14000.0, // max_rpm
        "PFi International",
    );
    let t = adapter.normalize(&data)?;

    assert!((t.speed_ms - 30.0).abs() < 0.001);
    assert!((t.rpm - 12500.0).abs() < 0.1);
    assert_eq!(t.gear, 3);
    assert!((t.throttle - 1.0).abs() < 0.001);
    assert!((t.slip_ratio - 0.55).abs() < 0.001);
    assert!((t.max_rpm - 14000.0).abs() < 0.1);
    assert_eq!(t.track_id.as_deref(), Some("PFi International"));
    Ok(())
}

#[test]
fn scenario_grid_idle() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let data = make_full_packet(0.0, 1800.0, 0.0, 0.0, 0.0, 0, 0.0, 13000.0, "Shenington");
    let t = adapter.normalize(&data)?;

    assert_eq!(t.speed_ms, 0.0);
    assert!((t.rpm - 1800.0).abs() < 0.1);
    assert_eq!(t.gear, 0);
    assert_eq!(t.slip_ratio, 0.0);
    assert!((t.max_rpm - 13000.0).abs() < 0.1);
    Ok(())
}

#[test]
fn scenario_braking_into_hairpin() -> TestResult {
    let adapter = KartKraftAdapter::new();
    let data = make_full_packet(
        12.0,   // slowing
        6000.0, // low rpm
        70.0,   // heavy lock → clamped to 1.0
        0.0,    // off throttle
        0.85,   // heavy brake
        1,      // downshifted
        0.15,   // moderate slip
        13000.0, "PFi",
    );
    let t = adapter.normalize(&data)?;

    assert!((t.speed_ms - 12.0).abs() < 0.001);
    assert_eq!(t.gear, 1);
    assert_eq!(t.throttle, 0.0);
    assert!((t.brake - 0.85).abs() < 0.001);
    // 70°/90° = 0.778 (within range, not clamped)
    assert!((t.steering_angle - (70.0 / 90.0)).abs() < 0.001);
    assert!((t.slip_ratio - 0.15).abs() < 0.001);
    Ok(())
}

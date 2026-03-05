//! Device protocol parsing snapshot tests for Fanatec HID protocol.
//!
//! These tests pin the exact parsed output from known-good byte sequences,
//! ensuring that any change to the parsing logic is caught by insta diffs.

use insta::assert_snapshot;
use racing_wheel_hid_fanatec_protocol::{
    self as fan, FANATEC_VENDOR_ID, FanatecModel, FanatecPedalModel, FanatecRimId,
    parse_extended_report, parse_pedal_report, parse_standard_report, product_ids, rim_ids,
};

// ── Standard input report parsing ────────────────────────────────────────────

/// Known-good byte sequence: centered steering, pedals released, hat neutral,
/// McLaren GT3 V2 rim attached (byte 0x1F = 0x0B).
#[test]
fn snap_parse_standard_center_mclaren_rim() -> Result<(), String> {
    let mut data = [0u8; 64];
    data[0] = 0x01; // report ID
    data[1] = 0x00;
    data[2] = 0x80; // steering center (0x8000 LE)
    data[3] = 0xFF; // throttle released (inverted)
    data[4] = 0xFF; // brake released
    data[5] = 0xFF; // clutch released
    // data[6] = 0x00 (padding)
    data[7] = 0x00;
    data[8] = 0x00; // buttons = 0
    data[9] = 0x0F; // hat neutral
    data[10] = 0x00; // funky center
    // rotary1 = 0, rotary2 = 0
    data[15] = 0xFF; // left clutch paddle released
    data[16] = 0xFF; // right clutch paddle released
    data[0x1F] = rim_ids::MCLAREN_GT3_V2; // rim ID byte

    let state = parse_standard_report(&data).ok_or("parse_standard_report returned None")?;
    assert_snapshot!(format!(
        "steering={:.4}, throttle={:.4}, brake={:.4}, clutch={:.4}, \
         buttons=0x{:04X}, hat=0x{:X}, funky={}, rotary1={}, rotary2={}, \
         clutch_left={:.4}, clutch_right={:.4}",
        state.steering,
        state.throttle,
        state.brake,
        state.clutch,
        state.buttons,
        state.hat,
        state.funky_dir,
        state.rotary1,
        state.rotary2,
        state.clutch_left,
        state.clutch_right,
    ));
    Ok(())
}

/// Known-good: full left steering, full throttle, half brake, buttons pressed.
#[test]
fn snap_parse_standard_full_left_mixed_pedals() -> Result<(), String> {
    let mut data = [0u8; 64];
    data[0] = 0x01; // report ID
    data[1] = 0x00;
    data[2] = 0x00; // steering full left (0x0000)
    data[3] = 0x00; // throttle fully pressed (inverted: 0x00 = 1.0)
    data[4] = 0x80; // brake half pressed (inverted: 0x80 ≈ 0.498)
    data[5] = 0xFF; // clutch released
    data[7] = 0b00000101;
    data[8] = 0b00000010; // buttons: 0, 2, 9
    data[9] = 0x02; // hat = right

    let state = parse_standard_report(&data).ok_or("parse_standard_report returned None")?;
    assert_snapshot!(format!(
        "steering={:.4}, throttle={:.4}, brake={:.4}, clutch={:.4}, \
         buttons=0x{:04X}, hat=0x{:X}",
        state.steering, state.throttle, state.brake, state.clutch, state.buttons, state.hat,
    ));
    Ok(())
}

/// Known-good: full right steering, all pedals full, all buttons pressed.
#[test]
fn snap_parse_standard_full_right_all_pressed() -> Result<(), String> {
    let mut data = [0u8; 64];
    data[0] = 0x01;
    data[1] = 0xFF;
    data[2] = 0xFF; // steering full right (0xFFFF)
    data[3] = 0x00; // throttle full
    data[4] = 0x00; // brake full
    data[5] = 0x00; // clutch full
    data[7] = 0xFF;
    data[8] = 0xFF; // all 16 buttons
    data[9] = 0x00; // hat = up
    data[10] = 0x03; // funky = down
    data[11] = 0xD2;
    data[12] = 0x04; // rotary1 = 1234
    data[13] = 0x39;
    data[14] = 0x05; // rotary2 = 1337
    data[15] = 0x00; // left clutch full
    data[16] = 0x00; // right clutch full

    let state = parse_standard_report(&data).ok_or("parse_standard_report returned None")?;
    assert_snapshot!(format!(
        "steering={:.4}, throttle={:.4}, brake={:.4}, clutch={:.4}, \
         buttons=0x{:04X}, hat=0x{:X}, funky={}, rotary1={}, rotary2={}, \
         clutch_left={:.4}, clutch_right={:.4}",
        state.steering,
        state.throttle,
        state.brake,
        state.clutch,
        state.buttons,
        state.hat,
        state.funky_dir,
        state.rotary1,
        state.rotary2,
        state.clutch_left,
        state.clutch_right,
    ));
    Ok(())
}

/// Minimal 10-byte report — rim extension fields should default to zero.
#[test]
fn snap_parse_standard_minimal_10_bytes() -> Result<(), String> {
    let mut data = [0u8; 10];
    data[0] = 0x01;
    data[1] = 0x00;
    data[2] = 0x80; // center
    data[3] = 0xFF; // throttle released
    data[4] = 0xFF; // brake released
    data[5] = 0xFF; // clutch released
    data[9] = 0x0F; // hat neutral

    let state = parse_standard_report(&data).ok_or("parse_standard_report returned None")?;
    assert_snapshot!(format!(
        "steering={:.4}, funky={}, rotary1={}, rotary2={}, clutch_left={:.4}, clutch_right={:.4}",
        state.steering,
        state.funky_dir,
        state.rotary1,
        state.rotary2,
        state.clutch_left,
        state.clutch_right,
    ));
    Ok(())
}

// ── Extended telemetry report parsing ────────────────────────────────────────

/// Known-good extended report: DD2 under load, 72°C motor, fault-free.
#[test]
fn snap_parse_extended_dd2_normal() -> Result<(), String> {
    let mut data = [0u8; 64];
    data[0] = 0x02; // extended report ID
    // steering_raw = 1024 (0x0400 LE)
    data[1] = 0x00;
    data[2] = 0x04;
    // steering_velocity = -500 (0xFE0C LE)
    data[3] = 0x0C;
    data[4] = 0xFE;
    data[5] = 72; // motor_temp_c
    data[6] = 38; // board_temp_c
    data[7] = 150; // current_raw (15.0 A)
    data[10] = 0x00; // no faults

    let state = parse_extended_report(&data).ok_or("parse_extended_report returned None")?;
    assert_snapshot!(format!(
        "steering_raw={}, velocity={}, motor_temp={}C, board_temp={}C, \
         current_raw={}, faults=0b{:04b}",
        state.steering_raw,
        state.steering_velocity,
        state.motor_temp_c,
        state.board_temp_c,
        state.current_raw,
        state.fault_flags,
    ));
    Ok(())
}

/// Extended report: over-temperature and over-current faults active.
#[test]
fn snap_parse_extended_faulted() -> Result<(), String> {
    let mut data = [0u8; 64];
    data[0] = 0x02;
    data[1] = 0x00;
    data[2] = 0x00; // steering_raw = 0
    data[3] = 0x00;
    data[4] = 0x00;
    data[5] = 95; // motor_temp_c (over-temp)
    data[6] = 82; // board_temp_c
    data[7] = 255; // current_raw (max)
    data[10] = 0b0011; // over-temp + over-current

    let state = parse_extended_report(&data).ok_or("parse_extended_report returned None")?;
    assert_snapshot!(format!(
        "steering_raw={}, motor_temp={}C, board_temp={}C, current_raw={}, faults=0b{:04b}",
        state.steering_raw,
        state.motor_temp_c,
        state.board_temp_c,
        state.current_raw,
        state.fault_flags,
    ));
    Ok(())
}

/// Extended report: all fault flags set.
#[test]
fn snap_parse_extended_all_faults() -> Result<(), String> {
    let mut data = [0u8; 64];
    data[0] = 0x02;
    data[5] = 100;
    data[6] = 90;
    data[7] = 200;
    data[10] = 0b1111; // all 4 faults

    let state = parse_extended_report(&data).ok_or("parse_extended_report returned None")?;
    assert_snapshot!(format!(
        "motor_temp={}C, board_temp={}C, faults=0b{:04b}",
        state.motor_temp_c, state.board_temp_c, state.fault_flags,
    ));
    Ok(())
}

// ── Pedal standalone report parsing ──────────────────────────────────────────

/// Known-good 3-axis pedal report: V3 pedals with load cell brake.
#[test]
fn snap_parse_pedal_v3_three_axes() -> Result<(), String> {
    let data: [u8; 7] = [
        0x01, // report ID
        0xFF, 0x07, // throttle = 0x07FF (2047, near max 12-bit)
        0x00, 0x04, // brake = 0x0400 (1024, mid-range)
        0x00, 0x00, // clutch = 0 (released)
    ];
    let state = parse_pedal_report(&data).ok_or("parse_pedal_report returned None")?;
    assert_snapshot!(format!(
        "throttle_raw={}, brake_raw={}, clutch_raw={}, axis_count={}",
        state.throttle_raw, state.brake_raw, state.clutch_raw, state.axis_count,
    ));
    Ok(())
}

/// Known-good 2-axis pedal report: CSL Elite Pedals (no clutch).
#[test]
fn snap_parse_pedal_2_axis_no_clutch() -> Result<(), String> {
    let data: [u8; 5] = [
        0x01, // report ID
        0xFF, 0x0F, // throttle = 0x0FFF (4095, max 12-bit)
        0xFF, 0x0F, // brake = 0x0FFF (max)
    ];
    let state = parse_pedal_report(&data).ok_or("parse_pedal_report returned None")?;
    assert_snapshot!(format!(
        "throttle_raw={}, brake_raw={}, clutch_raw={}, axis_count={}",
        state.throttle_raw, state.brake_raw, state.clutch_raw, state.axis_count,
    ));
    Ok(())
}

/// Pedal report: all axes fully released (zero Hall sensor values).
#[test]
fn snap_parse_pedal_all_released() -> Result<(), String> {
    let data: [u8; 7] = [
        0x01, 0x00, 0x00, // throttle = 0
        0x00, 0x00, // brake = 0
        0x00, 0x00, // clutch = 0
    ];
    let state = parse_pedal_report(&data).ok_or("parse_pedal_report returned None")?;
    assert_snapshot!(format!(
        "throttle_raw={}, brake_raw={}, clutch_raw={}, axis_count={}",
        state.throttle_raw, state.brake_raw, state.clutch_raw, state.axis_count,
    ));
    Ok(())
}

/// Verify 12-bit masking: upper nibble bits should be stripped.
#[test]
fn snap_parse_pedal_12bit_masking() -> Result<(), String> {
    let data: [u8; 7] = [
        0x01, 0xFF, 0xFF, // 0xFFFF → masked to 0x0FFF = 4095
        0xAB, 0xCD, // 0xCDAB → masked to 0x0DAB = 3499
        0x12, 0xF3, // 0xF312 → masked to 0x0312 = 786
    ];
    let state = parse_pedal_report(&data).ok_or("parse_pedal_report returned None")?;
    assert_snapshot!(format!(
        "throttle_raw={}, brake_raw={}, clutch_raw={}",
        state.throttle_raw, state.brake_raw, state.clutch_raw,
    ));
    Ok(())
}

// ── Protocol version detection via PID ───────────────────────────────────────

/// Pin V1/V2 protocol detection: DD wheelbases use high-res (16-bit) encoding,
/// older belt-driven bases use low-res (8-bit).
#[test]
fn snap_protocol_version_highres_detection() {
    let pids_and_names: &[(u16, &str)] = &[
        (product_ids::DD1, "DD1"),
        (product_ids::DD2, "DD2"),
        (product_ids::CSL_DD, "CSL DD"),
        (product_ids::GT_DD_PRO, "GT DD Pro"),
        (product_ids::CLUBSPORT_DD, "ClubSport DD"),
        (product_ids::CLUBSPORT_V2, "ClubSport V2"),
        (product_ids::CLUBSPORT_V2_5, "ClubSport V2.5"),
        (product_ids::CSL_ELITE, "CSL Elite"),
        (product_ids::CSL_ELITE_PS4, "CSL Elite PS4"),
        (product_ids::CSR_ELITE, "CSR Elite"),
    ];
    let mut lines = Vec::new();
    for &(pid, name) in pids_and_names {
        let model = FanatecModel::from_product_id(pid);
        lines.push(format!(
            "0x{pid:04X} {name}: highres={}, sign_fix={}",
            model.is_highres(),
            model.needs_sign_fix(),
        ));
    }
    assert_snapshot!(lines.join("\n"));
}

// ── VID/PID to vendor mapping consistency ────────────────────────────────────

/// Pin the Fanatec VID constant and verify all known PIDs map to expected categories.
#[test]
fn snap_vid_pid_category_matrix() {
    let mut lines = Vec::new();
    lines.push(format!("VENDOR_ID=0x{FANATEC_VENDOR_ID:04X}"));

    let all_pids: &[(u16, &str)] = &[
        (product_ids::CLUBSPORT_V2, "ClubSport V2"),
        (product_ids::CLUBSPORT_V2_5, "ClubSport V2.5"),
        (product_ids::CSL_ELITE_PS4, "CSL Elite PS4"),
        (product_ids::DD1, "DD1"),
        (product_ids::DD2, "DD2"),
        (product_ids::CSR_ELITE, "CSR Elite"),
        (product_ids::CSL_DD, "CSL DD"),
        (product_ids::GT_DD_PRO, "GT DD Pro"),
        (product_ids::CLUBSPORT_DD, "ClubSport DD"),
        (product_ids::CSL_ELITE, "CSL Elite"),
        (product_ids::CLUBSPORT_PEDALS_V1_V2, "Pedals V1/V2"),
        (product_ids::CLUBSPORT_PEDALS_V3, "Pedals V3"),
        (product_ids::CSL_ELITE_PEDALS, "CSL Elite Pedals"),
        (product_ids::CSL_PEDALS_LC, "CSL Pedals LC"),
        (product_ids::CSL_PEDALS_V2, "CSL Pedals V2"),
        (product_ids::CLUBSPORT_SHIFTER, "Shifter"),
        (product_ids::CLUBSPORT_HANDBRAKE, "Handbrake"),
    ];
    for &(pid, name) in all_pids {
        let is_wb = fan::is_wheelbase_product(pid);
        let is_ped = fan::is_pedal_product(pid);
        let category = if is_wb {
            "wheelbase"
        } else if is_ped {
            "pedals"
        } else {
            "other"
        };
        lines.push(format!("0x{pid:04X} {name}: {category}"));
    }
    assert_snapshot!(lines.join("\n"));
}

// ── Device capability detection from rim IDs ─────────────────────────────────

/// Pin capabilities detected for each known rim ID byte value.
#[test]
fn snap_rim_capability_detection() {
    let all_rims: &[(u8, &str)] = &[
        (rim_ids::BMW_GT2, "BMW GT2"),
        (rim_ids::FORMULA_V2, "Formula V2"),
        (rim_ids::FORMULA_V2_5, "Formula V2.5"),
        (rim_ids::CSL_ELITE_P1, "CSL Elite P1"),
        (rim_ids::MCLAREN_GT3_V2, "McLaren GT3 V2"),
        (rim_ids::PORSCHE_911_GT3_R, "Porsche 911 GT3 R"),
        (rim_ids::PORSCHE_918_RSR, "Porsche 918 RSR"),
        (rim_ids::CLUBSPORT_RS, "ClubSport RS"),
        (rim_ids::WRC, "WRC"),
        (rim_ids::PODIUM_HUB, "Podium Hub"),
        (0xFF, "Unknown (0xFF)"),
        (0x00, "Unknown (0x00)"),
    ];
    let mut lines = Vec::new();
    for &(byte, name) in all_rims {
        let rim = FanatecRimId::from_byte(byte);
        lines.push(format!(
            "0x{byte:02X} {name}: variant={rim:?}, funky={}, dual_clutch={}, rotary={}",
            rim.has_funky_switch(),
            rim.has_dual_clutch(),
            rim.has_rotary_encoders(),
        ));
    }
    assert_snapshot!(lines.join("\n"));
}

/// Pin wheelbase model capabilities for every known PID.
#[test]
fn snap_wheelbase_capability_matrix() {
    let wheelbases: &[(u16, &str)] = &[
        (product_ids::DD1, "DD1"),
        (product_ids::DD2, "DD2"),
        (product_ids::CSL_DD, "CSL DD"),
        (product_ids::GT_DD_PRO, "GT DD Pro"),
        (product_ids::CLUBSPORT_DD, "ClubSport DD"),
        (product_ids::CLUBSPORT_V2, "ClubSport V2"),
        (product_ids::CLUBSPORT_V2_5, "ClubSport V2.5"),
        (product_ids::CSL_ELITE, "CSL Elite"),
        (product_ids::CSL_ELITE_PS4, "CSL Elite PS4"),
        (product_ids::CSR_ELITE, "CSR Elite"),
    ];
    let mut lines = Vec::new();
    for &(pid, name) in wheelbases {
        let m = FanatecModel::from_product_id(pid);
        lines.push(format!(
            "{name}: torque={:.1}Nm, cpr={}, 1kHz={}, max_rot={}°, highres={}, sign_fix={}",
            m.max_torque_nm(),
            m.encoder_cpr(),
            m.supports_1000hz(),
            m.max_rotation_degrees(),
            m.is_highres(),
            m.needs_sign_fix(),
        ));
    }
    assert_snapshot!(lines.join("\n"));
}

/// Pin pedal model capabilities for every known pedal PID.
#[test]
fn snap_pedal_capability_matrix() {
    let pedals: &[(u16, &str)] = &[
        (product_ids::CLUBSPORT_PEDALS_V1_V2, "ClubSport V1/V2"),
        (product_ids::CLUBSPORT_PEDALS_V3, "ClubSport V3"),
        (product_ids::CSL_ELITE_PEDALS, "CSL Elite Pedals"),
        (product_ids::CSL_PEDALS_LC, "CSL Pedals LC"),
        (product_ids::CSL_PEDALS_V2, "CSL Pedals V2"),
    ];
    let mut lines = Vec::new();
    for &(pid, name) in pedals {
        let m = FanatecPedalModel::from_product_id(pid);
        lines.push(format!("{name}: model={m:?}, axes={}", m.axis_count()));
    }
    assert_snapshot!(lines.join("\n"));
}

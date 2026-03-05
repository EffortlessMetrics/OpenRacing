//! Device protocol parsing snapshot tests for Simagic HID protocol.
//!
//! These tests pin the exact parsed output from known-good byte sequences,
//! covering firmware version detection, all shifter gears, extended pedal
//! scenarios, and device capability matrices.

use insta::assert_snapshot;
use racing_wheel_hid_simagic_protocol::{
    self as simagic, SimagicModel, identify_device, product_ids,
};

// ── Firmware version detection from byte sequences ───────────────────────────

/// Known-good 23+ byte report with firmware version embedded at bytes 20-22.
#[test]
fn snap_parse_firmware_version_1_5_3() -> Result<(), String> {
    let mut data = vec![0u8; 64];
    data[0] = 0x00;
    data[1] = 0x80; // steering center
    data[20] = 1;
    data[21] = 5;
    data[22] = 3;
    let state = simagic::parse_input_report(&data).ok_or("parse_input_report returned None")?;
    assert_snapshot!(format!(
        "firmware={:?}, steering={:.4}",
        state.firmware_version, state.steering,
    ));
    Ok(())
}

/// Short report (17 bytes) — firmware version should be None.
#[test]
fn snap_parse_no_firmware_short_report() -> Result<(), String> {
    let data = vec![0u8; 17];
    let state = simagic::parse_input_report(&data).ok_or("parse_input_report returned None")?;
    assert_snapshot!(format!("firmware={:?}", state.firmware_version));
    Ok(())
}

// ── Shifter gear position snapshots ──────────────────────────────────────────

/// Pin all gear positions: neutral through 8th + unknown.
#[test]
fn snap_parse_all_gears() -> Result<(), String> {
    let gear_bytes: &[(u8, &str)] = &[
        (0, "Neutral"),
        (1, "First"),
        (2, "Second"),
        (3, "Third"),
        (4, "Fourth"),
        (5, "Fifth"),
        (6, "Sixth"),
        (7, "Seventh"),
        (8, "Eighth"),
        (0xFF, "Unknown(0xFF)"),
        (9, "Unknown(9)"),
    ];
    let mut lines = Vec::new();
    for &(byte, label) in gear_bytes {
        let mut data = vec![0u8; 64];
        data[15] = byte;
        let state = simagic::parse_input_report(&data).ok_or("parse_input_report returned None")?;
        lines.push(format!(
            "byte=0x{byte:02X} {label}: gear={:?}",
            state.shifter.gear
        ));
    }
    assert_snapshot!(lines.join("\n"));
    Ok(())
}

// ── Shifter flags combinations ───────────────────────────────────────────────

/// Pin all flag bit combinations (3 bits → 8 combinations).
#[test]
fn snap_parse_shifter_flag_combos() -> Result<(), String> {
    let mut lines = Vec::new();
    for flags in 0u8..=7 {
        let mut data = vec![0u8; 64];
        data[16] = flags;
        let state = simagic::parse_input_report(&data).ok_or("parse_input_report returned None")?;
        lines.push(format!(
            "flags=0b{flags:03b}: clutch_in_range={}, seq_up={}, seq_down={}",
            state.shifter.clutch_in_range,
            state.shifter.sequential_up_pressed,
            state.shifter.sequential_down_pressed,
        ));
    }
    assert_snapshot!(lines.join("\n"));
    Ok(())
}

// ── Mixed pedal + steering scenarios ─────────────────────────────────────────

/// Half-press on all axes simultaneously.
#[test]
fn snap_parse_half_press_all_axes() -> Result<(), String> {
    let mut data = vec![0u8; 64];
    // steering quarter right: 0x8000 + 0x4000 = 0xC000
    data[0] = 0x00;
    data[1] = 0xC0;
    // throttle half: 0x8000
    data[2] = 0x00;
    data[3] = 0x80;
    // brake half: 0x8000
    data[4] = 0x00;
    data[5] = 0x80;
    // clutch half: 0x8000
    data[6] = 0x00;
    data[7] = 0x80;
    // handbrake half: 0x8000
    data[8] = 0x00;
    data[9] = 0x80;
    data[10] = 0xFF;
    data[11] = 0xFF; // all buttons

    let state = simagic::parse_input_report(&data).ok_or("parse_input_report returned None")?;
    assert_snapshot!(format!(
        "steering={:.4}, throttle={:.4}, brake={:.4}, clutch={:.4}, \
         handbrake={:.4}, buttons=0x{:04X}",
        state.steering, state.throttle, state.brake, state.clutch, state.handbrake, state.buttons,
    ));
    Ok(())
}

// ── Quick release status snapshots ───────────────────────────────────────────

/// Pin all quick release statuses.
#[test]
fn snap_parse_quick_release_states() -> Result<(), String> {
    let mut lines = Vec::new();
    for &(byte, label) in &[
        (0u8, "Attached"),
        (1, "Detached"),
        (2, "Unknown(2)"),
        (0xFF, "Unknown(0xFF)"),
    ] {
        let mut data = vec![0u8; 64];
        data[19] = byte;
        let state = simagic::parse_input_report(&data).ok_or("parse_input_report returned None")?;
        lines.push(format!(
            "byte=0x{byte:02X} {label}: qr={:?}",
            state.quick_release
        ));
    }
    assert_snapshot!(lines.join("\n"));
    Ok(())
}

// ── VID/PID to model mapping ─────────────────────────────────────────────────

/// Pin SimagicModel::from_pid for all known PIDs.
#[test]
fn snap_model_from_pid_matrix() {
    let pids: &[(u16, &str)] = &[
        (product_ids::EVO_SPORT, "EVO Sport"),
        (product_ids::EVO, "EVO"),
        (product_ids::EVO_PRO, "EVO Pro"),
        (product_ids::HANDBRAKE, "Handbrake"),
        (0xFFFF, "Unknown"),
    ];
    let mut lines = Vec::new();
    for &(pid, name) in pids {
        let model = SimagicModel::from_pid(pid);
        lines.push(format!(
            "0x{pid:04X} {name}: model={model:?}, torque={:.1}Nm",
            model.max_torque_nm(),
        ));
    }
    assert_snapshot!(lines.join("\n"));
}

/// Pin VID constants.
#[test]
fn snap_vendor_id_constants() {
    assert_snapshot!(format!(
        "SIMAGIC_VENDOR_ID=0x{:04X}, SIMAGIC_LEGACY_VENDOR_ID=0x{:04X}, SIMAGIC_LEGACY_PID=0x{:04X}",
        simagic::SIMAGIC_VENDOR_ID,
        simagic::ids::SIMAGIC_LEGACY_VENDOR_ID,
        simagic::ids::SIMAGIC_LEGACY_PID,
    ));
}

/// Pin device identity for all verified PIDs with full field values.
#[test]
fn snap_device_identity_verified_pids() {
    let verified_pids = [
        product_ids::EVO_SPORT,
        product_ids::EVO,
        product_ids::EVO_PRO,
        product_ids::HANDBRAKE,
    ];
    let mut lines = Vec::new();
    for pid in verified_pids {
        let id = identify_device(pid);
        lines.push(format!(
            "0x{pid:04X}: name={}, cat={:?}, ffb={}, torque={:?}",
            id.name, id.category, id.supports_ffb, id.max_torque_nm,
        ));
    }
    assert_snapshot!(lines.join("\n"));
}

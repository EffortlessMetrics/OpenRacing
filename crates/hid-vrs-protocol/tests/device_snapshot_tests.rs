//! Device protocol parsing snapshot tests for VRS DirectForce Pro HID protocol.
//!
//! These tests pin exact parsed output from known-good byte sequences,
//! covering connection detection, full-range values, and device identity.

use insta::assert_snapshot;
use racing_wheel_hid_vrs_protocol::{self as vrs, VRS_VENDOR_ID, identify_device, product_ids};

// ── Connection status detection ──────────────────────────────────────────────

/// Known-good: all-FF steering bytes indicate disconnected device.
#[test]
fn snap_parse_disconnected_device() -> Result<(), String> {
    let mut data = vec![0u8; 64];
    data[0] = 0xFF;
    data[1] = 0xFF; // steering = 0xFFFF → connected = false
    let state = vrs::parse_input_report(&data).ok_or("parse_input_report returned None")?;
    assert_snapshot!(format!(
        "connected={}, steering={:.4}",
        state.connected, state.steering,
    ));
    Ok(())
}

/// Known-good: any non-FFFF steering → connected.
#[test]
fn snap_parse_connected_device() -> Result<(), String> {
    let mut data = vec![0u8; 64];
    data[0] = 0x00;
    data[1] = 0x00; // steering = 0 → connected = true
    let state = vrs::parse_input_report(&data).ok_or("parse_input_report returned None")?;
    assert_snapshot!(format!(
        "connected={}, steering={:.4}",
        state.connected, state.steering,
    ));
    Ok(())
}

// ── Mixed axis values ────────────────────────────────────────────────────────

/// Known-good: quarter-range values on all axes.
#[test]
fn snap_parse_quarter_values() -> Result<(), String> {
    let mut data = vec![0u8; 64];
    // steering = -16384 (quarter left) → 0xC000 as i16 bytes LE = 0x00, 0xC0
    data[0] = 0x00;
    data[1] = 0xC0; // i16: -16384
    // throttle = 0x4000 (quarter)
    data[2] = 0x00;
    data[3] = 0x40;
    // brake = 0x8000 (half)
    data[4] = 0x00;
    data[5] = 0x80;
    // clutch = 0xC000 (three-quarter)
    data[6] = 0x00;
    data[7] = 0xC0;
    data[12] = 0x06; // hat = SE (6)

    let state = vrs::parse_input_report(&data).ok_or("parse_input_report returned None")?;
    assert_snapshot!(format!(
        "steering={:.4}, throttle={:.4}, brake={:.4}, clutch={:.4}, hat=0x{:X}",
        state.steering, state.throttle, state.brake, state.clutch, state.hat,
    ));
    Ok(())
}

/// Known-good: maximum button bitmask.
#[test]
fn snap_parse_all_buttons_pressed() -> Result<(), String> {
    let mut data = vec![0u8; 64];
    data[8] = 0xFF; // buttons low
    data[9] = 0xFF; // buttons high
    let state = vrs::parse_input_report(&data).ok_or("parse_input_report returned None")?;
    assert_snapshot!(format!("buttons=0x{:04X}", state.buttons));
    Ok(())
}

/// Known-good: signed encoder values including negative.
#[test]
fn snap_parse_encoder_negative_values() -> Result<(), String> {
    let mut data = vec![0u8; 64];
    data[13] = 0xFE; // encoder1 = -2 (as i8)
    data[15] = 0x01; // encoder2 = 1
    let state = vrs::parse_input_report(&data).ok_or("parse_input_report returned None")?;
    assert_snapshot!(format!(
        "encoder1={}, encoder2={}",
        state.encoder1, state.encoder2,
    ));
    Ok(())
}

// ── VID/PID mapping consistency ──────────────────────────────────────────────

/// Pin VRS vendor ID and shared-VID note.
#[test]
fn snap_vrs_vendor_id_constant() {
    assert_snapshot!(format!("VRS_VENDOR_ID=0x{VRS_VENDOR_ID:04X}"));
}

/// Pin full device identity for all known VRS PIDs.
#[test]
fn snap_device_identity_all_pids() {
    let pids: &[(u16, &str)] = &[
        (product_ids::DIRECTFORCE_PRO, "DFP"),
        (product_ids::DIRECTFORCE_PRO_V2, "DFP V2"),
        (product_ids::R295, "R295"),
        (product_ids::PEDALS, "Pedals"),
        (product_ids::PEDALS_V2, "Pedals V2"),
        (product_ids::HANDBRAKE, "Handbrake"),
        (product_ids::SHIFTER, "Shifter"),
        (0xFFFF, "Unknown"),
    ];
    let mut lines = Vec::new();
    for &(pid, name) in pids {
        let id = identify_device(pid);
        let is_wb = vrs::is_wheelbase_product(pid);
        lines.push(format!(
            "0x{pid:04X} {name}: cat={:?}, ffb={}, torque={:?}, is_wheelbase={is_wb}",
            id.category, id.supports_ffb, id.max_torque_nm,
        ));
    }
    assert_snapshot!(lines.join("\n"));
}

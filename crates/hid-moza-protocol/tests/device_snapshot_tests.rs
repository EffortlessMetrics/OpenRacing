//! Device protocol parsing snapshot tests for Moza HID protocol.
//!
//! These tests pin the exact parsed output from known-good byte sequences
//! for wheelbase aggregated input reports, pedal axes, and device identity.

use insta::assert_snapshot;
use racing_wheel_hid_moza_protocol::{
    self as moza, MOZA_VENDOR_ID, MozaModel, identify_device, is_wheelbase_product, product_ids,
    rim_ids,
};

// ── Wheelbase input report parsing ───────────────────────────────────────────

/// Known-good: centered steering, pedals released, no buttons.
#[test]
fn snap_parse_wheelbase_center() -> Result<(), String> {
    let mut data = vec![0u8; 64];
    data[0] = 0x01; // report ID
    // steering center: 0x8000 LE
    data[1] = 0x00;
    data[2] = 0x80;
    // throttle = 0, brake = 0, clutch = 0, handbrake = 0
    // all remain zero

    let raw = moza::parse_wheelbase_input_report(&data)
        .ok_or("parse_wheelbase_input_report returned None")?;
    assert_snapshot!(format!(
        "steering={}, pedals=throttle:{}/brake:{}/clutch:{:?}/hb:{:?}, hat={}, funky={}",
        raw.steering,
        raw.pedals.throttle,
        raw.pedals.brake,
        raw.pedals.clutch,
        raw.pedals.handbrake,
        raw.hat,
        raw.funky,
    ));
    Ok(())
}

/// Known-good: full right steering, all pedals pressed to max.
#[test]
fn snap_parse_wheelbase_full_right_all_pedals() -> Result<(), String> {
    let mut data = vec![0u8; 64];
    data[0] = 0x01;
    // steering full right: 0xFFFF
    data[1] = 0xFF;
    data[2] = 0xFF;
    // throttle full: 0xFFFF
    data[3] = 0xFF;
    data[4] = 0xFF;
    // brake full: 0xFFFF
    data[5] = 0xFF;
    data[6] = 0xFF;
    // clutch full: 0xFFFF
    data[7] = 0xFF;
    data[8] = 0xFF;
    // handbrake full: 0xFFFF
    data[9] = 0xFF;
    data[10] = 0xFF;

    let raw = moza::parse_wheelbase_input_report(&data)
        .ok_or("parse_wheelbase_input_report returned None")?;
    assert_snapshot!(format!(
        "steering={}, throttle={}, brake={}, clutch={:?}, handbrake={:?}",
        raw.steering,
        raw.pedals.throttle,
        raw.pedals.brake,
        raw.pedals.clutch,
        raw.pedals.handbrake,
    ));
    Ok(())
}

/// Known-good: mixed pedal values with half-press.
#[test]
fn snap_parse_wheelbase_mixed_pedals() -> Result<(), String> {
    let mut data = vec![0u8; 64];
    data[0] = 0x01;
    data[1] = 0x00;
    data[2] = 0x80; // steering center
    // throttle = 0x4000 (quarter)
    data[3] = 0x00;
    data[4] = 0x40;
    // brake = 0x8000 (half)
    data[5] = 0x00;
    data[6] = 0x80;
    // clutch = 0xC000 (three-quarter)
    data[7] = 0x00;
    data[8] = 0xC0;
    // handbrake = 0x2000 (one-eighth)
    data[9] = 0x00;
    data[10] = 0x20;

    let raw = moza::parse_wheelbase_input_report(&data)
        .ok_or("parse_wheelbase_input_report returned None")?;
    assert_snapshot!(format!(
        "steering={}, throttle={}, brake={}, clutch={:?}, handbrake={:?}",
        raw.steering,
        raw.pedals.throttle,
        raw.pedals.brake,
        raw.pedals.clutch,
        raw.pedals.handbrake,
    ));
    Ok(())
}

/// Pedal axes parsing from raw report bytes.
#[test]
fn snap_parse_wheelbase_pedal_axes() -> Result<(), String> {
    let mut data = vec![0u8; 64];
    data[0] = 0x01;
    data[1] = 0x00;
    data[2] = 0x80;
    // throttle = 1234 (0x04D2)
    data[3] = 0xD2;
    data[4] = 0x04;
    // brake = 5678 (0x162E)
    data[5] = 0x2E;
    data[6] = 0x16;

    let axes = moza::parse_wheelbase_pedal_axes(&data)
        .ok_or("parse_wheelbase_pedal_axes returned None")?;
    assert_snapshot!(format!(
        "throttle={}, brake={}, clutch={:?}, handbrake={:?}",
        axes.throttle, axes.brake, axes.clutch, axes.handbrake,
    ));
    Ok(())
}

/// Minimal report just above MIN_REPORT_LEN threshold.
#[test]
fn snap_parse_wheelbase_minimal_report() -> Result<(), String> {
    // MIN_REPORT_LEN = BRAKE_START + 2 = 7
    let data: [u8; 7] = [
        0x01, // report ID
        0x00, 0x80, // steering center
        0xAA, 0xBB, // throttle
        0xCC, 0xDD, // brake
    ];
    let raw = moza::parse_wheelbase_input_report(&data)
        .ok_or("parse_wheelbase_input_report returned None")?;
    assert_snapshot!(format!(
        "steering={}, throttle={}, brake={}, clutch={:?}, handbrake={:?}, hat={}, funky={}",
        raw.steering,
        raw.pedals.throttle,
        raw.pedals.brake,
        raw.pedals.clutch,
        raw.pedals.handbrake,
        raw.hat,
        raw.funky,
    ));
    Ok(())
}

// ── V1/V2 protocol version detection from PIDs ──────────────────────────────

/// Pin V1→V2 PID pattern: V2 PID = V1 PID | 0x0010.
#[test]
fn snap_v1_v2_pid_pattern() {
    let pairs: &[(&str, u16, u16)] = &[
        ("R3", product_ids::R3_V1, product_ids::R3_V2),
        ("R5", product_ids::R5_V1, product_ids::R5_V2),
        ("R9", product_ids::R9_V1, product_ids::R9_V2),
        ("R12", product_ids::R12_V1, product_ids::R12_V2),
        ("R16/R21", product_ids::R16_R21_V1, product_ids::R16_R21_V2),
    ];
    let mut lines = Vec::new();
    for &(name, v1, v2) in pairs {
        let v1_model = MozaModel::from_pid(v1);
        let v2_model = MozaModel::from_pid(v2);
        lines.push(format!(
            "{name}: V1=0x{v1:04X}({v1_model:?}), V2=0x{v2:04X}({v2_model:?}), \
             pattern_ok={}",
            v2 == (v1 | 0x0010),
        ));
    }
    assert_snapshot!(lines.join("\n"));
}

// ── VID/PID to vendor mapping consistency ────────────────────────────────────

/// Pin MOZA VID constant.
#[test]
fn snap_moza_vendor_id() {
    assert_snapshot!(format!("MOZA_VENDOR_ID=0x{MOZA_VENDOR_ID:04X}"));
}

/// Pin device identity for all known PIDs.
#[test]
fn snap_device_identity_all() {
    let pids: &[(u16, &str)] = &[
        (product_ids::R3_V1, "R3 V1"),
        (product_ids::R3_V2, "R3 V2"),
        (product_ids::R5_V1, "R5 V1"),
        (product_ids::R5_V2, "R5 V2"),
        (product_ids::R9_V1, "R9 V1"),
        (product_ids::R9_V2, "R9 V2"),
        (product_ids::R12_V1, "R12 V1"),
        (product_ids::R12_V2, "R12 V2"),
        (product_ids::R16_R21_V1, "R16/R21 V1"),
        (product_ids::R16_R21_V2, "R16/R21 V2"),
        (product_ids::SR_P_PEDALS, "SR-P Pedals"),
        (product_ids::HGP_SHIFTER, "HGP Shifter"),
        (product_ids::SGP_SHIFTER, "SGP Shifter"),
        (product_ids::HBP_HANDBRAKE, "HBP Handbrake"),
        (0xFFFF, "Unknown"),
    ];
    let mut lines = Vec::new();
    for &(pid, name) in pids {
        let id = identify_device(pid);
        lines.push(format!(
            "0x{pid:04X} {name}: cat={:?}, topo={:?}, ffb={}, is_wb={}",
            id.category,
            id.topology_hint,
            id.supports_ffb,
            is_wheelbase_product(pid),
        ));
    }
    assert_snapshot!(lines.join("\n"));
}

// ── Model torque matrix ──────────────────────────────────────────────────────

/// Pin torque values for all models.
#[test]
fn snap_model_torque_matrix() {
    let models = [
        ("R3", MozaModel::R3),
        ("R5", MozaModel::R5),
        ("R9", MozaModel::R9),
        ("R12", MozaModel::R12),
        ("R16", MozaModel::R16),
        ("R21", MozaModel::R21),
        ("SrpPedals", MozaModel::SrpPedals),
        ("Unknown", MozaModel::Unknown),
    ];
    let mut lines = Vec::new();
    for (name, model) in models {
        lines.push(format!("{name}: {:.1}Nm", model.max_torque_nm()));
    }
    assert_snapshot!(lines.join("\n"));
}

// ── Rim IDs ──────────────────────────────────────────────────────────────────

/// Pin all known Moza rim ID byte values.
#[test]
fn snap_rim_id_constants() {
    let rims = [
        ("CS V2", rim_ids::CS_V2),
        ("GS V2", rim_ids::GS_V2),
        ("RS V2", rim_ids::RS_V2),
        ("FSR", rim_ids::FSR),
        ("KS", rim_ids::KS),
        ("ES", rim_ids::ES),
    ];
    let mut lines = Vec::new();
    for (name, id) in rims {
        lines.push(format!("{name}: 0x{id:02X}"));
    }
    assert_snapshot!(lines.join("\n"));
}

// ── ES compatibility ─────────────────────────────────────────────────────────

/// Pin ES compatibility for every wheelbase PID.
#[test]
fn snap_es_compatibility_matrix() {
    let pids: &[(u16, &str)] = &[
        (product_ids::R3_V1, "R3 V1"),
        (product_ids::R3_V2, "R3 V2"),
        (product_ids::R5_V1, "R5 V1"),
        (product_ids::R5_V2, "R5 V2"),
        (product_ids::R9_V1, "R9 V1"),
        (product_ids::R9_V2, "R9 V2"),
        (product_ids::R12_V1, "R12 V1"),
        (product_ids::R12_V2, "R12 V2"),
        (product_ids::R16_R21_V1, "R16/R21 V1"),
        (product_ids::R16_R21_V2, "R16/R21 V2"),
        (product_ids::SR_P_PEDALS, "SR-P Pedals"),
        (0xFFFF, "Unknown"),
    ];
    let mut lines = Vec::new();
    for &(pid, name) in pids {
        let compat = moza::es_compatibility(pid);
        lines.push(format!("{name}: {compat:?}"));
    }
    assert_snapshot!(lines.join("\n"));
}

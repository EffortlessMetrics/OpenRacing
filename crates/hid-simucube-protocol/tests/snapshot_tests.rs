//! Snapshot tests for the Simucube HID protocol.
//!
//! These tests lock in the wire format to catch accidental protocol regressions.

use hid_simucube_protocol as simucube;
use insta::assert_debug_snapshot;

#[test]
fn test_snapshot_output_zero_torque() -> Result<(), String> {
    let report = simucube::SimucubeOutputReport::new(0).with_torque(0.0);
    let data = report.build().map_err(|e| e.to_string())?;
    assert_debug_snapshot!(data);
    Ok(())
}

#[test]
fn test_snapshot_output_full_positive_torque() -> Result<(), String> {
    let report = simucube::SimucubeOutputReport::new(0).with_torque(simucube::MAX_TORQUE_NM);
    let data = report.build().map_err(|e| e.to_string())?;
    assert_debug_snapshot!(data);
    Ok(())
}

#[test]
fn test_snapshot_output_full_negative_torque() -> Result<(), String> {
    let report = simucube::SimucubeOutputReport::new(0).with_torque(-simucube::MAX_TORQUE_NM);
    let data = report.build().map_err(|e| e.to_string())?;
    assert_debug_snapshot!(data);
    Ok(())
}

#[test]
fn test_snapshot_output_quarter_torque() -> Result<(), String> {
    let report = simucube::SimucubeOutputReport::new(0).with_torque(simucube::MAX_TORQUE_NM * 0.25);
    let data = report.build().map_err(|e| e.to_string())?;
    assert_debug_snapshot!(data);
    Ok(())
}

#[test]
fn test_snapshot_output_with_rgb() -> Result<(), String> {
    let report = simucube::SimucubeOutputReport::new(0)
        .with_torque(5.0)
        .with_rgb(255, 128, 0);
    let data = report.build().map_err(|e| e.to_string())?;
    assert_debug_snapshot!(data);
    Ok(())
}

#[test]
fn test_snapshot_output_with_spring_effect() -> Result<(), String> {
    let report = simucube::SimucubeOutputReport::new(0)
        .with_torque(0.0)
        .with_effect(simucube::EffectType::Spring, 500);
    let data = report.build().map_err(|e| e.to_string())?;
    assert_debug_snapshot!(data);
    Ok(())
}

#[test]
fn test_snapshot_output_with_damper_effect() -> Result<(), String> {
    let report = simucube::SimucubeOutputReport::new(0)
        .with_torque(0.0)
        .with_effect(simucube::EffectType::Damper, 750);
    let data = report.build().map_err(|e| e.to_string())?;
    assert_debug_snapshot!(data);
    Ok(())
}

#[test]
fn test_snapshot_model_sport() {
    let model = simucube::SimucubeModel::Sport;
    assert_debug_snapshot!(format!(
        "name={}, max_torque={:.1}",
        model.display_name(),
        model.max_torque_nm()
    ));
}

#[test]
fn test_snapshot_model_pro() {
    let model = simucube::SimucubeModel::Pro;
    assert_debug_snapshot!(format!(
        "name={}, max_torque={:.1}",
        model.display_name(),
        model.max_torque_nm()
    ));
}

#[test]
fn test_snapshot_model_ultimate() {
    let model = simucube::SimucubeModel::Ultimate;
    assert_debug_snapshot!(format!(
        "name={}, max_torque={:.1}",
        model.display_name(),
        model.max_torque_nm()
    ));
}

#[test]
fn test_snapshot_model_active_pedal() {
    let model = simucube::SimucubeModel::ActivePedal;
    assert_debug_snapshot!(format!(
        "name={}, max_torque={:.1}",
        model.display_name(),
        model.max_torque_nm()
    ));
}

#[test]
fn test_snapshot_model_unknown() {
    let model = simucube::SimucubeModel::Unknown;
    assert_debug_snapshot!(format!(
        "name={}, max_torque={:.1}",
        model.display_name(),
        model.max_torque_nm()
    ));
}

#[test]
fn test_snapshot_model_from_info() {
    let results = [
        (
            "sport",
            simucube::simucube_model_from_info(
                simucube::SIMUCUBE_VENDOR_ID,
                simucube::SIMUCUBE_2_SPORT_PID,
            ),
        ),
        (
            "pro",
            simucube::simucube_model_from_info(
                simucube::SIMUCUBE_VENDOR_ID,
                simucube::SIMUCUBE_2_PRO_PID,
            ),
        ),
        (
            "ultimate",
            simucube::simucube_model_from_info(
                simucube::SIMUCUBE_VENDOR_ID,
                simucube::SIMUCUBE_2_ULTIMATE_PID,
            ),
        ),
        (
            "wrong_vid",
            simucube::simucube_model_from_info(0x0000, simucube::SIMUCUBE_2_SPORT_PID),
        ),
    ];
    assert_debug_snapshot!(format!("{:?}", results));
}

#[test]
fn test_snapshot_effect_type_values() {
    let effects = [
        ("None", simucube::EffectType::None as u8),
        ("Constant", simucube::EffectType::Constant as u8),
        ("Spring", simucube::EffectType::Spring as u8),
        ("Damper", simucube::EffectType::Damper as u8),
        ("Friction", simucube::EffectType::Friction as u8),
    ];
    assert_debug_snapshot!(format!("{:?}", effects));
}

// ─── HID joystick report snapshots ──────────────────────────────────────────

fn build_hid_bytes(steering: u16, y: u16, axes: [u16; 6], buttons: [u8; 16]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(32);
    buf.extend_from_slice(&steering.to_le_bytes());
    buf.extend_from_slice(&y.to_le_bytes());
    for ax in &axes {
        buf.extend_from_slice(&ax.to_le_bytes());
    }
    buf.extend_from_slice(&buttons);
    buf
}

#[test]
fn test_snapshot_hid_report_center() -> Result<(), String> {
    let data = build_hid_bytes(0x8000, 0x8000, [0; 6], [0; 16]);
    let report = simucube::SimucubeHidReport::parse(&data).map_err(|e| e.to_string())?;
    assert_debug_snapshot!(report);
    Ok(())
}

#[test]
fn test_snapshot_hid_report_full_left() -> Result<(), String> {
    let data = build_hid_bytes(0x0000, 0x8000, [0; 6], [0; 16]);
    let report = simucube::SimucubeHidReport::parse(&data).map_err(|e| e.to_string())?;
    assert_debug_snapshot!(report);
    Ok(())
}

#[test]
fn test_snapshot_hid_report_full_right() -> Result<(), String> {
    let data = build_hid_bytes(0xFFFF, 0x8000, [0; 6], [0; 16]);
    let report = simucube::SimucubeHidReport::parse(&data).map_err(|e| e.to_string())?;
    assert_debug_snapshot!(report);
    Ok(())
}

#[test]
fn test_snapshot_hid_report_with_buttons() -> Result<(), String> {
    let mut buttons = [0u8; 16];
    buttons[0] = 0b0000_0101; // buttons 0, 2
    buttons[15] = 0b1000_0000; // button 127
    let data = build_hid_bytes(0x8000, 0x8000, [0; 6], buttons);
    let report = simucube::SimucubeHidReport::parse(&data).map_err(|e| e.to_string())?;
    assert_debug_snapshot!(report);
    Ok(())
}

#[test]
fn test_snapshot_hid_report_with_axes() -> Result<(), String> {
    let axes = [1000, 2000, 3000, 4000, 5000, 6000];
    let data = build_hid_bytes(0x8000, 0x4000, axes, [0; 16]);
    let report = simucube::SimucubeHidReport::parse(&data).map_err(|e| e.to_string())?;
    assert_debug_snapshot!(report);
    Ok(())
}

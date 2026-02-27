//! Snapshot tests for the Simucube HID protocol.
//!
//! These tests lock in the wire format to catch accidental protocol regressions.

use insta::assert_debug_snapshot;
use hid_simucube_protocol as simucube;

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

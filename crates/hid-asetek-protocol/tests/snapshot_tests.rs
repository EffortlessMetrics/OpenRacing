//! Snapshot tests for the Asetek HID protocol.
//!
//! These tests lock in the wire format to catch accidental protocol regressions.

use hid_asetek_protocol as asetek;
use insta::assert_debug_snapshot;

#[test]
fn test_snapshot_output_zero_torque() -> Result<(), String> {
    let report = asetek::AsetekOutputReport::new(0).with_torque(0.0);
    let data = report.build().map_err(|e| e.to_string())?;
    assert_debug_snapshot!(data);
    Ok(())
}

#[test]
fn test_snapshot_output_full_positive_torque() -> Result<(), String> {
    let report = asetek::AsetekOutputReport::new(0).with_torque(asetek::MAX_TORQUE_NM);
    let data = report.build().map_err(|e| e.to_string())?;
    assert_debug_snapshot!(data);
    Ok(())
}

#[test]
fn test_snapshot_output_full_negative_torque() -> Result<(), String> {
    let report = asetek::AsetekOutputReport::new(0).with_torque(-asetek::MAX_TORQUE_NM);
    let data = report.build().map_err(|e| e.to_string())?;
    assert_debug_snapshot!(data);
    Ok(())
}

#[test]
fn test_snapshot_output_quarter_torque() -> Result<(), String> {
    let report = asetek::AsetekOutputReport::new(0).with_torque(asetek::MAX_TORQUE_NM * 0.25);
    let data = report.build().map_err(|e| e.to_string())?;
    assert_debug_snapshot!(data);
    Ok(())
}

#[test]
fn test_snapshot_output_with_led() -> Result<(), String> {
    let report = asetek::AsetekOutputReport::new(1)
        .with_torque(5.0)
        .with_led(0x01, 0x80);
    let data = report.build().map_err(|e| e.to_string())?;
    assert_debug_snapshot!(data);
    Ok(())
}

#[test]
fn test_snapshot_model_forte() {
    let model = asetek::AsetekModel::Forte;
    assert_debug_snapshot!(format!(
        "name={}, max_torque={:.1}",
        model.display_name(),
        model.max_torque_nm()
    ));
}

#[test]
fn test_snapshot_model_invicta() {
    let model = asetek::AsetekModel::Invicta;
    assert_debug_snapshot!(format!(
        "name={}, max_torque={:.1}",
        model.display_name(),
        model.max_torque_nm()
    ));
}

#[test]
fn test_snapshot_model_laprima() {
    let model = asetek::AsetekModel::LaPrima;
    assert_debug_snapshot!(format!(
        "name={}, max_torque={:.1}",
        model.display_name(),
        model.max_torque_nm()
    ));
}

#[test]
fn test_snapshot_model_unknown() {
    let model = asetek::AsetekModel::Unknown;
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
            "forte",
            asetek::asetek_model_from_info(asetek::ASETEK_VENDOR_ID, asetek::ASETEK_FORTE_PID),
        ),
        (
            "invicta",
            asetek::asetek_model_from_info(asetek::ASETEK_VENDOR_ID, asetek::ASETEK_INVICTA_PID),
        ),
        (
            "laprima",
            asetek::asetek_model_from_info(asetek::ASETEK_VENDOR_ID, asetek::ASETEK_LAPRIMA_PID),
        ),
        (
            "wrong_vid",
            asetek::asetek_model_from_info(0x0000, asetek::ASETEK_FORTE_PID),
        ),
    ];
    assert_debug_snapshot!(format!("{:?}", results));
}

#[test]
fn test_snapshot_is_asetek_device() {
    let results = [
        (
            "correct_vid",
            asetek::is_asetek_device(asetek::ASETEK_VENDOR_ID),
        ),
        ("wrong_vid", asetek::is_asetek_device(0x0000)),
    ];
    assert_debug_snapshot!(format!("{:?}", results));
}

#[test]
fn test_snapshot_model_tony_kanaan() {
    let model = asetek::AsetekModel::TonyKanaan;
    assert_debug_snapshot!(format!(
        "name={}, max_torque={:.1}",
        model.display_name(),
        model.max_torque_nm()
    ));
}

#[test]
fn test_snapshot_input_report_zero() -> Result<(), String> {
    let data = [0u8; 32];
    let report = asetek::AsetekInputReport::parse(&data).map_err(|e| e.to_string())?;
    assert_debug_snapshot!(format!(
        "angle={:.3}deg, speed={:.4}rad_s, torque={:.3}Nm, connected={}, enabled={}",
        report.wheel_angle_degrees(),
        report.wheel_speed_rad_s(),
        report.applied_torque_nm(),
        report.is_connected(),
        report.is_enabled()
    ));
    Ok(())
}

#[test]
fn test_snapshot_protocol_constants() {
    assert_debug_snapshot!(format!(
        "VID={:#06X}, FORTE_PID={:#06X}, INVICTA_PID={:#06X}, LAPRIMA_PID={:#06X}, \
         TONY_KANAAN_PID={:#06X}, INPUT_SIZE={}, OUTPUT_SIZE={}, MAX_TORQUE={}",
        asetek::ASETEK_VENDOR_ID,
        asetek::ASETEK_FORTE_PID,
        asetek::ASETEK_INVICTA_PID,
        asetek::ASETEK_LAPRIMA_PID,
        asetek::ASETEK_TONY_KANAAN_PID,
        asetek::REPORT_SIZE_INPUT,
        asetek::REPORT_SIZE_OUTPUT,
        asetek::MAX_TORQUE_NM,
    ));
}

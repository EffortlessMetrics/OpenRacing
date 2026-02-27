//! Snapshot tests for the Asetek HID protocol.
//!
//! These tests lock in the wire format to catch accidental protocol regressions.

use insta::assert_debug_snapshot;
use hid_asetek_protocol as asetek;

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
    let report = asetek::AsetekOutputReport::new(1).with_torque(5.0).with_led(0x01, 0x80);
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
        ("wrong_vid", asetek::asetek_model_from_info(0x0000, asetek::ASETEK_FORTE_PID)),
    ];
    assert_debug_snapshot!(format!("{:?}", results));
}

#[test]
fn test_snapshot_is_asetek_device() {
    let results = [
        ("correct_vid", asetek::is_asetek_device(asetek::ASETEK_VENDOR_ID)),
        ("wrong_vid", asetek::is_asetek_device(0x0000)),
    ];
    assert_debug_snapshot!(format!("{:?}", results));
}

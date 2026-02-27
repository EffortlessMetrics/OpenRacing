//! Snapshot tests for the Heusinkveld HID protocol.
//!
//! These tests lock in the wire format to catch accidental protocol regressions.

use hid_heusinkveld_protocol as heusinkveld;
use insta::assert_debug_snapshot;

#[test]
fn test_snapshot_parse_center() -> Result<(), String> {
    let data = [0u8; 8];
    let report = heusinkveld::HeusinkveldInputReport::parse(&data).map_err(|e| e.to_string())?;
    assert_debug_snapshot!(format!(
        "throttle={:.4}, brake={:.4}, clutch={:.4}, connected={}, calibrated={}, fault={}",
        report.throttle_normalized(),
        report.brake_normalized(),
        report.clutch_normalized(),
        report.is_connected(),
        report.is_calibrated(),
        report.has_fault()
    ));
    Ok(())
}

#[test]
fn test_snapshot_parse_full_throttle() -> Result<(), String> {
    let mut data = [0u8; 8];
    data[0] = 0xFF;
    data[1] = 0xFF;
    let report = heusinkveld::HeusinkveldInputReport::parse(&data).map_err(|e| e.to_string())?;
    assert_debug_snapshot!(format!("throttle={:.4}", report.throttle_normalized()));
    Ok(())
}

#[test]
fn test_snapshot_parse_full_brake() -> Result<(), String> {
    let mut data = [0u8; 8];
    data[2] = 0xFF;
    data[3] = 0xFF;
    let report = heusinkveld::HeusinkveldInputReport::parse(&data).map_err(|e| e.to_string())?;
    assert_debug_snapshot!(format!("brake={:.4}", report.brake_normalized()));
    Ok(())
}

#[test]
fn test_snapshot_parse_status_connected_calibrated() -> Result<(), String> {
    let mut data = [0u8; 8];
    data[6] = 0x03; // connected + calibrated
    let report = heusinkveld::HeusinkveldInputReport::parse(&data).map_err(|e| e.to_string())?;
    assert_debug_snapshot!(format!(
        "connected={}, calibrated={}, fault={}",
        report.is_connected(),
        report.is_calibrated(),
        report.has_fault()
    ));
    Ok(())
}

#[test]
fn test_snapshot_parse_status_fault() -> Result<(), String> {
    let mut data = [0u8; 8];
    data[6] = 0x07; // connected + calibrated + fault
    let report = heusinkveld::HeusinkveldInputReport::parse(&data).map_err(|e| e.to_string())?;
    assert_debug_snapshot!(format!(
        "connected={}, calibrated={}, fault={}",
        report.is_connected(),
        report.is_calibrated(),
        report.has_fault()
    ));
    Ok(())
}

#[test]
fn test_snapshot_model_sprint() {
    let model = heusinkveld::HeusinkveldModel::Sprint;
    assert_debug_snapshot!(format!(
        "name={}, max_load={:.1}, pedals={}",
        model.display_name(),
        model.max_load_kg(),
        model.pedal_count()
    ));
}

#[test]
fn test_snapshot_model_ultimate() {
    let model = heusinkveld::HeusinkveldModel::Ultimate;
    assert_debug_snapshot!(format!(
        "name={}, max_load={:.1}, pedals={}",
        model.display_name(),
        model.max_load_kg(),
        model.pedal_count()
    ));
}

#[test]
fn test_snapshot_model_pro() {
    let model = heusinkveld::HeusinkveldModel::Pro;
    assert_debug_snapshot!(format!(
        "name={}, max_load={:.1}, pedals={}",
        model.display_name(),
        model.max_load_kg(),
        model.pedal_count()
    ));
}

#[test]
fn test_snapshot_pedal_capabilities_sprint() {
    let caps = heusinkveld::PedalCapabilities::for_model(heusinkveld::PedalModel::Sprint);
    assert_debug_snapshot!(format!(
        "max_load={:.1}, hydraulic={}, load_cell={}, pedals={}",
        caps.max_load_kg, caps.has_hydraulic_damping, caps.has_load_cell, caps.pedal_count
    ));
}

#[test]
fn test_snapshot_pedal_capabilities_pro() {
    let caps = heusinkveld::PedalCapabilities::for_model(heusinkveld::PedalModel::Pro);
    assert_debug_snapshot!(format!(
        "max_load={:.1}, hydraulic={}, load_cell={}, pedals={}",
        caps.max_load_kg, caps.has_hydraulic_damping, caps.has_load_cell, caps.pedal_count
    ));
}

#[test]
fn test_snapshot_pedal_status_from_flags() {
    let statuses = [
        ("disconnected", heusinkveld::PedalStatus::from_flags(0x00)),
        ("calibrating", heusinkveld::PedalStatus::from_flags(0x01)),
        ("ready", heusinkveld::PedalStatus::from_flags(0x03)),
        ("error", heusinkveld::PedalStatus::from_flags(0x07)),
    ];
    assert_debug_snapshot!(format!("{:?}", statuses));
}

#[test]
fn test_snapshot_model_from_info() {
    let results = [
        (
            "sprint",
            heusinkveld::heusinkveld_model_from_info(
                heusinkveld::HEUSINKVELD_VENDOR_ID,
                heusinkveld::HEUSINKVELD_SPRINT_PID,
            ),
        ),
        (
            "ultimate",
            heusinkveld::heusinkveld_model_from_info(
                heusinkveld::HEUSINKVELD_VENDOR_ID,
                heusinkveld::HEUSINKVELD_ULTIMATE_PID,
            ),
        ),
        (
            "wrong_vid",
            heusinkveld::heusinkveld_model_from_info(0x0000, heusinkveld::HEUSINKVELD_SPRINT_PID),
        ),
    ];
    assert_debug_snapshot!(format!("{:?}", results));
}

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
fn test_snapshot_parse_full_range_all_pedals() -> Result<(), String> {
    // throttle=0xFFFF, brake=0xFFFF, clutch=0xFFFF, status=0x03 (connected + calibrated)
    let data = [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x03, 0x00];
    let report = heusinkveld::HeusinkveldInputReport::parse(&data).map_err(|e| e.to_string())?;
    assert_debug_snapshot!(format!(
        "throttle={:.4}, brake={:.4}, clutch={:.4}, connected={}, calibrated={}",
        report.throttle_normalized(),
        report.brake_normalized(),
        report.clutch_normalized(),
        report.is_connected(),
        report.is_calibrated()
    ));
    Ok(())
}

#[test]
fn test_snapshot_parse_short_report_error() {
    let data = [0u8; 4];
    let result = heusinkveld::HeusinkveldInputReport::parse(&data);
    assert!(result.is_err(), "parsing a 4-byte report must fail");
    if let Err(e) = result {
        assert_debug_snapshot!(format!("{e:?}"));
    }
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
fn test_snapshot_pedal_capabilities_ultimate() {
    let caps = heusinkveld::PedalCapabilities::for_model(heusinkveld::PedalModel::Ultimate);
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
fn test_snapshot_model_handbrake_v1() {
    let model = heusinkveld::HeusinkveldModel::HandbrakeV1;
    assert_debug_snapshot!(format!(
        "name={}, max_load={:.1}, pedals={}",
        model.display_name(),
        model.max_load_kg(),
        model.pedal_count()
    ));
}

#[test]
fn test_snapshot_model_handbrake_v2() {
    let model = heusinkveld::HeusinkveldModel::HandbrakeV2;
    assert_debug_snapshot!(format!(
        "name={}, max_load={:.1}, pedals={}",
        model.display_name(),
        model.max_load_kg(),
        model.pedal_count()
    ));
}

#[test]
fn test_snapshot_model_sequential_shifter() {
    let model = heusinkveld::HeusinkveldModel::SequentialShifter;
    assert_debug_snapshot!(format!(
        "name={}, max_load={:.1}, pedals={}",
        model.display_name(),
        model.max_load_kg(),
        model.pedal_count()
    ));
}

#[test]
fn test_snapshot_model_unknown() {
    let model = heusinkveld::HeusinkveldModel::Unknown;
    assert_debug_snapshot!(format!(
        "name={}, max_load={:.1}, pedals={}",
        model.display_name(),
        model.max_load_kg(),
        model.pedal_count()
    ));
}

#[test]
fn test_snapshot_all_device_variants() {
    let variants = [
        heusinkveld::HeusinkveldModel::Sprint,
        heusinkveld::HeusinkveldModel::Ultimate,
        heusinkveld::HeusinkveldModel::Pro,
        heusinkveld::HeusinkveldModel::HandbrakeV1,
        heusinkveld::HeusinkveldModel::HandbrakeV2,
        heusinkveld::HeusinkveldModel::SequentialShifter,
        heusinkveld::HeusinkveldModel::Unknown,
    ];
    let summary: Vec<String> = variants
        .iter()
        .map(|m| {
            format!(
                "{:?}: name={}, max_load={:.1}, pedals={}",
                m,
                m.display_name(),
                m.max_load_kg(),
                m.pedal_count()
            )
        })
        .collect();
    assert_debug_snapshot!(summary);
}

#[test]
fn test_snapshot_protocol_constants() {
    assert_debug_snapshot!(format!(
        "VENDOR_ID={:#06x}, SPRINT_PID={:#06x}, ULTIMATE_PID={:#06x}, \
         HANDBRAKE_V2_PID={:#06x}, LEGACY_VID={:#06x}, LEGACY_SPRINT_PID={:#06x}, \
         LEGACY_ULTIMATE_PID={:#06x}, PRO_PID={:#06x}, \
         HANDBRAKE_V1_VID={:#06x}, HANDBRAKE_V1_PID={:#06x}, \
         SHIFTER_VID={:#06x}, SHIFTER_PID={:#06x}, \
         REPORT_SIZE_INPUT={}, MAX_LOAD_CELL_VALUE={:#06x}",
        heusinkveld::HEUSINKVELD_VENDOR_ID,
        heusinkveld::HEUSINKVELD_SPRINT_PID,
        heusinkveld::HEUSINKVELD_ULTIMATE_PID,
        heusinkveld::HEUSINKVELD_HANDBRAKE_V2_PID,
        heusinkveld::HEUSINKVELD_LEGACY_VENDOR_ID,
        heusinkveld::HEUSINKVELD_LEGACY_SPRINT_PID,
        heusinkveld::HEUSINKVELD_LEGACY_ULTIMATE_PID,
        heusinkveld::HEUSINKVELD_PRO_PID,
        heusinkveld::HEUSINKVELD_HANDBRAKE_V1_VENDOR_ID,
        heusinkveld::HEUSINKVELD_HANDBRAKE_V1_PID,
        heusinkveld::HEUSINKVELD_SHIFTER_VENDOR_ID,
        heusinkveld::HEUSINKVELD_SHIFTER_PID,
        heusinkveld::REPORT_SIZE_INPUT,
        heusinkveld::MAX_LOAD_CELL_VALUE,
    ));
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
            "pro_legacy",
            heusinkveld::heusinkveld_model_from_info(
                heusinkveld::HEUSINKVELD_LEGACY_VENDOR_ID,
                heusinkveld::HEUSINKVELD_PRO_PID,
            ),
        ),
        (
            "wrong_vid",
            heusinkveld::heusinkveld_model_from_info(0x0000, heusinkveld::HEUSINKVELD_SPRINT_PID),
        ),
    ];
    assert_debug_snapshot!(format!("{:?}", results));
}

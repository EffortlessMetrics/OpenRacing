//! Additional insta snapshot tests for the Heusinkveld HID protocol.
//!
//! Complements `snapshot_tests.rs` with coverage for default values,
//! error formatting, device detection helpers, and PID-based model lookup.

use hid_heusinkveld_protocol as heusinkveld;
use insta::assert_debug_snapshot;

#[test]
fn snapshot_input_report_default() {
    let report = heusinkveld::HeusinkveldInputReport::default();
    assert_debug_snapshot!(format!(
        "throttle={}, brake={}, clutch={}, status={:#04x}, connected={}, calibrated={}, fault={}",
        report.throttle,
        report.brake,
        report.clutch,
        report.status,
        report.is_connected(),
        report.is_calibrated(),
        report.has_fault()
    ));
}

#[test]
fn snapshot_error_invalid_report_size() {
    let err = heusinkveld::HeusinkveldError::InvalidReportSize {
        expected: 8,
        actual: 3,
    };
    assert_debug_snapshot!(format!("{err}"));
}

#[test]
fn snapshot_is_heusinkveld_device() {
    let results = [
        (
            "current_vid",
            heusinkveld::is_heusinkveld_device(heusinkveld::HEUSINKVELD_VENDOR_ID),
        ),
        (
            "legacy_vid",
            heusinkveld::is_heusinkveld_device(heusinkveld::HEUSINKVELD_LEGACY_VENDOR_ID),
        ),
        (
            "handbrake_v1_vid",
            heusinkveld::is_heusinkveld_device(heusinkveld::HEUSINKVELD_HANDBRAKE_V1_VENDOR_ID),
        ),
        (
            "shifter_vid",
            heusinkveld::is_heusinkveld_device(heusinkveld::HEUSINKVELD_SHIFTER_VENDOR_ID),
        ),
        ("wrong_vid_zero", heusinkveld::is_heusinkveld_device(0x0000)),
        ("wrong_vid_ffff", heusinkveld::is_heusinkveld_device(0xFFFF)),
    ];
    assert_debug_snapshot!(format!("{results:?}"));
}

#[test]
fn snapshot_model_from_product_id_all() {
    let pids = [
        heusinkveld::HEUSINKVELD_SPRINT_PID,
        heusinkveld::HEUSINKVELD_ULTIMATE_PID,
        heusinkveld::HEUSINKVELD_HANDBRAKE_V2_PID,
        heusinkveld::HEUSINKVELD_LEGACY_SPRINT_PID,
        heusinkveld::HEUSINKVELD_LEGACY_ULTIMATE_PID,
        heusinkveld::HEUSINKVELD_PRO_PID,
        heusinkveld::HEUSINKVELD_HANDBRAKE_V1_PID,
        heusinkveld::HEUSINKVELD_SHIFTER_PID,
        0xFFFF,
    ];
    let summary: Vec<String> = pids
        .iter()
        .map(|pid| {
            let model = heusinkveld::HeusinkveldModel::from_product_id(*pid);
            format!("PID={pid:#06x} -> {:?} ({})", model, model.display_name())
        })
        .collect();
    assert_debug_snapshot!(summary);
}

#[test]
fn snapshot_pedal_capabilities_default() {
    let caps = heusinkveld::PedalCapabilities::default();
    assert_debug_snapshot!(format!(
        "max_load={:.1}kg, hydraulic={}, load_cell={}, pedals={}",
        caps.max_load_kg, caps.has_hydraulic_damping, caps.has_load_cell, caps.pedal_count
    ));
}

#[test]
fn snapshot_parse_mid_range_pedals() -> Result<(), String> {
    // throttle=0x8000, brake=0x4000, clutch=0xC000, status=connected+calibrated
    let data: [u8; 8] = [0x00, 0x80, 0x00, 0x40, 0x00, 0xC0, 0x03, 0x00];
    let report = heusinkveld::HeusinkveldInputReport::parse(&data).map_err(|e| e.to_string())?;
    assert_debug_snapshot!(format!(
        "throttle={:.4}, brake={:.4}, clutch={:.4}",
        report.throttle_normalized(),
        report.brake_normalized(),
        report.clutch_normalized()
    ));
    Ok(())
}

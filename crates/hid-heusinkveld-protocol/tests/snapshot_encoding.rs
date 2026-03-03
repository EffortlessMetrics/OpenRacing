//! Extended snapshot tests for Heusinkveld wire-format encoding.
//!
//! These tests supplement `snapshot_tests.rs` by covering byte-level parsing
//! boundary values, legacy VID/PID matching, normalized value precision,
//! error message formatting, and serialized capabilities.

use hid_heusinkveld_protocol as heusinkveld;
use insta::assert_snapshot;

// ── Byte-level input report parsing ──────────────────────────────────────────

#[test]
fn test_snapshot_parse_half_throttle() -> Result<(), String> {
    let mut data = [0u8; 8];
    // throttle = 0x7FFF (~half of 0xFFFF)
    data[0] = 0xFF;
    data[1] = 0x7F;
    data[6] = 0x03; // connected + calibrated
    let report =
        heusinkveld::HeusinkveldInputReport::parse(&data).map_err(|e| e.to_string())?;
    assert_snapshot!(format!(
        "throttle=0x{:04X}, norm={:.6}",
        report.throttle,
        report.throttle_normalized()
    ));
    Ok(())
}

#[test]
fn test_snapshot_parse_half_brake() -> Result<(), String> {
    let mut data = [0u8; 8];
    // brake = 0x8000 (exactly half + 1)
    data[2] = 0x00;
    data[3] = 0x80;
    data[6] = 0x03;
    let report =
        heusinkveld::HeusinkveldInputReport::parse(&data).map_err(|e| e.to_string())?;
    assert_snapshot!(format!(
        "brake=0x{:04X}, norm={:.6}",
        report.brake,
        report.brake_normalized()
    ));
    Ok(())
}

#[test]
fn test_snapshot_parse_all_pedals_midpoint() -> Result<(), String> {
    let mut data = [0u8; 8];
    // all pedals at 0x4000
    data[0] = 0x00;
    data[1] = 0x40;
    data[2] = 0x00;
    data[3] = 0x40;
    data[4] = 0x00;
    data[5] = 0x40;
    data[6] = 0x03;
    let report =
        heusinkveld::HeusinkveldInputReport::parse(&data).map_err(|e| e.to_string())?;
    assert_snapshot!(format!(
        "throttle={:.6}, brake={:.6}, clutch={:.6}",
        report.throttle_normalized(),
        report.brake_normalized(),
        report.clutch_normalized()
    ));
    Ok(())
}

// ── Status flag combinations ─────────────────────────────────────────────────

#[test]
fn test_snapshot_status_flags_exhaustive() {
    let flags: Vec<String> = (0u8..=7)
        .map(|f| {
            let mut data = [0u8; 8];
            data[6] = f;
            let report = heusinkveld::HeusinkveldInputReport::parse(&data);
            match report {
                Ok(r) => format!(
                    "0x{f:02X}: connected={}, calibrated={}, fault={}",
                    r.is_connected(),
                    r.is_calibrated(),
                    r.has_fault()
                ),
                Err(e) => format!("0x{f:02X}: err={e}"),
            }
        })
        .collect();
    assert_snapshot!(flags.join("\n"));
}

// ── Legacy VID/PID matching ──────────────────────────────────────────────────

#[test]
fn test_snapshot_model_from_vid_pid_all_combinations() {
    let combos: Vec<String> = [
        (heusinkveld::HEUSINKVELD_VENDOR_ID, heusinkveld::HEUSINKVELD_SPRINT_PID),
        (heusinkveld::HEUSINKVELD_VENDOR_ID, heusinkveld::HEUSINKVELD_ULTIMATE_PID),
        (heusinkveld::HEUSINKVELD_VENDOR_ID, heusinkveld::HEUSINKVELD_HANDBRAKE_V2_PID),
        (heusinkveld::HEUSINKVELD_LEGACY_VENDOR_ID, heusinkveld::HEUSINKVELD_LEGACY_SPRINT_PID),
        (heusinkveld::HEUSINKVELD_LEGACY_VENDOR_ID, heusinkveld::HEUSINKVELD_LEGACY_ULTIMATE_PID),
        (heusinkveld::HEUSINKVELD_LEGACY_VENDOR_ID, heusinkveld::HEUSINKVELD_PRO_PID),
        (heusinkveld::HEUSINKVELD_HANDBRAKE_V1_VENDOR_ID, heusinkveld::HEUSINKVELD_HANDBRAKE_V1_PID),
        (heusinkveld::HEUSINKVELD_SHIFTER_VENDOR_ID, heusinkveld::HEUSINKVELD_SHIFTER_PID),
        (0x0000, 0x0000),
        (heusinkveld::HEUSINKVELD_VENDOR_ID, 0xFFFF),
    ]
    .iter()
    .map(|&(vid, pid)| {
        let model = heusinkveld::HeusinkveldModel::from_vid_pid(vid, pid);
        format!("VID=0x{vid:04X},PID=0x{pid:04X} -> {:?}", model)
    })
    .collect();
    assert_snapshot!(combos.join("\n"));
}

// ── PID-only model lookup (backwards compat) ─────────────────────────────────

#[test]
fn test_snapshot_model_from_pid_all_known() {
    let pids: Vec<String> = [
        heusinkveld::HEUSINKVELD_SPRINT_PID,
        heusinkveld::HEUSINKVELD_ULTIMATE_PID,
        heusinkveld::HEUSINKVELD_LEGACY_SPRINT_PID,
        heusinkveld::HEUSINKVELD_LEGACY_ULTIMATE_PID,
        heusinkveld::HEUSINKVELD_PRO_PID,
        heusinkveld::HEUSINKVELD_HANDBRAKE_V1_PID,
        heusinkveld::HEUSINKVELD_HANDBRAKE_V2_PID,
        heusinkveld::HEUSINKVELD_SHIFTER_PID,
        0xFFFF,
    ]
    .iter()
    .map(|&pid| {
        let model = heusinkveld::HeusinkveldModel::from_product_id(pid);
        format!("0x{pid:04X} -> {:?}", model)
    })
    .collect();
    assert_snapshot!(pids.join("\n"));
}

// ── is_heusinkveld_device VID check ──────────────────────────────────────────

#[test]
fn test_snapshot_is_heusinkveld_device_all_vids() {
    let vids: Vec<String> = [
        0x0000u16,
        heusinkveld::HEUSINKVELD_VENDOR_ID,
        heusinkveld::HEUSINKVELD_LEGACY_VENDOR_ID,
        heusinkveld::HEUSINKVELD_HANDBRAKE_V1_VENDOR_ID,
        heusinkveld::HEUSINKVELD_SHIFTER_VENDOR_ID,
        0x16D0, // Simucube VID
        0xFFFF,
    ]
    .iter()
    .map(|&vid| format!("0x{vid:04X}={}", heusinkveld::is_heusinkveld_device(vid)))
    .collect();
    assert_snapshot!(vids.join(", "));
}

// ── Error message formatting ─────────────────────────────────────────────────

#[test]
fn test_snapshot_error_invalid_report_size() {
    let err = heusinkveld::HeusinkveldError::InvalidReportSize {
        expected: 8,
        actual: 3,
    };
    assert_snapshot!(format!("{err}"));
}

#[test]
fn test_snapshot_error_invalid_pedal_value() {
    let err = heusinkveld::HeusinkveldError::InvalidPedalValue(0xFFFF);
    assert_snapshot!(format!("{err}"));
}

#[test]
fn test_snapshot_error_device_not_found() {
    let err = heusinkveld::HeusinkveldError::DeviceNotFound("no matching VID/PID".into());
    assert_snapshot!(format!("{err}"));
}

#[test]
fn test_snapshot_all_error_debug_format() {
    let errors: Vec<String> = [
        heusinkveld::HeusinkveldError::InvalidReportSize {
            expected: 8,
            actual: 0,
        },
        heusinkveld::HeusinkveldError::InvalidPedalValue(0),
        heusinkveld::HeusinkveldError::DeviceNotFound("test".into()),
    ]
    .iter()
    .map(|e| format!("{e:?}"))
    .collect();
    assert_snapshot!(errors.join("\n"));
}

// ── PedalStatus from_flags boundary ──────────────────────────────────────────

#[test]
fn test_snapshot_pedal_status_flags_0_through_7() {
    let results: Vec<String> = (0u8..=7)
        .map(|f| format!("0x{f:02X} -> {:?}", heusinkveld::PedalStatus::from_flags(f)))
        .collect();
    assert_snapshot!(results.join("\n"));
}

// ── Default input report ─────────────────────────────────────────────────────

#[test]
fn test_snapshot_default_input_report() {
    let report = heusinkveld::HeusinkveldInputReport::default();
    assert_snapshot!(format!(
        "throttle={}, brake={}, clutch={}, status=0x{:02X}, connected={}, calibrated={}, fault={}",
        report.throttle,
        report.brake,
        report.clutch,
        report.status,
        report.is_connected(),
        report.is_calibrated(),
        report.has_fault()
    ));
}

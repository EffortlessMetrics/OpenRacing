//! Deep pedal-specific protocol tests for Heusinkveld HID protocol.
//!
//! Covers pedal axis parsing, load cell force curves, calibration data
//! handling, and multi-pedal report parsing.

use hid_heusinkveld_protocol::{
    HeusinkveldError, HeusinkveldInputReport, HeusinkveldModel, HeusinkveldResult,
    MAX_LOAD_CELL_VALUE, PedalCapabilities, PedalModel, PedalStatus, REPORT_SIZE_INPUT, VENDOR_ID,
};

// ─── Helper ──────────────────────────────────────────────────────────────────

/// Build a raw 8-byte pedal report from axis values + status.
fn build_raw_report(throttle: u16, brake: u16, clutch: u16, status: u8) -> [u8; 8] {
    let mut buf = [0u8; 8];
    buf[0..2].copy_from_slice(&throttle.to_le_bytes());
    buf[2..4].copy_from_slice(&brake.to_le_bytes());
    buf[4..6].copy_from_slice(&clutch.to_le_bytes());
    buf[6] = status;
    buf
}

// ─── Pedal axis parsing ─────────────────────────────────────────────────────

#[test]
fn parse_throttle_axis_full_range() -> HeusinkveldResult<()> {
    let raw = build_raw_report(0xFFFF, 0, 0, 0x03);
    let report = HeusinkveldInputReport::parse(&raw)?;
    assert_eq!(report.throttle, MAX_LOAD_CELL_VALUE);
    assert!((report.throttle_normalized() - 1.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn parse_brake_axis_full_range() -> HeusinkveldResult<()> {
    let raw = build_raw_report(0, 0xFFFF, 0, 0x03);
    let report = HeusinkveldInputReport::parse(&raw)?;
    assert_eq!(report.brake, MAX_LOAD_CELL_VALUE);
    assert!((report.brake_normalized() - 1.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn parse_clutch_axis_full_range() -> HeusinkveldResult<()> {
    let raw = build_raw_report(0, 0, 0xFFFF, 0x03);
    let report = HeusinkveldInputReport::parse(&raw)?;
    assert_eq!(report.clutch, MAX_LOAD_CELL_VALUE);
    assert!((report.clutch_normalized() - 1.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn parse_all_axes_zero() -> HeusinkveldResult<()> {
    let raw = build_raw_report(0, 0, 0, 0x03);
    let report = HeusinkveldInputReport::parse(&raw)?;
    assert_eq!(report.throttle, 0);
    assert_eq!(report.brake, 0);
    assert_eq!(report.clutch, 0);
    assert!((report.throttle_normalized()).abs() < f32::EPSILON);
    assert!((report.brake_normalized()).abs() < f32::EPSILON);
    assert!((report.clutch_normalized()).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn parse_axes_midpoint_values() -> HeusinkveldResult<()> {
    let mid = MAX_LOAD_CELL_VALUE / 2;
    let raw = build_raw_report(mid, mid, mid, 0x03);
    let report = HeusinkveldInputReport::parse(&raw)?;
    assert_eq!(report.throttle, mid);
    assert_eq!(report.brake, mid);
    assert_eq!(report.clutch, mid);
    let expected = mid as f32 / MAX_LOAD_CELL_VALUE as f32;
    assert!((report.throttle_normalized() - expected).abs() < 0.001);
    Ok(())
}

// ─── Load cell force curves ─────────────────────────────────────────────────

#[test]
fn load_cell_normalized_monotonic_over_range() {
    // Normalized output must be monotonically non-decreasing as raw value increases.
    let steps: Vec<u16> = (0..=10)
        .map(|i| (MAX_LOAD_CELL_VALUE as u32 * i / 10) as u16)
        .collect();
    let mut prev = 0.0_f32;
    for &raw_val in &steps {
        let report = HeusinkveldInputReport {
            throttle: raw_val,
            ..Default::default()
        };
        let norm = report.throttle_normalized();
        assert!(
            norm >= prev,
            "normalized must be monotonic: {norm} < {prev}"
        );
        prev = norm;
    }
}

#[test]
fn load_cell_max_value_constant_is_u16_max() {
    assert_eq!(MAX_LOAD_CELL_VALUE, u16::MAX);
}

#[test]
fn load_cell_normalized_bounds() {
    let report_zero = HeusinkveldInputReport {
        throttle: 0,
        brake: 0,
        clutch: 0,
        ..Default::default()
    };
    assert!(report_zero.throttle_normalized() >= 0.0);
    assert!(report_zero.brake_normalized() >= 0.0);
    assert!(report_zero.clutch_normalized() >= 0.0);

    let report_max = HeusinkveldInputReport {
        throttle: MAX_LOAD_CELL_VALUE,
        brake: MAX_LOAD_CELL_VALUE,
        clutch: MAX_LOAD_CELL_VALUE,
        ..Default::default()
    };
    assert!(report_max.throttle_normalized() <= 1.0);
    assert!(report_max.brake_normalized() <= 1.0);
    assert!(report_max.clutch_normalized() <= 1.0);
}

#[test]
fn force_curve_sprint_max_load() {
    let caps = PedalCapabilities::for_model(PedalModel::Sprint);
    assert_eq!(caps.max_load_kg, 55.0);
    assert!(caps.has_load_cell);
}

#[test]
fn force_curve_ultimate_max_load() {
    let caps = PedalCapabilities::for_model(PedalModel::Ultimate);
    assert_eq!(caps.max_load_kg, 140.0);
    assert!(caps.has_load_cell);
}

#[test]
fn force_curve_pro_max_load() {
    let caps = PedalCapabilities::for_model(PedalModel::Pro);
    assert_eq!(caps.max_load_kg, 200.0);
    assert!(caps.has_load_cell);
    assert!(caps.has_hydraulic_damping);
}

// ─── Calibration data handling ──────────────────────────────────────────────

#[test]
fn calibration_status_flags_connected_and_calibrated() {
    assert_eq!(PedalStatus::from_flags(0x03), PedalStatus::Ready);
}

#[test]
fn calibration_status_connected_not_calibrated() {
    assert_eq!(PedalStatus::from_flags(0x01), PedalStatus::Calibrating);
}

#[test]
fn calibration_status_disconnected() {
    assert_eq!(PedalStatus::from_flags(0x00), PedalStatus::Disconnected);
}

#[test]
fn calibration_status_error_flag() {
    // 0x07 = connected (0x01) + calibrated (0x02) + error (0x04)
    assert_eq!(PedalStatus::from_flags(0x07), PedalStatus::Error);
}

#[test]
fn calibration_status_error_without_calibration() {
    // 0x05 = connected (0x01) + error (0x04) but NOT calibrated
    // connected but not calibrated → Calibrating (checked before error)
    assert_eq!(PedalStatus::from_flags(0x05), PedalStatus::Calibrating);
}

#[test]
fn report_status_methods_match_flags() -> HeusinkveldResult<()> {
    let raw = build_raw_report(0, 0, 0, 0x03);
    let report = HeusinkveldInputReport::parse(&raw)?;
    assert!(report.is_connected());
    assert!(report.is_calibrated());
    assert!(!report.has_fault());
    Ok(())
}

#[test]
fn report_fault_flag_detected() -> HeusinkveldResult<()> {
    let raw = build_raw_report(0, 0, 0, 0x07);
    let report = HeusinkveldInputReport::parse(&raw)?;
    assert!(report.has_fault());
    Ok(())
}

// ─── Multi-pedal report parsing ─────────────────────────────────────────────

#[test]
fn multi_pedal_report_sprint_two_pedals() {
    // Sprint has 2 pedals (throttle + brake), clutch should be 0 in a typical
    // 2-pedal setup but the report always carries 3 channels.
    let caps = PedalCapabilities::for_model(PedalModel::Sprint);
    assert_eq!(caps.pedal_count, 2);

    let model = HeusinkveldModel::from_product_id(hid_heusinkveld_protocol::HEUSINKVELD_SPRINT_PID);
    assert_eq!(model.pedal_count(), 2);
}

#[test]
fn multi_pedal_report_ultimate_three_pedals() {
    let caps = PedalCapabilities::for_model(PedalModel::Ultimate);
    assert_eq!(caps.pedal_count, 3);

    let model =
        HeusinkveldModel::from_product_id(hid_heusinkveld_protocol::HEUSINKVELD_ULTIMATE_PID);
    assert_eq!(model.pedal_count(), 3);
}

#[test]
fn report_too_short_returns_error() {
    let short_data = [0u8; 4];
    let result = HeusinkveldInputReport::parse(&short_data);
    assert!(result.is_err());
    match result {
        Err(HeusinkveldError::InvalidReportSize { expected, actual }) => {
            assert_eq!(expected, REPORT_SIZE_INPUT);
            assert_eq!(actual, 4);
        }
        _ => panic!("expected InvalidReportSize error"),
    }
}

#[test]
fn report_default_has_connected_calibrated_status() {
    let report = HeusinkveldInputReport::default();
    assert!(report.is_connected());
    assert!(report.is_calibrated());
    assert!(!report.has_fault());
    assert_eq!(report.throttle, 0);
    assert_eq!(report.brake, 0);
    assert_eq!(report.clutch, 0);
}

#[test]
fn report_exactly_minimum_size_parses() -> HeusinkveldResult<()> {
    let raw = build_raw_report(100, 200, 300, 0x03);
    let report = HeusinkveldInputReport::parse(&raw)?;
    assert_eq!(report.throttle, 100);
    assert_eq!(report.brake, 200);
    assert_eq!(report.clutch, 300);
    Ok(())
}

#[test]
fn report_larger_than_minimum_still_parses() -> HeusinkveldResult<()> {
    let mut data = vec![0u8; 32];
    data[0..2].copy_from_slice(&500_u16.to_le_bytes());
    data[2..4].copy_from_slice(&600_u16.to_le_bytes());
    data[4..6].copy_from_slice(&700_u16.to_le_bytes());
    data[6] = 0x03;
    let report = HeusinkveldInputReport::parse(&data)?;
    assert_eq!(report.throttle, 500);
    assert_eq!(report.brake, 600);
    assert_eq!(report.clutch, 700);
    Ok(())
}

#[test]
fn vendor_id_constant_matches_ids_module() {
    assert_eq!(VENDOR_ID, hid_heusinkveld_protocol::HEUSINKVELD_VENDOR_ID);
}

#[test]
fn pedal_model_unknown_defaults_to_ultimate_caps() {
    let caps = PedalCapabilities::for_model(PedalModel::Unknown);
    let default = PedalCapabilities::default();
    assert_eq!(caps.max_load_kg, default.max_load_kg);
    assert_eq!(caps.pedal_count, default.pedal_count);
}

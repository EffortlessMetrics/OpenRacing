//! Deep integration tests for the Heusinkveld HID protocol crate.
//!
//! Covers: all device variants (Sprint/Ultimate/Pro + peripherals),
//! pedal report parsing with adversarial inputs, calibration routines,
//! VID/PID validation across legacy and current firmware, proptest fuzzing,
//! and comprehensive error handling.

use hid_heusinkveld_protocol::{
    HEUSINKVELD_HANDBRAKE_V1_PID, HEUSINKVELD_HANDBRAKE_V1_VENDOR_ID, HEUSINKVELD_HANDBRAKE_V2_PID,
    HEUSINKVELD_LEGACY_SPRINT_PID, HEUSINKVELD_LEGACY_ULTIMATE_PID, HEUSINKVELD_LEGACY_VENDOR_ID,
    HEUSINKVELD_PRO_PID, HEUSINKVELD_SHIFTER_PID, HEUSINKVELD_SHIFTER_VENDOR_ID,
    HEUSINKVELD_SPRINT_PID, HEUSINKVELD_ULTIMATE_PID, HEUSINKVELD_VENDOR_ID, HeusinkveldError,
    HeusinkveldInputReport, HeusinkveldModel, HeusinkveldResult, MAX_LOAD_CELL_VALUE,
    PRODUCT_ID_PRO, PRODUCT_ID_SPRINT, PRODUCT_ID_ULTIMATE, PedalCapabilities, PedalModel,
    PedalStatus, REPORT_SIZE_INPUT, VENDOR_ID, heusinkveld_model_from_info, is_heusinkveld_device,
};

use proptest::prelude::*;

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn build_raw(throttle: u16, brake: u16, clutch: u16, status: u8) -> Vec<u8> {
    let mut buf = vec![0u8; REPORT_SIZE_INPUT];
    buf[0..2].copy_from_slice(&throttle.to_le_bytes());
    buf[2..4].copy_from_slice(&brake.to_le_bytes());
    buf[4..6].copy_from_slice(&clutch.to_le_bytes());
    buf[6] = status;
    buf
}

// ═══════════════════════════════════════════════════════════════════════════════
// Device variant identification
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn sprint_identified_via_current_vid_pid() -> Result<(), Box<dyn std::error::Error>> {
    let model = HeusinkveldModel::from_vid_pid(HEUSINKVELD_VENDOR_ID, HEUSINKVELD_SPRINT_PID);
    assert_eq!(model, HeusinkveldModel::Sprint);
    assert_eq!(model.display_name(), "Heusinkveld Sprint");
    assert_eq!(model.pedal_count(), 2);
    assert!((model.max_load_kg() - 55.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn sprint_identified_via_legacy_vid_pid() -> Result<(), Box<dyn std::error::Error>> {
    let model =
        HeusinkveldModel::from_vid_pid(HEUSINKVELD_LEGACY_VENDOR_ID, HEUSINKVELD_LEGACY_SPRINT_PID);
    assert_eq!(model, HeusinkveldModel::Sprint);
    Ok(())
}

#[test]
fn ultimate_identified_via_current_vid_pid() -> Result<(), Box<dyn std::error::Error>> {
    let model = HeusinkveldModel::from_vid_pid(HEUSINKVELD_VENDOR_ID, HEUSINKVELD_ULTIMATE_PID);
    assert_eq!(model, HeusinkveldModel::Ultimate);
    assert_eq!(model.display_name(), "Heusinkveld Ultimate+");
    assert_eq!(model.pedal_count(), 3);
    assert!((model.max_load_kg() - 140.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn ultimate_identified_via_legacy_vid_pid() -> Result<(), Box<dyn std::error::Error>> {
    let model = HeusinkveldModel::from_vid_pid(
        HEUSINKVELD_LEGACY_VENDOR_ID,
        HEUSINKVELD_LEGACY_ULTIMATE_PID,
    );
    assert_eq!(model, HeusinkveldModel::Ultimate);
    Ok(())
}

#[test]
fn pro_identified_via_legacy_vid_pid() -> Result<(), Box<dyn std::error::Error>> {
    let model = HeusinkveldModel::from_vid_pid(HEUSINKVELD_LEGACY_VENDOR_ID, HEUSINKVELD_PRO_PID);
    assert_eq!(model, HeusinkveldModel::Pro);
    assert_eq!(model.display_name(), "Heusinkveld Pro");
    assert_eq!(model.pedal_count(), 3);
    assert!((model.max_load_kg() - 200.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn handbrake_v1_identified() -> Result<(), Box<dyn std::error::Error>> {
    let model = HeusinkveldModel::from_vid_pid(
        HEUSINKVELD_HANDBRAKE_V1_VENDOR_ID,
        HEUSINKVELD_HANDBRAKE_V1_PID,
    );
    assert_eq!(model, HeusinkveldModel::HandbrakeV1);
    assert_eq!(model.pedal_count(), 0);
    assert!((model.max_load_kg()).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn handbrake_v2_identified() -> Result<(), Box<dyn std::error::Error>> {
    let model = HeusinkveldModel::from_vid_pid(HEUSINKVELD_VENDOR_ID, HEUSINKVELD_HANDBRAKE_V2_PID);
    assert_eq!(model, HeusinkveldModel::HandbrakeV2);
    assert_eq!(model.pedal_count(), 0);
    Ok(())
}

#[test]
fn sequential_shifter_identified() -> Result<(), Box<dyn std::error::Error>> {
    let model =
        HeusinkveldModel::from_vid_pid(HEUSINKVELD_SHIFTER_VENDOR_ID, HEUSINKVELD_SHIFTER_PID);
    assert_eq!(model, HeusinkveldModel::SequentialShifter);
    assert_eq!(model.pedal_count(), 0);
    assert!((model.max_load_kg()).abs() < f32::EPSILON);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// VID/PID validation
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn all_known_vendor_ids_recognised() -> Result<(), Box<dyn std::error::Error>> {
    let vids = [
        HEUSINKVELD_VENDOR_ID,
        HEUSINKVELD_LEGACY_VENDOR_ID,
        HEUSINKVELD_HANDBRAKE_V1_VENDOR_ID,
        HEUSINKVELD_SHIFTER_VENDOR_ID,
    ];
    for vid in vids {
        assert!(
            is_heusinkveld_device(vid),
            "VID 0x{vid:04X} should be recognised"
        );
    }
    Ok(())
}

#[test]
fn foreign_vendor_ids_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let foreign = [0x0000, 0xFFFF, 0x2433, 0x16D0, 0x0483];
    for vid in foreign {
        assert!(
            !is_heusinkveld_device(vid),
            "VID 0x{vid:04X} should NOT be recognised as Heusinkveld"
        );
    }
    Ok(())
}

#[test]
fn vid_pid_cross_vendor_mismatch_returns_unknown() -> Result<(), Box<dyn std::error::Error>> {
    // Current PID with legacy VID should not match (sprint PID 0x1001 not in legacy table)
    let model =
        HeusinkveldModel::from_vid_pid(HEUSINKVELD_LEGACY_VENDOR_ID, HEUSINKVELD_SPRINT_PID);
    assert_eq!(model, HeusinkveldModel::Unknown);
    // Legacy PID with current VID
    let model =
        HeusinkveldModel::from_vid_pid(HEUSINKVELD_VENDOR_ID, HEUSINKVELD_LEGACY_SPRINT_PID);
    assert_eq!(model, HeusinkveldModel::Unknown);
    Ok(())
}

#[test]
fn lib_level_constants_match_ids_module() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(VENDOR_ID, HEUSINKVELD_VENDOR_ID);
    assert_eq!(PRODUCT_ID_SPRINT, HEUSINKVELD_SPRINT_PID);
    assert_eq!(PRODUCT_ID_ULTIMATE, HEUSINKVELD_ULTIMATE_PID);
    assert_eq!(PRODUCT_ID_PRO, HEUSINKVELD_PRO_PID);
    Ok(())
}

#[test]
fn heusinkveld_model_from_info_delegates_correctly() -> Result<(), Box<dyn std::error::Error>> {
    let pairs: Vec<(u16, u16, HeusinkveldModel)> = vec![
        (
            HEUSINKVELD_VENDOR_ID,
            HEUSINKVELD_SPRINT_PID,
            HeusinkveldModel::Sprint,
        ),
        (
            HEUSINKVELD_VENDOR_ID,
            HEUSINKVELD_ULTIMATE_PID,
            HeusinkveldModel::Ultimate,
        ),
        (
            HEUSINKVELD_LEGACY_VENDOR_ID,
            HEUSINKVELD_PRO_PID,
            HeusinkveldModel::Pro,
        ),
        (0x0000, 0x0000, HeusinkveldModel::Unknown),
    ];
    for (vid, pid, expected) in pairs {
        assert_eq!(heusinkveld_model_from_info(vid, pid), expected);
    }
    Ok(())
}

#[test]
fn all_pid_constants_are_nonzero_and_unique() -> Result<(), Box<dyn std::error::Error>> {
    let pids = [
        HEUSINKVELD_SPRINT_PID,
        HEUSINKVELD_HANDBRAKE_V2_PID,
        HEUSINKVELD_ULTIMATE_PID,
        HEUSINKVELD_HANDBRAKE_V1_PID,
        HEUSINKVELD_SHIFTER_PID,
        HEUSINKVELD_LEGACY_SPRINT_PID,
        HEUSINKVELD_LEGACY_ULTIMATE_PID,
        HEUSINKVELD_PRO_PID,
    ];
    for &pid in &pids {
        assert_ne!(pid, 0, "PID must be nonzero");
    }
    // Uniqueness
    let mut sorted = pids;
    sorted.sort();
    for window in sorted.windows(2) {
        assert_ne!(window[0], window[1], "PIDs must be unique");
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Pedal report parsing — adversarial & boundary inputs
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn parse_report_exactly_minimum_size() -> HeusinkveldResult<()> {
    let raw = build_raw(1000, 2000, 3000, 0x03);
    let report = HeusinkveldInputReport::parse(&raw)?;
    assert_eq!(report.throttle, 1000);
    assert_eq!(report.brake, 2000);
    assert_eq!(report.clutch, 3000);
    Ok(())
}

#[test]
fn parse_report_with_trailing_bytes() -> HeusinkveldResult<()> {
    let mut raw = build_raw(100, 200, 300, 0x03);
    raw.extend_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]);
    let report = HeusinkveldInputReport::parse(&raw)?;
    assert_eq!(report.throttle, 100);
    assert_eq!(report.brake, 200);
    assert_eq!(report.clutch, 300);
    Ok(())
}

#[test]
fn parse_report_empty_buffer_fails() {
    let result = HeusinkveldInputReport::parse(&[]);
    assert!(result.is_err());
    if let Err(HeusinkveldError::InvalidReportSize { expected, actual }) = result {
        assert_eq!(expected, REPORT_SIZE_INPUT);
        assert_eq!(actual, 0);
    } else {
        panic!("expected InvalidReportSize");
    }
}

#[test]
fn parse_report_one_byte_short_fails() {
    let result = HeusinkveldInputReport::parse(&[0u8; REPORT_SIZE_INPUT - 1]);
    assert!(result.is_err());
}

#[test]
fn parse_report_all_ff_bytes() -> HeusinkveldResult<()> {
    let raw = vec![0xFF; REPORT_SIZE_INPUT];
    let report = HeusinkveldInputReport::parse(&raw)?;
    assert_eq!(report.throttle, 0xFFFF);
    assert_eq!(report.brake, 0xFFFF);
    assert_eq!(report.clutch, 0xFFFF);
    assert_eq!(report.status, 0xFF);
    Ok(())
}

#[test]
fn parse_report_all_zeros() -> HeusinkveldResult<()> {
    let raw = vec![0x00; REPORT_SIZE_INPUT];
    let report = HeusinkveldInputReport::parse(&raw)?;
    assert_eq!(report.throttle, 0);
    assert_eq!(report.brake, 0);
    assert_eq!(report.clutch, 0);
    assert_eq!(report.status, 0);
    Ok(())
}

#[test]
fn parse_report_asymmetric_pedal_values() -> HeusinkveldResult<()> {
    let raw = build_raw(0x0001, 0x7FFF, 0xFFFE, 0x03);
    let report = HeusinkveldInputReport::parse(&raw)?;
    assert_eq!(report.throttle, 1);
    assert_eq!(report.brake, 0x7FFF);
    assert_eq!(report.clutch, 0xFFFE);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Normalization fidelity
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn normalized_value_at_one() -> Result<(), Box<dyn std::error::Error>> {
    let report = HeusinkveldInputReport {
        throttle: MAX_LOAD_CELL_VALUE,
        brake: MAX_LOAD_CELL_VALUE,
        clutch: MAX_LOAD_CELL_VALUE,
        ..Default::default()
    };
    assert!((report.throttle_normalized() - 1.0).abs() < f32::EPSILON);
    assert!((report.brake_normalized() - 1.0).abs() < f32::EPSILON);
    assert!((report.clutch_normalized() - 1.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn normalized_value_at_zero() -> Result<(), Box<dyn std::error::Error>> {
    let report = HeusinkveldInputReport {
        throttle: 0,
        brake: 0,
        clutch: 0,
        ..Default::default()
    };
    assert!(report.throttle_normalized().abs() < f32::EPSILON);
    assert!(report.brake_normalized().abs() < f32::EPSILON);
    assert!(report.clutch_normalized().abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn normalized_quarter_points_are_monotonic() -> Result<(), Box<dyn std::error::Error>> {
    let vals: [u16; 5] = [0, 16383, 32767, 49151, 65535];
    let mut prev = -1.0_f32;
    for &v in &vals {
        let report = HeusinkveldInputReport {
            throttle: v,
            ..Default::default()
        };
        let norm = report.throttle_normalized();
        assert!(norm > prev, "expected monotonic: {norm} > {prev}");
        prev = norm;
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Calibration routines
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn calibration_state_machine_transitions() -> Result<(), Box<dyn std::error::Error>> {
    // Disconnected → Calibrating → Ready → Error
    assert_eq!(PedalStatus::from_flags(0x00), PedalStatus::Disconnected);
    assert_eq!(PedalStatus::from_flags(0x01), PedalStatus::Calibrating);
    assert_eq!(PedalStatus::from_flags(0x03), PedalStatus::Ready);
    assert_eq!(PedalStatus::from_flags(0x07), PedalStatus::Error);
    Ok(())
}

#[test]
fn calibration_error_overrides_ready_when_fault_bit_set() -> Result<(), Box<dyn std::error::Error>>
{
    // status 0x07 = connected + calibrated + fault → Error
    let raw = build_raw(0, 0, 0, 0x07);
    let report = HeusinkveldInputReport::parse(&raw)?;
    assert!(report.is_connected());
    assert!(report.is_calibrated());
    assert!(report.has_fault());
    assert_eq!(PedalStatus::from_flags(0x07), PedalStatus::Error);
    Ok(())
}

#[test]
fn calibration_not_ready_until_bit1_set() -> Result<(), Box<dyn std::error::Error>> {
    // 0x01 = connected but not calibrated
    assert_eq!(PedalStatus::from_flags(0x01), PedalStatus::Calibrating);
    // 0x03 = connected + calibrated
    assert_eq!(PedalStatus::from_flags(0x03), PedalStatus::Ready);
    Ok(())
}

#[test]
fn calibrating_with_fault_bit_stays_calibrating() -> Result<(), Box<dyn std::error::Error>> {
    // 0x05 = connected (0x01) + fault (0x04) but NOT calibrated
    // The from_flags logic checks calibration first → Calibrating
    assert_eq!(PedalStatus::from_flags(0x05), PedalStatus::Calibrating);
    Ok(())
}

#[test]
fn all_status_byte_values_produce_valid_enum() -> Result<(), Box<dyn std::error::Error>> {
    for flags in 0..=0xFF_u8 {
        let status = PedalStatus::from_flags(flags);
        // Must be one of the four valid states
        assert!(matches!(
            status,
            PedalStatus::Disconnected
                | PedalStatus::Calibrating
                | PedalStatus::Ready
                | PedalStatus::Error
        ));
    }
    Ok(())
}

#[test]
fn report_status_bits_match_status_methods() -> HeusinkveldResult<()> {
    // Test all combinations of the first 3 bits
    for flags in 0..=0x07_u8 {
        let raw = build_raw(0, 0, 0, flags);
        let report = HeusinkveldInputReport::parse(&raw)?;
        assert_eq!(report.is_connected(), (flags & 0x01) != 0);
        assert_eq!(report.is_calibrated(), (flags & 0x02) != 0);
        assert_eq!(report.has_fault(), (flags & 0x04) != 0);
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// PedalCapabilities per model
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn capabilities_sprint_model() -> Result<(), Box<dyn std::error::Error>> {
    let caps = PedalCapabilities::for_model(PedalModel::Sprint);
    assert_eq!(caps.max_load_kg, 55.0);
    assert_eq!(caps.pedal_count, 2);
    assert!(caps.has_load_cell);
    assert!(caps.has_hydraulic_damping);
    Ok(())
}

#[test]
fn capabilities_ultimate_model() -> Result<(), Box<dyn std::error::Error>> {
    let caps = PedalCapabilities::for_model(PedalModel::Ultimate);
    assert_eq!(caps.max_load_kg, 140.0);
    assert_eq!(caps.pedal_count, 3);
    assert!(caps.has_load_cell);
    assert!(caps.has_hydraulic_damping);
    Ok(())
}

#[test]
fn capabilities_pro_model() -> Result<(), Box<dyn std::error::Error>> {
    let caps = PedalCapabilities::for_model(PedalModel::Pro);
    assert_eq!(caps.max_load_kg, 200.0);
    assert_eq!(caps.pedal_count, 3);
    assert!(caps.has_load_cell);
    assert!(caps.has_hydraulic_damping);
    Ok(())
}

#[test]
fn capabilities_unknown_model_defaults_to_ultimate_like() -> Result<(), Box<dyn std::error::Error>>
{
    let caps = PedalCapabilities::for_model(PedalModel::Unknown);
    let default = PedalCapabilities::default();
    assert_eq!(caps.max_load_kg, default.max_load_kg);
    assert_eq!(caps.pedal_count, default.pedal_count);
    Ok(())
}

#[test]
fn max_load_ordering_sprint_lt_ultimate_lt_pro() -> Result<(), Box<dyn std::error::Error>> {
    let sprint = PedalCapabilities::for_model(PedalModel::Sprint);
    let ultimate = PedalCapabilities::for_model(PedalModel::Ultimate);
    let pro = PedalCapabilities::for_model(PedalModel::Pro);
    assert!(sprint.max_load_kg < ultimate.max_load_kg);
    assert!(ultimate.max_load_kg < pro.max_load_kg);
    Ok(())
}

#[test]
fn all_pedal_models_have_load_cell() -> Result<(), Box<dyn std::error::Error>> {
    for model in [PedalModel::Sprint, PedalModel::Ultimate, PedalModel::Pro] {
        let caps = PedalCapabilities::for_model(model);
        assert!(caps.has_load_cell, "{model:?} should have load cell");
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// HeusinkveldModel → capabilities cross-check
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn model_max_load_matches_capabilities() -> Result<(), Box<dyn std::error::Error>> {
    let pairs: Vec<(HeusinkveldModel, PedalModel)> = vec![
        (HeusinkveldModel::Sprint, PedalModel::Sprint),
        (HeusinkveldModel::Ultimate, PedalModel::Ultimate),
        (HeusinkveldModel::Pro, PedalModel::Pro),
    ];
    for (hk_model, pedal_model) in pairs {
        let caps = PedalCapabilities::for_model(pedal_model);
        assert!(
            (hk_model.max_load_kg() - caps.max_load_kg).abs() < f32::EPSILON,
            "{hk_model:?} max_load mismatch"
        );
    }
    Ok(())
}

#[test]
fn model_pedal_count_matches_capabilities() -> Result<(), Box<dyn std::error::Error>> {
    let pairs: Vec<(HeusinkveldModel, PedalModel)> = vec![
        (HeusinkveldModel::Sprint, PedalModel::Sprint),
        (HeusinkveldModel::Ultimate, PedalModel::Ultimate),
        (HeusinkveldModel::Pro, PedalModel::Pro),
    ];
    for (hk_model, pedal_model) in pairs {
        let caps = PedalCapabilities::for_model(pedal_model);
        assert_eq!(
            hk_model.pedal_count(),
            caps.pedal_count,
            "{hk_model:?} pedal_count mismatch"
        );
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Error handling
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn error_display_invalid_report_size() -> Result<(), Box<dyn std::error::Error>> {
    let err = HeusinkveldError::InvalidReportSize {
        expected: 8,
        actual: 3,
    };
    let msg = format!("{err}");
    assert!(msg.contains("8"), "message should contain expected size");
    assert!(msg.contains("3"), "message should contain actual size");
    Ok(())
}

#[test]
fn error_display_invalid_pedal_value() -> Result<(), Box<dyn std::error::Error>> {
    let err = HeusinkveldError::InvalidPedalValue(9999);
    let msg = format!("{err}");
    assert!(msg.contains("9999"));
    Ok(())
}

#[test]
fn error_display_device_not_found() -> Result<(), Box<dyn std::error::Error>> {
    let err = HeusinkveldError::DeviceNotFound("test device".to_string());
    let msg = format!("{err}");
    assert!(msg.contains("test device"));
    Ok(())
}

#[test]
fn error_is_std_error() -> Result<(), Box<dyn std::error::Error>> {
    let err: Box<dyn std::error::Error> = Box::new(HeusinkveldError::InvalidPedalValue(42));
    let _msg = format!("{err}");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Default report integrity
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn default_report_is_idle_connected_calibrated() -> Result<(), Box<dyn std::error::Error>> {
    let report = HeusinkveldInputReport::default();
    assert_eq!(report.throttle, 0);
    assert_eq!(report.brake, 0);
    assert_eq!(report.clutch, 0);
    assert!(report.is_connected());
    assert!(report.is_calibrated());
    assert!(!report.has_fault());
    assert_eq!(report.status, 0x03);
    Ok(())
}

#[test]
fn max_load_cell_value_is_u16_max() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(MAX_LOAD_CELL_VALUE, u16::MAX);
    assert_eq!(MAX_LOAD_CELL_VALUE, 0xFFFF);
    Ok(())
}

#[test]
fn report_size_constant_is_8() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(REPORT_SIZE_INPUT, 8);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Rapid sequential parsing (stress)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn parse_1000_sequential_reports() -> HeusinkveldResult<()> {
    for i in 0..1000_u16 {
        let raw = build_raw(i, i.wrapping_mul(2), i.wrapping_mul(3), 0x03);
        let report = HeusinkveldInputReport::parse(&raw)?;
        assert_eq!(report.throttle, i);
    }
    Ok(())
}

#[test]
fn parse_boundary_sweep_all_u16_at_intervals() -> HeusinkveldResult<()> {
    // Test every 256th value across the u16 range
    let mut i: u16 = 0;
    loop {
        let raw = build_raw(i, i, i, 0x03);
        let report = HeusinkveldInputReport::parse(&raw)?;
        assert_eq!(report.throttle, i);
        let norm = report.throttle_normalized();
        assert!(norm >= 0.0 && norm <= 1.0);
        if i >= u16::MAX - 256 {
            break;
        }
        i = i.wrapping_add(256);
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Proptest fuzzing
// ═══════════════════════════════════════════════════════════════════════════════

proptest! {
    #[test]
    fn fuzz_parse_arbitrary_8_bytes(data in proptest::collection::vec(any::<u8>(), 8..=8)) {
        let result = HeusinkveldInputReport::parse(&data);
        // Must always succeed with exactly 8 bytes
        prop_assert!(result.is_ok());
    }

    #[test]
    fn fuzz_parse_short_buffer_always_fails(len in 0..8_usize) {
        let data = vec![0u8; len];
        let result = HeusinkveldInputReport::parse(&data);
        prop_assert!(result.is_err());
    }

    #[test]
    fn fuzz_parse_oversized_buffer_succeeds(extra in 1..128_usize) {
        let mut data = vec![0u8; REPORT_SIZE_INPUT + extra];
        data[6] = 0x03;
        let result = HeusinkveldInputReport::parse(&data);
        prop_assert!(result.is_ok());
    }

    #[test]
    fn fuzz_normalized_always_in_unit_range(throttle: u16, brake: u16, clutch: u16) {
        let report = HeusinkveldInputReport {
            throttle,
            brake,
            clutch,
            ..Default::default()
        };
        prop_assert!(report.throttle_normalized() >= 0.0);
        prop_assert!(report.throttle_normalized() <= 1.0);
        prop_assert!(report.brake_normalized() >= 0.0);
        prop_assert!(report.brake_normalized() <= 1.0);
        prop_assert!(report.clutch_normalized() >= 0.0);
        prop_assert!(report.clutch_normalized() <= 1.0);
    }

    #[test]
    fn fuzz_status_flags_always_valid(flags: u8) {
        let status = PedalStatus::from_flags(flags);
        prop_assert!(matches!(
            status,
            PedalStatus::Disconnected
                | PedalStatus::Calibrating
                | PedalStatus::Ready
                | PedalStatus::Error
        ));
    }

    #[test]
    fn fuzz_arbitrary_vid_pid_never_panics(vid: u16, pid: u16) {
        let model = HeusinkveldModel::from_vid_pid(vid, pid);
        let _name = model.display_name();
        let _load = model.max_load_kg();
        let _count = model.pedal_count();
    }

    #[test]
    fn fuzz_from_product_id_never_panics(pid: u16) {
        let model = HeusinkveldModel::from_product_id(pid);
        let _name = model.display_name();
    }

    #[test]
    fn fuzz_pedal_parse_round_trip(throttle: u16, brake: u16, clutch: u16, status: u8) {
        let raw = build_raw(throttle, brake, clutch, status);
        let report = HeusinkveldInputReport::parse(&raw).map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        prop_assert_eq!(report.throttle, throttle);
        prop_assert_eq!(report.brake, brake);
        prop_assert_eq!(report.clutch, clutch);
        prop_assert_eq!(report.status, status);
    }

    #[test]
    fn fuzz_max_load_always_non_negative(pid: u16) {
        let model = HeusinkveldModel::from_product_id(pid);
        prop_assert!(model.max_load_kg() >= 0.0);
        prop_assert!(model.max_load_kg().is_finite());
    }
}

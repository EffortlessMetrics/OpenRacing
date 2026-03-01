//! BDD end-to-end tests for the Heusinkveld pedal protocol stack.
//!
//! Each test follows a Given/When/Then pattern to verify input report parsing,
//! normalization, model identification, and error handling without real USB hardware.

use hid_heusinkveld_protocol::{
    HeusinkveldError, HeusinkveldInputReport, HeusinkveldModel, PedalCapabilities, PedalModel,
    PedalStatus, HEUSINKVELD_PRO_PID, HEUSINKVELD_SPRINT_PID, HEUSINKVELD_ULTIMATE_PID,
    HEUSINKVELD_VENDOR_ID, MAX_LOAD_CELL_VALUE, PRODUCT_ID_PRO, PRODUCT_ID_SPRINT,
    PRODUCT_ID_ULTIMATE, REPORT_SIZE_INPUT, VENDOR_ID, heusinkveld_model_from_info,
    is_heusinkveld_device,
};

/// Helper: build an 8-byte raw report from pedal axes and status.
fn build_raw_report(throttle: u16, brake: u16, clutch: u16, status: u8) -> [u8; 8] {
    let mut buf = [0u8; 8];
    buf[0..2].copy_from_slice(&throttle.to_le_bytes());
    buf[2..4].copy_from_slice(&brake.to_le_bytes());
    buf[4..6].copy_from_slice(&clutch.to_le_bytes());
    buf[6] = status;
    buf
}

// ─── Scenario 1: input report parses throttle/brake/clutch values ─────────────

#[test]
fn scenario_input_report_given_known_axes_when_parsed_then_values_match(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: a raw report with known pedal axis values
    let raw = build_raw_report(0x1000, 0x2000, 0x3000, 0x03);

    // When: parsed
    let report = HeusinkveldInputReport::parse(&raw)?;

    // Then: throttle, brake, clutch decode correctly
    assert_eq!(report.throttle, 0x1000);
    assert_eq!(report.brake, 0x2000);
    assert_eq!(report.clutch, 0x3000);

    Ok(())
}

// ─── Scenario 2: normalization maps raw to 0.0–1.0 range ─────────────────────

#[test]
fn scenario_normalization_given_half_range_when_normalized_then_approximately_half(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: axes at half of MAX_LOAD_CELL_VALUE (0xFFFF)
    let half = MAX_LOAD_CELL_VALUE / 2;
    let raw = build_raw_report(half, half, half, 0x03);

    // When: parsed and normalized
    let report = HeusinkveldInputReport::parse(&raw)?;

    // Then: normalized values are approximately 0.5
    assert!((report.throttle_normalized() - 0.5).abs() < 0.001);
    assert!((report.brake_normalized() - 0.5).abs() < 0.001);
    assert!((report.clutch_normalized() - 0.5).abs() < 0.001);

    Ok(())
}

#[test]
fn scenario_normalization_given_full_range_when_normalized_then_one(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: axes at maximum
    let raw = build_raw_report(MAX_LOAD_CELL_VALUE, MAX_LOAD_CELL_VALUE, MAX_LOAD_CELL_VALUE, 0x03);

    // When: parsed and normalized
    let report = HeusinkveldInputReport::parse(&raw)?;

    // Then: all axes normalize to 1.0
    assert!((report.throttle_normalized() - 1.0).abs() < f32::EPSILON);
    assert!((report.brake_normalized() - 1.0).abs() < f32::EPSILON);
    assert!((report.clutch_normalized() - 1.0).abs() < f32::EPSILON);

    Ok(())
}

#[test]
fn scenario_normalization_given_zero_when_normalized_then_zero(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: axes at zero
    let raw = build_raw_report(0, 0, 0, 0x03);

    // When: parsed and normalized
    let report = HeusinkveldInputReport::parse(&raw)?;

    // Then: all axes normalize to 0.0
    assert!((report.throttle_normalized()).abs() < f32::EPSILON);
    assert!((report.brake_normalized()).abs() < f32::EPSILON);
    assert!((report.clutch_normalized()).abs() < f32::EPSILON);

    Ok(())
}

// ─── Scenario 3: pedal model classification from product IDs ──────────────────

#[test]
fn scenario_model_given_sprint_pid_when_classified_then_sprint() {
    // Given: Sprint product ID
    let model = HeusinkveldModel::from_product_id(HEUSINKVELD_SPRINT_PID);

    // Then: model is Sprint
    assert_eq!(model, HeusinkveldModel::Sprint);
    assert_eq!(model.display_name(), "Heusinkveld Sprint");
}

#[test]
fn scenario_model_given_ultimate_pid_when_classified_then_ultimate() {
    // Given: Ultimate product ID
    let model = HeusinkveldModel::from_product_id(HEUSINKVELD_ULTIMATE_PID);

    // Then: model is Ultimate
    assert_eq!(model, HeusinkveldModel::Ultimate);
    assert_eq!(model.display_name(), "Heusinkveld Ultimate+");
}

#[test]
fn scenario_model_given_pro_pid_when_classified_then_pro() {
    // Given: Pro product ID
    let model = HeusinkveldModel::from_product_id(HEUSINKVELD_PRO_PID);

    // Then: model is Pro
    assert_eq!(model, HeusinkveldModel::Pro);
    assert_eq!(model.display_name(), "Heusinkveld Pro");
}

#[test]
fn scenario_model_given_unknown_pid_when_classified_then_unknown() {
    // Given: unrecognized product ID
    let model = HeusinkveldModel::from_product_id(0xBEEF);

    // Then: model is Unknown
    assert_eq!(model, HeusinkveldModel::Unknown);
    assert_eq!(model.display_name(), "Unknown Heusinkveld Device");
}

// ─── Scenario 4: status flags — connected, calibrated, fault ──────────────────

#[test]
fn scenario_status_given_connected_calibrated_when_parsed_then_flags_correct(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: status byte 0x03 (connected + calibrated, no fault)
    let raw = build_raw_report(0, 0, 0, 0x03);

    // When: parsed
    let report = HeusinkveldInputReport::parse(&raw)?;

    // Then: connected and calibrated, no fault
    assert!(report.is_connected());
    assert!(report.is_calibrated());
    assert!(!report.has_fault());

    Ok(())
}

#[test]
fn scenario_status_given_connected_only_when_parsed_then_not_calibrated(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: status byte 0x01 (connected, not calibrated)
    let raw = build_raw_report(0, 0, 0, 0x01);
    let report = HeusinkveldInputReport::parse(&raw)?;

    // Then: connected but not calibrated
    assert!(report.is_connected());
    assert!(!report.is_calibrated());
    assert!(!report.has_fault());

    Ok(())
}

#[test]
fn scenario_status_given_fault_flag_when_parsed_then_has_fault(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: status byte 0x04 (fault set, not connected or calibrated)
    let raw = build_raw_report(0, 0, 0, 0x04);
    let report = HeusinkveldInputReport::parse(&raw)?;

    // Then: has fault, not connected, not calibrated
    assert!(!report.is_connected());
    assert!(!report.is_calibrated());
    assert!(report.has_fault());

    Ok(())
}

#[test]
fn scenario_status_given_all_flags_set_when_parsed_then_all_true(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: status byte 0x07 (all flags set)
    let raw = build_raw_report(0, 0, 0, 0x07);
    let report = HeusinkveldInputReport::parse(&raw)?;

    // Then: all flags are true
    assert!(report.is_connected());
    assert!(report.is_calibrated());
    assert!(report.has_fault());

    Ok(())
}

#[test]
fn scenario_status_given_zero_when_parsed_then_disconnected(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: status byte 0x00 (no flags)
    let raw = build_raw_report(0, 0, 0, 0x00);
    let report = HeusinkveldInputReport::parse(&raw)?;

    // Then: nothing is set
    assert!(!report.is_connected());
    assert!(!report.is_calibrated());
    assert!(!report.has_fault());

    Ok(())
}

// ─── Scenario 5: PedalStatus from_flags mapping ──────────────────────────────

#[test]
fn scenario_pedal_status_given_flags_when_converted_then_correct_state() {
    // Given/Then: each flag combo maps to correct PedalStatus
    assert_eq!(PedalStatus::from_flags(0x00), PedalStatus::Disconnected);
    assert_eq!(PedalStatus::from_flags(0x01), PedalStatus::Calibrating);
    assert_eq!(PedalStatus::from_flags(0x03), PedalStatus::Ready);
    assert_eq!(PedalStatus::from_flags(0x07), PedalStatus::Error);
}

// ─── Scenario 6: load cell value boundaries and edge cases ────────────────────

#[test]
fn scenario_load_cell_given_max_value_when_parsed_then_saturates_at_one(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: load cell at maximum u16 value
    let raw = build_raw_report(0xFFFF, 0xFFFF, 0xFFFF, 0x03);
    let report = HeusinkveldInputReport::parse(&raw)?;

    // Then: normalized value is exactly 1.0
    assert!((report.throttle_normalized() - 1.0).abs() < f32::EPSILON);
    assert!((report.brake_normalized() - 1.0).abs() < f32::EPSILON);
    assert!((report.clutch_normalized() - 1.0).abs() < f32::EPSILON);

    Ok(())
}

#[test]
fn scenario_load_cell_given_one_when_parsed_then_near_zero(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: load cell at minimum nonzero (1)
    let raw = build_raw_report(1, 1, 1, 0x03);
    let report = HeusinkveldInputReport::parse(&raw)?;

    // Then: normalized value is very small but positive
    let expected = 1.0 / MAX_LOAD_CELL_VALUE as f32;
    assert!((report.throttle_normalized() - expected).abs() < f32::EPSILON);
    assert!(report.throttle_normalized() > 0.0);

    Ok(())
}

#[test]
fn scenario_load_cell_given_independent_axes_when_parsed_then_no_crosstalk(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: only throttle has a value; brake and clutch are zero
    let raw = build_raw_report(0x8000, 0, 0, 0x03);
    let report = HeusinkveldInputReport::parse(&raw)?;

    // Then: brake and clutch remain at zero
    assert!(report.throttle_normalized() > 0.49);
    assert!((report.brake_normalized()).abs() < f32::EPSILON);
    assert!((report.clutch_normalized()).abs() < f32::EPSILON);

    Ok(())
}

// ─── Scenario 7: product ID constants verification ────────────────────────────

#[test]
fn scenario_product_ids_given_constants_when_checked_then_match_spec() {
    // Then: vendor ID constants match
    assert_eq!(VENDOR_ID, 0x16D0);
    assert_eq!(HEUSINKVELD_VENDOR_ID, 0x16D0);

    // Then: product ID constants have expected values
    assert_eq!(PRODUCT_ID_SPRINT, 0x1156);
    assert_eq!(PRODUCT_ID_ULTIMATE, 0x1157);
    assert_eq!(PRODUCT_ID_PRO, 0x1158);

    // Then: named constants match positional constants
    assert_eq!(HEUSINKVELD_SPRINT_PID, PRODUCT_ID_SPRINT);
    assert_eq!(HEUSINKVELD_ULTIMATE_PID, PRODUCT_ID_ULTIMATE);
    assert_eq!(HEUSINKVELD_PRO_PID, PRODUCT_ID_PRO);

    // Then: report and load cell constants
    assert_eq!(REPORT_SIZE_INPUT, 8);
    assert_eq!(MAX_LOAD_CELL_VALUE, 0xFFFF);
}

#[test]
fn scenario_vendor_detection_given_heusinkveld_vid_when_checked_then_recognized() {
    // Then: is_heusinkveld_device recognizes the correct vendor ID
    assert!(is_heusinkveld_device(VENDOR_ID));
    assert!(!is_heusinkveld_device(0x0000));
    assert!(!is_heusinkveld_device(0xFFFF));
}

// ─── Scenario 8: capability lookups per model ─────────────────────────────────

#[test]
fn scenario_capabilities_given_sprint_when_queried_then_correct_specs() {
    // Given: Sprint pedal model
    let caps = PedalCapabilities::for_model(PedalModel::Sprint);

    // Then: Sprint specs
    assert!((caps.max_load_kg - 55.0).abs() < f32::EPSILON);
    assert_eq!(caps.pedal_count, 2);
    assert!(caps.has_load_cell);
    assert!(caps.has_hydraulic_damping);
}

#[test]
fn scenario_capabilities_given_ultimate_when_queried_then_correct_specs() {
    // Given: Ultimate pedal model
    let caps = PedalCapabilities::for_model(PedalModel::Ultimate);

    // Then: Ultimate+ specs
    assert!((caps.max_load_kg - 140.0).abs() < f32::EPSILON);
    assert_eq!(caps.pedal_count, 3);
    assert!(caps.has_load_cell);
    assert!(caps.has_hydraulic_damping);
}

#[test]
fn scenario_capabilities_given_pro_when_queried_then_correct_specs() {
    // Given: Pro pedal model
    let caps = PedalCapabilities::for_model(PedalModel::Pro);

    // Then: Pro specs (200 kg, 3 pedals)
    assert!((caps.max_load_kg - 200.0).abs() < f32::EPSILON);
    assert_eq!(caps.pedal_count, 3);
    assert!(caps.has_load_cell);
    assert!(caps.has_hydraulic_damping);
}

#[test]
fn scenario_capabilities_given_unknown_when_queried_then_safe_defaults() {
    // Given: Unknown pedal model
    let caps = PedalCapabilities::for_model(PedalModel::Unknown);

    // Then: safe defaults (same as Ultimate)
    assert!((caps.max_load_kg - 140.0).abs() < f32::EPSILON);
    assert_eq!(caps.pedal_count, 3);
    assert!(caps.has_load_cell);
    assert!(caps.has_hydraulic_damping);
}

#[test]
fn scenario_capabilities_given_model_when_checked_then_consistent_with_heusinkveld_model() {
    // Then: HeusinkveldModel and PedalCapabilities agree on max load
    assert!(
        (HeusinkveldModel::Sprint.max_load_kg()
            - PedalCapabilities::for_model(PedalModel::Sprint).max_load_kg)
            .abs()
            < f32::EPSILON
    );
    assert!(
        (HeusinkveldModel::Ultimate.max_load_kg()
            - PedalCapabilities::for_model(PedalModel::Ultimate).max_load_kg)
            .abs()
            < f32::EPSILON
    );
    assert!(
        (HeusinkveldModel::Pro.max_load_kg()
            - PedalCapabilities::for_model(PedalModel::Pro).max_load_kg)
            .abs()
            < f32::EPSILON
    );

    // Then: pedal counts agree
    assert_eq!(
        HeusinkveldModel::Sprint.pedal_count(),
        PedalCapabilities::for_model(PedalModel::Sprint).pedal_count
    );
    assert_eq!(
        HeusinkveldModel::Ultimate.pedal_count(),
        PedalCapabilities::for_model(PedalModel::Ultimate).pedal_count
    );
    assert_eq!(
        HeusinkveldModel::Pro.pedal_count(),
        PedalCapabilities::for_model(PedalModel::Pro).pedal_count
    );
}

// ─── Scenario 9: model identification via vendor+product info ─────────────────

#[test]
fn scenario_model_from_info_given_valid_vid_pid_when_queried_then_correct() {
    // Then: correct vendor+product combinations return the right model
    assert_eq!(
        heusinkveld_model_from_info(HEUSINKVELD_VENDOR_ID, HEUSINKVELD_SPRINT_PID),
        HeusinkveldModel::Sprint
    );
    assert_eq!(
        heusinkveld_model_from_info(HEUSINKVELD_VENDOR_ID, HEUSINKVELD_ULTIMATE_PID),
        HeusinkveldModel::Ultimate
    );
    assert_eq!(
        heusinkveld_model_from_info(HEUSINKVELD_VENDOR_ID, HEUSINKVELD_PRO_PID),
        HeusinkveldModel::Pro
    );
}

#[test]
fn scenario_model_from_info_given_wrong_vendor_when_queried_then_unknown() {
    // Then: wrong vendor ID always returns Unknown regardless of PID
    assert_eq!(
        heusinkveld_model_from_info(0x0000, HEUSINKVELD_SPRINT_PID),
        HeusinkveldModel::Unknown
    );
    assert_eq!(
        heusinkveld_model_from_info(0xFFFF, HEUSINKVELD_ULTIMATE_PID),
        HeusinkveldModel::Unknown
    );
}

#[test]
fn scenario_model_from_info_given_unknown_pid_when_queried_then_unknown() {
    // Then: correct vendor but unknown PID returns Unknown
    assert_eq!(
        heusinkveld_model_from_info(HEUSINKVELD_VENDOR_ID, 0xFFFF),
        HeusinkveldModel::Unknown
    );
    assert_eq!(
        heusinkveld_model_from_info(HEUSINKVELD_VENDOR_ID, 0x0000),
        HeusinkveldModel::Unknown
    );
}

// ─── Scenario 10: error handling — short buffer ───────────────────────────────

#[test]
fn scenario_error_given_short_buffer_when_parsed_then_invalid_report_size() {
    // Given: a buffer shorter than REPORT_SIZE_INPUT (8)
    let short = [0u8; 4];

    // When: parsed
    let result = HeusinkveldInputReport::parse(&short);

    // Then: returns InvalidReportSize error with correct sizes
    let Err(err) = result else {
        panic!("expected InvalidReportSize error for short buffer");
    };
    assert!(
        matches!(err, HeusinkveldError::InvalidReportSize { expected: 8, actual: 4 }),
        "expected InvalidReportSize {{ expected: 8, actual: 4 }}, got: {err:?}"
    );
}

#[test]
fn scenario_error_given_empty_buffer_when_parsed_then_invalid_report_size() {
    // Given: an empty buffer
    let empty: [u8; 0] = [];

    // When: parsed
    let result = HeusinkveldInputReport::parse(&empty);

    // Then: returns InvalidReportSize error
    let Err(err) = result else {
        panic!("expected InvalidReportSize error for empty buffer");
    };
    assert!(
        matches!(err, HeusinkveldError::InvalidReportSize { expected: 8, actual: 0 }),
        "expected InvalidReportSize {{ expected: 8, actual: 0 }}, got: {err:?}"
    );
}

#[test]
fn scenario_error_given_seven_bytes_when_parsed_then_invalid_report_size() {
    // Given: buffer one byte too short
    let buf = [0u8; 7];

    // When: parsed
    let result = HeusinkveldInputReport::parse(&buf);

    // Then: rejected
    let Err(err) = result else {
        panic!("expected InvalidReportSize error for 7-byte buffer");
    };
    assert!(
        matches!(err, HeusinkveldError::InvalidReportSize { expected: 8, actual: 7 }),
        "expected InvalidReportSize {{ expected: 8, actual: 7 }}, got: {err:?}"
    );
}

// ─── Scenario 11: oversized buffer accepted (extra bytes ignored) ─────────────

#[test]
fn scenario_parse_given_oversized_buffer_when_parsed_then_succeeds(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: a buffer larger than 8 bytes with known values in the first 8
    let mut buf = [0u8; 64];
    buf[0..2].copy_from_slice(&0x1234u16.to_le_bytes()); // throttle
    buf[2..4].copy_from_slice(&0x5678u16.to_le_bytes()); // brake
    buf[4..6].copy_from_slice(&0x9ABCu16.to_le_bytes()); // clutch
    buf[6] = 0x03; // status

    // When: parsed
    let report = HeusinkveldInputReport::parse(&buf)?;

    // Then: first 8 bytes parsed correctly, extra bytes ignored
    assert_eq!(report.throttle, 0x1234);
    assert_eq!(report.brake, 0x5678);
    assert_eq!(report.clutch, 0x9ABC);
    assert_eq!(report.status, 0x03);

    Ok(())
}

// ─── Scenario 12: wire format byte layout verification ────────────────────────

#[test]
fn scenario_wire_format_given_known_values_when_encoded_then_bytes_match(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: specific pedal values
    let throttle: u16 = 0x0A0B;
    let brake: u16 = 0x0C0D;
    let clutch: u16 = 0x0E0F;
    let status: u8 = 0x07;
    let raw = build_raw_report(throttle, brake, clutch, status);

    // Then: wire layout is little-endian at expected offsets
    assert_eq!(raw[0], 0x0B, "throttle low byte at offset 0");
    assert_eq!(raw[1], 0x0A, "throttle high byte at offset 1");
    assert_eq!(raw[2], 0x0D, "brake low byte at offset 2");
    assert_eq!(raw[3], 0x0C, "brake high byte at offset 3");
    assert_eq!(raw[4], 0x0F, "clutch low byte at offset 4");
    assert_eq!(raw[5], 0x0E, "clutch high byte at offset 5");
    assert_eq!(raw[6], 0x07, "status at offset 6");
    assert_eq!(raw[7], 0x00, "padding at offset 7");

    // When: parsed back
    let report = HeusinkveldInputReport::parse(&raw)?;

    // Then: round-trips correctly
    assert_eq!(report.throttle, throttle);
    assert_eq!(report.brake, brake);
    assert_eq!(report.clutch, clutch);
    assert_eq!(report.status, status);

    Ok(())
}

#[test]
fn scenario_wire_format_given_report_size_when_checked_then_eight_bytes() {
    // Then: protocol defines 8-byte input reports
    assert_eq!(REPORT_SIZE_INPUT, 8);
}

// ─── Scenario 13: default input report has safe state ─────────────────────────

#[test]
fn scenario_default_given_default_report_when_inspected_then_safe_state() {
    // Given: a default-constructed input report
    let report = HeusinkveldInputReport::default();

    // Then: all axes are zero
    assert_eq!(report.throttle, 0);
    assert_eq!(report.brake, 0);
    assert_eq!(report.clutch, 0);

    // Then: default status is 0x03 (connected + calibrated)
    assert_eq!(report.status, 0x03);
    assert!(report.is_connected());
    assert!(report.is_calibrated());
    assert!(!report.has_fault());
}

// ─── Scenario 14: PedalModel default is Unknown ──────────────────────────────

#[test]
fn scenario_pedal_model_given_default_when_inspected_then_unknown() {
    // Given: default PedalModel
    let model = PedalModel::default();

    // Then: is Unknown
    assert_eq!(model, PedalModel::Unknown);
}

// ─── Scenario 15: PedalStatus default is Disconnected ─────────────────────────

#[test]
fn scenario_pedal_status_given_default_when_inspected_then_disconnected() {
    // Given: default PedalStatus
    let status = PedalStatus::default();

    // Then: is Disconnected
    assert_eq!(status, PedalStatus::Disconnected);
}

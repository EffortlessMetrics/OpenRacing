//! BDD end-to-end tests for the Cammus protocol stack.
//!
//! Each test follows a Given/When/Then pattern to verify observable hardware-ready
//! behaviors without real USB hardware.  Cammus encoding is purely functional
//! (I/O-free), so no virtual device is required.

use racing_wheel_hid_cammus_protocol::{
    CammusModel, FFB_REPORT_ID, FFB_REPORT_LEN, MODE_GAME, ParseError,
    PRODUCT_C12, PRODUCT_C5, PRODUCT_CP5_PEDALS, PRODUCT_LC100_PEDALS, REPORT_LEN,
    STEERING_RANGE_DEG, VENDOR_ID, encode_stop, encode_torque, is_cammus, parse, product_name,
};

// ─── Scenario 1: zero torque encoding produces zero magnitude ────────────────

#[test]
fn given_zero_torque_when_encoded_then_magnitude_is_zero() -> Result<(), Box<dyn std::error::Error>>
{
    // Given: a normalised torque of exactly 0.0
    let torque: f32 = 0.0;

    // When: encoded into an FFB output report
    let report = encode_torque(torque);

    // Then: the i16 torque field is zero
    let raw = i16::from_le_bytes([report[1], report[2]]);
    assert_eq!(raw, 0, "zero torque must produce zero magnitude");
    assert_eq!(report[0], FFB_REPORT_ID);
    assert_eq!(report[3], MODE_GAME);

    Ok(())
}

// ─── Scenario 2: full positive saturation ────────────────────────────────────

#[test]
fn given_full_positive_torque_when_encoded_then_saturates_to_i16_max(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: maximum positive normalised torque
    let torque: f32 = 1.0;

    // When: encoded
    let report = encode_torque(torque);

    // Then: raw value equals i16::MAX
    let raw = i16::from_le_bytes([report[1], report[2]]);
    assert_eq!(raw, i16::MAX, "1.0 must saturate to i16::MAX (32767)");

    Ok(())
}

// ─── Scenario 3: full negative saturation ────────────────────────────────────

#[test]
fn given_full_negative_torque_when_encoded_then_saturates_to_neg_i16_max(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: maximum negative normalised torque
    let torque: f32 = -1.0;

    // When: encoded
    let report = encode_torque(torque);

    // Then: raw value equals -i16::MAX (not i16::MIN, which would be -32768)
    let raw = i16::from_le_bytes([report[1], report[2]]);
    assert_eq!(raw, -i16::MAX, "-1.0 must saturate to -i16::MAX (-32767)");

    Ok(())
}

// ─── Scenario 4: sign preservation for positive and negative inputs ──────────

#[test]
fn given_positive_input_when_encoded_then_raw_is_positive() -> Result<(), Box<dyn std::error::Error>>
{
    // Given: positive torque values
    for &torque in &[0.1_f32, 0.25, 0.5, 0.75, 0.99] {
        // When: encoded
        let report = encode_torque(torque);
        let raw = i16::from_le_bytes([report[1], report[2]]);

        // Then: sign is positive
        assert!(
            raw > 0,
            "positive torque {torque} must yield positive raw, got {raw}"
        );
    }

    Ok(())
}

#[test]
fn given_negative_input_when_encoded_then_raw_is_negative() -> Result<(), Box<dyn std::error::Error>>
{
    // Given: negative torque values
    for &torque in &[-0.1_f32, -0.25, -0.5, -0.75, -0.99] {
        // When: encoded
        let report = encode_torque(torque);
        let raw = i16::from_le_bytes([report[1], report[2]]);

        // Then: sign is negative
        assert!(
            raw < 0,
            "negative torque {torque} must yield negative raw, got {raw}"
        );
    }

    Ok(())
}

// ─── Scenario 5: report byte layout verification ────────────────────────────

#[test]
fn given_any_torque_when_encoded_then_report_layout_is_correct(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: several representative torque values
    for &torque in &[-1.0_f32, -0.5, 0.0, 0.5, 1.0] {
        // When: encoded
        let report = encode_torque(torque);

        // Then: report ID at offset 0
        assert_eq!(report[0], FFB_REPORT_ID, "byte 0 must be report ID 0x01");
        // Then: mode byte at offset 3
        assert_eq!(report[3], MODE_GAME, "byte 3 must be MODE_GAME (0x01)");
        // Then: reserved bytes 4..8 are zero
        assert_eq!(
            &report[4..],
            &[0x00, 0x00, 0x00, 0x00],
            "bytes 4-7 must be zero (reserved)"
        );
        // Then: total length is FFB_REPORT_LEN
        assert_eq!(report.len(), FFB_REPORT_LEN);
    }

    Ok(())
}

// ─── Scenario 6: encode_stop produces identical output to encode_torque(0.0) ─

#[test]
fn given_stop_command_when_encoded_then_equals_zero_torque(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: both a stop command and a zero-torque command
    let stop_report = encode_stop();
    let zero_report = encode_torque(0.0);

    // Then: byte-for-byte identical
    assert_eq!(
        stop_report, zero_report,
        "encode_stop() must produce the same bytes as encode_torque(0.0)"
    );

    Ok(())
}

// ─── Scenario 7: values outside [-1.0, 1.0] are clamped ─────────────────────

#[test]
fn given_out_of_range_torque_when_encoded_then_clamped_to_boundary(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: values exceeding the normalised range
    let over_positive = encode_torque(2.0);
    let at_positive = encode_torque(1.0);
    let over_negative = encode_torque(-5.0);
    let at_negative = encode_torque(-1.0);

    // Then: over-range clamps to boundary
    assert_eq!(
        over_positive, at_positive,
        "torque > 1.0 must clamp to 1.0 output"
    );
    assert_eq!(
        over_negative, at_negative,
        "torque < -1.0 must clamp to -1.0 output"
    );

    // Also verify extreme values
    assert_eq!(encode_torque(100.0), at_positive);
    assert_eq!(encode_torque(-100.0), at_negative);

    Ok(())
}

// ─── Scenario 8: half-scale encoding ─────────────────────────────────────────

#[test]
fn given_half_scale_torque_when_encoded_then_approximately_half_magnitude(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: half-scale positive torque
    let report = encode_torque(0.5);
    let raw = i16::from_le_bytes([report[1], report[2]]);

    // Then: magnitude is approximately half of i16::MAX (within 1 LSB of rounding)
    let expected = (0.5_f32 * i16::MAX as f32) as i16;
    let tolerance = 1_i16;
    assert!(
        (raw - expected).abs() <= tolerance,
        "0.5 torque: expected ~{expected}, got {raw}"
    );

    // Given: half-scale negative torque
    let report_neg = encode_torque(-0.5);
    let raw_neg = i16::from_le_bytes([report_neg[1], report_neg[2]]);

    // Then: magnitude is approximately -half of i16::MAX
    let expected_neg = (-0.5_f32 * i16::MAX as f32) as i16;
    assert!(
        (raw_neg - expected_neg).abs() <= tolerance,
        "-0.5 torque: expected ~{expected_neg}, got {raw_neg}"
    );

    Ok(())
}

// ─── Scenario 9: input report parsing round-trip ─────────────────────────────

#[test]
fn given_valid_input_bytes_when_parsed_then_fields_decoded_correctly(
) -> Result<(), ParseError> {
    // Given: a 64-byte input report with known field values
    let mut data = [0u8; REPORT_LEN];

    // Steering: i16 LE at offset 0-1, set to +16383 (~0.5 of i16::MAX)
    let steering_raw: i16 = 16383;
    let steering_bytes = steering_raw.to_le_bytes();
    data[0] = steering_bytes[0];
    data[1] = steering_bytes[1];

    // Throttle: u16 LE at offset 2-3, full scale
    data[2] = 0xFF;
    data[3] = 0xFF;

    // Brake: u16 LE at offset 4-5, half scale
    let brake_raw: u16 = 0x7FFF;
    let brake_bytes = brake_raw.to_le_bytes();
    data[4] = brake_bytes[0];
    data[5] = brake_bytes[1];

    // Buttons: bytes 6-7
    data[6] = 0x03; // buttons 0 and 1 pressed
    data[7] = 0x00;

    // Clutch: u16 LE at offset 8-9
    data[8] = 0x00;
    data[9] = 0x00;

    // Handbrake: u16 LE at offset 10-11
    data[10] = 0xFF;
    data[11] = 0xFF;

    // When: parsed
    let report = parse(&data)?;

    // Then: steering normalised to ~0.5
    assert!(
        (report.steering - 0.5).abs() < 0.01,
        "steering: expected ~0.5, got {}",
        report.steering
    );
    // Then: throttle at full
    assert!(
        (report.throttle - 1.0).abs() < 0.01,
        "throttle: expected ~1.0, got {}",
        report.throttle
    );
    // Then: brake at approximately half
    assert!(
        (report.brake - 0.5).abs() < 0.01,
        "brake: expected ~0.5, got {}",
        report.brake
    );
    // Then: buttons decoded
    assert_eq!(report.buttons, 0x0003);
    // Then: clutch at zero
    assert!(
        report.clutch.abs() < 0.01,
        "clutch: expected ~0.0, got {}",
        report.clutch
    );
    // Then: handbrake at full
    assert!(
        (report.handbrake - 1.0).abs() < 0.01,
        "handbrake: expected ~1.0, got {}",
        report.handbrake
    );

    Ok(())
}

#[test]
fn given_short_input_bytes_when_parsed_then_returns_too_short_error(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: input shorter than the minimum 12 bytes
    let short_data = [0u8; 8];

    // When: parsed
    let result = parse(&short_data);

    // Then: TooShort error with correct lengths
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err, ParseError::TooShort { got: 8, need: 12 });

    Ok(())
}

// ─── Scenario 10: product ID constants match expected values ─────────────────

#[test]
fn given_product_id_constants_then_values_match_specification(
) -> Result<(), Box<dyn std::error::Error>> {
    // Then: vendor ID
    assert_eq!(VENDOR_ID, 0x3416, "Cammus VID must be 0x3416");

    // Then: wheel product IDs
    assert_eq!(PRODUCT_C5, 0x0301, "C5 PID must be 0x0301");
    assert_eq!(PRODUCT_C12, 0x0302, "C12 PID must be 0x0302");

    // Then: pedal product IDs
    assert_eq!(PRODUCT_CP5_PEDALS, 0x1018, "CP5 Pedals PID must be 0x1018");
    assert_eq!(
        PRODUCT_LC100_PEDALS, 0x1019,
        "LC100 Pedals PID must be 0x1019"
    );

    // Then: FFB constants
    assert_eq!(FFB_REPORT_ID, 0x01);
    assert_eq!(FFB_REPORT_LEN, 8);

    // Then: input report constants
    assert_eq!(REPORT_LEN, 64);
    assert!((STEERING_RANGE_DEG - 1080.0).abs() < f32::EPSILON);

    Ok(())
}

// ─── Scenario 11: model classification from product IDs ──────────────────────

#[test]
fn given_known_pid_when_classified_then_correct_model_returned(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given/When/Then: C5
    assert_eq!(CammusModel::from_pid(PRODUCT_C5), Some(CammusModel::C5));
    assert_eq!(CammusModel::C5.name(), "Cammus C5");
    assert!((CammusModel::C5.max_torque_nm() - 5.0).abs() < f32::EPSILON);

    // Given/When/Then: C12
    assert_eq!(CammusModel::from_pid(PRODUCT_C12), Some(CammusModel::C12));
    assert_eq!(CammusModel::C12.name(), "Cammus C12");
    assert!((CammusModel::C12.max_torque_nm() - 12.0).abs() < f32::EPSILON);

    // Given/When/Then: pedals have zero torque
    assert_eq!(
        CammusModel::from_pid(PRODUCT_CP5_PEDALS),
        Some(CammusModel::Cp5Pedals)
    );
    assert!((CammusModel::Cp5Pedals.max_torque_nm()).abs() < f32::EPSILON);

    assert_eq!(
        CammusModel::from_pid(PRODUCT_LC100_PEDALS),
        Some(CammusModel::Lc100Pedals)
    );
    assert!((CammusModel::Lc100Pedals.max_torque_nm()).abs() < f32::EPSILON);

    // Given/When/Then: unknown PID
    assert_eq!(CammusModel::from_pid(0xFFFF), None);

    Ok(())
}

// ─── Scenario 12: is_cammus correctly identifies known and unknown devices ───

#[test]
fn given_vid_pid_pairs_when_checked_then_known_devices_recognised(
) -> Result<(), Box<dyn std::error::Error>> {
    // Then: all known Cammus devices are recognised
    assert!(is_cammus(VENDOR_ID, PRODUCT_C5));
    assert!(is_cammus(VENDOR_ID, PRODUCT_C12));
    assert!(is_cammus(VENDOR_ID, PRODUCT_CP5_PEDALS));
    assert!(is_cammus(VENDOR_ID, PRODUCT_LC100_PEDALS));

    // Then: wrong vendor ID is rejected
    assert!(!is_cammus(0x0000, PRODUCT_C5));
    assert!(!is_cammus(0xFFFF, PRODUCT_C12));

    // Then: unknown PID under correct VID is rejected
    assert!(!is_cammus(VENDOR_ID, 0x0000));
    assert!(!is_cammus(VENDOR_ID, 0xFFFF));

    Ok(())
}

// ─── Scenario 13: product_name returns correct human-readable strings ────────

#[test]
fn given_known_pids_when_named_then_human_readable_strings_returned(
) -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(product_name(PRODUCT_C5), Some("Cammus C5"));
    assert_eq!(product_name(PRODUCT_C12), Some("Cammus C12"));
    assert_eq!(product_name(PRODUCT_CP5_PEDALS), Some("Cammus CP5 Pedals"));
    assert_eq!(
        product_name(PRODUCT_LC100_PEDALS),
        Some("Cammus LC100 Pedals")
    );
    assert_eq!(product_name(0xFFFF), None);

    Ok(())
}

// ─── Scenario 14: encoding monotonicity across the full range ────────────────

#[test]
fn given_increasing_torque_when_encoded_then_raw_values_are_monotonic(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: a series of increasing torque values
    let values: Vec<f32> = (-10..=10).map(|i| i as f32 * 0.1).collect();

    let mut prev_raw = i16::MIN;
    for &torque in &values {
        // When: encoded
        let report = encode_torque(torque);
        let raw = i16::from_le_bytes([report[1], report[2]]);

        // Then: raw value is >= previous (monotone non-decreasing)
        assert!(
            raw >= prev_raw,
            "monotonicity violated: torque {torque} yielded {raw}, but previous was {prev_raw}"
        );
        prev_raw = raw;
    }

    Ok(())
}

// ─── Scenario 15: input report with minimum valid length ─────────────────────

#[test]
fn given_exactly_12_bytes_when_parsed_then_succeeds(
) -> Result<(), ParseError> {
    // Given: exactly 12 bytes (the minimum required)
    let data = [0u8; 12];

    // When: parsed
    let report = parse(&data)?;

    // Then: all fields at neutral
    assert!(report.steering.abs() < 0.01);
    assert!(report.throttle.abs() < 0.01);
    assert!(report.brake.abs() < 0.01);
    assert!(report.clutch.abs() < 0.01);
    assert!(report.handbrake.abs() < 0.01);
    assert_eq!(report.buttons, 0);

    Ok(())
}

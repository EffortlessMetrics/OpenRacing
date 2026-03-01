//! BDD end-to-end tests for the HID button box protocol stack.
//!
//! Each test follows a Given/When/Then pattern to verify observable behaviors
//! without real USB hardware.  Button box protocol parsing is purely functional
//! (I/O-free), so no virtual device is required.

use hid_button_box_protocol::{
    ButtonBoxCapabilities, ButtonBoxError, ButtonBoxInputReport, ButtonBoxType, HatDirection,
    MAX_AXES, MAX_BUTTONS, PRODUCT_ID_BUTTON_BOX, REPORT_SIZE_GAMEPAD, RotaryEncoderState,
    VENDOR_ID_GENERIC,
};

// ─── Scenario 1: standard gamepad report parsing ─────────────────────────────

#[test]
fn gamepad_report_given_valid_8_bytes_when_parsed_then_fields_decoded()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: an 8-byte gamepad report with known values
    //   buttons u16 LE = 0x0005 (buttons 0 and 2 pressed)
    //   axis_x  i16 LE = 0x7FFF (i16::MAX)
    //   axis_y  i16 LE = 0x8001 (i16::MIN + 1 = -32767)
    //   axis_z  i16 LE = 0x0000
    //   hat     u8     = 0x02 (Right)
    //   pad     u8     = 0x00
    let data: [u8; 10] = [
        0x05, 0x00, // buttons
        0xFF, 0x7F, // axis_x = i16::MAX
        0x01, 0x80, // axis_y = -32767
        0x00, 0x00, // axis_z = 0
        0x02, // hat = Right
        0x00, // pad
    ];

    // When: parsed as a gamepad report
    let report = ButtonBoxInputReport::parse_gamepad(&data)?;

    // Then: button field matches
    assert_eq!(report.buttons, 0x0005, "buttons must be 0x0005");

    // Then: axes decoded correctly
    assert_eq!(report.axis_x, i16::MAX, "axis_x must be i16::MAX");
    assert_eq!(report.axis_y, -32767, "axis_y must be -32767");
    assert_eq!(report.axis_z, 0, "axis_z must be 0");
    assert_eq!(
        report.axis_rz, 0,
        "axis_rz must be 0 (not in gamepad format)"
    );

    // Then: hat decoded
    assert_eq!(report.hat, 0x02);

    Ok(())
}

#[test]
fn gamepad_report_given_all_zeros_when_parsed_then_neutral_state()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: an all-zero 10-byte report (2 buttons + 3×i16 axes + hat + pad)
    let data = [0u8; 10];

    // When: parsed
    let report = ButtonBoxInputReport::parse_gamepad(&data)?;

    // Then: buttons clear, axes zero, hat = Up (0x00 maps to Up)
    assert_eq!(report.buttons, 0);
    assert_eq!(report.axis_x, 0);
    assert_eq!(report.axis_y, 0);
    assert_eq!(report.axis_z, 0);
    assert_eq!(report.hat_direction(), HatDirection::Up);

    Ok(())
}

// ─── Scenario 2: individual button get/set and boundary checks ───────────────

#[test]
fn button_access_given_default_report_when_button_set_then_readable()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: a default report with no buttons pressed
    let mut report = ButtonBoxInputReport::default();
    assert!(!report.button(0));

    // When: button 0 is set
    report.set_button(0, true);

    // Then: button 0 reads as pressed
    assert!(report.button(0), "button 0 must be set");
    // Then: adjacent button unaffected
    assert!(!report.button(1), "button 1 must remain unset");

    Ok(())
}

#[test]
fn button_access_given_set_button_when_cleared_then_no_longer_pressed()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: button 7 is pressed
    let mut report = ButtonBoxInputReport::default();
    report.set_button(7, true);
    assert!(report.button(7));

    // When: button 7 is cleared
    report.set_button(7, false);

    // Then: button 7 is no longer pressed
    assert!(!report.button(7));

    Ok(())
}

#[test]
fn button_access_given_out_of_range_index_when_queried_then_returns_false()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: a report with button 0 pressed
    let mut report = ButtonBoxInputReport::default();
    report.set_button(0, true);

    // When: querying button at MAX_BUTTONS (out of range)
    let result = report.button(MAX_BUTTONS);

    // Then: returns false without panic
    assert!(!result, "out-of-range button index must return false");

    // When: querying a very large index
    assert!(!report.button(999), "very large index must return false");

    Ok(())
}

#[test]
fn button_access_given_out_of_range_index_when_set_then_no_effect()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: a default report
    let mut report = ButtonBoxInputReport::default();

    // When: setting button at MAX_BUTTONS (out of range)
    report.set_button(MAX_BUTTONS, true);

    // Then: no buttons are pressed
    assert_eq!(report.buttons, 0, "out-of-range set_button must be no-op");

    Ok(())
}

#[test]
fn button_access_given_multiple_buttons_when_counted_then_popcount_correct()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: a report with 3 buttons set
    let mut report = ButtonBoxInputReport::default();
    report.set_button(0, true);
    report.set_button(5, true);
    report.set_button(31, true);

    // When: counting pressed buttons
    let count = report.button_count();

    // Then: count equals 3
    assert_eq!(count, 3, "3 buttons pressed must yield count 3");

    Ok(())
}

// ─── Scenario 3: hat switch direction encoding ──────────────────────────────

#[test]
fn hat_direction_given_all_valid_codes_when_decoded_then_correct_direction()
-> Result<(), Box<dyn std::error::Error>> {
    let expected = [
        (0, HatDirection::Up),
        (1, HatDirection::UpRight),
        (2, HatDirection::Right),
        (3, HatDirection::DownRight),
        (4, HatDirection::Down),
        (5, HatDirection::DownLeft),
        (6, HatDirection::Left),
        (7, HatDirection::UpLeft),
    ];

    for (code, direction) in expected {
        // Given: a report with hat set to a known code
        let report = ButtonBoxInputReport {
            hat: code,
            ..Default::default()
        };

        // When: decoded
        let result = report.hat_direction();

        // Then: direction matches expected
        assert_eq!(
            result, direction,
            "hat code {code} must decode to {direction:?}"
        );
    }

    Ok(())
}

#[test]
fn hat_direction_given_invalid_code_when_decoded_then_neutral()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: hat values outside 0..8
    for code in [8, 15, 0x80, 0xFF] {
        let report = ButtonBoxInputReport {
            hat: code,
            ..Default::default()
        };

        // When: decoded
        let result = report.hat_direction();

        // Then: Neutral
        assert_eq!(
            result,
            HatDirection::Neutral,
            "hat code {code} must decode to Neutral"
        );
    }

    Ok(())
}

// ─── Scenario 4: axis reading and normalization ──────────────────────────────

#[test]
fn axis_reading_given_known_values_when_indexed_then_correct_axis_returned()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: a report with distinct axis values
    let report = ButtonBoxInputReport {
        axis_x: 1000,
        axis_y: -2000,
        axis_z: 3000,
        axis_rz: -4000,
        ..Default::default()
    };

    // When/Then: each index returns the corresponding axis
    assert_eq!(report.axis(0), 1000);
    assert_eq!(report.axis(1), -2000);
    assert_eq!(report.axis(2), 3000);
    assert_eq!(report.axis(3), -4000);

    Ok(())
}

#[test]
fn axis_reading_given_out_of_range_index_when_queried_then_returns_zero()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: any report
    let report = ButtonBoxInputReport {
        axis_x: 1000,
        ..Default::default()
    };

    // When: axis index >= MAX_AXES
    let result = report.axis(MAX_AXES);

    // Then: returns 0
    assert_eq!(result, 0, "out-of-range axis index must return 0");
    assert_eq!(report.axis(100), 0);

    Ok(())
}

#[test]
fn axis_normalization_given_max_value_when_normalized_then_approximately_one()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: axis_x at i16::MAX
    let report = ButtonBoxInputReport {
        axis_x: i16::MAX,
        ..Default::default()
    };

    // When: normalized
    let norm = report.axis_normalized(0);

    // Then: approximately 1.0
    assert!(
        (norm - 1.0).abs() < 0.001,
        "i16::MAX normalized must be ~1.0, got {norm}"
    );

    Ok(())
}

#[test]
fn axis_normalization_given_zero_when_normalized_then_zero()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: axis at 0
    let report = ButtonBoxInputReport::default();

    // When: normalized
    let norm = report.axis_normalized(0);

    // Then: 0.0
    assert!(
        norm.abs() < 0.001,
        "zero axis normalized must be ~0.0, got {norm}"
    );

    Ok(())
}

#[test]
fn axis_normalization_given_negative_max_when_normalized_then_approximately_neg_one()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: axis at i16::MIN (note: -32768 / 32767 ≈ -1.000030)
    let report = ButtonBoxInputReport {
        axis_x: i16::MIN,
        ..Default::default()
    };

    // When: normalized
    let norm = report.axis_normalized(0);

    // Then: approximately -1.0
    assert!(
        (norm + 1.0).abs() < 0.01,
        "i16::MIN normalized must be ~-1.0, got {norm}"
    );

    Ok(())
}

// ─── Scenario 5: extended report parsing ─────────────────────────────────────

#[test]
fn extended_report_given_valid_13_bytes_when_parsed_then_all_fields_decoded()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: a 13-byte extended report
    //   buttons u32 LE = 0x80000001 (buttons 0 and 31)
    //   axis_x  i16 LE = 100
    //   axis_y  i16 LE = 200
    //   axis_z  i16 LE = 300
    //   axis_rz i16 LE = 400
    //   hat     u8     = 4 (Down)
    let mut data = [0u8; 13];
    // buttons
    data[0] = 0x01;
    data[1] = 0x00;
    data[2] = 0x00;
    data[3] = 0x80;
    // axis_x = 100
    data[4] = 100;
    data[5] = 0;
    // axis_y = 200
    data[6] = 200;
    data[7] = 0;
    // axis_z = 300
    let z_bytes = 300_i16.to_le_bytes();
    data[8] = z_bytes[0];
    data[9] = z_bytes[1];
    // axis_rz = 400
    let rz_bytes = 400_i16.to_le_bytes();
    data[10] = rz_bytes[0];
    data[11] = rz_bytes[1];
    // hat = Down
    data[12] = 4;

    // When: parsed as extended
    let report = ButtonBoxInputReport::parse_extended(&data)?;

    // Then: buttons include bit 0 and bit 31
    assert!(report.button(0), "button 0 must be set");
    assert!(report.button(31), "button 31 must be set");
    assert!(!report.button(1), "button 1 must not be set");
    assert_eq!(report.button_count(), 2);

    // Then: all four axes decoded
    assert_eq!(report.axis_x, 100);
    assert_eq!(report.axis_y, 200);
    assert_eq!(report.axis_z, 300);
    assert_eq!(report.axis_rz, 400);

    // Then: hat direction
    assert_eq!(report.hat_direction(), HatDirection::Down);

    Ok(())
}

#[test]
fn extended_report_given_all_buttons_when_parsed_then_full_32_bit_mask()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: all 32 bits set in extended report
    let mut data = [0u8; 13];
    data[0] = 0xFF;
    data[1] = 0xFF;
    data[2] = 0xFF;
    data[3] = 0xFF;

    // When: parsed
    let report = ButtonBoxInputReport::parse_extended(&data)?;

    // Then: all 32 buttons pressed
    assert_eq!(report.buttons, 0xFFFF_FFFF);
    assert_eq!(report.button_count(), 32);
    for i in 0..MAX_BUTTONS {
        assert!(report.button(i), "button {i} must be set");
    }

    Ok(())
}

// ─── Scenario 6: rotary encoder state tracking ──────────────────────────────

#[test]
fn rotary_encoder_given_new_state_when_updated_then_position_and_delta_correct()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: a fresh encoder
    let mut encoder = RotaryEncoderState::new();
    assert_eq!(encoder.position, 0);
    assert_eq!(encoder.delta, 0);

    // When: updated to position 10
    encoder.update(10);

    // Then: position = 10, delta = 10
    assert_eq!(encoder.position, 10);
    assert_eq!(encoder.delta, 10);

    // When: updated to position 12
    encoder.update(12);

    // Then: position = 12, delta = 2
    assert_eq!(encoder.position, 12);
    assert_eq!(encoder.delta, 2);

    // When: moved backwards
    encoder.update(5);

    // Then: delta is negative
    assert_eq!(encoder.position, 5);
    assert_eq!(encoder.delta, -7);

    Ok(())
}

#[test]
fn rotary_encoder_given_large_jump_when_updated_then_delta_clamped()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: encoder at position 0
    let mut encoder = RotaryEncoderState::new();

    // When: jumping far forward (delta > 127)
    encoder.update(500);

    // Then: delta clamped to 127
    assert_eq!(encoder.position, 500);
    assert_eq!(encoder.delta, 127);

    // When: jumping far backward (delta < -127)
    encoder.update(0);

    // Then: delta clamped to -127
    assert_eq!(encoder.position, 0);
    assert_eq!(encoder.delta, -127);

    Ok(())
}

#[test]
fn rotary_encoder_given_zero_movement_when_updated_then_delta_zero()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: encoder at position 42
    let mut encoder = RotaryEncoderState::new();
    encoder.update(42);

    // When: updated to same position
    encoder.update(42);

    // Then: delta = 0
    assert_eq!(encoder.delta, 0);
    assert_eq!(encoder.position, 42);

    Ok(())
}

#[test]
fn rotary_encoder_given_button_state_when_set_then_persists()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: a fresh encoder
    let mut encoder = RotaryEncoderState::new();
    assert!(!encoder.button_pressed);

    // When: button pressed
    encoder.button_pressed = true;

    // Then: state persists through update
    encoder.update(5);
    assert!(encoder.button_pressed);

    Ok(())
}

// ─── Scenario 7: button box type classification ─────────────────────────────

#[test]
fn button_box_type_given_variants_when_compared_then_distinct()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: the three button box types
    let simple = ButtonBoxType::Simple;
    let standard = ButtonBoxType::Standard;
    let extended = ButtonBoxType::Extended;

    // Then: each variant is distinct
    assert_ne!(simple, standard);
    assert_ne!(standard, extended);
    assert_ne!(simple, extended);

    // Then: default is Standard
    assert_eq!(ButtonBoxType::default(), ButtonBoxType::Standard);

    Ok(())
}

// ─── Scenario 8: capability detection ────────────────────────────────────────

#[test]
fn capabilities_given_basic_config_when_queried_then_limited_features()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: basic capabilities
    let caps = ButtonBoxCapabilities::basic();

    // Then: reduced button count, no rotary encoders, no axes
    assert_eq!(caps.button_count, 16);
    assert_eq!(caps.analog_axis_count, 0);
    assert!(caps.has_pov_hat);
    assert!(!caps.has_rotary_encoders);
    assert_eq!(caps.rotary_encoder_count, 0);

    Ok(())
}

#[test]
fn capabilities_given_extended_config_when_queried_then_full_features()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: extended capabilities
    let caps = ButtonBoxCapabilities::extended();

    // Then: full features
    assert_eq!(caps.button_count, 32);
    assert_eq!(caps.analog_axis_count, 4);
    assert!(caps.has_pov_hat);
    assert!(caps.has_rotary_encoders);
    assert_eq!(caps.rotary_encoder_count, 8);

    Ok(())
}

#[test]
fn capabilities_given_default_when_queried_then_matches_extended()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: default capabilities
    let def = ButtonBoxCapabilities::default();
    let ext = ButtonBoxCapabilities::extended();

    // Then: default matches extended
    assert_eq!(def.button_count, ext.button_count);
    assert_eq!(def.analog_axis_count, ext.analog_axis_count);
    assert_eq!(def.has_pov_hat, ext.has_pov_hat);
    assert_eq!(def.has_rotary_encoders, ext.has_rotary_encoders);
    assert_eq!(def.rotary_encoder_count, ext.rotary_encoder_count);

    Ok(())
}

// ─── Scenario 9: error handling — short and malformed buffers ────────────────

#[test]
fn error_handling_given_short_gamepad_buffer_when_parsed_then_invalid_report_size()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: buffers shorter than REPORT_SIZE_GAMEPAD (8)
    for len in [0, 1, 4, 7] {
        let data = vec![0u8; len];

        // When: parsed as gamepad
        let result = ButtonBoxInputReport::parse_gamepad(&data);

        // Then: InvalidReportSize error with correct sizes
        match result {
            Err(ButtonBoxError::InvalidReportSize { expected, actual }) => {
                assert_eq!(
                    expected, REPORT_SIZE_GAMEPAD,
                    "expected size must be {REPORT_SIZE_GAMEPAD}"
                );
                assert_eq!(actual, len, "actual size must be {len}");
            }
            other => {
                return Err(
                    format!("expected InvalidReportSize for len {len}, got {other:?}").into(),
                );
            }
        }
    }

    Ok(())
}

#[test]
fn error_handling_given_short_extended_buffer_when_parsed_then_invalid_report_size()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: buffers shorter than 12 (extended minimum)
    for len in [0, 4, 8, 11] {
        let data = vec![0u8; len];

        // When: parsed as extended
        let result = ButtonBoxInputReport::parse_extended(&data);

        // Then: InvalidReportSize error
        match result {
            Err(ButtonBoxError::InvalidReportSize { expected, actual }) => {
                assert_eq!(expected, 12);
                assert_eq!(actual, len);
            }
            other => {
                return Err(
                    format!("expected InvalidReportSize for len {len}, got {other:?}").into(),
                );
            }
        }
    }

    Ok(())
}

#[test]
fn error_handling_given_empty_buffer_when_parsed_then_error_not_panic()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: empty data
    let data: &[u8] = &[];

    // When: both parse methods tried
    let gamepad_result = ButtonBoxInputReport::parse_gamepad(data);
    let extended_result = ButtonBoxInputReport::parse_extended(data);

    // Then: both return errors (no panic)
    assert!(gamepad_result.is_err());
    assert!(extended_result.is_err());

    Ok(())
}

// ─── Scenario 10: button count limits ────────────────────────────────────────

#[test]
fn button_limits_given_max_button_index_when_set_then_accessible()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: the highest valid button index (31, since MAX_BUTTONS = 32)
    let mut report = ButtonBoxInputReport::default();
    let last_valid = MAX_BUTTONS - 1;

    // When: set
    report.set_button(last_valid, true);

    // Then: readable
    assert!(
        report.button(last_valid),
        "button {last_valid} must be settable"
    );
    assert_eq!(report.button_count(), 1);

    Ok(())
}

#[test]
fn button_limits_given_all_32_buttons_set_when_counted_then_32()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: every valid button pressed
    let mut report = ButtonBoxInputReport::default();
    for i in 0..MAX_BUTTONS {
        report.set_button(i, true);
    }

    // Then: all 32 pressed
    assert_eq!(report.button_count(), 32);
    assert_eq!(report.buttons, 0xFFFF_FFFF);

    Ok(())
}

#[test]
fn button_limits_given_no_buttons_when_counted_then_zero() -> Result<(), Box<dyn std::error::Error>>
{
    // Given: default report
    let report = ButtonBoxInputReport::default();

    // Then: no buttons pressed
    assert_eq!(report.button_count(), 0);
    assert_eq!(report.buttons, 0);

    Ok(())
}

// ─── Scenario 11: constants match specification ──────────────────────────────

#[test]
fn constants_given_protocol_values_then_match_specification()
-> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(REPORT_SIZE_GAMEPAD, 8);
    assert_eq!(MAX_BUTTONS, 32);
    assert_eq!(MAX_AXES, 4);
    assert_eq!(VENDOR_ID_GENERIC, 0x1209);
    assert_eq!(PRODUCT_ID_BUTTON_BOX, 0x1BBD);

    Ok(())
}

// ─── Scenario 12: default report state ───────────────────────────────────────

#[test]
fn default_report_given_default_constructor_then_neutral_state()
-> Result<(), Box<dyn std::error::Error>> {
    // Given/When: default report
    let report = ButtonBoxInputReport::default();

    // Then: everything at neutral
    assert_eq!(report.buttons, 0);
    assert_eq!(report.axis_x, 0);
    assert_eq!(report.axis_y, 0);
    assert_eq!(report.axis_z, 0);
    assert_eq!(report.axis_rz, 0);
    assert_eq!(report.hat, 0xFF, "default hat must be 0xFF (Neutral)");
    assert_eq!(report.hat_direction(), HatDirection::Neutral);

    Ok(())
}

// ─── Scenario 13: oversized buffers accepted gracefully ──────────────────────

#[test]
fn parsing_given_oversized_gamepad_buffer_when_parsed_then_succeeds()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: a buffer larger than required, with known data in the valid range
    let mut data = [0u8; 64];
    data[0] = 0x03; // buttons 0 and 1
    data[1] = 0x00;

    // When: parsed as gamepad
    let report = ButtonBoxInputReport::parse_gamepad(&data)?;

    // Then: parses successfully using only the first 8 bytes
    assert_eq!(report.buttons, 0x0003);

    Ok(())
}

#[test]
fn parsing_given_oversized_extended_buffer_when_parsed_then_succeeds()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: a 64-byte buffer
    let mut data = [0u8; 64];
    data[0] = 0xFF;
    data[1] = 0x00;
    data[2] = 0x00;
    data[3] = 0x00;

    // When: parsed as extended
    let report = ButtonBoxInputReport::parse_extended(&data)?;

    // Then: parses successfully
    assert_eq!(report.buttons, 0x0000_00FF);
    assert_eq!(report.button_count(), 8);

    Ok(())
}

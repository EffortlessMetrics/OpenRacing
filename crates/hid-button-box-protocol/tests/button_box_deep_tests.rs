//! Deep tests for the HID button-box protocol crate: report parsing, encoder
//! rotation detection, LED control commands, and edge-case coverage.

use hid_button_box_protocol::{
    ButtonBoxCapabilities, ButtonBoxError, ButtonBoxInputReport, ButtonBoxResult, ButtonBoxType,
    HatDirection, MAX_AXES, MAX_BUTTONS, PRODUCT_ID_BUTTON_BOX, REPORT_SIZE_GAMEPAD,
    RotaryEncoderState, VENDOR_ID_GENERIC,
};

type R = Result<(), Box<dyn std::error::Error>>;

// ── Helpers ──────────────────────────────────────────────────────────────────

fn gamepad_report_with_buttons(buttons_lo: u8, buttons_hi: u8) -> [u8; 10] {
    [buttons_lo, buttons_hi, 0, 0, 0, 0, 0, 0, 0xFF, 0x00]
}

fn extended_report_with_buttons(buttons: u32) -> Vec<u8> {
    let bytes = buttons.to_le_bytes();
    let mut data = vec![0u8; 13];
    data[0..4].copy_from_slice(&bytes);
    data[12] = 0xFF; // hat neutral
    data
}

// ── 1. Report parsing for various button states ─────────────────────────────

#[test]
fn parse_gamepad_no_buttons_pressed() -> R {
    let data = [0u8; 10];
    let report = ButtonBoxInputReport::parse_gamepad(&data).map_err(|e| e.to_string())?;
    assert_eq!(report.buttons, 0);
    assert_eq!(report.button_count(), 0);
    for i in 0..16 {
        assert!(!report.button(i));
    }
    Ok(())
}

#[test]
fn parse_gamepad_single_button_pressed() -> R {
    let data = gamepad_report_with_buttons(0x01, 0x00);
    let report = ButtonBoxInputReport::parse_gamepad(&data).map_err(|e| e.to_string())?;
    assert!(report.button(0));
    assert!(!report.button(1));
    assert_eq!(report.button_count(), 1);
    Ok(())
}

#[test]
fn parse_gamepad_multiple_buttons_pressed() -> R {
    let data = gamepad_report_with_buttons(0x15, 0x00); // bits 0, 2, 4
    let report = ButtonBoxInputReport::parse_gamepad(&data).map_err(|e| e.to_string())?;
    assert!(report.button(0));
    assert!(!report.button(1));
    assert!(report.button(2));
    assert!(!report.button(3));
    assert!(report.button(4));
    assert_eq!(report.button_count(), 3);
    Ok(())
}

#[test]
fn parse_gamepad_all_low_byte_buttons() -> R {
    let data = gamepad_report_with_buttons(0xFF, 0x00);
    let report = ButtonBoxInputReport::parse_gamepad(&data).map_err(|e| e.to_string())?;
    for i in 0..8 {
        assert!(report.button(i), "button {i} should be pressed");
    }
    assert_eq!(report.button_count(), 8);
    Ok(())
}

#[test]
fn parse_gamepad_high_byte_buttons() -> R {
    let data = gamepad_report_with_buttons(0x00, 0xFF);
    let report = ButtonBoxInputReport::parse_gamepad(&data).map_err(|e| e.to_string())?;
    for i in 0..8 {
        assert!(!report.button(i), "low byte button {i} should be off");
    }
    for i in 8..16 {
        assert!(report.button(i), "high byte button {i} should be on");
    }
    assert_eq!(report.button_count(), 8);
    Ok(())
}

#[test]
fn parse_extended_32_buttons_all_set() -> R {
    let data = extended_report_with_buttons(0xFFFF_FFFF);
    let report = ButtonBoxInputReport::parse_extended(&data).map_err(|e| e.to_string())?;
    assert_eq!(report.buttons, 0xFFFF_FFFF);
    for i in 0..32 {
        assert!(report.button(i), "button {i} should be pressed");
    }
    assert_eq!(report.button_count(), 32);
    Ok(())
}

#[test]
fn parse_extended_alternating_buttons() -> R {
    let data = extended_report_with_buttons(0xAAAA_AAAA);
    let report = ButtonBoxInputReport::parse_extended(&data).map_err(|e| e.to_string())?;
    for i in 0..32 {
        if i % 2 == 1 {
            assert!(report.button(i), "odd button {i} should be pressed");
        } else {
            assert!(!report.button(i), "even button {i} should be off");
        }
    }
    Ok(())
}

#[test]
fn parse_gamepad_too_short_report_fails() {
    let data = [0u8; 4];
    let result = ButtonBoxInputReport::parse_gamepad(&data);
    assert!(matches!(
        result,
        Err(ButtonBoxError::InvalidReportSize {
            expected: 8,
            actual: 4
        })
    ));
}

#[test]
fn parse_extended_too_short_report_fails() {
    let data = [0u8; 6];
    let result = ButtonBoxInputReport::parse_extended(&data);
    assert!(matches!(
        result,
        Err(ButtonBoxError::InvalidReportSize {
            expected: 12,
            actual: 6
        })
    ));
}

#[test]
fn set_button_and_clear_button_toggle() {
    let mut report = ButtonBoxInputReport::default();
    report.set_button(5, true);
    assert!(report.button(5));
    report.set_button(5, false);
    assert!(!report.button(5));
}

#[test]
fn set_button_out_of_range_is_noop() {
    let mut report = ButtonBoxInputReport::default();
    report.set_button(32, true);
    assert!(!report.button(32));
    assert_eq!(report.buttons, 0);
}

#[test]
fn button_out_of_range_returns_false() {
    let report = ButtonBoxInputReport {
        buttons: 0xFFFF_FFFF,
        ..Default::default()
    };
    assert!(!report.button(32));
    assert!(!report.button(100));
}

// ── 2. Encoder rotation detection ──────────────────────────────────────────

#[test]
fn encoder_initial_state() {
    let enc = RotaryEncoderState::new();
    assert_eq!(enc.position, 0);
    assert_eq!(enc.delta, 0);
    assert!(!enc.button_pressed);
}

#[test]
fn encoder_positive_rotation() {
    let mut enc = RotaryEncoderState::new();
    enc.update(5);
    assert_eq!(enc.position, 5);
    assert_eq!(enc.delta, 5);
}

#[test]
fn encoder_negative_rotation() {
    let mut enc = RotaryEncoderState::new();
    enc.update(-3);
    assert_eq!(enc.position, -3);
    assert_eq!(enc.delta, -3);
}

#[test]
fn encoder_incremental_steps() {
    let mut enc = RotaryEncoderState::new();
    enc.update(1);
    assert_eq!(enc.delta, 1);
    enc.update(2);
    assert_eq!(enc.delta, 1);
    enc.update(1);
    assert_eq!(enc.delta, -1);
    enc.update(1);
    assert_eq!(enc.delta, 0);
}

#[test]
fn encoder_large_positive_jump_clamps() {
    let mut enc = RotaryEncoderState::new();
    enc.update(500);
    assert_eq!(enc.delta, 127);
    assert_eq!(enc.position, 500);
}

#[test]
fn encoder_large_negative_jump_clamps() {
    let mut enc = RotaryEncoderState::new();
    enc.update(-500);
    assert_eq!(enc.delta, -127);
    assert_eq!(enc.position, -500);
}

#[test]
fn encoder_button_press_state() {
    let mut enc = RotaryEncoderState::new();
    assert!(!enc.button_pressed);
    enc.button_pressed = true;
    assert!(enc.button_pressed);
    enc.button_pressed = false;
    assert!(!enc.button_pressed);
}

// ── 3. LED control commands (hat/axis-driven feedback) ──────────────────────

#[test]
fn hat_direction_all_cardinal() {
    let cases = [
        (0, HatDirection::Up),
        (2, HatDirection::Right),
        (4, HatDirection::Down),
        (6, HatDirection::Left),
    ];
    for (hat_val, expected) in cases {
        let report = ButtonBoxInputReport {
            hat: hat_val,
            ..Default::default()
        };
        assert_eq!(report.hat_direction(), expected);
    }
}

#[test]
fn hat_direction_all_diagonals() {
    let cases = [
        (1, HatDirection::UpRight),
        (3, HatDirection::DownRight),
        (5, HatDirection::DownLeft),
        (7, HatDirection::UpLeft),
    ];
    for (hat_val, expected) in cases {
        let report = ButtonBoxInputReport {
            hat: hat_val,
            ..Default::default()
        };
        assert_eq!(report.hat_direction(), expected);
    }
}

#[test]
fn hat_direction_neutral_for_out_of_range() {
    for hat_val in [8, 9, 15, 128, 255] {
        let report = ButtonBoxInputReport {
            hat: hat_val,
            ..Default::default()
        };
        assert_eq!(
            report.hat_direction(),
            HatDirection::Neutral,
            "hat={hat_val} should be neutral"
        );
    }
}

#[test]
fn hat_direction_default_is_neutral() {
    assert_eq!(HatDirection::default(), HatDirection::Neutral);
}

// ── 4. Axis normalized ─────────────────────────────────────────────────────

#[test]
fn axis_normalized_full_positive() {
    let report = ButtonBoxInputReport {
        axis_x: i16::MAX,
        ..Default::default()
    };
    let norm = report.axis_normalized(0);
    assert!((norm - 1.0).abs() < 0.001);
}

#[test]
fn axis_normalized_full_negative() {
    let report = ButtonBoxInputReport {
        axis_x: i16::MIN,
        ..Default::default()
    };
    let norm = report.axis_normalized(0);
    assert!(norm < -1.0);
    assert!(norm > -1.1);
}

#[test]
fn axis_normalized_zero() {
    let report = ButtonBoxInputReport::default();
    assert!(report.axis_normalized(0).abs() < f32::EPSILON);
}

#[test]
fn axis_normalized_out_of_range_index() {
    let report = ButtonBoxInputReport::default();
    assert!(report.axis_normalized(5).abs() < f32::EPSILON);
    assert!(report.axis_normalized(100).abs() < f32::EPSILON);
}

// ── 5. Capabilities ─────────────────────────────────────────────────────────

#[test]
fn capabilities_basic_vs_extended() {
    let basic = ButtonBoxCapabilities::basic();
    let ext = ButtonBoxCapabilities::extended();

    assert!(basic.button_count < ext.button_count);
    assert_eq!(basic.analog_axis_count, 0);
    assert_eq!(ext.analog_axis_count, 4);
    assert!(!basic.has_rotary_encoders);
    assert!(ext.has_rotary_encoders);
}

#[test]
fn capabilities_default_matches_extended() {
    let def = ButtonBoxCapabilities::default();
    let ext = ButtonBoxCapabilities::extended();
    assert_eq!(def.button_count, ext.button_count);
    assert_eq!(def.analog_axis_count, ext.analog_axis_count);
    assert_eq!(def.has_pov_hat, ext.has_pov_hat);
    assert_eq!(def.has_rotary_encoders, ext.has_rotary_encoders);
    assert_eq!(def.rotary_encoder_count, ext.rotary_encoder_count);
}

#[test]
fn button_box_type_default_is_standard() {
    assert_eq!(ButtonBoxType::default(), ButtonBoxType::Standard);
}

#[test]
fn button_box_type_variants_are_distinct() {
    assert_ne!(ButtonBoxType::Simple, ButtonBoxType::Standard);
    assert_ne!(ButtonBoxType::Standard, ButtonBoxType::Extended);
    assert_ne!(ButtonBoxType::Simple, ButtonBoxType::Extended);
}

// ── 6. Constants and error formatting ──────────────────────────────────────

#[test]
fn constants_match_expected() {
    assert_eq!(REPORT_SIZE_GAMEPAD, 8);
    assert_eq!(MAX_BUTTONS, 32);
    assert_eq!(MAX_AXES, 4);
    assert_eq!(VENDOR_ID_GENERIC, 0x1209);
    assert_eq!(PRODUCT_ID_BUTTON_BOX, 0x1BBD);
}

#[test]
fn error_display_includes_relevant_data() {
    let err1 = ButtonBoxError::InvalidReportSize {
        expected: 8,
        actual: 3,
    };
    let msg = err1.to_string();
    assert!(msg.contains("8"));
    assert!(msg.contains("3"));

    let err2 = ButtonBoxError::InvalidButtonIndex(42);
    assert!(err2.to_string().contains("42"));

    let err3 = ButtonBoxError::InvalidAxisIndex(7);
    assert!(err3.to_string().contains("7"));

    let err4 = ButtonBoxError::HidError("device disconnected".into());
    assert!(err4.to_string().contains("device disconnected"));
}

#[test]
fn buttonbox_result_ok_and_err() {
    let ok: ButtonBoxResult<u32> = Ok(42);
    assert!(ok.is_ok());

    let err: ButtonBoxResult<u32> = Err(ButtonBoxError::InvalidButtonIndex(99));
    assert!(err.is_err());
}

// ── 7. Parse with specific axis data ───────────────────────────────────────

#[test]
fn parse_gamepad_with_axes() -> R {
    let mut data = [0u8; 10];
    // buttons = 0
    // axis_x = 1000 (little-endian)
    data[2] = 0xE8;
    data[3] = 0x03;
    // axis_y = -1000
    data[4] = 0x18;
    data[5] = 0xFC;

    let report = ButtonBoxInputReport::parse_gamepad(&data).map_err(|e| e.to_string())?;
    assert_eq!(report.axis_x, 0x03E8);
    assert_eq!(report.axis_y, -1000);
    Ok(())
}

#[test]
fn parse_extended_with_full_axes() -> R {
    let mut data = [0u8; 13];
    // Skip buttons (bytes 0-3)
    // axis_x at 4-5
    data[4] = 0x00;
    data[5] = 0x7F; // 32512
    // axis_y at 6-7
    data[6] = 0x01;
    data[7] = 0x00; // 1
    // axis_z at 8-9
    data[8] = 0xFF;
    data[9] = 0xFF; // -1
    // axis_rz at 10-11
    data[10] = 0x00;
    data[11] = 0x80; // -32768

    let report = ButtonBoxInputReport::parse_extended(&data).map_err(|e| e.to_string())?;
    assert_eq!(report.axis_x, 0x7F00);
    assert_eq!(report.axis_y, 1);
    assert_eq!(report.axis_z, -1);
    assert_eq!(report.axis_rz, i16::MIN);
    Ok(())
}

#[test]
fn axis_access_all_indices() {
    let report = ButtonBoxInputReport {
        axis_x: 10,
        axis_y: 20,
        axis_z: 30,
        axis_rz: 40,
        ..Default::default()
    };
    assert_eq!(report.axis(0), 10);
    assert_eq!(report.axis(1), 20);
    assert_eq!(report.axis(2), 30);
    assert_eq!(report.axis(3), 40);
    assert_eq!(report.axis(4), 0);
    assert_eq!(report.axis(99), 0);
}

#[test]
fn report_clone_preserves_all_fields() {
    let mut report = ButtonBoxInputReport::default();
    report.set_button(0, true);
    report.set_button(31, true);
    report.axis_x = 100;
    report.axis_y = -200;
    report.axis_z = 300;
    report.axis_rz = -400;
    report.hat = 3;

    let cloned = report.clone();
    assert_eq!(cloned.buttons, report.buttons);
    assert_eq!(cloned.axis_x, report.axis_x);
    assert_eq!(cloned.axis_y, report.axis_y);
    assert_eq!(cloned.axis_z, report.axis_z);
    assert_eq!(cloned.axis_rz, report.axis_rz);
    assert_eq!(cloned.hat, report.hat);
}

#[test]
fn default_report_hat_is_0xff() {
    let report = ButtonBoxInputReport::default();
    assert_eq!(report.hat, 0xFF);
    assert_eq!(report.hat_direction(), HatDirection::Neutral);
}

#[test]
fn encoder_clone_preserves_state() {
    let mut enc = RotaryEncoderState::new();
    enc.update(42);
    enc.button_pressed = true;
    let cloned = enc.clone();
    assert_eq!(cloned.position, 42);
    assert!(cloned.button_pressed);
}

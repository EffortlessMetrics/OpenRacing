//! Edge-case and boundary-value tests for button box protocol.
//!
//! Focuses on wire-format correctness, boundary values, serde round-trips,
//! and cross-format consistency between gamepad and extended report formats.

use hid_button_box_protocol::{
    ButtonBoxCapabilities, ButtonBoxError, ButtonBoxInputReport, ButtonBoxType, HatDirection,
    RotaryEncoderState, MAX_AXES, MAX_BUTTONS, PRODUCT_ID_BUTTON_BOX, REPORT_SIZE_GAMEPAD,
    VENDOR_ID_GENERIC,
};
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// Constant validation
// ---------------------------------------------------------------------------

#[test]
fn constants_golden_values() {
    assert_eq!(REPORT_SIZE_GAMEPAD, 8);
    assert_eq!(MAX_BUTTONS, 32);
    assert_eq!(MAX_AXES, 4);
    assert_eq!(VENDOR_ID_GENERIC, 0x1209);
    assert_eq!(PRODUCT_ID_BUTTON_BOX, 0x1BBD);
}

// ---------------------------------------------------------------------------
// Report parsing – boundary values
// ---------------------------------------------------------------------------

#[test]
fn gamepad_all_ones_report() -> Result<(), Box<dyn std::error::Error>> {
    let data = [0xFF_u8; 10];
    let report =
        ButtonBoxInputReport::parse_gamepad(&data).map_err(|e| e.to_string())?;
    assert_eq!(report.buttons, 0xFFFF);
    assert_eq!(report.axis_x, -1_i16);
    assert_eq!(report.axis_y, -1_i16);
    assert_eq!(report.axis_z, -1_i16);
    assert_eq!(report.hat_direction(), HatDirection::Neutral);
    Ok(())
}

#[test]
fn gamepad_all_zeros_report() -> Result<(), Box<dyn std::error::Error>> {
    let data = [0x00_u8; 10];
    let report =
        ButtonBoxInputReport::parse_gamepad(&data).map_err(|e| e.to_string())?;
    assert_eq!(report.buttons, 0);
    assert_eq!(report.axis_x, 0);
    assert_eq!(report.axis_y, 0);
    assert_eq!(report.axis_z, 0);
    assert_eq!(report.hat_direction(), HatDirection::Up);
    Ok(())
}

#[test]
fn extended_all_ones_report() -> Result<(), Box<dyn std::error::Error>> {
    let data = [0xFF_u8; 13];
    let report =
        ButtonBoxInputReport::parse_extended(&data).map_err(|e| e.to_string())?;
    assert_eq!(report.buttons, u32::MAX);
    assert_eq!(report.axis_x, -1_i16);
    assert_eq!(report.axis_rz, -1_i16);
    assert_eq!(report.hat_direction(), HatDirection::Neutral);
    Ok(())
}

#[test]
fn extended_all_zeros_report() -> Result<(), Box<dyn std::error::Error>> {
    let data = [0x00_u8; 13];
    let report =
        ButtonBoxInputReport::parse_extended(&data).map_err(|e| e.to_string())?;
    assert_eq!(report.buttons, 0);
    assert_eq!(report.axis_rz, 0);
    assert_eq!(report.hat_direction(), HatDirection::Up);
    Ok(())
}

// ---------------------------------------------------------------------------
// Report parsing – exact wire format validation
// ---------------------------------------------------------------------------

#[test]
fn gamepad_wire_format_buttons_at_byte_boundary() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 10];
    // Button 8 = bit 8 = byte[1] bit 0
    data[1] = 0x01;
    let report =
        ButtonBoxInputReport::parse_gamepad(&data).map_err(|e| e.to_string())?;
    assert!(report.button(8));
    assert!(!report.button(0));
    assert!(!report.button(7));
    assert!(!report.button(9));
    Ok(())
}

#[test]
fn extended_wire_format_high_buttons() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 13];
    // Button 24 = bit 24 = byte[3] bit 0
    data[3] = 0x01;
    let report =
        ButtonBoxInputReport::parse_extended(&data).map_err(|e| e.to_string())?;
    assert!(report.button(24));
    assert!(!report.button(23));
    assert!(!report.button(25));
    assert_eq!(report.button_count(), 1);
    Ok(())
}

// ---------------------------------------------------------------------------
// Gamepad does NOT populate axis_rz (always 0)
// ---------------------------------------------------------------------------

#[test]
fn gamepad_axis_rz_always_zero() -> Result<(), Box<dyn std::error::Error>> {
    let data = [0xFF_u8; 10];
    let report =
        ButtonBoxInputReport::parse_gamepad(&data).map_err(|e| e.to_string())?;
    assert_eq!(report.axis_rz, 0, "gamepad format must not populate axis_rz");
    Ok(())
}

// ---------------------------------------------------------------------------
// Error variant matching
// ---------------------------------------------------------------------------

#[test]
fn error_report_size_fields_accessible() {
    let err = ButtonBoxError::InvalidReportSize {
        expected: 8,
        actual: 3,
    };
    if let ButtonBoxError::InvalidReportSize { expected, actual } = err {
        assert_eq!(expected, 8);
        assert_eq!(actual, 3);
    } else {
        panic!("wrong variant");
    }
}

#[test]
fn error_from_hid_common_preserves_message() {
    let hid = openracing_hid_common::HidCommonError::ReadError("timeout".to_string());
    let bb: ButtonBoxError = hid.into();
    assert!(bb.to_string().contains("timeout"));
}

// ---------------------------------------------------------------------------
// ButtonBoxCapabilities – basic / extended consistency
// ---------------------------------------------------------------------------

#[test]
fn capabilities_basic_has_fewer_features_than_extended() {
    let basic = ButtonBoxCapabilities::basic();
    let ext = ButtonBoxCapabilities::extended();
    assert!(basic.button_count <= ext.button_count);
    assert!(basic.analog_axis_count <= ext.analog_axis_count);
    assert!(basic.rotary_encoder_count <= ext.rotary_encoder_count);
}

#[test]
fn button_box_type_all_variants_distinct() {
    assert_ne!(ButtonBoxType::Simple, ButtonBoxType::Standard);
    assert_ne!(ButtonBoxType::Simple, ButtonBoxType::Extended);
    assert_ne!(ButtonBoxType::Standard, ButtonBoxType::Extended);
}

#[test]
fn rotary_encoder_update_and_read_back() {
    let mut enc = RotaryEncoderState::new();
    enc.update(42);
    enc.button_pressed = true;
    assert_eq!(enc.position, 42);
    assert_eq!(enc.delta, 42);
    assert!(enc.button_pressed);
}

// ---------------------------------------------------------------------------
// RotaryEncoderState – boundary values
// ---------------------------------------------------------------------------

#[test]
fn rotary_encoder_i32_max_boundary() {
    let mut enc = RotaryEncoderState::new();
    enc.update(i32::MAX);
    assert_eq!(enc.position, i32::MAX);
    assert_eq!(enc.delta, 127); // clamped
}

#[test]
fn rotary_encoder_i32_min_boundary() {
    let mut enc = RotaryEncoderState::new();
    enc.update(i32::MIN);
    assert_eq!(enc.position, i32::MIN);
    assert_eq!(enc.delta, -127); // clamped
}

#[test]
fn rotary_encoder_zero_to_zero_delta_is_zero() {
    let mut enc = RotaryEncoderState::new();
    enc.update(0);
    assert_eq!(enc.delta, 0);
}

// ---------------------------------------------------------------------------
// Property tests – additional
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Rotary encoder delta is always within [-127, 127].
    #[test]
    fn prop_rotary_encoder_delta_clamped(
        pos1 in (i32::MIN / 2)..=(i32::MAX / 2),
        pos2 in (i32::MIN / 2)..=(i32::MAX / 2),
    ) {
        let mut enc = RotaryEncoderState::new();
        enc.update(pos1);
        enc.update(pos2);
        prop_assert!((enc.delta as i32) >= -127 && (enc.delta as i32) <= 127);
    }

    /// Extended format buttons: for any u32, button(i) == (buttons >> i) & 1 for i < 32.
    #[test]
    fn prop_button_bit_consistency(buttons: u32) {
        let report = ButtonBoxInputReport { buttons, ..Default::default() };
        for i in 0..32 {
            prop_assert_eq!(
                report.button(i),
                (buttons >> i) & 1 == 1,
                "button({}) must match bit {} of buttons={:#010x}", i, i, buttons
            );
        }
    }

    /// axis_normalized for out-of-range index is always 0.0.
    #[test]
    fn prop_axis_normalized_out_of_range_is_zero(index in 4usize..256) {
        let report = ButtonBoxInputReport {
            axis_x: i16::MAX,
            axis_y: i16::MAX,
            axis_z: i16::MAX,
            axis_rz: i16::MAX,
            ..Default::default()
        };
        prop_assert!((report.axis_normalized(index) - 0.0).abs() < f32::EPSILON);
    }

    /// Capabilities: basic always has fewer buttons than extended.
    #[test]
    fn prop_basic_fewer_buttons_than_extended(_dummy in 0u8..1u8) {
        let basic = ButtonBoxCapabilities::basic();
        let ext = ButtonBoxCapabilities::extended();
        prop_assert!(basic.button_count < ext.button_count);
    }
}

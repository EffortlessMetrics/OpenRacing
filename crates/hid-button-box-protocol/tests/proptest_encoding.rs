//! Property-based tests for button box HID input report byte-layout encoding.
//!
//! Uses proptest with 500 cases to verify that raw byte positions map to the
//! correct fields in [`ButtonBoxInputReport`] for both gamepad and extended
//! report formats.

use hid_button_box_protocol::ButtonBoxInputReport;
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Bytes 0–1 of a gamepad report encode the button bitmask (low 16 bits).
    #[test]
    fn prop_gamepad_buttons_from_bytes(buttons in 0u16..=u16::MAX) {
        let bytes = buttons.to_le_bytes();
        let mut data = [0u8; 10];
        data[0] = bytes[0];
        data[1] = bytes[1];
        let result = ButtonBoxInputReport::parse_gamepad(&data);
        prop_assert!(result.is_ok(), "10-byte report must parse");
        let report = result.expect("already checked is_ok");
        prop_assert_eq!(
            report.buttons as u16, buttons,
            "buttons low 16 bits must match bytes 0–1"
        );
    }

    /// Bytes 2–3 of a gamepad report encode axis_x as a little-endian i16.
    #[test]
    fn prop_gamepad_axis_x_from_bytes(raw in i16::MIN..=i16::MAX) {
        let bytes = raw.to_le_bytes();
        let mut data = [0u8; 10];
        data[2] = bytes[0];
        data[3] = bytes[1];
        let result = ButtonBoxInputReport::parse_gamepad(&data);
        prop_assert!(result.is_ok(), "10-byte report must parse");
        let report = result.expect("already checked is_ok");
        prop_assert_eq!(report.axis_x, raw, "axis_x must match bytes 2–3");
    }

    /// Bytes 4–5 of a gamepad report encode axis_y as a little-endian i16.
    #[test]
    fn prop_gamepad_axis_y_from_bytes(raw in i16::MIN..=i16::MAX) {
        let bytes = raw.to_le_bytes();
        let mut data = [0u8; 10];
        data[4] = bytes[0];
        data[5] = bytes[1];
        let result = ButtonBoxInputReport::parse_gamepad(&data);
        prop_assert!(result.is_ok(), "10-byte report must parse");
        let report = result.expect("already checked is_ok");
        prop_assert_eq!(report.axis_y, raw, "axis_y must match bytes 4–5");
    }

    /// Bytes 6–7 of a gamepad report encode axis_z as a little-endian i16.
    #[test]
    fn prop_gamepad_axis_z_from_bytes(raw in i16::MIN..=i16::MAX) {
        let bytes = raw.to_le_bytes();
        let mut data = [0u8; 10];
        data[6] = bytes[0];
        data[7] = bytes[1];
        let result = ButtonBoxInputReport::parse_gamepad(&data);
        prop_assert!(result.is_ok(), "10-byte report must parse");
        let report = result.expect("already checked is_ok");
        prop_assert_eq!(report.axis_z, raw, "axis_z must match bytes 6–7");
    }

    /// Byte 8 of a gamepad report encodes the hat switch value verbatim.
    #[test]
    fn prop_gamepad_hat_from_byte(hat in 0u8..=255u8) {
        let mut data = [0u8; 10];
        data[8] = hat;
        let result = ButtonBoxInputReport::parse_gamepad(&data);
        prop_assert!(result.is_ok(), "10-byte report must parse");
        let report = result.expect("already checked is_ok");
        prop_assert_eq!(report.hat, hat, "hat must match byte 8");
    }

    /// parse_gamepad succeeds for any slice of 10 or more bytes.
    #[test]
    fn prop_parse_gamepad_sufficient_length(
        data in proptest::collection::vec(any::<u8>(), 10..=64),
    ) {
        prop_assert!(
            ButtonBoxInputReport::parse_gamepad(&data).is_ok(),
            "parse_gamepad must succeed for {} bytes", data.len()
        );
    }

    /// parse_gamepad fails for any slice shorter than REPORT_SIZE_GAMEPAD (8 bytes).
    #[test]
    fn prop_parse_gamepad_short_fails(
        data in proptest::collection::vec(any::<u8>(), 0..8usize),
    ) {
        prop_assert!(
            ButtonBoxInputReport::parse_gamepad(&data).is_err(),
            "parse_gamepad must fail for {} bytes", data.len()
        );
    }

    /// Bytes 0–3 of an extended report encode the full 32-bit button bitmask.
    #[test]
    fn prop_extended_buttons_from_bytes(buttons: u32) {
        let bytes = buttons.to_le_bytes();
        let mut data = [0u8; 13];
        data[0] = bytes[0];
        data[1] = bytes[1];
        data[2] = bytes[2];
        data[3] = bytes[3];
        let result = ButtonBoxInputReport::parse_extended(&data);
        prop_assert!(result.is_ok(), "13-byte extended report must parse");
        let report = result.expect("already checked is_ok");
        prop_assert_eq!(report.buttons, buttons, "buttons must match bytes 0–3");
    }

    /// Bytes 4–5 of an extended report encode axis_x as a little-endian i16.
    #[test]
    fn prop_extended_axis_x_from_bytes(raw in i16::MIN..=i16::MAX) {
        let bytes = raw.to_le_bytes();
        let mut data = [0u8; 13];
        data[4] = bytes[0];
        data[5] = bytes[1];
        let result = ButtonBoxInputReport::parse_extended(&data);
        prop_assert!(result.is_ok(), "13-byte extended report must parse");
        let report = result.expect("already checked is_ok");
        prop_assert_eq!(report.axis_x, raw, "axis_x must match bytes 4–5 in extended");
    }

    /// Bytes 10–11 of an extended report encode axis_rz as a little-endian i16.
    #[test]
    fn prop_extended_axis_rz_from_bytes(raw in i16::MIN..=i16::MAX) {
        let bytes = raw.to_le_bytes();
        let mut data = [0u8; 13];
        data[10] = bytes[0];
        data[11] = bytes[1];
        let result = ButtonBoxInputReport::parse_extended(&data);
        prop_assert!(result.is_ok(), "13-byte extended report must parse");
        let report = result.expect("already checked is_ok");
        prop_assert_eq!(report.axis_rz, raw, "axis_rz must match bytes 10–11");
    }

    /// parse_extended succeeds for any slice of 13 or more bytes.
    #[test]
    fn prop_parse_extended_sufficient_length(
        data in proptest::collection::vec(any::<u8>(), 13..=64),
    ) {
        prop_assert!(
            ButtonBoxInputReport::parse_extended(&data).is_ok(),
            "parse_extended must succeed for {} bytes", data.len()
        );
    }
}

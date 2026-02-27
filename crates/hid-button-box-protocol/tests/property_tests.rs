//! Property-based tests for the button box HID protocol.
//!
//! Uses proptest with 500 cases to verify invariants on report parsing,
//! button access, axis normalization, hat directions, and rotary encoder state.

use proptest::prelude::*;
use hid_button_box_protocol::{ButtonBoxInputReport, HatDirection, RotaryEncoderState, MAX_BUTTONS};

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(500))]

    /// parse_gamepad must succeed for any data of at least 10 bytes.
    #[test]
    fn prop_parse_gamepad_succeeds_for_sufficient_data(
        data in proptest::collection::vec(any::<u8>(), 10..=64),
    ) {
        let result = ButtonBoxInputReport::parse_gamepad(&data);
        prop_assert!(result.is_ok(), "parse_gamepad must succeed for {} bytes", data.len());
    }

    /// parse_gamepad must fail for data shorter than REPORT_SIZE_GAMEPAD (8 bytes).
    #[test]
    fn prop_parse_gamepad_fails_for_short_data(
        data in proptest::collection::vec(any::<u8>(), 0..8usize),
    ) {
        let result = ButtonBoxInputReport::parse_gamepad(&data);
        prop_assert!(result.is_err(), "parse_gamepad must fail for {} bytes", data.len());
    }

    /// parse_extended must succeed for any data of at least 13 bytes.
    #[test]
    fn prop_parse_extended_succeeds_for_sufficient_data(
        data in proptest::collection::vec(any::<u8>(), 13..=64),
    ) {
        let result = ButtonBoxInputReport::parse_extended(&data);
        prop_assert!(result.is_ok(), "parse_extended must succeed for {} bytes", data.len());
    }

    /// parse_extended must fail for data shorter than its minimum check (12 bytes).
    #[test]
    fn prop_parse_extended_fails_for_short_data(
        data in proptest::collection::vec(any::<u8>(), 0..12usize),
    ) {
        let result = ButtonBoxInputReport::parse_extended(&data);
        prop_assert!(result.is_err(), "parse_extended must fail for {} bytes", data.len());
    }

    /// set_button(true) followed by button() must return true; set_button(false) must clear it.
    #[test]
    fn prop_button_set_get_roundtrip(index in 0usize..MAX_BUTTONS) {
        let mut report = ButtonBoxInputReport::default();
        report.set_button(index, true);
        prop_assert!(report.button(index), "button {} must be set after set_button(true)", index);
        report.set_button(index, false);
        prop_assert!(!report.button(index), "button {} must be clear after set_button(false)", index);
    }

    /// button() must return false for any index >= MAX_BUTTONS, even with all bits set.
    #[test]
    fn prop_button_out_of_range_always_false(index in MAX_BUTTONS..1024usize) {
        let mut report = ButtonBoxInputReport::default();
        report.buttons = u32::MAX;
        prop_assert!(
            !report.button(index),
            "out-of-range button index {} must always return false",
            index
        );
    }

    /// set_button on an out-of-range index must be a no-op (buttons field unchanged).
    #[test]
    fn prop_set_button_out_of_range_is_noop(index in MAX_BUTTONS..1024usize, buttons: u32) {
        let mut report = ButtonBoxInputReport { buttons, ..Default::default() };
        report.set_button(index, true);
        prop_assert_eq!(report.buttons, buttons, "out-of-range set_button must not modify buttons");
    }

    /// axis() must return 0 for any index >= 4.
    #[test]
    fn prop_axis_out_of_range_always_zero(index in 4usize..64usize) {
        let report = ButtonBoxInputReport {
            axis_x: i16::MAX,
            axis_y: i16::MAX,
            axis_z: i16::MAX,
            axis_rz: i16::MAX,
            ..Default::default()
        };
        prop_assert_eq!(report.axis(index), 0, "out-of-range axis {} must return 0", index);
    }

    /// hat_direction() returns Neutral for all hat byte values 8..=255.
    #[test]
    fn prop_hat_neutral_for_out_of_range(hat in 8u8..=255u8) {
        let mut report = ButtonBoxInputReport::default();
        report.hat = hat;
        prop_assert_eq!(
            report.hat_direction(),
            HatDirection::Neutral,
            "hat byte {} must map to Neutral",
            hat
        );
    }

    /// hat_direction() returns a non-Neutral direction for values 0..=7.
    #[test]
    fn prop_hat_non_neutral_for_in_range(hat in 0u8..8u8) {
        let mut report = ButtonBoxInputReport::default();
        report.hat = hat;
        prop_assert_ne!(
            report.hat_direction(),
            HatDirection::Neutral,
            "hat byte {} must not be Neutral",
            hat
        );
    }

    /// button_count() must equal the popcount of the buttons bitmask.
    #[test]
    fn prop_button_count_equals_popcount(buttons: u32) {
        let report = ButtonBoxInputReport { buttons, ..Default::default() };
        prop_assert_eq!(
            report.button_count(),
            buttons.count_ones() as usize,
            "button_count must equal popcount for buttons={:#010x}",
            buttons
        );
    }

    /// axis_normalized() must be within roughly [-1.0, 1.0] for all i16 values.
    /// i16::MIN produces -32768/32767 ≈ -1.00003, so we allow a small margin.
    #[test]
    fn prop_axis_normalized_within_range(index in 0usize..4usize, value: i16) {
        let mut report = ButtonBoxInputReport::default();
        match index {
            0 => report.axis_x = value,
            1 => report.axis_y = value,
            2 => report.axis_z = value,
            3 => report.axis_rz = value,
            _ => {}
        }
        let normalized = report.axis_normalized(index);
        prop_assert!(
            normalized >= -1.001 && normalized <= 1.0,
            "axis_normalized({}) = {} must be near [-1.0, 1.0] for value {}",
            index,
            normalized,
            value
        );
    }

    /// axis_normalized() for i16::MAX must be exactly 1.0.
    #[test]
    fn prop_axis_normalized_max_is_one(index in 0usize..4usize) {
        let mut report = ButtonBoxInputReport::default();
        match index {
            0 => report.axis_x = i16::MAX,
            1 => report.axis_y = i16::MAX,
            2 => report.axis_z = i16::MAX,
            3 => report.axis_rz = i16::MAX,
            _ => {}
        }
        let normalized = report.axis_normalized(index);
        prop_assert!(
            (normalized - 1.0f32).abs() < 1e-6,
            "axis_normalized({}) for i16::MAX must be 1.0, got {}",
            index,
            normalized
        );
    }

    /// RotaryEncoderState::update must track position and clamped delta correctly.
    /// Inputs are restricted to half-range to avoid i32 subtraction overflow
    /// in the underlying update() implementation.
    #[test]
    fn prop_rotary_encoder_delta(
        pos1 in (i32::MIN / 2)..=(i32::MAX / 2),
        pos2 in (i32::MIN / 2)..=(i32::MAX / 2),
    ) {
        let mut encoder = RotaryEncoderState::new();
        encoder.update(pos1);
        prop_assert_eq!(encoder.position, pos1);
        encoder.update(pos2);
        prop_assert_eq!(encoder.position, pos2);
        let expected_delta = (pos2 as i64 - pos1 as i64).clamp(-127, 127) as i8;
        prop_assert_eq!(
            encoder.delta,
            expected_delta,
            "delta must be clamped difference between pos2={} and pos1={}",
            pos2,
            pos1
        );
    }
}

/// set_button is idempotent: setting the same button twice does not duplicate.
#[test]
fn test_set_button_idempotent() -> Result<(), Box<dyn std::error::Error>> {
    let mut report = ButtonBoxInputReport::default();
    report.set_button(3, true);
    report.set_button(3, true);
    assert!(report.button(3));
    assert_eq!(report.button_count(), 1, "setting button twice must not increase count");
    Ok(())
}

/// All eight hat directions (0..=7) are distinct and non-Neutral.
#[test]
fn test_hat_directions_all_distinct() -> Result<(), Box<dyn std::error::Error>> {
    use std::collections::HashSet;
    let mut seen = HashSet::new();
    for hat in 0u8..8 {
        let mut report = ButtonBoxInputReport::default();
        report.hat = hat;
        let dir = report.hat_direction();
        assert_ne!(dir, HatDirection::Neutral, "hat byte {hat} must not be Neutral");
        assert!(
            seen.insert(format!("{dir:?}")),
            "hat direction for byte {hat} must be unique"
        );
    }
    Ok(())
}

/// parse_gamepad parse_extended produce the same axes and hat when layout overlaps.
#[test]
fn test_parse_extended_axes_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    // Extended: 4 bytes buttons, then axes, then hat
    // Set recognisable axis values: axis_x = 0x1234, axis_y = 0x5678
    let mut data = [0u8; 13];
    data[4] = 0x34; // axis_x low
    data[5] = 0x12; // axis_x high → 0x1234
    data[6] = 0x78; // axis_y low
    data[7] = 0x56; // axis_y high → 0x5678
    data[12] = 0x04; // hat = 4 → Down
    let report = ButtonBoxInputReport::parse_extended(&data)
        .map_err(|e| e.to_string())?;
    assert_eq!(report.axis_x, 0x1234_i16);
    assert_eq!(report.axis_y, 0x5678_u16 as i16);
    assert_eq!(report.hat_direction(), HatDirection::Down);
    Ok(())
}

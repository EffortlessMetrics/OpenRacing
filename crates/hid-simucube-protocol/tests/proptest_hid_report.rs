//! Property-based tests for the documented Simucube HID joystick report.
//!
//! Uses proptest with 500 cases to verify invariants on parsing, normalisation,
//! and button indexing for [`SimucubeHidReport`].

use hid_simucube_protocol::{HID_BUTTON_BYTES, HID_JOYSTICK_REPORT_MIN_BYTES, SimucubeHidReport};
use proptest::prelude::*;

/// Strategy: build a valid 32-byte HID report from arbitrary field values.
fn arb_hid_bytes() -> impl Strategy<Value = Vec<u8>> {
    proptest::collection::vec(any::<u8>(), HID_JOYSTICK_REPORT_MIN_BYTES..=64)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Parsing must succeed for any byte buffer ≥ 32 bytes.
    #[test]
    fn prop_parse_succeeds_for_valid_size(data in arb_hid_bytes()) {
        let result = SimucubeHidReport::parse(&data);
        prop_assert!(result.is_ok(), "parse must succeed for {} bytes", data.len());
    }

    /// Parsing must fail for any buffer shorter than 32 bytes.
    #[test]
    fn prop_parse_fails_for_short_buffer(len in 0usize..HID_JOYSTICK_REPORT_MIN_BYTES) {
        let data = vec![0u8; len];
        let result = SimucubeHidReport::parse(&data);
        prop_assert!(result.is_err(), "parse must fail for {len} bytes");
    }

    /// steering_normalized must always be in [0.0, 1.0].
    #[test]
    fn prop_steering_normalized_range(data in arb_hid_bytes()) {
        let report = SimucubeHidReport::parse(&data).map_err(|e| {
            TestCaseError::fail(format!("{e:?}"))
        })?;
        let n = report.steering_normalized();
        prop_assert!((0.0..=1.0).contains(&n),
            "steering_normalized={n} out of [0,1]");
    }

    /// steering_signed must always be in [-1.0, 1.0].
    #[test]
    fn prop_steering_signed_range(data in arb_hid_bytes()) {
        let report = SimucubeHidReport::parse(&data).map_err(|e| {
            TestCaseError::fail(format!("{e:?}"))
        })?;
        let s = report.steering_signed();
        prop_assert!((-1.001..=1.001).contains(&s),
            "steering_signed={s} out of [-1,1]");
    }

    /// button_pressed must return false for all out-of-range indices.
    #[test]
    fn prop_button_out_of_range_is_false(idx in 128usize..512usize) {
        let report = SimucubeHidReport::default();
        prop_assert!(!report.button_pressed(idx),
            "button {idx} must be false (out of range)");
    }

    /// pressed_count must equal the sum of popcount of all button bytes.
    #[test]
    fn prop_pressed_count_matches_popcount(data in arb_hid_bytes()) {
        let report = SimucubeHidReport::parse(&data).map_err(|e| {
            TestCaseError::fail(format!("{e:?}"))
        })?;
        let expected: u32 = report.buttons.iter().map(|b| b.count_ones()).sum();
        prop_assert_eq!(report.pressed_count(), expected);
    }

    /// axis_normalized must be in [0.0, 1.0] for valid indices and 0.0 for invalid.
    #[test]
    fn prop_axis_normalized_range(data in arb_hid_bytes(), idx in 0usize..10usize) {
        let report = SimucubeHidReport::parse(&data).map_err(|e| {
            TestCaseError::fail(format!("{e:?}"))
        })?;
        let v = report.axis_normalized(idx);
        if idx < 6 {
            prop_assert!((0.0..=1.0).contains(&v),
                "axis_normalized({idx})={v} out of [0,1]");
        } else {
            prop_assert!(v == 0.0,
                "axis_normalized({}) must be 0.0 for out-of-range", idx);
        }
    }

    /// Round-trip: fields parsed from constructed bytes must match the input.
    #[test]
    fn prop_field_roundtrip(
        steering: u16,
        y_axis: u16,
        ax0: u16, ax1: u16, ax2: u16, ax3: u16, ax4: u16, ax5: u16,
        btn_data in proptest::collection::vec(any::<u8>(), HID_BUTTON_BYTES..=HID_BUTTON_BYTES),
    ) {
        let mut buf = Vec::with_capacity(HID_JOYSTICK_REPORT_MIN_BYTES);
        buf.extend_from_slice(&steering.to_le_bytes());
        buf.extend_from_slice(&y_axis.to_le_bytes());
        for ax in [ax0, ax1, ax2, ax3, ax4, ax5] {
            buf.extend_from_slice(&ax.to_le_bytes());
        }
        buf.extend_from_slice(&btn_data);

        let report = SimucubeHidReport::parse(&buf).map_err(|e| {
            TestCaseError::fail(format!("{e:?}"))
        })?;
        prop_assert_eq!(report.steering, steering);
        prop_assert_eq!(report.y_axis, y_axis);
        prop_assert_eq!(report.axes, [ax0, ax1, ax2, ax3, ax4, ax5]);
        prop_assert_eq!(&report.buttons[..], &btn_data[..]);
    }
}

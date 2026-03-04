//! Property-based tests for OpenFFBoard input report parsing.

use proptest::prelude::*;
use racing_wheel_hid_openffboard_protocol::input::{
    BUTTON_BYTES, INPUT_REPORT_ID, INPUT_REPORT_LEN, MAX_BUTTONS, OpenFFBoardInputReport,
};

/// Strategy producing valid 25-byte input reports.
fn valid_report() -> impl Strategy<Value = Vec<u8>> {
    proptest::collection::vec(any::<u8>(), INPUT_REPORT_LEN..=INPUT_REPORT_LEN).prop_map(|mut r| {
        r[0] = INPUT_REPORT_ID;
        r
    })
}

/// Strategy producing an 8-byte button array.
fn button_array() -> impl Strategy<Value = [u8; BUTTON_BYTES]> {
    (
        any::<u8>(),
        any::<u8>(),
        any::<u8>(),
        any::<u8>(),
        any::<u8>(),
        any::<u8>(),
        any::<u8>(),
        any::<u8>(),
    )
        .prop_map(|(a, b, c, d, e, f, g, h)| [a, b, c, d, e, f, g, h])
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    #[test]
    fn parse_never_panics_on_arbitrary_bytes(data in proptest::collection::vec(any::<u8>(), 0..100)) {
        // Must not panic regardless of input
        let _ = OpenFFBoardInputReport::parse(&data);
    }

    #[test]
    fn parse_succeeds_for_valid_reports(report in valid_report()) {
        let result = OpenFFBoardInputReport::parse(&report);
        prop_assert!(result.is_some(), "valid report should parse");
    }

    #[test]
    fn axes_roundtrip(
        a0 in any::<i16>(), a1 in any::<i16>(), a2 in any::<i16>(), a3 in any::<i16>(),
        a4 in any::<i16>(), a5 in any::<i16>(), a6 in any::<i16>(), a7 in any::<i16>(),
    ) {
        let values = [a0, a1, a2, a3, a4, a5, a6, a7];
        let mut report = [0u8; INPUT_REPORT_LEN];
        report[0] = INPUT_REPORT_ID;
        for (i, &v) in values.iter().enumerate() {
            let bytes = v.to_le_bytes();
            report[9 + i * 2] = bytes[0];
            report[9 + i * 2 + 1] = bytes[1];
        }
        let parsed = OpenFFBoardInputReport::parse(&report);
        prop_assert!(parsed.is_some());
        let p = parsed.expect("checked above");
        prop_assert_eq!(p.axes, values);
    }

    #[test]
    fn buttons_roundtrip(buttons in button_array()) {
        let mut report = [0u8; INPUT_REPORT_LEN];
        report[0] = INPUT_REPORT_ID;
        report[1..9].copy_from_slice(&buttons);
        let parsed = OpenFFBoardInputReport::parse(&report);
        prop_assert!(parsed.is_some());
        let p = parsed.expect("checked above");
        prop_assert_eq!(p.buttons, buttons);
    }

    #[test]
    fn button_accessor_consistent_with_raw(buttons in button_array()) {
        let mut report = [0u8; INPUT_REPORT_LEN];
        report[0] = INPUT_REPORT_ID;
        report[1..9].copy_from_slice(&buttons);
        let parsed = OpenFFBoardInputReport::parse(&report).expect("valid report");
        for n in 0..MAX_BUTTONS {
            let byte_idx = n / 8;
            let bit_idx = n % 8;
            let expected = (buttons[byte_idx] >> bit_idx) & 1 == 1;
            prop_assert_eq!(parsed.button(n), expected, "button {} mismatch", n);
        }
    }

    #[test]
    fn buttons_pressed_matches_popcount(buttons in button_array()) {
        let mut report = [0u8; INPUT_REPORT_LEN];
        report[0] = INPUT_REPORT_ID;
        report[1..9].copy_from_slice(&buttons);
        let parsed = OpenFFBoardInputReport::parse(&report).expect("valid report");
        let expected: u32 = buttons.iter().map(|b| b.count_ones()).sum();
        prop_assert_eq!(parsed.buttons_pressed(), expected);
    }

    #[test]
    fn steering_normalized_bounded(raw_steering in -32767i16..=32767i16) {
        let mut report = [0u8; INPUT_REPORT_LEN];
        report[0] = INPUT_REPORT_ID;
        let bytes = raw_steering.to_le_bytes();
        report[9] = bytes[0];
        report[10] = bytes[1];
        let parsed = OpenFFBoardInputReport::parse(&report).expect("valid report");
        let normalized = parsed.steering_normalized();
        prop_assert!((-1.0..=1.0).contains(&normalized),
            "steering_normalized() = {} should be in [-1.0, 1.0]", normalized);
    }

    #[test]
    fn named_axis_accessors_match_array(report in valid_report()) {
        let parsed = OpenFFBoardInputReport::parse(&report).expect("valid report");
        prop_assert_eq!(parsed.x(), parsed.axes[0]);
        prop_assert_eq!(parsed.y(), parsed.axes[1]);
        prop_assert_eq!(parsed.z(), parsed.axes[2]);
        prop_assert_eq!(parsed.rx(), parsed.axes[3]);
        prop_assert_eq!(parsed.ry(), parsed.axes[4]);
        prop_assert_eq!(parsed.rz(), parsed.axes[5]);
        prop_assert_eq!(parsed.dial(), parsed.axes[6]);
        prop_assert_eq!(parsed.slider(), parsed.axes[7]);
    }

    #[test]
    fn wrong_report_id_always_fails(id in 0x02u8..=0xFF) {
        let mut report = [0u8; INPUT_REPORT_LEN];
        report[0] = id;
        prop_assert!(OpenFFBoardInputReport::parse(&report).is_none());
    }

    #[test]
    fn short_reports_always_fail(len in 0usize..INPUT_REPORT_LEN) {
        let mut data = vec![0u8; len];
        if !data.is_empty() {
            data[0] = INPUT_REPORT_ID;
        }
        prop_assert!(OpenFFBoardInputReport::parse(&data).is_none());
    }
}

//! Property-based tests for Heusinkveld pedal input report parsing.
//!
//! Uses proptest with 500 cases to verify:
//! - Input report parsing succeeds for valid-length buffers and fails for short ones
//! - Parsed field values match the raw bytes (parse round-trip)
//! - Axis normalization is always in [0.0, 1.0]
//! - Status flag decoding is correct (is_connected, is_calibrated, has_fault)
//! - Model metadata is consistent (positive max_load_kg, non-empty display names)

use hid_heusinkveld_protocol::{
    HeusinkveldInputReport, HeusinkveldModel, MAX_LOAD_CELL_VALUE, REPORT_SIZE_INPUT,
};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Parsing any buffer of exactly REPORT_SIZE_INPUT bytes must always succeed.
    #[test]
    fn prop_parse_valid_length_always_ok(data: [u8; 8]) {
        let result = HeusinkveldInputReport::parse(&data);
        prop_assert!(result.is_ok(),
            "parse of {}-byte buffer must succeed", REPORT_SIZE_INPUT);
    }

    /// Parsing a buffer shorter than REPORT_SIZE_INPUT must always fail.
    #[test]
    fn prop_parse_short_buffer_always_err(len in 0usize..REPORT_SIZE_INPUT) {
        let data = vec![0u8; len];
        let result = HeusinkveldInputReport::parse(&data);
        prop_assert!(result.is_err(),
            "parse of {len}-byte buffer must fail (REPORT_SIZE_INPUT={REPORT_SIZE_INPUT})");
    }

    /// Parsed fields must reflect the raw bytes in the input buffer (round-trip).
    /// throttle = LE u16 at bytes [0..2], brake = [2..4], clutch = [4..6], status = byte 6.
    #[test]
    fn prop_parse_fields_match_raw_bytes(data: [u8; 8]) {
        let result = HeusinkveldInputReport::parse(&data);
        prop_assert!(result.is_ok(), "parse must succeed for 8-byte buffer");
        if let Ok(report) = result {
            prop_assert_eq!(report.throttle, u16::from_le_bytes([data[0], data[1]]),
                "throttle must be LE u16 of bytes [0..2]");
            prop_assert_eq!(report.brake, u16::from_le_bytes([data[2], data[3]]),
                "brake must be LE u16 of bytes [2..4]");
            prop_assert_eq!(report.clutch, u16::from_le_bytes([data[4], data[5]]),
                "clutch must be LE u16 of bytes [4..6]");
            prop_assert_eq!(report.status, data[6],
                "status must be byte 6");
        }
    }

    /// throttle_normalized() must always be in [0.0, 1.0] for any u16 raw value.
    #[test]
    fn prop_throttle_normalized_in_unit_range(throttle: u16) {
        let report = HeusinkveldInputReport { throttle, brake: 0, clutch: 0, status: 0 };
        let n = report.throttle_normalized();
        prop_assert!((0.0f32..=1.0).contains(&n),
            "throttle_normalized()={n} must be in [0.0, 1.0] for throttle={throttle}");
    }

    /// brake_normalized() must always be in [0.0, 1.0] for any u16 raw value.
    #[test]
    fn prop_brake_normalized_in_unit_range(brake: u16) {
        let report = HeusinkveldInputReport { throttle: 0, brake, clutch: 0, status: 0 };
        let n = report.brake_normalized();
        prop_assert!((0.0f32..=1.0).contains(&n),
            "brake_normalized()={n} must be in [0.0, 1.0] for brake={brake}");
    }

    /// clutch_normalized() must always be in [0.0, 1.0] for any u16 raw value.
    #[test]
    fn prop_clutch_normalized_in_unit_range(clutch: u16) {
        let report = HeusinkveldInputReport { throttle: 0, brake: 0, clutch, status: 0 };
        let n = report.clutch_normalized();
        prop_assert!((0.0f32..=1.0).contains(&n),
            "clutch_normalized()={n} must be in [0.0, 1.0] for clutch={clutch}");
    }

    /// MAX_LOAD_CELL_VALUE must normalize to exactly 1.0 for all three axes.
    #[test]
    fn prop_max_load_cell_normalizes_to_one(_unused: u8) {
        let report = HeusinkveldInputReport {
            throttle: MAX_LOAD_CELL_VALUE,
            brake: MAX_LOAD_CELL_VALUE,
            clutch: MAX_LOAD_CELL_VALUE,
            status: 0,
        };
        prop_assert_eq!(report.throttle_normalized(), 1.0f32,
            "throttle_normalized(MAX_LOAD_CELL_VALUE) must be exactly 1.0");
        prop_assert_eq!(report.brake_normalized(), 1.0f32,
            "brake_normalized(MAX_LOAD_CELL_VALUE) must be exactly 1.0");
        prop_assert_eq!(report.clutch_normalized(), 1.0f32,
            "clutch_normalized(MAX_LOAD_CELL_VALUE) must be exactly 1.0");
    }

    /// is_connected() must be true iff bit 0 of status is set.
    #[test]
    fn prop_is_connected_reflects_bit0(status: u8) {
        let report = HeusinkveldInputReport { throttle: 0, brake: 0, clutch: 0, status };
        prop_assert_eq!(report.is_connected(), (status & 0x01) != 0,
            "is_connected must reflect bit 0 of status={:#04x}", status);
    }

    /// is_calibrated() must be true iff bit 1 of status is set.
    #[test]
    fn prop_is_calibrated_reflects_bit1(status: u8) {
        let report = HeusinkveldInputReport { throttle: 0, brake: 0, clutch: 0, status };
        prop_assert_eq!(report.is_calibrated(), (status & 0x02) != 0,
            "is_calibrated must reflect bit 1 of status={:#04x}", status);
    }

    /// has_fault() must be true iff bit 2 of status is set.
    #[test]
    fn prop_has_fault_reflects_bit2(status: u8) {
        let report = HeusinkveldInputReport { throttle: 0, brake: 0, clutch: 0, status };
        prop_assert_eq!(report.has_fault(), (status & 0x04) != 0,
            "has_fault must reflect bit 2 of status={:#04x}", status);
    }

    /// HeusinkveldModel::max_load_kg must always be strictly positive.
    #[test]
    fn prop_model_max_load_positive(idx in 0usize..3usize) {
        let models = [HeusinkveldModel::Sprint, HeusinkveldModel::Ultimate, HeusinkveldModel::Pro];
        let load = models[idx].max_load_kg();
        prop_assert!(load > 0.0,
            "{:?} must have positive max_load_kg, got {load}", models[idx]);
    }

    /// HeusinkveldModel::display_name must never be empty for any known model.
    #[test]
    fn prop_model_display_name_non_empty(idx in 0usize..4usize) {
        let models = [
            HeusinkveldModel::Sprint,
            HeusinkveldModel::Ultimate,
            HeusinkveldModel::Pro,
            HeusinkveldModel::Unknown,
        ];
        prop_assert!(!models[idx].display_name().is_empty(),
            "{:?} must have a non-empty display_name", models[idx]);
    }
}

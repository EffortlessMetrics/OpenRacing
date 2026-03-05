//! Fuzz-style property tests for Simucube protocol edge cases.
//!
//! Feeds arbitrary byte sequences and extreme values into all parse and encode
//! entry points to ensure no panics occur and all outputs are well-formed.

use hid_simucube_protocol::{
    DeviceStatus, HID_JOYSTICK_REPORT_MIN_BYTES, SimucubeHidReport, SimucubeInputReport,
    SimucubeModel,
};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    // ── Arbitrary-bytes fuzz: SimucubeHidReport::parse ────────────────────

    /// Feeding any byte sequence to SimucubeHidReport::parse must never panic.
    #[test]
    fn fuzz_hid_report_arbitrary_bytes(
        data in proptest::collection::vec(any::<u8>(), 0..=128),
    ) {
        let _ = SimucubeHidReport::parse(&data);
    }

    /// Short buffers (< 32 bytes) must return Err, not panic.
    #[test]
    fn fuzz_hid_report_short_lengths(len in 0usize..32) {
        let data = vec![0xFFu8; len];
        let result = SimucubeHidReport::parse(&data);
        prop_assert!(result.is_err(),
            "HidReport::parse with len={len} must return Err");
    }

    /// Exactly HID_JOYSTICK_REPORT_MIN_BYTES (32) bytes must succeed.
    #[test]
    fn fuzz_hid_report_exact_minimum(
        data in proptest::collection::vec(any::<u8>(), HID_JOYSTICK_REPORT_MIN_BYTES..=HID_JOYSTICK_REPORT_MIN_BYTES),
    ) {
        let result = SimucubeHidReport::parse(&data);
        prop_assert!(result.is_ok(),
            "HidReport::parse with exactly {} bytes must succeed", HID_JOYSTICK_REPORT_MIN_BYTES);
    }

    /// When HidReport::parse succeeds, the axes and buttons arrays have the
    /// expected sizes.
    #[test]
    fn fuzz_hid_report_output_invariants(
        data in proptest::collection::vec(any::<u8>(), 32..=128),
    ) {
        if let Ok(report) = SimucubeHidReport::parse(&data) {
            prop_assert_eq!(report.axes.len(), 6, "must have exactly 6 additional axes");
            prop_assert_eq!(report.buttons.len(), 16, "must have exactly 16 button bytes");
            // Verify steering and y_axis parsed without panic (values are u16, always valid)
            let _ = report.steering;
            let _ = report.y_axis;
        }
    }

    // ── Arbitrary-bytes fuzz: SimucubeInputReport::parse ──────────────────

    /// Feeding any byte sequence to SimucubeInputReport::parse must never
    /// panic.
    #[test]
    fn fuzz_input_report_arbitrary_bytes(
        data in proptest::collection::vec(any::<u8>(), 0..=128),
    ) {
        let _ = SimucubeInputReport::parse(&data);
    }

    // ── DeviceStatus::from_flags ─────────────────────────────────────────

    /// from_flags must never panic for any u8 and must be deterministic.
    #[test]
    fn fuzz_device_status_from_flags(flags: u8) {
        let a = DeviceStatus::from_flags(flags);
        let b = DeviceStatus::from_flags(flags);
        prop_assert_eq!(a, b,
            "DeviceStatus::from_flags(0x{:02X}) must be deterministic", flags);
    }

    // ── SimucubeModel::from_product_id ───────────────────────────────────

    /// from_product_id must never panic for any u16 and must be deterministic.
    #[test]
    fn fuzz_model_from_product_id(pid: u16) {
        let a = SimucubeModel::from_product_id(pid);
        let b = SimucubeModel::from_product_id(pid);
        prop_assert_eq!(a, b,
            "SimucubeModel::from_product_id(0x{:04X}) must be deterministic", pid);
    }

    // ── parse_block_load fuzz ────────────────────────────────────────────

    /// parse_block_load must never panic for any byte sequence.
    #[test]
    fn fuzz_parse_block_load_arbitrary_bytes(
        data in proptest::collection::vec(any::<u8>(), 0..=64),
    ) {
        let _ = hid_simucube_protocol::parse_block_load(&data);
    }

    // ── Encoder: NaN / Inf / extreme values ──────────────────────────────

    /// Building an output report with NaN torque must not panic.
    #[test]
    fn fuzz_output_report_nan(seq: u16) {
        let report = hid_simucube_protocol::SimucubeOutputReport::new(seq)
            .with_torque(f32::NAN);
        let _ = report.build();
    }

    /// Building an output report with ±Inf torque must not panic.
    #[test]
    fn fuzz_output_report_inf(positive: bool, seq: u16) {
        let val = if positive { f32::INFINITY } else { f32::NEG_INFINITY };
        let report = hid_simucube_protocol::SimucubeOutputReport::new(seq)
            .with_torque(val);
        let _ = report.build();
    }

    /// Building an output report with extreme torque must not overflow.
    #[test]
    fn fuzz_output_report_extreme_torque(torque in -1e10f32..=1e10f32, seq: u16) {
        let report = hid_simucube_protocol::SimucubeOutputReport::new(seq)
            .with_torque(torque);
        let _ = report.build();
    }

    /// is_simucube_device must never panic and must be deterministic.
    #[test]
    fn fuzz_is_simucube_device(vid: u16) {
        let a = hid_simucube_protocol::is_simucube_device(vid);
        let b = hid_simucube_protocol::is_simucube_device(vid);
        prop_assert_eq!(a, b,
            "is_simucube_device(0x{:04X}) must be deterministic", vid);
    }
}

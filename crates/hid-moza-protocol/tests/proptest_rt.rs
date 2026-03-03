//! Property-based tests for the RT `TorqueEncoder` trait implementation
//! and slew-rate encoding on `MozaDirectTorqueEncoder`.
//!
//! Covers:
//! - RT encode path (Q8.8 input) always produces correct report ID and length
//! - RT encode preserves torque sign
//! - `encode_zero` always disables motor
//! - Extra flags are OR'd into the output flags byte
//! - Slew-rate bit and payload consistency
//! - `positive_is_clockwise` always true for Moza

use proptest::prelude::*;
use racing_wheel_hid_moza_protocol::{MozaDirectTorqueEncoder, REPORT_LEN, TorqueEncoder};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    // ── RT TorqueEncoder::encode ─────────────────────────────────────────────

    /// RT encode always produces `REPORT_LEN` bytes with report ID 0x20.
    #[test]
    fn prop_rt_encode_report_id_and_len(
        max in 0.1_f32..=21.0_f32,
        torque: i16,
        seq: u16,
        flags: u8,
    ) {
        let enc = MozaDirectTorqueEncoder::new(max);
        let mut out = [0u8; REPORT_LEN];
        let len = TorqueEncoder::encode(&enc, torque, seq, flags, &mut out);
        prop_assert_eq!(len, REPORT_LEN, "encode must return REPORT_LEN");
        prop_assert_eq!(out[0], 0x20, "report ID must be 0x20 (DIRECT_TORQUE)");
    }

    /// RT encode preserves sign: positive Q8.8 → non-negative raw, negative → non-positive.
    #[test]
    fn prop_rt_encode_sign_preserved(
        max in 0.1_f32..=21.0_f32,
        torque: i16,
    ) {
        let enc = MozaDirectTorqueEncoder::new(max);
        let mut out = [0u8; REPORT_LEN];
        TorqueEncoder::encode(&enc, torque, 0, 0, &mut out);
        let raw = i16::from_le_bytes([out[1], out[2]]);

        // Small Q8.8 values near zero may quantize to 0; use a threshold.
        if torque > 1 {
            prop_assert!(
                raw >= 0,
                "positive Q8.8 {torque} must yield non-negative raw {raw}"
            );
        } else if torque < -1 {
            prop_assert!(
                raw <= 0,
                "negative Q8.8 {torque} must yield non-positive raw {raw}"
            );
        }
    }

    // ── encode_zero ──────────────────────────────────────────────────────────

    /// RT `encode_zero` always produces zero torque with motor disabled.
    #[test]
    fn prop_rt_encode_zero_disables_motor(max in 0.1_f32..=21.0_f32) {
        let enc = MozaDirectTorqueEncoder::new(max);
        let mut out = [0xFFu8; REPORT_LEN];
        let len = TorqueEncoder::encode_zero(&enc, &mut out);
        let raw = i16::from_le_bytes([out[1], out[2]]);

        prop_assert_eq!(len, REPORT_LEN);
        prop_assert_eq!(raw, 0, "encode_zero must produce raw=0");
        prop_assert_eq!(out[3] & 0x01, 0, "motor-enable bit must be clear for zero torque");
    }

    // ── Flags passthrough ────────────────────────────────────────────────────

    /// Extra flags passed to RT encode are OR'd into the output flags byte.
    #[test]
    fn prop_rt_encode_flags_passthrough(
        max in 0.1_f32..=21.0_f32,
        torque in 256i16..=i16::MAX,
        flags: u8,
    ) {
        let enc = MozaDirectTorqueEncoder::new(max);
        let mut out = [0u8; REPORT_LEN];
        TorqueEncoder::encode(&enc, torque, 0, flags, &mut out);
        prop_assert_eq!(
            out[3] & flags,
            flags,
            "all input flag bits {:#04x} must appear in output {:#04x}",
            flags, out[3]
        );
    }

    // ── Slew-rate ────────────────────────────────────────────────────────────

    /// Slew-rate flag (bit 1) is always set and payload matches when configured.
    #[test]
    fn prop_slew_rate_flag_and_payload(
        max in 0.1_f32..=21.0_f32,
        torque in -21.0_f32..=21.0_f32,
        slew_rate: u16,
    ) {
        let enc = MozaDirectTorqueEncoder::new(max).with_slew_rate(slew_rate);
        let mut out = [0u8; REPORT_LEN];
        enc.encode(torque, 0, &mut out);

        prop_assert_eq!(
            out[3] & 0x02,
            0x02,
            "slew-rate flag (bit 1) must be set"
        );
        prop_assert_eq!(
            u16::from_le_bytes([out[4], out[5]]),
            slew_rate,
            "slew rate payload must match configured value"
        );
    }

    /// Without slew-rate, bit 1 is clear and bytes 4-5 are zero.
    #[test]
    fn prop_no_slew_rate_clears_flag_and_payload(
        max in 0.1_f32..=21.0_f32,
        torque in -21.0_f32..=21.0_f32,
    ) {
        let enc = MozaDirectTorqueEncoder::new(max);
        let mut out = [0u8; REPORT_LEN];
        enc.encode(torque, 0, &mut out);

        prop_assert_eq!(
            out[3] & 0x02,
            0x00,
            "slew-rate flag (bit 1) must be clear without slew_rate"
        );
        prop_assert_eq!(out[4], 0x00, "byte 4 must be 0 without slew rate");
        prop_assert_eq!(out[5], 0x00, "byte 5 must be 0 without slew rate");
    }

    // ── Direction ────────────────────────────────────────────────────────────

    /// `positive_is_clockwise` is always true for Moza encoders.
    #[test]
    fn prop_positive_is_clockwise(max in 0.001_f32..=21.0_f32) {
        let enc = MozaDirectTorqueEncoder::new(max);
        prop_assert!(TorqueEncoder::positive_is_clockwise(&enc));
    }
}

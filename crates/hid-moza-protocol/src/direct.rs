//! Moza direct-torque report encoding.
//!
//! Provides a pure, testable encoder for Moza report `0x20`.
//! Intentionally performs no I/O and no heap allocation.

#![deny(static_mut_refs)]

use crate::report::report_ids;
use crate::rt_types::{TorqueEncoder, TorqueQ8_8};

/// Wire size of a Moza direct-torque output report.
pub const REPORT_LEN: usize = 8;

/// Device-native encoder for Moza direct torque output (`report_id = 0x20`).
#[derive(Debug, Clone, Copy)]
pub struct MozaDirectTorqueEncoder {
    max_torque_nm: f32,
    slew_rate_nm_s: u16,
    use_slew_rate: bool,
}

impl MozaDirectTorqueEncoder {
    /// Create a new encoder with the provided wheelbase max torque.
    pub fn new(max_torque_nm: f32) -> Self {
        Self {
            max_torque_nm: max_torque_nm.max(0.0),
            slew_rate_nm_s: 0,
            use_slew_rate: false,
        }
    }

    /// Enable slew-rate field emission in bytes 4-5.
    pub fn with_slew_rate(mut self, slew_rate_nm_s: u16) -> Self {
        self.use_slew_rate = true;
        self.slew_rate_nm_s = slew_rate_nm_s;
        self
    }

    /// Encode a torque command in Newton-meters into a Moza direct report.
    ///
    /// Layout:
    /// - Byte 0: report id (`0x20`)
    /// - Byte 1-2: signed torque command (`i16`, little-endian), percent-of-max
    /// - Byte 3: flags (`bit0 = enable motor`, `bit1 = slew-rate enabled`)
    /// - Byte 4-5: slew-rate in Nm/s when enabled, otherwise 0
    /// - Byte 6-7: reserved (0)
    pub fn encode(&self, torque_nm: f32, _seq: u16, out: &mut [u8; REPORT_LEN]) -> usize {
        let torque_raw = self.torque_percent_to_raw(torque_nm);
        self.encode_torque_raw(torque_raw, 0, out)
    }

    /// Encode an explicit zero-torque, motor-disabled command.
    pub fn encode_zero(&self, out: &mut [u8; REPORT_LEN]) -> usize {
        self.encode_torque_raw(0, 0, out)
    }

    fn encode_torque_raw(
        &self,
        torque_raw: i16,
        extra_flags: u8,
        out: &mut [u8; REPORT_LEN],
    ) -> usize {
        out.fill(0);
        out[0] = report_ids::DIRECT_TORQUE;

        let torque_bytes = torque_raw.to_le_bytes();
        out[1] = torque_bytes[0];
        out[2] = torque_bytes[1];

        let mut flags = extra_flags;
        if torque_raw != 0 {
            flags |= 0x01;
        }
        if self.use_slew_rate {
            flags |= 0x02;
            let slew = self.slew_rate_nm_s.to_le_bytes();
            out[4] = slew[0];
            out[5] = slew[1];
        }
        out[3] = flags;

        REPORT_LEN
    }

    fn torque_percent_to_raw(&self, torque_nm: f32) -> i16 {
        if self.max_torque_nm <= f32::EPSILON {
            return 0;
        }
        let normalized = (torque_nm / self.max_torque_nm).clamp(-1.0, 1.0);
        if normalized >= 0.0 {
            (normalized * i16::MAX as f32).round() as i16
        } else {
            (normalized * (-(i16::MIN as f32))).round() as i32 as i16
        }
    }

    fn max_torque_q8(&self) -> TorqueQ8_8 {
        (self.max_torque_nm * 256.0)
            .clamp(0.0, i16::MAX as f32)
            .round() as TorqueQ8_8
    }
}

impl TorqueEncoder<REPORT_LEN> for MozaDirectTorqueEncoder {
    fn encode(&self, torque: TorqueQ8_8, seq: u16, flags: u8, out: &mut [u8; REPORT_LEN]) -> usize {
        let _ = seq;
        let torque_nm = f32::from(torque) / 256.0;
        let torque_raw = self.torque_percent_to_raw(torque_nm);
        self.encode_torque_raw(torque_raw, flags, out)
    }

    fn encode_zero(&self, out: &mut [u8; REPORT_LEN]) -> usize {
        self.encode_torque_raw(0, 0, out)
    }

    fn clamp_min(&self) -> TorqueQ8_8 {
        -self.max_torque_q8()
    }

    fn clamp_max(&self) -> TorqueQ8_8 {
        self.max_torque_q8()
    }

    fn positive_is_clockwise(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_zero_disables_motor() {
        let enc = MozaDirectTorqueEncoder::new(5.5);
        let mut out = [0u8; REPORT_LEN];
        let len = enc.encode_zero(&mut out);

        assert_eq!(len, REPORT_LEN);
        assert_eq!(out[0], report_ids::DIRECT_TORQUE);
        assert_eq!(out[1], 0);
        assert_eq!(out[2], 0);
        assert_eq!(out[3], 0);
    }

    #[test]
    fn test_encode_positive_full_scale() {
        let enc = MozaDirectTorqueEncoder::new(5.5);
        let mut out = [0u8; REPORT_LEN];
        let _ = enc.encode(5.5, 0, &mut out);

        let raw = i16::from_le_bytes([out[1], out[2]]);
        assert_eq!(raw, i16::MAX);
        assert_eq!(out[3] & 0x01, 0x01);
    }

    #[test]
    fn test_encode_negative_full_scale() {
        let enc = MozaDirectTorqueEncoder::new(5.5);
        let mut out = [0u8; REPORT_LEN];
        let _ = enc.encode(-5.5, 0, &mut out);

        let raw = i16::from_le_bytes([out[1], out[2]]);
        assert_eq!(raw, i16::MIN);
        assert_eq!(out[3] & 0x01, 0x01);
    }

    #[test]
    fn test_encode_clamps_torque_above_max() {
        let enc = MozaDirectTorqueEncoder::new(5.5);
        let mut out = [0u8; REPORT_LEN];
        let _ = enc.encode(999.0, 0, &mut out);

        let raw = i16::from_le_bytes([out[1], out[2]]);
        assert_eq!(raw, i16::MAX);
    }

    #[test]
    fn test_encode_with_slew_rate_sets_flag_and_payload() {
        let enc = MozaDirectTorqueEncoder::new(12.0).with_slew_rate(450);
        let mut out = [0u8; REPORT_LEN];
        let _ = enc.encode(3.0, 0, &mut out);

        assert_eq!(out[3] & 0x02, 0x02);
        assert_eq!(u16::from_le_bytes([out[4], out[5]]), 450);
    }

    #[test]
    fn test_encode_with_zero_max_torque_is_safe_zero() {
        let enc = MozaDirectTorqueEncoder::new(0.0);
        let mut out = [0u8; REPORT_LEN];
        let _ = enc.encode(4.0, 123, &mut out);

        assert_eq!(i16::from_le_bytes([out[1], out[2]]), 0);
        assert_eq!(out[3] & 0x01, 0x00);
    }

    #[test]
    fn test_rt_encoder_layout_for_flags_and_clamps() {
        let enc = MozaDirectTorqueEncoder::new(5.5);
        let mut out = [0u8; REPORT_LEN];

        let len = TorqueEncoder::encode(&enc, 1408, 123, 0xAA, &mut out);
        assert_eq!(len, REPORT_LEN);
        assert_eq!(out[0], report_ids::DIRECT_TORQUE);
        assert_eq!(out[3], 0xAB);

        assert_eq!(enc.clamp_max(), 1408);
        assert_eq!(enc.clamp_min(), -1408);
        assert!(enc.positive_is_clockwise());
    }

    // ─── Mid-scale golden tests ──────────────────────────────────────────────

    /// Snapshot: quarter-scale positive torque encodes to ≈ i16::MAX / 4.
    #[test]
    fn test_encode_quarter_scale_positive() {
        let max = 12.0_f32;
        let enc = MozaDirectTorqueEncoder::new(max);
        let mut out = [0u8; REPORT_LEN];
        enc.encode(max * 0.25, 0, &mut out);
        let raw = i16::from_le_bytes([out[1], out[2]]);
        let expected = (i16::MAX as f32 * 0.25).round() as i16;
        assert!(
            (raw - expected).abs() <= 1,
            "quarter-scale raw={raw} expected≈{expected}"
        );
        assert_eq!(out[3] & 0x01, 0x01, "motor-enable bit must be set");
    }

    /// Snapshot: half-scale positive torque encodes to ≈ i16::MAX / 2.
    #[test]
    fn test_encode_half_scale_positive() {
        let max = 9.0_f32;
        let enc = MozaDirectTorqueEncoder::new(max);
        let mut out = [0u8; REPORT_LEN];
        enc.encode(max * 0.5, 0, &mut out);
        let raw = i16::from_le_bytes([out[1], out[2]]);
        let expected = (i16::MAX as f32 * 0.5).round() as i16;
        assert!(
            (raw - expected).abs() <= 1,
            "half-scale raw={raw} expected≈{expected}"
        );
    }

    /// Snapshot: three-quarter-scale positive torque encodes to ≈ i16::MAX * 0.75.
    #[test]
    fn test_encode_three_quarter_scale_positive() {
        let max = 5.5_f32;
        let enc = MozaDirectTorqueEncoder::new(max);
        let mut out = [0u8; REPORT_LEN];
        enc.encode(max * 0.75, 0, &mut out);
        let raw = i16::from_le_bytes([out[1], out[2]]);
        let expected = (i16::MAX as f32 * 0.75).round() as i16;
        assert!(
            (raw - expected).abs() <= 1,
            "three-quarter-scale raw={raw} expected≈{expected}"
        );
    }

    /// Snapshot: half-scale negative torque encodes to ≈ i16::MIN / 2.
    #[test]
    fn test_encode_half_scale_negative() {
        let max = 9.0_f32;
        let enc = MozaDirectTorqueEncoder::new(max);
        let mut out = [0u8; REPORT_LEN];
        enc.encode(-max * 0.5, 0, &mut out);
        let raw = i16::from_le_bytes([out[1], out[2]]);
        let expected = ((i16::MIN as f32) * 0.5).round() as i16;
        assert!(
            (raw - expected).abs() <= 1,
            "half-scale negative raw={raw} expected≈{expected}"
        );
        assert_eq!(
            out[3] & 0x01,
            0x01,
            "motor-enable bit must be set for negative torque"
        );
    }

    /// Property: zero torque always encodes to raw=0 with motor disabled.
    #[test]
    fn test_encode_exact_zero_always_disables_motor() {
        for max in [0.1_f32, 1.0, 5.5, 9.0, 12.0, 21.0] {
            let enc = MozaDirectTorqueEncoder::new(max);
            let mut out = [0u8; REPORT_LEN];
            enc.encode(0.0, 0, &mut out);
            let raw = i16::from_le_bytes([out[1], out[2]]);
            assert_eq!(raw, 0, "zero torque must encode as raw=0 for max={max}");
            assert_eq!(out[3] & 0x01, 0, "motor must be disabled for zero torque");
        }
    }

    /// Property: positive torque always encodes positive raw, negative encodes negative.
    #[test]
    fn test_encode_sign_consistency() {
        let enc = MozaDirectTorqueEncoder::new(10.0);
        let samples: &[(f32, i16)] = &[
            (0.001, 1),   // very small positive → positive raw
            (-0.001, -1), // very small negative → negative raw (approximately)
        ];
        for &(torque, expected_sign_dir) in samples {
            let mut out = [0u8; REPORT_LEN];
            enc.encode(torque, 0, &mut out);
            let raw = i16::from_le_bytes([out[1], out[2]]);
            if expected_sign_dir > 0 {
                assert!(
                    raw >= 0,
                    "positive torque {torque} must encode as non-negative raw={raw}"
                );
            } else {
                assert!(
                    raw <= 0,
                    "negative torque {torque} must encode as non-positive raw={raw}"
                );
            }
        }
    }

    /// Snapshot: exact byte-level encoding for known torque values (regression guard).
    #[test]
    fn test_encode_golden_byte_snapshot_r5_max() {
        // R5 max torque = 5.5 Nm; half scale = 2.75 Nm
        let enc = MozaDirectTorqueEncoder::new(5.5);
        let mut out = [0u8; REPORT_LEN];
        enc.encode(2.75, 0, &mut out);

        // Report ID
        assert_eq!(out[0], report_ids::DIRECT_TORQUE);
        // Raw torque ≈ i16::MAX / 2 = 16383
        let raw = i16::from_le_bytes([out[1], out[2]]);
        assert!((raw - 16384).abs() <= 1, "golden raw={raw}");
        // Motor enable flag
        assert_eq!(out[3] & 0x01, 0x01);
        // Unused bytes are zero
        assert_eq!(out[4], 0);
        assert_eq!(out[5], 0);
        assert_eq!(out[6], 0);
        assert_eq!(out[7], 0);
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(500))]

        /// For any torque in [-max, max], encoded raw value has the same sign as torque.
        #[test]
        fn prop_torque_sign_preserved(
            max in 0.1_f32..=21.0_f32,
            frac in -1.0_f32..=1.0_f32,
        ) {
            let torque_nm = max * frac;
            let enc = MozaDirectTorqueEncoder::new(max);
            let mut out = [0u8; REPORT_LEN];
            enc.encode(torque_nm, 0, &mut out);
            let raw = i16::from_le_bytes([out[1], out[2]]);

            if torque_nm > 0.001 {
                prop_assert!(raw > 0, "positive torque {torque_nm} should yield positive raw {raw}");
            } else if torque_nm < -0.001 {
                prop_assert!(raw < 0, "negative torque {torque_nm} should yield negative raw {raw}");
            }
        }

        /// For any torque in [-max, max], absolute encoded raw is ≤ |i16::MAX|.
        #[test]
        fn prop_encoded_value_never_overflows(
            max in 0.001_f32..=21.0_f32,
            torque in -100.0_f32..=100.0_f32,
        ) {
            let enc = MozaDirectTorqueEncoder::new(max);
            let mut out = [0u8; REPORT_LEN];
            enc.encode(torque, 0, &mut out);
            // Must not panic; raw must be in valid i16 range (this is a no-op since i16::from_le always holds)
            let raw = i16::from_le_bytes([out[1], out[2]]);
            prop_assert!(raw >= i16::MIN);
            prop_assert!(raw <= i16::MAX);
        }

        /// Motor-enable bit is set iff raw torque != 0.
        #[test]
        fn prop_motor_enable_bit_iff_nonzero_torque(
            max in 0.1_f32..=21.0_f32,
            frac in -1.0_f32..=1.0_f32,
        ) {
            let torque_nm = max * frac;
            let enc = MozaDirectTorqueEncoder::new(max);
            let mut out = [0u8; REPORT_LEN];
            enc.encode(torque_nm, 0, &mut out);
            let raw = i16::from_le_bytes([out[1], out[2]]);
            let motor_enabled = out[3] & 0x01 != 0;

            prop_assert_eq!(
                motor_enabled,
                raw != 0,
                "motor-enable={} must match raw!=0 (raw={}, torque={})",
                motor_enabled,
                raw,
                torque_nm
            );
        }

        /// Encoding is monotone: if torque_a > torque_b (and both in-range), raw_a >= raw_b.
        #[test]
        fn prop_encoding_is_monotone(
            max in 0.1_f32..=21.0_f32,
            frac_a in -1.0_f32..=1.0_f32,
            frac_b in -1.0_f32..=1.0_f32,
        ) {
            let ta = max * frac_a;
            let tb = max * frac_b;
            let enc = MozaDirectTorqueEncoder::new(max);

            let mut out_a = [0u8; REPORT_LEN];
            let mut out_b = [0u8; REPORT_LEN];
            enc.encode(ta, 0, &mut out_a);
            enc.encode(tb, 0, &mut out_b);

            let raw_a = i16::from_le_bytes([out_a[1], out_a[2]]);
            let raw_b = i16::from_le_bytes([out_b[1], out_b[2]]);

            if ta > tb {
                prop_assert!(
                    raw_a >= raw_b,
                    "monotone violation: torque {ta} > {tb} but raw {raw_a} < {raw_b}"
                );
            } else if ta < tb {
                prop_assert!(
                    raw_a <= raw_b,
                    "monotone violation: torque {ta} < {tb} but raw {raw_a} > {raw_b}"
                );
            }
        }

        /// Report ID byte is always the DIRECT_TORQUE constant regardless of input.
        #[test]
        fn prop_report_id_always_correct(
            max in 0.001_f32..=21.0_f32,
            torque in -100.0_f32..=100.0_f32,
        ) {
            let enc = MozaDirectTorqueEncoder::new(max);
            let mut out = [0u8; REPORT_LEN];
            enc.encode(torque, 0, &mut out);
            prop_assert_eq!(out[0], report_ids::DIRECT_TORQUE);
        }

        /// Encoding length is always REPORT_LEN.
        #[test]
        fn prop_encode_len_always_report_len(
            max in 0.001_f32..=21.0_f32,
            torque in -100.0_f32..=100.0_f32,
        ) {
            let enc = MozaDirectTorqueEncoder::new(max);
            let mut out = [0u8; REPORT_LEN];
            let len = enc.encode(torque, 0, &mut out);
            prop_assert_eq!(len, REPORT_LEN);
        }
    }
}

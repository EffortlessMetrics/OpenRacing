//! Moza direct-torque report encoding.
//!
//! This module provides a pure, testable encoder for Moza report `0x20`.
//! It intentionally performs no I/O and no heap allocation.

#![deny(static_mut_refs)]

use super::moza::report_ids;
use crate::hid::rt_stream::{TorqueEncoder, TorqueQ8_8};

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
}

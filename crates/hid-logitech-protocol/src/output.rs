//! Logitech HID output report encoding.
//!
//! All functions are pure and allocation-free.

#![deny(static_mut_refs)]

use crate::ids::{commands, report_ids};

/// Wire size of a Logitech constant-force output report.
pub const CONSTANT_FORCE_REPORT_LEN: usize = 4;

/// Wire size of a Logitech vendor feature/output report (0xF8 commands).
pub const VENDOR_REPORT_LEN: usize = 7;

/// Encoder for Logitech constant-force FFB output reports (report ID 0x12).
///
/// Converts a torque value in Newton-meters to the signed 16-bit Logitech wire
/// format (range ±10000, where 10000 = max torque).
#[derive(Debug, Clone, Copy)]
pub struct LogitechConstantForceEncoder {
    max_torque_nm: f32,
}

impl LogitechConstantForceEncoder {
    /// Create a new encoder for a wheel with the given peak torque.
    pub fn new(max_torque_nm: f32) -> Self {
        Self {
            max_torque_nm: max_torque_nm.max(0.01),
        }
    }

    /// Encode a torque command (Newton-meters) into a constant-force output report.
    ///
    /// Layout (4 bytes):
    /// - Byte 0: `0x12` (report ID)
    /// - Byte 1: effect block index (`1` = slot 1, 1-based)
    /// - Bytes 2–3: signed magnitude, little-endian (range ±10000)
    pub fn encode(&self, torque_nm: f32, out: &mut [u8; CONSTANT_FORCE_REPORT_LEN]) -> usize {
        out.fill(0);
        out[0] = report_ids::CONSTANT_FORCE;
        out[1] = 1; // effect block index (1-based)
        let mag = torque_to_magnitude(torque_nm, self.max_torque_nm);
        let bytes = mag.to_le_bytes();
        out[2] = bytes[0];
        out[3] = bytes[1];
        CONSTANT_FORCE_REPORT_LEN
    }

    /// Encode an explicit zero-force report.
    pub fn encode_zero(&self, out: &mut [u8; CONSTANT_FORCE_REPORT_LEN]) -> usize {
        out.fill(0);
        out[0] = report_ids::CONSTANT_FORCE;
        out[1] = 1;
        CONSTANT_FORCE_REPORT_LEN
    }
}

/// Convert torque (Nm) to Logitech magnitude units (±10000).
fn torque_to_magnitude(torque_nm: f32, max_torque_nm: f32) -> i16 {
    let normalized = (torque_nm / max_torque_nm).clamp(-1.0, 1.0);
    (normalized * 10_000.0) as i16
}

/// Build the 7-byte native mode feature report (0xF8, cmd 0x0A).
///
/// Send as a HID feature report to switch the wheel from compatibility mode
/// (200°) to native mode (full rotation + FFB).
///
/// After sending, wait at least 100 ms before issuing further commands.
pub fn build_native_mode_report() -> [u8; VENDOR_REPORT_LEN] {
    [
        report_ids::VENDOR,
        commands::NATIVE_MODE,
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
    ]
}

/// Build the 7-byte set-range feature report (0xF8, cmd 0x81).
///
/// `degrees` is the desired full rotation range (e.g. 900 for G920/G923,
/// 1080 for Pro Racing Wheel).
pub fn build_set_range_report(degrees: u16) -> [u8; VENDOR_REPORT_LEN] {
    let [lsb, msb] = degrees.to_le_bytes();
    [
        report_ids::VENDOR,
        commands::SET_RANGE,
        lsb,
        msb,
        0x00,
        0x00,
        0x00,
    ]
}

/// Build the 7-byte set-autocenter feature report (0xF8, cmd 0x14).
///
/// `strength` is the centering force (0x00–0xFF).
/// `rate` is the centering speed (0x00–0xFF).
pub fn build_set_autocenter_report(strength: u8, rate: u8) -> [u8; VENDOR_REPORT_LEN] {
    [
        report_ids::VENDOR,
        commands::SET_AUTOCENTER,
        strength,
        rate,
        0x00,
        0x00,
        0x00,
    ]
}

/// Build the 7-byte rev-light LED output report (0xF8, cmd 0x12).
///
/// `led_mask` is a 5-bit bitmask: bit 0 = LED 1 (leftmost), bit 4 = LED 5 (rightmost).
pub fn build_set_leds_report(led_mask: u8) -> [u8; VENDOR_REPORT_LEN] {
    [
        report_ids::VENDOR,
        commands::SET_LEDS,
        led_mask & 0x1F,
        0x00,
        0x00,
        0x00,
        0x00,
    ]
}

/// Build the 2-byte device gain output report (report ID 0x16).
///
/// `gain` is the overall FFB gain (0x00–0xFF, 0 = 0%, 0xFF = 100%).
pub fn build_gain_report(gain: u8) -> [u8; 2] {
    [report_ids::DEVICE_GAIN, gain]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_native_mode_report() -> Result<(), Box<dyn std::error::Error>> {
        let r = build_native_mode_report();
        assert_eq!(r[0], 0xF8, "report ID must be 0xF8");
        assert_eq!(r[1], 0x0A, "command must be NATIVE_MODE (0x0A)");
        assert_eq!(&r[2..], &[0u8; 5], "remaining bytes must be zero");
        Ok(())
    }

    #[test]
    fn test_set_range_900_degrees() -> Result<(), Box<dyn std::error::Error>> {
        let r = build_set_range_report(900);
        assert_eq!(r[0], 0xF8);
        assert_eq!(r[1], 0x81, "command must be SET_RANGE (0x81)");
        // 900 dec = 0x0384; little-endian = [0x84, 0x03]
        assert_eq!(r[2], 0x84, "LSB of 900 = 0x84");
        assert_eq!(r[3], 0x03, "MSB of 900 = 0x03");
        assert_eq!(&r[4..], &[0u8; 3]);
        Ok(())
    }

    #[test]
    fn test_set_range_200_degrees() -> Result<(), Box<dyn std::error::Error>> {
        let r = build_set_range_report(200);
        // 200 dec = 0x00C8; little-endian = [0xC8, 0x00]
        assert_eq!(r[2], 0xC8);
        assert_eq!(r[3], 0x00);
        Ok(())
    }

    #[test]
    fn test_set_range_1080_degrees() -> Result<(), Box<dyn std::error::Error>> {
        let r = build_set_range_report(1080);
        // 1080 dec = 0x0438; little-endian = [0x38, 0x04]
        assert_eq!(r[2], 0x38, "LSB of 1080 = 0x38");
        assert_eq!(r[3], 0x04, "MSB of 1080 = 0x04");
        Ok(())
    }

    #[test]
    fn test_set_autocenter_report() -> Result<(), Box<dyn std::error::Error>> {
        let r = build_set_autocenter_report(0x40, 0x80);
        assert_eq!(r[0], 0xF8);
        assert_eq!(r[1], 0x14, "command must be SET_AUTOCENTER (0x14)");
        assert_eq!(r[2], 0x40, "strength byte");
        assert_eq!(r[3], 0x80, "rate byte");
        assert_eq!(&r[4..], &[0u8; 3]);
        Ok(())
    }

    #[test]
    fn test_set_leds_report_all_on() -> Result<(), Box<dyn std::error::Error>> {
        let r = build_set_leds_report(0b00011111);
        assert_eq!(r[0], 0xF8);
        assert_eq!(r[1], 0x12, "command must be SET_LEDS (0x12)");
        assert_eq!(r[2], 0x1F, "all 5 LEDs on");
        assert_eq!(&r[3..], &[0u8; 4]);
        Ok(())
    }

    #[test]
    fn test_set_leds_masks_high_bits() -> Result<(), Box<dyn std::error::Error>> {
        let r = build_set_leds_report(0xFF);
        assert_eq!(r[2], 0x1F, "upper bits must be masked to 5-bit range");
        Ok(())
    }

    #[test]
    fn test_gain_report() -> Result<(), Box<dyn std::error::Error>> {
        let r_full = build_gain_report(0xFF);
        assert_eq!(r_full[0], 0x16, "Device Gain report ID");
        assert_eq!(r_full[1], 0xFF, "full gain");
        let r_zero = build_gain_report(0);
        assert_eq!(r_zero[0], 0x16);
        assert_eq!(r_zero[1], 0, "zero gain");
        Ok(())
    }

    #[test]
    fn test_constant_force_encoder_positive() -> Result<(), Box<dyn std::error::Error>> {
        let enc = LogitechConstantForceEncoder::new(2.2);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode(1.1, &mut out);
        assert_eq!(out[0], 0x12, "report ID");
        assert_eq!(out[1], 1, "effect block index");
        // 1.1 / 2.2 = 0.5 normalized → 5000 magnitude
        let mag = i16::from_le_bytes([out[2], out[3]]);
        assert_eq!(mag, 5000);
        Ok(())
    }

    #[test]
    fn test_constant_force_encoder_full_negative() -> Result<(), Box<dyn std::error::Error>> {
        let enc = LogitechConstantForceEncoder::new(2.2);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode(-2.2, &mut out);
        let mag = i16::from_le_bytes([out[2], out[3]]);
        assert_eq!(mag, -10000);
        Ok(())
    }

    #[test]
    fn test_constant_force_encoder_zero() -> Result<(), Box<dyn std::error::Error>> {
        let enc = LogitechConstantForceEncoder::new(2.2);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode_zero(&mut out);
        let mag = i16::from_le_bytes([out[2], out[3]]);
        assert_eq!(mag, 0);
        Ok(())
    }

    #[test]
    fn test_constant_force_saturation() -> Result<(), Box<dyn std::error::Error>> {
        let enc = LogitechConstantForceEncoder::new(2.2);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode(100.0, &mut out);
        let mag = i16::from_le_bytes([out[2], out[3]]);
        assert_eq!(mag, 10000, "over-torque must saturate at +10000");
        enc.encode(-100.0, &mut out);
        let mag = i16::from_le_bytes([out[2], out[3]]);
        assert_eq!(mag, -10000, "over-torque must saturate at -10000");
        Ok(())
    }
}

//! Fanatec HID output report encoding.
//!
//! All functions are pure and allocation-free.

#![deny(static_mut_refs)]

use crate::ids::{ffb_commands, led_commands, report_ids};

/// Wire size of a Fanatec constant-force output report.
pub const CONSTANT_FORCE_REPORT_LEN: usize = 8;

/// Encoder for Fanatec constant-force FFB output reports (report ID 0x01, command 0x01).
///
/// Converts a torque value in Newton-meters to the signed 16-bit Fanatec wire format.
#[derive(Debug, Clone, Copy)]
pub struct FanatecConstantForceEncoder {
    max_torque_nm: f32,
}

impl FanatecConstantForceEncoder {
    /// Create a new encoder for a wheelbase with the given peak torque.
    pub fn new(max_torque_nm: f32) -> Self {
        Self {
            max_torque_nm: max_torque_nm.max(0.0),
        }
    }

    /// Encode a torque command (Newton-meters) into a Fanatec constant-force output report.
    ///
    /// Layout (8 bytes):
    /// - Byte 0: `0x01` (report ID)
    /// - Byte 1: `0x01` (constant force command)
    /// - Bytes 2–3: signed force, little-endian (±32767 ↔ ±max_torque_nm)
    /// - Bytes 4–7: reserved (zero)
    pub fn encode(
        &self,
        torque_nm: f32,
        _seq: u16,
        out: &mut [u8; CONSTANT_FORCE_REPORT_LEN],
    ) -> usize {
        out.fill(0);
        out[0] = report_ids::FFB_OUTPUT;
        out[1] = ffb_commands::CONSTANT_FORCE;
        let force_raw = torque_to_raw(torque_nm, self.max_torque_nm);
        let bytes = force_raw.to_le_bytes();
        out[2] = bytes[0];
        out[3] = bytes[1];
        CONSTANT_FORCE_REPORT_LEN
    }

    /// Encode an explicit zero-force report.
    pub fn encode_zero(&self, out: &mut [u8; CONSTANT_FORCE_REPORT_LEN]) -> usize {
        out.fill(0);
        out[0] = report_ids::FFB_OUTPUT;
        out[1] = ffb_commands::CONSTANT_FORCE;
        CONSTANT_FORCE_REPORT_LEN
    }
}

/// Build the 8-byte mode-switch feature report payload (compatibility → advanced/PC mode).
///
/// Write this as a HID feature report; the first byte is the report ID.
/// Full payload: `[0x01, 0x01, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00]`
pub fn build_mode_switch_report() -> [u8; 8] {
    [
        report_ids::MODE_SWITCH,
        0x01, // Command: Set Mode
        0x03, // Mode: Advanced/PC
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
    ]
}

/// Build the 8-byte "stop all effects" output report.
pub fn build_stop_all_report() -> [u8; 8] {
    [
        report_ids::FFB_OUTPUT,
        ffb_commands::STOP_ALL,
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
    ]
}

/// Build the 8-byte "set device gain" output report.
///
/// `gain_percent` is clamped to the range 0–100.
pub fn build_set_gain_report(gain_percent: u8) -> [u8; 8] {
    [
        report_ids::FFB_OUTPUT,
        ffb_commands::SET_GAIN,
        gain_percent.min(100),
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
    ]
}

/// Wire size of all LED/display/rumble output reports.
pub const LED_REPORT_LEN: usize = 8;

/// Build a rev-light LED output report for the attached steering-wheel rim.
///
/// Layout (8 bytes, report ID 0x08):
/// - Byte 0: `0x08` (report ID)
/// - Byte 1: `0x80` (REV_LIGHTS command)
/// - Bytes 2–3: LED bitmask, little-endian (bit N = LED N lit)
/// - Byte 4: brightness (0 = off, 255 = maximum)
/// - Bytes 5–7: reserved (zero)
pub fn build_led_report(bitmask: u16, brightness: u8) -> [u8; LED_REPORT_LEN] {
    [
        report_ids::LED_DISPLAY,
        led_commands::REV_LIGHTS,
        (bitmask & 0xFF) as u8,
        ((bitmask >> 8) & 0xFF) as u8,
        brightness,
        0,
        0,
        0,
    ]
}

/// Build a numeric display output report for the attached steering-wheel rim.
///
/// Layout (8 bytes, report ID 0x08):
/// - Byte 0: `0x08` (report ID)
/// - Byte 1: `0x81` (DISPLAY command)
/// - Byte 2: display mode (0 = override, 1 = auto from wheel)
/// - Bytes 3–5: three display digits (ASCII / 7-segment nibbles)
/// - Byte 6: brightness (0 = off, 255 = maximum)
/// - Byte 7: reserved (zero)
pub fn build_display_report(mode: u8, digits: [u8; 3], brightness: u8) -> [u8; LED_REPORT_LEN] {
    [
        report_ids::LED_DISPLAY,
        led_commands::DISPLAY,
        mode,
        digits[0],
        digits[1],
        digits[2],
        brightness,
        0,
    ]
}

/// Build a rumble motor output report for the attached steering-wheel rim.
///
/// Layout (8 bytes, report ID 0x08):
/// - Byte 0: `0x08` (report ID)
/// - Byte 1: `0x82` (RUMBLE command)
/// - Byte 2: left motor intensity (0–255)
/// - Byte 3: right motor intensity (0–255)
/// - Byte 4: duration in 10 ms units (0 = stop, 255 = ~2.5 s)
/// - Bytes 5–7: reserved (zero)
pub fn build_rumble_report(left: u8, right: u8, duration_10ms: u8) -> [u8; LED_REPORT_LEN] {
    [
        report_ids::LED_DISPLAY,
        led_commands::RUMBLE,
        left,
        right,
        duration_10ms,
        0,
        0,
        0,
    ]
}

/// Convert torque (Nm) to a signed 16-bit raw value proportional to max_torque_nm.
fn torque_to_raw(torque_nm: f32, max_torque_nm: f32) -> i16 {
    if max_torque_nm <= f32::EPSILON {
        return 0;
    }
    let normalized = (torque_nm / max_torque_nm).clamp(-1.0, 1.0);
    if normalized >= 0.0 {
        (normalized * i16::MAX as f32).round() as i16
    } else {
        (normalized * (-(i16::MIN as f32))).round() as i32 as i16
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constant_force_zero() -> Result<(), Box<dyn std::error::Error>> {
        let encoder = FanatecConstantForceEncoder::new(8.0);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        let len = encoder.encode(0.0, 0, &mut out);
        assert_eq!(len, CONSTANT_FORCE_REPORT_LEN);
        assert_eq!(out[0], 0x01); // report ID
        assert_eq!(out[1], 0x01); // constant force command
        assert_eq!(out[2], 0x00); // force low byte = 0
        assert_eq!(out[3], 0x00); // force high byte = 0
        Ok(())
    }

    #[test]
    fn test_constant_force_full_positive() -> Result<(), Box<dyn std::error::Error>> {
        let encoder = FanatecConstantForceEncoder::new(8.0);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        let len = encoder.encode(8.0, 0, &mut out);
        assert_eq!(len, CONSTANT_FORCE_REPORT_LEN);
        // i16::MAX = 32767 = 0x7FFF → LE [0xFF, 0x7F]
        assert_eq!(out[2], 0xFF);
        assert_eq!(out[3], 0x7F);
        Ok(())
    }

    #[test]
    fn test_constant_force_full_negative() -> Result<(), Box<dyn std::error::Error>> {
        let encoder = FanatecConstantForceEncoder::new(8.0);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        let len = encoder.encode(-8.0, 0, &mut out);
        assert_eq!(len, CONSTANT_FORCE_REPORT_LEN);
        // i16::MIN = -32768 = 0x8000 → LE [0x00, 0x80]
        assert_eq!(out[2], 0x00);
        assert_eq!(out[3], 0x80);
        Ok(())
    }

    #[test]
    fn test_constant_force_half_positive() -> Result<(), Box<dyn std::error::Error>> {
        let encoder = FanatecConstantForceEncoder::new(8.0);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        encoder.encode(4.0, 0, &mut out);
        let raw = i16::from_le_bytes([out[2], out[3]]);
        // ~50% of i16::MAX
        assert!(raw > 16_000 && raw < 16_500, "expected ~16384, got {}", raw);
        Ok(())
    }

    #[test]
    fn test_constant_force_zero_max_torque_returns_zero() -> Result<(), Box<dyn std::error::Error>> {
        let encoder = FanatecConstantForceEncoder::new(0.0);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        encoder.encode(5.0, 0, &mut out);
        assert_eq!(out[2], 0x00);
        assert_eq!(out[3], 0x00);
        Ok(())
    }

    #[test]
    fn test_encode_zero_clears_force() -> Result<(), Box<dyn std::error::Error>> {
        let encoder = FanatecConstantForceEncoder::new(8.0);
        let mut out = [0xFFu8; CONSTANT_FORCE_REPORT_LEN];
        encoder.encode_zero(&mut out);
        assert_eq!(out[2], 0x00);
        assert_eq!(out[3], 0x00);
        Ok(())
    }

    #[test]
    fn test_stop_all_report() -> Result<(), Box<dyn std::error::Error>> {
        let report = build_stop_all_report();
        assert_eq!(report[0], 0x01);
        assert_eq!(report[1], 0x0F);
        Ok(())
    }

    #[test]
    fn test_set_gain_report_clamped() -> Result<(), Box<dyn std::error::Error>> {
        let report = build_set_gain_report(200); // over 100 → clamped
        assert_eq!(report[1], 0x10); // SET_GAIN command
        assert_eq!(report[2], 100);
        Ok(())
    }

    #[test]
    fn test_led_report_bitmask() -> Result<(), Box<dyn std::error::Error>> {
        let report = build_led_report(0b1010_1010_0101_0101, 200);
        assert_eq!(report[0], 0x08, "byte 0 must be LED_DISPLAY report ID");
        assert_eq!(report[1], 0x80, "byte 1 must be REV_LIGHTS command");
        assert_eq!(report[2], 0x55, "byte 2 must be bitmask low byte");
        assert_eq!(report[3], 0xAA, "byte 3 must be bitmask high byte");
        assert_eq!(report[4], 200, "byte 4 must be brightness");
        assert_eq!(&report[5..], &[0u8; 3]);
        Ok(())
    }

    #[test]
    fn test_led_report_all_off() -> Result<(), Box<dyn std::error::Error>> {
        let report = build_led_report(0, 0);
        assert_eq!(report[0], 0x08);
        assert_eq!(report[1], 0x80);
        assert_eq!(report[2], 0x00);
        assert_eq!(report[3], 0x00);
        assert_eq!(report[4], 0x00);
        Ok(())
    }

    #[test]
    fn test_display_report_structure() -> Result<(), Box<dyn std::error::Error>> {
        let report = build_display_report(0x00, [b'1', b'2', b'3'], 128);
        assert_eq!(report[0], 0x08, "byte 0 must be LED_DISPLAY report ID");
        assert_eq!(report[1], 0x81, "byte 1 must be DISPLAY command");
        assert_eq!(report[2], 0x00, "byte 2 must be mode");
        assert_eq!(report[3], b'1', "byte 3 must be digit 0");
        assert_eq!(report[4], b'2', "byte 4 must be digit 1");
        assert_eq!(report[5], b'3', "byte 5 must be digit 2");
        assert_eq!(report[6], 128, "byte 6 must be brightness");
        assert_eq!(report[7], 0, "byte 7 must be reserved");
        Ok(())
    }

    #[test]
    fn test_rumble_report_structure() -> Result<(), Box<dyn std::error::Error>> {
        let report = build_rumble_report(180, 90, 50);
        assert_eq!(report[0], 0x08, "byte 0 must be LED_DISPLAY report ID");
        assert_eq!(report[1], 0x82, "byte 1 must be RUMBLE command");
        assert_eq!(report[2], 180, "byte 2 must be left motor intensity");
        assert_eq!(report[3], 90, "byte 3 must be right motor intensity");
        assert_eq!(report[4], 50, "byte 4 must be duration");
        assert_eq!(&report[5..], &[0u8; 3]);
        Ok(())
    }

    #[test]
    fn test_rumble_report_stop() -> Result<(), Box<dyn std::error::Error>> {
        let report = build_rumble_report(0, 0, 0);
        assert_eq!(report[2], 0);
        assert_eq!(report[3], 0);
        assert_eq!(report[4], 0);
        Ok(())
    }

    #[test]
    fn test_mode_switch_report() -> Result<(), Box<dyn std::error::Error>> {
        let report = build_mode_switch_report();
        assert_eq!(report[0], 0x01); // report ID
        assert_eq!(report[1], 0x01); // Set Mode command
        assert_eq!(report[2], 0x03); // Advanced/PC mode
        Ok(())
    }
}

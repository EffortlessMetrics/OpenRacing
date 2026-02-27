//! OpenFFBoard HID output report encoding.
//!
//! OpenFFBoard uses standard USB HID PID with a constant force report
//! for real-time torque output. The command interface uses vendor-defined
//! feature reports on the same HID interface.

/// HID report ID for the constant force effect output report.
///
/// Sends a signed torque value to the motor controller.
pub const CONSTANT_FORCE_REPORT_ID: u8 = 0x01;

/// Length of the constant force output report in bytes (including report ID).
pub const CONSTANT_FORCE_REPORT_LEN: usize = 5;

/// HID feature report ID for FFB enable/disable.
pub const ENABLE_FFB_REPORT_ID: u8 = 0x60;

/// HID feature report ID for global gain control.
pub const GAIN_REPORT_ID: u8 = 0x61;

/// Scale factor: maps ±`MAX_TORQUE_SCALE` to ±1.0 normalized torque.
///
/// OpenFFBoard uses a signed 16-bit torque in the range [-10000, 10000].
pub const MAX_TORQUE_SCALE: i16 = 10_000;

/// Encodes torque commands for OpenFFBoard devices.
///
/// Produces 5-byte constant force HID output reports.
#[derive(Debug, Clone, Copy, Default)]
pub struct OpenFFBoardTorqueEncoder;

impl OpenFFBoardTorqueEncoder {
    /// Encode a normalised torque value in `[-1.0, 1.0]` into an output report.
    ///
    /// Values outside `[-1.0, 1.0]` are clamped before encoding.
    ///
    /// # Report layout
    /// ```text
    /// Byte 0: CONSTANT_FORCE_REPORT_ID (0x01)
    /// Bytes 1–2: torque i16 LE in [-10000, 10000]
    /// Bytes 3–4: reserved (0x00)
    /// ```
    pub fn encode(&self, torque_normalized: f32) -> [u8; CONSTANT_FORCE_REPORT_LEN] {
        let clamped = torque_normalized.clamp(-1.0, 1.0);
        let raw = (clamped * MAX_TORQUE_SCALE as f32) as i16;
        let [lo, hi] = raw.to_le_bytes();
        [CONSTANT_FORCE_REPORT_ID, lo, hi, 0x00, 0x00]
    }
}

/// Build a feature report that enables or disables FFB output.
///
/// Returns a 3-byte feature report: `[ENABLE_FFB_REPORT_ID, enabled, 0]`.
pub fn build_enable_ffb(enabled: bool) -> [u8; 3] {
    [ENABLE_FFB_REPORT_ID, if enabled { 0x01 } else { 0x00 }, 0x00]
}

/// Build a feature report that sets the global FFB gain.
///
/// `gain` is in `[0, 255]` where 255 is full scale.
pub fn build_set_gain(gain: u8) -> [u8; 3] {
    [GAIN_REPORT_ID, gain, 0x00]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoder_zero_torque_produces_zero_bytes() {
        let enc = OpenFFBoardTorqueEncoder;
        let report = enc.encode(0.0);
        assert_eq!(report[0], CONSTANT_FORCE_REPORT_ID);
        assert_eq!(i16::from_le_bytes([report[1], report[2]]), 0);
    }

    #[test]
    fn encoder_full_positive_torque() {
        let enc = OpenFFBoardTorqueEncoder;
        let report = enc.encode(1.0);
        let raw = i16::from_le_bytes([report[1], report[2]]);
        assert_eq!(raw, MAX_TORQUE_SCALE);
    }

    #[test]
    fn encoder_full_negative_torque() {
        let enc = OpenFFBoardTorqueEncoder;
        let report = enc.encode(-1.0);
        let raw = i16::from_le_bytes([report[1], report[2]]);
        assert_eq!(raw, -MAX_TORQUE_SCALE);
    }

    #[test]
    fn encoder_clamps_over_one() {
        let enc = OpenFFBoardTorqueEncoder;
        let over = enc.encode(2.0);
        let normal = enc.encode(1.0);
        assert_eq!(over, normal);
    }

    #[test]
    fn encoder_clamps_under_negative_one() {
        let enc = OpenFFBoardTorqueEncoder;
        let under = enc.encode(-2.0);
        let normal = enc.encode(-1.0);
        assert_eq!(under, normal);
    }

    #[test]
    fn enable_ffb_report_structure() {
        let on = build_enable_ffb(true);
        assert_eq!(on[0], ENABLE_FFB_REPORT_ID);
        assert_eq!(on[1], 0x01);

        let off = build_enable_ffb(false);
        assert_eq!(off[1], 0x00);
    }

    #[test]
    fn gain_report_structure() {
        let report = build_set_gain(255);
        assert_eq!(report[0], GAIN_REPORT_ID);
        assert_eq!(report[1], 255);
    }
}

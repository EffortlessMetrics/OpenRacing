//! Cammus input report parsing (64-byte USB HID report, report ID 0x01).
//!
//! ## Wire-format verification status
//!
//! ⚠ **Unverified.** The input report layout below is an internal estimate. No
//! community USB descriptor dump, open-source driver, or Cammus SDK documents this
//! byte layout. The Linux kernel handles Cammus via standard HID PID descriptors
//! and does not define a vendor-specific input report structure.
//!
//! # Report layout (estimated)
//! | Offset | Size | Field      | Encoding                          |
//! |--------|------|------------|-----------------------------------|
//! | 0–1    | i16  | steering   | LE, ±32767 → ±540°                |
//! | 2–3    | u16  | throttle   | LE, 0–65535                       |
//! | 4–5    | u16  | brake      | LE, 0–65535                       |
//! | 6      | u8   | buttons_lo | bits 0–7                          |
//! | 7      | u8   | buttons_hi | bits 8–15                         |
//! | 8–9    | u16  | clutch     | LE, 0–65535                       |
//! | 10–11  | u16  | handbrake  | LE, 0–65535                       |

/// Full Cammus input report length in bytes.
pub const REPORT_LEN: usize = 64;

/// Report ID for Cammus input reports.
pub const REPORT_ID: u8 = 0x01;

/// Total steering range in degrees (±540°).
pub const STEERING_RANGE_DEG: f32 = 1080.0;

/// Parsed Cammus input report with all axes normalised.
#[derive(Debug, Clone, PartialEq)]
pub struct CammusInputReport {
    /// Steering angle normalised to −1.0 … +1.0 (full range = ±540°).
    pub steering: f32,
    /// Throttle pedal position, 0.0 … 1.0.
    pub throttle: f32,
    /// Brake pedal position, 0.0 … 1.0.
    pub brake: f32,
    /// Clutch pedal position, 0.0 … 1.0.
    pub clutch: f32,
    /// Handbrake axis, 0.0 … 1.0.
    pub handbrake: f32,
    /// Packed button states (16 bits, bytes 6–7 of the raw report).
    pub buttons: u16,
}

/// Errors returned by [`parse`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// The byte slice was too short to contain all required fields.
    TooShort { got: usize, need: usize },
}

impl core::fmt::Display for ParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ParseError::TooShort { got, need } => {
                write!(f, "report too short: got {got} bytes, need {need}")
            }
        }
    }
}

/// Parse a raw Cammus input report byte slice into a [`CammusInputReport`].
///
/// The slice must be at least 12 bytes long; bytes beyond the first 12 are
/// ignored (the device sends 64-byte reports but only the first 12 carry data).
pub fn parse(data: &[u8]) -> Result<CammusInputReport, ParseError> {
    const NEED: usize = 12;
    if data.len() < NEED {
        return Err(ParseError::TooShort {
            got: data.len(),
            need: NEED,
        });
    }

    let raw_steering = i16::from_le_bytes([data[0], data[1]]);
    let raw_throttle = u16::from_le_bytes([data[2], data[3]]);
    let raw_brake = u16::from_le_bytes([data[4], data[5]]);
    let raw_clutch = u16::from_le_bytes([data[8], data[9]]);
    let raw_handbrake = u16::from_le_bytes([data[10], data[11]]);
    let buttons = (data[6] as u16) | ((data[7] as u16) << 8);

    Ok(CammusInputReport {
        steering: (raw_steering as f32 / i16::MAX as f32).clamp(-1.0, 1.0),
        throttle: (raw_throttle as f32 / u16::MAX as f32).clamp(0.0, 1.0),
        brake: (raw_brake as f32 / u16::MAX as f32).clamp(0.0, 1.0),
        clutch: (raw_clutch as f32 / u16::MAX as f32).clamp(0.0, 1.0),
        handbrake: (raw_handbrake as f32 / u16::MAX as f32).clamp(0.0, 1.0),
        buttons,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_too_short() -> Result<(), ParseError> {
        assert!(parse(&[0u8; 5]).is_err());
        let err = parse(&[0u8; 3]).expect_err("expected TooShort error");
        assert_eq!(err, ParseError::TooShort { got: 3, need: 12 });
        Ok(())
    }

    #[test]
    fn parse_center() -> Result<(), ParseError> {
        let data = [0u8; 64];
        let report = parse(&data)?;
        assert!(report.steering.abs() < 0.01);
        assert!(report.throttle.abs() < 0.01);
        assert!(report.brake.abs() < 0.01);
        Ok(())
    }

    #[test]
    fn parse_full_throttle() -> Result<(), ParseError> {
        let mut data = [0u8; 64];
        data[2] = 0xFF;
        data[3] = 0xFF;
        let report = parse(&data)?;
        assert!((report.throttle - 1.0).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn parse_full_brake() -> Result<(), ParseError> {
        let mut data = [0u8; 64];
        data[4] = 0xFF;
        data[5] = 0xFF;
        let report = parse(&data)?;
        assert!((report.brake - 1.0).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn parse_steering_positive() -> Result<(), ParseError> {
        let mut data = [0u8; 64];
        let val = i16::MAX;
        let bytes = val.to_le_bytes();
        data[0] = bytes[0];
        data[1] = bytes[1];
        let report = parse(&data)?;
        assert!((report.steering - 1.0).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn parse_steering_negative() -> Result<(), ParseError> {
        let mut data = [0u8; 64];
        let val: i16 = -i16::MAX;
        let bytes = val.to_le_bytes();
        data[0] = bytes[0];
        data[1] = bytes[1];
        let report = parse(&data)?;
        assert!((report.steering + 1.0).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn parse_buttons() -> Result<(), ParseError> {
        let mut data = [0u8; 64];
        data[6] = 0xAB;
        data[7] = 0xCD;
        let report = parse(&data)?;
        assert_eq!(report.buttons, 0xCDAB);
        Ok(())
    }

    #[test]
    fn parse_minimum_length() {
        // Exactly 12 bytes should succeed.
        let data = [0u8; 12];
        assert!(parse(&data).is_ok());
        // 11 bytes should fail.
        let data = [0u8; 11];
        assert!(parse(&data).is_err());
    }
}

//! PXN input report parsing (64-byte USB HID report, report ID 0x01).
//!
//! # Report layout
//! | Offset | Size | Field    | Encoding                          |
//! |--------|------|----------|-----------------------------------|
//! | 0–1    | i16  | steering | LE, ±32767 → ±900°                |
//! | 2–3    | u16  | throttle | LE, 0–65535                       |
//! | 4–5    | u16  | brake    | LE, 0–65535                       |
//! | 6–7    | u16  | buttons  | packed bits                       |
//! | 8–9    | u16  | clutch   | LE, 0–65535                       |

/// Full PXN input report length in bytes.
pub const REPORT_LEN: usize = 64;

/// Report ID for PXN input reports.
pub const REPORT_ID: u8 = 0x01;

/// Single-direction steering range in degrees (PXN V12 supports ±900°).
pub const STEERING_RANGE_DEG: f32 = 900.0;

/// Parsed PXN input report with all axes normalised.
#[derive(Debug, Clone, PartialEq)]
pub struct PxnInputReport {
    /// Steering angle normalised to −1.0 … +1.0 (full range = ±900°).
    pub steering: f32,
    /// Throttle pedal position, 0.0 … 1.0.
    pub throttle: f32,
    /// Brake pedal position, 0.0 … 1.0.
    pub brake: f32,
    /// Clutch pedal position, 0.0 … 1.0.
    pub clutch: f32,
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

/// Parse a raw PXN input report byte slice into a [`PxnInputReport`].
///
/// The slice must be at least 10 bytes long; bytes beyond the first 10 are
/// ignored (the device sends 64-byte reports but only the first 10 carry data).
pub fn parse(data: &[u8]) -> Result<PxnInputReport, ParseError> {
    const NEED: usize = 10;
    if data.len() < NEED {
        return Err(ParseError::TooShort {
            got: data.len(),
            need: NEED,
        });
    }

    let raw_steering = i16::from_le_bytes([data[0], data[1]]);
    let raw_throttle = u16::from_le_bytes([data[2], data[3]]);
    let raw_brake = u16::from_le_bytes([data[4], data[5]]);
    let buttons = (data[6] as u16) | ((data[7] as u16) << 8);
    let raw_clutch = u16::from_le_bytes([data[8], data[9]]);

    Ok(PxnInputReport {
        steering: (raw_steering as f32 / i16::MAX as f32).clamp(-1.0, 1.0),
        throttle: (raw_throttle as f32 / u16::MAX as f32).clamp(0.0, 1.0),
        brake: (raw_brake as f32 / u16::MAX as f32).clamp(0.0, 1.0),
        clutch: (raw_clutch as f32 / u16::MAX as f32).clamp(0.0, 1.0),
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
        assert_eq!(err, ParseError::TooShort { got: 3, need: 10 });
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
        let bytes = i16::MAX.to_le_bytes();
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
        // Exactly 10 bytes should succeed.
        assert!(parse(&[0u8; 10]).is_ok());
        // 9 bytes should fail.
        assert!(parse(&[0u8; 9]).is_err());
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(500))]

        /// Parsing any arbitrary byte sequence must never panic.
        #[test]
        fn prop_parse_never_panics(
            data in proptest::collection::vec(proptest::num::u8::ANY, 0..=64usize),
        ) {
            let _ = parse(&data);
        }

        /// When parse succeeds, steering must always be in [-1.0, 1.0].
        #[test]
        fn prop_steering_in_valid_range(
            steer_lsb in proptest::num::u8::ANY,
            steer_msb in proptest::num::u8::ANY,
            rest in proptest::collection::vec(proptest::num::u8::ANY, 8usize),
        ) {
            let mut data = vec![steer_lsb, steer_msb];
            data.extend_from_slice(&rest);
            if let Ok(report) = parse(&data) {
                prop_assert!(
                    report.steering >= -1.0 && report.steering <= 1.0,
                    "steering {} out of [-1.0, 1.0]",
                    report.steering
                );
            }
        }

        /// When parse succeeds, all axis values must be finite and in expected range.
        #[test]
        fn prop_axes_always_finite(
            data in proptest::collection::vec(proptest::num::u8::ANY, 10usize..=16usize),
        ) {
            if let Ok(report) = parse(&data) {
                prop_assert!(report.steering.is_finite(), "steering must be finite");
                prop_assert!(report.throttle.is_finite(), "throttle must be finite");
                prop_assert!(report.brake.is_finite(), "brake must be finite");
                prop_assert!(report.clutch.is_finite(), "clutch must be finite");
                prop_assert!(report.throttle >= 0.0 && report.throttle <= 1.0,
                    "throttle {} out of [0, 1]", report.throttle);
                prop_assert!(report.brake >= 0.0 && report.brake <= 1.0,
                    "brake {} out of [0, 1]", report.brake);
                prop_assert!(report.clutch >= 0.0 && report.clutch <= 1.0,
                    "clutch {} out of [0, 1]", report.clutch);
            }
        }

        /// Exact 10-byte slice always parses successfully.
        #[test]
        fn prop_10_bytes_always_ok(
            data in proptest::collection::vec(proptest::num::u8::ANY, 10usize),
        ) {
            prop_assert!(parse(&data).is_ok(), "10-byte slice must always parse OK");
        }

        /// Fewer than 10 bytes always fails with TooShort.
        #[test]
        fn prop_short_slice_always_fails(
            data in proptest::collection::vec(proptest::num::u8::ANY, 0..9usize),
        ) {
            let result = parse(&data);
            prop_assert!(
                matches!(result, Err(ParseError::TooShort { .. })),
                "short slice should return TooShort, got {:?}",
                result
            );
        }
    }
}

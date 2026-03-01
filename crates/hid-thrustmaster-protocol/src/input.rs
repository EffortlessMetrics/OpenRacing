//! Thrustmaster HID input report parsing.
//!
//! All functions are pure and allocation-free.
//!
//! # Input report format variants (from hid-tmff2 HID descriptors)
//!
//! Thrustmaster wheels expose different input report formats depending on mode:
//!
//! - **PS3 normal / advanced mode** (T300RS, TX, TS-XW, TS-PC): Report ID 0x07,
//!   steering as 16-bit LE (0–65535), pedals as 10-bit (0–1023, Usage Rz/Z/Y).
//! - **PS4 mode** (T300RS PS4, T248): Report ID 0x01, steering as 16-bit LE
//!   (bytes 44–45 in a larger 64-byte report), pedals as 16-bit LE.
//!
//! This module currently parses a simplified format (Report ID 0x01, 8-bit pedals)
//! suitable for initial integration. A full implementation should handle both
//! PS3-mode 10-bit and PS4-mode 16-bit pedal resolutions.
//!
//! Source: HID report descriptor fixups in Kimplul/hid-tmff2
//! `src/tmt300rs/hid-tmt300rs.c` (`t300rs_rdesc_nrm_fixed`,
//! `t300rs_rdesc_ps4_fixed`) and `src/tmt248/hid-tmt248.c`.

#![deny(static_mut_refs)]

use crate::types::ThrustmasterPedalAxesRaw;

/// Standard input report ID for Thrustmaster wheels.
pub const STANDARD_INPUT_REPORT_ID: u8 = 0x01;

/// Parsed state from a Thrustmaster standard input report (ID 0x01).
#[derive(Debug, Clone, Copy, Default)]
pub struct ThrustmasterInputState {
    /// Steering position, normalized to [-1.0, +1.0] (center = 0.0).
    pub steering: f32,
    /// Throttle position, normalized to [0.0, 1.0].
    pub throttle: f32,
    /// Brake position, normalized to [0.0, 1.0].
    pub brake: f32,
    /// Clutch position, normalized to [0.0, 1.0].
    pub clutch: f32,
    /// Button bitmask (bits 0-15).
    pub buttons: u16,
    /// D-pad hat switch value (0x0 = up, 0x8 = neutral).
    pub hat: u8,
    /// Right paddle shifter (upshift).
    pub paddle_right: bool,
    /// Left paddle shifter (downshift).
    pub paddle_left: bool,
}

/// Parse a Thrustmaster standard input report (ID 0x01, 16 bytes).
///
/// Returns `None` if `data` is too short or does not begin with report ID 0x01.
pub fn parse_input_report(data: &[u8]) -> Option<ThrustmasterInputState> {
    if data.len() < 10 || data[0] != STANDARD_INPUT_REPORT_ID {
        return None;
    }

    let steering_raw = u16::from_le_bytes([data[1], data[2]]);
    let steering = normalize_steering(steering_raw);

    let throttle = data[3] as f32 / 255.0;
    let brake = data[4] as f32 / 255.0;
    let clutch = data[5] as f32 / 255.0;

    let buttons = u16::from_le_bytes([data[6], data[7]]);

    let hat = data[8] & 0x0F;

    let paddle_right = (data[9] & 0x01) != 0;
    let paddle_left = (data[9] & 0x02) != 0;

    Some(ThrustmasterInputState {
        steering,
        throttle,
        brake,
        clutch,
        buttons,
        hat,
        paddle_right,
        paddle_left,
    })
}

/// Parse pedal data from a Thrustmaster pedal USB report.
///
/// Thrustmaster pedals typically send 3 bytes: throttle, brake, clutch.
pub fn parse_pedal_report(data: &[u8]) -> Option<ThrustmasterPedalAxesRaw> {
    if data.len() < 3 {
        return None;
    }

    Some(ThrustmasterPedalAxesRaw {
        throttle: data[0],
        brake: data[1],
        clutch: if data.len() >= 3 { Some(data[2]) } else { None },
    })
}

/// Normalize a 16-bit unsigned steering value to [-1.0, +1.0].
///
/// Center (0x8000) → 0.0, minimum (0x0000) → -1.0, maximum (0xFFFF) → ~+1.0.
fn normalize_steering(raw: u16) -> f32 {
    (raw as f32 - 32768.0) / 32768.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_center_steering() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = [0u8; 16];
        data[0] = STANDARD_INPUT_REPORT_ID;
        data[1] = 0x00;
        data[2] = 0x80;
        let state = parse_input_report(&data).ok_or("parse failed")?;
        assert!(state.steering.abs() < 0.0001);
        Ok(())
    }

    #[test]
    fn test_parse_full_left_steering() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = [0u8; 16];
        data[0] = STANDARD_INPUT_REPORT_ID;
        data[1] = 0x00;
        data[2] = 0x00;
        let state = parse_input_report(&data).ok_or("parse failed")?;
        assert!((state.steering + 1.0).abs() < 0.0001);
        Ok(())
    }

    #[test]
    fn test_parse_full_right_steering() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = [0u8; 16];
        data[0] = STANDARD_INPUT_REPORT_ID;
        data[1] = 0xFF;
        data[2] = 0xFF;
        let state = parse_input_report(&data).ok_or("parse failed")?;
        assert!((state.steering - 1.0).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn test_parse_pedals() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = [0u8; 16];
        data[0] = STANDARD_INPUT_REPORT_ID;
        data[3] = 0xFF;
        data[4] = 0x00;
        data[5] = 0x80;
        let state = parse_input_report(&data).ok_or("parse failed")?;
        assert!((state.throttle - 1.0).abs() < 0.001);
        assert!(state.brake.abs() < 0.001);
        assert!((state.clutch - 0.502).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn test_parse_buttons() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = [0u8; 16];
        data[0] = STANDARD_INPUT_REPORT_ID;
        data[6] = 0b00000001;
        data[7] = 0b00000010;
        let state = parse_input_report(&data).ok_or("parse failed")?;
        assert_eq!(state.buttons & 0x0001, 1);
        assert_eq!((state.buttons >> 9) & 1, 1);
        Ok(())
    }

    #[test]
    fn test_parse_paddles() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = [0u8; 16];
        data[0] = STANDARD_INPUT_REPORT_ID;
        data[9] = 0b00000011;
        let state = parse_input_report(&data).ok_or("parse failed")?;
        assert!(state.paddle_right);
        assert!(state.paddle_left);
        Ok(())
    }

    #[test]
    fn test_parse_report_too_short() {
        let data = [STANDARD_INPUT_REPORT_ID, 0x00, 0x80];
        assert!(parse_input_report(&data).is_none());
    }

    #[test]
    fn test_parse_wrong_report_id() {
        let data = [0x02u8, 0x00, 0x80, 0, 0, 0, 0, 0, 0, 0];
        assert!(parse_input_report(&data).is_none());
    }

    #[test]
    fn test_parse_pedal_report() -> Result<(), Box<dyn std::error::Error>> {
        let data = [0xFF, 0x80, 0x40];
        let pedals = parse_pedal_report(&data).ok_or("parse failed")?;
        assert_eq!(pedals.throttle, 0xFF);
        assert_eq!(pedals.brake, 0x80);
        assert_eq!(pedals.clutch, Some(0x40));
        Ok(())
    }

    #[test]
    fn test_parse_pedal_report_too_short() {
        let data = [0xFF, 0x80];
        assert!(parse_pedal_report(&data).is_none());
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(500))]

        #[test]
        fn prop_steering_center_is_zero(steering in 0u16..=65535u16) {
            let normalized = (steering as f32 - 32768.0) / 32768.0;
            if steering == 32768 {
                prop_assert!(normalized.abs() < 0.001);
            }
        }

        #[test]
        fn prop_steering_range(steering in 0u16..=65535u16) {
            let normalized = (steering as f32 - 32768.0) / 32768.0;
            prop_assert!((-1.001..=1.001).contains(&normalized));
        }

        #[test]
        fn prop_pedal_values_in_range(
            throttle in 0u8..=255u8,
            brake in 0u8..=255u8,
            clutch in 0u8..=255u8,
        ) {
            let throttle_f = throttle as f32 / 255.0;
            let brake_f = brake as f32 / 255.0;
            let clutch_f = clutch as f32 / 255.0;
            prop_assert!((0.0..=1.0).contains(&throttle_f));
            prop_assert!((0.0..=1.0).contains(&brake_f));
            prop_assert!((0.0..=1.0).contains(&clutch_f));
        }

        #[test]
        fn prop_valid_report_id_always_accepted(report_id in 0u8..=255u8) {
            let mut data = [0u8; 16];
            data[0] = report_id;
            data[1] = 0x00;
            data[2] = 0x80;
            if report_id == STANDARD_INPUT_REPORT_ID {
                prop_assert!(parse_input_report(&data).is_some());
            }
        }
    }
}

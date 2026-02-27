//! VRS DirectForce Pro HID input report parsing.
//!
//! All functions are pure and allocation-free.

#![deny(unsafe_op_in_unsafe_fn)]
#![deny(static_mut_refs)]

use crate::types::VrsPedalAxesRaw;

/// Parsed state from a VRS DirectForce Pro standard input report.
///
/// The standard input report is 64 bytes (HID interrupt), with the following layout:
/// - Bytes 0-1: Steering position (16-bit signed, -32768 to +32767, 0 = center)
/// - Bytes 2-3: Throttle (16-bit unsigned, 0 = released, 0xFFFF = fully pressed)
/// - Bytes 4-5: Brake (16-bit unsigned, 0 = released, 0xFFFF = fully pressed)
/// - Bytes 6-7: Clutch (16-bit unsigned, 0 = released, 0xFFFF = fully pressed)
/// - Bytes 8-9: Button bitmask lower (buttons 0-7)
/// - Bytes 10-11: Button bitmask upper (buttons 8-15)
/// - Byte 12: D-pad/Hat switch (0x0 = up, 0xF = neutral)
/// - Bytes 13-14: Encoder 1 position (signed, 0-255)
/// - Bytes 15-16: Encoder 2 position (signed, 0-255)
/// - Bytes 17-63: Reserved for future use
///
/// VRS DirectForce Pro uses signed 16-bit steering centered at 0.
#[derive(Debug, Clone, Copy, Default)]
pub struct VrsInputState {
    /// Steering position, normalized to [-1.0, +1.0] (center = 0.0).
    pub steering: f32,
    /// Throttle position, normalized to [0.0, 1.0] (0 = released, 1 = fully pressed).
    pub throttle: f32,
    /// Brake position, normalized to [0.0, 1.0] (0 = released, 1 = fully pressed).
    pub brake: f32,
    /// Clutch position, normalized to [0.0, 1.0] (0 = released, 1 = fully pressed).
    pub clutch: f32,
    /// Button bitmask (low byte = buttons 0–7, high byte = buttons 8–15).
    pub buttons: u16,
    /// D-pad hat switch value (0x0 = up, 0xF = neutral, per USB HID hat encoding).
    pub hat: u8,
    /// Encoder 1 position (incremental, wraps at 256).
    pub encoder1: i8,
    /// Encoder 2 position (incremental, wraps at 256).
    pub encoder2: i8,
    /// Connection status (true if device is connected and responding).
    pub connected: bool,
}

impl VrsInputState {
    /// Returns a zero-initialized state.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Returns the raw pedal values as a struct.
    pub fn pedal_axes_raw(&self) -> VrsPedalAxesRaw {
        VrsPedalAxesRaw {
            throttle: (self.throttle * u16::MAX as f32) as u16,
            brake: (self.brake * u16::MAX as f32) as u16,
            clutch: (self.clutch * u16::MAX as f32) as u16,
        }
    }
}

/// Parse a VRS DirectForce Pro standard input report.
///
/// Returns `None` if `data` is too short or invalid.
pub fn parse_input_report(data: &[u8]) -> Option<VrsInputState> {
    if data.len() < 17 {
        return None;
    }

    let steering_raw = i16::from_le_bytes([data[0], data[1]]);
    let steering = normalize_steering(steering_raw);

    let throttle_raw = u16::from_le_bytes([data[2], data[3]]);
    let throttle = throttle_raw as f32 / u16::MAX as f32;

    let brake_raw = u16::from_le_bytes([data[4], data[5]]);
    let brake = brake_raw as f32 / u16::MAX as f32;

    let clutch_raw = u16::from_le_bytes([data[6], data[7]]);
    let clutch = clutch_raw as f32 / u16::MAX as f32;

    let buttons = u16::from_le_bytes([data[8], data[9]]);

    let hat = data[12] & 0x0F;

    let encoder1 = data[13] as i8;
    let encoder2 = data[15] as i8;

    let connected = data[0] != 0xFF || data[1] != 0xFF;

    Some(VrsInputState {
        steering,
        throttle,
        brake,
        clutch,
        buttons,
        hat,
        encoder1,
        encoder2,
        connected,
    })
}

/// Normalize a 16-bit signed steering value to [-1.0, +1.0].
///
/// Center (0) → 0.0, minimum (-32768) → -1.0, maximum (+32767) → +1.0.
#[inline]
fn normalize_steering(raw: i16) -> f32 {
    const MAX_ABS: f32 = 32768.0;
    (raw as f32 / MAX_ABS).clamp(-1.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_steering_normalization_boundaries() {
        assert!((normalize_steering(-32768) + 1.0).abs() < 0.0001);
        assert!(normalize_steering(0).abs() < 0.0001);
        assert!((normalize_steering(32767) - 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_parse_center_steering() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = vec![0u8; 64];
        data[0] = 0x00;
        data[1] = 0x00; // 0 = center
        let state = parse_input_report(&data).ok_or("parse failed")?;
        assert!(state.steering.abs() < 0.0001);
        Ok(())
    }

    #[test]
    fn test_parse_full_left_steering() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = vec![0u8; 64];
        data[0] = 0x00;
        data[1] = 0x80; // -32768 = full left
        let state = parse_input_report(&data).ok_or("parse failed")?;
        assert!((state.steering + 1.0).abs() < 0.0001);
        Ok(())
    }

    #[test]
    fn test_parse_full_right_steering() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = vec![0u8; 64];
        data[0] = 0xFF;
        data[1] = 0x7F; // 32767 = full right
        let state = parse_input_report(&data).ok_or("parse failed")?;
        assert!((state.steering - 1.0).abs() < 0.0001);
        Ok(())
    }

    #[test]
    fn test_parse_pedals_full() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = vec![0u8; 64];
        data[2] = 0xFF;
        data[3] = 0xFF;
        data[4] = 0xFF;
        data[5] = 0xFF;
        data[6] = 0xFF;
        data[7] = 0xFF;
        let state = parse_input_report(&data).ok_or("parse failed")?;
        assert!((state.throttle - 1.0).abs() < 0.001);
        assert!((state.brake - 1.0).abs() < 0.001);
        assert!((state.clutch - 1.0).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn test_parse_pedals_released() -> Result<(), Box<dyn std::error::Error>> {
        let data = vec![0u8; 64];
        let state = parse_input_report(&data).ok_or("parse failed")?;
        assert!(state.throttle.abs() < 0.001);
        assert!(state.brake.abs() < 0.001);
        assert!(state.clutch.abs() < 0.001);
        Ok(())
    }

    #[test]
    fn test_parse_buttons() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = vec![0u8; 64];
        data[8] = 0b00000001; // button 0 in low byte (LSB)
        data[9] = 0b00000010; // button 9 in high byte (bit 1)
        let state = parse_input_report(&data).ok_or("parse failed")?;
        assert_eq!(state.buttons & 0x0001, 1);
        assert_eq!((state.buttons >> 9) & 1, 1);
        Ok(())
    }

    #[test]
    fn test_parse_hat() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = vec![0u8; 64];
        data[12] = 0x00;
        let state = parse_input_report(&data).ok_or("parse failed")?;
        assert_eq!(state.hat, 0x00);

        data[12] = 0x0F;
        let state = parse_input_report(&data).ok_or("parse failed")?;
        assert_eq!(state.hat, 0x0F);
        Ok(())
    }

    #[test]
    fn test_parse_encoders() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = vec![0u8; 64];
        data[13] = 42;
        data[15] = 128;
        let state = parse_input_report(&data).ok_or("parse failed")?;
        assert_eq!(state.encoder1, 42);
        assert_eq!(state.encoder2, -128);
        Ok(())
    }

    #[test]
    fn test_parse_report_too_short() -> Result<(), Box<dyn std::error::Error>> {
        let data = vec![0u8; 16];
        assert!(parse_input_report(&data).is_none());
        Ok(())
    }

    #[test]
    fn test_pedal_axes_raw() {
        let state = VrsInputState {
            throttle: 0.5,
            brake: 0.25,
            clutch: 0.75,
            ..Default::default()
        };
        let raw = state.pedal_axes_raw();

        let expected_throttle = (0.5 * u16::MAX as f32) as u16;
        let expected_brake = (0.25 * u16::MAX as f32) as u16;
        let expected_clutch = (0.75 * u16::MAX as f32) as u16;

        assert_eq!(raw.throttle, expected_throttle);
        assert_eq!(raw.brake, expected_brake);
        assert_eq!(raw.clutch, expected_clutch);
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(500))]

        #[test]
        fn prop_parse_input_report_arbitrary_data(ref data in any::<Vec<u8>>()) {
            let _ = parse_input_report(data);
        }

        #[test]
        fn prop_steering_normalization_never_exceeds_bounds(steering in -32768i16..=32767i16) {
            let normalized = normalize_steering(steering);
            prop_assert!(normalized >= -1.0);
            prop_assert!(normalized <= 1.0);
        }

        #[test]
        fn prop_pedal_normalization_never_exceeds_bounds(
            throttle in 0u16..=65535u16,
            brake in 0u16..=65535u16,
            clutch in 0u16..=65535u16,
        ) {
            let throttle = throttle as f32 / u16::MAX as f32;
            let brake = brake as f32 / u16::MAX as f32;
            let clutch = clutch as f32 / u16::MAX as f32;

            prop_assert!(throttle >= 0.0 && throttle <= 1.0);
            prop_assert!(brake >= 0.0 && brake <= 1.0);
            prop_assert!(clutch >= 0.0 && clutch <= 1.0);
        }
    }
}

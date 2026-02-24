//! Logitech HID input report parsing.
//!
//! All functions are pure and allocation-free.

#![deny(static_mut_refs)]

use crate::ids::report_ids;

/// Parsed state from a Logitech standard input report (ID 0x01, 12 bytes).
#[derive(Debug, Clone, Copy, Default)]
pub struct LogitechInputState {
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
    /// D-pad hat switch value (0x0 = up, 0x8 = neutral, per USB HID hat encoding).
    pub hat: u8,
    /// Paddle shifter bits (bit 0 = right/upshift, bit 1 = left/downshift).
    pub paddles: u8,
}

/// Parse a Logitech standard input report (ID 0x01, 12 bytes).
///
/// Returns `None` if `data` is too short or does not begin with report ID 0x01.
pub fn parse_input_report(data: &[u8]) -> Option<LogitechInputState> {
    if data.len() < 10 || data[0] != report_ids::STANDARD_INPUT {
        return None;
    }

    // Bytes 1–2: steering axis (16-bit unsigned, center = 0x8000)
    let steering_raw = u16::from_le_bytes([data[1], data[2]]);
    let steering = normalize_steering(steering_raw);

    // Bytes 3–5: throttle, brake, clutch (0x00 = released, 0xFF = fully pressed)
    let throttle = data[3] as f32 / 255.0;
    let brake = data[4] as f32 / 255.0;
    let clutch = data[5] as f32 / 255.0;

    // Bytes 6–7: button bitmasks
    let buttons = u16::from_le_bytes([data[6], data[7]]);

    // Byte 8: D-pad hat (lower nibble)
    let hat = data[8] & 0x0F;

    // Byte 9: paddle shifters (if present)
    let paddles = if data.len() > 9 { data[9] & 0x03 } else { 0 };

    Some(LogitechInputState {
        steering,
        throttle,
        brake,
        clutch,
        buttons,
        hat,
        paddles,
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
        let mut data = [0u8; 12];
        data[0] = 0x01;
        data[1] = 0x00;
        data[2] = 0x80; // 0x8000 = center
        let state = parse_input_report(&data).ok_or("parse failed")?;
        assert!(state.steering.abs() < 0.0001, "center should be ~0.0");
        Ok(())
    }

    #[test]
    fn test_parse_full_left_steering() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = [0u8; 12];
        data[0] = 0x01;
        data[1] = 0x00;
        data[2] = 0x00; // 0x0000 = full left
        let state = parse_input_report(&data).ok_or("parse failed")?;
        assert!((state.steering + 1.0).abs() < 0.0001, "should be -1.0");
        Ok(())
    }

    #[test]
    fn test_parse_full_right_steering() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = [0u8; 12];
        data[0] = 0x01;
        data[1] = 0xFF;
        data[2] = 0xFF; // 0xFFFF = full right
        let state = parse_input_report(&data).ok_or("parse failed")?;
        assert!((state.steering - 1.0).abs() < 0.001, "should be ~+1.0");
        Ok(())
    }

    #[test]
    fn test_parse_pedals() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = [0u8; 12];
        data[0] = 0x01;
        data[1] = 0x00;
        data[2] = 0x80; // steering center
        data[3] = 0xFF; // throttle fully pressed
        data[4] = 0x00; // brake released
        let state = parse_input_report(&data).ok_or("parse failed")?;
        assert!((state.throttle - 1.0).abs() < 0.001);
        assert!(state.brake.abs() < 0.001);
        Ok(())
    }

    #[test]
    fn test_parse_buttons() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = [0u8; 12];
        data[0] = 0x01;
        data[1] = 0x00;
        data[2] = 0x80;
        data[6] = 0b00000001; // button 0 pressed
        data[7] = 0b00000010; // button 9 pressed (bit 1 of high byte = bit 9 of u16)
        let state = parse_input_report(&data).ok_or("parse failed")?;
        assert_eq!(state.buttons & 0x0001, 1, "button 0 set");
        assert_eq!((state.buttons >> 9) & 1, 1, "button 9 set");
        Ok(())
    }

    #[test]
    fn test_parse_paddles() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = [0u8; 12];
        data[0] = 0x01;
        data[1] = 0x00;
        data[2] = 0x80;
        data[9] = 0b00000011; // both paddles
        let state = parse_input_report(&data).ok_or("parse failed")?;
        assert_eq!(state.paddles, 0x03);
        Ok(())
    }

    #[test]
    fn test_parse_report_too_short() -> Result<(), Box<dyn std::error::Error>> {
        let data = [0x01u8, 0x00, 0x80]; // too short
        assert!(parse_input_report(&data).is_none(), "should return None");
        Ok(())
    }

    #[test]
    fn test_parse_wrong_report_id() -> Result<(), Box<dyn std::error::Error>> {
        let data = [0x02u8, 0x00, 0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        assert!(parse_input_report(&data).is_none());
        Ok(())
    }
}

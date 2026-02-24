//! Fanatec HID input report parsing.
//!
//! All functions are pure and allocation-free.

#![deny(static_mut_refs)]

use crate::ids::report_ids;

/// Parsed state from a Fanatec standard input report (ID 0x01).
#[derive(Debug, Clone, Copy, Default)]
pub struct FanatecInputState {
    /// Steering position, normalized to [-1.0, +1.0] (center = 0.0).
    pub steering: f32,
    /// Throttle position, normalized to [0.0, 1.0] (0 = released).
    pub throttle: f32,
    /// Brake position, normalized to [0.0, 1.0] (0 = released).
    pub brake: f32,
    /// Clutch position, normalized to [0.0, 1.0] (0 = released).
    pub clutch: f32,
    /// Button bitmask (16 bits, see protocol docs for bit assignments).
    pub buttons: u16,
    /// D-pad / hat direction nibble (0x0–0x7 = cardinal/diagonal, 0xF = neutral).
    pub hat: u8,
}

/// Parsed state from a Fanatec extended telemetry report (ID 0x02).
#[derive(Debug, Clone, Copy, Default)]
pub struct FanatecExtendedState {
    /// High-resolution steering angle (raw signed 16-bit, device units).
    pub steering_raw: i16,
    /// Steering angular velocity (raw signed 16-bit, device units).
    pub steering_velocity: i16,
    /// Motor temperature in degrees Celsius.
    pub motor_temp_c: u8,
    /// Board temperature in degrees Celsius.
    pub board_temp_c: u8,
    /// Current draw in 0.1 A units.
    pub current_raw: u8,
    /// Fault flags (bit 0 = over-temp, bit 1 = over-current,
    /// bit 2 = communication error, bit 3 = motor fault).
    pub fault_flags: u8,
}

/// Parse a Fanatec standard input report (ID 0x01, 64 bytes).
///
/// Returns `None` if `data` is too short or does not begin with report ID 0x01.
pub fn parse_standard_report(data: &[u8]) -> Option<FanatecInputState> {
    if data.len() < 10 || data[0] != report_ids::STANDARD_INPUT {
        return None;
    }

    let steering_raw = u16::from_le_bytes([data[1], data[2]]);
    let steering = normalize_steering(steering_raw);

    // Axes are inverted: 0xFF = released (0.0), 0x00 = fully pressed (1.0).
    let throttle = normalize_inverted_axis(data[3]);
    let brake = normalize_inverted_axis(data[4]);
    let clutch = normalize_inverted_axis(data[5]);

    let buttons = u16::from_le_bytes([data[7], data[8]]);
    let hat = data[9] & 0x0F;

    Some(FanatecInputState {
        steering,
        throttle,
        brake,
        clutch,
        buttons,
        hat,
    })
}

/// Parse a Fanatec extended telemetry report (ID 0x02, 64 bytes).
///
/// Returns `None` if `data` is too short or does not begin with report ID 0x02.
pub fn parse_extended_report(data: &[u8]) -> Option<FanatecExtendedState> {
    if data.len() < 11 || data[0] != report_ids::EXTENDED_INPUT {
        return None;
    }

    let steering_raw = i16::from_le_bytes([data[1], data[2]]);
    let steering_velocity = i16::from_le_bytes([data[3], data[4]]);
    let motor_temp_c = data[5];
    let board_temp_c = data[6];
    let current_raw = data[7];
    let fault_flags = data[10];

    Some(FanatecExtendedState {
        steering_raw,
        steering_velocity,
        motor_temp_c,
        board_temp_c,
        current_raw,
        fault_flags,
    })
}

/// Normalize a 16-bit steering value (center = 0x8000) to [-1.0, +1.0].
fn normalize_steering(raw: u16) -> f32 {
    const CENTER: f32 = 0x8000 as f32;
    const HALF_RANGE: f32 = 0x8000 as f32;
    ((raw as f32 - CENTER) / HALF_RANGE).clamp(-1.0, 1.0)
}

/// Normalize an inverted pedal axis byte (0xFF = released = 0.0, 0x00 = full = 1.0).
fn normalize_inverted_axis(raw: u8) -> f32 {
    (255u8.wrapping_sub(raw) as f32) / 255.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_centered_steering() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = [0u8; 64];
        data[0] = 0x01;
        data[1] = 0x00;
        data[2] = 0x80; // steering center = 0x8000
        data[3] = 0xFF; // throttle released
        data[4] = 0xFF; // brake released
        data[5] = 0xFF; // clutch released
        data[9] = 0x0F; // hat neutral

        let state = parse_standard_report(&data).ok_or("parse failed")?;
        assert!((state.steering).abs() < 1e-4, "steering should be ~0");
        assert!((state.throttle).abs() < 1e-4, "throttle should be ~0");
        assert!((state.brake).abs() < 1e-4, "brake should be ~0");
        Ok(())
    }

    #[test]
    fn test_parse_full_right_steering() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = [0u8; 64];
        data[0] = 0x01;
        data[1] = 0xFF;
        data[2] = 0xFF; // steering = 0xFFFF (full right)
        let state = parse_standard_report(&data).ok_or("parse failed")?;
        assert!(state.steering > 0.99, "steering should be ~1.0");
        Ok(())
    }

    #[test]
    fn test_parse_full_throttle() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = [0u8; 64];
        data[0] = 0x01;
        data[1] = 0x00;
        data[2] = 0x80; // center steering
        data[3] = 0x00; // throttle fully pressed (inverted: 0x00 = 1.0)
        data[4] = 0xFF;
        data[5] = 0xFF;

        let state = parse_standard_report(&data).ok_or("parse failed")?;
        assert!((state.throttle - 1.0).abs() < 1e-4, "throttle should be ~1.0");
        Ok(())
    }

    #[test]
    fn test_parse_rejects_wrong_report_id() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = [0u8; 64];
        data[0] = 0x03; // wrong report ID
        assert!(parse_standard_report(&data).is_none());
        Ok(())
    }

    #[test]
    fn test_parse_rejects_short_data() -> Result<(), Box<dyn std::error::Error>> {
        let data = [0x01u8; 5]; // too short (need >= 10)
        assert!(parse_standard_report(&data).is_none());
        Ok(())
    }

    #[test]
    fn test_parse_extended_report_basic() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = [0u8; 64];
        data[0] = 0x02; // extended report ID
        data[5] = 75;   // motor temp
        data[6] = 45;   // board temp
        data[10] = 0x01; // over-temp fault

        let state = parse_extended_report(&data).ok_or("parse failed")?;
        assert_eq!(state.motor_temp_c, 75);
        assert_eq!(state.board_temp_c, 45);
        assert_eq!(state.fault_flags & 0x01, 0x01);
        Ok(())
    }

    #[test]
    fn test_parse_extended_rejects_wrong_id() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = [0u8; 64];
        data[0] = 0x01; // wrong report ID for extended
        assert!(parse_extended_report(&data).is_none());
        Ok(())
    }
}

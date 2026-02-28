//! Simagic HID input report parsing.
//!
//! All functions are pure and allocation-free.

#![deny(static_mut_refs)]

use crate::types::{QuickReleaseStatus, SimagicGear, SimagicPedalAxesRaw, SimagicShifterState};

/// Parsed state from a Simagic standard input report.
///
/// The standard input report is typically 64 bytes, with the following layout:
/// - Bytes 0-1: Steering position (16-bit unsigned, 0x8000 = center)
/// - Bytes 2-3: Throttle (16-bit unsigned, 0 = released, 0xFFFF = fully pressed)
/// - Bytes 4-5: Brake (16-bit unsigned, 0 = released, 0xFFFF = fully pressed)
/// - Bytes 6-7: Clutch (16-bit unsigned, 0 = released, 0xFFFF = fully pressed)
/// - Bytes 8-9: Handbrake (16-bit unsigned, 0 = released, 0xFFFF = fully pressed)
/// - Bytes 10-11: Button bitmask (buttons 0-15)
/// - Byte 12: D-pad/Hat switch (0x0 = up, 0x8 = neutral)
/// - Byte 13: Rotary encoder 1
/// - Byte 14: Rotary encoder 2
/// - Byte 15: Shifter state (0-8 for gears, 0xFF for neutral)
/// - Byte 16: Flags (bit 0: clutch in range, bit 1: sequential up, bit 2: sequential down)
/// - Bytes 17-19: Quick release status (0 = attached, 1 = detached)
/// - Bytes 20-63: Reserved
#[derive(Debug, Clone, Copy, Default)]
pub struct SimagicInputState {
    /// Steering position, normalized to [-1.0, +1.0] (center = 0.0).
    pub steering: f32,
    /// Throttle position, normalized to [0.0, 1.0] (0 = released, 1 = fully pressed).
    pub throttle: f32,
    /// Brake position, normalized to [0.0, 1.0] (0 = released, 1 = fully pressed).
    pub brake: f32,
    /// Clutch position, normalized to [0.0, 1.0] (0 = released, 1 = fully pressed).
    pub clutch: f32,
    /// Handbrake position, normalized to [0.0, 1.0] (0 = released, 1 = fully pressed).
    pub handbrake: f32,
    /// Button bitmask (low byte = buttons 0–7, high byte = buttons 8–15).
    pub buttons: u16,
    /// D-pad hat switch value (0x0 = up, 0x8 = neutral, per USB HID hat encoding).
    pub hat: u8,
    /// Rotary encoder 1 value (incremental, wraps at 256).
    pub rotary1: u8,
    /// Rotary encoder 2 value (incremental, wraps at 256).
    pub rotary2: u8,
    /// Shifter state (gear position).
    pub shifter: SimagicShifterState,
    /// Quick release system status.
    pub quick_release: QuickReleaseStatus,
    /// Firmware version (if available in report).
    pub firmware_version: Option<(u8, u8, u8)>,
}

impl SimagicInputState {
    /// Returns a zero-initialized state.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Returns the raw pedal values as a struct.
    pub fn pedal_axes_raw(&self) -> SimagicPedalAxesRaw {
        SimagicPedalAxesRaw {
            throttle: (self.throttle * u16::MAX as f32) as u16,
            brake: (self.brake * u16::MAX as f32) as u16,
            clutch: (self.clutch * u16::MAX as f32) as u16,
            handbrake: (self.handbrake * u16::MAX as f32) as u16,
        }
    }
}

/// Parse a Simagic standard input report.
///
/// Returns `None` if `data` is too short.
pub fn parse_input_report(data: &[u8]) -> Option<SimagicInputState> {
    if data.len() < 17 {
        return None;
    }

    let steering_raw = u16::from_le_bytes([data[0], data[1]]);
    let steering = normalize_steering(steering_raw);

    let throttle_raw = u16::from_le_bytes([data[2], data[3]]);
    let throttle = throttle_raw as f32 / u16::MAX as f32;

    let brake_raw = u16::from_le_bytes([data[4], data[5]]);
    let brake = brake_raw as f32 / u16::MAX as f32;

    let clutch_raw = u16::from_le_bytes([data[6], data[7]]);
    let clutch = clutch_raw as f32 / u16::MAX as f32;

    let handbrake_raw = u16::from_le_bytes([data[8], data[9]]);
    let handbrake = handbrake_raw as f32 / u16::MAX as f32;

    let buttons = u16::from_le_bytes([data[10], data[11]]);

    let hat = data[12] & 0x0F;

    let rotary1 = data[13];
    let rotary2 = data[14];

    let gear = SimagicGear::from_raw(data[15]);

    let flags = data[16];
    let shifter = SimagicShifterState {
        gear,
        clutch_in_range: (flags & 0x01) != 0,
        sequential_up_pressed: (flags & 0x02) != 0,
        sequential_down_pressed: (flags & 0x04) != 0,
    };

    let quick_release = if data.len() >= 20 {
        QuickReleaseStatus::from_raw(data[19])
    } else {
        QuickReleaseStatus::Unknown
    };

    let firmware_version = if data.len() >= 23 {
        Some((data[20], data[21], data[22]))
    } else {
        None
    };

    Some(SimagicInputState {
        steering,
        throttle,
        brake,
        clutch,
        handbrake,
        buttons,
        hat,
        rotary1,
        rotary2,
        shifter,
        quick_release,
        firmware_version,
    })
}

/// Normalize a 16-bit unsigned steering value to [-1.0, +1.0].
///
/// Center (0x8000) → 0.0, minimum (0x0000) → -1.0, maximum (0xFFFF) → +1.0.
fn normalize_steering(raw: u16) -> f32 {
    (raw as f32 - 32768.0) / 32768.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_steering_normalization_boundaries() {
        // Full left
        assert!((normalize_steering(0x0000) + 1.0).abs() < 0.0001);
        // Center
        assert!(normalize_steering(0x8000).abs() < 0.0001);
        // Full right
        assert!((normalize_steering(0xFFFF) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_normalize_steering_various() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = vec![0u8; 64];
        data[0] = 0x00;
        data[1] = 0x80; // 0x8000 = center
        let state = parse_input_report(&data).ok_or("parse failed")?;
        assert!(state.steering.abs() < 0.0001, "center should be ~0.0");
        Ok(())
    }

    #[test]
    fn test_parse_full_left_steering() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = vec![0u8; 64];
        data[0] = 0x00;
        data[1] = 0x00; // 0x0000 = full left
        let state = parse_input_report(&data).ok_or("parse failed")?;
        assert!((state.steering + 1.0).abs() < 0.0001, "should be -1.0");
        Ok(())
    }

    #[test]
    fn test_parse_full_right_steering() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = vec![0u8; 64];
        data[0] = 0xFF;
        data[1] = 0xFF; // 0xFFFF = full right
        let state = parse_input_report(&data).ok_or("parse failed")?;
        assert!((state.steering - 1.0).abs() < 0.001, "should be ~+1.0");
        Ok(())
    }

    #[test]
    fn test_parse_pedals_full() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = vec![0u8; 64];
        data[2] = 0xFF;
        data[3] = 0xFF; // throttle full
        data[4] = 0xFF;
        data[5] = 0xFF; // brake full
        data[6] = 0xFF;
        data[7] = 0xFF; // clutch full
        data[8] = 0xFF;
        data[9] = 0xFF; // handbrake full
        let state = parse_input_report(&data).ok_or("parse failed")?;
        assert!((state.throttle - 1.0).abs() < 0.001);
        assert!((state.brake - 1.0).abs() < 0.001);
        assert!((state.clutch - 1.0).abs() < 0.001);
        assert!((state.handbrake - 1.0).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn test_parse_pedals_released() -> Result<(), Box<dyn std::error::Error>> {
        let data = vec![0u8; 64];
        // all pedals released = 0
        let state = parse_input_report(&data).ok_or("parse failed")?;
        assert!(state.throttle.abs() < 0.001);
        assert!(state.brake.abs() < 0.001);
        assert!(state.clutch.abs() < 0.001);
        assert!(state.handbrake.abs() < 0.001);
        Ok(())
    }

    #[test]
    fn test_parse_buttons() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = vec![0u8; 64];
        data[10] = 0b00000001; // button 0 pressed
        data[11] = 0b00000010; // button 9 pressed
        let state = parse_input_report(&data).ok_or("parse failed")?;
        assert_eq!(state.buttons & 0x0001, 1, "button 0 set");
        assert_eq!((state.buttons >> 9) & 1, 1, "button 9 set");
        Ok(())
    }

    #[test]
    fn test_parse_hat() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = vec![0u8; 64];
        data[12] = 0x00; // hat up
        let state = parse_input_report(&data).ok_or("parse failed")?;
        assert_eq!(state.hat, 0x00);

        data[12] = 0x08; // hat center
        let state = parse_input_report(&data).ok_or("parse failed")?;
        assert_eq!(state.hat, 0x08);
        Ok(())
    }

    #[test]
    fn test_parse_rotary_encoders() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = vec![0u8; 64];
        data[13] = 42; // rotary 1
        data[14] = 128; // rotary 2
        let state = parse_input_report(&data).ok_or("parse failed")?;
        assert_eq!(state.rotary1, 42);
        assert_eq!(state.rotary2, 128);
        Ok(())
    }

    #[test]
    fn test_parse_shifter_gears() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = vec![0u8; 64];

        data[15] = 0; // neutral
        let state = parse_input_report(&data).ok_or("parse failed")?;
        assert_eq!(state.shifter.gear, SimagicGear::Neutral);

        data[15] = 1; // first gear
        let state = parse_input_report(&data).ok_or("parse failed")?;
        assert_eq!(state.shifter.gear, SimagicGear::First);

        data[15] = 6; // sixth gear
        let state = parse_input_report(&data).ok_or("parse failed")?;
        assert_eq!(state.shifter.gear, SimagicGear::Sixth);
        Ok(())
    }

    #[test]
    fn test_parse_shifter_flags() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = vec![0u8; 64];
        data[16] = 0x07; // all flags set
        let state = parse_input_report(&data).ok_or("parse failed")?;
        assert!(state.shifter.clutch_in_range);
        assert!(state.shifter.sequential_up_pressed);
        assert!(state.shifter.sequential_down_pressed);
        Ok(())
    }

    #[test]
    fn test_parse_quick_release() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = vec![0u8; 64];

        data[19] = 0; // attached
        let state = parse_input_report(&data).ok_or("parse failed")?;
        assert_eq!(state.quick_release, QuickReleaseStatus::Attached);

        data[19] = 1; // detached
        let state = parse_input_report(&data).ok_or("parse failed")?;
        assert_eq!(state.quick_release, QuickReleaseStatus::Detached);
        Ok(())
    }

    #[test]
    fn test_parse_firmware_version() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = vec![0u8; 64];
        data[20] = 1; // major
        data[21] = 2; // minor
        data[22] = 3; // patch
        let state = parse_input_report(&data).ok_or("parse failed")?;
        assert_eq!(state.firmware_version, Some((1, 2, 3)));
        Ok(())
    }

    #[test]
    fn test_parse_report_too_short() -> Result<(), Box<dyn std::error::Error>> {
        let data = vec![0u8; 16]; // too short
        assert!(parse_input_report(&data).is_none(), "should return None");
        Ok(())
    }

    #[test]
    fn test_pedal_axes_raw() {
        let state = SimagicInputState {
            throttle: 0.5,
            brake: 0.25,
            clutch: 0.75,
            handbrake: 0.0,
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

    #[test]
    fn test_normalize_steering_boundaries() {
        // Full left
        assert!((normalize_steering(0x0000) + 1.0).abs() < 0.0001);
        // Center
        assert!(normalize_steering(0x8000).abs() < 0.0001);
        // Full right
        assert!((normalize_steering(0xFFFF) - 1.0).abs() < 0.001);
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
        fn prop_steering_normalization_never_exceeds_bounds(steering in 0u16..=65535u16) {
            let mut data = vec![0u8; 64];
            data[0] = (steering & 0xFF) as u8;
            data[1] = ((steering >> 8) & 0xFF) as u8;

            if let Some(state) = parse_input_report(&data) {
                prop_assert!(state.steering >= -1.0, "steering should be >= -1.0");
                prop_assert!(state.steering <= 1.0, "steering should be <= 1.0");
            }
        }

        #[test]
        fn prop_pedal_normalization_never_exceeds_bounds(
            throttle in 0u16..=65535u16,
            brake in 0u16..=65535u16,
            clutch in 0u16..=65535u16,
            handbrake in 0u16..=65535u16,
        ) {
            let mut data = vec![0u8; 64];

            data[2] = (throttle & 0xFF) as u8;
            data[3] = ((throttle >> 8) & 0xFF) as u8;
            data[4] = (brake & 0xFF) as u8;
            data[5] = ((brake >> 8) & 0xFF) as u8;
            data[6] = (clutch & 0xFF) as u8;
            data[7] = ((clutch >> 8) & 0xFF) as u8;
            data[8] = (handbrake & 0xFF) as u8;
            data[9] = ((handbrake >> 8) & 0xFF) as u8;

            if let Some(state) = parse_input_report(&data) {
                prop_assert!(state.throttle >= 0.0 && state.throttle <= 1.0);
                prop_assert!(state.brake >= 0.0 && state.brake <= 1.0);
                prop_assert!(state.clutch >= 0.0 && state.clutch <= 1.0);
                prop_assert!(state.handbrake >= 0.0 && state.handbrake <= 1.0);
            }
        }
    }
}

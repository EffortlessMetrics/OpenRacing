//! Fanatec HID input report parsing.
//!
//! All functions are pure and allocation-free.
//!
//! ## Rim detection (Quick Release adapter)
//!
//! The attached steering wheel rim is identified by byte `0x1F` of the standard
//! input report (ID 0x01). Verified in `gotzl/hid-fanatecff` `hid-ftec.c`:
//! ```c
//! // ftecff_raw_event()
//! } else if (data[0] == 0x01) {
//!     bool changed = drv_data->wheel_id != data[0x1f];
//!     drv_data->wheel_id = data[0x1f];
//!     if (changed) kobject_uevent(&hdev->dev.kobj, KOBJ_CHANGE);
//! }
//! ```
//! When a rim is detached or swapped via the Fanatec Quick Release, the base
//! updates this byte and the driver detects the change on the next input report.

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
    /// Funky switch direction (byte 10): 0=center, 1=up, 2=right, 3=down, 4=left.
    /// Present on McLaren GT3 V2 and similar rims; 0 when rim does not have one.
    pub funky_dir: u8,
    /// Rotary encoder 1 raw value (signed 16-bit, bytes 11–12).
    /// Represents absolute position or cumulative delta depending on rim firmware.
    pub rotary1: i16,
    /// Rotary encoder 2 raw value (signed 16-bit, bytes 13–14).
    pub rotary2: i16,
    /// Left dual-clutch paddle [0.0=released, 1.0=fully pressed] (byte 15, inverted).
    /// Present on Formula V2/V2.5 and McLaren GT3 V2.
    pub clutch_left: f32,
    /// Right dual-clutch paddle [0.0=released, 1.0=fully pressed] (byte 16, inverted).
    pub clutch_right: f32,
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
        funky_dir: if data.len() > 10 { data[10] } else { 0 },
        rotary1: if data.len() > 12 {
            i16::from_le_bytes([data[11], data[12]])
        } else {
            0
        },
        rotary2: if data.len() > 14 {
            i16::from_le_bytes([data[13], data[14]])
        } else {
            0
        },
        clutch_left: if data.len() > 15 {
            normalize_inverted_axis(data[15])
        } else {
            0.0
        },
        clutch_right: if data.len() > 16 {
            normalize_inverted_axis(data[16])
        } else {
            0.0
        },
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

/// Parsed state from a Fanatec standalone pedal USB report.
///
/// Hall sensor values are 12-bit (0x000=released, 0xFFF=fully pressed), zero-padded
/// to 16 bits in the wire format. See FANATEC_PROTOCOL.md §Pedal Input Report for layout.
#[derive(Debug, Clone, Copy, Default)]
pub struct FanatecPedalState {
    /// Raw throttle value (0=released, 0x0FFF=fully pressed).
    pub throttle_raw: u16,
    /// Raw brake value (0=released, 0x0FFF=fully pressed).
    pub brake_raw: u16,
    /// Raw clutch value (0=released, 0x0FFF=fully pressed; 0 if not present).
    pub clutch_raw: u16,
    /// Number of axes detected in the report (2 = throttle+brake, 3 = +clutch).
    pub axis_count: u8,
}

/// Parse a Fanatec pedal standalone USB input report.
///
/// The wire format is: `[0x01, throttle_lo, throttle_hi, brake_lo, brake_hi,
/// [clutch_lo, clutch_hi, ...]]` where each axis is a 12-bit Hall sensor value
/// packed into a `u16` LE (mask with `0x0FFF`).
///
/// Returns `None` if `data` is too short (less than 5 bytes) or the report ID
/// does not match `0x01`.
pub fn parse_pedal_report(data: &[u8]) -> Option<FanatecPedalState> {
    if data.len() < 5 || data[0] != report_ids::STANDARD_INPUT {
        return None;
    }
    let throttle_raw = u16::from_le_bytes([data[1], data[2]]) & 0x0FFF;
    let brake_raw = u16::from_le_bytes([data[3], data[4]]) & 0x0FFF;
    let (clutch_raw, axis_count) = if data.len() >= 7 {
        (u16::from_le_bytes([data[5], data[6]]) & 0x0FFF, 3)
    } else {
        (0, 2)
    };
    Some(FanatecPedalState {
        throttle_raw,
        brake_raw,
        clutch_raw,
        axis_count,
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
        assert!(
            (state.throttle - 1.0).abs() < 1e-4,
            "throttle should be ~1.0"
        );
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
        data[5] = 75; // motor temp
        data[6] = 45; // board temp
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

    #[test]
    fn test_funky_switch_center() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = [0u8; 64];
        data[0] = 0x01;
        data[10] = 0x00; // funky center
        let state = parse_standard_report(&data).ok_or("parse failed")?;
        assert_eq!(state.funky_dir, 0x00, "funky center should be 0");
        Ok(())
    }

    #[test]
    fn test_funky_switch_up() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = [0u8; 64];
        data[0] = 0x01;
        data[10] = 0x01; // funky up
        let state = parse_standard_report(&data).ok_or("parse failed")?;
        assert_eq!(state.funky_dir, 0x01, "funky up should be 1");
        Ok(())
    }

    #[test]
    fn test_rotary_encoder_values() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = [0u8; 64];
        data[0] = 0x01;
        // rotary1 = 0x00F0 = 240 (little-endian)
        data[11] = 0xF0;
        data[12] = 0x00;
        // rotary2 = -100 = 0xFF9C
        data[13] = 0x9C;
        data[14] = 0xFF;
        let state = parse_standard_report(&data).ok_or("parse failed")?;
        assert_eq!(state.rotary1, 240);
        assert_eq!(state.rotary2, -100);
        Ok(())
    }

    #[test]
    fn test_dual_clutch_paddles() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = [0u8; 64];
        data[0] = 0x01;
        data[15] = 0x00; // left clutch fully pressed (inverted: 0x00 → 1.0)
        data[16] = 0xFF; // right clutch released (inverted: 0xFF → 0.0)
        let state = parse_standard_report(&data).ok_or("parse failed")?;
        assert!(
            (state.clutch_left - 1.0).abs() < 1e-4,
            "left clutch should be ~1.0"
        );
        assert!(
            (state.clutch_right).abs() < 1e-4,
            "right clutch should be ~0.0"
        );
        Ok(())
    }

    #[test]
    fn test_short_report_without_rim_extras() -> Result<(), Box<dyn std::error::Error>> {
        // 10-byte report with no rim extension bytes — extras should default to 0
        let mut data = [0u8; 10];
        data[0] = 0x01;
        let state = parse_standard_report(&data).ok_or("parse failed")?;
        assert_eq!(state.funky_dir, 0);
        assert_eq!(state.rotary1, 0);
        assert_eq!(state.rotary2, 0);
        assert!((state.clutch_left).abs() < 1e-4);
        assert!((state.clutch_right).abs() < 1e-4);
        Ok(())
    }

    #[test]
    fn test_parse_pedal_report_throttle_brake() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = [0u8; 8];
        data[0] = 0x01;
        // throttle = 0x0800 (half pressed)
        data[1] = 0x00;
        data[2] = 0x08;
        // brake = 0x0FFF (fully pressed)
        data[3] = 0xFF;
        data[4] = 0x0F;
        let state = parse_pedal_report(&data).ok_or("parse failed")?;
        assert_eq!(state.throttle_raw, 0x0800);
        assert_eq!(state.brake_raw, 0x0FFF);
        assert_eq!(state.axis_count, 3); // 8 bytes → clutch present
        Ok(())
    }

    #[test]
    fn test_parse_pedal_report_clutch_axis() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = [0u8; 7];
        data[0] = 0x01;
        data[1] = 0x00;
        data[2] = 0x04; // throttle = 0x0400
        data[3] = 0x00;
        data[4] = 0x08; // brake = 0x0800
        data[5] = 0xFF;
        data[6] = 0x0F; // clutch = 0x0FFF
        let state = parse_pedal_report(&data).ok_or("parse failed")?;
        assert_eq!(state.throttle_raw, 0x0400);
        assert_eq!(state.brake_raw, 0x0800);
        assert_eq!(state.clutch_raw, 0x0FFF);
        assert_eq!(state.axis_count, 3);
        Ok(())
    }

    #[test]
    fn test_parse_pedal_report_two_axis() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = [0u8; 5];
        data[0] = 0x01;
        data[1] = 0x00;
        data[2] = 0x08; // throttle = 0x0800
        data[3] = 0xFF;
        data[4] = 0x0F; // brake = 0x0FFF
        let state = parse_pedal_report(&data).ok_or("parse failed")?;
        assert_eq!(state.axis_count, 2);
        assert_eq!(state.clutch_raw, 0);
        Ok(())
    }

    #[test]
    fn test_parse_pedal_report_rejects_wrong_id() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = [0u8; 8];
        data[0] = 0x02; // not a pedal report
        assert!(parse_pedal_report(&data).is_none());
        Ok(())
    }

    #[test]
    fn test_parse_pedal_report_rejects_short_data() -> Result<(), Box<dyn std::error::Error>> {
        let data = [0x01u8; 4]; // too short
        assert!(parse_pedal_report(&data).is_none());
        Ok(())
    }

    /// Kill mutant: `& 0x0F` → `| 0x0F` in hat parsing (line 89).
    /// If & is replaced with |, hat would always have upper bits set.
    #[test]
    fn test_hat_mask_isolates_lower_nibble() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = [0u8; 64];
        data[0] = 0x01;
        data[9] = 0xF2; // upper nibble set, lower = 2 (right)
        let state = parse_standard_report(&data).ok_or("parse failed")?;
        assert_eq!(state.hat, 0x02, "hat must mask to lower nibble only");

        // Also test with all zeros in lower nibble
        data[9] = 0xF0;
        let state2 = parse_standard_report(&data).ok_or("parse failed")?;
        assert_eq!(state2.hat, 0x00, "hat must be 0 when lower nibble is 0");
        Ok(())
    }

    /// Kill mutants: `> 12` → `>= 12`, `> 14` → `>= 14`, `> 10` → `>= 10`
    /// Test exact boundary lengths for optional fields.
    #[test]
    fn test_optional_fields_exact_boundary_lengths() -> Result<(), Box<dyn std::error::Error>> {
        // 11 bytes: funky_dir at data[10] is present (len > 10 → true)
        let mut data11 = [0u8; 11];
        data11[0] = 0x01;
        data11[10] = 0x03; // funky direction
        let state11 = parse_standard_report(&data11).ok_or("parse failed")?;
        assert_eq!(state11.funky_dir, 0x03, "funky_dir should be present at len=11");
        assert_eq!(state11.rotary1, 0, "rotary1 should be 0 at len=11");

        // 13 bytes: rotary1 at data[11..12] present (len > 12 → true)
        let mut data13 = [0u8; 13];
        data13[0] = 0x01;
        data13[11] = 0x10;
        data13[12] = 0x00; // rotary1 = 16
        let state13 = parse_standard_report(&data13).ok_or("parse failed")?;
        assert_eq!(state13.rotary1, 16, "rotary1 should be present at len=13");
        assert_eq!(state13.rotary2, 0, "rotary2 should be 0 at len=13");

        // 12 bytes: rotary1 NOT present (len > 12 → false)
        let mut data12 = [0u8; 12];
        data12[0] = 0x01;
        data12[11] = 0xFF; // this should NOT be read as rotary1
        let state12 = parse_standard_report(&data12).ok_or("parse failed")?;
        assert_eq!(state12.rotary1, 0, "rotary1 should be 0 at len=12");

        // 15 bytes: rotary2 present (len > 14 → true)
        let mut data15 = [0u8; 15];
        data15[0] = 0x01;
        data15[13] = 0x20;
        data15[14] = 0x00; // rotary2 = 32
        let state15 = parse_standard_report(&data15).ok_or("parse failed")?;
        assert_eq!(state15.rotary2, 32, "rotary2 should be present at len=15");

        // 14 bytes: rotary2 NOT present (len > 14 → false)
        let mut data14 = [0u8; 14];
        data14[0] = 0x01;
        data14[13] = 0xFF;
        let state14 = parse_standard_report(&data14).ok_or("parse failed")?;
        assert_eq!(state14.rotary2, 0, "rotary2 should be 0 at len=14");

        Ok(())
    }

    /// Kill mutant: `< 11` → `<= 11` or `== 11` in parse_extended_report.
    /// 11 bytes is the exact minimum; verify it parses successfully.
    #[test]
    fn test_extended_report_exact_minimum_length() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = [0u8; 11];
        data[0] = 0x02;
        data[5] = 42; // motor temp
        data[6] = 30; // board temp
        let state = parse_extended_report(&data).ok_or("11-byte extended report must parse")?;
        assert_eq!(state.motor_temp_c, 42);
        assert_eq!(state.board_temp_c, 30);
        Ok(())
    }

    /// Kill mutant: `< 11` → `<= 11` — 10 bytes must be rejected.
    #[test]
    fn test_extended_report_rejects_10_bytes() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = [0u8; 10];
        data[0] = 0x02;
        assert!(
            parse_extended_report(&data).is_none(),
            "10-byte extended report must be rejected"
        );
        Ok(())
    }

    /// Kill mutant: `& 0x0FFF` → `| 0x0FFF` in parse_pedal_report (line 178).
    /// If & is replaced with |, all pedal values would have lower 12 bits fully set.
    #[test]
    fn test_pedal_report_mask_12bit() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = [0u8; 7];
        data[0] = 0x01;
        // Set throttle to 0x0000 — with & 0x0FFF → 0, with | 0x0FFF → 0x0FFF
        data[1] = 0x00;
        data[2] = 0x00;
        // Set brake to 0xF000 — with & 0x0FFF → 0, with | 0x0FFF → 0xFFFF (as u16)
        data[3] = 0x00;
        data[4] = 0xF0;
        let state = parse_pedal_report(&data).ok_or("parse failed")?;
        assert_eq!(state.throttle_raw, 0x0000, "zero input must produce zero with AND mask");
        assert_eq!(state.brake_raw, 0x0000, "upper bits must be masked off by & 0x0FFF");
        Ok(())
    }

    /// Kill mutant: normalize_steering `/` → `%` or `*`.
    /// Center (0x8000) must normalize to 0.0, extremes to ±1.0.
    #[test]
    fn test_normalize_steering_values() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = [0u8; 64];
        data[0] = 0x01;
        data[3] = 0xFF; // throttle released
        data[4] = 0xFF; // brake released
        data[5] = 0xFF; // clutch released

        // Center: 0x8000
        data[1] = 0x00;
        data[2] = 0x80;
        let center = parse_standard_report(&data).ok_or("parse failed")?;
        assert!(center.steering.abs() < 1e-4, "center must be ~0.0, got {}", center.steering);

        // Full left: 0x0000
        data[1] = 0x00;
        data[2] = 0x00;
        let left = parse_standard_report(&data).ok_or("parse failed")?;
        assert!((left.steering + 1.0).abs() < 1e-4, "left must be ~-1.0, got {}", left.steering);

        // Full right: 0xFFFF
        data[1] = 0xFF;
        data[2] = 0xFF;
        let right = parse_standard_report(&data).ok_or("parse failed")?;
        assert!(right.steering > 0.99, "right must be ~+1.0, got {}", right.steering);

        // Quarter left: 0x4000
        data[1] = 0x00;
        data[2] = 0x40;
        let quarter = parse_standard_report(&data).ok_or("parse failed")?;
        assert!(
            (quarter.steering + 0.5).abs() < 0.01,
            "quarter left must be ~-0.5, got {}",
            quarter.steering
        );

        Ok(())
    }
}

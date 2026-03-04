//! Standard HID PID force feedback effect report encoders for OpenFFBoard.
//!
//! OpenFFBoard implements the full USB HID PID (Physical Interface Device)
//! specification. All encoders, types, and constants are provided by the
//! shared [`openracing_pidff_common`] crate. This module re-exports them
//! so downstream code can access PIDFF through the device crate.
//!
//! # Sources
//!
//! - `ffb_defs.h` — all struct definitions and constants
//! - `Firmware/FFBoard/Src/FFBoardMain.cpp` — PID effect handling
//! - USB HID PID specification (USB-IF Device Class Definition for PID)

pub use openracing_pidff_common::*;

/// Maximum number of simultaneous effects supported by firmware.
pub const MAX_EFFECTS: u8 = 40;

// ---------------------------------------------------------------------------
// Parse: PID Pool Report (0x13)
// ---------------------------------------------------------------------------

/// Parsed PID Pool feature report (device->host).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PidPoolInfo {
    /// Total RAM pool size (number of effect slots).
    pub ram_pool_size: u16,
    /// Maximum simultaneous effects.
    pub max_simultaneous: u8,
    /// Memory management flags.
    pub memory_management: u8,
}

/// Parse a PID Pool feature response.
///
/// Expects at least 4 bytes (without report ID prefix):
/// ```text
/// Bytes 0-1: RAM pool size (u16 LE)
/// Byte  2: Max simultaneous effects
/// Byte  3: Memory management (0=device-managed, 1=shared params)
/// ```
pub fn parse_pid_pool(buf: &[u8]) -> Option<PidPoolInfo> {
    if buf.len() < 4 {
        return None;
    }
    Some(PidPoolInfo {
        ram_pool_size: u16::from_le_bytes([buf[0], buf[1]]),
        max_simultaneous: buf[2],
        memory_management: buf[3],
    })
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_ids_match_pid_spec() {
        assert_eq!(report_ids::SET_EFFECT, 0x01);
        assert_eq!(report_ids::SET_ENVELOPE, 0x02);
        assert_eq!(report_ids::SET_CONDITION, 0x03);
        assert_eq!(report_ids::SET_PERIODIC, 0x04);
        assert_eq!(report_ids::SET_CONSTANT_FORCE, 0x05);
        assert_eq!(report_ids::SET_RAMP_FORCE, 0x06);
        assert_eq!(report_ids::EFFECT_OPERATION, 0x0A);
        assert_eq!(report_ids::BLOCK_FREE, 0x0B);
        assert_eq!(report_ids::DEVICE_CONTROL, 0x0C);
        assert_eq!(report_ids::DEVICE_GAIN, 0x0D);
    }

    #[test]
    fn effect_type_values() {
        assert_eq!(EffectType::Constant as u8, 1);
        assert_eq!(EffectType::Sine as u8, 4);
        assert_eq!(EffectType::Spring as u8, 8);
        assert_eq!(EffectType::Friction as u8, 11);
    }

    #[test]
    fn max_effects_matches_firmware() {
        assert_eq!(MAX_EFFECTS, 40);
    }

    #[test]
    fn constant_force_smoke() {
        let buf = encode_set_constant_force(1, -5000);
        assert_eq!(buf[0], report_ids::SET_CONSTANT_FORCE);
        assert_eq!(i16::from_le_bytes([buf[2], buf[3]]), -5000);
    }

    #[test]
    fn device_control_enable() {
        let buf = encode_device_control(device_control::ENABLE_ACTUATORS);
        assert_eq!(buf, [0x0C, 0x01]);
    }

    #[test]
    fn device_gain_clamps() {
        let buf = encode_device_gain(20000);
        assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), 10000);
    }

    #[test]
    fn parse_pid_pool_default() {
        let buf = [0x28, 0x00, 40, 1];
        let info = parse_pid_pool(&buf);
        assert!(info.is_some());
        let i = info.as_ref();
        assert_eq!(i.map(|v| v.ram_pool_size), Some(40));
        assert_eq!(i.map(|v| v.max_simultaneous), Some(40));
        assert_eq!(i.map(|v| v.memory_management), Some(1));
    }

    #[test]
    fn parse_pid_pool_too_short() {
        assert_eq!(parse_pid_pool(&[0, 0, 0]), None);
    }
}
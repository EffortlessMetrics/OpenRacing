//! Standard USB HID PID effect reports for Simucube wheelbases.
//!
//! Simucube 2 (Sport/Pro/Ultimate) and Simucube 1 wheelbases use the
//! **standard USB HID PID (Physical Interface Device)** protocol for force
//! feedback. The Linux kernel confirms this via the `hid-pidff` driver
//! (Silver support for Simucube 2 as of Linux 6.15).
//!
//! All encoders, types, and constants are provided by the shared
//! [`openracing_pidff_common`] crate. This module re-exports them so
//! downstream code can access PIDFF through the device crate.
//!
//! # Sources
//!
//! - USB HID PID 1.01 specification (pid1_01.pdf)
//! - Linux kernel `hid-pidff.c` / `hid-universal-pidff.c`
//! - JacKeTUs/linux-steering-wheels: Simucube 2 compatibility (Silver)
//! - Granite Devices wiki: Simucube uses standard HID PID

pub use openracing_pidff_common::*;

/// Maximum number of concurrent effects (typical for Simucube PIDFF devices).
pub const MAX_EFFECTS: u8 = 40;

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
    fn max_effects_simucube_specific() {
        assert_eq!(MAX_EFFECTS, 40);
    }
}

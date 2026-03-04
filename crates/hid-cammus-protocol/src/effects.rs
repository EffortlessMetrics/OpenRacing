//! Standard USB HID PID effect reports for Cammus wheelbases.
//!
//! The Cammus C5 and C12 wheelbases use standard USB HID PID for force
//! feedback on Linux (via `hid-universal-pidff.c`). This module provides
//! allocation-free PIDFF report encoders complementing the direct torque
//! streaming in `direct.rs`.
//!
//! All encoders, types, and constants are provided by the shared
//! [`openracing_pidff_common`] crate. This module re-exports them so
//! downstream code can access PIDFF through the device crate.
//!
//! # Protocol note
//!
//! On Windows, Cammus may use DirectInput which maps to HID PID internally.
//! The direct torque API in `direct.rs` is a simplified alternative. Real
//! applications should prefer the PIDFF effect-based approach for
//! compatibility with the kernel driver.
//!
//! # Sources
//!
//! - USB HID PID 1.01 specification (pid1_01.pdf)
//! - Linux kernel `hid-universal-pidff.c` (Cammus support confirmed)

pub use openracing_pidff_common::*;

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
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(500))]

        #[test]
        fn prop_constant_force_preserved(mag in -10000i16..=10000i16) {
            let buf = encode_set_constant_force(1, mag);
            prop_assert_eq!(i16::from_le_bytes([buf[2], buf[3]]), mag);
        }

        #[test]
        fn prop_periodic_values_preserved(
            mag in 0u16..=10000u16,
            offset in -10000i16..=10000i16,
        ) {
            let buf = encode_set_periodic(1, mag, offset, 0, 100);
            prop_assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), mag);
            prop_assert_eq!(i16::from_le_bytes([buf[4], buf[5]]), offset);
        }

        #[test]
        fn prop_device_gain_bounded(gain in 0u16..=20000u16) {
            let buf = encode_device_gain(gain);
            let encoded = u16::from_le_bytes([buf[2], buf[3]]);
            prop_assert!(encoded <= 10000);
        }
    }
}

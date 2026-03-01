//! Thrustmaster HID output report encoding for Force Feedback.
//!
//! All functions are pure and allocation-free.
//!
//! # Wire protocol reference (hid-tmff2)
//!
//! The Thrustmaster T300RS-family wire protocol (observed in Kimplul/hid-tmff2)
//! uses a vendor-specific HID output report (ID 0x60) with 63-byte payloads
//! (31 bytes in PS4 mode):
//!
//! - **Constant force**: signed i16, little-endian, range approx. [-16384, 16384]
//!   (Linux FF level / 2), with direction applied via sin(direction).
//!   Source: `t300rs_calculate_constant_level()` in `src/tmt300rs/hid-tmt300rs.c`.
//! - **Gain**: setup header `cmd=0x02, code=gain>>8` (16-bit gain scaled to high byte).
//!   Source: `t300rs_set_gain()` in `src/tmt300rs/hid-tmt300rs.c`.
//! - **Range**: setup header `cmd=0x08, sub=0x11`, value = `degrees * 0x3C` as LE16.
//!   Source: `t300rs_set_range()` in `src/tmt300rs/hid-tmt300rs.c`.
//! - **Autocenter**: setup headers `cmd=0x08, code=0x04, value=0x01` then
//!   `cmd=0x08, code=0x03, value=autocenter_level` (LE16).
//!   Source: `t300rs_set_autocenter()` in `src/tmt300rs/hid-tmt300rs.c`.
//! - **Open/Close**: `cmd=0x01, code=0x05` (open) / `cmd=0x01, code=0x00` (close).
//!   T248/TX/TS-XW/TS-PC use a two-step open: `{0x01,0x04}` then `{0x01,0x05}`.
//!
//! # Protocol families (verified from hid-tmff2 source)
//!
//! All of the following wheels share the **T300RS FFB protocol** — they use
//! identical `t300rs_play_effect`, `t300rs_upload_effect`, `t300rs_update_effect`,
//! `t300rs_stop_effect`, `t300rs_set_gain`, and `t300rs_set_autocenter` functions:
//!
//! - T300 RS (PS3 normal 0xb66e, PS3 advanced 0xb66f, PS4 normal 0xb66d)
//! - T248 (0xb696) — uses T300RS protocol with 900° max range (vs 1080°)
//! - TX Racing (0xb669) — uses T300RS protocol with 900° max range
//! - TS-XW Racer (0xb692) — uses T300RS protocol, 1080° max range
//! - TS-PC Racer (0xb689) — uses T300RS protocol, 1080° max range
//!
//! **Not** in the T300RS family (separate or unknown protocols):
//! - T500 RS — uses a different, older protocol (hid-tmff2 issue #18, unsupported)
//! - T150 / TMX — not supported in hid-tmff2; separate protocol
//! - T818 — not supported in hid-tmff2 (reports T248 PID per issue #58)
//! - T-GT II — reuses T300 USB PIDs per hid-tmff2 README
//!
//! Source: Kimplul/hid-tmff2 commit f004195, `src/hid-tmff2.c` probe function
//! and `*_populate_api()` functions in each wheel subdirectory.
//!
//! # USB vs Bluetooth
//!
//! All Thrustmaster racing wheels are **USB-only**. No Bluetooth mode exists.
//! The community drivers (hid-tmff2, oversteer) exclusively use USB HID
//! interrupt transfers. There is no BLE or Bluetooth Classic profile for
//! any Thrustmaster wheel product.
//!
//! # Pedal calibration data
//!
//! No standard pedal calibration data format was found in community drivers.
//! Pedals connected via RJ12 to the wheelbase are reported as additional HID
//! axes (10-bit, 0–1023 in PS3 mode) within the wheel's input report.
//! Standalone USB pedal products (T-LCM, T3PA) have unverified protocols.
//!
//! This crate's encoding is an **application-level abstraction**, not the raw USB
//! wire format. Report IDs and field layouts here are internal to OpenRacing.
//! The transport layer is responsible for mapping these to actual USB HID reports.

#![deny(static_mut_refs)]

/// Wire size of a Thrustmaster constant force output report.
pub const EFFECT_REPORT_LEN: usize = 8;

pub mod report_ids {
    pub const VENDOR_SET_RANGE: u8 = 0x80;
    pub const DEVICE_GAIN: u8 = 0x81;
    pub const ACTUATOR_ENABLE: u8 = 0x82;
    pub const CONSTANT_FORCE: u8 = 0x23;
    pub const EFFECT_OP: u8 = 0x22;
}

pub mod commands {
    pub const SET_RANGE: u8 = 0x01;
    pub const ENABLE: u8 = 0x01;
    pub const DISABLE: u8 = 0x00;
}

pub const EFFECT_TYPE_CONSTANT: u8 = 0x26;
pub const EFFECT_TYPE_RAMP: u8 = 0x27;
pub const EFFECT_TYPE_SPRING: u8 = 0x40;
pub const EFFECT_TYPE_DAMPER: u8 = 0x41;
pub const EFFECT_TYPE_FRICTION: u8 = 0x43;

#[derive(Debug, Clone, Copy)]
pub struct ThrustmasterConstantForceEncoder {
    max_torque_nm: f32,
}

impl ThrustmasterConstantForceEncoder {
    pub fn new(max_torque_nm: f32) -> Self {
        Self {
            max_torque_nm: max_torque_nm.max(0.01),
        }
    }

    pub fn encode(&self, torque_nm: f32, out: &mut [u8; EFFECT_REPORT_LEN]) -> usize {
        out.fill(0);
        out[0] = report_ids::CONSTANT_FORCE;
        out[1] = 1;
        let mag = torque_to_magnitude(torque_nm, self.max_torque_nm);
        let bytes = mag.to_le_bytes();
        out[2] = bytes[0];
        out[3] = bytes[1];
        EFFECT_REPORT_LEN
    }

    pub fn encode_zero(&self, out: &mut [u8; EFFECT_REPORT_LEN]) -> usize {
        out.fill(0);
        out[0] = report_ids::CONSTANT_FORCE;
        out[1] = 1;
        EFFECT_REPORT_LEN
    }
}

/// Convert a torque in Nm to a signed magnitude value.
///
/// Returns a signed i16 in range [-10000, 10000], encoded as little-endian
/// in the output buffer. The sign indicates direction (positive = clockwise).
///
/// Note: The hid-tmff2 wire protocol uses a different scale (approx.
/// [-16384, 16384]) with direction via sin(). Our scale normalizes
/// torque_nm/max_torque_nm to [-1.0, 1.0] then maps to ±10000.
fn torque_to_magnitude(torque_nm: f32, max_torque_nm: f32) -> i16 {
    let normalized = (torque_nm / max_torque_nm).clamp(-1.0, 1.0);
    (normalized * 10000.0) as i16
}

pub trait ThrustmasterEffectEncoder {
    fn encode(&self, torque_nm: f32, out: &mut [u8; EFFECT_REPORT_LEN]) -> usize;
    fn encode_zero(&self, out: &mut [u8; EFFECT_REPORT_LEN]) -> usize;
}

impl ThrustmasterEffectEncoder for ThrustmasterConstantForceEncoder {
    fn encode(&self, torque_nm: f32, out: &mut [u8; EFFECT_REPORT_LEN]) -> usize {
        self.encode(torque_nm, out)
    }

    fn encode_zero(&self, out: &mut [u8; EFFECT_REPORT_LEN]) -> usize {
        self.encode_zero(out)
    }
}

pub fn build_set_range_report(degrees: u16) -> [u8; 7] {
    let [lsb, msb] = degrees.to_le_bytes();
    [
        report_ids::VENDOR_SET_RANGE,
        commands::SET_RANGE,
        lsb,
        msb,
        0x00,
        0x00,
        0x00,
    ]
}

pub fn build_device_gain(gain: u8) -> [u8; 2] {
    [report_ids::DEVICE_GAIN, gain]
}

pub fn build_actuator_enable(enabled: bool) -> [u8; 2] {
    [
        report_ids::ACTUATOR_ENABLE,
        if enabled {
            commands::ENABLE
        } else {
            commands::DISABLE
        },
    ]
}

pub fn build_spring_effect(center: i16, stiffness: u16) -> [u8; EFFECT_REPORT_LEN] {
    let center_bytes = center.to_le_bytes();
    let stiffness_bytes = stiffness.to_le_bytes();
    [
        report_ids::EFFECT_OP,
        EFFECT_TYPE_SPRING,
        0x01,
        center_bytes[0],
        center_bytes[1],
        stiffness_bytes[0],
        stiffness_bytes[1],
        0x00,
    ]
}

pub fn build_damper_effect(damping: u16) -> [u8; EFFECT_REPORT_LEN] {
    let damping_bytes = damping.to_le_bytes();
    [
        report_ids::EFFECT_OP,
        EFFECT_TYPE_DAMPER,
        0x01,
        damping_bytes[0],
        damping_bytes[1],
        0x00,
        0x00,
        0x00,
    ]
}

pub fn build_friction_effect(minimum: u16, maximum: u16) -> [u8; EFFECT_REPORT_LEN] {
    let min_bytes = minimum.to_le_bytes();
    let max_bytes = maximum.to_le_bytes();
    [
        report_ids::EFFECT_OP,
        EFFECT_TYPE_FRICTION,
        0x01,
        min_bytes[0],
        min_bytes[1],
        max_bytes[0],
        max_bytes[1],
        0x00,
    ]
}

/// Build a T300RS-family range command matching the kernel driver wire format.
///
/// Source: `t300rs_set_range()` in `Kimplul/hid-tmff2/src/tmt300rs/hid-tmt300rs.c`.
///
/// This produces the 4-byte payload that goes at the start of the 63-byte
/// output report buffer (Report ID 0x60):
/// - Byte 0: 0x08 (setup command)
/// - Byte 1: 0x11 (range sub-command)
/// - Bytes 2-3: `degrees * 0x3C` as LE16
///
/// `degrees` is clamped to 40..=1080 per the kernel driver's bounds check.
pub fn build_kernel_range_command(degrees: u16) -> [u8; 4] {
    let clamped = degrees.clamp(40, 1080);
    let scaled = (clamped as u32) * 0x3C;
    let bytes = (scaled as u16).to_le_bytes();
    [0x08, 0x11, bytes[0], bytes[1]]
}

/// Build a T300RS-family gain command matching the kernel wire format.
///
/// Source: `t300rs_set_gain()` — `{0x02, gain >> 8}`
pub fn build_kernel_gain_command(gain: u16) -> [u8; 2] {
    [0x02, (gain >> 8) as u8]
}

/// Build the T300RS open/init command.
/// Source: `t300rs_send_open()` — `{0x01, 0x05}`
pub fn build_kernel_open_command() -> [u8; 2] {
    [0x01, 0x05]
}

/// Build the T300RS close command.
/// Source: `t300rs_send_close()` — `{0x01, 0x00}`
pub fn build_kernel_close_command() -> [u8; 2] {
    [0x01, 0x00]
}

/// Build the T300RS-family autocenter command sequence.
/// Source: `t300rs_set_autocenter()` — two-step: `{0x08,0x04,0x01,0x00}` then `{0x08,0x03,value_lo,value_hi}`
pub fn build_kernel_autocenter_commands(value: u16) -> [[u8; 4]; 2] {
    let bytes = value.to_le_bytes();
    [
        [0x08, 0x04, 0x01, 0x00],
        [0x08, 0x03, bytes[0], bytes[1]],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_range_900_degrees() {
        let r = build_set_range_report(900);
        assert_eq!(r[0], report_ids::VENDOR_SET_RANGE);
        assert_eq!(r[1], commands::SET_RANGE);
        assert_eq!(r[2], 0x84);
        assert_eq!(r[3], 0x03);
    }

    #[test]
    fn test_set_range_1080_degrees() {
        let r = build_set_range_report(1080);
        assert_eq!(r[2], 0x38);
        assert_eq!(r[3], 0x04);
    }

    #[test]
    fn test_device_gain_full() {
        let r = build_device_gain(0xFF);
        assert_eq!(r[0], report_ids::DEVICE_GAIN);
        assert_eq!(r[1], 0xFF);
    }

    #[test]
    fn test_device_gain_zero() {
        let r = build_device_gain(0);
        assert_eq!(r[0], report_ids::DEVICE_GAIN);
        assert_eq!(r[1], 0);
    }

    #[test]
    fn test_actuator_enable() {
        let r = build_actuator_enable(true);
        assert_eq!(r[0], report_ids::ACTUATOR_ENABLE);
        assert_eq!(r[1], commands::ENABLE);
    }

    #[test]
    fn test_actuator_disable() {
        let r = build_actuator_enable(false);
        assert_eq!(r[0], report_ids::ACTUATOR_ENABLE);
        assert_eq!(r[1], commands::DISABLE);
    }

    #[test]
    fn test_constant_force_positive() {
        let enc = ThrustmasterConstantForceEncoder::new(6.0);
        let mut out = [0u8; EFFECT_REPORT_LEN];
        enc.encode(3.0, &mut out);
        assert_eq!(out[0], report_ids::CONSTANT_FORCE);
        assert_eq!(out[1], 1);
        let mag = i16::from_le_bytes([out[2], out[3]]);
        assert_eq!(mag, 5000);
    }

    #[test]
    fn test_constant_force_negative() {
        let enc = ThrustmasterConstantForceEncoder::new(6.0);
        let mut out = [0u8; EFFECT_REPORT_LEN];
        enc.encode(-3.0, &mut out);
        let mag = i16::from_le_bytes([out[2], out[3]]);
        assert_eq!(mag, -5000);
    }

    #[test]
    fn test_constant_force_zero() {
        let enc = ThrustmasterConstantForceEncoder::new(6.0);
        let mut out = [0u8; EFFECT_REPORT_LEN];
        enc.encode_zero(&mut out);
        let mag = i16::from_le_bytes([out[2], out[3]]);
        assert_eq!(mag, 0);
    }

    #[test]
    fn test_constant_force_saturation() {
        let enc = ThrustmasterConstantForceEncoder::new(6.0);
        let mut out = [0u8; EFFECT_REPORT_LEN];
        enc.encode(100.0, &mut out);
        let mag = i16::from_le_bytes([out[2], out[3]]);
        assert_eq!(mag, 10000);
        enc.encode(-100.0, &mut out);
        let mag = i16::from_le_bytes([out[2], out[3]]);
        assert_eq!(mag, -10000);
    }

    #[test]
    fn test_spring_effect() {
        let r = build_spring_effect(0, 500);
        assert_eq!(r[0], report_ids::EFFECT_OP);
        assert_eq!(r[1], EFFECT_TYPE_SPRING);
    }

    #[test]
    fn test_damper_effect() {
        let r = build_damper_effect(300);
        assert_eq!(r[0], report_ids::EFFECT_OP);
        assert_eq!(r[1], EFFECT_TYPE_DAMPER);
    }

    #[test]
    fn test_friction_effect() {
        let r = build_friction_effect(100, 800);
        assert_eq!(r[0], report_ids::EFFECT_OP);
        assert_eq!(r[1], EFFECT_TYPE_FRICTION);
    }

    #[test]
    fn test_kernel_range_900() {
        // 900 * 0x3C = 900 * 60 = 54000 = 0xD2F0
        let r = build_kernel_range_command(900);
        assert_eq!(r, [0x08, 0x11, 0xF0, 0xD2]);
    }

    #[test]
    fn test_kernel_range_1080() {
        // 1080 * 60 = 64800 = 0xFD20
        let r = build_kernel_range_command(1080);
        assert_eq!(r, [0x08, 0x11, 0x20, 0xFD]);
    }

    #[test]
    fn test_kernel_range_40() {
        // 40 * 60 = 2400 = 0x0960
        let r = build_kernel_range_command(40);
        assert_eq!(r, [0x08, 0x11, 0x60, 0x09]);
    }

    #[test]
    fn test_kernel_range_clamps_zero_to_min() {
        // 0 clamps to 40; 40 * 60 = 2400 = 0x0960
        let r = build_kernel_range_command(0);
        assert_eq!(r, build_kernel_range_command(40));
    }

    #[test]
    fn test_kernel_range_clamps_above_max() {
        // 2000 clamps to 1080
        let r = build_kernel_range_command(2000);
        assert_eq!(r, build_kernel_range_command(1080));
    }

    #[test]
    fn test_kernel_gain_full() {
        let r = build_kernel_gain_command(0xFFFF);
        assert_eq!(r, [0x02, 0xFF]);
    }

    #[test]
    fn test_kernel_gain_zero() {
        let r = build_kernel_gain_command(0);
        assert_eq!(r, [0x02, 0x00]);
    }

    #[test]
    fn test_kernel_open_command() {
        let r = build_kernel_open_command();
        assert_eq!(r, [0x01, 0x05]);
    }

    #[test]
    fn test_kernel_close_command() {
        let r = build_kernel_close_command();
        assert_eq!(r, [0x01, 0x00]);
    }

    #[test]
    fn test_kernel_autocenter_commands() {
        let cmds = build_kernel_autocenter_commands(0x1234);
        assert_eq!(cmds[0], [0x08, 0x04, 0x01, 0x00]);
        assert_eq!(cmds[1], [0x08, 0x03, 0x34, 0x12]);
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(500))]

        #[test]
        fn prop_torque_sign_preserved(
            max in 0.1_f32..=21.0_f32,
            frac in -1.0_f32..=1.0_f32,
        ) {
            let torque_nm = max * frac;
            let enc = ThrustmasterConstantForceEncoder::new(max);
            let mut out = [0u8; EFFECT_REPORT_LEN];
            enc.encode(torque_nm, &mut out);
            let raw = i16::from_le_bytes([out[2], out[3]]);

            if torque_nm > 0.001 {
                prop_assert!(raw >= 0, "positive torque {torque_nm} should yield positive raw {raw}");
            } else if torque_nm < -0.001 {
                prop_assert!(raw <= 0, "negative torque {torque_nm} should yield negative raw {raw}");
            }
        }

        #[test]
        fn prop_encoded_value_never_overflows(
            max in 0.001_f32..=21.0_f32,
            torque in -100.0_f32..=100.0_f32,
        ) {
            let enc = ThrustmasterConstantForceEncoder::new(max);
            let mut out = [0u8; EFFECT_REPORT_LEN];
            enc.encode(torque, &mut out);
            let raw = i16::from_le_bytes([out[2], out[3]]);
            prop_assert!(out[0] == report_ids::CONSTANT_FORCE);
            if torque > max {
                prop_assert_eq!(raw, 10000, "over-max torque must saturate to 10000");
            } else if torque < -max {
                prop_assert_eq!(raw, -10000, "under-max torque must saturate to -10000");
            }
        }

        #[test]
        fn prop_encoding_is_monotone(
            max in 0.1_f32..=21.0_f32,
            frac_a in -1.0_f32..=1.0_f32,
            frac_b in -1.0_f32..=1.0_f32,
        ) {
            let ta = max * frac_a;
            let tb = max * frac_b;
            let enc = ThrustmasterConstantForceEncoder::new(max);

            let mut out_a = [0u8; EFFECT_REPORT_LEN];
            let mut out_b = [0u8; EFFECT_REPORT_LEN];
            enc.encode(ta, &mut out_a);
            enc.encode(tb, &mut out_b);

            let raw_a = i16::from_le_bytes([out_a[2], out_a[3]]);
            let raw_b = i16::from_le_bytes([out_b[2], out_b[3]]);

            if ta > tb {
                prop_assert!(
                    raw_a >= raw_b,
                    "monotone violation: torque {ta} > {tb} but raw {raw_a} < {raw_b}"
                );
            }
        }

        #[test]
        fn prop_report_id_always_correct(
            max in 0.001_f32..=21.0_f32,
            torque in -100.0_f32..=100.0_f32,
        ) {
            let enc = ThrustmasterConstantForceEncoder::new(max);
            let mut out = [0u8; EFFECT_REPORT_LEN];
            enc.encode(torque, &mut out);
            prop_assert_eq!(out[0], report_ids::CONSTANT_FORCE);
        }

        #[test]
        fn prop_gain_values(gain in 0u8..=255u8) {
            let r = build_device_gain(gain);
            assert_eq!(r[0], report_ids::DEVICE_GAIN);
            assert_eq!(r[1], gain);
        }

        #[test]
        fn prop_rotation_range_values(degrees in 200u16..=1080u16) {
            let r = build_set_range_report(degrees);
            assert_eq!(r[0], report_ids::VENDOR_SET_RANGE);
            assert_eq!(r[1], commands::SET_RANGE);
            let decoded = u16::from_le_bytes([r[2], r[3]]);
            assert_eq!(decoded, degrees);
        }
    }
}

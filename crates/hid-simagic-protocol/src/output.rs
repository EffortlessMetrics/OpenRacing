//! Simagic HID output report encoding (FFB commands).
//!
//! All functions are pure and allocation-free.
//!
//! # ⚠ Speculative wire format — does NOT match hardware directly
//!
//! This module encodes reports using this crate's **own custom report IDs**
//! (see [`crate::ids::report_ids`]: `0x11`–`0x17`, `0x20`, `0x21`, `0x30`).
//! **These report IDs are NOT the actual Simagic hardware report IDs.**
//! The transport layer must translate them to the real wire protocol before
//! sending to hardware.
//!
//! # Real Simagic wire protocol (from JacKeTUs/simagic-ff)
//!
//! The actual hardware protocol uses 64-byte HID Output Reports where the
//! first byte (`value[0]`) is the report type ID and subsequent bytes carry
//! parameters. All values are in the `report->field[0]->value` array.
//! Source: `hid-simagic.c` (commit 52e73e7).
//!
//! ## Constant force (`SM_SET_CONSTANT_REPORT = 0x05`)
//! ```text
//! value[0] = 0x05          // report type
//! value[1] = block_id      // SM_CONSTANT = 0x01
//! value[2] = magnitude     // sm_rescale_signed_to_10k(level) → ±10000
//! ```
//!
//! ## Condition effects (`SM_SET_CONDITION_REPORT = 0x03`)
//! Used for spring (block 0x06), damper (0x05), friction (0x07), inertia (0x09).
//! ```text
//! value[0] = 0x03          // report type
//! value[1] = block_id      // e.g. SM_SPRING=0x06
//! value[2] = right_coeff   // sm_rescale_coeffs(right_coeff, 0x7fff, -10000, 10000)
//! value[3] = left_coeff    // sm_rescale_coeffs(left_coeff, 0x7fff, -10000, 10000)
//! value[4] = center        // center position
//! value[5] = deadband      // deadband width
//! ```
//!
//! ## Periodic effects (`SM_SET_PERIODIC_REPORT = 0x04`)
//! Used for sine (block 0x02). Square/triangle/sawtooth are defined but
//! "no effect seen on wheelbase" per the kernel driver comments.
//! ```text
//! value[0] = 0x04          // report type
//! value[1] = block_id      // SM_SINE = 0x02
//! value[2] = magnitude     // sm_rescale_signed_to_10k(magnitude) → ±10000
//! value[3] = offset        // sm_rescale_signed_to_10k(offset) → ±10000
//! value[4] = phase         // raw phase value
//! value[5] = period        // raw period value
//! ```
//!
//! ## Set effect (`SM_SET_EFFECT_REPORT = 0x01`)
//! Creates/updates an effect slot before playback.
//! ```text
//! value[0]  = 0x01         // report type
//! value[1]  = block_id
//! value[2]  = effect_type  // same as block_id (maps from ff_effect.type)
//! value[3]  = duration_lo  // duration & 0xFF
//! value[4]  = duration_hi  // (duration >> 8) & 0xFF
//! value[9]  = 0xFF         // gain
//! value[10] = 0xFF         // trigger button
//! value[11] = 0x04
//! value[12] = 0x3F
//! ```
//!
//! ## Effect operation (`SM_EFFECT_OPERATION_REPORT = 0x0a`)
//! ```text
//! value[0] = 0x0a          // report type
//! value[1] = block_id
//! value[2] = operation     // 0x01 = start, 0x03 = stop
//! value[3] = loop_count    // 0x00–0xFF
//! ```
//!
//! ## Gain (`SM_SET_GAIN = 0x40`)
//! ```text
//! value[0] = 0x40          // report type
//! value[1] = gain >> 8     // device-wide FFB gain
//! ```
//!
//! ## Magnitude scaling
//!
//! `sm_rescale_signed_to_10k()` maps signed 16-bit values to ±10000:
//! - Positive: `value * 10000 / 0x7FFF`
//! - Negative: `value * -10000 / -0x8000`
//! - Zero: 0
//!
//! Our `torque_to_magnitude()` function uses the same ±10000 range, which
//! is consistent with the real protocol scaling.
//!
//! ## Settings (Feature Reports)
//!
//! Settings use HID Feature Reports (not Output Reports):
//! - **Report `0x80`** (set): write wheel settings. Byte `[1]` selects the
//!   settings page (`0x01` = basic, `0x02` = angle lock, `0x10` = advanced).
//! - **Report `0x81`** (get): read status — returns a 57+ byte struct with
//!   max_angle (LE16, 90–2520), ff_strength (LE16, ±100), rotation speed,
//!   centering/damper/friction/inertia (0–100 mechanical, 0–200 game),
//!   ring light, filter level (0–20), slew rate (0–100), and more.
//!
//! Source: JacKeTUs/simagic-ff `hid-simagic.c`, `hid-simagic-settings.h`

#![deny(static_mut_refs)]

use crate::ids::report_ids;

/// Wire size of a Simagic constant-force output report.
pub const CONSTANT_FORCE_REPORT_LEN: usize = 8;

/// Wire size of a Simagic spring effect output report.
pub const SPRING_REPORT_LEN: usize = 10;

/// Wire size of a Simagic damper effect output report.
pub const DAMPER_REPORT_LEN: usize = 8;

/// Wire size of a Simagic friction effect output report.
pub const FRICTION_REPORT_LEN: usize = 10;

/// Encoder for Simagic constant-force FFB output reports.
///
/// Converts a torque value in Newton-meters to the signed 16-bit Simagic wire
/// format.
#[derive(Debug, Clone, Copy)]
pub struct SimagicConstantForceEncoder {
    max_torque_nm: f32,
}

impl SimagicConstantForceEncoder {
    pub fn new(max_torque_nm: f32) -> Self {
        Self {
            max_torque_nm: max_torque_nm.max(0.01),
        }
    }

    /// Encode a torque command (Newton-meters) into a constant-force output report.
    ///
    /// Layout (8 bytes):
    /// - Byte 0: `0x11` (report ID)
    /// - Bytes 1-2: effect block index (little-endian, 1-based)
    /// - Bytes 3-4: signed magnitude, little-endian (±10000)
    /// - Bytes 5-7: reserved
    pub fn encode(&self, torque_nm: f32, out: &mut [u8; CONSTANT_FORCE_REPORT_LEN]) -> usize {
        out.fill(0);
        out[0] = report_ids::CONSTANT_FORCE;
        out[1] = 1; // effect block index (1-based)
        out[2] = 0;
        let mag = torque_to_magnitude(torque_nm, self.max_torque_nm);
        let bytes = mag.to_le_bytes();
        out[3] = bytes[0];
        out[4] = bytes[1];
        CONSTANT_FORCE_REPORT_LEN
    }

    /// Encode an explicit zero-force report.
    pub fn encode_zero(&self, out: &mut [u8; CONSTANT_FORCE_REPORT_LEN]) -> usize {
        out.fill(0);
        out[0] = report_ids::CONSTANT_FORCE;
        out[1] = 1;
        CONSTANT_FORCE_REPORT_LEN
    }
}

/// Encoder for Simagic spring effect FFB output reports.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct SimagicSpringEncoder {
    max_torque_nm: f32,
}

impl SimagicSpringEncoder {
    pub fn new(max_torque_nm: f32) -> Self {
        Self {
            max_torque_nm: max_torque_nm.max(0.01),
        }
    }

    /// Encode a spring effect command.
    ///
    /// Layout (10 bytes):
    /// - Byte 0: `0x12` (report ID)
    /// - Byte 1: effect block index
    /// - Bytes 2-3: spring strength (0-1000)
    /// - Bytes 4-5: current steering position (-32768 to +32767)
    /// - Bytes 6-7: center offset (-32768 to +32767)
    /// - Bytes 8-9: deadzone (0-1000)
    pub fn encode(
        &self,
        strength: u16,
        steering_position: i16,
        center_offset: i16,
        deadzone: u16,
        out: &mut [u8; SPRING_REPORT_LEN],
    ) -> usize {
        out.fill(0);
        out[0] = report_ids::SPRING_EFFECT;
        out[1] = 1; // effect block index

        let strength_bytes = strength.to_le_bytes();
        out[2] = strength_bytes[0];
        out[3] = strength_bytes[1];

        let steering_bytes = steering_position.to_le_bytes();
        out[4] = steering_bytes[0];
        out[5] = steering_bytes[1];

        let center_bytes = center_offset.to_le_bytes();
        out[6] = center_bytes[0];
        out[7] = center_bytes[1];

        let deadzone_bytes = deadzone.to_le_bytes();
        out[8] = deadzone_bytes[0];
        out[9] = deadzone_bytes[1];

        SPRING_REPORT_LEN
    }

    /// Encode a zero spring effect (disable).
    pub fn encode_zero(&self, out: &mut [u8; SPRING_REPORT_LEN]) -> usize {
        self.encode(0, 0, 0, 0, out)
    }
}

/// Encoder for Simagic damper effect FFB output reports.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct SimagicDamperEncoder {
    max_torque_nm: f32,
}

impl SimagicDamperEncoder {
    pub fn new(max_torque_nm: f32) -> Self {
        Self {
            max_torque_nm: max_torque_nm.max(0.01),
        }
    }

    /// Encode a damper effect command.
    ///
    /// Layout (8 bytes):
    /// - Byte 0: `0x13` (report ID)
    /// - Byte 1: effect block index
    /// - Bytes 2-3: damper strength (0-1000)
    /// - Bytes 4-5: current velocity (0-10000)
    /// - Bytes 6-7: reserved
    pub fn encode(&self, strength: u16, velocity: u16, out: &mut [u8; DAMPER_REPORT_LEN]) -> usize {
        out.fill(0);
        out[0] = report_ids::DAMPER_EFFECT;
        out[1] = 1;

        let strength_bytes = strength.to_le_bytes();
        out[2] = strength_bytes[0];
        out[3] = strength_bytes[1];

        let velocity_bytes = velocity.to_le_bytes();
        out[4] = velocity_bytes[0];
        out[5] = velocity_bytes[1];

        DAMPER_REPORT_LEN
    }

    /// Encode a zero damper effect (disable).
    pub fn encode_zero(&self, out: &mut [u8; DAMPER_REPORT_LEN]) -> usize {
        self.encode(0, 0, out)
    }
}

/// Encoder for Simagic friction effect FFB output reports.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct SimagicFrictionEncoder {
    max_torque_nm: f32,
}

impl SimagicFrictionEncoder {
    pub fn new(max_torque_nm: f32) -> Self {
        Self {
            max_torque_nm: max_torque_nm.max(0.01),
        }
    }

    /// Encode a friction effect command.
    ///
    /// Layout (10 bytes):
    /// - Byte 0: `0x14` (report ID)
    /// - Byte 1: effect block index
    /// - Bytes 2-3: friction coefficient (0-1000)
    /// - Bytes 4-5: current velocity (0-10000)
    /// - Bytes 6-9: reserved
    pub fn encode(
        &self,
        coefficient: u16,
        velocity: u16,
        out: &mut [u8; FRICTION_REPORT_LEN],
    ) -> usize {
        out.fill(0);
        out[0] = report_ids::FRICTION_EFFECT;
        out[1] = 1;

        let coeff_bytes = coefficient.to_le_bytes();
        out[2] = coeff_bytes[0];
        out[3] = coeff_bytes[1];

        let velocity_bytes = velocity.to_le_bytes();
        out[4] = velocity_bytes[0];
        out[5] = velocity_bytes[1];

        FRICTION_REPORT_LEN
    }

    /// Encode a zero friction effect (disable).
    pub fn encode_zero(&self, out: &mut [u8; FRICTION_REPORT_LEN]) -> usize {
        self.encode(0, 0, out)
    }
}

/// Convert torque (Nm) to Simagic magnitude units (±10000).
fn torque_to_magnitude(torque_nm: f32, max_torque_nm: f32) -> i16 {
    let normalized = (torque_nm / max_torque_nm).clamp(-1.0, 1.0);
    (normalized * 10_000.0) as i16
}

/// Build the rotation range setting report.
///
/// `degrees` is the desired full rotation range (e.g. 900, 1080, 1440).
pub fn build_rotation_range(degrees: u16) -> [u8; 8] {
    let [lsb, msb] = degrees.to_le_bytes();
    [
        report_ids::ROTATION_RANGE,
        0x00,
        lsb,
        msb,
        0x00,
        0x00,
        0x00,
        0x00,
    ]
}

/// Build the device gain setting report.
///
/// `gain` is the overall FFB gain (0x00–0xFF, 0 = 0%, 0xFF = 100%).
pub fn build_device_gain(gain: u8) -> [u8; 8] {
    [
        report_ids::DEVICE_GAIN,
        gain,
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
    ]
}

/// Build the LED control report.
///
/// `led_pattern` is the LED pattern to display (0-255).
/// The exact mapping depends on the wheelbase/rim combination.
pub fn build_led_report(led_pattern: u8) -> [u8; 8] {
    [
        report_ids::LED_CONTROL,
        led_pattern,
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
    ]
}

/// Build a sine wave FFB effect report.
///
/// Layout: 10 bytes
/// - Byte 0: `0x15` (report ID)
/// - Byte 1: effect block index
/// - Bytes 2-3: amplitude (0-1000)
/// - Bytes 4-5: frequency (0.1-20 Hz)
/// - Bytes 6-9: phase offset (0-360 degrees)
pub fn build_sine_effect(amplitude: u16, frequency: f32, phase: u16) -> [u8; 10] {
    let [amp_lsb, amp_msb] = amplitude.to_le_bytes();
    let freq_encoded = (frequency.clamp(0.1, 20.0) * 100.0) as u16;
    let [freq_lsb, freq_msb] = freq_encoded.to_le_bytes();
    let [phase_lsb, phase_msb] = phase.to_le_bytes();

    [
        report_ids::SINE_EFFECT,
        0x01,
        amp_lsb,
        amp_msb,
        freq_lsb,
        freq_msb,
        phase_lsb,
        phase_msb,
        0x00,
        0x00,
    ]
}

/// Build a square wave FFB effect report.
///
/// Layout: 10 bytes
/// - Byte 0: `0x16` (report ID)
/// - Byte 1: effect block index
/// - Bytes 2-3: amplitude (0-1000)
/// - Bytes 4-5: frequency (0.1-20 Hz)
/// - Bytes 6-9: duty cycle (0-100%)
pub fn build_square_effect(amplitude: u16, frequency: f32, duty_cycle: u16) -> [u8; 10] {
    let [amp_lsb, amp_msb] = amplitude.to_le_bytes();
    let freq_encoded = (frequency.clamp(0.1, 20.0) * 100.0) as u16;
    let [freq_lsb, freq_msb] = freq_encoded.to_le_bytes();
    let [duty_lsb, duty_msb] = duty_cycle.min(100).to_le_bytes();

    [
        report_ids::SQUARE_EFFECT,
        0x01,
        amp_lsb,
        amp_msb,
        freq_lsb,
        freq_msb,
        duty_lsb,
        duty_msb,
        0x00,
        0x00,
    ]
}

/// Build a triangle wave FFB effect report.
///
/// Layout: 10 bytes
/// - Byte 0: `0x17` (report ID)
/// - Byte 1: effect block index
/// - Bytes 2-3: amplitude (0-1000)
/// - Bytes 4-5: frequency (0.1-20 Hz)
/// - Bytes 6-9: reserved
pub fn build_triangle_effect(amplitude: u16, frequency: f32) -> [u8; 10] {
    let [amp_lsb, amp_msb] = amplitude.to_le_bytes();
    let freq_encoded = (frequency.clamp(0.1, 20.0) * 100.0) as u16;
    let [freq_lsb, freq_msb] = freq_encoded.to_le_bytes();

    [
        report_ids::TRIANGLE_EFFECT,
        0x01,
        amp_lsb,
        amp_msb,
        freq_lsb,
        freq_msb,
        0x00,
        0x00,
        0x00,
        0x00,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constant_force_encoder_positive() -> Result<(), Box<dyn std::error::Error>> {
        let enc = SimagicConstantForceEncoder::new(15.0);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode(7.5, &mut out);
        assert_eq!(out[0], 0x11, "report ID");
        assert_eq!(out[1], 1, "effect block index");
        // 7.5 / 15.0 = 0.5 normalized → 5000 magnitude
        let mag = i16::from_le_bytes([out[3], out[4]]);
        assert_eq!(mag, 5000);
        Ok(())
    }

    #[test]
    fn test_constant_force_encoder_negative() -> Result<(), Box<dyn std::error::Error>> {
        let enc = SimagicConstantForceEncoder::new(15.0);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode(-7.5, &mut out);
        let mag = i16::from_le_bytes([out[3], out[4]]);
        assert_eq!(mag, -5000);
        Ok(())
    }

    #[test]
    fn test_constant_force_encoder_zero() -> Result<(), Box<dyn std::error::Error>> {
        let enc = SimagicConstantForceEncoder::new(15.0);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode_zero(&mut out);
        let mag = i16::from_le_bytes([out[3], out[4]]);
        assert_eq!(mag, 0);
        Ok(())
    }

    #[test]
    fn test_constant_force_saturation() -> Result<(), Box<dyn std::error::Error>> {
        let enc = SimagicConstantForceEncoder::new(15.0);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode(100.0, &mut out);
        let mag = i16::from_le_bytes([out[3], out[4]]);
        assert_eq!(mag, 10000, "over-torque must saturate at +10000");
        enc.encode(-100.0, &mut out);
        let mag = i16::from_le_bytes([out[3], out[4]]);
        assert_eq!(mag, -10000, "over-torque must saturate at -10000");
        Ok(())
    }

    #[test]
    fn test_spring_encoder() -> Result<(), Box<dyn std::error::Error>> {
        let enc = SimagicSpringEncoder::new(15.0);
        let mut out = [0u8; SPRING_REPORT_LEN];
        enc.encode(500, 1000, 0, 50, &mut out);
        assert_eq!(out[0], 0x12, "report ID");
        assert_eq!(out[1], 1, "effect block index");
        // strength = 500
        assert_eq!(out[2], 0xF4);
        assert_eq!(out[3], 0x01);
        Ok(())
    }

    #[test]
    fn test_spring_encoder_zero() -> Result<(), Box<dyn std::error::Error>> {
        let enc = SimagicSpringEncoder::new(15.0);
        let mut out = [0u8; SPRING_REPORT_LEN];
        enc.encode_zero(&mut out);
        assert_eq!(out[0], 0x12);
        assert_eq!(out[2], 0);
        assert_eq!(out[3], 0);
        Ok(())
    }

    #[test]
    fn test_damper_encoder() -> Result<(), Box<dyn std::error::Error>> {
        let enc = SimagicDamperEncoder::new(15.0);
        let mut out = [0u8; DAMPER_REPORT_LEN];
        enc.encode(750, 5000, &mut out);
        assert_eq!(out[0], 0x13, "report ID");
        assert_eq!(out[1], 1);
        // strength = 750
        assert_eq!(out[2], 0xEE);
        assert_eq!(out[3], 0x02);
        // velocity = 5000
        assert_eq!(out[4], 0x88);
        assert_eq!(out[5], 0x13);
        Ok(())
    }

    #[test]
    fn test_friction_encoder() -> Result<(), Box<dyn std::error::Error>> {
        let enc = SimagicFrictionEncoder::new(15.0);
        let mut out = [0u8; FRICTION_REPORT_LEN];
        enc.encode(300, 2000, &mut out);
        assert_eq!(out[0], 0x14, "report ID");
        assert_eq!(out[1], 1);
        // coefficient = 300
        assert_eq!(out[2], 0x2C);
        assert_eq!(out[3], 0x01);
        // velocity = 2000
        assert_eq!(out[4], 0xD0);
        assert_eq!(out[5], 0x07);
        Ok(())
    }

    #[test]
    fn test_rotation_range_900() -> Result<(), Box<dyn std::error::Error>> {
        let r = build_rotation_range(900);
        assert_eq!(r[0], 0x20);
        assert_eq!(r[1], 0x00);
        // 900 = 0x0384; little-endian = [0x84, 0x03]
        assert_eq!(r[2], 0x84);
        assert_eq!(r[3], 0x03);
        Ok(())
    }

    #[test]
    fn test_rotation_range_1080() -> Result<(), Box<dyn std::error::Error>> {
        let r = build_rotation_range(1080);
        assert_eq!(r[2], 0x38);
        assert_eq!(r[3], 0x04);
        Ok(())
    }

    #[test]
    fn test_device_gain() -> Result<(), Box<dyn std::error::Error>> {
        let r = build_device_gain(0x80);
        assert_eq!(r[0], 0x21);
        assert_eq!(r[1], 0x80);
        Ok(())
    }

    #[test]
    fn test_led_report() -> Result<(), Box<dyn std::error::Error>> {
        let r = build_led_report(0xFF);
        assert_eq!(r[0], 0x30);
        assert_eq!(r[1], 0xFF);
        Ok(())
    }

    #[test]
    fn test_sine_effect() -> Result<(), Box<dyn std::error::Error>> {
        let r = build_sine_effect(500, 2.0, 90);
        assert_eq!(r[0], 0x15);
        // amplitude = 500
        assert_eq!(r[2], 0xF4);
        assert_eq!(r[3], 0x01);
        // frequency = 2.0 * 100 = 200
        assert_eq!(r[4], 0xC8);
        assert_eq!(r[5], 0x00);
        // phase = 90
        assert_eq!(r[6], 0x5A);
        assert_eq!(r[7], 0x00);
        Ok(())
    }

    #[test]
    fn test_square_effect() -> Result<(), Box<dyn std::error::Error>> {
        let r = build_square_effect(750, 5.0, 50);
        assert_eq!(r[0], 0x16);
        // amplitude = 750
        assert_eq!(r[2], 0xEE);
        assert_eq!(r[3], 0x02);
        // frequency = 5.0 * 100 = 500
        assert_eq!(r[4], 0xF4);
        assert_eq!(r[5], 0x01);
        // duty cycle = 50
        assert_eq!(r[6], 0x32);
        assert_eq!(r[7], 0x00);
        Ok(())
    }

    #[test]
    fn test_triangle_effect() -> Result<(), Box<dyn std::error::Error>> {
        let r = build_triangle_effect(300, 1.5);
        assert_eq!(r[0], 0x17);
        // amplitude = 300
        assert_eq!(r[2], 0x2C);
        assert_eq!(r[3], 0x01);
        // frequency = 1.5 * 100 = 150
        assert_eq!(r[4], 0x96);
        assert_eq!(r[5], 0x00);
        Ok(())
    }

    #[test]
    fn test_torque_to_magnitude() {
        // 0 torque = 0 magnitude
        assert_eq!(torque_to_magnitude(0.0, 10.0), 0);

        // half torque = 5000 magnitude
        assert_eq!(torque_to_magnitude(5.0, 10.0), 5000);

        // full torque = 10000 magnitude
        assert_eq!(torque_to_magnitude(10.0, 10.0), 10000);

        // negative torque
        assert_eq!(torque_to_magnitude(-10.0, 10.0), -10000);

        // saturation
        assert_eq!(torque_to_magnitude(20.0, 10.0), 10000);
        assert_eq!(torque_to_magnitude(-20.0, 10.0), -10000);
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(500))]

        #[test]
        fn prop_constant_force_encoder_always_valid_output(torque in -20.0f32..20.0f32) {
            let enc = SimagicConstantForceEncoder::new(15.0);
            let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
            let _ = enc.encode(torque, &mut out);
            prop_assert_eq!(out[0], 0x11);
            prop_assert!(out[1] > 0);
        }

        #[test]
        fn prop_spring_encoder_always_valid_output(
            strength in 0u16..=1000u16,
            steering in -32768i16..=32767i16,
            center in -32768i16..=32767i16,
            deadzone in 0u16..=1000u16,
        ) {
            let enc = SimagicSpringEncoder::new(15.0);
            let mut out = [0u8; SPRING_REPORT_LEN];
            let _ = enc.encode(strength, steering, center, deadzone, &mut out);
            prop_assert_eq!(out[0], 0x12);
        }

        #[test]
        fn prop_damper_encoder_always_valid_output(
            strength in 0u16..=1000u16,
            velocity in 0u16..=10000u16,
        ) {
            let enc = SimagicDamperEncoder::new(15.0);
            let mut out = [0u8; DAMPER_REPORT_LEN];
            let _ = enc.encode(strength, velocity, &mut out);
            prop_assert_eq!(out[0], 0x13);
        }

        #[test]
        fn prop_friction_encoder_always_valid_output(
            coefficient in 0u16..=1000u16,
            velocity in 0u16..=10000u16,
        ) {
            let enc = SimagicFrictionEncoder::new(15.0);
            let mut out = [0u8; FRICTION_REPORT_LEN];
            let _ = enc.encode(coefficient, velocity, &mut out);
            prop_assert_eq!(out[0], 0x14);
        }

        #[test]
        fn prop_rotation_range_valid(degrees in 180u16..=2880u16) {
            let report = build_rotation_range(degrees);
            prop_assert_eq!(report[0], 0x20);
            let reported_degrees = u16::from_le_bytes([report[2], report[3]]);
            prop_assert_eq!(reported_degrees, degrees);
        }

        #[test]
        fn prop_device_gain_valid(gain in 0u8..=255u8) {
            let report = build_device_gain(gain);
            prop_assert_eq!(report[0], 0x21);
            prop_assert_eq!(report[1], gain);
        }

        #[test]
        fn prop_led_report_valid(pattern in 0u8..=255u8) {
            let report = build_led_report(pattern);
            prop_assert_eq!(report[0], 0x30);
            prop_assert_eq!(report[1], pattern);
        }

        #[test]
        fn prop_sine_effect_valid(
            amplitude in 0u16..=1000u16,
            frequency in 0.1f32..=20.0f32,
            phase in 0u16..=360u16,
        ) {
            let report = build_sine_effect(amplitude, frequency, phase);
            prop_assert_eq!(report[0], 0x15);
        }

        #[test]
        fn prop_square_effect_valid(
            amplitude in 0u16..=1000u16,
            frequency in 0.1f32..=20.0f32,
            duty in 0u16..=100u16,
        ) {
            let report = build_square_effect(amplitude, frequency, duty);
            prop_assert_eq!(report[0], 0x16);
        }

        #[test]
        fn prop_triangle_effect_valid(
            amplitude in 0u16..=1000u16,
            frequency in 0.1f32..=20.0f32,
        ) {
            let report = build_triangle_effect(amplitude, frequency);
            prop_assert_eq!(report[0], 0x17);
        }
    }
}

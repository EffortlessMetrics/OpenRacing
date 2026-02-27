//! VRS DirectForce Pro HID output report encoding (FFB commands).
//!
//! All functions are pure and allocation-free.

#![deny(unsafe_op_in_unsafe_fn)]
#![deny(static_mut_refs)]

use crate::ids::report_ids;

/// Wire size of a VRS constant-force output report (PIDFF).
pub const CONSTANT_FORCE_REPORT_LEN: usize = 8;

/// Wire size of a VRS spring effect output report.
pub const SPRING_REPORT_LEN: usize = 10;

/// Wire size of a VRS damper effect output report.
pub const DAMPER_REPORT_LEN: usize = 8;

/// Wire size of a VRS friction effect output report.
pub const FRICTION_REPORT_LEN: usize = 10;

/// Encoder for VRS constant-force FFB output reports (PIDFF).
///
/// Converts a torque value in Newton-meters to the signed 16-bit VRS wire
/// format using PIDFF standard encoding.
#[derive(Debug, Clone, Copy)]
pub struct VrsConstantForceEncoder {
    max_torque_nm: f32,
}

impl VrsConstantForceEncoder {
    /// Create a new encoder with the specified maximum torque.
    pub fn new(max_torque_nm: f32) -> Self {
        Self {
            max_torque_nm: max_torque_nm.max(0.01),
        }
    }

    /// Encode a torque command (Newton-meters) into a constant-force output report.
    ///
    /// PIDFF Layout (8 bytes):
    /// - Byte 0: `0x11` (report ID)
    /// - Bytes 1-2: effect block index (little-endian, 1-based)
    /// - Bytes 3-4: signed magnitude, little-endian (±10000)
    /// - Bytes 5-7: reserved
    pub fn encode(&self, torque_nm: f32, out: &mut [u8; CONSTANT_FORCE_REPORT_LEN]) -> usize {
        out.fill(0);
        out[0] = report_ids::CONSTANT_FORCE;
        out[1] = 1;
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

/// Encoder for VRS spring effect FFB output reports (PIDFF Condition).
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct VrsSpringEncoder {
    max_torque_nm: f32,
}

impl VrsSpringEncoder {
    /// Create a new spring encoder with the specified maximum torque.
    pub fn new(max_torque_nm: f32) -> Self {
        Self {
            max_torque_nm: max_torque_nm.max(0.01),
        }
    }

    /// Encode a spring effect command.
    ///
    /// PIDFF Spring/Condition Layout (10 bytes):
    /// - Byte 0: `0x19` (report ID)
    /// - Byte 1: effect block index
    /// - Bytes 2-3: spring coefficient (0-10000)
    /// - Bytes 4-5: current steering position (signed, -32768 to +32767)
    /// - Bytes 6-7: center offset (signed, -32768 to +32767)
    /// - Bytes 8-9: deadzone (0-10000)
    pub fn encode(
        &self,
        coefficient: u16,
        steering_position: i16,
        center_offset: i16,
        deadzone: u16,
        out: &mut [u8; SPRING_REPORT_LEN],
    ) -> usize {
        out.fill(0);
        out[0] = report_ids::SPRING_EFFECT;
        out[1] = 1;

        let coeff_bytes = coefficient.to_le_bytes();
        out[2] = coeff_bytes[0];
        out[3] = coeff_bytes[1];

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

/// Encoder for VRS damper effect FFB output reports (PIDFF Condition).
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct VrsDamperEncoder {
    max_torque_nm: f32,
}

impl VrsDamperEncoder {
    /// Create a new damper encoder with the specified maximum torque.
    pub fn new(max_torque_nm: f32) -> Self {
        Self {
            max_torque_nm: max_torque_nm.max(0.01),
        }
    }

    /// Encode a damper effect command.
    ///
    /// PIDFF Damper Layout (8 bytes):
    /// - Byte 0: `0x1A` (report ID)
    /// - Byte 1: effect block index
    /// - Bytes 2-3: damper coefficient (0-10000)
    /// - Bytes 4-5: current velocity (0-10000)
    /// - Bytes 6-7: reserved
    pub fn encode(
        &self,
        coefficient: u16,
        velocity: u16,
        out: &mut [u8; DAMPER_REPORT_LEN],
    ) -> usize {
        out.fill(0);
        out[0] = report_ids::DAMPER_EFFECT;
        out[1] = 1;

        let coeff_bytes = coefficient.to_le_bytes();
        out[2] = coeff_bytes[0];
        out[3] = coeff_bytes[1];

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

/// Encoder for VRS friction effect FFB output reports (PIDFF Condition).
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct VrsFrictionEncoder {
    max_torque_nm: f32,
}

impl VrsFrictionEncoder {
    /// Create a new friction encoder with the specified maximum torque.
    pub fn new(max_torque_nm: f32) -> Self {
        Self {
            max_torque_nm: max_torque_nm.max(0.01),
        }
    }

    /// Encode a friction effect command.
    ///
    /// PIDFF Friction Layout (10 bytes):
    /// - Byte 0: `0x1B` (report ID)
    /// - Byte 1: effect block index
    /// - Bytes 2-3: friction coefficient (0-10000)
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

/// Convert torque (Nm) to VRS magnitude units (±10000).
#[inline]
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
        report_ids::SET_REPORT,
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
        report_ids::SET_REPORT,
        0x01,
        gain,
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
    ]
}

/// Build the FFB enable/disable control report.
///
/// `enable` is true to enable force feedback, false to disable.
pub fn build_ffb_enable(enable: bool) -> [u8; 8] {
    [
        report_ids::DEVICE_CONTROL,
        if enable { 0x01 } else { 0x00 },
        0x00,
        0x00,
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
        let enc = VrsConstantForceEncoder::new(20.0);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode(10.0, &mut out);
        assert_eq!(out[0], 0x11);
        assert_eq!(out[1], 1);
        let mag = i16::from_le_bytes([out[3], out[4]]);
        assert_eq!(mag, 5000);
        Ok(())
    }

    #[test]
    fn test_constant_force_encoder_negative() -> Result<(), Box<dyn std::error::Error>> {
        let enc = VrsConstantForceEncoder::new(20.0);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode(-10.0, &mut out);
        let mag = i16::from_le_bytes([out[3], out[4]]);
        assert_eq!(mag, -5000);
        Ok(())
    }

    #[test]
    fn test_constant_force_encoder_zero() -> Result<(), Box<dyn std::error::Error>> {
        let enc = VrsConstantForceEncoder::new(20.0);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode_zero(&mut out);
        let mag = i16::from_le_bytes([out[3], out[4]]);
        assert_eq!(mag, 0);
        Ok(())
    }

    #[test]
    fn test_constant_force_saturation() -> Result<(), Box<dyn std::error::Error>> {
        let enc = VrsConstantForceEncoder::new(20.0);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode(100.0, &mut out);
        let mag = i16::from_le_bytes([out[3], out[4]]);
        assert_eq!(mag, 10000);
        enc.encode(-100.0, &mut out);
        let mag = i16::from_le_bytes([out[3], out[4]]);
        assert_eq!(mag, -10000);
        Ok(())
    }

    #[test]
    fn test_spring_encoder() -> Result<(), Box<dyn std::error::Error>> {
        let enc = VrsSpringEncoder::new(20.0);
        let mut out = [0u8; SPRING_REPORT_LEN];
        enc.encode(5000, 1000, 0, 500, &mut out);
        assert_eq!(out[0], 0x19);
        assert_eq!(out[1], 1);
        Ok(())
    }

    #[test]
    fn test_spring_encoder_zero() -> Result<(), Box<dyn std::error::Error>> {
        let enc = VrsSpringEncoder::new(20.0);
        let mut out = [0u8; SPRING_REPORT_LEN];
        enc.encode_zero(&mut out);
        assert_eq!(out[0], 0x19);
        assert_eq!(out[2], 0);
        Ok(())
    }

    #[test]
    fn test_damper_encoder() -> Result<(), Box<dyn std::error::Error>> {
        let enc = VrsDamperEncoder::new(20.0);
        let mut out = [0u8; DAMPER_REPORT_LEN];
        enc.encode(7500, 5000, &mut out);
        assert_eq!(out[0], 0x1A);
        assert_eq!(out[1], 1);
        Ok(())
    }

    #[test]
    fn test_friction_encoder() -> Result<(), Box<dyn std::error::Error>> {
        let enc = VrsFrictionEncoder::new(20.0);
        let mut out = [0u8; FRICTION_REPORT_LEN];
        enc.encode(3000, 2000, &mut out);
        assert_eq!(out[0], 0x1B);
        assert_eq!(out[1], 1);
        Ok(())
    }

    #[test]
    fn test_rotation_range_900() -> Result<(), Box<dyn std::error::Error>> {
        let r = build_rotation_range(900);
        assert_eq!(r[0], 0x0C);
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
        assert_eq!(r[0], 0x0C);
        assert_eq!(r[2], 0x80);
        Ok(())
    }

    #[test]
    fn test_ffb_enable() -> Result<(), Box<dyn std::error::Error>> {
        let r = build_ffb_enable(true);
        assert_eq!(r[0], 0x0B);
        assert_eq!(r[1], 0x01);

        let r = build_ffb_enable(false);
        assert_eq!(r[1], 0x00);
        Ok(())
    }

    #[test]
    fn test_torque_to_magnitude() {
        assert_eq!(torque_to_magnitude(0.0, 10.0), 0);
        assert_eq!(torque_to_magnitude(5.0, 10.0), 5000);
        assert_eq!(torque_to_magnitude(10.0, 10.0), 10000);
        assert_eq!(torque_to_magnitude(-10.0, 10.0), -10000);
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
        fn prop_constant_force_encoder_always_valid_output(torque in -30.0f32..30.0f32) {
            let enc = VrsConstantForceEncoder::new(20.0);
            let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
            let _ = enc.encode(torque, &mut out);
            prop_assert_eq!(out[0], 0x11);
            prop_assert!(out[1] > 0);
        }

        #[test]
        fn prop_spring_encoder_always_valid_output(
            coefficient in 0u16..=10000u16,
            steering in -32768i16..=32767i16,
            center in -32768i16..=32767i16,
            deadzone in 0u16..=10000u16,
        ) {
            let enc = VrsSpringEncoder::new(20.0);
            let mut out = [0u8; SPRING_REPORT_LEN];
            let _ = enc.encode(coefficient, steering, center, deadzone, &mut out);
            prop_assert_eq!(out[0], 0x19);
        }

        #[test]
        fn prop_damper_encoder_always_valid_output(
            coefficient in 0u16..=10000u16,
            velocity in 0u16..=10000u16,
        ) {
            let enc = VrsDamperEncoder::new(20.0);
            let mut out = [0u8; DAMPER_REPORT_LEN];
            let _ = enc.encode(coefficient, velocity, &mut out);
            prop_assert_eq!(out[0], 0x1A);
        }

        #[test]
        fn prop_friction_encoder_always_valid_output(
            coefficient in 0u16..=10000u16,
            velocity in 0u16..=10000u16,
        ) {
            let enc = VrsFrictionEncoder::new(20.0);
            let mut out = [0u8; FRICTION_REPORT_LEN];
            let _ = enc.encode(coefficient, velocity, &mut out);
            prop_assert_eq!(out[0], 0x1B);
        }

        #[test]
        fn prop_rotation_range_valid(degrees in 180u16..=2880u16) {
            let report = build_rotation_range(degrees);
            prop_assert_eq!(report[0], 0x0C);
            let reported_degrees = u16::from_le_bytes([report[2], report[3]]);
            prop_assert_eq!(reported_degrees, degrees);
        }

        #[test]
        fn prop_device_gain_valid(gain in 0u8..=255u8) {
            let report = build_device_gain(gain);
            prop_assert_eq!(report[0], 0x0C);
            prop_assert_eq!(report[2], gain);
        }
    }
}

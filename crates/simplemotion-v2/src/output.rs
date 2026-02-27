//! SimpleMotion V2 output report encoding (torque commands and control).
//!
//! This module provides encoders for sending torque commands and control
//! messages to SimpleMotion V2 devices over USB HID.

#![deny(unsafe_op_in_unsafe_fn)]
#![deny(static_mut_refs)]

use crate::commands::{SmCommand, SmCommandType, encode_command};

pub const TORQUE_COMMAND_LEN: usize = 15;
pub const FEEDBACK_REPORT_LEN: usize = 64;
pub const SETPARAM_REPORT_LEN: usize = 15;
pub const STATUS_REPORT_LEN: usize = 15;

#[derive(Debug, Clone, Copy)]
pub struct TorqueCommandEncoder {
    max_torque_nm: f32,
    torque_constant: f32,
    sequence: u8,
}

impl TorqueCommandEncoder {
    pub fn new(max_torque_nm: f32) -> Self {
        Self {
            max_torque_nm: max_torque_nm.max(0.01),
            torque_constant: 0.1,
            sequence: 0,
        }
    }

    pub fn with_torque_constant(mut self, constant: f32) -> Self {
        self.torque_constant = constant;
        self
    }

    pub fn encode(&mut self, torque_nm: f32, out: &mut [u8; TORQUE_COMMAND_LEN]) -> usize {
        let torque_q8_8 = torque_to_q8_8(torque_nm, self.max_torque_nm);
        let cmd =
            SmCommand::new(self.sequence, SmCommandType::SetTorque).with_data(torque_q8_8 as i32);
        self.sequence = self.sequence.wrapping_add(1);
        encode_command(&cmd, out).unwrap_or(0)
    }

    pub fn encode_with_velocity(
        &mut self,
        torque_nm: f32,
        velocity_rpm: f32,
        out: &mut [u8; TORQUE_COMMAND_LEN],
    ) -> usize {
        let torque_q8_8 = torque_to_q8_8(torque_nm, self.max_torque_nm);
        let velocity_q8_8 = (velocity_rpm * 256.0 / 60.0) as i16;
        let combined = ((torque_q8_8 as i32) << 16) | (velocity_q8_8 as i32 & 0xFFFF);
        let cmd = SmCommand::new(self.sequence, SmCommandType::SetTorque).with_data(combined);
        self.sequence = self.sequence.wrapping_add(1);
        encode_command(&cmd, out).unwrap_or(0)
    }

    pub fn encode_zero(&mut self, out: &mut [u8; TORQUE_COMMAND_LEN]) -> usize {
        self.encode(0.0, out)
    }

    pub fn sequence(&self) -> u8 {
        self.sequence
    }
}

#[inline]
fn torque_to_q8_8(torque_nm: f32, max_torque_nm: f32) -> i16 {
    let normalized = (torque_nm / max_torque_nm).clamp(-1.0, 1.0);
    (normalized * 32767.0) as i16
}

pub fn build_set_zero_position(seq: u8) -> [u8; TORQUE_COMMAND_LEN] {
    let cmd = SmCommand::new(seq, SmCommandType::SetZero).with_data(0);
    let mut out = [0u8; TORQUE_COMMAND_LEN];
    encode_command(&cmd, &mut out).expect("set zero position command should never fail");
    out
}

pub fn build_device_enable(enable: bool, seq: u8) -> [u8; TORQUE_COMMAND_LEN] {
    let value = if enable { 1 } else { 0 };
    let cmd = SmCommand::new(seq, SmCommandType::SetParameter).with_param(0x1001, value);
    let mut out = [0u8; TORQUE_COMMAND_LEN];
    encode_command(&cmd, &mut out).expect("device enable command should never fail");
    out
}

pub fn build_get_parameter(param_addr: u16, seq: u8) -> [u8; TORQUE_COMMAND_LEN] {
    let cmd = SmCommand::new(seq, SmCommandType::GetParameter).with_param(param_addr, 0);
    let mut out = [0u8; TORQUE_COMMAND_LEN];
    encode_command(&cmd, &mut out).expect("get parameter command should never fail");
    out
}

pub fn build_set_parameter(param_addr: u16, value: i32, seq: u8) -> [u8; TORQUE_COMMAND_LEN] {
    let cmd = SmCommand::new(seq, SmCommandType::SetParameter).with_param(param_addr, value);
    let mut out = [0u8; TORQUE_COMMAND_LEN];
    encode_command(&cmd, &mut out).expect("set parameter command should never fail");
    out
}

pub fn build_get_status(seq: u8) -> [u8; TORQUE_COMMAND_LEN] {
    let cmd = SmCommand::new(seq, SmCommandType::GetStatus);
    let mut out = [0u8; TORQUE_COMMAND_LEN];
    encode_command(&cmd, &mut out).expect("get status command should never fail");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_torque_encoder_positive() {
        let mut enc = TorqueCommandEncoder::new(20.0);
        let mut out = [0u8; TORQUE_COMMAND_LEN];
        enc.encode(10.0, &mut out);
        assert_eq!(out[0], 0x01);
        assert_eq!(out[2], 0x10);
    }

    #[test]
    fn test_torque_encoder_negative() {
        let mut enc = TorqueCommandEncoder::new(20.0);
        let mut out = [0u8; TORQUE_COMMAND_LEN];
        enc.encode(-10.0, &mut out);
        assert_eq!(out[0], 0x01);
    }

    #[test]
    fn test_torque_encoder_zero() {
        let mut enc = TorqueCommandEncoder::new(20.0);
        let mut out = [0u8; TORQUE_COMMAND_LEN];
        enc.encode_zero(&mut out);
        let cmd_type = u16::from_le_bytes([out[2], out[3]]);
        assert_eq!(cmd_type, 0x0010);
    }

    #[test]
    fn test_torque_encoder_saturation() {
        let mut enc = TorqueCommandEncoder::new(20.0);
        let mut out = [0u8; TORQUE_COMMAND_LEN];
        enc.encode(100.0, &mut out);
        enc.encode(-100.0, &mut out);
    }

    #[test]
    fn test_torque_encoder_with_velocity() {
        let mut enc = TorqueCommandEncoder::new(20.0);
        let mut out = [0u8; TORQUE_COMMAND_LEN];
        enc.encode_with_velocity(5.0, 100.0, &mut out);
        assert_eq!(out[0], 0x01);
    }

    #[test]
    fn test_torque_encoder_sequence() {
        let mut enc = TorqueCommandEncoder::new(20.0);
        let mut out = [0u8; TORQUE_COMMAND_LEN];
        enc.encode(0.0, &mut out);
        let seq1 = out[1];
        enc.encode(0.0, &mut out);
        let seq2 = out[1];
        assert_eq!(seq1.wrapping_add(1), seq2);
    }

    #[test]
    fn test_build_set_zero_position() {
        let out = build_set_zero_position(0);
        assert_eq!(out[0], 0x01);
        assert_eq!(out[2], 0x13);
    }

    #[test]
    fn test_build_device_enable() {
        let out = build_device_enable(true, 0);
        assert_eq!(out[0], 0x01);
        assert_eq!(out[2], 0x02);
        assert_eq!(out[4], 0x01);
        assert_eq!(out[5], 0x10);

        let out = build_device_enable(false, 0);
        assert_eq!(out[6], 0x00);
    }

    #[test]
    fn test_torque_to_q8_8() {
        assert_eq!(torque_to_q8_8(0.0, 10.0), 0);
        assert_eq!(torque_to_q8_8(5.0, 10.0), 16383);
        assert_eq!(torque_to_q8_8(10.0, 10.0), 32767);
        assert_eq!(torque_to_q8_8(-10.0, 10.0), -32767);
        assert_eq!(torque_to_q8_8(20.0, 10.0), 32767);
        assert_eq!(torque_to_q8_8(-20.0, 10.0), -32767);
    }

    #[test]
    fn test_torque_encoder_custom_constant() {
        let mut enc = TorqueCommandEncoder::new(35.0).with_torque_constant(0.15);
        let mut out = [0u8; TORQUE_COMMAND_LEN];
        enc.encode(17.5, &mut out);
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(500))]

        #[test]
        fn prop_torque_encoder_always_valid_output(torque in -30.0f32..30.0f32) {
            let mut enc = TorqueCommandEncoder::new(20.0);
            let mut out = [0u8; TORQUE_COMMAND_LEN];
            let _ = enc.encode(torque, &mut out);
            prop_assert_eq!(out[0], 0x01);
        }

        #[test]
        fn prop_torque_encoder_with_velocity_valid(
            torque in -30.0f32..30.0f32,
            velocity in -5000.0f32..5000.0f32,
        ) {
            let mut enc = TorqueCommandEncoder::new(20.0);
            let mut out = [0u8; TORQUE_COMMAND_LEN];
            let _ = enc.encode_with_velocity(torque, velocity, &mut out);
            prop_assert_eq!(out[0], 0x01);
        }

        #[test]
        fn prop_build_device_enable_valid(enable in proptest::bool::ANY, seq in 0u8..=255) {
            let report = build_device_enable(enable, seq);
            prop_assert_eq!(report[0], 0x01);
            prop_assert_eq!(report[1], seq);
        }

        #[test]
        fn prop_build_set_zero_position_valid(seq in 0u8..=255) {
            let report = build_set_zero_position(seq);
            prop_assert_eq!(report[0], 0x01);
            prop_assert_eq!(report[1], seq);
            prop_assert_eq!(report[2], 0x13);
        }
    }
}

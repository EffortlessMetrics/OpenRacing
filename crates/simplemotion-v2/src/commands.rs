//! SimpleMotion V2 command encoding and decoding.
//!
//! SimpleMotion V2 uses a binary protocol with the following structure:
//! - Byte 0: Report ID (0x01 for commands, 0x02 for feedback)
//! - Byte 1: Sequence number
//! - Bytes 2-3: Command type (little-endian)
//! - Bytes 4-5: Parameter address (for parameter commands)
//! - Bytes 6-9: Parameter value (for set commands)
//! - Bytes 10-13: Timestamp
//! - Byte 14: CRC8

#![deny(unsafe_op_in_unsafe_fn)]
#![deny(static_mut_refs)]

use crate::error::{SmError, SmResult};

pub const MOTOR_POLE_PAIRS: u8 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmCommandType {
    GetParameter,
    SetParameter,
    GetStatus,
    SetTorque,
    SetVelocity,
    SetPosition,
    SetZero,
    Reset,
}

impl SmCommandType {
    pub fn from_u16(val: u16) -> Option<Self> {
        match val {
            0x0001 => Some(Self::GetParameter),
            0x0002 => Some(Self::SetParameter),
            0x0003 => Some(Self::GetStatus),
            0x0010 => Some(Self::SetTorque),
            0x0011 => Some(Self::SetVelocity),
            0x0012 => Some(Self::SetPosition),
            0x0013 => Some(Self::SetZero),
            0xFFFF => Some(Self::Reset),
            _ => None,
        }
    }

    pub fn to_u16(self) -> u16 {
        match self {
            Self::GetParameter => 0x0001,
            Self::SetParameter => 0x0002,
            Self::GetStatus => 0x0003,
            Self::SetTorque => 0x0010,
            Self::SetVelocity => 0x0011,
            Self::SetPosition => 0x0012,
            Self::SetZero => 0x0013,
            Self::Reset => 0xFFFF,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SmStatus {
    Ok,
    Error,
    Busy,
    NotReady,
    #[default]
    Unknown,
}

impl SmStatus {
    pub fn from_u8(val: u8) -> Self {
        match val {
            0 => Self::Ok,
            1 => Self::Error,
            2 => Self::Busy,
            3 => Self::NotReady,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SmCommand {
    pub seq: u8,
    pub cmd_type: SmCommandType,
    pub param_addr: Option<u16>,
    pub param_value: Option<i32>,
    pub data: Option<i32>,
}

impl SmCommand {
    pub fn new(seq: u8, cmd_type: SmCommandType) -> Self {
        Self {
            seq,
            cmd_type,
            param_addr: None,
            param_value: None,
            data: None,
        }
    }

    pub fn with_param(mut self, addr: u16, value: i32) -> Self {
        self.param_addr = Some(addr);
        self.param_value = Some(value);
        self
    }

    pub fn with_data(mut self, data: i32) -> Self {
        self.data = Some(data);
        self
    }
}

fn compute_crc8(data: &[u8]) -> u8 {
    let mut crc: u8 = 0x00;
    for &byte in data {
        crc ^= byte;
        for _ in 0..8 {
            if crc & 0x80 != 0 {
                crc = (crc << 1) ^ 0x07;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}

pub fn encode_command(cmd: &SmCommand, out: &mut [u8]) -> SmResult<usize> {
    if out.len() < 15 {
        return Err(SmError::InvalidLength {
            expected: 15,
            actual: out.len(),
        });
    }

    out.fill(0);
    out[0] = 0x01;
    out[1] = cmd.seq;

    let cmd_type_bytes = cmd.cmd_type.to_u16().to_le_bytes();
    out[2] = cmd_type_bytes[0];
    out[3] = cmd_type_bytes[1];

    if let Some(addr) = cmd.param_addr {
        let addr_bytes = addr.to_le_bytes();
        out[4] = addr_bytes[0];
        out[5] = addr_bytes[1];
    }

    if let Some(value) = cmd.param_value {
        let value_bytes = value.to_le_bytes();
        out[6] = value_bytes[0];
        out[7] = value_bytes[1];
        out[8] = value_bytes[2];
        out[9] = value_bytes[3];
    }

    if let Some(data) = cmd.data {
        let data_bytes = data.to_le_bytes();
        out[10] = data_bytes[0];
        out[11] = data_bytes[1];
        out[12] = data_bytes[2];
        out[13] = data_bytes[3];
    }

    let crc = compute_crc8(&out[..14]);
    out[14] = crc;

    Ok(15)
}

pub fn decode_command(data: &[u8]) -> SmResult<SmCommand> {
    if data.len() < 15 {
        return Err(SmError::InvalidLength {
            expected: 15,
            actual: data.len(),
        });
    }

    let computed_crc = compute_crc8(&data[..14]);
    let reported_crc = data[14];
    if computed_crc != reported_crc {
        return Err(SmError::CrcMismatch {
            expected: computed_crc,
            actual: reported_crc,
        });
    }

    let seq = data[1];
    let cmd_type_val = u16::from_le_bytes([data[2], data[3]]);
    let cmd_type = SmCommandType::from_u16(cmd_type_val)
        .ok_or(SmError::InvalidCommandType(cmd_type_val as u8))?;

    let param_addr = u16::from_le_bytes([data[4], data[5]]);
    let param_value = i32::from_le_bytes([data[6], data[7], data[8], data[9]]);
    let data_val = i32::from_le_bytes([data[10], data[11], data[12], data[13]]);

    Ok(SmCommand {
        seq,
        cmd_type,
        param_addr: Some(param_addr),
        param_value: Some(param_value),
        data: Some(data_val),
    })
}

pub fn build_set_torque_command(torque_q8_8: i16, seq: u8) -> [u8; 15] {
    let cmd = SmCommand::new(seq, SmCommandType::SetTorque).with_data(torque_q8_8 as i32);
    let mut out = [0u8; 15];
    encode_command(&cmd, &mut out).expect("set torque command should never fail");
    out
}

pub fn build_set_torque_command_with_velocity(
    torque_q8_8: i16,
    velocity_q8_8: i16,
    seq: u8,
) -> [u8; 15] {
    let torque = (torque_q8_8 as i32) << 16 | (velocity_q8_8 as i32 & 0xFFFF);
    let cmd = SmCommand::new(seq, SmCommandType::SetTorque).with_data(torque);
    let mut out = [0u8; 15];
    encode_command(&cmd, &mut out).expect("set torque command with velocity should never fail");
    out
}

pub fn build_get_parameter(param_addr: u16, seq: u8) -> [u8; 15] {
    let cmd = SmCommand::new(seq, SmCommandType::GetParameter).with_param(param_addr, 0);
    let mut out = [0u8; 15];
    encode_command(&cmd, &mut out).expect("get parameter command should never fail");
    out
}

pub fn build_set_parameter(param_addr: u16, value: i32, seq: u8) -> [u8; 15] {
    let cmd = SmCommand::new(seq, SmCommandType::SetParameter).with_param(param_addr, value);
    let mut out = [0u8; 15];
    encode_command(&cmd, &mut out).expect("set parameter command should never fail");
    out
}

pub fn build_get_status(seq: u8) -> [u8; 15] {
    let cmd = SmCommand::new(seq, SmCommandType::GetStatus);
    let mut out = [0u8; 15];
    encode_command(&cmd, &mut out).expect("get status command should never fail");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_type_roundtrip() {
        let types = [
            SmCommandType::GetParameter,
            SmCommandType::SetParameter,
            SmCommandType::GetStatus,
            SmCommandType::SetTorque,
            SmCommandType::SetVelocity,
            SmCommandType::SetPosition,
            SmCommandType::SetZero,
            SmCommandType::Reset,
        ];

        for cmd_type in types {
            let val = cmd_type.to_u16();
            let recovered = SmCommandType::from_u16(val);
            assert_eq!(recovered, Some(cmd_type));
        }
    }

    #[test]
    fn test_encode_set_torque() {
        let cmd = SmCommand::new(0x05, SmCommandType::SetTorque).with_data(1000);
        let mut out = [0u8; 15];
        let len = encode_command(&cmd, &mut out).expect("operation failed");
        assert_eq!(len, 15);
        assert_eq!(out[0], 0x01);
        assert_eq!(out[1], 0x05);
        assert_eq!(out[2], 0x10);
        assert_eq!(out[3], 0x00);
    }

    #[test]
    fn test_decode_set_torque() {
        let cmd = SmCommand::new(0x05, SmCommandType::SetTorque).with_data(1000);
        let mut out = [0u8; 15];
        encode_command(&cmd, &mut out).expect("operation failed");
        let decoded = decode_command(&out).expect("operation failed");
        assert_eq!(decoded.seq, 0x05);
        assert_eq!(decoded.cmd_type, SmCommandType::SetTorque);
    }

    #[test]
    fn test_encode_crc() {
        let cmd = SmCommand::new(0, SmCommandType::GetStatus);
        let mut out = [0u8; 15];
        encode_command(&cmd, &mut out).expect("operation failed");
        assert_ne!(out[14], 0);
    }

    #[test]
    fn test_invalid_length_encode() {
        let cmd = SmCommand::new(0, SmCommandType::GetStatus);
        let mut out = [0u8; 10];
        let result = encode_command(&cmd, &mut out);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_length_decode() {
        let data = vec![0u8; 10];
        let result = decode_command(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_build_set_torque_command() {
        let out = build_set_torque_command(2560, 0);
        assert_eq!(out[0], 0x01);
        assert_eq!(out[1], 0x00);
        assert_eq!(out[2], 0x10);
    }

    #[test]
    fn test_build_set_torque_with_velocity() {
        let out = build_set_torque_command_with_velocity(2560, 1280, 0);
        assert_eq!(out[0], 0x01);
        assert_eq!(out[2], 0x10);
    }

    #[test]
    fn test_build_get_parameter() {
        let out = build_get_parameter(0x1001, 0);
        assert_eq!(out[0], 0x01);
        assert_eq!(out[2], 0x01);
        assert_eq!(out[3], 0x00);
    }

    #[test]
    fn test_build_set_parameter() {
        let out = build_set_parameter(0x1001, 1000, 0);
        assert_eq!(out[0], 0x01);
        assert_eq!(out[2], 0x02);
    }

    #[test]
    fn test_build_get_status() {
        let out = build_get_status(0);
        assert_eq!(out[0], 0x01);
        assert_eq!(out[2], 0x03);
    }

    #[test]
    fn test_crc_mismatch() {
        let mut out = [0u8; 15];
        out[0] = 0x01;
        out[1] = 0;
        out[2] = 0x03;
        out[14] = 0x00;
        let result = decode_command(&out);
        assert!(matches!(result, Err(SmError::CrcMismatch { .. })));
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(500))]

        #[test]
        fn prop_encode_decode_roundtrip(seq in 0u8..=255, torque in -1000i32..=1000i32) {
            let cmd = SmCommand::new(seq, SmCommandType::SetTorque).with_data(torque);
            let mut out = [0u8; 15];
            encode_command(&cmd, &mut out).expect("operation failed");
            let decoded = decode_command(&out).expect("operation failed");
            prop_assert_eq!(decoded.seq, seq);
            prop_assert_eq!(decoded.cmd_type, SmCommandType::SetTorque);
        }

        #[test]
        fn prop_encode_command_valid(seq in 0u8..=255) {
            let cmd = SmCommand::new(seq, SmCommandType::GetStatus);
            let mut out = [0u8; 15];
            let result = encode_command(&cmd, &mut out);
            prop_assert!(result.is_ok());
            prop_assert_eq!(out[0], 0x01);
            prop_assert_eq!(out[1], seq);
        }

        #[test]
        fn prop_decode_command_invalid_crc(ref data in any::<[u8; 15]>()) {
            let mut data = *data;
            data[14] = 0x00;
            let result = decode_command(&data);
            if result.is_ok() {
                let computed = compute_crc8(&data[..14]);
                prop_assert_eq!(computed, 0x00);
            }
        }

        #[test]
        fn prop_command_type_from_u16_valid(cmd_val in 0u16..=10u16) {
            let cmd_type_opt = SmCommandType::from_u16(cmd_val);
            if let Some(cmd_type) = cmd_type_opt {
                prop_assert_eq!(cmd_type.to_u16(), cmd_val);
            }
        }
    }
}

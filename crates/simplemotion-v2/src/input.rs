//! SimpleMotion V2 feedback report parsing.
//!
//! Feedback reports contain motor position, velocity, torque, and status information.

#![deny(unsafe_op_in_unsafe_fn)]
#![deny(static_mut_refs)]

use crate::commands::SmStatus as CmdStatus;
use crate::error::{SmError, SmResult};

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SmMotorFeedback {
    pub position: i32,
    pub velocity: i32,
    pub torque: i16,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SmFeedbackState {
    pub seq: u8,
    pub status: CmdStatus,
    pub motor: SmMotorFeedback,
    pub bus_voltage: u16,
    pub motor_current: i16,
    pub temperature: i8,
    pub connected: bool,
}

impl SmFeedbackState {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn position_degrees(&self, encoder_cpr: u32) -> f32 {
        let position = self.motor.position as f32;
        position / (encoder_cpr as f32) * 360.0
    }

    pub fn velocity_rpm(&self, encoder_cpr: u32) -> f32 {
        let velocity = self.motor.velocity as f32;
        velocity / (encoder_cpr as f32) * 60.0
    }

    pub fn torque_nm(&self, torque_constant: f32) -> f32 {
        self.motor.torque as f32 * torque_constant / 256.0
    }
}

pub fn parse_feedback_report(data: &[u8]) -> SmResult<SmFeedbackState> {
    if data.len() < 64 {
        return Err(SmError::InvalidLength {
            expected: 64,
            actual: data.len(),
        });
    }

    if data[0] != 0x02 {
        return Err(SmError::InvalidCommandType(data[0]));
    }

    let seq = data[1];
    let status = CmdStatus::from_u8(data[2]);

    let position = i32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    let velocity = i32::from_le_bytes([data[8], data[9], data[10], data[11]]);
    let torque = i16::from_le_bytes([data[12], data[13]]);

    let bus_voltage = u16::from_le_bytes([data[14], data[15]]);
    let motor_current = i16::from_le_bytes([data[16], data[17]]);
    let temperature = data[18] as i8;

    let connected = data[4] != 0xFF || data[5] != 0xFF;

    Ok(SmFeedbackState {
        seq,
        status,
        motor: SmMotorFeedback {
            position,
            velocity,
            torque,
        },
        bus_voltage,
        motor_current,
        temperature,
        connected,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_feedback_report_minimal() {
        let mut data = vec![0u8; 64];
        data[0] = 0x02;
        data[1] = 0x05;
        data[2] = 0x00;

        let state = parse_feedback_report(&data).unwrap();
        assert_eq!(state.seq, 0x05);
        assert_eq!(state.status, CmdStatus::Ok);
        assert_eq!(state.motor.position, 0);
        assert_eq!(state.motor.velocity, 0);
        assert_eq!(state.motor.torque, 0);
        assert!(state.connected);
    }

    #[test]
    fn test_parse_feedback_report_full() {
        let mut data = vec![0u8; 64];
        data[0] = 0x02;
        data[1] = 0x10;
        data[2] = 0x00;

        data[4] = 0x00;
        data[5] = 0x01;
        data[6] = 0x00;
        data[7] = 0x00;

        data[8] = 0x00;
        data[9] = 0x02;
        data[10] = 0x00;
        data[11] = 0x00;

        data[12] = 0x00;
        data[13] = 0x10;

        data[14] = 0xE8;
        data[15] = 0x03;

        data[16] = 0x00;
        data[17] = 0x20;

        data[18] = 0x32;

        let state = parse_feedback_report(&data).unwrap();
        assert_eq!(state.seq, 0x10);
        assert_eq!(state.motor.position, 0x100);
        assert_eq!(state.motor.velocity, 0x200);
        assert_eq!(state.motor.torque, 0x1000);
        assert_eq!(state.bus_voltage, 1000);
        assert_eq!(state.motor_current, 0x2000);
        assert_eq!(state.temperature, 50);
    }

    #[test]
    fn test_parse_feedback_report_invalid_id() {
        let data = vec![0u8; 64];
        let result = parse_feedback_report(&data);
        assert!(matches!(result, Err(SmError::InvalidCommandType(0))));
    }

    #[test]
    fn test_parse_feedback_report_too_short() {
        let data = vec![0u8; 32];
        let result = parse_feedback_report(&data);
        assert!(matches!(result, Err(SmError::InvalidLength { .. })));
    }

    #[test]
    fn test_position_degrees() {
        let state = SmFeedbackState {
            motor: SmMotorFeedback {
                position: 14400,
                ..Default::default()
            },
            ..Default::default()
        };

        let degrees = state.position_degrees(14400);
        assert!((degrees - 360.0).abs() < 0.1);
    }

    #[test]
    fn test_velocity_rpm() {
        let state = SmFeedbackState {
            motor: SmMotorFeedback {
                velocity: 14400,
                ..Default::default()
            },
            ..Default::default()
        };

        let rpm = state.velocity_rpm(14400);
        assert!((rpm - 60.0).abs() < 0.1);
    }

    #[test]
    fn test_torque_nm() {
        let state = SmFeedbackState {
            motor: SmMotorFeedback {
                torque: 256,
                ..Default::default()
            },
            ..Default::default()
        };

        let torque = state.torque_nm(0.1);
        assert!((torque - 0.1).abs() < 0.01);
    }

    #[test]
    fn test_status_from_u8() {
        assert_eq!(CmdStatus::from_u8(0), CmdStatus::Ok);
        assert_eq!(CmdStatus::from_u8(1), CmdStatus::Error);
        assert_eq!(CmdStatus::from_u8(2), CmdStatus::Busy);
        assert_eq!(CmdStatus::from_u8(3), CmdStatus::NotReady);
        assert_eq!(CmdStatus::from_u8(255), CmdStatus::Unknown);
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(500))]

        #[test]
        fn prop_parse_feedback_report_arbitrary_data(ref data in any::<Vec<u8>>()) {
            let _ = parse_feedback_report(data);
        }

        #[test]
        fn prop_position_degrees_always_valid(position in i32::MIN..=i32::MAX, encoder_cpr in 1000u32..=20000u32) {
            let state = SmFeedbackState {
                motor: SmMotorFeedback { position, ..Default::default() },
                ..Default::default()
            };
            let degrees = state.position_degrees(encoder_cpr);
            prop_assert!(degrees.is_finite());
        }

        #[test]
        fn prop_velocity_rpm_always_valid(velocity in i32::MIN..=i32::MAX, encoder_cpr in 1000u32..=20000u32) {
            let state = SmFeedbackState {
                motor: SmMotorFeedback { velocity, ..Default::default() },
                ..Default::default()
            };
            let rpm = state.velocity_rpm(encoder_cpr);
            prop_assert!(rpm.is_finite());
        }
    }
}

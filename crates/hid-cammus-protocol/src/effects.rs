//! Standard USB HID PID effect reports for Cammus wheelbases.
//!
//! The Cammus C5 and C12 wheelbases use standard USB HID PID for force
//! feedback on Linux (via `hid-universal-pidff.c`). This module provides
//! allocation-free PIDFF report encoders complementing the direct torque
//! streaming in `direct.rs`.
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

/// Standard PIDFF report IDs.
pub mod report_ids {
    pub const SET_EFFECT: u8 = 0x01;
    pub const SET_ENVELOPE: u8 = 0x02;
    pub const SET_CONDITION: u8 = 0x03;
    pub const SET_PERIODIC: u8 = 0x04;
    pub const SET_CONSTANT_FORCE: u8 = 0x05;
    pub const SET_RAMP_FORCE: u8 = 0x06;
    pub const EFFECT_OPERATION: u8 = 0x0A;
    pub const BLOCK_FREE: u8 = 0x0B;
    pub const DEVICE_CONTROL: u8 = 0x0C;
    pub const DEVICE_GAIN: u8 = 0x0D;
}

pub const DURATION_INFINITE: u16 = 0xFFFF;

/// Standard PIDFF effect types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum EffectType {
    Constant = 1,
    Ramp = 2,
    Square = 3,
    Sine = 4,
    Triangle = 5,
    SawtoothUp = 6,
    SawtoothDown = 7,
    Spring = 8,
    Damper = 9,
    Inertia = 10,
    Friction = 11,
}

/// Effect operation types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum EffectOp {
    Start = 1,
    StartSolo = 2,
    Stop = 3,
}

// ---------------------------------------------------------------------------
// Report sizes
// ---------------------------------------------------------------------------

pub const SET_EFFECT_LEN: usize = 14;
pub const SET_PERIODIC_LEN: usize = 10;
pub const SET_CONSTANT_FORCE_LEN: usize = 4;
pub const SET_RAMP_FORCE_LEN: usize = 6;
pub const SET_CONDITION_LEN: usize = 14;
pub const EFFECT_OPERATION_LEN: usize = 4;
pub const DEVICE_CONTROL_LEN: usize = 2;
pub const DEVICE_GAIN_LEN: usize = 4;

// ---------------------------------------------------------------------------
// Encoders
// ---------------------------------------------------------------------------

/// Encode a Set Effect report (14 bytes).
pub fn encode_set_effect(
    block_index: u8,
    effect_type: EffectType,
    duration_ms: u16,
    gain: u8,
    direction: u16,
) -> [u8; SET_EFFECT_LEN] {
    let mut buf = [0u8; SET_EFFECT_LEN];
    buf[0] = report_ids::SET_EFFECT;
    buf[1] = block_index;
    buf[2] = effect_type as u8;
    buf[3..5].copy_from_slice(&duration_ms.to_le_bytes());
    buf[9] = gain;
    buf[10] = 0xFF;
    buf[11..13].copy_from_slice(&direction.to_le_bytes());
    buf
}

/// Encode a Set Constant Force report (4 bytes).
pub fn encode_set_constant_force(block_index: u8, magnitude: i16) -> [u8; SET_CONSTANT_FORCE_LEN] {
    let mut buf = [0u8; SET_CONSTANT_FORCE_LEN];
    buf[0] = report_ids::SET_CONSTANT_FORCE;
    buf[1] = block_index;
    buf[2..4].copy_from_slice(&magnitude.to_le_bytes());
    buf
}

/// Encode a Set Periodic report (10 bytes).
pub fn encode_set_periodic(
    block_index: u8,
    magnitude: u16,
    offset: i16,
    phase: u16,
    period_ms: u16,
) -> [u8; SET_PERIODIC_LEN] {
    let mut buf = [0u8; SET_PERIODIC_LEN];
    buf[0] = report_ids::SET_PERIODIC;
    buf[1] = block_index;
    buf[2..4].copy_from_slice(&magnitude.to_le_bytes());
    buf[4..6].copy_from_slice(&offset.to_le_bytes());
    buf[6..8].copy_from_slice(&phase.to_le_bytes());
    buf[8..10].copy_from_slice(&period_ms.to_le_bytes());
    buf
}

/// Encode a Set Ramp Force report (6 bytes).
pub fn encode_set_ramp_force(block_index: u8, start: i16, end: i16) -> [u8; SET_RAMP_FORCE_LEN] {
    let mut buf = [0u8; SET_RAMP_FORCE_LEN];
    buf[0] = report_ids::SET_RAMP_FORCE;
    buf[1] = block_index;
    buf[2..4].copy_from_slice(&start.to_le_bytes());
    buf[4..6].copy_from_slice(&end.to_le_bytes());
    buf
}

/// Encode a Set Condition report (14 bytes).
pub fn encode_set_condition(
    block_index: u8,
    axis: u8,
    center_point: i16,
    positive_coeff: i16,
    negative_coeff: i16,
    positive_sat: u16,
    negative_sat: u16,
    dead_band: u8,
) -> [u8; SET_CONDITION_LEN] {
    let mut buf = [0u8; SET_CONDITION_LEN];
    buf[0] = report_ids::SET_CONDITION;
    buf[1] = block_index;
    buf[2] = axis;
    buf[3..5].copy_from_slice(&center_point.to_le_bytes());
    buf[5..7].copy_from_slice(&positive_coeff.to_le_bytes());
    buf[7..9].copy_from_slice(&negative_coeff.to_le_bytes());
    buf[9..11].copy_from_slice(&positive_sat.to_le_bytes());
    buf[11..13].copy_from_slice(&negative_sat.to_le_bytes());
    buf[13] = dead_band;
    buf
}

/// Encode an Effect Operation report (4 bytes).
pub fn encode_effect_operation(
    block_index: u8,
    op: EffectOp,
    loop_count: u8,
) -> [u8; EFFECT_OPERATION_LEN] {
    [
        report_ids::EFFECT_OPERATION,
        block_index,
        op as u8,
        loop_count,
    ]
}

/// Encode a Device Control report (2 bytes).
pub fn encode_device_control(command: u8) -> [u8; DEVICE_CONTROL_LEN] {
    [report_ids::DEVICE_CONTROL, command]
}

/// Encode a Device Gain report (4 bytes, gain 0-10000).
pub fn encode_device_gain(gain: u16) -> [u8; DEVICE_GAIN_LEN] {
    let mut buf = [0u8; DEVICE_GAIN_LEN];
    buf[0] = report_ids::DEVICE_GAIN;
    let g = gain.min(10000);
    buf[2..4].copy_from_slice(&g.to_le_bytes());
    buf
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
        assert_eq!(report_ids::DEVICE_CONTROL, 0x0C);
        assert_eq!(report_ids::DEVICE_GAIN, 0x0D);
    }

    #[test]
    fn set_effect_report() {
        let buf = encode_set_effect(1, EffectType::Sine, 5000, 200, 9000);
        assert_eq!(buf[0], 0x01);
        assert_eq!(buf[2], 4); // Sine
        assert_eq!(u16::from_le_bytes([buf[3], buf[4]]), 5000);
        assert_eq!(buf[9], 200);
        assert_eq!(buf.len(), SET_EFFECT_LEN);
    }

    #[test]
    fn set_constant_force_report() {
        let buf = encode_set_constant_force(1, -7000);
        assert_eq!(buf[0], 0x05);
        assert_eq!(i16::from_le_bytes([buf[2], buf[3]]), -7000);
    }

    #[test]
    fn set_periodic_report() {
        let buf = encode_set_periodic(2, 5000, -1000, 18000, 200);
        assert_eq!(buf[0], 0x04);
        assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), 5000);
        assert_eq!(i16::from_le_bytes([buf[4], buf[5]]), -1000);
    }

    #[test]
    fn set_ramp_force_report() {
        let buf = encode_set_ramp_force(1, -5000, 5000);
        assert_eq!(buf[0], 0x06);
        assert_eq!(i16::from_le_bytes([buf[2], buf[3]]), -5000);
        assert_eq!(i16::from_le_bytes([buf[4], buf[5]]), 5000);
    }

    #[test]
    fn set_condition_report() {
        let buf = encode_set_condition(1, 0, 0, 5000, -5000, 10000, 10000, 25);
        assert_eq!(buf[0], 0x03);
        assert_eq!(i16::from_le_bytes([buf[5], buf[6]]), 5000);
        assert_eq!(buf[13], 25);
        assert_eq!(buf.len(), SET_CONDITION_LEN);
    }

    #[test]
    fn effect_operation_start() {
        let buf = encode_effect_operation(1, EffectOp::Start, 0);
        assert_eq!(buf, [0x0A, 1, 1, 0]);
    }

    #[test]
    fn effect_operation_stop() {
        let buf = encode_effect_operation(3, EffectOp::Stop, 0);
        assert_eq!(buf, [0x0A, 3, 3, 0]);
    }

    #[test]
    fn device_control_report() {
        let buf = encode_device_control(0x01); // enable
        assert_eq!(buf, [0x0C, 0x01]);
    }

    #[test]
    fn device_gain_report() {
        let buf = encode_device_gain(8000);
        assert_eq!(buf[0], 0x0D);
        assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), 8000);
    }

    #[test]
    fn device_gain_clamps() {
        let buf = encode_device_gain(50000);
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

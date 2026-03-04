//! Standard USB HID PID effect reports for PXN wheelbases.
//!
//! PXN devices (V10, V12, V12 Lite, GT987) use the standard USB HID PID
//! (Physical Interface Device) protocol for force feedback. The Linux
//! kernel applies a `HID_PIDFF_QUIRK_PERIODIC_SINE_ONLY` quirk, meaning
//! the firmware only supports sine waveform for periodic effects —
//! square, triangle, and sawtooth may be ignored or aliased to sine.
//!
//! This module provides allocation-free encoders for the PIDFF reports.
//! All report IDs follow the USB HID PID 1.01 specification.
//!
//! # Quirks
//!
//! - **Periodic sine only**: PXN firmware implements only the sine
//!   waveform. Other waveforms (square, triangle, sawtooth) are exposed
//!   in the HID descriptor but may not produce distinct output.
//!   The [`EffectType`] enum includes all standard types for protocol
//!   completeness, but callers should prefer [`EffectType::Sine`].
//!
//! # Sources
//!
//! - USB HID PID specification (pid1_01.pdf)
//! - Linux kernel `hid-universal-pidff` driver with PXN quirk table
//! - JacKeTUs/linux-steering-wheels compatibility list

/// Standard PIDFF report IDs (USB HID PID 1.01).
pub mod report_ids {
    /// Set Effect report.
    pub const SET_EFFECT: u8 = 0x01;
    /// Set Envelope report.
    pub const SET_ENVELOPE: u8 = 0x02;
    /// Set Condition report (spring/damper/inertia/friction).
    pub const SET_CONDITION: u8 = 0x03;
    /// Set Periodic report (sine waveform only on PXN hardware).
    pub const SET_PERIODIC: u8 = 0x04;
    /// Set Constant Force report.
    pub const SET_CONSTANT_FORCE: u8 = 0x05;
    /// Set Ramp Force report.
    pub const SET_RAMP_FORCE: u8 = 0x06;
    /// Effect Operation report (start/stop).
    pub const EFFECT_OPERATION: u8 = 0x0A;
    /// Block Free report.
    pub const BLOCK_FREE: u8 = 0x0B;
    /// Device Control report.
    pub const DEVICE_CONTROL: u8 = 0x0C;
    /// Device Gain report.
    pub const DEVICE_GAIN: u8 = 0x0D;
    /// Create New Effect (request block allocation).
    pub const CREATE_NEW_EFFECT: u8 = 0x11;
    /// Block Load response (feature report).
    pub const BLOCK_LOAD: u8 = 0x12;
}

/// Duration value meaning "infinite" in PIDFF.
pub const DURATION_INFINITE: u16 = 0xFFFF;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Standard PIDFF effect types.
///
/// Note: PXN hardware only supports sine for periodic effects. Other
/// waveform types are included for protocol completeness but may not
/// produce distinct output on PXN devices.
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

/// Device control commands (bitfield).
pub mod device_control {
    pub const ENABLE_ACTUATORS: u8 = 0x01;
    pub const DISABLE_ACTUATORS: u8 = 0x02;
    pub const STOP_ALL_EFFECTS: u8 = 0x04;
    pub const DEVICE_RESET: u8 = 0x08;
    pub const DEVICE_PAUSE: u8 = 0x10;
    pub const DEVICE_CONTINUE: u8 = 0x20;
}

// ---------------------------------------------------------------------------
// Report sizes
// ---------------------------------------------------------------------------

pub const SET_EFFECT_LEN: usize = 14;
pub const SET_ENVELOPE_LEN: usize = 10;
pub const SET_CONDITION_LEN: usize = 14;
pub const SET_PERIODIC_LEN: usize = 10;
pub const SET_CONSTANT_FORCE_LEN: usize = 4;
pub const SET_RAMP_FORCE_LEN: usize = 6;
pub const EFFECT_OPERATION_LEN: usize = 4;
pub const DEVICE_CONTROL_LEN: usize = 2;
pub const DEVICE_GAIN_LEN: usize = 4;
pub const BLOCK_FREE_LEN: usize = 2;

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
    buf[10] = 0xFF; // no trigger button
    buf[11..13].copy_from_slice(&direction.to_le_bytes());
    buf
}

/// Encode a Set Envelope report (10 bytes).
pub fn encode_set_envelope(
    block_index: u8,
    attack_level: u16,
    fade_level: u16,
    attack_time_ms: u16,
    fade_time_ms: u16,
) -> [u8; SET_ENVELOPE_LEN] {
    let mut buf = [0u8; SET_ENVELOPE_LEN];
    buf[0] = report_ids::SET_ENVELOPE;
    buf[1] = block_index;
    buf[2..4].copy_from_slice(&attack_level.to_le_bytes());
    buf[4..6].copy_from_slice(&fade_level.to_le_bytes());
    buf[6..8].copy_from_slice(&attack_time_ms.to_le_bytes());
    buf[8..10].copy_from_slice(&fade_time_ms.to_le_bytes());
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

/// Encode a Set Periodic report (10 bytes).
///
/// Note: PXN hardware only supports sine waveform. The periodic
/// parameters (magnitude, offset, phase, period) are always sent
/// as standard PIDFF regardless of the chosen effect type.
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

/// Encode a Set Constant Force report (4 bytes).
pub fn encode_set_constant_force(block_index: u8, magnitude: i16) -> [u8; SET_CONSTANT_FORCE_LEN] {
    let mut buf = [0u8; SET_CONSTANT_FORCE_LEN];
    buf[0] = report_ids::SET_CONSTANT_FORCE;
    buf[1] = block_index;
    buf[2..4].copy_from_slice(&magnitude.to_le_bytes());
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

/// Encode a Block Free report (2 bytes).
pub fn encode_block_free(block_index: u8) -> [u8; BLOCK_FREE_LEN] {
    [report_ids::BLOCK_FREE, block_index]
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
    fn set_effect_report_layout() {
        let buf = encode_set_effect(1, EffectType::Constant, DURATION_INFINITE, 255, 0);
        assert_eq!(buf[0], 0x01);
        assert_eq!(buf[1], 1);
        assert_eq!(buf[2], 1);
        assert_eq!(u16::from_le_bytes([buf[3], buf[4]]), 0xFFFF);
        assert_eq!(buf[9], 255);
        assert_eq!(buf.len(), SET_EFFECT_LEN);
    }

    #[test]
    fn set_effect_sine_direction() {
        let buf = encode_set_effect(2, EffectType::Sine, 1000, 128, 18000);
        assert_eq!(u16::from_le_bytes([buf[11], buf[12]]), 18000);
    }

    #[test]
    fn set_envelope_report_layout() {
        let buf = encode_set_envelope(1, 5000, 8000, 100, 200);
        assert_eq!(buf[0], 0x02);
        assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), 5000);
        assert_eq!(u16::from_le_bytes([buf[4], buf[5]]), 8000);
    }

    #[test]
    fn set_condition_report_layout() {
        let buf = encode_set_condition(1, 0, -500, 3000, -2000, 10000, 10000, 50);
        assert_eq!(buf[0], 0x03);
        assert_eq!(i16::from_le_bytes([buf[3], buf[4]]), -500);
        assert_eq!(i16::from_le_bytes([buf[5], buf[6]]), 3000);
        assert_eq!(buf[13], 50);
    }

    #[test]
    fn set_periodic_report_layout() {
        let buf = encode_set_periodic(1, 7500, -2000, 9000, 250);
        assert_eq!(buf[0], 0x04);
        assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), 7500);
        assert_eq!(i16::from_le_bytes([buf[4], buf[5]]), -2000);
    }

    #[test]
    fn set_constant_force_report() {
        let buf = encode_set_constant_force(1, -5000);
        assert_eq!(buf[0], 0x05);
        assert_eq!(i16::from_le_bytes([buf[2], buf[3]]), -5000);
    }

    #[test]
    fn set_ramp_force_report() {
        let buf = encode_set_ramp_force(2, -3000, 3000);
        assert_eq!(buf[0], 0x06);
        assert_eq!(i16::from_le_bytes([buf[2], buf[3]]), -3000);
        assert_eq!(i16::from_le_bytes([buf[4], buf[5]]), 3000);
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
    fn block_free_report() {
        let buf = encode_block_free(5);
        assert_eq!(buf, [0x0B, 5]);
    }

    #[test]
    fn device_control_enable() {
        let buf = encode_device_control(device_control::ENABLE_ACTUATORS);
        assert_eq!(buf, [0x0C, 0x01]);
    }

    #[test]
    fn device_gain_report() {
        let buf = encode_device_gain(7500);
        assert_eq!(buf[0], 0x0D);
        assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), 7500);
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
        fn prop_periodic_magnitude_preserved(mag in 0u16..=10000u16) {
            let buf = encode_set_periodic(1, mag, 0, 0, 100);
            prop_assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), mag);
        }

        #[test]
        fn prop_device_gain_bounded(gain in 0u16..=20000u16) {
            let buf = encode_device_gain(gain);
            let encoded = u16::from_le_bytes([buf[2], buf[3]]);
            prop_assert!(encoded <= 10000);
        }

        #[test]
        fn prop_ramp_values_preserved(start in -10000i16..=10000i16, end in -10000i16..=10000i16) {
            let buf = encode_set_ramp_force(1, start, end);
            prop_assert_eq!(i16::from_le_bytes([buf[2], buf[3]]), start);
            prop_assert_eq!(i16::from_le_bytes([buf[4], buf[5]]), end);
        }
    }
}

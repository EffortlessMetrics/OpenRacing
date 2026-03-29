//! Shared USB HID PID (PIDFF) effect encoders for force feedback devices.
//!
//! This crate provides the canonical implementation of standard USB HID PID
//! (Physical Interface Device) report encoders used by multiple device protocol
//! crates. Devices that implement standard PIDFF — such as Simucube, AccuForce,
//! Asetek, FFBeast, Leo Bodnar, PXN, OpenFFBoard, Simagic, and others — all
//! share the same wire format defined by the USB HID PID 1.01 specification.
//!
//! # Design
//!
//! - **Allocation-free**: all encoders write to fixed-size arrays
//! - **I/O-free**: pure functions only, no hardware access
//! - **Deterministic**: no floating-point, no branching on platform state
//!
//! # Usage
//!
//! Device-specific crates can re-export these encoders or use them internally:
//!
//! ```rust
//! use openracing_pidff_common::*;
//!
//! // Create a constant force effect
//! let report = encode_set_constant_force(1, -5000);
//! assert_eq!(report[0], report_ids::SET_CONSTANT_FORCE);
//! ```
//!
//! # Sources
//!
//! - USB HID PID specification 1.01 (pid1_01.pdf)
//! - Linux kernel `hid-pidff` driver

#![deny(unsafe_op_in_unsafe_fn)]
#![deny(static_mut_refs)]
#![deny(clippy::unwrap_used)]

/// Standard PIDFF report IDs (USB HID PID 1.01).
pub mod report_ids {
    /// Set Effect report.
    pub const SET_EFFECT: u8 = 0x01;
    /// Set Envelope report.
    pub const SET_ENVELOPE: u8 = 0x02;
    /// Set Condition report (spring/damper/inertia/friction).
    pub const SET_CONDITION: u8 = 0x03;
    /// Set Periodic report (sine/square/triangle/sawtooth).
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
    /// PID Pool report (feature report).
    pub const PID_POOL: u8 = 0x13;
}

/// Duration value meaning "infinite" in PIDFF.
pub const DURATION_INFINITE: u16 = 0xFFFF;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Standard PIDFF effect types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum EffectType {
    /// Constant force effect.
    Constant = 1,
    /// Ramp force effect.
    Ramp = 2,
    /// Square wave periodic effect.
    Square = 3,
    /// Sine wave periodic effect.
    Sine = 4,
    /// Triangle wave periodic effect.
    Triangle = 5,
    /// Upward sawtooth wave periodic effect.
    SawtoothUp = 6,
    /// Downward sawtooth wave periodic effect.
    SawtoothDown = 7,
    /// Spring condition effect.
    Spring = 8,
    /// Damper condition effect.
    Damper = 9,
    /// Inertia condition effect.
    Inertia = 10,
    /// Friction condition effect.
    Friction = 11,
}

/// Effect operation types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum EffectOp {
    /// Start the effect.
    Start = 1,
    /// Start the effect and stop all other effects.
    StartSolo = 2,
    /// Stop the effect.
    Stop = 3,
}

/// Device control commands (bitfield).
pub mod device_control {
    /// Enable actuators.
    pub const ENABLE_ACTUATORS: u8 = 0x01;
    /// Disable actuators.
    pub const DISABLE_ACTUATORS: u8 = 0x02;
    /// Stop all effects.
    pub const STOP_ALL_EFFECTS: u8 = 0x04;
    /// Device reset.
    pub const DEVICE_RESET: u8 = 0x08;
    /// Pause all effects.
    pub const DEVICE_PAUSE: u8 = 0x10;
    /// Continue (resume paused effects).
    pub const DEVICE_CONTINUE: u8 = 0x20;
}

/// Block load status values (from Block Load feature report).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BlockLoadStatus {
    /// Block loaded successfully.
    Success = 1,
    /// No more memory for effect blocks.
    Full = 2,
    /// Load error.
    Error = 3,
}

// ---------------------------------------------------------------------------
// Report sizes
// ---------------------------------------------------------------------------

/// Set Effect report size (14 bytes).
pub const SET_EFFECT_LEN: usize = 14;
/// Set Envelope report size (10 bytes).
pub const SET_ENVELOPE_LEN: usize = 10;
/// Set Condition report size (14 bytes).
pub const SET_CONDITION_LEN: usize = 14;
/// Set Periodic report size (10 bytes).
pub const SET_PERIODIC_LEN: usize = 10;
/// Set Constant Force report size (4 bytes).
pub const SET_CONSTANT_FORCE_LEN: usize = 4;
/// Set Ramp Force report size (6 bytes).
pub const SET_RAMP_FORCE_LEN: usize = 6;
/// Effect Operation report size (4 bytes).
pub const EFFECT_OPERATION_LEN: usize = 4;
/// Device Control report size (2 bytes).
pub const DEVICE_CONTROL_LEN: usize = 2;
/// Device Gain report size (4 bytes).
pub const DEVICE_GAIN_LEN: usize = 4;
/// Block Free report size (2 bytes).
pub const BLOCK_FREE_LEN: usize = 2;
/// Create New Effect report size (2 bytes).
pub const CREATE_NEW_EFFECT_LEN: usize = 2;

// ---------------------------------------------------------------------------
// Encoders
// ---------------------------------------------------------------------------

/// Encode a Set Effect report (14 bytes).
///
/// # Parameters
/// - `block_index`: effect block index (assigned by device)
/// - `effect_type`: the type of effect (constant, sine, spring, etc.)
/// - `duration_ms`: effect duration in milliseconds (0xFFFF = infinite)
/// - `gain`: effect gain (0-255)
/// - `direction`: effect direction in hundredths of degrees (0-35999)
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
    // bytes 5-8: trigger repeat interval, sample period (defaults to 0)
    buf[9] = gain;
    buf[10] = 0xFF; // no trigger button
    buf[11..13].copy_from_slice(&direction.to_le_bytes());
    buf
}

/// Encode a Set Envelope report (10 bytes).
///
/// Defines the attack/fade envelope for an effect.
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
///
/// Used for spring, damper, inertia, and friction effects.
///
/// # Parameters
/// - `block_index`: effect block index
/// - `axis`: axis index (0 = X, 1 = Y)
/// - `center_point`: center of the condition effect
/// - `positive_coeff`: coefficient for positive displacement
/// - `negative_coeff`: coefficient for negative displacement
/// - `positive_sat`: positive saturation
/// - `negative_sat`: negative saturation
/// - `dead_band`: dead band width
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
/// Defines parameters for periodic effects (sine, square, triangle, sawtooth).
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

/// Encode a Device Gain report (4 bytes).
///
/// Gain is clamped to 0-10000 (PIDFF convention).
pub fn encode_device_gain(gain: u16) -> [u8; DEVICE_GAIN_LEN] {
    let mut buf = [0u8; DEVICE_GAIN_LEN];
    buf[0] = report_ids::DEVICE_GAIN;
    let g = gain.min(10000);
    buf[2..4].copy_from_slice(&g.to_le_bytes());
    buf
}

/// Encode a Create New Effect report (2 bytes).
pub fn encode_create_new_effect(effect_type: EffectType) -> [u8; CREATE_NEW_EFFECT_LEN] {
    [report_ids::CREATE_NEW_EFFECT, effect_type as u8]
}

// ---------------------------------------------------------------------------
// Block Load parser
// ---------------------------------------------------------------------------

/// Parsed Block Load feature report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockLoadReport {
    /// Effect block index assigned by the device.
    pub block_index: u8,
    /// Load status.
    pub status: BlockLoadStatus,
    /// RAM pool available (bytes), if reported.
    pub ram_pool_available: u16,
}

/// Parse a Block Load feature report.
///
/// Expects at least 5 bytes: `[report_id, block_index, status, ram_lo, ram_hi]`.
/// Returns `None` if the buffer is too short or has wrong report ID.
pub fn parse_block_load(buf: &[u8]) -> Option<BlockLoadReport> {
    if buf.len() < 5 || buf[0] != report_ids::BLOCK_LOAD {
        return None;
    }
    let status = match buf[2] {
        1 => BlockLoadStatus::Success,
        2 => BlockLoadStatus::Full,
        3 => BlockLoadStatus::Error,
        _ => return None,
    };
    Some(BlockLoadReport {
        block_index: buf[1],
        status,
        ram_pool_available: u16::from_le_bytes([buf[3], buf[4]]),
    })
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
        assert_eq!(report_ids::CREATE_NEW_EFFECT, 0x11);
        assert_eq!(report_ids::BLOCK_LOAD, 0x12);
        assert_eq!(report_ids::PID_POOL, 0x13);
    }

    #[test]
    fn effect_type_values() {
        assert_eq!(EffectType::Constant as u8, 1);
        assert_eq!(EffectType::Ramp as u8, 2);
        assert_eq!(EffectType::Square as u8, 3);
        assert_eq!(EffectType::Sine as u8, 4);
        assert_eq!(EffectType::Triangle as u8, 5);
        assert_eq!(EffectType::SawtoothUp as u8, 6);
        assert_eq!(EffectType::SawtoothDown as u8, 7);
        assert_eq!(EffectType::Spring as u8, 8);
        assert_eq!(EffectType::Damper as u8, 9);
        assert_eq!(EffectType::Inertia as u8, 10);
        assert_eq!(EffectType::Friction as u8, 11);
    }

    #[test]
    fn effect_op_values() {
        assert_eq!(EffectOp::Start as u8, 1);
        assert_eq!(EffectOp::StartSolo as u8, 2);
        assert_eq!(EffectOp::Stop as u8, 3);
    }

    #[test]
    fn device_control_flags() {
        assert_eq!(device_control::ENABLE_ACTUATORS, 0x01);
        assert_eq!(device_control::DISABLE_ACTUATORS, 0x02);
        assert_eq!(device_control::STOP_ALL_EFFECTS, 0x04);
        assert_eq!(device_control::DEVICE_RESET, 0x08);
        assert_eq!(device_control::DEVICE_PAUSE, 0x10);
        assert_eq!(device_control::DEVICE_CONTINUE, 0x20);
    }

    #[test]
    fn set_effect_report_layout() {
        let buf = encode_set_effect(1, EffectType::Constant, DURATION_INFINITE, 255, 0);
        assert_eq!(buf[0], 0x01);
        assert_eq!(buf[1], 1);
        assert_eq!(buf[2], 1); // ConstantForce
        assert_eq!(u16::from_le_bytes([buf[3], buf[4]]), 0xFFFF);
        assert_eq!(buf[9], 255); // gain
        assert_eq!(buf[10], 0xFF); // no trigger button
        assert_eq!(u16::from_le_bytes([buf[11], buf[12]]), 0); // direction
        assert_eq!(buf.len(), SET_EFFECT_LEN);
    }

    #[test]
    fn set_effect_direction() {
        let buf = encode_set_effect(2, EffectType::Sine, 1000, 128, 18000);
        assert_eq!(u16::from_le_bytes([buf[11], buf[12]]), 18000);
        assert_eq!(buf[2], EffectType::Sine as u8);
    }

    #[test]
    fn set_envelope_report_layout() {
        let buf = encode_set_envelope(1, 5000, 8000, 100, 200);
        assert_eq!(buf[0], 0x02);
        assert_eq!(buf[1], 1);
        assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), 5000);
        assert_eq!(u16::from_le_bytes([buf[4], buf[5]]), 8000);
        assert_eq!(u16::from_le_bytes([buf[6], buf[7]]), 100);
        assert_eq!(u16::from_le_bytes([buf[8], buf[9]]), 200);
        assert_eq!(buf.len(), SET_ENVELOPE_LEN);
    }

    #[test]
    fn set_condition_report_layout() {
        let buf = encode_set_condition(1, 0, -500, 3000, -2000, 10000, 10000, 50);
        assert_eq!(buf[0], 0x03);
        assert_eq!(buf[1], 1);
        assert_eq!(buf[2], 0); // axis
        assert_eq!(i16::from_le_bytes([buf[3], buf[4]]), -500);
        assert_eq!(i16::from_le_bytes([buf[5], buf[6]]), 3000);
        assert_eq!(i16::from_le_bytes([buf[7], buf[8]]), -2000);
        assert_eq!(u16::from_le_bytes([buf[9], buf[10]]), 10000);
        assert_eq!(u16::from_le_bytes([buf[11], buf[12]]), 10000);
        assert_eq!(buf[13], 50);
        assert_eq!(buf.len(), SET_CONDITION_LEN);
    }

    #[test]
    fn set_periodic_report_layout() {
        let buf = encode_set_periodic(1, 7500, -2000, 9000, 250);
        assert_eq!(buf[0], 0x04);
        assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), 7500);
        assert_eq!(i16::from_le_bytes([buf[4], buf[5]]), -2000);
        assert_eq!(u16::from_le_bytes([buf[6], buf[7]]), 9000);
        assert_eq!(u16::from_le_bytes([buf[8], buf[9]]), 250);
        assert_eq!(buf.len(), SET_PERIODIC_LEN);
    }

    #[test]
    fn set_constant_force_report() {
        let buf = encode_set_constant_force(1, -5000);
        assert_eq!(buf[0], 0x05);
        assert_eq!(buf[1], 1);
        assert_eq!(i16::from_le_bytes([buf[2], buf[3]]), -5000);
        assert_eq!(buf.len(), SET_CONSTANT_FORCE_LEN);
    }

    #[test]
    fn set_ramp_force_report() {
        let buf = encode_set_ramp_force(2, -3000, 3000);
        assert_eq!(buf[0], 0x06);
        assert_eq!(buf[1], 2);
        assert_eq!(i16::from_le_bytes([buf[2], buf[3]]), -3000);
        assert_eq!(i16::from_le_bytes([buf[4], buf[5]]), 3000);
        assert_eq!(buf.len(), SET_RAMP_FORCE_LEN);
    }

    #[test]
    fn effect_operation_start() {
        let buf = encode_effect_operation(1, EffectOp::Start, 0);
        assert_eq!(buf, [0x0A, 1, 1, 0]);
    }

    #[test]
    fn effect_operation_start_solo() {
        let buf = encode_effect_operation(1, EffectOp::StartSolo, 0);
        assert_eq!(buf, [0x0A, 1, 2, 0]);
    }

    #[test]
    fn effect_operation_stop() {
        let buf = encode_effect_operation(3, EffectOp::Stop, 0);
        assert_eq!(buf, [0x0A, 3, 3, 0]);
    }

    #[test]
    fn effect_operation_loop_count() {
        let buf = encode_effect_operation(1, EffectOp::Start, 5);
        assert_eq!(buf[3], 5);
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
    fn device_control_reset() {
        let buf = encode_device_control(device_control::DEVICE_RESET);
        assert_eq!(buf, [0x0C, 0x08]);
    }

    #[test]
    fn device_control_combined_flags() {
        let cmd = device_control::ENABLE_ACTUATORS | device_control::STOP_ALL_EFFECTS;
        let buf = encode_device_control(cmd);
        assert_eq!(buf[1], 0x05);
    }

    #[test]
    fn device_gain_report() {
        let buf = encode_device_gain(7500);
        assert_eq!(buf[0], 0x0D);
        assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), 7500);
    }

    #[test]
    fn device_gain_clamps_to_max() {
        let buf = encode_device_gain(20000);
        assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), 10000);
    }

    #[test]
    fn device_gain_zero() {
        let buf = encode_device_gain(0);
        assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), 0);
    }

    #[test]
    fn create_new_effect_report() {
        let buf = encode_create_new_effect(EffectType::Sine);
        assert_eq!(buf, [0x11, 4]);
    }

    #[test]
    fn block_load_success() {
        let buf = [0x12, 3, 1, 0x00, 0x10];
        let report = parse_block_load(&buf);
        assert!(report.is_some());
        let r = report.expect("tested above");
        assert_eq!(r.block_index, 3);
        assert_eq!(r.status, BlockLoadStatus::Success);
        assert_eq!(r.ram_pool_available, 0x1000);
    }

    #[test]
    fn block_load_full() {
        let buf = [0x12, 0, 2, 0x00, 0x00];
        let r = parse_block_load(&buf).expect("valid report");
        assert_eq!(r.status, BlockLoadStatus::Full);
    }

    #[test]
    fn block_load_too_short() {
        let buf = [0x12, 0, 1];
        assert!(parse_block_load(&buf).is_none());
    }

    #[test]
    fn block_load_wrong_id() {
        let buf = [0x13, 0, 1, 0, 0];
        assert!(parse_block_load(&buf).is_none());
    }

    #[test]
    fn block_load_invalid_status() {
        let buf = [0x12, 0, 0, 0, 0]; // status 0 is invalid
        assert!(parse_block_load(&buf).is_none());
    }

    #[test]
    fn duration_infinite_value() {
        assert_eq!(DURATION_INFINITE, 0xFFFF);
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(500))]

        #[test]
        fn prop_constant_force_magnitude_preserved(mag in i16::MIN..=i16::MAX) {
            let buf = encode_set_constant_force(1, mag);
            prop_assert_eq!(buf[0], report_ids::SET_CONSTANT_FORCE);
            prop_assert_eq!(i16::from_le_bytes([buf[2], buf[3]]), mag);
        }

        #[test]
        fn prop_periodic_magnitude_preserved(mag in 0u16..=u16::MAX) {
            let buf = encode_set_periodic(1, mag, 0, 0, 100);
            prop_assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), mag);
        }

        #[test]
        fn prop_device_gain_bounded(gain in 0u16..=u16::MAX) {
            let buf = encode_device_gain(gain);
            let encoded = u16::from_le_bytes([buf[2], buf[3]]);
            prop_assert!(encoded <= 10000);
            if gain <= 10000 {
                prop_assert_eq!(encoded, gain);
            }
        }

        #[test]
        fn prop_ramp_values_preserved(start in i16::MIN..=i16::MAX, end in i16::MIN..=i16::MAX) {
            let buf = encode_set_ramp_force(1, start, end);
            prop_assert_eq!(i16::from_le_bytes([buf[2], buf[3]]), start);
            prop_assert_eq!(i16::from_le_bytes([buf[4], buf[5]]), end);
        }

        #[test]
        fn prop_condition_center_preserved(center in i16::MIN..=i16::MAX) {
            let buf = encode_set_condition(1, 0, center, 0, 0, 0, 0, 0);
            prop_assert_eq!(i16::from_le_bytes([buf[3], buf[4]]), center);
        }

        #[test]
        fn prop_envelope_values_preserved(
            attack in 0u16..=u16::MAX,
            fade in 0u16..=u16::MAX,
            at_ms in 0u16..=u16::MAX,
            ft_ms in 0u16..=u16::MAX,
        ) {
            let buf = encode_set_envelope(1, attack, fade, at_ms, ft_ms);
            prop_assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), attack);
            prop_assert_eq!(u16::from_le_bytes([buf[4], buf[5]]), fade);
            prop_assert_eq!(u16::from_le_bytes([buf[6], buf[7]]), at_ms);
            prop_assert_eq!(u16::from_le_bytes([buf[8], buf[9]]), ft_ms);
        }

        #[test]
        fn prop_effect_operation_report_id(block in 0u8..=255u8) {
            let buf = encode_effect_operation(block, EffectOp::Start, 0);
            prop_assert_eq!(buf[0], report_ids::EFFECT_OPERATION);
            prop_assert_eq!(buf[1], block);
        }

        #[test]
        fn prop_block_free_report_id(block in 0u8..=255u8) {
            let buf = encode_block_free(block);
            prop_assert_eq!(buf[0], report_ids::BLOCK_FREE);
            prop_assert_eq!(buf[1], block);
        }
    }
}

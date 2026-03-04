//! Standard USB HID PID effect reports for Simucube wheelbases.
//!
//! Simucube 2 (Sport/Pro/Ultimate) and Simucube 1 wheelbases use the
//! **standard USB HID PID (Physical Interface Device)** protocol for force
//! feedback. The Linux kernel confirms this via the `hid-pidff` driver
//! (Silver support for Simucube 2 as of Linux 6.15).
//!
//! This module provides allocation-free encoders for the PIDFF reports needed
//! to manage force feedback effects. Report IDs follow the USB HID PID 1.01
//! specification and are consistent across all standard PIDFF devices.
//!
//! # Effect lifecycle
//!
//! 1. Upload effect parameters via [`encode_set_effect`]
//! 2. Set type-specific data (constant force, periodic, ramp, condition)
//! 3. Start the effect via [`encode_effect_operation`] with [`EffectOp::Start`]
//! 4. Update parameters dynamically as needed
//! 5. Stop via [`encode_effect_operation`] with [`EffectOp::Stop`]
//!
//! # Sources
//!
//! - USB HID PID 1.01 specification (pid1_01.pdf)
//! - Linux kernel `hid-pidff.c` / `hid-universal-pidff.c`
//! - JacKeTUs/linux-steering-wheels: Simucube 2 compatibility (Silver)
//! - Granite Devices wiki: Simucube uses standard HID PID

// ---------------------------------------------------------------------------
// Report IDs (standard USB HID PID)
// ---------------------------------------------------------------------------

/// Standard PIDFF report IDs.
///
/// These are defined by the USB HID PID specification and are the same
/// across all conforming PIDFF devices (Simucube, VRS, OpenFFBoard, etc.).
pub mod report_ids {
    /// Set Effect report — upload/update effect parameters.
    pub const SET_EFFECT: u8 = 0x01;
    /// Set Envelope — attack/fade shaping.
    pub const SET_ENVELOPE: u8 = 0x02;
    /// Set Condition — spring/damper/inertia/friction parameters.
    pub const SET_CONDITION: u8 = 0x03;
    /// Set Periodic — sine/square/triangle/sawtooth parameters.
    pub const SET_PERIODIC: u8 = 0x04;
    /// Set Constant Force — magnitude for constant force effects.
    pub const SET_CONSTANT_FORCE: u8 = 0x05;
    /// Set Ramp Force — start/end magnitude for ramp effects.
    pub const SET_RAMP_FORCE: u8 = 0x06;
    /// Effect Operation — start/stop/solo control.
    pub const EFFECT_OPERATION: u8 = 0x0A;
    /// Block Free — release an effect block.
    pub const BLOCK_FREE: u8 = 0x0B;
    /// Device Control — enable/disable/pause/reset.
    pub const DEVICE_CONTROL: u8 = 0x0C;
    /// Device Gain — overall FFB gain.
    pub const DEVICE_GAIN: u8 = 0x0D;
    /// Create New Effect — request a new effect block.
    pub const CREATE_NEW_EFFECT: u8 = 0x11;
    /// Block Load — device response with allocated block index.
    pub const BLOCK_LOAD: u8 = 0x12;
    /// PID Pool — device capabilities (max effects, etc.).
    pub const PID_POOL: u8 = 0x13;
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Duration value meaning "infinite" in PIDFF.
pub const DURATION_INFINITE: u16 = 0xFFFF;

/// Maximum number of concurrent effects (typical for PIDFF devices).
pub const MAX_EFFECTS: u8 = 40;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Standard PIDFF effect types (USB HID PID Table B-2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum EffectType {
    /// Constant force.
    Constant = 1,
    /// Ramp force (linearly varying).
    Ramp = 2,
    /// Square wave periodic.
    Square = 3,
    /// Sine wave periodic.
    Sine = 4,
    /// Triangle wave periodic.
    Triangle = 5,
    /// Sawtooth up periodic.
    SawtoothUp = 6,
    /// Sawtooth down periodic.
    SawtoothDown = 7,
    /// Spring (position-dependent condition).
    Spring = 8,
    /// Damper (velocity-dependent condition).
    Damper = 9,
    /// Inertia (acceleration-dependent condition).
    Inertia = 10,
    /// Friction condition.
    Friction = 11,
}

/// PIDFF effect operation types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum EffectOp {
    /// Start the effect.
    Start = 1,
    /// Start the effect solo (stop all others first).
    StartSolo = 2,
    /// Stop the effect.
    Stop = 3,
}

/// PIDFF device control commands (bitfield).
pub mod device_control {
    /// Enable all actuators.
    pub const ENABLE_ACTUATORS: u8 = 0x01;
    /// Disable all actuators.
    pub const DISABLE_ACTUATORS: u8 = 0x02;
    /// Stop all running effects.
    pub const STOP_ALL_EFFECTS: u8 = 0x04;
    /// Reset the device (clear all effects).
    pub const DEVICE_RESET: u8 = 0x08;
    /// Pause all running effects.
    pub const DEVICE_PAUSE: u8 = 0x10;
    /// Continue (unpause) all effects.
    pub const DEVICE_CONTINUE: u8 = 0x20;
}

// ---------------------------------------------------------------------------
// Report sizes
// ---------------------------------------------------------------------------

/// Wire size of Set Effect report.
pub const SET_EFFECT_LEN: usize = 14;
/// Wire size of Set Envelope report.
pub const SET_ENVELOPE_LEN: usize = 10;
/// Wire size of Set Condition report.
pub const SET_CONDITION_LEN: usize = 14;
/// Wire size of Set Periodic report.
pub const SET_PERIODIC_LEN: usize = 10;
/// Wire size of Set Constant Force report.
pub const SET_CONSTANT_FORCE_LEN: usize = 4;
/// Wire size of Set Ramp Force report.
pub const SET_RAMP_FORCE_LEN: usize = 6;
/// Wire size of Effect Operation report.
pub const EFFECT_OPERATION_LEN: usize = 4;
/// Wire size of Block Free report.
pub const BLOCK_FREE_LEN: usize = 2;
/// Wire size of Device Control report.
pub const DEVICE_CONTROL_LEN: usize = 2;
/// Wire size of Device Gain report.
pub const DEVICE_GAIN_LEN: usize = 4;

// ---------------------------------------------------------------------------
// Set Effect (0x01)
// ---------------------------------------------------------------------------

/// Parameters for the PIDFF Set Effect report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetEffect {
    /// Effect block index (1-based).
    pub block_index: u8,
    /// Effect type.
    pub effect_type: EffectType,
    /// Duration in milliseconds (0xFFFF = infinite).
    pub duration_ms: u16,
    /// Trigger repeat interval in milliseconds.
    pub trigger_repeat_ms: u16,
    /// Sample period in milliseconds (0 = default).
    pub sample_period_ms: u16,
    /// Overall gain (0-255).
    pub gain: u8,
    /// Trigger button index (0xFF = no trigger).
    pub trigger_button: u8,
    /// Direction in degrees × 100 (0-36000).
    pub direction: u16,
}

impl Default for SetEffect {
    fn default() -> Self {
        Self {
            block_index: 1,
            effect_type: EffectType::Constant,
            duration_ms: DURATION_INFINITE,
            trigger_repeat_ms: 0,
            sample_period_ms: 0,
            gain: 255,
            trigger_button: 0xFF,
            direction: 0,
        }
    }
}

/// Encode a Set Effect report.
///
/// # Wire format (14 bytes)
/// ```text
/// Byte 0:    Report ID (0x01)
/// Byte 1:    Effect block index
/// Byte 2:    Effect type
/// Bytes 3-4: Duration (ms, LE)
/// Bytes 5-6: Trigger repeat (ms, LE)
/// Bytes 7-8: Sample period (ms, LE)
/// Byte 9:    Gain (0-255)
/// Byte 10:   Trigger button (0xFF = none)
/// Bytes 11-12: Direction (degrees × 100, LE)
/// Byte 13:   Reserved
/// ```
pub fn encode_set_effect(params: &SetEffect) -> [u8; SET_EFFECT_LEN] {
    let mut buf = [0u8; SET_EFFECT_LEN];
    buf[0] = report_ids::SET_EFFECT;
    buf[1] = params.block_index;
    buf[2] = params.effect_type as u8;
    buf[3..5].copy_from_slice(&params.duration_ms.to_le_bytes());
    buf[5..7].copy_from_slice(&params.trigger_repeat_ms.to_le_bytes());
    buf[7..9].copy_from_slice(&params.sample_period_ms.to_le_bytes());
    buf[9] = params.gain;
    buf[10] = params.trigger_button;
    buf[11..13].copy_from_slice(&params.direction.to_le_bytes());
    buf
}

// ---------------------------------------------------------------------------
// Set Envelope (0x02)
// ---------------------------------------------------------------------------

/// Encode a Set Envelope report.
///
/// # Wire format (10 bytes)
/// ```text
/// Byte 0:    Report ID (0x02)
/// Byte 1:    Effect block index
/// Bytes 2-3: Attack level (0-10000, LE)
/// Bytes 4-5: Fade level (0-10000, LE)
/// Bytes 6-7: Attack time (ms, LE)
/// Bytes 8-9: Fade time (ms, LE)
/// ```
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

// ---------------------------------------------------------------------------
// Set Condition (0x03) — Spring, Damper, Inertia, Friction
// ---------------------------------------------------------------------------

/// Encode a Set Condition report.
///
/// # Wire format (14 bytes)
/// ```text
/// Byte 0:    Report ID (0x03)
/// Byte 1:    Effect block index
/// Byte 2:    Parameter block offset (axis index, 0 = X axis)
/// Bytes 3-4: Center point offset (signed, LE)
/// Bytes 5-6: Positive coefficient (signed, LE)
/// Bytes 7-8: Negative coefficient (signed, LE)
/// Bytes 9-10: Positive saturation (unsigned, LE)
/// Bytes 11-12: Negative saturation (unsigned, LE)
/// Byte 13:   Dead band (unsigned)
/// ```
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

// ---------------------------------------------------------------------------
// Set Periodic (0x04) — Sine, Square, Triangle, Sawtooth
// ---------------------------------------------------------------------------

/// Encode a Set Periodic report.
///
/// # Wire format (10 bytes)
/// ```text
/// Byte 0:    Report ID (0x04)
/// Byte 1:    Effect block index
/// Bytes 2-3: Magnitude (0-10000, LE)
/// Bytes 4-5: Offset (signed, LE)
/// Bytes 6-7: Phase (degrees × 100, LE, 0-36000)
/// Bytes 8-9: Period (ms, LE)
/// ```
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

// ---------------------------------------------------------------------------
// Set Constant Force (0x05)
// ---------------------------------------------------------------------------

/// Encode a Set Constant Force report.
///
/// # Wire format (4 bytes)
/// ```text
/// Byte 0:    Report ID (0x05)
/// Byte 1:    Effect block index
/// Bytes 2-3: Magnitude (signed, LE, -10000 to +10000)
/// ```
pub fn encode_set_constant_force(block_index: u8, magnitude: i16) -> [u8; SET_CONSTANT_FORCE_LEN] {
    let mut buf = [0u8; SET_CONSTANT_FORCE_LEN];
    buf[0] = report_ids::SET_CONSTANT_FORCE;
    buf[1] = block_index;
    buf[2..4].copy_from_slice(&magnitude.to_le_bytes());
    buf
}

// ---------------------------------------------------------------------------
// Set Ramp Force (0x06)
// ---------------------------------------------------------------------------

/// Encode a Set Ramp Force report.
///
/// # Wire format (6 bytes)
/// ```text
/// Byte 0:    Report ID (0x06)
/// Byte 1:    Effect block index
/// Bytes 2-3: Ramp start (signed, LE, -10000 to +10000)
/// Bytes 4-5: Ramp end (signed, LE, -10000 to +10000)
/// ```
pub fn encode_set_ramp_force(block_index: u8, start: i16, end: i16) -> [u8; SET_RAMP_FORCE_LEN] {
    let mut buf = [0u8; SET_RAMP_FORCE_LEN];
    buf[0] = report_ids::SET_RAMP_FORCE;
    buf[1] = block_index;
    buf[2..4].copy_from_slice(&start.to_le_bytes());
    buf[4..6].copy_from_slice(&end.to_le_bytes());
    buf
}

// ---------------------------------------------------------------------------
// Effect Operation (0x0A)
// ---------------------------------------------------------------------------

/// Encode an Effect Operation report.
///
/// # Wire format (4 bytes)
/// ```text
/// Byte 0: Report ID (0x0A)
/// Byte 1: Effect block index
/// Byte 2: Operation (1=start, 2=start solo, 3=stop)
/// Byte 3: Loop count (0 = infinite)
/// ```
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

// ---------------------------------------------------------------------------
// Block Free (0x0B)
// ---------------------------------------------------------------------------

/// Encode a Block Free report (release an effect slot).
pub fn encode_block_free(block_index: u8) -> [u8; BLOCK_FREE_LEN] {
    [report_ids::BLOCK_FREE, block_index]
}

// ---------------------------------------------------------------------------
// Device Control (0x0C)
// ---------------------------------------------------------------------------

/// Encode a Device Control report.
///
/// Use constants from [`device_control`] for the `command` parameter.
pub fn encode_device_control(command: u8) -> [u8; DEVICE_CONTROL_LEN] {
    [report_ids::DEVICE_CONTROL, command]
}

// ---------------------------------------------------------------------------
// Device Gain (0x0D)
// ---------------------------------------------------------------------------

/// Encode a Device Gain report.
///
/// # Wire format (4 bytes)
/// ```text
/// Byte 0:    Report ID (0x0D)
/// Byte 1:    Reserved (0)
/// Bytes 2-3: Gain (0-10000, LE)
/// ```
pub fn encode_device_gain(gain: u16) -> [u8; DEVICE_GAIN_LEN] {
    let mut buf = [0u8; DEVICE_GAIN_LEN];
    buf[0] = report_ids::DEVICE_GAIN;
    let g = gain.min(10000);
    buf[2..4].copy_from_slice(&g.to_le_bytes());
    buf
}

// ---------------------------------------------------------------------------
// Block Load response parser (Feature Report 0x12)
// ---------------------------------------------------------------------------

/// Status of a block load response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockLoadStatus {
    /// Effect loaded successfully.
    Success = 1,
    /// Not enough memory for the effect.
    Full = 2,
    /// Error loading the effect.
    Error = 3,
}

/// Parsed Block Load response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockLoadResponse {
    /// Allocated effect block index.
    pub block_index: u8,
    /// Load status.
    pub status: BlockLoadStatus,
    /// Amount of RAM available (device-specific units).
    pub ram_pool_available: u16,
}

/// Parse a Block Load feature report response.
///
/// Minimum 5 bytes: [block_index, status, ram_lo, ram_hi, ...]
pub fn parse_block_load(buf: &[u8]) -> Option<BlockLoadResponse> {
    if buf.len() < 4 {
        return None;
    }
    let status = match buf[1] {
        1 => BlockLoadStatus::Success,
        2 => BlockLoadStatus::Full,
        3 => BlockLoadStatus::Error,
        _ => return None,
    };
    Some(BlockLoadResponse {
        block_index: buf[0],
        status,
        ram_pool_available: u16::from_le_bytes([buf[2], buf[3]]),
    })
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Report ID constants
    // -----------------------------------------------------------------------

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

    // -----------------------------------------------------------------------
    // Effect type enum
    // -----------------------------------------------------------------------

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

    // -----------------------------------------------------------------------
    // Set Effect
    // -----------------------------------------------------------------------

    #[test]
    fn set_effect_report_id_and_length() {
        let buf = encode_set_effect(&SetEffect::default());
        assert_eq!(buf[0], 0x01);
        assert_eq!(buf.len(), SET_EFFECT_LEN);
    }

    #[test]
    fn set_effect_fields_encoded() {
        let params = SetEffect {
            block_index: 5,
            effect_type: EffectType::Sine,
            duration_ms: 2000,
            trigger_repeat_ms: 100,
            sample_period_ms: 5,
            gain: 200,
            trigger_button: 3,
            direction: 18000,
        };
        let buf = encode_set_effect(&params);
        assert_eq!(buf[1], 5);
        assert_eq!(buf[2], 4); // Sine
        assert_eq!(u16::from_le_bytes([buf[3], buf[4]]), 2000);
        assert_eq!(u16::from_le_bytes([buf[5], buf[6]]), 100);
        assert_eq!(u16::from_le_bytes([buf[7], buf[8]]), 5);
        assert_eq!(buf[9], 200);
        assert_eq!(buf[10], 3);
        assert_eq!(u16::from_le_bytes([buf[11], buf[12]]), 18000);
    }

    #[test]
    fn set_effect_default_infinite() {
        let d = SetEffect::default();
        assert_eq!(d.duration_ms, DURATION_INFINITE);
    }

    // -----------------------------------------------------------------------
    // Set Envelope
    // -----------------------------------------------------------------------

    #[test]
    fn set_envelope_report_id_and_length() {
        let buf = encode_set_envelope(1, 0, 0, 0, 0);
        assert_eq!(buf[0], 0x02);
        assert_eq!(buf.len(), SET_ENVELOPE_LEN);
    }

    #[test]
    fn set_envelope_fields() {
        let buf = encode_set_envelope(3, 5000, 8000, 200, 500);
        assert_eq!(buf[1], 3);
        assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), 5000);
        assert_eq!(u16::from_le_bytes([buf[4], buf[5]]), 8000);
        assert_eq!(u16::from_le_bytes([buf[6], buf[7]]), 200);
        assert_eq!(u16::from_le_bytes([buf[8], buf[9]]), 500);
    }

    // -----------------------------------------------------------------------
    // Set Condition
    // -----------------------------------------------------------------------

    #[test]
    fn set_condition_report_id_and_length() {
        let buf = encode_set_condition(1, 0, 0, 0, 0, 0, 0, 0);
        assert_eq!(buf[0], 0x03);
        assert_eq!(buf.len(), SET_CONDITION_LEN);
    }

    #[test]
    fn set_condition_fields() {
        let buf = encode_set_condition(2, 0, -500, 3000, -2000, 10000, 10000, 50);
        assert_eq!(buf[1], 2);
        assert_eq!(buf[2], 0);
        assert_eq!(i16::from_le_bytes([buf[3], buf[4]]), -500);
        assert_eq!(i16::from_le_bytes([buf[5], buf[6]]), 3000);
        assert_eq!(i16::from_le_bytes([buf[7], buf[8]]), -2000);
        assert_eq!(u16::from_le_bytes([buf[9], buf[10]]), 10000);
        assert_eq!(u16::from_le_bytes([buf[11], buf[12]]), 10000);
        assert_eq!(buf[13], 50);
    }

    // -----------------------------------------------------------------------
    // Set Periodic
    // -----------------------------------------------------------------------

    #[test]
    fn set_periodic_report_id_and_length() {
        let buf = encode_set_periodic(1, 0, 0, 0, 100);
        assert_eq!(buf[0], 0x04);
        assert_eq!(buf.len(), SET_PERIODIC_LEN);
    }

    #[test]
    fn set_periodic_fields() {
        let buf = encode_set_periodic(4, 7500, -2000, 9000, 250);
        assert_eq!(buf[1], 4);
        assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), 7500);
        assert_eq!(i16::from_le_bytes([buf[4], buf[5]]), -2000);
        assert_eq!(u16::from_le_bytes([buf[6], buf[7]]), 9000);
        assert_eq!(u16::from_le_bytes([buf[8], buf[9]]), 250);
    }

    // -----------------------------------------------------------------------
    // Set Constant Force
    // -----------------------------------------------------------------------

    #[test]
    fn set_constant_force_report_id_and_length() {
        let buf = encode_set_constant_force(1, 0);
        assert_eq!(buf[0], 0x05);
        assert_eq!(buf.len(), SET_CONSTANT_FORCE_LEN);
    }

    #[test]
    fn set_constant_force_signed() {
        let buf = encode_set_constant_force(1, -5000);
        assert_eq!(i16::from_le_bytes([buf[2], buf[3]]), -5000);
    }

    // -----------------------------------------------------------------------
    // Set Ramp Force
    // -----------------------------------------------------------------------

    #[test]
    fn set_ramp_force_report_id_and_length() {
        let buf = encode_set_ramp_force(1, 0, 0);
        assert_eq!(buf[0], 0x06);
        assert_eq!(buf.len(), SET_RAMP_FORCE_LEN);
    }

    #[test]
    fn set_ramp_force_fields() {
        let buf = encode_set_ramp_force(2, -3000, 3000);
        assert_eq!(buf[1], 2);
        assert_eq!(i16::from_le_bytes([buf[2], buf[3]]), -3000);
        assert_eq!(i16::from_le_bytes([buf[4], buf[5]]), 3000);
    }

    // -----------------------------------------------------------------------
    // Effect Operation
    // -----------------------------------------------------------------------

    #[test]
    fn effect_operation_start() {
        let buf = encode_effect_operation(1, EffectOp::Start, 0);
        assert_eq!(buf[0], 0x0A);
        assert_eq!(buf[1], 1);
        assert_eq!(buf[2], 1);
        assert_eq!(buf[3], 0);
    }

    #[test]
    fn effect_operation_stop() {
        let buf = encode_effect_operation(3, EffectOp::Stop, 0);
        assert_eq!(buf[1], 3);
        assert_eq!(buf[2], 3);
    }

    #[test]
    fn effect_operation_start_solo_with_loops() {
        let buf = encode_effect_operation(2, EffectOp::StartSolo, 5);
        assert_eq!(buf[2], 2);
        assert_eq!(buf[3], 5);
    }

    // -----------------------------------------------------------------------
    // Block Free
    // -----------------------------------------------------------------------

    #[test]
    fn block_free_report() {
        let buf = encode_block_free(7);
        assert_eq!(buf[0], 0x0B);
        assert_eq!(buf[1], 7);
        assert_eq!(buf.len(), BLOCK_FREE_LEN);
    }

    // -----------------------------------------------------------------------
    // Device Control
    // -----------------------------------------------------------------------

    #[test]
    fn device_control_enable() {
        let buf = encode_device_control(device_control::ENABLE_ACTUATORS);
        assert_eq!(buf[0], 0x0C);
        assert_eq!(buf[1], 0x01);
    }

    #[test]
    fn device_control_disable() {
        let buf = encode_device_control(device_control::DISABLE_ACTUATORS);
        assert_eq!(buf[1], 0x02);
    }

    #[test]
    fn device_control_reset() {
        let buf = encode_device_control(device_control::DEVICE_RESET);
        assert_eq!(buf[1], 0x08);
    }

    // -----------------------------------------------------------------------
    // Device Gain
    // -----------------------------------------------------------------------

    #[test]
    fn device_gain_report_id_and_length() {
        let buf = encode_device_gain(5000);
        assert_eq!(buf[0], 0x0D);
        assert_eq!(buf.len(), DEVICE_GAIN_LEN);
    }

    #[test]
    fn device_gain_value() {
        let buf = encode_device_gain(7500);
        assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), 7500);
    }

    #[test]
    fn device_gain_clamps_to_max() {
        let buf = encode_device_gain(20000);
        assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), 10000);
    }

    // -----------------------------------------------------------------------
    // Block Load parser
    // -----------------------------------------------------------------------

    #[test]
    fn parse_block_load_success() {
        let buf = [5, 1, 0x00, 0x10]; // block 5, success, 4096 RAM
        let resp = parse_block_load(&buf);
        assert!(resp.is_some());
        let r = resp.as_ref();
        assert_eq!(r.map(|v| v.block_index), Some(5));
        assert_eq!(r.map(|v| v.status), Some(BlockLoadStatus::Success));
        assert_eq!(r.map(|v| v.ram_pool_available), Some(4096));
    }

    #[test]
    fn parse_block_load_full() {
        let buf = [1, 2, 0, 0];
        let resp = parse_block_load(&buf);
        assert_eq!(resp.as_ref().map(|v| v.status), Some(BlockLoadStatus::Full));
    }

    #[test]
    fn parse_block_load_error() {
        let buf = [1, 3, 0, 0];
        let resp = parse_block_load(&buf);
        assert_eq!(
            resp.as_ref().map(|v| v.status),
            Some(BlockLoadStatus::Error)
        );
    }

    #[test]
    fn parse_block_load_too_short() {
        assert!(parse_block_load(&[1, 2, 3]).is_none());
    }

    #[test]
    fn parse_block_load_invalid_status() {
        assert!(parse_block_load(&[1, 99, 0, 0]).is_none());
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(500))]

        #[test]
        fn prop_set_effect_always_correct_report_id(
            block in 1u8..=40u8,
            gain in 0u8..=255u8,
        ) {
            let params = SetEffect {
                block_index: block,
                gain,
                ..SetEffect::default()
            };
            let buf = encode_set_effect(&params);
            prop_assert_eq!(buf[0], report_ids::SET_EFFECT);
            prop_assert_eq!(buf[1], block);
            prop_assert_eq!(buf[9], gain);
        }

        #[test]
        fn prop_periodic_values_preserved(
            mag in 0u16..=10000u16,
            offset in -10000i16..=10000i16,
            phase in 0u16..=36000u16,
            period in 1u16..=10000u16,
        ) {
            let buf = encode_set_periodic(1, mag, offset, phase, period);
            prop_assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), mag);
            prop_assert_eq!(i16::from_le_bytes([buf[4], buf[5]]), offset);
            prop_assert_eq!(u16::from_le_bytes([buf[6], buf[7]]), phase);
            prop_assert_eq!(u16::from_le_bytes([buf[8], buf[9]]), period);
        }

        #[test]
        fn prop_condition_values_preserved(
            center in -10000i16..=10000i16,
            pos_coeff in -10000i16..=10000i16,
            neg_coeff in -10000i16..=10000i16,
        ) {
            let buf = encode_set_condition(1, 0, center, pos_coeff, neg_coeff, 10000, 10000, 0);
            prop_assert_eq!(i16::from_le_bytes([buf[3], buf[4]]), center);
            prop_assert_eq!(i16::from_le_bytes([buf[5], buf[6]]), pos_coeff);
            prop_assert_eq!(i16::from_le_bytes([buf[7], buf[8]]), neg_coeff);
        }

        #[test]
        fn prop_constant_force_sign(mag in -10000i16..=10000i16) {
            let buf = encode_set_constant_force(1, mag);
            prop_assert_eq!(i16::from_le_bytes([buf[2], buf[3]]), mag);
        }

        #[test]
        fn prop_ramp_values(start in -10000i16..=10000i16, end in -10000i16..=10000i16) {
            let buf = encode_set_ramp_force(1, start, end);
            prop_assert_eq!(i16::from_le_bytes([buf[2], buf[3]]), start);
            prop_assert_eq!(i16::from_le_bytes([buf[4], buf[5]]), end);
        }

        #[test]
        fn prop_device_gain_bounded(gain in 0u16..=20000u16) {
            let buf = encode_device_gain(gain);
            let encoded = u16::from_le_bytes([buf[2], buf[3]]);
            prop_assert!(encoded <= 10000);
        }
    }
}

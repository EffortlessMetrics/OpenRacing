//! Standard HID PID force feedback effect report encoders for OpenFFBoard.
//!
//! OpenFFBoard implements the full USB HID PID (Physical Interface Device)
//! specification. These encoders produce the standard PID output reports
//! for effect management.
//!
//! # Report IDs (from `ffb_defs.h`)
//!
//! | ID   | Name              | Description                      |
//! |------|-------------------|----------------------------------|
//! | 0x01 | Set Effect        | Configure effect parameters      |
//! | 0x02 | Set Envelope      | Attack/fade envelope             |
//! | 0x03 | Set Condition     | Spring/damper/friction/inertia   |
//! | 0x04 | Set Periodic      | Sine/square/triangle/sawtooth    |
//! | 0x05 | Constant Force    | Set constant force magnitude     |
//! | 0x06 | Ramp Force        | Set ramp start/end levels        |
//! | 0x0A | Effect Operation  | Play / stop / pause effects      |
//! | 0x0B | Block Free        | Delete an effect block           |
//! | 0x0C | Device Control    | Enable actuators, reset, etc.    |
//! | 0x0D | Device Gain       | Global gain scaler               |
//! | 0x11 | Create New Effect | Feature: allocate effect block   |
//! | 0x12 | Block Load        | Feature: allocation result       |
//! | 0x13 | PID Pool          | Feature: pool info               |
//!
//! # Effect types (from `ffb_defs.h`)
//!
//! | Value | Name          |
//! |-------|---------------|
//! | 0x01  | Constant      |
//! | 0x02  | Ramp          |
//! | 0x03  | Square        |
//! | 0x04  | Sine          |
//! | 0x05  | Triangle      |
//! | 0x06  | Sawtooth Up   |
//! | 0x07  | Sawtooth Down |
//! | 0x08  | Spring        |
//! | 0x09  | Damper        |
//! | 0x0A  | Inertia       |
//! | 0x0B  | Friction      |
//!
//! # Sources
//!
//! - `ffb_defs.h` — all struct definitions and constants
//! - `Firmware/FFBoard/Src/FFBoardMain.cpp` — PID effect handling
//! - USB HID PID specification (USB-IF Device Class Definition for PID)

// ---------------------------------------------------------------------------
// Report IDs
// ---------------------------------------------------------------------------

/// Set Effect Report (configure parameters for an effect block).
pub const REPORT_SET_EFFECT: u8 = 0x01;
/// Set Envelope Report (attack/fade for an effect).
pub const REPORT_SET_ENVELOPE: u8 = 0x02;
/// Set Condition Report (spring/damper/friction/inertia parameters).
pub const REPORT_SET_CONDITION: u8 = 0x03;
/// Set Periodic Report (sine/square/triangle/sawtooth parameters).
pub const REPORT_SET_PERIODIC: u8 = 0x04;
/// Set Constant Force Report (magnitude for constant force effect).
pub const REPORT_SET_CONSTANT: u8 = 0x05;
/// Set Ramp Force Report (start/end levels for ramp effect).
pub const REPORT_SET_RAMP: u8 = 0x06;
/// Effect Operation Report (play/stop/pause).
pub const REPORT_EFFECT_OP: u8 = 0x0A;
/// Block Free Report (delete an effect).
pub const REPORT_BLOCK_FREE: u8 = 0x0B;
/// Device Control Report (enable/disable actuators, reset).
pub const REPORT_DEVICE_CONTROL: u8 = 0x0C;
/// Device Gain Report (global gain scaler).
pub const REPORT_DEVICE_GAIN: u8 = 0x0D;
/// Create New Effect (feature report, host→device).
pub const REPORT_CREATE_EFFECT: u8 = 0x11;
/// Block Load (feature report, device→host response).
pub const REPORT_BLOCK_LOAD: u8 = 0x12;
/// PID Pool Report (feature report, device→host).
pub const REPORT_PID_POOL: u8 = 0x13;

/// Maximum number of simultaneous effects supported by firmware.
pub const MAX_EFFECTS: u8 = 40;

/// Infinite effect duration value.
pub const DURATION_INFINITE: u16 = 0xFFFF;

// ---------------------------------------------------------------------------
// Effect types
// ---------------------------------------------------------------------------

/// PID effect type identifiers.
///
/// Source: `FFB_EFFECT_*` constants in `ffb_defs.h`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum EffectType {
    /// Constant force (steady push/pull).
    Constant = 0x01,
    /// Ramp force (linear interpolation between start and end).
    Ramp = 0x02,
    /// Square wave periodic.
    Square = 0x03,
    /// Sine wave periodic.
    Sine = 0x04,
    /// Triangle wave periodic.
    Triangle = 0x05,
    /// Sawtooth up periodic.
    SawtoothUp = 0x06,
    /// Sawtooth down periodic.
    SawtoothDown = 0x07,
    /// Spring condition (position-dependent).
    Spring = 0x08,
    /// Damper condition (velocity-dependent).
    Damper = 0x09,
    /// Inertia condition (acceleration-dependent).
    Inertia = 0x0A,
    /// Friction condition (velocity-dependent with deadband).
    Friction = 0x0B,
}

// ---------------------------------------------------------------------------
// Effect operation states
// ---------------------------------------------------------------------------

/// Effect operation state for play/stop commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum EffectOp {
    /// Start playing the effect.
    Start = 1,
    /// Start playing solo (stop all others first).
    StartSolo = 2,
    /// Stop the effect.
    Stop = 3,
}

// ---------------------------------------------------------------------------
// Device control operations
// ---------------------------------------------------------------------------

/// Device control flags (bitmask).
pub mod device_control {
    /// Enable actuators.
    pub const ENABLE_ACTUATORS: u8 = 0x01;
    /// Disable actuators.
    pub const DISABLE_ACTUATORS: u8 = 0x02;
    /// Stop all effects.
    pub const STOP_ALL: u8 = 0x04;
    /// Reset device.
    pub const RESET: u8 = 0x08;
    /// Pause effects.
    pub const PAUSE: u8 = 0x10;
    /// Continue (unpause) effects.
    pub const CONTINUE: u8 = 0x20;
}

// ---------------------------------------------------------------------------
// Block load status
// ---------------------------------------------------------------------------

/// Result of a "Create New Effect" request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BlockLoadStatus {
    /// Effect was allocated successfully.
    Success = 1,
    /// Effect pool is full — no room.
    Full = 2,
    /// Error allocating the effect.
    Error = 3,
}

impl BlockLoadStatus {
    /// Parse from raw byte.
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            1 => Some(Self::Success),
            2 => Some(Self::Full),
            3 => Some(Self::Error),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Encode: Set Effect Report (0x01)
// ---------------------------------------------------------------------------

/// Encode a Set Effect Report.
///
/// # Wire format (18 bytes)
/// ```text
/// Byte  0: Report ID (0x01)
/// Byte  1: Effect block index (1..40)
/// Byte  2: Effect type
/// Bytes 3-4: Duration (u16 LE, ms; 0xFFFF = infinite)
/// Bytes 5-6: Trigger repeat interval (u16 LE, ms)
/// Bytes 7-8: Sample period (u16 LE, ms)
/// Bytes 9-10: Start delay (u16 LE, ms)
/// Byte 11: Gain (0-255)
/// Byte 12: Trigger button (unused, 0)
/// Byte 13: Enable axis flags (bit 0=X, bit 1=Y, bit 2=direction enable)
/// Bytes 14-15: Direction X (u16 LE, 0-36000 = 0-360°)
/// Bytes 16-17: Direction Y (u16 LE, 0-36000 = 0-360°)
/// ```
#[allow(clippy::too_many_arguments)]
pub fn encode_set_effect(
    block_index: u8,
    effect_type: EffectType,
    duration_ms: u16,
    gain: u8,
    direction_x: u16,
    start_delay_ms: u16,
) -> [u8; 18] {
    let mut buf = [0u8; 18];
    buf[0] = REPORT_SET_EFFECT;
    buf[1] = block_index;
    buf[2] = effect_type as u8;
    buf[3..5].copy_from_slice(&duration_ms.to_le_bytes());
    // trigger repeat interval = 0
    // sample period = 0
    buf[9..11].copy_from_slice(&start_delay_ms.to_le_bytes());
    buf[11] = gain;
    // trigger button = 0
    // enable axis: X enabled + direction enable
    buf[13] = 0x01 | 0x04;
    buf[14..16].copy_from_slice(&direction_x.to_le_bytes());
    buf
}

// ---------------------------------------------------------------------------
// Encode: Set Envelope Report (0x02)
// ---------------------------------------------------------------------------

/// Encode a Set Envelope Report.
///
/// # Wire format (14 bytes)
/// ```text
/// Byte  0: Report ID (0x02)
/// Byte  1: Effect block index (1..40)
/// Bytes 2-3: Attack level (u16 LE)
/// Bytes 4-5: Fade level (u16 LE)
/// Bytes 6-9: Attack time (u32 LE, ms)
/// Bytes 10-13: Fade time (u32 LE, ms)
/// ```
pub fn encode_set_envelope(
    block_index: u8,
    attack_level: u16,
    fade_level: u16,
    attack_time_ms: u32,
    fade_time_ms: u32,
) -> [u8; 14] {
    let mut buf = [0u8; 14];
    buf[0] = REPORT_SET_ENVELOPE;
    buf[1] = block_index;
    buf[2..4].copy_from_slice(&attack_level.to_le_bytes());
    buf[4..6].copy_from_slice(&fade_level.to_le_bytes());
    buf[6..10].copy_from_slice(&attack_time_ms.to_le_bytes());
    buf[10..14].copy_from_slice(&fade_time_ms.to_le_bytes());
    buf
}

// ---------------------------------------------------------------------------
// Encode: Set Condition Report (0x03)
// ---------------------------------------------------------------------------

/// Encode a Set Condition Report (spring, damper, friction, inertia).
///
/// # Wire format (14 bytes)
/// ```text
/// Byte  0: Report ID (0x03)
/// Byte  1: Effect block index
/// Byte  2: Parameter block offset (axis index in bits 0-3)
/// Bytes 3-4: Center point offset (i16 LE)
/// Bytes 5-6: Positive coefficient (i16 LE)
/// Bytes 7-8: Negative coefficient (i16 LE)
/// Bytes 9-10: Positive saturation (u16 LE)
/// Bytes 11-12: Negative saturation (u16 LE)
/// Bytes 13-14: Dead band (u16 LE)
/// ```
#[allow(clippy::too_many_arguments)]
pub fn encode_set_condition(
    block_index: u8,
    axis_index: u8,
    center_offset: i16,
    pos_coefficient: i16,
    neg_coefficient: i16,
    pos_saturation: u16,
    neg_saturation: u16,
    dead_band: u16,
) -> [u8; 15] {
    let mut buf = [0u8; 15];
    buf[0] = REPORT_SET_CONDITION;
    buf[1] = block_index;
    buf[2] = axis_index & 0x0F;
    buf[3..5].copy_from_slice(&center_offset.to_le_bytes());
    buf[5..7].copy_from_slice(&pos_coefficient.to_le_bytes());
    buf[7..9].copy_from_slice(&neg_coefficient.to_le_bytes());
    buf[9..11].copy_from_slice(&pos_saturation.to_le_bytes());
    buf[11..13].copy_from_slice(&neg_saturation.to_le_bytes());
    buf[13..15].copy_from_slice(&dead_band.to_le_bytes());
    buf
}

// ---------------------------------------------------------------------------
// Encode: Set Periodic Report (0x04)
// ---------------------------------------------------------------------------

/// Encode a Set Periodic Report (sine, square, triangle, sawtooth).
///
/// # Wire format (12 bytes)
/// ```text
/// Byte  0: Report ID (0x04)
/// Byte  1: Effect block index
/// Bytes 2-3: Magnitude (u16 LE)
/// Bytes 4-5: Offset (i16 LE)
/// Bytes 6-7: Phase (u16 LE, degrees)
/// Bytes 8-11: Period (u32 LE, ms)
/// ```
pub fn encode_set_periodic(
    block_index: u8,
    magnitude: u16,
    offset: i16,
    phase_deg: u16,
    period_ms: u32,
) -> [u8; 12] {
    let mut buf = [0u8; 12];
    buf[0] = REPORT_SET_PERIODIC;
    buf[1] = block_index;
    buf[2..4].copy_from_slice(&magnitude.to_le_bytes());
    buf[4..6].copy_from_slice(&offset.to_le_bytes());
    buf[6..8].copy_from_slice(&phase_deg.to_le_bytes());
    buf[8..12].copy_from_slice(&period_ms.to_le_bytes());
    buf
}

// ---------------------------------------------------------------------------
// Encode: Set Constant Force Report (0x05)
// ---------------------------------------------------------------------------

/// Encode a Set Constant Force Report.
///
/// # Wire format (4 bytes)
/// ```text
/// Byte  0: Report ID (0x05)
/// Byte  1: Effect block index
/// Bytes 2-3: Magnitude (i16 LE)
/// ```
pub fn encode_set_constant_force(block_index: u8, magnitude: i16) -> [u8; 4] {
    let mut buf = [0u8; 4];
    buf[0] = REPORT_SET_CONSTANT;
    buf[1] = block_index;
    buf[2..4].copy_from_slice(&magnitude.to_le_bytes());
    buf
}

// ---------------------------------------------------------------------------
// Encode: Set Ramp Force Report (0x06)
// ---------------------------------------------------------------------------

/// Encode a Set Ramp Force Report.
///
/// # Wire format (6 bytes)
/// ```text
/// Byte  0: Report ID (0x06)
/// Byte  1: Effect block index
/// Bytes 2-3: Start level (u16 LE)
/// Bytes 4-5: End level (u16 LE)
/// ```
pub fn encode_set_ramp(block_index: u8, start_level: u16, end_level: u16) -> [u8; 6] {
    let mut buf = [0u8; 6];
    buf[0] = REPORT_SET_RAMP;
    buf[1] = block_index;
    buf[2..4].copy_from_slice(&start_level.to_le_bytes());
    buf[4..6].copy_from_slice(&end_level.to_le_bytes());
    buf
}

// ---------------------------------------------------------------------------
// Encode: Effect Operation Report (0x0A)
// ---------------------------------------------------------------------------

/// Encode an Effect Operation Report (play / stop / pause).
///
/// # Wire format (4 bytes)
/// ```text
/// Byte  0: Report ID (0x0A)
/// Byte  1: Effect block index
/// Byte  2: Operation (1=start, 2=start solo, 3=stop)
/// Byte  3: Loop count (0 = infinite, 1+ = count)
/// ```
pub fn encode_effect_operation(block_index: u8, op: EffectOp, loop_count: u8) -> [u8; 4] {
    [REPORT_EFFECT_OP, block_index, op as u8, loop_count]
}

// ---------------------------------------------------------------------------
// Encode: Block Free Report (0x0B)
// ---------------------------------------------------------------------------

/// Encode a Block Free Report (delete effect).
///
/// # Wire format (2 bytes)
/// ```text
/// Byte 0: Report ID (0x0B)
/// Byte 1: Effect block index to free
/// ```
pub fn encode_block_free(block_index: u8) -> [u8; 2] {
    [REPORT_BLOCK_FREE, block_index]
}

// ---------------------------------------------------------------------------
// Encode: Device Control Report (0x0C)
// ---------------------------------------------------------------------------

/// Encode a Device Control Report.
///
/// Use `device_control::*` constants for the control byte.
pub fn encode_device_control(control: u8) -> [u8; 2] {
    [REPORT_DEVICE_CONTROL, control]
}

// ---------------------------------------------------------------------------
// Encode: Device Gain Report (0x0D)
// ---------------------------------------------------------------------------

/// Encode a Device Gain Report.
///
/// `gain` is in 0–255 where 255 = full strength.
pub fn encode_device_gain(gain: u8) -> [u8; 2] {
    [REPORT_DEVICE_GAIN, gain]
}

// ---------------------------------------------------------------------------
// Feature: Create New Effect (0x11)
// ---------------------------------------------------------------------------

/// Encode a Create New Effect feature report (host→device).
///
/// # Wire format (3 bytes, feature set report)
/// ```text
/// Byte 0: Report ID (0x11)
/// Byte 1: Effect type
/// Bytes 2-3: Byte count (usually 0)
/// ```
pub fn encode_create_effect(effect_type: EffectType) -> [u8; 4] {
    [REPORT_CREATE_EFFECT, effect_type as u8, 0x00, 0x00]
}

// ---------------------------------------------------------------------------
// Parse: Block Load response (0x12)
// ---------------------------------------------------------------------------

/// Parsed Block Load feature report (device→host).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockLoadResponse {
    /// Allocated effect block index (1..40).
    pub block_index: u8,
    /// Whether allocation succeeded.
    pub status: BlockLoadStatus,
    /// Remaining RAM pool entries available.
    pub ram_pool_available: u16,
}

/// Parse a Block Load feature response.
///
/// Expects at least 4 bytes (without report ID prefix, since tinyusb
/// strips it for feature GET reports):
/// ```text
/// Byte 0: Effect block index
/// Byte 1: Load status (1=success, 2=full, 3=error)
/// Bytes 2-3: RAM pool available (u16 LE)
/// ```
pub fn parse_block_load(buf: &[u8]) -> Option<BlockLoadResponse> {
    if buf.len() < 4 {
        return None;
    }
    let status = BlockLoadStatus::from_byte(buf[1])?;
    Some(BlockLoadResponse {
        block_index: buf[0],
        status,
        ram_pool_available: u16::from_le_bytes([buf[2], buf[3]]),
    })
}

// ---------------------------------------------------------------------------
// Parse: PID Pool Report (0x13)
// ---------------------------------------------------------------------------

/// Parsed PID Pool feature report (device→host).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PidPoolInfo {
    /// Total RAM pool size (number of effect slots).
    pub ram_pool_size: u16,
    /// Maximum simultaneous effects.
    pub max_simultaneous: u8,
    /// Memory management flags.
    pub memory_management: u8,
}

/// Parse a PID Pool feature response.
///
/// Expects at least 4 bytes (without report ID prefix):
/// ```text
/// Bytes 0-1: RAM pool size (u16 LE)
/// Byte  2: Max simultaneous effects
/// Byte  3: Memory management (0=device-managed, 1=shared params)
/// ```
pub fn parse_pid_pool(buf: &[u8]) -> Option<PidPoolInfo> {
    if buf.len() < 4 {
        return None;
    }
    Some(PidPoolInfo {
        ram_pool_size: u16::from_le_bytes([buf[0], buf[1]]),
        max_simultaneous: buf[2],
        memory_management: buf[3],
    })
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Report ID constants match firmware
    // -----------------------------------------------------------------------

    #[test]
    fn report_ids_match_firmware_defs() {
        // Cross-check against ffb_defs.h HID_ID_* constants
        assert_eq!(REPORT_SET_EFFECT, 0x01);
        assert_eq!(REPORT_SET_ENVELOPE, 0x02);
        assert_eq!(REPORT_SET_CONDITION, 0x03);
        assert_eq!(REPORT_SET_PERIODIC, 0x04);
        assert_eq!(REPORT_SET_CONSTANT, 0x05);
        assert_eq!(REPORT_SET_RAMP, 0x06);
        assert_eq!(REPORT_EFFECT_OP, 0x0A);
        assert_eq!(REPORT_BLOCK_FREE, 0x0B);
        assert_eq!(REPORT_DEVICE_CONTROL, 0x0C);
        assert_eq!(REPORT_DEVICE_GAIN, 0x0D);
        assert_eq!(REPORT_CREATE_EFFECT, 0x11);
        assert_eq!(REPORT_BLOCK_LOAD, 0x12);
        assert_eq!(REPORT_PID_POOL, 0x13);
    }

    #[test]
    fn effect_type_values_match_firmware() {
        // FFB_EFFECT_* in ffb_defs.h
        assert_eq!(EffectType::Constant as u8, 0x01);
        assert_eq!(EffectType::Ramp as u8, 0x02);
        assert_eq!(EffectType::Square as u8, 0x03);
        assert_eq!(EffectType::Sine as u8, 0x04);
        assert_eq!(EffectType::Triangle as u8, 0x05);
        assert_eq!(EffectType::SawtoothUp as u8, 0x06);
        assert_eq!(EffectType::SawtoothDown as u8, 0x07);
        assert_eq!(EffectType::Spring as u8, 0x08);
        assert_eq!(EffectType::Damper as u8, 0x09);
        assert_eq!(EffectType::Inertia as u8, 0x0A);
        assert_eq!(EffectType::Friction as u8, 0x0B);
    }

    #[test]
    fn max_effects_matches_firmware() {
        // MAX_EFFECTS = 40 in ffb_defs.h
        assert_eq!(MAX_EFFECTS, 40);
    }

    // -----------------------------------------------------------------------
    // Set Effect Report
    // -----------------------------------------------------------------------

    #[test]
    fn set_effect_report_id() {
        let buf = encode_set_effect(1, EffectType::Constant, 1000, 255, 0, 0);
        assert_eq!(buf[0], REPORT_SET_EFFECT);
    }

    #[test]
    fn set_effect_block_index() {
        let buf = encode_set_effect(5, EffectType::Sine, 0, 0, 0, 0);
        assert_eq!(buf[1], 5);
    }

    #[test]
    fn set_effect_type_encoding() {
        let buf = encode_set_effect(1, EffectType::Spring, 0, 0, 0, 0);
        assert_eq!(buf[2], EffectType::Spring as u8);
    }

    #[test]
    fn set_effect_duration_le16() {
        let buf = encode_set_effect(1, EffectType::Constant, 0x1234, 0, 0, 0);
        assert_eq!(u16::from_le_bytes([buf[3], buf[4]]), 0x1234);
    }

    #[test]
    fn set_effect_infinite_duration() {
        let buf = encode_set_effect(1, EffectType::Constant, DURATION_INFINITE, 0, 0, 0);
        assert_eq!(u16::from_le_bytes([buf[3], buf[4]]), 0xFFFF);
    }

    #[test]
    fn set_effect_gain() {
        let buf = encode_set_effect(1, EffectType::Constant, 0, 200, 0, 0);
        assert_eq!(buf[11], 200);
    }

    #[test]
    fn set_effect_direction() {
        let buf = encode_set_effect(1, EffectType::Constant, 0, 0, 18000, 0);
        assert_eq!(u16::from_le_bytes([buf[14], buf[15]]), 18000);
    }

    #[test]
    fn set_effect_start_delay() {
        let buf = encode_set_effect(1, EffectType::Constant, 0, 0, 0, 500);
        assert_eq!(u16::from_le_bytes([buf[9], buf[10]]), 500);
    }

    #[test]
    fn set_effect_length() {
        let buf = encode_set_effect(1, EffectType::Constant, 0, 0, 0, 0);
        assert_eq!(buf.len(), 18);
    }

    // -----------------------------------------------------------------------
    // Set Envelope Report
    // -----------------------------------------------------------------------

    #[test]
    fn set_envelope_report_id() {
        let buf = encode_set_envelope(1, 0, 0, 0, 0);
        assert_eq!(buf[0], REPORT_SET_ENVELOPE);
    }

    #[test]
    fn set_envelope_attack_fade() {
        let buf = encode_set_envelope(3, 1000, 500, 200, 300);
        assert_eq!(buf[1], 3);
        assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), 1000);
        assert_eq!(u16::from_le_bytes([buf[4], buf[5]]), 500);
        assert_eq!(u32::from_le_bytes([buf[6], buf[7], buf[8], buf[9]]), 200);
        assert_eq!(
            u32::from_le_bytes([buf[10], buf[11], buf[12], buf[13]]),
            300
        );
    }

    #[test]
    fn set_envelope_length() {
        let buf = encode_set_envelope(1, 0, 0, 0, 0);
        assert_eq!(buf.len(), 14);
    }

    // -----------------------------------------------------------------------
    // Set Condition Report
    // -----------------------------------------------------------------------

    #[test]
    fn set_condition_report_id() {
        let buf = encode_set_condition(1, 0, 0, 0, 0, 0, 0, 0);
        assert_eq!(buf[0], REPORT_SET_CONDITION);
    }

    #[test]
    fn set_condition_axis_mask() {
        let buf = encode_set_condition(1, 0x13, 0, 0, 0, 0, 0, 0);
        assert_eq!(buf[2], 0x03); // only lower 4 bits
    }

    #[test]
    fn set_condition_coefficients() {
        let buf = encode_set_condition(2, 0, 100, 500, -500, 10000, 10000, 200);
        assert_eq!(i16::from_le_bytes([buf[3], buf[4]]), 100);
        assert_eq!(i16::from_le_bytes([buf[5], buf[6]]), 500);
        assert_eq!(i16::from_le_bytes([buf[7], buf[8]]), -500);
        assert_eq!(u16::from_le_bytes([buf[9], buf[10]]), 10000);
        assert_eq!(u16::from_le_bytes([buf[11], buf[12]]), 10000);
        assert_eq!(u16::from_le_bytes([buf[13], buf[14]]), 200);
    }

    #[test]
    fn set_condition_length() {
        let buf = encode_set_condition(1, 0, 0, 0, 0, 0, 0, 0);
        assert_eq!(buf.len(), 15);
    }

    // -----------------------------------------------------------------------
    // Set Periodic Report
    // -----------------------------------------------------------------------

    #[test]
    fn set_periodic_report_id() {
        let buf = encode_set_periodic(1, 0, 0, 0, 0);
        assert_eq!(buf[0], REPORT_SET_PERIODIC);
    }

    #[test]
    fn set_periodic_params() {
        let buf = encode_set_periodic(5, 8000, -100, 90, 50);
        assert_eq!(buf[1], 5);
        assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), 8000);
        assert_eq!(i16::from_le_bytes([buf[4], buf[5]]), -100);
        assert_eq!(u16::from_le_bytes([buf[6], buf[7]]), 90);
        assert_eq!(u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]), 50);
    }

    #[test]
    fn set_periodic_length() {
        let buf = encode_set_periodic(1, 0, 0, 0, 0);
        assert_eq!(buf.len(), 12);
    }

    // -----------------------------------------------------------------------
    // Set Constant Force Report
    // -----------------------------------------------------------------------

    #[test]
    fn set_constant_force_report_id() {
        let buf = encode_set_constant_force(1, 0);
        assert_eq!(buf[0], REPORT_SET_CONSTANT);
    }

    #[test]
    fn set_constant_force_positive() {
        let buf = encode_set_constant_force(1, 10000);
        assert_eq!(i16::from_le_bytes([buf[2], buf[3]]), 10000);
    }

    #[test]
    fn set_constant_force_negative() {
        let buf = encode_set_constant_force(1, -10000);
        assert_eq!(i16::from_le_bytes([buf[2], buf[3]]), -10000);
    }

    #[test]
    fn set_constant_force_length() {
        let buf = encode_set_constant_force(1, 0);
        assert_eq!(buf.len(), 4);
    }

    // -----------------------------------------------------------------------
    // Set Ramp Force Report
    // -----------------------------------------------------------------------

    #[test]
    fn set_ramp_report_id() {
        let buf = encode_set_ramp(1, 0, 0);
        assert_eq!(buf[0], REPORT_SET_RAMP);
    }

    #[test]
    fn set_ramp_levels() {
        let buf = encode_set_ramp(2, 1000, 5000);
        assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), 1000);
        assert_eq!(u16::from_le_bytes([buf[4], buf[5]]), 5000);
    }

    #[test]
    fn set_ramp_length() {
        let buf = encode_set_ramp(1, 0, 0);
        assert_eq!(buf.len(), 6);
    }

    // -----------------------------------------------------------------------
    // Effect Operation Report
    // -----------------------------------------------------------------------

    #[test]
    fn effect_op_start() {
        let buf = encode_effect_operation(3, EffectOp::Start, 0);
        assert_eq!(buf, [REPORT_EFFECT_OP, 3, 1, 0]);
    }

    #[test]
    fn effect_op_stop() {
        let buf = encode_effect_operation(1, EffectOp::Stop, 0);
        assert_eq!(buf, [REPORT_EFFECT_OP, 1, 3, 0]);
    }

    #[test]
    fn effect_op_start_solo() {
        let buf = encode_effect_operation(2, EffectOp::StartSolo, 5);
        assert_eq!(buf, [REPORT_EFFECT_OP, 2, 2, 5]);
    }

    // -----------------------------------------------------------------------
    // Block Free Report
    // -----------------------------------------------------------------------

    #[test]
    fn block_free_report() {
        let buf = encode_block_free(7);
        assert_eq!(buf, [REPORT_BLOCK_FREE, 7]);
    }

    // -----------------------------------------------------------------------
    // Device Control Report
    // -----------------------------------------------------------------------

    #[test]
    fn device_control_enable() {
        let buf = encode_device_control(device_control::ENABLE_ACTUATORS);
        assert_eq!(buf, [REPORT_DEVICE_CONTROL, 0x01]);
    }

    #[test]
    fn device_control_reset() {
        let buf = encode_device_control(device_control::RESET);
        assert_eq!(buf, [REPORT_DEVICE_CONTROL, 0x08]);
    }

    // -----------------------------------------------------------------------
    // Device Gain Report
    // -----------------------------------------------------------------------

    #[test]
    fn device_gain_full() {
        let buf = encode_device_gain(255);
        assert_eq!(buf, [REPORT_DEVICE_GAIN, 255]);
    }

    #[test]
    fn device_gain_half() {
        let buf = encode_device_gain(128);
        assert_eq!(buf, [REPORT_DEVICE_GAIN, 128]);
    }

    // -----------------------------------------------------------------------
    // Create Effect Feature
    // -----------------------------------------------------------------------

    #[test]
    fn create_effect_constant() {
        let buf = encode_create_effect(EffectType::Constant);
        assert_eq!(buf[0], REPORT_CREATE_EFFECT);
        assert_eq!(buf[1], EffectType::Constant as u8);
    }

    #[test]
    fn create_effect_spring() {
        let buf = encode_create_effect(EffectType::Spring);
        assert_eq!(buf[1], EffectType::Spring as u8);
    }

    // -----------------------------------------------------------------------
    // Block Load Response parsing
    // -----------------------------------------------------------------------

    #[test]
    fn parse_block_load_success() {
        let buf = [5, 1, 0x25, 0x00]; // index=5, success, 37 remaining
        let resp = parse_block_load(&buf);
        assert!(resp.is_some());
        let r = resp.as_ref();
        assert_eq!(r.map(|v| v.block_index), Some(5));
        assert_eq!(r.map(|v| v.status), Some(BlockLoadStatus::Success));
        assert_eq!(r.map(|v| v.ram_pool_available), Some(37));
    }

    #[test]
    fn parse_block_load_full() {
        let buf = [0, 2, 0x00, 0x00];
        let resp = parse_block_load(&buf);
        assert!(resp.is_some());
        assert_eq!(resp.as_ref().map(|v| v.status), Some(BlockLoadStatus::Full));
    }

    #[test]
    fn parse_block_load_error() {
        let buf = [0, 3, 0x00, 0x00];
        let resp = parse_block_load(&buf);
        assert!(resp.is_some());
        assert_eq!(
            resp.as_ref().map(|v| v.status),
            Some(BlockLoadStatus::Error)
        );
    }

    #[test]
    fn parse_block_load_too_short() {
        assert_eq!(parse_block_load(&[0, 1, 0]), None);
    }

    #[test]
    fn parse_block_load_invalid_status() {
        let buf = [0, 99, 0x00, 0x00];
        assert_eq!(parse_block_load(&buf), None);
    }

    // -----------------------------------------------------------------------
    // PID Pool Response parsing
    // -----------------------------------------------------------------------

    #[test]
    fn parse_pid_pool_default() {
        // Default firmware: pool=40, max_simultaneous=40, shared_params=1
        let buf = [0x28, 0x00, 40, 1];
        let info = parse_pid_pool(&buf);
        assert!(info.is_some());
        let i = info.as_ref();
        assert_eq!(i.map(|v| v.ram_pool_size), Some(40));
        assert_eq!(i.map(|v| v.max_simultaneous), Some(40));
        assert_eq!(i.map(|v| v.memory_management), Some(1));
    }

    #[test]
    fn parse_pid_pool_too_short() {
        assert_eq!(parse_pid_pool(&[0, 0, 0]), None);
    }
}

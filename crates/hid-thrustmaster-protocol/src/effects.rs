//! Thrustmaster T300RS FFB effect wire format encoders.
//!
//! # Wire protocol (verified from Kimplul/hid-tmff2)
//!
//! The T300RS uses a custom vendor-specific FFB protocol sent via USB
//! interrupt OUT endpoint with Report ID `0x60` (normal mode, 63 bytes)
//! or `0x05` (PS4 mode, 31 bytes).
//!
//! ## Packet structure
//!
//! All effect packets share a 3-byte header:
//!
//! ```text
//! [0x00, effect_id+1, opcode]
//! ```
//!
//! ### Opcodes
//!
//! | Opcode | Purpose |
//! |--------|---------|
//! | `0x6a` | Upload / modify constant force |
//! | `0x6b` | Upload / modify ramp or periodic |
//! | `0x64` | Upload / modify condition (spring/damper/friction/inertia) |
//! | `0x89` | Play / stop effect |
//!
//! ### Timing block (9 bytes)
//!
//! ```text
//! [0x4f, duration_lo, duration_hi, 0x00, 0x00, offset_lo, offset_hi, 0x00, 0xff, 0xff]
//! ```
//!
//! Duration `0xFFFF` means infinite.
//!
//! ### Envelope block (8 bytes)
//!
//! ```text
//! [attack_len_lo, attack_len_hi, attack_level_lo, attack_level_hi,
//!  fade_len_lo, fade_len_hi, fade_level_lo, fade_level_hi]
//! ```
//!
//! ## Setup commands (2-byte header: `[cmd, code]`)
//!
//! | Cmd  | Code | Purpose |
//! |------|------|---------|
//! | `0x01` | `0x05` | Open (enable FFB) |
//! | `0x01` | `0x00` | Close (disable FFB) |
//! | `0x02` | `gain>>8` | Set global gain |
//! | `0x08` | `0x11` | Set range (+ 2-byte scaled value) |
//! | `0x08` | `0x04` | Autocenter enable (+ `0x01`) |
//! | `0x08` | `0x03` | Autocenter strength (+ 2-byte value) |
//!
//! ## Mode switch (USB control transfer)
//!
//! - Normal mode (base): `bRequest=83, wValue=5`
//! - Advanced mode (F1): `bRequest=83, wValue=3`

/// Maximum number of simultaneous effects.
pub const MAX_EFFECTS: u8 = 16;

/// Normal-mode buffer length (Report ID 0x60, 63 payload bytes).
pub const NORM_BUFFER_LENGTH: usize = 63;

/// PS4-mode buffer length (Report ID 0x05, 31 payload bytes).
pub const PS4_BUFFER_LENGTH: usize = 31;

/// Range: minimum steering angle in degrees.
pub const MIN_RANGE: u16 = 40;

/// Range: maximum steering angle in degrees.
pub const MAX_RANGE: u16 = 1080;

/// Range scale factor (value × 0x3c).
pub const RANGE_SCALE: u16 = 0x3c;

/// Timing block start marker.
pub const TIMING_START_MARKER: u8 = 0x4f;

/// Timing block end marker.
pub const TIMING_END_MARKER: u16 = 0xFFFF;

/// Duration value meaning "infinite".
pub const INFINITE_DURATION: u16 = 0xFFFF;

/// Condition hardcoded values (from `condition_values[]` in the driver).
pub const CONDITION_HARDCODED: [u8; 8] = [0xfe, 0xff, 0xfe, 0xff, 0xfe, 0xff, 0xfe, 0xff];

/// Spring max saturation.
pub const SPRING_MAX_SATURATION: u16 = 0x6aa6;

/// Default condition max saturation (for damper/friction/inertia).
pub const DEFAULT_MAX_SATURATION: u16 = 0xFFFF;

// ---------------------------------------------------------------------------
// Waveform types (FF_* - 0x57)
// ---------------------------------------------------------------------------

/// Waveform byte values for periodic effects.
///
/// The kernel uses `periodic.waveform - 0x57` to compute the wire byte.
/// FF_SQUARE=0x58, FF_TRIANGLE=0x59, FF_SINE=0x5a, FF_SAW_UP=0x5b, FF_SAW_DOWN=0x5c.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Waveform {
    /// FF_SQUARE (0x58 - 0x57 = 0x01)
    Square = 0x01,
    /// FF_TRIANGLE (0x59 - 0x57 = 0x02)
    Triangle = 0x02,
    /// FF_SINE (0x5a - 0x57 = 0x03)
    Sine = 0x03,
    /// FF_SAW_UP (0x5b - 0x57 = 0x04)
    SawUp = 0x04,
    /// FF_SAW_DOWN (0x5c - 0x57 = 0x05)
    SawDown = 0x05,
}

/// Condition effect sub-type byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ConditionType {
    /// Spring effect.
    Spring = 0x00,
    /// Damper / friction / inertia effect.
    Other = 0x01,
}

// ---------------------------------------------------------------------------
// Envelope
// ---------------------------------------------------------------------------

/// FFB envelope parameters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Envelope {
    pub attack_length: u16,
    pub attack_level: u16,
    pub fade_length: u16,
    pub fade_level: u16,
}

impl Envelope {
    /// Encode to 8 bytes (LE16 × 4).
    pub fn encode(&self) -> [u8; 8] {
        let mut buf = [0u8; 8];
        buf[0..2].copy_from_slice(&self.attack_length.to_le_bytes());
        buf[2..4].copy_from_slice(&self.attack_level.to_le_bytes());
        buf[4..6].copy_from_slice(&self.fade_length.to_le_bytes());
        buf[6..8].copy_from_slice(&self.fade_level.to_le_bytes());
        buf
    }
}

// ---------------------------------------------------------------------------
// Timing
// ---------------------------------------------------------------------------

/// FFB timing parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timing {
    pub duration: u16,
    pub offset: u16,
}

impl Timing {
    /// Create with infinite duration and zero offset.
    pub fn infinite() -> Self {
        Self { duration: INFINITE_DURATION, offset: 0 }
    }

    /// Encode to 9 bytes: `[0x4f, dur_lo, dur_hi, 0, 0, off_lo, off_hi, 0, 0xff, 0xff]`.
    pub fn encode(&self) -> [u8; 10] {
        let mut buf = [0u8; 10];
        buf[0] = TIMING_START_MARKER;
        buf[1..3].copy_from_slice(&self.duration.to_le_bytes());
        // buf[3..5] = zeroes
        buf[5..7].copy_from_slice(&self.offset.to_le_bytes());
        // buf[7] = zero
        let end = TIMING_END_MARKER.to_le_bytes();
        buf[8] = end[0];
        buf[9] = end[1];
        buf
    }
}

/// Convert a replay length (0 = infinite in Linux) to device duration.
pub fn calculate_duration(length: u16) -> u16 {
    if length == 0 { INFINITE_DURATION } else { length }
}

// ---------------------------------------------------------------------------
// Effect header
// ---------------------------------------------------------------------------

/// Encode the 3-byte effect packet header.
///
/// `effect_id`: 0-based effect index. Wire format uses `id + 1`.
/// `opcode`: operation code (e.g. `0x6a`, `0x6b`, `0x64`, `0x89`).
pub fn encode_header(effect_id: u8, opcode: u8) -> [u8; 3] {
    [0x00, effect_id.wrapping_add(1), opcode]
}

// ---------------------------------------------------------------------------
// Constant force effect
// ---------------------------------------------------------------------------

/// Encode a constant force effect upload packet.
///
/// Returns up to 24 bytes:
/// - 3-byte header (opcode 0x6a)
/// - 2-byte magnitude (LE16)
/// - 8-byte envelope
/// - 1-byte zero
/// - 10-byte timing
pub fn encode_constant_upload(
    effect_id: u8,
    level: i16,
    envelope: &Envelope,
    duration: u16,
    offset: u16,
) -> [u8; 24] {
    let mut buf = [0u8; 24];
    let hdr = encode_header(effect_id, 0x6a);
    buf[0..3].copy_from_slice(&hdr);
    buf[3..5].copy_from_slice(&level.to_le_bytes());
    buf[5..13].copy_from_slice(&envelope.encode());
    // buf[13] = 0x00
    let timing = Timing { duration, offset };
    buf[14..24].copy_from_slice(&timing.encode());
    buf
}

/// Encode a constant force effect modify packet.
///
/// Returns up to 16 bytes:
/// - 3-byte header (opcode 0x6a)
/// - 2-byte magnitude (LE16)
/// - 8-byte envelope
/// - 1-byte effect_type (0x00)
/// - 1-byte update_type (0x45)
/// - 2-byte duration (LE16)
/// - 2-byte offset (LE16)
///
/// This is the "modification" format; it updates an already-uploaded effect.
pub fn encode_constant_modify(
    effect_id: u8,
    level: i16,
    envelope: &Envelope,
    duration: u16,
    offset: u16,
) -> [u8; 19] {
    let mut buf = [0u8; 19];
    let hdr = encode_header(effect_id, 0x6a);
    buf[0..3].copy_from_slice(&hdr);
    buf[3..5].copy_from_slice(&level.to_le_bytes());
    buf[5..13].copy_from_slice(&envelope.encode());
    buf[13] = 0x00; // effect_type
    buf[14] = 0x45; // update_type
    buf[15..17].copy_from_slice(&duration.to_le_bytes());
    buf[17..19].copy_from_slice(&offset.to_le_bytes());
    buf
}

// ---------------------------------------------------------------------------
// Periodic effect
// ---------------------------------------------------------------------------

/// Encode a periodic effect upload packet.
///
/// Returns 26 bytes:
/// - 3-byte header (opcode 0x6b)
/// - 2-byte magnitude (LE16)
/// - 2-byte periodic_offset (LE16)
/// - 2-byte phase (LE16)
/// - 2-byte period (LE16)
/// - 2-byte marker (0x8000 LE)
/// - 8-byte envelope
/// - 1-byte waveform
/// - 10-byte timing
#[allow(clippy::too_many_arguments)]
pub fn encode_periodic_upload(
    effect_id: u8,
    magnitude: i16,
    periodic_offset: i16,
    phase: u16,
    period: u16,
    waveform: Waveform,
    envelope: &Envelope,
    duration: u16,
    offset: u16,
) -> [u8; 32] {
    let mut buf = [0u8; 32];
    let hdr = encode_header(effect_id, 0x6b);
    buf[0..3].copy_from_slice(&hdr);
    buf[3..5].copy_from_slice(&magnitude.to_le_bytes());
    buf[5..7].copy_from_slice(&periodic_offset.to_le_bytes());
    buf[7..9].copy_from_slice(&phase.to_le_bytes());
    buf[9..11].copy_from_slice(&period.to_le_bytes());
    buf[11..13].copy_from_slice(&0x8000u16.to_le_bytes());
    buf[13..21].copy_from_slice(&envelope.encode());
    buf[21] = waveform as u8;
    let timing = Timing { duration, offset };
    buf[22..32].copy_from_slice(&timing.encode());
    buf
}

// ---------------------------------------------------------------------------
// Ramp effect
// ---------------------------------------------------------------------------

/// Encode a ramp effect upload packet.
///
/// Returns 26 bytes:
/// - 3-byte header (opcode 0x6b)
/// - 2-byte slope (LE16)
/// - 2-byte center (LE16)
/// - 2-byte zero padding
/// - 2-byte duration (LE16)
/// - 2-byte marker (0x8000 LE)
/// - 8-byte envelope
/// - 1-byte invert flag
/// - 10-byte timing
#[allow(clippy::too_many_arguments)]
pub fn encode_ramp_upload(
    effect_id: u8,
    slope: u16,
    center: i16,
    invert: u8,
    envelope: &Envelope,
    duration: u16,
    offset: u16,
) -> [u8; 32] {
    let mut buf = [0u8; 32];
    let hdr = encode_header(effect_id, 0x6b);
    buf[0..3].copy_from_slice(&hdr);
    buf[3..5].copy_from_slice(&slope.to_le_bytes());
    buf[5..7].copy_from_slice(&center.to_le_bytes());
    // buf[7..9] = zero padding
    buf[9..11].copy_from_slice(&duration.to_le_bytes());
    buf[11..13].copy_from_slice(&0x8000u16.to_le_bytes());
    buf[13..21].copy_from_slice(&envelope.encode());
    buf[21] = invert;
    let timing = Timing { duration, offset };
    buf[22..32].copy_from_slice(&timing.encode());
    buf
}

// ---------------------------------------------------------------------------
// Condition effect (spring / damper / friction / inertia)
// ---------------------------------------------------------------------------

/// Encode a condition effect upload packet.
///
/// Returns 36 bytes:
/// - 3-byte header (opcode 0x64)
/// - 2-byte right_coeff (LE16)
/// - 2-byte left_coeff (LE16)
/// - 2-byte right_deadband (LE16)
/// - 2-byte left_deadband (LE16)
/// - 2-byte right_saturation (LE16)
/// - 2-byte left_saturation (LE16)
/// - 8-byte hardcoded values
/// - 2-byte max_right_saturation (LE16)
/// - 2-byte max_left_saturation (LE16)
/// - 1-byte condition type
/// - 10-byte timing
#[allow(clippy::too_many_arguments)]
pub fn encode_condition_upload(
    effect_id: u8,
    right_coeff: i16,
    left_coeff: i16,
    right_deadband: i16,
    left_deadband: i16,
    right_saturation: u16,
    left_saturation: u16,
    condition_type: ConditionType,
    duration: u16,
    offset: u16,
) -> [u8; 38] {
    let max_sat = match condition_type {
        ConditionType::Spring => SPRING_MAX_SATURATION,
        ConditionType::Other => DEFAULT_MAX_SATURATION,
    };

    let mut buf = [0u8; 38];
    let hdr = encode_header(effect_id, 0x64);
    buf[0..3].copy_from_slice(&hdr);
    buf[3..5].copy_from_slice(&right_coeff.to_le_bytes());
    buf[5..7].copy_from_slice(&left_coeff.to_le_bytes());
    buf[7..9].copy_from_slice(&right_deadband.to_le_bytes());
    buf[9..11].copy_from_slice(&left_deadband.to_le_bytes());
    buf[11..13].copy_from_slice(&right_saturation.to_le_bytes());
    buf[13..15].copy_from_slice(&left_saturation.to_le_bytes());
    buf[15..23].copy_from_slice(&CONDITION_HARDCODED);
    buf[23..25].copy_from_slice(&max_sat.to_le_bytes());
    buf[25..27].copy_from_slice(&max_sat.to_le_bytes());
    buf[27] = condition_type as u8;
    let timing = Timing { duration, offset };
    buf[28..38].copy_from_slice(&timing.encode());
    buf
}

// ---------------------------------------------------------------------------
// Play / stop
// ---------------------------------------------------------------------------

/// Encode a play effect command.
///
/// `count`: 0 for infinite, otherwise the number of iterations.
pub fn encode_play(effect_id: u8, count: u16) -> [u8; 5] {
    let mut buf = [0u8; 5];
    let hdr = encode_header(effect_id, 0x89);
    buf[0..3].copy_from_slice(&hdr);
    buf[3..5].copy_from_slice(&count.to_le_bytes());
    buf
}

/// Encode a stop effect command.
pub fn encode_stop(effect_id: u8) -> [u8; 5] {
    let mut buf = [0u8; 5];
    let hdr = encode_header(effect_id, 0x89);
    buf[0..3].copy_from_slice(&hdr);
    // count = 0 means stop
    buf
}

// ---------------------------------------------------------------------------
// Setup commands
// ---------------------------------------------------------------------------

/// Encode an FFB "open" command (enable force feedback).
///
/// Wire: `[0x01, 0x05, 0, ...]`
pub fn encode_open() -> [u8; 2] {
    [0x01, 0x05]
}

/// Encode an FFB "close" command (disable force feedback).
///
/// Wire: `[0x01, 0x00, 0, ...]`
pub fn encode_close() -> [u8; 2] {
    [0x01, 0x00]
}

/// Encode a global gain command.
///
/// `gain`: 0–0xFFFF (the high byte is sent as the code).
///
/// Wire: `[0x02, gain_hi]`
pub fn encode_gain(gain: u16) -> [u8; 2] {
    [0x02, (gain >> 8) as u8]
}

/// Encode a steering range command.
///
/// `degrees`: 40–1080.
///
/// Wire: `[0x08, 0x11, scaled_lo, scaled_hi]` where `scaled = degrees × 0x3c`.
pub fn encode_range(degrees: u16) -> [u8; 4] {
    let clamped = degrees.clamp(MIN_RANGE, MAX_RANGE);
    let scaled = clamped.wrapping_mul(RANGE_SCALE);
    let mut buf = [0u8; 4];
    buf[0] = 0x08;
    buf[1] = 0x11;
    buf[2..4].copy_from_slice(&scaled.to_le_bytes());
    buf
}

/// Encode an autocenter-enable + strength sequence.
///
/// Returns two commands:
/// 1. Enable: `[0x08, 0x04, 0x01, 0x00]`
/// 2. Strength: `[0x08, 0x03, value_lo, value_hi]`
pub fn encode_autocenter(strength: u16) -> ([u8; 4], [u8; 4]) {
    let enable = [0x08u8, 0x04, 0x01, 0x00];
    let mut set = [0x08u8, 0x03, 0x00, 0x00];
    set[2..4].copy_from_slice(&strength.to_le_bytes());
    (enable, set)
}

/// Mode switch USB control transfer parameters.
///
/// Used with `usb_control_msg(bRequestType=0x41, bRequest=83, wValue=...)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModeSwitch {
    /// USB bRequest value (always 83).
    pub b_request: u8,
    /// USB bRequestType value (always 0x41).
    pub b_request_type: u8,
    /// USB wValue field: 5 = normal/base, 3 = advanced/F1.
    pub w_value: u16,
}

/// Encode a mode switch to normal (base) mode.
pub fn mode_switch_normal() -> ModeSwitch {
    ModeSwitch {
        b_request: 83,
        b_request_type: 0x41,
        w_value: 5,
    }
}

/// Encode a mode switch to advanced (F1 attachment) mode.
pub fn mode_switch_advanced() -> ModeSwitch {
    ModeSwitch {
        b_request: 83,
        b_request_type: 0x41,
        w_value: 3,
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Header
    // -----------------------------------------------------------------------

    #[test]
    fn header_effect_id_incremented() {
        let h = encode_header(0, 0x6a);
        assert_eq!(h, [0x00, 0x01, 0x6a]);
    }

    #[test]
    fn header_effect_id_15() {
        let h = encode_header(15, 0x89);
        assert_eq!(h, [0x00, 16, 0x89]);
    }

    // -----------------------------------------------------------------------
    // Envelope
    // -----------------------------------------------------------------------

    #[test]
    fn envelope_default_is_zero() {
        let e = Envelope::default();
        assert_eq!(e.encode(), [0; 8]);
    }

    #[test]
    fn envelope_encode_le() {
        let e = Envelope {
            attack_length: 0x1234,
            attack_level: 0x5678,
            fade_length: 0x9abc,
            fade_level: 0xdef0,
        };
        let buf = e.encode();
        assert_eq!(buf[0..2], [0x34, 0x12]);
        assert_eq!(buf[2..4], [0x78, 0x56]);
        assert_eq!(buf[4..6], [0xbc, 0x9a]);
        assert_eq!(buf[6..8], [0xf0, 0xde]);
    }

    // -----------------------------------------------------------------------
    // Timing
    // -----------------------------------------------------------------------

    #[test]
    fn timing_infinite() {
        let t = Timing::infinite();
        assert_eq!(t.duration, 0xFFFF);
        assert_eq!(t.offset, 0);
    }

    #[test]
    fn timing_encode_start_end_markers() {
        let t = Timing { duration: 1000, offset: 500 };
        let buf = t.encode();
        assert_eq!(buf[0], TIMING_START_MARKER);
        assert_eq!(buf[8], 0xFF);
        assert_eq!(buf[9], 0xFF);
    }

    #[test]
    fn timing_encode_duration_offset() {
        let t = Timing { duration: 0x1234, offset: 0x5678 };
        let buf = t.encode();
        assert_eq!(buf[1..3], [0x34, 0x12]);
        assert_eq!(buf[5..7], [0x78, 0x56]);
    }

    #[test]
    fn calculate_duration_zero_is_infinite() {
        assert_eq!(calculate_duration(0), INFINITE_DURATION);
    }

    #[test]
    fn calculate_duration_nonzero_passthrough() {
        assert_eq!(calculate_duration(500), 500);
    }

    // -----------------------------------------------------------------------
    // Constant force
    // -----------------------------------------------------------------------

    #[test]
    fn constant_upload_header() {
        let buf = encode_constant_upload(0, 0, &Envelope::default(), 0xFFFF, 0);
        assert_eq!(buf[0..3], [0x00, 0x01, 0x6a]);
    }

    #[test]
    fn constant_upload_level() {
        let buf = encode_constant_upload(0, -1000, &Envelope::default(), 0xFFFF, 0);
        let level = i16::from_le_bytes([buf[3], buf[4]]);
        assert_eq!(level, -1000);
    }

    #[test]
    fn constant_upload_timing() {
        let buf = encode_constant_upload(5, 0, &Envelope::default(), 2000, 100);
        // Timing starts at byte 14
        assert_eq!(buf[14], TIMING_START_MARKER);
        let dur = u16::from_le_bytes([buf[15], buf[16]]);
        let off = u16::from_le_bytes([buf[19], buf[20]]);
        assert_eq!(dur, 2000);
        assert_eq!(off, 100);
    }

    #[test]
    fn constant_modify_opcode() {
        let buf = encode_constant_modify(0, 0, &Envelope::default(), 0xFFFF, 0);
        assert_eq!(buf[2], 0x6a);
    }

    #[test]
    fn constant_modify_update_type() {
        let buf = encode_constant_modify(0, 0, &Envelope::default(), 0xFFFF, 0);
        assert_eq!(buf[13], 0x00); // effect_type
        assert_eq!(buf[14], 0x45); // update_type
    }

    // -----------------------------------------------------------------------
    // Periodic effect
    // -----------------------------------------------------------------------

    #[test]
    fn periodic_upload_header() {
        let buf = encode_periodic_upload(
            0, 0, 0, 0, 100, Waveform::Sine, &Envelope::default(), 0xFFFF, 0,
        );
        assert_eq!(buf[0..3], [0x00, 0x01, 0x6b]);
    }

    #[test]
    fn periodic_upload_waveform_sine() {
        let buf = encode_periodic_upload(
            0, 0, 0, 0, 100, Waveform::Sine, &Envelope::default(), 0xFFFF, 0,
        );
        assert_eq!(buf[21], 0x03);
    }

    #[test]
    fn periodic_upload_waveform_square() {
        let buf = encode_periodic_upload(
            0, 0, 0, 0, 100, Waveform::Square, &Envelope::default(), 0xFFFF, 0,
        );
        assert_eq!(buf[21], 0x01);
    }

    #[test]
    fn periodic_upload_marker() {
        let buf = encode_periodic_upload(
            0, 0, 0, 0, 100, Waveform::Sine, &Envelope::default(), 0xFFFF, 0,
        );
        assert_eq!(buf[11..13], 0x8000u16.to_le_bytes());
    }

    #[test]
    fn periodic_upload_magnitude() {
        let buf = encode_periodic_upload(
            0, 16000, 0, 0, 100, Waveform::Sine, &Envelope::default(), 0xFFFF, 0,
        );
        let mag = i16::from_le_bytes([buf[3], buf[4]]);
        assert_eq!(mag, 16000);
    }

    // -----------------------------------------------------------------------
    // Ramp effect
    // -----------------------------------------------------------------------

    #[test]
    fn ramp_upload_header() {
        let buf = encode_ramp_upload(0, 100, 0, 0, &Envelope::default(), 0xFFFF, 0);
        assert_eq!(buf[0..3], [0x00, 0x01, 0x6b]);
    }

    #[test]
    fn ramp_upload_slope_center() {
        let buf = encode_ramp_upload(0, 0x1234, -500, 1, &Envelope::default(), 0xFFFF, 0);
        let slope = u16::from_le_bytes([buf[3], buf[4]]);
        let center = i16::from_le_bytes([buf[5], buf[6]]);
        assert_eq!(slope, 0x1234);
        assert_eq!(center, -500);
    }

    #[test]
    fn ramp_upload_invert_flag() {
        let buf = encode_ramp_upload(0, 100, 0, 1, &Envelope::default(), 0xFFFF, 0);
        assert_eq!(buf[21], 1);
    }

    // -----------------------------------------------------------------------
    // Condition effect
    // -----------------------------------------------------------------------

    #[test]
    fn condition_upload_header() {
        let buf = encode_condition_upload(
            0, 0, 0, 0, 0, 0, 0, ConditionType::Spring, 0xFFFF, 0,
        );
        assert_eq!(buf[0..3], [0x00, 0x01, 0x64]);
    }

    #[test]
    fn condition_upload_spring_type() {
        let buf = encode_condition_upload(
            0, 0, 0, 0, 0, 0, 0, ConditionType::Spring, 0xFFFF, 0,
        );
        assert_eq!(buf[27], 0x00);
    }

    #[test]
    fn condition_upload_damper_type() {
        let buf = encode_condition_upload(
            0, 0, 0, 0, 0, 0, 0, ConditionType::Other, 0xFFFF, 0,
        );
        assert_eq!(buf[27], 0x01);
    }

    #[test]
    fn condition_upload_spring_max_sat() {
        let buf = encode_condition_upload(
            0, 0, 0, 0, 0, 0, 0, ConditionType::Spring, 0xFFFF, 0,
        );
        let max_r = u16::from_le_bytes([buf[23], buf[24]]);
        let max_l = u16::from_le_bytes([buf[25], buf[26]]);
        assert_eq!(max_r, SPRING_MAX_SATURATION);
        assert_eq!(max_l, SPRING_MAX_SATURATION);
    }

    #[test]
    fn condition_upload_damper_max_sat() {
        let buf = encode_condition_upload(
            0, 0, 0, 0, 0, 0, 0, ConditionType::Other, 0xFFFF, 0,
        );
        let max_r = u16::from_le_bytes([buf[23], buf[24]]);
        assert_eq!(max_r, DEFAULT_MAX_SATURATION);
    }

    #[test]
    fn condition_upload_hardcoded_values() {
        let buf = encode_condition_upload(
            0, 100, -100, 50, -50, 0x1000, 0x2000, ConditionType::Spring, 0xFFFF, 0,
        );
        assert_eq!(&buf[15..23], &CONDITION_HARDCODED);
    }

    #[test]
    fn condition_upload_coefficients() {
        let buf = encode_condition_upload(
            0, 1000, -2000, 50, -50, 0, 0, ConditionType::Spring, 0xFFFF, 0,
        );
        let rc = i16::from_le_bytes([buf[3], buf[4]]);
        let lc = i16::from_le_bytes([buf[5], buf[6]]);
        assert_eq!(rc, 1000);
        assert_eq!(lc, -2000);
    }

    // -----------------------------------------------------------------------
    // Play / Stop
    // -----------------------------------------------------------------------

    #[test]
    fn play_effect() {
        let buf = encode_play(3, 1);
        assert_eq!(buf, [0x00, 4, 0x89, 0x01, 0x00]);
    }

    #[test]
    fn play_infinite() {
        let buf = encode_play(0, 0);
        assert_eq!(buf, [0x00, 1, 0x89, 0x00, 0x00]);
    }

    #[test]
    fn stop_effect() {
        let buf = encode_stop(5);
        assert_eq!(buf, [0x00, 6, 0x89, 0x00, 0x00]);
    }

    // -----------------------------------------------------------------------
    // Setup commands
    // -----------------------------------------------------------------------

    #[test]
    fn open_command() {
        assert_eq!(encode_open(), [0x01, 0x05]);
    }

    #[test]
    fn close_command() {
        assert_eq!(encode_close(), [0x01, 0x00]);
    }

    #[test]
    fn gain_full() {
        let buf = encode_gain(0xFFFF);
        assert_eq!(buf, [0x02, 0xFF]);
    }

    #[test]
    fn gain_half() {
        let buf = encode_gain(0x8000);
        assert_eq!(buf, [0x02, 0x80]);
    }

    #[test]
    fn gain_zero() {
        let buf = encode_gain(0);
        assert_eq!(buf, [0x02, 0x00]);
    }

    // -----------------------------------------------------------------------
    // Range
    // -----------------------------------------------------------------------

    #[test]
    fn range_1080() {
        let buf = encode_range(1080);
        assert_eq!(buf[0], 0x08);
        assert_eq!(buf[1], 0x11);
        let scaled = u16::from_le_bytes([buf[2], buf[3]]);
        assert_eq!(scaled, 1080 * RANGE_SCALE);
    }

    #[test]
    fn range_clamp_low() {
        let buf = encode_range(10);
        let scaled = u16::from_le_bytes([buf[2], buf[3]]);
        assert_eq!(scaled, MIN_RANGE * RANGE_SCALE);
    }

    #[test]
    fn range_clamp_high() {
        let buf = encode_range(2000);
        let scaled = u16::from_le_bytes([buf[2], buf[3]]);
        assert_eq!(scaled, MAX_RANGE * RANGE_SCALE);
    }

    #[test]
    fn range_270() {
        let buf = encode_range(270);
        let scaled = u16::from_le_bytes([buf[2], buf[3]]);
        assert_eq!(scaled, 270 * RANGE_SCALE);
    }

    // -----------------------------------------------------------------------
    // Autocenter
    // -----------------------------------------------------------------------

    #[test]
    fn autocenter_enable_cmd() {
        let (enable, _) = encode_autocenter(0x4000);
        assert_eq!(enable, [0x08, 0x04, 0x01, 0x00]);
    }

    #[test]
    fn autocenter_strength() {
        let (_, strength) = encode_autocenter(0x4000);
        assert_eq!(strength[0], 0x08);
        assert_eq!(strength[1], 0x03);
        let val = u16::from_le_bytes([strength[2], strength[3]]);
        assert_eq!(val, 0x4000);
    }

    // -----------------------------------------------------------------------
    // Mode switch
    // -----------------------------------------------------------------------

    #[test]
    fn mode_switch_normal_values() {
        let ms = mode_switch_normal();
        assert_eq!(ms.b_request, 83);
        assert_eq!(ms.b_request_type, 0x41);
        assert_eq!(ms.w_value, 5);
    }

    #[test]
    fn mode_switch_advanced_values() {
        let ms = mode_switch_advanced();
        assert_eq!(ms.b_request, 83);
        assert_eq!(ms.b_request_type, 0x41);
        assert_eq!(ms.w_value, 3);
    }

    // -----------------------------------------------------------------------
    // Waveform values
    // -----------------------------------------------------------------------

    #[test]
    fn waveform_values_match_kernel() {
        // FF_SQUARE=0x58, FF_TRIANGLE=0x59, FF_SINE=0x5a, FF_SAW_UP=0x5b, FF_SAW_DOWN=0x5c
        // Wire byte = waveform - 0x57
        assert_eq!(Waveform::Square as u8, 0x58 - 0x57);
        assert_eq!(Waveform::Triangle as u8, 0x59 - 0x57);
        assert_eq!(Waveform::Sine as u8, 0x5a - 0x57);
        assert_eq!(Waveform::SawUp as u8, 0x5b - 0x57);
        assert_eq!(Waveform::SawDown as u8, 0x5c - 0x57);
    }

    // -----------------------------------------------------------------------
    // Constants cross-check
    // -----------------------------------------------------------------------

    #[test]
    fn condition_hardcoded_matches_kernel() {
        // From hid-tmt300rs.c: condition_values[] = {0xfe, 0xff, 0xfe, 0xff, 0xfe, 0xff, 0xfe, 0xff}
        assert_eq!(
            CONDITION_HARDCODED,
            [0xfe, 0xff, 0xfe, 0xff, 0xfe, 0xff, 0xfe, 0xff]
        );
    }

    #[test]
    fn spring_max_saturation_matches_kernel() {
        // From hid-tmt300rs.c: t300rs_condition_max_saturation for FF_SPRING = 0x6aa6
        assert_eq!(SPRING_MAX_SATURATION, 0x6aa6);
    }

    #[test]
    fn range_scale_matches_kernel() {
        // From hid-tmt300rs.c: scaled_value = value * 0x3c
        assert_eq!(RANGE_SCALE, 0x3c);
    }
}

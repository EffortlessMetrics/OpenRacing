//! VRS DirectForce Pro PIDFF effect management reports.
//!
//! Implements the USB HID PID (Physical Interface Device) effect management
//! layer for VRS wheelbases. These reports handle the full effect lifecycle:
//! creating effects, setting parameters, starting/stopping, and cleanup.
//!
//! VRS uses standard USB HID PIDFF as confirmed by the Linux kernel driver
//! (`hid-universal-pidff.c` with `HID_PIDFF_QUIRK_PERMISSIVE_CONTROL`).
//!
//! # Report IDs (from ids.rs)
//!
//! | ID   | Report Type           |
//! |------|-----------------------|
//! | 0x02 | Set Effect            |
//! | 0x0A | Effect Operation      |
//! | 0x0B | Device Control        |
//! | 0x11 | Constant Force        |
//! | 0x13 | Ramp Force            |
//! | 0x14 | Square (periodic)     |
//! | 0x15 | Sine (periodic)       |
//! | 0x16 | Triangle (periodic)   |
//! | 0x17 | Sawtooth Up (periodic)|
//! | 0x18 | Sawtooth Down (periodic)|
//!
//! # Sources
//! - USB HID PID specification (pid1_01.pdf)
//! - Linux kernel `hid-universal-pidff.c`

use crate::ids::report_ids;

/// Wire size of a Set Effect report (PIDFF).
pub const SET_EFFECT_REPORT_LEN: usize = 14;

/// Wire size of a periodic effect report (sine/square/triangle/sawtooth).
pub const PERIODIC_REPORT_LEN: usize = 10;

/// Wire size of a ramp force report.
pub const RAMP_REPORT_LEN: usize = 8;

/// Wire size of an effect operation report (play/stop).
pub const EFFECT_OPERATION_REPORT_LEN: usize = 4;

/// Wire size of an envelope report.
pub const ENVELOPE_REPORT_LEN: usize = 10;

/// Duration value meaning "infinite" in PIDFF.
pub const DURATION_INFINITE: u16 = 0xFFFF;

/// Standard PIDFF effect types (USB HID PID Table B-2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PidEffectType {
    /// Constant force output.
    ConstantForce = 1,
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
    /// Spring condition (position-dependent).
    Spring = 8,
    /// Damper condition (velocity-dependent).
    Damper = 9,
    /// Inertia condition (acceleration-dependent).
    Inertia = 10,
    /// Friction condition.
    Friction = 11,
}

/// PIDFF effect operation types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum EffectOperation {
    /// Start the effect.
    Start = 1,
    /// Start the effect solo (stop all others first).
    StartSolo = 2,
    /// Stop the effect.
    Stop = 3,
}

// ---------------------------------------------------------------------------
// Set Effect Report (0x02)
// ---------------------------------------------------------------------------

/// Parameters for the PIDFF Set Effect report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetEffectParams {
    /// Effect block index (1-based, as allocated by the device).
    pub block_index: u8,
    /// Effect type.
    pub effect_type: PidEffectType,
    /// Duration in milliseconds (0xFFFF = infinite).
    pub duration_ms: u16,
    /// Trigger repeat interval in milliseconds.
    pub trigger_repeat_ms: u16,
    /// Sample period in milliseconds (typically 0 = default).
    pub sample_period_ms: u16,
    /// Overall gain (0-255).
    pub gain: u8,
    /// Trigger button index (0xFF = no trigger).
    pub trigger_button: u8,
    /// Direction in degrees × 100 (0-36000, where 9000 = 90°).
    pub direction: u16,
}

impl Default for SetEffectParams {
    fn default() -> Self {
        Self {
            block_index: 1,
            effect_type: PidEffectType::ConstantForce,
            duration_ms: DURATION_INFINITE,
            trigger_repeat_ms: 0,
            sample_period_ms: 0,
            gain: 255,
            trigger_button: 0xFF,
            direction: 0,
        }
    }
}

/// Encode a Set Effect report (PIDFF report ID 0x02).
///
/// # Wire format (14 bytes)
/// ```text
/// Byte 0:    Report ID (0x02)
/// Byte 1:    Effect block index (1-based)
/// Byte 2:    Effect type
/// Bytes 3-4: Duration (ms, LE; 0xFFFF = infinite)
/// Bytes 5-6: Trigger repeat interval (ms, LE)
/// Bytes 7-8: Sample period (ms, LE)
/// Byte 9:    Gain (0-255)
/// Byte 10:   Trigger button (0xFF = none)
/// Bytes 11-12: Direction (degrees × 100, LE)
/// Byte 13:   Reserved
/// ```
pub fn encode_set_effect(params: &SetEffectParams) -> [u8; SET_EFFECT_REPORT_LEN] {
    let mut buf = [0u8; SET_EFFECT_REPORT_LEN];
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
// Effect Operation Report (0x0A)
// ---------------------------------------------------------------------------

/// Encode an Effect Operation report (PIDFF report ID 0x0A).
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
    operation: EffectOperation,
    loop_count: u8,
) -> [u8; EFFECT_OPERATION_REPORT_LEN] {
    [
        report_ids::EFFECT_OPERATION,
        block_index,
        operation as u8,
        loop_count,
    ]
}

// ---------------------------------------------------------------------------
// Periodic Effects (Sine, Square, Triangle, Sawtooth Up/Down)
// ---------------------------------------------------------------------------

/// Parameters for a periodic effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeriodicParams {
    /// Effect block index (1-based).
    pub block_index: u8,
    /// Magnitude (0-10000).
    pub magnitude: u16,
    /// Offset from center (-10000 to +10000).
    pub offset: i16,
    /// Phase angle in degrees × 100 (0-36000).
    pub phase: u16,
    /// Period in milliseconds.
    pub period_ms: u16,
}

impl Default for PeriodicParams {
    fn default() -> Self {
        Self {
            block_index: 1,
            magnitude: 0,
            offset: 0,
            phase: 0,
            period_ms: 100,
        }
    }
}

/// Encode a periodic effect report body.
///
/// The caller selects the waveform by choosing the appropriate report ID.
///
/// # Wire format (10 bytes)
/// ```text
/// Byte 0:    Report ID (waveform-specific)
/// Byte 1:    Effect block index
/// Bytes 2-3: Magnitude (0-10000, LE)
/// Bytes 4-5: Offset (signed, LE)
/// Bytes 6-7: Phase (degrees × 100, LE)
/// Bytes 8-9: Period (ms, LE)
/// ```
fn encode_periodic_inner(report_id: u8, params: &PeriodicParams) -> [u8; PERIODIC_REPORT_LEN] {
    let mut buf = [0u8; PERIODIC_REPORT_LEN];
    buf[0] = report_id;
    buf[1] = params.block_index;
    buf[2..4].copy_from_slice(&params.magnitude.to_le_bytes());
    buf[4..6].copy_from_slice(&params.offset.to_le_bytes());
    buf[6..8].copy_from_slice(&params.phase.to_le_bytes());
    buf[8..10].copy_from_slice(&params.period_ms.to_le_bytes());
    buf
}

/// Encode a sine wave effect report.
pub fn encode_sine(params: &PeriodicParams) -> [u8; PERIODIC_REPORT_LEN] {
    encode_periodic_inner(report_ids::SINE_EFFECT, params)
}

/// Encode a square wave effect report.
pub fn encode_square(params: &PeriodicParams) -> [u8; PERIODIC_REPORT_LEN] {
    encode_periodic_inner(report_ids::SQUARE_EFFECT, params)
}

/// Encode a triangle wave effect report.
pub fn encode_triangle(params: &PeriodicParams) -> [u8; PERIODIC_REPORT_LEN] {
    encode_periodic_inner(report_ids::TRIANGLE_EFFECT, params)
}

/// Encode a sawtooth up effect report.
pub fn encode_sawtooth_up(params: &PeriodicParams) -> [u8; PERIODIC_REPORT_LEN] {
    encode_periodic_inner(report_ids::SAWTOOTH_UP_EFFECT, params)
}

/// Encode a sawtooth down effect report.
pub fn encode_sawtooth_down(params: &PeriodicParams) -> [u8; PERIODIC_REPORT_LEN] {
    encode_periodic_inner(report_ids::SAWTOOTH_DOWN_EFFECT, params)
}

// ---------------------------------------------------------------------------
// Ramp Force (0x13)
// ---------------------------------------------------------------------------

/// Encode a ramp force effect report.
///
/// # Wire format (8 bytes)
/// ```text
/// Byte 0:    Report ID (0x13)
/// Byte 1:    Effect block index
/// Bytes 2-3: Ramp start (signed, LE, -10000 to +10000)
/// Bytes 4-5: Ramp end (signed, LE, -10000 to +10000)
/// Bytes 6-7: Reserved
/// ```
pub fn encode_ramp(block_index: u8, ramp_start: i16, ramp_end: i16) -> [u8; RAMP_REPORT_LEN] {
    let mut buf = [0u8; RAMP_REPORT_LEN];
    buf[0] = report_ids::RAMP_FORCE;
    buf[1] = block_index;
    buf[2..4].copy_from_slice(&ramp_start.to_le_bytes());
    buf[4..6].copy_from_slice(&ramp_end.to_le_bytes());
    buf
}

// ---------------------------------------------------------------------------
// Envelope (no dedicated report ID — part of Set Effect flow)
// ---------------------------------------------------------------------------

/// PIDFF envelope parameters applied to an effect.
///
/// The envelope shapes the attack and fade of an effect's magnitude.
/// Note: envelope is typically sent as part of the Set Effect flow,
/// not as a standalone report in standard PIDFF. The exact encoding
/// depends on the device's HID descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnvelopeParams {
    /// Effect block index.
    pub block_index: u8,
    /// Attack level (0-10000).
    pub attack_level: u16,
    /// Fade level (0-10000).
    pub fade_level: u16,
    /// Attack time in milliseconds.
    pub attack_time_ms: u16,
    /// Fade time in milliseconds.
    pub fade_time_ms: u16,
}

impl Default for EnvelopeParams {
    fn default() -> Self {
        Self {
            block_index: 1,
            attack_level: 10000,
            fade_level: 10000,
            attack_time_ms: 0,
            fade_time_ms: 0,
        }
    }
}

/// Encode an envelope report.
///
/// # Wire format (10 bytes)
/// ```text
/// Byte 0:    Report ID (0x02, shared with Set Effect)
/// Byte 1:    Effect block index
/// Bytes 2-3: Attack level (0-10000, LE)
/// Bytes 4-5: Fade level (0-10000, LE)
/// Bytes 6-7: Attack time (ms, LE)
/// Bytes 8-9: Fade time (ms, LE)
/// ```
///
/// Note: in standard PIDFF the envelope data may be sent as part of the
/// Set Effect report or as a separate transfer. This encoder produces a
/// standalone report suitable for devices that accept it.
pub fn encode_envelope(params: &EnvelopeParams) -> [u8; ENVELOPE_REPORT_LEN] {
    let mut buf = [0u8; ENVELOPE_REPORT_LEN];
    buf[0] = report_ids::SET_EFFECT;
    buf[1] = params.block_index;
    buf[2..4].copy_from_slice(&params.attack_level.to_le_bytes());
    buf[4..6].copy_from_slice(&params.fade_level.to_le_bytes());
    buf[6..8].copy_from_slice(&params.attack_time_ms.to_le_bytes());
    buf[8..10].copy_from_slice(&params.fade_time_ms.to_le_bytes());
    buf
}

// ---------------------------------------------------------------------------
// Convenience: full effect lifecycle helpers
// ---------------------------------------------------------------------------

/// Create and start a simple constant force effect.
///
/// Returns the Set Effect report, then the caller should also send the
/// constant force magnitude via `VrsConstantForceEncoder` from `output.rs`.
pub fn create_constant_force_effect(
    block_index: u8,
    gain: u8,
) -> (
    [u8; SET_EFFECT_REPORT_LEN],
    [u8; EFFECT_OPERATION_REPORT_LEN],
) {
    let set = encode_set_effect(&SetEffectParams {
        block_index,
        effect_type: PidEffectType::ConstantForce,
        duration_ms: DURATION_INFINITE,
        gain,
        ..SetEffectParams::default()
    });
    let op = encode_effect_operation(block_index, EffectOperation::Start, 0);
    (set, op)
}

/// Create and start a periodic effect with the given waveform.
pub fn create_periodic_effect(
    block_index: u8,
    effect_type: PidEffectType,
    periodic: &PeriodicParams,
    gain: u8,
) -> (
    [u8; SET_EFFECT_REPORT_LEN],
    [u8; PERIODIC_REPORT_LEN],
    [u8; EFFECT_OPERATION_REPORT_LEN],
) {
    let set = encode_set_effect(&SetEffectParams {
        block_index,
        effect_type,
        duration_ms: DURATION_INFINITE,
        gain,
        ..SetEffectParams::default()
    });

    let periodic_params = PeriodicParams {
        block_index,
        ..*periodic
    };

    let waveform = match effect_type {
        PidEffectType::Sine => encode_sine(&periodic_params),
        PidEffectType::Square => encode_square(&periodic_params),
        PidEffectType::Triangle => encode_triangle(&periodic_params),
        PidEffectType::SawtoothUp => encode_sawtooth_up(&periodic_params),
        PidEffectType::SawtoothDown => encode_sawtooth_down(&periodic_params),
        // For non-periodic types, encode as sine (caller should use correct type)
        _ => encode_sine(&periodic_params),
    };

    let op = encode_effect_operation(block_index, EffectOperation::Start, 0);
    (set, waveform, op)
}

/// Stop an effect by block index.
pub fn stop_effect(block_index: u8) -> [u8; EFFECT_OPERATION_REPORT_LEN] {
    encode_effect_operation(block_index, EffectOperation::Stop, 0)
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Effect type enum values (USB HID PID Table B-2)
    // -----------------------------------------------------------------------

    #[test]
    fn effect_type_values_match_pid_spec() {
        assert_eq!(PidEffectType::ConstantForce as u8, 1);
        assert_eq!(PidEffectType::Ramp as u8, 2);
        assert_eq!(PidEffectType::Square as u8, 3);
        assert_eq!(PidEffectType::Sine as u8, 4);
        assert_eq!(PidEffectType::Triangle as u8, 5);
        assert_eq!(PidEffectType::SawtoothUp as u8, 6);
        assert_eq!(PidEffectType::SawtoothDown as u8, 7);
        assert_eq!(PidEffectType::Spring as u8, 8);
        assert_eq!(PidEffectType::Damper as u8, 9);
        assert_eq!(PidEffectType::Inertia as u8, 10);
        assert_eq!(PidEffectType::Friction as u8, 11);
    }

    #[test]
    fn operation_values() {
        assert_eq!(EffectOperation::Start as u8, 1);
        assert_eq!(EffectOperation::StartSolo as u8, 2);
        assert_eq!(EffectOperation::Stop as u8, 3);
    }

    // -----------------------------------------------------------------------
    // Set Effect
    // -----------------------------------------------------------------------

    #[test]
    fn set_effect_report_id() {
        let buf = encode_set_effect(&SetEffectParams::default());
        assert_eq!(buf[0], report_ids::SET_EFFECT);
        assert_eq!(buf[0], 0x02);
    }

    #[test]
    fn set_effect_length() {
        let buf = encode_set_effect(&SetEffectParams::default());
        assert_eq!(buf.len(), SET_EFFECT_REPORT_LEN);
        assert_eq!(buf.len(), 14);
    }

    #[test]
    fn set_effect_fields() {
        let params = SetEffectParams {
            block_index: 3,
            effect_type: PidEffectType::Sine,
            duration_ms: 5000,
            trigger_repeat_ms: 100,
            sample_period_ms: 10,
            gain: 200,
            trigger_button: 2,
            direction: 9000,
        };
        let buf = encode_set_effect(&params);
        assert_eq!(buf[1], 3);
        assert_eq!(buf[2], PidEffectType::Sine as u8);
        assert_eq!(u16::from_le_bytes([buf[3], buf[4]]), 5000);
        assert_eq!(u16::from_le_bytes([buf[5], buf[6]]), 100);
        assert_eq!(u16::from_le_bytes([buf[7], buf[8]]), 10);
        assert_eq!(buf[9], 200);
        assert_eq!(buf[10], 2);
        assert_eq!(u16::from_le_bytes([buf[11], buf[12]]), 9000);
    }

    #[test]
    fn set_effect_default_infinite_duration() {
        let params = SetEffectParams::default();
        assert_eq!(params.duration_ms, DURATION_INFINITE);
        assert_eq!(params.duration_ms, 0xFFFF);
    }

    // -----------------------------------------------------------------------
    // Effect Operation
    // -----------------------------------------------------------------------

    #[test]
    fn effect_operation_start() {
        let buf = encode_effect_operation(1, EffectOperation::Start, 0);
        assert_eq!(buf[0], report_ids::EFFECT_OPERATION);
        assert_eq!(buf[0], 0x0A);
        assert_eq!(buf[1], 1);
        assert_eq!(buf[2], 1);
        assert_eq!(buf[3], 0);
    }

    #[test]
    fn effect_operation_stop() {
        let buf = encode_effect_operation(5, EffectOperation::Stop, 0);
        assert_eq!(buf[1], 5);
        assert_eq!(buf[2], 3);
    }

    #[test]
    fn effect_operation_start_solo() {
        let buf = encode_effect_operation(1, EffectOperation::StartSolo, 3);
        assert_eq!(buf[2], 2);
        assert_eq!(buf[3], 3);
    }

    #[test]
    fn effect_operation_length() {
        let buf = encode_effect_operation(1, EffectOperation::Start, 0);
        assert_eq!(buf.len(), EFFECT_OPERATION_REPORT_LEN);
    }

    // -----------------------------------------------------------------------
    // Periodic effects
    // -----------------------------------------------------------------------

    #[test]
    fn sine_report_id() {
        let buf = encode_sine(&PeriodicParams::default());
        assert_eq!(buf[0], report_ids::SINE_EFFECT);
        assert_eq!(buf[0], 0x15);
    }

    #[test]
    fn square_report_id() {
        let buf = encode_square(&PeriodicParams::default());
        assert_eq!(buf[0], report_ids::SQUARE_EFFECT);
        assert_eq!(buf[0], 0x14);
    }

    #[test]
    fn triangle_report_id() {
        let buf = encode_triangle(&PeriodicParams::default());
        assert_eq!(buf[0], report_ids::TRIANGLE_EFFECT);
        assert_eq!(buf[0], 0x16);
    }

    #[test]
    fn sawtooth_up_report_id() {
        let buf = encode_sawtooth_up(&PeriodicParams::default());
        assert_eq!(buf[0], report_ids::SAWTOOTH_UP_EFFECT);
        assert_eq!(buf[0], 0x17);
    }

    #[test]
    fn sawtooth_down_report_id() {
        let buf = encode_sawtooth_down(&PeriodicParams::default());
        assert_eq!(buf[0], report_ids::SAWTOOTH_DOWN_EFFECT);
        assert_eq!(buf[0], 0x18);
    }

    #[test]
    fn periodic_report_length() {
        let buf = encode_sine(&PeriodicParams::default());
        assert_eq!(buf.len(), PERIODIC_REPORT_LEN);
        assert_eq!(buf.len(), 10);
    }

    #[test]
    fn periodic_fields() {
        let params = PeriodicParams {
            block_index: 2,
            magnitude: 8000,
            offset: -3000,
            phase: 18000,
            period_ms: 250,
        };
        let buf = encode_sine(&params);
        assert_eq!(buf[1], 2);
        assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), 8000);
        assert_eq!(i16::from_le_bytes([buf[4], buf[5]]), -3000);
        assert_eq!(u16::from_le_bytes([buf[6], buf[7]]), 18000);
        assert_eq!(u16::from_le_bytes([buf[8], buf[9]]), 250);
    }

    #[test]
    fn all_periodic_same_layout() {
        let params = PeriodicParams {
            block_index: 1,
            magnitude: 5000,
            offset: 1000,
            phase: 9000,
            period_ms: 100,
        };
        let sine = encode_sine(&params);
        let square = encode_square(&params);
        let tri = encode_triangle(&params);
        let up = encode_sawtooth_up(&params);
        let down = encode_sawtooth_down(&params);

        // All share the same layout bytes 1-9, only report ID differs
        for (a, b) in [
            (&sine, &square),
            (&sine, &tri),
            (&sine, &up),
            (&sine, &down),
        ] {
            assert_eq!(&a[1..], &b[1..]);
        }
    }

    // -----------------------------------------------------------------------
    // Ramp
    // -----------------------------------------------------------------------

    #[test]
    fn ramp_report_id() {
        let buf = encode_ramp(1, 0, 0);
        assert_eq!(buf[0], report_ids::RAMP_FORCE);
        assert_eq!(buf[0], 0x13);
    }

    #[test]
    fn ramp_report_length() {
        let buf = encode_ramp(1, 0, 0);
        assert_eq!(buf.len(), RAMP_REPORT_LEN);
    }

    #[test]
    fn ramp_fields() {
        let buf = encode_ramp(3, -5000, 5000);
        assert_eq!(buf[1], 3);
        assert_eq!(i16::from_le_bytes([buf[2], buf[3]]), -5000);
        assert_eq!(i16::from_le_bytes([buf[4], buf[5]]), 5000);
        assert_eq!(buf[6], 0);
        assert_eq!(buf[7], 0);
    }

    // -----------------------------------------------------------------------
    // Envelope
    // -----------------------------------------------------------------------

    #[test]
    fn envelope_report_id() {
        let buf = encode_envelope(&EnvelopeParams::default());
        assert_eq!(buf[0], report_ids::SET_EFFECT);
    }

    #[test]
    fn envelope_report_length() {
        let buf = encode_envelope(&EnvelopeParams::default());
        assert_eq!(buf.len(), ENVELOPE_REPORT_LEN);
    }

    #[test]
    fn envelope_fields() {
        let params = EnvelopeParams {
            block_index: 2,
            attack_level: 3000,
            fade_level: 7000,
            attack_time_ms: 500,
            fade_time_ms: 1000,
        };
        let buf = encode_envelope(&params);
        assert_eq!(buf[1], 2);
        assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), 3000);
        assert_eq!(u16::from_le_bytes([buf[4], buf[5]]), 7000);
        assert_eq!(u16::from_le_bytes([buf[6], buf[7]]), 500);
        assert_eq!(u16::from_le_bytes([buf[8], buf[9]]), 1000);
    }

    // -----------------------------------------------------------------------
    // Lifecycle helpers
    // -----------------------------------------------------------------------

    #[test]
    fn create_constant_force_produces_valid_reports() {
        let (set, op) = create_constant_force_effect(1, 255);
        assert_eq!(set[0], report_ids::SET_EFFECT);
        assert_eq!(set[1], 1);
        assert_eq!(set[2], PidEffectType::ConstantForce as u8);
        assert_eq!(set[9], 255);
        assert_eq!(op[0], report_ids::EFFECT_OPERATION);
        assert_eq!(op[2], EffectOperation::Start as u8);
    }

    #[test]
    fn create_periodic_sine_effect() {
        let periodic = PeriodicParams {
            magnitude: 5000,
            period_ms: 200,
            ..PeriodicParams::default()
        };
        let (set, wave, op) = create_periodic_effect(2, PidEffectType::Sine, &periodic, 128);
        assert_eq!(set[1], 2);
        assert_eq!(set[2], PidEffectType::Sine as u8);
        assert_eq!(set[9], 128);
        assert_eq!(wave[0], report_ids::SINE_EFFECT);
        assert_eq!(wave[1], 2);
        assert_eq!(u16::from_le_bytes([wave[2], wave[3]]), 5000);
        assert_eq!(op[1], 2);
    }

    #[test]
    fn stop_effect_report() {
        let buf = stop_effect(4);
        assert_eq!(buf[0], report_ids::EFFECT_OPERATION);
        assert_eq!(buf[1], 4);
        assert_eq!(buf[2], EffectOperation::Stop as u8);
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(500))]

        #[test]
        fn prop_set_effect_always_valid(
            block_index in 1u8..=40u8,
            effect_type in 1u8..=11u8,
            duration in proptest::num::u16::ANY,
            gain in 0u8..=255u8,
            direction in 0u16..=36000u16,
        ) {
            let params = SetEffectParams {
                block_index,
                // Safety: we constrain to valid range
                effect_type: match effect_type {
                    1 => PidEffectType::ConstantForce,
                    2 => PidEffectType::Ramp,
                    3 => PidEffectType::Square,
                    4 => PidEffectType::Sine,
                    5 => PidEffectType::Triangle,
                    6 => PidEffectType::SawtoothUp,
                    7 => PidEffectType::SawtoothDown,
                    8 => PidEffectType::Spring,
                    9 => PidEffectType::Damper,
                    10 => PidEffectType::Inertia,
                    _ => PidEffectType::Friction,
                },
                duration_ms: duration,
                gain,
                direction,
                ..SetEffectParams::default()
            };
            let buf = encode_set_effect(&params);
            prop_assert_eq!(buf[0], report_ids::SET_EFFECT);
            prop_assert_eq!(buf[1], block_index);
            prop_assert_eq!(buf.len(), SET_EFFECT_REPORT_LEN);
        }

        #[test]
        fn prop_periodic_magnitude_preserved(
            magnitude in 0u16..=10000u16,
            offset in -10000i16..=10000i16,
            phase in 0u16..=36000u16,
            period in 1u16..=10000u16,
        ) {
            let params = PeriodicParams {
                block_index: 1,
                magnitude,
                offset,
                phase,
                period_ms: period,
            };
            let buf = encode_sine(&params);
            prop_assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), magnitude);
            prop_assert_eq!(i16::from_le_bytes([buf[4], buf[5]]), offset);
            prop_assert_eq!(u16::from_le_bytes([buf[6], buf[7]]), phase);
            prop_assert_eq!(u16::from_le_bytes([buf[8], buf[9]]), period);
        }

        #[test]
        fn prop_ramp_values_preserved(
            start in -10000i16..=10000i16,
            end in -10000i16..=10000i16,
        ) {
            let buf = encode_ramp(1, start, end);
            prop_assert_eq!(i16::from_le_bytes([buf[2], buf[3]]), start);
            prop_assert_eq!(i16::from_le_bytes([buf[4], buf[5]]), end);
        }

        #[test]
        fn prop_effect_operation_values(
            block in 1u8..=40u8,
            op_type in 1u8..=3u8,
            loops in 0u8..=255u8,
        ) {
            let op = match op_type {
                1 => EffectOperation::Start,
                2 => EffectOperation::StartSolo,
                _ => EffectOperation::Stop,
            };
            let buf = encode_effect_operation(block, op, loops);
            prop_assert_eq!(buf[0], report_ids::EFFECT_OPERATION);
            prop_assert_eq!(buf[1], block);
            prop_assert_eq!(buf[2], op_type);
            prop_assert_eq!(buf[3], loops);
        }
    }
}

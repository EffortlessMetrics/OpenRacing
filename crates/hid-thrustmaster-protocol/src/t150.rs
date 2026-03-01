//! T150/TMX wire-format output encoding for Force Feedback.
//!
//! All functions are pure and allocation-free (RT-path compatible).
//!
//! # Wire protocol reference (scarburato/t150_driver)
//!
//! The T150 and TMX use a proprietary FFB protocol that is **different from
//! the T300RS family** (which uses HID Report ID 0x60). Output is sent via
//! USB interrupt OUT endpoint.
//!
//! ## Commands
//!
//! - **Range**: `[0x40, 0x11, <u16_le>]` — 0xFFFF = max rotation (1080° T150, 900° TMX).
//! - **Gain**: `[0x43, <gain_u8>]` — single byte gain value.
//! - **Play effect**: `[0x41, <effect_id>, <mode>, <times>]` — trigger an uploaded effect.
//! - **Stop effect**: `[0x41, <effect_id>, 0x00, 0x00]` — stop a running effect.
//!
//! ## Effect types (from scarburato/t150_driver)
//!
//! - `0x4000`: Constant force
//! - `0x4022`: Sine (periodic)
//! - `0x4023`: Sawtooth up (periodic)
//! - `0x4024`: Sawtooth down (periodic)
//! - `0x4040`: Spring (condition)
//! - `0x4041`: Damper (condition)
//!
//! ## Effect upload
//!
//! Effects use a 3-packet pattern: `ff_first` → `ff_update` → `ff_commit`.
//! This module encodes individual packets; sequencing is the caller's
//! responsibility.
//!
//! ## Init
//!
//! T150/TMX use init switch value `0x0006` (vs `0x0005` for T300RS family).
//!
//! Source: `scarburato/t150_driver` Linux kernel module.

#![deny(static_mut_refs)]

// ── Command bytes ────────────────────────────────────────────────────────────

/// T150/TMX range/setup command prefix.
pub const CMD_RANGE: u8 = 0x40;
/// T150/TMX effect play/stop command prefix.
pub const CMD_EFFECT: u8 = 0x41;
/// T150/TMX gain command prefix.
pub const CMD_GAIN: u8 = 0x43;

/// Range sub-command byte.
pub const SUBCMD_RANGE: u8 = 0x11;

// ── Effect type codes (scarburato/t150_driver) ───────────────────────────────

/// T150/TMX effect type identifiers.
///
/// These are the 16-bit type codes used in effect upload packets.
/// Source: `scarburato/t150_driver` effect definitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum T150EffectType {
    /// Constant force effect.
    Constant = 0x4000,
    /// Sine periodic effect.
    Sine = 0x4022,
    /// Sawtooth-up periodic effect.
    SawtoothUp = 0x4023,
    /// Sawtooth-down periodic effect.
    SawtoothDown = 0x4024,
    /// Spring condition effect.
    Spring = 0x4040,
    /// Damper condition effect.
    Damper = 0x4041,
}

impl T150EffectType {
    /// Convert to the 16-bit wire value.
    pub const fn as_u16(self) -> u16 {
        self as u16
    }

    /// Try to parse a 16-bit wire value into a known effect type.
    pub const fn from_u16(value: u16) -> Option<Self> {
        match value {
            0x4000 => Some(Self::Constant),
            0x4022 => Some(Self::Sine),
            0x4023 => Some(Self::SawtoothUp),
            0x4024 => Some(Self::SawtoothDown),
            0x4040 => Some(Self::Spring),
            0x4041 => Some(Self::Damper),
            _ => None,
        }
    }
}

// ── Encoding functions ───────────────────────────────────────────────────────

/// Build a T150/TMX range command.
///
/// Wire format: `[0x40, 0x11, lo, hi]` where `(lo, hi)` is `range_value` as LE16.
///
/// The `range_value` is the raw 16-bit value sent on the wire. The T150 driver
/// maps 0xFFFF to maximum rotation (1080° on T150, 900° on TMX). Callers
/// should convert degrees to the appropriate scale before calling this function.
///
/// Source: `scarburato/t150_driver` — `t150_set_range()`.
pub fn encode_range_t150(range_value: u16) -> [u8; 4] {
    let [lo, hi] = range_value.to_le_bytes();
    [CMD_RANGE, SUBCMD_RANGE, lo, hi]
}

/// Build a T150/TMX gain command.
///
/// Wire format: `[0x43, gain]`.
///
/// `gain` is a single byte (0x00 = no force, 0xFF = full force).
///
/// Source: `scarburato/t150_driver` — `t150_set_gain()`.
pub fn encode_gain_t150(gain: u8) -> [u8; 2] {
    [CMD_GAIN, gain]
}

/// Build a T150/TMX play-effect command.
///
/// Wire format: `[0x41, effect_id, mode, times]`.
///
/// - `effect_id`: the slot index of the uploaded effect (0-based).
/// - `mode`: playback mode (driver-defined; typically 0x01 = play, 0x41 = solo).
/// - `times`: repetition count (0 = infinite loop until stopped).
///
/// Source: `scarburato/t150_driver` — `t150_play_effect()`.
pub fn encode_play_effect_t150(effect_id: u8, mode: u8, times: u8) -> [u8; 4] {
    [CMD_EFFECT, effect_id, mode, times]
}

/// Build a T150/TMX stop-effect command.
///
/// Wire format: `[0x41, effect_id, 0x00, 0x00]`.
///
/// This is a special case of the play command with mode=0 and times=0,
/// which tells the firmware to stop the specified effect immediately.
///
/// Source: `scarburato/t150_driver` — `t150_play_effect()` with stop semantics.
pub fn encode_stop_effect_t150(effect_id: u8) -> [u8; 4] {
    [CMD_EFFECT, effect_id, 0x00, 0x00]
}

// ── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── encode_range_t150 ────────────────────────────────────────────────

    #[test]
    fn test_range_max_rotation() {
        let r = encode_range_t150(0xFFFF);
        assert_eq!(r, [0x40, 0x11, 0xFF, 0xFF]);
    }

    #[test]
    fn test_range_zero() {
        let r = encode_range_t150(0x0000);
        assert_eq!(r, [0x40, 0x11, 0x00, 0x00]);
    }

    #[test]
    fn test_range_midpoint() {
        let r = encode_range_t150(0x8000);
        assert_eq!(r, [0x40, 0x11, 0x00, 0x80]);
    }

    #[test]
    fn test_range_known_value() {
        // 0x1234 → LE bytes: 0x34, 0x12
        let r = encode_range_t150(0x1234);
        assert_eq!(r, [0x40, 0x11, 0x34, 0x12]);
    }

    // ── encode_gain_t150 ─────────────────────────────────────────────────

    #[test]
    fn test_gain_full() {
        let r = encode_gain_t150(0xFF);
        assert_eq!(r, [0x43, 0xFF]);
    }

    #[test]
    fn test_gain_zero() {
        let r = encode_gain_t150(0x00);
        assert_eq!(r, [0x43, 0x00]);
    }

    #[test]
    fn test_gain_half() {
        let r = encode_gain_t150(0x80);
        assert_eq!(r, [0x43, 0x80]);
    }

    // ── encode_play_effect_t150 ──────────────────────────────────────────

    #[test]
    fn test_play_effect_basic() {
        let r = encode_play_effect_t150(0, 0x01, 1);
        assert_eq!(r, [0x41, 0x00, 0x01, 0x01]);
    }

    #[test]
    fn test_play_effect_infinite_loop() {
        let r = encode_play_effect_t150(3, 0x01, 0);
        assert_eq!(r, [0x41, 0x03, 0x01, 0x00]);
    }

    #[test]
    fn test_play_effect_solo_mode() {
        let r = encode_play_effect_t150(1, 0x41, 5);
        assert_eq!(r, [0x41, 0x01, 0x41, 0x05]);
    }

    #[test]
    fn test_play_effect_max_values() {
        let r = encode_play_effect_t150(0xFF, 0xFF, 0xFF);
        assert_eq!(r, [0x41, 0xFF, 0xFF, 0xFF]);
    }

    // ── encode_stop_effect_t150 ──────────────────────────────────────────

    #[test]
    fn test_stop_effect_zero() {
        let r = encode_stop_effect_t150(0);
        assert_eq!(r, [0x41, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn test_stop_effect_nonzero_id() {
        let r = encode_stop_effect_t150(5);
        assert_eq!(r, [0x41, 0x05, 0x00, 0x00]);
    }

    #[test]
    fn test_stop_effect_max_id() {
        let r = encode_stop_effect_t150(0xFF);
        assert_eq!(r, [0x41, 0xFF, 0x00, 0x00]);
    }

    #[test]
    fn test_stop_matches_play_with_zero_mode_times() {
        // Stop is equivalent to play with mode=0, times=0
        let stop = encode_stop_effect_t150(7);
        let play = encode_play_effect_t150(7, 0x00, 0x00);
        assert_eq!(stop, play);
    }

    // ── T150EffectType ───────────────────────────────────────────────────

    #[test]
    fn test_effect_type_constant_value() {
        assert_eq!(T150EffectType::Constant.as_u16(), 0x4000);
    }

    #[test]
    fn test_effect_type_sine_value() {
        assert_eq!(T150EffectType::Sine.as_u16(), 0x4022);
    }

    #[test]
    fn test_effect_type_sawtooth_up_value() {
        assert_eq!(T150EffectType::SawtoothUp.as_u16(), 0x4023);
    }

    #[test]
    fn test_effect_type_sawtooth_down_value() {
        assert_eq!(T150EffectType::SawtoothDown.as_u16(), 0x4024);
    }

    #[test]
    fn test_effect_type_spring_value() {
        assert_eq!(T150EffectType::Spring.as_u16(), 0x4040);
    }

    #[test]
    fn test_effect_type_damper_value() {
        assert_eq!(T150EffectType::Damper.as_u16(), 0x4041);
    }

    #[test]
    fn test_effect_type_roundtrip() {
        let types = [
            T150EffectType::Constant,
            T150EffectType::Sine,
            T150EffectType::SawtoothUp,
            T150EffectType::SawtoothDown,
            T150EffectType::Spring,
            T150EffectType::Damper,
        ];
        for ty in types {
            let decoded = T150EffectType::from_u16(ty.as_u16());
            assert_eq!(decoded, Some(ty));
        }
    }

    #[test]
    fn test_effect_type_unknown_returns_none() {
        assert_eq!(T150EffectType::from_u16(0x0000), None);
        assert_eq!(T150EffectType::from_u16(0xFFFF), None);
        assert_eq!(T150EffectType::from_u16(0x4001), None);
    }

    // ── Command byte constants ───────────────────────────────────────────

    #[test]
    fn test_command_constants() {
        assert_eq!(CMD_RANGE, 0x40);
        assert_eq!(CMD_EFFECT, 0x41);
        assert_eq!(CMD_GAIN, 0x43);
        assert_eq!(SUBCMD_RANGE, 0x11);
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(512))]

        /// Range command always starts with [0x40, 0x11] and encodes the
        /// value as LE u16 in bytes [2,3].
        #[test]
        fn prop_range_header_and_roundtrip(value: u16) {
            let cmd = encode_range_t150(value);
            prop_assert_eq!(cmd[0], CMD_RANGE, "byte 0 must be CMD_RANGE (0x40)");
            prop_assert_eq!(cmd[1], SUBCMD_RANGE, "byte 1 must be SUBCMD_RANGE (0x11)");
            let decoded = u16::from_le_bytes([cmd[2], cmd[3]]);
            prop_assert_eq!(decoded, value, "range value must round-trip via LE bytes");
        }

        /// Gain command is always exactly 2 bytes: [0x43, gain].
        #[test]
        fn prop_gain_preserves_value(gain: u8) {
            let cmd = encode_gain_t150(gain);
            prop_assert_eq!(cmd[0], CMD_GAIN, "byte 0 must be CMD_GAIN (0x43)");
            prop_assert_eq!(cmd[1], gain, "byte 1 must be the gain value");
        }

        /// Play-effect command preserves all three parameters.
        #[test]
        fn prop_play_effect_preserves_params(effect_id: u8, mode: u8, times: u8) {
            let cmd = encode_play_effect_t150(effect_id, mode, times);
            prop_assert_eq!(cmd[0], CMD_EFFECT, "byte 0 must be CMD_EFFECT (0x41)");
            prop_assert_eq!(cmd[1], effect_id, "byte 1 must be effect_id");
            prop_assert_eq!(cmd[2], mode, "byte 2 must be mode");
            prop_assert_eq!(cmd[3], times, "byte 3 must be times");
        }

        /// Stop-effect command has mode=0 and times=0.
        #[test]
        fn prop_stop_effect_zeroes_mode_and_times(effect_id: u8) {
            let cmd = encode_stop_effect_t150(effect_id);
            prop_assert_eq!(cmd[0], CMD_EFFECT, "byte 0 must be CMD_EFFECT (0x41)");
            prop_assert_eq!(cmd[1], effect_id, "byte 1 must be effect_id");
            prop_assert_eq!(cmd[2], 0x00, "byte 2 (mode) must be 0x00 for stop");
            prop_assert_eq!(cmd[3], 0x00, "byte 3 (times) must be 0x00 for stop");
        }

        /// Stop is equivalent to play with mode=0 and times=0.
        #[test]
        fn prop_stop_equals_play_zero(effect_id: u8) {
            let stop = encode_stop_effect_t150(effect_id);
            let play = encode_play_effect_t150(effect_id, 0x00, 0x00);
            prop_assert_eq!(stop, play, "stop must equal play(id, 0, 0)");
        }

        /// T150EffectType round-trips through as_u16 → from_u16 for all known types.
        #[test]
        fn prop_effect_type_roundtrip(idx in 0usize..6usize) {
            let types = [
                T150EffectType::Constant,
                T150EffectType::Sine,
                T150EffectType::SawtoothUp,
                T150EffectType::SawtoothDown,
                T150EffectType::Spring,
                T150EffectType::Damper,
            ];
            let ty = types[idx];
            let decoded = T150EffectType::from_u16(ty.as_u16());
            prop_assert_eq!(decoded, Some(ty), "effect type must round-trip");
        }

        /// Unknown effect type values return None.
        #[test]
        fn prop_unknown_effect_type_returns_none(value: u16) {
            let known = [0x4000u16, 0x4022, 0x4023, 0x4024, 0x4040, 0x4041];
            if !known.contains(&value) {
                prop_assert_eq!(
                    T150EffectType::from_u16(value),
                    None,
                    "unknown value 0x{:04X} must return None",
                    value
                );
            }
        }
    }
}

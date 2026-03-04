//! Fanatec 5-slot FFB command encoder (kernel wire format).
//!
//! # Wire protocol (verified from gotzl/hid-fanatecff `hid-ftecff.c`)
//!
//! Fanatec wheelbases use a 5-slot system for force feedback effects.
//! Each slot command is a 7-byte HID output report.
//!
//! ## Slot layout
//!
//! | Slot | Purpose  | Effect cmd | Effect type |
//! |------|----------|-----------|-------------|
//! | 0    | Constant | `0x08`    | FF_CONSTANT |
//! | 1    | Spring   | `0x0b`    | FF_SPRING   |
//! | 2    | Damper   | `0x0c`    | FF_DAMPER   |
//! | 3    | Inertia  | `0x0c`    | FF_INERTIA  |
//! | 4    | Friction | `0x0c`    | FF_FRICTION |
//!
//! ## Command byte layout
//!
//! ```text
//! Byte 0: (slot_id << 4) | flags
//!   - Bit 0 (0x01): active
//!   - Bit 1 (0x02): disable (clear effect)
//! Byte 1: slot command (effect type identifier)
//! Bytes 2–6: effect-specific parameters
//! ```
//!
//! ## Constant force encoding
//!
//! **Low-res** (ClubSport V2/V2.5, CSR Elite, CSL Elite):
//! ```text
//! [flags, 0x08, force_8bit, 0, 0, 0, 0]
//! ```
//!
//! **High-res** (DD1, DD2, CSL DD, ClubSport DD+):
//! ```text
//! [flags, 0x08, force_lo, force_hi, 0, 0, 0x01]
//! ```
//!
//! Force encoding uses `TRANSLATE_FORCE(level, bits)`:
//! `(CLAMP_VALUE_S16(level) + 0x8000) >> (16 - bits)`
//! - 8-bit: 0x00 = full left, 0x80 = center, 0xFF = full right
//! - 16-bit: 0x0000 = full left, 0x8000 = center, 0xFFFF = full right
//!
//! ## Spring encoding (slot 1, cmd 0x0b)
//!
//! Same layout as Logitech spring:
//! ```text
//! Byte 2: d1 >> 3         (11-bit deadband left, upper 8)
//! Byte 3: d2 >> 3         (11-bit deadband right, upper 8)
//! Byte 4: (k2_4bit << 4) | k1_4bit
//! Byte 5: 0x00            (no sign/d-fraction packing like Logitech)
//! Byte 6: clip (8-bit)
//! ```
//!
//! ## Damper/Inertia/Friction encoding (slots 2-4, cmd 0x0c)
//!
//! ```text
//! Byte 2: k1 (4-bit scaled coefficient)
//! Byte 3: 0x00
//! Byte 4: k2 (4-bit scaled coefficient)
//! Byte 5: 0x00
//! Byte 6: clip (8-bit)
//! ```
//!
//! ## Stop all effects
//!
//! `[0xF3, 0, 0, 0, 0, 0, 0]`
//!
//! Source: `ftecff_update_slot()` and `ftecff_timer()` in
//! `gotzl/hid-fanatecff/hid-ftecff.c`.

#![deny(static_mut_refs)]

/// Wire size of a slot FFB command.
pub const SLOT_CMD_SIZE: usize = 7;

/// Slot IDs for the Fanatec 5-slot system.
pub mod slot {
    pub const CONSTANT: u8 = 0;
    pub const SPRING: u8 = 1;
    pub const DAMPER: u8 = 2;
    pub const INERTIA: u8 = 3;
    pub const FRICTION: u8 = 4;
}

/// Effect command bytes (byte 1 of the 7-byte payload).
pub mod effect_cmd {
    /// Constant force.
    pub const CONSTANT: u8 = 0x08;
    /// Spring condition.
    pub const SPRING: u8 = 0x0B;
    /// Damper / inertia / friction conditions (all share same cmd).
    pub const RESISTANCE: u8 = 0x0C;
}

/// Build the flags byte: `(slot_id << 4) | flags`.
fn flags_byte(slot_id: u8, active: bool, disable: bool) -> u8 {
    let mut flags = slot_id << 4;
    if active {
        flags |= 0x01;
    }
    if disable {
        flags |= 0x02;
    }
    flags
}

/// Translate a signed force level to unsigned using the kernel's TRANSLATE_FORCE macro.
///
/// `TRANSLATE_FORCE(x, bits) = (CLAMP_VALUE_S16(x) + 0x8000) >> (16 - bits)`
fn translate_force(level: i16, bits: u8) -> u16 {
    let clamped = level.max(-0x7FFF) as i32;
    let shifted = (clamped + 0x8000) >> (16 - bits);
    shifted as u16
}

/// Scale a coefficient to N bits: `abs(x) * 2 >> (16 - bits)`.
fn scale_coeff(coeff: i16, bits: u8) -> u8 {
    let abs_val = (coeff as i32).unsigned_abs();
    let doubled = (abs_val * 2).min(0xFFFF);
    (doubled >> (16 - bits)) as u8
}

/// Scale an unsigned 16-bit value to N bits.
fn scale_value_u16(val: u16, bits: u8) -> u8 {
    (val >> (16 - bits)) as u8
}

/// Encode a constant force slot command (low-res, 8-bit).
///
/// For ClubSport V2/V2.5, CSR Elite, CSL Elite.
pub fn encode_constant_lowres(level: i16) -> [u8; SLOT_CMD_SIZE] {
    let force = translate_force(level, 8) as u8;
    let active = level != 0;
    let disable = level == 0;
    [
        flags_byte(slot::CONSTANT, active, disable),
        effect_cmd::CONSTANT,
        force,
        0x00,
        0x00,
        0x00,
        0x00,
    ]
}

/// Encode a constant force slot command (high-res, 16-bit).
///
/// For DD1, DD2, CSL DD, ClubSport DD+. Byte 6 = 0x01 as highres marker.
pub fn encode_constant_highres(level: i16) -> [u8; SLOT_CMD_SIZE] {
    let force = translate_force(level, 16);
    let active = level != 0;
    let disable = level == 0;
    [
        flags_byte(slot::CONSTANT, active, disable),
        effect_cmd::CONSTANT,
        (force & 0xFF) as u8,
        ((force >> 8) & 0xFF) as u8,
        0x00,
        0x00,
        0x01, // highres marker
    ]
}

/// Encode a spring effect slot command.
///
/// - `d1`, `d2`: deadband positions (signed i16)
/// - `k1`, `k2`: spring coefficients (signed i16)
/// - `clip`: saturation (u16)
pub fn encode_spring(d1: i16, d2: i16, k1: i16, k2: i16, clip: u16) -> [u8; SLOT_CMD_SIZE] {
    let d1_u = ((d1 as i32 + 0x8000) & 0xFFFF) as u16;
    let d2_u = ((d2 as i32 + 0x8000) & 0xFFFF) as u16;
    let d1_11 = scale_value_u16(d1_u, 11) as u16;
    let d2_11 = scale_value_u16(d2_u, 11) as u16;

    let disable = clip == 0;
    let active = !disable;

    [
        flags_byte(slot::SPRING, active, disable),
        effect_cmd::SPRING,
        (d1_11 >> 3) as u8,
        (d2_11 >> 3) as u8,
        (scale_coeff(k2, 4) << 4) | scale_coeff(k1, 4),
        0x00,
        scale_value_u16(clip, 8),
    ]
}

/// Encode a damper effect slot command.
pub fn encode_damper(k1: i16, k2: i16, clip: u16) -> [u8; SLOT_CMD_SIZE] {
    let disable = clip == 0;
    let active = !disable;
    [
        flags_byte(slot::DAMPER, active, disable),
        effect_cmd::RESISTANCE,
        scale_coeff(k1, 4),
        0x00,
        scale_coeff(k2, 4),
        0x00,
        scale_value_u16(clip, 8),
    ]
}

/// Encode an inertia effect slot command.
pub fn encode_inertia(k1: i16, k2: i16, clip: u16) -> [u8; SLOT_CMD_SIZE] {
    let disable = clip == 0;
    let active = !disable;
    [
        flags_byte(slot::INERTIA, active, disable),
        effect_cmd::RESISTANCE,
        scale_coeff(k1, 4),
        0x00,
        scale_coeff(k2, 4),
        0x00,
        scale_value_u16(clip, 8),
    ]
}

/// Encode a friction effect slot command.
pub fn encode_friction(k1: i16, k2: i16, clip: u16) -> [u8; SLOT_CMD_SIZE] {
    let disable = clip == 0;
    let active = !disable;
    [
        flags_byte(slot::FRICTION, active, disable),
        effect_cmd::RESISTANCE,
        scale_coeff(k1, 4),
        0x00,
        scale_coeff(k2, 4),
        0x00,
        scale_value_u16(clip, 8),
    ]
}

/// Encode the "stop all effects" command.
///
/// Wire format: `[0xF3, 0, 0, 0, 0, 0, 0]`
pub fn encode_stop_all() -> [u8; SLOT_CMD_SIZE] {
    [0xF3, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
}

/// Encode a slot disable command for any slot.
pub fn encode_disable_slot(slot_id: u8, effect_cmd_byte: u8) -> [u8; SLOT_CMD_SIZE] {
    [
        flags_byte(slot_id, false, true),
        effect_cmd_byte,
        0x00,
        0x00,
        0x00,
        0x00,
        0xFF,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_translate_force_8bit_center() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(translate_force(0, 8), 0x80);
        Ok(())
    }

    #[test]
    fn test_translate_force_8bit_full_left() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(translate_force(-0x7FFF, 8), 0x00);
        Ok(())
    }

    #[test]
    fn test_translate_force_8bit_full_right() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(translate_force(0x7FFF, 8), 0xFF);
        Ok(())
    }

    #[test]
    fn test_translate_force_16bit_center() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(translate_force(0, 16), 0x8000);
        Ok(())
    }

    #[test]
    fn test_translate_force_16bit_full_left() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(translate_force(-0x7FFF, 16), 0x0001);
        Ok(())
    }

    #[test]
    fn test_translate_force_16bit_full_right() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(translate_force(0x7FFF, 16), 0xFFFF);
        Ok(())
    }

    #[test]
    fn test_constant_lowres_zero_disables() -> Result<(), Box<dyn std::error::Error>> {
        let cmd = encode_constant_lowres(0);
        // Disable flag set (bit 1)
        assert_eq!(cmd[0] & 0x02, 0x02, "zero level must set disable flag");
        assert_eq!(cmd[0] & 0x01, 0x00, "zero level must clear active flag");
        assert_eq!(cmd[1], effect_cmd::CONSTANT);
        assert_eq!(cmd[2], 0x80, "zero → 0x80 (center)");
        Ok(())
    }

    #[test]
    fn test_constant_lowres_positive() -> Result<(), Box<dyn std::error::Error>> {
        let cmd = encode_constant_lowres(0x7FFF);
        assert_eq!(cmd[0] & 0x01, 0x01, "non-zero level must set active flag");
        assert_eq!(cmd[2], 0xFF, "full positive");
        Ok(())
    }

    #[test]
    fn test_constant_highres_marker() -> Result<(), Box<dyn std::error::Error>> {
        let cmd = encode_constant_highres(0x4000);
        assert_eq!(cmd[6], 0x01, "highres marker byte");
        assert_eq!(cmd[1], effect_cmd::CONSTANT);
        let force = u16::from_le_bytes([cmd[2], cmd[3]]);
        assert_eq!(force, translate_force(0x4000, 16));
        Ok(())
    }

    #[test]
    fn test_constant_highres_zero_disables() -> Result<(), Box<dyn std::error::Error>> {
        let cmd = encode_constant_highres(0);
        assert_eq!(cmd[0] & 0x02, 0x02, "disable flag");
        assert_eq!(cmd[6], 0x01, "highres marker even when disabled");
        Ok(())
    }

    #[test]
    fn test_spring_structure() -> Result<(), Box<dyn std::error::Error>> {
        let cmd = encode_spring(0, 0, 0x4000, 0x4000, 0xFFFF);
        assert_eq!(cmd[0] & 0xF0, slot::SPRING << 4, "spring slot ID");
        assert_eq!(cmd[1], effect_cmd::SPRING);
        assert_eq!(cmd[6], 0xFF, "full clip");
        Ok(())
    }

    #[test]
    fn test_spring_zero_clip_disables() -> Result<(), Box<dyn std::error::Error>> {
        let cmd = encode_spring(0, 0, 0x4000, 0x4000, 0);
        assert_eq!(cmd[0] & 0x02, 0x02, "zero clip disables slot");
        Ok(())
    }

    #[test]
    fn test_damper_structure() -> Result<(), Box<dyn std::error::Error>> {
        let cmd = encode_damper(0x2000, -0x2000, 0x8000);
        assert_eq!(cmd[0] & 0xF0, slot::DAMPER << 4, "damper slot ID");
        assert_eq!(cmd[1], effect_cmd::RESISTANCE);
        assert_eq!(cmd[3], 0x00, "damper byte 3 always 0");
        assert_eq!(cmd[5], 0x00, "damper byte 5 always 0");
        Ok(())
    }

    #[test]
    fn test_inertia_structure() -> Result<(), Box<dyn std::error::Error>> {
        let cmd = encode_inertia(0x2000, -0x2000, 0x8000);
        assert_eq!(cmd[0] & 0xF0, slot::INERTIA << 4, "inertia slot ID");
        assert_eq!(cmd[1], effect_cmd::RESISTANCE);
        Ok(())
    }

    #[test]
    fn test_friction_structure() -> Result<(), Box<dyn std::error::Error>> {
        let cmd = encode_friction(0x2000, -0x2000, 0x8000);
        assert_eq!(cmd[0] & 0xF0, slot::FRICTION << 4, "friction slot ID");
        assert_eq!(cmd[1], effect_cmd::RESISTANCE);
        Ok(())
    }

    #[test]
    fn test_stop_all() -> Result<(), Box<dyn std::error::Error>> {
        let cmd = encode_stop_all();
        assert_eq!(cmd, [0xF3, 0, 0, 0, 0, 0, 0]);
        Ok(())
    }

    #[test]
    fn test_disable_slot_resistance_resets() -> Result<(), Box<dyn std::error::Error>> {
        let cmd = encode_disable_slot(slot::DAMPER, effect_cmd::RESISTANCE);
        assert_eq!(cmd[0] & 0x02, 0x02, "disable flag set");
        assert_eq!(cmd[6], 0xFF, "trailing 0xFF on disable");
        Ok(())
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(500))]

        #[test]
        fn prop_translate_force_8bit_monotone(a in i16::MIN..=i16::MAX, b in i16::MIN..=i16::MAX) {
            let fa = translate_force(a, 8);
            let fb = translate_force(b, 8);
            if a < b {
                prop_assert!(fa <= fb, "8-bit monotone: f({})={} > f({})={}", a, fa, b, fb);
            }
        }

        #[test]
        fn prop_translate_force_16bit_monotone(a in i16::MIN..=i16::MAX, b in i16::MIN..=i16::MAX) {
            let fa = translate_force(a, 16);
            let fb = translate_force(b, 16);
            if a < b {
                prop_assert!(fa <= fb, "16-bit monotone: f({})={} > f({})={}", a, fa, b, fb);
            }
        }

        #[test]
        fn prop_constant_lowres_force_roundtrips(level in i16::MIN..=i16::MAX) {
            let cmd = encode_constant_lowres(level);
            let expected_force = translate_force(level, 8) as u8;
            prop_assert_eq!(cmd[2], expected_force, "lowres force byte");
            prop_assert_eq!(cmd[1], effect_cmd::CONSTANT, "cmd byte");
        }

        #[test]
        fn prop_constant_highres_has_marker(level in i16::MIN..=i16::MAX) {
            let cmd = encode_constant_highres(level);
            prop_assert_eq!(cmd[6], 0x01, "highres marker always 0x01");
        }

        #[test]
        fn prop_spring_slot_id_correct(
            d1 in i16::MIN..=i16::MAX,
            d2 in i16::MIN..=i16::MAX,
            k1 in i16::MIN..=i16::MAX,
            k2 in i16::MIN..=i16::MAX,
            clip in 1u16..=u16::MAX,
        ) {
            let cmd = encode_spring(d1, d2, k1, k2, clip);
            prop_assert_eq!(cmd[0] >> 4, slot::SPRING, "spring slot ID");
            prop_assert_eq!(cmd[1], effect_cmd::SPRING);
        }

        #[test]
        fn prop_resistance_slots_correct(
            k1 in i16::MIN..=i16::MAX,
            k2 in i16::MIN..=i16::MAX,
            clip in 1u16..=u16::MAX,
        ) {
            let damper = encode_damper(k1, k2, clip);
            let inertia = encode_inertia(k1, k2, clip);
            let friction = encode_friction(k1, k2, clip);

            prop_assert_eq!(damper[0] >> 4, slot::DAMPER);
            prop_assert_eq!(inertia[0] >> 4, slot::INERTIA);
            prop_assert_eq!(friction[0] >> 4, slot::FRICTION);

            // All resistance types use same effect cmd
            prop_assert_eq!(damper[1], effect_cmd::RESISTANCE);
            prop_assert_eq!(inertia[1], effect_cmd::RESISTANCE);
            prop_assert_eq!(friction[1], effect_cmd::RESISTANCE);

            // Same coefficients → same bytes 2-6
            prop_assert_eq!(damper[2], inertia[2]);
            prop_assert_eq!(damper[4], inertia[4]);
            prop_assert_eq!(damper[6], inertia[6]);
        }
    }
}

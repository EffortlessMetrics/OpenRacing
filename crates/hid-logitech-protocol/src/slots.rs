//! Logitech 4-slot FFB command encoder (kernel wire format).
//!
//! # Wire protocol (verified from berarma/new-lg4ff `lg4ff_update_slot`)
//!
//! Logitech G25/G27/G29/G923/DFGT wheels use a 4-slot system for force
//! feedback. Each slot can hold one effect at a time, sent as 7-byte HID
//! output reports.
//!
//! ## Slot layout
//!
//! | Slot | Purpose   | Constant force byte position |
//! |------|-----------|-----------------------------|
//! | 0    | Constant  | byte 2                       |
//! | 1    | Spring    | byte 3                       |
//! | 2    | Damper    | byte 4                       |
//! | 3    | Friction  | byte 5 (G25+, DFP firmware)  |
//!
//! ## Command byte layout
//!
//! ```text
//! Byte 0: (slot_id << 4) | operation
//!   - Operations: 0x01 = start, 0x03 = stop, 0x0c = update
//! Bytes 1–6: effect-type-specific data
//! ```
//!
//! ## Effect encodings
//!
//! ### Constant force (slot 0)
//! ```text
//! [0x11, 0x08, force, 0x80, 0x00, 0x00, 0x00]   (in-tree kernel)
//! [0x1c, 0x00, force, 0x00, 0x00, 0x00, 0x00]   (new-lg4ff, slot 0 update)
//! ```
//! `force` is unsigned 0x00–0xFF: 0x80 = no force (center), 0x00 = full left,
//! 0xFF = full right. This maps via `TRANSLATE_FORCE(level)`:
//! `((CLAMP_VALUE_S16(level) + 0x8000) >> 8)`
//!
//! ### Spring (slot 1, effect type 0x0b)
//! ```text
//! Byte 1: 0x0b
//! Byte 2: d1 >> 3         (deadband center left, 11-bit, upper 8)
//! Byte 3: d2 >> 3         (deadband center right, 11-bit, upper 8)
//! Byte 4: (k2_4bit << 4) | k1_4bit   (4-bit scaled coefficients)
//! Byte 5: ((d2 & 7) << 5) | ((d1 & 7) << 1) | (s2 << 4) | s1
//! Byte 6: clip (8-bit)
//! ```
//!
//! ### Damper (slot 2, effect type 0x0c)
//! ```text
//! Byte 1: 0x0c
//! Byte 2: k1_4bit         (4-bit scaled coefficient)
//! Byte 3: s1              (sign bit: 0 or 1)
//! Byte 4: k2_4bit         (4-bit scaled coefficient)
//! Byte 5: s2              (sign bit: 0 or 1)
//! Byte 6: clip (8-bit)
//! ```
//!
//! ### Friction (slot 3, effect type 0x0e)
//! ```text
//! Byte 1: 0x0e
//! Byte 2: k1_8bit         (8-bit scaled coefficient)
//! Byte 3: k2_8bit         (8-bit scaled coefficient)
//! Byte 4: clip (8-bit)
//! Byte 5: (s2 << 4) | s1  (sign nibble)
//! Byte 6: 0x00
//! ```
//!
//! Source: `lg4ff_update_slot()` in berarma/new-lg4ff `hid-lg4ff.c`.

#![deny(static_mut_refs)]

/// Wire size of a slot FFB command.
pub const SLOT_CMD_SIZE: usize = 7;

/// Slot IDs matching the kernel's 4-slot system.
pub mod slot {
    pub const CONSTANT: u8 = 0;
    pub const SPRING: u8 = 1;
    pub const DAMPER: u8 = 2;
    pub const FRICTION: u8 = 3;
}

/// Operations (combined with slot_id in byte 0).
pub mod op {
    /// Start a new effect in the slot.
    pub const START: u8 = 0x01;
    /// Stop the effect in the slot.
    pub const STOP: u8 = 0x03;
    /// Update the running effect's parameters.
    pub const UPDATE: u8 = 0x0C;
}

/// Effect type bytes (byte 1 of the 7-byte payload).
pub mod effect_type {
    /// Constant force (byte position = 2 + slot_id).
    pub const CONSTANT: u8 = 0x00;
    /// Spring condition.
    pub const SPRING: u8 = 0x0B;
    /// Damper condition.
    pub const DAMPER: u8 = 0x0C;
    /// Friction condition.
    pub const FRICTION: u8 = 0x0E;
}

/// Build the command byte: `(slot_id << 4) | operation`.
fn cmd_byte(slot_id: u8, operation: u8) -> u8 {
    (slot_id << 4) | operation
}

/// Translate a signed force level to the kernel's unsigned 8-bit format.
///
/// Matches the kernel macro `TRANSLATE_FORCE(x)`:
/// `((CLAMP_VALUE_S16(x) + 0x8000) >> 8)`
///
/// - `0x80` = no force (center)
/// - `0x00` = full negative (left)
/// - `0xFF` = full positive (right)
pub fn translate_force(level: i16) -> u8 {
    let clamped = level.max(-0x7FFF) as i32;
    ((clamped + 0x8000) >> 8) as u8
}

/// Scale a coefficient to N bits using the kernel macro `SCALE_COEFF`:
/// `SCALE_VALUE_U16(abs(x) * 2, bits)` = `(min(abs(x)*2, 0xFFFF)) >> (16 - bits)`
fn scale_coeff(coeff: i16, bits: u8) -> u8 {
    let abs_val = (coeff as i32).unsigned_abs();
    let doubled = (abs_val * 2).min(0xFFFF);
    (doubled >> (16 - bits)) as u8
}

/// Scale an unsigned 16-bit value to N bits.
/// Matches `SCALE_VALUE_U16(x, bits)` = `min(x, 0xFFFF) >> (16 - bits)`.
fn scale_value_u16(val: u16, bits: u8) -> u8 {
    (val >> (16 - bits)) as u8
}

/// Encode a constant force slot command.
///
/// `force_level` is a signed i16 where:
/// - Negative = left/counter-clockwise
/// - Positive = right/clockwise
/// - 0 = no force
///
/// Returns a 7-byte command for slot 0.
pub fn encode_constant(operation: u8, force_level: i16) -> [u8; SLOT_CMD_SIZE] {
    let force_byte = translate_force(force_level);
    let mut cmd = [0u8; SLOT_CMD_SIZE];
    cmd[0] = cmd_byte(slot::CONSTANT, operation);
    cmd[1] = effect_type::CONSTANT;
    // Force goes in byte (2 + slot_id). For slot 0, that's byte 2.
    cmd[2] = force_byte;
    cmd
}

/// Encode a spring effect slot command.
///
/// Parameters match the kernel's `lg4ff_effect_parameters`:
/// - `d1`, `d2`: deadband positions (signed i16, mapped to u16 offset by 0x8000)
/// - `k1`, `k2`: spring coefficients (signed i16)
/// - `clip`: saturation clip level (unsigned u16, scaled to 8 bits)
pub fn encode_spring(
    operation: u8,
    d1: i16,
    d2: i16,
    k1: i16,
    k2: i16,
    clip: u16,
) -> [u8; SLOT_CMD_SIZE] {
    // Map signed positions to unsigned offset (kernel: ((d+0x8000) & 0xffff))
    let d1_u = ((d1 as i32 + 0x8000) & 0xFFFF) as u16;
    let d2_u = ((d2 as i32 + 0x8000) & 0xFFFF) as u16;

    let s1 = if k1 < 0 { 1u8 } else { 0u8 };
    let s2 = if k2 < 0 { 1u8 } else { 0u8 };

    // Scale to 11 bits
    let d1_11 = scale_value_u16(d1_u, 11) as u16;
    let d2_11 = scale_value_u16(d2_u, 11) as u16;

    // Apply deadband threshold from kernel
    let (d1_final, k1_scaled) = if (k1 as i32).unsigned_abs() < 2048 {
        (0u16, scale_coeff(k1, 4))
    } else {
        let adjusted_k = if k1 >= 0 {
            k1.saturating_sub(2048_i16.min(k1))
        } else {
            k1.saturating_add(2048i32.min((-(k1 as i32)).max(0)) as i16)
        };
        (d1_11, scale_coeff(adjusted_k, 4))
    };

    let (d2_final, k2_scaled) = if (k2 as i32).unsigned_abs() < 2048 {
        (2047u16, scale_coeff(k2, 4))
    } else {
        let adjusted_k = if k2 >= 0 {
            k2.saturating_sub(2048_i16.min(k2))
        } else {
            k2.saturating_add(2048i32.min((-(k2 as i32)).max(0)) as i16)
        };
        (d2_11, scale_coeff(adjusted_k, 4))
    };

    let mut cmd = [0u8; SLOT_CMD_SIZE];
    cmd[0] = cmd_byte(slot::SPRING, operation);
    cmd[1] = effect_type::SPRING;
    cmd[2] = (d1_final >> 3) as u8;
    cmd[3] = (d2_final >> 3) as u8;
    cmd[4] = (k2_scaled << 4) | k1_scaled;
    cmd[5] = (((d2_final & 7) as u8) << 5) | (((d1_final & 7) as u8) << 1) | (s2 << 4) | s1;
    cmd[6] = scale_value_u16(clip, 8);
    cmd
}

/// Encode a damper effect slot command.
///
/// - `k1`, `k2`: damping coefficients (signed i16)
/// - `clip`: saturation clip level (u16, scaled to 8 bits)
pub fn encode_damper(operation: u8, k1: i16, k2: i16, clip: u16) -> [u8; SLOT_CMD_SIZE] {
    let s1 = if k1 < 0 { 1u8 } else { 0u8 };
    let s2 = if k2 < 0 { 1u8 } else { 0u8 };

    let mut cmd = [0u8; SLOT_CMD_SIZE];
    cmd[0] = cmd_byte(slot::DAMPER, operation);
    cmd[1] = effect_type::DAMPER;
    cmd[2] = scale_coeff(k1, 4);
    cmd[3] = s1;
    cmd[4] = scale_coeff(k2, 4);
    cmd[5] = s2;
    cmd[6] = scale_value_u16(clip, 8);
    cmd
}

/// Encode a friction effect slot command.
///
/// - `k1`, `k2`: friction coefficients (signed i16)
/// - `clip`: saturation clip level (u16, scaled to 8 bits)
pub fn encode_friction(operation: u8, k1: i16, k2: i16, clip: u16) -> [u8; SLOT_CMD_SIZE] {
    let s1 = if k1 < 0 { 1u8 } else { 0u8 };
    let s2 = if k2 < 0 { 1u8 } else { 0u8 };

    let mut cmd = [0u8; SLOT_CMD_SIZE];
    cmd[0] = cmd_byte(slot::FRICTION, operation);
    cmd[1] = effect_type::FRICTION;
    cmd[2] = scale_coeff(k1, 8);
    cmd[3] = scale_coeff(k2, 8);
    cmd[4] = scale_value_u16(clip, 8);
    cmd[5] = (s2 << 4) | s1;
    cmd[6] = 0x00;
    cmd
}

/// Encode a slot stop command (any slot).
pub fn encode_slot_stop(slot_id: u8) -> [u8; SLOT_CMD_SIZE] {
    let mut cmd = [0u8; SLOT_CMD_SIZE];
    cmd[0] = cmd_byte(slot_id, op::STOP);
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_translate_force_center() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(translate_force(0), 0x80);
        Ok(())
    }

    #[test]
    fn test_translate_force_full_left() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(translate_force(-0x7FFF), 0x00);
        Ok(())
    }

    #[test]
    fn test_translate_force_full_right() -> Result<(), Box<dyn std::error::Error>> {
        // 0x7FFF + 0x8000 = 0xFFFF; >> 8 = 0xFF
        assert_eq!(translate_force(0x7FFF), 0xFF);
        Ok(())
    }

    #[test]
    fn test_cmd_byte_constant_start() -> Result<(), Box<dyn std::error::Error>> {
        // slot 0 start: (0 << 4) | 0x01 = 0x01
        assert_eq!(cmd_byte(slot::CONSTANT, op::START), 0x01);
        Ok(())
    }

    #[test]
    fn test_cmd_byte_spring_update() -> Result<(), Box<dyn std::error::Error>> {
        // slot 1 update: (1 << 4) | 0x0c = 0x1c
        assert_eq!(cmd_byte(slot::SPRING, op::UPDATE), 0x1C);
        Ok(())
    }

    #[test]
    fn test_cmd_byte_damper_stop() -> Result<(), Box<dyn std::error::Error>> {
        // slot 2 stop: (2 << 4) | 0x03 = 0x23
        assert_eq!(cmd_byte(slot::DAMPER, op::STOP), 0x23);
        Ok(())
    }

    #[test]
    fn test_cmd_byte_friction_start() -> Result<(), Box<dyn std::error::Error>> {
        // slot 3 start: (3 << 4) | 0x01 = 0x31
        assert_eq!(cmd_byte(slot::FRICTION, op::START), 0x31);
        Ok(())
    }

    #[test]
    fn test_constant_force_zero() -> Result<(), Box<dyn std::error::Error>> {
        let cmd = encode_constant(op::START, 0);
        assert_eq!(cmd[0], 0x01, "slot 0, start op");
        assert_eq!(cmd[1], 0x00, "constant effect type");
        assert_eq!(cmd[2], 0x80, "zero force = 0x80 (center)");
        Ok(())
    }

    #[test]
    fn test_constant_force_full_positive() -> Result<(), Box<dyn std::error::Error>> {
        let cmd = encode_constant(op::UPDATE, 0x7FFF);
        assert_eq!(cmd[0], 0x0C, "slot 0, update op");
        assert_eq!(cmd[2], 0xFF, "full positive = 0xFF");
        Ok(())
    }

    #[test]
    fn test_constant_force_full_negative() -> Result<(), Box<dyn std::error::Error>> {
        let cmd = encode_constant(op::UPDATE, -0x7FFF);
        assert_eq!(cmd[2], 0x00, "full negative = 0x00");
        Ok(())
    }

    #[test]
    fn test_damper_encoding_structure() -> Result<(), Box<dyn std::error::Error>> {
        let cmd = encode_damper(op::START, 0x4000, -0x4000, 0xFFFF);
        assert_eq!(cmd[0], cmd_byte(slot::DAMPER, op::START));
        assert_eq!(cmd[1], effect_type::DAMPER);
        assert_eq!(cmd[3], 0, "k1 positive → s1=0");
        assert_eq!(cmd[5], 1, "k2 negative → s2=1");
        assert_eq!(cmd[6], 0xFF, "full clip");
        Ok(())
    }

    #[test]
    fn test_friction_encoding_structure() -> Result<(), Box<dyn std::error::Error>> {
        let cmd = encode_friction(op::START, -0x2000, 0x2000, 0x8000);
        assert_eq!(cmd[0], cmd_byte(slot::FRICTION, op::START));
        assert_eq!(cmd[1], effect_type::FRICTION);
        assert_eq!(cmd[5] & 0x01, 1, "k1 negative → s1=1");
        assert_eq!((cmd[5] >> 4) & 0x01, 0, "k2 positive → s2=0");
        assert_eq!(cmd[6], 0x00, "friction trailing byte always 0");
        Ok(())
    }

    #[test]
    fn test_slot_stop() -> Result<(), Box<dyn std::error::Error>> {
        let cmd = encode_slot_stop(slot::SPRING);
        assert_eq!(cmd[0], cmd_byte(slot::SPRING, op::STOP));
        assert_eq!(&cmd[1..], &[0u8; 6], "stop command payload is all zeros");
        Ok(())
    }

    #[test]
    fn test_spring_encoding_basic() -> Result<(), Box<dyn std::error::Error>> {
        let cmd = encode_spring(op::START, 0, 0, 0x4000, 0x4000, 0xFFFF);
        assert_eq!(cmd[0], cmd_byte(slot::SPRING, op::START));
        assert_eq!(cmd[1], effect_type::SPRING);
        assert_eq!(cmd[6], 0xFF, "full clip");
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
        fn prop_translate_force_always_u8(level in i16::MIN..=i16::MAX) {
            let _result = translate_force(level);
            // Result is a u8, so always in [0, 255]. The type system guarantees this.
        }

        #[test]
        fn prop_translate_force_monotone(
            a in i16::MIN..=i16::MAX,
            b in i16::MIN..=i16::MAX,
        ) {
            let fa = translate_force(a);
            let fb = translate_force(b);
            if a < b {
                prop_assert!(
                    fa <= fb,
                    "monotone: force({}) = {} should be <= force({}) = {}",
                    a, fa, b, fb
                );
            }
        }

        #[test]
        fn prop_constant_cmd_byte_correct(
            force in i16::MIN..=i16::MAX,
            operation in prop_oneof![Just(op::START), Just(op::STOP), Just(op::UPDATE)],
        ) {
            let cmd = encode_constant(operation, force);
            let expected_op = (slot::CONSTANT << 4) | operation;
            prop_assert_eq!(cmd[0], expected_op, "constant cmd byte must match");
            prop_assert_eq!(cmd[1], effect_type::CONSTANT);
        }

        #[test]
        fn prop_damper_sign_bits(k1 in i16::MIN..=i16::MAX, k2 in i16::MIN..=i16::MAX) {
            let cmd = encode_damper(op::UPDATE, k1, k2, 0x8000);
            let s1_expected = if k1 < 0 { 1u8 } else { 0u8 };
            let s2_expected = if k2 < 0 { 1u8 } else { 0u8 };
            prop_assert_eq!(cmd[3], s1_expected, "k1 sign bit");
            prop_assert_eq!(cmd[5], s2_expected, "k2 sign bit");
        }

        #[test]
        fn prop_friction_sign_nibble(k1 in i16::MIN..=i16::MAX, k2 in i16::MIN..=i16::MAX) {
            let cmd = encode_friction(op::START, k1, k2, 0x8000);
            let s1 = cmd[5] & 0x01;
            let s2 = (cmd[5] >> 4) & 0x01;
            let s1_expected = if k1 < 0 { 1u8 } else { 0u8 };
            let s2_expected = if k2 < 0 { 1u8 } else { 0u8 };
            prop_assert_eq!(s1, s1_expected, "friction k1 sign");
            prop_assert_eq!(s2, s2_expected, "friction k2 sign");
            prop_assert_eq!(cmd[6], 0x00, "friction trailing byte");
        }

        #[test]
        fn prop_slot_stop_is_zeroed(slot_id in 0u8..=3u8) {
            let cmd = encode_slot_stop(slot_id);
            let expected_byte0 = (slot_id << 4) | op::STOP;
            prop_assert_eq!(cmd[0], expected_byte0);
            for (i, &b) in cmd[1..].iter().enumerate() {
                prop_assert_eq!(b, 0, "stop byte {} must be zero", i + 1);
            }
        }
    }
}

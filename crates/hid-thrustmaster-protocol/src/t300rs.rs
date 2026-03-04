//! T300RS-family wire-format encoders for the actual USB HID output reports.
//!
//! # Wire protocol (verified from hid-tmff2 FFBEFFECTS.md)
//!
//! Source: `Kimplul/hid-tmff2/docs/FFBEFFECTS.md` — USB-captured T300RS/T248/TX
//! packets. All values are little-endian.
//!
//! ## Report structure
//!
//! Output reports are 64 bytes (Report ID + 63 payload) in USB mode,
//! or 32 bytes in PS4 mode. The header is always `0x60 XX` where `XX`
//! is the command category.
//!
//! ## Command categories (byte[1] after header byte 0x60)
//!
//! | Header  | Purpose                              |
//! |---------|--------------------------------------|
//! | `60 00` | Effect create / modify / play / stop |
//! | `60 01` | Open / close FFB device              |
//! | `60 02` | Set gain (FF_GAIN)                   |
//! | `60 08` | Settings: range, autocenter          |
//! | `60 12` | Wheel-specific init (varies)         |
//!
//! ## Effect opcodes (byte[3] in `60 00` packets)
//!
//! | Opcode | Meaning                  |
//! |--------|--------------------------|
//! | `0x6a` | New constant effect      |
//! | `0x6b` | New ramp effect          |
//! | `0x0a` | Modify constant force    |
//! | `0x29` | Modify envelope (all)    |
//! | `0x49` | Modify duration / offset |
//! | `0x89` | Play / stop control      |
//!
//! ## Play control (byte[4] in `60 00 ID 89` packets)
//!
//! | Value | Meaning                        |
//! |-------|--------------------------------|
//! | `0x01`| Play once                      |
//! | `0x41`| Play with repeat count (u16 LE)|
//! | `0x00`| Stop                           |
//!
//! ## Settings sub-commands (byte[2] in `60 08` packets)
//!
//! | Sub-cmd | Purpose         | Value format          |
//! |---------|-----------------|-----------------------|
//! | `0x11`  | Rotation angle  | `degrees * 0x3C` LE16|
//! | `0x03`  | Autocenter force| LE16 (0x0000–0xFFFF) |
//! | `0x04`  | Autocenter enable| `0x01` = on          |

#![deny(static_mut_refs)]

/// Wire size of a T300RS-family output report (USB mode).
pub const T300RS_REPORT_SIZE: usize = 64;

/// Wire size of a T300RS-family output report (PS4 mode).
pub const T300RS_REPORT_SIZE_PS4: usize = 32;

/// Header byte that begins all T300RS-family output reports.
pub const HEADER_BYTE: u8 = 0x60;

/// Command categories (second byte of the 64-byte report).
pub mod cmd {
    /// Effect create / modify / play / stop commands.
    pub const EFFECT: u8 = 0x00;
    /// Open / close FFB device.
    pub const OPEN_CLOSE: u8 = 0x01;
    /// Set global gain (FF_GAIN).
    pub const GAIN: u8 = 0x02;
    /// Settings: rotation angle, autocenter.
    pub const SETTINGS: u8 = 0x08;
}

/// Effect opcodes used within `60 00` reports.
pub mod effect_op {
    /// Create a new constant-force effect.
    pub const NEW_CONSTANT: u8 = 0x6a;
    /// Create a new ramp effect.
    pub const NEW_RAMP: u8 = 0x6b;
    /// Modify constant-force magnitude.
    pub const MODIFY_CONSTANT: u8 = 0x0a;
    /// Modify envelope (all: attack/fade length/level).
    pub const MODIFY_ENVELOPE: u8 = 0x29;
    /// Modify duration and/or offset.
    pub const MODIFY_DURATION: u8 = 0x49;
    /// Play / stop control.
    pub const PLAY_CONTROL: u8 = 0x89;
}

/// Play-control values (byte[4] when opcode = 0x89).
pub mod play_ctl {
    /// Play effect once.
    pub const PLAY_ONCE: u8 = 0x01;
    /// Play with repeat count.
    pub const PLAY_REPEAT: u8 = 0x41;
    /// Stop effect.
    pub const STOP: u8 = 0x00;
}

/// Settings sub-commands for `60 08` packets.
pub mod settings {
    /// Set rotation angle.
    pub const ROTATION_ANGLE: u8 = 0x11;
    /// Set autocenter force level.
    pub const AUTOCENTER_FORCE: u8 = 0x03;
    /// Enable/disable autocenter.
    pub const AUTOCENTER_ENABLE: u8 = 0x04;
}

/// Encode a "play effect once" command into a 64-byte report buffer.
///
/// Wire format: `60 00 <effect_id> 89 01 ...zeros...`
pub fn encode_play_once(effect_id: u8, buf: &mut [u8; T300RS_REPORT_SIZE]) {
    buf.fill(0);
    buf[0] = HEADER_BYTE;
    buf[1] = cmd::EFFECT;
    buf[2] = effect_id;
    buf[3] = effect_op::PLAY_CONTROL;
    buf[4] = play_ctl::PLAY_ONCE;
}

/// Encode a "play effect N times" command.
///
/// `count = 0` means infinite. Wire format: `60 00 <id> 89 41 <count_lo> <count_hi> ...`
pub fn encode_play_repeat(effect_id: u8, count: u16, buf: &mut [u8; T300RS_REPORT_SIZE]) {
    buf.fill(0);
    buf[0] = HEADER_BYTE;
    buf[1] = cmd::EFFECT;
    buf[2] = effect_id;
    buf[3] = effect_op::PLAY_CONTROL;
    buf[4] = play_ctl::PLAY_REPEAT;
    let bytes = count.to_le_bytes();
    buf[5] = bytes[0];
    buf[6] = bytes[1];
}

/// Encode a "stop effect" command.
///
/// Wire format: `60 00 <id> 89 00 ...zeros...`
pub fn encode_stop_effect(effect_id: u8, buf: &mut [u8; T300RS_REPORT_SIZE]) {
    buf.fill(0);
    buf[0] = HEADER_BYTE;
    buf[1] = cmd::EFFECT;
    buf[2] = effect_id;
    buf[3] = effect_op::PLAY_CONTROL;
    buf[4] = play_ctl::STOP;
}

/// Encode a "modify constant force" command.
///
/// `magnitude` is a signed i16 in range [-16385, 16381] per FFBEFFECTS.md
/// (direction-affected). Wire format: `60 00 <id> 0a <mag_lo> <mag_hi> ...`
pub fn encode_modify_constant(effect_id: u8, magnitude: i16, buf: &mut [u8; T300RS_REPORT_SIZE]) {
    buf.fill(0);
    buf[0] = HEADER_BYTE;
    buf[1] = cmd::EFFECT;
    buf[2] = effect_id;
    buf[3] = effect_op::MODIFY_CONSTANT;
    let bytes = magnitude.to_le_bytes();
    buf[4] = bytes[0];
    buf[5] = bytes[1];
}

/// Encode the envelope (all four parameters) for an existing effect.
///
/// Wire format: `60 00 <id> 29 <atk_len_lo> <atk_len_hi> <atk_lvl_lo> <atk_lvl_hi>
///                              <fad_len_lo> <fad_len_hi> <fad_lvl_lo> <fad_lvl_hi> ...`
pub fn encode_modify_envelope(
    effect_id: u8,
    attack_length: u16,
    attack_level: u16,
    fade_length: u16,
    fade_level: u16,
    buf: &mut [u8; T300RS_REPORT_SIZE],
) {
    buf.fill(0);
    buf[0] = HEADER_BYTE;
    buf[1] = cmd::EFFECT;
    buf[2] = effect_id;
    buf[3] = effect_op::MODIFY_ENVELOPE;
    let al = attack_length.to_le_bytes();
    buf[4] = al[0];
    buf[5] = al[1];
    let alv = attack_level.to_le_bytes();
    buf[6] = alv[0];
    buf[7] = alv[1];
    let fl = fade_length.to_le_bytes();
    buf[8] = fl[0];
    buf[9] = fl[1];
    let flv = fade_level.to_le_bytes();
    buf[10] = flv[0];
    buf[11] = flv[1];
}

/// Encode the "set gain" command.
///
/// Wire format: `60 02 <gain> ...zeros...`
/// `gain` is 0x00–0xFF.
pub fn encode_set_gain(gain: u8, buf: &mut [u8; T300RS_REPORT_SIZE]) {
    buf.fill(0);
    buf[0] = HEADER_BYTE;
    buf[1] = cmd::GAIN;
    buf[2] = gain;
}

/// Encode the "set rotation angle" settings command.
///
/// Wire format: `60 08 11 <scaled_lo> <scaled_hi> ...zeros...`
/// where scaled = degrees * 0x3C. Clamped to 40–1080°.
pub fn encode_set_rotation(degrees: u16, buf: &mut [u8; T300RS_REPORT_SIZE]) {
    buf.fill(0);
    let clamped = degrees.clamp(40, 1080);
    let scaled = (clamped as u32) * 0x3C;
    let bytes = (scaled as u16).to_le_bytes();
    buf[0] = HEADER_BYTE;
    buf[1] = cmd::SETTINGS;
    buf[2] = settings::ROTATION_ANGLE;
    buf[3] = bytes[0];
    buf[4] = bytes[1];
}

/// Encode the "set autocenter force" settings command.
///
/// Wire format: `60 08 03 <force_lo> <force_hi> ...zeros...`
pub fn encode_set_autocenter(force: u16, buf: &mut [u8; T300RS_REPORT_SIZE]) {
    buf.fill(0);
    buf[0] = HEADER_BYTE;
    buf[1] = cmd::SETTINGS;
    buf[2] = settings::AUTOCENTER_FORCE;
    let bytes = force.to_le_bytes();
    buf[3] = bytes[0];
    buf[4] = bytes[1];
}

/// Encode the "open FFB device" command.
///
/// Wire format: `60 01 05 ...zeros...`
/// Followed by optional `60 01 04` on T248/TX (not encoded here).
pub fn encode_open(buf: &mut [u8; T300RS_REPORT_SIZE]) {
    buf.fill(0);
    buf[0] = HEADER_BYTE;
    buf[1] = cmd::OPEN_CLOSE;
    buf[2] = 0x05;
}

/// Encode the "close FFB device" command.
///
/// Wire format: `60 01 00 ...zeros...`
pub fn encode_close(buf: &mut [u8; T300RS_REPORT_SIZE]) {
    buf.fill(0);
    buf[0] = HEADER_BYTE;
    buf[1] = cmd::OPEN_CLOSE;
    buf[2] = 0x00;
}

/// Parameters for creating a new constant-force effect.
#[derive(Debug, Clone, Copy)]
pub struct NewConstantParams {
    /// Effect slot ID.
    pub effect_id: u8,
    /// Signed magnitude, approx [-16384, 16384].
    pub magnitude: i16,
    /// Envelope attack length in ms.
    pub attack_length: u16,
    /// Envelope attack level.
    pub attack_level: u16,
    /// Envelope fade length in ms.
    pub fade_length: u16,
    /// Envelope fade level.
    pub fade_level: u16,
    /// Effect duration in ms.
    pub duration_ms: u16,
    /// Start offset in ms.
    pub offset_ms: u16,
}

/// Create a new constant-force effect.
///
/// Wire format verified from FFBEFFECTS.md:
/// ```text
/// 60 00 <id> 6a <mag_lo> <mag_hi>
///    <atk_len_lo> <atk_len_hi> <atk_lvl_lo> <atk_lvl_hi>
///    <fad_len_lo> <fad_len_hi> <fad_lvl_lo> <fad_lvl_hi>
///    <effect_type> 4f <dur_lo> <dur_hi>
///    00 00 <offset_lo> <offset_hi> 00 ff ff ...zeros...
/// ```
pub fn encode_new_constant(params: &NewConstantParams, buf: &mut [u8; T300RS_REPORT_SIZE]) {
    buf.fill(0);
    buf[0] = HEADER_BYTE;
    buf[1] = cmd::EFFECT;
    buf[2] = params.effect_id;
    buf[3] = effect_op::NEW_CONSTANT;

    let mag = params.magnitude.to_le_bytes();
    buf[4] = mag[0];
    buf[5] = mag[1];

    let al = params.attack_length.to_le_bytes();
    buf[6] = al[0];
    buf[7] = al[1];

    let alv = params.attack_level.to_le_bytes();
    buf[8] = alv[0];
    buf[9] = alv[1];

    let fl = params.fade_length.to_le_bytes();
    buf[10] = fl[0];
    buf[11] = fl[1];

    let flv = params.fade_level.to_le_bytes();
    buf[12] = flv[0];
    buf[13] = flv[1];

    buf[14] = 0x00; // effect type marker
    buf[15] = 0x4F;

    let dur = params.duration_ms.to_le_bytes();
    buf[16] = dur[0];
    buf[17] = dur[1];

    buf[18] = 0x00;
    buf[19] = 0x00;

    let off = params.offset_ms.to_le_bytes();
    buf[20] = off[0];
    buf[21] = off[1];

    buf[22] = 0x00;
    buf[23] = 0xFF;
    buf[24] = 0xFF;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_play_once_matches_wire_capture() -> Result<(), Box<dyn std::error::Error>> {
        // Verified: "60 00 01 89 01 ...zeros..."
        let mut buf = [0u8; T300RS_REPORT_SIZE];
        encode_play_once(0x01, &mut buf);
        assert_eq!(buf[0], 0x60);
        assert_eq!(buf[1], 0x00);
        assert_eq!(buf[2], 0x01);
        assert_eq!(buf[3], 0x89);
        assert_eq!(buf[4], 0x01);
        assert_eq!(buf[5..], vec![0u8; 59]);
        Ok(())
    }

    #[test]
    fn test_play_infinite_matches_wire_capture() -> Result<(), Box<dyn std::error::Error>> {
        // Verified: count=0 means infinite
        let mut buf = [0u8; T300RS_REPORT_SIZE];
        encode_play_repeat(0x01, 0, &mut buf);
        assert_eq!(buf[3], 0x89);
        assert_eq!(buf[4], 0x41);
        assert_eq!(buf[5], 0x00);
        assert_eq!(buf[6], 0x00);
        Ok(())
    }

    #[test]
    fn test_stop_matches_wire_capture() -> Result<(), Box<dyn std::error::Error>> {
        // Verified: "60 00 01 89 00 ...zeros..."
        let mut buf = [0u8; T300RS_REPORT_SIZE];
        encode_stop_effect(0x01, &mut buf);
        assert_eq!(buf[3], 0x89);
        assert_eq!(buf[4], 0x00);
        Ok(())
    }

    #[test]
    fn test_modify_constant_force() -> Result<(), Box<dyn std::error::Error>> {
        // Verified against FFBEFFECTS.md: "60 00 01 0a 05 16 ...zeros..."
        let mut buf = [0u8; T300RS_REPORT_SIZE];
        encode_modify_constant(0x01, 0x1605_i16, &mut buf);
        assert_eq!(buf[0..6], [0x60, 0x00, 0x01, 0x0a, 0x05, 0x16]);
        Ok(())
    }

    #[test]
    fn test_modify_envelope_all() -> Result<(), Box<dyn std::error::Error>> {
        // Verified: "60 00 01 29 e8 03 cc 0c dc 05 cc 0c ..."
        let mut buf = [0u8; T300RS_REPORT_SIZE];
        encode_modify_envelope(0x01, 0x03E8, 0x0CCC, 0x05DC, 0x0CCC, &mut buf);
        assert_eq!(
            buf[0..12],
            [
                0x60, 0x00, 0x01, 0x29, 0xE8, 0x03, 0xCC, 0x0C, 0xDC, 0x05, 0xCC, 0x0C
            ]
        );
        Ok(())
    }

    #[test]
    fn test_set_gain_bf() -> Result<(), Box<dyn std::error::Error>> {
        // Verified: "60 02 bf ...zeros..."
        let mut buf = [0u8; T300RS_REPORT_SIZE];
        encode_set_gain(0xBF, &mut buf);
        assert_eq!(buf[0..3], [0x60, 0x02, 0xBF]);
        Ok(())
    }

    #[test]
    fn test_set_rotation_900() -> Result<(), Box<dyn std::error::Error>> {
        // 900 * 0x3C = 900 * 60 = 54000 = 0xD2F0
        let mut buf = [0u8; T300RS_REPORT_SIZE];
        encode_set_rotation(900, &mut buf);
        assert_eq!(buf[0..5], [0x60, 0x08, 0x11, 0xF0, 0xD2]);
        Ok(())
    }

    #[test]
    fn test_set_autocenter() -> Result<(), Box<dyn std::error::Error>> {
        // Verified: "60 08 03 8f 02 ...zeros..."
        let mut buf = [0u8; T300RS_REPORT_SIZE];
        encode_set_autocenter(0x028F, &mut buf);
        assert_eq!(buf[0..5], [0x60, 0x08, 0x03, 0x8F, 0x02]);
        Ok(())
    }

    #[test]
    fn test_open_command() -> Result<(), Box<dyn std::error::Error>> {
        let mut buf = [0u8; T300RS_REPORT_SIZE];
        encode_open(&mut buf);
        assert_eq!(buf[0..3], [0x60, 0x01, 0x05]);
        Ok(())
    }

    #[test]
    fn test_close_command() -> Result<(), Box<dyn std::error::Error>> {
        let mut buf = [0u8; T300RS_REPORT_SIZE];
        encode_close(&mut buf);
        assert_eq!(buf[0..3], [0x60, 0x01, 0x00]);
        Ok(())
    }

    #[test]
    fn test_new_constant_effect_structure() -> Result<(), Box<dyn std::error::Error>> {
        let mut buf = [0u8; T300RS_REPORT_SIZE];
        encode_new_constant(
            &NewConstantParams {
                effect_id: 0x01,
                magnitude: -1_i16,
                attack_length: 0,
                attack_level: 0,
                fade_length: 0,
                fade_level: 0,
                duration_ms: 6135,
                offset_ms: 7,
            },
            &mut buf,
        );
        assert_eq!(buf[0], 0x60); // header
        assert_eq!(buf[1], 0x00); // effect command
        assert_eq!(buf[2], 0x01); // effect_id
        assert_eq!(buf[3], 0x6a); // new constant
        assert_eq!(buf[4], 0xFF); // magnitude low
        assert_eq!(buf[5], 0xFF); // magnitude high (0xFFFF = -1 i16)
        assert_eq!(buf[14], 0x00); // effect type
        assert_eq!(buf[15], 0x4F);
        assert_eq!(buf[16], 0xF7); // duration low
        assert_eq!(buf[17], 0x17); // duration high
        assert_eq!(buf[20], 0x07); // offset low
        assert_eq!(buf[21], 0x00); // offset high
        assert_eq!(buf[22], 0x00);
        assert_eq!(buf[23], 0xFF); // end marker
        assert_eq!(buf[24], 0xFF); // end marker
        Ok(())
    }

    #[test]
    fn test_report_size_is_64() {
        assert_eq!(T300RS_REPORT_SIZE, 64);
    }

    #[test]
    fn test_all_buffers_are_zeroed_outside_payload() -> Result<(), Box<dyn std::error::Error>> {
        let mut buf = [0xFFu8; T300RS_REPORT_SIZE];
        encode_set_gain(0x80, &mut buf);
        // Everything after byte 2 should be zero
        for (i, &b) in buf[3..].iter().enumerate() {
            assert_eq!(b, 0, "byte {} should be zero, was {:#04x}", i + 3, b);
        }
        Ok(())
    }

    #[test]
    fn test_rotation_clamps_low() -> Result<(), Box<dyn std::error::Error>> {
        let mut buf = [0u8; T300RS_REPORT_SIZE];
        encode_set_rotation(10, &mut buf);
        // Clamped to 40: 40 * 60 = 2400 = 0x0960
        assert_eq!(buf[3], 0x60);
        assert_eq!(buf[4], 0x09);
        Ok(())
    }

    #[test]
    fn test_rotation_clamps_high() -> Result<(), Box<dyn std::error::Error>> {
        let mut buf = [0u8; T300RS_REPORT_SIZE];
        encode_set_rotation(2000, &mut buf);
        // Clamped to 1080: 1080 * 60 = 64800 = 0xFD20
        assert_eq!(buf[3], 0x20);
        assert_eq!(buf[4], 0xFD);
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
        fn prop_header_always_0x60(
            effect_id in 0u8..=255u8,
            magnitude in -16385i16..=16381i16,
        ) {
            let mut buf = [0u8; T300RS_REPORT_SIZE];
            encode_modify_constant(effect_id, magnitude, &mut buf);
            prop_assert_eq!(buf[0], 0x60, "header must always be 0x60");
        }

        #[test]
        fn prop_magnitude_roundtrips(
            effect_id in 0u8..=255u8,
            magnitude in -16385i16..=16381i16,
        ) {
            let mut buf = [0u8; T300RS_REPORT_SIZE];
            encode_modify_constant(effect_id, magnitude, &mut buf);
            let decoded = i16::from_le_bytes([buf[4], buf[5]]);
            prop_assert_eq!(decoded, magnitude, "magnitude must round-trip");
        }

        #[test]
        fn prop_rotation_in_valid_range(degrees in 0u16..=2000u16) {
            let mut buf = [0u8; T300RS_REPORT_SIZE];
            encode_set_rotation(degrees, &mut buf);
            let scaled = u16::from_le_bytes([buf[3], buf[4]]);
            let min_scaled = 40u16 * 60;
            let max_scaled = 1080u16 * 60;
            prop_assert!(
                scaled >= min_scaled && scaled <= max_scaled,
                "scaled {} out of range [{}, {}]",
                scaled,
                min_scaled,
                max_scaled
            );
        }

        #[test]
        fn prop_play_control_opcodes(effect_id in 0u8..=255u8) {
            let mut buf = [0u8; T300RS_REPORT_SIZE];

            encode_play_once(effect_id, &mut buf);
            prop_assert_eq!(buf[3], 0x89, "play opcode must be 0x89");
            prop_assert_eq!(buf[4], 0x01, "play_once value must be 0x01");

            encode_stop_effect(effect_id, &mut buf);
            prop_assert_eq!(buf[3], 0x89, "stop opcode must be 0x89");
            prop_assert_eq!(buf[4], 0x00, "stop value must be 0x00");
        }

        #[test]
        fn prop_gain_single_byte(gain in 0u8..=255u8) {
            let mut buf = [0u8; T300RS_REPORT_SIZE];
            encode_set_gain(gain, &mut buf);
            prop_assert_eq!(buf[0], 0x60);
            prop_assert_eq!(buf[1], 0x02);
            prop_assert_eq!(buf[2], gain);
            // rest is zeroed
            for (i, &b) in buf[3..].iter().enumerate() {
                prop_assert_eq!(b, 0, "byte {} should be zero", i + 3);
            }
        }

        #[test]
        fn prop_envelope_fields_roundtrip(
            atk_len in 0u16..=10000u16,
            atk_lvl in 0u16..=32767u16,
            fad_len in 0u16..=10000u16,
            fad_lvl in 0u16..=32767u16,
        ) {
            let mut buf = [0u8; T300RS_REPORT_SIZE];
            encode_modify_envelope(1, atk_len, atk_lvl, fad_len, fad_lvl, &mut buf);
            let d_atk_len = u16::from_le_bytes([buf[4], buf[5]]);
            let d_atk_lvl = u16::from_le_bytes([buf[6], buf[7]]);
            let d_fad_len = u16::from_le_bytes([buf[8], buf[9]]);
            let d_fad_lvl = u16::from_le_bytes([buf[10], buf[11]]);
            prop_assert_eq!(d_atk_len, atk_len);
            prop_assert_eq!(d_atk_lvl, atk_lvl);
            prop_assert_eq!(d_fad_len, fad_len);
            prop_assert_eq!(d_fad_lvl, fad_lvl);
        }
    }
}

//! Simagic 64-byte FFB wire-format encoders (kernel protocol).
//!
//! # Wire protocol (verified from JacKeTUs/simagic-ff `hid-simagic.c`)
//!
//! Simagic wheelbases use 64-byte HID Output Reports where byte 0 is
//! the report type and subsequent bytes carry LE16 parameters.
//!
//! ## Report types
//!
//! | Type   | Name               | Purpose                     |
//! |--------|--------------------|-----------------------------|
//! | `0x01` | SET_EFFECT         | Create/update effect slot   |
//! | `0x03` | SET_CONDITION      | Spring/damper/friction/etc  |
//! | `0x04` | SET_PERIODIC       | Sine (and unsupported waves)|
//! | `0x05` | SET_CONSTANT       | Constant force              |
//! | `0x0A` | EFFECT_OPERATION   | Start/stop effect playback  |
//! | `0x12` | SET_ENVELOPE       | Attack/fade envelope        |
//! | `0x16` | SET_RAMP_FORCE     | Ramp force (no hw effect)   |
//! | `0x17` | SET_CUSTOM_FORCE   | Custom force (no hw effect) |
//! | `0x40` | SET_GAIN           | Device-wide FFB gain        |
//!
//! ## Block IDs (effect types)
//!
//! | Block ID | Effect    | Notes                              |
//! |----------|-----------|------------------------------------|
//! | `0x01`   | Constant  | Primary FFB                        |
//! | `0x02`   | Sine      | Only supported periodic waveform   |
//! | `0x05`   | Damper    |                                    |
//! | `0x06`   | Spring    |                                    |
//! | `0x07`   | Friction  |                                    |
//! | `0x09`   | Inertia   |                                    |
//! | `0x0E`   | Ramp      | No effect seen on wheelbase        |
//! | `0x0F`   | Square    | No effect seen on wheelbase        |
//! | `0x10`   | Triangle  | No effect seen on wheelbase        |
//! | `0x11`   | SawUp     | No effect seen on wheelbase        |
//! | `0x12`   | SawDown   | No effect seen on wheelbase        |
//!
//! ## Magnitude scaling
//!
//! `sm_rescale_signed_to_10k()`:
//! - Positive: `value * 10000 / 0x7FFF`
//! - Negative: `value * -10000 / -0x8000`
//! - Zero: 0
//!
//! ## VID/PIDs (from hid-simagic.h)
//!
//! | VID      | PID      | Device                |
//! |----------|----------|-----------------------|
//! | `0x0483` | `0x0522` | Alpha (original VID)  |
//! | `0x3670` | `0x0500` | EVO                   |
//! | `0x3670` | `0x0501` | EVO variant 1         |
//! | `0x3670` | `0x0502` | EVO variant 2         |
//!
//! Source: JacKeTUs/simagic-ff (commit 52e73e7) `hid-simagic.c` + `hid-simagic.h`

#![deny(static_mut_refs)]

/// Wire size of all Simagic output reports.
pub const REPORT_SIZE: usize = 64;

/// Report type identifiers (byte 0 of the 64-byte report).
pub mod report_type {
    pub const SET_EFFECT: u8 = 0x01;
    pub const SET_CONDITION: u8 = 0x03;
    pub const SET_PERIODIC: u8 = 0x04;
    pub const SET_CONSTANT: u8 = 0x05;
    pub const EFFECT_OPERATION: u8 = 0x0A;
    pub const SET_ENVELOPE: u8 = 0x12;
    pub const SET_RAMP_FORCE: u8 = 0x16;
    pub const SET_CUSTOM_FORCE: u8 = 0x17;
    pub const SET_GAIN: u8 = 0x40;
}

/// Block IDs that identify effect types on the device.
pub mod block_id {
    pub const CONSTANT: u8 = 0x01;
    pub const SINE: u8 = 0x02;
    pub const DAMPER: u8 = 0x05;
    pub const SPRING: u8 = 0x06;
    pub const FRICTION: u8 = 0x07;
    pub const INERTIA: u8 = 0x09;
    pub const RAMP: u8 = 0x0E;
    pub const SQUARE: u8 = 0x0F;
    pub const TRIANGLE: u8 = 0x10;
    pub const SAWTOOTH_UP: u8 = 0x11;
    pub const SAWTOOTH_DOWN: u8 = 0x12;
}

/// Effect operation codes for `EFFECT_OPERATION` reports.
pub mod effect_op {
    pub const START: u8 = 0x01;
    pub const STOP: u8 = 0x03;
}

/// Rescale a signed 16-bit value to ±10000 (Simagic firmware range).
///
/// Matches the kernel driver's `sm_rescale_signed_to_10k()`:
/// - Positive: `value * 10000 / 0x7FFF`
/// - Negative: `value * -10000 / -0x8000`
/// - Zero: `0`
pub fn rescale_signed_to_10k(value: i16) -> i16 {
    if value == 0 {
        return 0;
    }
    let v = value as i32;
    if v > 0 {
        (v * 10000 / 0x7FFF) as i16
    } else {
        (v * -10000 / -0x8000) as i16
    }
}

/// Rescale an unsigned value with range 0..max to [min_field, max_field].
///
/// Matches the kernel driver's `sm_rescale_coeffs()`.
pub fn rescale_coeffs(value: i32, max: i32, min_field: i32, max_field: i32) -> i16 {
    (value * (max_field - min_field) / max + min_field) as i16
}

/// Write an LE16 value at the given offset in a 64-byte buffer.
fn write_le16(buf: &mut [u8; REPORT_SIZE], offset: usize, value: i16) {
    let bytes = value.to_le_bytes();
    buf[offset] = bytes[0];
    buf[offset + 1] = bytes[1];
}

/// Encode a "set effect" report — creates/updates an effect slot.
///
/// Wire format: `[0x01, block_id, 0x01, dur_lo, dur_hi, 0..., gain=0xFF, trigger=0xFF, 0x04, 0x3F, 0...]`
pub fn encode_set_effect(
    effect_block_id: u8,
    duration_ms: u16,
) -> [u8; REPORT_SIZE] {
    let mut buf = [0u8; REPORT_SIZE];
    let dur = if duration_ms == 0 { 0xFFFFu16 } else { duration_ms };
    buf[0] = report_type::SET_EFFECT;
    buf[1] = effect_block_id;
    buf[2] = 0x01; // always 1 per kernel driver
    buf[3] = (dur & 0xFF) as u8;
    buf[4] = ((dur >> 8) & 0xFF) as u8;
    buf[9] = 0xFF;  // gain
    buf[10] = 0xFF; // trigger button
    buf[11] = 0x04;
    buf[12] = 0x3F;
    buf
}

/// Encode a constant force report.
///
/// Wire format: `[0x05, block_id, mag_lo, mag_hi, 0...]`
/// Magnitude is in ±10000 range (use `rescale_signed_to_10k` to convert from i16).
pub fn encode_constant(level: i16) -> [u8; REPORT_SIZE] {
    let mut buf = [0u8; REPORT_SIZE];
    let mag = rescale_signed_to_10k(level);
    buf[0] = report_type::SET_CONSTANT;
    buf[1] = block_id::CONSTANT;
    write_le16(&mut buf, 2, mag);
    buf
}

/// Parameters for a condition effect (spring, damper, friction, inertia).
pub struct ConditionParams {
    /// Center position (signed i16).
    pub center: i16,
    /// Right coefficient (signed i16).
    pub right_coeff: i16,
    /// Left coefficient (signed i16).
    pub left_coeff: i16,
    /// Right saturation (unsigned u16).
    pub right_saturation: u16,
    /// Left saturation (unsigned u16).
    pub left_saturation: u16,
    /// Deadband width (unsigned u16).
    pub deadband: u16,
}

/// Encode a condition effect (spring/damper/friction/inertia).
///
/// Wire format:
/// ```text
/// [0x03, block_id, 0x00,
///  center_lo, center_hi,
///  right_coeff_lo, right_coeff_hi,
///  left_coeff_lo, left_coeff_hi,
///  right_sat_lo, right_sat_hi,
///  left_sat_lo, left_sat_hi,
///  deadband_lo, deadband_hi, 0...]
/// ```
pub fn encode_condition(effect_block_id: u8, params: &ConditionParams) -> [u8; REPORT_SIZE] {
    let mut buf = [0u8; REPORT_SIZE];
    buf[0] = report_type::SET_CONDITION;
    buf[1] = effect_block_id;
    buf[2] = 0x00;

    let center = rescale_signed_to_10k(params.center);
    let right_coeff = rescale_signed_to_10k(params.right_coeff);
    let left_coeff = rescale_signed_to_10k(params.left_coeff);
    let right_sat = rescale_coeffs(params.right_saturation as i32, 0xFFFF, -10000, 10000);
    let left_sat = rescale_coeffs(params.left_saturation as i32, 0xFFFF, -10000, 10000);
    let deadband = rescale_coeffs(params.deadband as i32, 0xFFFF, 0, 10000);

    write_le16(&mut buf, 3, center);
    write_le16(&mut buf, 5, right_coeff);
    write_le16(&mut buf, 7, left_coeff);
    write_le16(&mut buf, 9, right_sat);
    write_le16(&mut buf, 11, left_sat);
    write_le16(&mut buf, 13, deadband);
    buf
}

/// Encode a periodic effect (sine waveform).
///
/// Wire format:
/// ```text
/// [0x04, block_id, mag_lo, mag_hi, offset_lo, offset_hi,
///  phase_lo, phase_hi, period_lo, period_hi, 0...]
/// ```
///
/// Note: Only sine (block 0x02) has confirmed hardware effect per the kernel
/// driver. Square/triangle/sawtooth block IDs are defined but produce "no
/// effect seen on wheelbase".
pub fn encode_periodic(
    effect_block_id: u8,
    magnitude: i16,
    offset: i16,
    phase: i16,
    period: i16,
) -> [u8; REPORT_SIZE] {
    let mut buf = [0u8; REPORT_SIZE];
    buf[0] = report_type::SET_PERIODIC;
    buf[1] = effect_block_id;

    let mag = rescale_signed_to_10k(magnitude);
    let off = rescale_signed_to_10k(offset);
    let ph = rescale_signed_to_10k(phase);
    let per = rescale_signed_to_10k(period);

    write_le16(&mut buf, 2, mag);
    write_le16(&mut buf, 4, off);
    write_le16(&mut buf, 6, ph);
    write_le16(&mut buf, 8, per);
    buf
}

/// Encode an effect operation (start/stop).
///
/// Wire format: `[0x0A, block_id, operation, loop_count, 0x00, 0...]`
pub fn encode_effect_operation(
    effect_block_id: u8,
    start: bool,
    loop_count: u8,
) -> [u8; REPORT_SIZE] {
    let mut buf = [0u8; REPORT_SIZE];
    buf[0] = report_type::EFFECT_OPERATION;
    buf[1] = effect_block_id;
    if start {
        buf[2] = effect_op::START;
        buf[3] = loop_count;
    } else {
        buf[2] = effect_op::STOP;
        buf[3] = 0x00;
    }
    buf
}

/// Encode a gain control report.
///
/// Wire format: `[0x40, gain_hi, 0...]`
/// The kernel driver sends `gain >> 8` as byte 1.
pub fn encode_gain(gain: u16) -> [u8; REPORT_SIZE] {
    let mut buf = [0u8; REPORT_SIZE];
    buf[0] = report_type::SET_GAIN;
    buf[1] = (gain >> 8) as u8;
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rescale_zero() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(rescale_signed_to_10k(0), 0);
        Ok(())
    }

    #[test]
    fn test_rescale_max_positive() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(rescale_signed_to_10k(0x7FFF), 10000);
        Ok(())
    }

    #[test]
    fn test_rescale_max_negative() -> Result<(), Box<dyn std::error::Error>> {
        // -0x8000 * -10000 / -0x8000 = -10000
        assert_eq!(rescale_signed_to_10k(-0x7FFF), -9999);
        Ok(())
    }

    #[test]
    fn test_rescale_min_negative() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(rescale_signed_to_10k(i16::MIN), -10000);
        Ok(())
    }

    #[test]
    fn test_rescale_half_positive() -> Result<(), Box<dyn std::error::Error>> {
        let half = 0x7FFF / 2;
        let result = rescale_signed_to_10k(half);
        assert!(result > 4900 && result < 5100, "half should ≈ 5000, got {result}");
        Ok(())
    }

    #[test]
    fn test_constant_report_structure() -> Result<(), Box<dyn std::error::Error>> {
        let buf = encode_constant(0x4000);
        assert_eq!(buf[0], report_type::SET_CONSTANT);
        assert_eq!(buf[1], block_id::CONSTANT);
        // Check magnitude is LE16 at bytes 2-3
        let mag = i16::from_le_bytes([buf[2], buf[3]]);
        assert_eq!(mag, rescale_signed_to_10k(0x4000));
        // Rest should be zero
        for byte in &buf[4..REPORT_SIZE] {
            assert_eq!(*byte, 0, "trailing bytes should be 0");
        }
        Ok(())
    }

    #[test]
    fn test_constant_zero() -> Result<(), Box<dyn std::error::Error>> {
        let buf = encode_constant(0);
        let mag = i16::from_le_bytes([buf[2], buf[3]]);
        assert_eq!(mag, 0);
        Ok(())
    }

    #[test]
    fn test_set_effect_structure() -> Result<(), Box<dyn std::error::Error>> {
        let buf = encode_set_effect(block_id::CONSTANT, 1000);
        assert_eq!(buf[0], report_type::SET_EFFECT);
        assert_eq!(buf[1], block_id::CONSTANT);
        assert_eq!(buf[2], 0x01, "always 1");
        assert_eq!(u16::from_le_bytes([buf[3], buf[4]]), 1000);
        assert_eq!(buf[9], 0xFF, "gain");
        assert_eq!(buf[10], 0xFF, "trigger button");
        assert_eq!(buf[11], 0x04);
        assert_eq!(buf[12], 0x3F);
        Ok(())
    }

    #[test]
    fn test_set_effect_zero_duration_becomes_infinite() -> Result<(), Box<dyn std::error::Error>> {
        let buf = encode_set_effect(block_id::CONSTANT, 0);
        assert_eq!(u16::from_le_bytes([buf[3], buf[4]]), 0xFFFF);
        Ok(())
    }

    #[test]
    fn test_condition_spring() -> Result<(), Box<dyn std::error::Error>> {
        let params = ConditionParams {
            center: 0,
            right_coeff: 0x4000,
            left_coeff: -0x4000,
            right_saturation: 0xFFFF,
            left_saturation: 0x8000,
            deadband: 0x1000,
        };
        let buf = encode_condition(block_id::SPRING, &params);
        assert_eq!(buf[0], report_type::SET_CONDITION);
        assert_eq!(buf[1], block_id::SPRING);
        assert_eq!(buf[2], 0x00);
        // Center should be 0
        let center = i16::from_le_bytes([buf[3], buf[4]]);
        assert_eq!(center, 0);
        Ok(())
    }

    #[test]
    fn test_periodic_sine() -> Result<(), Box<dyn std::error::Error>> {
        let buf = encode_periodic(block_id::SINE, 0x7FFF, 0, 0, 100);
        assert_eq!(buf[0], report_type::SET_PERIODIC);
        assert_eq!(buf[1], block_id::SINE);
        let mag = i16::from_le_bytes([buf[2], buf[3]]);
        assert_eq!(mag, 10000, "full magnitude");
        Ok(())
    }

    #[test]
    fn test_effect_operation_start() -> Result<(), Box<dyn std::error::Error>> {
        let buf = encode_effect_operation(block_id::CONSTANT, true, 5);
        assert_eq!(buf[0], report_type::EFFECT_OPERATION);
        assert_eq!(buf[1], block_id::CONSTANT);
        assert_eq!(buf[2], effect_op::START);
        assert_eq!(buf[3], 5, "loop count");
        Ok(())
    }

    #[test]
    fn test_effect_operation_stop() -> Result<(), Box<dyn std::error::Error>> {
        let buf = encode_effect_operation(block_id::DAMPER, false, 0);
        assert_eq!(buf[0], report_type::EFFECT_OPERATION);
        assert_eq!(buf[1], block_id::DAMPER);
        assert_eq!(buf[2], effect_op::STOP);
        assert_eq!(buf[3], 0);
        Ok(())
    }

    #[test]
    fn test_gain_full() -> Result<(), Box<dyn std::error::Error>> {
        let buf = encode_gain(0xFFFF);
        assert_eq!(buf[0], report_type::SET_GAIN);
        assert_eq!(buf[1], 0xFF, "gain >> 8");
        Ok(())
    }

    #[test]
    fn test_gain_half() -> Result<(), Box<dyn std::error::Error>> {
        let buf = encode_gain(0x8000);
        assert_eq!(buf[0], report_type::SET_GAIN);
        assert_eq!(buf[1], 0x80);
        Ok(())
    }

    #[test]
    fn test_all_reports_64_bytes() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(encode_constant(100).len(), 64);
        assert_eq!(encode_set_effect(block_id::CONSTANT, 100).len(), 64);
        assert_eq!(
            encode_condition(
                block_id::SPRING,
                &ConditionParams {
                    center: 0,
                    right_coeff: 0,
                    left_coeff: 0,
                    right_saturation: 0,
                    left_saturation: 0,
                    deadband: 0
                }
            )
            .len(),
            64
        );
        assert_eq!(encode_periodic(block_id::SINE, 0, 0, 0, 0).len(), 64);
        assert_eq!(encode_effect_operation(block_id::CONSTANT, true, 1).len(), 64);
        assert_eq!(encode_gain(0).len(), 64);
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
        fn prop_rescale_monotone(a in i16::MIN..=i16::MAX, b in i16::MIN..=i16::MAX) {
            let ra = rescale_signed_to_10k(a);
            let rb = rescale_signed_to_10k(b);
            if a < b {
                prop_assert!(ra <= rb, "monotone: f({})={} > f({})={}", a, ra, b, rb);
            }
        }

        #[test]
        fn prop_rescale_bounded(v in i16::MIN..=i16::MAX) {
            let r = rescale_signed_to_10k(v);
            prop_assert!((-10000..=10000).contains(&r), "out of range: {r}");
        }

        #[test]
        fn prop_constant_report_type(level in i16::MIN..=i16::MAX) {
            let buf = encode_constant(level);
            prop_assert_eq!(buf[0], report_type::SET_CONSTANT);
            prop_assert_eq!(buf[1], block_id::CONSTANT);
        }

        #[test]
        fn prop_condition_report_type(
            bid in prop::sample::select(vec![
                block_id::SPRING, block_id::DAMPER,
                block_id::FRICTION, block_id::INERTIA,
            ]),
            center in i16::MIN..=i16::MAX,
            right_coeff in i16::MIN..=i16::MAX,
        ) {
            let params = ConditionParams {
                center,
                right_coeff,
                left_coeff: 0,
                right_saturation: 0x8000,
                left_saturation: 0x8000,
                deadband: 0,
            };
            let buf = encode_condition(bid, &params);
            prop_assert_eq!(buf[0], report_type::SET_CONDITION);
            prop_assert_eq!(buf[1], bid);
        }

        #[test]
        fn prop_effect_op_start_stop(
            bid in 0u8..=15,
            start in proptest::bool::ANY,
            loops in 0u8..=255,
        ) {
            let buf = encode_effect_operation(bid, start, loops);
            prop_assert_eq!(buf[0], report_type::EFFECT_OPERATION);
            prop_assert_eq!(buf[1], bid);
            if start {
                prop_assert_eq!(buf[2], effect_op::START);
                prop_assert_eq!(buf[3], loops);
            } else {
                prop_assert_eq!(buf[2], effect_op::STOP);
                prop_assert_eq!(buf[3], 0);
            }
        }

        #[test]
        fn prop_gain_high_byte(gain in 0u16..=u16::MAX) {
            let buf = encode_gain(gain);
            prop_assert_eq!(buf[1], (gain >> 8) as u8);
        }
    }
}

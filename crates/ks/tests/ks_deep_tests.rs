//! Deep tests for kernel-space representation types, parsing, and layout stability.

use racing_wheel_ks::{
    KS_BUTTON_BYTES, KS_ENCODER_COUNT, KsAxisSource, KsBitSource, KsByteSource, KsClutchMode,
    KsJoystickMode, KsReportMap, KsReportSnapshot, KsRotaryMode,
};

type R = Result<(), Box<dyn std::error::Error>>;

// ═══════════════════════════════════════════════════════════════════════
// §1  KS struct constants and sizes
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn button_bytes_constant_is_sixteen() {
    assert_eq!(KS_BUTTON_BYTES, 16);
}

#[test]
fn encoder_count_constant_is_eight() {
    assert_eq!(KS_ENCODER_COUNT, 8);
}

// ═══════════════════════════════════════════════════════════════════════
// §2  KsAxisSource — boundary values
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn axis_source_u16_zero() -> R {
    let src = KsAxisSource::new(0, false);
    let data = 0u16.to_le_bytes();
    assert_eq!(src.parse_u16(&data).ok_or("u16 zero")?, 0);
    Ok(())
}

#[test]
fn axis_source_u16_max() -> R {
    let src = KsAxisSource::new(0, false);
    let data = u16::MAX.to_le_bytes();
    assert_eq!(src.parse_u16(&data).ok_or("u16 max")?, u16::MAX);
    Ok(())
}

#[test]
fn axis_source_i16_min() -> R {
    let src = KsAxisSource::new(0, true);
    let data = i16::MIN.to_le_bytes();
    assert_eq!(src.parse_i16(&data).ok_or("i16 min")?, i16::MIN);
    Ok(())
}

#[test]
fn axis_source_i16_max() -> R {
    let src = KsAxisSource::new(0, true);
    let data = i16::MAX.to_le_bytes();
    assert_eq!(src.parse_i16(&data).ok_or("i16 max")?, i16::MAX);
    Ok(())
}

#[test]
fn axis_source_i16_minus_one() -> R {
    let src = KsAxisSource::new(0, true);
    let data = (-1i16).to_le_bytes();
    assert_eq!(src.parse_i16(&data).ok_or("i16 -1")?, -1);
    Ok(())
}

#[test]
fn axis_source_offset_at_end_of_buffer() -> R {
    let src = KsAxisSource::new(3, false);
    let data = [0x00, 0x00, 0x00, 0x34, 0x12]; // offset 3, data at bytes 3–4
    assert_eq!(src.parse_u16(&data).ok_or("end-of-buf")?, 0x1234);
    Ok(())
}

#[test]
fn axis_source_offset_exactly_too_short() {
    let src = KsAxisSource::new(4, false);
    let data = [0x00, 0x00, 0x00, 0x00, 0xFF]; // need 2 bytes at offset 4, only 1 available
    assert!(src.parse_u16(&data).is_none());
}

#[test]
fn axis_source_offset_zero_length_data() {
    let src = KsAxisSource::new(0, false);
    let data: [u8; 0] = [];
    assert!(src.parse_u16(&data).is_none());
    assert!(src.parse_i16(&data).is_none());
}

#[test]
fn axis_source_large_offset_doesnt_panic() {
    let src = KsAxisSource::new(usize::MAX, false);
    let data = [0x00, 0x01];
    assert!(src.parse_u16(&data).is_none());
    assert!(src.parse_i16(&data).is_none());
}

#[test]
fn axis_source_new_const_fields() {
    let src = KsAxisSource::new(42, true);
    assert_eq!(src.offset, 42);
    assert!(src.signed);
}

// ═══════════════════════════════════════════════════════════════════════
// §3  KsBitSource — boundary values
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn bit_source_all_masks() -> R {
    for bit in 0u8..8 {
        let mask = 1u8 << bit;
        let src = KsBitSource::new(0, mask);
        assert!(src.parse(&[mask]).ok_or("bit set")?);
        assert!(!src.parse(&[0x00]).ok_or("bit clear")?);
    }
    Ok(())
}

#[test]
fn bit_source_full_byte_mask() -> R {
    let src = KsBitSource::new(0, 0xFF);
    // Any non-zero byte should be active.
    assert!(src.parse(&[0x01]).ok_or("any bit")?);
    assert!(src.parse(&[0xFF]).ok_or("all bits")?);
    assert!(!src.parse(&[0x00]).ok_or("no bits")?);
    Ok(())
}

#[test]
fn bit_source_inverted_with_full_mask() -> R {
    let src = KsBitSource::inverted(0, 0xFF);
    assert!(!src.parse(&[0xFF]).ok_or("inverted all set")?);
    assert!(src.parse(&[0x00]).ok_or("inverted all clear")?);
    Ok(())
}

#[test]
fn bit_source_with_invert_alias() {
    let a = KsBitSource::with_invert(3, 0x40);
    let b = KsBitSource::inverted(3, 0x40);
    assert_eq!(a, b);
}

#[test]
fn bit_source_oob_returns_none() {
    let src = KsBitSource::new(10, 0x01);
    assert!(src.parse(&[0xFF]).is_none());
}

#[test]
fn bit_source_exact_offset() -> R {
    let src = KsBitSource::new(2, 0x80);
    let data = [0x00, 0x00, 0x80];
    assert!(src.parse(&data).ok_or("exact offset")?);
    Ok(())
}

#[test]
fn bit_source_large_offset_doesnt_panic() {
    let src = KsBitSource::new(usize::MAX, 0x01);
    assert!(src.parse(&[0xFF]).is_none());
}

// ═══════════════════════════════════════════════════════════════════════
// §4  KsByteSource — boundary values
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn byte_source_first_byte() -> R {
    let src = KsByteSource::new(0);
    assert_eq!(src.parse(&[0xAA]).ok_or("first byte")?, 0xAA);
    Ok(())
}

#[test]
fn byte_source_last_byte() -> R {
    let src = KsByteSource::new(3);
    assert_eq!(src.parse(&[0, 0, 0, 0xBB]).ok_or("last byte")?, 0xBB);
    Ok(())
}

#[test]
fn byte_source_oob() {
    let src = KsByteSource::new(5);
    assert!(src.parse(&[0x00]).is_none());
}

#[test]
fn byte_source_empty_data() {
    let src = KsByteSource::new(0);
    assert!(src.parse(&[]).is_none());
}

#[test]
fn byte_source_large_offset_doesnt_panic() {
    let src = KsByteSource::new(usize::MAX);
    assert!(src.parse(&[0xFF]).is_none());
}

// ═══════════════════════════════════════════════════════════════════════
// §5  KsReportSnapshot — defaults and constructors
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn snapshot_default_all_zeroed() {
    let s = KsReportSnapshot::default();
    assert_eq!(s.tick, 0);
    assert_eq!(s.buttons, [0u8; KS_BUTTON_BYTES]);
    assert_eq!(s.hat, 0);
    assert_eq!(s.encoders, [0i16; KS_ENCODER_COUNT]);
    assert_eq!(s.clutch_combined, None);
    assert_eq!(s.clutch_left, None);
    assert_eq!(s.clutch_right, None);
    assert_eq!(s.clutch_left_button, None);
    assert_eq!(s.clutch_right_button, None);
    assert_eq!(s.clutch_mode, KsClutchMode::Unknown);
    assert_eq!(s.rotary_mode, KsRotaryMode::Unknown);
    assert_eq!(s.joystick_mode, KsJoystickMode::Unknown);
}

#[test]
fn snapshot_from_common_controls_populates_correctly() {
    let buttons = [0xAB; KS_BUTTON_BYTES];
    let s = KsReportSnapshot::from_common_controls(999, buttons, 0x07);
    assert_eq!(s.tick, 999);
    assert_eq!(s.buttons, buttons);
    assert_eq!(s.hat, 0x07);
    assert_eq!(s.clutch_mode, KsClutchMode::Unknown);
    assert!(s.clutch_combined.is_none());
}

#[test]
fn snapshot_from_common_controls_max_tick() {
    let s = KsReportSnapshot::from_common_controls(u32::MAX, [0; KS_BUTTON_BYTES], 0);
    assert_eq!(s.tick, u32::MAX);
}

// ═══════════════════════════════════════════════════════════════════════
// §6  KsReportSnapshot — both_clutches_pressed
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn both_clutches_combined_at_threshold() {
    let s = KsReportSnapshot {
        clutch_mode: KsClutchMode::CombinedAxis,
        clutch_combined: Some(30_000),
        ..Default::default()
    };
    assert_eq!(s.both_clutches_pressed(30_000), Some(true));
    assert_eq!(s.both_clutches_pressed(30_001), Some(false));
}

#[test]
fn both_clutches_combined_zero_threshold() {
    let s = KsReportSnapshot {
        clutch_mode: KsClutchMode::CombinedAxis,
        clutch_combined: Some(0),
        ..Default::default()
    };
    assert_eq!(s.both_clutches_pressed(0), Some(true));
}

#[test]
fn both_clutches_combined_max_value() {
    let s = KsReportSnapshot {
        clutch_mode: KsClutchMode::CombinedAxis,
        clutch_combined: Some(u16::MAX),
        ..Default::default()
    };
    assert_eq!(s.both_clutches_pressed(u16::MAX), Some(true));
}

#[test]
fn both_clutches_combined_none_data() {
    let s = KsReportSnapshot {
        clutch_mode: KsClutchMode::CombinedAxis,
        clutch_combined: None,
        ..Default::default()
    };
    assert_eq!(s.both_clutches_pressed(0), None);
}

#[test]
fn both_clutches_independent_both_above() {
    let s = KsReportSnapshot {
        clutch_mode: KsClutchMode::IndependentAxis,
        clutch_left: Some(50_000),
        clutch_right: Some(50_000),
        ..Default::default()
    };
    assert_eq!(s.both_clutches_pressed(40_000), Some(true));
}

#[test]
fn both_clutches_independent_one_below() {
    let s = KsReportSnapshot {
        clutch_mode: KsClutchMode::IndependentAxis,
        clutch_left: Some(50_000),
        clutch_right: Some(10_000),
        ..Default::default()
    };
    assert_eq!(s.both_clutches_pressed(40_000), Some(false));
}

#[test]
fn both_clutches_independent_left_missing() {
    let s = KsReportSnapshot {
        clutch_mode: KsClutchMode::IndependentAxis,
        clutch_left: None,
        clutch_right: Some(50_000),
        ..Default::default()
    };
    assert_eq!(s.both_clutches_pressed(0), None);
}

#[test]
fn both_clutches_button_both_true() {
    let s = KsReportSnapshot {
        clutch_mode: KsClutchMode::Button,
        clutch_left_button: Some(true),
        clutch_right_button: Some(true),
        ..Default::default()
    };
    assert_eq!(s.both_clutches_pressed(0), Some(true));
}

#[test]
fn both_clutches_button_left_false() {
    let s = KsReportSnapshot {
        clutch_mode: KsClutchMode::Button,
        clutch_left_button: Some(false),
        clutch_right_button: Some(true),
        ..Default::default()
    };
    assert_eq!(s.both_clutches_pressed(0), Some(false));
}

#[test]
fn both_clutches_button_right_missing() {
    let s = KsReportSnapshot {
        clutch_mode: KsClutchMode::Button,
        clutch_left_button: Some(true),
        clutch_right_button: None,
        ..Default::default()
    };
    assert_eq!(s.both_clutches_pressed(0), None);
}

#[test]
fn both_clutches_unknown_always_none() {
    let s = KsReportSnapshot::default();
    assert_eq!(s.both_clutches_pressed(0), None);
    assert_eq!(s.both_clutches_pressed(u16::MAX), None);
}

// ═══════════════════════════════════════════════════════════════════════
// §7  KsReportMap — empty map
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn empty_map_all_fields_none() {
    let map = KsReportMap::empty();
    assert_eq!(map.report_id, None);
    assert_eq!(map.buttons_offset, None);
    assert_eq!(map.hat_offset, None);
    assert!(map.encoders.iter().all(|e| e.is_none()));
    assert!(map.clutch_left_axis.is_none());
    assert!(map.clutch_right_axis.is_none());
    assert!(map.clutch_combined_axis.is_none());
    assert!(map.clutch_left_button.is_none());
    assert!(map.clutch_right_button.is_none());
    assert!(map.left_rotary_axis.is_none());
    assert!(map.right_rotary_axis.is_none());
    assert!(map.joystick_hat.is_none());
    assert_eq!(map.clutch_mode_hint, KsClutchMode::Unknown);
    assert_eq!(map.rotary_mode_hint, KsRotaryMode::Unknown);
    assert_eq!(map.joystick_mode_hint, KsJoystickMode::Unknown);
}

// ═══════════════════════════════════════════════════════════════════════
// §8  KsReportMap — parse
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn parse_empty_map_any_report() -> R {
    let map = KsReportMap::empty();
    let snapshot = map.parse(42, &[0xAA]).ok_or("empty map should accept")?;
    assert_eq!(snapshot.tick, 42);
    assert_eq!(snapshot.buttons, [0u8; KS_BUTTON_BYTES]);
    Ok(())
}

#[test]
fn parse_rejects_wrong_report_id() {
    let mut map = KsReportMap::empty();
    map.report_id = Some(0x01);
    assert!(map.parse(0, &[0x02]).is_none());
}

#[test]
fn parse_rejects_empty_report_with_id() {
    let mut map = KsReportMap::empty();
    map.report_id = Some(0x01);
    assert!(map.parse(0, &[]).is_none());
}

#[test]
fn parse_accepts_correct_report_id() -> R {
    let mut map = KsReportMap::empty();
    map.report_id = Some(0x01);
    let s = map.parse(0, &[0x01]).ok_or("correct id")?;
    assert_eq!(s.tick, 0);
    Ok(())
}

#[test]
fn parse_buttons_full() -> R {
    let mut map = KsReportMap::empty();
    map.buttons_offset = Some(0);
    let mut data = [0u8; KS_BUTTON_BYTES];
    for (i, b) in data.iter_mut().enumerate() {
        *b = (i as u8).wrapping_add(0x10);
    }
    let s = map.parse(0, &data).ok_or("full buttons")?;
    assert_eq!(s.buttons, data);
    Ok(())
}

#[test]
fn parse_buttons_partial() -> R {
    let mut map = KsReportMap::empty();
    map.buttons_offset = Some(0);
    let data = [0xDE, 0xAD];
    let s = map.parse(0, &data).ok_or("partial buttons")?;
    assert_eq!(s.buttons[0], 0xDE);
    assert_eq!(s.buttons[1], 0xAD);
    assert_eq!(s.buttons[2..], [0u8; KS_BUTTON_BYTES - 2]);
    Ok(())
}

#[test]
fn parse_buttons_with_offset() -> R {
    let mut map = KsReportMap::empty();
    map.buttons_offset = Some(2);
    let mut data = [0u8; KS_BUTTON_BYTES + 2];
    data[2] = 0xBE;
    data[3] = 0xEF;
    let s = map.parse(0, &data).ok_or("buttons with offset")?;
    assert_eq!(s.buttons[0], 0xBE);
    assert_eq!(s.buttons[1], 0xEF);
    Ok(())
}

#[test]
fn parse_hat_from_joystick_hat_source() -> R {
    let mut map = KsReportMap::empty();
    map.joystick_hat = Some(KsByteSource::new(0));
    let s = map.parse(0, &[0x05]).ok_or("hat from joystick")?;
    assert_eq!(s.hat, 0x05);
    Ok(())
}

#[test]
fn parse_hat_from_offset_fallback() -> R {
    let mut map = KsReportMap::empty();
    map.hat_offset = Some(0);
    let s = map.parse(0, &[0x07]).ok_or("hat from offset")?;
    assert_eq!(s.hat, 0x07);
    Ok(())
}

#[test]
fn parse_hat_joystick_overrides_offset() -> R {
    let mut map = KsReportMap::empty();
    map.joystick_hat = Some(KsByteSource::new(0));
    map.hat_offset = Some(1);
    let s = map.parse(0, &[0xAA, 0xBB]).ok_or("hat override")?;
    // joystick_hat takes priority over hat_offset.
    assert_eq!(s.hat, 0xAA);
    Ok(())
}

#[test]
fn parse_hat_both_oob_defaults_to_zero() -> R {
    let mut map = KsReportMap::empty();
    map.hat_offset = Some(100);
    let s = map.parse(0, &[0xFF]).ok_or("hat oob")?;
    assert_eq!(s.hat, 0);
    Ok(())
}

#[test]
fn parse_encoders_all_slots() -> R {
    let mut map = KsReportMap::empty();
    for i in 0..KS_ENCODER_COUNT {
        map.encoders[i] = Some(KsAxisSource::new(i * 2, true));
    }
    let mut data = vec![0u8; KS_ENCODER_COUNT * 2];
    for i in 0..KS_ENCODER_COUNT {
        let val = (i as i16 + 1) * 100;
        data[i * 2..i * 2 + 2].copy_from_slice(&val.to_le_bytes());
    }
    let s = map.parse(0, &data).ok_or("all encoders")?;
    for i in 0..KS_ENCODER_COUNT {
        assert_eq!(s.encoders[i], (i as i16 + 1) * 100);
    }
    Ok(())
}

#[test]
fn parse_encoder_oob_defaults_zero() -> R {
    let mut map = KsReportMap::empty();
    map.encoders[0] = Some(KsAxisSource::new(100, true));
    let s = map.parse(0, &[0xFF]).ok_or("encoder oob")?;
    assert_eq!(s.encoders[0], 0);
    Ok(())
}

#[test]
fn parse_encoder_unsigned_reinterpret_as_i16() -> R {
    let mut map = KsReportMap::empty();
    map.encoders[0] = Some(KsAxisSource::new(0, false));
    // 0xFFFF as u16 = 65535; reinterpreted as i16 = -1
    let data = 0xFFFFu16.to_le_bytes();
    let s = map.parse(0, &data).ok_or("unsigned encoder")?;
    assert_eq!(s.encoders[0], -1);
    Ok(())
}

#[test]
fn parse_rotary_axes_override_encoder_slots() -> R {
    let mut map = KsReportMap::empty();
    map.encoders[0] = Some(KsAxisSource::new(0, true));
    map.encoders[1] = Some(KsAxisSource::new(2, true));
    map.left_rotary_axis = Some(KsAxisSource::new(4, true));
    map.right_rotary_axis = Some(KsAxisSource::new(6, true));

    let mut data = [0u8; 8];
    data[0..2].copy_from_slice(&10i16.to_le_bytes());
    data[2..4].copy_from_slice(&20i16.to_le_bytes());
    data[4..6].copy_from_slice(&30i16.to_le_bytes());
    data[6..8].copy_from_slice(&40i16.to_le_bytes());

    let s = map.parse(0, &data).ok_or("rotary override")?;
    assert_eq!(s.encoders[0], 30);
    assert_eq!(s.encoders[1], 40);
    Ok(())
}

#[test]
fn parse_clutch_combined() -> R {
    let mut map = KsReportMap::empty();
    map.clutch_mode_hint = KsClutchMode::CombinedAxis;
    map.clutch_combined_axis = Some(KsAxisSource::new(0, false));
    let data = 1234u16.to_le_bytes();
    let s = map.parse(0, &data).ok_or("clutch combined")?;
    assert_eq!(s.clutch_combined, Some(1234));
    assert_eq!(s.clutch_mode, KsClutchMode::CombinedAxis);
    Ok(())
}

#[test]
fn parse_clutch_independent() -> R {
    let mut map = KsReportMap::empty();
    map.clutch_mode_hint = KsClutchMode::IndependentAxis;
    map.clutch_left_axis = Some(KsAxisSource::new(0, false));
    map.clutch_right_axis = Some(KsAxisSource::new(2, false));
    let mut data = [0u8; 4];
    data[0..2].copy_from_slice(&1000u16.to_le_bytes());
    data[2..4].copy_from_slice(&2000u16.to_le_bytes());
    let s = map.parse(0, &data).ok_or("clutch independent")?;
    assert_eq!(s.clutch_left, Some(1000));
    assert_eq!(s.clutch_right, Some(2000));
    Ok(())
}

#[test]
fn parse_clutch_buttons() -> R {
    let mut map = KsReportMap::empty();
    map.clutch_mode_hint = KsClutchMode::Button;
    map.clutch_left_button = Some(KsBitSource::new(0, 0x01));
    map.clutch_right_button = Some(KsBitSource::new(0, 0x02));

    // Both pressed.
    let s = map.parse(0, &[0x03]).ok_or("clutch buttons both")?;
    assert_eq!(s.clutch_left_button, Some(true));
    assert_eq!(s.clutch_right_button, Some(true));

    // Only left pressed.
    let s = map.parse(0, &[0x01]).ok_or("clutch buttons left")?;
    assert_eq!(s.clutch_left_button, Some(true));
    assert_eq!(s.clutch_right_button, Some(false));
    Ok(())
}

#[test]
fn parse_clutch_axis_oob_returns_none() -> R {
    let mut map = KsReportMap::empty();
    map.clutch_combined_axis = Some(KsAxisSource::new(100, false));
    let s = map.parse(0, &[0xFF]).ok_or("clutch oob")?;
    assert_eq!(s.clutch_combined, None);
    Ok(())
}

#[test]
fn parse_mode_hints_propagate() -> R {
    let mut map = KsReportMap::empty();
    map.clutch_mode_hint = KsClutchMode::IndependentAxis;
    map.rotary_mode_hint = KsRotaryMode::Knob;
    map.joystick_mode_hint = KsJoystickMode::DPad;
    let s = map.parse(0, &[0x00]).ok_or("mode hints")?;
    assert_eq!(s.clutch_mode, KsClutchMode::IndependentAxis);
    assert_eq!(s.rotary_mode, KsRotaryMode::Knob);
    assert_eq!(s.joystick_mode, KsJoystickMode::DPad);
    Ok(())
}

#[test]
fn parse_fully_populated_report() -> R {
    let mut map = KsReportMap::empty();
    map.report_id = Some(0x01);
    map.buttons_offset = Some(1);
    map.hat_offset = Some(17);
    map.clutch_mode_hint = KsClutchMode::IndependentAxis;
    map.clutch_left_axis = Some(KsAxisSource::new(18, false));
    map.clutch_right_axis = Some(KsAxisSource::new(20, false));
    map.clutch_left_button = Some(KsBitSource::new(22, 0x01));
    map.clutch_right_button = Some(KsBitSource::new(22, 0x02));
    map.encoders[0] = Some(KsAxisSource::new(23, true));
    map.encoders[1] = Some(KsAxisSource::new(25, true));
    map.joystick_hat = Some(KsByteSource::new(27));
    map.joystick_mode_hint = KsJoystickMode::DPad;
    map.rotary_mode_hint = KsRotaryMode::Knob;

    let mut report = vec![0u8; 28];
    report[0] = 0x01;
    for (i, byte) in report.iter_mut().enumerate().take(17).skip(1) {
        *byte = i as u8;
    }
    report[17] = 0x05;
    report[18..20].copy_from_slice(&1000u16.to_le_bytes());
    report[20..22].copy_from_slice(&2000u16.to_le_bytes());
    report[22] = 0x03;
    report[23..25].copy_from_slice(&(-100i16).to_le_bytes());
    report[25..27].copy_from_slice(&200i16.to_le_bytes());
    report[27] = 0x07;

    let s = map.parse(99, &report).ok_or("full report")?;
    assert_eq!(s.tick, 99);
    assert_eq!(s.buttons[0], 1);
    assert_eq!(s.buttons[15], 16);
    assert_eq!(s.hat, 0x07); // joystick_hat overrides hat_offset
    assert_eq!(s.clutch_left, Some(1000));
    assert_eq!(s.clutch_right, Some(2000));
    assert_eq!(s.clutch_left_button, Some(true));
    assert_eq!(s.clutch_right_button, Some(true));
    assert_eq!(s.encoders[0], -100);
    assert_eq!(s.encoders[1], 200);
    assert_eq!(s.clutch_mode, KsClutchMode::IndependentAxis);
    assert_eq!(s.rotary_mode, KsRotaryMode::Knob);
    assert_eq!(s.joystick_mode, KsJoystickMode::DPad);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// §9  KS serde (feature = "serde")
// ═══════════════════════════════════════════════════════════════════════

#[cfg(feature = "serde")]
mod serde_tests {
    use super::*;

    #[test]
    fn clutch_mode_all_variants_round_trip() -> R {
        for mode in [
            KsClutchMode::Unknown,
            KsClutchMode::CombinedAxis,
            KsClutchMode::IndependentAxis,
            KsClutchMode::Button,
        ] {
            let json = serde_json::to_string(&mode)?;
            let rt: KsClutchMode = serde_json::from_str(&json)?;
            assert_eq!(mode, rt);
        }
        Ok(())
    }

    #[test]
    fn rotary_mode_all_variants_round_trip() -> R {
        for mode in [
            KsRotaryMode::Unknown,
            KsRotaryMode::Button,
            KsRotaryMode::Knob,
        ] {
            let json = serde_json::to_string(&mode)?;
            let rt: KsRotaryMode = serde_json::from_str(&json)?;
            assert_eq!(mode, rt);
        }
        Ok(())
    }

    #[test]
    fn joystick_mode_all_variants_round_trip() -> R {
        for mode in [
            KsJoystickMode::Unknown,
            KsJoystickMode::Button,
            KsJoystickMode::DPad,
        ] {
            let json = serde_json::to_string(&mode)?;
            let rt: KsJoystickMode = serde_json::from_str(&json)?;
            assert_eq!(mode, rt);
        }
        Ok(())
    }

    #[test]
    fn axis_source_serde_round_trip() -> R {
        for (offset, signed) in [(0, false), (usize::MAX / 2, true), (42, true)] {
            let src = KsAxisSource::new(offset, signed);
            let json = serde_json::to_string(&src)?;
            let rt: KsAxisSource = serde_json::from_str(&json)?;
            assert_eq!(src, rt);
        }
        Ok(())
    }

    #[test]
    fn bit_source_serde_round_trip() -> R {
        let sources = [
            KsBitSource::new(0, 0x01),
            KsBitSource::new(3, 0x10),
            KsBitSource::inverted(7, 0x80),
        ];
        for src in sources {
            let json = serde_json::to_string(&src)?;
            let rt: KsBitSource = serde_json::from_str(&json)?;
            assert_eq!(src, rt);
        }
        Ok(())
    }

    #[test]
    fn byte_source_serde_round_trip() -> R {
        let src = KsByteSource::new(15);
        let json = serde_json::to_string(&src)?;
        let rt: KsByteSource = serde_json::from_str(&json)?;
        assert_eq!(src, rt);
        Ok(())
    }

    #[test]
    fn report_map_serde_round_trip() -> R {
        let mut map = KsReportMap::empty();
        map.report_id = Some(0x01);
        map.buttons_offset = Some(2);
        map.hat_offset = Some(18);
        map.clutch_mode_hint = KsClutchMode::CombinedAxis;
        map.clutch_combined_axis = Some(KsAxisSource::new(19, false));
        map.encoders[0] = Some(KsAxisSource::new(21, true));
        map.left_rotary_axis = Some(KsAxisSource::new(21, true));
        map.rotary_mode_hint = KsRotaryMode::Knob;
        map.joystick_mode_hint = KsJoystickMode::DPad;
        map.joystick_hat = Some(KsByteSource::new(23));

        let json = serde_json::to_string(&map)?;
        let rt: KsReportMap = serde_json::from_str(&json)?;
        assert_eq!(map, rt);
        Ok(())
    }

    #[test]
    fn snapshot_serde_round_trip() -> R {
        let snapshot = KsReportSnapshot {
            tick: u32::MAX,
            buttons: [0xFF; KS_BUTTON_BYTES],
            hat: 0x0F,
            encoders: [i16::MIN, i16::MAX, 0, 1, -1, 100, -100, 32767],
            clutch_combined: Some(u16::MAX),
            clutch_left: Some(0),
            clutch_right: Some(32768),
            clutch_left_button: Some(true),
            clutch_right_button: Some(false),
            clutch_mode: KsClutchMode::IndependentAxis,
            rotary_mode: KsRotaryMode::Button,
            joystick_mode: KsJoystickMode::DPad,
        };
        let json = serde_json::to_string(&snapshot)?;
        let rt: KsReportSnapshot = serde_json::from_str(&json)?;
        assert_eq!(snapshot, rt);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════
// §10  Cross-platform layout stability
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn button_array_length_matches_constant() {
    let s = KsReportSnapshot::default();
    assert_eq!(s.buttons.len(), KS_BUTTON_BYTES);
}

#[test]
fn encoder_array_length_matches_constant() {
    let s = KsReportSnapshot::default();
    assert_eq!(s.encoders.len(), KS_ENCODER_COUNT);
}

#[test]
fn encoder_array_in_report_map_matches_constant() {
    let m = KsReportMap::empty();
    assert_eq!(m.encoders.len(), KS_ENCODER_COUNT);
}

#[test]
fn axis_source_struct_size_is_stable() {
    // KsAxisSource contains offset (usize) + signed (bool)
    // Minimum size: usize + 1, but may be padded.
    let size = std::mem::size_of::<KsAxisSource>();
    assert!(size > std::mem::size_of::<usize>());
    assert!(size <= std::mem::size_of::<usize>() * 2);
}

#[test]
fn bit_source_struct_size_is_stable() {
    let size = std::mem::size_of::<KsBitSource>();
    // Contains offset (usize) + mask (u8) + invert (bool)
    assert!(size >= std::mem::size_of::<usize>() + 2);
    assert!(size <= std::mem::size_of::<usize>() * 3);
}

#[test]
fn byte_source_struct_size_is_stable() {
    let size = std::mem::size_of::<KsByteSource>();
    assert_eq!(size, std::mem::size_of::<usize>());
}

#[test]
fn snapshot_copy_semantics() {
    let a = KsReportSnapshot {
        tick: 42,
        buttons: [0xAA; KS_BUTTON_BYTES],
        hat: 0x05,
        ..Default::default()
    };
    let b = a; // Copy
    assert_eq!(a, b);
}

#[test]
fn report_map_copy_semantics() {
    let mut a = KsReportMap::empty();
    a.report_id = Some(0x01);
    let b = a; // Copy
    assert_eq!(a, b);
}

// ═══════════════════════════════════════════════════════════════════════
// §11  Proptest
// ═══════════════════════════════════════════════════════════════════════

use proptest::prelude::*;

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(256))]

    #[test]
    fn prop_u16_all_values_parse(val: u16) {
        let src = KsAxisSource::new(0, false);
        let data = val.to_le_bytes();
        prop_assert_eq!(src.parse_u16(&data), Some(val));
    }

    #[test]
    fn prop_i16_all_values_parse(val: i16) {
        let src = KsAxisSource::new(0, true);
        let data = val.to_le_bytes();
        prop_assert_eq!(src.parse_i16(&data), Some(val));
    }

    #[test]
    fn prop_bit_source_consistency(byte: u8, bit in 0u8..8u8) {
        let mask = 1u8 << bit;
        let normal = KsBitSource::new(0, mask);
        let inv = KsBitSource::inverted(0, mask);
        let n = normal.parse(&[byte]);
        let i = inv.parse(&[byte]);
        prop_assert_eq!(n.map(|v| !v), i);
    }

    #[test]
    fn prop_byte_source_all_positions(data in proptest::collection::vec(any::<u8>(), 1..=32)) {
        for (i, &expected) in data.iter().enumerate() {
            let src = KsByteSource::new(i);
            prop_assert_eq!(src.parse(&data), Some(expected));
        }
    }

    #[test]
    fn prop_empty_map_always_parses(
        data in proptest::collection::vec(any::<u8>(), 1..=64),
        tick: u32,
    ) {
        let map = KsReportMap::empty();
        let result = map.parse(tick, &data);
        prop_assert!(result.is_some());
        if let Some(s) = result {
            prop_assert_eq!(s.tick, tick);
        }
    }

    #[test]
    fn prop_snapshot_both_clutches_combined_threshold(value: u16, threshold: u16) {
        let s = KsReportSnapshot {
            clutch_mode: KsClutchMode::CombinedAxis,
            clutch_combined: Some(value),
            ..Default::default()
        };
        let result = s.both_clutches_pressed(threshold);
        prop_assert_eq!(result, Some(value >= threshold));
    }

    #[test]
    fn prop_encoder_round_trip(values in proptest::collection::vec(any::<i16>(), KS_ENCODER_COUNT..=KS_ENCODER_COUNT)) {
        let mut map = KsReportMap::empty();
        for i in 0..KS_ENCODER_COUNT {
            map.encoders[i] = Some(KsAxisSource::new(i * 2, true));
        }
        let mut data = vec![0u8; KS_ENCODER_COUNT * 2];
        for (i, &val) in values.iter().enumerate() {
            data[i * 2..i * 2 + 2].copy_from_slice(&val.to_le_bytes());
        }
        if let Some(s) = map.parse(0, &data) {
            for (i, &val) in values.iter().enumerate().take(KS_ENCODER_COUNT) {
                prop_assert_eq!(s.encoders[i], val);
            }
        }
    }
}

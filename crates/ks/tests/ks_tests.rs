//! Comprehensive integration tests for the KS crate covering key/switch
//! definitions, rotary encoder parsing, KS map compilation, report parsing,
//! snapshot semantics, edge cases, and property-based tests.

use racing_wheel_ks::{
    KS_BUTTON_BYTES, KS_ENCODER_COUNT, KsAxisSource, KsBitSource, KsByteSource, KsClutchMode,
    KsJoystickMode, KsReportMap, KsReportSnapshot, KsRotaryMode,
};

type R = Result<(), Box<dyn std::error::Error>>;

// ── KsAxisSource parsing ─────────────────────────────────────────────────

#[test]
fn axis_source_u16_basic() -> R {
    let src = KsAxisSource::new(0, false);
    let data = 0x1234u16.to_le_bytes();
    let val = src.parse_u16(&data).ok_or("expected u16 parse")?;
    assert_eq!(val, 0x1234);
    Ok(())
}

#[test]
fn axis_source_u16_at_offset() -> R {
    let src = KsAxisSource::new(2, false);
    let data = [0x00, 0x00, 0xAB, 0xCD];
    let val = src.parse_u16(&data).ok_or("expected u16 at offset")?;
    assert_eq!(val, u16::from_le_bytes([0xAB, 0xCD]));
    Ok(())
}

#[test]
fn axis_source_i16_negative() -> R {
    let src = KsAxisSource::new(0, true);
    let data = (-1000i16).to_le_bytes();
    let val = src.parse_i16(&data).ok_or("expected i16 parse")?;
    assert_eq!(val, -1000);
    Ok(())
}

#[test]
fn axis_source_i16_min_value() -> R {
    let src = KsAxisSource::new(0, true);
    let data = i16::MIN.to_le_bytes();
    let val = src.parse_i16(&data).ok_or("expected i16 min")?;
    assert_eq!(val, i16::MIN);
    Ok(())
}

#[test]
fn axis_source_i16_max_value() -> R {
    let src = KsAxisSource::new(0, true);
    let data = i16::MAX.to_le_bytes();
    let val = src.parse_i16(&data).ok_or("expected i16 max")?;
    assert_eq!(val, i16::MAX);
    Ok(())
}

#[test]
fn axis_source_returns_none_when_data_too_short() {
    let src = KsAxisSource::new(5, false);
    assert!(src.parse_u16(&[0x00, 0x01]).is_none());
    assert!(src.parse_i16(&[0x00, 0x01]).is_none());
}

#[test]
fn axis_source_returns_none_for_empty_data() {
    let src = KsAxisSource::new(0, false);
    assert!(src.parse_u16(&[]).is_none());
    assert!(src.parse_i16(&[]).is_none());
}

#[test]
fn axis_source_returns_none_for_off_by_one() {
    // Need 2 bytes at offset 1, so data length must be >= 3
    let src = KsAxisSource::new(1, false);
    assert!(src.parse_u16(&[0xFF, 0xFF]).is_none());
}

#[test]
fn axis_source_succeeds_at_exact_boundary() -> R {
    let src = KsAxisSource::new(1, false);
    let data = [0x00, 0x34, 0x12];
    let val = src.parse_u16(&data).ok_or("expected boundary parse")?;
    assert_eq!(val, 0x1234);
    Ok(())
}

#[test]
fn axis_source_large_offset_doesnt_panic() {
    let src = KsAxisSource::new(usize::MAX - 1, false);
    assert!(src.parse_u16(&[0x00, 0x00, 0x00, 0x00]).is_none());
}

#[test]
fn axis_source_const_new_fields() {
    let src = KsAxisSource::new(42, true);
    assert_eq!(src.offset, 42);
    assert!(src.signed);
    let src2 = KsAxisSource::new(0, false);
    assert_eq!(src2.offset, 0);
    assert!(!src2.signed);
}

// ── KsBitSource parsing ─────────────────────────────────────────────────

#[test]
fn bit_source_active_bit() -> R {
    let src = KsBitSource::new(0, 0x04);
    let val = src.parse(&[0x07]).ok_or("expected bit parse")?;
    assert!(val);
    Ok(())
}

#[test]
fn bit_source_inactive_bit() -> R {
    let src = KsBitSource::new(0, 0x04);
    let val = src.parse(&[0x03]).ok_or("expected bit parse")?;
    assert!(!val);
    Ok(())
}

#[test]
fn bit_source_inverted_active() -> R {
    let src = KsBitSource::inverted(0, 0x04);
    let val = src.parse(&[0x04]).ok_or("expected inverted parse")?;
    assert!(!val, "inverted: bit set should return false");
    Ok(())
}

#[test]
fn bit_source_inverted_inactive() -> R {
    let src = KsBitSource::with_invert(0, 0x04);
    let val = src.parse(&[0x00]).ok_or("expected inverted parse")?;
    assert!(val, "inverted: bit clear should return true");
    Ok(())
}

#[test]
fn bit_source_all_single_bit_masks() -> R {
    for bit in 0..8u8 {
        let mask = 1u8 << bit;
        let src = KsBitSource::new(0, mask);
        let active = src.parse(&[0xFF]).ok_or("should parse full mask byte")?;
        assert!(active);
        let inactive = src.parse(&[0x00]).ok_or("should parse zero byte")?;
        assert!(!inactive);
    }
    Ok(())
}

#[test]
fn bit_source_full_byte_mask() -> R {
    let src = KsBitSource::new(0, 0xFF);
    let active = src
        .parse(&[0x01])
        .ok_or("any bit set activates full mask")?;
    assert!(active);
    let inactive = src.parse(&[0x00]).ok_or("zero byte")?;
    assert!(!inactive);
    Ok(())
}

#[test]
fn bit_source_with_invert_alias_same_as_inverted() {
    let a = KsBitSource::with_invert(3, 0x10);
    let b = KsBitSource::inverted(3, 0x10);
    assert_eq!(a, b);
}

#[test]
fn bit_source_oob_returns_none() {
    let src = KsBitSource::new(5, 0x01);
    assert!(src.parse(&[0x00]).is_none());
}

#[test]
fn bit_source_exact_boundary() -> R {
    let src = KsBitSource::new(2, 0x80);
    let val = src.parse(&[0x00, 0x00, 0x80]).ok_or("boundary parse")?;
    assert!(val);
    Ok(())
}

#[test]
fn bit_source_large_offset_doesnt_panic() {
    let src = KsBitSource::new(usize::MAX, 0x01);
    assert!(src.parse(&[0xFF]).is_none());
}

// ── KsByteSource parsing ─────────────────────────────────────────────────

#[test]
fn byte_source_basic() -> R {
    let src = KsByteSource::new(2);
    let val = src
        .parse(&[0x00, 0x11, 0xAB])
        .ok_or("expected byte parse")?;
    assert_eq!(val, 0xAB);
    Ok(())
}

#[test]
fn byte_source_first_byte() -> R {
    let src = KsByteSource::new(0);
    let val = src.parse(&[0x42]).ok_or("first byte")?;
    assert_eq!(val, 0x42);
    Ok(())
}

#[test]
fn byte_source_oob() {
    let src = KsByteSource::new(5);
    assert!(src.parse(&[0x00, 0x01]).is_none());
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

// ── KsReportSnapshot ────────────────────────────────────────────────────

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
fn snapshot_from_common_controls_populates() {
    let buttons = [0xAA; KS_BUTTON_BYTES];
    let s = KsReportSnapshot::from_common_controls(999, buttons, 0x07);
    assert_eq!(s.tick, 999);
    assert_eq!(s.buttons, buttons);
    assert_eq!(s.hat, 0x07);
    assert_eq!(s.clutch_mode, KsClutchMode::Unknown);
    assert_eq!(s.encoders, [0i16; KS_ENCODER_COUNT]);
}

#[test]
fn snapshot_from_common_controls_max_tick() {
    let s = KsReportSnapshot::from_common_controls(u32::MAX, [0; KS_BUTTON_BYTES], 0);
    assert_eq!(s.tick, u32::MAX);
}

// ── both_clutches_pressed ────────────────────────────────────────────────

#[test]
fn both_clutches_combined_axis_at_threshold() {
    let s = KsReportSnapshot {
        clutch_mode: KsClutchMode::CombinedAxis,
        clutch_combined: Some(30_000),
        ..Default::default()
    };
    assert_eq!(s.both_clutches_pressed(30_000), Some(true));
    assert_eq!(s.both_clutches_pressed(30_001), Some(false));
}

#[test]
fn both_clutches_combined_axis_zero_threshold() {
    let s = KsReportSnapshot {
        clutch_mode: KsClutchMode::CombinedAxis,
        clutch_combined: Some(0),
        ..Default::default()
    };
    assert_eq!(s.both_clutches_pressed(0), Some(true));
}

#[test]
fn both_clutches_combined_axis_max_value() {
    let s = KsReportSnapshot {
        clutch_mode: KsClutchMode::CombinedAxis,
        clutch_combined: Some(u16::MAX),
        ..Default::default()
    };
    assert_eq!(s.both_clutches_pressed(u16::MAX), Some(true));
}

#[test]
fn both_clutches_combined_axis_none_data() {
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
        clutch_left: Some(40_000),
        clutch_right: Some(50_000),
        ..Default::default()
    };
    assert_eq!(s.both_clutches_pressed(30_000), Some(true));
}

#[test]
fn both_clutches_independent_one_below() {
    let s = KsReportSnapshot {
        clutch_mode: KsClutchMode::IndependentAxis,
        clutch_left: Some(40_000),
        clutch_right: Some(20_000),
        ..Default::default()
    };
    assert_eq!(s.both_clutches_pressed(30_000), Some(false));
}

#[test]
fn both_clutches_independent_left_missing() {
    let s = KsReportSnapshot {
        clutch_mode: KsClutchMode::IndependentAxis,
        clutch_left: None,
        clutch_right: Some(40_000),
        ..Default::default()
    };
    assert_eq!(s.both_clutches_pressed(30_000), None);
}

#[test]
fn both_clutches_independent_right_missing() {
    let s = KsReportSnapshot {
        clutch_mode: KsClutchMode::IndependentAxis,
        clutch_left: Some(40_000),
        clutch_right: None,
        ..Default::default()
    };
    assert_eq!(s.both_clutches_pressed(30_000), None);
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

// ── KsReportMap: empty map ──────────────────────────────────────────────

#[test]
fn empty_map_all_fields_none() {
    let m = KsReportMap::empty();
    assert_eq!(m.report_id, None);
    assert_eq!(m.buttons_offset, None);
    assert_eq!(m.hat_offset, None);
    assert_eq!(m.clutch_mode_hint, KsClutchMode::Unknown);
    assert_eq!(m.rotary_mode_hint, KsRotaryMode::Unknown);
    assert_eq!(m.joystick_mode_hint, KsJoystickMode::Unknown);
    assert!(m.clutch_left_axis.is_none());
    assert!(m.clutch_right_axis.is_none());
    assert!(m.clutch_combined_axis.is_none());
    assert!(m.clutch_left_button.is_none());
    assert!(m.clutch_right_button.is_none());
    assert!(m.left_rotary_axis.is_none());
    assert!(m.right_rotary_axis.is_none());
    assert!(m.joystick_hat.is_none());
    for i in 0..KS_ENCODER_COUNT {
        assert!(m.encoders[i].is_none());
    }
}

#[test]
fn empty_map_parses_any_report() -> R {
    let m = KsReportMap::empty();
    let s = m
        .parse(42, &[0xFF, 0x00])
        .ok_or("empty map should accept any report")?;
    assert_eq!(s.tick, 42);
    Ok(())
}

// ── KsReportMap: report ID filtering ────────────────────────────────────

#[test]
fn map_rejects_wrong_report_id() {
    let mut m = KsReportMap::empty();
    m.report_id = Some(0x01);
    assert!(m.parse(0, &[0x02, 0x00]).is_none());
}

#[test]
fn map_rejects_empty_report_with_required_id() {
    let mut m = KsReportMap::empty();
    m.report_id = Some(0x01);
    assert!(m.parse(0, &[]).is_none());
}

#[test]
fn map_accepts_matching_report_id() -> R {
    let mut m = KsReportMap::empty();
    m.report_id = Some(0x01);
    let s = m
        .parse(0, &[0x01, 0x00])
        .ok_or("should accept matching id")?;
    assert_eq!(s.tick, 0);
    Ok(())
}

// ── KsReportMap: button parsing ─────────────────────────────────────────

#[test]
fn parse_buttons_fully_populated() -> R {
    let mut m = KsReportMap::empty();
    m.buttons_offset = Some(0);
    let mut data = [0u8; KS_BUTTON_BYTES];
    for (i, byte) in data.iter_mut().enumerate() {
        *byte = (i + 1) as u8;
    }
    let s = m.parse(0, &data).ok_or("should parse full buttons")?;
    for i in 0..KS_BUTTON_BYTES {
        assert_eq!(s.buttons[i], (i + 1) as u8);
    }
    Ok(())
}

#[test]
fn parse_buttons_partial_fill() -> R {
    let mut m = KsReportMap::empty();
    m.buttons_offset = Some(0);
    let data = [0xAA, 0xBB, 0xCC];
    let s = m.parse(0, &data).ok_or("should parse partial buttons")?;
    assert_eq!(s.buttons[0], 0xAA);
    assert_eq!(s.buttons[1], 0xBB);
    assert_eq!(s.buttons[2], 0xCC);
    for i in 3..KS_BUTTON_BYTES {
        assert_eq!(s.buttons[i], 0);
    }
    Ok(())
}

#[test]
fn parse_buttons_with_offset() -> R {
    let mut m = KsReportMap::empty();
    m.buttons_offset = Some(2);
    let mut data = vec![0x00; 2 + KS_BUTTON_BYTES];
    data[2] = 0x42;
    data[3] = 0x99;
    let s = m.parse(0, &data).ok_or("should parse buttons at offset")?;
    assert_eq!(s.buttons[0], 0x42);
    assert_eq!(s.buttons[1], 0x99);
    Ok(())
}

// ── KsReportMap: encoder parsing ────────────────────────────────────────

#[test]
fn parse_all_encoder_slots() -> R {
    let mut m = KsReportMap::empty();
    for i in 0..KS_ENCODER_COUNT {
        m.encoders[i] = Some(KsAxisSource::new(i * 2, true));
    }
    let mut data = vec![0u8; KS_ENCODER_COUNT * 2];
    for i in 0..KS_ENCODER_COUNT {
        let val = ((i as i16) + 1) * 100;
        data[i * 2..i * 2 + 2].copy_from_slice(&val.to_le_bytes());
    }
    let s = m.parse(0, &data).ok_or("should parse encoders")?;
    for i in 0..KS_ENCODER_COUNT {
        assert_eq!(s.encoders[i], ((i as i16) + 1) * 100);
    }
    Ok(())
}

#[test]
fn parse_encoder_oob_defaults_to_zero() -> R {
    let mut m = KsReportMap::empty();
    m.encoders[0] = Some(KsAxisSource::new(100, true));
    let s = m
        .parse(0, &[0xFF, 0xFF])
        .ok_or("should parse with oob encoder")?;
    assert_eq!(s.encoders[0], 0);
    Ok(())
}

#[test]
fn parse_encoder_unsigned_reinterpret_as_i16() -> R {
    let mut m = KsReportMap::empty();
    m.encoders[0] = Some(KsAxisSource::new(0, false));
    let data = 0xFFFFu16.to_le_bytes();
    let s = m.parse(0, &data).ok_or("should parse unsigned encoder")?;
    assert_eq!(s.encoders[0], -1); // 0xFFFF as i16 = -1
    Ok(())
}

// ── KsReportMap: rotary axis override ───────────────────────────────────

#[test]
fn rotary_axes_override_encoder_slots() -> R {
    let mut m = KsReportMap::empty();
    m.encoders[0] = Some(KsAxisSource::new(0, true));
    m.encoders[1] = Some(KsAxisSource::new(2, true));
    m.left_rotary_axis = Some(KsAxisSource::new(4, true));
    m.right_rotary_axis = Some(KsAxisSource::new(6, true));

    let mut data = [0u8; 8];
    data[0..2].copy_from_slice(&10i16.to_le_bytes());
    data[2..4].copy_from_slice(&20i16.to_le_bytes());
    data[4..6].copy_from_slice(&30i16.to_le_bytes());
    data[6..8].copy_from_slice(&40i16.to_le_bytes());

    let s = m.parse(0, &data).ok_or("should parse rotary override")?;
    // Rotary axes overwrite encoder slots 0 and 1
    assert_eq!(s.encoders[0], 30);
    assert_eq!(s.encoders[1], 40);
    Ok(())
}

#[test]
fn single_left_rotary_leaves_right_slot_default() -> R {
    let mut m = KsReportMap::empty();
    m.left_rotary_axis = Some(KsAxisSource::new(0, true));
    let mut data = [0u8; 4];
    data[0..2].copy_from_slice(&42i16.to_le_bytes());
    let s = m.parse(0, &data).ok_or("should parse single rotary")?;
    assert_eq!(s.encoders[0], 42);
    assert_eq!(s.encoders[1], 0);
    Ok(())
}

// ── KsReportMap: hat parsing ────────────────────────────────────────────

#[test]
fn hat_from_joystick_source() -> R {
    let mut m = KsReportMap::empty();
    m.joystick_hat = Some(KsByteSource::new(0));
    let s = m.parse(0, &[0x42]).ok_or("should parse joystick hat")?;
    assert_eq!(s.hat, 0x42);
    Ok(())
}

#[test]
fn hat_from_offset_fallback() -> R {
    let mut m = KsReportMap::empty();
    m.hat_offset = Some(1);
    let s = m.parse(0, &[0x00, 0x99]).ok_or("should parse hat offset")?;
    assert_eq!(s.hat, 0x99);
    Ok(())
}

#[test]
fn hat_joystick_overrides_offset() -> R {
    let mut m = KsReportMap::empty();
    m.joystick_hat = Some(KsByteSource::new(0));
    m.hat_offset = Some(1);
    let s = m
        .parse(0, &[0xAA, 0xBB])
        .ok_or("should parse hat override")?;
    assert_eq!(s.hat, 0xAA); // joystick_hat wins
    Ok(())
}

#[test]
fn hat_both_oob_defaults_to_zero() -> R {
    let mut m = KsReportMap::empty();
    m.joystick_hat = Some(KsByteSource::new(100));
    m.hat_offset = Some(200);
    let s = m.parse(0, &[0xFF]).ok_or("should parse with oob hat")?;
    assert_eq!(s.hat, 0);
    Ok(())
}

// ── KsReportMap: clutch parsing ─────────────────────────────────────────

#[test]
fn parse_clutch_combined() -> R {
    let mut m = KsReportMap::empty();
    m.clutch_mode_hint = KsClutchMode::CombinedAxis;
    m.clutch_combined_axis = Some(KsAxisSource::new(0, false));
    let data = 5000u16.to_le_bytes();
    let s = m.parse(0, &data).ok_or("should parse combined clutch")?;
    assert_eq!(s.clutch_combined, Some(5000));
    assert_eq!(s.clutch_mode, KsClutchMode::CombinedAxis);
    Ok(())
}

#[test]
fn parse_clutch_independent() -> R {
    let mut m = KsReportMap::empty();
    m.clutch_mode_hint = KsClutchMode::IndependentAxis;
    m.clutch_left_axis = Some(KsAxisSource::new(0, false));
    m.clutch_right_axis = Some(KsAxisSource::new(2, false));
    let mut data = [0u8; 4];
    data[0..2].copy_from_slice(&3000u16.to_le_bytes());
    data[2..4].copy_from_slice(&4000u16.to_le_bytes());
    let s = m.parse(0, &data).ok_or("should parse independent clutch")?;
    assert_eq!(s.clutch_left, Some(3000));
    assert_eq!(s.clutch_right, Some(4000));
    Ok(())
}

#[test]
fn parse_clutch_buttons() -> R {
    let mut m = KsReportMap::empty();
    m.clutch_mode_hint = KsClutchMode::Button;
    m.clutch_left_button = Some(KsBitSource::new(0, 0x01));
    m.clutch_right_button = Some(KsBitSource::new(0, 0x02));
    let s = m.parse(0, &[0x03]).ok_or("should parse clutch buttons")?;
    assert_eq!(s.clutch_left_button, Some(true));
    assert_eq!(s.clutch_right_button, Some(true));
    assert_eq!(s.clutch_mode, KsClutchMode::Button);
    Ok(())
}

#[test]
fn parse_clutch_axis_oob_returns_none() -> R {
    let mut m = KsReportMap::empty();
    m.clutch_combined_axis = Some(KsAxisSource::new(100, false));
    let s = m.parse(0, &[0xFF]).ok_or("should parse with oob clutch")?;
    assert_eq!(s.clutch_combined, None);
    Ok(())
}

// ── KsReportMap: mode hints propagation ─────────────────────────────────

#[test]
fn parse_mode_hints_propagate() -> R {
    let mut m = KsReportMap::empty();
    m.clutch_mode_hint = KsClutchMode::IndependentAxis;
    m.rotary_mode_hint = KsRotaryMode::Knob;
    m.joystick_mode_hint = KsJoystickMode::DPad;
    let s = m.parse(0, &[0x00]).ok_or("should parse mode hints")?;
    assert_eq!(s.clutch_mode, KsClutchMode::IndependentAxis);
    assert_eq!(s.rotary_mode, KsRotaryMode::Knob);
    assert_eq!(s.joystick_mode, KsJoystickMode::DPad);
    Ok(())
}

// ── KsReportMap: fully populated report ─────────────────────────────────

#[test]
fn parse_fully_populated_report() -> R {
    let mut m = KsReportMap::empty();
    m.report_id = Some(0x01);
    m.buttons_offset = Some(1);
    m.hat_offset = Some(17);
    m.clutch_mode_hint = KsClutchMode::IndependentAxis;
    m.clutch_left_axis = Some(KsAxisSource::new(18, false));
    m.clutch_right_axis = Some(KsAxisSource::new(20, false));
    m.clutch_left_button = Some(KsBitSource::new(22, 0x01));
    m.clutch_right_button = Some(KsBitSource::new(22, 0x02));
    m.encoders[0] = Some(KsAxisSource::new(23, true));
    m.encoders[1] = Some(KsAxisSource::new(25, true));
    m.joystick_hat = Some(KsByteSource::new(27));
    m.joystick_mode_hint = KsJoystickMode::DPad;
    m.rotary_mode_hint = KsRotaryMode::Knob;

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

    let s = m.parse(42, &report).ok_or("should parse full report")?;
    assert_eq!(s.tick, 42);
    assert_eq!(s.buttons[0], 1);
    assert_eq!(s.buttons[15], 16);
    assert_eq!(s.hat, 0x07); // joystick_hat overrides hat_offset
    assert_eq!(s.clutch_mode, KsClutchMode::IndependentAxis);
    assert_eq!(s.clutch_left, Some(1000));
    assert_eq!(s.clutch_right, Some(2000));
    assert_eq!(s.clutch_left_button, Some(true));
    assert_eq!(s.clutch_right_button, Some(true));
    assert_eq!(s.encoders[0], -100);
    assert_eq!(s.encoders[1], 200);
    assert_eq!(s.rotary_mode, KsRotaryMode::Knob);
    assert_eq!(s.joystick_mode, KsJoystickMode::DPad);
    Ok(())
}

// ── Edge cases: empty maps, short reports ───────────────────────────────

#[test]
fn empty_map_parses_empty_report() -> R {
    let m = KsReportMap::empty();
    let s = m
        .parse(0, &[])
        .ok_or("empty map should accept empty report")?;
    assert_eq!(s.tick, 0);
    assert_eq!(s.hat, 0);
    assert_eq!(s.buttons, [0u8; KS_BUTTON_BYTES]);
    Ok(())
}

#[test]
fn map_with_report_id_rejects_empty_report() {
    let mut m = KsReportMap::empty();
    m.report_id = Some(0x01);
    assert!(m.parse(0, &[]).is_none());
}

#[test]
fn very_short_report_graceful_degradation() -> R {
    let mut m = KsReportMap::empty();
    m.buttons_offset = Some(0);
    m.hat_offset = Some(100);
    m.encoders[0] = Some(KsAxisSource::new(100, true));

    let s = m
        .parse(0, &[0xAA, 0xBB])
        .ok_or("should parse short report")?;
    assert_eq!(s.buttons[0], 0xAA);
    assert_eq!(s.buttons[1], 0xBB);
    assert_eq!(s.buttons[2..], [0u8; KS_BUTTON_BYTES - 2]);
    assert_eq!(s.hat, 0);
    assert_eq!(s.encoders[0], 0);
    Ok(())
}

// ── Default enum values ─────────────────────────────────────────────────

#[test]
fn clutch_mode_default_is_unknown() {
    assert_eq!(KsClutchMode::default(), KsClutchMode::Unknown);
}

#[test]
fn rotary_mode_default_is_unknown() {
    assert_eq!(KsRotaryMode::default(), KsRotaryMode::Unknown);
}

#[test]
fn joystick_mode_default_is_unknown() {
    assert_eq!(KsJoystickMode::default(), KsJoystickMode::Unknown);
}

// ── Constants ───────────────────────────────────────────────────────────

#[test]
fn button_bytes_constant_is_sixteen() {
    assert_eq!(KS_BUTTON_BYTES, 16);
}

#[test]
fn encoder_count_constant_is_eight() {
    assert_eq!(KS_ENCODER_COUNT, 8);
}

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
fn encoder_array_in_map_matches_constant() {
    let m = KsReportMap::empty();
    assert_eq!(m.encoders.len(), KS_ENCODER_COUNT);
}

// ── Copy/Clone semantics ────────────────────────────────────────────────

#[test]
fn snapshot_copy_semantics() {
    let a = KsReportSnapshot {
        tick: 100,
        buttons: [0xFF; KS_BUTTON_BYTES],
        hat: 0x05,
        encoders: [1, 2, 3, 4, 5, 6, 7, 8],
        clutch_combined: Some(5000),
        clutch_left: Some(3000),
        clutch_right: Some(4000),
        clutch_left_button: Some(true),
        clutch_right_button: Some(false),
        clutch_mode: KsClutchMode::IndependentAxis,
        rotary_mode: KsRotaryMode::Button,
        joystick_mode: KsJoystickMode::DPad,
    };
    let b = a;
    assert_eq!(a, b);
}

#[test]
fn report_map_copy_semantics() {
    let mut a = KsReportMap::empty();
    a.report_id = Some(0x01);
    a.encoders[0] = Some(KsAxisSource::new(0, true));
    let b = a;
    assert_eq!(a, b);
}

// ── Duplicate bindings edge case ────────────────────────────────────────

#[test]
fn duplicate_encoder_offsets_both_stored() -> R {
    let mut m = KsReportMap::empty();
    // Two encoders at same offset — both slots get populated
    m.encoders[0] = Some(KsAxisSource::new(0, true));
    m.encoders[1] = Some(KsAxisSource::new(0, true));

    let data = 500i16.to_le_bytes();
    let s = m
        .parse(0, &data)
        .ok_or("should parse duplicate encoder offsets")?;
    assert_eq!(s.encoders[0], 500);
    assert_eq!(s.encoders[1], 500);
    Ok(())
}

#[test]
fn duplicate_clutch_and_encoder_at_same_offset() -> R {
    let mut m = KsReportMap::empty();
    m.clutch_combined_axis = Some(KsAxisSource::new(0, false));
    m.encoders[0] = Some(KsAxisSource::new(0, true));

    let data = 1234u16.to_le_bytes();
    let s = m.parse(0, &data).ok_or("should parse overlapping")?;
    assert_eq!(s.clutch_combined, Some(1234));
    // Encoder reads same bytes but as i16
    assert_eq!(s.encoders[0], 1234);
    Ok(())
}

// ── Proptest ─────────────────────────────────────────────────────────────

use proptest::prelude::*;

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(256))]

    #[test]
    fn prop_u16_all_values_round_trip(val: u16) {
        let src = KsAxisSource::new(0, false);
        let data = val.to_le_bytes();
        prop_assert_eq!(src.parse_u16(&data), Some(val));
    }

    #[test]
    fn prop_i16_all_values_round_trip(val: i16) {
        let src = KsAxisSource::new(0, true);
        let data = val.to_le_bytes();
        prop_assert_eq!(src.parse_i16(&data), Some(val));
    }

    #[test]
    fn prop_bit_source_non_inverted_matches_mask(byte: u8, bit in 0u8..8u8) {
        let mask = 1u8 << bit;
        let src = KsBitSource::new(0, mask);
        let expected = byte & mask != 0;
        prop_assert_eq!(src.parse(&[byte]), Some(expected));
    }

    #[test]
    fn prop_bit_source_inverted_is_opposite(byte: u8, bit in 0u8..8u8) {
        let mask = 1u8 << bit;
        let normal = KsBitSource::new(0, mask);
        let inv = KsBitSource::inverted(0, mask);
        let n = normal.parse(&[byte]);
        let i = inv.parse(&[byte]);
        prop_assert_eq!(n.map(|v| !v), i);
    }

    #[test]
    fn prop_byte_source_matches_index(data in proptest::collection::vec(any::<u8>(), 1..=16)) {
        for (i, &expected) in data.iter().enumerate() {
            let src = KsByteSource::new(i);
            prop_assert_eq!(src.parse(&data), Some(expected));
        }
    }

    #[test]
    fn prop_empty_map_always_parses_non_empty_report(
        data in proptest::collection::vec(any::<u8>(), 1..=64),
        tick: u32,
    ) {
        let m = KsReportMap::empty();
        prop_assert!(m.parse(tick, &data).is_some());
    }

    #[test]
    fn prop_both_clutches_combined_threshold(value: u16, threshold: u16) {
        let s = KsReportSnapshot {
            clutch_mode: KsClutchMode::CombinedAxis,
            clutch_combined: Some(value),
            ..Default::default()
        };
        let result = s.both_clutches_pressed(threshold);
        prop_assert_eq!(result, Some(value >= threshold));
    }

    #[test]
    fn prop_encoder_values_round_trip(
        values in proptest::collection::vec(any::<i16>(), KS_ENCODER_COUNT..=KS_ENCODER_COUNT),
    ) {
        let mut m = KsReportMap::empty();
        for i in 0..KS_ENCODER_COUNT {
            m.encoders[i] = Some(KsAxisSource::new(i * 2, true));
        }
        let mut data = vec![0u8; KS_ENCODER_COUNT * 2];
        for (i, &val) in values.iter().enumerate() {
            data[i * 2..i * 2 + 2].copy_from_slice(&val.to_le_bytes());
        }
        let s = m.parse(0, &data).ok_or_else(|| TestCaseError::Fail("parse failed".into()))?;
        for (i, &expected) in values.iter().enumerate() {
            prop_assert_eq!(s.encoders[i], expected);
        }
    }

    #[test]
    fn prop_snapshot_clutch_independent_both_missing_is_none(threshold: u16) {
        let s = KsReportSnapshot {
            clutch_mode: KsClutchMode::IndependentAxis,
            clutch_left: None,
            clutch_right: None,
            ..Default::default()
        };
        prop_assert_eq!(s.both_clutches_pressed(threshold), None);
    }
}

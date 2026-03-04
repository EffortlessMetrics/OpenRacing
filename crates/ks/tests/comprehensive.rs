#![allow(clippy::redundant_closure)]

use racing_wheel_ks::{
    KS_BUTTON_BYTES, KS_ENCODER_COUNT, KsAxisSource, KsBitSource, KsByteSource, KsClutchMode,
    KsJoystickMode, KsReportMap, KsReportSnapshot, KsRotaryMode,
};

type R = Result<(), Box<dyn std::error::Error>>;

// ── KsReportMap: empty map ──────────────────────────────────────────────

#[test]
fn empty_map_has_no_bindings() {
    let map = KsReportMap::empty();
    assert_eq!(map.report_id, None);
    assert_eq!(map.buttons_offset, None);
    assert_eq!(map.hat_offset, None);
    assert_eq!(map.clutch_mode_hint, KsClutchMode::Unknown);
    assert_eq!(map.rotary_mode_hint, KsRotaryMode::Unknown);
    assert_eq!(map.joystick_mode_hint, KsJoystickMode::Unknown);
}

#[test]
fn empty_map_accepts_any_report() -> R {
    let map = KsReportMap::empty();
    let report = [0xFF, 0x00, 0x00];
    let snapshot = map
        .parse(1, &report)
        .ok_or("empty map should accept any report")?;
    assert_eq!(snapshot.tick, 1);
    Ok(())
}

// ── Report ID filtering ─────────────────────────────────────────────────

#[test]
fn rejects_wrong_report_id() {
    let mut map = KsReportMap::empty();
    map.report_id = Some(0x01);
    let report = [0x02, 0x00, 0x00];
    assert!(map.parse(0, &report).is_none());
}

#[test]
fn rejects_empty_report_with_required_id() {
    let mut map = KsReportMap::empty();
    map.report_id = Some(0x01);
    assert!(map.parse(0, &[]).is_none());
}

#[test]
fn accepts_matching_report_id() -> R {
    let mut map = KsReportMap::empty();
    map.report_id = Some(0x01);
    let report = [0x01, 0x00, 0x00];
    let snapshot = map.parse(5, &report).ok_or("matching ID should parse")?;
    assert_eq!(snapshot.tick, 5);
    Ok(())
}

// ── Button parsing ──────────────────────────────────────────────────────

#[test]
fn buttons_fully_populated() -> R {
    let mut map = KsReportMap::empty();
    map.buttons_offset = Some(0);
    let mut data = [0u8; KS_BUTTON_BYTES];
    for (i, b) in data.iter_mut().enumerate() {
        *b = i as u8 + 1;
    }
    let snapshot = map.parse(0, &data).ok_or("full buttons should parse")?;
    assert_eq!(snapshot.buttons, data);
    Ok(())
}

#[test]
fn buttons_partial_fill() -> R {
    let mut map = KsReportMap::empty();
    map.buttons_offset = Some(0);
    let data = [0xAA, 0xBB, 0xCC];
    let snapshot = map.parse(0, &data).ok_or("partial buttons should parse")?;
    assert_eq!(snapshot.buttons[0], 0xAA);
    assert_eq!(snapshot.buttons[1], 0xBB);
    assert_eq!(snapshot.buttons[2], 0xCC);
    assert_eq!(snapshot.buttons[3..], [0u8; KS_BUTTON_BYTES - 3]);
    Ok(())
}

// ── Encoder parsing ─────────────────────────────────────────────────────

#[test]
fn all_encoder_slots() -> R {
    let mut map = KsReportMap::empty();
    for i in 0..KS_ENCODER_COUNT {
        map.encoders[i] = Some(KsAxisSource::new(i * 2, true));
    }
    let mut data = vec![0u8; KS_ENCODER_COUNT * 2];
    for i in 0..KS_ENCODER_COUNT {
        let val = (i as i16 + 1) * 100;
        data[i * 2..i * 2 + 2].copy_from_slice(&val.to_le_bytes());
    }
    let snapshot = map.parse(0, &data).ok_or("all encoders should parse")?;
    for i in 0..KS_ENCODER_COUNT {
        assert_eq!(snapshot.encoders[i], (i as i16 + 1) * 100);
    }
    Ok(())
}

#[test]
fn encoder_oob_defaults_to_zero() -> R {
    let mut map = KsReportMap::empty();
    map.encoders[0] = Some(KsAxisSource::new(100, true));
    let data = [0xAA, 0xBB];
    let snapshot = map.parse(0, &data).ok_or("oob encoder should default")?;
    assert_eq!(snapshot.encoders[0], 0);
    Ok(())
}

// ── Rotary axis override ────────────────────────────────────────────────

#[test]
fn rotary_axes_override_encoder_slots() -> R {
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

    let snapshot = map.parse(0, &data).ok_or("rotary override should parse")?;
    assert_eq!(snapshot.encoders[0], 30);
    assert_eq!(snapshot.encoders[1], 40);
    Ok(())
}

// ── Hat parsing ─────────────────────────────────────────────────────────

#[test]
fn hat_from_joystick_source() -> R {
    let mut map = KsReportMap::empty();
    map.joystick_hat = Some(KsByteSource::new(0));
    let snapshot = map.parse(0, &[0x42]).ok_or("hat from joystick source")?;
    assert_eq!(snapshot.hat, 0x42);
    Ok(())
}

#[test]
fn hat_from_offset_fallback() -> R {
    let mut map = KsReportMap::empty();
    map.hat_offset = Some(0);
    let snapshot = map.parse(0, &[0x77]).ok_or("hat from offset fallback")?;
    assert_eq!(snapshot.hat, 0x77);
    Ok(())
}

// ── Clutch parsing ──────────────────────────────────────────────────────

#[test]
fn clutch_combined() -> R {
    let mut map = KsReportMap::empty();
    map.clutch_mode_hint = KsClutchMode::CombinedAxis;
    map.clutch_combined_axis = Some(KsAxisSource::new(0, false));
    let data = 0x1234u16.to_le_bytes();
    let snapshot = map.parse(0, &data).ok_or("combined clutch should parse")?;
    assert_eq!(snapshot.clutch_combined, Some(0x1234));
    assert_eq!(snapshot.clutch_mode, KsClutchMode::CombinedAxis);
    Ok(())
}

#[test]
fn clutch_buttons() -> R {
    let mut map = KsReportMap::empty();
    map.clutch_mode_hint = KsClutchMode::Button;
    map.clutch_left_button = Some(KsBitSource::new(0, 0x01));
    map.clutch_right_button = Some(KsBitSource::new(0, 0x02));
    let snapshot = map.parse(0, &[0x03]).ok_or("clutch buttons should parse")?;
    assert_eq!(snapshot.clutch_left_button, Some(true));
    assert_eq!(snapshot.clutch_right_button, Some(true));
    Ok(())
}

// ── KsReportSnapshot: both_clutches_pressed ─────────────────────────────

#[test]
fn both_clutches_combined_axis() {
    let snapshot = KsReportSnapshot {
        clutch_mode: KsClutchMode::CombinedAxis,
        clutch_combined: Some(31_000),
        ..Default::default()
    };
    assert_eq!(snapshot.both_clutches_pressed(30_000), Some(true));
    assert_eq!(snapshot.both_clutches_pressed(32_000), Some(false));
}

#[test]
fn both_clutches_independent_axis() {
    let snapshot = KsReportSnapshot {
        clutch_mode: KsClutchMode::IndependentAxis,
        clutch_left: Some(31_000),
        clutch_right: Some(40_000),
        ..Default::default()
    };
    assert_eq!(snapshot.both_clutches_pressed(30_000), Some(true));
    assert_eq!(snapshot.both_clutches_pressed(32_000), Some(false));
}

#[test]
fn both_clutches_button_mode() {
    let mut snapshot = KsReportSnapshot {
        clutch_mode: KsClutchMode::Button,
        clutch_left_button: Some(true),
        clutch_right_button: Some(true),
        ..Default::default()
    };
    assert_eq!(snapshot.both_clutches_pressed(0), Some(true));
    snapshot.clutch_right_button = Some(false);
    assert_eq!(snapshot.both_clutches_pressed(0), Some(false));
}

#[test]
fn both_clutches_unknown_returns_none() {
    let snapshot = KsReportSnapshot::default();
    assert_eq!(snapshot.both_clutches_pressed(0), None);
}

#[test]
fn independent_one_missing_returns_none() {
    let snapshot = KsReportSnapshot {
        clutch_mode: KsClutchMode::IndependentAxis,
        clutch_left: Some(31_000),
        clutch_right: None,
        ..Default::default()
    };
    assert_eq!(snapshot.both_clutches_pressed(30_000), None);
}

// ── from_common_controls ────────────────────────────────────────────────

#[test]
fn from_common_controls() {
    let buttons = [0x01u8; KS_BUTTON_BYTES];
    let snapshot = KsReportSnapshot::from_common_controls(7, buttons, 0x42);
    assert_eq!(snapshot.tick, 7);
    assert_eq!(snapshot.buttons, buttons);
    assert_eq!(snapshot.hat, 0x42);
    assert_eq!(snapshot.clutch_mode, KsClutchMode::Unknown);
}

// ── KsAxisSource ────────────────────────────────────────────────────────

#[test]
fn axis_source_u16() -> R {
    let src = KsAxisSource::new(1, false);
    let data = [0x00, 0x34, 0x12];
    assert_eq!(src.parse_u16(&data).ok_or("u16 parse")?, 0x1234);
    Ok(())
}

#[test]
fn axis_source_i16() -> R {
    let src = KsAxisSource::new(0, true);
    let data = (-32768i16).to_le_bytes();
    assert_eq!(src.parse_i16(&data).ok_or("i16 parse")?, -32768);
    Ok(())
}

#[test]
fn axis_source_short_data() {
    let src = KsAxisSource::new(5, false);
    assert!(src.parse_u16(&[0x00, 0x01]).is_none());
    assert!(src.parse_i16(&[0x00, 0x01]).is_none());
}

// ── KsBitSource ─────────────────────────────────────────────────────────

#[test]
fn bit_source_active() -> R {
    let src = KsBitSource::new(0, 0x04);
    assert!(src.parse(&[0x07]).ok_or("bit parse")?);
    Ok(())
}

#[test]
fn bit_source_inactive() -> R {
    let src = KsBitSource::new(0, 0x04);
    assert!(!src.parse(&[0x03]).ok_or("bit parse")?);
    Ok(())
}

#[test]
fn bit_source_inverted() -> R {
    let src = KsBitSource::inverted(0, 0x04);
    assert!(!src.parse(&[0x04]).ok_or("inverted bit set")?);
    assert!(src.parse(&[0x00]).ok_or("inverted bit clear")?);
    Ok(())
}

#[test]
fn bit_source_oob() {
    let src = KsBitSource::new(5, 0x01);
    assert!(src.parse(&[0x00]).is_none());
}

// ── KsByteSource ────────────────────────────────────────────────────────

#[test]
fn byte_source_parse() -> R {
    let src = KsByteSource::new(2);
    assert_eq!(
        src.parse(&[0x00, 0x11, 0xAB, 0x00]).ok_or("byte parse")?,
        0xAB
    );
    Ok(())
}

#[test]
fn byte_source_oob() {
    let src = KsByteSource::new(5);
    assert!(src.parse(&[0x00, 0x01]).is_none());
}

// ── Default values ──────────────────────────────────────────────────────

#[test]
fn defaults() {
    assert_eq!(KsClutchMode::default(), KsClutchMode::Unknown);
    assert_eq!(KsRotaryMode::default(), KsRotaryMode::Unknown);
    assert_eq!(KsJoystickMode::default(), KsJoystickMode::Unknown);
    let snapshot = KsReportSnapshot::default();
    assert_eq!(snapshot.tick, 0);
    assert_eq!(snapshot.buttons, [0u8; KS_BUTTON_BYTES]);
    assert_eq!(snapshot.encoders, [0i16; KS_ENCODER_COUNT]);
}

// ── Proptest ────────────────────────────────────────────────────────────

use proptest::prelude::*;

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(256))]

    #[test]
    fn prop_axis_source_u16_round_trips(lo in 0u8..=255u8, hi in 0u8..=255u8) {
        let src = KsAxisSource::new(0, false);
        let expected = u16::from_le_bytes([lo, hi]);
        prop_assert_eq!(src.parse_u16(&[lo, hi]), Some(expected));
    }

    #[test]
    fn prop_axis_source_i16_round_trips(lo in 0u8..=255u8, hi in 0u8..=255u8) {
        let src = KsAxisSource::new(0, true);
        let expected = i16::from_le_bytes([lo, hi]);
        prop_assert_eq!(src.parse_i16(&[lo, hi]), Some(expected));
    }

    #[test]
    fn prop_bit_source_non_inverted(byte: u8, bit in 0u8..8u8) {
        let mask = 1u8 << bit;
        let src = KsBitSource::new(0, mask);
        let expected = byte & mask != 0;
        prop_assert_eq!(src.parse(&[byte]), Some(expected));
    }

    #[test]
    fn prop_bit_source_inverted_is_opposite(byte: u8, bit in 0u8..8u8) {
        let mask = 1u8 << bit;
        let normal = KsBitSource::new(0, mask);
        let inverted = KsBitSource::inverted(0, mask);
        prop_assert_eq!(normal.parse(&[byte]).map(|v| !v), inverted.parse(&[byte]));
    }

    #[test]
    fn prop_byte_source_matches_index(data in proptest::collection::vec(any::<u8>(), 1..=16)) {
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
        prop_assert!(map.parse(tick, &data).is_some());
    }
}

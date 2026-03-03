#![allow(clippy::redundant_closure)]

use racing_wheel_hbp::{
    parse_axis, parse_hbp_usb_report_best_effort, HbpHandbrakeSample, HbpHandbrakeSampleRaw,
    RAW_AXIS_START, RAW_BUTTON, WITH_REPORT_ID_AXIS_START, WITH_REPORT_ID_BUTTON,
};

type R = Result<(), Box<dyn std::error::Error>>;

// ── Round-trip: prefixed layout ──────────────────────────────────────────

#[test]
fn round_trip_prefixed_layout() -> R {
    let axis: u16 = 0xBEEF;
    let btn: u8 = 0x42;
    let le = axis.to_le_bytes();
    let report = [0x01, le[0], le[1], btn];
    let parsed =
        parse_hbp_usb_report_best_effort(&report).ok_or("prefixed layout should parse")?;
    assert_eq!(parsed.handbrake, axis);
    assert_eq!(parsed.button_byte, Some(btn));

    let reconstructed = [
        0x01,
        parsed.handbrake.to_le_bytes()[0],
        parsed.handbrake.to_le_bytes()[1],
        parsed.button_byte.ok_or("expected button byte")?,
    ];
    assert_eq!(report, reconstructed);
    Ok(())
}

// ── Round-trip: raw two-byte layout ─────────────────────────────────────

#[test]
fn round_trip_raw_two_byte() -> R {
    let axis: u16 = 0xCAFE;
    let le = axis.to_le_bytes();
    let report = [le[0], le[1]];
    let parsed =
        parse_hbp_usb_report_best_effort(&report).ok_or("two-byte layout should parse")?;
    assert_eq!(parsed.handbrake, axis);
    assert_eq!(parsed.button_byte, None);
    assert_eq!(parsed.handbrake.to_le_bytes(), report);
    Ok(())
}

// ── Round-trip: raw three-byte layout ───────────────────────────────────

#[test]
fn round_trip_raw_three_byte_with_button() -> R {
    let report = [0x00u8, 0xAA, 0x55];
    let parsed =
        parse_hbp_usb_report_best_effort(&report).ok_or("three-byte raw should parse")?;
    assert_eq!(parsed.handbrake, u16::from_le_bytes([0x00, 0xAA]));
    assert_eq!(parsed.button_byte, Some(0x55));
    Ok(())
}

// ── Edge cases ──────────────────────────────────────────────────────────

#[test]
fn empty_report_returns_none() {
    assert!(parse_hbp_usb_report_best_effort(&[]).is_none());
}

#[test]
fn single_byte_returns_none() {
    assert!(parse_hbp_usb_report_best_effort(&[0x01]).is_none());
}

#[test]
fn oversized_report_still_parses_prefixed() -> R {
    let report = [0x05u8, 0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70];
    let parsed = parse_hbp_usb_report_best_effort(&report)
        .ok_or("oversized prefixed should still parse")?;
    assert_eq!(parsed.handbrake, u16::from_le_bytes([0x10, 0x20]));
    assert_eq!(parsed.button_byte, Some(0x30));
    Ok(())
}

#[test]
fn all_zeros_four_bytes_uses_raw_path() -> R {
    let report = [0x00u8, 0x00, 0x00, 0x00];
    let parsed =
        parse_hbp_usb_report_best_effort(&report).ok_or("all-zero four-byte should parse")?;
    assert_eq!(parsed.handbrake, 0);
    assert_eq!(parsed.button_byte, Some(0x00));
    Ok(())
}

#[test]
fn all_ff_four_bytes_uses_prefixed_path() -> R {
    let report = [0xFFu8, 0xFF, 0xFF, 0xFF];
    let parsed =
        parse_hbp_usb_report_best_effort(&report).ok_or("all-0xFF four-byte should parse")?;
    assert_eq!(parsed.handbrake, u16::MAX);
    assert_eq!(parsed.button_byte, Some(0xFF));
    Ok(())
}

// ── parse_axis edge cases ───────────────────────────────────────────────

#[test]
fn parse_axis_empty_returns_none() {
    assert_eq!(parse_axis(&[], 0), None);
}

#[test]
fn parse_axis_single_byte_returns_none() {
    assert_eq!(parse_axis(&[0xFF], 0), None);
}

#[test]
fn parse_axis_oob_offset_returns_none() {
    assert_eq!(parse_axis(&[0x00, 0x00], 1), None);
}

#[test]
fn parse_axis_usize_max_returns_none() {
    assert_eq!(parse_axis(&[0x00, 0x00], usize::MAX), None);
}

#[test]
fn parse_axis_boundary_values() -> R {
    let zero = parse_axis(&[0x00, 0x00], 0).ok_or("zero parse")?;
    assert_eq!(zero, 0u16);
    let max = parse_axis(&[0xFF, 0xFF], 0).ok_or("max parse")?;
    assert_eq!(max, u16::MAX);
    Ok(())
}

// ── Normalization ───────────────────────────────────────────────────────

#[test]
fn normalize_zero() {
    let n = HbpHandbrakeSampleRaw {
        handbrake: 0,
        button_byte: None,
    }
    .normalize();
    assert!(n.handbrake.abs() < f32::EPSILON);
}

#[test]
fn normalize_max() {
    let n = HbpHandbrakeSampleRaw {
        handbrake: u16::MAX,
        button_byte: None,
    }
    .normalize();
    assert!((n.handbrake - 1.0).abs() < f32::EPSILON);
}

#[test]
fn normalize_midpoint() {
    let n = HbpHandbrakeSampleRaw {
        handbrake: 32768,
        button_byte: Some(0x01),
    }
    .normalize();
    assert!((n.handbrake - (32768.0 / 65535.0)).abs() < 0.00002);
    assert_eq!(n.button_byte, Some(0x01));
}

#[test]
fn normalize_monotonic() {
    let values = [0u16, 1, 256, 32767, 32768, 65534, 65535];
    let normalized: Vec<f32> = values
        .iter()
        .map(|&v| {
            HbpHandbrakeSampleRaw {
                handbrake: v,
                button_byte: None,
            }
            .normalize()
            .handbrake
        })
        .collect();
    for pair in normalized.windows(2) {
        assert!(pair[0] <= pair[1]);
    }
}

// ── Derive trait smoke tests ────────────────────────────────────────────

#[test]
fn sample_raw_debug_clone_eq() {
    let a = HbpHandbrakeSampleRaw {
        handbrake: 1234,
        button_byte: Some(0x01),
    };
    let b = a;
    assert_eq!(a, b);
    let _ = format!("{a:?}");
}

#[test]
fn sample_normalized_debug_clone() {
    let a = HbpHandbrakeSample {
        handbrake: 0.5,
        button_byte: None,
    };
    let b = a;
    assert_eq!(b.button_byte, None);
    let _ = format!("{a:?}");
}

// ── Constant offset consistency ─────────────────────────────────────────

#[test]
fn constant_offsets_are_consistent() {
    assert_eq!(WITH_REPORT_ID_AXIS_START, 1);
    assert_eq!(WITH_REPORT_ID_BUTTON, 3);
    assert_eq!(RAW_AXIS_START, 0);
    assert_eq!(RAW_BUTTON, 2);
    assert_eq!(WITH_REPORT_ID_BUTTON, WITH_REPORT_ID_AXIS_START + 2);
    assert_eq!(RAW_BUTTON, RAW_AXIS_START + 2);
}

// ── Proptest ────────────────────────────────────────────────────────────

use proptest::prelude::*;

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(256))]

    #[test]
    fn prop_parse_axis_round_trips(lo in 0u8..=255u8, hi in 0u8..=255u8) {
        let expected = u16::from_le_bytes([lo, hi]);
        prop_assert_eq!(parse_axis(&[lo, hi], 0), Some(expected));
    }

    #[test]
    fn prop_normalize_within_unit_range(value: u16) {
        let n = HbpHandbrakeSampleRaw { handbrake: value, button_byte: None }.normalize();
        prop_assert!(n.handbrake >= 0.0);
        prop_assert!(n.handbrake <= 1.0);
    }

    #[test]
    fn prop_two_byte_always_parses(lo in 0u8..=255u8, hi in 0u8..=255u8) {
        let parsed = parse_hbp_usb_report_best_effort(&[lo, hi]);
        prop_assert!(parsed.is_some());
    }

    #[test]
    fn prop_four_byte_nonzero_id_prefixed(
        id in 1u8..=255u8,
        lo in 0u8..=255u8,
        hi in 0u8..=255u8,
        btn in 0u8..=255u8,
    ) {
        let report = [id, lo, hi, btn];
        let parsed = parse_hbp_usb_report_best_effort(&report);
        prop_assert!(parsed.is_some());
        if let Some(s) = parsed {
            prop_assert_eq!(s.handbrake, u16::from_le_bytes([lo, hi]));
            prop_assert_eq!(s.button_byte, Some(btn));
        }
    }

    #[test]
    fn prop_raw_two_byte_round_trip(value: u16) {
        let le = value.to_le_bytes();
        let parsed = parse_hbp_usb_report_best_effort(&le);
        prop_assert!(parsed.is_some());
        if let Some(s) = parsed {
            prop_assert_eq!(s.handbrake, value);
            prop_assert_eq!(s.handbrake.to_le_bytes(), le);
        }
    }

    #[test]
    fn prop_normalize_preserves_button_byte(value: u16, btn: u8) {
        let raw = HbpHandbrakeSampleRaw { handbrake: value, button_byte: Some(btn) };
        let norm = raw.normalize();
        prop_assert_eq!(norm.button_byte, Some(btn));
    }
}

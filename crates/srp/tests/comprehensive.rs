#![allow(clippy::redundant_closure)]

use racing_wheel_srp::{
    parse_axis, parse_srp_usb_report_best_effort, SrpPedalAxes, SrpPedalAxesRaw, BRAKE_START,
    MIN_REPORT_LEN, THROTTLE_START,
};

type R = Result<(), Box<dyn std::error::Error>>;

// ── Basic parsing ───────────────────────────────────────────────────────

#[test]
fn parse_throttle_and_brake() -> R {
    let report = [0x01u8, 0xFF, 0xFF, 0x00, 0x80];
    let axes =
        parse_srp_usb_report_best_effort(&report).ok_or("throttle/brake should parse")?;
    assert_eq!(axes.throttle, 0xFFFF);
    assert_eq!(axes.brake, Some(0x8000));
    Ok(())
}

#[test]
fn parse_exact_min_length() -> R {
    let report = [0x00, 0x34, 0x12, 0x78, 0x56];
    let axes =
        parse_srp_usb_report_best_effort(&report).ok_or("exact min length should parse")?;
    assert_eq!(axes.throttle, 0x1234);
    assert_eq!(axes.brake, Some(0x5678));
    Ok(())
}

// ── Edge cases ──────────────────────────────────────────────────────────

#[test]
fn short_report_returns_none() {
    assert!(parse_srp_usb_report_best_effort(&[]).is_none());
    assert!(parse_srp_usb_report_best_effort(&[0x01]).is_none());
    assert!(parse_srp_usb_report_best_effort(&[0x01, 0xFF]).is_none());
    assert!(parse_srp_usb_report_best_effort(&[0x01, 0xFF, 0xFF]).is_none());
    assert!(parse_srp_usb_report_best_effort(&[0x01, 0xFF, 0xFF, 0x00]).is_none());
}

#[test]
fn all_zeros() -> R {
    let report = [0x00u8; MIN_REPORT_LEN];
    let axes = parse_srp_usb_report_best_effort(&report).ok_or("all-zero should parse")?;
    assert_eq!(axes.throttle, 0);
    assert_eq!(axes.brake, Some(0));
    Ok(())
}

#[test]
fn all_ff() -> R {
    let report = [0xFF; MIN_REPORT_LEN];
    let axes = parse_srp_usb_report_best_effort(&report).ok_or("all-0xFF should parse")?;
    assert_eq!(axes.throttle, u16::MAX);
    assert_eq!(axes.brake, Some(u16::MAX));
    Ok(())
}

#[test]
fn oversized_report_still_parses() -> R {
    let report = [0x01, 0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70];
    let axes = parse_srp_usb_report_best_effort(&report).ok_or("oversized should parse")?;
    assert_eq!(axes.throttle, u16::from_le_bytes([0x10, 0x20]));
    assert_eq!(axes.brake, Some(u16::from_le_bytes([0x30, 0x40])));
    Ok(())
}

// ── Round-trip encoding ─────────────────────────────────────────────────

#[test]
fn round_trip_encoding() -> R {
    let throttle: u16 = 0xBEEF;
    let brake: u16 = 0xCAFE;
    let mut report = [0u8; MIN_REPORT_LEN];
    report[0] = 0x01;
    report[THROTTLE_START..THROTTLE_START + 2].copy_from_slice(&throttle.to_le_bytes());
    report[BRAKE_START..BRAKE_START + 2].copy_from_slice(&brake.to_le_bytes());

    let parsed = parse_srp_usb_report_best_effort(&report).ok_or("round trip")?;
    assert_eq!(parsed.throttle, throttle);
    assert_eq!(parsed.brake, Some(brake));
    Ok(())
}

// ── parse_axis ──────────────────────────────────────────────────────────

#[test]
fn parse_axis_empty() {
    assert_eq!(parse_axis(&[], 0), None);
}

#[test]
fn parse_axis_single_byte() {
    assert_eq!(parse_axis(&[0xFF], 0), None);
}

#[test]
fn parse_axis_oob() {
    assert_eq!(parse_axis(&[0x00, 0x00], 1), None);
}

#[test]
fn parse_axis_usize_max() {
    assert_eq!(parse_axis(&[0x00, 0x00], usize::MAX), None);
}

#[test]
fn parse_axis_valid() -> R {
    let value = 0xABCDu16;
    let bytes = value.to_le_bytes();
    let parsed = parse_axis(&bytes, 0).ok_or("valid axis")?;
    assert_eq!(parsed, value);
    Ok(())
}

#[test]
fn parse_axis_boundary() -> R {
    assert_eq!(parse_axis(&[0x00, 0x00], 0).ok_or("zero")?, 0u16);
    assert_eq!(parse_axis(&[0xFF, 0xFF], 0).ok_or("max")?, u16::MAX);
    Ok(())
}

// ── Normalization ───────────────────────────────────────────────────────

#[test]
fn normalize_max_throttle() {
    let raw = SrpPedalAxesRaw {
        throttle: u16::MAX,
        brake: None,
    };
    let norm = raw.normalize();
    assert!((norm.throttle - 1.0).abs() < 0.00002);
    assert_eq!(norm.brake, None);
}

#[test]
fn normalize_zero_throttle() {
    let raw = SrpPedalAxesRaw {
        throttle: 0,
        brake: Some(0),
    };
    let norm = raw.normalize();
    assert!(norm.throttle.abs() < f32::EPSILON);
    assert!((norm.brake.unwrap_or(1.0)).abs() < f32::EPSILON);
}

#[test]
fn normalize_with_brake() -> R {
    let raw = SrpPedalAxesRaw {
        throttle: 65535,
        brake: Some(32768),
    };
    let norm = raw.normalize();
    assert!((norm.throttle - 1.0).abs() < 0.00002);
    let brake = norm.brake.ok_or("expected brake")?;
    assert!((brake - (32768.0 / 65535.0)).abs() < 0.00002);
    Ok(())
}

#[test]
fn normalize_preserves_none_brake() {
    let raw = SrpPedalAxesRaw {
        throttle: 1000,
        brake: None,
    };
    assert_eq!(raw.normalize().brake, None);
}

// ── Derive trait smoke tests ────────────────────────────────────────────

#[test]
fn raw_debug_clone_eq() {
    let a = SrpPedalAxesRaw {
        throttle: 100,
        brake: Some(200),
    };
    let b = a;
    assert_eq!(a, b);
    let _ = format!("{a:?}");
}

#[test]
fn normalized_debug_clone() {
    let a = SrpPedalAxes {
        throttle: 0.5,
        brake: Some(0.25),
    };
    let b = a;
    assert_eq!(b.throttle, a.throttle);
    let _ = format!("{a:?}");
}

// ── Constant consistency ────────────────────────────────────────────────

#[test]
fn constants_consistent() {
    assert_eq!(THROTTLE_START, 1);
    assert_eq!(BRAKE_START, 3);
    assert_eq!(MIN_REPORT_LEN, 5);
    assert_eq!(MIN_REPORT_LEN, BRAKE_START + 2);
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
    fn prop_normalize_throttle_in_unit_range(value: u16) {
        let raw = SrpPedalAxesRaw { throttle: value, brake: None };
        let norm = raw.normalize();
        prop_assert!(norm.throttle >= 0.0);
        prop_assert!(norm.throttle <= 1.0);
    }

    #[test]
    fn prop_normalize_brake_in_unit_range(value: u16) {
        let raw = SrpPedalAxesRaw { throttle: 0, brake: Some(value) };
        let norm = raw.normalize();
        if let Some(b) = norm.brake {
            prop_assert!(b >= 0.0);
            prop_assert!(b <= 1.0);
        }
    }

    #[test]
    fn prop_full_report_round_trip(
        id in 0u8..=255u8,
        t_lo in 0u8..=255u8,
        t_hi in 0u8..=255u8,
        b_lo in 0u8..=255u8,
        b_hi in 0u8..=255u8,
    ) {
        let report = [id, t_lo, t_hi, b_lo, b_hi];
        let parsed = parse_srp_usb_report_best_effort(&report);
        prop_assert!(parsed.is_some());
        if let Some(axes) = parsed {
            prop_assert_eq!(axes.throttle, u16::from_le_bytes([t_lo, t_hi]));
            prop_assert_eq!(axes.brake, Some(u16::from_le_bytes([b_lo, b_hi])));
        }
    }

    #[test]
    fn prop_short_reports_always_none(len in 0usize..MIN_REPORT_LEN) {
        let report = vec![0xFFu8; len];
        prop_assert!(parse_srp_usb_report_best_effort(&report).is_none());
    }

    #[test]
    fn prop_normalize_preserves_brake_option(
        throttle: u16,
        brake: u16,
    ) {
        let with = SrpPedalAxesRaw { throttle, brake: Some(brake) }.normalize();
        let without = SrpPedalAxesRaw { throttle, brake: None }.normalize();
        prop_assert!(with.brake.is_some());
        prop_assert!(without.brake.is_none());
    }
}

//! Deep tests for the HBP (Handbrake Protocol) micro-crate.
//!
//! Covers: message encoding/decoding, layout inference, axis parsing,
//! normalization, sequence/boundary values, and error handling.

use racing_wheel_hbp::{
    HbpHandbrakeSample, HbpHandbrakeSampleRaw, RAW_AXIS_START, RAW_BUTTON,
    WITH_REPORT_ID_AXIS_START, WITH_REPORT_ID_BUTTON, parse_axis, parse_hbp_usb_report_best_effort,
};

type R = Result<(), Box<dyn std::error::Error>>;

// ═══════════════════════════════════════════════════════════════════════
// § Message encoding / decoding — layout inference
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn prefixed_layout_every_report_id_value() -> R {
    // Every non-zero first byte should trigger the prefixed layout on a 4-byte report.
    for id in 1u8..=255 {
        let report = [id, 0xCD, 0xAB, 0x77];
        let parsed = parse_hbp_usb_report_best_effort(&report)
            .ok_or(format!("id={id:#04x}: prefixed layout should parse"))?;
        assert_eq!(parsed.handbrake, 0xABCD, "id={id:#04x}: axis mismatch");
        assert_eq!(
            parsed.button_byte,
            Some(0x77),
            "id={id:#04x}: button mismatch"
        );
    }
    Ok(())
}

#[test]
fn zero_report_id_falls_through_to_raw_on_four_bytes() -> R {
    let report = [0x00, 0xFE, 0xCA, 0x99];
    let parsed =
        parse_hbp_usb_report_best_effort(&report).ok_or("zero id four-byte should use raw path")?;
    // Raw layout: axis at [0..2], button at [2]
    assert_eq!(parsed.handbrake, u16::from_le_bytes([0x00, 0xFE]));
    assert_eq!(parsed.button_byte, Some(0xCA));
    Ok(())
}

#[test]
fn three_byte_nonzero_first_uses_raw_layout() -> R {
    // 3-byte report: report.len() > WITH_REPORT_ID_BUTTON is false (3 > 3 is false),
    // so the prefixed branch is skipped; raw 3-byte path is used.
    let report = [0xAA, 0xBB, 0xCC];
    let parsed = parse_hbp_usb_report_best_effort(&report).ok_or("3-byte should parse as raw")?;
    assert_eq!(parsed.handbrake, u16::from_le_bytes([0xAA, 0xBB]));
    assert_eq!(parsed.button_byte, Some(0xCC));
    Ok(())
}

#[test]
fn encoding_decoding_le_byte_order_verification() -> R {
    // Verify little-endian: low byte first, high byte second.
    let report = [0x01, 0x78, 0x56, 0x00]; // prefixed layout
    let parsed =
        parse_hbp_usb_report_best_effort(&report).ok_or("LE byte order test should parse")?;
    // 0x78 is low byte, 0x56 is high byte → 0x5678
    assert_eq!(parsed.handbrake, 0x5678);

    let report_raw = [0x78, 0x56]; // raw two-byte
    let parsed_raw = parse_hbp_usb_report_best_effort(&report_raw)
        .ok_or("LE byte order raw test should parse")?;
    assert_eq!(parsed_raw.handbrake, 0x5678);
    Ok(())
}

#[test]
fn encoding_round_trip_all_layouts() -> R {
    let test_values: &[(u16, Option<u8>)] = &[
        (0x0000, None),
        (0xFFFF, None),
        (0x8000, Some(0x00)),
        (0x0001, Some(0xFF)),
        (0x1234, Some(0x56)),
    ];

    for &(axis, btn) in test_values {
        let le = axis.to_le_bytes();

        // Two-byte raw layout (button is dropped)
        let raw2 = [le[0], le[1]];
        let p2 = parse_hbp_usb_report_best_effort(&raw2)
            .ok_or(format!("raw2 failed for axis={axis:#06x}"))?;
        assert_eq!(p2.handbrake, axis);
        assert_eq!(p2.button_byte, None);

        // Three-byte raw layout
        if let Some(b) = btn {
            let raw3 = [le[0], le[1], b];
            let p3 = parse_hbp_usb_report_best_effort(&raw3)
                .ok_or(format!("raw3 failed for axis={axis:#06x}"))?;
            assert_eq!(p3.handbrake, axis);
            assert_eq!(p3.button_byte, Some(b));
        }

        // Four-byte prefixed layout
        if let Some(b) = btn {
            let prefixed = [0x01, le[0], le[1], b];
            let pp = parse_hbp_usb_report_best_effort(&prefixed)
                .ok_or(format!("prefixed failed for axis={axis:#06x}"))?;
            assert_eq!(pp.handbrake, axis);
            assert_eq!(pp.button_byte, Some(b));
        }
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// § Sequence numbers / boundary axis values
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn sequential_axis_values_decode_correctly() -> R {
    // Verify a monotonically increasing sequence of axis values.
    let sequence: [u16; 8] = [0, 1, 255, 256, 1000, 32767, 32768, 65535];
    for &val in &sequence {
        let le = val.to_le_bytes();
        let parsed = parse_hbp_usb_report_best_effort(&le)
            .ok_or(format!("sequence value {val} should parse"))?;
        assert_eq!(parsed.handbrake, val, "value {val} mismatch");
    }
    Ok(())
}

#[test]
fn axis_value_one_encodes_correctly() -> R {
    let report = [0x01, 0x00]; // LE → 0x0001
    let parsed = parse_hbp_usb_report_best_effort(&report).ok_or("axis=1 should parse")?;
    assert_eq!(parsed.handbrake, 1);
    Ok(())
}

#[test]
fn axis_value_byte_boundary_256() -> R {
    let report = [0x00, 0x01]; // LE → 0x0100 = 256
    let parsed = parse_hbp_usb_report_best_effort(&report).ok_or("axis=256 should parse")?;
    assert_eq!(parsed.handbrake, 256);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// § Timeout / error handling — rejections
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn empty_report_returns_none() {
    assert!(parse_hbp_usb_report_best_effort(&[]).is_none());
}

#[test]
fn single_byte_report_returns_none() {
    for b in [0x00, 0x01, 0x7F, 0xFF] {
        assert!(
            parse_hbp_usb_report_best_effort(&[b]).is_none(),
            "single byte {b:#04x} should return None"
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════
// § parse_axis — edge cases and error handling
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn parse_axis_at_various_offsets() -> R {
    let data = [0x11, 0x22, 0x33, 0x44, 0x55];
    assert_eq!(parse_axis(&data, 0), Some(u16::from_le_bytes([0x11, 0x22])));
    assert_eq!(parse_axis(&data, 1), Some(u16::from_le_bytes([0x22, 0x33])));
    assert_eq!(parse_axis(&data, 2), Some(u16::from_le_bytes([0x33, 0x44])));
    assert_eq!(parse_axis(&data, 3), Some(u16::from_le_bytes([0x44, 0x55])));
    assert_eq!(parse_axis(&data, 4), None); // only one byte left
    assert_eq!(parse_axis(&data, 5), None); // out of bounds
    Ok(())
}

#[test]
fn parse_axis_usize_max_offset_does_not_panic() {
    assert_eq!(parse_axis(&[0xFF; 100], usize::MAX), None);
}

#[test]
fn parse_axis_usize_max_minus_one_offset_does_not_panic() {
    assert_eq!(parse_axis(&[0xFF; 100], usize::MAX - 1), None);
}

#[test]
fn parse_axis_exact_fit() -> R {
    // Exactly 2 bytes starting at offset 0
    let val = parse_axis(&[0xAB, 0xCD], 0).ok_or("exact fit should parse")?;
    assert_eq!(val, 0xCDAB);
    Ok(())
}

#[test]
fn parse_axis_offset_just_past_end() {
    let data = [0x00, 0x00, 0x00];
    // offset 2: needs bytes at [2] and [3], but only [2] exists
    assert_eq!(parse_axis(&data, 2), None);
}

// ═══════════════════════════════════════════════════════════════════════
// § Normalization — precision, range, and edge cases
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn normalize_zero_is_exactly_zero() {
    let n = HbpHandbrakeSampleRaw {
        handbrake: 0,
        button_byte: None,
    }
    .normalize();
    assert_eq!(n.handbrake, 0.0);
}

#[test]
fn normalize_max_is_exactly_one() {
    let n = HbpHandbrakeSampleRaw {
        handbrake: u16::MAX,
        button_byte: None,
    }
    .normalize();
    assert!((n.handbrake - 1.0).abs() < f32::EPSILON);
}

#[test]
fn normalize_one_is_very_small() {
    let n = HbpHandbrakeSampleRaw {
        handbrake: 1,
        button_byte: None,
    }
    .normalize();
    let expected = 1.0_f32 / 65535.0;
    assert!((n.handbrake - expected).abs() < f32::EPSILON);
}

#[test]
fn normalize_midpoint_precision() {
    let n = HbpHandbrakeSampleRaw {
        handbrake: 32768,
        button_byte: None,
    }
    .normalize();
    let expected = 32768.0_f32 / 65535.0;
    assert!((n.handbrake - expected).abs() < 1e-5);
}

#[test]
fn normalize_is_strictly_monotonic_for_distinct_inputs() {
    // Verify strict monotonicity over a range of samples
    let step = 257u32; // cover the range in ~255 steps
    let mut prev = -1.0_f32;
    let mut val = 0u32;
    while val <= u16::MAX as u32 {
        let n = HbpHandbrakeSampleRaw {
            handbrake: val as u16,
            button_byte: None,
        }
        .normalize()
        .handbrake;
        assert!(
            n > prev || val == 0,
            "not strictly monotonic at {val}: {prev} >= {n}"
        );
        prev = n;
        val += step;
    }
}

#[test]
fn normalize_preserves_button_byte_some() {
    let raw = HbpHandbrakeSampleRaw {
        handbrake: 5000,
        button_byte: Some(0xAB),
    };
    let n = raw.normalize();
    assert_eq!(n.button_byte, Some(0xAB));
}

#[test]
fn normalize_preserves_button_byte_none() {
    let raw = HbpHandbrakeSampleRaw {
        handbrake: 5000,
        button_byte: None,
    };
    assert_eq!(raw.normalize().button_byte, None);
}

#[test]
fn normalize_result_always_within_unit_interval() {
    // Spot-check powers of two
    for shift in 0..16 {
        let val = 1u16 << shift;
        let n = HbpHandbrakeSampleRaw {
            handbrake: val,
            button_byte: None,
        }
        .normalize();
        assert!(n.handbrake >= 0.0, "negative at {val}");
        assert!(n.handbrake <= 1.0, "exceeds 1.0 at {val}");
    }
}

// ═══════════════════════════════════════════════════════════════════════
// § Derive trait verification
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn raw_sample_copy_semantics() {
    let a = HbpHandbrakeSampleRaw {
        handbrake: 42,
        button_byte: Some(1),
    };
    let b = a; // Copy
    let c = a; // still valid — Copy
    assert_eq!(b, c);
    assert_eq!(a, b);
}

#[test]
fn raw_sample_ne() {
    let a = HbpHandbrakeSampleRaw {
        handbrake: 1,
        button_byte: None,
    };
    let b = HbpHandbrakeSampleRaw {
        handbrake: 2,
        button_byte: None,
    };
    assert_ne!(a, b);
}

#[test]
fn raw_sample_ne_button_byte() {
    let a = HbpHandbrakeSampleRaw {
        handbrake: 100,
        button_byte: Some(0),
    };
    let b = HbpHandbrakeSampleRaw {
        handbrake: 100,
        button_byte: Some(1),
    };
    assert_ne!(a, b);
}

#[test]
fn raw_sample_ne_button_some_vs_none() {
    let a = HbpHandbrakeSampleRaw {
        handbrake: 100,
        button_byte: Some(0),
    };
    let b = HbpHandbrakeSampleRaw {
        handbrake: 100,
        button_byte: None,
    };
    assert_ne!(a, b);
}

#[test]
fn normalized_sample_debug_format() {
    let s = HbpHandbrakeSample {
        handbrake: 0.5,
        button_byte: Some(0xFF),
    };
    let dbg = format!("{s:?}");
    assert!(dbg.contains("0.5"));
    assert!(dbg.contains("255")); // 0xFF = 255
}

#[test]
fn raw_sample_debug_format() {
    let s = HbpHandbrakeSampleRaw {
        handbrake: 1234,
        button_byte: None,
    };
    let dbg = format!("{s:?}");
    assert!(dbg.contains("1234"));
    assert!(dbg.contains("None"));
}

// ═══════════════════════════════════════════════════════════════════════
// § Layout selection edge cases — longer reports
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn five_byte_report_prefixed_ignores_trailing() -> R {
    let report = [0x03, 0xDE, 0xAD, 0xBE, 0xEF];
    let parsed = parse_hbp_usb_report_best_effort(&report).ok_or("5-byte prefixed should parse")?;
    assert_eq!(parsed.handbrake, 0xADDE);
    assert_eq!(parsed.button_byte, Some(0xBE));
    Ok(())
}

#[test]
fn eight_byte_report_uses_prefixed() -> R {
    let report = [0x01, 0x00, 0x80, 0xFF, 0x00, 0x00, 0x00, 0x00];
    let parsed = parse_hbp_usb_report_best_effort(&report).ok_or("8-byte prefixed should parse")?;
    assert_eq!(parsed.handbrake, 0x8000);
    assert_eq!(parsed.button_byte, Some(0xFF));
    Ok(())
}

#[test]
fn large_report_with_zero_id_uses_raw() -> R {
    let mut report = [0u8; 64];
    report[0] = 0x00;
    report[1] = 0xAA;
    report[2] = 0xBB;
    let parsed = parse_hbp_usb_report_best_effort(&report).ok_or("large zero-id should use raw")?;
    // Raw: axis at [0..2] = [0x00, 0xAA], button at [2] = 0xBB
    assert_eq!(parsed.handbrake, u16::from_le_bytes([0x00, 0xAA]));
    assert_eq!(parsed.button_byte, Some(0xBB));
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// § Constant offset consistency
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn constant_offsets_document_layout_correctly() {
    // Prefixed: [report_id, axis_lo, axis_hi, button]
    assert_eq!(WITH_REPORT_ID_AXIS_START, 1);
    assert_eq!(WITH_REPORT_ID_BUTTON, 3);
    // Raw: [axis_lo, axis_hi, button]
    assert_eq!(RAW_AXIS_START, 0);
    assert_eq!(RAW_BUTTON, 2);

    // Axis is always 2 bytes wide; button follows immediately
    assert_eq!(WITH_REPORT_ID_BUTTON - WITH_REPORT_ID_AXIS_START, 2);
    assert_eq!(RAW_BUTTON - RAW_AXIS_START, 2);
}

// ═══════════════════════════════════════════════════════════════════════
// § Proptest — deep property-based coverage
// ═══════════════════════════════════════════════════════════════════════

use proptest::prelude::*;

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(512))]

    #[test]
    fn prop_prefixed_layout_always_returns_correct_axis_and_button(
        id in 1u8..=255u8,
        lo in 0u8..=255u8,
        hi in 0u8..=255u8,
        btn in 0u8..=255u8,
    ) {
        let report = [id, lo, hi, btn];
        let parsed = parse_hbp_usb_report_best_effort(&report);
        prop_assert!(parsed.is_some(), "4-byte nonzero-id must parse");
        if let Some(s) = parsed {
            prop_assert_eq!(s.handbrake, u16::from_le_bytes([lo, hi]));
            prop_assert_eq!(s.button_byte, Some(btn));
        }
    }

    #[test]
    fn prop_two_byte_layout_always_parses(value: u16) {
        let le = value.to_le_bytes();
        let parsed = parse_hbp_usb_report_best_effort(&le);
        prop_assert!(parsed.is_some(), "2-byte must always parse");
        if let Some(s) = parsed {
            prop_assert_eq!(s.handbrake, value);
            prop_assert_eq!(s.button_byte, None);
        }
    }

    #[test]
    fn prop_three_byte_raw_always_parses(
        lo in 0u8..=255u8,
        hi in 0u8..=255u8,
        btn in 0u8..=255u8,
    ) {
        let report = [lo, hi, btn];
        let parsed = parse_hbp_usb_report_best_effort(&report);
        prop_assert!(parsed.is_some(), "3-byte must always parse");
        if let Some(s) = parsed {
            prop_assert_eq!(s.handbrake, u16::from_le_bytes([lo, hi]));
            prop_assert_eq!(s.button_byte, Some(btn));
        }
    }

    #[test]
    fn prop_normalize_output_bounded(value: u16) {
        let n = HbpHandbrakeSampleRaw { handbrake: value, button_byte: None }.normalize();
        prop_assert!(n.handbrake >= 0.0);
        prop_assert!(n.handbrake <= 1.0);
    }

    #[test]
    fn prop_normalize_button_byte_passthrough(value: u16, btn: u8) {
        let raw = HbpHandbrakeSampleRaw { handbrake: value, button_byte: Some(btn) };
        let n = raw.normalize();
        prop_assert_eq!(n.button_byte, Some(btn));
    }

    #[test]
    fn prop_parse_axis_agrees_with_from_le_bytes(lo in 0u8..=255u8, hi in 0u8..=255u8) {
        let expected = u16::from_le_bytes([lo, hi]);
        prop_assert_eq!(parse_axis(&[lo, hi], 0), Some(expected));
    }

    #[test]
    fn prop_parse_axis_with_prefix(
        prefix in 0u8..=255u8,
        lo in 0u8..=255u8,
        hi in 0u8..=255u8,
    ) {
        let buf = [prefix, lo, hi];
        let expected = u16::from_le_bytes([lo, hi]);
        prop_assert_eq!(parse_axis(&buf, 1), Some(expected));
    }

    #[test]
    fn prop_parse_axis_oob_never_panics(
        len in 0usize..=16usize,
        start in 0usize..=20usize,
    ) {
        let buf = vec![0xAA; len];
        let result = parse_axis(&buf, start);
        if start.checked_add(2).is_some_and(|end| end <= len) {
            prop_assert!(result.is_some());
        } else {
            prop_assert!(result.is_none());
        }
    }

    #[test]
    fn prop_raw_round_trip_reconstruction(value: u16) {
        let le = value.to_le_bytes();
        if let Some(s) = parse_hbp_usb_report_best_effort(&le) {
            let reconstructed = s.handbrake.to_le_bytes();
            prop_assert_eq!(le, reconstructed);
        }
    }

    #[test]
    fn prop_normalize_monotonic_pair(a: u16, b: u16) {
        let na = HbpHandbrakeSampleRaw { handbrake: a, button_byte: None }.normalize().handbrake;
        let nb = HbpHandbrakeSampleRaw { handbrake: b, button_byte: None }.normalize().handbrake;
        if a <= b {
            prop_assert!(na <= nb, "normalize({a}) = {na} > normalize({b}) = {nb}");
        } else {
            prop_assert!(na >= nb, "normalize({a}) = {na} < normalize({b}) = {nb}");
        }
    }
}

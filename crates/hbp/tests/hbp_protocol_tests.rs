//! Comprehensive protocol-level tests for the HBP (handbrake protocol) micro-crate.
//!
//! Covers: encoding/decoding roundtrips, sequence/timing boundary values,
//! timeout-style detection via None returns, overflow and reset edge cases,
//! concurrent-style multi-layout parsing, and proptest fuzzing.

use racing_wheel_hbp::{
    HbpHandbrakeSample, HbpHandbrakeSampleRaw, RAW_AXIS_START, RAW_BUTTON,
    WITH_REPORT_ID_AXIS_START, WITH_REPORT_ID_BUTTON, parse_axis, parse_hbp_usb_report_best_effort,
};

type R = Result<(), Box<dyn std::error::Error>>;

// ═══════════════════════════════════════════════════════════════════════
// § Encoding / Decoding roundtrips
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn roundtrip_prefixed_layout_reconstructs_original_bytes() -> R {
    let axis: u16 = 0xDEAD;
    let btn: u8 = 0x7F;
    let le = axis.to_le_bytes();
    let report = [0x05, le[0], le[1], btn];

    let parsed =
        parse_hbp_usb_report_best_effort(&report).ok_or("prefixed roundtrip should parse")?;
    assert_eq!(parsed.handbrake, axis);
    assert_eq!(parsed.button_byte, Some(btn));

    let reconstructed = [
        0x05,
        parsed.handbrake.to_le_bytes()[0],
        parsed.handbrake.to_le_bytes()[1],
        parsed
            .button_byte
            .ok_or("expected button byte in reconstruction")?,
    ];
    assert_eq!(report, reconstructed);
    Ok(())
}

#[test]
fn roundtrip_raw_two_byte_layout() -> R {
    for &axis in &[0u16, 1, 255, 256, 32767, 32768, 65534, u16::MAX] {
        let le = axis.to_le_bytes();
        let parsed =
            parse_hbp_usb_report_best_effort(&le).ok_or(format!("raw2 axis={axis} parse"))?;
        assert_eq!(parsed.handbrake, axis);
        assert_eq!(parsed.button_byte, None);
        assert_eq!(parsed.handbrake.to_le_bytes(), le);
    }
    Ok(())
}

#[test]
fn roundtrip_raw_three_byte_with_button() -> R {
    let axis: u16 = 0x1234;
    let btn: u8 = 0xAB;
    let le = axis.to_le_bytes();
    let report = [le[0], le[1], btn];

    let parsed = parse_hbp_usb_report_best_effort(&report).ok_or("raw3 roundtrip should parse")?;
    assert_eq!(parsed.handbrake, axis);
    assert_eq!(parsed.button_byte, Some(btn));
    Ok(())
}

#[test]
fn roundtrip_every_report_id_prefixed() -> R {
    // Every non-zero first byte triggers the prefixed layout on 4-byte reports.
    let axis: u16 = 0x9876;
    let btn: u8 = 0x01;
    let le = axis.to_le_bytes();
    for id in 1u8..=255 {
        let report = [id, le[0], le[1], btn];
        let parsed = parse_hbp_usb_report_best_effort(&report)
            .ok_or(format!("prefixed id={id:#04x} should parse"))?;
        assert_eq!(parsed.handbrake, axis, "axis mismatch for id={id:#04x}");
        assert_eq!(
            parsed.button_byte,
            Some(btn),
            "button mismatch for id={id:#04x}"
        );
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// § Timing / sequence number handling — sequential axis values
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn sequential_axis_values_are_monotonically_decoded() -> R {
    let sequence: Vec<u16> = (0..=65535u32).step_by(4096).map(|v| v as u16).collect();
    let mut prev_val: Option<u16> = None;
    for &val in &sequence {
        let le = val.to_le_bytes();
        let parsed =
            parse_hbp_usb_report_best_effort(&le).ok_or(format!("seq value {val} should parse"))?;
        assert_eq!(parsed.handbrake, val);
        if let Some(pv) = prev_val {
            assert!(
                parsed.handbrake >= pv,
                "sequence not monotonic: {pv} > {val}"
            );
        }
        prev_val = Some(parsed.handbrake);
    }
    Ok(())
}

#[test]
fn rapid_sequence_from_zero_to_max() -> R {
    // Simulates a quick full-range sweep like a handbrake pull
    let values = [0u16, 100, 500, 2000, 10000, 30000, 50000, 65535];
    for &v in &values {
        let le = v.to_le_bytes();
        let parsed =
            parse_hbp_usb_report_best_effort(&le).ok_or(format!("sweep value {v} parse"))?;
        assert_eq!(parsed.handbrake, v);
    }
    Ok(())
}

#[test]
fn sequence_number_wrap_around_pattern() -> R {
    // Simulates axis values cycling near u16 boundary
    let values = [65530u16, 65533, 65535, 0, 1, 5, 100];
    for &v in &values {
        let le = v.to_le_bytes();
        let parsed =
            parse_hbp_usb_report_best_effort(&le).ok_or(format!("wrap-around {v} parse"))?;
        assert_eq!(parsed.handbrake, v);
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// § Timeout detection — None/rejection as timeout signal
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn timeout_empty_report_is_none() {
    // An empty buffer signals "no data received" / timeout
    assert!(parse_hbp_usb_report_best_effort(&[]).is_none());
}

#[test]
fn timeout_single_byte_is_none_for_all_values() {
    // A truncated single-byte report signals incomplete/timeout
    for b in 0u8..=255 {
        assert!(
            parse_hbp_usb_report_best_effort(&[b]).is_none(),
            "single byte {b:#04x} should return None"
        );
    }
}

#[test]
fn timeout_detection_via_successive_none_results() {
    // Simulate: good report → timeout (empty) → good report
    let good = [0x01u8, 0xAA, 0xBB, 0xCC];
    let timeout: &[u8] = &[];

    assert!(parse_hbp_usb_report_best_effort(&good).is_some());
    assert!(parse_hbp_usb_report_best_effort(timeout).is_none());
    assert!(parse_hbp_usb_report_best_effort(&good).is_some());
}

// ═══════════════════════════════════════════════════════════════════════
// § Edge cases: overflow, reset, concurrent heartbeat patterns
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn overflow_u16_max_axis_prefixed() -> R {
    let report = [0x01, 0xFF, 0xFF, 0x00];
    let parsed = parse_hbp_usb_report_best_effort(&report).ok_or("u16::MAX prefixed")?;
    assert_eq!(parsed.handbrake, u16::MAX);
    Ok(())
}

#[test]
fn overflow_u16_max_axis_raw() -> R {
    let report = [0xFF, 0xFF];
    let parsed = parse_hbp_usb_report_best_effort(&report).ok_or("u16::MAX raw")?;
    assert_eq!(parsed.handbrake, u16::MAX);
    Ok(())
}

#[test]
fn reset_zero_axis_prefixed() -> R {
    let report = [0x01, 0x00, 0x00, 0x00];
    let parsed = parse_hbp_usb_report_best_effort(&report).ok_or("zero prefixed")?;
    assert_eq!(parsed.handbrake, 0);
    assert_eq!(parsed.button_byte, Some(0x00));
    Ok(())
}

#[test]
fn concurrent_heartbeat_different_layouts_independent() -> R {
    // Parse two "concurrent" reports from different layout types —
    // each should be independent.
    let prefixed = [0x02, 0xCD, 0xAB, 0x77];
    let raw2 = [0xEF, 0xBE];
    let raw3 = [0x12, 0x34, 0x56];

    let p1 = parse_hbp_usb_report_best_effort(&prefixed).ok_or("concurrent prefixed")?;
    let p2 = parse_hbp_usb_report_best_effort(&raw2).ok_or("concurrent raw2")?;
    let p3 = parse_hbp_usb_report_best_effort(&raw3).ok_or("concurrent raw3")?;

    assert_eq!(p1.handbrake, 0xABCD);
    assert_eq!(p1.button_byte, Some(0x77));
    assert_eq!(p2.handbrake, 0xBEEF);
    assert_eq!(p2.button_byte, None);
    assert_eq!(p3.handbrake, 0x3412);
    assert_eq!(p3.button_byte, Some(0x56));
    Ok(())
}

#[test]
fn alternating_layouts_no_cross_contamination() -> R {
    // Interleave layouts and ensure no state leakage
    let reports: &[&[u8]] = &[
        &[0xFF, 0xFF],             // raw 2-byte → max
        &[0x01, 0x00, 0x00, 0x42], // prefixed → 0, btn=0x42
        &[0xAA, 0xBB, 0xCC],       // raw 3-byte
        &[0x01, 0x00, 0x00, 0x42], // same prefixed again
    ];
    let expected: &[(u16, Option<u8>)] = &[
        (u16::MAX, None),
        (0x0000, Some(0x42)),
        (0xBBAA, Some(0xCC)),
        (0x0000, Some(0x42)),
    ];

    for (report, &(axis, btn)) in reports.iter().zip(expected) {
        let parsed = parse_hbp_usb_report_best_effort(report)
            .ok_or(format!("alternating layout for report {report:?}"))?;
        assert_eq!(parsed.handbrake, axis);
        assert_eq!(parsed.button_byte, btn);
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// § Normalization edge cases
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn normalize_adjacent_values_no_equality_collapse() {
    // Verify that normalization doesn't collapse adjacent u16 values into the same f32.
    // (f32 has enough precision for distinct u16 values in [0, 65535])
    let a = HbpHandbrakeSampleRaw {
        handbrake: 32767,
        button_byte: None,
    }
    .normalize()
    .handbrake;
    let b = HbpHandbrakeSampleRaw {
        handbrake: 32768,
        button_byte: None,
    }
    .normalize()
    .handbrake;
    assert!(a < b, "adjacent values collapsed: {a} == {b}");
}

#[test]
fn normalize_powers_of_two_within_unit_range() {
    for shift in 0..16u32 {
        let val = 1u16 << shift;
        let n = HbpHandbrakeSampleRaw {
            handbrake: val,
            button_byte: None,
        }
        .normalize();
        assert!(n.handbrake >= 0.0, "negative at shift={shift}");
        assert!(n.handbrake <= 1.0, "exceeds 1.0 at shift={shift}");
    }
}

#[test]
fn normalize_endpoint_exactness() {
    let zero = HbpHandbrakeSampleRaw {
        handbrake: 0,
        button_byte: None,
    }
    .normalize();
    assert_eq!(zero.handbrake, 0.0);

    let max = HbpHandbrakeSampleRaw {
        handbrake: u16::MAX,
        button_byte: None,
    }
    .normalize();
    assert!((max.handbrake - 1.0).abs() < f32::EPSILON);
}

#[test]
fn normalized_sample_partial_eq_symmetry() {
    let a = HbpHandbrakeSample {
        handbrake: 0.5,
        button_byte: Some(0x01),
    };
    let b = HbpHandbrakeSample {
        handbrake: 0.5,
        button_byte: Some(0x01),
    };
    assert_eq!(a, b);
    assert_eq!(b, a);
}

// ═══════════════════════════════════════════════════════════════════════
// § parse_axis — thorough edge cases
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn parse_axis_sliding_window_over_buffer() -> R {
    let data = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66];
    let expected_at_offsets: &[(usize, Option<u16>)] = &[
        (0, Some(u16::from_le_bytes([0x11, 0x22]))),
        (1, Some(u16::from_le_bytes([0x22, 0x33]))),
        (2, Some(u16::from_le_bytes([0x33, 0x44]))),
        (3, Some(u16::from_le_bytes([0x44, 0x55]))),
        (4, Some(u16::from_le_bytes([0x55, 0x66]))),
        (5, None), // only 1 byte left
        (6, None), // out of bounds
    ];
    for &(offset, expected) in expected_at_offsets {
        assert_eq!(
            parse_axis(&data, offset),
            expected,
            "offset={offset} mismatch"
        );
    }
    Ok(())
}

#[test]
fn parse_axis_saturating_add_prevents_panic_at_usize_max() {
    // usize::MAX + 2 would overflow; saturating_add prevents panic
    assert_eq!(parse_axis(&[0xFF; 256], usize::MAX), None);
    assert_eq!(parse_axis(&[0xFF; 256], usize::MAX - 1), None);
}

#[test]
fn parse_axis_large_valid_offset() -> R {
    let mut buf = vec![0u8; 1000];
    buf[998] = 0xAB;
    buf[999] = 0xCD;
    let val = parse_axis(&buf, 998).ok_or("large offset parse")?;
    assert_eq!(val, 0xCDAB);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// § Layout discrimination edge cases
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn three_byte_report_nonzero_first_byte_uses_raw() -> R {
    // 3 bytes: report.len() > WITH_REPORT_ID_BUTTON (3 > 3 = false)
    // so prefixed branch is skipped, falls to raw 3-byte path.
    let report = [0xFF, 0x12, 0x34];
    let parsed = parse_hbp_usb_report_best_effort(&report).ok_or("3-byte raw nonzero first")?;
    assert_eq!(parsed.handbrake, u16::from_le_bytes([0xFF, 0x12]));
    assert_eq!(parsed.button_byte, Some(0x34));
    Ok(())
}

#[test]
fn four_byte_zero_first_falls_through_to_raw() -> R {
    let report = [0x00, 0xAB, 0xCD, 0xEF];
    let parsed =
        parse_hbp_usb_report_best_effort(&report).ok_or("4-byte zero-first raw fallthrough")?;
    // Raw: axis=[0x00, 0xAB], button=0xCD
    assert_eq!(parsed.handbrake, u16::from_le_bytes([0x00, 0xAB]));
    assert_eq!(parsed.button_byte, Some(0xCD));
    Ok(())
}

#[test]
fn oversized_reports_always_parse() -> R {
    for len in 4..=64 {
        let mut report = vec![0x01u8; len];
        report[0] = 0x05; // nonzero ID
        report[1] = 0xAA;
        report[2] = 0xBB;
        report[3] = 0xCC;
        let parsed = parse_hbp_usb_report_best_effort(&report)
            .ok_or(format!("oversized len={len} should parse"))?;
        assert_eq!(parsed.handbrake, 0xBBAA);
        assert_eq!(parsed.button_byte, Some(0xCC));
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// § Byte-order verification
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn le_byte_order_low_byte_first() -> R {
    // 0x5678: low=0x78, high=0x56
    let report = [0x78, 0x56];
    let parsed = parse_hbp_usb_report_best_effort(&report).ok_or("LE order verify")?;
    assert_eq!(parsed.handbrake, 0x5678);
    Ok(())
}

#[test]
fn le_byte_order_prefixed() -> R {
    let report = [0x01, 0x78, 0x56, 0x00];
    let parsed = parse_hbp_usb_report_best_effort(&report).ok_or("LE order prefixed")?;
    assert_eq!(parsed.handbrake, 0x5678);
    assert_eq!(report[WITH_REPORT_ID_AXIS_START], 0x78); // low byte
    assert_eq!(report[WITH_REPORT_ID_AXIS_START + 1], 0x56); // high byte
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// § Constant offset verification
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn constant_offsets_encode_layout_correctly() {
    // Prefixed: [report_id, axis_lo, axis_hi, button]
    assert_eq!(WITH_REPORT_ID_AXIS_START, 1);
    assert_eq!(WITH_REPORT_ID_BUTTON, WITH_REPORT_ID_AXIS_START + 2);
    assert_eq!(WITH_REPORT_ID_BUTTON, 3);

    // Raw: [axis_lo, axis_hi, button]
    assert_eq!(RAW_AXIS_START, 0);
    assert_eq!(RAW_BUTTON, RAW_AXIS_START + 2);
    assert_eq!(RAW_BUTTON, 2);
}

// ═══════════════════════════════════════════════════════════════════════
// § Proptest fuzzing
// ═══════════════════════════════════════════════════════════════════════

use proptest::prelude::*;

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(1024))]

    #[test]
    fn prop_any_two_byte_report_always_parses(lo in 0u8..=255u8, hi in 0u8..=255u8) {
        let report = [lo, hi];
        let parsed = parse_hbp_usb_report_best_effort(&report);
        prop_assert!(parsed.is_some(), "2-byte must always parse");
        if let Some(s) = parsed {
            prop_assert_eq!(s.handbrake, u16::from_le_bytes([lo, hi]));
            prop_assert_eq!(s.button_byte, None);
        }
    }

    #[test]
    fn prop_any_three_byte_report_always_parses(
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
    fn prop_four_byte_nonzero_id_uses_prefixed(
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
    fn prop_four_byte_zero_id_uses_raw(
        lo in 0u8..=255u8,
        hi in 0u8..=255u8,
        b2 in 0u8..=255u8,
        b3 in 0u8..=255u8,
    ) {
        let report = [0x00, lo, hi, b2];
        let _ = b3; // unused — we only need 4 bytes total
        let parsed = parse_hbp_usb_report_best_effort(&report);
        prop_assert!(parsed.is_some());
        if let Some(s) = parsed {
            // Raw: axis at [0..2], button at [2]
            prop_assert_eq!(s.handbrake, u16::from_le_bytes([0x00, lo]));
            prop_assert_eq!(s.button_byte, Some(hi));
        }
    }

    #[test]
    fn prop_roundtrip_two_byte_reconstruction(value: u16) {
        let le = value.to_le_bytes();
        if let Some(s) = parse_hbp_usb_report_best_effort(&le) {
            prop_assert_eq!(s.handbrake, value);
            prop_assert_eq!(s.handbrake.to_le_bytes(), le);
        }
    }

    #[test]
    fn prop_normalize_always_bounded(value: u16) {
        let n = HbpHandbrakeSampleRaw { handbrake: value, button_byte: None }.normalize();
        prop_assert!(n.handbrake >= 0.0);
        prop_assert!(n.handbrake <= 1.0);
    }

    #[test]
    fn prop_normalize_monotonic_pair(a: u16, b: u16) {
        let na = HbpHandbrakeSampleRaw { handbrake: a, button_byte: None }.normalize().handbrake;
        let nb = HbpHandbrakeSampleRaw { handbrake: b, button_byte: None }.normalize().handbrake;
        if a <= b {
            prop_assert!(na <= nb, "normalize({a})={na} > normalize({b})={nb}");
        } else {
            prop_assert!(na >= nb, "normalize({a})={na} < normalize({b})={nb}");
        }
    }

    #[test]
    fn prop_normalize_preserves_button_byte(value: u16, btn: u8) {
        let raw = HbpHandbrakeSampleRaw { handbrake: value, button_byte: Some(btn) };
        prop_assert_eq!(raw.normalize().button_byte, Some(btn));
    }

    #[test]
    fn prop_parse_axis_agrees_with_from_le(lo in 0u8..=255u8, hi in 0u8..=255u8) {
        prop_assert_eq!(parse_axis(&[lo, hi], 0), Some(u16::from_le_bytes([lo, hi])));
    }

    #[test]
    fn prop_parse_axis_oob_never_panics(len in 0usize..=32usize, start in 0usize..=40usize) {
        let buf = vec![0xCC; len];
        let result = parse_axis(&buf, start);
        if start.checked_add(2).is_some_and(|end| end <= len) {
            prop_assert!(result.is_some());
        } else {
            prop_assert!(result.is_none());
        }
    }

    #[test]
    fn prop_arbitrary_report_never_panics(len in 0usize..=128usize) {
        let buf = vec![0xAA; len];
        // Must not panic, regardless of length
        let _ = parse_hbp_usb_report_best_effort(&buf);
    }
}

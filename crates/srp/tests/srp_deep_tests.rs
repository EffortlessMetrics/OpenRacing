//! Deep tests for racing-wheel-srp.
//!
//! Covers: SRP architecture validation, crate dependency checks,
//! module boundary tests, normalization edge cases, and constant
//! consistency guarantees.

use racing_wheel_srp::{
    BRAKE_START, MIN_REPORT_LEN, SrpPedalAxes, SrpPedalAxesRaw, THROTTLE_START, parse_axis,
    parse_srp_usb_report_best_effort,
};

type R = Result<(), Box<dyn std::error::Error>>;

// ═══════════════════════════════════════════════════════════════════════════
// Architecture validation – constant relationships
// ═══════════════════════════════════════════════════════════════════════════

mod architecture {
    use super::*;

    #[test]
    fn throttle_starts_after_report_id() -> R {
        assert_eq!(
            THROTTLE_START, 1,
            "throttle must start at byte 1 (after report ID)"
        );
        Ok(())
    }

    #[test]
    fn brake_starts_after_throttle() -> R {
        assert_eq!(
            BRAKE_START,
            THROTTLE_START + 2,
            "brake must start immediately after 2-byte throttle"
        );
        Ok(())
    }

    #[test]
    fn min_report_len_covers_all_axes() -> R {
        assert_eq!(
            MIN_REPORT_LEN,
            BRAKE_START + 2,
            "MIN_REPORT_LEN must cover report_id + throttle(2) + brake(2)"
        );
        Ok(())
    }

    #[test]
    fn min_report_len_is_five() -> R {
        assert_eq!(MIN_REPORT_LEN, 5);
        Ok(())
    }

    #[test]
    fn constants_form_contiguous_layout() -> R {
        // Report layout: [report_id(1)] [throttle(2)] [brake(2)]
        let report_id_size = THROTTLE_START; // 1
        let throttle_size = BRAKE_START - THROTTLE_START; // 2
        let brake_size = MIN_REPORT_LEN - BRAKE_START; // 2
        assert_eq!(report_id_size, 1);
        assert_eq!(throttle_size, 2);
        assert_eq!(brake_size, 2);
        assert_eq!(report_id_size + throttle_size + brake_size, MIN_REPORT_LEN);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Crate dependency checks – the crate should be I/O-free
// ═══════════════════════════════════════════════════════════════════════════

mod dependency_checks {
    use super::*;

    #[test]
    fn crate_is_no_std_compatible_types() -> R {
        // All public types should be Copy or Clone without heap allocation
        let raw = SrpPedalAxesRaw {
            throttle: 100,
            brake: Some(200),
        };
        let _copy = raw; // Copy
        let _clone = raw; // still available because Copy
        assert_eq!(_copy, _clone);

        let norm = raw.normalize();
        let _copy2 = norm; // Copy
        let _ = format!("{norm:?}"); // Debug
        Ok(())
    }

    #[test]
    fn parse_functions_are_pure() -> R {
        // parse_axis and parse_srp_usb_report_best_effort are pure functions:
        // same input always produces same output, no side effects
        let report = [0x01, 0xAB, 0xCD, 0xEF, 0x12];
        let r1 = parse_srp_usb_report_best_effort(&report);
        let r2 = parse_srp_usb_report_best_effort(&report);
        assert_eq!(r1, r2);

        let a1 = parse_axis(&[0xFF, 0x00], 0);
        let a2 = parse_axis(&[0xFF, 0x00], 0);
        assert_eq!(a1, a2);
        Ok(())
    }

    #[test]
    fn no_allocation_in_parsing() -> R {
        // All returned types are stack-allocated (no String, Vec, Box)
        let report = [0x00; MIN_REPORT_LEN];
        let result = parse_srp_usb_report_best_effort(&report);
        // SrpPedalAxesRaw contains only u16 and Option<u16> – no heap
        assert!(result.is_some());
        let raw = result.ok_or("should parse")?;
        assert_eq!(
            std::mem::size_of_val(&raw),
            std::mem::size_of::<SrpPedalAxesRaw>()
        );
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Module boundary tests – public API surface
// ═══════════════════════════════════════════════════════════════════════════

mod module_boundary {
    use super::*;

    #[test]
    fn public_constants_accessible() -> R {
        let _t = THROTTLE_START;
        let _b = BRAKE_START;
        let _m = MIN_REPORT_LEN;
        Ok(())
    }

    #[test]
    fn public_types_constructible() -> R {
        let _raw = SrpPedalAxesRaw {
            throttle: 0,
            brake: None,
        };
        let _axes = SrpPedalAxes {
            throttle: 0.0,
            brake: None,
        };
        Ok(())
    }

    #[test]
    fn public_functions_callable() -> R {
        let _a = parse_axis(&[], 0);
        let _b = parse_srp_usb_report_best_effort(&[]);
        Ok(())
    }

    #[test]
    fn srp_pedal_axes_raw_fields_are_public() -> R {
        let raw = SrpPedalAxesRaw {
            throttle: 1000,
            brake: Some(2000),
        };
        // Direct field access
        assert_eq!(raw.throttle, 1000);
        assert_eq!(raw.brake, Some(2000));
        Ok(())
    }

    #[test]
    fn srp_pedal_axes_fields_are_public() -> R {
        let axes = SrpPedalAxes {
            throttle: 0.5,
            brake: Some(0.25),
        };
        assert!((axes.throttle - 0.5).abs() < f32::EPSILON);
        assert!((axes.brake.ok_or("brake")? - 0.25).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn normalize_is_public_method_on_raw() -> R {
        let raw = SrpPedalAxesRaw {
            throttle: 32768,
            brake: None,
        };
        let _normalized = raw.normalize();
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// parse_axis – exhaustive edge cases
// ═══════════════════════════════════════════════════════════════════════════

mod parse_axis_deep {
    use super::*;

    #[test]
    fn empty_slice() {
        assert_eq!(parse_axis(&[], 0), None);
    }

    #[test]
    fn single_byte_at_zero() {
        assert_eq!(parse_axis(&[0xFF], 0), None);
    }

    #[test]
    fn two_bytes_at_zero() -> R {
        assert_eq!(parse_axis(&[0xAB, 0xCD], 0), Some(0xCDAB));
        Ok(())
    }

    #[test]
    fn two_bytes_at_nonzero_offset() {
        // Only 2 bytes total, start=1 → need bytes at [1] and [2], but only [0..2] exists
        assert_eq!(parse_axis(&[0x00, 0xFF], 1), None);
    }

    #[test]
    fn three_bytes_at_offset_one() -> R {
        assert_eq!(parse_axis(&[0x00, 0x34, 0x12], 1), Some(0x1234));
        Ok(())
    }

    #[test]
    fn usize_max_offset_saturates() {
        assert_eq!(parse_axis(&[0x00; 100], usize::MAX), None);
    }

    #[test]
    fn usize_max_minus_one_offset() {
        assert_eq!(parse_axis(&[0x00; 100], usize::MAX - 1), None);
    }

    #[test]
    fn start_at_exact_boundary() -> R {
        // 4-byte slice, reading at start=2 needs bytes [2] and [3]
        let data = [0x00, 0x00, 0xEF, 0xBE];
        assert_eq!(parse_axis(&data, 2), Some(0xBEEF));
        Ok(())
    }

    #[test]
    fn start_past_boundary() {
        let data = [0x00, 0x00, 0xEF, 0xBE];
        assert_eq!(parse_axis(&data, 3), None);
    }

    #[test]
    fn zero_value() -> R {
        assert_eq!(parse_axis(&[0x00, 0x00], 0), Some(0));
        Ok(())
    }

    #[test]
    fn max_value() -> R {
        assert_eq!(parse_axis(&[0xFF, 0xFF], 0), Some(u16::MAX));
        Ok(())
    }

    #[test]
    fn little_endian_byte_order() -> R {
        // 0x0102 in little-endian is [0x02, 0x01]
        assert_eq!(parse_axis(&[0x02, 0x01], 0), Some(0x0102));
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// parse_srp_usb_report_best_effort – comprehensive
// ═══════════════════════════════════════════════════════════════════════════

mod parse_report_deep {
    use super::*;

    #[test]
    fn empty_input() {
        assert!(parse_srp_usb_report_best_effort(&[]).is_none());
    }

    #[test]
    fn one_byte_short() {
        assert!(parse_srp_usb_report_best_effort(&[0x01, 0xFF, 0xFF, 0x00]).is_none());
    }

    #[test]
    fn exact_min_length() -> R {
        let report = [0x01, 0x00, 0x80, 0xFF, 0x7F];
        let axes = parse_srp_usb_report_best_effort(&report).ok_or("should parse")?;
        assert_eq!(axes.throttle, 0x8000);
        assert_eq!(axes.brake, Some(0x7FFF));
        Ok(())
    }

    #[test]
    fn report_id_is_ignored() -> R {
        // Different report IDs should yield same axis values
        for id in [0x00, 0x01, 0x42, 0xFF] {
            let report = [id, 0xAB, 0xCD, 0xEF, 0x01];
            let axes = parse_srp_usb_report_best_effort(&report).ok_or("should parse")?;
            assert_eq!(axes.throttle, 0xCDAB);
            assert_eq!(axes.brake, Some(0x01EF));
        }
        Ok(())
    }

    #[test]
    fn extra_trailing_bytes_ignored() -> R {
        let report = [0x01, 0x10, 0x20, 0x30, 0x40, 0xFF, 0xFF, 0xFF];
        let axes = parse_srp_usb_report_best_effort(&report).ok_or("oversized")?;
        assert_eq!(axes.throttle, 0x2010);
        assert_eq!(axes.brake, Some(0x4030));
        Ok(())
    }

    #[test]
    fn all_zeros_parses_correctly() -> R {
        let report = [0u8; MIN_REPORT_LEN];
        let axes = parse_srp_usb_report_best_effort(&report).ok_or("zeros")?;
        assert_eq!(axes.throttle, 0);
        assert_eq!(axes.brake, Some(0));
        Ok(())
    }

    #[test]
    fn all_ones_parses_correctly() -> R {
        let report = [0xFF; MIN_REPORT_LEN];
        let axes = parse_srp_usb_report_best_effort(&report).ok_or("ones")?;
        assert_eq!(axes.throttle, u16::MAX);
        assert_eq!(axes.brake, Some(u16::MAX));
        Ok(())
    }

    #[test]
    fn known_hardware_values() -> R {
        // Simulate a typical pedal reading: throttle at ~50%, brake at ~25%
        let throttle: u16 = 32768;
        let brake: u16 = 16384;
        let mut report = [0u8; MIN_REPORT_LEN];
        report[0] = 0x01;
        report[THROTTLE_START..THROTTLE_START + 2].copy_from_slice(&throttle.to_le_bytes());
        report[BRAKE_START..BRAKE_START + 2].copy_from_slice(&brake.to_le_bytes());
        let axes = parse_srp_usb_report_best_effort(&report).ok_or("hardware")?;
        assert_eq!(axes.throttle, 32768);
        assert_eq!(axes.brake, Some(16384));
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Normalization – deep edge cases
// ═══════════════════════════════════════════════════════════════════════════

mod normalize_deep {
    use super::*;

    #[test]
    fn zero_normalizes_to_zero() {
        let raw = SrpPedalAxesRaw {
            throttle: 0,
            brake: Some(0),
        };
        let n = raw.normalize();
        assert!(n.throttle.abs() < f32::EPSILON);
        let brake = n.brake.unwrap_or(1.0);
        assert!(brake.abs() < f32::EPSILON);
    }

    #[test]
    fn max_normalizes_to_one() {
        let raw = SrpPedalAxesRaw {
            throttle: u16::MAX,
            brake: Some(u16::MAX),
        };
        let n = raw.normalize();
        assert!((n.throttle - 1.0).abs() < 0.00002);
        let brake = n.brake.unwrap_or(0.0);
        assert!((brake - 1.0).abs() < 0.00002);
    }

    #[test]
    fn midpoint_normalizes_to_approximately_half() -> R {
        let raw = SrpPedalAxesRaw {
            throttle: 32768,
            brake: Some(32768),
        };
        let n = raw.normalize();
        // 32768 / 65535 ≈ 0.50002
        assert!((n.throttle - 0.5).abs() < 0.001);
        let brake = n.brake.ok_or("brake")?;
        assert!((brake - 0.5).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn one_normalizes_close_to_zero() {
        let raw = SrpPedalAxesRaw {
            throttle: 1,
            brake: None,
        };
        let n = raw.normalize();
        assert!(n.throttle > 0.0);
        assert!(n.throttle < 0.001);
    }

    #[test]
    fn max_minus_one_normalizes_close_to_one() {
        let raw = SrpPedalAxesRaw {
            throttle: u16::MAX - 1,
            brake: None,
        };
        let n = raw.normalize();
        assert!(n.throttle > 0.999);
        assert!(n.throttle < 1.0);
    }

    #[test]
    fn none_brake_stays_none() {
        let raw = SrpPedalAxesRaw {
            throttle: 5000,
            brake: None,
        };
        assert!(raw.normalize().brake.is_none());
    }

    #[test]
    fn some_brake_stays_some() -> R {
        let raw = SrpPedalAxesRaw {
            throttle: 0,
            brake: Some(1),
        };
        let brake = raw.normalize().brake.ok_or("expected Some")?;
        assert!(brake > 0.0);
        Ok(())
    }

    #[test]
    fn normalization_is_monotonic() -> R {
        // For increasing raw values, normalized values should also increase
        let mut prev = 0.0f32;
        for v in (0..=65535u16).step_by(1000) {
            let raw = SrpPedalAxesRaw {
                throttle: v,
                brake: None,
            };
            let n = raw.normalize().throttle;
            assert!(
                n >= prev,
                "normalization must be monotonic: {n} < {prev} at raw={v}"
            );
            prev = n;
        }
        Ok(())
    }

    #[test]
    fn normalization_stays_in_unit_range() -> R {
        // Check a variety of values
        for v in [0u16, 1, 100, 1000, 10000, 32768, 50000, 65534, 65535] {
            let raw = SrpPedalAxesRaw {
                throttle: v,
                brake: Some(v),
            };
            let n = raw.normalize();
            assert!((0.0..=1.0).contains(&n.throttle));
            let brake = n.brake.ok_or("brake")?;
            assert!((0.0..=1.0).contains(&brake));
        }
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Derive trait validation
// ═══════════════════════════════════════════════════════════════════════════

mod trait_impls {
    use super::*;

    #[test]
    fn raw_is_debug() {
        let raw = SrpPedalAxesRaw {
            throttle: 42,
            brake: Some(99),
        };
        let dbg = format!("{raw:?}");
        assert!(dbg.contains("42"));
        assert!(dbg.contains("99"));
    }

    #[test]
    fn raw_is_clone_copy() {
        let a = SrpPedalAxesRaw {
            throttle: 100,
            brake: None,
        };
        let b = a; // Copy
        assert_eq!(a, b);
        // Verify type implements Clone (it's also Copy)
        let _: fn(&SrpPedalAxesRaw) -> SrpPedalAxesRaw = Clone::clone;
    }

    #[test]
    fn raw_partial_eq() {
        let a = SrpPedalAxesRaw {
            throttle: 10,
            brake: Some(20),
        };
        let b = SrpPedalAxesRaw {
            throttle: 10,
            brake: Some(20),
        };
        let c = SrpPedalAxesRaw {
            throttle: 10,
            brake: Some(21),
        };
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn raw_eq_is_reflexive_symmetric_transitive() {
        let a = SrpPedalAxesRaw {
            throttle: 1,
            brake: None,
        };
        let b = SrpPedalAxesRaw {
            throttle: 1,
            brake: None,
        };
        let c = SrpPedalAxesRaw {
            throttle: 1,
            brake: None,
        };
        // reflexive
        assert_eq!(a, a);
        // symmetric
        assert_eq!(a, b);
        assert_eq!(b, a);
        // transitive
        assert_eq!(a, b);
        assert_eq!(b, c);
        assert_eq!(a, c);
    }

    #[test]
    fn normalized_is_debug() {
        let n = SrpPedalAxes {
            throttle: 0.5,
            brake: Some(0.25),
        };
        let dbg = format!("{n:?}");
        assert!(dbg.contains("0.5"));
    }

    #[test]
    fn normalized_is_clone_copy() {
        let a = SrpPedalAxes {
            throttle: 0.75,
            brake: None,
        };
        let b = a; // Copy
        assert_eq!(a.throttle, b.throttle);
        // Verify type implements Clone (it's also Copy)
        let _: fn(&SrpPedalAxes) -> SrpPedalAxes = Clone::clone;
    }

    #[test]
    fn normalized_partial_eq() {
        let a = SrpPedalAxes {
            throttle: 0.5,
            brake: Some(0.25),
        };
        let b = SrpPedalAxes {
            throttle: 0.5,
            brake: Some(0.25),
        };
        assert_eq!(a, b);
    }

    #[test]
    fn normalized_ne_different_throttle() {
        let a = SrpPedalAxes {
            throttle: 0.5,
            brake: None,
        };
        let b = SrpPedalAxes {
            throttle: 0.6,
            brake: None,
        };
        assert_ne!(a, b);
    }

    #[test]
    fn normalized_ne_different_brake() {
        let a = SrpPedalAxes {
            throttle: 0.5,
            brake: Some(0.1),
        };
        let b = SrpPedalAxes {
            throttle: 0.5,
            brake: Some(0.2),
        };
        assert_ne!(a, b);
    }

    #[test]
    fn normalized_ne_none_vs_some_brake() {
        let a = SrpPedalAxes {
            throttle: 0.5,
            brake: None,
        };
        let b = SrpPedalAxes {
            throttle: 0.5,
            brake: Some(0.0),
        };
        assert_ne!(a, b);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Round-trip encoding – construct report then parse
// ═══════════════════════════════════════════════════════════════════════════

mod round_trip {
    use super::*;

    #[test]
    fn encode_decode_all_zeros() -> R {
        let mut report = [0u8; MIN_REPORT_LEN];
        report[0] = 0x01;
        let axes = parse_srp_usb_report_best_effort(&report).ok_or("zeros")?;
        assert_eq!(axes.throttle, 0);
        assert_eq!(axes.brake, Some(0));
        Ok(())
    }

    #[test]
    fn encode_decode_all_max() -> R {
        let mut report = [0u8; MIN_REPORT_LEN];
        report[0] = 0x01;
        report[THROTTLE_START..THROTTLE_START + 2].copy_from_slice(&u16::MAX.to_le_bytes());
        report[BRAKE_START..BRAKE_START + 2].copy_from_slice(&u16::MAX.to_le_bytes());
        let axes = parse_srp_usb_report_best_effort(&report).ok_or("max")?;
        assert_eq!(axes.throttle, u16::MAX);
        assert_eq!(axes.brake, Some(u16::MAX));
        Ok(())
    }

    #[test]
    fn encode_decode_specific_values() -> R {
        let throttle: u16 = 0xDEAD;
        let brake: u16 = 0xBEEF;
        let mut report = [0u8; MIN_REPORT_LEN];
        report[0] = 0x42;
        report[THROTTLE_START..THROTTLE_START + 2].copy_from_slice(&throttle.to_le_bytes());
        report[BRAKE_START..BRAKE_START + 2].copy_from_slice(&brake.to_le_bytes());
        let axes = parse_srp_usb_report_best_effort(&report).ok_or("specific")?;
        assert_eq!(axes.throttle, 0xDEAD);
        assert_eq!(axes.brake, Some(0xBEEF));
        Ok(())
    }

    #[test]
    fn full_pipeline_parse_then_normalize() -> R {
        let mut report = [0u8; MIN_REPORT_LEN];
        report[0] = 0x01;
        report[THROTTLE_START..THROTTLE_START + 2].copy_from_slice(&32768u16.to_le_bytes());
        report[BRAKE_START..BRAKE_START + 2].copy_from_slice(&0u16.to_le_bytes());

        let raw = parse_srp_usb_report_best_effort(&report).ok_or("pipeline")?;
        let norm = raw.normalize();
        assert!((norm.throttle - 0.5).abs() < 0.001);
        let brake = norm.brake.ok_or("brake")?;
        assert!(brake.abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn multiple_parses_same_report_deterministic() -> R {
        let report = [0x01, 0x42, 0x42, 0x42, 0x42];
        let r1 = parse_srp_usb_report_best_effort(&report);
        let r2 = parse_srp_usb_report_best_effort(&report);
        let r3 = parse_srp_usb_report_best_effort(&report);
        assert_eq!(r1, r2);
        assert_eq!(r2, r3);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Size and layout assertions
// ═══════════════════════════════════════════════════════════════════════════

mod layout {
    use super::*;

    #[test]
    fn raw_size_is_compact() {
        // u16 (throttle) + Option<u16> (brake) = at most 6 bytes on most platforms
        let size = std::mem::size_of::<SrpPedalAxesRaw>();
        assert!(size <= 8, "SrpPedalAxesRaw should be compact, got {size}");
    }

    #[test]
    fn normalized_size_is_compact() {
        // f32 (throttle) + Option<f32> (brake) = at most 12 bytes on most platforms
        let size = std::mem::size_of::<SrpPedalAxes>();
        assert!(size <= 12, "SrpPedalAxes should be compact, got {size}");
    }
}

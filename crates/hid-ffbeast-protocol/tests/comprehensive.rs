//! Comprehensive tests for the FFBeast HID protocol crate.
//!
//! Covers: output report construction, device identification, encoding precision
//! and safety, edge cases, property tests, and constant validation.

use proptest::prelude::*;
use racing_wheel_hid_ffbeast_protocol::output::{ENABLE_FFB_REPORT_ID, MAX_TORQUE_SCALE};
use racing_wheel_hid_ffbeast_protocol::{
    CONSTANT_FORCE_REPORT_ID, CONSTANT_FORCE_REPORT_LEN, FFBEAST_PRODUCT_ID_JOYSTICK,
    FFBEAST_PRODUCT_ID_RUDDER, FFBEAST_PRODUCT_ID_WHEEL, FFBEAST_VENDOR_ID, FFBeastTorqueEncoder,
    GAIN_REPORT_ID, build_enable_ffb, build_set_gain, is_ffbeast_product,
};

// ── Constant validation ──────────────────────────────────────────────────────

#[test]
fn vendor_id_matches_linux_kernel() {
    assert_eq!(FFBEAST_VENDOR_ID, 0x045B);
}

#[test]
fn product_ids_are_distinct() {
    let pids = [
        FFBEAST_PRODUCT_ID_JOYSTICK,
        FFBEAST_PRODUCT_ID_RUDDER,
        FFBEAST_PRODUCT_ID_WHEEL,
    ];
    for i in 0..pids.len() {
        for j in (i + 1)..pids.len() {
            assert_ne!(
                pids[i], pids[j],
                "PID index {i} ({:#06X}) collides with index {j} ({:#06X})",
                pids[i], pids[j]
            );
        }
    }
}

#[test]
fn constant_force_report_id_is_0x01() {
    assert_eq!(CONSTANT_FORCE_REPORT_ID, 0x01);
}

#[test]
fn constant_force_report_len_is_5() {
    assert_eq!(CONSTANT_FORCE_REPORT_LEN, 5);
}

#[test]
fn enable_ffb_report_id_is_0x60() {
    assert_eq!(ENABLE_FFB_REPORT_ID, 0x60);
}

#[test]
fn gain_report_id_is_0x61() {
    assert_eq!(GAIN_REPORT_ID, 0x61);
}

#[test]
fn max_torque_scale_is_10000() {
    assert_eq!(MAX_TORQUE_SCALE, 10_000);
}

// ── Device identification ────────────────────────────────────────────────────

#[test]
fn all_three_products_recognised() {
    assert!(is_ffbeast_product(FFBEAST_PRODUCT_ID_JOYSTICK));
    assert!(is_ffbeast_product(FFBEAST_PRODUCT_ID_RUDDER));
    assert!(is_ffbeast_product(FFBEAST_PRODUCT_ID_WHEEL));
}

#[test]
fn zero_pid_not_recognised() {
    assert!(!is_ffbeast_product(0x0000));
}

#[test]
fn max_pid_not_recognised() {
    assert!(!is_ffbeast_product(0xFFFF));
}

#[test]
fn adjacent_pids_not_recognised() {
    // Test values one above/below each known PID
    let known = [
        FFBEAST_PRODUCT_ID_JOYSTICK,
        FFBEAST_PRODUCT_ID_RUDDER,
        FFBEAST_PRODUCT_ID_WHEEL,
    ];
    for pid in known {
        if pid > 0 {
            assert!(
                !is_ffbeast_product(pid - 1),
                "PID {:#06X} (one below {:#06X}) should not be recognised",
                pid - 1,
                pid
            );
        }
        if pid < u16::MAX {
            assert!(
                !is_ffbeast_product(pid + 1),
                "PID {:#06X} (one above {:#06X}) should not be recognised",
                pid + 1,
                pid
            );
        }
    }
}

// ── Output report construction ───────────────────────────────────────────────

#[test]
fn encode_zero_torque() {
    let enc = FFBeastTorqueEncoder;
    let report = enc.encode(0.0);
    assert_eq!(report.len(), CONSTANT_FORCE_REPORT_LEN);
    assert_eq!(report[0], CONSTANT_FORCE_REPORT_ID);
    assert_eq!(i16::from_le_bytes([report[1], report[2]]), 0);
    assert_eq!(report[3], 0x00);
    assert_eq!(report[4], 0x00);
}

#[test]
fn encode_full_positive() {
    let enc = FFBeastTorqueEncoder;
    let report = enc.encode(1.0);
    let raw = i16::from_le_bytes([report[1], report[2]]);
    assert_eq!(raw, MAX_TORQUE_SCALE);
}

#[test]
fn encode_full_negative() {
    let enc = FFBeastTorqueEncoder;
    let report = enc.encode(-1.0);
    let raw = i16::from_le_bytes([report[1], report[2]]);
    assert_eq!(raw, -MAX_TORQUE_SCALE);
}

#[test]
fn encode_half_positive() {
    let enc = FFBeastTorqueEncoder;
    let report = enc.encode(0.5);
    let raw = i16::from_le_bytes([report[1], report[2]]);
    assert_eq!(raw, 5000);
}

#[test]
fn encode_half_negative() {
    let enc = FFBeastTorqueEncoder;
    let report = enc.encode(-0.5);
    let raw = i16::from_le_bytes([report[1], report[2]]);
    assert_eq!(raw, -5000);
}

#[test]
fn encode_quarter_torque() {
    let enc = FFBeastTorqueEncoder;
    let report = enc.encode(0.25);
    let raw = i16::from_le_bytes([report[1], report[2]]);
    assert_eq!(raw, 2500);
}

// ── Encoding precision and safety ────────────────────────────────────────────

#[test]
fn encode_small_positive_torque_rounds_toward_zero() {
    let enc = FFBeastTorqueEncoder;
    // 0.00005 * 10000 = 0.5, truncated to 0 as i16
    let report = enc.encode(0.00005);
    let raw = i16::from_le_bytes([report[1], report[2]]);
    assert!(raw.abs() <= 1, "tiny torque should map to ~0, got {raw}");
}

#[test]
fn encode_negative_zero() {
    let enc = FFBeastTorqueEncoder;
    let report = enc.encode(-0.0);
    let raw = i16::from_le_bytes([report[1], report[2]]);
    assert_eq!(raw, 0, "-0.0 must encode as zero torque");
}

#[test]
fn encode_is_deterministic() {
    let enc = FFBeastTorqueEncoder;
    let r1 = enc.encode(0.7);
    let r2 = enc.encode(0.7);
    assert_eq!(r1, r2, "encoding must be deterministic");
}

#[test]
fn encode_is_monotonic_in_range() -> Result<(), String> {
    let enc = FFBeastTorqueEncoder;
    let values: Vec<f32> = (-100..=100).map(|i| i as f32 / 100.0).collect();
    for window in values.windows(2) {
        let raw_a = i16::from_le_bytes({
            let r = enc.encode(window[0]);
            [r[1], r[2]]
        });
        let raw_b = i16::from_le_bytes({
            let r = enc.encode(window[1]);
            [r[1], r[2]]
        });
        if raw_b < raw_a {
            return Err(format!(
                "not monotonic: encode({}) -> {raw_a}, encode({}) -> {raw_b}",
                window[0], window[1]
            ));
        }
    }
    Ok(())
}

#[test]
fn encoder_default_is_equivalent() {
    let a = FFBeastTorqueEncoder;
    let b = FFBeastTorqueEncoder;
    assert_eq!(a.encode(0.42), b.encode(0.42));
}

// ── Edge cases: clamping ─────────────────────────────────────────────────────

#[test]
fn clamp_over_positive_one() {
    let enc = FFBeastTorqueEncoder;
    assert_eq!(enc.encode(2.0), enc.encode(1.0));
    assert_eq!(enc.encode(100.0), enc.encode(1.0));
    assert_eq!(enc.encode(f32::MAX), enc.encode(1.0));
}

#[test]
fn clamp_under_negative_one() {
    let enc = FFBeastTorqueEncoder;
    assert_eq!(enc.encode(-2.0), enc.encode(-1.0));
    assert_eq!(enc.encode(-100.0), enc.encode(-1.0));
    assert_eq!(enc.encode(f32::MIN), enc.encode(-1.0));
}

#[test]
fn encode_nan_clamps_and_does_not_panic() {
    let enc = FFBeastTorqueEncoder;
    // NaN.clamp produces NaN with some implementations; ensure no panic
    let _report = enc.encode(f32::NAN);
    // Just verify it doesn't panic; result is implementation-defined for NaN
}

#[test]
fn encode_infinity_clamps_to_max() {
    let enc = FFBeastTorqueEncoder;
    let report = enc.encode(f32::INFINITY);
    let raw = i16::from_le_bytes([report[1], report[2]]);
    assert_eq!(raw, MAX_TORQUE_SCALE, "+inf must clamp to max");
}

#[test]
fn encode_neg_infinity_clamps_to_min() {
    let enc = FFBeastTorqueEncoder;
    let report = enc.encode(f32::NEG_INFINITY);
    let raw = i16::from_le_bytes([report[1], report[2]]);
    assert_eq!(raw, -MAX_TORQUE_SCALE, "-inf must clamp to min");
}

// ── Feature report construction ──────────────────────────────────────────────

#[test]
fn enable_ffb_true_structure() {
    let report = build_enable_ffb(true);
    assert_eq!(report.len(), 3);
    assert_eq!(report[0], ENABLE_FFB_REPORT_ID);
    assert_eq!(report[1], 0x01);
    assert_eq!(report[2], 0x00);
}

#[test]
fn enable_ffb_false_structure() {
    let report = build_enable_ffb(false);
    assert_eq!(report.len(), 3);
    assert_eq!(report[0], ENABLE_FFB_REPORT_ID);
    assert_eq!(report[1], 0x00);
    assert_eq!(report[2], 0x00);
}

#[test]
fn gain_report_min_and_max() {
    let min_report = build_set_gain(0);
    assert_eq!(min_report[0], GAIN_REPORT_ID);
    assert_eq!(min_report[1], 0);
    assert_eq!(min_report[2], 0x00);

    let max_report = build_set_gain(255);
    assert_eq!(max_report[0], GAIN_REPORT_ID);
    assert_eq!(max_report[1], 255);
    assert_eq!(max_report[2], 0x00);
}

#[test]
fn gain_report_preserves_midpoint() {
    let report = build_set_gain(128);
    assert_eq!(report[1], 128);
}

// ── Property tests ───────────────────────────────────────────────────────────

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(500))]

    #[test]
    fn prop_report_len_always_5(torque in any::<f32>()) {
        let report = FFBeastTorqueEncoder.encode(torque);
        prop_assert_eq!(report.len(), CONSTANT_FORCE_REPORT_LEN);
    }

    #[test]
    fn prop_report_id_always_correct(torque in any::<f32>()) {
        let report = FFBeastTorqueEncoder.encode(torque);
        prop_assert_eq!(report[0], CONSTANT_FORCE_REPORT_ID);
    }

    #[test]
    fn prop_reserved_bytes_always_zero(torque in any::<f32>()) {
        let report = FFBeastTorqueEncoder.encode(torque);
        prop_assert_eq!(report[3], 0u8);
        prop_assert_eq!(report[4], 0u8);
    }

    #[test]
    fn prop_in_range_torque_bounded(torque in -1.0f32..=1.0f32) {
        let report = FFBeastTorqueEncoder.encode(torque);
        let raw = i16::from_le_bytes([report[1], report[2]]);
        prop_assert!(raw >= -MAX_TORQUE_SCALE);
        prop_assert!(raw <= MAX_TORQUE_SCALE);
    }

    #[test]
    fn prop_positive_maps_positive(torque in 0.01f32..=1.0f32) {
        let report = FFBeastTorqueEncoder.encode(torque);
        let raw = i16::from_le_bytes([report[1], report[2]]);
        prop_assert!(raw > 0, "positive torque {torque} gave raw={raw}");
    }

    #[test]
    fn prop_negative_maps_negative(torque in -1.0f32..=-0.01f32) {
        let report = FFBeastTorqueEncoder.encode(torque);
        let raw = i16::from_le_bytes([report[1], report[2]]);
        prop_assert!(raw < 0, "negative torque {torque} gave raw={raw}");
    }

    #[test]
    fn prop_gain_round_trips(gain: u8) {
        let report = build_set_gain(gain);
        prop_assert_eq!(report[0], GAIN_REPORT_ID);
        prop_assert_eq!(report[1], gain);
        prop_assert_eq!(report[2], 0u8);
    }

    #[test]
    fn prop_enable_ffb_boolean(enabled: bool) {
        let report = build_enable_ffb(enabled);
        let expected = if enabled { 0x01u8 } else { 0x00u8 };
        prop_assert_eq!(report[0], ENABLE_FFB_REPORT_ID);
        prop_assert_eq!(report[1], expected);
        prop_assert_eq!(report[2], 0u8);
    }

    #[test]
    fn prop_is_ffbeast_product_exhaustive(pid: u16) {
        let known = pid == FFBEAST_PRODUCT_ID_JOYSTICK
            || pid == FFBEAST_PRODUCT_ID_RUDDER
            || pid == FFBEAST_PRODUCT_ID_WHEEL;
        prop_assert_eq!(is_ffbeast_product(pid), known);
    }

    #[test]
    fn prop_symmetry_positive_negative(torque in 0.0f32..=1.0f32) {
        let enc = FFBeastTorqueEncoder;
        let pos_raw = i16::from_le_bytes({
            let r = enc.encode(torque);
            [r[1], r[2]]
        });
        let neg_raw = i16::from_le_bytes({
            let r = enc.encode(-torque);
            [r[1], r[2]]
        });
        prop_assert_eq!(pos_raw, -neg_raw, "encoding must be symmetric around zero");
    }
}

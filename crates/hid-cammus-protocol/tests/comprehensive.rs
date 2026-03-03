//! Comprehensive tests for the Cammus HID protocol crate.
//!
//! Covers: input report parsing round-trips, output report construction,
//! device identification via PID, torque encoding precision and safety limits,
//! edge cases (boundary values, short reports, invalid data), property tests
//! for encoding round-trips, and known constant validation.

use racing_wheel_hid_cammus_protocol::{
    CammusModel, FFB_REPORT_ID, FFB_REPORT_LEN, MODE_CONFIG, MODE_GAME, PRODUCT_C5, PRODUCT_C12,
    PRODUCT_CP5_PEDALS, PRODUCT_LC100_PEDALS, ParseError, REPORT_ID, REPORT_LEN,
    STEERING_RANGE_DEG, VENDOR_ID, encode_stop, encode_torque, is_cammus, parse, product_name,
};

// ---------------------------------------------------------------------------
// 1. Known constant validation
// ---------------------------------------------------------------------------

#[test]
fn constants_vendor_id() {
    assert_eq!(
        VENDOR_ID, 0x3416,
        "Cammus VID must match Linux kernel hid-ids.h"
    );
}

#[test]
fn constants_product_ids() {
    assert_eq!(PRODUCT_C5, 0x0301);
    assert_eq!(PRODUCT_C12, 0x0302);
    assert_eq!(PRODUCT_CP5_PEDALS, 0x1018);
    assert_eq!(PRODUCT_LC100_PEDALS, 0x1019);
}

#[test]
fn constants_report_sizes() {
    assert_eq!(REPORT_LEN, 64);
    assert_eq!(FFB_REPORT_LEN, 8);
}

#[test]
fn constants_report_ids() {
    assert_eq!(REPORT_ID, 0x01);
    assert_eq!(FFB_REPORT_ID, 0x01);
}

#[test]
fn constants_mode_bytes() {
    assert_eq!(MODE_GAME, 0x01);
    assert_eq!(MODE_CONFIG, 0x00);
}

#[test]
fn constants_steering_range() {
    assert!((STEERING_RANGE_DEG - 1080.0).abs() < f32::EPSILON);
}

// ---------------------------------------------------------------------------
// 2. Device identification via PID
// ---------------------------------------------------------------------------

#[test]
fn is_cammus_accepts_all_known_pids() {
    let known = [
        PRODUCT_C5,
        PRODUCT_C12,
        PRODUCT_CP5_PEDALS,
        PRODUCT_LC100_PEDALS,
    ];
    for pid in known {
        assert!(
            is_cammus(VENDOR_ID, pid),
            "PID 0x{pid:04X} should be recognised"
        );
    }
}

#[test]
fn is_cammus_rejects_wrong_vid() {
    assert!(!is_cammus(0x0000, PRODUCT_C5));
    assert!(!is_cammus(0xFFFF, PRODUCT_C12));
    assert!(!is_cammus(0x0483, PRODUCT_C5)); // VRS VID
}

#[test]
fn is_cammus_rejects_unknown_pid() {
    assert!(!is_cammus(VENDOR_ID, 0x0000));
    assert!(!is_cammus(VENDOR_ID, 0xFFFF));
    assert!(!is_cammus(VENDOR_ID, 0x0303)); // adjacent to C12
}

#[test]
fn product_name_returns_correct_strings() {
    assert_eq!(product_name(PRODUCT_C5), Some("Cammus C5"));
    assert_eq!(product_name(PRODUCT_C12), Some("Cammus C12"));
    assert_eq!(product_name(PRODUCT_CP5_PEDALS), Some("Cammus CP5 Pedals"));
    assert_eq!(
        product_name(PRODUCT_LC100_PEDALS),
        Some("Cammus LC100 Pedals")
    );
}

#[test]
fn product_name_none_for_unknown() {
    assert_eq!(product_name(0x0000), None);
    assert_eq!(product_name(0xFFFF), None);
}

// ---------------------------------------------------------------------------
// 3. CammusModel identification and properties
// ---------------------------------------------------------------------------

#[test]
fn model_from_pid_roundtrip() {
    let cases: &[(u16, CammusModel)] = &[
        (PRODUCT_C5, CammusModel::C5),
        (PRODUCT_C12, CammusModel::C12),
        (PRODUCT_CP5_PEDALS, CammusModel::Cp5Pedals),
        (PRODUCT_LC100_PEDALS, CammusModel::Lc100Pedals),
    ];
    for &(pid, expected) in cases {
        let model = CammusModel::from_pid(pid);
        assert_eq!(model, Some(expected));
    }
}

#[test]
fn model_from_pid_unknown_returns_none() {
    assert_eq!(CammusModel::from_pid(0x0000), None);
    assert_eq!(CammusModel::from_pid(0xFFFF), None);
}

#[test]
fn model_torque_limits() {
    assert!((CammusModel::C5.max_torque_nm() - 5.0).abs() < f32::EPSILON);
    assert!((CammusModel::C12.max_torque_nm() - 12.0).abs() < f32::EPSILON);
    assert!((CammusModel::Cp5Pedals.max_torque_nm()).abs() < f32::EPSILON);
    assert!((CammusModel::Lc100Pedals.max_torque_nm()).abs() < f32::EPSILON);
}

#[test]
fn model_name_matches_product_name() {
    let pairs: &[(u16, CammusModel)] = &[
        (PRODUCT_C5, CammusModel::C5),
        (PRODUCT_C12, CammusModel::C12),
        (PRODUCT_CP5_PEDALS, CammusModel::Cp5Pedals),
        (PRODUCT_LC100_PEDALS, CammusModel::Lc100Pedals),
    ];
    for &(pid, model) in pairs {
        assert_eq!(
            Some(model.name()),
            product_name(pid),
            "Model::name() should match product_name() for PID 0x{pid:04X}"
        );
    }
}

// ---------------------------------------------------------------------------
// 4. Input report parsing
// ---------------------------------------------------------------------------

#[test]
fn parse_too_short_returns_error() {
    for len in 0..12 {
        let data = vec![0u8; len];
        let err = parse(&data);
        assert_eq!(err, Err(ParseError::TooShort { got: len, need: 12 }));
    }
}

#[test]
fn parse_exact_minimum_length() -> Result<(), ParseError> {
    let data = [0u8; 12];
    let report = parse(&data)?;
    assert!(report.steering.abs() < 0.001);
    assert!(report.throttle.abs() < 0.001);
    assert!(report.brake.abs() < 0.001);
    assert!(report.clutch.abs() < 0.001);
    assert!(report.handbrake.abs() < 0.001);
    assert_eq!(report.buttons, 0);
    Ok(())
}

#[test]
fn parse_full_64_byte_report_zeroed() -> Result<(), ParseError> {
    let report = parse(&[0u8; 64])?;
    assert!(report.steering.abs() < 0.001);
    Ok(())
}

#[test]
fn parse_steering_full_positive() -> Result<(), ParseError> {
    let mut data = [0u8; 64];
    let bytes = i16::MAX.to_le_bytes();
    data[0] = bytes[0];
    data[1] = bytes[1];
    let report = parse(&data)?;
    assert!((report.steering - 1.0).abs() < 0.001);
    Ok(())
}

#[test]
fn parse_steering_full_negative() -> Result<(), ParseError> {
    let mut data = [0u8; 64];
    let bytes = (-i16::MAX).to_le_bytes();
    data[0] = bytes[0];
    data[1] = bytes[1];
    let report = parse(&data)?;
    assert!((report.steering + 1.0).abs() < 0.001);
    Ok(())
}

#[test]
fn parse_steering_i16_min_clamps() -> Result<(), ParseError> {
    let mut data = [0u8; 64];
    let bytes = i16::MIN.to_le_bytes();
    data[0] = bytes[0];
    data[1] = bytes[1];
    let report = parse(&data)?;
    // i16::MIN / i16::MAX > -1.0 in magnitude, should clamp to -1.0
    assert!((report.steering + 1.0).abs() < 0.01);
    Ok(())
}

#[test]
fn parse_throttle_full() -> Result<(), ParseError> {
    let mut data = [0u8; 64];
    data[2] = 0xFF;
    data[3] = 0xFF;
    let report = parse(&data)?;
    assert!((report.throttle - 1.0).abs() < 0.001);
    Ok(())
}

#[test]
fn parse_brake_full() -> Result<(), ParseError> {
    let mut data = [0u8; 64];
    data[4] = 0xFF;
    data[5] = 0xFF;
    let report = parse(&data)?;
    assert!((report.brake - 1.0).abs() < 0.001);
    Ok(())
}

#[test]
fn parse_clutch_full() -> Result<(), ParseError> {
    let mut data = [0u8; 64];
    data[8] = 0xFF;
    data[9] = 0xFF;
    let report = parse(&data)?;
    assert!((report.clutch - 1.0).abs() < 0.001);
    Ok(())
}

#[test]
fn parse_handbrake_full() -> Result<(), ParseError> {
    let mut data = [0u8; 64];
    data[10] = 0xFF;
    data[11] = 0xFF;
    let report = parse(&data)?;
    assert!((report.handbrake - 1.0).abs() < 0.001);
    Ok(())
}

#[test]
fn parse_buttons_all_set() -> Result<(), ParseError> {
    let mut data = [0u8; 64];
    data[6] = 0xFF;
    data[7] = 0xFF;
    let report = parse(&data)?;
    assert_eq!(report.buttons, 0xFFFF);
    Ok(())
}

#[test]
fn parse_buttons_individual_bits() -> Result<(), ParseError> {
    for bit in 0..16u16 {
        let mut data = [0u8; 64];
        if bit < 8 {
            data[6] = 1 << bit;
        } else {
            data[7] = 1 << (bit - 8);
        }
        let report = parse(&data)?;
        assert_eq!(
            report.buttons,
            1 << bit,
            "Button bit {bit} not correctly parsed"
        );
    }
    Ok(())
}

#[test]
fn parse_half_pedal_values() -> Result<(), ParseError> {
    let mut data = [0u8; 64];
    // ~50% throttle: 0x7FFF
    let half = 0x7FFFu16.to_le_bytes();
    data[2] = half[0];
    data[3] = half[1];
    let report = parse(&data)?;
    assert!((report.throttle - 0.5).abs() < 0.01);
    Ok(())
}

#[test]
fn parse_extra_bytes_ignored() -> Result<(), ParseError> {
    let mut data = [0xFFu8; 128];
    // Zero the first 12 bytes
    data[..12].fill(0);
    let report = parse(&data)?;
    assert!(report.steering.abs() < 0.001);
    Ok(())
}

// ---------------------------------------------------------------------------
// 5. Output report construction (FFB / torque commands)
// ---------------------------------------------------------------------------

#[test]
fn encode_torque_zero() {
    let report = encode_torque(0.0);
    assert_eq!(report.len(), FFB_REPORT_LEN);
    assert_eq!(report[0], FFB_REPORT_ID);
    let raw = i16::from_le_bytes([report[1], report[2]]);
    assert_eq!(raw, 0);
    assert_eq!(report[3], MODE_GAME);
    assert_eq!(&report[4..], &[0, 0, 0, 0]);
}

#[test]
fn encode_torque_positive_max() {
    let report = encode_torque(1.0);
    let raw = i16::from_le_bytes([report[1], report[2]]);
    assert_eq!(raw, i16::MAX);
}

#[test]
fn encode_torque_negative_max() {
    let report = encode_torque(-1.0);
    let raw = i16::from_le_bytes([report[1], report[2]]);
    assert_eq!(raw, -i16::MAX);
}

#[test]
fn encode_torque_clamps_above_one() {
    let r1 = encode_torque(1.0);
    let r2 = encode_torque(2.5);
    let r3 = encode_torque(f32::MAX);
    assert_eq!(r1, r2);
    assert_eq!(r1, r3);
}

#[test]
fn encode_torque_clamps_below_neg_one() {
    let r1 = encode_torque(-1.0);
    let r2 = encode_torque(-2.5);
    let r3 = encode_torque(f32::MIN);
    assert_eq!(r1, r2);
    assert_eq!(r1, r3);
}

#[test]
fn encode_torque_nan_clamps_safely() {
    let report = encode_torque(f32::NAN);
    // NaN.clamp(-1.0, 1.0) is NaN in Rust, but (NaN * i16::MAX) as i16 = 0
    // The important thing is no panic and the report is well-formed
    assert_eq!(report[0], FFB_REPORT_ID);
    assert_eq!(report[3], MODE_GAME);
    assert_eq!(report.len(), FFB_REPORT_LEN);
}

#[test]
fn encode_torque_infinity_clamps() {
    let pos = encode_torque(f32::INFINITY);
    let neg = encode_torque(f32::NEG_INFINITY);
    assert_eq!(pos, encode_torque(1.0));
    assert_eq!(neg, encode_torque(-1.0));
}

#[test]
fn encode_stop_is_zero_torque() {
    assert_eq!(encode_stop(), encode_torque(0.0));
}

#[test]
fn encode_stop_reserved_bytes_zero() {
    let report = encode_stop();
    assert_eq!(&report[4..], &[0, 0, 0, 0]);
}

// ---------------------------------------------------------------------------
// 6. Torque encoding precision and safety limits
// ---------------------------------------------------------------------------

#[test]
fn encode_torque_sign_preserved() {
    for &t in &[0.01f32, 0.1, 0.25, 0.5, 0.75, 0.99] {
        let pos = encode_torque(t);
        let neg = encode_torque(-t);
        let raw_pos = i16::from_le_bytes([pos[1], pos[2]]);
        let raw_neg = i16::from_le_bytes([neg[1], neg[2]]);
        assert!(raw_pos > 0, "positive torque {t} must yield positive raw");
        assert!(raw_neg < 0, "negative torque {t} must yield negative raw");
    }
}

#[test]
fn encode_torque_monotonic() {
    let vals: &[f32] = &[-1.0, -0.75, -0.5, -0.25, 0.0, 0.25, 0.5, 0.75, 1.0];
    let raws: Vec<i16> = vals
        .iter()
        .map(|&t| {
            let r = encode_torque(t);
            i16::from_le_bytes([r[1], r[2]])
        })
        .collect();
    for i in 1..raws.len() {
        assert!(
            raws[i] >= raws[i - 1],
            "monotonicity violated at index {i}: {} >= {}",
            raws[i],
            raws[i - 1]
        );
    }
}

#[test]
fn encode_torque_precision_at_half() {
    let report = encode_torque(0.5);
    let raw = i16::from_le_bytes([report[1], report[2]]);
    let expected = (0.5f32 * i16::MAX as f32) as i16;
    assert_eq!(raw, expected);
}

#[test]
fn encode_torque_symmetric() {
    for &t in &[0.1f32, 0.25, 0.5, 0.75, 1.0] {
        let pos_raw = i16::from_le_bytes({
            let r = encode_torque(t);
            [r[1], r[2]]
        });
        let neg_raw = i16::from_le_bytes({
            let r = encode_torque(-t);
            [r[1], r[2]]
        });
        assert_eq!(pos_raw, -neg_raw, "torque encoding not symmetric for ±{t}");
    }
}

// ---------------------------------------------------------------------------
// 7. Edge cases: boundary values
// ---------------------------------------------------------------------------

#[test]
fn parse_all_ones_report() -> Result<(), ParseError> {
    let data = [0xFFu8; 64];
    let report = parse(&data)?;
    // steering: 0xFFFF as i16 = -1, so -1/32767 ≈ -0.00003
    assert!(report.steering.abs() < 0.01);
    // pedals: 0xFFFF as u16 = 65535, normalized to 1.0
    assert!((report.throttle - 1.0).abs() < 0.001);
    assert!((report.brake - 1.0).abs() < 0.001);
    assert!((report.clutch - 1.0).abs() < 0.001);
    assert!((report.handbrake - 1.0).abs() < 0.001);
    assert_eq!(report.buttons, 0xFFFF);
    Ok(())
}

#[test]
fn parse_alternating_bytes() -> Result<(), ParseError> {
    let mut data = [0u8; 64];
    for i in (0..64).step_by(2) {
        data[i] = 0xAA;
    }
    for i in (1..64).step_by(2) {
        data[i] = 0x55;
    }
    // Should not panic, just parse
    let _report = parse(&data)?;
    Ok(())
}

#[test]
fn parse_error_display() {
    let err = ParseError::TooShort { got: 5, need: 12 };
    let msg = format!("{err}");
    assert!(
        msg.contains("5"),
        "error message should contain the 'got' value"
    );
    assert!(
        msg.contains("12"),
        "error message should contain the 'need' value"
    );
}

// ---------------------------------------------------------------------------
// 8. Input report round-trip: write fields → parse → verify
// ---------------------------------------------------------------------------

fn build_input_report(
    steering: i16,
    throttle: u16,
    brake: u16,
    clutch: u16,
    handbrake: u16,
    buttons: u16,
) -> [u8; 64] {
    let mut data = [0u8; 64];
    let s = steering.to_le_bytes();
    data[0] = s[0];
    data[1] = s[1];
    let t = throttle.to_le_bytes();
    data[2] = t[0];
    data[3] = t[1];
    let b = brake.to_le_bytes();
    data[4] = b[0];
    data[5] = b[1];
    data[6] = (buttons & 0xFF) as u8;
    data[7] = (buttons >> 8) as u8;
    let c = clutch.to_le_bytes();
    data[8] = c[0];
    data[9] = c[1];
    let h = handbrake.to_le_bytes();
    data[10] = h[0];
    data[11] = h[1];
    data
}

#[test]
fn input_roundtrip_center() -> Result<(), ParseError> {
    let data = build_input_report(0, 0, 0, 0, 0, 0);
    let r = parse(&data)?;
    assert!(r.steering.abs() < 0.001);
    assert!(r.throttle.abs() < 0.001);
    assert!(r.brake.abs() < 0.001);
    assert!(r.clutch.abs() < 0.001);
    assert!(r.handbrake.abs() < 0.001);
    assert_eq!(r.buttons, 0);
    Ok(())
}

#[test]
fn input_roundtrip_full_scale() -> Result<(), ParseError> {
    let data = build_input_report(i16::MAX, u16::MAX, u16::MAX, u16::MAX, u16::MAX, 0xFFFF);
    let r = parse(&data)?;
    assert!((r.steering - 1.0).abs() < 0.001);
    assert!((r.throttle - 1.0).abs() < 0.001);
    assert!((r.brake - 1.0).abs() < 0.001);
    assert!((r.clutch - 1.0).abs() < 0.001);
    assert!((r.handbrake - 1.0).abs() < 0.001);
    assert_eq!(r.buttons, 0xFFFF);
    Ok(())
}

#[test]
fn input_roundtrip_negative_steering() -> Result<(), ParseError> {
    let data = build_input_report(-i16::MAX, 0, 0, 0, 0, 0);
    let r = parse(&data)?;
    assert!((r.steering + 1.0).abs() < 0.001);
    Ok(())
}

// ---------------------------------------------------------------------------
// 9. Property tests
// ---------------------------------------------------------------------------

mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(500))]

        #[test]
        fn prop_encode_torque_report_always_valid(torque in -100.0f32..=100.0f32) {
            let report = encode_torque(torque);
            prop_assert_eq!(report[0], FFB_REPORT_ID);
            prop_assert_eq!(report[3], MODE_GAME);
            prop_assert_eq!(&report[4..], &[0u8, 0, 0, 0]);
            prop_assert_eq!(report.len(), FFB_REPORT_LEN);
        }

        #[test]
        fn prop_encode_torque_magnitude_bounded(torque in -100.0f32..=100.0f32) {
            let report = encode_torque(torque);
            let raw = i16::from_le_bytes([report[1], report[2]]);
            prop_assert!(raw >= -i16::MAX);
            // raw is always in [-i16::MAX, i16::MAX] by construction
        }

        #[test]
        fn prop_encode_torque_sign_preserved(torque in -1.0f32..=1.0f32) {
            let report = encode_torque(torque);
            let raw = i16::from_le_bytes([report[1], report[2]]);
            if torque > 0.001 {
                prop_assert!(raw > 0);
            } else if torque < -0.001 {
                prop_assert!(raw < 0);
            }
        }

        #[test]
        fn prop_encode_torque_monotone(a in -1.0f32..=1.0f32, b in -1.0f32..=1.0f32) {
            let ra = encode_torque(a);
            let rb = encode_torque(b);
            let raw_a = i16::from_le_bytes([ra[1], ra[2]]);
            let raw_b = i16::from_le_bytes([rb[1], rb[2]]);
            if a > b {
                prop_assert!(raw_a >= raw_b);
            } else if a < b {
                prop_assert!(raw_a <= raw_b);
            }
        }

        #[test]
        fn prop_parse_never_panics(data in proptest::collection::vec(any::<u8>(), 0..128)) {
            let _ = parse(&data);
        }

        #[test]
        fn prop_parse_valid_when_long_enough(data in proptest::collection::vec(any::<u8>(), 12..128)) {
            let result = parse(&data);
            prop_assert!(result.is_ok());
            if let Ok(r) = result {
                prop_assert!((-1.0..=1.0).contains(&r.steering));
                prop_assert!((0.0..=1.0).contains(&r.throttle));
                prop_assert!((0.0..=1.0).contains(&r.brake));
                prop_assert!((0.0..=1.0).contains(&r.clutch));
                prop_assert!((0.0..=1.0).contains(&r.handbrake));
            }
        }

        #[test]
        fn prop_input_roundtrip(
            steering in -32767i16..=32767i16,
            throttle in 0u16..=65535u16,
            brake in 0u16..=65535u16,
            clutch in 0u16..=65535u16,
            handbrake in 0u16..=65535u16,
            buttons in 0u16..=65535u16,
        ) {
            let data = build_input_report(steering, throttle, brake, clutch, handbrake, buttons);
            let result = parse(&data);
            prop_assert!(result.is_ok());
            let r = result.ok().ok_or_else(|| proptest::test_runner::TestCaseError::Fail("parse failed".into()))?;
            prop_assert!((-1.0..=1.0).contains(&r.steering));
            prop_assert!((0.0..=1.0).contains(&r.throttle));
            prop_assert!((0.0..=1.0).contains(&r.brake));
            prop_assert!((0.0..=1.0).contains(&r.clutch));
            prop_assert!((0.0..=1.0).contains(&r.handbrake));
            prop_assert_eq!(r.buttons, buttons);
        }

        #[test]
        fn prop_is_cammus_only_with_correct_vid(pid in 0u16..=0xFFFFu16) {
            if is_cammus(VENDOR_ID, pid) {
                let name = product_name(pid);
                prop_assert!(name.is_some(), "recognised PID must have a product name");
            }
        }
    }
}

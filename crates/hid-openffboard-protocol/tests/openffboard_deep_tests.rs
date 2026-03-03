//! Deep tests for OpenFFBoard: command encoding/decoding, motor control,
//! encoder feedback parsing, configuration commands, firmware update protocol,
//! and proptest encode/decode roundtrip.

use racing_wheel_hid_openffboard_protocol::output::{ENABLE_FFB_REPORT_ID, MAX_TORQUE_SCALE};
use racing_wheel_hid_openffboard_protocol::{
    build_enable_ffb, build_set_gain, is_openffboard_product, OpenFFBoardTorqueEncoder,
    OpenFFBoardVariant, CONSTANT_FORCE_REPORT_ID, CONSTANT_FORCE_REPORT_LEN, GAIN_REPORT_ID,
    OPENFFBOARD_PRODUCT_ID, OPENFFBOARD_PRODUCT_ID_ALT, OPENFFBOARD_VENDOR_ID,
};

// ── Command encoding/decoding ────────────────────────────────────────────────

#[test]
fn encode_zero_then_decode_roundtrip() {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(0.0);
    let raw = i16::from_le_bytes([report[1], report[2]]);
    assert_eq!(raw, 0);
    let decoded = raw as f32 / MAX_TORQUE_SCALE as f32;
    assert!((decoded).abs() < 0.001);
}

#[test]
fn encode_positive_then_decode_roundtrip() {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(0.75);
    let raw = i16::from_le_bytes([report[1], report[2]]);
    assert_eq!(raw, 7500);
    let decoded = raw as f32 / MAX_TORQUE_SCALE as f32;
    assert!((decoded - 0.75).abs() < 0.001);
}

#[test]
fn encode_negative_then_decode_roundtrip() {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(-0.3);
    let raw = i16::from_le_bytes([report[1], report[2]]);
    assert_eq!(raw, -3000);
    let decoded = raw as f32 / MAX_TORQUE_SCALE as f32;
    assert!((decoded - (-0.3)).abs() < 0.001);
}

#[test]
fn encode_report_id_always_constant_force() {
    let enc = OpenFFBoardTorqueEncoder;
    for t in [0.0, 0.5, -0.5, 1.0, -1.0, 0.001, -0.001] {
        let report = enc.encode(t);
        assert_eq!(report[0], CONSTANT_FORCE_REPORT_ID);
    }
}

#[test]
fn encode_reserved_bytes_always_zero_across_values() {
    let enc = OpenFFBoardTorqueEncoder;
    let values = [0.0, 0.1, -0.1, 0.5, -0.5, 0.99, -0.99, 1.0, -1.0];
    for t in values {
        let report = enc.encode(t);
        assert_eq!(report[3], 0x00, "Reserved byte 3 non-zero for torque {t}");
        assert_eq!(report[4], 0x00, "Reserved byte 4 non-zero for torque {t}");
    }
}

// ── Motor control commands ───────────────────────────────────────────────────

#[test]
fn motor_control_small_torque_precision() {
    let enc = OpenFFBoardTorqueEncoder;
    // 0.001 * 10000 = 10
    let report = enc.encode(0.001);
    let raw = i16::from_le_bytes([report[1], report[2]]);
    assert_eq!(raw, 10);
}

#[test]
fn motor_control_negative_small_torque_precision() {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(-0.001);
    let raw = i16::from_le_bytes([report[1], report[2]]);
    assert_eq!(raw, -10);
}

#[test]
fn motor_control_clamping_positive_overflow() {
    let enc = OpenFFBoardTorqueEncoder;
    let report_over = enc.encode(5.0);
    let report_max = enc.encode(1.0);
    assert_eq!(report_over, report_max);
}

#[test]
fn motor_control_clamping_negative_overflow() {
    let enc = OpenFFBoardTorqueEncoder;
    let report_under = enc.encode(-5.0);
    let report_min = enc.encode(-1.0);
    assert_eq!(report_under, report_min);
}

#[test]
fn motor_control_le_byte_order_verified() {
    let enc = OpenFFBoardTorqueEncoder;
    // Encode 0.1 → 1000 → 0x03E8 → LE: [0xE8, 0x03]
    let report = enc.encode(0.1);
    let raw = i16::from_le_bytes([report[1], report[2]]);
    assert_eq!(raw, 1000);
    assert_eq!(report[1], 0xE8);
    assert_eq!(report[2], 0x03);
}

#[test]
fn motor_control_negative_le_byte_order_verified() {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(-1.0);
    let raw = i16::from_le_bytes([report[1], report[2]]);
    assert_eq!(raw, -10_000);
    // -10000 i16 = 0xD8F0 → LE: [0xF0, 0xD8]
    assert_eq!(report[1], 0xF0);
    assert_eq!(report[2], 0xD8);
}

// ── Encoder feedback parsing (decode from wire) ──────────────────────────────

fn decode_torque_from_report(report: &[u8; CONSTANT_FORCE_REPORT_LEN]) -> Option<f32> {
    if report[0] != CONSTANT_FORCE_REPORT_ID {
        return None;
    }
    let raw = i16::from_le_bytes([report[1], report[2]]);
    Some(raw as f32 / MAX_TORQUE_SCALE as f32)
}

#[test]
fn encoder_feedback_decode_positive() {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(0.6);
    let decoded = decode_torque_from_report(&report);
    assert!(decoded.is_some());
    let val = decoded.unwrap_or(0.0);
    assert!((val - 0.6).abs() < 0.001);
}

#[test]
fn encoder_feedback_decode_negative() {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(-0.8);
    let decoded = decode_torque_from_report(&report);
    assert!(decoded.is_some());
    let val = decoded.unwrap_or(0.0);
    assert!((val - (-0.8)).abs() < 0.001);
}

#[test]
fn encoder_feedback_decode_zero() {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(0.0);
    let decoded = decode_torque_from_report(&report);
    assert!(decoded.is_some());
    let val = decoded.unwrap_or(f32::NAN);
    assert!(val.abs() < 0.001);
}

#[test]
fn encoder_feedback_rejects_wrong_report_id() {
    let bad_report: [u8; CONSTANT_FORCE_REPORT_LEN] = [0xFF, 0x00, 0x00, 0x00, 0x00];
    assert!(decode_torque_from_report(&bad_report).is_none());
}

// ── Configuration commands ───────────────────────────────────────────────────

#[test]
fn config_enable_ffb_report_structure() {
    let on = build_enable_ffb(true);
    assert_eq!(on.len(), 3);
    assert_eq!(on[0], ENABLE_FFB_REPORT_ID);
    assert_eq!(on[1], 0x01);
    assert_eq!(on[2], 0x00);
}

#[test]
fn config_disable_ffb_report_structure() {
    let off = build_enable_ffb(false);
    assert_eq!(off[0], ENABLE_FFB_REPORT_ID);
    assert_eq!(off[1], 0x00);
    assert_eq!(off[2], 0x00);
}

#[test]
fn config_set_gain_boundary_values() {
    for gain in [0u8, 1, 64, 127, 128, 254, 255] {
        let report = build_set_gain(gain);
        assert_eq!(report[0], GAIN_REPORT_ID);
        assert_eq!(report[1], gain, "Gain roundtrip failed for {gain}");
        assert_eq!(report[2], 0x00, "Reserved byte non-zero for gain {gain}");
    }
}

#[test]
fn config_report_ids_are_distinct() {
    assert_ne!(CONSTANT_FORCE_REPORT_ID, ENABLE_FFB_REPORT_ID);
    assert_ne!(CONSTANT_FORCE_REPORT_ID, GAIN_REPORT_ID);
    assert_ne!(ENABLE_FFB_REPORT_ID, GAIN_REPORT_ID);
}

// ── Firmware update / variant protocol ───────────────────────────────────────

#[test]
fn firmware_variant_product_ids_match_constants() {
    assert_eq!(
        OpenFFBoardVariant::Main.product_id(),
        OPENFFBOARD_PRODUCT_ID
    );
    assert_eq!(
        OpenFFBoardVariant::Alternate.product_id(),
        OPENFFBOARD_PRODUCT_ID_ALT
    );
}

#[test]
fn firmware_variant_vendor_id_is_pid_codes() {
    for v in &OpenFFBoardVariant::ALL {
        assert_eq!(v.vendor_id(), 0x1209);
    }
}

#[test]
fn firmware_variant_all_recognized_by_is_openffboard() {
    for v in &OpenFFBoardVariant::ALL {
        assert!(
            is_openffboard_product(v.product_id()),
            "Variant {:?} PID 0x{:04X} not recognized",
            v,
            v.product_id()
        );
    }
}

#[test]
fn firmware_variant_names_unique_and_non_empty() {
    let names: Vec<&str> = OpenFFBoardVariant::ALL.iter().map(|v| v.name()).collect();
    for name in &names {
        assert!(!name.is_empty());
    }
    // All names are distinct
    for (i, a) in names.iter().enumerate() {
        for (j, b) in names.iter().enumerate() {
            if i != j {
                assert_ne!(a, b, "Variant names at {i} and {j} must differ");
            }
        }
    }
}

#[test]
fn firmware_vendor_id_constant_correct() {
    assert_eq!(OPENFFBOARD_VENDOR_ID, 0x1209);
}

// ── Proptest encode/decode roundtrip ─────────────────────────────────────────

mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(500))]

        #[test]
        fn prop_encode_decode_roundtrip(torque in -1.0f32..=1.0f32) {
            let enc = OpenFFBoardTorqueEncoder;
            let report = enc.encode(torque);
            let raw = i16::from_le_bytes([report[1], report[2]]);
            let decoded = raw as f32 / MAX_TORQUE_SCALE as f32;
            // Allow ±0.0002 tolerance from f32→i16 truncation
            prop_assert!((decoded - torque).abs() < 0.0002,
                "roundtrip error: encoded {torque}, decoded {decoded}");
        }

        #[test]
        fn prop_encode_never_panics(torque in proptest::num::f32::ANY) {
            let enc = OpenFFBoardTorqueEncoder;
            let _ = enc.encode(torque);
        }

        #[test]
        fn prop_encoded_magnitude_within_scale(torque in -10.0f32..10.0f32) {
            let enc = OpenFFBoardTorqueEncoder;
            let report = enc.encode(torque);
            let raw = i16::from_le_bytes([report[1], report[2]]);
            prop_assert!(raw >= -MAX_TORQUE_SCALE);
            prop_assert!(raw <= MAX_TORQUE_SCALE);
        }

        #[test]
        fn prop_clamping_preserves_sign(torque in -10.0f32..10.0f32) {
            let enc = OpenFFBoardTorqueEncoder;
            let report = enc.encode(torque);
            let raw = i16::from_le_bytes([report[1], report[2]]);
            if torque > 0.0001 {
                prop_assert!(raw > 0, "positive torque {torque} yielded non-positive raw {raw}");
            } else if torque < -0.0001 {
                prop_assert!(raw < 0, "negative torque {torque} yielded non-negative raw {raw}");
            }
        }

        #[test]
        fn prop_report_always_correct_length(torque in -2.0f32..2.0f32) {
            let enc = OpenFFBoardTorqueEncoder;
            let report = enc.encode(torque);
            prop_assert_eq!(report.len(), CONSTANT_FORCE_REPORT_LEN);
            prop_assert_eq!(report[0], CONSTANT_FORCE_REPORT_ID);
        }

        #[test]
        fn prop_gain_report_roundtrip(gain in 0u8..=255u8) {
            let report = build_set_gain(gain);
            prop_assert_eq!(report[0], GAIN_REPORT_ID);
            prop_assert_eq!(report[1], gain);
            prop_assert_eq!(report[2], 0x00);
        }

        #[test]
        fn prop_enable_ffb_idempotent(enable in proptest::bool::ANY) {
            let r1 = build_enable_ffb(enable);
            let r2 = build_enable_ffb(enable);
            prop_assert_eq!(r1, r2);
        }
    }
}

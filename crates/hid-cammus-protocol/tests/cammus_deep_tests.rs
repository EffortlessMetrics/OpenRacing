//! Deep integration tests for Cammus HID protocol.
//!
//! Covers PIDFF effect encoding/decoding roundtrips, block load parsing,
//! device control commands, input report field isolation, direct torque
//! wire-level encoding, clutch/handbrake axis parsing, error formatting,
//! cross-module consistency, and proptest fuzzing.

use racing_wheel_hid_cammus_protocol::{
    // Model
    CammusModel,
    DURATION_INFINITE,
    EffectOp,
    // PIDFF re-exports
    EffectType as PidEffectType,
    // Direct torque API
    FFB_REPORT_ID,
    FFB_REPORT_LEN,
    MODE_CONFIG,
    MODE_GAME,
    PRODUCT_C5,
    PRODUCT_C12,
    PRODUCT_CP5_PEDALS,
    PRODUCT_LC100_PEDALS,
    ParseError,
    REPORT_ID,
    REPORT_LEN,
    STEERING_RANGE_DEG,
    // IDs
    VENDOR_ID,
    encode_block_free,
    encode_device_control,
    encode_device_gain,
    encode_effect_operation,
    encode_set_condition,
    encode_set_constant_force,
    encode_set_effect,
    encode_set_envelope,
    encode_set_periodic,
    encode_set_ramp_force,
    encode_stop,
    encode_torque,
    is_cammus,
    parse,
    product_name,
};

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn make_input_report(
    steering: i16,
    throttle: u16,
    brake: u16,
    buttons_lo: u8,
    buttons_hi: u8,
    clutch: u16,
    handbrake: u16,
) -> Vec<u8> {
    let mut data = vec![0u8; REPORT_LEN];
    let s = steering.to_le_bytes();
    data[0] = s[0];
    data[1] = s[1];
    let t = throttle.to_le_bytes();
    data[2] = t[0];
    data[3] = t[1];
    let b = brake.to_le_bytes();
    data[4] = b[0];
    data[5] = b[1];
    data[6] = buttons_lo;
    data[7] = buttons_hi;
    let c = clutch.to_le_bytes();
    data[8] = c[0];
    data[9] = c[1];
    let h = handbrake.to_le_bytes();
    data[10] = h[0];
    data[11] = h[1];
    data
}

// ═══════════════════════════════════════════════════════════════════════════
// 1. PIDFF effect encoding roundtrips
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn pidff_set_effect_encodes_all_fields() -> Result<(), Box<dyn std::error::Error>> {
    let buf = encode_set_effect(2, PidEffectType::Damper, DURATION_INFINITE, 128, 0x5678);
    assert_eq!(buf[0], 0x01, "report ID = SET_EFFECT");
    assert_eq!(buf[1], 2, "block index");
    assert_eq!(buf[2], PidEffectType::Damper as u8, "effect type");
    let dur = u16::from_le_bytes([buf[3], buf[4]]);
    assert_eq!(dur, DURATION_INFINITE);
    assert_eq!(buf[9], 128, "gain");
    let dir = u16::from_le_bytes([buf[11], buf[12]]);
    assert_eq!(dir, 0x5678, "direction");
    Ok(())
}

#[test]
fn pidff_set_constant_force_magnitude_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    for mag in [i16::MIN, -10000, -1, 0, 1, 10000, i16::MAX] {
        let buf = encode_set_constant_force(1, mag);
        assert_eq!(buf[0], 0x05, "report ID");
        let decoded = i16::from_le_bytes([buf[2], buf[3]]);
        assert_eq!(decoded, mag, "magnitude roundtrip for {mag}");
    }
    Ok(())
}

#[test]
fn pidff_set_periodic_fields_preserved() -> Result<(), Box<dyn std::error::Error>> {
    let buf = encode_set_periodic(3, 8000, -4000, 4500, 20);
    assert_eq!(buf[0], 0x04, "report ID = SET_PERIODIC");
    assert_eq!(buf[1], 3, "block index");
    assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), 8000, "magnitude");
    assert_eq!(i16::from_le_bytes([buf[4], buf[5]]), -4000, "offset");
    assert_eq!(u16::from_le_bytes([buf[6], buf[7]]), 4500, "phase");
    assert_eq!(u16::from_le_bytes([buf[8], buf[9]]), 20, "period");
    Ok(())
}

#[test]
fn pidff_set_envelope_fields_preserved() -> Result<(), Box<dyn std::error::Error>> {
    let buf = encode_set_envelope(1, 6000, 3000, 200, 800);
    assert_eq!(buf[0], 0x02, "report ID = SET_ENVELOPE");
    assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), 6000, "attack level");
    assert_eq!(u16::from_le_bytes([buf[4], buf[5]]), 3000, "fade level");
    assert_eq!(u16::from_le_bytes([buf[6], buf[7]]), 200, "attack time");
    assert_eq!(u16::from_le_bytes([buf[8], buf[9]]), 800, "fade time");
    Ok(())
}

#[test]
fn pidff_set_condition_fields_preserved() -> Result<(), Box<dyn std::error::Error>> {
    let buf = encode_set_condition(5, 0, -500, 3000, -3000, 7000, 7000, 20);
    assert_eq!(buf[0], 0x03, "report ID = SET_CONDITION");
    assert_eq!(buf[1], 5, "block index");
    assert_eq!(buf[2], 0, "axis");
    assert_eq!(i16::from_le_bytes([buf[3], buf[4]]), -500, "center point");
    assert_eq!(i16::from_le_bytes([buf[5], buf[6]]), 3000, "positive coeff");
    assert_eq!(
        i16::from_le_bytes([buf[7], buf[8]]),
        -3000,
        "negative coeff"
    );
    assert_eq!(buf[13], 20, "dead band");
    Ok(())
}

#[test]
fn pidff_set_ramp_force_start_end() -> Result<(), Box<dyn std::error::Error>> {
    let buf = encode_set_ramp_force(1, 10000, -10000);
    assert_eq!(buf[0], 0x06, "report ID = SET_RAMP_FORCE");
    assert_eq!(i16::from_le_bytes([buf[2], buf[3]]), 10000, "start");
    assert_eq!(i16::from_le_bytes([buf[4], buf[5]]), -10000, "end");
    Ok(())
}

#[test]
fn pidff_effect_operation_all_ops() -> Result<(), Box<dyn std::error::Error>> {
    let ops = [
        (EffectOp::Start, 1u8),
        (EffectOp::StartSolo, 2),
        (EffectOp::Stop, 3),
    ];
    for (op, expected_val) in ops {
        let buf = encode_effect_operation(1, op, 0);
        assert_eq!(buf[0], 0x0A, "report ID = EFFECT_OPERATION");
        assert_eq!(buf[2], expected_val, "op byte for {op:?}");
    }
    Ok(())
}

#[test]
fn pidff_block_free_encoding() -> Result<(), Box<dyn std::error::Error>> {
    let buf = encode_block_free(99);
    assert_eq!(buf[0], 0x0B, "report ID = BLOCK_FREE");
    assert_eq!(buf[1], 99, "block index");
    Ok(())
}

#[test]
fn pidff_device_control_all_commands() -> Result<(), Box<dyn std::error::Error>> {
    let commands: &[(u8, &str)] = &[
        (0x01, "enable actuators"),
        (0x02, "disable actuators"),
        (0x04, "stop all effects"),
        (0x08, "device reset"),
        (0x10, "device pause"),
        (0x20, "device continue"),
    ];
    for &(cmd, label) in commands {
        let buf = encode_device_control(cmd);
        assert_eq!(buf[0], 0x0C, "report ID for {label}");
        assert_eq!(buf[1], cmd, "command byte for {label}");
    }
    Ok(())
}

#[test]
fn pidff_device_gain_clamps_to_10000() -> Result<(), Box<dyn std::error::Error>> {
    let buf_normal = encode_device_gain(5000);
    assert_eq!(u16::from_le_bytes([buf_normal[2], buf_normal[3]]), 5000);

    let buf_over = encode_device_gain(15000);
    assert_eq!(u16::from_le_bytes([buf_over[2], buf_over[3]]), 10000);

    let buf_zero = encode_device_gain(0);
    assert_eq!(u16::from_le_bytes([buf_zero[2], buf_zero[3]]), 0);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// 2. Input report — field isolation
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn parse_clutch_full() -> Result<(), ParseError> {
    let data = make_input_report(0, 0, 0, 0, 0, u16::MAX, 0);
    let report = parse(&data)?;
    assert!((report.clutch - 1.0).abs() < 0.01, "clutch should be 1.0");
    assert!(report.handbrake.abs() < 0.01, "handbrake should be 0.0");
    Ok(())
}

#[test]
fn parse_handbrake_full() -> Result<(), ParseError> {
    let data = make_input_report(0, 0, 0, 0, 0, 0, u16::MAX);
    let report = parse(&data)?;
    assert!((report.handbrake - 1.0).abs() < 0.01);
    assert!(report.clutch.abs() < 0.01);
    Ok(())
}

#[test]
fn parse_all_axes_mid_range() -> Result<(), ParseError> {
    let mid: u16 = u16::MAX / 2;
    let data = make_input_report(0, mid, mid, 0, 0, mid, mid);
    let report = parse(&data)?;
    assert!((report.throttle - 0.5).abs() < 0.01);
    assert!((report.brake - 0.5).abs() < 0.01);
    assert!((report.clutch - 0.5).abs() < 0.01);
    assert!((report.handbrake - 0.5).abs() < 0.01);
    Ok(())
}

#[test]
fn parse_steering_center_is_zero() -> Result<(), ParseError> {
    let data = make_input_report(0, 0, 0, 0, 0, 0, 0);
    let report = parse(&data)?;
    assert!(report.steering.abs() < 0.01);
    Ok(())
}

#[test]
fn parse_buttons_individual_bits() -> Result<(), ParseError> {
    let data = make_input_report(0, 0, 0, 0b1010_0101, 0b0101_1010, 0, 0);
    let report = parse(&data)?;
    assert_eq!(report.buttons, 0b0101_1010_1010_0101);
    Ok(())
}

#[test]
fn parse_steering_i16_min_clamps_to_neg1() -> Result<(), ParseError> {
    let data = make_input_report(i16::MIN, 0, 0, 0, 0, 0, 0);
    let report = parse(&data)?;
    // i16::MIN / i16::MAX overflows past -1.0, clamped to -1.0
    assert!(
        (report.steering + 1.0).abs() < 0.01,
        "i16::MIN steering should clamp to -1.0, got {}",
        report.steering
    );
    Ok(())
}

#[test]
fn parse_isolates_each_axis() -> Result<(), ParseError> {
    // Only throttle set
    let data = make_input_report(0, u16::MAX, 0, 0, 0, 0, 0);
    let report = parse(&data)?;
    assert!((report.throttle - 1.0).abs() < 0.01);
    assert!(report.brake.abs() < 0.01);
    assert!(report.clutch.abs() < 0.01);
    assert!(report.handbrake.abs() < 0.01);

    // Only brake set
    let data = make_input_report(0, 0, u16::MAX, 0, 0, 0, 0);
    let report = parse(&data)?;
    assert!(report.throttle.abs() < 0.01);
    assert!((report.brake - 1.0).abs() < 0.01);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// 3. Direct torque wire-level byte verification
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn torque_half_positive_wire_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let report = encode_torque(0.5);
    assert_eq!(report.len(), FFB_REPORT_LEN);
    assert_eq!(report[0], FFB_REPORT_ID);
    let raw = i16::from_le_bytes([report[1], report[2]]);
    // 0.5 * 32767 = 16383 (truncated)
    assert_eq!(raw, (0.5_f32 * i16::MAX as f32) as i16);
    assert_eq!(report[3], MODE_GAME);
    assert_eq!(&report[4..], &[0, 0, 0, 0]);
    Ok(())
}

#[test]
fn torque_half_negative_wire_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let report = encode_torque(-0.5);
    let raw = i16::from_le_bytes([report[1], report[2]]);
    assert_eq!(raw, (-0.5_f32 * i16::MAX as f32) as i16);
    Ok(())
}

#[test]
fn stop_command_wire_bytes_exact() -> Result<(), Box<dyn std::error::Error>> {
    let report = encode_stop();
    assert_eq!(report, [FFB_REPORT_ID, 0x00, 0x00, MODE_GAME, 0, 0, 0, 0]);
    Ok(())
}

#[test]
fn torque_tiny_positive_above_zero() -> Result<(), Box<dyn std::error::Error>> {
    let report = encode_torque(0.001);
    let raw = i16::from_le_bytes([report[1], report[2]]);
    // Even a tiny positive torque should yield a non-negative raw
    assert!(
        raw >= 0,
        "tiny positive torque should not go negative: {raw}"
    );
    Ok(())
}

#[test]
fn torque_tiny_negative_below_zero() -> Result<(), Box<dyn std::error::Error>> {
    let report = encode_torque(-0.001);
    let raw = i16::from_le_bytes([report[1], report[2]]);
    assert!(
        raw <= 0,
        "tiny negative torque should not go positive: {raw}"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// 4. VID/PID cross-consistency
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn all_pids_are_nonzero() {
    for pid in [
        PRODUCT_C5,
        PRODUCT_C12,
        PRODUCT_CP5_PEDALS,
        PRODUCT_LC100_PEDALS,
    ] {
        assert_ne!(pid, 0, "PID must not be zero");
    }
}

#[test]
fn wheel_pids_in_low_range_pedal_pids_in_high_range() {
    // C5/C12 are 0x03xx, pedals are 0x10xx — distinct ranges
    const { assert!(PRODUCT_C5 < 0x1000) };
    const { assert!(PRODUCT_C12 < 0x1000) };
    const { assert!(PRODUCT_CP5_PEDALS >= 0x1000) };
    const { assert!(PRODUCT_LC100_PEDALS >= 0x1000) };
}

#[test]
fn is_cammus_matches_model_from_pid() -> Result<(), Box<dyn std::error::Error>> {
    let known = [
        PRODUCT_C5,
        PRODUCT_C12,
        PRODUCT_CP5_PEDALS,
        PRODUCT_LC100_PEDALS,
    ];
    for pid in known {
        assert!(
            is_cammus(VENDOR_ID, pid),
            "is_cammus should accept PID 0x{pid:04X}"
        );
        assert!(
            CammusModel::from_pid(pid).is_some(),
            "CammusModel::from_pid should resolve PID 0x{pid:04X}"
        );
    }
    Ok(())
}

#[test]
fn product_name_matches_model_name() -> Result<(), Box<dyn std::error::Error>> {
    let pids = [
        PRODUCT_C5,
        PRODUCT_C12,
        PRODUCT_CP5_PEDALS,
        PRODUCT_LC100_PEDALS,
    ];
    for pid in pids {
        let pname = product_name(pid).ok_or_else(|| format!("no product name for 0x{pid:04X}"))?;
        let model =
            CammusModel::from_pid(pid).ok_or_else(|| format!("no model for 0x{pid:04X}"))?;
        assert_eq!(
            pname,
            model.name(),
            "product_name and model.name() must agree for PID 0x{pid:04X}"
        );
    }
    Ok(())
}

#[test]
fn model_torque_non_negative() {
    let models = [
        CammusModel::C5,
        CammusModel::C12,
        CammusModel::Cp5Pedals,
        CammusModel::Lc100Pedals,
    ];
    for m in models {
        assert!(m.max_torque_nm() >= 0.0, "{:?} torque must be >= 0", m);
    }
}

#[test]
fn c12_torque_greater_than_c5() {
    assert!(
        CammusModel::C12.max_torque_nm() > CammusModel::C5.max_torque_nm(),
        "C12 (12Nm) > C5 (5Nm)"
    );
}

#[test]
fn pedal_models_have_zero_torque() {
    assert!((CammusModel::Cp5Pedals.max_torque_nm()).abs() < 0.001);
    assert!((CammusModel::Lc100Pedals.max_torque_nm()).abs() < 0.001);
}

// ═══════════════════════════════════════════════════════════════════════════
// 5. Error formatting
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn parse_error_display_too_short() {
    let err = ParseError::TooShort { got: 5, need: 12 };
    let msg = format!("{err}");
    assert!(msg.contains("5"), "should mention got");
    assert!(msg.contains("12"), "should mention need");
}

#[test]
fn parse_error_eq_impl() {
    let a = ParseError::TooShort { got: 3, need: 12 };
    let b = ParseError::TooShort { got: 3, need: 12 };
    assert_eq!(a, b);
    let c = ParseError::TooShort { got: 4, need: 12 };
    assert_ne!(a, c);
}

// ═══════════════════════════════════════════════════════════════════════════
// 6. Report constants cross-checks
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn report_len_is_standard_hid_size() {
    assert_eq!(REPORT_LEN, 64, "standard 64-byte HID report");
}

#[test]
fn ffb_report_len_is_8() {
    assert_eq!(FFB_REPORT_LEN, 8, "direct torque output is 8 bytes");
}

#[test]
fn steering_range_is_1080_degrees() {
    assert!(
        (STEERING_RANGE_DEG - 1080.0).abs() < 0.01,
        "±540° = 1080° total"
    );
}

#[test]
fn mode_constants_are_distinct_and_correct() {
    assert_ne!(MODE_GAME, MODE_CONFIG);
    assert_eq!(MODE_GAME, 0x01);
    assert_eq!(MODE_CONFIG, 0x00);
}

#[test]
fn report_ids_match() {
    assert_eq!(
        REPORT_ID, FFB_REPORT_ID,
        "input and output share report ID 0x01"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// 7. PIDFF effect type coverage via Cammus re-export
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn all_pid_effect_types_have_distinct_values() -> Result<(), Box<dyn std::error::Error>> {
    let types: &[(PidEffectType, u8)] = &[
        (PidEffectType::Constant, 1),
        (PidEffectType::Ramp, 2),
        (PidEffectType::Square, 3),
        (PidEffectType::Sine, 4),
        (PidEffectType::Triangle, 5),
        (PidEffectType::SawtoothUp, 6),
        (PidEffectType::SawtoothDown, 7),
        (PidEffectType::Spring, 8),
        (PidEffectType::Damper, 9),
        (PidEffectType::Inertia, 10),
        (PidEffectType::Friction, 11),
    ];
    for (et, expected) in types {
        assert_eq!(*et as u8, *expected, "{et:?} should be {expected}");
    }
    Ok(())
}

#[test]
fn set_effect_with_each_type() -> Result<(), Box<dyn std::error::Error>> {
    let types = [
        PidEffectType::Constant,
        PidEffectType::Ramp,
        PidEffectType::Square,
        PidEffectType::Sine,
        PidEffectType::Triangle,
        PidEffectType::SawtoothUp,
        PidEffectType::SawtoothDown,
        PidEffectType::Spring,
        PidEffectType::Damper,
        PidEffectType::Inertia,
        PidEffectType::Friction,
    ];
    for et in types {
        let buf = encode_set_effect(1, et, 1000, 255, 0);
        assert_eq!(buf[2], et as u8);
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// 8. Proptest fuzzing
// ═══════════════════════════════════════════════════════════════════════════

mod proptest_deep {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(500))]

        /// Any 12+ byte slice must parse without panic.
        #[test]
        fn prop_input_report_parse_never_panics(data in proptest::collection::vec(any::<u8>(), 12..=128)) {
            let _ = parse(&data);
        }

        /// Slices shorter than 12 must always return an error.
        #[test]
        fn prop_input_report_rejects_short(data in proptest::collection::vec(any::<u8>(), 0..12)) {
            let result = parse(&data);
            prop_assert!(result.is_err());
        }

        /// Parsed steering is always clamped to [-1.0, 1.0].
        #[test]
        fn prop_steering_clamped(data in proptest::collection::vec(any::<u8>(), 12..=64)) {
            if let Ok(report) = parse(&data) {
                prop_assert!((-1.0..=1.0).contains(&report.steering),
                    "steering={} out of range", report.steering);
            }
        }

        /// Parsed throttle is always in [0.0, 1.0].
        #[test]
        fn prop_throttle_clamped(data in proptest::collection::vec(any::<u8>(), 12..=64)) {
            if let Ok(report) = parse(&data) {
                prop_assert!((0.0..=1.0).contains(&report.throttle),
                    "throttle={} out of range", report.throttle);
            }
        }

        /// Parsed brake is always in [0.0, 1.0].
        #[test]
        fn prop_brake_clamped(data in proptest::collection::vec(any::<u8>(), 12..=64)) {
            if let Ok(report) = parse(&data) {
                prop_assert!((0.0..=1.0).contains(&report.brake),
                    "brake={} out of range", report.brake);
            }
        }

        /// Parsed clutch is always in [0.0, 1.0].
        #[test]
        fn prop_clutch_clamped(data in proptest::collection::vec(any::<u8>(), 12..=64)) {
            if let Ok(report) = parse(&data) {
                prop_assert!((0.0..=1.0).contains(&report.clutch),
                    "clutch={} out of range", report.clutch);
            }
        }

        /// Parsed handbrake is always in [0.0, 1.0].
        #[test]
        fn prop_handbrake_clamped(data in proptest::collection::vec(any::<u8>(), 12..=64)) {
            if let Ok(report) = parse(&data) {
                prop_assert!((0.0..=1.0).contains(&report.handbrake),
                    "handbrake={} out of range", report.handbrake);
            }
        }

        /// Steering roundtrip: write i16 LE, parse, verify sign.
        #[test]
        fn prop_steering_sign_preserved(val in -32767i16..=32767i16) {
            let data = make_input_report(val, 0, 0, 0, 0, 0, 0);
            if let Ok(report) = parse(&data) {
                if val > 100 {
                    prop_assert!(report.steering > 0.0);
                } else if val < -100 {
                    prop_assert!(report.steering < 0.0);
                }
            }
        }

        /// Torque encoding: any f32 input produces a valid 8-byte report.
        #[test]
        fn prop_encode_torque_valid_structure(torque in -100.0f32..=100.0f32) {
            let r = encode_torque(torque);
            prop_assert_eq!(r.len(), FFB_REPORT_LEN);
            prop_assert_eq!(r[0], FFB_REPORT_ID);
            prop_assert_eq!(r[3], MODE_GAME);
            prop_assert_eq!(&r[4..], &[0u8, 0, 0, 0]);
        }

        /// Torque encoding saturates properly.
        #[test]
        fn prop_encode_torque_saturates(torque in -100.0f32..=100.0f32) {
            let r = encode_torque(torque);
            let raw = i16::from_le_bytes([r[1], r[2]]);
            if torque >= 1.0 {
                prop_assert_eq!(raw, i16::MAX);
            } else if torque <= -1.0 {
                prop_assert_eq!(raw, -i16::MAX);
            }
        }

        /// PIDFF constant force magnitude roundtrip via Cammus re-export.
        #[test]
        fn prop_pidff_constant_force_roundtrip(block in 0u8..=255u8, mag in any::<i16>()) {
            let buf = encode_set_constant_force(block, mag);
            prop_assert_eq!(buf[1], block);
            prop_assert_eq!(i16::from_le_bytes([buf[2], buf[3]]), mag);
        }

        /// PIDFF device gain is always clamped to [0, 10000].
        #[test]
        fn prop_pidff_device_gain_bounded(gain in any::<u16>()) {
            let buf = encode_device_gain(gain);
            let encoded = u16::from_le_bytes([buf[2], buf[3]]);
            prop_assert!(encoded <= 10000);
        }

        /// is_cammus is deterministic.
        #[test]
        fn prop_is_cammus_deterministic(vid in any::<u16>(), pid in any::<u16>()) {
            prop_assert_eq!(is_cammus(vid, pid), is_cammus(vid, pid));
        }

        /// Buttons roundtrip: lo/hi bytes reconstruct u16.
        #[test]
        fn prop_buttons_roundtrip(lo in any::<u8>(), hi in any::<u8>()) {
            let data = make_input_report(0, 0, 0, lo, hi, 0, 0);
            if let Ok(report) = parse(&data) {
                let expected = (lo as u16) | ((hi as u16) << 8);
                prop_assert_eq!(report.buttons, expected);
            }
        }

        /// Throttle roundtrip: u16 LE encodes and normalises consistently.
        #[test]
        fn prop_throttle_roundtrip(raw in any::<u16>()) {
            let data = make_input_report(0, raw, 0, 0, 0, 0, 0);
            if let Ok(report) = parse(&data) {
                let expected = (raw as f32 / u16::MAX as f32).clamp(0.0, 1.0);
                prop_assert!((report.throttle - expected).abs() < 0.001,
                    "throttle mismatch: {} vs {}", report.throttle, expected);
            }
        }
    }
}

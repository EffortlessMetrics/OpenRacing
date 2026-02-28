//! Property-based tests for Logitech HID output report generation and input
//! report parsing.
//!
//! Uses proptest with 500 cases to verify invariants that hold across the full
//! input domain, complementing the snapshot and unit tests in the crate.

use proptest::prelude::*;
use racing_wheel_hid_logitech_protocol::{
    CONSTANT_FORCE_REPORT_LEN, LogitechConstantForceEncoder, LogitechModel, parse_input_report,
};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    // ── Input parsing: never panics ───────────────────────────────────────────

    /// Parsing any arbitrary byte sequence must never panic.
    #[test]
    fn prop_parse_never_panics(
        data in proptest::collection::vec(proptest::num::u8::ANY, 0..=64usize),
    ) {
        let _ = parse_input_report(&data);
    }

    // ── Input parsing: steering range ─────────────────────────────────────────

    /// When parse succeeds, the steering value must be in [-1.0, +1.0].
    #[test]
    fn prop_steering_in_valid_range(
        steer_lsb: u8,
        steer_msb: u8,
        rest in proptest::collection::vec(proptest::num::u8::ANY, 8usize),
    ) {
        let mut data = vec![0x01u8, steer_lsb, steer_msb];
        data.extend_from_slice(&rest);
        if let Some(state) = parse_input_report(&data) {
            prop_assert!(
                state.steering >= -1.0 && state.steering <= 1.0,
                "steering {} out of [-1.0, 1.0]",
                state.steering
            );
        }
    }

    // ── Input parsing: axis range ─────────────────────────────────────────────

    /// When parse succeeds, throttle/brake/clutch must each be in [0.0, 1.0].
    #[test]
    fn prop_axes_in_unit_range(
        data in proptest::collection::vec(proptest::num::u8::ANY, 10usize..=16usize),
    ) {
        let mut d = data;
        d[0] = 0x01;
        if let Some(state) = parse_input_report(&d) {
            prop_assert!(
                state.throttle >= 0.0 && state.throttle <= 1.0,
                "throttle {} not in [0.0, 1.0]",
                state.throttle
            );
            prop_assert!(
                state.brake >= 0.0 && state.brake <= 1.0,
                "brake {} not in [0.0, 1.0]",
                state.brake
            );
            prop_assert!(
                state.clutch >= 0.0 && state.clutch <= 1.0,
                "clutch {} not in [0.0, 1.0]",
                state.clutch
            );
        }
    }

    // ── Input parsing: hat lower nibble ───────────────────────────────────────

    /// Hat switch must always be in [0x0, 0xF] (lower nibble only).
    #[test]
    fn prop_hat_lower_nibble(
        data in proptest::collection::vec(proptest::num::u8::ANY, 12usize),
    ) {
        let mut d = data;
        d[0] = 0x01;
        if let Some(state) = parse_input_report(&d) {
            prop_assert!(
                state.hat <= 0x0F,
                "hat 0x{:02X} must be in lower nibble (≤ 0x0F)",
                state.hat
            );
        }
    }

    // ── Input parsing: paddle bits ────────────────────────────────────────────

    /// Paddle bits must be in 0..=3 (two-bit field).
    #[test]
    fn prop_paddles_two_bits(
        data in proptest::collection::vec(proptest::num::u8::ANY, 12usize),
    ) {
        let mut d = data;
        d[0] = 0x01;
        if let Some(state) = parse_input_report(&d) {
            prop_assert!(
                state.paddles <= 3,
                "paddles must be 0..=3, got {}",
                state.paddles
            );
        }
    }

    // ── Input parsing: reject non-0x01 report ID ─────────────────────────────

    /// Any report that does not start with 0x01 must not parse successfully.
    #[test]
    fn prop_wrong_id_returns_none(
        id in 0x02u8..=0xFFu8,
        tail in proptest::collection::vec(proptest::num::u8::ANY, 11usize),
    ) {
        let mut buf = vec![id];
        buf.extend_from_slice(&tail);
        prop_assert!(
            parse_input_report(&buf).is_none(),
            "report with ID 0x{:02X} must return None",
            id
        );
    }

    // ── Encoder: monotonicity ─────────────────────────────────────────────────

    /// Within the clamped range, the encoder must be monotone: larger torque →
    /// larger or equal encoded magnitude.
    #[test]
    fn prop_encoder_monotone(
        max_torque in 0.01f32..=50.0f32,
        frac_a in -1.0f32..=1.0f32,
        frac_b in -1.0f32..=1.0f32,
    ) {
        let ta = max_torque * frac_a;
        let tb = max_torque * frac_b;
        let enc = LogitechConstantForceEncoder::new(max_torque);
        let mut out_a = [0u8; CONSTANT_FORCE_REPORT_LEN];
        let mut out_b = [0u8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode(ta, &mut out_a);
        enc.encode(tb, &mut out_b);
        let mag_a = i16::from_le_bytes([out_a[2], out_a[3]]);
        let mag_b = i16::from_le_bytes([out_b[2], out_b[3]]);
        if ta > tb {
            prop_assert!(
                mag_a >= mag_b,
                "monotone violated: encode({ta}, max={max_torque}) = {mag_a} < encode({tb}) = {mag_b}"
            );
        }
    }

    // ── Encoder: encode_zero sets correct report ID ───────────────────────────

    /// encode_zero must always produce the correct report ID and effect block
    /// index even when the buffer was previously filled with non-zero bytes.
    #[test]
    fn prop_encode_zero_report_id(max_torque in 0.01f32..=50.0f32) {
        let enc = LogitechConstantForceEncoder::new(max_torque);
        let mut out = [0xFFu8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode_zero(&mut out);
        prop_assert_eq!(out[0], 0x12u8, "encode_zero byte 0 must be report ID 0x12");
        prop_assert_eq!(out[1], 1u8, "encode_zero byte 1 must be effect block index 1");
        prop_assert_eq!(out[2], 0u8, "encode_zero must zero magnitude low byte");
        prop_assert_eq!(out[3], 0u8, "encode_zero must zero magnitude high byte");
    }

    // ── Model: determinism ────────────────────────────────────────────────────

    /// LogitechModel::from_product_id is a pure function; same PID must always
    /// produce the same variant.
    #[test]
    fn prop_model_detection_deterministic(pid in 0u16..=65535u16) {
        let a = LogitechModel::from_product_id(pid);
        let b = LogitechModel::from_product_id(pid);
        prop_assert_eq!(
            a, b,
            "from_product_id must be deterministic for PID 0x{:04X}",
            pid
        );
    }

    // ── Model: max_torque_nm is always positive ───────────────────────────────

    /// All Logitech models (including Unknown) must report a positive peak torque.
    #[test]
    fn prop_model_torque_positive(pid in 0u16..=65535u16) {
        let model = LogitechModel::from_product_id(pid);
        prop_assert!(
            model.max_torque_nm() > 0.0,
            "model {:?} for PID 0x{:04X} must have positive max torque",
            model,
            pid
        );
    }

    // ── Model: max_rotation_deg is always 900 ────────────────────────────────

    /// Every Logitech model uses a 900° rotation range.
    #[test]
    fn prop_model_rotation_is_900(pid in 0u16..=65535u16) {
        let model = LogitechModel::from_product_id(pid);
        prop_assert_eq!(
            model.max_rotation_deg(),
            900u16,
            "model {:?} must report 900° rotation range",
            model
        );
    }
}

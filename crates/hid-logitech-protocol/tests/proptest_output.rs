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

    // ── Model: max_rotation_deg is valid ─────────────────────────────────────

    /// Every Logitech model reports a valid rotation range (180°, 270°, 900° or 1080°).
    #[test]
    fn prop_model_rotation_is_valid(pid in 0u16..=65535u16) {
        let model = LogitechModel::from_product_id(pid);
        let deg = model.max_rotation_deg();
        prop_assert!(
            deg == 180 || deg == 270 || deg == 900 || deg == 1080,
            "model {:?} must report 180°, 270°, 900° or 1080° rotation, got {}°",
            model,
            deg
        );
    }
}

// ── Kernel-verified protocol property tests ──────────────────────────────

use racing_wheel_hid_logitech_protocol::{
    VENDOR_REPORT_LEN, build_mode_switch_report, build_set_range_dfp_reports,
};

/// Strategy that produces every `LogitechModel` variant uniformly.
fn arb_logitech_model() -> impl Strategy<Value = LogitechModel> {
    prop_oneof![
        Just(LogitechModel::WingManFormulaForce),
        Just(LogitechModel::MOMO),
        Just(LogitechModel::DrivingForceEX),
        Just(LogitechModel::DrivingForcePro),
        Just(LogitechModel::DrivingForceGT),
        Just(LogitechModel::SpeedForceWireless),
        Just(LogitechModel::VibrationWheel),
        Just(LogitechModel::G25),
        Just(LogitechModel::G27),
        Just(LogitechModel::G29),
        Just(LogitechModel::G920),
        Just(LogitechModel::G923),
        Just(LogitechModel::GPro),
        Just(LogitechModel::Unknown),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    // ── build_set_range_dfp_report: correct command byte ─────────────────

    /// DFP range reports must have correct structure for both coarse and fine commands.
    #[test]
    fn prop_dfp_range_reports_structure(degrees in 0u16..=2000u16) {
        let [coarse, fine] = build_set_range_dfp_reports(degrees);
        prop_assert_eq!(coarse.len(), VENDOR_REPORT_LEN,
            "DFP coarse report must be 7 bytes");
        prop_assert_eq!(fine.len(), VENDOR_REPORT_LEN,
            "DFP fine report must be 7 bytes");
        prop_assert_eq!(coarse[0], 0xF8, "coarse byte 0 must be VENDOR report ID");
        prop_assert!(coarse[1] == 0x02 || coarse[1] == 0x03,
            "coarse byte 1 must be 0x02 or 0x03, got {:#04x}", coarse[1]);
        prop_assert_eq!(fine[0], 0x81, "fine byte 0 must be 0x81");
        prop_assert_eq!(fine[1], 0x0b, "fine byte 1 must be 0x0b");
    }

    // ── build_mode_switch_report: returns correct structure ──────────────

    /// Mode-switch report must start with VENDOR report ID and MODE_SWITCH
    /// command, be 7 bytes, and encode mode_id and detach correctly.
    #[test]
    fn prop_mode_switch_report_structure(mode_id: u8, detach: bool) {
        let r = build_mode_switch_report(mode_id, detach);
        prop_assert_eq!(r.len(), VENDOR_REPORT_LEN,
            "mode-switch report must be 7 bytes");
        prop_assert_eq!(r[0], 0xF8, "byte 0 must be VENDOR report ID (0xF8)");
        prop_assert_eq!(r[1], 0x09, "byte 1 must be MODE_SWITCH command (0x09)");
        prop_assert_eq!(r[2], mode_id, "byte 2 must be mode_id");
        prop_assert_eq!(r[3], 0x01, "byte 3 must be 0x01");
        let expected_detach = if detach { 0x01u8 } else { 0x00u8 };
        prop_assert_eq!(r[4], expected_detach,
            "byte 4 must be 0x01 if detach, 0x00 otherwise");
        prop_assert_eq!(&r[5..], &[0x00, 0x00],
            "bytes 5-6 must be zero");
    }

    // ── supports_hardware_friction: correct models ───────────────────────

    /// supports_hardware_friction must return true only for DFP, G25, DFGT, G27.
    #[test]
    fn prop_supports_hardware_friction(model in arb_logitech_model()) {
        let expected = matches!(
            model,
            LogitechModel::DrivingForcePro
                | LogitechModel::G25
                | LogitechModel::DrivingForceGT
                | LogitechModel::G27
        );
        prop_assert_eq!(model.supports_hardware_friction(), expected,
            "supports_hardware_friction for {:?} should be {}", model, expected);
    }

    // ── supports_range_command: correct models ───────────────────────────

    /// supports_range_command must return true for DFP and above (DFP, G25,
    /// DFGT, G27, G29, G920, G923, GPro) and false for older models.
    #[test]
    fn prop_supports_range_command(model in arb_logitech_model()) {
        let expected = matches!(
            model,
            LogitechModel::DrivingForcePro
                | LogitechModel::G25
                | LogitechModel::DrivingForceGT
                | LogitechModel::G27
                | LogitechModel::G29
                | LogitechModel::G920
                | LogitechModel::G923
                | LogitechModel::GPro
        );
        prop_assert_eq!(model.supports_range_command(), expected,
            "supports_range_command for {:?} should be {}", model, expected);
    }

    /// Models without range command support: WingMan, MOMO, DrivingForceEX,
    /// SpeedForceWireless, VibrationWheel, Unknown must return false.
    #[test]
    fn prop_no_range_for_legacy_models(model in arb_logitech_model()) {
        let is_legacy = matches!(
            model,
            LogitechModel::WingManFormulaForce
                | LogitechModel::MOMO
                | LogitechModel::DrivingForceEX
                | LogitechModel::SpeedForceWireless
                | LogitechModel::VibrationWheel
                | LogitechModel::Unknown
        );
        if is_legacy {
            prop_assert!(!model.supports_range_command(),
                "legacy model {:?} must not support range command", model);
        }
    }
}

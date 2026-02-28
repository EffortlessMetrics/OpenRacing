use proptest::prelude::*;
use racing_wheel_hid_thrustmaster_protocol as tm;

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(500))]

    // ── Torque / FFB encoding properties ─────────────────────────────────────

    #[test]
    fn prop_torque_sign_preserved(
        max in 0.1_f32..=21.0_f32,
        frac in -1.0_f32..=1.0_f32,
    ) {
        let torque_nm = max * frac;
        let enc = tm::ThrustmasterConstantForceEncoder::new(max);
        let mut out = [0u8; tm::EFFECT_REPORT_LEN];
        enc.encode(torque_nm, &mut out);
        let raw = i16::from_le_bytes([out[2], out[3]]);
        if torque_nm > 0.001 {
            prop_assert!(raw >= 0, "positive torque {torque_nm} must yield positive raw {raw}");
        } else if torque_nm < -0.001 {
            prop_assert!(raw <= 0, "negative torque {torque_nm} must yield negative raw {raw}");
        }
    }

    #[test]
    fn prop_encoded_value_within_bounds(
        max in 0.001_f32..=21.0_f32,
        torque in -100.0_f32..=100.0_f32,
    ) {
        let enc = tm::ThrustmasterConstantForceEncoder::new(max);
        let mut out = [0u8; tm::EFFECT_REPORT_LEN];
        enc.encode(torque, &mut out);
        let raw = i16::from_le_bytes([out[2], out[3]]);
        prop_assert!((-10000..=10000).contains(&(raw as i32)));
    }

    #[test]
    fn prop_encoding_saturates_at_max(
        max in 0.001_f32..=21.0_f32,
        torque in -100.0_f32..=100.0_f32,
    ) {
        let enc = tm::ThrustmasterConstantForceEncoder::new(max);
        let mut out = [0u8; tm::EFFECT_REPORT_LEN];
        enc.encode(torque, &mut out);
        let raw = i16::from_le_bytes([out[2], out[3]]);
        if torque > max {
            prop_assert_eq!(raw, 10000, "over-max torque must saturate to 10000");
        } else if torque < -max {
            prop_assert_eq!(raw, -10000, "under-max torque must saturate to -10000");
        }
    }

    #[test]
    fn prop_encoding_is_monotone(
        max in 0.1_f32..=21.0_f32,
        frac_a in -1.0_f32..=1.0_f32,
        frac_b in -1.0_f32..=1.0_f32,
    ) {
        let ta = max * frac_a;
        let tb = max * frac_b;
        let enc = tm::ThrustmasterConstantForceEncoder::new(max);
        let mut out_a = [0u8; tm::EFFECT_REPORT_LEN];
        let mut out_b = [0u8; tm::EFFECT_REPORT_LEN];
        enc.encode(ta, &mut out_a);
        enc.encode(tb, &mut out_b);
        let raw_a = i16::from_le_bytes([out_a[2], out_a[3]]);
        let raw_b = i16::from_le_bytes([out_b[2], out_b[3]]);
        if ta > tb {
            prop_assert!(
                raw_a >= raw_b,
                "monotone violation: torque {ta} > {tb} but encoded {raw_a} < {raw_b}"
            );
        }
    }

    #[test]
    fn prop_report_id_always_constant_force(
        max in 0.001_f32..=21.0_f32,
        torque in -100.0_f32..=100.0_f32,
    ) {
        let enc = tm::ThrustmasterConstantForceEncoder::new(max);
        let mut out = [0u8; tm::EFFECT_REPORT_LEN];
        enc.encode(torque, &mut out);
        prop_assert_eq!(out[0], 0x23u8, "report ID byte must always be CONSTANT_FORCE (0x23)");
    }

    // ── Model detection properties ────────────────────────────────────────────

    #[test]
    fn prop_model_detection_deterministic(pid in 0u16..=65535u16) {
        let model_a = tm::Model::from_product_id(pid);
        let model_b = tm::Model::from_product_id(pid);
        prop_assert_eq!(
            model_a, model_b,
            "Model::from_product_id must be deterministic for PID 0x{:04X}", pid
        );
    }

    #[test]
    fn prop_is_wheel_consistent_with_identify(pid in 0u16..=65535u16) {
        let is_wheel = tm::is_wheel_product(pid);
        let ident = tm::identify_device(pid);
        let category_is_wheel = matches!(ident.category, tm::ThrustmasterDeviceCategory::Wheelbase);
        prop_assert_eq!(
            is_wheel, category_is_wheel,
            "is_wheel_product and identify_device must agree for PID 0x{:04X}", pid
        );
    }

    #[test]
    fn prop_gain_report_roundtrip(gain in 0u8..=255u8) {
        let report = tm::build_device_gain(gain);
        prop_assert_eq!(report[1], gain, "gain byte must round-trip through build_device_gain");
    }

    #[test]
    fn prop_set_range_roundtrip(degrees in 200u16..=1080u16) {
        let report = tm::build_set_range_report(degrees);
        let decoded = u16::from_le_bytes([report[2], report[3]]);
        prop_assert_eq!(
            decoded, degrees,
            "build_set_range_report must round-trip degrees={}", degrees
        );
    }
}

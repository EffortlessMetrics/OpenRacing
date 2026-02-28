use proptest::prelude::*;
use racing_wheel_hid_moza_protocol::{
    MozaDirectTorqueEncoder, MozaRetryPolicy, REPORT_LEN, TorqueEncoder, es_compatibility,
    identify_device, is_wheelbase_product, product_ids,
};

// ── Torque encoder: sign preservation ───────────────────────────────────────

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(500))]

    /// For any torque in (-max, 0) ∪ (0, max), the encoded raw value has the
    /// same sign as the torque command.
    #[test]
    fn prop_torque_sign_preserved(
        max in 0.1_f32..=21.0_f32,
        frac in -1.0_f32..=1.0_f32,
    ) {
        let torque_nm = max * frac;
        let enc = MozaDirectTorqueEncoder::new(max);
        let mut out = [0u8; REPORT_LEN];
        enc.encode(torque_nm, 0, &mut out);
        let raw = i16::from_le_bytes([out[1], out[2]]);

        if torque_nm > 0.001 {
            prop_assert!(raw > 0, "positive torque {torque_nm} must yield positive raw {raw}");
        } else if torque_nm < -0.001 {
            prop_assert!(raw < 0, "negative torque {torque_nm} must yield negative raw {raw}");
        }
    }

    /// Encoded value never exceeds i16 range; over-range inputs saturate cleanly.
    #[test]
    fn prop_overflow_prevention(
        max in 0.001_f32..=21.0_f32,
        torque in -100.0_f32..=100.0_f32,
    ) {
        let enc = MozaDirectTorqueEncoder::new(max);
        let mut out = [0u8; REPORT_LEN];
        enc.encode(torque, 0, &mut out);
        let raw = i16::from_le_bytes([out[1], out[2]]);

        if torque > max {
            prop_assert_eq!(raw, i16::MAX, "over-max must saturate to i16::MAX");
        } else if torque < -max {
            prop_assert_eq!(raw, i16::MIN, "under-min must saturate to i16::MIN");
        } else if torque > 0.0 {
            prop_assert!(raw >= 0, "positive in-range torque must stay non-negative");
        } else if torque < 0.0 {
            prop_assert!(raw <= 0, "negative in-range torque must stay non-positive");
        }
    }

    /// Motor-enable bit (flags bit0) is set iff the encoded raw value is nonzero.
    #[test]
    fn prop_motor_enable_bit_iff_nonzero(
        max in 0.1_f32..=21.0_f32,
        frac in -1.0_f32..=1.0_f32,
    ) {
        let torque_nm = max * frac;
        let enc = MozaDirectTorqueEncoder::new(max);
        let mut out = [0u8; REPORT_LEN];
        enc.encode(torque_nm, 0, &mut out);
        let raw = i16::from_le_bytes([out[1], out[2]]);
        let motor_enabled = (out[3] & 0x01) != 0;

        prop_assert_eq!(
            motor_enabled,
            raw != 0,
            "motor-enable={} must match raw!=0 (raw={}, torque={})",
            motor_enabled,
            raw,
            torque_nm
        );
    }

    /// Encoding is monotone: if torque_a > torque_b (both in-range), raw_a >= raw_b.
    #[test]
    fn prop_encoding_is_monotone(
        max in 0.1_f32..=21.0_f32,
        frac_a in -1.0_f32..=1.0_f32,
        frac_b in -1.0_f32..=1.0_f32,
    ) {
        let ta = max * frac_a;
        let tb = max * frac_b;
        let enc = MozaDirectTorqueEncoder::new(max);
        let mut out_a = [0u8; REPORT_LEN];
        let mut out_b = [0u8; REPORT_LEN];
        enc.encode(ta, 0, &mut out_a);
        enc.encode(tb, 0, &mut out_b);
        let raw_a = i16::from_le_bytes([out_a[1], out_a[2]]);
        let raw_b = i16::from_le_bytes([out_b[1], out_b[2]]);

        if ta > tb {
            prop_assert!(raw_a >= raw_b, "monotone violation: {ta} > {tb} but raw {raw_a} < {raw_b}");
        } else if ta < tb {
            prop_assert!(raw_a <= raw_b, "monotone violation: {ta} < {tb} but raw {raw_a} > {raw_b}");
        }
    }

    /// Report ID byte is always DIRECT_TORQUE (0x20) regardless of input.
    #[test]
    fn prop_report_id_always_correct(
        max in 0.001_f32..=21.0_f32,
        torque in -100.0_f32..=100.0_f32,
    ) {
        let enc = MozaDirectTorqueEncoder::new(max);
        let mut out = [0u8; REPORT_LEN];
        enc.encode(torque, 0, &mut out);
        prop_assert_eq!(out[0], 0x20u8);
    }

    /// Encode length is always exactly REPORT_LEN.
    #[test]
    fn prop_encode_len_always_report_len(
        max in 0.001_f32..=21.0_f32,
        torque in -100.0_f32..=100.0_f32,
    ) {
        let enc = MozaDirectTorqueEncoder::new(max);
        let mut out = [0u8; REPORT_LEN];
        let len = enc.encode(torque, 0, &mut out);
        prop_assert_eq!(len, REPORT_LEN);
    }

    /// TorqueEncoder trait: clamp_max >= 0 and clamp_min <= 0, with clamp_min == -clamp_max.
    #[test]
    fn prop_torque_encoder_clamp_symmetry(max in 0.001_f32..=21.0_f32) {
        let enc = MozaDirectTorqueEncoder::new(max);
        let cmax = TorqueEncoder::clamp_max(&enc);
        let cmin = TorqueEncoder::clamp_min(&enc);
        prop_assert!(cmax >= 0, "clamp_max must be >= 0, got {cmax}");
        prop_assert!(cmin <= 0, "clamp_min must be <= 0, got {cmin}");
        prop_assert_eq!(cmin, -cmax, "clamp_min must equal -clamp_max");
    }

    /// identify_device category must be consistent with is_wheelbase_product.
    #[test]
    fn prop_identify_device_wheelbase_consistency(pid in 0u16..=0xFFFF_u16) {
        use racing_wheel_hid_moza_protocol::MozaDeviceCategory;
        let identity = identify_device(pid);
        let is_wb = is_wheelbase_product(pid);
        let cat_is_wb = matches!(identity.category, MozaDeviceCategory::Wheelbase);
        prop_assert_eq!(
            is_wb,
            cat_is_wb,
            "is_wheelbase_product({}) = {} but category = {:?}",
            pid,
            is_wb,
            identity.category
        );
    }

    /// For any wheelbase PID, supports_ffb must be true.
    #[test]
    fn prop_wheelbase_always_supports_ffb(pid in 0u16..=0xFFFF_u16) {
        let identity = identify_device(pid);
        if is_wheelbase_product(pid) {
            prop_assert!(identity.supports_ffb, "wheelbase pid={pid} must support FFB");
        }
    }

    /// MozaRetryPolicy delay grows monotonically with attempt index.
    #[test]
    fn prop_retry_delay_monotone(
        base_ms in 1u32..=1000_u32,
        attempt_a in 0u8..=3_u8,
        attempt_b in 0u8..=3_u8,
    ) {
        let policy = MozaRetryPolicy { max_retries: 10, base_delay_ms: base_ms };
        let delay_a = policy.delay_ms_for(attempt_a);
        let delay_b = policy.delay_ms_for(attempt_b);
        if attempt_a < attempt_b {
            prop_assert!(delay_a <= delay_b, "delay must be monotone: attempt {attempt_a}={delay_a}ms > attempt {attempt_b}={delay_b}ms");
        }
    }

    /// MozaModel::max_torque_nm is always non-negative and bounded by 25 Nm.
    #[test]
    fn prop_model_max_torque_bounded(pid in 0u16..=0xFFFF_u16) {
        use racing_wheel_hid_moza_protocol::MozaModel;
        let model = MozaModel::from_pid(pid);
        let max_nm = model.max_torque_nm();
        prop_assert!(max_nm >= 0.0, "max_torque_nm must be >= 0, got {max_nm}");
        prop_assert!(max_nm <= 25.0, "max_torque_nm must be <= 25 Nm, got {max_nm}");
    }

    /// es_compatibility for non-wheelbase PIDs is always NotWheelbase.
    #[test]
    fn prop_es_compatibility_not_wheelbase_for_peripherals(pid in 0u16..=0xFFFF_u16) {
        use racing_wheel_hid_moza_protocol::MozaEsCompatibility;
        if !is_wheelbase_product(pid) {
            let compat = es_compatibility(pid);
            prop_assert_eq!(
                compat,
                MozaEsCompatibility::NotWheelbase,
                "non-wheelbase pid={} must have NotWheelbase compatibility",
                pid
            );
        }
    }
}

// ── Known-PID round-trip checks ──────────────────────────────────────────────

#[test]
fn known_wheelbase_pids_all_return_true() {
    let pids = [
        product_ids::R3_V1,
        product_ids::R3_V2,
        product_ids::R5_V1,
        product_ids::R5_V2,
        product_ids::R9_V1,
        product_ids::R9_V2,
        product_ids::R12_V1,
        product_ids::R12_V2,
        product_ids::R16_R21_V1,
        product_ids::R16_R21_V2,
    ];
    for pid in pids {
        assert!(
            is_wheelbase_product(pid),
            "expected is_wheelbase_product(0x{pid:04X}) == true"
        );
    }
}

#[test]
fn known_peripheral_pids_all_return_false() {
    let pids = [
        product_ids::SR_P_PEDALS,
        product_ids::HGP_SHIFTER,
        product_ids::SGP_SHIFTER,
        product_ids::HBP_HANDBRAKE,
    ];
    for pid in pids {
        assert!(
            !is_wheelbase_product(pid),
            "expected is_wheelbase_product(0x{pid:04X}) == false"
        );
    }
}

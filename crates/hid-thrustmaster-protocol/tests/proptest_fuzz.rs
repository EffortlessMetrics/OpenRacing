//! Fuzz-style property tests for Thrustmaster protocol edge cases.
//!
//! Feeds arbitrary byte sequences and extreme values into all parse and encode
//! entry points to ensure no panics occur and all outputs are well-formed.

use proptest::prelude::*;
use racing_wheel_hid_thrustmaster_protocol as tm;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    // ── Arbitrary-bytes fuzz: parse_pedal_report ─────────────────────────

    /// Feeding any byte sequence to parse_pedal_report must never panic.
    #[test]
    fn fuzz_pedal_report_arbitrary_bytes(
        data in proptest::collection::vec(any::<u8>(), 0..=128),
    ) {
        let _ = tm::input::parse_pedal_report(&data);
    }

    /// Pedal report requires 3+ bytes. Lengths 0–2 must return None.
    #[test]
    fn fuzz_pedal_report_short_lengths(len in 0usize..=2) {
        let data = vec![0xFFu8; len];
        prop_assert!(tm::input::parse_pedal_report(&data).is_none(),
            "pedal report with len={len} must return None");
    }

    /// Exactly 3 bytes must succeed (minimum valid pedal report).
    #[test]
    fn fuzz_pedal_report_exact_minimum(a: u8, b: u8, c: u8) {
        let data = [a, b, c];
        prop_assert!(tm::input::parse_pedal_report(&data).is_some(),
            "3-byte pedal report must always parse");
    }

    // ── Pedal report output ranges ───────────────────────────────────────

    /// When pedal parse succeeds, axes must be in valid u8 range and the
    /// normalised result must be in [0.0, 1.0].
    #[test]
    fn fuzz_pedal_report_output_ranges(
        data in proptest::collection::vec(any::<u8>(), 3..=64),
    ) {
        if let Some(raw) = tm::input::parse_pedal_report(&data) {
            let norm = raw.normalize();
            prop_assert!(norm.throttle.is_finite() && (0.0..=1.0).contains(&norm.throttle),
                "throttle {} out of range", norm.throttle);
            prop_assert!(norm.brake.is_finite() && (0.0..=1.0).contains(&norm.brake),
                "brake {} out of range", norm.brake);
            if let Some(c) = norm.clutch {
                prop_assert!(c.is_finite() && (0.0..=1.0).contains(&c),
                    "clutch {} out of range", c);
            }
        }
    }

    // ── Input report: ID 0x00 edge case ──────────────────────────────────

    /// Report ID 0x00 with valid length must return None.
    #[test]
    fn fuzz_input_report_id_zero(
        tail in proptest::collection::vec(any::<u8>(), 15usize),
    ) {
        let mut data = vec![0x00u8];
        data.extend_from_slice(&tail);
        prop_assert!(tm::parse_input_report(&data).is_none(),
            "report with ID 0x00 must return None");
    }

    // ── T150EffectType::from_u16 ─────────────────────────────────────────

    /// from_u16 must never panic for any u16 value. Unknown values return None.
    #[test]
    fn fuzz_t150_effect_type_from_u16(val: u16) {
        let a = tm::T150EffectType::from_u16(val);
        let b = tm::T150EffectType::from_u16(val);
        prop_assert_eq!(a, b, "from_u16 must be deterministic for 0x{:04X}", val);
    }

    // ── Model::from_product_id ───────────────────────────────────────────

    /// from_product_id must never panic for any u16 and must be deterministic.
    #[test]
    fn fuzz_model_from_product_id(pid: u16) {
        let a = tm::Model::from_product_id(pid);
        let b = tm::Model::from_product_id(pid);
        prop_assert_eq!(a, b, "from_product_id must be deterministic for 0x{:04X}", pid);
    }

    // ── Encoder: NaN / Inf / extreme values ──────────────────────────────

    /// Encoding NaN must not panic and must produce a valid-length report.
    #[test]
    fn fuzz_encoder_nan(_dummy in 0u8..=1) {
        let encoder = tm::ThrustmasterConstantForceEncoder::new(10.0);
        let mut out = [0u8; tm::EFFECT_REPORT_LEN];
        encoder.encode(f32::NAN, &mut out);
    }

    /// Encoding ±Inf must not panic.
    #[test]
    fn fuzz_encoder_inf(positive: bool) {
        let val = if positive { f32::INFINITY } else { f32::NEG_INFINITY };
        let encoder = tm::ThrustmasterConstantForceEncoder::new(10.0);
        let mut out = [0u8; tm::EFFECT_REPORT_LEN];
        encoder.encode(val, &mut out);
    }

    /// Encoding with max_torque=0.0 must not divide by zero or panic.
    #[test]
    fn fuzz_encoder_zero_max_torque(torque in -100.0f32..=100.0) {
        let encoder = tm::ThrustmasterConstantForceEncoder::new(0.0);
        let mut out = [0u8; tm::EFFECT_REPORT_LEN];
        encoder.encode(torque, &mut out);
    }

    // ── identify_device: full u16 range ──────────────────────────────────

    /// identify_device and classification must be consistent for any PID.
    #[test]
    fn fuzz_device_classification_consistent(pid: u16) {
        let ident = tm::identify_device(pid);
        let is_wheel = tm::is_wheel_product(pid);
        let is_pedal = tm::is_pedal_product(pid);
        // A device identified as a wheel must have is_wheel_product return true
        if ident.category == tm::ThrustmasterDeviceCategory::Wheelbase {
            prop_assert!(is_wheel,
                "PID 0x{:04X} categorised as Wheelbase but is_wheel_product is false", pid);
        }
        if ident.category == tm::ThrustmasterDeviceCategory::Pedals {
            prop_assert!(is_pedal,
                "PID 0x{:04X} categorised as Pedals but is_pedal_product is false", pid);
        }
        // Never both wheel and pedal
        prop_assert!(!(is_wheel && is_pedal),
            "PID 0x{:04X} must not be both wheel and pedal", pid);
    }
}

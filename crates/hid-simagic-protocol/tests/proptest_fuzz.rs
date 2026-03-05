//! Fuzz-style property tests for Simagic protocol edge cases.
//!
//! Feeds arbitrary byte sequences and extreme values into all parse and encode
//! entry points to ensure no panics occur and all outputs are well-formed.

use proptest::prelude::*;
use racing_wheel_hid_simagic_protocol as simagic;
use racing_wheel_hid_simagic_protocol::SimagicConstantForceEncoder;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    // ── Arbitrary-bytes fuzz: parse_input_report ─────────────────────────

    /// Feeding any byte sequence of any length to parse_input_report must
    /// never panic.
    #[test]
    fn fuzz_input_report_arbitrary_bytes(
        data in proptest::collection::vec(any::<u8>(), 0..=128),
    ) {
        let _ = simagic::parse_input_report(&data);
    }

    /// When parse_input_report succeeds, all normalised axes must be finite
    /// and in [0.0, 1.0], and steering in [-1.0, 1.0].
    #[test]
    fn fuzz_input_report_output_ranges(
        data in proptest::collection::vec(any::<u8>(), 17..=64),
    ) {
        if let Some(s) = simagic::parse_input_report(&data) {
            prop_assert!(s.steering.is_finite() && s.steering >= -1.0 && s.steering <= 1.0,
                "steering {} out of range", s.steering);
            prop_assert!(s.throttle.is_finite() && s.throttle >= 0.0 && s.throttle <= 1.0,
                "throttle {} out of range", s.throttle);
            prop_assert!(s.brake.is_finite() && s.brake >= 0.0 && s.brake <= 1.0,
                "brake {} out of range", s.brake);
            prop_assert!(s.clutch.is_finite() && s.clutch >= 0.0 && s.clutch <= 1.0,
                "clutch {} out of range", s.clutch);
            prop_assert!(s.handbrake.is_finite() && s.handbrake >= 0.0 && s.handbrake <= 1.0,
                "handbrake {} out of range", s.handbrake);
        }
    }

    // ── settings::parse_status1 fuzz ─────────────────────────────────────

    /// Feeding any byte sequence to parse_status1 must never panic.
    #[test]
    fn fuzz_parse_status1_arbitrary_bytes(
        data in proptest::collection::vec(any::<u8>(), 0..=128),
    ) {
        let _ = simagic::settings::parse_status1(&data);
    }

    /// parse_status1 requires 53+ bytes with report ID 0x81. Short buffers
    /// must return None.
    #[test]
    fn fuzz_parse_status1_short_lengths(len in 0usize..=52) {
        let mut data = vec![0x81u8; len.max(1)];
        data.resize(len, 0xFF);
        if !data.is_empty() {
            data[0] = 0x81;
        }
        prop_assert!(simagic::settings::parse_status1(&data).is_none(),
            "parse_status1 with len={len} must return None");
    }

    /// parse_status1 with wrong report ID must return None.
    #[test]
    fn fuzz_parse_status1_wrong_id(
        id in (0u8..=0xFF).prop_filter("not 0x81", |&id| id != 0x81),
    ) {
        let mut data = vec![0u8; 64];
        data[0] = id;
        prop_assert!(simagic::settings::parse_status1(&data).is_none(),
            "parse_status1 with ID 0x{:02X} must return None", id);
    }

    // ── settings::AngleLockStrength::from_byte ───────────────────────────

    /// from_byte must never panic for any u8 and must return None for values
    /// > 2.
    #[test]
    fn fuzz_angle_lock_strength_from_byte(byte: u8) {
        let result = simagic::settings::AngleLockStrength::from_byte(byte);
        if byte > 2 {
            prop_assert!(result.is_none(),
                "AngleLockStrength::from_byte({byte}) should be None");
        } else {
            prop_assert!(result.is_some(),
                "AngleLockStrength::from_byte({byte}) should be Some");
        }
    }

    // ── settings::encode_ring_light / decode_ring_light roundtrip ────────

    /// encode → decode must round-trip for any (enabled, brightness) pair.
    #[test]
    fn fuzz_ring_light_roundtrip(enabled: bool, brightness in 0u8..=100) {
        let encoded = simagic::settings::encode_ring_light(enabled, brightness);
        let (dec_enabled, dec_brightness) = simagic::settings::decode_ring_light(encoded);
        prop_assert_eq!(dec_enabled, enabled,
            "enabled must round-trip: encoded=0x{:02X}", encoded);
        prop_assert_eq!(dec_brightness, brightness,
            "brightness must round-trip: encoded=0x{:02X}", encoded);
    }

    /// decode_ring_light must never panic for any u8 and brightness must be
    /// ≤ 127 (7-bit field).
    #[test]
    fn fuzz_decode_ring_light_any_byte(raw: u8) {
        let (enabled, brightness) = simagic::settings::decode_ring_light(raw);
        prop_assert!(brightness <= 127,
            "brightness {} > 127 for raw 0x{:02X}", brightness, raw);
        // Enabled flag must match bit 7
        prop_assert_eq!(enabled, raw & 0x80 != 0,
            "enabled flag mismatch for raw 0x{:02X}", raw);
    }

    // ── Encoder: NaN / Inf / extreme values ──────────────────────────────

    /// Encoding NaN must not panic.
    #[test]
    fn fuzz_encoder_nan(_dummy in 0u8..=1) {
        let encoder = SimagicConstantForceEncoder::new(10.0);
        let mut out = [0u8; simagic::CONSTANT_FORCE_REPORT_LEN];
        let _len = encoder.encode(f32::NAN, &mut out);
    }

    /// Encoding ±Inf must not panic.
    #[test]
    fn fuzz_encoder_inf(positive: bool) {
        let val = if positive { f32::INFINITY } else { f32::NEG_INFINITY };
        let encoder = SimagicConstantForceEncoder::new(10.0);
        let mut out = [0u8; simagic::CONSTANT_FORCE_REPORT_LEN];
        let _len = encoder.encode(val, &mut out);
    }

    /// Encoding with max_torque=0.0 must not divide by zero or panic.
    #[test]
    fn fuzz_encoder_zero_max_torque(torque in -100.0f32..=100.0) {
        let encoder = SimagicConstantForceEncoder::new(0.0);
        let mut out = [0u8; simagic::CONSTANT_FORCE_REPORT_LEN];
        let _len = encoder.encode(torque, &mut out);
    }

    // ── SimagicModel: from_pid full range ────────────────────────────────

    /// from_pid must never panic for any u16 and must be deterministic.
    #[test]
    fn fuzz_model_from_pid(pid: u16) {
        let a = simagic::SimagicModel::from_pid(pid);
        let b = simagic::SimagicModel::from_pid(pid);
        prop_assert_eq!(a, b, "from_pid must be deterministic for 0x{:04X}", pid);
    }

    /// identify_device must echo back the input PID.
    #[test]
    fn fuzz_identify_device_echo_pid(pid: u16) {
        let ident = simagic::identify_device(pid);
        prop_assert_eq!(ident.product_id, pid,
            "identify_device(0x{:04X}).product_id must equal input", pid);
    }
}

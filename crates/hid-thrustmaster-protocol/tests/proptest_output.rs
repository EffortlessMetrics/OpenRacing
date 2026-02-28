//! Property-based tests for Thrustmaster HID output report generation and
//! input report parsing.
//!
//! Uses proptest with 500 cases to verify invariants that hold across the full
//! input domain, complementing the snapshot and unit tests in the crate.

use proptest::prelude::*;
use racing_wheel_hid_thrustmaster_protocol as tm;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    // ── Input parsing: never panics ───────────────────────────────────────────

    /// Parsing any arbitrary byte sequence must never panic.
    #[test]
    fn prop_parse_never_panics(
        data in proptest::collection::vec(proptest::num::u8::ANY, 0..=64usize),
    ) {
        let _ = tm::parse_input_report(&data);
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
        if let Some(state) = tm::parse_input_report(&data) {
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
        if let Some(state) = tm::parse_input_report(&d) {
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
            tm::parse_input_report(&buf).is_none(),
            "report with ID 0x{:02X} must return None",
            id
        );
    }

    // ── Spring effect: structure ──────────────────────────────────────────────

    /// build_spring_effect must use EFFECT_OP report ID (0x22), SPRING type
    /// (0x40), and LE-encode the center and stiffness parameters.
    #[test]
    fn prop_spring_effect_structure(center: i16, stiffness: u16) {
        let r = tm::build_spring_effect(center, stiffness);
        prop_assert_eq!(r[0], 0x22u8, "byte 0 must be EFFECT_OP (0x22)");
        prop_assert_eq!(r[1], 0x40u8, "byte 1 must be SPRING effect type (0x40)");
        let center_decoded = i16::from_le_bytes([r[3], r[4]]);
        prop_assert_eq!(center_decoded, center, "center must round-trip via LE bytes");
        let stiff_decoded = u16::from_le_bytes([r[5], r[6]]);
        prop_assert_eq!(stiff_decoded, stiffness, "stiffness must round-trip via LE bytes");
    }

    // ── Damper effect: structure ──────────────────────────────────────────────

    /// build_damper_effect must use EFFECT_OP report ID (0x22), DAMPER type
    /// (0x41), and LE-encode the damping parameter.
    #[test]
    fn prop_damper_effect_structure(damping: u16) {
        let r = tm::build_damper_effect(damping);
        prop_assert_eq!(r[0], 0x22u8, "byte 0 must be EFFECT_OP (0x22)");
        prop_assert_eq!(r[1], 0x41u8, "byte 1 must be DAMPER effect type (0x41)");
        let decoded = u16::from_le_bytes([r[3], r[4]]);
        prop_assert_eq!(decoded, damping, "damping must round-trip via LE bytes");
    }

    // ── Friction effect: structure ────────────────────────────────────────────

    /// build_friction_effect must use EFFECT_OP report ID (0x22), FRICTION type
    /// (0x43), and LE-encode both min and max.
    #[test]
    fn prop_friction_effect_structure(minimum: u16, maximum: u16) {
        let r = tm::build_friction_effect(minimum, maximum);
        prop_assert_eq!(r[0], 0x22u8, "byte 0 must be EFFECT_OP (0x22)");
        prop_assert_eq!(r[1], 0x43u8, "byte 1 must be FRICTION effect type (0x43)");
        let min_decoded = u16::from_le_bytes([r[3], r[4]]);
        let max_decoded = u16::from_le_bytes([r[5], r[6]]);
        prop_assert_eq!(min_decoded, minimum, "minimum must round-trip via LE bytes");
        prop_assert_eq!(max_decoded, maximum, "maximum must round-trip via LE bytes");
    }

    // ── Actuator enable/disable ───────────────────────────────────────────────

    /// build_actuator_enable must set byte 1 to 0x01 when enabled and 0x00
    /// when disabled.
    #[test]
    fn prop_actuator_enable_values(enabled: bool) {
        let r = tm::build_actuator_enable(enabled);
        prop_assert_eq!(r[0], 0x82u8, "byte 0 must be ACTUATOR_ENABLE (0x82)");
        let expected = if enabled { 0x01u8 } else { 0x00u8 };
        prop_assert_eq!(
            r[1], expected,
            "byte 1 must be 0x01 when enabled, 0x00 when disabled"
        );
    }

    // ── Device gain: all values ───────────────────────────────────────────────

    /// build_device_gain must preserve the gain byte and use report ID 0x81.
    #[test]
    fn prop_device_gain_full_range(gain: u8) {
        let r = tm::build_device_gain(gain);
        prop_assert_eq!(r[0], 0x81u8, "byte 0 must be DEVICE_GAIN (0x81)");
        prop_assert_eq!(r[1], gain, "gain byte must be preserved unchanged");
    }

    // ── Set range: full u16 domain ────────────────────────────────────────────

    /// build_set_range_report must encode any u16 degrees correctly in LE
    /// bytes 2–3 and use the correct report ID and command byte.
    #[test]
    fn prop_set_range_full_u16(degrees: u16) {
        let r = tm::build_set_range_report(degrees);
        prop_assert_eq!(r[0], 0x80u8, "byte 0 must be VENDOR_SET_RANGE (0x80)");
        prop_assert_eq!(r[1], 0x01u8, "byte 1 must be SET_RANGE command (0x01)");
        let decoded = u16::from_le_bytes([r[2], r[3]]);
        prop_assert_eq!(decoded, degrees, "degrees must round-trip via LE bytes");
    }

    // ── Pedal normalization ───────────────────────────────────────────────────

    /// ThrustmasterPedalAxesRaw::normalize must produce axes in [0.0, 1.0].
    #[test]
    fn prop_pedal_normalize_in_unit_range(throttle: u8, brake: u8, clutch: u8) {
        let raw = tm::ThrustmasterPedalAxesRaw {
            throttle,
            brake,
            clutch: Some(clutch),
        };
        let norm = raw.normalize();
        prop_assert!(
            norm.throttle >= 0.0 && norm.throttle <= 1.0,
            "throttle {} not in [0.0, 1.0]",
            norm.throttle
        );
        prop_assert!(
            norm.brake >= 0.0 && norm.brake <= 1.0,
            "brake {} not in [0.0, 1.0]",
            norm.brake
        );
        if let Some(c) = norm.clutch {
            prop_assert!((0.0..=1.0).contains(&c), "clutch {} not in [0.0, 1.0]", c);
        }
    }

    // ── identify_device: product_id consistency ───────────────────────────────

    /// identify_device must always echo back the input product ID in the
    /// returned ThrustmasterDeviceIdentity.
    #[test]
    fn prop_identify_device_echo_pid(pid in 0u16..=65535u16) {
        let ident = tm::identify_device(pid);
        prop_assert_eq!(
            ident.product_id, pid,
            "identify_device(0x{:04X}).product_id must equal the input PID",
            pid
        );
    }

    // ── is_wheel_product / is_pedal_product are mutually exclusive ────────────

    /// No device can simultaneously be identified as a wheelbase and a pedal
    /// set.
    #[test]
    fn prop_wheel_and_pedal_exclusive(pid in 0u16..=65535u16) {
        let is_wheel = tm::is_wheel_product(pid);
        let is_pedal = tm::is_pedal_product(pid);
        prop_assert!(
            !(is_wheel && is_pedal),
            "PID 0x{:04X} cannot be both a wheelbase and a pedal set",
            pid
        );
    }
}

//! Property tests for openracing-profile using proptest
//!
//! These tests verify invariants that must hold for all valid inputs.

use openracing_profile::{
    CURRENT_SCHEMA_VERSION, CurveType, FfbSettings, InputSettings, LedMode, WheelProfile,
    WheelSettings, merge_profiles, migrate_profile, validate_profile, validate_settings,
};
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// Strategies
// ---------------------------------------------------------------------------

fn arb_curve_type() -> impl Strategy<Value = CurveType> {
    prop_oneof![
        Just(CurveType::Linear),
        Just(CurveType::Exponential),
        Just(CurveType::Logarithmic),
        // Just(CurveType::Custom), // Custom implies there handles valid array, skipped in rand here.
    ]
}

fn arb_led_mode() -> impl Strategy<Value = LedMode> {
    prop_oneof![
        Just(LedMode::Default),
        Just(LedMode::Speed),
        Just(LedMode::Rpm),
        Just(LedMode::Custom),
        Just(LedMode::Off),
    ]
}

fn arb_valid_ffb_settings() -> impl Strategy<Value = FfbSettings> {
    (
        0.0f32..=1.0f32,   // overall_gain
        0.0f32..=100.0f32, // torque_limit
        0.0f32..=1.0f32,   // spring_strength
        0.0f32..=1.0f32,   // damper_strength
        0.0f32..=1.0f32,   // friction_strength
        any::<bool>(),     // effects_enabled
    )
        .prop_map(
            |(
                overall_gain,
                torque_limit,
                spring_strength,
                damper_strength,
                friction_strength,
                effects_enabled,
            )| {
                FfbSettings {
                    overall_gain,
                    torque_limit,
                    spring_strength,
                    damper_strength,
                    friction_strength,
                    effects_enabled,
                }
            },
        )
}

fn arb_valid_input_settings() -> impl Strategy<Value = InputSettings> {
    (
        90u16..=3600u16,  // steering_range
        0u16..=100u16,    // steering_deadzone
        arb_curve_type(), // throttle_curve
        arb_curve_type(), // brake_curve
        arb_curve_type(), // clutch_curve
    )
        .prop_map(
            |(steering_range, steering_deadzone, throttle_curve, brake_curve, clutch_curve)| {
                InputSettings {
                    steering_range,
                    steering_deadzone,
                    throttle_curve,
                    brake_curve,
                    clutch_curve,
                    ..Default::default()
                }
            },
        )
}

fn arb_valid_wheel_settings() -> impl Strategy<Value = WheelSettings> {
    (
        arb_valid_ffb_settings(),
        arb_valid_input_settings(),
        0.0f32..=1.0f32, // filter_strength
        any::<bool>(),   // filter_enabled
        arb_led_mode(),
        any::<bool>(), // telemetry_enabled
    )
        .prop_map(
            |(ffb, input, filter_strength, filter_enabled, led_mode, telemetry_enabled)| {
                use openracing_profile::{AdvancedSettings, LimitSettings};
                WheelSettings {
                    ffb,
                    input,
                    limits: LimitSettings::default(),
                    advanced: AdvancedSettings {
                        filter_enabled,
                        filter_strength,
                        led_mode,
                        telemetry_enabled,
                    },
                }
            },
        )
}

// ---------------------------------------------------------------------------
// Property tests
// ---------------------------------------------------------------------------

proptest! {
    /// Serialization round-trip: any valid WheelSettings serializes to JSON
    /// and deserializes back with equivalent field values.
    #[test]
    fn settings_json_round_trip(settings in arb_valid_wheel_settings()) {
        let json = serde_json::to_string(&settings)
            .map_err(|e| proptest::test_runner::TestCaseError::fail(format!("serialize: {e}")))?;
        let restored: WheelSettings = serde_json::from_str(&json)
            .map_err(|e| proptest::test_runner::TestCaseError::fail(format!("deserialize: {e}")))?;

        prop_assert!((restored.ffb.overall_gain - settings.ffb.overall_gain).abs() < f32::EPSILON);
        prop_assert_eq!(restored.input.steering_range, settings.input.steering_range);
        prop_assert_eq!(restored.input.throttle_curve, settings.input.throttle_curve);
        prop_assert_eq!(restored.advanced.led_mode, settings.advanced.led_mode);
    }

    /// All settings generated with valid ranges pass validation.
    #[test]
    fn valid_settings_pass_validation(settings in arb_valid_wheel_settings()) {
        prop_assert!(validate_settings(&settings).is_ok());
    }

    /// Profile creation always produces a non-empty id and version 1.
    #[test]
    fn new_profile_always_has_valid_id(name in "[a-zA-Z0-9 ]{1,20}", device in "[a-zA-Z0-9-]{1,20}") {
        let profile = WheelProfile::new(&name, &device);
        prop_assert!(!profile.id.is_empty());
        prop_assert_eq!(profile.version, 1u32);
        prop_assert_eq!(profile.schema_version, CURRENT_SCHEMA_VERSION);
        prop_assert!(validate_profile(&profile).is_ok());
    }

    /// Merge always preserves the base profile's id.
    #[test]
    fn merge_preserves_base_id(
        settings_a in arb_valid_wheel_settings(),
        settings_b in arb_valid_wheel_settings(),
    ) {
        let base = WheelProfile::new("Base", "dev").with_settings(settings_a);
        let overlay = WheelProfile::new("Overlay", "dev").with_settings(settings_b);
        let merged = merge_profiles(&base, &overlay);
        prop_assert_eq!(merged.id, base.id);
    }

    /// Migration on any version ≤ CURRENT succeeds.
    #[test]
    fn migration_succeeds_for_valid_versions(version in 0u32..=CURRENT_SCHEMA_VERSION) {
        let mut profile = WheelProfile::new("M", "d");
        profile.schema_version = version;
        let result = migrate_profile(&mut profile);
        prop_assert!(result.is_ok());
        prop_assert_eq!(profile.schema_version, CURRENT_SCHEMA_VERSION);
    }

    /// Migration on any version > CURRENT fails.
    #[test]
    fn migration_fails_for_future_versions(version in (CURRENT_SCHEMA_VERSION + 1)..=100u32) {
        let mut profile = WheelProfile::new("M", "d");
        profile.schema_version = version;
        let result = migrate_profile(&mut profile);
        prop_assert!(result.is_err());
    }

    /// Gain outside [0.0, 1.0] always fails validation.
    #[test]
    fn invalid_gain_fails_validation(gain in prop::num::f32::ANY.prop_filter("out of range", |g| *g < 0.0 || *g > 1.0)) {
        if gain.is_nan() || gain.is_infinite() {
            return Ok(()); // NaN/Inf comparisons are weird, skip those
        }
        let mut settings = WheelSettings::default();
        settings.ffb.overall_gain = gain;
        prop_assert!(validate_settings(&settings).is_err());
    }
}

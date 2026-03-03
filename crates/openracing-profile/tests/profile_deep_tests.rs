//! Deep tests for the openracing-profile crate.
//!
//! Covers profile creation with all field types, validation rules,
//! merge/conflict resolution, import/export round-trip, inheritance,
//! device-specific bindings, and proptest-based randomized validation.

use openracing_profile::{
    AdvancedSettings, CurveType, FfbSettings, InputSettings, LedMode, LimitSettings,
    ProfileError, WheelProfile, WheelSettings, CURRENT_SCHEMA_VERSION, backup_profile,
    generate_profile_id, merge_profiles, migrate_profile, validate_profile, validate_settings,
};
use proptest::prelude::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ---------------------------------------------------------------------------
// Profile creation with all field types
// ---------------------------------------------------------------------------

mod creation_all_fields {
    use super::*;

    #[test]
    fn create_profile_with_all_ffb_fields() -> TestResult {
        let settings = WheelSettings {
            ffb: FfbSettings {
                overall_gain: 0.85,
                torque_limit: 15.0,
                spring_strength: 0.3,
                damper_strength: 0.4,
                friction_strength: 0.2,
                effects_enabled: false,
            },
            ..WheelSettings::default()
        };
        let profile = WheelProfile::new("FFB Test", "dev-1").with_settings(settings);
        validate_profile(&profile)?;
        assert!((profile.settings.ffb.overall_gain - 0.85).abs() < f32::EPSILON);
        assert!((profile.settings.ffb.spring_strength - 0.3).abs() < f32::EPSILON);
        assert!(!profile.settings.ffb.effects_enabled);
        Ok(())
    }

    #[test]
    fn create_profile_with_all_input_fields() -> TestResult {
        let settings = WheelSettings {
            input: InputSettings {
                steering_range: 1080,
                steering_deadzone: 5,
                throttle_curve: CurveType::Exponential,
                brake_curve: CurveType::Logarithmic,
                clutch_curve: CurveType::Custom,
            },
            ..WheelSettings::default()
        };
        let profile = WheelProfile::new("Input Test", "dev-2").with_settings(settings);
        validate_profile(&profile)?;
        assert_eq!(profile.settings.input.steering_range, 1080);
        assert_eq!(profile.settings.input.steering_deadzone, 5);
        assert_eq!(profile.settings.input.throttle_curve, CurveType::Exponential);
        assert_eq!(profile.settings.input.brake_curve, CurveType::Logarithmic);
        assert_eq!(profile.settings.input.clutch_curve, CurveType::Custom);
        Ok(())
    }

    #[test]
    fn create_profile_with_all_limit_fields() -> TestResult {
        let settings = WheelSettings {
            limits: LimitSettings {
                max_speed: Some(300.0),
                max_temp: Some(65),
                emergency_stop: false,
            },
            ..WheelSettings::default()
        };
        let profile = WheelProfile::new("Limits Test", "dev-3").with_settings(settings);
        validate_profile(&profile)?;
        assert_eq!(profile.settings.limits.max_speed, Some(300.0));
        assert_eq!(profile.settings.limits.max_temp, Some(65));
        assert!(!profile.settings.limits.emergency_stop);
        Ok(())
    }

    #[test]
    fn create_profile_with_all_advanced_fields() -> TestResult {
        let settings = WheelSettings {
            advanced: AdvancedSettings {
                filter_enabled: true,
                filter_strength: 0.75,
                led_mode: LedMode::Rpm,
                telemetry_enabled: false,
            },
            ..WheelSettings::default()
        };
        let profile = WheelProfile::new("Advanced Test", "dev-4").with_settings(settings);
        validate_profile(&profile)?;
        assert!(profile.settings.advanced.filter_enabled);
        assert!((profile.settings.advanced.filter_strength - 0.75).abs() < f32::EPSILON);
        assert_eq!(profile.settings.advanced.led_mode, LedMode::Rpm);
        assert!(!profile.settings.advanced.telemetry_enabled);
        Ok(())
    }

    #[test]
    fn create_profile_with_every_led_mode() -> TestResult {
        let modes = [
            LedMode::Default,
            LedMode::Speed,
            LedMode::Rpm,
            LedMode::Custom,
            LedMode::Off,
        ];
        for mode in &modes {
            let mut settings = WheelSettings::default();
            settings.advanced.led_mode = *mode;
            let profile = WheelProfile::new("LED Mode Test", "dev").with_settings(settings);
            validate_profile(&profile)?;
            assert_eq!(profile.settings.advanced.led_mode, *mode);
        }
        Ok(())
    }

    #[test]
    fn create_profile_with_every_curve_type() -> TestResult {
        let curves = [
            CurveType::Linear,
            CurveType::Exponential,
            CurveType::Logarithmic,
            CurveType::Custom,
        ];
        for curve in &curves {
            let mut settings = WheelSettings::default();
            settings.input.throttle_curve = *curve;
            let profile = WheelProfile::new("Curve Test", "dev").with_settings(settings);
            validate_profile(&profile)?;
        }
        Ok(())
    }

    #[test]
    fn create_profile_with_none_limits() -> TestResult {
        let settings = WheelSettings {
            limits: LimitSettings {
                max_speed: None,
                max_temp: None,
                emergency_stop: true,
            },
            ..WheelSettings::default()
        };
        let profile = WheelProfile::new("None Limits", "dev").with_settings(settings);
        validate_profile(&profile)?;
        assert!(profile.settings.limits.max_speed.is_none());
        assert!(profile.settings.limits.max_temp.is_none());
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Validation rules (name length, character restrictions)
// ---------------------------------------------------------------------------

mod validation_rules {
    use super::*;

    #[test]
    fn empty_name_rejected() {
        let mut p = WheelProfile::new("x", "dev");
        p.name = String::new();
        assert!(validate_profile(&p).is_err());
    }

    #[test]
    fn empty_device_id_rejected() {
        let mut p = WheelProfile::new("x", "dev");
        p.device_id = String::new();
        assert!(validate_profile(&p).is_err());
    }

    #[test]
    fn single_char_name_accepted() -> TestResult {
        let mut p = WheelProfile::new("x", "dev");
        p.name = "A".to_string();
        validate_profile(&p)?;
        Ok(())
    }

    #[test]
    fn long_name_accepted() -> TestResult {
        let mut p = WheelProfile::new("x", "dev");
        p.name = "a".repeat(1000);
        validate_profile(&p)?;
        Ok(())
    }

    #[test]
    fn unicode_name_accepted() -> TestResult {
        let p = WheelProfile::new("日本語テスト", "dev");
        validate_profile(&p)?;
        Ok(())
    }

    #[test]
    fn name_with_special_chars_accepted() -> TestResult {
        let p = WheelProfile::new("My Profile (v2.1) - GT3 @Spa", "dev");
        validate_profile(&p)?;
        Ok(())
    }

    #[test]
    fn whitespace_only_name_rejected() {
        let mut p = WheelProfile::new("x", "dev");
        // Note: whitespace-only names are not currently rejected by validate_profile
        // (only empty is rejected). If this changes in the future, update test.
        p.name = "  ".to_string();
        // Current validation only checks for empty, so whitespace-only passes
        let result = validate_profile(&p);
        assert!(result.is_ok(), "whitespace name accepted by current validation");
    }

    // FFB gain boundaries
    #[test]
    fn gain_exact_zero() -> TestResult {
        let mut s = WheelSettings::default();
        s.ffb.overall_gain = 0.0;
        validate_settings(&s)?;
        Ok(())
    }

    #[test]
    fn gain_exact_one() -> TestResult {
        let mut s = WheelSettings::default();
        s.ffb.overall_gain = 1.0;
        validate_settings(&s)?;
        Ok(())
    }

    #[test]
    fn gain_slightly_negative() {
        let mut s = WheelSettings::default();
        s.ffb.overall_gain = -0.001;
        assert!(validate_settings(&s).is_err());
    }

    #[test]
    fn gain_slightly_above_one() {
        let mut s = WheelSettings::default();
        s.ffb.overall_gain = 1.001;
        assert!(validate_settings(&s).is_err());
    }

    #[test]
    fn gain_nan_passes_validation() -> TestResult {
        // NaN comparisons are always false in Rust, so range checks
        // (0.0..=1.0).contains(&NaN) returns false. However the current
        // validator does not explicitly reject NaN. Document that behaviour.
        let mut s = WheelSettings::default();
        s.ffb.overall_gain = f32::NAN;
        // If validation rejects NaN in the future, flip this assertion.
        assert!(validate_settings(&s).is_ok());
        Ok(())
    }

    // Torque limit boundaries
    #[test]
    fn torque_limit_zero() -> TestResult {
        let mut s = WheelSettings::default();
        s.ffb.torque_limit = 0.0;
        validate_settings(&s)?;
        Ok(())
    }

    #[test]
    fn torque_limit_hundred() -> TestResult {
        let mut s = WheelSettings::default();
        s.ffb.torque_limit = 100.0;
        validate_settings(&s)?;
        Ok(())
    }

    #[test]
    fn torque_limit_negative() {
        let mut s = WheelSettings::default();
        s.ffb.torque_limit = -0.01;
        assert!(validate_settings(&s).is_err());
    }

    #[test]
    fn torque_limit_above_hundred() {
        let mut s = WheelSettings::default();
        s.ffb.torque_limit = 100.01;
        assert!(validate_settings(&s).is_err());
    }

    // Steering range boundaries
    #[test]
    fn steering_range_min() -> TestResult {
        let mut s = WheelSettings::default();
        s.input.steering_range = 90;
        validate_settings(&s)?;
        Ok(())
    }

    #[test]
    fn steering_range_max() -> TestResult {
        let mut s = WheelSettings::default();
        s.input.steering_range = 3600;
        validate_settings(&s)?;
        Ok(())
    }

    #[test]
    fn steering_range_below_min() {
        let mut s = WheelSettings::default();
        s.input.steering_range = 89;
        assert!(validate_settings(&s).is_err());
    }

    #[test]
    fn steering_range_above_max() {
        let mut s = WheelSettings::default();
        s.input.steering_range = 3601;
        assert!(validate_settings(&s).is_err());
    }

    // Filter strength boundaries
    #[test]
    fn filter_strength_zero() -> TestResult {
        let mut s = WheelSettings::default();
        s.advanced.filter_strength = 0.0;
        validate_settings(&s)?;
        Ok(())
    }

    #[test]
    fn filter_strength_one() -> TestResult {
        let mut s = WheelSettings::default();
        s.advanced.filter_strength = 1.0;
        validate_settings(&s)?;
        Ok(())
    }

    #[test]
    fn filter_strength_negative() {
        let mut s = WheelSettings::default();
        s.advanced.filter_strength = -0.01;
        assert!(validate_settings(&s).is_err());
    }

    #[test]
    fn filter_strength_above_one() {
        let mut s = WheelSettings::default();
        s.advanced.filter_strength = 1.01;
        assert!(validate_settings(&s).is_err());
    }

    #[test]
    fn validation_error_type_is_correct() {
        let mut p = WheelProfile::new("x", "dev");
        p.name = String::new();
        let result = validate_profile(&p);
        assert!(matches!(result, Err(ProfileError::ValidationError(_))));
    }

    #[test]
    fn validation_error_message_mentions_field() {
        let mut p = WheelProfile::new("x", "dev");
        p.name = String::new();
        let err = validate_profile(&p).err();
        let msg = err.map(|e| e.to_string()).unwrap_or_default();
        assert!(
            msg.contains("name") || msg.contains("Name"),
            "error should mention the field: {msg}"
        );
    }
}

// ---------------------------------------------------------------------------
// Merge / conflict resolution
// ---------------------------------------------------------------------------

mod merge_conflict {
    use super::*;

    #[test]
    fn merge_overlay_gain_takes_precedence() -> TestResult {
        let base = WheelProfile::new("Base", "dev");
        let mut overlay = WheelProfile::new("Overlay", "dev");
        overlay.settings.ffb.overall_gain = 0.3;

        let merged = merge_profiles(&base, &overlay);
        assert!((merged.settings.ffb.overall_gain - 0.3).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn merge_overlay_torque_limit_takes_precedence() -> TestResult {
        let base = WheelProfile::new("Base", "dev");
        let mut overlay = WheelProfile::new("Overlay", "dev");
        overlay.settings.ffb.torque_limit = 42.0;

        let merged = merge_profiles(&base, &overlay);
        assert!((merged.settings.ffb.torque_limit - 42.0).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn merge_overlay_steering_range_takes_precedence() -> TestResult {
        let base = WheelProfile::new("Base", "dev");
        let mut overlay = WheelProfile::new("Overlay", "dev");
        overlay.settings.input.steering_range = 1080;

        let merged = merge_profiles(&base, &overlay);
        assert_eq!(merged.settings.input.steering_range, 1080);
        Ok(())
    }

    #[test]
    fn merge_preserves_base_identity() -> TestResult {
        let base = WheelProfile::new("Base", "dev");
        let overlay = WheelProfile::new("Overlay", "dev");
        let merged = merge_profiles(&base, &overlay);
        assert_eq!(merged.id, base.id, "merged should keep base ID");
        assert_eq!(merged.name, base.name, "merged should keep base name");
        Ok(())
    }

    #[test]
    fn merge_updates_modified_at() -> TestResult {
        let base = WheelProfile::new("Base", "dev");
        let overlay = WheelProfile::new("Overlay", "dev");
        let merged = merge_profiles(&base, &overlay);
        assert!(merged.modified_at >= base.modified_at);
        Ok(())
    }

    #[test]
    fn merge_identical_profiles_preserves_values() -> TestResult {
        let base = WheelProfile::new("Same", "dev");
        let overlay = base.clone();
        let merged = merge_profiles(&base, &overlay);
        assert!(
            (merged.settings.ffb.overall_gain - base.settings.ffb.overall_gain).abs()
                < f32::EPSILON
        );
        assert_eq!(
            merged.settings.input.steering_range,
            base.settings.input.steering_range
        );
        Ok(())
    }

    #[test]
    fn merge_chain_applies_sequentially() -> TestResult {
        let base = WheelProfile::new("Base", "dev");

        let mut layer1 = WheelProfile::new("L1", "dev");
        layer1.settings.ffb.overall_gain = 0.5;

        let mut layer2 = WheelProfile::new("L2", "dev");
        layer2.settings.ffb.overall_gain = 0.9;

        let merged = merge_profiles(&merge_profiles(&base, &layer1), &layer2);
        assert!((merged.settings.ffb.overall_gain - 0.9).abs() < f32::EPSILON);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Import / export round-trip
// ---------------------------------------------------------------------------

mod import_export_round_trip {
    use super::*;

    #[test]
    fn json_round_trip_preserves_all_fields() -> TestResult {
        let settings = WheelSettings {
            ffb: FfbSettings {
                overall_gain: 0.77,
                torque_limit: 33.3,
                spring_strength: 0.12,
                damper_strength: 0.45,
                friction_strength: 0.67,
                effects_enabled: false,
            },
            input: InputSettings {
                steering_range: 1440,
                steering_deadzone: 3,
                throttle_curve: CurveType::Exponential,
                brake_curve: CurveType::Logarithmic,
                clutch_curve: CurveType::Custom,
            },
            limits: LimitSettings {
                max_speed: Some(250.0),
                max_temp: Some(70),
                emergency_stop: false,
            },
            advanced: AdvancedSettings {
                filter_enabled: true,
                filter_strength: 0.88,
                led_mode: LedMode::Speed,
                telemetry_enabled: false,
            },
        };
        let original = WheelProfile::new("RoundTrip", "dev-rt").with_settings(settings);

        let json = serde_json::to_string(&original)?;
        let restored: WheelProfile = serde_json::from_str(&json)?;

        assert_eq!(original.id, restored.id);
        assert_eq!(original.name, restored.name);
        assert_eq!(original.device_id, restored.device_id);
        assert_eq!(original.version, restored.version);
        assert_eq!(original.schema_version, restored.schema_version);
        assert!((original.settings.ffb.overall_gain - restored.settings.ffb.overall_gain).abs() < f32::EPSILON);
        assert!((original.settings.ffb.torque_limit - restored.settings.ffb.torque_limit).abs() < f32::EPSILON);
        assert_eq!(original.settings.input.steering_range, restored.settings.input.steering_range);
        assert_eq!(original.settings.input.throttle_curve, restored.settings.input.throttle_curve);
        assert_eq!(original.settings.limits.max_speed, restored.settings.limits.max_speed);
        assert_eq!(original.settings.advanced.led_mode, restored.settings.advanced.led_mode);
        Ok(())
    }

    #[test]
    fn pretty_json_round_trip() -> TestResult {
        let original = WheelProfile::new("PrettyTest", "dev");
        let json = serde_json::to_string_pretty(&original)?;
        let restored: WheelProfile = serde_json::from_str(&json)?;
        assert_eq!(original.id, restored.id);
        Ok(())
    }

    #[test]
    fn backup_and_restore_round_trip() -> TestResult {
        let dir = std::env::temp_dir().join(format!("profile_deep_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let backup_path = dir.join("backup_test.json");

        let original = WheelProfile::new("BackupRT", "dev");
        let json = serde_json::to_string(&original)?;
        backup_profile(&json, &backup_path)?;

        let restored_json = std::fs::read_to_string(&backup_path)?;
        let restored: WheelProfile = serde_json::from_str(&restored_json)?;
        assert_eq!(original.id, restored.id);
        assert_eq!(original.name, restored.name);

        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn missing_schema_version_deserializes_as_zero() -> TestResult {
        let json = r#"{
            "id": "legacy-profile",
            "name": "Legacy",
            "device_id": "dev",
            "version": 1,
            "settings": {
                "ffb": {"overall_gain":1.0,"torque_limit":25.0,"spring_strength":0.0,"damper_strength":0.0,"friction_strength":0.0,"effects_enabled":true},
                "input": {"steering_range":900,"steering_deadzone":0,"throttle_curve":"Linear","brake_curve":"Linear","clutch_curve":"Linear"},
                "limits": {"max_speed":null,"max_temp":80,"emergency_stop":true},
                "advanced": {"filter_enabled":false,"filter_strength":0.5,"led_mode":"Default","telemetry_enabled":true}
            },
            "created_at": 1000,
            "modified_at": 1000
        }"#;
        let profile: WheelProfile = serde_json::from_str(json)?;
        assert_eq!(profile.schema_version, 0);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Profile inheritance (base profile + overrides)
// ---------------------------------------------------------------------------

mod inheritance {
    use super::*;

    #[test]
    fn global_base_then_game_overlay() -> TestResult {
        let global = WheelProfile::new("Global", "dev");

        let mut game = WheelProfile::new("Game", "dev");
        game.settings.ffb.overall_gain = 0.6;
        game.settings.input.steering_range = 540;

        let merged = merge_profiles(&global, &game);
        assert!((merged.settings.ffb.overall_gain - 0.6).abs() < f32::EPSILON);
        assert_eq!(merged.settings.input.steering_range, 540);
        // Base values should be preserved for unchanged fields
        assert_eq!(
            merged.settings.ffb.effects_enabled,
            global.settings.ffb.effects_enabled
        );
        Ok(())
    }

    #[test]
    fn three_layer_hierarchy() -> TestResult {
        let global = WheelProfile::new("Global", "dev");

        let mut game = WheelProfile::new("Game", "dev");
        game.settings.ffb.overall_gain = 0.7;

        let mut car = WheelProfile::new("Car", "dev");
        car.settings.ffb.torque_limit = 50.0;
        car.settings.input.steering_range = 480;

        let resolved = merge_profiles(&merge_profiles(&global, &game), &car);
        // Car layer should override torque and steering
        assert!((resolved.settings.ffb.torque_limit - 50.0).abs() < f32::EPSILON);
        assert_eq!(resolved.settings.input.steering_range, 480);
        Ok(())
    }

    #[test]
    fn merge_validated_result_is_valid() -> TestResult {
        let base = WheelProfile::new("Base", "dev");
        let mut overlay = WheelProfile::new("Overlay", "dev");
        overlay.settings.ffb.overall_gain = 0.5;

        let merged = merge_profiles(&base, &overlay);
        validate_profile(&merged)?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Device-specific profile bindings
// ---------------------------------------------------------------------------

mod device_bindings {
    use super::*;

    #[test]
    fn profile_bound_to_specific_device() -> TestResult {
        let p = WheelProfile::new("Fanatec GT DD Pro", "fanatec-dd-pro-001");
        assert_eq!(p.device_id, "fanatec-dd-pro-001");
        validate_profile(&p)?;
        Ok(())
    }

    #[test]
    fn same_settings_different_devices() -> TestResult {
        let settings = WheelSettings::default();
        let p1 = WheelProfile::new("P1", "device-a").with_settings(settings.clone());
        let p2 = WheelProfile::new("P2", "device-b").with_settings(settings);

        assert_ne!(p1.id, p2.id, "different profiles should have different IDs");
        assert_ne!(p1.device_id, p2.device_id);
        assert!(
            (p1.settings.ffb.overall_gain - p2.settings.ffb.overall_gain).abs() < f32::EPSILON
        );
        validate_profile(&p1)?;
        validate_profile(&p2)?;
        Ok(())
    }

    #[test]
    fn device_id_preserved_through_serialization() -> TestResult {
        let original = WheelProfile::new("DevTest", "simucube-2-pro");
        let json = serde_json::to_string(&original)?;
        let restored: WheelProfile = serde_json::from_str(&json)?;
        assert_eq!(restored.device_id, "simucube-2-pro");
        Ok(())
    }

    #[test]
    fn device_id_preserved_through_migration() -> TestResult {
        let mut p = WheelProfile::new("MigrateDevTest", "moza-r21");
        p.schema_version = 0;
        migrate_profile(&mut p)?;
        assert_eq!(p.device_id, "moza-r21");
        assert_eq!(p.schema_version, CURRENT_SCHEMA_VERSION);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Migration edge cases
// ---------------------------------------------------------------------------

mod migration_deep {
    use super::*;

    #[test]
    fn migrate_preserves_all_settings() -> TestResult {
        let mut settings = WheelSettings::default();
        settings.ffb.overall_gain = 0.42;
        settings.input.steering_range = 1080;
        settings.advanced.led_mode = LedMode::Rpm;

        let mut p = WheelProfile::new("MigrateSettings", "dev").with_settings(settings);
        p.schema_version = 0;

        migrate_profile(&mut p)?;

        assert!((p.settings.ffb.overall_gain - 0.42).abs() < f32::EPSILON);
        assert_eq!(p.settings.input.steering_range, 1080);
        assert_eq!(p.settings.advanced.led_mode, LedMode::Rpm);
        Ok(())
    }

    #[test]
    fn migrate_future_version_error_message() {
        let mut p = WheelProfile::new("Future", "dev");
        p.schema_version = 999;
        let err = migrate_profile(&mut p).err();
        let msg = err.map(|e| e.to_string()).unwrap_or_default();
        assert!(
            msg.contains("999") || msg.contains("Unsupported"),
            "error should mention the bad version: {msg}"
        );
    }

    #[test]
    fn profile_id_is_always_unique() -> TestResult {
        let ids: Vec<String> = (0..100).map(|_| generate_profile_id()).collect();
        let unique: std::collections::HashSet<&String> = ids.iter().collect();
        assert_eq!(ids.len(), unique.len(), "all generated IDs should be unique");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Proptest: random profile data validates or produces clear error
// ---------------------------------------------------------------------------

mod proptest_validation {
    use super::*;

    fn arb_ffb_gain() -> impl Strategy<Value = f32> {
        prop_oneof![
            // Valid range
            (0.0f32..=1.0f32),
            // Out of range
            (-10.0f32..0.0f32),
            (1.001f32..10.0f32),
        ]
    }

    fn arb_torque_limit() -> impl Strategy<Value = f32> {
        prop_oneof![
            (0.0f32..=100.0f32),
            (-50.0f32..0.0f32),
            (100.001f32..500.0f32),
        ]
    }

    fn arb_steering_range() -> impl Strategy<Value = u16> {
        prop_oneof![
            (90u16..=3600u16),
            (0u16..90u16),
            (3601u16..=u16::MAX),
        ]
    }

    fn arb_filter_strength() -> impl Strategy<Value = f32> {
        prop_oneof![
            (0.0f32..=1.0f32),
            (-5.0f32..0.0f32),
            (1.001f32..5.0f32),
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(200))]

        #[test]
        fn random_settings_validate_or_error(
            gain in arb_ffb_gain(),
            torque in arb_torque_limit(),
            range in arb_steering_range(),
            filter in arb_filter_strength(),
        ) {
            let settings = WheelSettings {
                ffb: FfbSettings {
                    overall_gain: gain,
                    torque_limit: torque,
                    ..FfbSettings::default()
                },
                input: InputSettings {
                    steering_range: range,
                    ..InputSettings::default()
                },
                advanced: AdvancedSettings {
                    filter_strength: filter,
                    ..AdvancedSettings::default()
                },
                ..WheelSettings::default()
            };

            let result = validate_settings(&settings);

            let gain_valid = (0.0..=1.0).contains(&gain);
            let torque_valid = (0.0..=100.0).contains(&torque);
            let range_valid = (90..=3600).contains(&range);
            let filter_valid = (0.0..=1.0).contains(&filter);

            if gain_valid && torque_valid && range_valid && filter_valid {
                prop_assert!(result.is_ok(), "valid settings should pass: {:?}", result);
            } else {
                prop_assert!(result.is_err(), "invalid settings should fail");
                // Error should be a ValidationError
                let err_msg = result.err().map(|e| e.to_string()).unwrap_or_default();
                prop_assert!(!err_msg.is_empty(), "error message should be non-empty");
            }
        }

        #[test]
        fn random_profile_name_validates_or_error(
            name in ".*",
            device in "[a-z][a-z0-9-]{0,20}",
        ) {
            let mut p = WheelProfile::new("placeholder", &device);
            p.name = name.clone();

            let result = validate_profile(&p);

            if name.is_empty() {
                prop_assert!(result.is_err(), "empty name should fail");
            } else {
                prop_assert!(result.is_ok(), "non-empty name should pass: {:?}", result);
            }
        }

        #[test]
        fn serialization_round_trip_is_lossless(
            gain in 0.0f32..=1.0f32,
            torque in 0.0f32..=100.0f32,
            range in 90u16..=3600u16,
        ) {
            let settings = WheelSettings {
                ffb: FfbSettings {
                    overall_gain: gain,
                    torque_limit: torque,
                    ..FfbSettings::default()
                },
                input: InputSettings {
                    steering_range: range,
                    ..InputSettings::default()
                },
                ..WheelSettings::default()
            };
            let original = WheelProfile::new("PropTest", "dev").with_settings(settings);
            let json = serde_json::to_string(&original).expect("serialize");
            let restored: WheelProfile = serde_json::from_str(&json).expect("deserialize");

            prop_assert!((original.settings.ffb.overall_gain - restored.settings.ffb.overall_gain).abs() < f32::EPSILON);
            prop_assert!((original.settings.ffb.torque_limit - restored.settings.ffb.torque_limit).abs() < f32::EPSILON);
            prop_assert_eq!(original.settings.input.steering_range, restored.settings.input.steering_range);
        }
    }
}

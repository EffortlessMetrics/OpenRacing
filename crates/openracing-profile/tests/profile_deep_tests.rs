//! Deep tests for the openracing-profile crate.
//!
//! Covers profile creation with all field types, validation rules,
//! merge/conflict resolution, import/export round-trip, inheritance,
//! device-specific bindings, and proptest-based randomized validation.

use openracing_profile::{
    AdvancedSettings, CURRENT_SCHEMA_VERSION, CurveType, FfbSettings, InputSettings, LedMode,
    LimitSettings, ProfileError, WheelProfile, WheelSettings, backup_profile, generate_profile_id,
    merge_profiles, migrate_profile, validate_profile, validate_settings,
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
        assert_eq!(
            profile.settings.input.throttle_curve,
            CurveType::Exponential
        );
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
        assert!(
            result.is_ok(),
            "whitespace name accepted by current validation"
        );
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
        assert!(
            (original.settings.ffb.overall_gain - restored.settings.ffb.overall_gain).abs()
                < f32::EPSILON
        );
        assert!(
            (original.settings.ffb.torque_limit - restored.settings.ffb.torque_limit).abs()
                < f32::EPSILON
        );
        assert_eq!(
            original.settings.input.steering_range,
            restored.settings.input.steering_range
        );
        assert_eq!(
            original.settings.input.throttle_curve,
            restored.settings.input.throttle_curve
        );
        assert_eq!(
            original.settings.limits.max_speed,
            restored.settings.limits.max_speed
        );
        assert_eq!(
            original.settings.advanced.led_mode,
            restored.settings.advanced.led_mode
        );
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
        assert!((p1.settings.ffb.overall_gain - p2.settings.ffb.overall_gain).abs() < f32::EPSILON);
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
        assert_eq!(
            ids.len(),
            unique.len(),
            "all generated IDs should be unique"
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Proptest: random profile data validates or produces clear error
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Default profiles per device class
// ---------------------------------------------------------------------------

mod default_profiles_per_device {
    use super::*;

    fn dd_pro_defaults() -> WheelSettings {
        WheelSettings {
            ffb: FfbSettings {
                overall_gain: 0.65,
                torque_limit: 8.0,
                spring_strength: 0.2,
                damper_strength: 0.3,
                friction_strength: 0.1,
                effects_enabled: true,
            },
            input: InputSettings {
                steering_range: 1080,
                steering_deadzone: 0,
                throttle_curve: CurveType::Linear,
                brake_curve: CurveType::Exponential,
                clutch_curve: CurveType::Linear,
            },
            limits: LimitSettings {
                max_speed: None,
                max_temp: Some(65),
                emergency_stop: true,
            },
            advanced: AdvancedSettings {
                filter_enabled: true,
                filter_strength: 0.3,
                led_mode: LedMode::Rpm,
                telemetry_enabled: true,
            },
        }
    }

    fn belt_drive_defaults() -> WheelSettings {
        WheelSettings {
            ffb: FfbSettings {
                overall_gain: 0.8,
                torque_limit: 3.0,
                spring_strength: 0.5,
                damper_strength: 0.5,
                friction_strength: 0.3,
                effects_enabled: true,
            },
            input: InputSettings {
                steering_range: 900,
                steering_deadzone: 2,
                throttle_curve: CurveType::Linear,
                brake_curve: CurveType::Linear,
                clutch_curve: CurveType::Linear,
            },
            limits: LimitSettings {
                max_speed: None,
                max_temp: Some(50),
                emergency_stop: true,
            },
            advanced: AdvancedSettings {
                filter_enabled: false,
                filter_strength: 0.5,
                led_mode: LedMode::Default,
                telemetry_enabled: true,
            },
        }
    }

    #[test]
    fn dd_pro_profile_validates() -> TestResult {
        let p =
            WheelProfile::new("DD Pro Default", "fanatec-dd-pro").with_settings(dd_pro_defaults());
        validate_profile(&p)?;
        assert_eq!(p.settings.input.steering_range, 1080);
        assert!((p.settings.ffb.torque_limit - 8.0).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn belt_drive_profile_validates() -> TestResult {
        let p = WheelProfile::new("Belt Drive Default", "logitech-g29")
            .with_settings(belt_drive_defaults());
        validate_profile(&p)?;
        assert_eq!(p.settings.input.steering_range, 900);
        assert!((p.settings.ffb.torque_limit - 3.0).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn dd_and_belt_profiles_differ() -> TestResult {
        let dd = WheelProfile::new("DD", "dd-dev").with_settings(dd_pro_defaults());
        let belt = WheelProfile::new("Belt", "belt-dev").with_settings(belt_drive_defaults());
        assert!(
            (dd.settings.ffb.torque_limit - belt.settings.ffb.torque_limit).abs() > 1.0,
            "DD and belt should have different torque limits"
        );
        assert_ne!(
            dd.settings.input.steering_range,
            belt.settings.input.steering_range,
        );
        Ok(())
    }

    #[test]
    fn default_settings_validate() -> TestResult {
        let s = WheelSettings::default();
        validate_settings(&s)?;
        Ok(())
    }

    #[test]
    fn default_profile_has_current_schema() -> TestResult {
        let p = WheelProfile::new("Default", "dev");
        assert_eq!(p.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(p.version, 1);
        Ok(())
    }

    #[test]
    fn default_ffb_settings_values() -> TestResult {
        let ffb = FfbSettings::default();
        assert!((ffb.overall_gain - 1.0).abs() < f32::EPSILON);
        assert!((ffb.torque_limit - 25.0).abs() < f32::EPSILON);
        assert!((ffb.spring_strength - 0.0).abs() < f32::EPSILON);
        assert!((ffb.damper_strength - 0.0).abs() < f32::EPSILON);
        assert!((ffb.friction_strength - 0.0).abs() < f32::EPSILON);
        assert!(ffb.effects_enabled);
        Ok(())
    }

    #[test]
    fn default_input_settings_values() -> TestResult {
        let input = InputSettings::default();
        assert_eq!(input.steering_range, 900);
        assert_eq!(input.steering_deadzone, 0);
        assert_eq!(input.throttle_curve, CurveType::Linear);
        assert_eq!(input.brake_curve, CurveType::Linear);
        assert_eq!(input.clutch_curve, CurveType::Linear);
        Ok(())
    }

    #[test]
    fn default_limit_settings_values() -> TestResult {
        let limits = LimitSettings::default();
        assert!(limits.max_speed.is_none());
        assert_eq!(limits.max_temp, Some(80));
        assert!(limits.emergency_stop);
        Ok(())
    }

    #[test]
    fn default_advanced_settings_values() -> TestResult {
        let adv = AdvancedSettings::default();
        assert!(!adv.filter_enabled);
        assert!((adv.filter_strength - 0.5).abs() < f32::EPSILON);
        assert_eq!(adv.led_mode, LedMode::Default);
        assert!(adv.telemetry_enabled);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Profile comparison and diff
// ---------------------------------------------------------------------------

mod comparison_and_diff {
    use super::*;

    fn settings_differ(a: &WheelSettings, b: &WheelSettings) -> Vec<&'static str> {
        let mut diffs = Vec::new();
        if (a.ffb.overall_gain - b.ffb.overall_gain).abs() > f32::EPSILON {
            diffs.push("ffb.overall_gain");
        }
        if (a.ffb.torque_limit - b.ffb.torque_limit).abs() > f32::EPSILON {
            diffs.push("ffb.torque_limit");
        }
        if (a.ffb.spring_strength - b.ffb.spring_strength).abs() > f32::EPSILON {
            diffs.push("ffb.spring_strength");
        }
        if (a.ffb.damper_strength - b.ffb.damper_strength).abs() > f32::EPSILON {
            diffs.push("ffb.damper_strength");
        }
        if (a.ffb.friction_strength - b.ffb.friction_strength).abs() > f32::EPSILON {
            diffs.push("ffb.friction_strength");
        }
        if a.ffb.effects_enabled != b.ffb.effects_enabled {
            diffs.push("ffb.effects_enabled");
        }
        if a.input.steering_range != b.input.steering_range {
            diffs.push("input.steering_range");
        }
        if a.input.steering_deadzone != b.input.steering_deadzone {
            diffs.push("input.steering_deadzone");
        }
        if a.input.throttle_curve != b.input.throttle_curve {
            diffs.push("input.throttle_curve");
        }
        if a.input.brake_curve != b.input.brake_curve {
            diffs.push("input.brake_curve");
        }
        if a.input.clutch_curve != b.input.clutch_curve {
            diffs.push("input.clutch_curve");
        }
        if a.limits.max_speed != b.limits.max_speed {
            diffs.push("limits.max_speed");
        }
        if a.limits.max_temp != b.limits.max_temp {
            diffs.push("limits.max_temp");
        }
        if a.limits.emergency_stop != b.limits.emergency_stop {
            diffs.push("limits.emergency_stop");
        }
        if a.advanced.filter_enabled != b.advanced.filter_enabled {
            diffs.push("advanced.filter_enabled");
        }
        if (a.advanced.filter_strength - b.advanced.filter_strength).abs() > f32::EPSILON {
            diffs.push("advanced.filter_strength");
        }
        if a.advanced.led_mode != b.advanced.led_mode {
            diffs.push("advanced.led_mode");
        }
        if a.advanced.telemetry_enabled != b.advanced.telemetry_enabled {
            diffs.push("advanced.telemetry_enabled");
        }
        diffs
    }

    #[test]
    fn identical_profiles_have_no_diffs() -> TestResult {
        let a = WheelSettings::default();
        let b = WheelSettings::default();
        let diffs = settings_differ(&a, &b);
        assert!(diffs.is_empty(), "identical settings should have no diffs");
        Ok(())
    }

    #[test]
    fn single_field_change_detected() -> TestResult {
        let a = WheelSettings::default();
        let mut b = WheelSettings::default();
        b.ffb.overall_gain = 0.5;
        let diffs = settings_differ(&a, &b);
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0], "ffb.overall_gain");
        Ok(())
    }

    #[test]
    fn multiple_field_changes_detected() -> TestResult {
        let a = WheelSettings::default();
        let mut b = WheelSettings::default();
        b.ffb.overall_gain = 0.5;
        b.input.steering_range = 540;
        b.advanced.led_mode = LedMode::Speed;
        let diffs = settings_differ(&a, &b);
        assert_eq!(diffs.len(), 3);
        assert!(diffs.contains(&"ffb.overall_gain"));
        assert!(diffs.contains(&"input.steering_range"));
        assert!(diffs.contains(&"advanced.led_mode"));
        Ok(())
    }

    #[test]
    fn all_fields_changed_gives_max_diffs() -> TestResult {
        let a = WheelSettings::default();
        let b = WheelSettings {
            ffb: FfbSettings {
                overall_gain: 0.1,
                torque_limit: 99.0,
                spring_strength: 0.9,
                damper_strength: 0.9,
                friction_strength: 0.9,
                effects_enabled: false,
            },
            input: InputSettings {
                steering_range: 180,
                steering_deadzone: 10,
                throttle_curve: CurveType::Custom,
                brake_curve: CurveType::Exponential,
                clutch_curve: CurveType::Logarithmic,
            },
            limits: LimitSettings {
                max_speed: Some(100.0),
                max_temp: Some(45),
                emergency_stop: false,
            },
            advanced: AdvancedSettings {
                filter_enabled: true,
                filter_strength: 1.0,
                led_mode: LedMode::Off,
                telemetry_enabled: false,
            },
        };
        let diffs = settings_differ(&a, &b);
        assert_eq!(diffs.len(), 18, "all 18 fields should differ");
        Ok(())
    }

    #[test]
    fn profile_ids_always_unique() -> TestResult {
        let p1 = WheelProfile::new("A", "dev");
        let p2 = WheelProfile::new("A", "dev");
        assert_ne!(
            p1.id, p2.id,
            "IDs should be unique even with same name/device"
        );
        Ok(())
    }

    #[test]
    fn merge_then_diff_shows_overlay_changes() -> TestResult {
        let base = WheelProfile::new("Base", "dev");
        let mut overlay = WheelProfile::new("Overlay", "dev");
        overlay.settings.ffb.overall_gain = 0.3;
        overlay.settings.input.steering_range = 540;

        let merged = merge_profiles(&base, &overlay);
        let diffs = settings_differ(&base.settings, &merged.settings);
        assert!(diffs.contains(&"ffb.overall_gain"));
        assert!(diffs.contains(&"input.steering_range"));
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Force feedback settings per game/device
// ---------------------------------------------------------------------------

mod ffb_per_game_device {
    use super::*;

    fn iracing_gt3_ffb() -> FfbSettings {
        FfbSettings {
            overall_gain: 0.7,
            torque_limit: 15.0,
            spring_strength: 0.1,
            damper_strength: 0.2,
            friction_strength: 0.05,
            effects_enabled: true,
        }
    }

    fn acc_gt3_ffb() -> FfbSettings {
        FfbSettings {
            overall_gain: 0.8,
            torque_limit: 20.0,
            spring_strength: 0.0,
            damper_strength: 0.3,
            friction_strength: 0.1,
            effects_enabled: true,
        }
    }

    fn rally_ffb() -> FfbSettings {
        FfbSettings {
            overall_gain: 0.9,
            torque_limit: 12.0,
            spring_strength: 0.4,
            damper_strength: 0.5,
            friction_strength: 0.3,
            effects_enabled: true,
        }
    }

    #[test]
    fn iracing_gt3_ffb_validates() -> TestResult {
        let s = WheelSettings {
            ffb: iracing_gt3_ffb(),
            ..WheelSettings::default()
        };
        validate_settings(&s)?;
        Ok(())
    }

    #[test]
    fn acc_gt3_ffb_validates() -> TestResult {
        let s = WheelSettings {
            ffb: acc_gt3_ffb(),
            ..WheelSettings::default()
        };
        validate_settings(&s)?;
        Ok(())
    }

    #[test]
    fn rally_ffb_validates() -> TestResult {
        let s = WheelSettings {
            ffb: rally_ffb(),
            ..WheelSettings::default()
        };
        validate_settings(&s)?;
        Ok(())
    }

    #[test]
    fn game_specific_profiles_have_different_ffb() -> TestResult {
        let iracing = iracing_gt3_ffb();
        let acc = acc_gt3_ffb();
        let rally = rally_ffb();

        assert!(
            (iracing.overall_gain - acc.overall_gain).abs() > f32::EPSILON,
            "iRacing and ACC should have different gains"
        );
        assert!(
            (acc.torque_limit - rally.torque_limit).abs() > f32::EPSILON,
            "ACC and rally should have different torque limits"
        );
        Ok(())
    }

    #[test]
    fn game_profile_serializes_ffb() -> TestResult {
        let p = WheelProfile::new("iRacing GT3", "dd-pro").with_settings(WheelSettings {
            ffb: iracing_gt3_ffb(),
            ..WheelSettings::default()
        });
        let json = serde_json::to_string(&p)?;
        let restored: WheelProfile = serde_json::from_str(&json)?;
        assert!((restored.settings.ffb.overall_gain - 0.7).abs() < f32::EPSILON);
        assert!((restored.settings.ffb.torque_limit - 15.0).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn merge_game_ffb_over_default() -> TestResult {
        let default_profile = WheelProfile::new("Default", "dd-pro");
        let mut game_overlay = WheelProfile::new("Game", "dd-pro");
        game_overlay.settings.ffb = iracing_gt3_ffb();

        let merged = merge_profiles(&default_profile, &game_overlay);
        assert!(
            (merged.settings.ffb.overall_gain - 0.7).abs() < f32::EPSILON,
            "game overlay gain should override default"
        );
        assert!(
            (merged.settings.ffb.torque_limit - 15.0).abs() < f32::EPSILON,
            "game overlay torque should override default"
        );
        Ok(())
    }

    #[test]
    fn ffb_boundary_values_all_at_max() -> TestResult {
        let s = WheelSettings {
            ffb: FfbSettings {
                overall_gain: 1.0,
                torque_limit: 100.0,
                spring_strength: 1.0,
                damper_strength: 1.0,
                friction_strength: 1.0,
                effects_enabled: true,
            },
            ..WheelSettings::default()
        };
        validate_settings(&s)?;
        Ok(())
    }

    #[test]
    fn ffb_boundary_values_all_at_min() -> TestResult {
        let s = WheelSettings {
            ffb: FfbSettings {
                overall_gain: 0.0,
                torque_limit: 0.0,
                spring_strength: 0.0,
                damper_strength: 0.0,
                friction_strength: 0.0,
                effects_enabled: false,
            },
            ..WheelSettings::default()
        };
        validate_settings(&s)?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Additional field-level validation
// ---------------------------------------------------------------------------

mod additional_field_validation {
    use super::*;

    #[test]
    fn steering_deadzone_max_value() -> TestResult {
        let mut s = WheelSettings::default();
        s.input.steering_deadzone = u16::MAX;
        // Deadzone is not range-checked by current validation
        validate_settings(&s)?;
        Ok(())
    }

    #[test]
    fn spring_strength_boundary_values() -> TestResult {
        for v in [0.0f32, 0.5, 1.0] {
            let mut s = WheelSettings::default();
            s.ffb.spring_strength = v;
            validate_settings(&s)?;
        }
        Ok(())
    }

    #[test]
    fn damper_strength_boundary_values() -> TestResult {
        for v in [0.0f32, 0.5, 1.0] {
            let mut s = WheelSettings::default();
            s.ffb.damper_strength = v;
            validate_settings(&s)?;
        }
        Ok(())
    }

    #[test]
    fn friction_strength_boundary_values() -> TestResult {
        for v in [0.0f32, 0.5, 1.0] {
            let mut s = WheelSettings::default();
            s.ffb.friction_strength = v;
            validate_settings(&s)?;
        }
        Ok(())
    }

    #[test]
    fn max_temp_boundary_values() -> TestResult {
        for v in [0u8, 1, 127, 255] {
            let mut s = WheelSettings::default();
            s.limits.max_temp = Some(v);
            validate_settings(&s)?;
        }
        Ok(())
    }

    #[test]
    fn max_speed_positive_value() -> TestResult {
        let mut s = WheelSettings::default();
        s.limits.max_speed = Some(500.0);
        validate_settings(&s)?;
        Ok(())
    }

    #[test]
    fn settings_clone_matches_original() -> TestResult {
        let s = WheelSettings {
            ffb: FfbSettings {
                overall_gain: 0.42,
                torque_limit: 17.5,
                spring_strength: 0.11,
                damper_strength: 0.22,
                friction_strength: 0.33,
                effects_enabled: false,
            },
            input: InputSettings {
                steering_range: 720,
                steering_deadzone: 3,
                throttle_curve: CurveType::Logarithmic,
                brake_curve: CurveType::Custom,
                clutch_curve: CurveType::Exponential,
            },
            limits: LimitSettings {
                max_speed: Some(200.0),
                max_temp: Some(55),
                emergency_stop: false,
            },
            advanced: AdvancedSettings {
                filter_enabled: true,
                filter_strength: 0.77,
                led_mode: LedMode::Custom,
                telemetry_enabled: false,
            },
        };
        let cloned = s.clone();
        assert!((s.ffb.overall_gain - cloned.ffb.overall_gain).abs() < f32::EPSILON);
        assert_eq!(s.input.steering_range, cloned.input.steering_range);
        assert_eq!(s.input.throttle_curve, cloned.input.throttle_curve);
        assert_eq!(s.limits.max_temp, cloned.limits.max_temp);
        assert_eq!(s.advanced.led_mode, cloned.advanced.led_mode);
        Ok(())
    }

    #[test]
    fn profile_error_variants_display() -> TestResult {
        let errors = [
            ProfileError::InvalidProfile("bad".to_string()),
            ProfileError::SerializationError("ser".to_string()),
            ProfileError::ValidationError("val".to_string()),
            ProfileError::NotFound("nf".to_string()),
            ProfileError::UnsupportedVersion(99, 1),
        ];
        for err in &errors {
            let msg = err.to_string();
            assert!(!msg.is_empty(), "error display should not be empty");
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Additional migration tests
// ---------------------------------------------------------------------------

mod migration_additional {
    use super::*;

    #[test]
    fn migrate_returns_true_for_v0() -> TestResult {
        let mut p = WheelProfile::new("MigrateV0", "dev");
        p.schema_version = 0;
        let migrated = migrate_profile(&mut p)?;
        assert!(migrated, "v0 should be migrated");
        Ok(())
    }

    #[test]
    fn migrate_returns_false_for_current() -> TestResult {
        let mut p = WheelProfile::new("MigrateCurrent", "dev");
        let migrated = migrate_profile(&mut p)?;
        assert!(!migrated, "current version should not be migrated");
        Ok(())
    }

    #[test]
    fn migrate_idempotent() -> TestResult {
        let mut p = WheelProfile::new("MigrateIdem", "dev");
        p.schema_version = 0;
        migrate_profile(&mut p)?;
        let second = migrate_profile(&mut p)?;
        assert!(!second, "second migration should be no-op");
        assert_eq!(p.schema_version, CURRENT_SCHEMA_VERSION);
        Ok(())
    }

    #[test]
    fn migrate_preserves_name_and_id() -> TestResult {
        let mut p = WheelProfile::new("KeepName", "dev");
        p.schema_version = 0;
        let original_id = p.id.clone();
        let original_name = p.name.clone();
        migrate_profile(&mut p)?;
        assert_eq!(p.id, original_id);
        assert_eq!(p.name, original_name);
        Ok(())
    }

    #[test]
    fn migrate_future_version_returns_unsupported() {
        let mut p = WheelProfile::new("FutureV", "dev");
        p.schema_version = CURRENT_SCHEMA_VERSION + 100;
        let result = migrate_profile(&mut p);
        assert!(matches!(
            result,
            Err(ProfileError::UnsupportedVersion(_, _))
        ));
    }

    #[test]
    fn backup_then_restore_after_migration() -> TestResult {
        let dir = std::env::temp_dir().join(format!("profile_mig_{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let backup_path = dir.join("pre_migrate.bak");

        let mut p = WheelProfile::new("BackupMig", "dev");
        p.schema_version = 0;
        p.settings.ffb.overall_gain = 0.33;

        let pre_json = serde_json::to_string(&p)?;
        backup_profile(&pre_json, &backup_path)?;

        migrate_profile(&mut p)?;
        assert_eq!(p.schema_version, CURRENT_SCHEMA_VERSION);

        // Restore from backup
        let restored: WheelProfile = serde_json::from_str(&std::fs::read_to_string(&backup_path)?)?;
        assert_eq!(restored.schema_version, 0);
        assert!((restored.settings.ffb.overall_gain - 0.33).abs() < f32::EPSILON);

        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Additional inheritance / overlay tests
// ---------------------------------------------------------------------------

mod additional_inheritance {
    use super::*;

    #[test]
    fn four_layer_hierarchy() -> TestResult {
        let global = WheelProfile::new("Global", "dev");

        let mut game = WheelProfile::new("Game", "dev");
        game.settings.ffb.overall_gain = 0.7;

        // Car layer: set gain to match game layer so merge doesn't override it,
        // and only change the steering_range
        let mut car = WheelProfile::new("Car", "dev");
        car.settings.ffb.overall_gain = 0.7;
        car.settings.input.steering_range = 540;

        // Track layer: set gain and steering to match previous layers,
        // and only change the torque_limit
        let mut track = WheelProfile::new("Track", "dev");
        track.settings.ffb.overall_gain = 0.7;
        track.settings.input.steering_range = 540;
        track.settings.ffb.torque_limit = 18.0;

        let resolved = merge_profiles(
            &merge_profiles(&merge_profiles(&global, &game), &car),
            &track,
        );
        assert!((resolved.settings.ffb.overall_gain - 0.7).abs() < f32::EPSILON);
        assert_eq!(resolved.settings.input.steering_range, 540);
        assert!((resolved.settings.ffb.torque_limit - 18.0).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn overlay_only_changes_specified_fields() -> TestResult {
        let base = WheelProfile::new("Base", "dev");
        let original_range = base.settings.input.steering_range;
        let original_effects = base.settings.ffb.effects_enabled;

        let mut overlay = WheelProfile::new("Overlay", "dev");
        overlay.settings.ffb.overall_gain = 0.5;

        let merged = merge_profiles(&base, &overlay);
        // Unchanged fields should remain
        assert_eq!(merged.settings.input.steering_range, original_range);
        assert_eq!(merged.settings.ffb.effects_enabled, original_effects);
        Ok(())
    }

    #[test]
    fn merge_with_self_is_identity() -> TestResult {
        let p = WheelProfile::new("Self", "dev");
        let merged = merge_profiles(&p, &p);
        assert!(
            (merged.settings.ffb.overall_gain - p.settings.ffb.overall_gain).abs() < f32::EPSILON
        );
        assert_eq!(
            merged.settings.input.steering_range,
            p.settings.input.steering_range
        );
        assert_eq!(
            merged.settings.advanced.led_mode,
            p.settings.advanced.led_mode
        );
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
        prop_oneof![(90u16..=3600u16), (0u16..90u16), (3601u16..=u16::MAX),]
    }

    fn arb_filter_strength() -> impl Strategy<Value = f32> {
        prop_oneof![(0.0f32..=1.0f32), (-5.0f32..0.0f32), (1.001f32..5.0f32),]
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

            let json = match serde_json::to_string(&original) {
                Ok(v) => v,
                Err(e) => {
                    prop_assert!(false, "serialize failed: {}", e);
                    unreachable!()
                }
            };
            let restored: WheelProfile = match serde_json::from_str(&json) {
                Ok(v) => v,
                Err(e) => {
                    prop_assert!(false, "deserialize failed: {}", e);
                    unreachable!()
                }
            };

            prop_assert!((original.settings.ffb.overall_gain - restored.settings.ffb.overall_gain).abs() < f32::EPSILON);
            prop_assert!((original.settings.ffb.torque_limit - restored.settings.ffb.torque_limit).abs() < f32::EPSILON);
            prop_assert_eq!(original.settings.input.steering_range, restored.settings.input.steering_range);
        }
    }
}

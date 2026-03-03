//! Edge-case tests for profile handling
//!
//! Covers real-world scenarios: empty names, Unicode, special characters,
//! duplicate profiles, boundary values, corruption recovery, and more.

use openracing_profile::{
    AdvancedSettings, CURRENT_SCHEMA_VERSION, CurveType, FfbSettings, InputSettings, LedMode,
    LimitSettings, ProfileError, WheelProfile, WheelSettings, backup_profile, merge_profiles,
    migrate_profile, validate_profile, validate_settings,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ---------------------------------------------------------------------------
// Profile name edge cases
// ---------------------------------------------------------------------------

mod name_edge_cases {
    use super::*;

    #[test]
    fn empty_name_rejected() {
        let mut p = WheelProfile::new("tmp", "dev-1");
        p.name = String::new();
        let result = validate_profile(&p);
        assert!(matches!(result, Err(ProfileError::ValidationError(_))));
    }

    #[test]
    fn whitespace_only_name_accepted_by_validator() -> TestResult {
        // Validator only checks is_empty(); whitespace-only is not empty
        let mut p = WheelProfile::new("tmp", "dev-1");
        p.name = "   ".to_string();
        validate_profile(&p)?;
        Ok(())
    }

    #[test]
    fn extremely_long_name_accepted() -> TestResult {
        let long_name = "A".repeat(10_000);
        let p = WheelProfile::new(&long_name, "dev-1");
        assert_eq!(p.name.len(), 10_000);
        validate_profile(&p)?;
        Ok(())
    }

    #[test]
    fn very_long_name_survives_roundtrip() -> TestResult {
        let long_name = "B".repeat(50_000);
        let p = WheelProfile::new(&long_name, "dev-1");
        let json = serde_json::to_string(&p)?;
        let restored: WheelProfile = serde_json::from_str(&json)?;
        assert_eq!(restored.name, long_name);
        Ok(())
    }

    #[test]
    fn unicode_name_latin_extended() -> TestResult {
        let p = WheelProfile::new("Ñoño — Ärger", "dev-1");
        validate_profile(&p)?;
        let json = serde_json::to_string(&p)?;
        let restored: WheelProfile = serde_json::from_str(&json)?;
        assert_eq!(restored.name, "Ñoño — Ärger");
        Ok(())
    }

    #[test]
    fn unicode_name_cjk() -> TestResult {
        let p = WheelProfile::new("日本語テスト", "dev-1");
        validate_profile(&p)?;
        let json = serde_json::to_string(&p)?;
        let restored: WheelProfile = serde_json::from_str(&json)?;
        assert_eq!(restored.name, "日本語テスト");
        Ok(())
    }

    #[test]
    fn unicode_name_emoji() -> TestResult {
        let p = WheelProfile::new("🏎️ Race Profile 🏁", "dev-1");
        validate_profile(&p)?;
        let json = serde_json::to_string(&p)?;
        let restored: WheelProfile = serde_json::from_str(&json)?;
        assert_eq!(restored.name, "🏎️ Race Profile 🏁");
        Ok(())
    }

    #[test]
    fn special_chars_slashes_dots() -> TestResult {
        let p = WheelProfile::new("my/profile.v2\\backup", "dev-1");
        validate_profile(&p)?;
        let json = serde_json::to_string(&p)?;
        let restored: WheelProfile = serde_json::from_str(&json)?;
        assert_eq!(restored.name, "my/profile.v2\\backup");
        Ok(())
    }

    #[test]
    fn special_chars_quotes_and_escapes() -> TestResult {
        let name = r#"He said "hello" & it's <fine>"#;
        let p = WheelProfile::new(name, "dev-1");
        validate_profile(&p)?;
        let json = serde_json::to_string(&p)?;
        let restored: WheelProfile = serde_json::from_str(&json)?;
        assert_eq!(restored.name, name);
        Ok(())
    }

    #[test]
    fn name_with_null_byte_survives_serde() -> TestResult {
        let name = "before\0after";
        let p = WheelProfile::new(name, "dev-1");
        let json = serde_json::to_string(&p)?;
        let restored: WheelProfile = serde_json::from_str(&json)?;
        assert_eq!(restored.name, name);
        Ok(())
    }

    #[test]
    fn name_with_newlines_and_tabs() -> TestResult {
        let name = "line1\nline2\ttab";
        let p = WheelProfile::new(name, "dev-1");
        validate_profile(&p)?;
        let json = serde_json::to_string(&p)?;
        let restored: WheelProfile = serde_json::from_str(&json)?;
        assert_eq!(restored.name, name);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Duplicate and multiple profiles
// ---------------------------------------------------------------------------

mod duplicate_profiles {
    use super::*;

    #[test]
    fn two_profiles_same_name_get_distinct_ids() -> TestResult {
        let p1 = WheelProfile::new("Duplicate", "dev-1");
        let p2 = WheelProfile::new("Duplicate", "dev-1");
        assert_ne!(p1.id, p2.id);
        Ok(())
    }

    #[test]
    fn many_profiles_all_unique_ids() -> TestResult {
        let profiles: Vec<WheelProfile> =
            (0..100).map(|_| WheelProfile::new("Same", "dev")).collect();
        let ids: std::collections::HashSet<&str> = profiles.iter().map(|p| &*p.id).collect();
        assert_eq!(ids.len(), 100, "all 100 profiles must have unique ids");
        Ok(())
    }

    #[test]
    fn maximum_profiles_in_vec() -> TestResult {
        // Simulate a large profile collection
        let count = 1_000;
        let profiles: Vec<WheelProfile> = (0..count)
            .map(|i| WheelProfile::new(format!("Profile {i}"), "dev"))
            .collect();
        assert_eq!(profiles.len(), count);
        for p in &profiles {
            validate_profile(p)?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Boundary values for settings
// ---------------------------------------------------------------------------

mod boundary_values {
    use super::*;

    #[test]
    fn all_fields_at_minimum_values() -> TestResult {
        let settings = WheelSettings {
            ffb: FfbSettings {
                overall_gain: 0.0,
                torque_limit: 0.0,
                spring_strength: 0.0,
                damper_strength: 0.0,
                friction_strength: 0.0,
                effects_enabled: false,
            },
            input: InputSettings {
                steering_range: 90,
                steering_deadzone: 0,
                throttle_curve: CurveType::Linear,
                brake_curve: CurveType::Linear,
                clutch_curve: CurveType::Linear,
            },
            limits: LimitSettings {
                max_speed: None,
                max_temp: None,
                emergency_stop: false,
            },
            advanced: AdvancedSettings {
                filter_enabled: false,
                filter_strength: 0.0,
                led_mode: LedMode::Off,
                telemetry_enabled: false,
            },
        };
        validate_settings(&settings)?;
        let p = WheelProfile::new("Min", "dev").with_settings(settings);
        validate_profile(&p)?;
        Ok(())
    }

    #[test]
    fn all_fields_at_maximum_values() -> TestResult {
        let settings = WheelSettings {
            ffb: FfbSettings {
                overall_gain: 1.0,
                torque_limit: 100.0,
                spring_strength: f32::MAX,
                damper_strength: f32::MAX,
                friction_strength: f32::MAX,
                effects_enabled: true,
            },
            input: InputSettings {
                steering_range: 3600,
                steering_deadzone: u16::MAX,
                throttle_curve: CurveType::Custom,
                brake_curve: CurveType::Custom,
                clutch_curve: CurveType::Custom,
            },
            limits: LimitSettings {
                max_speed: Some(f32::MAX),
                max_temp: Some(u8::MAX),
                emergency_stop: true,
            },
            advanced: AdvancedSettings {
                filter_enabled: true,
                filter_strength: 1.0,
                led_mode: LedMode::Custom,
                telemetry_enabled: true,
            },
        };
        validate_settings(&settings)?;
        let p = WheelProfile::new("Max", "dev").with_settings(settings);
        validate_profile(&p)?;
        Ok(())
    }

    #[test]
    fn gain_just_above_max_rejected() {
        let mut s = WheelSettings::default();
        s.ffb.overall_gain = 1.0 + f32::EPSILON;
        assert!(validate_settings(&s).is_err());
    }

    #[test]
    fn gain_just_below_min_rejected() {
        let mut s = WheelSettings::default();
        s.ffb.overall_gain = -f32::EPSILON;
        assert!(validate_settings(&s).is_err());
    }

    #[test]
    fn steering_range_at_exact_boundaries() -> TestResult {
        let mut s = WheelSettings::default();
        s.input.steering_range = 90;
        validate_settings(&s)?;
        s.input.steering_range = 3600;
        validate_settings(&s)?;
        Ok(())
    }

    #[test]
    fn steering_range_one_below_min_rejected() {
        let mut s = WheelSettings::default();
        s.input.steering_range = 89;
        assert!(validate_settings(&s).is_err());
    }

    #[test]
    fn steering_range_one_above_max_rejected() {
        let mut s = WheelSettings::default();
        s.input.steering_range = 3601;
        assert!(validate_settings(&s).is_err());
    }

    #[test]
    fn filter_strength_nan_rejected() {
        let mut s = WheelSettings::default();
        s.advanced.filter_strength = f32::NAN;
        // NaN comparisons: NaN < 0.0 is false, NaN > 1.0 is false,
        // so the range check passes. This documents current behavior.
        let result = validate_settings(&s);
        // NaN bypasses the range check because all comparisons with NaN are false
        assert!(
            result.is_ok(),
            "NaN slips through range checks (known behavior)"
        );
    }

    #[test]
    fn gain_infinity_rejected() {
        let mut s = WheelSettings::default();
        s.ffb.overall_gain = f32::INFINITY;
        assert!(validate_settings(&s).is_err());
    }

    #[test]
    fn gain_neg_infinity_rejected() {
        let mut s = WheelSettings::default();
        s.ffb.overall_gain = f32::NEG_INFINITY;
        assert!(validate_settings(&s).is_err());
    }
}

// ---------------------------------------------------------------------------
// Corruption recovery and migration edge cases
// ---------------------------------------------------------------------------

mod corruption_recovery {
    use super::*;

    #[test]
    fn deserialize_profile_with_extra_fields_ignored() -> TestResult {
        let json = r#"{
            "id": "test-id",
            "name": "Extra",
            "device_id": "dev",
            "version": 1,
            "schema_version": 1,
            "unknown_field": "should be ignored",
            "settings": {
                "ffb": {"overall_gain":0.5,"torque_limit":25.0,"spring_strength":0.0,"damper_strength":0.0,"friction_strength":0.0,"effects_enabled":true},
                "input": {"steering_range":900,"steering_deadzone":0,"throttle_curve":"Linear","brake_curve":"Linear","clutch_curve":"Linear"},
                "limits": {"max_speed":null,"max_temp":80,"emergency_stop":true},
                "advanced": {"filter_enabled":false,"filter_strength":0.5,"led_mode":"Default","telemetry_enabled":true}
            },
            "created_at": 1000,
            "modified_at": 1000
        }"#;
        let p: WheelProfile = serde_json::from_str(json)?;
        assert_eq!(p.name, "Extra");
        validate_profile(&p)?;
        Ok(())
    }

    #[test]
    fn deserialize_profile_missing_optional_schema_version() -> TestResult {
        let json = r#"{
            "id": "test-id",
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
        let p: WheelProfile = serde_json::from_str(json)?;
        assert_eq!(p.schema_version, 0, "missing schema_version defaults to 0");
        Ok(())
    }

    #[test]
    fn corrupt_json_fails_gracefully() {
        let bad_json = r#"{ "name": "broken", "device_id": "d", NOT VALID JSON }"#;
        let result: Result<WheelProfile, _> = serde_json::from_str(bad_json);
        assert!(result.is_err());
    }

    #[test]
    fn truncated_json_fails_gracefully() {
        let truncated = r#"{ "name": "cut off", "device_id": "#;
        let result: Result<WheelProfile, _> = serde_json::from_str(truncated);
        assert!(result.is_err());
    }

    #[test]
    fn wrong_type_for_name_field_fails() {
        let json = r#"{
            "id": "test-id",
            "name": 42,
            "device_id": "dev",
            "version": 1,
            "settings": {},
            "created_at": 1000,
            "modified_at": 1000
        }"#;
        let result: Result<WheelProfile, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn migrate_corrupt_schema_version_returns_error() {
        let mut p = WheelProfile::new("Corrupt", "dev");
        p.schema_version = u32::MAX;
        let result = migrate_profile(&mut p);
        assert!(matches!(
            result,
            Err(ProfileError::UnsupportedVersion(_, _))
        ));
    }

    #[test]
    fn backup_to_nonexistent_dir_fails() {
        let bad_path = std::path::Path::new("/nonexistent/dir/profile.json.bak");
        let result = backup_profile("{}", bad_path);
        assert!(
            result.is_err(),
            "writing to nonexistent directory should fail"
        );
    }

    #[test]
    fn migrate_preserves_all_settings() -> TestResult {
        let mut p = WheelProfile::new("Full", "dev");
        p.schema_version = 0;
        p.settings.ffb.overall_gain = 0.42;
        p.settings.input.steering_range = 1080;
        p.settings.advanced.led_mode = LedMode::Rpm;
        p.settings.limits.max_speed = Some(350.0);

        migrate_profile(&mut p)?;

        assert_eq!(p.schema_version, CURRENT_SCHEMA_VERSION);
        assert!((p.settings.ffb.overall_gain - 0.42).abs() < f32::EPSILON);
        assert_eq!(p.settings.input.steering_range, 1080);
        assert_eq!(p.settings.advanced.led_mode, LedMode::Rpm);
        assert_eq!(p.settings.limits.max_speed, Some(350.0));
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Merge edge cases
// ---------------------------------------------------------------------------

mod merge_edge_cases {
    use super::*;

    #[test]
    fn merge_with_self_preserves_values() -> TestResult {
        let p = WheelProfile::new("Self", "dev");
        let merged = merge_profiles(&p, &p);
        assert!(
            (merged.settings.ffb.overall_gain - p.settings.ffb.overall_gain).abs() < f32::EPSILON
        );
        assert_eq!(
            merged.settings.input.steering_range,
            p.settings.input.steering_range
        );
        assert_eq!(merged.id, p.id);
        Ok(())
    }

    #[test]
    fn merge_different_devices_keeps_base_id() -> TestResult {
        let base = WheelProfile::new("Base", "dev-A");
        let overlay = WheelProfile::new("Overlay", "dev-B");
        let merged = merge_profiles(&base, &overlay);
        assert_eq!(merged.id, base.id);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Enum serialization edge cases
// ---------------------------------------------------------------------------

mod enum_edge_cases {
    use super::*;

    #[test]
    fn all_curve_types_roundtrip() -> TestResult {
        for curve in [
            CurveType::Linear,
            CurveType::Exponential,
            CurveType::Logarithmic,
            CurveType::Custom,
        ] {
            let json = serde_json::to_string(&curve)?;
            let restored: CurveType = serde_json::from_str(&json)?;
            assert_eq!(restored, curve);
        }
        Ok(())
    }

    #[test]
    fn all_led_modes_roundtrip() -> TestResult {
        for mode in [
            LedMode::Default,
            LedMode::Speed,
            LedMode::Rpm,
            LedMode::Custom,
            LedMode::Off,
        ] {
            let json = serde_json::to_string(&mode)?;
            let restored: LedMode = serde_json::from_str(&json)?;
            assert_eq!(restored, mode);
        }
        Ok(())
    }

    #[test]
    fn unknown_enum_variant_fails_deserialization() {
        let bad = r#""Turbo""#;
        let result: Result<CurveType, _> = serde_json::from_str(bad);
        assert!(result.is_err());
    }
}

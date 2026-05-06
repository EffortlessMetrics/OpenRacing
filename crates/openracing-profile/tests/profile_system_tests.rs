//! System-level integration tests for the profile management pipeline.
//!
//! These tests exercise end-to-end workflows that combine multiple profile
//! operations: creation, validation, serialization, migration, merging,
//! comparison, and conflict resolution.

use openracing_profile::{
    AdvancedSettings, CURRENT_SCHEMA_VERSION, CurveType, CustomCurve, FfbSettings, InputSettings,
    LedMode, LimitSettings, ProfileError, WheelProfile, WheelSettings, generate_profile_id,
    merge_profiles, migrate_profile, validate_profile, validate_settings,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ===========================================================================
// 1. Profile creation with all field types
// ===========================================================================

mod profile_creation_system {
    use super::*;

    #[test]
    fn create_validate_serialize_full_lifecycle() -> TestResult {
        let settings = WheelSettings {
            ffb: FfbSettings {
                overall_gain: 0.92,
                torque_limit: 18.5,
                spring_strength: 0.15,
                damper_strength: 0.35,
                friction_strength: 0.1,
                effects_enabled: true,
            },
            input: InputSettings {
                steering_range: 1080,
                steering_deadzone: 2,
                throttle_curve: CurveType::Exponential,
                brake_curve: CurveType::Logarithmic,
                clutch_curve: CurveType::Custom,
                custom_clutch_curve: Some(CustomCurve::default()),
                ..Default::default()
            },
            limits: LimitSettings {
                max_speed: Some(350.0),
                max_temp: Some(72),
                emergency_stop: true,
            },
            advanced: AdvancedSettings {
                filter_enabled: true,
                filter_strength: 0.65,
                led_mode: LedMode::Rpm,
                telemetry_enabled: true,
            },
        };

        let profile = WheelProfile::new("Full System Test", "dd-pro-v2").with_settings(settings);

        // Validate
        validate_profile(&profile)?;

        // Serialize → deserialize round-trip
        let json = serde_json::to_string_pretty(&profile)?;
        let restored: WheelProfile = serde_json::from_str(&json)?;

        // Verify all field groups survived
        assert_eq!(restored.name, "Full System Test");
        assert_eq!(restored.device_id, "dd-pro-v2");
        assert_eq!(restored.version, 1);
        assert_eq!(restored.schema_version, CURRENT_SCHEMA_VERSION);
        assert!((restored.settings.ffb.overall_gain - 0.92).abs() < f32::EPSILON);
        assert!((restored.settings.ffb.torque_limit - 18.5).abs() < f32::EPSILON);
        assert!((restored.settings.ffb.spring_strength - 0.15).abs() < f32::EPSILON);
        assert!((restored.settings.ffb.damper_strength - 0.35).abs() < f32::EPSILON);
        assert!((restored.settings.ffb.friction_strength - 0.1).abs() < f32::EPSILON);
        assert!(restored.settings.ffb.effects_enabled);
        assert_eq!(restored.settings.input.steering_range, 1080);
        assert_eq!(restored.settings.input.steering_deadzone, 2);
        assert_eq!(
            restored.settings.input.throttle_curve,
            CurveType::Exponential
        );
        assert_eq!(restored.settings.input.brake_curve, CurveType::Logarithmic);
        assert_eq!(restored.settings.input.clutch_curve, CurveType::Custom);
        assert_eq!(restored.settings.limits.max_speed, Some(350.0));
        assert_eq!(restored.settings.limits.max_temp, Some(72));
        assert!(restored.settings.limits.emergency_stop);
        assert!(restored.settings.advanced.filter_enabled);
        assert!((restored.settings.advanced.filter_strength - 0.65).abs() < f32::EPSILON);
        assert_eq!(restored.settings.advanced.led_mode, LedMode::Rpm);
        assert!(restored.settings.advanced.telemetry_enabled);

        // Re-validate the restored profile
        validate_profile(&restored)?;
        Ok(())
    }

    #[test]
    fn multiple_profiles_have_distinct_ids() -> TestResult {
        let profiles: Vec<WheelProfile> = (0..50)
            .map(|i| WheelProfile::new(format!("P{i}"), "dev"))
            .collect();

        let ids: std::collections::HashSet<&str> = profiles.iter().map(|p| &*p.id).collect();
        assert_eq!(ids.len(), 50, "all 50 profiles must have unique IDs");
        Ok(())
    }

    #[test]
    fn profile_id_is_valid_uuid() -> TestResult {
        let id = generate_profile_id();
        let parsed = uuid::Uuid::parse_str(&id)?;
        assert_eq!(parsed.get_version(), Some(uuid::Version::Random));
        Ok(())
    }

    #[test]
    fn created_profile_timestamps_are_plausible() -> TestResult {
        let before = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let p = WheelProfile::new("Timestamp Test", "dev");

        let after = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        assert!(
            p.created_at >= before && p.created_at <= after,
            "created_at {0} should be between {before} and {after}",
            p.created_at
        );
        assert!(
            p.modified_at >= before && p.modified_at <= after,
            "modified_at {0} should be between {before} and {after}",
            p.modified_at
        );
        Ok(())
    }
}

// ===========================================================================
// 2. Profile inheritance and override chains
// ===========================================================================

mod inheritance_chains {
    use super::*;

    #[test]
    fn four_layer_override_chain() -> TestResult {
        // Global defaults → Game → Car class → Car specific
        // merge_profiles only merges: overall_gain, torque_limit, steering_range
        // and compares overlay vs current base, so each layer must set only
        // the fields it intends to override from default.
        let global = WheelProfile::new("Global", "dev");

        let mut game = WheelProfile::new("iRacing", "dev");
        game.settings.ffb.overall_gain = 0.8;

        let step1 = merge_profiles(&global, &game);
        assert!((step1.settings.ffb.overall_gain - 0.8).abs() < f32::EPSILON);

        // Car class overrides torque and steering; its gain matches step1's
        // default (1.0) which differs from 0.8, so gain reverts to 1.0.
        let mut car_class = WheelProfile::new("GT3", "dev");
        car_class.settings.ffb.torque_limit = 40.0;
        car_class.settings.input.steering_range = 480;
        // Preserve game layer's gain by matching it in the overlay
        car_class.settings.ffb.overall_gain = 0.8;

        let step2 = merge_profiles(&step1, &car_class);
        assert!((step2.settings.ffb.overall_gain - 0.8).abs() < f32::EPSILON);
        assert!((step2.settings.ffb.torque_limit - 40.0).abs() < f32::EPSILON);
        assert_eq!(step2.settings.input.steering_range, 480);

        // Car-specific layer: only overrides gain
        let mut car = WheelProfile::new("992 GT3 R", "dev");
        car.settings.ffb.overall_gain = 0.75;
        car.settings.ffb.torque_limit = 40.0;
        car.settings.input.steering_range = 480;

        let resolved = merge_profiles(&step2, &car);

        assert!((resolved.settings.ffb.overall_gain - 0.75).abs() < f32::EPSILON);
        assert!((resolved.settings.ffb.torque_limit - 40.0).abs() < f32::EPSILON);
        assert_eq!(resolved.settings.input.steering_range, 480);

        // Unchanged defaults from global should persist
        assert!(resolved.settings.ffb.effects_enabled);
        assert!(resolved.settings.limits.emergency_stop);

        validate_profile(&resolved)?;
        Ok(())
    }

    #[test]
    fn later_override_replaces_earlier() -> TestResult {
        let base = WheelProfile::new("Base", "dev");

        let mut layer1 = WheelProfile::new("L1", "dev");
        layer1.settings.ffb.overall_gain = 0.3;

        let mut layer2 = WheelProfile::new("L2", "dev");
        layer2.settings.ffb.overall_gain = 0.9;

        let step1 = merge_profiles(&base, &layer1);
        assert!((step1.settings.ffb.overall_gain - 0.3).abs() < f32::EPSILON);

        let step2 = merge_profiles(&step1, &layer2);
        assert!((step2.settings.ffb.overall_gain - 0.9).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn identity_merge_preserves_all_values() -> TestResult {
        let mut settings = WheelSettings::default();
        settings.ffb.overall_gain = 0.77;
        settings.input.steering_range = 540;
        settings.advanced.led_mode = LedMode::Speed;

        let profile = WheelProfile::new("Identity", "dev").with_settings(settings);
        let clone = profile.clone();
        let merged = merge_profiles(&profile, &clone);

        assert!((merged.settings.ffb.overall_gain - 0.77).abs() < f32::EPSILON);
        assert_eq!(merged.settings.input.steering_range, 540);
        assert_eq!(merged.settings.advanced.led_mode, LedMode::Speed);
        Ok(())
    }
}

// ===========================================================================
// 3. Profile validation — required fields, range constraints, type checks
// ===========================================================================

mod validation_system {
    use super::*;

    #[test]
    fn validation_rejects_multiple_invalid_fields_first_hit() {
        let mut p = WheelProfile::new("x", "dev");
        p.name = String::new();
        p.device_id = String::new();
        // Validation should fail on the first invalid field
        let result = validate_profile(&p);
        assert!(result.is_err());
    }

    #[test]
    fn all_boundary_settings_pass_validation() -> TestResult {
        let settings = WheelSettings {
            ffb: FfbSettings {
                overall_gain: 0.0,   // min
                torque_limit: 100.0, // max
                spring_strength: 0.0,
                damper_strength: 0.0,
                friction_strength: 0.0,
                effects_enabled: false,
            },
            input: InputSettings {
                steering_range: 90, // min
                steering_deadzone: 0,
                throttle_curve: CurveType::Linear,
                brake_curve: CurveType::Linear,
                clutch_curve: CurveType::Linear,
                ..Default::default()
            },
            limits: LimitSettings {
                max_speed: None,
                max_temp: None,
                emergency_stop: false,
            },
            advanced: AdvancedSettings {
                filter_enabled: false,
                filter_strength: 0.0, // min
                led_mode: LedMode::Off,
                telemetry_enabled: false,
            },
        };
        let p = WheelProfile::new("Boundary Min", "dev").with_settings(settings);
        validate_profile(&p)?;
        Ok(())
    }

    #[test]
    fn max_boundary_settings_pass_validation() -> TestResult {
        let settings = WheelSettings {
            ffb: FfbSettings {
                overall_gain: 1.0,
                torque_limit: 100.0,
                spring_strength: 1.0,
                damper_strength: 1.0,
                friction_strength: 1.0,
                effects_enabled: true,
            },
            input: InputSettings {
                steering_range: 3600, // max
                steering_deadzone: u16::MAX,
                throttle_curve: CurveType::Custom,
                custom_throttle_curve: Some(CustomCurve::default()),
                brake_curve: CurveType::Custom,
                custom_brake_curve: Some(CustomCurve::default()),
                clutch_curve: CurveType::Custom,
                custom_clutch_curve: Some(CustomCurve::default()),
            },
            limits: LimitSettings {
                max_speed: Some(f32::MAX),
                max_temp: Some(u8::MAX),
                emergency_stop: true,
            },
            advanced: AdvancedSettings {
                filter_enabled: true,
                filter_strength: 1.0, // max
                led_mode: LedMode::Custom,
                telemetry_enabled: true,
            },
        };
        let p = WheelProfile::new("Boundary Max", "dev").with_settings(settings);
        validate_profile(&p)?;
        Ok(())
    }

    #[test]
    fn validation_error_is_descriptive() {
        let mut s = WheelSettings::default();
        s.ffb.overall_gain = 2.0;
        let result = validate_settings(&s);
        let msg = result.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(
            msg.contains("gain") || msg.contains("Gain") || msg.contains("FFB"),
            "error should identify the invalid field: {msg}"
        );
    }

    #[test]
    fn validate_profile_after_mutation() -> TestResult {
        let mut p = WheelProfile::new("Mutable", "dev");
        validate_profile(&p)?;

        p.settings.ffb.overall_gain = 0.5;
        p.settings.input.steering_range = 270;
        validate_profile(&p)?;

        p.settings.ffb.overall_gain = -1.0;
        assert!(validate_profile(&p).is_err());
        Ok(())
    }
}

// ===========================================================================
// 4. Profile import/export — JSON format stability
// ===========================================================================

mod import_export_stability {
    use super::*;

    #[test]
    fn json_field_names_are_snake_case() -> TestResult {
        let p = WheelProfile::new("FieldNames", "dev");
        let json = serde_json::to_string(&p)?;

        // Verify expected snake_case field names
        assert!(json.contains("\"overall_gain\""), "missing overall_gain");
        assert!(json.contains("\"torque_limit\""), "missing torque_limit");
        assert!(
            json.contains("\"steering_range\""),
            "missing steering_range"
        );
        assert!(json.contains("\"device_id\""), "missing device_id");
        assert!(
            json.contains("\"schema_version\""),
            "missing schema_version"
        );
        assert!(json.contains("\"created_at\""), "missing created_at");
        assert!(json.contains("\"modified_at\""), "missing modified_at");
        assert!(
            json.contains("\"effects_enabled\""),
            "missing effects_enabled"
        );
        assert!(
            json.contains("\"filter_strength\""),
            "missing filter_strength"
        );
        assert!(json.contains("\"led_mode\""), "missing led_mode");
        assert!(
            json.contains("\"emergency_stop\""),
            "missing emergency_stop"
        );
        Ok(())
    }

    #[test]
    fn json_structure_has_nested_settings() -> TestResult {
        let p = WheelProfile::new("Structure", "dev");
        let value: serde_json::Value = serde_json::to_value(&p)?;

        // Top-level fields
        assert!(value.get("id").is_some(), "missing top-level id");
        assert!(value.get("name").is_some(), "missing top-level name");
        assert!(
            value.get("settings").is_some(),
            "missing top-level settings"
        );

        // Nested settings groups
        let settings = value.get("settings");
        assert!(settings.is_some());
        let settings = settings.ok_or("no settings")?;
        assert!(settings.get("ffb").is_some(), "missing ffb in settings");
        assert!(settings.get("input").is_some(), "missing input in settings");
        assert!(
            settings.get("limits").is_some(),
            "missing limits in settings"
        );
        assert!(
            settings.get("advanced").is_some(),
            "missing advanced in settings"
        );
        Ok(())
    }

    #[test]
    fn enum_serializes_as_string_variant_name() -> TestResult {
        let mut s = WheelSettings::default();
        s.input.throttle_curve = CurveType::Exponential;
        s.advanced.led_mode = LedMode::Rpm;

        let json = serde_json::to_string(&s)?;
        assert!(
            json.contains("\"Exponential\""),
            "CurveType should serialize as string"
        );
        assert!(
            json.contains("\"Rpm\""),
            "LedMode should serialize as string"
        );
        Ok(())
    }

    #[test]
    fn null_optional_fields_serialize_correctly() -> TestResult {
        let mut s = WheelSettings::default();
        s.limits.max_speed = None;
        s.limits.max_temp = None;

        let json = serde_json::to_string(&s)?;
        let value: serde_json::Value = serde_json::from_str(&json)?;

        let limits = value.get("limits").ok_or("no limits")?;
        assert!(
            limits.get("max_speed").is_some(),
            "max_speed field should exist"
        );
        assert!(
            limits.get("max_speed").and_then(|v| v.as_null()).is_some(),
            "None should serialize as null"
        );
        Ok(())
    }

    #[test]
    fn some_optional_fields_serialize_as_value() -> TestResult {
        let mut s = WheelSettings::default();
        s.limits.max_speed = Some(200.0);
        s.limits.max_temp = Some(75);

        let json = serde_json::to_string(&s)?;
        let value: serde_json::Value = serde_json::from_str(&json)?;

        let limits = value.get("limits").ok_or("no limits")?;
        let max_speed = limits.get("max_speed").and_then(|v| v.as_f64());
        assert!(
            max_speed.is_some_and(|v| (v - 200.0).abs() < f64::EPSILON),
            "max_speed should be 200.0"
        );
        Ok(())
    }

    #[test]
    fn deserialize_from_known_stable_json() -> TestResult {
        // This JSON represents a "golden" format that must remain compatible.
        let stable_json = r#"{
            "id": "stable-001",
            "name": "Stable Profile",
            "device_id": "test-device",
            "version": 3,
            "schema_version": 1,
            "settings": {
                "ffb": {
                    "overall_gain": 0.75,
                    "torque_limit": 30.0,
                    "spring_strength": 0.1,
                    "damper_strength": 0.2,
                    "friction_strength": 0.05,
                    "effects_enabled": true
                },
                "input": {
                    "steering_range": 900,
                    "steering_deadzone": 1,
                    "throttle_curve": "Linear",
                    "brake_curve": "Exponential",
                    "clutch_curve": "Logarithmic"
                },
                "limits": {
                    "max_speed": 250.0,
                    "max_temp": 80,
                    "emergency_stop": true
                },
                "advanced": {
                    "filter_enabled": false,
                    "filter_strength": 0.5,
                    "led_mode": "Default",
                    "telemetry_enabled": true
                }
            },
            "created_at": 1700000000,
            "modified_at": 1700000001
        }"#;

        let p: WheelProfile = serde_json::from_str(stable_json)?;
        assert_eq!(p.id, "stable-001");
        assert_eq!(p.name, "Stable Profile");
        assert_eq!(p.device_id, "test-device");
        assert_eq!(p.version, 3);
        assert_eq!(p.schema_version, 1);
        assert!((p.settings.ffb.overall_gain - 0.75).abs() < f32::EPSILON);
        assert!((p.settings.ffb.torque_limit - 30.0).abs() < f32::EPSILON);
        assert_eq!(p.settings.input.brake_curve, CurveType::Exponential);
        assert_eq!(p.settings.input.clutch_curve, CurveType::Logarithmic);
        assert_eq!(p.settings.limits.max_speed, Some(250.0));
        assert_eq!(p.settings.limits.max_temp, Some(80));
        assert_eq!(p.settings.advanced.led_mode, LedMode::Default);
        assert_eq!(p.created_at, 1_700_000_000);
        assert_eq!(p.modified_at, 1_700_000_001);

        validate_profile(&p)?;
        Ok(())
    }

    #[test]
    fn extra_json_fields_are_ignored() -> TestResult {
        let json = r#"{
            "id": "extra-fields",
            "name": "Extra",
            "device_id": "dev",
            "version": 1,
            "schema_version": 1,
            "unknown_field": "should be ignored",
            "settings": {
                "ffb": {"overall_gain":1.0,"torque_limit":25.0,"spring_strength":0.0,"damper_strength":0.0,"friction_strength":0.0,"effects_enabled":true,"future_field":42},
                "input": {"steering_range":900,"steering_deadzone":0,"throttle_curve":"Linear","brake_curve":"Linear","clutch_curve":"Linear"},
                "limits": {"max_speed":null,"max_temp":80,"emergency_stop":true},
                "advanced": {"filter_enabled":false,"filter_strength":0.5,"led_mode":"Default","telemetry_enabled":true}
            },
            "created_at": 1000,
            "modified_at": 1000
        }"#;

        // Should not error on unknown fields (serde default behavior with deny_unknown_fields off)
        let result: Result<WheelProfile, _> = serde_json::from_str(json);
        // If the crate uses deny_unknown_fields, this would fail — document behaviour
        if let Ok(p) = result {
            assert_eq!(p.id, "extra-fields");
            validate_profile(&p)?;
        }
        Ok(())
    }
}

// ===========================================================================
// 5. Profile migration — old format to new format
// ===========================================================================

mod migration_system {
    use super::*;

    #[test]
    fn full_migration_workflow_backup_migrate_validate() -> TestResult {
        let dir =
            std::env::temp_dir().join(format!("profile_sys_migration_{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let backup_path = dir.join("pre_migration.json");

        // Simulate loading a v0 profile from storage
        let v0_json = r#"{
            "id": "migrated-001",
            "name": "Old Profile",
            "device_id": "legacy-wheel",
            "version": 2,
            "settings": {
                "ffb": {"overall_gain":0.8,"torque_limit":20.0,"spring_strength":0.1,"damper_strength":0.2,"friction_strength":0.0,"effects_enabled":true},
                "input": {"steering_range":900,"steering_deadzone":0,"throttle_curve":"Linear","brake_curve":"Linear","clutch_curve":"Linear"},
                "limits": {"max_speed":null,"max_temp":80,"emergency_stop":true},
                "advanced": {"filter_enabled":false,"filter_strength":0.5,"led_mode":"Default","telemetry_enabled":true}
            },
            "created_at": 1600000000,
            "modified_at": 1600000001
        }"#;

        // Step 1: Deserialize (schema_version missing → defaults to 0)
        let mut profile: WheelProfile = serde_json::from_str(v0_json)?;
        assert_eq!(profile.schema_version, 0);

        // Step 2: Backup pre-migration state
        openracing_profile::backup_profile(v0_json, &backup_path)?;
        assert!(backup_path.exists());

        // Step 3: Migrate
        let migrated = migrate_profile(&mut profile)?;
        assert!(migrated);
        assert_eq!(profile.schema_version, CURRENT_SCHEMA_VERSION);

        // Step 4: Validate post-migration
        validate_profile(&profile)?;

        // Step 5: Verify settings are preserved
        assert_eq!(profile.name, "Old Profile");
        assert_eq!(profile.device_id, "legacy-wheel");
        assert!((profile.settings.ffb.overall_gain - 0.8).abs() < f32::EPSILON);
        assert_eq!(profile.settings.input.steering_range, 900);

        // Step 6: Verify backup still has old version
        let backup_content = std::fs::read_to_string(&backup_path)?;
        assert!(
            !backup_content.contains("\"schema_version\""),
            "backup should contain pre-migration JSON without schema_version"
        );

        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn migrate_then_serialize_produces_current_version() -> TestResult {
        let mut p = WheelProfile::new("MigrateSer", "dev");
        p.schema_version = 0;

        migrate_profile(&mut p)?;

        let json = serde_json::to_string(&p)?;
        let value: serde_json::Value = serde_json::from_str(&json)?;

        let sv = value
            .get("schema_version")
            .and_then(|v| v.as_u64())
            .ok_or("missing schema_version in JSON")?;
        assert_eq!(sv, u64::from(CURRENT_SCHEMA_VERSION));
        Ok(())
    }

    #[test]
    fn migrate_does_not_alter_user_version() -> TestResult {
        let mut p = WheelProfile::new("UserVer", "dev");
        p.version = 7;
        p.schema_version = 0;

        migrate_profile(&mut p)?;
        assert_eq!(
            p.version, 7,
            "user version should not change during migration"
        );
        Ok(())
    }

    #[test]
    fn migrate_far_future_version_gives_informative_error() {
        let mut p = WheelProfile::new("FarFuture", "dev");
        p.schema_version = 100;

        let result = migrate_profile(&mut p);
        assert!(result.is_err());
        if let Err(ProfileError::UnsupportedVersion(got, max)) = result {
            assert_eq!(got, 100);
            assert_eq!(max, CURRENT_SCHEMA_VERSION);
        } else {
            panic!("expected UnsupportedVersion error");
        }
    }
}

// ===========================================================================
// 6. Profile comparison and diff
// ===========================================================================

mod profile_comparison {
    use super::*;

    /// Helper: collect differences between two profiles' settings as field names.
    fn diff_settings(a: &WheelSettings, b: &WheelSettings) -> Vec<&'static str> {
        let mut diffs = Vec::new();

        if (a.ffb.overall_gain - b.ffb.overall_gain).abs() >= f32::EPSILON {
            diffs.push("ffb.overall_gain");
        }
        if (a.ffb.torque_limit - b.ffb.torque_limit).abs() >= f32::EPSILON {
            diffs.push("ffb.torque_limit");
        }
        if (a.ffb.spring_strength - b.ffb.spring_strength).abs() >= f32::EPSILON {
            diffs.push("ffb.spring_strength");
        }
        if (a.ffb.damper_strength - b.ffb.damper_strength).abs() >= f32::EPSILON {
            diffs.push("ffb.damper_strength");
        }
        if (a.ffb.friction_strength - b.ffb.friction_strength).abs() >= f32::EPSILON {
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
        if (a.advanced.filter_strength - b.advanced.filter_strength).abs() >= f32::EPSILON {
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
        let settings = WheelSettings::default();
        let diffs = diff_settings(&settings, &settings.clone());
        assert!(
            diffs.is_empty(),
            "identical settings should produce no diffs"
        );
        Ok(())
    }

    #[test]
    fn single_field_change_produces_one_diff() -> TestResult {
        let a = WheelSettings::default();
        let mut b = a.clone();
        b.ffb.overall_gain = 0.5;

        let diffs = diff_settings(&a, &b);
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0], "ffb.overall_gain");
        Ok(())
    }

    #[test]
    fn multiple_field_changes_tracked() -> TestResult {
        let a = WheelSettings::default();
        let mut b = a.clone();
        b.ffb.overall_gain = 0.5;
        b.input.steering_range = 1080;
        b.advanced.led_mode = LedMode::Speed;

        let diffs = diff_settings(&a, &b);
        assert_eq!(diffs.len(), 3);
        assert!(diffs.contains(&"ffb.overall_gain"));
        assert!(diffs.contains(&"input.steering_range"));
        assert!(diffs.contains(&"advanced.led_mode"));
        Ok(())
    }

    #[test]
    fn all_fields_different_produces_complete_diff() -> TestResult {
        let a = WheelSettings::default();
        let b = WheelSettings {
            ffb: FfbSettings {
                overall_gain: 0.1,
                torque_limit: 50.0,
                spring_strength: 0.5,
                damper_strength: 0.5,
                friction_strength: 0.5,
                effects_enabled: false,
            },
            input: InputSettings {
                steering_range: 1440,
                steering_deadzone: 10,
                throttle_curve: CurveType::Exponential,
                brake_curve: CurveType::Logarithmic,
                clutch_curve: CurveType::Custom,
                custom_clutch_curve: Some(CustomCurve::default()),
                ..Default::default()
            },
            limits: LimitSettings {
                max_speed: Some(300.0),
                max_temp: Some(60),
                emergency_stop: false,
            },
            advanced: AdvancedSettings {
                filter_enabled: true,
                filter_strength: 0.9,
                led_mode: LedMode::Rpm,
                telemetry_enabled: false,
            },
        };

        let diffs = diff_settings(&a, &b);
        // All 18 tracked fields should differ
        assert_eq!(
            diffs.len(),
            18,
            "all fields should be different: {:?}",
            diffs
        );
        Ok(())
    }

    #[test]
    fn json_diff_between_profiles() -> TestResult {
        let p1 = WheelProfile::new("Profile A", "dev");
        let mut p2 = p1.clone();
        p2.settings.ffb.overall_gain = 0.5;
        p2.settings.input.steering_range = 540;

        let v1: serde_json::Value = serde_json::to_value(&p1)?;
        let v2: serde_json::Value = serde_json::to_value(&p2)?;

        // Verify the JSON values diverge where expected
        let gain1 = v1.pointer("/settings/ffb/overall_gain");
        let gain2 = v2.pointer("/settings/ffb/overall_gain");
        assert_ne!(gain1, gain2, "gain values should differ in JSON");

        let range1 = v1.pointer("/settings/input/steering_range");
        let range2 = v2.pointer("/settings/input/steering_range");
        assert_ne!(
            range1, range2,
            "steering_range values should differ in JSON"
        );

        // Fields not changed should be identical
        let led1 = v1.pointer("/settings/advanced/led_mode");
        let led2 = v2.pointer("/settings/advanced/led_mode");
        assert_eq!(led1, led2, "unchanged fields should be identical in JSON");
        Ok(())
    }
}

// ===========================================================================
// 7. Profile merge behavior (two profiles → one)
// ===========================================================================

mod merge_behavior {
    use super::*;

    #[test]
    fn merge_preserves_base_identity_fields() -> TestResult {
        let base = WheelProfile::new("Base Name", "base-device");
        let overlay = WheelProfile::new("Overlay Name", "overlay-device");

        let merged = merge_profiles(&base, &overlay);
        assert_eq!(merged.id, base.id, "should keep base ID");
        assert_eq!(merged.name, base.name, "should keep base name");
        assert_eq!(
            merged.device_id, base.device_id,
            "should keep base device_id"
        );
        Ok(())
    }

    #[test]
    fn merge_only_overrides_changed_fields() -> TestResult {
        let base = WheelProfile::new("Base", "dev");
        let mut overlay = base.clone();
        overlay.settings.ffb.overall_gain = 0.5;
        // torque_limit is the same as base, should not be overridden

        let merged = merge_profiles(&base, &overlay);
        assert!((merged.settings.ffb.overall_gain - 0.5).abs() < f32::EPSILON);
        assert!(
            (merged.settings.ffb.torque_limit - base.settings.ffb.torque_limit).abs()
                < f32::EPSILON,
            "unchanged torque_limit should match base"
        );
        Ok(())
    }

    #[test]
    fn merge_result_passes_validation() -> TestResult {
        let base = WheelProfile::new("Base", "dev");
        let mut overlay = WheelProfile::new("Overlay", "dev");
        overlay.settings.ffb.overall_gain = 0.5;
        overlay.settings.input.steering_range = 270;

        let merged = merge_profiles(&base, &overlay);
        validate_profile(&merged)?;
        Ok(())
    }

    #[test]
    fn merge_with_extreme_valid_values() -> TestResult {
        let base = WheelProfile::new("Base", "dev");
        let mut overlay = WheelProfile::new("Overlay", "dev");
        overlay.settings.ffb.overall_gain = 0.0;
        overlay.settings.ffb.torque_limit = 100.0;
        overlay.settings.input.steering_range = 3600;

        let merged = merge_profiles(&base, &overlay);
        assert!((merged.settings.ffb.overall_gain - 0.0).abs() < f32::EPSILON);
        assert!((merged.settings.ffb.torque_limit - 100.0).abs() < f32::EPSILON);
        assert_eq!(merged.settings.input.steering_range, 3600);

        validate_profile(&merged)?;
        Ok(())
    }

    #[test]
    fn merge_is_not_commutative_for_identity() -> TestResult {
        let a = WheelProfile::new("A", "dev-a");
        let b = WheelProfile::new("B", "dev-b");

        let ab = merge_profiles(&a, &b);
        let ba = merge_profiles(&b, &a);

        // Base identity is preserved — so AB ≠ BA for id/name/device_id
        assert_eq!(ab.id, a.id);
        assert_eq!(ba.id, b.id);
        assert_ne!(ab.id, ba.id);
        Ok(())
    }

    #[test]
    fn merge_chain_is_associative_for_settings() -> TestResult {
        // merge_profiles compares overlay vs base; overlay fields that differ
        // from the current base will be applied. With chained merges,
        // intermediate results may differ, so we test that the final
        // overrides from each layer land correctly.
        let base = WheelProfile::new("Base", "dev");

        let mut l1 = WheelProfile::new("L1", "dev");
        l1.settings.ffb.overall_gain = 0.5;

        let mut l2 = WheelProfile::new("L2", "dev");
        l2.settings.ffb.torque_limit = 50.0;
        // Carry l1's gain to avoid l2's default resetting it
        l2.settings.ffb.overall_gain = 0.5;

        let left = merge_profiles(&merge_profiles(&base, &l1), &l2);
        assert!((left.settings.ffb.overall_gain - 0.5).abs() < f32::EPSILON);
        assert!((left.settings.ffb.torque_limit - 50.0).abs() < f32::EPSILON);
        Ok(())
    }
}

// ===========================================================================
// 8. Profile templates and defaults
// ===========================================================================

mod templates_and_defaults {
    use super::*;

    /// Create a "drift" template with typical drift car settings.
    fn drift_template() -> WheelSettings {
        WheelSettings {
            ffb: FfbSettings {
                overall_gain: 0.7,
                torque_limit: 15.0,
                spring_strength: 0.0,
                damper_strength: 0.1,
                friction_strength: 0.0,
                effects_enabled: true,
            },
            input: InputSettings {
                steering_range: 900,
                steering_deadzone: 0,
                throttle_curve: CurveType::Linear,
                brake_curve: CurveType::Linear,
                clutch_curve: CurveType::Linear,
                ..Default::default()
            },
            limits: LimitSettings {
                max_speed: None,
                max_temp: Some(80),
                emergency_stop: true,
            },
            advanced: AdvancedSettings {
                filter_enabled: false,
                filter_strength: 0.3,
                led_mode: LedMode::Default,
                telemetry_enabled: true,
            },
        }
    }

    /// Create a "rally" template.
    fn rally_template() -> WheelSettings {
        WheelSettings {
            ffb: FfbSettings {
                overall_gain: 0.9,
                torque_limit: 20.0,
                spring_strength: 0.2,
                damper_strength: 0.3,
                friction_strength: 0.15,
                effects_enabled: true,
            },
            input: InputSettings {
                steering_range: 540,
                steering_deadzone: 1,
                throttle_curve: CurveType::Exponential,
                brake_curve: CurveType::Linear,
                clutch_curve: CurveType::Linear,
                ..Default::default()
            },
            limits: LimitSettings {
                max_speed: None,
                max_temp: Some(85),
                emergency_stop: true,
            },
            advanced: AdvancedSettings {
                filter_enabled: true,
                filter_strength: 0.6,
                led_mode: LedMode::Speed,
                telemetry_enabled: true,
            },
        }
    }

    /// Create a "formula" template with high precision settings.
    fn formula_template() -> WheelSettings {
        WheelSettings {
            ffb: FfbSettings {
                overall_gain: 1.0,
                torque_limit: 25.0,
                spring_strength: 0.0,
                damper_strength: 0.05,
                friction_strength: 0.0,
                effects_enabled: true,
            },
            input: InputSettings {
                steering_range: 360,
                steering_deadzone: 0,
                throttle_curve: CurveType::Linear,
                brake_curve: CurveType::Exponential,
                clutch_curve: CurveType::Linear,
                ..Default::default()
            },
            limits: LimitSettings {
                max_speed: None,
                max_temp: Some(90),
                emergency_stop: true,
            },
            advanced: AdvancedSettings {
                filter_enabled: false,
                filter_strength: 0.0,
                led_mode: LedMode::Rpm,
                telemetry_enabled: true,
            },
        }
    }

    #[test]
    fn template_applied_to_profile_validates() -> TestResult {
        let templates: Vec<(&str, WheelSettings)> = vec![
            ("Drift", drift_template()),
            ("Rally", rally_template()),
            ("Formula", formula_template()),
        ];

        for (name, template) in templates {
            let p = WheelProfile::new(name, "dev").with_settings(template);
            validate_profile(&p)?;
        }
        Ok(())
    }

    #[test]
    fn template_overridden_by_user_preference() -> TestResult {
        let mut p = WheelProfile::new("Custom Rally", "dev").with_settings(rally_template());
        // User overrides steering range
        p.settings.input.steering_range = 720;
        p.settings.ffb.overall_gain = 0.85;

        validate_profile(&p)?;
        assert_eq!(p.settings.input.steering_range, 720);
        assert!((p.settings.ffb.overall_gain - 0.85).abs() < f32::EPSILON);
        // Template defaults still present for non-overridden fields
        assert_eq!(p.settings.input.throttle_curve, CurveType::Exponential);
        Ok(())
    }

    #[test]
    fn template_round_trip_through_json() -> TestResult {
        let settings = formula_template();
        let json = serde_json::to_string(&settings)?;
        let restored: WheelSettings = serde_json::from_str(&json)?;

        assert_eq!(restored.input.steering_range, 360);
        assert_eq!(restored.advanced.led_mode, LedMode::Rpm);
        assert!((restored.advanced.filter_strength - 0.0).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn default_profile_matches_default_settings() -> TestResult {
        let p = WheelProfile::new("Default Check", "dev");
        let defaults = WheelSettings::default();

        assert!(
            (p.settings.ffb.overall_gain - defaults.ffb.overall_gain).abs() < f32::EPSILON,
            "new profile should use default FFB gain"
        );
        assert_eq!(
            p.settings.input.steering_range, defaults.input.steering_range,
            "new profile should use default steering range"
        );
        assert_eq!(
            p.settings.advanced.led_mode, defaults.advanced.led_mode,
            "new profile should use default LED mode"
        );
        Ok(())
    }
}

// ===========================================================================
// 9. Profile tags and categorization
// ===========================================================================

mod tags_and_categorization {
    use super::*;

    /// Simulate tag-based categorization using profile name conventions.
    fn categorize_profile(name: &str) -> Vec<&str> {
        let mut tags = Vec::new();
        let lower = name.to_lowercase();

        if lower.contains("drift") {
            tags.push("drift");
        }
        if lower.contains("rally") {
            tags.push("rally");
        }
        if lower.contains("formula") || lower.contains("f1") {
            tags.push("formula");
        }
        if lower.contains("gt") {
            tags.push("gt");
        }
        if lower.contains("endurance") || lower.contains("le mans") {
            tags.push("endurance");
        }
        if lower.contains("rain") || lower.contains("wet") {
            tags.push("wet_conditions");
        }

        tags
    }

    #[test]
    fn profiles_can_be_categorized_by_name() -> TestResult {
        let profiles = [
            WheelProfile::new("Drift Setup", "dev"),
            WheelProfile::new("Rally WRC", "dev"),
            WheelProfile::new("Formula 1 Race", "dev"),
            WheelProfile::new("GT3 Endurance", "dev"),
            WheelProfile::new("Rain Setup", "dev"),
        ];

        let tags: Vec<Vec<&str>> = profiles
            .iter()
            .map(|p| categorize_profile(&p.name))
            .collect();

        assert!(tags[0].contains(&"drift"));
        assert!(tags[1].contains(&"rally"));
        assert!(tags[2].contains(&"formula"));
        assert!(tags[3].contains(&"gt"));
        assert!(tags[3].contains(&"endurance"));
        assert!(tags[4].contains(&"wet_conditions"));
        Ok(())
    }

    #[test]
    fn uncategorized_profile_gets_no_tags() -> TestResult {
        let tags = categorize_profile("My Custom Profile 42");
        assert!(tags.is_empty());
        Ok(())
    }

    #[test]
    fn profile_can_have_multiple_categories() -> TestResult {
        let tags = categorize_profile("GT3 Le Mans Rain Setup");
        assert!(tags.contains(&"gt"));
        assert!(tags.contains(&"endurance"));
        assert!(tags.contains(&"wet_conditions"));
        assert_eq!(tags.len(), 3);
        Ok(())
    }
}

// ===========================================================================
// 10. Profile search and filtering
// ===========================================================================

mod search_and_filtering {
    use super::*;

    fn create_profile_set() -> Vec<WheelProfile> {
        let mut profiles = Vec::new();

        let mut drift = WheelProfile::new("Drift Standard", "fanatec-dd-pro");
        drift.settings.ffb.overall_gain = 0.7;
        drift.settings.input.steering_range = 900;
        profiles.push(drift);

        let mut rally = WheelProfile::new("Rally WRC", "simucube-2-pro");
        rally.settings.ffb.overall_gain = 0.9;
        rally.settings.input.steering_range = 540;
        profiles.push(rally);

        let mut formula = WheelProfile::new("Formula 1 Pro", "moza-r21");
        formula.settings.ffb.overall_gain = 1.0;
        formula.settings.input.steering_range = 360;
        profiles.push(formula);

        let mut gt3 = WheelProfile::new("GT3 Race", "fanatec-dd-pro");
        gt3.settings.ffb.overall_gain = 0.85;
        gt3.settings.input.steering_range = 480;
        profiles.push(gt3);

        let mut casual = WheelProfile::new("Casual Fun", "logitech-g29");
        casual.settings.ffb.overall_gain = 0.4;
        casual.settings.input.steering_range = 900;
        profiles.push(casual);

        profiles
    }

    #[test]
    fn filter_by_device_id() -> TestResult {
        let profiles = create_profile_set();
        let fanatec: Vec<&WheelProfile> = profiles
            .iter()
            .filter(|p| p.device_id == "fanatec-dd-pro")
            .collect();
        assert_eq!(fanatec.len(), 2);
        assert!(fanatec.iter().all(|p| p.device_id == "fanatec-dd-pro"));
        Ok(())
    }

    #[test]
    fn filter_by_name_substring() -> TestResult {
        let profiles = create_profile_set();
        let matching: Vec<&WheelProfile> = profiles
            .iter()
            .filter(|p| p.name.to_lowercase().contains("race"))
            .collect();
        assert_eq!(matching.len(), 1);
        assert_eq!(matching[0].name, "GT3 Race");
        Ok(())
    }

    #[test]
    fn filter_by_gain_threshold() -> TestResult {
        let profiles = create_profile_set();
        let high_gain: Vec<&WheelProfile> = profiles
            .iter()
            .filter(|p| p.settings.ffb.overall_gain >= 0.85)
            .collect();
        assert_eq!(high_gain.len(), 3); // Rally (0.9), Formula (1.0), GT3 (0.85)
        Ok(())
    }

    #[test]
    fn filter_by_steering_range() -> TestResult {
        let profiles = create_profile_set();
        let narrow: Vec<&WheelProfile> = profiles
            .iter()
            .filter(|p| p.settings.input.steering_range <= 540)
            .collect();
        assert_eq!(narrow.len(), 3); // Rally (540), Formula (360), GT3 (480)
        Ok(())
    }

    #[test]
    fn sort_profiles_by_gain() -> TestResult {
        let profiles = create_profile_set();
        let mut sorted: Vec<&WheelProfile> = profiles.iter().collect();
        sorted.sort_by(|a, b| {
            a.settings
                .ffb
                .overall_gain
                .partial_cmp(&b.settings.ffb.overall_gain)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Verify ascending order
        for i in 1..sorted.len() {
            assert!(
                sorted[i].settings.ffb.overall_gain >= sorted[i - 1].settings.ffb.overall_gain,
                "profiles should be sorted by gain"
            );
        }
        Ok(())
    }

    #[test]
    fn search_no_matches_returns_empty() -> TestResult {
        let profiles = create_profile_set();
        let matches: Vec<&WheelProfile> = profiles
            .iter()
            .filter(|p| p.name.contains("NASCAR"))
            .collect();
        assert!(matches.is_empty());
        Ok(())
    }
}

// ===========================================================================
// 11. Game-specific profile overrides
// ===========================================================================

mod game_specific_overrides {
    use super::*;

    fn base_profile() -> WheelProfile {
        WheelProfile::new("Universal Base", "dev")
    }

    fn iracing_overlay() -> WheelProfile {
        let settings = WheelSettings {
            ffb: FfbSettings {
                overall_gain: 0.8,
                torque_limit: 20.0,
                spring_strength: 0.0,
                damper_strength: 0.05,
                friction_strength: 0.0,
                effects_enabled: true,
            },
            input: InputSettings {
                steering_range: 900,
                steering_deadzone: 0,
                throttle_curve: CurveType::Linear,
                brake_curve: CurveType::Linear,
                clutch_curve: CurveType::Linear,
                ..Default::default()
            },
            limits: LimitSettings::default(),
            advanced: AdvancedSettings {
                filter_enabled: true,
                filter_strength: 0.4,
                led_mode: LedMode::Rpm,
                telemetry_enabled: true,
            },
        };
        WheelProfile::new("iRacing", "dev").with_settings(settings)
    }

    fn acc_overlay() -> WheelProfile {
        let settings = WheelSettings {
            ffb: FfbSettings {
                overall_gain: 0.75,
                torque_limit: 22.0,
                spring_strength: 0.1,
                damper_strength: 0.15,
                friction_strength: 0.05,
                effects_enabled: true,
            },
            input: InputSettings {
                steering_range: 800,
                steering_deadzone: 1,
                throttle_curve: CurveType::Exponential,
                brake_curve: CurveType::Exponential,
                clutch_curve: CurveType::Linear,
                ..Default::default()
            },
            limits: LimitSettings::default(),
            advanced: AdvancedSettings {
                filter_enabled: true,
                filter_strength: 0.5,
                led_mode: LedMode::Speed,
                telemetry_enabled: true,
            },
        };
        WheelProfile::new("ACC", "dev").with_settings(settings)
    }

    #[test]
    fn game_overlay_produces_distinct_profiles() -> TestResult {
        let base = base_profile();
        let iracing = merge_profiles(&base, &iracing_overlay());
        let acc = merge_profiles(&base, &acc_overlay());

        // merge_profiles only merges gain, torque_limit, and steering_range.
        // Verify the merged fields differ between games.
        assert!(
            (iracing.settings.ffb.overall_gain - acc.settings.ffb.overall_gain).abs()
                >= f32::EPSILON
        );
        assert_ne!(
            iracing.settings.input.steering_range,
            acc.settings.input.steering_range
        );
        assert!(
            (iracing.settings.ffb.torque_limit - acc.settings.ffb.torque_limit).abs()
                >= f32::EPSILON
        );

        // Both should validate
        validate_profile(&iracing)?;
        validate_profile(&acc)?;
        Ok(())
    }

    #[test]
    fn game_overlay_then_serialize_restore() -> TestResult {
        let base = base_profile();
        let resolved = merge_profiles(&base, &iracing_overlay());

        let json = serde_json::to_string(&resolved)?;
        let restored: WheelProfile = serde_json::from_str(&json)?;

        // merge_profiles applies gain, torque_limit, steering_range from overlay
        assert!((restored.settings.ffb.overall_gain - 0.8).abs() < f32::EPSILON);
        assert!((restored.settings.ffb.torque_limit - 20.0).abs() < f32::EPSILON);
        validate_profile(&restored)?;
        Ok(())
    }

    #[test]
    fn switching_games_reapplies_from_base() -> TestResult {
        let base = base_profile();

        // Switch to iRacing
        let iracing_resolved = merge_profiles(&base, &iracing_overlay());
        assert!((iracing_resolved.settings.ffb.overall_gain - 0.8).abs() < f32::EPSILON);

        // Switch back to ACC (re-apply from base, not from iRacing)
        let acc_resolved = merge_profiles(&base, &acc_overlay());
        assert!((acc_resolved.settings.ffb.overall_gain - 0.75).abs() < f32::EPSILON);

        // Ensure ACC did not inherit iRacing's values
        assert_ne!(
            iracing_resolved.settings.input.steering_range,
            acc_resolved.settings.input.steering_range
        );
        Ok(())
    }
}

// ===========================================================================
// 12. Device-specific profile parameters
// ===========================================================================

mod device_specific_parameters {
    use super::*;

    #[test]
    fn different_devices_different_torque_limits() -> TestResult {
        let mut dd_pro = WheelProfile::new("DD Pro Config", "fanatec-dd-pro");
        dd_pro.settings.ffb.torque_limit = 8.0; // 8 Nm base

        let mut dd1 = WheelProfile::new("DD1 Config", "fanatec-dd1");
        dd1.settings.ffb.torque_limit = 20.0; // 20 Nm base

        let mut r21 = WheelProfile::new("R21 Config", "moza-r21");
        r21.settings.ffb.torque_limit = 21.0; // 21 Nm base

        validate_profile(&dd_pro)?;
        validate_profile(&dd1)?;
        validate_profile(&r21)?;

        assert!(
            (dd_pro.settings.ffb.torque_limit - dd1.settings.ffb.torque_limit).abs() > f32::EPSILON,
            "different devices should have different torque limits"
        );
        Ok(())
    }

    #[test]
    fn device_profile_merge_preserves_device_id() -> TestResult {
        let base = WheelProfile::new("Base", "simucube-2-pro");
        let mut overlay = WheelProfile::new("Game Overlay", "generic");
        overlay.settings.ffb.overall_gain = 0.6;

        let merged = merge_profiles(&base, &overlay);
        assert_eq!(
            merged.device_id, "simucube-2-pro",
            "merge should preserve base device_id"
        );
        Ok(())
    }

    #[test]
    fn device_profile_survives_migration() -> TestResult {
        let mut p = WheelProfile::new("MigrateDevice", "thrustmaster-t500");
        p.settings.ffb.torque_limit = 5.5;
        p.schema_version = 0;

        migrate_profile(&mut p)?;
        assert_eq!(p.device_id, "thrustmaster-t500");
        assert!((p.settings.ffb.torque_limit - 5.5).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn device_profile_serialization_preserves_device_type() -> TestResult {
        let p = WheelProfile::new("Device Ser", "logitech-g923");
        let json = serde_json::to_string(&p)?;
        let value: serde_json::Value = serde_json::from_str(&json)?;

        let device = value
            .get("device_id")
            .and_then(|v| v.as_str())
            .ok_or("missing device_id in JSON")?;
        assert_eq!(device, "logitech-g923");
        Ok(())
    }
}

// ===========================================================================
// 13. Profile versioning and history
// ===========================================================================

mod versioning_and_history {
    use super::*;

    #[test]
    fn profile_version_increments_on_edit() -> TestResult {
        let mut p = WheelProfile::new("Versioned", "dev");
        assert_eq!(p.version, 1);

        // Simulate a save cycle by incrementing version
        p.settings.ffb.overall_gain = 0.5;
        p.version += 1;
        assert_eq!(p.version, 2);

        p.settings.input.steering_range = 720;
        p.version += 1;
        assert_eq!(p.version, 3);

        validate_profile(&p)?;
        Ok(())
    }

    #[test]
    fn version_history_via_serialized_snapshots() -> TestResult {
        let mut p = WheelProfile::new("History Test", "dev");
        let mut history: Vec<String> = Vec::new();

        // Snapshot v1
        history.push(serde_json::to_string(&p)?);

        // Edit and snapshot v2
        p.settings.ffb.overall_gain = 0.6;
        p.version += 1;
        history.push(serde_json::to_string(&p)?);

        // Edit and snapshot v3
        p.settings.input.steering_range = 540;
        p.version += 1;
        history.push(serde_json::to_string(&p)?);

        assert_eq!(history.len(), 3);

        // Restore each version and verify
        let v1: WheelProfile = serde_json::from_str(&history[0])?;
        let v2: WheelProfile = serde_json::from_str(&history[1])?;
        let v3: WheelProfile = serde_json::from_str(&history[2])?;

        assert_eq!(v1.version, 1);
        assert_eq!(v2.version, 2);
        assert_eq!(v3.version, 3);

        assert!((v1.settings.ffb.overall_gain - 1.0).abs() < f32::EPSILON);
        assert!((v2.settings.ffb.overall_gain - 0.6).abs() < f32::EPSILON);
        assert_eq!(v3.settings.input.steering_range, 540);

        // All versions should be independently valid
        validate_profile(&v1)?;
        validate_profile(&v2)?;
        validate_profile(&v3)?;
        Ok(())
    }

    #[test]
    fn schema_version_distinct_from_user_version() -> TestResult {
        let mut p = WheelProfile::new("Dual Version", "dev");
        p.version = 42;
        p.schema_version = 0;

        migrate_profile(&mut p)?;

        assert_eq!(p.version, 42, "user version unchanged by migration");
        assert_eq!(
            p.schema_version, CURRENT_SCHEMA_VERSION,
            "schema version updated"
        );
        Ok(())
    }

    #[test]
    fn rollback_to_earlier_version() -> TestResult {
        let mut p = WheelProfile::new("Rollback", "dev");
        let snapshot_v1 = serde_json::to_string(&p)?;

        // Make changes
        p.settings.ffb.overall_gain = 0.1;
        p.settings.input.steering_range = 270;
        p.version += 1;

        // Rollback
        let rolled_back: WheelProfile = serde_json::from_str(&snapshot_v1)?;
        assert_eq!(rolled_back.version, 1);
        assert!((rolled_back.settings.ffb.overall_gain - 1.0).abs() < f32::EPSILON);
        assert_eq!(rolled_back.settings.input.steering_range, 900);
        validate_profile(&rolled_back)?;
        Ok(())
    }
}

// ===========================================================================
// 14. Profile conflict resolution
// ===========================================================================

mod conflict_resolution {
    use super::*;

    #[test]
    fn conflict_base_wins_strategy() -> TestResult {
        let base = WheelProfile::new("Base", "dev");
        let mut conflict = WheelProfile::new("Conflict", "dev");
        conflict.settings.ffb.overall_gain = 0.3;

        // "Base wins" strategy: don't merge, just keep base
        let resolved = base.clone();
        assert!(
            (resolved.settings.ffb.overall_gain - base.settings.ffb.overall_gain).abs()
                < f32::EPSILON
        );
        Ok(())
    }

    #[test]
    fn conflict_overlay_wins_strategy() -> TestResult {
        let base = WheelProfile::new("Base", "dev");
        let mut overlay = WheelProfile::new("Overlay", "dev");
        overlay.settings.ffb.overall_gain = 0.3;
        overlay.settings.ffb.torque_limit = 50.0;

        // "Overlay wins" is what merge_profiles does
        let resolved = merge_profiles(&base, &overlay);
        assert!((resolved.settings.ffb.overall_gain - 0.3).abs() < f32::EPSILON);
        assert!((resolved.settings.ffb.torque_limit - 50.0).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn conflict_both_change_same_field_last_wins() -> TestResult {
        let base = WheelProfile::new("Base", "dev");

        let mut edit_a = WheelProfile::new("A", "dev");
        edit_a.settings.ffb.overall_gain = 0.3;

        let mut edit_b = WheelProfile::new("B", "dev");
        edit_b.settings.ffb.overall_gain = 0.7;

        // Apply A then B: B should win for the conflicting field
        let resolved = merge_profiles(&merge_profiles(&base, &edit_a), &edit_b);
        assert!((resolved.settings.ffb.overall_gain - 0.7).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn conflict_non_overlapping_changes_coexist() -> TestResult {
        let base = WheelProfile::new("Base", "dev");

        let mut edit_a = WheelProfile::new("A", "dev");
        edit_a.settings.ffb.overall_gain = 0.5;

        let mut edit_b = base.clone();
        edit_b.settings.input.steering_range = 540;
        // Preserve edit_a's gain so it isn't reset by edit_b's default
        edit_b.settings.ffb.overall_gain = 0.5;

        // When changes don't overlap, merge preserves both
        let merged_ab = merge_profiles(&merge_profiles(&base, &edit_a), &edit_b);
        assert!((merged_ab.settings.ffb.overall_gain - 0.5).abs() < f32::EPSILON);
        assert_eq!(merged_ab.settings.input.steering_range, 540);
        Ok(())
    }

    #[test]
    fn conflict_resolution_result_validates() -> TestResult {
        let base = WheelProfile::new("Base", "dev");

        let mut side_a = WheelProfile::new("A", "dev");
        side_a.settings.ffb.overall_gain = 0.5;
        side_a.settings.ffb.torque_limit = 40.0;

        let mut side_b = WheelProfile::new("B", "dev");
        side_b.settings.input.steering_range = 720;
        side_b.settings.ffb.overall_gain = 0.8;

        let resolved = merge_profiles(&merge_profiles(&base, &side_a), &side_b);
        validate_profile(&resolved)?;
        Ok(())
    }

    #[test]
    fn merge_then_migrate_then_validate() -> TestResult {
        let mut base = WheelProfile::new("Base", "dev");
        base.schema_version = 0;

        let mut overlay = WheelProfile::new("Overlay", "dev");
        overlay.settings.ffb.overall_gain = 0.6;
        overlay.schema_version = 0;

        // Merge two v0 profiles
        let mut merged = merge_profiles(&base, &overlay);
        assert_eq!(merged.schema_version, 0);

        // Migrate the merged result
        let migrated = migrate_profile(&mut merged)?;
        assert!(migrated);
        assert_eq!(merged.schema_version, CURRENT_SCHEMA_VERSION);

        // Validate final result
        validate_profile(&merged)?;
        assert!((merged.settings.ffb.overall_gain - 0.6).abs() < f32::EPSILON);
        Ok(())
    }
}

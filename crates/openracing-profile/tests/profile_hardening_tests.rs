//! Profile hardening tests
//!
//! Comprehensive tests covering profile creation, validation, serialization
//! roundtrips, schema migration, and property-based fuzzing.

use openracing_profile::{
    AdvancedSettings, CURRENT_SCHEMA_VERSION, CurveType, FfbSettings, InputSettings, LedMode,
    LimitSettings, ProfileError, WheelProfile, WheelSettings, backup_profile, generate_profile_id,
    merge_profiles, migrate_profile, validate_profile, validate_settings,
};
use proptest::prelude::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ---------------------------------------------------------------------------
// Serialization roundtrip tests
// ---------------------------------------------------------------------------

mod serialization_roundtrip {
    use super::*;

    #[test]
    fn default_profile_survives_json_roundtrip() -> TestResult {
        let original = WheelProfile::new("Roundtrip", "dev-rt");
        let json = serde_json::to_string(&original)?;
        let restored: WheelProfile = serde_json::from_str(&json)?;

        assert_eq!(restored.name, original.name);
        assert_eq!(restored.device_id, original.device_id);
        assert_eq!(restored.version, original.version);
        assert_eq!(restored.schema_version, original.schema_version);
        assert_eq!(
            restored.settings.ffb.overall_gain,
            original.settings.ffb.overall_gain
        );
        assert_eq!(
            restored.settings.input.steering_range,
            original.settings.input.steering_range
        );
        Ok(())
    }

    #[test]
    fn custom_settings_survive_json_roundtrip() -> TestResult {
        let settings = WheelSettings {
            ffb: FfbSettings {
                overall_gain: 0.75,
                torque_limit: 18.5,
                spring_strength: 0.3,
                damper_strength: 0.4,
                friction_strength: 0.2,
                effects_enabled: false,
            },
            input: InputSettings {
                steering_range: 540,
                steering_deadzone: 5,
                throttle_curve: CurveType::Exponential,
                brake_curve: CurveType::Logarithmic,
                clutch_curve: CurveType::Custom,
            },
            limits: LimitSettings {
                max_speed: Some(300.0),
                max_temp: Some(65),
                emergency_stop: false,
            },
            advanced: AdvancedSettings {
                filter_enabled: true,
                filter_strength: 0.8,
                led_mode: LedMode::Rpm,
                telemetry_enabled: false,
            },
        };
        let original = WheelProfile::new("Custom", "dev-custom").with_settings(settings);
        let json = serde_json::to_string_pretty(&original)?;
        let restored: WheelProfile = serde_json::from_str(&json)?;

        assert_eq!(restored.settings.ffb.torque_limit, 18.5);
        assert_eq!(restored.settings.input.steering_range, 540);
        assert_eq!(
            restored.settings.input.throttle_curve,
            CurveType::Exponential
        );
        assert_eq!(restored.settings.input.brake_curve, CurveType::Logarithmic);
        assert_eq!(restored.settings.limits.max_speed, Some(300.0));
        assert_eq!(restored.settings.advanced.led_mode, LedMode::Rpm);
        assert!(!restored.settings.advanced.telemetry_enabled);
        Ok(())
    }

    #[test]
    fn schema_version_zero_deserializes_via_serde_default() -> TestResult {
        // JSON without "schema_version" should deserialize to 0 via #[serde(default)]
        let json = r#"{
            "id": "test-id",
            "name": "Legacy",
            "device_id": "dev-1",
            "version": 1,
            "settings": {
                "ffb": {
                    "overall_gain": 1.0,
                    "torque_limit": 25.0,
                    "spring_strength": 0.0,
                    "damper_strength": 0.0,
                    "friction_strength": 0.0,
                    "effects_enabled": true
                },
                "input": {
                    "steering_range": 900,
                    "steering_deadzone": 0,
                    "throttle_curve": "Linear",
                    "brake_curve": "Linear",
                    "clutch_curve": "Linear"
                },
                "limits": {
                    "max_speed": null,
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
            "modified_at": 1700000000
        }"#;

        let profile: WheelProfile = serde_json::from_str(json)?;
        assert_eq!(profile.schema_version, 0, "missing field must default to 0");
        Ok(())
    }

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
            assert_eq!(
                restored, curve,
                "CurveType roundtrip failed for {:?}",
                curve
            );
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
            assert_eq!(restored, mode, "LedMode roundtrip failed for {:?}", mode);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Schema validation tests
// ---------------------------------------------------------------------------

mod schema_validation {
    use super::*;

    #[test]
    fn valid_default_profile_passes_validation() -> TestResult {
        let profile = WheelProfile::new("Valid", "dev-1");
        validate_profile(&profile)?;
        Ok(())
    }

    #[test]
    fn empty_name_fails_validation() {
        let mut profile = WheelProfile::new("X", "dev-1");
        profile.name = String::new();
        let result = validate_profile(&profile);
        assert!(result.is_err());
    }

    #[test]
    fn empty_device_id_fails_validation() {
        let mut profile = WheelProfile::new("X", "dev-1");
        profile.device_id = String::new();
        let result = validate_profile(&profile);
        assert!(result.is_err());
    }

    #[test]
    fn gain_below_zero_fails() {
        let mut settings = WheelSettings::default();
        settings.ffb.overall_gain = -0.01;
        assert!(validate_settings(&settings).is_err());
    }

    #[test]
    fn gain_above_one_fails() {
        let mut settings = WheelSettings::default();
        settings.ffb.overall_gain = 1.01;
        assert!(validate_settings(&settings).is_err());
    }

    #[test]
    fn gain_boundary_zero_passes() -> TestResult {
        let mut settings = WheelSettings::default();
        settings.ffb.overall_gain = 0.0;
        validate_settings(&settings)?;
        Ok(())
    }

    #[test]
    fn gain_boundary_one_passes() -> TestResult {
        let mut settings = WheelSettings::default();
        settings.ffb.overall_gain = 1.0;
        validate_settings(&settings)?;
        Ok(())
    }

    #[test]
    fn torque_limit_out_of_range_fails() {
        let mut settings = WheelSettings::default();
        settings.ffb.torque_limit = 101.0;
        assert!(validate_settings(&settings).is_err());

        settings.ffb.torque_limit = -1.0;
        assert!(validate_settings(&settings).is_err());
    }

    #[test]
    fn steering_range_below_minimum_fails() {
        let mut settings = WheelSettings::default();
        settings.input.steering_range = 89;
        assert!(validate_settings(&settings).is_err());
    }

    #[test]
    fn steering_range_above_maximum_fails() {
        let mut settings = WheelSettings::default();
        settings.input.steering_range = 3601;
        assert!(validate_settings(&settings).is_err());
    }

    #[test]
    fn steering_range_boundaries_pass() -> TestResult {
        let mut settings = WheelSettings::default();
        settings.input.steering_range = 90;
        validate_settings(&settings)?;

        settings.input.steering_range = 3600;
        validate_settings(&settings)?;
        Ok(())
    }

    #[test]
    fn filter_strength_out_of_range_fails() {
        let mut settings = WheelSettings::default();
        settings.advanced.filter_strength = 1.01;
        assert!(validate_settings(&settings).is_err());

        settings.advanced.filter_strength = -0.01;
        assert!(validate_settings(&settings).is_err());
    }
}

// ---------------------------------------------------------------------------
// Migration tests
// ---------------------------------------------------------------------------

mod migration {
    use super::*;

    #[test]
    fn migrate_from_v0_bumps_to_current() -> TestResult {
        let mut profile = WheelProfile::new("Migrate", "dev-1");
        profile.schema_version = 0;

        let migrated = migrate_profile(&mut profile)?;
        assert!(migrated);
        assert_eq!(profile.schema_version, CURRENT_SCHEMA_VERSION);
        Ok(())
    }

    #[test]
    fn migrate_current_is_noop() -> TestResult {
        let mut profile = WheelProfile::new("Already", "dev-1");
        assert_eq!(profile.schema_version, CURRENT_SCHEMA_VERSION);

        let migrated = migrate_profile(&mut profile)?;
        assert!(!migrated);
        Ok(())
    }

    #[test]
    fn migrate_future_version_returns_error() {
        let mut profile = WheelProfile::new("Future", "dev-1");
        profile.schema_version = CURRENT_SCHEMA_VERSION + 99;

        let result = migrate_profile(&mut profile);
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(ProfileError::UnsupportedVersion(_, _))
        ));
    }

    #[test]
    fn migrate_preserves_profile_data() -> TestResult {
        let mut profile = WheelProfile::new("Data Preserved", "dev-99");
        profile.schema_version = 0;
        profile.settings.ffb.overall_gain = 0.42;
        profile.settings.input.steering_range = 1080;

        migrate_profile(&mut profile)?;

        assert_eq!(profile.name, "Data Preserved");
        assert_eq!(profile.device_id, "dev-99");
        assert_eq!(profile.settings.ffb.overall_gain, 0.42);
        assert_eq!(profile.settings.input.steering_range, 1080);
        Ok(())
    }

    #[test]
    fn migrate_is_idempotent() -> TestResult {
        let mut profile = WheelProfile::new("Idempotent", "dev-1");
        profile.schema_version = 0;

        migrate_profile(&mut profile)?;
        assert_eq!(profile.schema_version, CURRENT_SCHEMA_VERSION);

        let migrated_again = migrate_profile(&mut profile)?;
        assert!(!migrated_again, "second migration must be a no-op");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Merge tests
// ---------------------------------------------------------------------------

mod merge {
    use super::*;

    #[test]
    fn merge_inherits_changed_gain() {
        let base = WheelProfile::new("Base", "dev-1");
        let mut overlay = WheelProfile::new("Overlay", "dev-1");
        overlay.settings.ffb.overall_gain = 0.3;

        let merged = merge_profiles(&base, &overlay);
        assert_eq!(merged.settings.ffb.overall_gain, 0.3);
    }

    #[test]
    fn merge_inherits_changed_torque_limit() {
        let base = WheelProfile::new("Base", "dev-1");
        let mut overlay = WheelProfile::new("Overlay", "dev-1");
        overlay.settings.ffb.torque_limit = 50.0;

        let merged = merge_profiles(&base, &overlay);
        assert_eq!(merged.settings.ffb.torque_limit, 50.0);
    }

    #[test]
    fn merge_inherits_changed_steering_range() {
        let base = WheelProfile::new("Base", "dev-1");
        let mut overlay = WheelProfile::new("Overlay", "dev-1");
        overlay.settings.input.steering_range = 270;

        let merged = merge_profiles(&base, &overlay);
        assert_eq!(merged.settings.input.steering_range, 270);
    }

    #[test]
    fn merge_preserves_base_id() {
        let base = WheelProfile::new("Base", "dev-1");
        let overlay = WheelProfile::new("Overlay", "dev-1");
        let merged = merge_profiles(&base, &overlay);

        assert_eq!(merged.id, base.id, "merged profile must keep the base ID");
    }

    #[test]
    fn merge_unchanged_fields_stay_as_base() {
        let base = WheelProfile::new("Base", "dev-1");
        let overlay = WheelProfile::new("Overlay", "dev-1");

        let merged = merge_profiles(&base, &overlay);
        // When overlay has the same default values, base values remain
        assert_eq!(
            merged.settings.ffb.overall_gain,
            base.settings.ffb.overall_gain
        );
    }
}

// ---------------------------------------------------------------------------
// Backup tests
// ---------------------------------------------------------------------------

mod backup {
    use super::*;

    #[test]
    fn backup_creates_file_with_content() -> TestResult {
        let dir =
            std::env::temp_dir().join(format!("profile_hardening_backup_{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("profile.bak");

        let payload = r#"{"id":"bak-test","version":1}"#;
        backup_profile(payload, &path)?;

        assert!(path.exists());
        let content = std::fs::read_to_string(&path)?;
        assert_eq!(content, payload);

        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn backup_to_nonexistent_dir_fails() {
        let path = std::path::PathBuf::from("/nonexistent_8dfa2b/dir/profile.bak");
        let result = backup_profile("{}", &path);
        assert!(result.is_err());
    }
}

// ---------------------------------------------------------------------------
// Profile ID generation tests
// ---------------------------------------------------------------------------

mod id_generation {
    use super::*;

    #[test]
    fn generated_ids_are_valid_uuid_v4() -> TestResult {
        for _ in 0..20 {
            let id = generate_profile_id();
            uuid::Uuid::parse_str(&id)?;
        }
        Ok(())
    }

    #[test]
    fn generated_ids_are_unique() {
        let ids: Vec<String> = (0..100).map(|_| generate_profile_id()).collect();
        let unique: std::collections::HashSet<&String> = ids.iter().collect();
        assert_eq!(unique.len(), ids.len(), "all generated IDs must be unique");
    }
}

// ---------------------------------------------------------------------------
// Proptest: fuzz profile fields
// ---------------------------------------------------------------------------

mod property_tests {
    use super::*;

    fn arb_gain() -> impl Strategy<Value = f32> {
        0.0f32..=1.0
    }

    fn arb_torque_limit() -> impl Strategy<Value = f32> {
        0.0f32..=100.0
    }

    fn arb_steering_range() -> impl Strategy<Value = u16> {
        90u16..=3600
    }

    fn arb_filter_strength() -> impl Strategy<Value = f32> {
        0.0f32..=1.0
    }

    proptest! {
        #[test]
        fn valid_settings_always_pass_validation(
            gain in arb_gain(),
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
            prop_assert!(validate_settings(&settings).is_ok());
        }

        #[test]
        fn profile_roundtrip_preserves_name(name in "[a-zA-Z0-9 _-]{1,64}") {
            let profile = WheelProfile::new(&name, "dev-1");
            let json = serde_json::to_string(&profile).map_err(|e| {
                TestCaseError::fail(format!("serialization failed: {e}"))
            })?;
            let restored: WheelProfile = serde_json::from_str(&json).map_err(|e| {
                TestCaseError::fail(format!("deserialization failed: {e}"))
            })?;
            prop_assert_eq!(restored.name, name);
        }

        #[test]
        fn out_of_range_gain_always_fails(gain in prop::num::f32::ANY) {
            prop_assume!(!(0.0..=1.0).contains(&gain));
            // Exclude NaN which can pass comparisons unpredictably
            prop_assume!(!gain.is_nan());
            let mut settings = WheelSettings::default();
            settings.ffb.overall_gain = gain;
            prop_assert!(validate_settings(&settings).is_err());
        }
    }
}

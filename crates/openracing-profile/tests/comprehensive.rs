//! Comprehensive integration tests for openracing-profile
//!
//! Tests profile creation, serialization/deserialization, validation,
//! default values, and profile merging/inheritance.

use openracing_profile::{
    AdvancedSettings, CURRENT_SCHEMA_VERSION, CurveType, FfbSettings, InputSettings, LedMode,
    LimitSettings, ProfileError, WheelProfile, WheelSettings, generate_profile_id, merge_profiles,
    migrate_profile, validate_profile, validate_settings,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ---------------------------------------------------------------------------
// Profile creation
// ---------------------------------------------------------------------------

mod creation_tests {
    use super::*;

    #[test]
    fn new_profile_has_unique_id() -> TestResult {
        let p1 = WheelProfile::new("A", "dev-1");
        let p2 = WheelProfile::new("B", "dev-2");
        assert_ne!(p1.id, p2.id, "two profiles must have distinct ids");
        Ok(())
    }

    #[test]
    fn new_profile_stores_name_and_device() -> TestResult {
        let p = WheelProfile::new("My Profile", "device-42");
        assert_eq!(p.name, "My Profile");
        assert_eq!(p.device_id, "device-42");
        Ok(())
    }

    #[test]
    fn new_profile_version_is_one() -> TestResult {
        let p = WheelProfile::new("V", "d");
        assert_eq!(p.version, 1);
        Ok(())
    }

    #[test]
    fn new_profile_has_current_schema_version() -> TestResult {
        let p = WheelProfile::new("V", "d");
        assert_eq!(p.schema_version, CURRENT_SCHEMA_VERSION);
        Ok(())
    }

    #[test]
    fn new_profile_timestamps_are_nonzero() -> TestResult {
        let p = WheelProfile::new("T", "d");
        assert!(p.created_at > 0, "created_at should be set");
        assert!(p.modified_at > 0, "modified_at should be set");
        Ok(())
    }

    #[test]
    fn with_settings_replaces_defaults() -> TestResult {
        let mut settings = WheelSettings::default();
        settings.ffb.overall_gain = 0.42;
        let p = WheelProfile::new("S", "d").with_settings(settings);
        assert!((p.settings.ffb.overall_gain - 0.42).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn generate_profile_id_returns_valid_uuid() -> TestResult {
        let id = generate_profile_id();
        uuid::Uuid::parse_str(&id)?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Default values
// ---------------------------------------------------------------------------

mod default_value_tests {
    use super::*;

    #[test]
    fn ffb_settings_defaults() -> TestResult {
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
    fn input_settings_defaults() -> TestResult {
        let input = InputSettings::default();
        assert_eq!(input.steering_range, 900);
        assert_eq!(input.steering_deadzone, 0);
        assert_eq!(input.throttle_curve, CurveType::Linear);
        assert_eq!(input.brake_curve, CurveType::Linear);
        assert_eq!(input.clutch_curve, CurveType::Linear);
        Ok(())
    }

    #[test]
    fn limit_settings_defaults() -> TestResult {
        let limits = LimitSettings::default();
        assert!(limits.max_speed.is_none());
        assert_eq!(limits.max_temp, Some(80));
        assert!(limits.emergency_stop);
        Ok(())
    }

    #[test]
    fn advanced_settings_defaults() -> TestResult {
        let adv = AdvancedSettings::default();
        assert!(!adv.filter_enabled);
        assert!((adv.filter_strength - 0.5).abs() < f32::EPSILON);
        assert_eq!(adv.led_mode, LedMode::Default);
        assert!(adv.telemetry_enabled);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Serialization / deserialization round-trip
// ---------------------------------------------------------------------------

mod serde_tests {
    use super::*;

    #[test]
    fn profile_json_round_trip() -> TestResult {
        let original = WheelProfile::new("RoundTrip", "dev-rt");
        let json = serde_json::to_string(&original)?;
        let restored: WheelProfile = serde_json::from_str(&json)?;

        assert_eq!(original.id, restored.id);
        assert_eq!(original.name, restored.name);
        assert_eq!(original.device_id, restored.device_id);
        assert_eq!(original.version, restored.version);
        assert_eq!(original.schema_version, restored.schema_version);
        Ok(())
    }

    #[test]
    fn settings_json_round_trip() -> TestResult {
        let mut settings = WheelSettings::default();
        settings.ffb.overall_gain = 0.75;
        settings.input.steering_range = 1080;
        settings.input.throttle_curve = CurveType::Exponential;
        settings.advanced.led_mode = LedMode::Rpm;

        let json = serde_json::to_string(&settings)?;
        let restored: WheelSettings = serde_json::from_str(&json)?;

        assert!((restored.ffb.overall_gain - 0.75).abs() < f32::EPSILON);
        assert_eq!(restored.input.steering_range, 1080);
        assert_eq!(restored.input.throttle_curve, CurveType::Exponential);
        assert_eq!(restored.advanced.led_mode, LedMode::Rpm);
        Ok(())
    }

    #[test]
    fn profile_with_custom_settings_round_trip() -> TestResult {
        let mut settings = WheelSettings::default();
        settings.ffb.torque_limit = 50.0;
        settings.ffb.effects_enabled = false;
        settings.limits.emergency_stop = false;
        settings.limits.max_speed = Some(200.0);

        let profile = WheelProfile::new("Custom", "dev-c").with_settings(settings);
        let json = serde_json::to_string(&profile)?;
        let restored: WheelProfile = serde_json::from_str(&json)?;

        assert!((restored.settings.ffb.torque_limit - 50.0).abs() < f32::EPSILON);
        assert!(!restored.settings.ffb.effects_enabled);
        assert!(!restored.settings.limits.emergency_stop);
        assert_eq!(restored.settings.limits.max_speed, Some(200.0));
        Ok(())
    }

    #[test]
    fn missing_schema_version_defaults_to_zero() -> TestResult {
        // Simulate a legacy JSON payload without schema_version
        let json = r#"{
            "id": "legacy-id",
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
        let restored: WheelProfile = serde_json::from_str(json)?;
        assert_eq!(
            restored.schema_version, 0,
            "missing field should default to 0"
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

mod validation_tests {
    use super::*;

    #[test]
    fn valid_default_profile_passes() -> TestResult {
        let p = WheelProfile::new("Valid", "dev");
        validate_profile(&p)?;
        Ok(())
    }

    #[test]
    fn empty_name_fails_validation() {
        let mut p = WheelProfile::new("x", "dev");
        p.name = String::new();
        let result = validate_profile(&p);
        assert!(result.is_err());
        assert!(matches!(result, Err(ProfileError::ValidationError(_))));
    }

    #[test]
    fn empty_device_id_fails_validation() {
        let mut p = WheelProfile::new("x", "dev");
        p.device_id = String::new();
        let result = validate_profile(&p);
        assert!(result.is_err());
    }

    #[test]
    fn gain_below_zero_fails() {
        let mut s = WheelSettings::default();
        s.ffb.overall_gain = -0.1;
        assert!(validate_settings(&s).is_err());
    }

    #[test]
    fn gain_above_one_fails() {
        let mut s = WheelSettings::default();
        s.ffb.overall_gain = 1.01;
        assert!(validate_settings(&s).is_err());
    }

    #[test]
    fn gain_at_boundaries_passes() -> TestResult {
        let mut s = WheelSettings::default();

        s.ffb.overall_gain = 0.0;
        validate_settings(&s)?;

        s.ffb.overall_gain = 1.0;
        validate_settings(&s)?;
        Ok(())
    }

    #[test]
    fn torque_limit_out_of_range_fails() {
        let mut s = WheelSettings::default();
        s.ffb.torque_limit = -1.0;
        assert!(validate_settings(&s).is_err());

        s.ffb.torque_limit = 101.0;
        assert!(validate_settings(&s).is_err());
    }

    #[test]
    fn torque_limit_at_boundaries_passes() -> TestResult {
        let mut s = WheelSettings::default();
        s.ffb.torque_limit = 0.0;
        validate_settings(&s)?;
        s.ffb.torque_limit = 100.0;
        validate_settings(&s)?;
        Ok(())
    }

    #[test]
    fn steering_range_below_min_fails() {
        let mut s = WheelSettings::default();
        s.input.steering_range = 89;
        assert!(validate_settings(&s).is_err());
    }

    #[test]
    fn steering_range_above_max_fails() {
        let mut s = WheelSettings::default();
        s.input.steering_range = 3601;
        assert!(validate_settings(&s).is_err());
    }

    #[test]
    fn steering_range_at_boundaries_passes() -> TestResult {
        let mut s = WheelSettings::default();
        s.input.steering_range = 90;
        validate_settings(&s)?;
        s.input.steering_range = 3600;
        validate_settings(&s)?;
        Ok(())
    }

    #[test]
    fn filter_strength_out_of_range_fails() {
        let mut s = WheelSettings::default();
        s.advanced.filter_strength = -0.01;
        assert!(validate_settings(&s).is_err());

        s.advanced.filter_strength = 1.01;
        assert!(validate_settings(&s).is_err());
    }

    #[test]
    fn filter_strength_at_boundaries_passes() -> TestResult {
        let mut s = WheelSettings::default();
        s.advanced.filter_strength = 0.0;
        validate_settings(&s)?;
        s.advanced.filter_strength = 1.0;
        validate_settings(&s)?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Migration
// ---------------------------------------------------------------------------

mod migration_tests {
    use super::*;

    #[test]
    fn migrate_v0_to_current() -> TestResult {
        let mut p = WheelProfile::new("M", "d");
        p.schema_version = 0;
        let migrated = migrate_profile(&mut p)?;
        assert!(migrated);
        assert_eq!(p.schema_version, CURRENT_SCHEMA_VERSION);
        Ok(())
    }

    #[test]
    fn migrate_current_is_noop() -> TestResult {
        let mut p = WheelProfile::new("M", "d");
        let migrated = migrate_profile(&mut p)?;
        assert!(!migrated);
        Ok(())
    }

    #[test]
    fn migrate_future_version_errors() {
        let mut p = WheelProfile::new("M", "d");
        p.schema_version = CURRENT_SCHEMA_VERSION + 1;
        let result = migrate_profile(&mut p);
        assert!(matches!(
            result,
            Err(ProfileError::UnsupportedVersion(_, _))
        ));
    }

    #[test]
    fn migrate_is_idempotent() -> TestResult {
        let mut p = WheelProfile::new("M", "d");
        p.schema_version = 0;
        migrate_profile(&mut p)?;
        let second = migrate_profile(&mut p)?;
        assert!(!second, "second migration should be a no-op");
        assert_eq!(p.schema_version, CURRENT_SCHEMA_VERSION);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Merging / inheritance
// ---------------------------------------------------------------------------

mod merge_tests {
    use super::*;

    #[test]
    fn merge_overlay_gain_overrides_base() -> TestResult {
        let base = WheelProfile::new("Base", "dev");
        let mut overlay = WheelProfile::new("Overlay", "dev");
        overlay.settings.ffb.overall_gain = 0.3;

        let merged = merge_profiles(&base, &overlay);
        assert!((merged.settings.ffb.overall_gain - 0.3).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn merge_overlay_torque_limit_overrides_base() -> TestResult {
        let base = WheelProfile::new("Base", "dev");
        let mut overlay = WheelProfile::new("Overlay", "dev");
        overlay.settings.ffb.torque_limit = 42.0;

        let merged = merge_profiles(&base, &overlay);
        assert!((merged.settings.ffb.torque_limit - 42.0).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn merge_overlay_steering_range_overrides_base() -> TestResult {
        let base = WheelProfile::new("Base", "dev");
        let mut overlay = WheelProfile::new("Overlay", "dev");
        overlay.settings.input.steering_range = 1080;

        let merged = merge_profiles(&base, &overlay);
        assert_eq!(merged.settings.input.steering_range, 1080);
        Ok(())
    }

    #[test]
    fn merge_preserves_base_id() -> TestResult {
        let base = WheelProfile::new("Base", "dev");
        let overlay = WheelProfile::new("Overlay", "dev");
        let merged = merge_profiles(&base, &overlay);
        assert_eq!(merged.id, base.id, "merged profile should keep base id");
        Ok(())
    }

    #[test]
    fn merge_same_profiles_keeps_base_values() -> TestResult {
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
    fn merge_updates_modified_at() -> TestResult {
        let base = WheelProfile::new("Base", "dev");
        let overlay = WheelProfile::new("Overlay", "dev");
        let merged = merge_profiles(&base, &overlay);
        assert!(
            merged.modified_at >= base.modified_at,
            "merged modified_at should be >= base"
        );
        Ok(())
    }
}

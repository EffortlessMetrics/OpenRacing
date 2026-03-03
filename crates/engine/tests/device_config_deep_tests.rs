//! Deep tests for per-device configuration, persistence, validation,
//! defaults, and migration.
//!
//! Covers force feedback strength and deadzone config, profile save/load
//! round-trips, validation of invalid configs, default configs per device
//! type, schema version migration, and inheritance chains.

use racing_wheel_schemas::prelude::*;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_profile_id(name: &str) -> Result<ProfileId, Box<dyn std::error::Error>> {
    Ok(name.parse::<ProfileId>()?)
}

fn make_profile(name: &str) -> Result<Profile, Box<dyn std::error::Error>> {
    let id = make_profile_id(name)?;
    Ok(Profile {
        id,
        parent: None,
        scope: ProfileScope::global(),
        base_settings: BaseSettings::default(),
        led_config: None,
        haptics_config: None,
        metadata: ProfileMetadata {
            name: name.to_string(),
            description: Some(format!("Test profile: {name}")),
            author: Some("test".to_string()),
            version: "1.0".to_string(),
            created_at: "2025-01-01T00:00:00Z".to_string(),
            modified_at: "2025-01-01T00:00:00Z".to_string(),
            tags: vec!["test".to_string()],
        },
    })
}

fn make_profile_with_settings(
    name: &str,
    gain: f32,
    dor: f32,
    torque_cap: f32,
) -> Result<Profile, Box<dyn std::error::Error>> {
    let mut profile = make_profile(name)?;
    profile.base_settings.ffb_gain = Gain::new(gain)?;
    profile.base_settings.degrees_of_rotation = Degrees::new_dor(dor)?;
    profile.base_settings.torque_cap = TorqueNm::new(torque_cap)?;
    Ok(profile)
}

// ===================================================================
// 1. Per-device configuration (force feedback strength, deadzone, etc.)
// ===================================================================

#[test]
fn default_base_settings_have_sane_values() -> Result<(), Box<dyn std::error::Error>> {
    let settings = BaseSettings::default();
    assert!((settings.ffb_gain.value() - 0.7).abs() < f32::EPSILON);
    assert!((settings.degrees_of_rotation.value() - 900.0).abs() < f32::EPSILON);
    assert!((settings.torque_cap.value() - 15.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn custom_ffb_gain_applied_to_profile() -> Result<(), Box<dyn std::error::Error>> {
    let profile = make_profile_with_settings("custom-gain", 0.5, 900.0, 10.0)?;
    assert!((profile.base_settings.ffb_gain.value() - 0.5).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn custom_dor_applied_to_profile() -> Result<(), Box<dyn std::error::Error>> {
    let profile = make_profile_with_settings("custom-dor", 0.7, 540.0, 15.0)?;
    assert!((profile.base_settings.degrees_of_rotation.value() - 540.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn filter_config_defaults_are_stable() -> Result<(), Box<dyn std::error::Error>> {
    let filters = FilterConfig::default();
    assert_eq!(filters.reconstruction, 0);
    assert!((filters.friction.value()).abs() < f32::EPSILON);
    assert!((filters.damper.value()).abs() < f32::EPSILON);
    assert!((filters.inertia.value()).abs() < f32::EPSILON);
    assert!((filters.slew_rate.value() - 1.0).abs() < f32::EPSILON);
    assert!((filters.torque_cap.value() - 1.0).abs() < f32::EPSILON);
    assert!(filters.notch_filters.is_empty());
    assert_eq!(filters.curve_points.len(), 2);
    Ok(())
}

#[test]
fn bumpstop_config_defaults() -> Result<(), Box<dyn std::error::Error>> {
    let bumpstop = BumpstopConfig::default();
    assert!(bumpstop.enabled);
    assert!((bumpstop.start_angle - 450.0).abs() < f32::EPSILON);
    assert!((bumpstop.max_angle - 540.0).abs() < f32::EPSILON);
    assert!((bumpstop.stiffness - 0.8).abs() < f32::EPSILON);
    assert!((bumpstop.damping - 0.3).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn hands_off_config_defaults() -> Result<(), Box<dyn std::error::Error>> {
    let hands_off = HandsOffConfig::default();
    assert!(hands_off.enabled);
    assert!((hands_off.threshold - 0.05).abs() < f32::EPSILON);
    assert!((hands_off.timeout_seconds - 5.0).abs() < f32::EPSILON);
    Ok(())
}

// ===================================================================
// 2. Configuration persistence (save / load via InMemoryProfileStore)
// ===================================================================

#[test]
fn store_and_retrieve_profile() -> Result<(), Box<dyn std::error::Error>> {
    let mut store = InMemoryProfileStore::new();
    let profile = make_profile("persist-test")?;
    let id = profile.id.clone();

    store.add(profile);
    let retrieved = store.get(&id).ok_or("profile not found after add")?;
    assert_eq!(retrieved.id, id);
    assert!((retrieved.base_settings.ffb_gain.value() - 0.7).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn store_multiple_profiles_and_retrieve_each() -> Result<(), Box<dyn std::error::Error>> {
    let mut store = InMemoryProfileStore::new();

    for i in 0..5 {
        let name = format!("multi-{i}");
        let profile = make_profile_with_settings(
            &name,
            (i as f32 + 1.0) / 10.0,
            900.0,
            (i as f32 + 1.0) * 2.0,
        )?;
        store.add(profile);
    }

    assert_eq!(store.len(), 5);

    for i in 0..5 {
        let name = format!("multi-{i}");
        let id = make_profile_id(&name)?;
        let profile = store.get(&id).ok_or("missing profile")?;
        let expected_gain = (i as f32 + 1.0) / 10.0;
        assert!(
            (profile.base_settings.ffb_gain.value() - expected_gain).abs() < f32::EPSILON,
            "gain mismatch for profile {name}"
        );
    }
    Ok(())
}

#[test]
fn remove_profile_from_store() -> Result<(), Box<dyn std::error::Error>> {
    let mut store = InMemoryProfileStore::new();
    let profile = make_profile("remove-me")?;
    let id = profile.id.clone();

    store.add(profile);
    assert_eq!(store.len(), 1);

    let removed = store.remove(&id);
    assert!(removed.is_some());
    assert_eq!(store.len(), 0);
    assert!(store.get(&id).is_none());
    Ok(())
}

#[test]
fn update_profile_overwrites_existing() -> Result<(), Box<dyn std::error::Error>> {
    let mut store = InMemoryProfileStore::new();
    let profile = make_profile_with_settings("update-me", 0.5, 900.0, 10.0)?;
    let id = profile.id.clone();
    store.add(profile);

    // Update with new settings
    let updated = make_profile_with_settings("update-me", 0.9, 540.0, 25.0)?;
    store.update(updated);

    let retrieved = store.get(&id).ok_or("profile not found after update")?;
    assert!((retrieved.base_settings.ffb_gain.value() - 0.9).abs() < f32::EPSILON);
    assert!((retrieved.base_settings.degrees_of_rotation.value() - 540.0).abs() < f32::EPSILON);
    assert!((retrieved.base_settings.torque_cap.value() - 25.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn profile_round_trip_via_json() -> Result<(), Box<dyn std::error::Error>> {
    let profile = make_profile_with_settings("json-roundtrip", 0.8, 720.0, 20.0)?;
    let json = serde_json::to_string_pretty(&profile)?;
    let restored: Profile = serde_json::from_str(&json)?;

    assert_eq!(restored.id, profile.id);
    assert!(
        (restored.base_settings.ffb_gain.value() - profile.base_settings.ffb_gain.value()).abs()
            < f32::EPSILON
    );
    assert!(
        (restored.base_settings.degrees_of_rotation.value()
            - profile.base_settings.degrees_of_rotation.value())
        .abs()
            < f32::EPSILON
    );
    assert!(
        (restored.base_settings.torque_cap.value() - profile.base_settings.torque_cap.value())
            .abs()
            < f32::EPSILON
    );
    Ok(())
}

// ===================================================================
// 3. Configuration validation
// ===================================================================

#[test]
fn gain_rejects_out_of_range_values() {
    assert!(Gain::new(-0.1).is_err());
    assert!(Gain::new(1.1).is_err());
    assert!(Gain::new(f32::NAN).is_err());
    assert!(Gain::new(f32::INFINITY).is_err());
}

#[test]
fn gain_accepts_boundary_values() -> Result<(), Box<dyn std::error::Error>> {
    let zero = Gain::new(0.0)?;
    assert!((zero.value()).abs() < f32::EPSILON);

    let full = Gain::new(1.0)?;
    assert!((full.value() - 1.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn torque_nm_rejects_negative() {
    assert!(TorqueNm::new(-1.0).is_err());
}

#[test]
fn torque_nm_rejects_over_max() {
    assert!(TorqueNm::new(TorqueNm::MAX_TORQUE + 1.0).is_err());
}

#[test]
fn torque_nm_accepts_boundary_values() -> Result<(), Box<dyn std::error::Error>> {
    let zero = TorqueNm::new(0.0)?;
    assert!((zero.value()).abs() < f32::EPSILON);

    let max = TorqueNm::new(TorqueNm::MAX_TORQUE)?;
    assert!((max.value() - TorqueNm::MAX_TORQUE).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn degrees_of_rotation_rejects_below_minimum() {
    assert!(Degrees::new_dor(179.0).is_err());
    assert!(Degrees::new_dor(0.0).is_err());
    assert!(Degrees::new_dor(-1.0).is_err());
}

#[test]
fn degrees_of_rotation_rejects_above_maximum() {
    assert!(Degrees::new_dor(2161.0).is_err());
    assert!(Degrees::new_dor(f32::INFINITY).is_err());
}

#[test]
fn degrees_of_rotation_accepts_boundaries() -> Result<(), Box<dyn std::error::Error>> {
    let min = Degrees::new_dor(Degrees::MIN_DOR)?;
    assert!((min.value() - 180.0).abs() < f32::EPSILON);

    let max = Degrees::new_dor(Degrees::MAX_DOR)?;
    assert!((max.value() - 2160.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn curve_point_rejects_out_of_range() {
    assert!(CurvePoint::new(-0.1, 0.5).is_err());
    assert!(CurvePoint::new(0.5, 1.1).is_err());
    assert!(CurvePoint::new(f32::NAN, 0.5).is_err());
}

#[test]
fn curve_point_accepts_valid_range() -> Result<(), Box<dyn std::error::Error>> {
    let p = CurvePoint::new(0.0, 0.0)?;
    assert!((p.input).abs() < f32::EPSILON);

    let p = CurvePoint::new(1.0, 1.0)?;
    assert!((p.input - 1.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn frequency_hz_rejects_non_positive() {
    assert!(FrequencyHz::new(0.0).is_err());
    assert!(FrequencyHz::new(-10.0).is_err());
    assert!(FrequencyHz::new(f32::NAN).is_err());
}

// ===================================================================
// 4. Default configurations per device type
// ===================================================================

#[test]
fn default_led_config_has_progressive_pattern() -> Result<(), Box<dyn std::error::Error>> {
    let led = LedConfig::default();
    assert_eq!(led.pattern, "progressive");
    assert!(!led.rpm_bands.is_empty());
    assert!((led.brightness.value() - 0.8).abs() < f32::EPSILON);
    assert!(led.colors.contains_key("green"));
    assert!(led.colors.contains_key("red"));
    Ok(())
}

#[test]
fn default_haptics_config_has_common_effects() -> Result<(), Box<dyn std::error::Error>> {
    let haptics = HapticsConfig::default();
    assert!(haptics.enabled);
    assert!((haptics.intensity.value() - 0.6).abs() < f32::EPSILON);
    assert!(haptics.effects.contains_key("kerb"));
    assert!(haptics.effects.contains_key("slip"));
    assert!(haptics.effects.contains_key("collision"));
    Ok(())
}

#[test]
fn default_filter_curve_is_linear() -> Result<(), Box<dyn std::error::Error>> {
    let filters = FilterConfig::default();
    assert_eq!(filters.curve_points.len(), 2);
    let first = &filters.curve_points[0];
    let last = &filters.curve_points[1];
    assert!((first.input).abs() < f32::EPSILON);
    assert!((first.output).abs() < f32::EPSILON);
    assert!((last.input - 1.0).abs() < f32::EPSILON);
    assert!((last.output - 1.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn device_capabilities_ffb_detection() -> Result<(), Box<dyn std::error::Error>> {
    // Device with PID support
    let caps_pid = DeviceCapabilities::new(
        true,
        false,
        false,
        false,
        TorqueNm::new(10.0)?,
        4096,
        4000,
    );
    assert!(caps_pid.supports_ffb());

    // Device with raw torque
    let caps_raw = DeviceCapabilities::new(
        false,
        true,
        false,
        false,
        TorqueNm::new(25.0)?,
        10000,
        1000,
    );
    assert!(caps_raw.supports_ffb());

    // Input-only device (pedals/shifter)
    let caps_none = DeviceCapabilities::new(
        false,
        false,
        false,
        false,
        TorqueNm::new(0.0)?,
        0,
        0,
    );
    assert!(!caps_none.supports_ffb());
    Ok(())
}

#[test]
fn device_capabilities_update_rate_calculation() -> Result<(), Box<dyn std::error::Error>> {
    let caps = DeviceCapabilities::new(
        true,
        true,
        true,
        true,
        TorqueNm::new(25.0)?,
        10000,
        1000, // 1000us = 1kHz
    );
    assert!((caps.max_update_rate_hz() - 1000.0).abs() < f32::EPSILON);

    let caps_250hz = DeviceCapabilities::new(
        true,
        false,
        false,
        false,
        TorqueNm::new(5.0)?,
        900,
        4000, // 4000us = 250Hz
    );
    assert!((caps_250hz.max_update_rate_hz() - 250.0).abs() < f32::EPSILON);
    Ok(())
}

// ===================================================================
// 5. Configuration migration
// ===================================================================

#[test]
fn schema_version_parsing() -> Result<(), Box<dyn std::error::Error>> {
    let ver = SchemaVersion::parse("wheel.profile/1")?;
    assert_eq!(ver.major, 1);
    assert_eq!(ver.minor, 0);
    assert_eq!(ver.version, "wheel.profile/1");
    Ok(())
}

#[test]
fn current_schema_version_matches_expected() {
    assert_eq!(CURRENT_SCHEMA_VERSION, "wheel.profile/1");
}

#[test]
fn migration_config_defaults_are_reasonable() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let config = MigrationConfig {
        backup_dir: dir.path().to_path_buf(),
        create_backups: true,
        max_backups: 5,
        validate_after_migration: true,
    };
    assert!(config.create_backups);
    assert_eq!(config.max_backups, 5);
    assert!(config.validate_after_migration);
    Ok(())
}

#[test]
fn profile_json_serialization_includes_all_fields() -> Result<(), Box<dyn std::error::Error>> {
    let mut profile = make_profile("full-serialize")?;
    profile.led_config = Some(LedConfig::default());
    profile.haptics_config = Some(HapticsConfig::default());

    let json = serde_json::to_string(&profile)?;

    // Verify key fields present in JSON
    assert!(json.contains("ffb_gain"));
    assert!(json.contains("degrees_of_rotation"));
    assert!(json.contains("torque_cap"));
    assert!(json.contains("led_config"));
    assert!(json.contains("haptics_config"));
    assert!(json.contains("bumpstop"));
    assert!(json.contains("hands_off"));
    Ok(())
}

#[test]
fn profile_inheritance_chain_tracked() -> Result<(), Box<dyn std::error::Error>> {
    let mut store = InMemoryProfileStore::new();

    let parent = make_profile("parent-profile")?;
    let parent_id = parent.id.clone();
    store.add(parent);

    let mut child = make_profile("child-profile")?;
    child.parent = Some(parent_id.clone());
    let child_id = child.id.clone();
    store.add(child);

    // Verify inheritance relationship
    let children = store.find_children(&parent_id);
    assert_eq!(children.len(), 1);
    assert_eq!(children[0], child_id);
    Ok(())
}

#[test]
fn profile_deep_inheritance_chain() -> Result<(), Box<dyn std::error::Error>> {
    let mut store = InMemoryProfileStore::new();

    let root = make_profile("root")?;
    let root_id = root.id.clone();
    store.add(root);

    let mut prev_id = root_id.clone();
    for i in 0..4 {
        let name = format!("child-{i}");
        let mut child = make_profile(&name)?;
        child.parent = Some(prev_id.clone());
        prev_id = child.id.clone();
        store.add(child);
    }

    let descendants = store.find_all_descendants(&root_id);
    assert_eq!(descendants.len(), 4);
    Ok(())
}

#[test]
fn profile_scope_variants() -> Result<(), Box<dyn std::error::Error>> {
    let global = ProfileScope::global();
    assert!(global.game.is_none());
    assert!(global.car.is_none());
    assert!(global.track.is_none());

    let game = ProfileScope::for_game("iRacing".to_string());
    assert_eq!(game.game.as_deref(), Some("iRacing"));
    assert!(game.car.is_none());

    let car = ProfileScope::for_car("ACC".to_string(), "GT3 BMW".to_string());
    assert_eq!(car.game.as_deref(), Some("ACC"));
    assert_eq!(car.car.as_deref(), Some("GT3 BMW"));
    Ok(())
}

#[test]
fn calibration_data_center_type() -> Result<(), Box<dyn std::error::Error>> {
    let cal = CalibrationData {
        center_position: Some(0.5),
        min_position: None,
        max_position: None,
        pedal_ranges: None,
        calibrated_at: Some("2025-01-01T00:00:00Z".to_string()),
        calibration_type: CalibrationType::Center,
    };
    assert!((cal.center_position.ok_or("missing center")? - 0.5).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn calibration_data_full_with_pedals() -> Result<(), Box<dyn std::error::Error>> {
    let cal = CalibrationData {
        center_position: Some(0.0),
        min_position: Some(-540.0),
        max_position: Some(540.0),
        pedal_ranges: Some(PedalCalibrationData {
            throttle: Some((0.0, 1.0)),
            brake: Some((0.0, 0.95)),
            clutch: Some((0.0, 1.0)),
        }),
        calibrated_at: Some("2025-01-01T00:00:00Z".to_string()),
        calibration_type: CalibrationType::Full,
    };

    let pedals = cal.pedal_ranges.as_ref().ok_or("missing pedal ranges")?;
    let throttle = pedals.throttle.ok_or("missing throttle")?;
    assert!((throttle.1 - 1.0).abs() < f32::EPSILON);
    let brake = pedals.brake.ok_or("missing brake")?;
    assert!((brake.1 - 0.95).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn notch_filter_creation() -> Result<(), Box<dyn std::error::Error>> {
    let freq = FrequencyHz::new(50.0)?;
    let filter = NotchFilter::new(freq, 2.0, -20.0)?;
    assert!((filter.frequency.value() - 50.0).abs() < f32::EPSILON);
    assert!((filter.q_factor - 2.0).abs() < f32::EPSILON);
    assert!((filter.gain_db - (-20.0)).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn notch_filter_rejects_invalid_q_factor() -> Result<(), Box<dyn std::error::Error>> {
    let freq = FrequencyHz::new(50.0)?;
    let result = NotchFilter::new(freq, 0.0, -10.0);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn store_empty_initially() {
    let store = InMemoryProfileStore::new();
    assert!(store.is_empty());
    assert_eq!(store.len(), 0);
}

#[test]
fn store_iteration_covers_all_profiles() -> Result<(), Box<dyn std::error::Error>> {
    let mut store = InMemoryProfileStore::new();
    for i in 0..3 {
        store.add(make_profile(&format!("iter-{i}"))?);
    }

    let count = store.iter().count();
    assert_eq!(count, 3);
    Ok(())
}

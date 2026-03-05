//! Comprehensive tests for configuration migration, profile management,
//! and settings persistence.
//!
//! Covers:
//! - Profile CRUD operations (create, read, update, delete)
//! - Profile inheritance (child profiles inherit parent settings)
//! - Profile migration (legacy v0 → current v1 upgrade)
//! - Settings roundtrip (serialize → deserialize preserves all fields)
//! - Import/export (JSON file round-trip)
//! - Validation (invalid profiles rejected with clear errors)
//! - Conflict resolution (concurrent profile edits)
//! - Default profiles (factory defaults for known devices)
//! - Profile search/listing (filtering by scope)
//! - Property-based testing of serialization roundtrips

use crate::config::{ProfileMigrator, ProfileValidator, SchemaError};
use crate::domain::{CurvePoint, Degrees, DomainError, FrequencyHz, Gain, ProfileId, TorqueNm};
use crate::entities::{
    BaseSettings, BumpstopConfig, FilterConfig, HapticsConfig, InMemoryProfileStore, LedConfig,
    MAX_INHERITANCE_DEPTH, NotchFilter, Profile, ProfileScope, ProfileStore,
};
use crate::migration::{
    CURRENT_SCHEMA_VERSION, MigrationConfig, MigrationManager, MigrationResult, SchemaVersion,
};
use proptest::prelude::*;

// ============================================================================
// Test helpers
// ============================================================================

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn make_profile_id(name: &str) -> Result<ProfileId, DomainError> {
    name.parse()
}

fn make_gain(v: f32) -> Result<Gain, DomainError> {
    Gain::new(v)
}

fn make_torque(v: f32) -> Result<TorqueNm, DomainError> {
    TorqueNm::new(v)
}

fn make_dor(v: f32) -> Result<Degrees, DomainError> {
    Degrees::new_dor(v)
}

fn make_freq(v: f32) -> Result<FrequencyHz, DomainError> {
    FrequencyHz::new(v)
}

/// Build a simple Profile for testing purposes.
fn build_test_profile(id: &str) -> Result<Profile, DomainError> {
    let pid = make_profile_id(id)?;
    Ok(Profile::new(
        pid,
        ProfileScope::global(),
        BaseSettings::default(),
        format!("Test-{}", id),
    ))
}

/// Build a Profile with custom FFB gain and DOR.
fn build_custom_profile(
    id: &str,
    gain: f32,
    dor: f32,
    torque: f32,
) -> Result<Profile, DomainError> {
    let pid = make_profile_id(id)?;
    Ok(Profile::new(
        pid,
        ProfileScope::global(),
        BaseSettings::new(
            make_gain(gain)?,
            make_dor(dor)?,
            make_torque(torque)?,
            FilterConfig::default(),
        ),
        format!("Custom-{}", id),
    ))
}

/// Create a MigrationManager with backups disabled (in-memory only).
fn test_migration_manager() -> MigrationResult<MigrationManager> {
    MigrationManager::new(MigrationConfig::without_backups())
}

/// Build a valid v1 JSON profile string for config layer tests.
fn build_v1_json(ffb_gain: f64, dor: u16, torque: f64, game: Option<&str>) -> String {
    serde_json::json!({
        "schema": CURRENT_SCHEMA_VERSION,
        "scope": { "game": game, "car": null, "track": null },
        "base": {
            "ffbGain": ffb_gain,
            "dorDeg": dor,
            "torqueCapNm": torque,
            "filters": {
                "reconstruction": 0,
                "friction": 0.0,
                "damper": 0.0,
                "inertia": 0.0,
                "notchFilters": [],
                "slewRate": 1.0,
                "curvePoints": [
                    {"input": 0.0, "output": 0.0},
                    {"input": 1.0, "output": 1.0}
                ]
            }
        }
    })
    .to_string()
}

/// Build a legacy (v0) JSON profile string.
fn build_legacy_json(ffb_gain: f64, dor: u16, torque: f64) -> String {
    serde_json::json!({
        "ffb_gain": ffb_gain,
        "degrees_of_rotation": dor,
        "torque_cap": torque
    })
    .to_string()
}

// ============================================================================
// 1. Profile CRUD tests
// ============================================================================

#[test]
fn profile_crud_create_and_read() -> TestResult {
    let mut store = InMemoryProfileStore::new();
    let profile = build_test_profile("crud-create")?;
    let pid = profile.id.clone();

    store.add(profile);
    let retrieved = store.get(&pid);
    assert!(retrieved.is_some());
    let retrieved = retrieved.ok_or("profile not found")?;
    assert_eq!(retrieved.id, pid);
    assert_eq!(retrieved.metadata.name, "Test-crud-create");
    Ok(())
}

#[test]
fn profile_crud_update() -> TestResult {
    let mut store = InMemoryProfileStore::new();
    let mut profile = build_test_profile("crud-update")?;
    let pid = profile.id.clone();
    store.add(profile.clone());

    // Mutate and update
    profile.base_settings.ffb_gain = make_gain(0.9)?;
    store.update(profile);

    let retrieved = store.get(&pid).ok_or("profile not found")?;
    assert!(
        (retrieved.base_settings.ffb_gain.value() - 0.9).abs() < f32::EPSILON,
        "ffb_gain should be updated to 0.9"
    );
    Ok(())
}

#[test]
fn profile_crud_delete() -> TestResult {
    let mut store = InMemoryProfileStore::new();
    let profile = build_test_profile("crud-delete")?;
    let pid = profile.id.clone();
    store.add(profile);

    let removed = store.remove(&pid);
    assert!(removed.is_some());
    assert!(store.get(&pid).is_none());
    assert!(store.is_empty());
    Ok(())
}

#[test]
fn profile_crud_delete_nonexistent_returns_none() -> TestResult {
    let mut store = InMemoryProfileStore::new();
    let pid = make_profile_id("nonexistent")?;
    let removed = store.remove(&pid);
    assert!(removed.is_none());
    Ok(())
}

// ============================================================================
// 2. Profile inheritance tests
// ============================================================================

#[test]
fn inheritance_child_overrides_parent_gain() -> TestResult {
    let mut store = InMemoryProfileStore::new();

    let parent = build_custom_profile("parent", 0.5, 900.0, 10.0)?;
    store.add(parent.clone());

    let child = Profile::new_with_parent(
        make_profile_id("child")?,
        parent.id.clone(),
        ProfileScope::for_game("iracing".into()),
        BaseSettings::new(
            make_gain(0.8)?,
            make_dor(900.0)?,
            make_torque(10.0)?,
            FilterConfig::default(),
        ),
        "Child Profile".into(),
    );
    store.add(child.clone());

    let resolved = child.resolve(&store)?;
    // Child explicitly set gain to 0.8, which differs from default 0.7,
    // so it should override parent's 0.5.
    assert!(
        (resolved.effective_settings.ffb_gain.value() - 0.8).abs() < f32::EPSILON,
        "child gain should override parent"
    );
    Ok(())
}

#[test]
fn inheritance_child_inherits_parent_led_when_child_has_none() -> TestResult {
    let mut store = InMemoryProfileStore::new();

    let parent = build_test_profile("led-parent")?;
    store.add(parent.clone());

    let mut child = Profile::new_with_parent(
        make_profile_id("led-child")?,
        parent.id.clone(),
        ProfileScope::global(),
        BaseSettings::default(),
        "LED Child".into(),
    );
    child.led_config = None;
    store.add(child.clone());

    let resolved = child.resolve(&store)?;
    // Parent has default LED config, child has None → should inherit parent
    assert!(
        resolved.led_config.is_some(),
        "child should inherit parent LED config"
    );
    Ok(())
}

#[test]
fn inheritance_depth_limit_enforced() -> TestResult {
    let mut store = InMemoryProfileStore::new();

    // Build a chain of MAX_INHERITANCE_DEPTH + 1 profiles
    let root = build_test_profile("depth-0")?;
    store.add(root.clone());

    let mut prev_id = root.id.clone();
    for i in 1..=(MAX_INHERITANCE_DEPTH + 1) {
        let name = format!("depth-{}", i);
        let p = Profile::new_with_parent(
            make_profile_id(&name)?,
            prev_id.clone(),
            ProfileScope::global(),
            BaseSettings::default(),
            name.clone(),
        );
        store.add(p.clone());
        prev_id = p.id.clone();
    }

    // The deepest child should fail to resolve
    let deepest = store.get(&prev_id).ok_or("deepest profile not found")?;
    let result = deepest.resolve(&store);
    assert!(result.is_err(), "should fail due to depth limit");
    match result {
        Err(DomainError::InheritanceDepthExceeded { .. }) => {} // expected
        other => return Err(format!("unexpected result: {:?}", other).into()),
    }
    Ok(())
}

#[test]
fn inheritance_circular_detected() -> TestResult {
    let mut store = InMemoryProfileStore::new();

    let mut a = build_test_profile("circ-a")?;
    let mut b = build_test_profile("circ-b")?;

    a.parent = Some(b.id.clone());
    b.parent = Some(a.id.clone());

    store.add(a.clone());
    store.add(b.clone());

    let result = a.resolve(&store);
    assert!(result.is_err(), "circular inheritance should fail");
    match result {
        Err(DomainError::CircularInheritance { .. }) => {} // expected
        other => return Err(format!("unexpected result: {:?}", other).into()),
    }
    Ok(())
}

#[test]
fn inheritance_missing_parent_detected() -> TestResult {
    let store = InMemoryProfileStore::new();

    let child = Profile::new_with_parent(
        make_profile_id("orphan")?,
        make_profile_id("nonexistent-parent")?,
        ProfileScope::global(),
        BaseSettings::default(),
        "Orphan".into(),
    );
    // Don't add parent to store

    let result = child.resolve(&store);
    assert!(result.is_err());
    match result {
        Err(DomainError::ParentProfileNotFound { .. }) => {} // expected
        other => return Err(format!("unexpected: {:?}", other).into()),
    }
    Ok(())
}

#[test]
fn inheritance_chain_of_three_resolves_correctly() -> TestResult {
    let mut store = InMemoryProfileStore::new();

    // Grandparent: gain 0.3, DOR 1080
    let grandparent = build_custom_profile("gp", 0.3, 1080.0, 20.0)?;
    store.add(grandparent.clone());

    // Parent: gain 0.6, inherits DOR from grandparent (uses default DOR=900 which
    // differs from grandparent's 1080, so parent overrides to 900)
    let parent = Profile::new_with_parent(
        make_profile_id("par")?,
        grandparent.id.clone(),
        ProfileScope::for_game("acc".into()),
        BaseSettings::new(
            make_gain(0.6)?,
            make_dor(900.0)?,
            make_torque(20.0)?,
            FilterConfig::default(),
        ),
        "Parent".into(),
    );
    store.add(parent.clone());

    // Child: default gain (0.7), default DOR (900.0) => inherits parent's
    let child = Profile::new_with_parent(
        make_profile_id("ch")?,
        parent.id.clone(),
        ProfileScope::for_car("acc".into(), "gt3-audi".into()),
        BaseSettings::default(),
        "Child".into(),
    );
    store.add(child.clone());

    let resolved = child.resolve(&store)?;
    assert_eq!(resolved.inheritance_chain.len(), 3);
    Ok(())
}

// ============================================================================
// 3. Profile migration tests
// ============================================================================

#[test]
fn migration_legacy_to_v1_preserves_values() -> TestResult {
    let manager = test_migration_manager()?;
    let legacy = build_legacy_json(0.85, 540, 18.0);

    let migrated = manager.migrate_profile(&legacy)?;
    let value: serde_json::Value = serde_json::from_str(&migrated)?;

    assert_eq!(
        value.get("schema").and_then(|v| v.as_str()),
        Some(CURRENT_SCHEMA_VERSION)
    );

    let base = value
        .get("base")
        .ok_or("missing 'base' in migrated profile")?;
    assert_eq!(base.get("ffbGain").and_then(|v| v.as_f64()), Some(0.85));
    assert_eq!(base.get("dorDeg").and_then(|v| v.as_u64()), Some(540));
    assert_eq!(base.get("torqueCapNm").and_then(|v| v.as_f64()), Some(18.0));

    assert!(value.get("scope").is_some());
    Ok(())
}

#[test]
fn migration_already_current_is_noop() -> TestResult {
    let manager = test_migration_manager()?;
    let current = build_v1_json(0.7, 900, 15.0, None);

    assert!(!manager.needs_migration(&current)?);
    let migrated = manager.migrate_profile(&current)?;
    let orig: serde_json::Value = serde_json::from_str(&current)?;
    let result: serde_json::Value = serde_json::from_str(&migrated)?;
    assert_eq!(orig.get("schema"), result.get("schema"));
    Ok(())
}

#[test]
fn migration_unknown_version_rejected() -> TestResult {
    let manager = test_migration_manager()?;
    let unknown = serde_json::json!({
        "schema": "wheel.profile/99",
        "scope": {},
        "base": { "ffbGain": 0.5 }
    })
    .to_string();

    let result = manager.migrate_profile(&unknown);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn migration_legacy_default_values_filled() -> TestResult {
    // Legacy profile with no explicit values → migration should use defaults
    let manager = test_migration_manager()?;
    let minimal_legacy = serde_json::json!({
        "ffb_gain": 0.5
    })
    .to_string();

    let migrated = manager.migrate_profile(&minimal_legacy)?;
    let value: serde_json::Value = serde_json::from_str(&migrated)?;

    let base = value.get("base").ok_or("missing 'base'")?;
    // DOR defaults to 900 in migration
    assert_eq!(base.get("dorDeg").and_then(|v| v.as_u64()), Some(900));
    // torqueCapNm defaults to 15.0 in migration
    assert_eq!(base.get("torqueCapNm").and_then(|v| v.as_f64()), Some(15.0));
    Ok(())
}

#[test]
fn migration_version_detection_legacy_format() -> TestResult {
    let manager = test_migration_manager()?;

    // Flat fields without "schema" key → detected as v0
    let legacy = r#"{"ffb_gain": 0.6, "degrees_of_rotation": 720}"#;
    let version = manager.detect_version(legacy)?;
    assert_eq!(version.major, 0);
    assert!(version.is_older_than(&SchemaVersion::parse(CURRENT_SCHEMA_VERSION)?));
    Ok(())
}

#[test]
fn migration_version_detection_v1_format() -> TestResult {
    let manager = test_migration_manager()?;
    let v1 = build_v1_json(0.7, 900, 15.0, None);
    let version = manager.detect_version(&v1)?;
    assert_eq!(version.major, 1);
    assert!(version.is_current());
    Ok(())
}

#[test]
fn migration_config_without_backups_works() -> TestResult {
    let config = MigrationConfig::without_backups();
    assert!(!config.create_backups);
    assert_eq!(config.max_backups, 0);
    assert!(config.validate_after_migration);
    Ok(())
}

// ============================================================================
// 4. Settings roundtrip tests (serialize → deserialize)
// ============================================================================

#[test]
fn roundtrip_profile_json_preserves_all_fields() -> TestResult {
    let profile = build_custom_profile("roundtrip", 0.85, 540.0, 20.0)?;
    let json = serde_json::to_string(&profile)?;
    let deserialized: Profile = serde_json::from_str(&json)?;

    assert_eq!(deserialized.id, profile.id);
    assert!((deserialized.base_settings.ffb_gain.value() - 0.85).abs() < f32::EPSILON);
    assert!((deserialized.base_settings.degrees_of_rotation.value() - 540.0).abs() < f32::EPSILON);
    assert!((deserialized.base_settings.torque_cap.value() - 20.0).abs() < f32::EPSILON);
    assert_eq!(deserialized.scope, profile.scope);
    assert_eq!(deserialized.metadata.name, profile.metadata.name);
    Ok(())
}

#[test]
fn roundtrip_filter_config_preserves_notch_filters() -> TestResult {
    let freq = make_freq(120.0)?;
    let notch = NotchFilter::new(freq, 3.0, -6.0)?;

    let config = FilterConfig::new(
        4,
        make_gain(0.1)?,
        make_gain(0.2)?,
        make_gain(0.05)?,
        vec![notch],
        make_gain(0.8)?,
        vec![CurvePoint::new(0.0, 0.0)?, CurvePoint::new(1.0, 1.0)?],
    )?;

    let json = serde_json::to_string(&config)?;
    let deserialized: FilterConfig = serde_json::from_str(&json)?;

    assert_eq!(deserialized.notch_filters.len(), 1);
    assert!((deserialized.notch_filters[0].frequency.value() - 120.0).abs() < f32::EPSILON);
    assert!((deserialized.notch_filters[0].q_factor - 3.0).abs() < f32::EPSILON);
    assert!((deserialized.notch_filters[0].gain_db - (-6.0)).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn roundtrip_profile_with_optional_configs() -> TestResult {
    let mut profile = build_test_profile("opt-configs")?;
    profile.led_config = Some(LedConfig::default());
    profile.haptics_config = Some(HapticsConfig::default());

    let json = serde_json::to_string(&profile)?;
    let deserialized: Profile = serde_json::from_str(&json)?;

    assert!(deserialized.led_config.is_some());
    assert!(deserialized.haptics_config.is_some());

    let led = deserialized.led_config.ok_or("missing led config")?;
    assert_eq!(led.pattern, "progressive");

    let haptics = deserialized
        .haptics_config
        .ok_or("missing haptics config")?;
    assert!(haptics.enabled);
    Ok(())
}

#[test]
fn roundtrip_profile_without_optional_configs() -> TestResult {
    let mut profile = build_test_profile("no-opt")?;
    profile.led_config = None;
    profile.haptics_config = None;

    let json = serde_json::to_string(&profile)?;
    let deserialized: Profile = serde_json::from_str(&json)?;

    assert!(deserialized.led_config.is_none());
    assert!(deserialized.haptics_config.is_none());
    Ok(())
}

#[test]
fn roundtrip_base_settings_default() -> TestResult {
    let settings = BaseSettings::default();
    let json = serde_json::to_string(&settings)?;
    let deserialized: BaseSettings = serde_json::from_str(&json)?;

    assert!((deserialized.ffb_gain.value() - settings.ffb_gain.value()).abs() < f32::EPSILON);
    assert!(
        (deserialized.degrees_of_rotation.value() - settings.degrees_of_rotation.value()).abs()
            < f32::EPSILON
    );
    assert!((deserialized.torque_cap.value() - settings.torque_cap.value()).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn roundtrip_profile_metadata_preserved() -> TestResult {
    let mut profile = build_test_profile("meta")?;
    profile.metadata.description = Some("A test description".into());
    profile.metadata.author = Some("test-author".into());
    profile.metadata.tags = vec!["drift".into(), "rally".into()];

    let json = serde_json::to_string(&profile)?;
    let deserialized: Profile = serde_json::from_str(&json)?;

    assert_eq!(
        deserialized.metadata.description,
        Some("A test description".into())
    );
    assert_eq!(deserialized.metadata.author, Some("test-author".into()));
    assert_eq!(deserialized.metadata.tags, vec!["drift", "rally"]);
    Ok(())
}

// ============================================================================
// 5. Import/export tests (JSON config layer)
// ============================================================================

#[test]
fn import_export_v1_profile_roundtrip() -> TestResult {
    let validator = ProfileValidator::new()?;

    let json = build_v1_json(0.75, 900, 15.0, Some("iracing"));
    let profile = validator.validate_json(&json)?;

    // Re-export
    let exported = serde_json::to_string_pretty(&profile)?;
    let reimported = validator.validate_json(&exported)?;

    assert!((reimported.base.ffb_gain - 0.75).abs() < f32::EPSILON);
    assert_eq!(reimported.base.dor_deg, 900);
    assert!((reimported.base.torque_cap_nm - 15.0).abs() < f32::EPSILON);
    assert_eq!(reimported.scope.game, Some("iracing".into()));
    Ok(())
}

#[test]
fn import_export_with_leds_and_haptics() -> TestResult {
    let validator = ProfileValidator::new()?;

    let json = serde_json::json!({
        "schema": "wheel.profile/1",
        "scope": { "game": "acc" },
        "base": {
            "ffbGain": 0.8,
            "dorDeg": 540,
            "torqueCapNm": 12.0,
            "filters": {
                "reconstruction": 2,
                "friction": 0.1,
                "damper": 0.15,
                "inertia": 0.05,
                "notchFilters": [{"hz": 60.0, "q": 2.0, "gainDb": -12.0}],
                "slewRate": 0.9,
                "curvePoints": [
                    {"input": 0.0, "output": 0.0},
                    {"input": 0.5, "output": 0.6},
                    {"input": 1.0, "output": 1.0}
                ]
            }
        },
        "leds": {
            "rpmBands": [0.75, 0.82, 0.88, 0.92, 0.96],
            "pattern": "progressive",
            "brightness": 0.8,
            "colors": {"green": [0, 255, 0]}
        },
        "haptics": {
            "enabled": true,
            "intensity": 0.6,
            "frequencyHz": 80.0,
            "effects": {"kerb": true, "slip": false}
        }
    })
    .to_string();

    let profile = validator.validate_json(&json)?;
    let exported = serde_json::to_string(&profile)?;
    let reimported = validator.validate_json(&exported)?;

    let leds = reimported.leds.ok_or("missing LEDs after roundtrip")?;
    assert_eq!(leds.rpm_bands.len(), 5);
    assert_eq!(leds.pattern, "progressive");

    let haptics = reimported
        .haptics
        .ok_or("missing haptics after roundtrip")?;
    assert!(haptics.enabled);
    assert!((haptics.intensity - 0.6).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn import_export_migrator_roundtrip() -> TestResult {
    let json = build_v1_json(0.7, 900, 15.0, None);
    let profile = ProfileMigrator::migrate_profile(&json)?;
    let re_exported = serde_json::to_string(&profile)?;
    let re_imported = ProfileMigrator::migrate_profile(&re_exported)?;

    assert!((re_imported.base.ffb_gain - 0.7).abs() < f32::EPSILON);
    assert_eq!(re_imported.base.dor_deg, 900);
    Ok(())
}

// ============================================================================
// 6. Validation tests
// ============================================================================

#[test]
fn validation_rejects_unknown_schema_version() -> TestResult {
    let validator = ProfileValidator::new()?;
    let mut json = build_v1_json(0.7, 900, 15.0, None);
    json = json.replace("wheel.profile/1", "wheel.profile/99");

    let result = validator.validate_json(&json);
    assert!(result.is_err(), "should reject unknown schema version");
    // The JSON Schema itself may reject the version string before business rules
    // run, so accept either a ValidationError or UnsupportedSchemaVersion
    match &result {
        Err(SchemaError::UnsupportedSchemaVersion(_))
        | Err(SchemaError::ValidationError { .. }) => {} // both are acceptable
        other => return Err(format!("unexpected: {:?}", other).into()),
    }
    Ok(())
}

#[test]
fn validation_rejects_non_monotonic_curve() -> TestResult {
    let validator = ProfileValidator::new()?;
    let json = serde_json::json!({
        "schema": "wheel.profile/1",
        "scope": {},
        "base": {
            "ffbGain": 0.7,
            "dorDeg": 900,
            "torqueCapNm": 15.0,
            "filters": {
                "reconstruction": 0,
                "friction": 0.0,
                "damper": 0.0,
                "inertia": 0.0,
                "notchFilters": [],
                "slewRate": 1.0,
                "curvePoints": [
                    {"input": 0.0, "output": 0.0},
                    {"input": 0.8, "output": 0.9},
                    {"input": 0.5, "output": 0.6}
                ]
            }
        }
    })
    .to_string();

    let result = validator.validate_json(&json);
    assert!(result.is_err());
    match result {
        Err(SchemaError::NonMonotonicCurve) => {} // expected
        other => return Err(format!("unexpected: {:?}", other).into()),
    }
    Ok(())
}

#[test]
fn validation_rejects_unsorted_rpm_bands() -> TestResult {
    let validator = ProfileValidator::new()?;
    let json = serde_json::json!({
        "schema": "wheel.profile/1",
        "scope": {},
        "base": {
            "ffbGain": 0.7,
            "dorDeg": 900,
            "torqueCapNm": 15.0,
            "filters": {
                "reconstruction": 0,
                "friction": 0.0,
                "damper": 0.0,
                "inertia": 0.0,
                "notchFilters": [],
                "slewRate": 1.0,
                "curvePoints": [
                    {"input": 0.0, "output": 0.0},
                    {"input": 1.0, "output": 1.0}
                ]
            }
        },
        "leds": {
            "rpmBands": [0.9, 0.8, 0.7],
            "pattern": "progressive",
            "brightness": 0.5,
            "colors": {}
        }
    })
    .to_string();

    let result = validator.validate_json(&json);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn validation_rejects_malformed_json() -> TestResult {
    let validator = ProfileValidator::new()?;
    let result = validator.validate_json("not valid json {{{");
    assert!(result.is_err());
    match result {
        Err(SchemaError::JsonError(_)) => {} // expected
        other => return Err(format!("unexpected: {:?}", other).into()),
    }
    Ok(())
}

#[test]
fn validation_domain_invalid_gain() -> TestResult {
    let result = Gain::new(1.5);
    assert!(result.is_err());
    match result {
        Err(DomainError::InvalidGain(v)) => assert!((v - 1.5).abs() < f32::EPSILON),
        other => return Err(format!("unexpected: {:?}", other).into()),
    }

    let result = Gain::new(-0.1);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn validation_domain_invalid_torque() -> TestResult {
    let result = TorqueNm::new(51.0);
    assert!(result.is_err());

    let result = TorqueNm::new(-1.0);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn validation_domain_invalid_dor() -> TestResult {
    let result = Degrees::new_dor(100.0);
    assert!(result.is_err());

    let result = Degrees::new_dor(3000.0);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn validation_domain_invalid_profile_id() -> TestResult {
    let result = "".parse::<ProfileId>();
    assert!(result.is_err());

    let result = "has spaces".parse::<ProfileId>();
    assert!(result.is_err());
    Ok(())
}

#[test]
fn validation_filter_reconstruction_out_of_range() -> TestResult {
    let result = FilterConfig::new(
        10, // > 8 is invalid
        make_gain(0.0)?,
        make_gain(0.0)?,
        make_gain(0.0)?,
        vec![],
        make_gain(1.0)?,
        vec![CurvePoint::new(0.0, 0.0)?, CurvePoint::new(1.0, 1.0)?],
    );
    assert!(result.is_err());
    Ok(())
}

// ============================================================================
// 7. Conflict resolution / concurrent profile edits
// ============================================================================

#[test]
fn conflict_last_write_wins_on_update() -> TestResult {
    let mut store = InMemoryProfileStore::new();
    let profile = build_test_profile("conflict")?;
    let pid = profile.id.clone();
    store.add(profile.clone());

    // Simulate two concurrent edits by updating twice
    let mut edit_a = profile.clone();
    edit_a.base_settings.ffb_gain = make_gain(0.3)?;

    let mut edit_b = profile.clone();
    edit_b.base_settings.ffb_gain = make_gain(0.9)?;

    store.update(edit_a);
    store.update(edit_b);

    let final_profile = store.get(&pid).ok_or("profile not found")?;
    assert!(
        (final_profile.base_settings.ffb_gain.value() - 0.9).abs() < f32::EPSILON,
        "last write should win"
    );
    Ok(())
}

#[test]
fn conflict_observer_notified_on_change() -> TestResult {
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct TestObserver {
        events: Mutex<Vec<String>>,
    }

    impl crate::entities::ProfileChangeObserver for TestObserver {
        fn on_profile_change(
            &self,
            event: &crate::entities::ProfileChangeEvent,
            _affected_children: &[ProfileId],
        ) {
            let desc = match event {
                crate::entities::ProfileChangeEvent::Modified { profile_id } => {
                    format!("modified:{}", profile_id)
                }
                crate::entities::ProfileChangeEvent::Removed { profile_id } => {
                    format!("removed:{}", profile_id)
                }
            };
            if let Ok(mut events) = self.events.lock() {
                events.push(desc);
            }
        }
    }

    let mut store = InMemoryProfileStore::new();
    let observer = Arc::new(TestObserver::default());
    store.register_observer(observer.clone());

    let profile = build_test_profile("observed")?;
    let pid = profile.id.clone();
    store.add(profile.clone());

    let mut updated = profile;
    updated.base_settings.ffb_gain = make_gain(0.5)?;
    store.update(updated);

    store.remove(&pid);

    let events = observer
        .events
        .lock()
        .map_err(|e| format!("lock error: {}", e))?;
    assert!(events.len() >= 2, "should have at least 2 events");
    assert!(events.iter().any(|e| e.starts_with("modified:")));
    assert!(events.iter().any(|e| e.starts_with("removed:")));
    Ok(())
}

// ============================================================================
// 8. Default profiles / factory defaults
// ============================================================================

#[test]
fn default_global_profile_is_valid() -> TestResult {
    let profile = Profile::default_global()?;
    assert_eq!(profile.id.as_str(), "global");
    assert!(profile.scope.game.is_none());
    assert!(profile.scope.car.is_none());
    assert!(profile.scope.track.is_none());
    assert!(!profile.has_parent());

    // Verify default settings are sensible
    let gain = profile.base_settings.ffb_gain.value();
    assert!(gain > 0.0 && gain <= 1.0, "default gain must be in (0,1]");

    let dor = profile.base_settings.degrees_of_rotation.value();
    assert!(
        (Degrees::MIN_DOR..=Degrees::MAX_DOR).contains(&dor),
        "default DOR must be in valid range"
    );

    let torque = profile.base_settings.torque_cap.value();
    assert!(
        torque > 0.0 && torque <= TorqueNm::MAX_TORQUE,
        "default torque must be in valid range"
    );
    Ok(())
}

#[test]
fn default_filter_config_is_linear() -> TestResult {
    let config = FilterConfig::default();
    assert!(config.is_linear());
    assert_eq!(config.reconstruction, 0);
    assert!((config.friction.value() - 0.0).abs() < f32::EPSILON);
    assert!((config.damper.value() - 0.0).abs() < f32::EPSILON);
    assert!((config.inertia.value() - 0.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn default_led_config_has_standard_bands() -> TestResult {
    let led = LedConfig::default();
    assert_eq!(led.rpm_bands.len(), 5);
    assert_eq!(led.pattern, "progressive");
    assert!(led.brightness.value() > 0.0);
    assert!(led.colors.contains_key("green"));
    assert!(led.colors.contains_key("red"));
    Ok(())
}

#[test]
fn default_haptics_config_enables_common_effects() -> TestResult {
    let haptics = HapticsConfig::default();
    assert!(haptics.enabled);
    assert!(haptics.intensity.value() > 0.0);
    assert!(haptics.frequency.value() > 0.0);
    assert_eq!(haptics.effects.get("kerb"), Some(&true));
    assert_eq!(haptics.effects.get("collision"), Some(&true));
    Ok(())
}

#[test]
fn default_bumpstop_config_is_sensible() -> TestResult {
    let bs = BumpstopConfig::default();
    assert!(bs.enabled);
    assert!(bs.start_angle > 0.0);
    assert!(bs.max_angle > bs.start_angle);
    assert!(bs.stiffness > 0.0 && bs.stiffness <= 1.0);
    assert!(bs.damping >= 0.0 && bs.damping <= 1.0);
    Ok(())
}

// ============================================================================
// 9. Profile search/listing tests
// ============================================================================

#[test]
fn profile_listing_counts_correct() -> TestResult {
    let mut store = InMemoryProfileStore::new();
    assert!(store.is_empty());
    assert_eq!(store.len(), 0);

    store.add(build_test_profile("list-a")?);
    store.add(build_test_profile("list-b")?);
    store.add(build_test_profile("list-c")?);

    assert_eq!(store.len(), 3);
    assert!(!store.is_empty());
    Ok(())
}

#[test]
fn profile_scope_matching_game_filter() -> TestResult {
    let mut store = InMemoryProfileStore::new();

    let global = build_test_profile("global")?;
    store.add(global);

    let mut iracing = build_test_profile("iracing")?;
    iracing.scope = ProfileScope::for_game("iracing".into());
    store.add(iracing);

    let mut acc = build_test_profile("acc")?;
    acc.scope = ProfileScope::for_game("acc".into());
    store.add(acc);

    // Filter profiles matching "iracing"
    let iracing_profiles: Vec<_> = store
        .iter()
        .filter(|(_, p)| p.scope.matches(Some("iracing"), None, None))
        .collect();
    // Global matches everything, iracing matches iracing
    assert_eq!(iracing_profiles.len(), 2);

    // Filter profiles matching "acc"
    let acc_profiles: Vec<_> = store
        .iter()
        .filter(|(_, p)| p.scope.matches(Some("acc"), None, None))
        .collect();
    assert_eq!(acc_profiles.len(), 2);

    // Filter with no game → only global matches
    let no_game_profiles: Vec<_> = store
        .iter()
        .filter(|(_, p)| p.scope.matches(None, None, None))
        .collect();
    // game-specific profiles DON'T match when no game is specified
    assert_eq!(no_game_profiles.len(), 1);
    Ok(())
}

#[test]
fn profile_scope_specificity_ordering() -> TestResult {
    let global = ProfileScope::global();
    let game = ProfileScope::for_game("iracing".into());
    let car = ProfileScope::for_car("iracing".into(), "gt3".into());
    let track = ProfileScope::for_track("iracing".into(), "gt3".into(), "spa".into());

    assert_eq!(global.specificity_level(), 0);
    assert_eq!(game.specificity_level(), 1);
    assert_eq!(car.specificity_level(), 2);
    assert_eq!(track.specificity_level(), 3);

    assert!(game.is_more_specific_than(&global));
    assert!(car.is_more_specific_than(&game));
    assert!(track.is_more_specific_than(&car));
    assert!(!global.is_more_specific_than(&game));
    Ok(())
}

#[test]
fn profile_find_children() -> TestResult {
    let mut store = InMemoryProfileStore::new();

    let parent = build_test_profile("parent")?;
    store.add(parent.clone());

    let child1 = Profile::new_with_parent(
        make_profile_id("child1")?,
        parent.id.clone(),
        ProfileScope::global(),
        BaseSettings::default(),
        "Child1".into(),
    );
    store.add(child1);

    let child2 = Profile::new_with_parent(
        make_profile_id("child2")?,
        parent.id.clone(),
        ProfileScope::global(),
        BaseSettings::default(),
        "Child2".into(),
    );
    store.add(child2);

    let children = store.find_children(&parent.id);
    assert_eq!(children.len(), 2);

    let descendants = store.find_all_descendants(&parent.id);
    assert_eq!(descendants.len(), 2);
    Ok(())
}

#[test]
fn profile_find_descendants_multi_level() -> TestResult {
    let mut store = InMemoryProfileStore::new();

    let root = build_test_profile("root")?;
    store.add(root.clone());

    let mid = Profile::new_with_parent(
        make_profile_id("mid")?,
        root.id.clone(),
        ProfileScope::global(),
        BaseSettings::default(),
        "Mid".into(),
    );
    store.add(mid.clone());

    let leaf = Profile::new_with_parent(
        make_profile_id("leaf")?,
        mid.id.clone(),
        ProfileScope::global(),
        BaseSettings::default(),
        "Leaf".into(),
    );
    store.add(leaf);

    let descendants = store.find_all_descendants(&root.id);
    assert_eq!(descendants.len(), 2, "should find mid and leaf");
    Ok(())
}

// ============================================================================
// 10. Schema version tests
// ============================================================================

#[test]
fn schema_version_parse_valid() -> TestResult {
    let v = SchemaVersion::parse("wheel.profile/1")?;
    assert_eq!(v.major, 1);
    assert_eq!(v.minor, 0);
    assert!(v.is_current());
    Ok(())
}

#[test]
fn schema_version_parse_with_minor() -> TestResult {
    let v = SchemaVersion::parse("wheel.profile/2.3")?;
    assert_eq!(v.major, 2);
    assert_eq!(v.minor, 3);
    assert!(!v.is_current());
    Ok(())
}

#[test]
fn schema_version_parse_invalid() -> TestResult {
    let result = SchemaVersion::parse("invalid");
    assert!(result.is_err());

    let result = SchemaVersion::parse("other.format/1");
    assert!(result.is_err());
    Ok(())
}

#[test]
fn schema_version_ordering() -> TestResult {
    let v0 = SchemaVersion::new(0, 0);
    let v1 = SchemaVersion::parse("wheel.profile/1")?;
    let v1_1 = SchemaVersion::parse("wheel.profile/1.1")?;
    let v2 = SchemaVersion::new(2, 0);

    assert!(v0.is_older_than(&v1));
    assert!(v1.is_older_than(&v1_1));
    assert!(v1_1.is_older_than(&v2));
    assert!(!v2.is_older_than(&v1));
    assert!(!v1.is_older_than(&v0));
    Ok(())
}

// ============================================================================
// 11. Profile hash / comparison tests
// ============================================================================

#[test]
fn profile_hash_deterministic() -> TestResult {
    let p1 = build_custom_profile("hash-a", 0.7, 900.0, 15.0)?;
    let p2 = build_custom_profile("hash-a", 0.7, 900.0, 15.0)?;

    assert_eq!(p1.calculate_hash(), p2.calculate_hash());
    Ok(())
}

#[test]
fn profile_hash_differs_on_gain_change() -> TestResult {
    let p1 = build_custom_profile("hash-b", 0.7, 900.0, 15.0)?;
    let p2 = build_custom_profile("hash-b", 0.8, 900.0, 15.0)?;

    assert_ne!(p1.calculate_hash(), p2.calculate_hash());
    Ok(())
}

// ============================================================================
// 12. Proptest: property-based serialization roundtrips
// ============================================================================

/// Strategy for valid FFB gain values
fn gain_strategy() -> impl Strategy<Value = f32> {
    0.0f32..=1.0f32
}

/// Strategy for valid DOR values
fn dor_strategy() -> impl Strategy<Value = f32> {
    180.0f32..=2160.0f32
}

/// Strategy for valid torque values
fn torque_strategy() -> impl Strategy<Value = f32> {
    0.1f32..=50.0f32
}

/// Strategy for valid frequency values
fn frequency_strategy() -> impl Strategy<Value = f32> {
    1.0f32..=20000.0f32
}

/// Strategy for valid curve point coordinates
fn curve_coord_strategy() -> impl Strategy<Value = f32> {
    0.0f32..=1.0f32
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn proptest_gain_roundtrip(v in gain_strategy()) {
        let gain = Gain::new(v);
        prop_assert!(gain.is_ok());
        let gain = gain.map_err(|e| TestCaseError::Fail(format!("{:?}", e).into()))?;
        let json = serde_json::to_string(&gain).map_err(|e| TestCaseError::Fail(format!("{}", e).into()))?;
        let back: Gain = serde_json::from_str(&json).map_err(|e| TestCaseError::Fail(format!("{}", e).into()))?;
        prop_assert!((back.value() - v).abs() < f32::EPSILON);
    }

    #[test]
    fn proptest_torque_roundtrip(v in torque_strategy()) {
        let t = TorqueNm::new(v);
        prop_assert!(t.is_ok());
        let t = t.map_err(|e| TestCaseError::Fail(format!("{:?}", e).into()))?;
        let json = serde_json::to_string(&t).map_err(|e| TestCaseError::Fail(format!("{}", e).into()))?;
        let back: TorqueNm = serde_json::from_str(&json).map_err(|e| TestCaseError::Fail(format!("{}", e).into()))?;
        prop_assert!((back.value() - v).abs() < f32::EPSILON);
    }

    #[test]
    fn proptest_degrees_roundtrip(v in dor_strategy()) {
        let d = Degrees::new_dor(v);
        prop_assert!(d.is_ok());
        let d = d.map_err(|e| TestCaseError::Fail(format!("{:?}", e).into()))?;
        let json = serde_json::to_string(&d).map_err(|e| TestCaseError::Fail(format!("{}", e).into()))?;
        let back: Degrees = serde_json::from_str(&json).map_err(|e| TestCaseError::Fail(format!("{}", e).into()))?;
        prop_assert!((back.value() - v).abs() < f32::EPSILON);
    }

    #[test]
    fn proptest_profile_roundtrip(
        gain in gain_strategy(),
        dor in dor_strategy(),
        torque in torque_strategy(),
    ) {
        let g = Gain::new(gain).map_err(|e| TestCaseError::Fail(format!("{:?}", e).into()))?;
        let d = Degrees::new_dor(dor).map_err(|e| TestCaseError::Fail(format!("{:?}", e).into()))?;
        let t = TorqueNm::new(torque).map_err(|e| TestCaseError::Fail(format!("{:?}", e).into()))?;
        let pid: ProfileId = "proptest".parse().map_err(|e: DomainError| TestCaseError::Fail(format!("{:?}", e).into()))?;

        let profile = Profile::new(
            pid,
            ProfileScope::global(),
            BaseSettings::new(g, d, t, FilterConfig::default()),
            "Proptest".into(),
        );

        let json = serde_json::to_string(&profile).map_err(|e| TestCaseError::Fail(format!("{}", e).into()))?;
        let back: Profile = serde_json::from_str(&json).map_err(|e| TestCaseError::Fail(format!("{}", e).into()))?;

        prop_assert!((back.base_settings.ffb_gain.value() - gain).abs() < f32::EPSILON);
        prop_assert!((back.base_settings.degrees_of_rotation.value() - dor).abs() < f32::EPSILON);
        prop_assert!((back.base_settings.torque_cap.value() - torque).abs() < f32::EPSILON);
        prop_assert_eq!(back.id, profile.id);
        prop_assert_eq!(back.scope, profile.scope);
    }

    #[test]
    fn proptest_frequency_roundtrip(v in frequency_strategy()) {
        let f = FrequencyHz::new(v).map_err(|e| TestCaseError::Fail(format!("{:?}", e).into()))?;
        let json = serde_json::to_string(&f).map_err(|e| TestCaseError::Fail(format!("{}", e).into()))?;
        let back: FrequencyHz = serde_json::from_str(&json).map_err(|e| TestCaseError::Fail(format!("{}", e).into()))?;
        prop_assert!((back.value() - v).abs() < f32::EPSILON);
    }

    #[test]
    fn proptest_curve_point_roundtrip(
        input in curve_coord_strategy(),
        output in curve_coord_strategy(),
    ) {
        let cp = CurvePoint::new(input, output).map_err(|e| TestCaseError::Fail(format!("{:?}", e).into()))?;
        let json = serde_json::to_string(&cp).map_err(|e| TestCaseError::Fail(format!("{}", e).into()))?;
        let back: CurvePoint = serde_json::from_str(&json).map_err(|e| TestCaseError::Fail(format!("{}", e).into()))?;
        prop_assert!((back.input - input).abs() < f32::EPSILON);
        prop_assert!((back.output - output).abs() < f32::EPSILON);
    }

    #[test]
    fn proptest_v1_json_migration_noop(
        gain in 0.0f64..=1.0f64,
        torque in 0.1f64..=50.0f64,
    ) {
        let dor: u16 = 900;
        let json = build_v1_json(gain, dor, torque, None);
        let manager = MigrationManager::new(MigrationConfig::without_backups())
            .map_err(|e| TestCaseError::Fail(format!("{:?}", e).into()))?;

        let needs = manager.needs_migration(&json)
            .map_err(|e| TestCaseError::Fail(format!("{:?}", e).into()))?;
        prop_assert!(!needs, "v1 profiles should not need migration");
    }
}

// ============================================================================
// 13. Migration service tests
// ============================================================================

#[test]
fn migration_service_detect_and_migrate() -> TestResult {
    use crate::migration::ProfileMigrationService;

    let service = ProfileMigrationService::new(MigrationConfig::without_backups())?;
    let legacy = build_legacy_json(0.6, 720, 12.0);

    assert!(service.needs_migration(&legacy)?);

    let outcome = service.migrate_with_backup(&legacy, None)?;
    assert!(outcome.was_migrated());
    assert_eq!(outcome.migration_count(), 1);
    assert!(outcome.target_version.is_current());
    Ok(())
}

#[test]
fn migration_service_no_migration_needed() -> TestResult {
    use crate::migration::ProfileMigrationService;

    let service = ProfileMigrationService::new(MigrationConfig::without_backups())?;
    let current = build_v1_json(0.7, 900, 15.0, None);

    assert!(!service.needs_migration(&current)?);

    let outcome = service.migrate_with_backup(&current, None)?;
    assert!(!outcome.was_migrated());
    assert_eq!(outcome.migration_count(), 0);
    Ok(())
}

#[test]
fn migration_service_file_roundtrip() -> TestResult {
    use crate::migration::ProfileMigrationService;
    use tempfile::TempDir;

    let temp = TempDir::new()?;
    let backup_dir = temp.path().join("backups");
    std::fs::create_dir_all(&backup_dir)?;

    let config = MigrationConfig::new(&backup_dir);
    let service = ProfileMigrationService::new(config)?;

    // Write a legacy profile to a file
    let profile_path = temp.path().join("test_profile.json");
    let legacy = build_legacy_json(0.75, 540, 18.0);
    std::fs::write(&profile_path, &legacy)?;

    // Migrate in place
    let outcome = service.migrate_file(&profile_path)?;
    assert!(outcome.was_migrated());
    assert!(outcome.backup_info.is_some());

    // Read back and verify it's now v1
    let migrated_content = std::fs::read_to_string(&profile_path)?;
    let value: serde_json::Value = serde_json::from_str(&migrated_content)?;
    assert_eq!(
        value.get("schema").and_then(|v| v.as_str()),
        Some(CURRENT_SCHEMA_VERSION)
    );

    // Verify backup was created
    let backup = outcome.backup_info.ok_or("expected backup info")?;
    assert!(backup.backup_path.exists());
    Ok(())
}

// ============================================================================
// 14. Edge cases
// ============================================================================

#[test]
fn edge_case_empty_store_operations() -> TestResult {
    let store = InMemoryProfileStore::new();
    assert!(store.is_empty());
    assert_eq!(store.len(), 0);

    let pid = make_profile_id("empty")?;
    assert!(store.get(&pid).is_none());

    let children = store.find_children(&pid);
    assert!(children.is_empty());

    let descendants = store.find_all_descendants(&pid);
    assert!(descendants.is_empty());
    Ok(())
}

#[test]
fn edge_case_profile_merge_with_self() -> TestResult {
    let profile = build_test_profile("self-merge")?;
    let merged = profile.merge_with(&profile);
    assert_eq!(merged.base_settings, profile.base_settings);
    Ok(())
}

#[test]
fn edge_case_profile_scope_all_none_matches_everything() -> TestResult {
    let scope = ProfileScope::global();
    assert!(scope.matches(None, None, None));
    assert!(scope.matches(Some("iracing"), None, None));
    assert!(scope.matches(Some("acc"), Some("gt3"), None));
    assert!(scope.matches(Some("acc"), Some("gt3"), Some("spa")));
    Ok(())
}

#[test]
fn edge_case_game_scope_doesnt_match_different_game() -> TestResult {
    let scope = ProfileScope::for_game("iracing".into());
    assert!(scope.matches(Some("iracing"), None, None));
    assert!(!scope.matches(Some("acc"), None, None));
    assert!(!scope.matches(None, None, None));
    Ok(())
}

#[test]
fn edge_case_migration_invalid_json() -> TestResult {
    let manager = test_migration_manager()?;
    let result = manager.migrate_profile("not json");
    assert!(result.is_err());
    Ok(())
}

#[test]
fn edge_case_migration_empty_object() -> TestResult {
    let manager = test_migration_manager()?;
    // An empty object has no schema field; detect_version may see it as
    // legacy format (no "schema" and no "base" fields) or fail
    let result = manager.detect_version("{}");
    // If detected as legacy v0, that's acceptable behavior; the key point
    // is it doesn't detect as current version
    if let Ok(v) = result {
        assert!(
            !v.is_current(),
            "empty object should not be current version"
        );
    }
    Ok(())
}

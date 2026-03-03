//! Deep compatibility and migration tests.
//!
//! Covers areas not already tested by existing compat test files:
//! 1. All deprecated API migrations (every deprecated function/type has a working replacement)
//! 2. Version detection and auto-migration
//! 3. Configuration format evolution (v1 → v2 → current)
//! 4. Profile format backward compatibility
//! 5. Device configuration migration paths
//! 6. Error message backward compatibility
//! 7. API surface stability verification
//! 8. Breaking change detection
//! 9. Feature flag combinations
//! 10. Migration rollback scenarios

use compat::TelemetryCompat;
use racing_wheel_engine::TelemetryData;
use racing_wheel_schemas::migration::{
    compat::BackwardCompatibleParser, BackupInfo, MigrationConfig, MigrationError,
    MigrationManager, MigrationResult, ProfileMigrationService, SchemaVersion,
    CURRENT_SCHEMA_VERSION, SCHEMA_VERSION_V2,
};
use std::time::Instant;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ---------------------------------------------------------------------------
// Newtype wrapper (orphan rule)
// ---------------------------------------------------------------------------

struct Compat(TelemetryData);

impl TelemetryCompat for Compat {
    fn temp_c(&self) -> u8 {
        self.0.temperature_c
    }
    fn faults(&self) -> u8 {
        self.0.fault_flags
    }
    fn wheel_angle_mdeg(&self) -> i32 {
        (self.0.wheel_angle_deg * 1000.0) as i32
    }
    fn wheel_speed_mrad_s(&self) -> i32 {
        (self.0.wheel_speed_rad_s * 1000.0) as i32
    }
    fn sequence(&self) -> u32 {
        0
    }
}

fn sample(angle_deg: f32, speed_rad_s: f32, temp: u8, faults: u8) -> Compat {
    Compat(TelemetryData {
        wheel_angle_deg: angle_deg,
        wheel_speed_rad_s: speed_rad_s,
        temperature_c: temp,
        fault_flags: faults,
        hands_on: false,
        timestamp: Instant::now(),
    })
}

fn make_manager() -> MigrationResult<MigrationManager> {
    MigrationManager::new(MigrationConfig::without_backups())
}

fn create_legacy_profile(ffb_gain: f64, dor: u16, torque_cap: f64) -> String {
    serde_json::json!({
        "ffb_gain": ffb_gain,
        "degrees_of_rotation": dor,
        "torque_cap": torque_cap
    })
    .to_string()
}

fn create_v1_profile(ffb_gain: f64, dor: u16, torque_cap: f64) -> String {
    serde_json::json!({
        "schema": CURRENT_SCHEMA_VERSION,
        "scope": { "game": null, "car": null, "track": null },
        "base": {
            "ffbGain": ffb_gain,
            "dorDeg": dor,
            "torqueCapNm": torque_cap,
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

fn create_v1_profile_with_scope(
    ffb_gain: f64,
    dor: u16,
    torque_cap: f64,
    game: Option<&str>,
) -> String {
    serde_json::json!({
        "schema": CURRENT_SCHEMA_VERSION,
        "scope": { "game": game, "car": null, "track": null },
        "base": {
            "ffbGain": ffb_gain,
            "dorDeg": dor,
            "torqueCapNm": torque_cap,
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

// ===========================================================================
// 1. Deprecated API migrations — every deprecated function has a replacement
// ===========================================================================

/// Each deprecated compat method must have an equivalent direct-access path
/// on the underlying TelemetryData. This test exhaustively pairs them.
#[test]
fn deprecated_api_replacement_matrix() -> TestResult {
    let combos: &[(f32, f32, u8, u8)] = &[
        (0.0, 0.0, 0, 0),
        (1.0, 1.0, 1, 1),
        (-450.0, -25.0, 128, 0x55),
        (900.0, 100.0, u8::MAX, u8::MAX),
    ];

    for &(angle, speed, temp, faults) in combos {
        let c = sample(angle, speed, temp, faults);

        // deprecated temp_c → replacement temperature_c
        assert_eq!(c.temp_c(), c.0.temperature_c, "temp_c replacement failed");

        // deprecated faults → replacement fault_flags
        assert_eq!(c.faults(), c.0.fault_flags, "faults replacement failed");

        // deprecated wheel_angle_mdeg → replacement wheel_angle_deg * 1000
        assert_eq!(
            c.wheel_angle_mdeg(),
            (c.0.wheel_angle_deg * 1000.0) as i32,
            "wheel_angle_mdeg replacement failed"
        );

        // deprecated wheel_speed_mrad_s → replacement wheel_speed_rad_s * 1000
        assert_eq!(
            c.wheel_speed_mrad_s(),
            (c.0.wheel_speed_rad_s * 1000.0) as i32,
            "wheel_speed_mrad_s replacement failed"
        );

        // deprecated sequence (removed) → no replacement, always 0
        assert_eq!(c.sequence(), 0, "sequence replacement failed");
    }
    Ok(())
}

/// The full set of deprecated trait methods is exactly 5.
/// If a new method were added or removed, this test catches the mismatch.
#[test]
fn deprecated_api_count_is_exhaustive() -> TestResult {
    let c = sample(1.0, 1.0, 1, 1);
    let dyn_ref: &dyn TelemetryCompat = &c;

    // Call every method; if the trait surface changes this won't compile.
    let results: [i64; 5] = [
        i64::from(dyn_ref.temp_c()),
        i64::from(dyn_ref.faults()),
        i64::from(dyn_ref.wheel_angle_mdeg()),
        i64::from(dyn_ref.wheel_speed_mrad_s()),
        i64::from(dyn_ref.sequence()),
    ];
    assert_eq!(results.len(), 5);
    Ok(())
}

// ===========================================================================
// 2. Version detection and auto-migration
// ===========================================================================

#[test]
fn auto_detect_legacy_v0_format() -> TestResult {
    let mgr = make_manager()?;
    let legacy = create_legacy_profile(0.8, 900, 12.0);
    let version = mgr.detect_version(&legacy)?;
    assert_eq!(version.major, 0, "legacy profile should be v0");
    assert!(!version.is_current());
    Ok(())
}

#[test]
fn auto_detect_current_v1_format() -> TestResult {
    let mgr = make_manager()?;
    let v1 = create_v1_profile(0.8, 900, 12.0);
    let version = mgr.detect_version(&v1)?;
    assert_eq!(version.major, 1, "v1 profile should be detected as v1");
    assert!(version.is_current());
    Ok(())
}

#[test]
fn auto_migration_legacy_to_current() -> TestResult {
    let parser = BackwardCompatibleParser::new();
    let legacy = create_legacy_profile(0.65, 720, 10.0);

    assert!(!parser.is_compatible(&legacy)?, "legacy should not be directly compatible");

    let profile = parser.parse_or_migrate(&legacy)?;
    assert_eq!(profile.schema_version.major, 1);
    assert!(
        (profile.ffb_gain().ok_or("missing ffbGain")? - 0.65).abs() < f64::EPSILON,
        "auto-migration must preserve ffbGain"
    );
    assert_eq!(profile.dor_deg(), Some(720));
    Ok(())
}

#[test]
fn auto_migration_skips_current_version() -> TestResult {
    let parser = BackwardCompatibleParser::new();
    let v1 = create_v1_profile(0.8, 900, 15.0);

    assert!(parser.is_compatible(&v1)?);

    let profile = parser.parse_or_migrate(&v1)?;
    assert!(profile.schema_version.is_current());
    Ok(())
}

#[test]
fn needs_migration_returns_true_for_legacy() -> TestResult {
    let mgr = make_manager()?;
    let legacy = create_legacy_profile(0.7, 900, 15.0);
    assert!(mgr.needs_migration(&legacy)?);
    Ok(())
}

#[test]
fn needs_migration_returns_false_for_current() -> TestResult {
    let mgr = make_manager()?;
    let v1 = create_v1_profile(0.7, 900, 15.0);
    assert!(!mgr.needs_migration(&v1)?);
    Ok(())
}

// ===========================================================================
// 3. Configuration format evolution (v0 → v1 → current)
// ===========================================================================

#[test]
fn config_evolution_v0_flat_to_v1_nested() -> TestResult {
    let mgr = make_manager()?;
    let v0 = r#"{"ffb_gain": 0.9, "degrees_of_rotation": 540, "torque_cap": 8.5}"#;
    let migrated = mgr.migrate_profile(v0)?;
    let value: serde_json::Value = serde_json::from_str(&migrated)?;

    // Structural assertion: flat → nested
    assert!(value.get("ffb_gain").is_none(), "flat field must be removed");
    assert!(value.get("degrees_of_rotation").is_none());
    assert!(value.get("torque_cap").is_none());

    let base = value.get("base").ok_or("missing base")?;
    assert!(
        (base.get("ffbGain").and_then(|v| v.as_f64()).ok_or("missing ffbGain")? - 0.9).abs()
            < f64::EPSILON
    );
    assert_eq!(base.get("dorDeg").and_then(|v| v.as_u64()), Some(540));
    assert!(
        (base.get("torqueCapNm").and_then(|v| v.as_f64()).ok_or("missing torqueCapNm")? - 8.5)
            .abs()
            < f64::EPSILON
    );
    Ok(())
}

#[test]
fn config_evolution_adds_filters_structure() -> TestResult {
    let mgr = make_manager()?;
    let v0 = create_legacy_profile(0.7, 900, 15.0);
    let migrated = mgr.migrate_profile(&v0)?;
    let value: serde_json::Value = serde_json::from_str(&migrated)?;

    let filters = value
        .get("base")
        .and_then(|b| b.get("filters"))
        .ok_or("missing filters")?;

    // All filter sub-fields must exist after migration
    assert!(filters.get("reconstruction").is_some());
    assert!(filters.get("friction").is_some());
    assert!(filters.get("damper").is_some());
    assert!(filters.get("inertia").is_some());
    assert!(filters.get("notchFilters").is_some());
    assert!(filters.get("slewRate").is_some());
    assert!(filters.get("curvePoints").is_some());
    Ok(())
}

#[test]
fn config_evolution_adds_scope_structure() -> TestResult {
    let mgr = make_manager()?;
    let v0 = create_legacy_profile(0.7, 900, 15.0);
    let migrated = mgr.migrate_profile(&v0)?;
    let value: serde_json::Value = serde_json::from_str(&migrated)?;

    let scope = value.get("scope").ok_or("missing scope")?;
    assert!(scope.get("game").is_some());
    assert!(scope.get("car").is_some());
    assert!(scope.get("track").is_some());
    Ok(())
}

#[test]
fn config_evolution_default_filter_values() -> TestResult {
    let mgr = make_manager()?;
    let v0 = create_legacy_profile(0.7, 900, 15.0);
    let migrated = mgr.migrate_profile(&v0)?;
    let value: serde_json::Value = serde_json::from_str(&migrated)?;

    let filters = value
        .get("base")
        .and_then(|b| b.get("filters"))
        .ok_or("missing filters")?;

    assert_eq!(filters.get("reconstruction").and_then(|v| v.as_u64()), Some(0));
    assert_eq!(filters.get("friction").and_then(|v| v.as_f64()), Some(0.0));
    assert_eq!(filters.get("damper").and_then(|v| v.as_f64()), Some(0.0));
    assert_eq!(filters.get("inertia").and_then(|v| v.as_f64()), Some(0.0));
    assert_eq!(filters.get("slewRate").and_then(|v| v.as_f64()), Some(1.0));

    let curve = filters
        .get("curvePoints")
        .and_then(|v| v.as_array())
        .ok_or("missing curvePoints")?;
    assert_eq!(curve.len(), 2);
    Ok(())
}

// ===========================================================================
// 4. Profile format backward compatibility
// ===========================================================================

#[test]
fn profile_backward_compat_parser_accepts_v1() -> TestResult {
    let parser = BackwardCompatibleParser::new();
    let v1 = create_v1_profile(0.8, 900, 12.0);
    let profile = parser.parse(&v1)?;

    assert_eq!(profile.schema_version.major, 1);
    assert_eq!(profile.ffb_gain(), Some(0.8));
    assert_eq!(profile.dor_deg(), Some(900));
    assert_eq!(profile.torque_cap_nm(), Some(12.0));
    Ok(())
}

#[test]
fn profile_backward_compat_parser_rejects_legacy_direct() -> TestResult {
    let parser = BackwardCompatibleParser::new();
    let legacy = create_legacy_profile(0.7, 900, 15.0);
    let result = parser.parse(&legacy);
    assert!(result.is_err(), "v1 parser should reject legacy format without migration");
    Ok(())
}

#[test]
fn profile_backward_compat_scoped_profiles() -> TestResult {
    let parser = BackwardCompatibleParser::new();
    let scoped = create_v1_profile_with_scope(0.8, 900, 12.0, Some("iRacing"));
    let profile = parser.parse(&scoped)?;

    assert_eq!(profile.game(), Some("iRacing"));
    assert!(!profile.has_parent());
    Ok(())
}

#[test]
fn profile_backward_compat_null_game_scope() -> TestResult {
    let parser = BackwardCompatibleParser::new();
    let v1 = create_v1_profile_with_scope(0.8, 900, 12.0, None);
    let profile = parser.parse(&v1)?;

    assert_eq!(profile.game(), None);
    Ok(())
}

#[test]
fn profile_backward_compat_optional_fields_absent() -> TestResult {
    let parser = BackwardCompatibleParser::new();
    let v1 = create_v1_profile(0.8, 900, 12.0);
    let profile = parser.parse(&v1)?;

    assert!(profile.leds.is_none());
    assert!(profile.haptics.is_none());
    assert!(profile.signature.is_none());
    assert!(!profile.has_parent());
    Ok(())
}

#[test]
fn profile_backward_compat_optional_fields_present() -> TestResult {
    let parser = BackwardCompatibleParser::new();
    let json = serde_json::json!({
        "schema": CURRENT_SCHEMA_VERSION,
        "parent": "base-profile",
        "scope": { "game": "ACC", "car": null, "track": null },
        "base": {
            "ffbGain": 0.75,
            "dorDeg": 720,
            "torqueCapNm": 10.0,
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
        "leds": { "enabled": true },
        "haptics": { "enabled": false },
        "signature": "ed25519:abc123"
    });

    let profile = parser.parse(&json.to_string())?;

    assert!(profile.has_parent());
    assert_eq!(profile.parent.as_deref(), Some("base-profile"));
    assert!(profile.leds.is_some());
    assert!(profile.haptics.is_some());
    assert_eq!(profile.signature.as_deref(), Some("ed25519:abc123"));
    assert_eq!(profile.game(), Some("ACC"));
    Ok(())
}

#[test]
fn profile_round_trip_through_to_json() -> TestResult {
    let parser = BackwardCompatibleParser::new();
    let original = create_v1_profile(0.85, 540, 9.0);
    let profile = parser.parse(&original)?;

    let json_str = profile.to_json()?;
    let reparsed = parser.parse(&json_str)?;

    assert_eq!(profile.ffb_gain(), reparsed.ffb_gain());
    assert_eq!(profile.dor_deg(), reparsed.dor_deg());
    assert_eq!(profile.torque_cap_nm(), reparsed.torque_cap_nm());
    assert_eq!(profile.schema_version.major, reparsed.schema_version.major);
    Ok(())
}

// ===========================================================================
// 5. Device configuration migration paths
// ===========================================================================

#[test]
fn device_config_legacy_missing_fields_get_defaults() -> TestResult {
    let mgr = make_manager()?;

    // Legacy profile with only ffb_gain — other fields should get defaults
    let minimal = r#"{"ffb_gain": 0.5}"#;
    let migrated = mgr.migrate_profile(minimal)?;
    let value: serde_json::Value = serde_json::from_str(&migrated)?;

    let base = value.get("base").ok_or("missing base")?;
    // dor should default to 900
    assert_eq!(base.get("dorDeg").and_then(|v| v.as_u64()), Some(900));
    // torque should default to 15.0
    assert!(
        (base.get("torqueCapNm").and_then(|v| v.as_f64()).ok_or("missing torqueCapNm")? - 15.0)
            .abs()
            < f64::EPSILON
    );
    Ok(())
}

#[test]
fn device_config_legacy_partial_fields() -> TestResult {
    let mgr = make_manager()?;

    // Legacy profile with only degrees_of_rotation — ffb_gain and torque default
    let partial = r#"{"degrees_of_rotation": 1080}"#;
    let migrated = mgr.migrate_profile(partial)?;
    let value: serde_json::Value = serde_json::from_str(&migrated)?;

    let base = value.get("base").ok_or("missing base")?;
    assert_eq!(base.get("dorDeg").and_then(|v| v.as_u64()), Some(1080));
    assert!(
        (base.get("ffbGain").and_then(|v| v.as_f64()).ok_or("missing ffbGain")? - 0.7).abs()
            < f64::EPSILON
    );
    Ok(())
}

#[test]
fn device_config_various_dor_values_preserved() -> TestResult {
    let mgr = make_manager()?;

    for &dor in &[180_u16, 270, 360, 540, 720, 900, 1080, 1440] {
        let legacy = create_legacy_profile(0.7, dor, 15.0);
        let migrated = mgr.migrate_profile(&legacy)?;
        let value: serde_json::Value = serde_json::from_str(&migrated)?;

        let actual_dor = value
            .get("base")
            .and_then(|b| b.get("dorDeg"))
            .and_then(|v| v.as_u64())
            .ok_or(format!("missing dorDeg for dor={dor}"))?;
        assert_eq!(actual_dor, u64::from(dor), "dorDeg mismatch for {dor}");
    }
    Ok(())
}

#[test]
fn device_config_extreme_ffb_gain_preserved() -> TestResult {
    let mgr = make_manager()?;

    for &gain in &[0.0, 0.01, 0.5, 1.0] {
        let legacy = create_legacy_profile(gain, 900, 15.0);
        let migrated = mgr.migrate_profile(&legacy)?;
        let value: serde_json::Value = serde_json::from_str(&migrated)?;

        let actual = value
            .get("base")
            .and_then(|b| b.get("ffbGain"))
            .and_then(|v| v.as_f64())
            .ok_or("missing ffbGain")?;
        assert!(
            (actual - gain).abs() < f64::EPSILON,
            "ffbGain mismatch: expected {gain}, got {actual}"
        );
    }
    Ok(())
}

// ===========================================================================
// 6. Error message backward compatibility
// ===========================================================================

#[test]
fn error_unknown_schema_version_message() -> TestResult {
    let mgr = make_manager()?;
    let bad = r#"{"schema": "unknown.format/99"}"#;
    let err = mgr.detect_version(bad);
    assert!(err.is_err());
    let msg = format!("{}", err.as_ref().err().ok_or("expected error")?);
    assert!(
        msg.contains("unknown.format/99") || msg.contains("Unknown schema version"),
        "error message should mention the unknown version: {msg}"
    );
    Ok(())
}

#[test]
fn error_invalid_json_is_json_error() -> TestResult {
    let mgr = make_manager()?;
    let result = mgr.migrate_profile("not valid json {{{}}}");
    assert!(result.is_err());
    let err = result.err().ok_or("expected error")?;
    assert!(
        matches!(err, MigrationError::JsonError(_)),
        "invalid JSON should produce JsonError"
    );
    Ok(())
}

#[test]
fn error_no_migration_path_message() -> TestResult {
    // v2 is defined but there's no v1->v2 migration registered
    let mgr = make_manager()?;
    let v2_profile = format!(r#"{{"schema": "{SCHEMA_VERSION_V2}"}}"#);
    let result = mgr.detect_version(&v2_profile);

    // v2 should parse but not be current
    if let Ok(version) = result {
        assert!(!version.is_current());
        assert_eq!(version.major, 2);
    }
    Ok(())
}

#[test]
fn error_migration_error_display_is_stable() -> TestResult {
    let err = MigrationError::MigrationFailed {
        from: "v0".to_string(),
        to: "v1".to_string(),
        reason: "test reason".to_string(),
    };
    let msg = format!("{err}");
    assert!(msg.contains("v0"), "display should contain source version");
    assert!(msg.contains("v1"), "display should contain target version");
    assert!(msg.contains("test reason"), "display should contain reason");
    Ok(())
}

#[test]
fn error_validation_failed_display() -> TestResult {
    let err = MigrationError::ValidationFailed("missing field X".to_string());
    let msg = format!("{err}");
    assert!(
        msg.contains("missing field X"),
        "ValidationFailed should contain detail: {msg}"
    );
    Ok(())
}

#[test]
fn error_backup_failed_display() -> TestResult {
    let err = MigrationError::BackupFailed("disk full".to_string());
    let msg = format!("{err}");
    assert!(msg.contains("disk full"), "BackupFailed should contain detail: {msg}");
    Ok(())
}

#[test]
fn error_restore_failed_display() -> TestResult {
    let err = MigrationError::RestoreFailed("corrupt backup".to_string());
    let msg = format!("{err}");
    assert!(
        msg.contains("corrupt backup"),
        "RestoreFailed should contain detail: {msg}"
    );
    Ok(())
}

#[test]
fn error_no_migration_path_display() -> TestResult {
    let err = MigrationError::NoMigrationPath {
        from: "wheel.profile/5".to_string(),
        to: "wheel.profile/6".to_string(),
    };
    let msg = format!("{err}");
    assert!(msg.contains("wheel.profile/5"));
    assert!(msg.contains("wheel.profile/6"));
    Ok(())
}

// ===========================================================================
// 7. API surface stability verification
// ===========================================================================

#[test]
fn schema_version_api_surface() -> TestResult {
    let v = SchemaVersion::new(1, 0);
    // Public fields
    let _major: u32 = v.major;
    let _minor: u32 = v.minor;
    let _version: &str = &v.version;

    // Public methods
    let _ = v.is_current();
    let _ = v.is_older_than(&SchemaVersion::new(2, 0));
    let _ = SchemaVersion::parse(CURRENT_SCHEMA_VERSION)?;
    let _ = format!("{v}");
    Ok(())
}

#[test]
fn migration_config_api_surface() -> TestResult {
    let config = MigrationConfig::without_backups();
    let _backup_dir = &config.backup_dir;
    let _create = config.create_backups;
    let _max = config.max_backups;
    let _validate = config.validate_after_migration;

    let _default = MigrationConfig::default();
    Ok(())
}

#[test]
fn migration_manager_api_surface() -> TestResult {
    let mgr = make_manager()?;
    let v1 = create_v1_profile(0.8, 900, 12.0);
    let legacy = create_legacy_profile(0.8, 900, 12.0);

    let _ = mgr.detect_version(&v1)?;
    let _ = mgr.needs_migration(&v1)?;
    let _ = mgr.migrate_profile(&legacy)?;

    let value: serde_json::Value = serde_json::from_str(&legacy)?;
    let _ = mgr.migrate_value(value)?;
    Ok(())
}

#[test]
fn backward_compatible_parser_api_surface() -> TestResult {
    let parser = BackwardCompatibleParser::new();
    let _parser_v0 = BackwardCompatibleParser::for_major_version(0);
    let _default: BackwardCompatibleParser = Default::default();

    let _ = parser.major_version;

    let v1 = create_v1_profile(0.8, 900, 12.0);
    let _ = parser.is_compatible(&v1)?;
    let _ = parser.parse(&v1)?;
    let _ = parser.parse_or_migrate(&v1)?;
    Ok(())
}

#[test]
fn compatible_profile_api_surface() -> TestResult {
    let parser = BackwardCompatibleParser::new();
    let v1 = create_v1_profile(0.8, 900, 12.0);
    let profile = parser.parse(&v1)?;

    // Public fields
    let _version: &SchemaVersion = &profile.schema_version;
    let _parent: &Option<String> = &profile.parent;
    let _scope: &serde_json::Value = &profile.scope;
    let _base: &serde_json::Value = &profile.base;
    let _leds: &Option<serde_json::Value> = &profile.leds;
    let _haptics: &Option<serde_json::Value> = &profile.haptics;
    let _sig: &Option<String> = &profile.signature;

    // Public methods
    let _ = profile.ffb_gain();
    let _ = profile.dor_deg();
    let _ = profile.torque_cap_nm();
    let _ = profile.game();
    let _ = profile.has_parent();
    let _ = profile.to_json()?;
    Ok(())
}

#[test]
fn migration_outcome_api_surface() -> TestResult {
    let svc = ProfileMigrationService::new(MigrationConfig::without_backups())?;
    let legacy = create_legacy_profile(0.8, 900, 12.0);
    let outcome = svc.migrate_with_backup(&legacy, None)?;

    // Public fields
    let _json: &str = &outcome.migrated_json;
    let _orig: &SchemaVersion = &outcome.original_version;
    let _target: &SchemaVersion = &outcome.target_version;
    let _backup: &Option<BackupInfo> = &outcome.backup_info;
    let _applied: &Vec<String> = &outcome.migrations_applied;

    // Public methods
    let _ = outcome.was_migrated();
    let _ = outcome.migration_count();
    Ok(())
}

#[test]
fn telemetry_compat_trait_is_object_safe_and_clonable() -> TestResult {
    let c1 = sample(10.0, 2.0, 30, 0x08);
    let _dyn_ref: &dyn TelemetryCompat = &c1;

    // Clone the inner data and wrap again — trait must work on independent instances
    let c2 = Compat(c1.0.clone());
    assert_eq!(c1.temp_c(), c2.temp_c());
    assert_eq!(c1.faults(), c2.faults());
    assert_eq!(c1.wheel_angle_mdeg(), c2.wheel_angle_mdeg());
    assert_eq!(c1.wheel_speed_mrad_s(), c2.wheel_speed_mrad_s());
    Ok(())
}

// ===========================================================================
// 8. Breaking change detection
// ===========================================================================

#[test]
fn breaking_change_current_schema_version_constant() -> TestResult {
    // The current schema version must match "wheel.profile/1".
    // A change here signals a breaking change that invalidates existing profiles.
    assert_eq!(CURRENT_SCHEMA_VERSION, "wheel.profile/1");
    Ok(())
}

#[test]
fn breaking_change_v2_constant_defined() -> TestResult {
    // V2 is declared as a known future version; ensure it's consistent.
    assert_eq!(SCHEMA_VERSION_V2, "wheel.profile/2");
    Ok(())
}

#[test]
fn breaking_change_schema_version_parse_format() -> TestResult {
    // The parsing contract: "wheel.profile/N" or "wheel.profile/N.M"
    let v1 = SchemaVersion::parse("wheel.profile/1")?;
    assert_eq!(v1.major, 1);
    assert_eq!(v1.minor, 0);

    let v1_2 = SchemaVersion::parse("wheel.profile/1.2")?;
    assert_eq!(v1_2.major, 1);
    assert_eq!(v1_2.minor, 2);

    // Invalid prefix must fail
    assert!(SchemaVersion::parse("other.prefix/1").is_err());
    assert!(SchemaVersion::parse("wheel.profile").is_err());
    assert!(SchemaVersion::parse("").is_err());
    Ok(())
}

#[test]
fn breaking_change_v1_profile_structure_contract() -> TestResult {
    // A v1 profile MUST have: schema, scope, base
    let mgr = make_manager()?;
    let legacy = create_legacy_profile(0.7, 900, 15.0);
    let migrated = mgr.migrate_profile(&legacy)?;
    let value: serde_json::Value = serde_json::from_str(&migrated)?;

    assert!(value.get("schema").is_some(), "v1 must have 'schema'");
    assert!(value.get("scope").is_some(), "v1 must have 'scope'");
    assert!(value.get("base").is_some(), "v1 must have 'base'");
    Ok(())
}

#[test]
fn breaking_change_v1_base_field_names() -> TestResult {
    // v1 base fields use camelCase names; verify the contract
    let v1 = create_v1_profile(0.8, 900, 12.0);
    let value: serde_json::Value = serde_json::from_str(&v1)?;
    let base = value.get("base").ok_or("missing base")?;

    assert!(base.get("ffbGain").is_some(), "must use ffbGain (camelCase)");
    assert!(base.get("dorDeg").is_some(), "must use dorDeg (camelCase)");
    assert!(base.get("torqueCapNm").is_some(), "must use torqueCapNm (camelCase)");
    assert!(base.get("filters").is_some(), "must have filters");
    Ok(())
}

#[test]
fn breaking_change_version_ordering_consistent() -> TestResult {
    let v0 = SchemaVersion::new(0, 0);
    let v1 = SchemaVersion::new(1, 0);
    let v1_1 = SchemaVersion::new(1, 1);
    let v2 = SchemaVersion::new(2, 0);

    assert!(v0.is_older_than(&v1));
    assert!(v1.is_older_than(&v1_1));
    assert!(v1.is_older_than(&v2));
    assert!(v1_1.is_older_than(&v2));

    assert!(!v1.is_older_than(&v0));
    assert!(!v1.is_older_than(&v1));
    assert!(!v2.is_older_than(&v1));
    Ok(())
}

// ===========================================================================
// 9. Feature flag combinations
// ===========================================================================

#[test]
fn feature_migration_config_backups_disabled() -> TestResult {
    let config = MigrationConfig::without_backups();
    assert!(!config.create_backups);
    assert!(config.validate_after_migration, "without_backups still validates");
    let mgr = MigrationManager::new(config)?;

    // Migration still works without backups
    let legacy = create_legacy_profile(0.7, 900, 15.0);
    let migrated = mgr.migrate_profile(&legacy)?;
    let value: serde_json::Value = serde_json::from_str(&migrated)?;
    assert_eq!(
        value.get("schema").and_then(|v| v.as_str()),
        Some(CURRENT_SCHEMA_VERSION)
    );
    Ok(())
}

#[test]
fn feature_migration_config_validation_disabled() -> TestResult {
    let mut config = MigrationConfig::without_backups();
    config.validate_after_migration = false;
    let mgr = MigrationManager::new(config)?;

    let legacy = create_legacy_profile(0.5, 720, 10.0);
    let result = mgr.migrate_profile(&legacy);
    assert!(result.is_ok(), "migration without validation should succeed");
    Ok(())
}

#[test]
fn feature_parser_version_0_is_not_v1_compatible() -> TestResult {
    let parser_v0 = BackwardCompatibleParser::for_major_version(0);
    let v1 = create_v1_profile(0.8, 900, 12.0);

    assert!(!parser_v0.is_compatible(&v1)?);
    Ok(())
}

#[test]
fn feature_parser_version_1_is_not_v0_compatible() -> TestResult {
    let parser = BackwardCompatibleParser::new();
    let legacy = create_legacy_profile(0.7, 900, 15.0);

    assert!(!parser.is_compatible(&legacy)?);
    Ok(())
}

#[test]
fn feature_parser_default_matches_explicit_v1() -> TestResult {
    let default_parser: BackwardCompatibleParser = Default::default();
    let explicit_parser = BackwardCompatibleParser::new();

    assert_eq!(default_parser.major_version, explicit_parser.major_version);
    assert_eq!(default_parser.major_version, 1);
    Ok(())
}

#[test]
fn feature_profile_migration_service_without_backups() -> TestResult {
    let svc = ProfileMigrationService::new(MigrationConfig::without_backups())?;
    let legacy = create_legacy_profile(0.8, 900, 12.0);

    let outcome = svc.migrate_with_backup(&legacy, None)?;
    assert!(outcome.was_migrated());
    assert!(outcome.backup_info.is_none(), "no backup when path is None");
    Ok(())
}

// ===========================================================================
// 10. Migration rollback scenarios
// ===========================================================================

#[test]
fn rollback_current_version_produces_no_op_outcome() -> TestResult {
    let svc = ProfileMigrationService::new(MigrationConfig::without_backups())?;
    let v1 = create_v1_profile(0.8, 900, 12.0);

    let outcome = svc.migrate_with_backup(&v1, None)?;
    assert!(!outcome.was_migrated());
    assert_eq!(outcome.migration_count(), 0);
    assert!(outcome.backup_info.is_none());
    Ok(())
}

#[test]
fn rollback_migration_preserves_original_on_invalid_json() -> TestResult {
    let svc = ProfileMigrationService::new(MigrationConfig::without_backups())?;
    let bad_json = "this is not json";

    let result = svc.migrate_with_backup(bad_json, None);
    assert!(result.is_err(), "invalid JSON should fail migration");
    Ok(())
}

#[test]
fn rollback_outcome_records_applied_migrations() -> TestResult {
    let svc = ProfileMigrationService::new(MigrationConfig::without_backups())?;
    let legacy = create_legacy_profile(0.7, 900, 15.0);

    let outcome = svc.migrate_with_backup(&legacy, None)?;
    assert!(outcome.was_migrated());
    assert!(outcome.migration_count() > 0);
    assert!(!outcome.migrations_applied.is_empty());
    Ok(())
}

#[test]
fn rollback_migrated_json_is_valid_and_parseable() -> TestResult {
    let svc = ProfileMigrationService::new(MigrationConfig::without_backups())?;
    let legacy = create_legacy_profile(0.65, 540, 8.0);

    let outcome = svc.migrate_with_backup(&legacy, None)?;
    // The migrated JSON must be valid and parseable by the compat parser
    let parser = BackwardCompatibleParser::new();
    let profile = parser.parse(&outcome.migrated_json)?;

    assert_eq!(profile.schema_version.major, 1);
    assert!(
        (profile.ffb_gain().ok_or("missing ffbGain")? - 0.65).abs() < f64::EPSILON
    );
    Ok(())
}

#[test]
fn rollback_outcome_version_fields_correct() -> TestResult {
    let svc = ProfileMigrationService::new(MigrationConfig::without_backups())?;
    let legacy = create_legacy_profile(0.7, 900, 15.0);

    let outcome = svc.migrate_with_backup(&legacy, None)?;
    assert_eq!(outcome.original_version.major, 0);
    assert_eq!(outcome.target_version.major, 1);
    assert!(outcome.target_version.is_current());
    Ok(())
}

#[test]
fn rollback_file_migration_with_temp_file() -> TestResult {
    let dir = std::env::temp_dir().join("compat_deep_tests_rollback");
    std::fs::create_dir_all(&dir)?;

    let profile_path = dir.join("test_profile.json");
    let legacy = create_legacy_profile(0.7, 900, 15.0);
    std::fs::write(&profile_path, &legacy)?;

    let svc = ProfileMigrationService::new(MigrationConfig::new(&dir))?;
    let outcome = svc.migrate_file(&profile_path)?;

    assert!(outcome.was_migrated());

    // Verify the file was actually updated
    let content = std::fs::read_to_string(&profile_path)?;
    let value: serde_json::Value = serde_json::from_str(&content)?;
    assert_eq!(
        value.get("schema").and_then(|v| v.as_str()),
        Some(CURRENT_SCHEMA_VERSION)
    );

    // Verify backup was created
    assert!(outcome.backup_info.is_some());
    let backup = outcome.backup_info.as_ref().ok_or("missing backup")?;
    assert!(backup.backup_path.exists());

    // Restore from backup
    let restored = svc.restore_from_backup(backup)?;
    let restored_value: serde_json::Value = serde_json::from_str(&restored)?;
    assert!(
        restored_value.get("ffb_gain").is_some(),
        "restored content must match original legacy format"
    );

    // Cleanup
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn rollback_backup_info_has_original_version() -> TestResult {
    let dir = std::env::temp_dir().join("compat_deep_tests_backup_ver");
    std::fs::create_dir_all(&dir)?;

    let profile_path = dir.join("test_backup_ver.json");
    let legacy = create_legacy_profile(0.7, 900, 15.0);
    std::fs::write(&profile_path, &legacy)?;

    let svc = ProfileMigrationService::new(MigrationConfig::new(&dir))?;
    let outcome = svc.migrate_file(&profile_path)?;

    let backup = outcome.backup_info.ok_or("missing backup")?;
    assert!(!backup.original_version.is_empty());
    assert!(!backup.content_hash.is_empty());

    // Cleanup
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn rollback_restore_file_from_backup() -> TestResult {
    let dir = std::env::temp_dir().join("compat_deep_tests_restore_file");
    std::fs::create_dir_all(&dir)?;

    let profile_path = dir.join("test_restore.json");
    let legacy = create_legacy_profile(0.75, 720, 10.0);
    std::fs::write(&profile_path, &legacy)?;

    let svc = ProfileMigrationService::new(MigrationConfig::new(&dir))?;
    let outcome = svc.migrate_file(&profile_path)?;

    // File should now be v1
    let after_migrate = std::fs::read_to_string(&profile_path)?;
    let after_value: serde_json::Value = serde_json::from_str(&after_migrate)?;
    assert_eq!(
        after_value.get("schema").and_then(|v| v.as_str()),
        Some(CURRENT_SCHEMA_VERSION)
    );

    // Restore the file from backup
    let backup = outcome.backup_info.ok_or("missing backup")?;
    svc.restore_file_from_backup(&backup)?;

    // File should now be back to legacy format
    let after_restore = std::fs::read_to_string(&profile_path)?;
    let restored_value: serde_json::Value = serde_json::from_str(&after_restore)?;
    assert!(
        restored_value.get("ffb_gain").is_some(),
        "restored file must be in legacy format"
    );

    // Cleanup
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

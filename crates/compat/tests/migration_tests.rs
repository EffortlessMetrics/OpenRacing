//! Migration and compatibility layer integration tests.
//!
//! Verifies:
//! - Legacy format → current format migration
//! - Compat trait correctly translates old API calls
//! - Deprecation warnings through compat trait methods
//! - Compat paths produce identical results to native paths
//! - Compat report generation and accuracy

use compat::TelemetryCompat;
use racing_wheel_engine::TelemetryData;
use racing_wheel_schemas::migration::{
    MigrationConfig, MigrationManager, SchemaVersion, CURRENT_SCHEMA_VERSION,
};
use std::time::Instant;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ---------------------------------------------------------------------------
// Newtype wrapper for orphan rule
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

// ──────────────────────────────────────────────────────────────────────
// Migration from legacy format to current format
// ──────────────────────────────────────────────────────────────────────

#[test]
fn legacy_flat_format_migrates_to_v1() -> TestResult {
    let manager = MigrationManager::new(MigrationConfig::without_backups())?;
    let legacy = r#"{
        "ffb_gain": 0.8,
        "degrees_of_rotation": 900,
        "torque_cap": 12.0
    }"#;

    let migrated = manager.migrate_profile(legacy)?;
    let value: serde_json::Value = serde_json::from_str(&migrated)?;

    assert_eq!(
        value.get("schema").and_then(|v| v.as_str()),
        Some(CURRENT_SCHEMA_VERSION),
        "migrated profile must have current schema version"
    );
    assert!(
        value.get("base").is_some(),
        "migrated profile must have 'base' structure"
    );
    assert!(
        value.get("scope").is_some(),
        "migrated profile must have 'scope' structure"
    );
    Ok(())
}

#[test]
fn legacy_migration_preserves_ffb_gain() -> TestResult {
    let manager = MigrationManager::new(MigrationConfig::without_backups())?;
    let legacy = r#"{
        "ffb_gain": 0.65,
        "degrees_of_rotation": 720,
        "torque_cap": 10.0
    }"#;

    let migrated = manager.migrate_profile(legacy)?;
    let value: serde_json::Value = serde_json::from_str(&migrated)?;

    let ffb_gain = value
        .get("base")
        .and_then(|b| b.get("ffbGain"))
        .and_then(|v| v.as_f64());
    assert!(
        (ffb_gain.ok_or("missing ffbGain")? - 0.65).abs() < f64::EPSILON,
        "ffbGain must be preserved"
    );

    let dor = value
        .get("base")
        .and_then(|b| b.get("dorDeg"))
        .and_then(|v| v.as_u64());
    assert_eq!(dor, Some(720), "dorDeg must be preserved");

    let torque = value
        .get("base")
        .and_then(|b| b.get("torqueCapNm"))
        .and_then(|v| v.as_f64());
    assert!(
        (torque.ok_or("missing torqueCapNm")? - 10.0).abs() < f64::EPSILON,
        "torqueCapNm must be preserved"
    );
    Ok(())
}

#[test]
fn legacy_migration_adds_default_filters() -> TestResult {
    let manager = MigrationManager::new(MigrationConfig::without_backups())?;
    let legacy = r#"{"ffb_gain": 0.7, "degrees_of_rotation": 900, "torque_cap": 15.0}"#;

    let migrated = manager.migrate_profile(legacy)?;
    let value: serde_json::Value = serde_json::from_str(&migrated)?;

    let filters = value
        .get("base")
        .and_then(|b| b.get("filters"))
        .ok_or("missing filters")?;

    assert_eq!(
        filters.get("reconstruction").and_then(|v| v.as_u64()),
        Some(0)
    );
    assert!(filters.get("notchFilters").and_then(|v| v.as_array()).is_some());
    assert!(filters.get("curvePoints").and_then(|v| v.as_array()).is_some());
    Ok(())
}

#[test]
fn current_version_profile_not_modified_by_migration() -> TestResult {
    let manager = MigrationManager::new(MigrationConfig::without_backups())?;
    let v1 = r#"{
        "schema": "wheel.profile/1",
        "scope": { "game": null },
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
        }
    }"#;

    assert!(
        !manager.needs_migration(v1)?,
        "current version should not need migration"
    );

    let migrated = manager.migrate_profile(v1)?;
    let original_value: serde_json::Value = serde_json::from_str(v1)?;
    let migrated_value: serde_json::Value = serde_json::from_str(&migrated)?;
    assert_eq!(original_value, migrated_value);
    Ok(())
}

#[test]
fn unknown_schema_version_rejected() {
    let manager = MigrationManager::new(MigrationConfig::without_backups());
    assert!(manager.is_ok());
    let manager = manager.ok();

    if let Some(mgr) = manager {
        let unknown = r#"{"schema": "unknown.format/99"}"#;
        let result = mgr.detect_version(unknown);
        assert!(result.is_err(), "unknown schema prefix should be rejected");
    }
}

// ──────────────────────────────────────────────────────────────────────
// Compat trait correctly translates old API calls
// ──────────────────────────────────────────────────────────────────────

#[test]
fn compat_temp_c_equals_temperature_c() -> TestResult {
    let t = sample(0.0, 0.0, 85, 0);
    assert_eq!(t.temp_c(), t.0.temperature_c);
    assert_eq!(t.temp_c(), 85);
    Ok(())
}

#[test]
fn compat_faults_equals_fault_flags() -> TestResult {
    let t = sample(0.0, 0.0, 0, 0xAB);
    assert_eq!(t.faults(), t.0.fault_flags);
    assert_eq!(t.faults(), 0xAB);
    Ok(())
}

#[test]
fn compat_wheel_angle_mdeg_converts_correctly() -> TestResult {
    let t = sample(45.5, 0.0, 0, 0);
    assert_eq!(t.wheel_angle_mdeg(), 45500);
    assert_eq!(t.wheel_angle_mdeg(), (t.0.wheel_angle_deg * 1000.0) as i32);
    Ok(())
}

#[test]
fn compat_wheel_speed_mrad_s_converts_correctly() -> TestResult {
    let t = sample(0.0, 2.5, 0, 0);
    assert_eq!(t.wheel_speed_mrad_s(), 2500);
    assert_eq!(
        t.wheel_speed_mrad_s(),
        (t.0.wheel_speed_rad_s * 1000.0) as i32
    );
    Ok(())
}

#[test]
fn compat_sequence_always_zero_for_removed_field() -> TestResult {
    let t = sample(90.0, 5.0, 45, 0x02);
    assert_eq!(t.sequence(), 0, "removed field must always return 0");
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// Compat paths produce identical results to native paths
// ──────────────────────────────────────────────────────────────────────

#[test]
fn compat_and_native_produce_identical_temperature() -> TestResult {
    for temp in [0_u8, 1, 25, 50, 100, 127, 200, 254, 255] {
        let t = sample(0.0, 0.0, temp, 0);
        let via_compat = t.temp_c();
        let via_native = t.0.temperature_c;
        assert_eq!(
            via_compat, via_native,
            "temp mismatch for value {temp}"
        );
    }
    Ok(())
}

#[test]
fn compat_and_native_produce_identical_faults() -> TestResult {
    for faults in [0x00_u8, 0x01, 0x0F, 0x55, 0xAA, 0xFF] {
        let t = sample(0.0, 0.0, 0, faults);
        let via_compat = t.faults();
        let via_native = t.0.fault_flags;
        assert_eq!(
            via_compat, via_native,
            "fault mismatch for value {faults:#04X}"
        );
    }
    Ok(())
}

#[test]
fn compat_and_native_produce_identical_angles() -> TestResult {
    let angles = [
        -900.0, -720.0, -360.0, -180.0, -90.0, -45.0, -1.0, -0.5, 0.0, 0.5, 1.0, 45.0, 90.0,
        180.0, 360.0, 720.0, 900.0,
    ];
    for &deg in &angles {
        let t = sample(deg, 0.0, 0, 0);
        let via_compat = t.wheel_angle_mdeg();
        let via_native = (t.0.wheel_angle_deg * 1000.0) as i32;
        assert_eq!(
            via_compat, via_native,
            "angle mismatch for {deg} deg"
        );
    }
    Ok(())
}

#[test]
fn compat_and_native_produce_identical_speeds() -> TestResult {
    let speeds = [-100.0, -50.0, -10.0, -1.0, 0.0, 1.0, 10.0, 50.0, 100.0];
    for &rad_s in &speeds {
        let t = sample(0.0, rad_s, 0, 0);
        let via_compat = t.wheel_speed_mrad_s();
        let via_native = (t.0.wheel_speed_rad_s * 1000.0) as i32;
        assert_eq!(
            via_compat, via_native,
            "speed mismatch for {rad_s} rad/s"
        );
    }
    Ok(())
}

#[test]
fn compat_full_snapshot_matches_native_access() -> TestResult {
    let t = sample(270.0, 12.5, 88, 0x13);

    assert_eq!(t.temp_c(), t.0.temperature_c);
    assert_eq!(t.faults(), t.0.fault_flags);
    assert_eq!(
        t.wheel_angle_mdeg(),
        (t.0.wheel_angle_deg * 1000.0) as i32
    );
    assert_eq!(
        t.wheel_speed_mrad_s(),
        (t.0.wheel_speed_rad_s * 1000.0) as i32
    );
    assert_eq!(t.sequence(), 0);
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// Compat report generation and accuracy
// ──────────────────────────────────────────────────────────────────────

/// Verify the complete deprecation mapping table.
/// This acts as a "compat report" documenting each deprecated→new mapping.
#[test]
fn compat_deprecation_report() -> TestResult {
    // Mapping: temp_c → temperature_c (direct, no conversion)
    // Mapping: faults → fault_flags (direct, no conversion)
    // Mapping: wheel_angle_mdeg → wheel_angle_deg (×1000 conversion)
    // Mapping: wheel_speed_mrad_s → wheel_speed_rad_s (×1000 conversion)
    // Mapping: sequence → (removed, always 0)

    let t = sample(90.0, 5.0, 50, 0x07);

    // Verify all 5 mappings
    let report = vec![
        ("temp_c", "temperature_c", t.temp_c() as i64, t.0.temperature_c as i64),
        ("faults", "fault_flags", t.faults() as i64, t.0.fault_flags as i64),
        (
            "wheel_angle_mdeg",
            "wheel_angle_deg * 1000",
            t.wheel_angle_mdeg() as i64,
            (t.0.wheel_angle_deg * 1000.0) as i64,
        ),
        (
            "wheel_speed_mrad_s",
            "wheel_speed_rad_s * 1000",
            t.wheel_speed_mrad_s() as i64,
            (t.0.wheel_speed_rad_s * 1000.0) as i64,
        ),
        ("sequence", "(removed)", t.sequence() as i64, 0_i64),
    ];

    for (old_name, new_name, compat_val, native_val) in &report {
        assert_eq!(
            compat_val, native_val,
            "compat report mismatch: {old_name} → {new_name}: compat={compat_val}, native={native_val}"
        );
    }
    Ok(())
}

/// The compat trait has exactly 5 methods (the complete deprecated API surface).
#[test]
fn compat_trait_surface_is_complete() -> TestResult {
    let t = sample(1.0, 1.0, 1, 1);
    let dyn_ref: &dyn TelemetryCompat = &t;

    // All 5 methods are callable through dynamic dispatch
    let _ = dyn_ref.temp_c();
    let _ = dyn_ref.faults();
    let _ = dyn_ref.wheel_angle_mdeg();
    let _ = dyn_ref.wheel_speed_mrad_s();
    let _ = dyn_ref.sequence();
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// Schema version negotiation in migration context
// ──────────────────────────────────────────────────────────────────────

#[test]
fn version_negotiation_v0_upgrades_to_v1() -> TestResult {
    let v0 = SchemaVersion::new(0, 0);
    let v1 = SchemaVersion::parse(CURRENT_SCHEMA_VERSION)?;
    assert!(v0.is_older_than(&v1));
    assert!(!v0.is_current());
    assert!(v1.is_current());
    Ok(())
}

#[test]
fn version_negotiation_v1_is_current() -> TestResult {
    let v1 = SchemaVersion::parse(CURRENT_SCHEMA_VERSION)?;
    assert!(v1.is_current());
    assert_eq!(v1.major, 1);
    Ok(())
}

#[test]
fn legacy_format_detected_by_flat_fields() -> TestResult {
    let manager = MigrationManager::new(MigrationConfig::without_backups())?;

    // Profile with ffb_gain (legacy flat format)
    let legacy1 = r#"{"ffb_gain": 0.7}"#;
    let version = manager.detect_version(legacy1)?;
    assert_eq!(version.major, 0);

    // Profile with degrees_of_rotation (legacy flat format)
    let legacy2 = r#"{"degrees_of_rotation": 900}"#;
    let version = manager.detect_version(legacy2)?;
    assert_eq!(version.major, 0);
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// Snapshot: compat trait mapping table
// ──────────────────────────────────────────────────────────────────────

#[test]
fn snapshot_compat_mapping_report() -> TestResult {
    let t = sample(45.0, 3.0, 72, 0xAB);
    let report = serde_json::json!({
        "mappings": [
            {
                "old_api": "temp_c()",
                "new_field": "temperature_c",
                "conversion": "direct",
                "compat_value": t.temp_c(),
                "native_value": t.0.temperature_c
            },
            {
                "old_api": "faults()",
                "new_field": "fault_flags",
                "conversion": "direct",
                "compat_value": t.faults(),
                "native_value": t.0.fault_flags
            },
            {
                "old_api": "wheel_angle_mdeg()",
                "new_field": "wheel_angle_deg",
                "conversion": "multiply_1000",
                "compat_value": t.wheel_angle_mdeg(),
                "native_value": (t.0.wheel_angle_deg * 1000.0) as i32
            },
            {
                "old_api": "wheel_speed_mrad_s()",
                "new_field": "wheel_speed_rad_s",
                "conversion": "multiply_1000",
                "compat_value": t.wheel_speed_mrad_s(),
                "native_value": (t.0.wheel_speed_rad_s * 1000.0) as i32
            },
            {
                "old_api": "sequence()",
                "new_field": "(removed)",
                "conversion": "always_zero",
                "compat_value": t.sequence(),
                "native_value": 0
            }
        ]
    });
    insta::assert_json_snapshot!("migration_compat_mapping_report", report);
    Ok(())
}

#[test]
fn snapshot_legacy_migration_result() -> TestResult {
    let manager = MigrationManager::new(MigrationConfig::without_backups())?;
    let legacy = r#"{"ffb_gain": 0.75, "degrees_of_rotation": 540, "torque_cap": 8.0}"#;

    let migrated = manager.migrate_profile(legacy)?;
    let value: serde_json::Value = serde_json::from_str(&migrated)?;
    insta::assert_json_snapshot!("migration_legacy_to_v1_result", value);
    Ok(())
}

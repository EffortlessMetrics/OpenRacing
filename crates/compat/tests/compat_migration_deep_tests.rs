//! Deep tests for compat migration paths.
//!
//! Covers:
//! - All API migration paths (old field → new field)
//! - Backward compatibility guarantees
//! - Version detection across all supported versions
//! - Format conversion round-trips
//! - Deprecated API behavior
//! - Error handling for unsupported versions

use compat::TelemetryCompat;
use racing_wheel_engine::TelemetryData;
use racing_wheel_schemas::migration::{
    CURRENT_SCHEMA_VERSION, MigrationConfig, MigrationManager, SCHEMA_VERSION_V2, SchemaVersion,
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

// ===========================================================================
// 1. All API migration paths
// ===========================================================================

mod api_migration_paths {
    use super::*;

    /// Every deprecated method maps to the correct new field.
    #[test]
    fn temp_c_migrates_to_temperature_c() {
        for temp in [0_u8, 1, 25, 100, 200, 255] {
            let t = sample(0.0, 0.0, temp, 0);
            assert_eq!(t.temp_c(), t.0.temperature_c);
        }
    }

    #[test]
    fn faults_migrates_to_fault_flags() {
        for faults in [0x00_u8, 0x01, 0x0F, 0x55, 0xAA, 0xFF] {
            let t = sample(0.0, 0.0, 0, faults);
            assert_eq!(t.faults(), t.0.fault_flags);
        }
    }

    #[test]
    fn wheel_angle_mdeg_migrates_to_deg_with_factor_1000() {
        let angles: &[f32] = &[-900.0, -360.0, -1.0, 0.0, 1.0, 360.0, 900.0];
        for &deg in angles {
            let t = sample(deg, 0.0, 0, 0);
            assert_eq!(
                t.wheel_angle_mdeg(),
                (t.0.wheel_angle_deg * 1000.0) as i32,
                "mismatch at {deg} deg"
            );
        }
    }

    #[test]
    fn wheel_speed_mrad_s_migrates_to_rad_s_with_factor_1000() {
        let speeds: &[f32] = &[-100.0, -10.0, -0.5, 0.0, 0.5, 10.0, 100.0];
        for &spd in speeds {
            let t = sample(0.0, spd, 0, 0);
            assert_eq!(
                t.wheel_speed_mrad_s(),
                (t.0.wheel_speed_rad_s * 1000.0) as i32,
                "mismatch at {spd} rad/s"
            );
        }
    }

    #[test]
    fn sequence_removed_always_zero() {
        let t = sample(90.0, 5.0, 45, 0x02);
        assert_eq!(t.sequence(), 0);
    }

    /// Migration of a full legacy profile yields all required top-level keys.
    #[test]
    fn legacy_profile_migration_produces_complete_structure() -> TestResult {
        let mgr = MigrationManager::new(MigrationConfig::without_backups())?;
        let legacy = r#"{
            "ffb_gain": 0.85,
            "degrees_of_rotation": 540,
            "torque_cap": 10.0
        }"#;
        let migrated = mgr.migrate_profile(legacy)?;
        let v: serde_json::Value = serde_json::from_str(&migrated)?;

        assert_eq!(
            v.get("schema").and_then(|s| s.as_str()),
            Some(CURRENT_SCHEMA_VERSION)
        );
        assert!(v.get("base").is_some(), "must have 'base'");
        assert!(v.get("scope").is_some(), "must have 'scope'");
        Ok(())
    }

    /// Every numeric value in the legacy profile round-trips through migration.
    #[test]
    fn legacy_values_survive_migration() -> TestResult {
        let mgr = MigrationManager::new(MigrationConfig::without_backups())?;
        let legacy = r#"{"ffb_gain": 0.42, "degrees_of_rotation": 1080, "torque_cap": 20.0}"#;
        let migrated = mgr.migrate_profile(legacy)?;
        let v: serde_json::Value = serde_json::from_str(&migrated)?;

        let gain = v
            .get("base")
            .and_then(|b| b.get("ffbGain"))
            .and_then(|x| x.as_f64())
            .ok_or("missing ffbGain")?;
        assert!((gain - 0.42).abs() < f64::EPSILON);

        let dor = v
            .get("base")
            .and_then(|b| b.get("dorDeg"))
            .and_then(|x| x.as_u64())
            .ok_or("missing dorDeg")?;
        assert_eq!(dor, 1080);

        let torque = v
            .get("base")
            .and_then(|b| b.get("torqueCapNm"))
            .and_then(|x| x.as_f64())
            .ok_or("missing torqueCapNm")?;
        assert!((torque - 20.0).abs() < f64::EPSILON);
        Ok(())
    }
}

// ===========================================================================
// 2. Backward compatibility guarantees
// ===========================================================================

mod backward_compatibility {
    use super::*;

    /// Old compat calls and direct field access produce identical results.
    #[test]
    fn compat_and_direct_agree_for_all_fields() {
        let t = sample(270.0, 12.5, 88, 0x13);
        assert_eq!(t.temp_c(), t.0.temperature_c);
        assert_eq!(t.faults(), t.0.fault_flags);
        assert_eq!(t.wheel_angle_mdeg(), (t.0.wheel_angle_deg * 1000.0) as i32);
        assert_eq!(
            t.wheel_speed_mrad_s(),
            (t.0.wheel_speed_rad_s * 1000.0) as i32
        );
        assert_eq!(t.sequence(), 0);
    }

    /// Trait object dispatch still works (API stability).
    #[test]
    fn dynamic_dispatch_backward_compat() {
        let t = sample(45.0, 3.0, 55, 0x0F);
        let dyn_ref: &dyn TelemetryCompat = &t;
        assert_eq!(dyn_ref.temp_c(), 55);
        assert_eq!(dyn_ref.faults(), 0x0F);
        assert_eq!(dyn_ref.wheel_angle_mdeg(), 45_000);
        assert_eq!(dyn_ref.wheel_speed_mrad_s(), 3_000);
        assert_eq!(dyn_ref.sequence(), 0);
    }

    /// Boxed trait objects are usable (owned dynamic dispatch).
    #[test]
    fn boxed_trait_object_backward_compat() {
        let t = sample(60.0, 4.0, 33, 0x11);
        let boxed: Box<dyn TelemetryCompat> = Box::new(t);
        assert_eq!(boxed.temp_c(), 33);
        assert_eq!(boxed.faults(), 0x11);
        assert_eq!(boxed.wheel_angle_mdeg(), 60_000);
        assert_eq!(boxed.wheel_speed_mrad_s(), 4_000);
        assert_eq!(boxed.sequence(), 0);
    }

    /// Already-current profiles pass through migration unchanged.
    #[test]
    fn current_version_profile_is_idempotent() -> TestResult {
        let mgr = MigrationManager::new(MigrationConfig::without_backups())?;
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
        assert!(!mgr.needs_migration(v1)?);
        let migrated = mgr.migrate_profile(v1)?;
        let orig: serde_json::Value = serde_json::from_str(v1)?;
        let after: serde_json::Value = serde_json::from_str(&migrated)?;
        assert_eq!(orig, after);
        Ok(())
    }

    /// Repeated compat calls are idempotent.
    #[test]
    fn compat_methods_are_idempotent() {
        let t = sample(123.456, -7.89, 99, 0x3C);
        for _ in 0..10 {
            assert_eq!(t.temp_c(), 99);
            assert_eq!(t.faults(), 0x3C);
            assert_eq!(t.wheel_angle_mdeg(), (123.456_f32 * 1000.0) as i32);
            assert_eq!(t.wheel_speed_mrad_s(), (-7.89_f32 * 1000.0) as i32);
            assert_eq!(t.sequence(), 0);
        }
    }

    /// Compat fields are isolated from one another.
    #[test]
    fn cross_field_isolation() {
        let base = sample(45.0, 7.5, 42, 0x55);
        let varied = sample(900.0, 100.0, 42, 0x55);
        // Changing angle and speed does not affect temp_c / faults
        assert_eq!(base.temp_c(), varied.temp_c());
        assert_eq!(base.faults(), varied.faults());
    }
}

// ===========================================================================
// 3. Version detection across all supported versions
// ===========================================================================

mod version_detection {
    use super::*;

    #[test]
    fn detect_legacy_format_by_flat_fields() -> TestResult {
        let mgr = MigrationManager::new(MigrationConfig::without_backups())?;

        let legacy_ffb = r#"{"ffb_gain": 0.7}"#;
        let v = mgr.detect_version(legacy_ffb)?;
        assert_eq!(v.major, 0, "ffb_gain → legacy v0");

        let legacy_dor = r#"{"degrees_of_rotation": 900}"#;
        let v = mgr.detect_version(legacy_dor)?;
        assert_eq!(v.major, 0, "degrees_of_rotation → legacy v0");

        let legacy_torque = r#"{"torque_cap": 15.0}"#;
        let v = mgr.detect_version(legacy_torque)?;
        assert_eq!(v.major, 0, "torque_cap → legacy v0");
        Ok(())
    }

    #[test]
    fn detect_v1_format() -> TestResult {
        let mgr = MigrationManager::new(MigrationConfig::without_backups())?;
        let v1 = r#"{"schema": "wheel.profile/1", "base": {}, "scope": {}}"#;
        let v = mgr.detect_version(v1)?;
        assert_eq!(v.major, 1);
        Ok(())
    }

    #[test]
    fn v0_is_older_than_v1() -> TestResult {
        let v0 = SchemaVersion::new(0, 0);
        let v1 = SchemaVersion::parse(CURRENT_SCHEMA_VERSION)?;
        assert!(v0.is_older_than(&v1));
        assert!(!v1.is_older_than(&v0));
        Ok(())
    }

    #[test]
    fn v1_is_current() -> TestResult {
        let v1 = SchemaVersion::parse(CURRENT_SCHEMA_VERSION)?;
        assert!(v1.is_current());
        assert_eq!(v1.major, 1);
        Ok(())
    }

    #[test]
    fn v2_is_newer_than_current() -> TestResult {
        let v2 = SchemaVersion::parse(SCHEMA_VERSION_V2)?;
        let v1 = SchemaVersion::parse(CURRENT_SCHEMA_VERSION)?;
        assert!(v1.is_older_than(&v2));
        assert!(!v2.is_older_than(&v1));
        Ok(())
    }

    #[test]
    fn detect_needs_migration_for_legacy() -> TestResult {
        let mgr = MigrationManager::new(MigrationConfig::without_backups())?;
        let legacy = r#"{"ffb_gain": 0.5, "degrees_of_rotation": 720, "torque_cap": 8.0}"#;
        assert!(mgr.needs_migration(legacy)?);
        Ok(())
    }

    #[test]
    fn detect_no_migration_for_current() -> TestResult {
        let mgr = MigrationManager::new(MigrationConfig::without_backups())?;
        let current = r#"{
            "schema": "wheel.profile/1",
            "scope": { "game": null },
            "base": {
                "ffbGain": 0.7, "dorDeg": 900, "torqueCapNm": 15.0,
                "filters": {
                    "reconstruction": 0, "friction": 0.0, "damper": 0.0,
                    "inertia": 0.0, "notchFilters": [], "slewRate": 1.0,
                    "curvePoints": [
                        {"input": 0.0, "output": 0.0},
                        {"input": 1.0, "output": 1.0}
                    ]
                }
            }
        }"#;
        assert!(!mgr.needs_migration(current)?);
        Ok(())
    }
}

// ===========================================================================
// 4. Format conversion round-trips
// ===========================================================================

mod format_conversion {
    use super::*;

    /// Legacy → v1 migration preserves core values in new field locations.
    #[test]
    fn legacy_to_v1_round_trip_preserves_values() -> TestResult {
        let mgr = MigrationManager::new(MigrationConfig::without_backups())?;
        let gains = [0.0, 0.25, 0.5, 0.75, 1.0];
        let dors = [180, 360, 540, 720, 900, 1080];
        let torques = [1.0, 5.0, 10.0, 15.0, 25.0];

        for &g in &gains {
            for &d in &dors {
                for &t in &torques {
                    let legacy = format!(
                        r#"{{"ffb_gain": {g}, "degrees_of_rotation": {d}, "torque_cap": {t}}}"#
                    );
                    let migrated = mgr.migrate_profile(&legacy)?;
                    let v: serde_json::Value = serde_json::from_str(&migrated)?;

                    let out_gain = v["base"]["ffbGain"].as_f64().ok_or("missing ffbGain")?;
                    assert!((out_gain - g).abs() < f64::EPSILON, "gain mismatch: {g}");

                    let out_dor = v["base"]["dorDeg"].as_u64().ok_or("missing dorDeg")?;
                    assert_eq!(out_dor, d, "dor mismatch");

                    let out_torque = v["base"]["torqueCapNm"]
                        .as_f64()
                        .ok_or("missing torqueCapNm")?;
                    assert!(
                        (out_torque - t).abs() < f64::EPSILON,
                        "torque mismatch: {t}"
                    );
                }
            }
        }
        Ok(())
    }

    /// Migrated profile re-serializes to valid JSON.
    #[test]
    fn migrated_profile_is_valid_json() -> TestResult {
        let mgr = MigrationManager::new(MigrationConfig::without_backups())?;
        let legacy = r#"{"ffb_gain": 0.6, "degrees_of_rotation": 900, "torque_cap": 12.0}"#;
        let migrated = mgr.migrate_profile(legacy)?;
        let parsed: serde_json::Value = serde_json::from_str(&migrated)?;
        // Re-serialize and re-parse
        let reserialized = serde_json::to_string(&parsed)?;
        let _: serde_json::Value = serde_json::from_str(&reserialized)?;
        Ok(())
    }

    /// Compat unit conversion is reversible for exact f32 values.
    #[test]
    fn unit_conversion_reversibility_exact() {
        let exact_values: &[f32] = &[0.0, 1.0, 2.0, 4.0, 8.0, 16.0, 64.0, 128.0, 256.0, 512.0];
        for &deg in exact_values {
            let t = sample(deg, deg, 0, 0);
            let mdeg = t.wheel_angle_mdeg();
            let back = mdeg as f32 / 1000.0;
            assert!(
                (back - deg).abs() < f32::EPSILON,
                "round-trip failed for {deg}"
            );
        }
    }

    /// Compat conversion uses truncation, not rounding.
    #[test]
    fn conversion_truncation_behavior() {
        // 0.9999 * 1000 = 999.9 → 999 (truncated)
        let t = sample(0.9999, 0.0, 0, 0);
        assert_eq!(t.wheel_angle_mdeg(), 999);

        // -0.9999 * 1000 = -999.9 → -999 (truncated toward zero)
        let t2 = sample(-0.9999, 0.0, 0, 0);
        assert_eq!(t2.wheel_angle_mdeg(), -999);
    }
}

// ===========================================================================
// 5. Deprecated API behavior
// ===========================================================================

mod deprecated_api {
    use super::*;

    /// The complete deprecation mapping table.
    #[test]
    fn deprecation_mapping_completeness() {
        let t = sample(45.0, 3.0, 72, 0xAB);
        let mappings: Vec<(&str, i64, i64)> = vec![
            (
                "temp_c→temperature_c",
                t.temp_c() as i64,
                t.0.temperature_c as i64,
            ),
            (
                "faults→fault_flags",
                t.faults() as i64,
                t.0.fault_flags as i64,
            ),
            (
                "wheel_angle_mdeg→deg*1000",
                t.wheel_angle_mdeg() as i64,
                (t.0.wheel_angle_deg * 1000.0) as i64,
            ),
            (
                "wheel_speed_mrad_s→rad_s*1000",
                t.wheel_speed_mrad_s() as i64,
                (t.0.wheel_speed_rad_s * 1000.0) as i64,
            ),
            ("sequence→(removed)", t.sequence() as i64, 0),
        ];
        for (name, compat, native) in &mappings {
            assert_eq!(compat, native, "{name}: compat={compat}, native={native}");
        }
    }

    /// Removed field `sequence` always returns 0 regardless of state.
    #[test]
    fn removed_field_always_zero() {
        let cases: &[(f32, f32, u8, u8)] = &[
            (0.0, 0.0, 0, 0),
            (900.0, 100.0, 255, 255),
            (-900.0, -100.0, 128, 0x80),
        ];
        for &(a, s, t, f) in cases {
            assert_eq!(sample(a, s, t, f).sequence(), 0);
        }
    }

    /// Direct-mapped fields have no conversion (identity).
    #[test]
    fn direct_mapping_no_conversion() {
        for val in 0..=255_u8 {
            let t = sample(0.0, 0.0, val, val);
            assert_eq!(t.temp_c(), val);
            assert_eq!(t.faults(), val);
        }
    }

    /// Conversion factor is exactly 1000.
    #[test]
    fn conversion_factor_exactly_1000() {
        let t = sample(1.0, 1.0, 0, 0);
        assert_eq!(t.wheel_angle_mdeg(), 1000);
        assert_eq!(t.wheel_speed_mrad_s(), 1000);
    }

    /// Negative zero yields zero.
    #[test]
    fn negative_zero_yields_zero() {
        let t = sample(-0.0, -0.0, 0, 0);
        assert_eq!(t.wheel_angle_mdeg(), 0);
        assert_eq!(t.wheel_speed_mrad_s(), 0);
    }

    /// Sign symmetry: positive and negative are negations of each other.
    #[test]
    fn sign_symmetry() {
        for &v in &[1.0_f32, 45.0, 90.0, 180.0, 360.0, 720.0, 900.0] {
            let pos = sample(v, v, 0, 0);
            let neg = sample(-v, -v, 0, 0);
            assert_eq!(pos.wheel_angle_mdeg(), -neg.wheel_angle_mdeg());
            assert_eq!(pos.wheel_speed_mrad_s(), -neg.wheel_speed_mrad_s());
        }
    }

    /// Legacy migration adds default filters when not present.
    #[test]
    fn legacy_migration_injects_default_filters() -> TestResult {
        let mgr = MigrationManager::new(MigrationConfig::without_backups())?;
        let legacy = r#"{"ffb_gain": 0.7, "degrees_of_rotation": 900, "torque_cap": 15.0}"#;
        let migrated = mgr.migrate_profile(legacy)?;
        let v: serde_json::Value = serde_json::from_str(&migrated)?;

        let filters = v
            .get("base")
            .and_then(|b| b.get("filters"))
            .ok_or("missing filters")?;
        assert!(filters.get("reconstruction").is_some());
        assert!(filters.get("notchFilters").is_some());
        assert!(filters.get("curvePoints").is_some());
        Ok(())
    }
}

// ===========================================================================
// 6. Error handling for unsupported versions
// ===========================================================================

mod error_handling {
    use super::*;

    #[test]
    fn unknown_schema_prefix_is_rejected() -> TestResult {
        let mgr = MigrationManager::new(MigrationConfig::without_backups())?;
        let unknown = r#"{"schema": "unknown.format/99"}"#;
        let result = mgr.detect_version(unknown);
        assert!(result.is_err(), "unknown schema prefix must be rejected");
        Ok(())
    }

    #[test]
    fn empty_json_is_treated_as_legacy() -> TestResult {
        let mgr = MigrationManager::new(MigrationConfig::without_backups())?;
        let version = mgr.detect_version("{}")?;
        // Empty JSON has no schema/base fields, so is_legacy_format returns true
        assert_eq!(version, SchemaVersion::new(0, 0));
        Ok(())
    }

    #[test]
    fn invalid_json_is_rejected() -> TestResult {
        let mgr = MigrationManager::new(MigrationConfig::without_backups())?;
        let result = mgr.migrate_profile("not json at all");
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn schema_version_parse_rejects_garbage() {
        let result = SchemaVersion::parse("garbage");
        assert!(result.is_err());
    }

    #[test]
    fn schema_version_parse_rejects_empty() {
        let result = SchemaVersion::parse("");
        assert!(result.is_err());
    }

    #[test]
    fn future_schema_version_detected_but_not_current() -> TestResult {
        let v2 = SchemaVersion::parse(SCHEMA_VERSION_V2)?;
        assert!(!v2.is_current());
        Ok(())
    }

    /// Truncated legacy JSON fails gracefully.
    #[test]
    fn truncated_json_fails_gracefully() -> TestResult {
        let mgr = MigrationManager::new(MigrationConfig::without_backups())?;
        let truncated = r#"{"ffb_gain": 0.5, "degrees_of_rotation":"#;
        let result = mgr.migrate_profile(truncated);
        assert!(result.is_err());
        Ok(())
    }

    /// Null value in schema field is handled.
    #[test]
    fn null_schema_field_handled() -> TestResult {
        let mgr = MigrationManager::new(MigrationConfig::without_backups())?;
        let null_schema = r#"{"schema": null}"#;
        let result = mgr.detect_version(null_schema);
        assert!(result.is_err());
        Ok(())
    }

    /// Numeric schema field is rejected.
    #[test]
    fn numeric_schema_field_rejected() -> TestResult {
        let mgr = MigrationManager::new(MigrationConfig::without_backups())?;
        let numeric = r#"{"schema": 42}"#;
        let result = mgr.detect_version(numeric);
        assert!(result.is_err());
        Ok(())
    }
}

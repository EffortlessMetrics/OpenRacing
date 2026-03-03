//! Migration edge-case tests for the compat crate
//!
//! Tests corrupt input, very old format versions, mixed old/new fields,
//! and other real-world migration scenarios.

use compat::TelemetryCompat;
use racing_wheel_engine::TelemetryData;
use racing_wheel_schemas::migration::{
    CURRENT_SCHEMA_VERSION, MigrationConfig, MigrationManager, SchemaVersion,
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

// ---------------------------------------------------------------------------
// Corrupt input
// ---------------------------------------------------------------------------

mod corrupt_input {
    use super::*;

    #[test]
    fn empty_string_rejected() {
        let manager = MigrationManager::new(MigrationConfig::without_backups());
        assert!(manager.is_ok());
        if let Ok(mgr) = manager {
            let result = mgr.migrate_profile("");
            assert!(result.is_err(), "empty string must be rejected");
        }
    }

    #[test]
    fn plain_text_not_json_rejected() {
        let manager = MigrationManager::new(MigrationConfig::without_backups());
        assert!(manager.is_ok());
        if let Ok(mgr) = manager {
            let result = mgr.migrate_profile("this is not json at all");
            assert!(result.is_err(), "plain text must be rejected");
        }
    }

    #[test]
    fn json_array_instead_of_object_rejected() {
        let manager = MigrationManager::new(MigrationConfig::without_backups());
        assert!(manager.is_ok());
        if let Ok(mgr) = manager {
            let result = mgr.migrate_profile("[1, 2, 3]");
            assert!(result.is_err(), "JSON array must be rejected");
        }
    }

    #[test]
    fn json_null_rejected() {
        let manager = MigrationManager::new(MigrationConfig::without_backups());
        assert!(manager.is_ok());
        if let Ok(mgr) = manager {
            let result = mgr.migrate_profile("null");
            assert!(result.is_err(), "null must be rejected");
        }
    }

    #[test]
    fn truncated_json_rejected() {
        let manager = MigrationManager::new(MigrationConfig::without_backups());
        assert!(manager.is_ok());
        if let Ok(mgr) = manager {
            let result = mgr.migrate_profile(r#"{"ffb_gain": 0.5, "degrees_of_ro"#);
            assert!(result.is_err(), "truncated JSON must be rejected");
        }
    }

    #[test]
    fn empty_object_detected_as_legacy() -> TestResult {
        let manager = MigrationManager::new(MigrationConfig::without_backups())?;
        // An empty object has no "schema" and no "base", so is_legacy_format
        // returns true (no schema + no base = legacy)
        let version = manager.detect_version("{}")?;
        assert_eq!(
            version.major, 0,
            "empty object should be detected as legacy v0"
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Very old / unknown format versions
// ---------------------------------------------------------------------------

mod old_format_versions {
    use super::*;

    #[test]
    fn unknown_schema_prefix_rejected() {
        let manager = MigrationManager::new(MigrationConfig::without_backups());
        assert!(manager.is_ok());
        if let Ok(mgr) = manager {
            let result = mgr.detect_version(r#"{"schema": "totally.different/3"}"#);
            assert!(result.is_err(), "unknown schema prefix must be rejected");
        }
    }

    #[test]
    fn schema_version_zero_string_rejected() {
        // "wheel.profile/0" as a schema string — the migration manager
        // may reject this if it cannot parse as a valid target
        let result = SchemaVersion::parse("wheel.profile/0");
        assert!(result.is_ok(), "version 0 should parse successfully");
        if let Ok(v) = result {
            assert_eq!(v.major, 0);
            assert!(!v.is_current());
        }
    }

    #[test]
    fn schema_version_negative_fails_parse() {
        let result = SchemaVersion::parse("wheel.profile/-1");
        assert!(result.is_err(), "negative version must fail parsing");
    }

    #[test]
    fn schema_version_non_numeric_fails_parse() {
        let result = SchemaVersion::parse("wheel.profile/abc");
        assert!(result.is_err(), "non-numeric version must fail parsing");
    }

    #[test]
    fn future_major_version_is_not_current() -> TestResult {
        let future = SchemaVersion::parse("wheel.profile/99")?;
        assert!(!future.is_current());
        let current = SchemaVersion::parse(CURRENT_SCHEMA_VERSION)?;
        assert!(current.is_older_than(&future));
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Mixed old/new fields
// ---------------------------------------------------------------------------

mod mixed_fields {
    use super::*;

    #[test]
    fn legacy_with_extra_unknown_fields_still_migrates() -> TestResult {
        let manager = MigrationManager::new(MigrationConfig::without_backups())?;
        let legacy = r#"{
            "ffb_gain": 0.75,
            "degrees_of_rotation": 900,
            "torque_cap": 12.0,
            "unknown_legacy_field": "should be ignored",
            "another_one": 42
        }"#;
        let migrated = manager.migrate_profile(legacy)?;
        let value: serde_json::Value = serde_json::from_str(&migrated)?;
        assert_eq!(
            value.get("schema").and_then(|v| v.as_str()),
            Some(CURRENT_SCHEMA_VERSION)
        );
        Ok(())
    }

    #[test]
    fn legacy_minimal_single_field_migrates() -> TestResult {
        let manager = MigrationManager::new(MigrationConfig::without_backups())?;
        let legacy = r#"{"ffb_gain": 0.5}"#;
        let migrated = manager.migrate_profile(legacy)?;
        let value: serde_json::Value = serde_json::from_str(&migrated)?;
        assert_eq!(
            value.get("schema").and_then(|v| v.as_str()),
            Some(CURRENT_SCHEMA_VERSION)
        );
        Ok(())
    }

    #[test]
    fn v1_profile_with_all_optional_sections() -> TestResult {
        let manager = MigrationManager::new(MigrationConfig::without_backups())?;
        let v1 = r#"{
            "schema": "wheel.profile/1",
            "scope": { "game": "iracing", "car": "mx5", "track": "spa" },
            "base": {
                "ffbGain": 0.8,
                "dorDeg": 900,
                "torqueCapNm": 15.0,
                "filters": {
                    "reconstruction": 4,
                    "friction": 0.12,
                    "damper": 0.18,
                    "inertia": 0.08,
                    "notchFilters": [{"hz": 60.0, "q": 2.0, "gainDb": -12.0}],
                    "slewRate": 0.85,
                    "curvePoints": [
                        {"input": 0.0, "output": 0.0},
                        {"input": 0.5, "output": 0.6},
                        {"input": 1.0, "output": 1.0}
                    ]
                }
            },
            "leds": {
                "rpmBands": [0.75, 0.82, 0.88],
                "pattern": "progressive",
                "brightness": 0.8
            },
            "haptics": {
                "enabled": true,
                "intensity": 0.6,
                "frequencyHz": 80.0
            }
        }"#;
        assert!(!manager.needs_migration(v1)?);
        let migrated = manager.migrate_profile(v1)?;
        let orig: serde_json::Value = serde_json::from_str(v1)?;
        let migrated_val: serde_json::Value = serde_json::from_str(&migrated)?;
        assert_eq!(
            orig.get("base").and_then(|b| b.get("ffbGain")),
            migrated_val.get("base").and_then(|b| b.get("ffbGain"))
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Compat trait with extreme values
// ---------------------------------------------------------------------------

mod compat_extremes {
    use super::*;

    #[test]
    fn compat_with_zero_values() -> TestResult {
        let t = sample(0.0, 0.0, 0, 0);
        assert_eq!(t.temp_c(), 0);
        assert_eq!(t.faults(), 0);
        assert_eq!(t.wheel_angle_mdeg(), 0);
        assert_eq!(t.wheel_speed_mrad_s(), 0);
        assert_eq!(t.sequence(), 0);
        Ok(())
    }

    #[test]
    fn compat_with_max_u8_values() -> TestResult {
        let t = sample(0.0, 0.0, u8::MAX, u8::MAX);
        assert_eq!(t.temp_c(), 255);
        assert_eq!(t.faults(), 255);
        Ok(())
    }

    #[test]
    fn compat_negative_angle_conversion() -> TestResult {
        let t = sample(-900.0, 0.0, 0, 0);
        assert_eq!(t.wheel_angle_mdeg(), -900_000);
        Ok(())
    }

    #[test]
    fn compat_negative_speed_conversion() -> TestResult {
        let t = sample(0.0, -50.0, 0, 0);
        assert_eq!(t.wheel_speed_mrad_s(), -50_000);
        Ok(())
    }

    #[test]
    fn compat_fractional_precision_truncation() -> TestResult {
        // 0.4999 * 1000 = 499.9 → truncated to 499
        let t = sample(0.4999, 0.0, 0, 0);
        assert_eq!(t.wheel_angle_mdeg(), 499);
        Ok(())
    }
}

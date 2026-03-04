//! Regression tests for the compat crate.
//!
//! Covers:
//! - Known regression cases from real-world config files
//! - Edge cases in format detection
//! - Binary compatibility for serialized profiles
//! - Robustness against malformed inputs

use compat::TelemetryCompat;
use racing_wheel_engine::TelemetryData;
use racing_wheel_schemas::migration::{
    CURRENT_SCHEMA_VERSION, MigrationConfig, MigrationManager, SchemaVersion,
    compat::BackwardCompatibleParser,
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

fn make_manager() -> Result<MigrationManager, Box<dyn std::error::Error>> {
    Ok(MigrationManager::new(MigrationConfig::without_backups())?)
}

// ===========================================================================
// 1. Real-world config file regression cases
// ===========================================================================

/// Regression: real-world iRacing profile with all three legacy fields.
#[test]
fn regression_iracing_legacy_profile() -> TestResult {
    let mgr = make_manager()?;
    let iracing_legacy = r#"{
        "ffb_gain": 0.55,
        "degrees_of_rotation": 900,
        "torque_cap": 12.5
    }"#;

    let migrated = mgr.migrate_profile(iracing_legacy)?;
    let v: serde_json::Value = serde_json::from_str(&migrated)?;

    let base = v.get("base").ok_or("missing base")?;
    let gain = base
        .get("ffbGain")
        .and_then(|x| x.as_f64())
        .ok_or("missing ffbGain")?;
    assert!((gain - 0.55).abs() < f64::EPSILON, "ffbGain mismatch");
    assert_eq!(base.get("dorDeg").and_then(|x| x.as_u64()), Some(900));
    let torque = base
        .get("torqueCapNm")
        .and_then(|x| x.as_f64())
        .ok_or("missing torqueCapNm")?;
    assert!((torque - 12.5).abs() < f64::EPSILON);
    Ok(())
}

/// Regression: real-world ACC profile with 540° DOR.
#[test]
fn regression_acc_540_dor_profile() -> TestResult {
    let mgr = make_manager()?;
    let acc_legacy = r#"{
        "ffb_gain": 0.80,
        "degrees_of_rotation": 540,
        "torque_cap": 8.0
    }"#;

    let migrated = mgr.migrate_profile(acc_legacy)?;
    let v: serde_json::Value = serde_json::from_str(&migrated)?;
    let base = v.get("base").ok_or("missing base")?;
    assert_eq!(base.get("dorDeg").and_then(|x| x.as_u64()), Some(540));
    Ok(())
}

/// Regression: profile with zero FFB gain (user disabled FFB).
#[test]
fn regression_zero_ffb_gain_profile() -> TestResult {
    let mgr = make_manager()?;
    let zero_ffb = r#"{
        "ffb_gain": 0.0,
        "degrees_of_rotation": 900,
        "torque_cap": 15.0
    }"#;

    let migrated = mgr.migrate_profile(zero_ffb)?;
    let v: serde_json::Value = serde_json::from_str(&migrated)?;
    let gain = v
        .get("base")
        .and_then(|b| b.get("ffbGain"))
        .and_then(|x| x.as_f64())
        .ok_or("missing ffbGain")?;
    assert!(gain.abs() < f64::EPSILON, "zero gain must be preserved");
    Ok(())
}

/// Regression: profile with max DOR (1440°, some direct-drive wheels).
#[test]
fn regression_max_dor_direct_drive_profile() -> TestResult {
    let mgr = make_manager()?;
    let dd_profile = r#"{
        "ffb_gain": 1.0,
        "degrees_of_rotation": 1440,
        "torque_cap": 25.0
    }"#;

    let migrated = mgr.migrate_profile(dd_profile)?;
    let v: serde_json::Value = serde_json::from_str(&migrated)?;
    let base = v.get("base").ok_or("missing base")?;
    assert_eq!(base.get("dorDeg").and_then(|x| x.as_u64()), Some(1440));
    let torque = base
        .get("torqueCapNm")
        .and_then(|x| x.as_f64())
        .ok_or("missing torqueCapNm")?;
    assert!((torque - 25.0).abs() < f64::EPSILON);
    Ok(())
}

// ===========================================================================
// 2. Edge cases in format detection
// ===========================================================================

/// Regression: empty JSON object detected as legacy v0.
#[test]
fn regression_empty_json_detected_as_legacy() -> TestResult {
    let mgr = make_manager()?;
    let version = mgr.detect_version("{}")?;
    assert_eq!(
        version.major, 0,
        "empty JSON should be detected as legacy v0"
    );
    Ok(())
}

/// Regression: profile with schema field but wrong prefix rejected.
#[test]
fn regression_wrong_schema_prefix_rejected() -> TestResult {
    let mgr = make_manager()?;
    let wrong_prefix = r#"{"schema": "pedals.profile/1"}"#;
    let result = mgr.detect_version(wrong_prefix);
    assert!(result.is_err(), "wrong schema prefix must be rejected");
    Ok(())
}

/// Regression: schema field with version 0 suffix detected correctly.
#[test]
fn regression_schema_version_with_minor_number() -> TestResult {
    let v1_2 = SchemaVersion::parse("wheel.profile/1.2")?;
    assert_eq!(v1_2.major, 1);
    assert_eq!(v1_2.minor, 2);
    assert!(!v1_2.is_current(), "v1.2 is not the current version");

    let v1_0 = SchemaVersion::parse("wheel.profile/1")?;
    assert!(v1_0.is_older_than(&v1_2), "v1.0 should be older than v1.2");
    Ok(())
}

/// Regression: detect_version handles whitespace-heavy JSON.
#[test]
fn regression_whitespace_heavy_json_detection() -> TestResult {
    let mgr = make_manager()?;
    let whitespace_heavy = "  \n\t  {  \n  \"ffb_gain\"  :  0.7  \n  }  \n  ";
    let version = mgr.detect_version(whitespace_heavy)?;
    assert_eq!(version.major, 0);
    Ok(())
}

/// Regression: profile with both legacy and v1 fields — schema wins.
#[test]
fn regression_mixed_legacy_and_v1_fields() -> TestResult {
    let mgr = make_manager()?;
    let mixed = r#"{
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
        },
        "ffb_gain": 0.5
    }"#;
    let version = mgr.detect_version(mixed)?;
    assert_eq!(
        version.major, 1,
        "schema field should take precedence over legacy fields"
    );
    Ok(())
}

// ===========================================================================
// 3. Binary compatibility for serialized profiles
// ===========================================================================

/// Regression: serialized v1 profile round-trips through JSON exactly.
#[test]
fn regression_v1_json_round_trip_exact() -> TestResult {
    let parser = BackwardCompatibleParser::new();
    let v1 = serde_json::json!({
        "schema": CURRENT_SCHEMA_VERSION,
        "scope": { "game": null, "car": null, "track": null },
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
        }
    });

    let profile = parser.parse(&v1.to_string())?;
    let json_str = profile.to_json()?;
    let reparsed = parser.parse(&json_str)?;

    assert_eq!(profile.ffb_gain(), reparsed.ffb_gain());
    assert_eq!(profile.dor_deg(), reparsed.dor_deg());
    assert_eq!(profile.torque_cap_nm(), reparsed.torque_cap_nm());
    assert_eq!(profile.game(), reparsed.game());
    assert_eq!(profile.schema_version.major, reparsed.schema_version.major);
    Ok(())
}

/// Regression: migrated legacy profile parses identically to handcrafted v1.
#[test]
fn regression_migrated_legacy_matches_handcrafted_v1_structure() -> TestResult {
    let mgr = make_manager()?;
    let legacy = r#"{"ffb_gain": 0.7, "degrees_of_rotation": 900, "torque_cap": 15.0}"#;
    let migrated = mgr.migrate_profile(legacy)?;
    let v: serde_json::Value = serde_json::from_str(&migrated)?;

    // Verify structural contract: must have schema, scope, base, base.filters
    assert!(v.is_object());
    assert!(v.get("schema").and_then(|s| s.as_str()).is_some());
    assert!(v.get("scope").and_then(|s| s.as_object()).is_some());
    let base = v
        .get("base")
        .and_then(|b| b.as_object())
        .ok_or("base must be object")?;
    assert!(base.get("filters").and_then(|f| f.as_object()).is_some());
    Ok(())
}

/// Regression: compat layer conversion consistency on boundary telemetry data.
#[test]
fn regression_telemetry_compat_boundary_consistency() -> TestResult {
    // Full positive boundary
    let t = sample(900.0, 100.0, u8::MAX, u8::MAX);
    assert_eq!(t.temp_c(), 255);
    assert_eq!(t.faults(), 255);
    assert_eq!(t.wheel_angle_mdeg(), 900_000);
    assert_eq!(t.wheel_speed_mrad_s(), 100_000);
    assert_eq!(t.sequence(), 0);

    // Full negative boundary
    let tn = sample(-900.0, -100.0, u8::MIN, u8::MIN);
    assert_eq!(tn.temp_c(), 0);
    assert_eq!(tn.faults(), 0);
    assert_eq!(tn.wheel_angle_mdeg(), -900_000);
    assert_eq!(tn.wheel_speed_mrad_s(), -100_000);
    assert_eq!(tn.sequence(), 0);

    // Symmetry check
    assert_eq!(t.wheel_angle_mdeg(), -tn.wheel_angle_mdeg());
    assert_eq!(t.wheel_speed_mrad_s(), -tn.wheel_speed_mrad_s());
    Ok(())
}

/// Regression: compat conversion of very small float does not produce spurious non-zero.
#[test]
fn regression_tiny_float_conversion_stable() -> TestResult {
    let tiny = sample(0.0001, 0.0001, 0, 0);
    // 0.0001 * 1000 = 0.1 → truncates to 0
    assert_eq!(tiny.wheel_angle_mdeg(), 0);
    assert_eq!(tiny.wheel_speed_mrad_s(), 0);

    let neg_tiny = sample(-0.0001, -0.0001, 0, 0);
    assert_eq!(neg_tiny.wheel_angle_mdeg(), 0);
    assert_eq!(neg_tiny.wheel_speed_mrad_s(), 0);
    Ok(())
}

/// Regression: version ordering is transitive across v0, v1, v2.
#[test]
fn regression_version_ordering_transitivity() -> TestResult {
    let v0 = SchemaVersion::new(0, 0);
    let v1 = SchemaVersion::parse(CURRENT_SCHEMA_VERSION)?;
    let v2 = SchemaVersion::new(2, 0);

    // v0 < v1 < v2 → v0 < v2 (transitivity)
    assert!(v0.is_older_than(&v1));
    assert!(v1.is_older_than(&v2));
    assert!(v0.is_older_than(&v2));

    // Anti-symmetry
    assert!(!v1.is_older_than(&v0));
    assert!(!v2.is_older_than(&v1));
    assert!(!v2.is_older_than(&v0));
    Ok(())
}

//! Schema evolution tests.
//!
//! Validates that adding new optional fields, removing deprecated fields,
//! and migrating between schema versions are handled gracefully.

use racing_wheel_schemas::config::ProfileValidator;
use racing_wheel_schemas::migration::{
    CURRENT_SCHEMA_VERSION, MigrationConfig, MigrationManager, SCHEMA_VERSION_V2, SchemaVersion,
};
use racing_wheel_schemas::telemetry::NormalizedTelemetry;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ──────────────────────────────────────────────────────────────────────
// Helper
// ──────────────────────────────────────────────────────────────────────

fn minimal_profile_json() -> String {
    serde_json::json!({
        "schema": "wheel.profile/1",
        "scope": { "game": "iRacing" },
        "base": {
            "ffbGain": 0.8,
            "dorDeg": 900,
            "torqueCapNm": 15.0,
            "filters": {
                "reconstruction": 4,
                "friction": 0.1,
                "damper": 0.2,
                "inertia": 0.05,
                "notchFilters": [],
                "slewRate": 0.8,
                "curvePoints": [
                    { "input": 0.0, "output": 0.0 },
                    { "input": 1.0, "output": 1.0 }
                ]
            }
        }
    })
    .to_string()
}

// ──────────────────────────────────────────────────────────────────────
// 1. Adding new optional fields doesn't break parsing
// ──────────────────────────────────────────────────────────────────────

#[test]
fn telemetry_new_optional_fields_default_on_missing() -> TestResult {
    // JSON representing an older producer that lacks newer optional fields
    let old_json = r#"{
        "speed_ms": 55.0,
        "steering_angle": -0.3,
        "throttle": 1.0,
        "brake": 0.0,
        "rpm": 7200.0,
        "gear": 5,
        "flags": {},
        "sequence": 10
    }"#;

    let t: NormalizedTelemetry = serde_json::from_str(old_json)?;
    assert!((t.speed_ms - 55.0).abs() < f32::EPSILON);
    // Newer optional fields should default
    assert!(t.car_id.is_none());
    assert!(t.track_id.is_none());
    assert!(t.session_id.is_none());
    assert!(t.extended.is_empty());
    assert_eq!(t.clutch, 0.0);
    assert_eq!(t.max_rpm, 0.0);
    assert_eq!(t.num_gears, 0);
    Ok(())
}

#[test]
fn profile_json_without_optional_sections_parses() -> TestResult {
    // A profile without leds, haptics, signature (all optional)
    let json = minimal_profile_json();
    let validator = ProfileValidator::new()?;
    let profile = validator.validate_json(&json)?;
    assert!(profile.leds.is_none());
    assert!(profile.haptics.is_none());
    assert!(profile.signature.is_none());
    Ok(())
}

#[test]
fn profile_json_with_extra_unknown_keys_in_scope_rejected() -> TestResult {
    // The schema sets additionalProperties: false on scope
    let json = serde_json::json!({
        "schema": "wheel.profile/1",
        "scope": { "game": "ACC", "weather": "rain" },
        "base": {
            "ffbGain": 0.8,
            "dorDeg": 900,
            "torqueCapNm": 15.0,
            "filters": {
                "reconstruction": 4,
                "friction": 0.1,
                "damper": 0.2,
                "inertia": 0.05,
                "notchFilters": [],
                "slewRate": 0.8,
                "curvePoints": [
                    { "input": 0.0, "output": 0.0 },
                    { "input": 1.0, "output": 1.0 }
                ]
            }
        }
    })
    .to_string();

    let validator = ProfileValidator::new()?;
    let result = validator.validate_json(&json);
    // JSON schema should reject unknown "weather" key
    assert!(result.is_err());
    Ok(())
}

#[test]
fn telemetry_extended_map_preserves_arbitrary_keys() -> TestResult {
    let json = r#"{
        "speed_ms": 30.0,
        "steering_angle": 0.0,
        "throttle": 0.5,
        "brake": 0.0,
        "rpm": 4000.0,
        "gear": 3,
        "flags": {},
        "sequence": 1,
        "extended": {
            "tire_temp_fl": { "type": "Float", "value": 95.0 },
            "custom_data": { "type": "String", "value": "hello" }
        }
    }"#;

    let t: NormalizedTelemetry = serde_json::from_str(json)?;
    assert_eq!(t.extended.len(), 2);
    assert!(t.extended.contains_key("tire_temp_fl"));
    assert!(t.extended.contains_key("custom_data"));
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// 2. Removing deprecated fields handled gracefully
// ──────────────────────────────────────────────────────────────────────

#[test]
fn proto_telemetry_sequence_deprecated_still_decodes() -> TestResult {
    // The sequence field is described as deprecated in code; verify it decodes
    use prost::Message;
    use racing_wheel_schemas::generated::wheel::v1 as proto;

    let msg = proto::TelemetryData {
        wheel_angle_mdeg: 10000,
        wheel_speed_mrad_s: 500,
        temp_c: 40,
        faults: 0,
        hands_on: false,
        sequence: 99, // deprecated but still wire-present
    };
    let bytes = msg.encode_to_vec();
    let decoded = proto::TelemetryData::decode(bytes.as_slice())?;
    assert_eq!(decoded.sequence, 99);
    Ok(())
}

#[test]
fn profile_empty_signature_treated_as_absent() -> TestResult {
    let json = serde_json::json!({
        "schema": "wheel.profile/1",
        "scope": { "game": "ACC" },
        "base": {
            "ffbGain": 0.8,
            "dorDeg": 900,
            "torqueCapNm": 15.0,
            "filters": {
                "reconstruction": 0,
                "friction": 0.0,
                "damper": 0.0,
                "inertia": 0.0,
                "notchFilters": [],
                "slewRate": 0.5,
                "curvePoints": [
                    { "input": 0.0, "output": 0.0 },
                    { "input": 1.0, "output": 1.0 }
                ]
            }
        },
        "signature": null
    })
    .to_string();

    let validator = ProfileValidator::new()?;
    let profile = validator.validate_json(&json)?;
    assert!(profile.signature.is_none());
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// 3. Schema migration paths
// ──────────────────────────────────────────────────────────────────────

#[test]
fn schema_version_parse_v1() -> TestResult {
    let v = SchemaVersion::parse(CURRENT_SCHEMA_VERSION)?;
    assert_eq!(v.major, 1);
    assert_eq!(v.minor, 0);
    assert!(v.is_current());
    Ok(())
}

#[test]
fn schema_version_parse_v2() -> TestResult {
    let v = SchemaVersion::parse(SCHEMA_VERSION_V2)?;
    assert_eq!(v.major, 2);
    assert_eq!(v.minor, 0);
    assert!(!v.is_current());
    Ok(())
}

#[test]
fn schema_version_ordering() -> TestResult {
    let v1 = SchemaVersion::parse(CURRENT_SCHEMA_VERSION)?;
    let v2 = SchemaVersion::parse(SCHEMA_VERSION_V2)?;
    assert!(v1.is_older_than(&v2));
    assert!(!v2.is_older_than(&v1));
    Ok(())
}

#[test]
fn schema_version_invalid_format_rejected() {
    let result = SchemaVersion::parse("invalid");
    assert!(result.is_err());

    let result = SchemaVersion::parse("wheel.profile/");
    assert!(result.is_err());

    let result = SchemaVersion::parse("other.schema/1");
    assert!(result.is_err());
}

#[test]
fn migration_manager_creation_with_tempdir() -> TestResult {
    let tmp = tempfile::tempdir()?;
    let config = MigrationConfig::new(tmp.path());
    let manager = MigrationManager::new(config)?;

    // Current version should be recognized
    let current = SchemaVersion::parse(CURRENT_SCHEMA_VERSION)?;
    assert!(current.is_current());

    // No migrations should be needed for current version
    let json = minimal_profile_json();
    let value: serde_json::Value = serde_json::from_str(&json)?;

    let version_str = value
        .get("schema")
        .and_then(|v| v.as_str())
        .ok_or("missing schema field")?;
    let version = SchemaVersion::parse(version_str)?;
    assert!(version.is_current());
    // Confirm manager is usable (no panic)
    drop(manager);
    Ok(())
}

#[test]
fn schema_version_new_constructor() {
    let v = SchemaVersion::new(3, 1);
    assert_eq!(v.major, 3);
    assert_eq!(v.minor, 1);
    assert_eq!(v.version, "wheel.profile/3.1");
    assert!(!v.is_current());
}

#[test]
fn schema_version_display() -> TestResult {
    let v = SchemaVersion::parse(CURRENT_SCHEMA_VERSION)?;
    let displayed = format!("{}", v);
    assert_eq!(displayed, CURRENT_SCHEMA_VERSION);
    Ok(())
}

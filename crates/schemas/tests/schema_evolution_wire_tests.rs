//! Schema evolution & wire format stability tests.
//!
//! Covers:
//! 1. Snapshot tests for all key types (config, entities, telemetry, IPC)
//! 2. Wire format backward compatibility with golden v1 data
//! 3. Schema migration tests (v0 → v1, data preservation, unknown fields)
//! 4. JSON Schema validation (valid, invalid, edge cases)

use racing_wheel_schemas::config::{
    BaseConfig, BumpstopConfig as CfgBumpstop, CurvePoint as CfgCurvePoint,
    FilterConfig as CfgFilter, HandsOffConfig as CfgHandsOff, HapticsConfig, LedConfig,
    NotchFilter as CfgNotch, ProfileMigrator, ProfileSchema, ProfileScope as CfgScope,
    ProfileValidator,
};
use racing_wheel_schemas::domain::TorqueNm;
use racing_wheel_schemas::entities::{
    CalibrationData, CalibrationType, Device, DeviceCapabilities, DeviceState, DeviceType,
    FilterConfig, PedalCalibrationData,
};
use racing_wheel_schemas::migration::{MigrationConfig, MigrationManager, SchemaVersion};
use racing_wheel_schemas::telemetry::{
    NormalizedTelemetry, TelemetryData, TelemetryFlags, TelemetryFrame, TelemetrySnapshot,
    TelemetryValue,
};
use std::collections::BTreeMap;

type Err = Box<dyn std::error::Error + Send + Sync>;

// ═══════════════════════════════════════════════════════════════════════
// 1. SNAPSHOT TESTS — lock JSON wire format for all key types
// ═══════════════════════════════════════════════════════════════════════

// -- Configuration types --

#[test]
fn snap_profile_schema_minimal() -> Result<(), Err> {
    let profile = ProfileSchema {
        schema: "wheel.profile/1".into(),
        scope: CfgScope {
            game: None,
            car: None,
            track: None,
        },
        base: BaseConfig {
            ffb_gain: 0.75,
            dor_deg: 900,
            torque_cap_nm: 15.0,
            filters: CfgFilter {
                reconstruction: 3,
                friction: 0.1,
                damper: 0.2,
                inertia: 0.05,
                bumpstop: CfgBumpstop::default(),
                hands_off: CfgHandsOff::default(),
                torque_cap: None,
                notch_filters: vec![],
                slew_rate: 0.8,
                curve_points: vec![
                    CfgCurvePoint {
                        input: 0.0,
                        output: 0.0,
                    },
                    CfgCurvePoint {
                        input: 1.0,
                        output: 1.0,
                    },
                ],
            },
        },
        leds: None,
        haptics: None,
        signature: None,
    };
    let val = serde_json::to_value(&profile)?;
    insta::assert_json_snapshot!("wire_profile_schema_minimal", val);
    Ok(())
}

#[test]
fn snap_profile_schema_full() -> Result<(), Err> {
    let mut colors = std::collections::HashMap::new();
    colors.insert("green".into(), [0u8, 255, 0]);
    colors.insert("red".into(), [255, 0, 0]);

    let mut effects = std::collections::HashMap::new();
    effects.insert("abs".into(), true);
    effects.insert("tc".into(), false);

    let profile = ProfileSchema {
        schema: "wheel.profile/1".into(),
        scope: CfgScope {
            game: Some("iRacing".into()),
            car: Some("mx5".into()),
            track: Some("laguna-seca".into()),
        },
        base: BaseConfig {
            ffb_gain: 0.85,
            dor_deg: 540,
            torque_cap_nm: 20.0,
            filters: CfgFilter {
                reconstruction: 5,
                friction: 0.15,
                damper: 0.25,
                inertia: 0.1,
                bumpstop: CfgBumpstop {
                    enabled: true,
                    strength: 0.7,
                },
                hands_off: CfgHandsOff {
                    enabled: false,
                    sensitivity: 0.5,
                },
                torque_cap: Some(18.0),
                notch_filters: vec![CfgNotch {
                    hz: 60.0,
                    q: 2.0,
                    gain_db: -12.0,
                }],
                slew_rate: 0.6,
                curve_points: vec![
                    CfgCurvePoint {
                        input: 0.0,
                        output: 0.0,
                    },
                    CfgCurvePoint {
                        input: 0.5,
                        output: 0.7,
                    },
                    CfgCurvePoint {
                        input: 1.0,
                        output: 1.0,
                    },
                ],
            },
        },
        leds: Some(LedConfig {
            rpm_bands: vec![0.6, 0.75, 0.9],
            pattern: "progressive".into(),
            brightness: 0.8,
            colors: Some(colors),
        }),
        haptics: Some(HapticsConfig {
            enabled: true,
            intensity: 0.6,
            frequency_hz: 150.0,
            effects: Some(effects),
        }),
        signature: Some("sig-abc-123".into()),
    };
    let val = serde_json::to_value(&profile)?;
    insta::assert_json_snapshot!("wire_profile_schema_full", val);
    Ok(())
}

#[test]
fn snap_filter_config_defaults() -> Result<(), Err> {
    let fc = CfgFilter::default();
    let val = serde_json::to_value(&fc)?;
    insta::assert_json_snapshot!("wire_cfg_filter_defaults", val);
    Ok(())
}

// -- Entity types --

#[test]
fn snap_device_capabilities() -> Result<(), Err> {
    let caps = DeviceCapabilities::new(
        true,  // pid
        true,  // raw_torque_1khz
        true,  // health_stream
        false, // led_bus
        TorqueNm::new(25.0)?,
        4096, // encoder cpr
        1000, // min_report_period_us
    );
    let val = serde_json::to_value(&caps)?;
    insta::assert_json_snapshot!("wire_device_capabilities", val);
    Ok(())
}

#[test]
fn snap_calibration_data_full() -> Result<(), Err> {
    let cal = CalibrationData {
        center_position: Some(0.0),
        min_position: Some(-540.0),
        max_position: Some(540.0),
        pedal_ranges: Some(PedalCalibrationData {
            throttle: Some((0.0, 1.0)),
            brake: Some((0.05, 0.95)),
            clutch: Some((0.1, 0.9)),
        }),
        calibrated_at: Some("2024-01-15T10:30:00Z".into()),
        calibration_type: CalibrationType::Full,
    };
    let val = serde_json::to_value(&cal)?;
    insta::assert_json_snapshot!("wire_calibration_data_full", val);
    Ok(())
}

#[test]
fn snap_device_entity() -> Result<(), Err> {
    let dev = Device::new(
        "dd-pro-001".parse()?,
        "Fanatec DD Pro".into(),
        DeviceType::WheelBase,
        DeviceCapabilities::new(true, true, true, true, TorqueNm::new(8.0)?, 4096, 1000),
    );
    let val = serde_json::to_value(&dev)?;
    insta::assert_json_snapshot!("wire_device_entity", val);
    Ok(())
}

#[test]
fn snap_entity_filter_config_default() -> Result<(), Err> {
    let fc = FilterConfig::default();
    let val = serde_json::to_value(&fc)?;
    insta::assert_json_snapshot!("wire_entity_filter_config_default", val);
    Ok(())
}

// -- Telemetry types --

#[test]
fn snap_telemetry_frame() -> Result<(), Err> {
    let frame = TelemetryFrame::new(
        NormalizedTelemetry::builder()
            .speed_ms(33.0)
            .rpm(5000.0)
            .gear(3)
            .build(),
        1_000_000, // 1ms
        42,
        256,
    );
    let val = serde_json::to_value(&frame)?;
    insta::assert_json_snapshot!("wire_telemetry_frame", val);
    Ok(())
}

#[test]
fn snap_telemetry_data() -> Result<(), Err> {
    let data = TelemetryData {
        wheel_angle_deg: 45.5,
        wheel_speed_rad_s: 1.2,
        temperature_c: 42,
        fault_flags: 0,
        hands_on: true,
        timestamp: 12345,
    };
    let val = serde_json::to_value(&data)?;
    insta::assert_json_snapshot!("wire_telemetry_data", val);
    Ok(())
}

#[test]
fn snap_telemetry_snapshot() -> Result<(), Err> {
    let snap = TelemetrySnapshot {
        timestamp_ns: 5_000_000,
        speed_ms: 22.0,
        steering_angle: 0.1,
        throttle: 0.6,
        brake: 0.0,
        clutch: 0.0,
        rpm: 4500.0,
        max_rpm: 8000.0,
        gear: 3,
        num_gears: 6,
        lateral_g: 0.3,
        longitudinal_g: 0.1,
        vertical_g: 0.0,
        slip_ratio: 0.02,
        slip_angle_fl: 0.005,
        slip_angle_fr: 0.006,
        slip_angle_rl: 0.004,
        slip_angle_rr: 0.003,
        ffb_scalar: 0.5,
        ffb_torque_nm: 6.0,
        flags: TelemetryFlags::default(),
        position: 5,
        lap: 3,
        current_lap_time_s: 82.5,
        fuel_percent: 0.65,
        sequence: 100,
    };
    let val = serde_json::to_value(&snap)?;
    insta::assert_json_snapshot!("wire_telemetry_snapshot", val);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// 2. WIRE FORMAT BACKWARD COMPATIBILITY — golden v1 data
// ═══════════════════════════════════════════════════════════════════════

/// Golden v1 profile JSON representing a stable wire format.
/// If any change breaks deserialization of this data, the test catches it.
const GOLDEN_V1_PROFILE: &str = r#"{
    "schema": "wheel.profile/1",
    "scope": {
        "game": "Assetto Corsa Competizione",
        "car": "Ferrari 488 GT3",
        "track": null
    },
    "base": {
        "ffbGain": 0.8,
        "dorDeg": 900,
        "torqueCapNm": 15.0,
        "filters": {
            "reconstruction": 3,
            "friction": 0.1,
            "damper": 0.2,
            "inertia": 0.05,
            "bumpstop": { "enabled": true, "strength": 0.5 },
            "handsOff": { "enabled": true, "sensitivity": 0.3 },
            "notchFilters": [
                { "hz": 60.0, "q": 2.0, "gainDb": -12.0 }
            ],
            "slewRate": 0.8,
            "curvePoints": [
                { "input": 0.0, "output": 0.0 },
                { "input": 0.5, "output": 0.6 },
                { "input": 1.0, "output": 1.0 }
            ]
        }
    },
    "leds": {
        "rpmBands": [0.7, 0.85, 0.95],
        "pattern": "progressive",
        "brightness": 0.9
    },
    "haptics": {
        "enabled": true,
        "intensity": 0.5,
        "frequencyHz": 120.0
    },
    "signature": "golden-v1-sig"
}"#;

#[test]
fn golden_v1_profile_deserializes() -> Result<(), Err> {
    let profile: ProfileSchema = serde_json::from_str(GOLDEN_V1_PROFILE)?;
    assert_eq!(profile.schema, "wheel.profile/1");
    assert_eq!(
        profile.scope.game.as_deref(),
        Some("Assetto Corsa Competizione")
    );
    assert_eq!(profile.scope.car.as_deref(), Some("Ferrari 488 GT3"));
    assert!(profile.scope.track.is_none());
    assert!((profile.base.ffb_gain - 0.8).abs() < f32::EPSILON);
    assert_eq!(profile.base.dor_deg, 900);
    assert!((profile.base.torque_cap_nm - 15.0).abs() < f32::EPSILON);
    assert_eq!(profile.base.filters.reconstruction, 3);
    assert_eq!(profile.base.filters.notch_filters.len(), 1);
    assert_eq!(profile.base.filters.curve_points.len(), 3);
    assert!(profile.leds.is_some());
    assert!(profile.haptics.is_some());
    assert_eq!(profile.signature.as_deref(), Some("golden-v1-sig"));
    Ok(())
}

#[test]
fn golden_v1_profile_round_trips() -> Result<(), Err> {
    let profile: ProfileSchema = serde_json::from_str(GOLDEN_V1_PROFILE)?;
    let json = serde_json::to_string(&profile)?;
    let profile2: ProfileSchema = serde_json::from_str(&json)?;
    // Compare key fields (ProfileSchema doesn't derive PartialEq)
    assert_eq!(profile.schema, profile2.schema);
    assert_eq!(profile.base.dor_deg, profile2.base.dor_deg);
    assert!((profile.base.ffb_gain - profile2.base.ffb_gain).abs() < f32::EPSILON);
    assert_eq!(
        profile.base.filters.notch_filters.len(),
        profile2.base.filters.notch_filters.len()
    );
    Ok(())
}

#[test]
fn golden_v1_profile_validates_against_schema() -> Result<(), Err> {
    let validator = ProfileValidator::new()?;
    let result = validator.validate_json(GOLDEN_V1_PROFILE);
    assert!(result.is_ok(), "golden v1 profile must pass validation");
    Ok(())
}

/// Golden v1 telemetry frame — minimal required fields only.
const GOLDEN_V1_TELEMETRY: &str = r#"{
    "speed_ms": 55.2,
    "steering_angle": -0.15,
    "throttle": 0.9,
    "brake": 0.0,
    "rpm": 7200.0,
    "gear": 5,
    "flags": { "green_flag": true },
    "sequence": 1001
}"#;

#[test]
fn golden_v1_telemetry_deserializes() -> Result<(), Err> {
    let t: NormalizedTelemetry = serde_json::from_str(GOLDEN_V1_TELEMETRY)?;
    assert!((t.speed_ms - 55.2).abs() < f32::EPSILON);
    assert_eq!(t.gear, 5);
    assert!(t.flags.green_flag);
    // Optional fields absent → default
    assert_eq!(t.clutch, 0.0);
    assert_eq!(t.max_rpm, 0.0);
    assert!(t.car_id.is_none());
    assert!(t.extended.is_empty());
    Ok(())
}

/// Golden v1 TelemetryData (device-level).
const GOLDEN_V1_DEVICE_TELEMETRY: &str = r#"{
    "wheel_angle_deg": 120.5,
    "wheel_speed_rad_s": 2.1,
    "temperature_c": 38,
    "fault_flags": 0,
    "hands_on": true,
    "timestamp": 99999
}"#;

#[test]
fn golden_v1_device_telemetry_deserializes() -> Result<(), Err> {
    let td: TelemetryData = serde_json::from_str(GOLDEN_V1_DEVICE_TELEMETRY)?;
    assert!((td.wheel_angle_deg - 120.5).abs() < f32::EPSILON);
    assert_eq!(td.temperature_c, 38);
    assert!(td.hands_on);
    assert_eq!(td.fault_flags, 0);
    Ok(())
}

// -- Adding optional fields doesn't break old data --

#[test]
fn v1_telemetry_with_extra_unknown_fields_still_parses() -> Result<(), Err> {
    let json = r#"{
        "speed_ms": 10.0,
        "steering_angle": 0.0,
        "throttle": 0.0,
        "brake": 0.0,
        "rpm": 0.0,
        "gear": 0,
        "flags": {},
        "sequence": 0,
        "future_field_v2": "some-value",
        "another_v3_field": 42
    }"#;
    let t: NormalizedTelemetry = serde_json::from_str(json)?;
    assert!((t.speed_ms - 10.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn v1_profile_with_extra_fields_at_root_rejected_by_schema() -> Result<(), Err> {
    // The JSON schema has additionalProperties: false at root
    let json = r#"{
        "schema": "wheel.profile/1",
        "scope": {},
        "base": {
            "ffbGain": 0.5,
            "dorDeg": 900,
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
        "unknownRootField": true
    }"#;
    let validator = ProfileValidator::new()?;
    let result = validator.validate_json(json);
    assert!(
        result.is_err(),
        "extra root field must be rejected by JSON schema"
    );
    Ok(())
}

// -- Required field removal is detected --

#[test]
fn missing_required_speed_ms_fails() {
    let json = r#"{
        "steering_angle": 0.0,
        "throttle": 0.0,
        "brake": 0.0,
        "rpm": 0.0,
        "gear": 0,
        "flags": {},
        "sequence": 0
    }"#;
    let result: Result<NormalizedTelemetry, _> = serde_json::from_str(json);
    assert!(result.is_err(), "missing speed_ms must fail");
}

#[test]
fn missing_required_profile_base_fails() {
    let json = r#"{
        "schema": "wheel.profile/1",
        "scope": {}
    }"#;
    let result: Result<ProfileSchema, _> = serde_json::from_str(json);
    assert!(result.is_err(), "missing 'base' must fail");
}

#[test]
fn missing_required_profile_schema_field_fails() {
    let json = r#"{
        "scope": {},
        "base": {
            "ffbGain": 0.5,
            "dorDeg": 900,
            "torqueCapNm": 10.0,
            "filters": {
                "reconstruction": 0, "friction": 0.0, "damper": 0.0, "inertia": 0.0,
                "notchFilters": [], "slewRate": 1.0,
                "curvePoints": [{"input":0.0,"output":0.0},{"input":1.0,"output":1.0}]
            }
        }
    }"#;
    let result: Result<ProfileSchema, _> = serde_json::from_str(json);
    assert!(result.is_err(), "missing 'schema' must fail");
}

// ═══════════════════════════════════════════════════════════════════════
// 3. SCHEMA MIGRATION TESTS
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn migrate_legacy_v0_to_v1_preserves_gain() -> Result<(), Err> {
    let legacy = r#"{"ffb_gain": 0.85, "degrees_of_rotation": 720, "torque_cap": 12.0}"#;
    let manager = MigrationManager::new(MigrationConfig::without_backups())?;
    let migrated_json = manager.migrate_profile(legacy)?;
    let val: serde_json::Value = serde_json::from_str(&migrated_json)?;

    assert_eq!(val["schema"], "wheel.profile/1");
    assert!((val["base"]["ffbGain"].as_f64().unwrap_or(0.0) - 0.85).abs() < 0.001);
    assert_eq!(val["base"]["dorDeg"], 720);
    assert!((val["base"]["torqueCapNm"].as_f64().unwrap_or(0.0) - 12.0).abs() < 0.001);
    Ok(())
}

#[test]
fn migrate_legacy_v0_creates_scope() -> Result<(), Err> {
    let legacy = r#"{"ffb_gain": 0.7, "degrees_of_rotation": 900}"#;
    let manager = MigrationManager::new(MigrationConfig::without_backups())?;
    let migrated_json = manager.migrate_profile(legacy)?;
    let val: serde_json::Value = serde_json::from_str(&migrated_json)?;

    assert!(
        val["scope"].is_object(),
        "migration must create 'scope' object"
    );
    Ok(())
}

#[test]
fn migrate_legacy_v0_creates_default_filters() -> Result<(), Err> {
    let legacy = r#"{"ffb_gain": 0.5, "degrees_of_rotation": 1080}"#;
    let manager = MigrationManager::new(MigrationConfig::without_backups())?;
    let migrated_json = manager.migrate_profile(legacy)?;
    let val: serde_json::Value = serde_json::from_str(&migrated_json)?;

    let filters = &val["base"]["filters"];
    assert_eq!(filters["reconstruction"], 0);
    assert_eq!(filters["friction"], 0.0);
    assert_eq!(filters["damper"], 0.0);
    assert!(filters["curvePoints"].is_array());
    assert_eq!(filters["curvePoints"].as_array().map(|a| a.len()), Some(2));
    Ok(())
}

#[test]
fn migrate_v1_is_noop() -> Result<(), Err> {
    let v1_json = r#"{
        "schema": "wheel.profile/1",
        "scope": { "game": "test" },
        "base": { "ffbGain": 0.5, "dorDeg": 900, "torqueCapNm": 10.0,
            "filters": { "reconstruction": 0, "friction": 0.0, "damper": 0.0,
                "inertia": 0.0, "notchFilters": [], "slewRate": 1.0,
                "curvePoints": [{"input":0.0,"output":0.0},{"input":1.0,"output":1.0}]
            }
        }
    }"#;
    let manager = MigrationManager::new(MigrationConfig::without_backups())?;
    let migrated = manager.migrate_profile(v1_json)?;
    let val: serde_json::Value = serde_json::from_str(&migrated)?;
    assert_eq!(val["scope"]["game"], "test");
    Ok(())
}

#[test]
fn migrate_preserves_extra_legacy_fields_in_object() -> Result<(), Err> {
    // Unknown fields in legacy format should survive migration (forward compat)
    let legacy = r#"{
        "ffb_gain": 0.7,
        "degrees_of_rotation": 900,
        "user_custom_note": "my custom note"
    }"#;
    let manager = MigrationManager::new(MigrationConfig::without_backups())?;
    let migrated_json = manager.migrate_profile(legacy)?;
    let val: serde_json::Value = serde_json::from_str(&migrated_json)?;

    // The migration should preserve the unknown field at root level
    assert_eq!(
        val["user_custom_note"].as_str(),
        Some("my custom note"),
        "unknown fields should be preserved through migration"
    );
    Ok(())
}

#[test]
fn schema_version_v0_detected_for_legacy() -> Result<(), Err> {
    let manager = MigrationManager::new(MigrationConfig::without_backups())?;
    let v = manager.detect_version(r#"{"ffb_gain": 0.5}"#)?;
    assert_eq!(v.major, 0);
    assert_eq!(v.minor, 0);
    Ok(())
}

#[test]
fn schema_version_v1_detected_for_current() -> Result<(), Err> {
    let manager = MigrationManager::new(MigrationConfig::without_backups())?;
    let v = manager.detect_version(r#"{"schema": "wheel.profile/1", "scope": {}, "base": {}}"#)?;
    assert!(v.is_current());
    assert_eq!(v.major, 1);
    Ok(())
}

#[test]
fn unknown_schema_version_is_error() {
    let result = SchemaVersion::parse("unknown.schema/99");
    assert!(result.is_err());
}

#[test]
fn profile_migrator_rejects_unknown_version() {
    let json = r#"{
        "schema": "wheel.profile/999",
        "scope": {},
        "base": { "ffbGain": 0.5, "dorDeg": 900, "torqueCapNm": 10.0,
            "filters": { "reconstruction": 0, "friction": 0.0, "damper": 0.0,
                "inertia": 0.0, "notchFilters": [], "slewRate": 1.0,
                "curvePoints": [{"input":0.0,"output":0.0},{"input":1.0,"output":1.0}]
            }
        }
    }"#;
    let result = ProfileMigrator::migrate_profile(json);
    assert!(result.is_err(), "unknown schema version must be rejected");
}

// ═══════════════════════════════════════════════════════════════════════
// 4. JSON SCHEMA VALIDATION
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn valid_minimal_profile_passes_validation() -> Result<(), Err> {
    let json = r#"{
        "schema": "wheel.profile/1",
        "scope": {},
        "base": {
            "ffbGain": 0.5,
            "dorDeg": 900,
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
    }"#;
    let validator = ProfileValidator::new()?;
    let profile = validator.validate_json(json)?;
    assert_eq!(profile.schema, "wheel.profile/1");
    Ok(())
}

#[test]
fn empty_json_object_rejected() -> Result<(), Err> {
    let validator = ProfileValidator::new()?;
    let result = validator.validate_json("{}");
    assert!(result.is_err(), "empty object must fail schema validation");
    Ok(())
}

#[test]
fn null_json_rejected() -> Result<(), Err> {
    let validator = ProfileValidator::new()?;
    let result = validator.validate_json("null");
    assert!(result.is_err(), "null must fail schema validation");
    Ok(())
}

#[test]
fn invalid_ffb_gain_above_max_rejected() -> Result<(), Err> {
    let json = r#"{
        "schema": "wheel.profile/1",
        "scope": {},
        "base": {
            "ffbGain": 1.5,
            "dorDeg": 900,
            "torqueCapNm": 10.0,
            "filters": {
                "reconstruction": 0, "friction": 0.0, "damper": 0.0, "inertia": 0.0,
                "notchFilters": [], "slewRate": 1.0,
                "curvePoints": [{"input":0.0,"output":0.0},{"input":1.0,"output":1.0}]
            }
        }
    }"#;
    let validator = ProfileValidator::new()?;
    let result = validator.validate_json(json);
    assert!(result.is_err(), "ffbGain > 1.0 must be rejected");
    Ok(())
}

#[test]
fn invalid_dor_below_min_rejected() -> Result<(), Err> {
    let json = r#"{
        "schema": "wheel.profile/1",
        "scope": {},
        "base": {
            "ffbGain": 0.5,
            "dorDeg": 10,
            "torqueCapNm": 10.0,
            "filters": {
                "reconstruction": 0, "friction": 0.0, "damper": 0.0, "inertia": 0.0,
                "notchFilters": [], "slewRate": 1.0,
                "curvePoints": [{"input":0.0,"output":0.0},{"input":1.0,"output":1.0}]
            }
        }
    }"#;
    let validator = ProfileValidator::new()?;
    let result = validator.validate_json(json);
    assert!(result.is_err(), "dorDeg < 180 must be rejected");
    Ok(())
}

#[test]
fn wrong_schema_version_rejected() -> Result<(), Err> {
    let json = r#"{
        "schema": "wheel.profile/2",
        "scope": {},
        "base": {
            "ffbGain": 0.5,
            "dorDeg": 900,
            "torqueCapNm": 10.0,
            "filters": {
                "reconstruction": 0, "friction": 0.0, "damper": 0.0, "inertia": 0.0,
                "notchFilters": [], "slewRate": 1.0,
                "curvePoints": [{"input":0.0,"output":0.0},{"input":1.0,"output":1.0}]
            }
        }
    }"#;
    let validator = ProfileValidator::new()?;
    let result = validator.validate_json(json);
    assert!(
        result.is_err(),
        "schema version 2 must be rejected by v1 validator"
    );
    Ok(())
}

#[test]
fn non_monotonic_curve_points_rejected() -> Result<(), Err> {
    let json = r#"{
        "schema": "wheel.profile/1",
        "scope": {},
        "base": {
            "ffbGain": 0.5,
            "dorDeg": 900,
            "torqueCapNm": 10.0,
            "filters": {
                "reconstruction": 0, "friction": 0.0, "damper": 0.0, "inertia": 0.0,
                "notchFilters": [], "slewRate": 1.0,
                "curvePoints": [
                    {"input": 0.0, "output": 0.0},
                    {"input": 0.5, "output": 0.8},
                    {"input": 0.3, "output": 0.9},
                    {"input": 1.0, "output": 1.0}
                ]
            }
        }
    }"#;
    let validator = ProfileValidator::new()?;
    let result = validator.validate_json(json);
    assert!(
        result.is_err(),
        "non-monotonic curve points must be rejected"
    );
    Ok(())
}

#[test]
fn unsorted_rpm_bands_rejected() -> Result<(), Err> {
    let json = r#"{
        "schema": "wheel.profile/1",
        "scope": {},
        "base": {
            "ffbGain": 0.5,
            "dorDeg": 900,
            "torqueCapNm": 10.0,
            "filters": {
                "reconstruction": 0, "friction": 0.0, "damper": 0.0, "inertia": 0.0,
                "notchFilters": [], "slewRate": 1.0,
                "curvePoints": [{"input":0.0,"output":0.0},{"input":1.0,"output":1.0}]
            }
        },
        "leds": {
            "rpmBands": [0.9, 0.7, 0.5],
            "pattern": "progressive",
            "brightness": 0.8
        }
    }"#;
    let validator = ProfileValidator::new()?;
    let result = validator.validate_json(json);
    assert!(result.is_err(), "unsorted RPM bands must be rejected");
    Ok(())
}

#[test]
fn too_few_curve_points_rejected() -> Result<(), Err> {
    let json = r#"{
        "schema": "wheel.profile/1",
        "scope": {},
        "base": {
            "ffbGain": 0.5,
            "dorDeg": 900,
            "torqueCapNm": 10.0,
            "filters": {
                "reconstruction": 0, "friction": 0.0, "damper": 0.0, "inertia": 0.0,
                "notchFilters": [], "slewRate": 1.0,
                "curvePoints": [{"input": 0.0, "output": 0.0}]
            }
        }
    }"#;
    let validator = ProfileValidator::new()?;
    let result = validator.validate_json(json);
    assert!(result.is_err(), "less than 2 curve points must be rejected");
    Ok(())
}

#[test]
fn additional_properties_in_filters_rejected() -> Result<(), Err> {
    let json = r#"{
        "schema": "wheel.profile/1",
        "scope": {},
        "base": {
            "ffbGain": 0.5,
            "dorDeg": 900,
            "torqueCapNm": 10.0,
            "filters": {
                "reconstruction": 0, "friction": 0.0, "damper": 0.0, "inertia": 0.0,
                "notchFilters": [], "slewRate": 1.0,
                "curvePoints": [{"input":0.0,"output":0.0},{"input":1.0,"output":1.0}],
                "unknownFilter": true
            }
        }
    }"#;
    let validator = ProfileValidator::new()?;
    let result = validator.validate_json(json);
    assert!(
        result.is_err(),
        "additional properties in filters must be rejected"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// Wire format stability for device enums
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn device_state_serialization_stable() -> Result<(), Err> {
    let states = vec![
        DeviceState::Disconnected,
        DeviceState::Connected,
        DeviceState::Active,
        DeviceState::Faulted,
        DeviceState::SafeMode,
    ];
    let val = serde_json::to_value(&states)?;
    insta::assert_json_snapshot!("wire_device_state_all", val);
    Ok(())
}

#[test]
fn device_type_serialization_stable() -> Result<(), Err> {
    let types = vec![
        DeviceType::Other,
        DeviceType::WheelBase,
        DeviceType::SteeringWheel,
        DeviceType::Pedals,
        DeviceType::Shifter,
        DeviceType::Handbrake,
        DeviceType::ButtonBox,
    ];
    let val = serde_json::to_value(&types)?;
    insta::assert_json_snapshot!("wire_device_type_all", val);
    Ok(())
}

#[test]
fn calibration_type_serialization_stable() -> Result<(), Err> {
    let types = vec![
        CalibrationType::Center,
        CalibrationType::Range,
        CalibrationType::Pedals,
        CalibrationType::Full,
    ];
    let val = serde_json::to_value(&types)?;
    insta::assert_json_snapshot!("wire_calibration_type_all", val);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// Wire format stability for TelemetryValue tagged enum
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn telemetry_value_tagged_format_stable() -> Result<(), Err> {
    let vals: BTreeMap<&str, TelemetryValue> = BTreeMap::from([
        ("float", TelemetryValue::Float(42.5)),
        ("int", TelemetryValue::Integer(-42)),
        ("bool", TelemetryValue::Boolean(true)),
        ("str", TelemetryValue::String("hello".into())),
    ]);
    let val = serde_json::to_value(&vals)?;
    insta::assert_json_snapshot!("wire_telemetry_value_tagged", val);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// Cross-version deserialization: old consumer, new producer
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn old_telemetry_consumer_ignores_new_flags() -> Result<(), Err> {
    // Simulate a newer producer adding extra flag fields
    let json_with_future_flags = r#"{
        "speed_ms": 30.0,
        "steering_angle": 0.0,
        "throttle": 0.5,
        "brake": 0.0,
        "rpm": 3000.0,
        "gear": 3,
        "flags": {
            "green_flag": true,
            "future_flag_v2": true,
            "another_future_flag": false
        },
        "sequence": 10
    }"#;
    let t: NormalizedTelemetry = serde_json::from_str(json_with_future_flags)?;
    assert!(t.flags.green_flag);
    assert!((t.speed_ms - 30.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn telemetry_snapshot_backward_compat() -> Result<(), Err> {
    // Minimal TelemetrySnapshot from older version (no optional fields)
    let json = r#"{
        "timestamp_ns": 1000000,
        "speed_ms": 20.0,
        "steering_angle": 0.0,
        "throttle": 0.5,
        "brake": 0.1,
        "rpm": 4000.0,
        "gear": 3,
        "flags": {},
        "sequence": 5
    }"#;
    let snap: TelemetrySnapshot = serde_json::from_str(json)?;
    assert_eq!(snap.timestamp_ns, 1_000_000);
    assert!((snap.speed_ms - 20.0).abs() < f32::EPSILON);
    assert_eq!(snap.clutch, 0.0); // default
    assert_eq!(snap.max_rpm, 0.0); // default
    assert_eq!(snap.num_gears, 0); // default
    Ok(())
}

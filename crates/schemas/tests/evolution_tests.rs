//! Schema evolution and backward compatibility tests.
//!
//! Verifies that schema changes are backward-compatible:
//! - Old JSON payloads still parse after new optional fields are added
//! - Missing optional fields get correct defaults
//! - Round-trip serialization preserves data
//! - Extended telemetry map preserves arbitrary keys
//! - All TelemetryValue variants serialize/deserialize correctly
//! - Builder covers all fields
//! - Snapshot tests lock serialization format

use racing_wheel_schemas::migration::{
    MigrationConfig, MigrationManager, SchemaVersion, CURRENT_SCHEMA_VERSION, SCHEMA_VERSION_V2,
};
use racing_wheel_schemas::telemetry::{
    NormalizedTelemetry, NormalizedTelemetryBuilder, TelemetryFlags, TelemetryValue,
};
use std::collections::BTreeMap;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ──────────────────────────────────────────────────────────────────────
// Backward compatibility: old JSON still parses
// ──────────────────────────────────────────────────────────────────────

/// Minimal JSON with only the required (non-defaulted) fields.
/// This simulates data produced by an older version of the software
/// before optional fields were added.
#[test]
fn old_json_without_optional_fields_parses() -> TestResult {
    let old_json = r#"{
        "speed_ms": 42.0,
        "steering_angle": 0.1,
        "throttle": 0.8,
        "brake": 0.2,
        "rpm": 6500.0,
        "gear": 4,
        "flags": {},
        "sequence": 1
    }"#;

    let t: NormalizedTelemetry = serde_json::from_str(old_json)?;
    assert!((t.speed_ms - 42.0).abs() < f32::EPSILON);
    assert_eq!(t.gear, 4);
    // Fields not present in old JSON use their defaults
    assert_eq!(t.clutch, 0.0);
    assert_eq!(t.max_rpm, 0.0);
    assert_eq!(t.num_gears, 0);
    assert_eq!(t.lateral_g, 0.0);
    assert!(t.car_id.is_none());
    assert!(t.track_id.is_none());
    assert!(t.session_id.is_none());
    assert!(t.extended.is_empty());
    Ok(())
}

/// JSON from a hypothetical older version that has no flags at all.
#[test]
fn json_missing_flags_uses_defaults() -> TestResult {
    let json = r#"{
        "speed_ms": 10.0,
        "steering_angle": 0.0,
        "throttle": 0.0,
        "brake": 0.0,
        "rpm": 0.0,
        "gear": 0,
        "sequence": 0
    }"#;

    let t: NormalizedTelemetry = serde_json::from_str(json)?;
    // Default flags: green_flag=true, everything else false
    assert!(t.flags.green_flag);
    assert!(!t.flags.yellow_flag);
    assert!(!t.flags.red_flag);
    assert!(!t.flags.blue_flag);
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// Default values for missing optional fields
// ──────────────────────────────────────────────────────────────────────

#[test]
fn default_normalized_telemetry_has_zero_motion() -> TestResult {
    let t = NormalizedTelemetry::default();
    assert_eq!(t.speed_ms, 0.0);
    assert_eq!(t.steering_angle, 0.0);
    assert_eq!(t.throttle, 0.0);
    assert_eq!(t.brake, 0.0);
    assert_eq!(t.clutch, 0.0);
    Ok(())
}

#[test]
fn default_normalized_telemetry_has_zero_engine() -> TestResult {
    let t = NormalizedTelemetry::default();
    assert_eq!(t.rpm, 0.0);
    assert_eq!(t.max_rpm, 0.0);
    assert_eq!(t.gear, 0);
    assert_eq!(t.num_gears, 0);
    Ok(())
}

#[test]
fn default_normalized_telemetry_has_zero_forces() -> TestResult {
    let t = NormalizedTelemetry::default();
    assert_eq!(t.lateral_g, 0.0);
    assert_eq!(t.longitudinal_g, 0.0);
    assert_eq!(t.vertical_g, 0.0);
    Ok(())
}

#[test]
fn default_optional_strings_are_none() -> TestResult {
    let t = NormalizedTelemetry::default();
    assert!(t.car_id.is_none());
    assert!(t.track_id.is_none());
    assert!(t.session_id.is_none());
    Ok(())
}

#[test]
fn default_extended_map_is_empty() -> TestResult {
    let t = NormalizedTelemetry::default();
    assert!(t.extended.is_empty());
    Ok(())
}

#[test]
fn default_flags_green_flag_is_true() -> TestResult {
    let flags = TelemetryFlags::default();
    assert!(flags.green_flag);
    assert!(!flags.yellow_flag);
    assert!(!flags.red_flag);
    assert!(!flags.blue_flag);
    assert!(!flags.checkered_flag);
    assert!(!flags.pit_limiter);
    assert!(!flags.in_pits);
    assert!(!flags.drs_available);
    assert!(!flags.drs_active);
    assert!(!flags.ers_available);
    assert!(!flags.ers_active);
    assert!(!flags.launch_control);
    assert!(!flags.traction_control);
    assert!(!flags.abs_active);
    assert!(!flags.engine_limiter);
    assert!(!flags.safety_car);
    assert!(!flags.formation_lap);
    assert!(!flags.session_paused);
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// Schema version negotiation
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
    let v0 = SchemaVersion::new(0, 0);
    let v1 = SchemaVersion::parse(CURRENT_SCHEMA_VERSION)?;
    let v1_1 = SchemaVersion::parse("wheel.profile/1.1")?;
    let v2 = SchemaVersion::parse(SCHEMA_VERSION_V2)?;

    assert!(v0.is_older_than(&v1));
    assert!(v1.is_older_than(&v1_1));
    assert!(v1_1.is_older_than(&v2));
    assert!(!v2.is_older_than(&v1));
    assert!(!v1.is_older_than(&v0));
    Ok(())
}

#[test]
fn schema_version_rejects_invalid_format() {
    let result = SchemaVersion::parse("invalid/version/format");
    assert!(result.is_err());
}

#[test]
fn schema_version_rejects_wrong_prefix() {
    let result = SchemaVersion::parse("other.schema/1");
    assert!(result.is_err());
}

#[test]
fn migration_manager_detects_current_version() -> TestResult {
    let manager = MigrationManager::new(MigrationConfig::without_backups())?;
    let v1_json = r#"{"schema": "wheel.profile/1", "scope": {}, "base": {}}"#;
    let version = manager.detect_version(v1_json)?;
    assert!(version.is_current());
    Ok(())
}

#[test]
fn migration_manager_detects_legacy_version() -> TestResult {
    let manager = MigrationManager::new(MigrationConfig::without_backups())?;
    let legacy_json = r#"{"ffb_gain": 0.7, "degrees_of_rotation": 900}"#;
    let version = manager.detect_version(legacy_json)?;
    assert_eq!(version.major, 0);
    assert_eq!(version.minor, 0);
    assert!(version.is_older_than(&SchemaVersion::parse(CURRENT_SCHEMA_VERSION)?));
    Ok(())
}

#[test]
fn current_version_does_not_need_migration() -> TestResult {
    let manager = MigrationManager::new(MigrationConfig::without_backups())?;
    let v1_json = r#"{"schema": "wheel.profile/1", "scope": {}, "base": {}}"#;
    assert!(!manager.needs_migration(v1_json)?);
    Ok(())
}

#[test]
fn legacy_version_needs_migration() -> TestResult {
    let manager = MigrationManager::new(MigrationConfig::without_backups())?;
    let legacy_json = r#"{"ffb_gain": 0.7, "degrees_of_rotation": 900}"#;
    assert!(manager.needs_migration(legacy_json)?);
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// Round-trip stability: serialize → deserialize produces identical struct
// ──────────────────────────────────────────────────────────────────────

#[test]
fn round_trip_default_telemetry() -> TestResult {
    let original = NormalizedTelemetry::default();
    let json = serde_json::to_string(&original)?;
    let deserialized: NormalizedTelemetry = serde_json::from_str(&json)?;

    // Timestamp uses Instant::now() and is skipped in serde, so compare everything else
    assert_eq!(original.speed_ms, deserialized.speed_ms);
    assert_eq!(original.steering_angle, deserialized.steering_angle);
    assert_eq!(original.throttle, deserialized.throttle);
    assert_eq!(original.brake, deserialized.brake);
    assert_eq!(original.clutch, deserialized.clutch);
    assert_eq!(original.rpm, deserialized.rpm);
    assert_eq!(original.max_rpm, deserialized.max_rpm);
    assert_eq!(original.gear, deserialized.gear);
    assert_eq!(original.num_gears, deserialized.num_gears);
    assert_eq!(original.lateral_g, deserialized.lateral_g);
    assert_eq!(original.longitudinal_g, deserialized.longitudinal_g);
    assert_eq!(original.vertical_g, deserialized.vertical_g);
    assert_eq!(original.slip_ratio, deserialized.slip_ratio);
    assert_eq!(original.slip_angle_fl, deserialized.slip_angle_fl);
    assert_eq!(original.slip_angle_fr, deserialized.slip_angle_fr);
    assert_eq!(original.slip_angle_rl, deserialized.slip_angle_rl);
    assert_eq!(original.slip_angle_rr, deserialized.slip_angle_rr);
    assert_eq!(original.tire_temps_c, deserialized.tire_temps_c);
    assert_eq!(original.tire_pressures_psi, deserialized.tire_pressures_psi);
    assert_eq!(original.ffb_scalar, deserialized.ffb_scalar);
    assert_eq!(original.ffb_torque_nm, deserialized.ffb_torque_nm);
    assert_eq!(original.flags, deserialized.flags);
    assert_eq!(original.car_id, deserialized.car_id);
    assert_eq!(original.track_id, deserialized.track_id);
    assert_eq!(original.session_id, deserialized.session_id);
    assert_eq!(original.position, deserialized.position);
    assert_eq!(original.lap, deserialized.lap);
    assert_eq!(original.current_lap_time_s, deserialized.current_lap_time_s);
    assert_eq!(original.best_lap_time_s, deserialized.best_lap_time_s);
    assert_eq!(original.last_lap_time_s, deserialized.last_lap_time_s);
    assert_eq!(original.delta_ahead_s, deserialized.delta_ahead_s);
    assert_eq!(original.delta_behind_s, deserialized.delta_behind_s);
    assert_eq!(original.fuel_percent, deserialized.fuel_percent);
    assert_eq!(original.engine_temp_c, deserialized.engine_temp_c);
    assert_eq!(original.extended, deserialized.extended);
    assert_eq!(original.sequence, deserialized.sequence);
    Ok(())
}

#[test]
fn round_trip_populated_telemetry() -> TestResult {
    let original = NormalizedTelemetry::builder()
        .speed_ms(55.0)
        .steering_angle(-0.3)
        .throttle(0.9)
        .brake(0.1)
        .clutch(0.5)
        .rpm(7200.0)
        .max_rpm(9000.0)
        .gear(5)
        .num_gears(6)
        .lateral_g(1.2)
        .longitudinal_g(-0.5)
        .vertical_g(0.02)
        .slip_ratio(0.15)
        .slip_angle_fl(0.01)
        .slip_angle_fr(0.02)
        .slip_angle_rl(0.03)
        .slip_angle_rr(0.04)
        .tire_temps_c([85, 90, 80, 82])
        .tire_pressures_psi([26.0, 26.5, 25.0, 25.5])
        .ffb_scalar(0.75)
        .ffb_torque_nm(8.5)
        .flags(TelemetryFlags {
            yellow_flag: true,
            pit_limiter: true,
            ..TelemetryFlags::default()
        })
        .car_id("porsche-911-gt3")
        .track_id("spa-francorchamps")
        .session_id("race-001")
        .position(3)
        .lap(12)
        .current_lap_time_s(91.234)
        .best_lap_time_s(89.001)
        .last_lap_time_s(90.5)
        .delta_ahead_s(-1.2)
        .delta_behind_s(0.8)
        .fuel_percent(0.45)
        .engine_temp_c(95.0)
        .extended("turbo_psi", TelemetryValue::Float(14.7))
        .extended("abs_interventions", TelemetryValue::Integer(3))
        .extended("drs_zone", TelemetryValue::Boolean(true))
        .extended("tire_compound", TelemetryValue::String("soft".into()))
        .sequence(42)
        .build();

    let json = serde_json::to_string(&original)?;
    let deserialized: NormalizedTelemetry = serde_json::from_str(&json)?;

    // Verify all serializable fields survived the round trip
    assert_eq!(original.speed_ms, deserialized.speed_ms);
    assert_eq!(original.steering_angle, deserialized.steering_angle);
    assert_eq!(original.throttle, deserialized.throttle);
    assert_eq!(original.brake, deserialized.brake);
    assert_eq!(original.clutch, deserialized.clutch);
    assert_eq!(original.rpm, deserialized.rpm);
    assert_eq!(original.max_rpm, deserialized.max_rpm);
    assert_eq!(original.gear, deserialized.gear);
    assert_eq!(original.num_gears, deserialized.num_gears);
    assert_eq!(original.lateral_g, deserialized.lateral_g);
    assert_eq!(original.longitudinal_g, deserialized.longitudinal_g);
    assert_eq!(original.vertical_g, deserialized.vertical_g);
    assert_eq!(original.slip_ratio, deserialized.slip_ratio);
    assert_eq!(original.ffb_scalar, deserialized.ffb_scalar);
    assert_eq!(original.ffb_torque_nm, deserialized.ffb_torque_nm);
    assert_eq!(original.flags, deserialized.flags);
    assert_eq!(original.car_id, deserialized.car_id);
    assert_eq!(original.track_id, deserialized.track_id);
    assert_eq!(original.session_id, deserialized.session_id);
    assert_eq!(original.position, deserialized.position);
    assert_eq!(original.lap, deserialized.lap);
    assert_eq!(original.fuel_percent, deserialized.fuel_percent);
    assert_eq!(original.engine_temp_c, deserialized.engine_temp_c);
    assert_eq!(original.extended, deserialized.extended);
    assert_eq!(original.sequence, deserialized.sequence);
    Ok(())
}

#[test]
fn round_trip_preserves_json_value_equality() -> TestResult {
    let original = NormalizedTelemetry::builder()
        .speed_ms(30.0)
        .rpm(4000.0)
        .gear(3)
        .build();

    let json1 = serde_json::to_value(&original)?;
    let deserialized: NormalizedTelemetry = serde_json::from_value(json1.clone())?;
    let json2 = serde_json::to_value(&deserialized)?;
    assert_eq!(json1, json2, "double round-trip must produce identical JSON");
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// Extended telemetry map: arbitrary keys preserved through round-trip
// ──────────────────────────────────────────────────────────────────────

#[test]
fn extended_map_preserves_keys_through_round_trip() -> TestResult {
    let mut extended = BTreeMap::new();
    extended.insert("custom_key_1".to_string(), TelemetryValue::Float(1.5));
    extended.insert("custom_key_2".to_string(), TelemetryValue::Integer(42));
    extended.insert(
        "custom_key_3".to_string(),
        TelemetryValue::String("hello".into()),
    );
    extended.insert("custom_key_4".to_string(), TelemetryValue::Boolean(false));

    let t = NormalizedTelemetry {
        extended: extended.clone(),
        ..NormalizedTelemetry::default()
    };

    let json = serde_json::to_string(&t)?;
    let deserialized: NormalizedTelemetry = serde_json::from_str(&json)?;

    assert_eq!(deserialized.extended.len(), 4);
    assert_eq!(deserialized.extended, extended);
    Ok(())
}

#[test]
fn extended_map_empty_is_omitted_in_json() -> TestResult {
    let t = NormalizedTelemetry::default();
    let json = serde_json::to_string(&t)?;
    // When extended is empty, skip_serializing_if omits it
    assert!(
        !json.contains("extended"),
        "empty extended map should be omitted from JSON"
    );
    Ok(())
}

#[test]
fn extended_map_present_when_populated() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .extended("key", TelemetryValue::Float(1.0))
        .build();

    let json = serde_json::to_string(&t)?;
    assert!(
        json.contains("extended"),
        "populated extended map should be in JSON"
    );
    Ok(())
}

#[test]
fn extended_map_with_builder_helper() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .extended("boost", TelemetryValue::Float(1.2))
        .extended("laps_left", TelemetryValue::Integer(5))
        .build();

    assert_eq!(t.extended.len(), 2);
    assert_eq!(
        t.get_extended("boost"),
        Some(&TelemetryValue::Float(1.2))
    );
    assert_eq!(
        t.get_extended("laps_left"),
        Some(&TelemetryValue::Integer(5))
    );
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// TelemetryValue variants: all serialize/deserialize correctly
// ──────────────────────────────────────────────────────────────────────

#[test]
fn telemetry_value_float_round_trip() -> TestResult {
    let val = TelemetryValue::Float(42.5);
    let json = serde_json::to_string(&val)?;
    let deserialized: TelemetryValue = serde_json::from_str(&json)?;
    assert_eq!(val, deserialized);
    Ok(())
}

#[test]
fn telemetry_value_integer_round_trip() -> TestResult {
    let val = TelemetryValue::Integer(-42);
    let json = serde_json::to_string(&val)?;
    let deserialized: TelemetryValue = serde_json::from_str(&json)?;
    assert_eq!(val, deserialized);
    Ok(())
}

#[test]
fn telemetry_value_boolean_round_trip() -> TestResult {
    for b in [true, false] {
        let val = TelemetryValue::Boolean(b);
        let json = serde_json::to_string(&val)?;
        let deserialized: TelemetryValue = serde_json::from_str(&json)?;
        assert_eq!(val, deserialized);
    }
    Ok(())
}

#[test]
fn telemetry_value_string_round_trip() -> TestResult {
    let val = TelemetryValue::String("test string with spaces".into());
    let json = serde_json::to_string(&val)?;
    let deserialized: TelemetryValue = serde_json::from_str(&json)?;
    assert_eq!(val, deserialized);
    Ok(())
}

#[test]
fn telemetry_value_tagged_json_format() -> TestResult {
    let val = TelemetryValue::Float(1.5);
    let json = serde_json::to_string(&val)?;
    // TelemetryValue uses internally-tagged representation
    assert!(json.contains("\"type\""), "tagged enum must include 'type'");
    assert!(
        json.contains("\"Float\""),
        "Float variant tag must be present"
    );
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// Builder exhaustiveness: all fields covered
// ──────────────────────────────────────────────────────────────────────

#[test]
fn builder_covers_all_fields() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .speed_ms(10.0)
        .steering_angle(0.5)
        .throttle(0.7)
        .brake(0.3)
        .clutch(0.2)
        .rpm(3000.0)
        .max_rpm(8000.0)
        .gear(3)
        .num_gears(6)
        .lateral_g(0.5)
        .longitudinal_g(0.3)
        .vertical_g(0.1)
        .slip_ratio(0.05)
        .slip_angle_fl(0.01)
        .slip_angle_fr(0.02)
        .slip_angle_rl(0.03)
        .slip_angle_rr(0.04)
        .tire_temps_c([80, 85, 78, 82])
        .tire_pressures_psi([26.0, 26.5, 25.0, 25.5])
        .ffb_scalar(0.5)
        .ffb_torque_nm(5.0)
        .flags(TelemetryFlags::default())
        .car_id("test-car")
        .track_id("test-track")
        .session_id("test-session")
        .position(1)
        .lap(5)
        .current_lap_time_s(60.0)
        .best_lap_time_s(58.0)
        .last_lap_time_s(59.0)
        .delta_ahead_s(-0.5)
        .delta_behind_s(1.0)
        .fuel_percent(0.8)
        .engine_temp_c(90.0)
        .extended("key", TelemetryValue::Float(1.0))
        .sequence(100)
        .build();

    // Verify every field was set
    assert!((t.speed_ms - 10.0).abs() < f32::EPSILON);
    assert!((t.steering_angle - 0.5).abs() < f32::EPSILON);
    assert!((t.throttle - 0.7).abs() < f32::EPSILON);
    assert!((t.brake - 0.3).abs() < f32::EPSILON);
    assert!((t.clutch - 0.2).abs() < f32::EPSILON);
    assert!((t.rpm - 3000.0).abs() < f32::EPSILON);
    assert!((t.max_rpm - 8000.0).abs() < f32::EPSILON);
    assert_eq!(t.gear, 3);
    assert_eq!(t.num_gears, 6);
    assert!((t.lateral_g - 0.5).abs() < f32::EPSILON);
    assert!((t.longitudinal_g - 0.3).abs() < f32::EPSILON);
    assert!((t.vertical_g - 0.1).abs() < f32::EPSILON);
    assert!((t.slip_ratio - 0.05).abs() < f32::EPSILON);
    assert!((t.slip_angle_fl - 0.01).abs() < f32::EPSILON);
    assert!((t.slip_angle_fr - 0.02).abs() < f32::EPSILON);
    assert!((t.slip_angle_rl - 0.03).abs() < f32::EPSILON);
    assert!((t.slip_angle_rr - 0.04).abs() < f32::EPSILON);
    assert_eq!(t.tire_temps_c, [80, 85, 78, 82]);
    assert_eq!(t.tire_pressures_psi, [26.0, 26.5, 25.0, 25.5]);
    assert!((t.ffb_scalar - 0.5).abs() < f32::EPSILON);
    assert!((t.ffb_torque_nm - 5.0).abs() < f32::EPSILON);
    assert_eq!(t.car_id.as_deref(), Some("test-car"));
    assert_eq!(t.track_id.as_deref(), Some("test-track"));
    assert_eq!(t.session_id.as_deref(), Some("test-session"));
    assert_eq!(t.position, 1);
    assert_eq!(t.lap, 5);
    assert!((t.current_lap_time_s - 60.0).abs() < f32::EPSILON);
    assert!((t.best_lap_time_s - 58.0).abs() < f32::EPSILON);
    assert!((t.last_lap_time_s - 59.0).abs() < f32::EPSILON);
    assert!((t.delta_ahead_s - (-0.5)).abs() < f32::EPSILON);
    assert!((t.delta_behind_s - 1.0).abs() < f32::EPSILON);
    assert!((t.fuel_percent - 0.8).abs() < f32::EPSILON);
    assert!((t.engine_temp_c - 90.0).abs() < f32::EPSILON);
    assert_eq!(t.extended.len(), 1);
    assert_eq!(t.sequence, 100);
    Ok(())
}

#[test]
fn builder_default_produces_same_as_default() -> TestResult {
    let from_builder = NormalizedTelemetryBuilder::new().build();
    let from_default = NormalizedTelemetry::default();

    // Both should produce identical serializable data
    let json_builder = serde_json::to_value(&from_builder)?;
    let json_default = serde_json::to_value(&from_default)?;
    assert_eq!(json_builder, json_default);
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// Snapshot tests: lock serialization formats
// ──────────────────────────────────────────────────────────────────────

#[test]
fn snapshot_default_telemetry_json() -> TestResult {
    let t = NormalizedTelemetry::default();
    let value = serde_json::to_value(&t)?;
    insta::assert_json_snapshot!("evolution_default_telemetry", value);
    Ok(())
}

#[test]
fn snapshot_populated_telemetry_json() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .speed_ms(50.0)
        .rpm(6000.0)
        .max_rpm(8000.0)
        .gear(4)
        .throttle(0.8)
        .brake(0.1)
        .lateral_g(1.5)
        .car_id("ferrari_488")
        .track_id("monza")
        .extended("boost_psi", TelemetryValue::Float(12.5))
        .extended("is_wet", TelemetryValue::Boolean(false))
        .sequence(99)
        .build();

    let value = serde_json::to_value(&t)?;
    insta::assert_json_snapshot!("evolution_populated_telemetry", value);
    Ok(())
}

#[test]
fn snapshot_telemetry_flags_json() -> TestResult {
    let flags = TelemetryFlags {
        yellow_flag: true,
        blue_flag: true,
        pit_limiter: true,
        drs_available: true,
        ..TelemetryFlags::default()
    };
    let value = serde_json::to_value(&flags)?;
    insta::assert_json_snapshot!("evolution_telemetry_flags", value);
    Ok(())
}

#[test]
fn snapshot_all_telemetry_value_variants() -> TestResult {
    let variants: BTreeMap<String, TelemetryValue> = BTreeMap::from([
        ("float_val".to_string(), TelemetryValue::Float(42.5)),
        ("int_val".to_string(), TelemetryValue::Integer(-99)),
        ("bool_val".to_string(), TelemetryValue::Boolean(true)),
        (
            "str_val".to_string(),
            TelemetryValue::String("hello world".into()),
        ),
    ]);
    let value = serde_json::to_value(&variants)?;
    insta::assert_json_snapshot!("evolution_telemetry_value_variants", value);
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// Removing a field is detected (runtime check via deserialization)
// ──────────────────────────────────────────────────────────────────────

#[test]
fn unknown_fields_in_json_are_ignored_by_default() -> TestResult {
    // JSON with an extra field that doesn't exist in the struct.
    // serde's default behavior is to ignore unknown fields, preserving
    // forward compatibility (new producers, old consumers).
    let json_with_extra = r#"{
        "speed_ms": 10.0,
        "steering_angle": 0.0,
        "throttle": 0.0,
        "brake": 0.0,
        "rpm": 0.0,
        "gear": 0,
        "flags": {},
        "sequence": 0,
        "removed_future_field": 42
    }"#;

    let t: NormalizedTelemetry = serde_json::from_str(json_with_extra)?;
    assert!((t.speed_ms - 10.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn missing_required_field_causes_deserialization_error() {
    // NormalizedTelemetry requires at least speed_ms, steering_angle, etc.
    // without #[serde(default)] on the struct itself, non-defaulted fields
    // must be present. gear (i8) has no serde(default), so missing it fails.
    let json_missing_gear = r#"{
        "speed_ms": 10.0,
        "steering_angle": 0.0,
        "throttle": 0.0,
        "brake": 0.0,
        "rpm": 0.0,
        "flags": {},
        "sequence": 0
    }"#;

    let result: Result<NormalizedTelemetry, _> = serde_json::from_str(json_missing_gear);
    assert!(result.is_err(), "missing required field 'gear' should fail");
}

// ──────────────────────────────────────────────────────────────────────
// Optional field addition preserves backward compat
// ──────────────────────────────────────────────────────────────────────

#[test]
fn optional_string_fields_absent_in_old_json() -> TestResult {
    let json = r#"{
        "speed_ms": 0.0,
        "steering_angle": 0.0,
        "throttle": 0.0,
        "brake": 0.0,
        "rpm": 0.0,
        "gear": 0,
        "flags": {},
        "sequence": 0
    }"#;

    let t: NormalizedTelemetry = serde_json::from_str(json)?;
    assert!(t.car_id.is_none());
    assert!(t.track_id.is_none());
    assert!(t.session_id.is_none());
    assert!(t.extended.is_empty());
    Ok(())
}

#[test]
fn optional_numeric_fields_default_to_zero() -> TestResult {
    let json = r#"{
        "speed_ms": 0.0,
        "steering_angle": 0.0,
        "throttle": 0.0,
        "brake": 0.0,
        "rpm": 0.0,
        "gear": 0,
        "flags": {},
        "sequence": 0
    }"#;

    let t: NormalizedTelemetry = serde_json::from_str(json)?;
    assert_eq!(t.clutch, 0.0);
    assert_eq!(t.max_rpm, 0.0);
    assert_eq!(t.num_gears, 0);
    assert_eq!(t.lateral_g, 0.0);
    assert_eq!(t.longitudinal_g, 0.0);
    assert_eq!(t.vertical_g, 0.0);
    assert_eq!(t.slip_ratio, 0.0);
    assert_eq!(t.ffb_scalar, 0.0);
    assert_eq!(t.ffb_torque_nm, 0.0);
    assert_eq!(t.position, 0);
    assert_eq!(t.lap, 0);
    assert_eq!(t.fuel_percent, 0.0);
    assert_eq!(t.engine_temp_c, 0.0);
    Ok(())
}

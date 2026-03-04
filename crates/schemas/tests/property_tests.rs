#![allow(clippy::redundant_closure)]
//! Property-based tests for racing-wheel-schemas.
//!
//! Tests NormalizedTelemetry builder, serde round-trips, schema versioning,
//! TelemetryValue conversions, and builder field coverage.

use proptest::prelude::*;
use racing_wheel_schemas::migration::{
    CURRENT_SCHEMA_VERSION, MigrationConfig, MigrationManager, SchemaVersion,
};
use racing_wheel_schemas::telemetry::{
    NormalizedTelemetry, NormalizedTelemetryBuilder, TelemetryFlags, TelemetryValue,
};
use std::collections::BTreeMap;

// ── Strategies ──────────────────────────────────────────────────────────────

fn finite_f32() -> impl Strategy<Value = f32> {
    prop_oneof![
        prop::num::f32::NORMAL,
        Just(0.0_f32),
        Just(1.0_f32),
        Just(-1.0_f32),
    ]
}

fn positive_f32() -> impl Strategy<Value = f32> {
    (0.0_f32..1_000_000.0_f32).prop_filter("must be finite", |v| v.is_finite())
}

fn unit_f32() -> impl Strategy<Value = f32> {
    0.0_f32..=1.0_f32
}

fn telemetry_value_strategy() -> impl Strategy<Value = TelemetryValue> {
    prop_oneof![
        finite_f32().prop_map(TelemetryValue::Float),
        any::<i32>().prop_map(TelemetryValue::Integer),
        any::<bool>().prop_map(TelemetryValue::Boolean),
        "[a-zA-Z0-9_]{0,20}".prop_map(|s| TelemetryValue::String(s)),
    ]
}

fn telemetry_flags_strategy() -> impl Strategy<Value = TelemetryFlags> {
    (
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(
            |(yellow, red, blue, checkered, green, pit, in_pits, drs_a, drs_act, ers_a)| {
                TelemetryFlags {
                    yellow_flag: yellow,
                    red_flag: red,
                    blue_flag: blue,
                    checkered_flag: checkered,
                    green_flag: green,
                    pit_limiter: pit,
                    in_pits,
                    drs_available: drs_a,
                    drs_active: drs_act,
                    ers_available: ers_a,
                    ..TelemetryFlags::default()
                }
            },
        )
}

fn normalized_telemetry_strategy() -> impl Strategy<Value = NormalizedTelemetry> {
    // Split into two groups to stay within proptest's 12-tuple limit
    let motion = (
        positive_f32(), // speed_ms
        finite_f32(),   // steering_angle
        unit_f32(),     // throttle
        unit_f32(),     // brake
        unit_f32(),     // clutch
        positive_f32(), // rpm
        positive_f32(), // max_rpm
        -1_i8..=8_i8,   // gear
        0_u8..=10_u8,   // num_gears
    );
    let extras = (
        finite_f32(), // lateral_g
        finite_f32(), // longitudinal_g
        unit_f32(),   // slip_ratio
        unit_f32(),   // ffb_scalar seed [0,1]
        finite_f32(), // ffb_torque_nm
        any::<u64>(), // sequence
        telemetry_flags_strategy(),
    );
    (motion, extras).prop_map(
        |(
            (speed, steer, throttle, brake, clutch, rpm, max_rpm, gear, num_gears),
            (lat_g, lon_g, slip, ffb_s, ffb_t, seq, flags),
        )| {
            NormalizedTelemetry::builder()
                .speed_ms(speed)
                .steering_angle(steer)
                .throttle(throttle)
                .brake(brake)
                .clutch(clutch)
                .rpm(rpm)
                .max_rpm(max_rpm)
                .gear(gear)
                .num_gears(num_gears)
                .lateral_g(lat_g)
                .longitudinal_g(lon_g)
                .slip_ratio(slip)
                .ffb_scalar(ffb_s * 2.0 - 1.0) // map [0,1] to [-1,1]
                .ffb_torque_nm(ffb_t)
                .sequence(seq)
                .flags(flags)
                .build()
        },
    )
}

// ── Builder validity tests ──────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn builder_always_produces_valid_telemetry(
        speed in positive_f32(),
        steer in finite_f32(),
        throttle in unit_f32(),
        brake in unit_f32(),
        rpm in positive_f32(),
        gear in -1_i8..=8_i8,
    ) {
        let t = NormalizedTelemetry::builder()
            .speed_ms(speed)
            .steering_angle(steer)
            .throttle(throttle)
            .brake(brake)
            .rpm(rpm)
            .gear(gear)
            .build();

        // Speed is always non-negative
        prop_assert!(t.speed_ms >= 0.0);
        // Throttle and brake are clamped to [0, 1]
        prop_assert!(t.throttle >= 0.0 && t.throttle <= 1.0);
        prop_assert!(t.brake >= 0.0 && t.brake <= 1.0);
        // RPM is always non-negative
        prop_assert!(t.rpm >= 0.0);
        // All finite values remain finite
        prop_assert!(t.speed_ms.is_finite());
        prop_assert!(t.steering_angle.is_finite());
        prop_assert!(t.throttle.is_finite());
        prop_assert!(t.brake.is_finite());
        prop_assert!(t.rpm.is_finite());
    }

    #[test]
    fn builder_rejects_nan_and_infinity(
        gear in -1_i8..=8_i8,
    ) {
        // NaN inputs should be rejected (fields stay at default 0.0)
        let t = NormalizedTelemetry::builder()
            .speed_ms(f32::NAN)
            .throttle(f32::NAN)
            .brake(f32::INFINITY)
            .rpm(f32::NEG_INFINITY)
            .gear(gear)
            .build();

        prop_assert!((t.speed_ms - 0.0).abs() < f32::EPSILON);
        prop_assert!((t.throttle - 0.0).abs() < f32::EPSILON);
        prop_assert!((t.brake - 0.0).abs() < f32::EPSILON);
        prop_assert!((t.rpm - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn builder_clamps_values_correctly(
        throttle in -10.0_f32..10.0_f32,
        brake in -10.0_f32..10.0_f32,
        clutch in -10.0_f32..10.0_f32,
        ffb in -10.0_f32..10.0_f32,
        fuel in -10.0_f32..10.0_f32,
        slip in -10.0_f32..10.0_f32,
    ) {
        let t = NormalizedTelemetry::builder()
            .throttle(throttle)
            .brake(brake)
            .clutch(clutch)
            .ffb_scalar(ffb)
            .fuel_percent(fuel)
            .slip_ratio(slip)
            .build();

        prop_assert!(t.throttle >= 0.0 && t.throttle <= 1.0);
        prop_assert!(t.brake >= 0.0 && t.brake <= 1.0);
        prop_assert!(t.clutch >= 0.0 && t.clutch <= 1.0);
        prop_assert!(t.ffb_scalar >= -1.0 && t.ffb_scalar <= 1.0);
        prop_assert!(t.fuel_percent >= 0.0 && t.fuel_percent <= 1.0);
        prop_assert!(t.slip_ratio >= 0.0 && t.slip_ratio <= 1.0);
    }
}

// ── Serde round-trip tests ──────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn serde_roundtrip_preserves_all_fields(t in normalized_telemetry_strategy()) {
        let json = serde_json::to_string(&t);
        prop_assert!(json.is_ok(), "serialization failed: {:?}", json.err());
        let json = json.map_err(|e| TestCaseError::Fail(format!("ser: {e}").into()))?;

        let deserialized: Result<NormalizedTelemetry, _> = serde_json::from_str(&json);
        prop_assert!(deserialized.is_ok(), "deserialization failed: {:?}", deserialized.err());
        let d = deserialized.map_err(|e| TestCaseError::Fail(format!("de: {e}").into()))?;

        // Compare all serializable numeric fields
        prop_assert!((t.speed_ms - d.speed_ms).abs() < f32::EPSILON);
        prop_assert!((t.steering_angle - d.steering_angle).abs() < f32::EPSILON);
        prop_assert!((t.throttle - d.throttle).abs() < f32::EPSILON);
        prop_assert!((t.brake - d.brake).abs() < f32::EPSILON);
        prop_assert!((t.clutch - d.clutch).abs() < f32::EPSILON);
        prop_assert!((t.rpm - d.rpm).abs() < f32::EPSILON);
        prop_assert!((t.max_rpm - d.max_rpm).abs() < f32::EPSILON);
        prop_assert_eq!(t.gear, d.gear);
        prop_assert_eq!(t.num_gears, d.num_gears);
        prop_assert!((t.lateral_g - d.lateral_g).abs() < f32::EPSILON);
        prop_assert!((t.longitudinal_g - d.longitudinal_g).abs() < f32::EPSILON);
        prop_assert!((t.slip_ratio - d.slip_ratio).abs() < f32::EPSILON);
        prop_assert!((t.ffb_scalar - d.ffb_scalar).abs() < f32::EPSILON);
        prop_assert!((t.ffb_torque_nm - d.ffb_torque_nm).abs() < f32::EPSILON);
        prop_assert_eq!(t.sequence, d.sequence);

        // Flags round-trip
        prop_assert_eq!(t.flags.yellow_flag, d.flags.yellow_flag);
        prop_assert_eq!(t.flags.red_flag, d.flags.red_flag);
        prop_assert_eq!(t.flags.blue_flag, d.flags.blue_flag);
        prop_assert_eq!(t.flags.green_flag, d.flags.green_flag);
    }

    #[test]
    fn serde_roundtrip_telemetry_value(val in telemetry_value_strategy()) {
        let json = serde_json::to_string(&val);
        prop_assert!(json.is_ok());
        let json = json.map_err(|e| TestCaseError::Fail(format!("ser: {e}").into()))?;

        let deserialized: Result<TelemetryValue, _> = serde_json::from_str(&json);
        prop_assert!(deserialized.is_ok());
        let d = deserialized.map_err(|e| TestCaseError::Fail(format!("de: {e}").into()))?;

        match (&val, &d) {
            (TelemetryValue::Float(a), TelemetryValue::Float(b)) => {
                prop_assert!((a - b).abs() < f32::EPSILON);
            }
            (TelemetryValue::Integer(a), TelemetryValue::Integer(b)) => {
                prop_assert_eq!(a, b);
            }
            (TelemetryValue::Boolean(a), TelemetryValue::Boolean(b)) => {
                prop_assert_eq!(a, b);
            }
            (TelemetryValue::String(a), TelemetryValue::String(b)) => {
                prop_assert_eq!(a, b);
            }
            _ => {
                prop_assert!(false, "variant mismatch: {:?} vs {:?}", val, d);
            }
        }
    }

    #[test]
    fn serde_roundtrip_telemetry_flags(flags in telemetry_flags_strategy()) {
        let json = serde_json::to_string(&flags);
        prop_assert!(json.is_ok());
        let json = json.map_err(|e| TestCaseError::Fail(format!("ser: {e}").into()))?;

        let d: Result<TelemetryFlags, _> = serde_json::from_str(&json);
        prop_assert!(d.is_ok());
        let d = d.map_err(|e| TestCaseError::Fail(format!("de: {e}").into()))?;

        prop_assert_eq!(flags, d);
    }
}

// ── Schema versioning and migration tests ───────────────────────────────────

#[test]
fn schema_version_parsing_current() -> Result<(), Box<dyn std::error::Error>> {
    let v = SchemaVersion::parse(CURRENT_SCHEMA_VERSION)?;
    assert_eq!(v.major, 1);
    assert_eq!(v.minor, 0);
    assert!(v.is_current());
    Ok(())
}

#[test]
fn schema_version_parsing_with_minor() -> Result<(), Box<dyn std::error::Error>> {
    let v = SchemaVersion::parse("wheel.profile/2.3")?;
    assert_eq!(v.major, 2);
    assert_eq!(v.minor, 3);
    assert!(!v.is_current());
    Ok(())
}

#[test]
fn schema_version_ordering() -> Result<(), Box<dyn std::error::Error>> {
    let v1 = SchemaVersion::parse("wheel.profile/1")?;
    let v2 = SchemaVersion::parse("wheel.profile/2")?;
    let v1_1 = SchemaVersion::parse("wheel.profile/1.1")?;

    assert!(v1.is_older_than(&v2));
    assert!(v1.is_older_than(&v1_1));
    assert!(!v2.is_older_than(&v1));
    assert!(!v1.is_older_than(&v1)); // same version
    Ok(())
}

#[test]
fn schema_version_parsing_rejects_invalid() {
    assert!(SchemaVersion::parse("invalid").is_err());
    assert!(SchemaVersion::parse("wheel.profile/").is_err());
    assert!(SchemaVersion::parse("other.format/1").is_err());
    assert!(SchemaVersion::parse("").is_err());
}

#[test]
fn schema_version_display() -> Result<(), Box<dyn std::error::Error>> {
    let v = SchemaVersion::new(2, 1);
    assert_eq!(format!("{v}"), "wheel.profile/2.1");
    Ok(())
}

#[test]
fn migration_manager_rejects_unknown_version() -> Result<(), Box<dyn std::error::Error>> {
    let config = MigrationConfig::without_backups();
    let manager = MigrationManager::new(config)?;

    let bad_json = r#"{"schema": "wheel.profile/99"}"#;
    let result = manager.detect_version(bad_json);
    assert!(result.is_ok()); // detect_version just parses
    let version = result?;
    assert_eq!(version.major, 99);
    assert!(!version.is_current());
    Ok(())
}

#[test]
fn migration_config_without_backups() {
    let config = MigrationConfig::without_backups();
    assert!(!config.create_backups);
    assert_eq!(config.max_backups, 0);
    assert!(config.validate_after_migration);
}

// ── TelemetryValue type conversion tests ────────────────────────────────────

#[test]
fn telemetry_value_float_serde() -> Result<(), Box<dyn std::error::Error>> {
    let val = TelemetryValue::Float(2.5);
    let json = serde_json::to_string(&val)?;
    let parsed: TelemetryValue = serde_json::from_str(&json)?;
    match parsed {
        TelemetryValue::Float(f) => assert!((f - 2.5).abs() < f32::EPSILON),
        other => panic!("expected Float, got {:?}", other),
    }
    Ok(())
}

#[test]
fn telemetry_value_integer_serde() -> Result<(), Box<dyn std::error::Error>> {
    let val = TelemetryValue::Integer(42);
    let json = serde_json::to_string(&val)?;
    let parsed: TelemetryValue = serde_json::from_str(&json)?;
    assert_eq!(parsed, TelemetryValue::Integer(42));
    Ok(())
}

#[test]
fn telemetry_value_boolean_serde() -> Result<(), Box<dyn std::error::Error>> {
    let val = TelemetryValue::Boolean(true);
    let json = serde_json::to_string(&val)?;
    let parsed: TelemetryValue = serde_json::from_str(&json)?;
    assert_eq!(parsed, TelemetryValue::Boolean(true));
    Ok(())
}

#[test]
fn telemetry_value_string_serde() -> Result<(), Box<dyn std::error::Error>> {
    let val = TelemetryValue::String("hello_world".to_string());
    let json = serde_json::to_string(&val)?;
    let parsed: TelemetryValue = serde_json::from_str(&json)?;
    assert_eq!(parsed, TelemetryValue::String("hello_world".to_string()));
    Ok(())
}

#[test]
fn telemetry_value_in_extended_map() -> Result<(), Box<dyn std::error::Error>> {
    let mut ext = BTreeMap::new();
    ext.insert("boost_psi".to_string(), TelemetryValue::Float(14.7));
    ext.insert("lap_count".to_string(), TelemetryValue::Integer(5));
    ext.insert("drs_enabled".to_string(), TelemetryValue::Boolean(true));
    ext.insert(
        "car_class".to_string(),
        TelemetryValue::String("GT3".to_string()),
    );

    let json = serde_json::to_string(&ext)?;
    let parsed: BTreeMap<String, TelemetryValue> = serde_json::from_str(&json)?;

    assert_eq!(ext.len(), parsed.len());
    for (key, val) in &ext {
        assert_eq!(parsed.get(key), Some(val));
    }
    Ok(())
}

// ── Builder full field coverage ─────────────────────────────────────────────

#[test]
fn builder_covers_all_fields() -> Result<(), Box<dyn std::error::Error>> {
    let flags = TelemetryFlags {
        yellow_flag: true,
        red_flag: true,
        blue_flag: true,
        checkered_flag: true,
        green_flag: false,
        pit_limiter: true,
        in_pits: true,
        drs_available: true,
        drs_active: true,
        ers_available: true,
        ers_active: true,
        launch_control: true,
        traction_control: true,
        abs_active: true,
        engine_limiter: true,
        safety_car: true,
        formation_lap: true,
        session_paused: true,
    };

    let t = NormalizedTelemetry::builder()
        .speed_ms(50.0)
        .steering_angle(0.1)
        .throttle(0.8)
        .brake(0.3)
        .clutch(0.5)
        .rpm(6500.0)
        .max_rpm(8000.0)
        .gear(4)
        .num_gears(6)
        .lateral_g(1.2)
        .longitudinal_g(-0.5)
        .vertical_g(0.1)
        .slip_ratio(0.05)
        .slip_angle_fl(0.01)
        .slip_angle_fr(0.02)
        .slip_angle_rl(0.03)
        .slip_angle_rr(0.04)
        .tire_temps_c([80, 85, 78, 82])
        .tire_pressures_psi([26.0, 26.5, 25.0, 25.5])
        .ffb_scalar(0.7)
        .ffb_torque_nm(8.5)
        .flags(flags.clone())
        .car_id("ferrari_488")
        .track_id("spa_francorchamps")
        .session_id("race_001")
        .position(3)
        .lap(12)
        .current_lap_time_s(85.3)
        .best_lap_time_s(82.1)
        .last_lap_time_s(83.5)
        .delta_ahead_s(-1.2)
        .delta_behind_s(0.8)
        .fuel_percent(0.45)
        .engine_temp_c(95.0)
        .sequence(42)
        .extended("custom_key", TelemetryValue::Float(1.5))
        .build();

    // Verify every field was set
    assert!((t.speed_ms - 50.0).abs() < f32::EPSILON);
    assert!((t.steering_angle - 0.1).abs() < f32::EPSILON);
    assert!((t.throttle - 0.8).abs() < f32::EPSILON);
    assert!((t.brake - 0.3).abs() < f32::EPSILON);
    assert!((t.clutch - 0.5).abs() < f32::EPSILON);
    assert!((t.rpm - 6500.0).abs() < f32::EPSILON);
    assert!((t.max_rpm - 8000.0).abs() < f32::EPSILON);
    assert_eq!(t.gear, 4);
    assert_eq!(t.num_gears, 6);
    assert!((t.lateral_g - 1.2).abs() < f32::EPSILON);
    assert!((t.longitudinal_g - (-0.5)).abs() < f32::EPSILON);
    assert!((t.vertical_g - 0.1).abs() < f32::EPSILON);
    assert!((t.slip_ratio - 0.05).abs() < f32::EPSILON);
    assert!((t.slip_angle_fl - 0.01).abs() < f32::EPSILON);
    assert!((t.slip_angle_fr - 0.02).abs() < f32::EPSILON);
    assert!((t.slip_angle_rl - 0.03).abs() < f32::EPSILON);
    assert!((t.slip_angle_rr - 0.04).abs() < f32::EPSILON);
    assert_eq!(t.tire_temps_c, [80, 85, 78, 82]);
    assert_eq!(t.tire_pressures_psi, [26.0, 26.5, 25.0, 25.5]);
    assert!((t.ffb_scalar - 0.7).abs() < f32::EPSILON);
    assert!((t.ffb_torque_nm - 8.5).abs() < f32::EPSILON);
    assert_eq!(t.flags, flags);
    assert_eq!(t.car_id.as_deref(), Some("ferrari_488"));
    assert_eq!(t.track_id.as_deref(), Some("spa_francorchamps"));
    assert_eq!(t.session_id.as_deref(), Some("race_001"));
    assert_eq!(t.position, 3);
    assert_eq!(t.lap, 12);
    assert!((t.current_lap_time_s - 85.3).abs() < f32::EPSILON);
    assert!((t.best_lap_time_s - 82.1).abs() < f32::EPSILON);
    assert!((t.last_lap_time_s - 83.5).abs() < f32::EPSILON);
    assert!((t.delta_ahead_s - (-1.2)).abs() < f32::EPSILON);
    assert!((t.delta_behind_s - 0.8).abs() < f32::EPSILON);
    assert!((t.fuel_percent - 0.45).abs() < f32::EPSILON);
    assert!((t.engine_temp_c - 95.0).abs() < f32::EPSILON);
    assert_eq!(t.sequence, 42);
    assert_eq!(
        t.extended.get("custom_key"),
        Some(&TelemetryValue::Float(1.5))
    );
    Ok(())
}

#[test]
fn builder_default_produces_valid_zeroed_state() {
    let t = NormalizedTelemetryBuilder::new().build();
    assert!((t.speed_ms - 0.0).abs() < f32::EPSILON);
    assert!((t.throttle - 0.0).abs() < f32::EPSILON);
    assert!((t.brake - 0.0).abs() < f32::EPSILON);
    assert!((t.rpm - 0.0).abs() < f32::EPSILON);
    assert_eq!(t.gear, 0);
    assert!(t.car_id.is_none());
    assert!(t.track_id.is_none());
    assert!(t.session_id.is_none());
    assert!(t.extended.is_empty());
    assert!(t.flags.green_flag); // default is true
    assert!(!t.flags.yellow_flag);
}

#[test]
fn builder_empty_string_ids_are_ignored() {
    let t = NormalizedTelemetry::builder()
        .car_id("")
        .track_id("")
        .session_id("")
        .build();

    assert!(t.car_id.is_none());
    assert!(t.track_id.is_none());
    assert!(t.session_id.is_none());
}

#[test]
fn validated_clamps_non_finite_to_zero() {
    let t = NormalizedTelemetry {
        speed_ms: f32::NAN,
        throttle: f32::INFINITY,
        brake: f32::NEG_INFINITY,
        rpm: f32::NAN,
        ..NormalizedTelemetry::default()
    };

    let v = t.validated();
    assert!((v.speed_ms - 0.0).abs() < f32::EPSILON);
    assert!((v.throttle - 0.0).abs() < f32::EPSILON);
    assert!((v.brake - 0.0).abs() < f32::EPSILON);
    assert!((v.rpm - 0.0).abs() < f32::EPSILON);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn validated_always_produces_finite_values(t in normalized_telemetry_strategy()) {
        let v = t.validated();
        prop_assert!(v.speed_ms.is_finite());
        prop_assert!(v.throttle.is_finite());
        prop_assert!(v.brake.is_finite());
        prop_assert!(v.clutch.is_finite());
        prop_assert!(v.rpm.is_finite());
        prop_assert!(v.max_rpm.is_finite());
        prop_assert!(v.steering_angle.is_finite());
        prop_assert!(v.lateral_g.is_finite());
        prop_assert!(v.longitudinal_g.is_finite());
        prop_assert!(v.vertical_g.is_finite());
        prop_assert!(v.ffb_scalar.is_finite());
        prop_assert!(v.ffb_torque_nm.is_finite());
        prop_assert!(v.fuel_percent.is_finite());
        prop_assert!(v.engine_temp_c.is_finite());
    }
}

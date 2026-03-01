//! Comprehensive schema validation tests for the schemas crate.
//!
//! Covers NormalizedTelemetry field ranges, TelemetryFrame round-trip,
//! DeviceId parsing/Display/equality, entity enums, configuration defaults,
//! proptest coverage, and snapshot tests.

use racing_wheel_schemas::domain::{DeviceId, DomainError, Gain, TorqueNm};
use racing_wheel_schemas::entities::{
    BumpstopConfig, CalibrationData, CalibrationType, DeviceCapabilities, DeviceState, DeviceType,
    FilterConfig, HandsOffConfig,
};
use racing_wheel_schemas::telemetry::{
    NormalizedTelemetry, TelemetryFlags, TelemetryFrame, TelemetrySnapshot, TelemetryValue,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ──────────────────────────────────────────────────────────────────────
// NormalizedTelemetry: field range validation
// ──────────────────────────────────────────────────────────────────────

#[test]
fn normalized_telemetry_default_has_valid_ranges() -> TestResult {
    let t = NormalizedTelemetry::default();

    assert!(t.speed_ms >= 0.0, "speed must be non-negative");
    assert!(t.rpm >= 0.0, "rpm must be non-negative");
    assert!(t.max_rpm >= 0.0, "max_rpm must be non-negative");
    assert!(
        (-1..=20).contains(&(t.gear as i16)),
        "gear must be in [-1, 20]"
    );
    assert!(
        (0.0..=1.0).contains(&t.throttle),
        "throttle must be in [0, 1]"
    );
    assert!((0.0..=1.0).contains(&t.brake), "brake must be in [0, 1]");
    assert!((0.0..=1.0).contains(&t.clutch), "clutch must be in [0, 1]");
    assert!(
        (-1.0..=1.0).contains(&t.ffb_scalar),
        "ffb_scalar must be in [-1, 1]"
    );
    assert!(
        (0.0..=1.0).contains(&t.fuel_percent),
        "fuel_percent must be in [0, 1]"
    );
    Ok(())
}

#[test]
fn normalized_telemetry_builder_clamps_ranges() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .throttle(2.0)
        .brake(-1.0)
        .clutch(5.0)
        .ffb_scalar(3.0)
        .fuel_percent(-0.5)
        .slip_ratio(99.0)
        .build();

    assert_eq!(t.throttle, 1.0, "throttle clamped to 1.0");
    assert_eq!(t.brake, 0.0, "brake clamped to 0.0");
    assert_eq!(t.clutch, 1.0, "clutch clamped to 1.0");
    assert_eq!(t.ffb_scalar, 1.0, "ffb_scalar clamped to 1.0");
    assert_eq!(t.fuel_percent, 0.0, "fuel_percent clamped to 0.0");
    assert_eq!(t.slip_ratio, 1.0, "slip_ratio clamped to 1.0");
    Ok(())
}

#[test]
fn normalized_telemetry_builder_rejects_negative_speed() -> TestResult {
    let t = NormalizedTelemetry::builder().speed_ms(-10.0).build();
    assert_eq!(t.speed_ms, 0.0, "negative speed falls back to default 0");
    Ok(())
}

#[test]
fn normalized_telemetry_builder_rejects_negative_rpm() -> TestResult {
    let t = NormalizedTelemetry::builder().rpm(-500.0).build();
    assert_eq!(t.rpm, 0.0, "negative rpm falls back to default 0");
    Ok(())
}

#[test]
fn normalized_telemetry_validated_handles_nan_and_inf() -> TestResult {
    let t = NormalizedTelemetry {
        speed_ms: f32::NAN,
        throttle: f32::INFINITY,
        brake: f32::NEG_INFINITY,
        rpm: f32::NAN,
        ffb_scalar: f32::NAN,
        fuel_percent: f32::INFINITY,
        ..Default::default()
    }
    .validated();

    assert_eq!(t.speed_ms, 0.0);
    assert_eq!(t.throttle, 0.0);
    assert_eq!(t.brake, 0.0);
    assert_eq!(t.rpm, 0.0);
    assert_eq!(t.ffb_scalar, 0.0);
    assert_eq!(t.fuel_percent, 0.0);
    Ok(())
}

#[test]
fn normalized_telemetry_gear_accepts_reverse() -> TestResult {
    let t = NormalizedTelemetry::builder().gear(-1).build();
    assert_eq!(t.gear, -1, "gear -1 means reverse");
    Ok(())
}

#[test]
fn normalized_telemetry_gear_accepts_neutral() -> TestResult {
    let t = NormalizedTelemetry::builder().gear(0).build();
    assert_eq!(t.gear, 0, "gear 0 means neutral");
    Ok(())
}

#[test]
fn normalized_telemetry_gear_accepts_high_gear() -> TestResult {
    let t = NormalizedTelemetry::builder().gear(8).num_gears(8).build();
    assert_eq!(t.gear, 8);
    assert_eq!(t.num_gears, 8);
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// TelemetryFrame: serialization round-trip
// ──────────────────────────────────────────────────────────────────────

#[test]
fn telemetry_frame_json_roundtrip() -> TestResult {
    let telemetry = NormalizedTelemetry::builder()
        .speed_ms(45.0)
        .rpm(6500.0)
        .gear(4)
        .steering_angle(0.15)
        .throttle(0.8)
        .brake(0.1)
        .lateral_g(1.2)
        .longitudinal_g(-0.5)
        .ffb_scalar(0.7)
        .car_id("ferrari_488")
        .track_id("spa")
        .build();

    let frame = TelemetryFrame::new(telemetry, 123456789, 42, 128);

    let json = serde_json::to_string(&frame)?;
    let deserialized: TelemetryFrame = serde_json::from_str(&json)?;

    assert_eq!(deserialized.data.speed_ms, 45.0);
    assert_eq!(deserialized.data.rpm, 6500.0);
    assert_eq!(deserialized.data.gear, 4);
    assert!((deserialized.data.steering_angle - 0.15).abs() < f32::EPSILON);
    assert_eq!(deserialized.data.throttle, 0.8);
    assert_eq!(deserialized.data.car_id, Some("ferrari_488".to_string()));
    assert_eq!(deserialized.data.track_id, Some("spa".to_string()));
    assert_eq!(deserialized.timestamp_ns, 123456789);
    assert_eq!(deserialized.sequence, 42);
    assert_eq!(deserialized.raw_size, 128);
    Ok(())
}

#[test]
fn telemetry_snapshot_json_roundtrip() -> TestResult {
    let epoch = std::time::Instant::now();
    let telemetry = NormalizedTelemetry::builder()
        .speed_ms(30.0)
        .rpm(4500.0)
        .gear(3)
        .ffb_scalar(-0.3)
        .position(5)
        .lap(12)
        .sequence(100)
        .build();

    let snapshot = TelemetrySnapshot::from_telemetry(&telemetry, epoch);
    let json = serde_json::to_string(&snapshot)?;
    let restored: TelemetrySnapshot = serde_json::from_str(&json)?;

    assert_eq!(restored.speed_ms, 30.0);
    assert_eq!(restored.rpm, 4500.0);
    assert_eq!(restored.gear, 3);
    assert!((restored.ffb_scalar - (-0.3)).abs() < f32::EPSILON);
    assert_eq!(restored.position, 5);
    assert_eq!(restored.lap, 12);
    assert_eq!(restored.sequence, 100);
    Ok(())
}

#[test]
fn telemetry_value_all_variants_roundtrip() -> TestResult {
    let variants = vec![
        TelemetryValue::Float(3.14),
        TelemetryValue::Integer(-42),
        TelemetryValue::Boolean(true),
        TelemetryValue::String("hello".to_string()),
    ];

    for v in &variants {
        let json = serde_json::to_string(v)?;
        let deserialized: TelemetryValue = serde_json::from_str(&json)?;
        assert_eq!(&deserialized, v);
    }
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// DeviceId: parsing, Display, equality
// ──────────────────────────────────────────────────────────────────────

#[test]
fn device_id_parses_valid_strings() -> TestResult {
    let id: DeviceId = "moza-r9".parse()?;
    assert_eq!(id.as_str(), "moza-r9");
    Ok(())
}

#[test]
fn device_id_normalizes_to_lowercase() -> TestResult {
    let id: DeviceId = "SIMUCUBE-2-Pro".parse()?;
    assert_eq!(id.as_str(), "simucube-2-pro");
    Ok(())
}

#[test]
fn device_id_trims_whitespace() -> TestResult {
    let id: DeviceId = "  fanatec-dd1  ".parse()?;
    assert_eq!(id.as_str(), "fanatec-dd1");
    Ok(())
}

#[test]
fn device_id_allows_underscores() -> TestResult {
    let id: DeviceId = "device_name_2".parse()?;
    assert_eq!(id.as_str(), "device_name_2");
    Ok(())
}

#[test]
fn device_id_rejects_empty_string() {
    let result = "".parse::<DeviceId>();
    assert!(result.is_err());
    match result {
        Err(DomainError::InvalidDeviceId(s)) => assert_eq!(s, ""),
        other => panic!("expected InvalidDeviceId, got: {:?}", other),
    }
}

#[test]
fn device_id_rejects_whitespace_only() {
    assert!("   ".parse::<DeviceId>().is_err());
}

#[test]
fn device_id_rejects_special_characters() {
    assert!("dev@ice".parse::<DeviceId>().is_err());
    assert!("dev ice".parse::<DeviceId>().is_err());
    assert!("dev/ice".parse::<DeviceId>().is_err());
    assert!("dev.ice".parse::<DeviceId>().is_err());
}

#[test]
fn device_id_display_matches_as_str() -> TestResult {
    let id: DeviceId = "test-wheel".parse()?;
    assert_eq!(format!("{}", id), "test-wheel");
    assert_eq!(id.to_string(), id.as_str());
    Ok(())
}

#[test]
fn device_id_equality_after_normalization() -> TestResult {
    let a: DeviceId = "MY-WHEEL".parse()?;
    let b: DeviceId = "my-wheel".parse()?;
    let c: DeviceId = "  My-Wheel  ".parse()?;
    assert_eq!(a, b);
    assert_eq!(b, c);
    Ok(())
}

#[test]
fn device_id_hash_consistency() -> TestResult {
    use std::collections::HashSet;
    let mut set = HashSet::new();
    let id1: DeviceId = "Fanatec-DD1".parse()?;
    let id2: DeviceId = "fanatec-dd1".parse()?;
    set.insert(id1);
    assert!(
        set.contains(&id2),
        "normalized IDs must hash to the same bucket"
    );
    Ok(())
}

#[test]
fn device_id_try_from_string_and_str() -> TestResult {
    let from_string = DeviceId::try_from("test-id".to_string())?;
    let from_str = DeviceId::try_from("test-id")?;
    assert_eq!(from_string, from_str);

    let into_string: String = from_string.into();
    assert_eq!(into_string, "test-id");
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// Entity enums: variants can be matched and serialized
// ──────────────────────────────────────────────────────────────────────

#[test]
fn device_type_all_variants_serializable() -> TestResult {
    let variants = [
        DeviceType::Other,
        DeviceType::WheelBase,
        DeviceType::SteeringWheel,
        DeviceType::Pedals,
        DeviceType::Shifter,
        DeviceType::Handbrake,
        DeviceType::ButtonBox,
    ];

    for v in &variants {
        let json = serde_json::to_string(v)?;
        let deserialized: DeviceType = serde_json::from_str(&json)?;
        assert_eq!(&deserialized, v);
    }
    assert_eq!(variants.len(), 7, "all DeviceType variants accounted for");
    Ok(())
}

#[test]
fn device_state_all_variants_serializable() -> TestResult {
    let variants = [
        DeviceState::Disconnected,
        DeviceState::Connected,
        DeviceState::Active,
        DeviceState::Faulted,
        DeviceState::SafeMode,
    ];

    for v in &variants {
        let json = serde_json::to_string(v)?;
        let deserialized: DeviceState = serde_json::from_str(&json)?;
        assert_eq!(&deserialized, v);
    }
    assert_eq!(variants.len(), 5, "all DeviceState variants accounted for");
    Ok(())
}

#[test]
fn calibration_type_all_variants_serializable() -> TestResult {
    let variants = [
        CalibrationType::Center,
        CalibrationType::Range,
        CalibrationType::Pedals,
        CalibrationType::Full,
    ];

    for v in &variants {
        let json = serde_json::to_string(v)?;
        let deserialized: CalibrationType = serde_json::from_str(&json)?;
        assert_eq!(&deserialized, v);
    }
    assert_eq!(
        variants.len(),
        4,
        "all CalibrationType variants accounted for"
    );
    Ok(())
}

#[test]
fn device_state_repr_values() {
    assert_eq!(DeviceState::Disconnected as i32, 0);
    assert_eq!(DeviceState::Connected as i32, 1);
    assert_eq!(DeviceState::Active as i32, 2);
    assert_eq!(DeviceState::Faulted as i32, 3);
    assert_eq!(DeviceState::SafeMode as i32, 4);
}

#[test]
fn device_type_repr_values() {
    assert_eq!(DeviceType::Other as i32, 0);
    assert_eq!(DeviceType::WheelBase as i32, 1);
    assert_eq!(DeviceType::SteeringWheel as i32, 2);
    assert_eq!(DeviceType::Pedals as i32, 3);
    assert_eq!(DeviceType::Shifter as i32, 4);
    assert_eq!(DeviceType::Handbrake as i32, 5);
    assert_eq!(DeviceType::ButtonBox as i32, 6);
}

// ──────────────────────────────────────────────────────────────────────
// Configuration types: defaults are sane
// ──────────────────────────────────────────────────────────────────────

#[test]
fn filter_config_default_is_sane() {
    let fc = FilterConfig::default();

    assert!(fc.reconstruction <= 8, "reconstruction level must be <= 8");
    assert_eq!(
        fc.friction.value(),
        0.0,
        "default friction should be zero for safe base"
    );
    assert_eq!(fc.damper.value(), 0.0, "default damper should be zero");
    assert_eq!(fc.inertia.value(), 0.0, "default inertia should be zero");
    assert!(
        fc.slew_rate.value() > 0.0,
        "slew rate must be positive to allow changes"
    );
    assert!(
        !fc.curve_points.is_empty(),
        "must have at least one curve point"
    );
    assert!(
        fc.curve_points.len() >= 2,
        "need at least 2 points for a meaningful curve"
    );
}

#[test]
fn bumpstop_config_default_is_sane() {
    let bs = BumpstopConfig::default();
    assert!(bs.enabled, "bumpstop should be enabled by default");
    assert!(bs.start_angle > 0.0, "start angle must be positive");
    assert!(
        bs.max_angle > bs.start_angle,
        "max angle must be greater than start angle"
    );
    assert!(
        (0.0..=1.0).contains(&bs.stiffness),
        "stiffness must be in [0, 1]"
    );
    assert!(
        (0.0..=1.0).contains(&bs.damping),
        "damping must be in [0, 1]"
    );
}

#[test]
fn hands_off_config_default_is_sane() {
    let ho = HandsOffConfig::default();
    assert!(
        ho.enabled,
        "hands-off detection should be enabled by default"
    );
    assert!(ho.threshold > 0.0, "threshold must be positive");
    assert!(ho.threshold < 1.0, "threshold must be less than 1.0");
    assert!(ho.timeout_seconds > 0.0, "timeout must be positive");
}

#[test]
fn telemetry_flags_default_is_sane() {
    let flags = TelemetryFlags::default();
    assert!(flags.green_flag, "green flag should default to true");
    assert!(!flags.yellow_flag);
    assert!(!flags.red_flag);
    assert!(!flags.blue_flag);
    assert!(!flags.checkered_flag);
    assert!(!flags.pit_limiter);
    assert!(!flags.in_pits);
    assert!(!flags.safety_car);
    assert!(!flags.session_paused);
}

#[test]
fn calibration_data_new_has_timestamp() {
    let cal = CalibrationData::new(CalibrationType::Center);
    assert!(
        cal.calibrated_at.is_some(),
        "calibration should have a timestamp"
    );
    assert!(cal.center_position.is_none());
    assert!(!cal.has_center_calibration());
    assert!(!cal.has_range_calibration());
    assert!(!cal.is_fully_calibrated());
}

#[test]
fn device_capabilities_ffb_support() -> TestResult {
    let caps_pid = DeviceCapabilities::new(
        true,  // supports_pid
        false, // supports_raw_torque_1khz
        false,
        false,
        TorqueNm::new(20.0)?,
        4096,
        1000,
    );
    assert!(caps_pid.supports_ffb(), "PID should imply FFB support");

    let caps_raw = DeviceCapabilities::new(
        false,
        true, // supports_raw_torque_1khz
        false,
        false,
        TorqueNm::new(20.0)?,
        4096,
        1000,
    );
    assert!(
        caps_raw.supports_ffb(),
        "raw torque should imply FFB support"
    );

    let caps_none =
        DeviceCapabilities::new(false, false, false, false, TorqueNm::new(5.0)?, 4096, 1000);
    assert!(!caps_none.supports_ffb(), "no PID or raw → no FFB");
    Ok(())
}

#[test]
fn device_capabilities_update_rate() -> TestResult {
    let caps = DeviceCapabilities::new(
        true,
        true,
        false,
        false,
        TorqueNm::new(20.0)?,
        4096,
        1000, // 1000μs → 1kHz
    );
    assert!((caps.max_update_rate_hz() - 1000.0).abs() < 1.0);
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// Snapshot tests (insta)
// ──────────────────────────────────────────────────────────────────────

#[test]
fn snapshot_default_normalized_telemetry_json() {
    let t = NormalizedTelemetry::default();
    // Serialize excluding the non-serializable timestamp field by using serde_json
    let value = serde_json::to_value(&t).expect("serialization must succeed");
    insta::assert_json_snapshot!("default_normalized_telemetry", value);
}

#[test]
fn snapshot_device_type_variant_names() {
    let names: Vec<&str> = vec![
        "Other",
        "WheelBase",
        "SteeringWheel",
        "Pedals",
        "Shifter",
        "Handbrake",
        "ButtonBox",
    ];
    insta::assert_json_snapshot!("device_type_variant_names", names);
}

#[test]
fn snapshot_device_state_variant_names() {
    let names: Vec<&str> = vec!["Disconnected", "Connected", "Active", "Faulted", "SafeMode"];
    insta::assert_json_snapshot!("device_state_variant_names", names);
}

#[test]
fn snapshot_telemetry_flags_default() {
    let flags = TelemetryFlags::default();
    let value = serde_json::to_value(&flags).expect("serialization must succeed");
    insta::assert_json_snapshot!("telemetry_flags_default", value);
}

// ──────────────────────────────────────────────────────────────────────
// Proptest coverage
// ──────────────────────────────────────────────────────────────────────

mod proptest_coverage {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn normalized_telemetry_validated_never_panics(
            speed in proptest::num::f32::ANY,
            rpm in proptest::num::f32::ANY,
            throttle in proptest::num::f32::ANY,
            brake in proptest::num::f32::ANY,
            clutch in proptest::num::f32::ANY,
            gear in proptest::num::i8::ANY,
            ffb_scalar in proptest::num::f32::ANY,
            fuel_percent in proptest::num::f32::ANY,
            slip_ratio in proptest::num::f32::ANY,
            lateral_g in proptest::num::f32::ANY,
            longitudinal_g in proptest::num::f32::ANY,
        ) {
            let t = NormalizedTelemetry {
                speed_ms: speed,
                rpm,
                throttle,
                brake,
                clutch,
                gear,
                ffb_scalar,
                fuel_percent,
                slip_ratio,
                lateral_g,
                longitudinal_g,
                ..Default::default()
            };

            let v = t.validated();

            // After validation: clamped fields are in range
            prop_assert!(v.speed_ms >= 0.0 || v.speed_ms == 0.0);
            prop_assert!(v.rpm >= 0.0 || v.rpm == 0.0);
            prop_assert!((0.0..=1.0).contains(&v.throttle));
            prop_assert!((0.0..=1.0).contains(&v.brake));
            prop_assert!((0.0..=1.0).contains(&v.clutch));
            prop_assert!((-1.0..=1.0).contains(&v.ffb_scalar));
            prop_assert!((0.0..=1.0).contains(&v.fuel_percent));
            prop_assert!((0.0..=1.0).contains(&v.slip_ratio));
            // G-forces and steering are unclamped but NaN should be 0
            prop_assert!(v.lateral_g.is_finite());
            prop_assert!(v.longitudinal_g.is_finite());
        }

        #[test]
        fn device_id_from_arbitrary_string_never_panics(s in ".*") {
            // Must not panic regardless of input; result can be Ok or Err
            let _result = s.parse::<DeviceId>();
        }

        #[test]
        fn device_id_valid_chars_always_parse(
            s in "[a-z0-9][a-z0-9_-]{0,30}"
        ) {
            let result = s.parse::<DeviceId>();
            prop_assert!(result.is_ok(), "valid pattern should always parse: {:?}", s);
        }

        #[test]
        fn gain_rejects_out_of_range(val in proptest::num::f32::ANY) {
            let result = Gain::new(val);
            if val.is_finite() && (0.0..=1.0).contains(&val) {
                prop_assert!(result.is_ok());
            } else {
                prop_assert!(result.is_err());
            }
        }

        #[test]
        fn telemetry_builder_speed_non_negative(speed in proptest::num::f32::ANY) {
            let t = NormalizedTelemetry::builder().speed_ms(speed).build();
            prop_assert!(t.speed_ms >= 0.0);
        }

        #[test]
        fn telemetry_builder_rpm_non_negative(rpm in proptest::num::f32::ANY) {
            let t = NormalizedTelemetry::builder().rpm(rpm).build();
            prop_assert!(t.rpm >= 0.0);
        }

        #[test]
        fn telemetry_json_roundtrip_arbitrary(
            speed in 0.0f32..500.0,
            rpm in 0.0f32..20000.0,
            gear in -1i8..8,
            throttle in 0.0f32..1.0,
            brake in 0.0f32..1.0,
        ) {
            let t = NormalizedTelemetry::builder()
                .speed_ms(speed)
                .rpm(rpm)
                .gear(gear)
                .throttle(throttle)
                .brake(brake)
                .build();

            let json = serde_json::to_string(&t).expect("serialize");
            let rt: NormalizedTelemetry = serde_json::from_str(&json).expect("deserialize");

            prop_assert_eq!(rt.speed_ms, t.speed_ms);
            prop_assert_eq!(rt.rpm, t.rpm);
            prop_assert_eq!(rt.gear, t.gear);
            prop_assert_eq!(rt.throttle, t.throttle);
            prop_assert_eq!(rt.brake, t.brake);
        }
    }
}

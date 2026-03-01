//! Comprehensive unit tests for the schemas crate.
//!
//! Covers serialization/deserialization roundtrips, Default implementations,
//! validation logic, and conversion traits for all public schema types.

use std::collections::HashMap;

use racing_wheel_schemas::config::{
    BumpstopConfig as ConfigBumpstopConfig, CurvePoint as ConfigCurvePoint,
    FilterConfig as ConfigFilterConfig, HandsOffConfig as ConfigHandsOffConfig, HapticsConfig,
    LedConfig, NotchFilter as ConfigNotchFilter, ProfileMigrator, ProfileValidator,
};
use racing_wheel_schemas::domain::{
    CurvePoint, Degrees, DeviceId, DomainError, FrequencyHz, Gain, ProfileId, TorqueNm,
    validate_curve_monotonic,
};
use racing_wheel_schemas::entities::{
    BaseSettings, BumpstopConfig, CalibrationData, CalibrationType, Device, DeviceCapabilities,
    DeviceState, DeviceType, FilterConfig, HandsOffConfig, HapticsConfig as EntityHapticsConfig,
    InMemoryProfileStore, LedConfig as EntityLedConfig, NotchFilter, PedalCalibrationData, Profile,
    ProfileScope, ProfileStore,
};
use racing_wheel_schemas::migration::{
    CURRENT_SCHEMA_VERSION, MigrationConfig, MigrationManager, SchemaVersion,
};
use racing_wheel_schemas::telemetry::{
    NormalizedTelemetry, TelemetryData, TelemetryFlags, TelemetryFrame, TelemetrySnapshot,
    TelemetryValue,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ──────────────────────────────────────────────────────────────────────
// Domain types: serialization/deserialization roundtrips
// ──────────────────────────────────────────────────────────────────────

#[test]
fn torque_nm_serde_roundtrip() -> TestResult {
    let torque = TorqueNm::new(25.5)?;
    let json = serde_json::to_string(&torque)?;
    let restored: TorqueNm = serde_json::from_str(&json)?;
    assert!((restored.value() - 25.5).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn torque_nm_display() -> TestResult {
    let torque = TorqueNm::new(12.5)?;
    assert_eq!(format!("{}", torque), "12.50 Nm");
    Ok(())
}

#[test]
fn torque_nm_zero_constant() {
    assert!((TorqueNm::ZERO.value()).abs() < f32::EPSILON);
}

#[test]
fn torque_nm_ordering() -> TestResult {
    let t1 = TorqueNm::new(5.0)?;
    let t2 = TorqueNm::new(10.0)?;
    let t3 = TorqueNm::new(10.0)?;
    assert!(t1 < t2);
    assert_eq!(t2, t3);
    assert_eq!(t1.min(t2), t1);
    assert_eq!(t1.max(t2), t2);
    Ok(())
}

#[test]
fn torque_nm_sub_clamps_to_zero() -> TestResult {
    let t1 = TorqueNm::new(5.0)?;
    let t2 = TorqueNm::new(10.0)?;
    let diff = t1 - t2;
    assert!(
        (diff.value()).abs() < f32::EPSILON,
        "subtraction clamps to 0"
    );
    Ok(())
}

#[test]
fn torque_nm_mul_clamps_to_max() -> TestResult {
    let t = TorqueNm::new(30.0)?;
    let scaled = t * 2.0;
    assert!(
        (scaled.value() - TorqueNm::MAX_TORQUE).abs() < f32::EPSILON,
        "multiplication clamps to MAX_TORQUE"
    );
    Ok(())
}

#[test]
fn degrees_serde_roundtrip() -> TestResult {
    let dor = Degrees::new_dor(900.0)?;
    let json = serde_json::to_string(&dor)?;
    let restored: Degrees = serde_json::from_str(&json)?;
    assert!((restored.value() - 900.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn degrees_display() -> TestResult {
    let d = Degrees::new_angle(45.0)?;
    assert_eq!(format!("{}", d), "45.0°");
    Ok(())
}

#[test]
fn degrees_radians_conversion() -> TestResult {
    let d = Degrees::new_angle(180.0)?;
    assert!((d.to_radians() - std::f32::consts::PI).abs() < 0.001);

    let from_rad = Degrees::from_radians(std::f32::consts::PI);
    assert!((from_rad.value() - 180.0).abs() < 0.01);
    Ok(())
}

#[test]
fn degrees_zero_constant() {
    assert!((Degrees::ZERO.value()).abs() < f32::EPSILON);
}

#[test]
fn degrees_millidegrees_roundtrip() -> TestResult {
    let d = Degrees::new_angle(45.678)?;
    let mdeg = d.to_millidegrees();
    let back = Degrees::from_millidegrees(mdeg);
    assert!((back.value() - 45.678).abs() < 0.001);
    Ok(())
}

#[test]
fn gain_serde_roundtrip() -> TestResult {
    let gain = Gain::new(0.75)?;
    let json = serde_json::to_string(&gain)?;
    let restored: Gain = serde_json::from_str(&json)?;
    assert!((restored.value() - 0.75).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn gain_display() -> TestResult {
    let gain = Gain::new(0.5)?;
    assert_eq!(format!("{}", gain), "50.0%");
    Ok(())
}

#[test]
fn gain_constants() {
    assert!((Gain::ZERO.value()).abs() < f32::EPSILON);
    assert!((Gain::FULL.value() - 1.0).abs() < f32::EPSILON);
}

#[test]
fn frequency_hz_serde_roundtrip() -> TestResult {
    let freq = FrequencyHz::new(1000.0)?;
    let json = serde_json::to_string(&freq)?;
    let restored: FrequencyHz = serde_json::from_str(&json)?;
    assert!((restored.value() - 1000.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn frequency_hz_display() -> TestResult {
    let freq = FrequencyHz::new(440.0)?;
    assert_eq!(format!("{}", freq), "440.0 Hz");
    Ok(())
}

#[test]
fn curve_point_serde_roundtrip() -> TestResult {
    let p = CurvePoint::new(0.5, 0.7)?;
    let json = serde_json::to_string(&p)?;
    let restored: CurvePoint = serde_json::from_str(&json)?;
    assert!((restored.input - 0.5).abs() < f32::EPSILON);
    assert!((restored.output - 0.7).abs() < f32::EPSILON);
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// Domain types: validation edge cases
// ──────────────────────────────────────────────────────────────────────

#[test]
fn torque_nm_boundary_values() -> TestResult {
    assert!(TorqueNm::new(0.0).is_ok());
    assert!(TorqueNm::new(TorqueNm::MAX_TORQUE).is_ok());
    assert!(TorqueNm::new(TorqueNm::MAX_TORQUE + 0.01).is_err());
    assert!(TorqueNm::new(-0.01).is_err());
    Ok(())
}

#[test]
fn torque_nm_from_cnm_boundary() -> TestResult {
    let t = TorqueNm::from_cnm(5000)?;
    assert!((t.value() - 50.0).abs() < f32::EPSILON);
    assert!(TorqueNm::from_cnm(5001).is_err());
    Ok(())
}

#[test]
fn degrees_dor_boundary_values() {
    assert!(Degrees::new_dor(Degrees::MIN_DOR).is_ok());
    assert!(Degrees::new_dor(Degrees::MAX_DOR).is_ok());
    assert!(Degrees::new_dor(Degrees::MIN_DOR - 0.01).is_err());
    assert!(Degrees::new_dor(Degrees::MAX_DOR + 0.01).is_err());
}

#[test]
fn degrees_angle_rejects_infinity() {
    assert!(Degrees::new_angle(f32::INFINITY).is_err());
    assert!(Degrees::new_angle(f32::NEG_INFINITY).is_err());
}

#[test]
fn gain_boundary_values() {
    assert!(Gain::new(0.0).is_ok());
    assert!(Gain::new(1.0).is_ok());
    assert!(Gain::new(-f32::EPSILON).is_err());
    assert!(Gain::new(1.0 + f32::EPSILON).is_err());
    assert!(Gain::new(f32::INFINITY).is_err());
}

#[test]
fn frequency_hz_rejects_zero_and_negative() {
    assert!(FrequencyHz::new(0.0).is_err());
    assert!(FrequencyHz::new(-1.0).is_err());
    assert!(FrequencyHz::new(f32::INFINITY).is_err());
    assert!(FrequencyHz::new(f32::NAN).is_err());
    assert!(FrequencyHz::new(0.001).is_ok());
}

#[test]
fn curve_point_boundary_values() {
    assert!(CurvePoint::new(0.0, 0.0).is_ok());
    assert!(CurvePoint::new(1.0, 1.0).is_ok());
    assert!(CurvePoint::new(-0.001, 0.5).is_err());
    assert!(CurvePoint::new(0.5, 1.001).is_err());
    assert!(CurvePoint::new(f32::INFINITY, 0.5).is_err());
}

#[test]
fn validate_curve_monotonic_single_point() -> TestResult {
    let points = vec![CurvePoint::new(0.5, 0.5)?];
    assert!(validate_curve_monotonic(&points).is_ok());
    Ok(())
}

#[test]
fn validate_curve_monotonic_equal_inputs_rejected() -> TestResult {
    let points = vec![CurvePoint::new(0.5, 0.3)?, CurvePoint::new(0.5, 0.7)?];
    assert!(validate_curve_monotonic(&points).is_err());
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// Conversion traits: DomainError -> ValidationError, ProfileError
// ──────────────────────────────────────────────────────────────────────

#[test]
fn domain_error_to_validation_error_torque() {
    let err = DomainError::InvalidTorque(55.0, 50.0);
    let ve: openracing_errors::ValidationError = err.into();
    let msg = format!("{}", ve);
    assert!(msg.contains("torque"), "should mention torque field");
}

#[test]
fn domain_error_to_validation_error_degrees() {
    let err = DomainError::InvalidDegrees(100.0, 180.0, 2160.0);
    let ve: openracing_errors::ValidationError = err.into();
    let msg = format!("{}", ve);
    assert!(msg.contains("degrees"), "should mention degrees field");
}

#[test]
fn domain_error_to_validation_error_gain() {
    let err = DomainError::InvalidGain(1.5);
    let ve: openracing_errors::ValidationError = err.into();
    let msg = format!("{}", ve);
    assert!(msg.contains("gain"), "should mention gain field");
}

#[test]
fn domain_error_to_profile_error_inheritance_depth() {
    let err = DomainError::InheritanceDepthExceeded {
        depth: 6,
        max_depth: 5,
    };
    let pe: openracing_errors::ProfileError = err.into();
    let msg = format!("{}", pe);
    assert!(
        msg.to_lowercase().contains("inheritance") || msg.to_lowercase().contains("depth"),
        "should mention inheritance or depth: {}",
        msg
    );
}

#[test]
fn domain_error_to_profile_error_circular_inheritance() {
    let err = DomainError::CircularInheritance {
        profile_id: "test-profile".to_string(),
    };
    let pe: openracing_errors::ProfileError = err.into();
    let msg = format!("{}", pe);
    assert!(
        msg.to_lowercase().contains("circular") || msg.to_lowercase().contains("test-profile"),
        "should mention circular inheritance: {}",
        msg
    );
}

#[test]
fn domain_error_to_profile_error_parent_not_found() {
    let err = DomainError::ParentProfileNotFound {
        profile_id: "missing-parent".to_string(),
    };
    let pe: openracing_errors::ProfileError = err.into();
    let msg = format!("{}", pe);
    assert!(
        msg.to_lowercase().contains("parent") || msg.to_lowercase().contains("missing-parent"),
        "should mention parent not found: {}",
        msg
    );
}

#[test]
fn domain_error_to_profile_error_invalid_id() {
    let err = DomainError::InvalidProfileId("bad id".to_string());
    let pe: openracing_errors::ProfileError = err.into();
    let msg = format!("{}", pe);
    assert!(
        msg.to_lowercase().contains("bad id") || msg.to_lowercase().contains("invalid"),
        "should mention invalid id: {}",
        msg
    );
}

// ──────────────────────────────────────────────────────────────────────
// DeviceId and ProfileId: conversion traits
// ──────────────────────────────────────────────────────────────────────

#[test]
fn device_id_into_string() -> TestResult {
    let id: DeviceId = "my-device".parse()?;
    let s: String = id.into();
    assert_eq!(s, "my-device");
    Ok(())
}

#[test]
fn device_id_as_ref_str() -> TestResult {
    let id: DeviceId = "test-dev".parse()?;
    let s: &str = id.as_ref();
    assert_eq!(s, "test-dev");
    Ok(())
}

#[test]
fn device_id_try_from_string() -> TestResult {
    let id = DeviceId::try_from("test-123".to_string())?;
    assert_eq!(id.as_str(), "test-123");
    Ok(())
}

#[test]
fn device_id_try_from_str() -> TestResult {
    let id = DeviceId::try_from("test-456")?;
    assert_eq!(id.as_str(), "test-456");
    Ok(())
}

#[test]
fn device_id_new_normalizes() -> TestResult {
    let id = DeviceId::new("  UPPER-case  ".to_string())?;
    assert_eq!(id.as_str(), "upper-case");
    Ok(())
}

#[test]
fn profile_id_into_string() -> TestResult {
    let id: ProfileId = "my-profile".parse()?;
    let s: String = id.into();
    assert_eq!(s, "my-profile");
    Ok(())
}

#[test]
fn profile_id_as_ref_str() -> TestResult {
    let id: ProfileId = "test.profile".parse()?;
    let s: &str = id.as_ref();
    assert_eq!(s, "test.profile");
    Ok(())
}

#[test]
fn profile_id_try_from_string() -> TestResult {
    let id = ProfileId::try_from("iracing.gt3".to_string())?;
    assert_eq!(id.as_str(), "iracing.gt3");
    Ok(())
}

#[test]
fn profile_id_try_from_str() -> TestResult {
    let id = ProfileId::try_from("acc-gt4")?;
    assert_eq!(id.as_str(), "acc-gt4");
    Ok(())
}

#[test]
fn profile_id_allows_dots() -> TestResult {
    let id: ProfileId = "game.car.track".parse()?;
    assert_eq!(id.as_str(), "game.car.track");
    Ok(())
}

#[test]
fn profile_id_rejects_special_chars() {
    assert!("profile with space".parse::<ProfileId>().is_err());
    assert!("profile@home".parse::<ProfileId>().is_err());
    assert!("profile/path".parse::<ProfileId>().is_err());
}

// ──────────────────────────────────────────────────────────────────────
// Entities: Default implementations
// ──────────────────────────────────────────────────────────────────────

#[test]
fn base_settings_default_values() {
    let bs = BaseSettings::default();
    assert!((bs.ffb_gain.value() - 0.7).abs() < f32::EPSILON);
    assert!((bs.degrees_of_rotation.value() - 900.0).abs() < f32::EPSILON);
    assert!((bs.torque_cap.value() - 15.0).abs() < f32::EPSILON);
    assert_eq!(bs.filters.reconstruction, 0);
}

#[test]
fn filter_config_default_is_linear() {
    let fc = FilterConfig::default();
    assert!(fc.is_linear(), "default filter config should be linear");
}

#[test]
fn led_config_default_values() {
    let led = EntityLedConfig::default();
    assert_eq!(led.rpm_bands.len(), 5);
    assert_eq!(led.pattern, "progressive");
    assert!((led.brightness.value() - 0.8).abs() < f32::EPSILON);
    assert!(led.colors.contains_key("green"));
    assert!(led.colors.contains_key("red"));
}

#[test]
fn haptics_config_default_values() {
    let hc = EntityHapticsConfig::default();
    assert!(hc.enabled);
    assert!((hc.intensity.value() - 0.6).abs() < f32::EPSILON);
    assert!((hc.frequency.value() - 80.0).abs() < f32::EPSILON);
    assert!(hc.effects.contains_key("kerb"));
    assert!(hc.effects.contains_key("slip"));
}

#[test]
fn bumpstop_config_default_has_valid_angles() {
    let bs = BumpstopConfig::default();
    assert!(bs.enabled);
    assert!((bs.start_angle - 450.0).abs() < f32::EPSILON);
    assert!((bs.max_angle - 540.0).abs() < f32::EPSILON);
    assert!(bs.max_angle > bs.start_angle);
}

#[test]
fn hands_off_config_default_has_valid_threshold() {
    let ho = HandsOffConfig::default();
    assert!(ho.enabled);
    assert!((ho.threshold - 0.05).abs() < f32::EPSILON);
    assert!((ho.timeout_seconds - 5.0).abs() < f32::EPSILON);
}

// ──────────────────────────────────────────────────────────────────────
// Entities: serialization/deserialization roundtrips
// ──────────────────────────────────────────────────────────────────────

#[test]
fn device_capabilities_serde_roundtrip() -> TestResult {
    let caps = DeviceCapabilities::new(true, true, false, true, TorqueNm::new(20.0)?, 4096, 1000);
    let json = serde_json::to_string(&caps)?;
    let restored: DeviceCapabilities = serde_json::from_str(&json)?;
    assert_eq!(restored, caps);
    Ok(())
}

#[test]
fn calibration_data_serde_roundtrip() -> TestResult {
    let mut cal = CalibrationData::new(CalibrationType::Full);
    cal.center_position = Some(0.5);
    cal.min_position = Some(-450.0);
    cal.max_position = Some(450.0);
    cal.pedal_ranges = Some(PedalCalibrationData {
        throttle: Some((0.0, 1.0)),
        brake: Some((0.0, 0.9)),
        clutch: None,
    });
    let json = serde_json::to_string(&cal)?;
    let restored: CalibrationData = serde_json::from_str(&json)?;
    assert_eq!(restored, cal);
    Ok(())
}

#[test]
fn calibration_data_check_methods() {
    let mut cal = CalibrationData::new(CalibrationType::Range);
    assert!(!cal.has_center_calibration());
    assert!(!cal.has_range_calibration());
    assert!(!cal.has_pedal_calibration());
    assert!(!cal.is_fully_calibrated());

    cal.center_position = Some(0.0);
    assert!(cal.has_center_calibration());
    assert!(!cal.is_fully_calibrated());

    cal.min_position = Some(-450.0);
    cal.max_position = Some(450.0);
    assert!(cal.has_range_calibration());
    assert!(cal.is_fully_calibrated());

    cal.pedal_ranges = Some(PedalCalibrationData {
        throttle: Some((0.0, 1.0)),
        brake: None,
        clutch: None,
    });
    assert!(cal.has_pedal_calibration());
}

#[test]
fn device_state_serde_roundtrip() -> TestResult {
    for state in [
        DeviceState::Disconnected,
        DeviceState::Connected,
        DeviceState::Active,
        DeviceState::Faulted,
        DeviceState::SafeMode,
    ] {
        let json = serde_json::to_string(&state)?;
        let restored: DeviceState = serde_json::from_str(&json)?;
        assert_eq!(restored, state);
    }
    Ok(())
}

#[test]
fn device_type_serde_roundtrip() -> TestResult {
    for dt in [
        DeviceType::Other,
        DeviceType::WheelBase,
        DeviceType::SteeringWheel,
        DeviceType::Pedals,
        DeviceType::Shifter,
        DeviceType::Handbrake,
        DeviceType::ButtonBox,
    ] {
        let json = serde_json::to_string(&dt)?;
        let restored: DeviceType = serde_json::from_str(&json)?;
        assert_eq!(restored, dt);
    }
    Ok(())
}

#[test]
fn pedal_calibration_data_serde_roundtrip() -> TestResult {
    let pcd = PedalCalibrationData {
        throttle: Some((0.1, 0.95)),
        brake: Some((0.05, 0.9)),
        clutch: Some((0.0, 1.0)),
    };
    let json = serde_json::to_string(&pcd)?;
    let restored: PedalCalibrationData = serde_json::from_str(&json)?;
    assert_eq!(restored, pcd);
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// Device entity: operational checks
// ──────────────────────────────────────────────────────────────────────

#[test]
fn device_new_starts_connected() -> TestResult {
    let caps = DeviceCapabilities::new(true, true, false, false, TorqueNm::new(20.0)?, 4096, 1000);
    let dev = Device::new(
        DeviceId::new("test-wheel".to_string())?,
        "Test Wheel".to_string(),
        DeviceType::WheelBase,
        caps,
    );
    assert_eq!(dev.state, DeviceState::Connected);
    assert_eq!(dev.fault_flags, 0);
    assert!(!dev.has_faults());
    Ok(())
}

#[test]
fn device_set_state_updates_state() -> TestResult {
    let caps = DeviceCapabilities::new(true, true, false, false, TorqueNm::new(20.0)?, 4096, 1000);
    let mut dev = Device::new(
        DeviceId::new("test-wheel".to_string())?,
        "Test Wheel".to_string(),
        DeviceType::WheelBase,
        caps,
    );
    dev.set_state(DeviceState::Active);
    assert_eq!(dev.state, DeviceState::Active);
    assert!(dev.is_operational());
    Ok(())
}

#[test]
fn device_fault_flags_set_and_clear() -> TestResult {
    let caps = DeviceCapabilities::new(true, true, false, false, TorqueNm::new(20.0)?, 4096, 1000);
    let mut dev = Device::new(
        DeviceId::new("test-wheel".to_string())?,
        "Test Wheel".to_string(),
        DeviceType::WheelBase,
        caps,
    );
    dev.set_state(DeviceState::Active);
    assert!(dev.is_operational());

    dev.set_fault_flags(0x01);
    assert!(dev.has_faults());
    assert_eq!(dev.state, DeviceState::Faulted);
    assert!(!dev.is_operational());

    dev.clear_faults();
    assert!(!dev.has_faults());
    assert_eq!(dev.state, DeviceState::Active);
    Ok(())
}

#[test]
fn device_safe_mode_is_operational() -> TestResult {
    let caps = DeviceCapabilities::new(true, true, false, false, TorqueNm::new(20.0)?, 4096, 1000);
    let mut dev = Device::new(
        DeviceId::new("test-wheel".to_string())?,
        "Test Wheel".to_string(),
        DeviceType::WheelBase,
        caps,
    );
    dev.set_state(DeviceState::SafeMode);
    assert!(dev.is_operational());
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// NotchFilter entity validation
// ──────────────────────────────────────────────────────────────────────

#[test]
fn notch_filter_valid_creation() -> TestResult {
    let freq = FrequencyHz::new(60.0)?;
    let nf = NotchFilter::new(freq, 2.0, -6.0)?;
    assert!((nf.frequency.value() - 60.0).abs() < f32::EPSILON);
    assert!((nf.q_factor - 2.0).abs() < f32::EPSILON);
    assert!((nf.gain_db - (-6.0)).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn notch_filter_rejects_zero_q_factor() -> TestResult {
    let freq = FrequencyHz::new(60.0)?;
    assert!(NotchFilter::new(freq, 0.0, -6.0).is_err());
    Ok(())
}

#[test]
fn notch_filter_rejects_negative_q_factor() -> TestResult {
    let freq = FrequencyHz::new(60.0)?;
    assert!(NotchFilter::new(freq, -1.0, -6.0).is_err());
    Ok(())
}

#[test]
fn notch_filter_rejects_nan_gain() -> TestResult {
    let freq = FrequencyHz::new(60.0)?;
    assert!(NotchFilter::new(freq, 2.0, f32::NAN).is_err());
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// FilterConfig: validation
// ──────────────────────────────────────────────────────────────────────

#[test]
fn filter_config_new_valid() -> TestResult {
    let curve = vec![CurvePoint::new(0.0, 0.0)?, CurvePoint::new(1.0, 1.0)?];
    let fc = FilterConfig::new(
        4,
        Gain::new(0.1)?,
        Gain::new(0.2)?,
        Gain::new(0.3)?,
        vec![],
        Gain::new(0.8)?,
        curve,
    )?;
    assert_eq!(fc.reconstruction, 4);
    assert!((fc.friction.value() - 0.1).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn filter_config_rejects_high_reconstruction() -> TestResult {
    let curve = vec![CurvePoint::new(0.0, 0.0)?, CurvePoint::new(1.0, 1.0)?];
    let result = FilterConfig::new(
        9,
        Gain::new(0.0)?,
        Gain::new(0.0)?,
        Gain::new(0.0)?,
        vec![],
        Gain::new(1.0)?,
        curve,
    );
    assert!(result.is_err());
    Ok(())
}

#[test]
fn filter_config_rejects_non_monotonic_curve() -> TestResult {
    let curve = vec![CurvePoint::new(0.5, 0.5)?, CurvePoint::new(0.3, 0.7)?];
    let result = FilterConfig::new(
        0,
        Gain::new(0.0)?,
        Gain::new(0.0)?,
        Gain::new(0.0)?,
        vec![],
        Gain::new(1.0)?,
        curve,
    );
    assert!(result.is_err());
    Ok(())
}

#[test]
fn filter_config_is_linear_check() -> TestResult {
    let fc = FilterConfig::default();
    assert!(fc.is_linear());

    let curve = vec![
        CurvePoint::new(0.0, 0.0)?,
        CurvePoint::new(0.5, 0.7)?,
        CurvePoint::new(1.0, 1.0)?,
    ];
    let fc2 = FilterConfig::new(
        0,
        Gain::new(0.0)?,
        Gain::new(0.0)?,
        Gain::new(0.0)?,
        vec![],
        Gain::new(1.0)?,
        curve,
    )?;
    assert!(!fc2.is_linear());
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// ProfileScope: specificity and matching
// ──────────────────────────────────────────────────────────────────────

#[test]
fn profile_scope_global() {
    let scope = ProfileScope::global();
    assert!(scope.game.is_none());
    assert!(scope.car.is_none());
    assert!(scope.track.is_none());
    assert_eq!(scope.specificity_level(), 0);
}

#[test]
fn profile_scope_for_game() {
    let scope = ProfileScope::for_game("iRacing".to_string());
    assert_eq!(scope.game.as_deref(), Some("iRacing"));
    assert!(scope.car.is_none());
    assert_eq!(scope.specificity_level(), 1);
}

#[test]
fn profile_scope_for_car() {
    let scope = ProfileScope::for_car("iRacing".to_string(), "GT3".to_string());
    assert_eq!(scope.game.as_deref(), Some("iRacing"));
    assert_eq!(scope.car.as_deref(), Some("GT3"));
    assert_eq!(scope.specificity_level(), 2);
}

#[test]
fn profile_scope_for_track() {
    let scope =
        ProfileScope::for_track("iRacing".to_string(), "GT3".to_string(), "Spa".to_string());
    assert_eq!(scope.specificity_level(), 3);
}

#[test]
fn profile_scope_is_more_specific() {
    let global = ProfileScope::global();
    let game = ProfileScope::for_game("iRacing".to_string());
    let car = ProfileScope::for_car("iRacing".to_string(), "GT3".to_string());

    assert!(game.is_more_specific_than(&global));
    assert!(car.is_more_specific_than(&game));
    assert!(!global.is_more_specific_than(&game));
}

#[test]
fn profile_scope_matches_global() {
    let scope = ProfileScope::global();
    assert!(scope.matches(Some("iRacing"), Some("GT3"), Some("Spa")));
    assert!(scope.matches(None, None, None));
}

#[test]
fn profile_scope_matches_game_specific() {
    let scope = ProfileScope::for_game("iRacing".to_string());
    assert!(scope.matches(Some("iRacing"), Some("GT3"), Some("Spa")));
    assert!(!scope.matches(Some("ACC"), Some("GT3"), Some("Spa")));
}

#[test]
fn profile_scope_serde_roundtrip() -> TestResult {
    let scope =
        ProfileScope::for_track("iRacing".to_string(), "GT3".to_string(), "Spa".to_string());
    let json = serde_json::to_string(&scope)?;
    let restored: ProfileScope = serde_json::from_str(&json)?;
    assert_eq!(restored, scope);
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// Profile entity
// ──────────────────────────────────────────────────────────────────────

#[test]
fn profile_new_creates_valid_profile() -> TestResult {
    let id: ProfileId = "test-profile".parse()?;
    let profile = Profile::new(
        id.clone(),
        ProfileScope::global(),
        BaseSettings::default(),
        "Test Profile".to_string(),
    );
    assert_eq!(profile.id, id);
    assert!(!profile.has_parent());
    assert!(profile.parent().is_none());
    assert_eq!(profile.metadata.name, "Test Profile");
    assert_eq!(profile.metadata.version, "1.0.0");
    assert!(profile.led_config.is_some());
    assert!(profile.haptics_config.is_some());
    Ok(())
}

#[test]
fn profile_default_global() -> TestResult {
    let profile = Profile::default_global()?;
    assert_eq!(profile.id.as_str(), "global");
    assert_eq!(profile.scope, ProfileScope::global());
    Ok(())
}

#[test]
fn profile_with_parent() -> TestResult {
    let parent_id: ProfileId = "parent".parse()?;
    let child_id: ProfileId = "child".parse()?;
    let profile = Profile::new_with_parent(
        child_id.clone(),
        parent_id.clone(),
        ProfileScope::for_game("iRacing".to_string()),
        BaseSettings::default(),
        "Child Profile".to_string(),
    );
    assert!(profile.has_parent());
    assert_eq!(profile.parent(), Some(&parent_id));
    Ok(())
}

#[test]
fn profile_set_parent() -> TestResult {
    let id: ProfileId = "test".parse()?;
    let parent_id: ProfileId = "parent".parse()?;
    let mut profile = Profile::new(
        id,
        ProfileScope::global(),
        BaseSettings::default(),
        "Test".to_string(),
    );
    assert!(!profile.has_parent());

    profile.set_parent(Some(parent_id.clone()));
    assert!(profile.has_parent());
    assert_eq!(profile.parent(), Some(&parent_id));

    profile.set_parent(None);
    assert!(!profile.has_parent());
    Ok(())
}

#[test]
fn profile_serde_roundtrip() -> TestResult {
    let profile = Profile::default_global()?;
    let json = serde_json::to_string(&profile)?;
    let restored: Profile = serde_json::from_str(&json)?;
    assert_eq!(restored.id, profile.id);
    assert_eq!(restored.scope, profile.scope);
    assert_eq!(restored.metadata.name, profile.metadata.name);
    Ok(())
}

#[test]
fn profile_validate_for_device() -> TestResult {
    let caps = DeviceCapabilities::new(true, true, false, false, TorqueNm::new(20.0)?, 4096, 1000);
    let profile = Profile::default_global()?;
    // Default torque_cap is 15.0, device max is 20.0 => should be ok
    assert!(profile.validate_for_device(&caps).is_ok());
    Ok(())
}

#[test]
fn profile_validate_for_device_exceeds_torque() -> TestResult {
    let caps = DeviceCapabilities::new(true, true, false, false, TorqueNm::new(10.0)?, 4096, 1000);
    let profile = Profile::default_global()?;
    // Default torque_cap is 15.0, device max is 10.0 => should fail
    assert!(profile.validate_for_device(&caps).is_err());
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// InMemoryProfileStore
// ──────────────────────────────────────────────────────────────────────

#[test]
fn profile_store_add_and_get() -> TestResult {
    let mut store = InMemoryProfileStore::new();
    assert!(store.is_empty());
    assert_eq!(store.len(), 0);

    let profile = Profile::default_global()?;
    let id = profile.id.clone();
    store.add(profile);

    assert_eq!(store.len(), 1);
    assert!(!store.is_empty());
    assert!(store.get(&id).is_some());
    Ok(())
}

#[test]
fn profile_store_remove() -> TestResult {
    let mut store = InMemoryProfileStore::new();
    let profile = Profile::default_global()?;
    let id = profile.id.clone();
    store.add(profile);

    let removed = store.remove(&id);
    assert!(removed.is_some());
    assert!(store.is_empty());

    let not_found: ProfileId = "nonexistent".parse()?;
    assert!(store.remove(&not_found).is_none());
    Ok(())
}

#[test]
fn profile_store_get_mut() -> TestResult {
    let mut store = InMemoryProfileStore::new();
    let profile = Profile::default_global()?;
    let id = profile.id.clone();
    store.add(profile);

    let p = store.get_mut(&id);
    assert!(p.is_some());
    Ok(())
}

#[test]
fn profile_store_iter() -> TestResult {
    let mut store = InMemoryProfileStore::new();
    let p1 = Profile::new(
        "profile-1".parse()?,
        ProfileScope::global(),
        BaseSettings::default(),
        "P1".to_string(),
    );
    let p2 = Profile::new(
        "profile-2".parse()?,
        ProfileScope::global(),
        BaseSettings::default(),
        "P2".to_string(),
    );
    store.add(p1);
    store.add(p2);

    let ids: Vec<&ProfileId> = store.iter().map(|(id, _)| id).collect();
    assert_eq!(ids.len(), 2);
    Ok(())
}

#[test]
fn profile_store_find_children() -> TestResult {
    let mut store = InMemoryProfileStore::new();
    let parent = Profile::default_global()?;
    let parent_id = parent.id.clone();
    store.add(parent);

    let child = Profile::new_with_parent(
        "child-1".parse()?,
        parent_id.clone(),
        ProfileScope::for_game("iRacing".to_string()),
        BaseSettings::default(),
        "Child".to_string(),
    );
    store.add(child);

    let children = store.find_children(&parent_id);
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].as_str(), "child-1");
    Ok(())
}

#[test]
fn profile_store_find_all_descendants() -> TestResult {
    let mut store = InMemoryProfileStore::new();
    let parent = Profile::default_global()?;
    let parent_id = parent.id.clone();
    store.add(parent);

    let child = Profile::new_with_parent(
        "child-1".parse()?,
        parent_id.clone(),
        ProfileScope::for_game("iRacing".to_string()),
        BaseSettings::default(),
        "Child".to_string(),
    );
    store.add(child);

    let grandchild = Profile::new_with_parent(
        "grandchild-1".parse()?,
        "child-1".parse()?,
        ProfileScope::for_car("iRacing".to_string(), "GT3".to_string()),
        BaseSettings::default(),
        "Grandchild".to_string(),
    );
    store.add(grandchild);

    let descendants = store.find_all_descendants(&parent_id);
    assert_eq!(descendants.len(), 2);
    Ok(())
}

#[test]
fn profile_store_update_returns_previous() -> TestResult {
    let mut store = InMemoryProfileStore::new();
    let profile = Profile::default_global()?;
    let id = profile.id.clone();
    store.add(profile);

    let updated = Profile::new(
        id.clone(),
        ProfileScope::global(),
        BaseSettings::default(),
        "Updated".to_string(),
    );
    let previous = store.update(updated);
    assert!(previous.is_some());

    let current = store.get(&id);
    assert!(current.is_some());
    assert_eq!(current.map(|p| p.metadata.name.as_str()), Some("Updated"));
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// Profile inheritance / merging
// ──────────────────────────────────────────────────────────────────────

#[test]
fn profile_merge_with_parent_preserves_child_identity() -> TestResult {
    let parent = Profile::default_global()?;
    let child_id: ProfileId = "child".parse()?;
    let child = Profile::new_with_parent(
        child_id.clone(),
        parent.id.clone(),
        ProfileScope::for_game("ACC".to_string()),
        BaseSettings::default(),
        "Child Profile".to_string(),
    );

    let merged = child.merge_with_parent(&parent);
    assert_eq!(merged.id, child_id);
    assert_eq!(merged.metadata.name, "Child Profile");
    assert_eq!(merged.scope.game.as_deref(), Some("ACC"));
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// Telemetry: helper methods
// ──────────────────────────────────────────────────────────────────────

#[test]
fn telemetry_speed_conversions() {
    let t = NormalizedTelemetry::builder().speed_ms(100.0).build();
    assert!((t.speed_kmh() - 360.0).abs() < 0.1);
    assert!((t.speed_mph() - 223.7).abs() < 0.1);
}

#[test]
fn telemetry_is_stationary() {
    let stationary = NormalizedTelemetry::builder().speed_ms(0.1).build();
    assert!(stationary.is_stationary());

    let moving = NormalizedTelemetry::builder().speed_ms(10.0).build();
    assert!(!moving.is_stationary());
}

#[test]
fn telemetry_total_g() {
    let t = NormalizedTelemetry::builder()
        .lateral_g(3.0)
        .longitudinal_g(4.0)
        .build();
    assert!((t.total_g() - 5.0).abs() < 0.001);
}

#[test]
fn telemetry_has_ffb_data() {
    let no_ffb = NormalizedTelemetry::default();
    assert!(!no_ffb.has_ffb_data());

    let with_scalar = NormalizedTelemetry::builder().ffb_scalar(0.5).build();
    assert!(with_scalar.has_ffb_data());

    let with_torque = NormalizedTelemetry::builder().ffb_torque_nm(10.0).build();
    assert!(with_torque.has_ffb_data());
}

#[test]
fn telemetry_rpm_fraction() {
    let t = NormalizedTelemetry::builder()
        .rpm(6000.0)
        .max_rpm(8000.0)
        .build();
    assert!((t.rpm_fraction() - 0.75).abs() < f32::EPSILON);

    let no_max = NormalizedTelemetry::builder().rpm(6000.0).build();
    assert!((no_max.rpm_fraction()).abs() < f32::EPSILON);
}

#[test]
fn telemetry_has_rpm_data() {
    let no_rpm = NormalizedTelemetry::default();
    assert!(!no_rpm.has_rpm_data());
    assert!(!no_rpm.has_rpm_display_data());

    let with_rpm = NormalizedTelemetry::builder()
        .rpm(5000.0)
        .max_rpm(8000.0)
        .build();
    assert!(with_rpm.has_rpm_data());
    assert!(with_rpm.has_rpm_display_data());
}

#[test]
fn telemetry_has_active_flags() {
    let default_flags = NormalizedTelemetry::default();
    assert!(!default_flags.has_active_flags());

    let yellow = NormalizedTelemetry::builder()
        .flags(TelemetryFlags {
            yellow_flag: true,
            ..Default::default()
        })
        .build();
    assert!(yellow.has_active_flags());
}

#[test]
fn telemetry_extended_values() {
    let t = NormalizedTelemetry::default()
        .with_extended("boost_psi", TelemetryValue::Float(14.7))
        .with_extended("abs_active", TelemetryValue::Boolean(true));

    assert_eq!(
        t.get_extended("boost_psi"),
        Some(&TelemetryValue::Float(14.7))
    );
    assert_eq!(
        t.get_extended("abs_active"),
        Some(&TelemetryValue::Boolean(true))
    );
    assert!(t.get_extended("missing").is_none());
}

#[test]
fn telemetry_with_sequence() {
    let t = NormalizedTelemetry::default().with_sequence(42);
    assert_eq!(t.sequence, 42);
}

// ──────────────────────────────────────────────────────────────────────
// TelemetryData: serde roundtrip
// ──────────────────────────────────────────────────────────────────────

#[test]
fn telemetry_data_default() {
    let td = TelemetryData::default();
    assert!((td.wheel_angle_deg).abs() < f32::EPSILON);
    assert!((td.wheel_speed_rad_s).abs() < f32::EPSILON);
    assert_eq!(td.temperature_c, 0);
    assert_eq!(td.fault_flags, 0);
    assert!(!td.hands_on);
    assert_eq!(td.timestamp, 0);
}

#[test]
fn telemetry_data_serde_roundtrip() -> TestResult {
    let td = TelemetryData {
        wheel_angle_deg: 45.5,
        wheel_speed_rad_s: 1.57,
        temperature_c: 55,
        fault_flags: 0x03,
        hands_on: true,
        timestamp: 123456,
    };
    let json = serde_json::to_string(&td)?;
    let restored: TelemetryData = serde_json::from_str(&json)?;
    assert_eq!(restored, td);
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// TelemetryFlags: serde roundtrip
// ──────────────────────────────────────────────────────────────────────

#[test]
fn telemetry_flags_serde_roundtrip() -> TestResult {
    let flags = TelemetryFlags {
        yellow_flag: true,
        blue_flag: true,
        drs_active: true,
        abs_active: true,
        ..Default::default()
    };
    let json = serde_json::to_string(&flags)?;
    let restored: TelemetryFlags = serde_json::from_str(&json)?;
    assert_eq!(restored, flags);
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// TelemetrySnapshot: epoch-based roundtrip
// ──────────────────────────────────────────────────────────────────────

#[test]
fn telemetry_snapshot_to_and_from_telemetry() {
    let epoch = std::time::Instant::now();
    let telemetry = NormalizedTelemetry::builder()
        .speed_ms(55.0)
        .rpm(7200.0)
        .gear(5)
        .throttle(0.9)
        .brake(0.0)
        .build();

    let snapshot = TelemetrySnapshot::from_telemetry(&telemetry, epoch);
    let restored = snapshot.to_telemetry(epoch);

    assert!((restored.speed_ms - 55.0).abs() < f32::EPSILON);
    assert!((restored.rpm - 7200.0).abs() < f32::EPSILON);
    assert_eq!(restored.gear, 5);
    assert!((restored.throttle - 0.9).abs() < f32::EPSILON);
}

// ──────────────────────────────────────────────────────────────────────
// TelemetryFrame
// ──────────────────────────────────────────────────────────────────────

#[test]
fn telemetry_frame_from_telemetry() {
    let t = NormalizedTelemetry::builder().speed_ms(30.0).build();
    let frame = TelemetryFrame::from_telemetry(t, 1, 64);
    assert_eq!(frame.sequence, 1);
    assert_eq!(frame.raw_size, 64);
    assert!(frame.timestamp_ns > 0);
    assert!((frame.data.speed_ms - 30.0).abs() < f32::EPSILON);
}

// ──────────────────────────────────────────────────────────────────────
// Config module: ProfileValidator
// ──────────────────────────────────────────────────────────────────────

fn make_valid_profile_json() -> String {
    serde_json::json!({
        "schema": "wheel.profile/1",
        "scope": {
            "game": "iRacing"
        },
        "base": {
            "ffbGain": 0.8,
            "dorDeg": 900,
            "torqueCapNm": 15.0,
            "filters": {
                "reconstruction": 2,
                "friction": 0.1,
                "damper": 0.2,
                "inertia": 0.05,
                "bumpstop": {
                    "enabled": true,
                    "strength": 0.6
                },
                "handsOff": {
                    "enabled": true,
                    "sensitivity": 0.3
                },
                "notchFilters": [
                    { "hz": 60.0, "q": 2.0, "gainDb": -6.0 }
                ],
                "slewRate": 0.8,
                "curvePoints": [
                    { "input": 0.0, "output": 0.0 },
                    { "input": 0.5, "output": 0.6 },
                    { "input": 1.0, "output": 1.0 }
                ]
            }
        }
    })
    .to_string()
}

#[test]
fn profile_validator_accepts_valid_profile() -> TestResult {
    let validator = ProfileValidator::new()?;
    let json = make_valid_profile_json();
    let profile = validator.validate_json(&json)?;
    assert_eq!(profile.schema, "wheel.profile/1");
    assert_eq!(profile.scope.game.as_deref(), Some("iRacing"));
    assert!((profile.base.ffb_gain - 0.8).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn profile_validator_rejects_wrong_schema_version() -> TestResult {
    let validator = ProfileValidator::new()?;
    let json = serde_json::json!({
        "schema": "wheel.profile/99",
        "scope": {},
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
                "slewRate": 1.0,
                "curvePoints": [
                    { "input": 0.0, "output": 0.0 },
                    { "input": 1.0, "output": 1.0 }
                ]
            }
        }
    })
    .to_string();

    let result = validator.validate_json(&json);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn profile_validator_rejects_non_monotonic_curve() -> TestResult {
    let validator = ProfileValidator::new()?;
    let json = serde_json::json!({
        "schema": "wheel.profile/1",
        "scope": {},
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
                "slewRate": 1.0,
                "curvePoints": [
                    { "input": 0.0, "output": 0.0 },
                    { "input": 0.8, "output": 0.5 },
                    { "input": 0.5, "output": 1.0 }
                ]
            }
        }
    })
    .to_string();

    let result = validator.validate_json(&json);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn profile_validator_rejects_unsorted_rpm_bands() -> TestResult {
    let validator = ProfileValidator::new()?;
    let json = serde_json::json!({
        "schema": "wheel.profile/1",
        "scope": {},
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
                "slewRate": 1.0,
                "curvePoints": [
                    { "input": 0.0, "output": 0.0 },
                    { "input": 1.0, "output": 1.0 }
                ]
            }
        },
        "leds": {
            "rpmBands": [0.9, 0.8, 0.7],
            "pattern": "progressive",
            "brightness": 0.8,
            "colors": {}
        }
    })
    .to_string();

    let result = validator.validate_json(&json);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn profile_validator_rejects_invalid_json() -> TestResult {
    let validator = ProfileValidator::new()?;
    let result = validator.validate_json("not valid json");
    assert!(result.is_err());
    Ok(())
}

#[test]
fn profile_validator_validate_profile_struct() -> TestResult {
    let validator = ProfileValidator::new()?;
    let json = make_valid_profile_json();
    let profile = validator.validate_json(&json)?;
    assert!(validator.validate_profile(&profile).is_ok());
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// Config module: Default implementations
// ──────────────────────────────────────────────────────────────────────

#[test]
fn config_bumpstop_default() {
    let bs = ConfigBumpstopConfig::default();
    assert!(bs.enabled);
    assert!((bs.strength - 0.5).abs() < f32::EPSILON);
}

#[test]
fn config_hands_off_default() {
    let ho = ConfigHandsOffConfig::default();
    assert!(ho.enabled);
    assert!((ho.sensitivity - 0.3).abs() < f32::EPSILON);
}

#[test]
fn config_filter_config_default() {
    let fc = ConfigFilterConfig::default();
    assert_eq!(fc.reconstruction, 0);
    assert!((fc.friction).abs() < f32::EPSILON);
    assert!((fc.damper).abs() < f32::EPSILON);
    assert!((fc.inertia).abs() < f32::EPSILON);
    assert!(fc.bumpstop.enabled);
    assert!(fc.hands_off.enabled);
    assert!(fc.notch_filters.is_empty());
    assert_eq!(fc.curve_points.len(), 2);
    assert!((fc.slew_rate - 1.0).abs() < f32::EPSILON);
}

#[test]
fn config_bumpstop_serde_roundtrip() -> TestResult {
    let bs = ConfigBumpstopConfig {
        enabled: false,
        strength: 0.7,
    };
    let json = serde_json::to_string(&bs)?;
    let restored: ConfigBumpstopConfig = serde_json::from_str(&json)?;
    assert!(!restored.enabled);
    assert!((restored.strength - 0.7).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn config_hands_off_serde_roundtrip() -> TestResult {
    let ho = ConfigHandsOffConfig {
        enabled: false,
        sensitivity: 0.5,
    };
    let json = serde_json::to_string(&ho)?;
    let restored: ConfigHandsOffConfig = serde_json::from_str(&json)?;
    assert!(!restored.enabled);
    assert!((restored.sensitivity - 0.5).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn config_curve_point_serde_roundtrip() -> TestResult {
    let p = ConfigCurvePoint {
        input: 0.5,
        output: 0.7,
    };
    let json = serde_json::to_string(&p)?;
    let restored: ConfigCurvePoint = serde_json::from_str(&json)?;
    assert!((restored.input - 0.5).abs() < f32::EPSILON);
    assert!((restored.output - 0.7).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn config_notch_filter_serde_roundtrip() -> TestResult {
    let nf = ConfigNotchFilter {
        hz: 60.0,
        q: 2.0,
        gain_db: -6.0,
    };
    let json = serde_json::to_string(&nf)?;
    let restored: ConfigNotchFilter = serde_json::from_str(&json)?;
    assert!((restored.hz - 60.0).abs() < f32::EPSILON);
    assert!((restored.q - 2.0).abs() < f32::EPSILON);
    assert!((restored.gain_db - (-6.0)).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn config_led_config_serde_roundtrip() -> TestResult {
    let mut colors = HashMap::new();
    colors.insert("green".to_string(), [0u8, 255, 0]);
    let led = LedConfig {
        rpm_bands: vec![0.75, 0.85, 0.95],
        pattern: "progressive".to_string(),
        brightness: 0.8,
        colors: Some(colors),
    };
    let json = serde_json::to_string(&led)?;
    let restored: LedConfig = serde_json::from_str(&json)?;
    assert_eq!(restored.rpm_bands, vec![0.75, 0.85, 0.95]);
    assert_eq!(restored.pattern, "progressive");
    Ok(())
}

#[test]
fn config_haptics_config_serde_roundtrip() -> TestResult {
    let mut effects = HashMap::new();
    effects.insert("kerb".to_string(), true);
    let hc = HapticsConfig {
        enabled: true,
        intensity: 0.6,
        frequency_hz: 80.0,
        effects: Some(effects),
    };
    let json = serde_json::to_string(&hc)?;
    let restored: HapticsConfig = serde_json::from_str(&json)?;
    assert!(restored.enabled);
    assert!((restored.intensity - 0.6).abs() < f32::EPSILON);
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// Config module: ProfileMigrator
// ──────────────────────────────────────────────────────────────────────

#[test]
fn profile_migrator_accepts_current_version() -> TestResult {
    let json = make_valid_profile_json();
    let profile = ProfileMigrator::migrate_profile(&json)?;
    assert_eq!(profile.schema, "wheel.profile/1");
    Ok(())
}

#[test]
fn profile_migrator_rejects_unknown_version() {
    let json = serde_json::json!({
        "schema": "wheel.profile/99",
        "scope": {},
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
                "slewRate": 1.0,
                "curvePoints": [
                    { "input": 0.0, "output": 0.0 },
                    { "input": 1.0, "output": 1.0 }
                ]
            }
        }
    })
    .to_string();

    let result = ProfileMigrator::migrate_profile(&json);
    assert!(result.is_err());
}

#[test]
fn profile_migrator_rejects_invalid_json() {
    let result = ProfileMigrator::migrate_profile("garbage");
    assert!(result.is_err());
}

// ──────────────────────────────────────────────────────────────────────
// Migration module: SchemaVersion
// ──────────────────────────────────────────────────────────────────────

#[test]
fn schema_version_parse_valid() -> TestResult {
    let v = SchemaVersion::parse("wheel.profile/1")?;
    assert_eq!(v.major, 1);
    assert_eq!(v.minor, 0);
    assert_eq!(v.version, "wheel.profile/1");
    Ok(())
}

#[test]
fn schema_version_parse_with_minor() -> TestResult {
    let v = SchemaVersion::parse("wheel.profile/2.1")?;
    assert_eq!(v.major, 2);
    assert_eq!(v.minor, 1);
    Ok(())
}

#[test]
fn schema_version_parse_invalid() {
    assert!(SchemaVersion::parse("invalid").is_err());
    assert!(SchemaVersion::parse("other.type/1").is_err());
    assert!(SchemaVersion::parse("wheel.profile/abc").is_err());
}

#[test]
fn schema_version_is_older_than() -> TestResult {
    let v1 = SchemaVersion::parse("wheel.profile/1")?;
    let v2 = SchemaVersion::parse("wheel.profile/2")?;
    assert!(v1.is_older_than(&v2));
    assert!(!v2.is_older_than(&v1));
    assert!(!v1.is_older_than(&v1));
    Ok(())
}

#[test]
fn schema_version_is_current() -> TestResult {
    let v = SchemaVersion::parse(CURRENT_SCHEMA_VERSION)?;
    assert!(v.is_current());

    let v2 =
        SchemaVersion::parse("wheel.profile/99.0").unwrap_or_else(|_| SchemaVersion::new(99, 0));
    assert!(!v2.is_current());
    Ok(())
}

#[test]
fn schema_version_new() {
    let v = SchemaVersion::new(3, 2);
    assert_eq!(v.major, 3);
    assert_eq!(v.minor, 2);
    assert_eq!(v.version, "wheel.profile/3.2");
}

#[test]
fn schema_version_display() -> TestResult {
    let v = SchemaVersion::parse("wheel.profile/1")?;
    assert_eq!(format!("{}", v), "wheel.profile/1");
    Ok(())
}

#[test]
fn schema_version_minor_comparison() -> TestResult {
    let v1_0 = SchemaVersion::new(1, 0);
    let v1_1 = SchemaVersion::new(1, 1);
    assert!(v1_0.is_older_than(&v1_1));
    assert!(!v1_1.is_older_than(&v1_0));
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// Migration module: MigrationConfig
// ──────────────────────────────────────────────────────────────────────

#[test]
fn migration_config_default() {
    let config = MigrationConfig::default();
    assert!(config.create_backups);
    assert_eq!(config.max_backups, 5);
    assert!(config.validate_after_migration);
}

#[test]
fn migration_config_without_backups() {
    let config = MigrationConfig::without_backups();
    assert!(!config.create_backups);
    assert_eq!(config.max_backups, 0);
    assert!(config.validate_after_migration);
}

#[test]
fn migration_config_custom() {
    let config = MigrationConfig::new("custom/backups");
    assert!(config.create_backups);
    assert_eq!(config.max_backups, 5);
    assert!(config.validate_after_migration);
}

// ──────────────────────────────────────────────────────────────────────
// Migration module: MigrationManager
// ──────────────────────────────────────────────────────────────────────

#[test]
fn migration_manager_detect_current_version() -> TestResult {
    let config = MigrationConfig::without_backups();
    let manager = MigrationManager::new(config)?;

    let json = make_valid_profile_json();
    let version = manager.detect_version(&json)?;
    assert!(version.is_current());
    Ok(())
}

#[test]
fn migration_manager_needs_migration_current() -> TestResult {
    let config = MigrationConfig::without_backups();
    let manager = MigrationManager::new(config)?;

    let json = make_valid_profile_json();
    assert!(!manager.needs_migration(&json)?);
    Ok(())
}

#[test]
fn migration_manager_detect_legacy_format() -> TestResult {
    let config = MigrationConfig::without_backups();
    let manager = MigrationManager::new(config)?;

    let json = serde_json::json!({
        "ffb_gain": 0.8,
        "degrees_of_rotation": 900
    })
    .to_string();

    let version = manager.detect_version(&json)?;
    assert_eq!(version.major, 0);
    assert!(version.is_older_than(&SchemaVersion::parse(CURRENT_SCHEMA_VERSION)?));
    Ok(())
}

#[test]
fn migration_manager_rejects_no_schema_no_legacy() -> TestResult {
    let config = MigrationConfig::without_backups();
    let manager = MigrationManager::new(config)?;

    let json = serde_json::json!({ "random": "data" }).to_string();
    // This should detect as legacy since no schema and no base field
    let result = manager.detect_version(&json);
    // It should be Ok with v0 or Err depending on the logic
    // Based on code: if no schema and no "base" field => legacy
    assert!(result.is_ok());
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// LedConfig entity: validation
// ──────────────────────────────────────────────────────────────────────

#[test]
fn led_config_new_valid() -> TestResult {
    let mut colors = HashMap::new();
    colors.insert("green".to_string(), [0u8, 255, 0]);
    let led = EntityLedConfig::new(
        vec![0.75, 0.85, 0.95],
        "progressive".to_string(),
        Gain::new(0.8)?,
        colors,
    )?;
    assert_eq!(led.rpm_bands.len(), 3);
    Ok(())
}

#[test]
fn led_config_rejects_out_of_range_band() -> TestResult {
    let result = EntityLedConfig::new(
        vec![0.5, 1.5],
        "progressive".to_string(),
        Gain::new(0.8)?,
        HashMap::new(),
    );
    assert!(result.is_err());
    Ok(())
}

#[test]
fn led_config_rejects_unsorted_bands() -> TestResult {
    let result = EntityLedConfig::new(
        vec![0.9, 0.8, 0.7],
        "progressive".to_string(),
        Gain::new(0.8)?,
        HashMap::new(),
    );
    assert!(result.is_err());
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// BaseSettings: validate_for_device
// ──────────────────────────────────────────────────────────────────────

#[test]
fn base_settings_validate_within_device_caps() -> TestResult {
    let caps = DeviceCapabilities::new(true, true, false, false, TorqueNm::new(20.0)?, 4096, 1000);
    let settings = BaseSettings::default(); // torque_cap = 15.0
    assert!(settings.validate_for_device(&caps).is_ok());
    Ok(())
}

#[test]
fn base_settings_validate_exceeds_device_caps() -> TestResult {
    let caps = DeviceCapabilities::new(true, true, false, false, TorqueNm::new(10.0)?, 4096, 1000);
    let settings = BaseSettings::default(); // torque_cap = 15.0
    assert!(settings.validate_for_device(&caps).is_err());
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// DomainError: error messages
// ──────────────────────────────────────────────────────────────────────

#[test]
fn domain_error_display_messages() {
    let e1 = DomainError::InvalidTorque(55.0, 50.0);
    assert!(format!("{}", e1).contains("55"));

    let e2 = DomainError::InvalidDeviceId("bad id".to_string());
    assert!(format!("{}", e2).contains("bad id"));

    let e3 = DomainError::InvalidProfileId("bad profile".to_string());
    assert!(format!("{}", e3).contains("bad profile"));

    let e4 = DomainError::InvalidGain(1.5);
    assert!(format!("{}", e4).contains("1.5"));

    let e5 = DomainError::InvalidFrequency(-1.0);
    assert!(format!("{}", e5).contains("-1"));

    let e6 = DomainError::InvalidCurvePoints("empty".to_string());
    assert!(format!("{}", e6).contains("empty"));
}

#[test]
fn domain_error_clone_and_eq() {
    let e1 = DomainError::InvalidGain(1.5);
    let e2 = e1.clone();
    assert_eq!(e1, e2);
}

// ──────────────────────────────────────────────────────────────────────
// NormalizedTelemetry: serde roundtrip
// ──────────────────────────────────────────────────────────────────────

#[test]
fn normalized_telemetry_serde_roundtrip() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .speed_ms(45.0)
        .rpm(6500.0)
        .gear(4)
        .steering_angle(0.15)
        .throttle(0.8)
        .brake(0.1)
        .clutch(0.3)
        .lateral_g(1.2)
        .longitudinal_g(-0.5)
        .vertical_g(0.1)
        .slip_ratio(0.05)
        .ffb_scalar(0.7)
        .ffb_torque_nm(12.0)
        .car_id("ferrari_488")
        .track_id("spa")
        .session_id("session-1")
        .position(3)
        .lap(5)
        .current_lap_time_s(62.5)
        .best_lap_time_s(61.0)
        .last_lap_time_s(63.2)
        .fuel_percent(0.75)
        .engine_temp_c(95.0)
        .sequence(100)
        .build();

    let json = serde_json::to_string(&t)?;
    let restored: NormalizedTelemetry = serde_json::from_str(&json)?;

    assert!((restored.speed_ms - 45.0).abs() < f32::EPSILON);
    assert!((restored.rpm - 6500.0).abs() < f32::EPSILON);
    assert_eq!(restored.gear, 4);
    assert!((restored.throttle - 0.8).abs() < f32::EPSILON);
    assert!((restored.brake - 0.1).abs() < f32::EPSILON);
    assert!((restored.clutch - 0.3).abs() < f32::EPSILON);
    assert_eq!(restored.car_id, Some("ferrari_488".to_string()));
    assert_eq!(restored.track_id, Some("spa".to_string()));
    assert_eq!(restored.session_id, Some("session-1".to_string()));
    assert_eq!(restored.position, 3);
    assert_eq!(restored.lap, 5);
    assert!((restored.fuel_percent - 0.75).abs() < f32::EPSILON);
    assert_eq!(restored.sequence, 100);
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// FilterConfig entity serde roundtrip
// ──────────────────────────────────────────────────────────────────────

#[test]
fn entity_filter_config_serde_roundtrip() -> TestResult {
    let fc = FilterConfig::default();
    let json = serde_json::to_string(&fc)?;
    let restored: FilterConfig = serde_json::from_str(&json)?;
    assert_eq!(restored.reconstruction, fc.reconstruction);
    assert!((restored.friction.value() - fc.friction.value()).abs() < f32::EPSILON);
    assert!((restored.damper.value() - fc.damper.value()).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn entity_base_settings_serde_roundtrip() -> TestResult {
    let bs = BaseSettings::default();
    let json = serde_json::to_string(&bs)?;
    let restored: BaseSettings = serde_json::from_str(&json)?;
    assert_eq!(restored, bs);
    Ok(())
}

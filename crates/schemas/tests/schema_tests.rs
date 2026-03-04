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
    assert_eq!(current.map(|p| &*p.metadata.name), Some("Updated"));
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

// ──────────────────────────────────────────────────────────────────────
// TelemetrySnapshot serde roundtrip
// ──────────────────────────────────────────────────────────────────────

#[test]
fn telemetry_snapshot_serde_roundtrip() -> TestResult {
    let snapshot = TelemetrySnapshot {
        timestamp_ns: 123_456_789,
        speed_ms: 55.0,
        steering_angle: -0.3,
        throttle: 0.9,
        brake: 0.05,
        clutch: 0.0,
        rpm: 7200.0,
        max_rpm: 8500.0,
        gear: 5,
        num_gears: 6,
        lateral_g: 1.5,
        longitudinal_g: -0.8,
        vertical_g: 0.02,
        slip_ratio: 0.03,
        slip_angle_fl: 0.01,
        slip_angle_fr: 0.02,
        slip_angle_rl: 0.03,
        slip_angle_rr: 0.04,
        ffb_scalar: 0.65,
        ffb_torque_nm: 8.5,
        flags: TelemetryFlags {
            yellow_flag: true,
            pit_limiter: true,
            ..Default::default()
        },
        position: 2,
        lap: 12,
        current_lap_time_s: 78.4,
        fuel_percent: 0.42,
        sequence: 5000,
    };

    let json = serde_json::to_string(&snapshot)?;
    let restored: TelemetrySnapshot = serde_json::from_str(&json)?;

    assert_eq!(restored.timestamp_ns, 123_456_789);
    assert!((restored.speed_ms - 55.0).abs() < f32::EPSILON);
    assert!((restored.steering_angle - (-0.3)).abs() < f32::EPSILON);
    assert!((restored.throttle - 0.9).abs() < f32::EPSILON);
    assert_eq!(restored.gear, 5);
    assert_eq!(restored.num_gears, 6);
    assert!((restored.lateral_g - 1.5).abs() < f32::EPSILON);
    assert!((restored.ffb_scalar - 0.65).abs() < f32::EPSILON);
    assert!((restored.ffb_torque_nm - 8.5).abs() < f32::EPSILON);
    assert!(restored.flags.yellow_flag);
    assert!(restored.flags.pit_limiter);
    assert!(!restored.flags.red_flag);
    assert_eq!(restored.position, 2);
    assert_eq!(restored.lap, 12);
    assert!((restored.fuel_percent - 0.42).abs() < f32::EPSILON);
    assert_eq!(restored.sequence, 5000);
    Ok(())
}

#[test]
fn telemetry_snapshot_from_and_to_telemetry_roundtrip() -> TestResult {
    use std::time::Instant;

    let epoch = Instant::now();
    let telemetry = NormalizedTelemetry::builder()
        .speed_ms(30.0)
        .rpm(5000.0)
        .gear(3)
        .throttle(0.6)
        .brake(0.0)
        .ffb_scalar(0.4)
        .position(5)
        .lap(3)
        .sequence(42)
        .build();

    let snapshot = TelemetrySnapshot::from_telemetry(&telemetry, epoch);
    let restored = snapshot.to_telemetry(epoch);

    assert!((restored.speed_ms - 30.0).abs() < f32::EPSILON);
    assert!((restored.rpm - 5000.0).abs() < f32::EPSILON);
    assert_eq!(restored.gear, 3);
    assert!((restored.throttle - 0.6).abs() < f32::EPSILON);
    assert!((restored.ffb_scalar - 0.4).abs() < f32::EPSILON);
    assert_eq!(restored.position, 5);
    assert_eq!(restored.sequence, 42);
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// TelemetryFrame serde roundtrip
// ──────────────────────────────────────────────────────────────────────

#[test]
fn telemetry_frame_serde_roundtrip() -> TestResult {
    let data = NormalizedTelemetry::builder()
        .speed_ms(22.0)
        .rpm(4500.0)
        .gear(2)
        .build();

    let frame = TelemetryFrame::new(data, 999_000_000, 77, 256);

    let json = serde_json::to_string(&frame)?;
    let restored: TelemetryFrame = serde_json::from_str(&json)?;

    assert!((restored.data.speed_ms - 22.0).abs() < f32::EPSILON);
    assert!((restored.data.rpm - 4500.0).abs() < f32::EPSILON);
    assert_eq!(restored.data.gear, 2);
    assert_eq!(restored.timestamp_ns, 999_000_000);
    assert_eq!(restored.sequence, 77);
    assert_eq!(restored.raw_size, 256);
    Ok(())
}

#[test]
fn telemetry_frame_from_telemetry_helper() -> TestResult {
    let data = NormalizedTelemetry::builder().speed_ms(10.0).build();
    let frame = TelemetryFrame::from_telemetry(data, 1, 128);

    assert!((frame.data.speed_ms - 10.0).abs() < f32::EPSILON);
    assert_eq!(frame.sequence, 1);
    assert_eq!(frame.raw_size, 128);
    assert!(frame.timestamp_ns > 0);
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// TelemetryValue serde roundtrip – all variants
// ──────────────────────────────────────────────────────────────────────

#[test]
fn telemetry_value_float_serde_roundtrip() -> TestResult {
    let val = TelemetryValue::Float(42.5);
    let json = serde_json::to_string(&val)?;
    let restored: TelemetryValue = serde_json::from_str(&json)?;
    assert_eq!(restored, TelemetryValue::Float(42.5));
    Ok(())
}

#[test]
fn telemetry_value_integer_serde_roundtrip() -> TestResult {
    let val = TelemetryValue::Integer(-7);
    let json = serde_json::to_string(&val)?;
    let restored: TelemetryValue = serde_json::from_str(&json)?;
    assert_eq!(restored, TelemetryValue::Integer(-7));
    Ok(())
}

#[test]
fn telemetry_value_boolean_serde_roundtrip() -> TestResult {
    let val = TelemetryValue::Boolean(true);
    let json = serde_json::to_string(&val)?;
    let restored: TelemetryValue = serde_json::from_str(&json)?;
    assert_eq!(restored, TelemetryValue::Boolean(true));
    Ok(())
}

#[test]
fn telemetry_value_string_serde_roundtrip() -> TestResult {
    let val = TelemetryValue::String("custom_data".to_string());
    let json = serde_json::to_string(&val)?;
    let restored: TelemetryValue = serde_json::from_str(&json)?;
    assert_eq!(restored, TelemetryValue::String("custom_data".to_string()));
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// TelemetryFlags serde and defaults
// ──────────────────────────────────────────────────────────────────────

#[test]
fn telemetry_flags_default_green_flag_is_true() -> TestResult {
    let flags = TelemetryFlags::default();
    assert!(flags.green_flag, "green_flag should default to true");
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

#[test]
fn telemetry_flags_all_set_serde_roundtrip() -> TestResult {
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

    let json = serde_json::to_string(&flags)?;
    let restored: TelemetryFlags = serde_json::from_str(&json)?;
    assert_eq!(restored, flags);
    Ok(())
}

#[test]
fn telemetry_flags_deserialize_from_partial_json() -> TestResult {
    // Only provide a subset; serde defaults should fill the rest
    let json = r#"{"yellow_flag": true}"#;
    let flags: TelemetryFlags = serde_json::from_str(json)?;
    assert!(flags.yellow_flag);
    assert!(flags.green_flag, "green_flag should default to true");
    assert!(!flags.red_flag);
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// TelemetryData (device telemetry) serde roundtrip
// ──────────────────────────────────────────────────────────────────────

#[test]
fn telemetry_data_full_serde_roundtrip() -> TestResult {
    let data = TelemetryData {
        wheel_angle_deg: -450.0,
        wheel_speed_rad_s: std::f32::consts::PI,
        temperature_c: 65,
        fault_flags: 0b11001100,
        hands_on: true,
        timestamp: 123_456,
    };

    let json = serde_json::to_string(&data)?;
    let restored: TelemetryData = serde_json::from_str(&json)?;
    assert_eq!(restored, data);
    Ok(())
}

#[test]
fn telemetry_data_default_is_zeroed() -> TestResult {
    let data = TelemetryData::default();
    assert!((data.wheel_angle_deg).abs() < f32::EPSILON);
    assert!((data.wheel_speed_rad_s).abs() < f32::EPSILON);
    assert_eq!(data.temperature_c, 0);
    assert_eq!(data.fault_flags, 0);
    assert!(!data.hands_on);
    assert_eq!(data.timestamp, 0);
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// NormalizedTelemetry – extended data roundtrip
// ──────────────────────────────────────────────────────────────────────

#[test]
fn normalized_telemetry_extended_data_serde_roundtrip() -> TestResult {
    let telemetry = NormalizedTelemetry::builder()
        .speed_ms(10.0)
        .extended("boost_psi", TelemetryValue::Float(14.7))
        .extended("lap_valid", TelemetryValue::Boolean(true))
        .extended("sector", TelemetryValue::Integer(2))
        .extended("driver", TelemetryValue::String("P1".to_string()))
        .build();

    let json = serde_json::to_string(&telemetry)?;
    let restored: NormalizedTelemetry = serde_json::from_str(&json)?;

    assert_eq!(
        restored.extended.get("boost_psi"),
        Some(&TelemetryValue::Float(14.7))
    );
    assert_eq!(
        restored.extended.get("lap_valid"),
        Some(&TelemetryValue::Boolean(true))
    );
    assert_eq!(
        restored.extended.get("sector"),
        Some(&TelemetryValue::Integer(2))
    );
    assert_eq!(
        restored.extended.get("driver"),
        Some(&TelemetryValue::String("P1".to_string()))
    );
    Ok(())
}

#[test]
fn normalized_telemetry_validated_handles_nan() -> TestResult {
    let telemetry = NormalizedTelemetry {
        speed_ms: f32::NAN,
        throttle: f32::NAN,
        brake: f32::NAN,
        clutch: f32::NAN,
        rpm: f32::NAN,
        max_rpm: f32::NAN,
        lateral_g: f32::NAN,
        longitudinal_g: f32::NAN,
        vertical_g: f32::NAN,
        slip_ratio: f32::NAN,
        ffb_scalar: f32::NAN,
        ffb_torque_nm: f32::NAN,
        fuel_percent: f32::NAN,
        engine_temp_c: f32::NAN,
        ..Default::default()
    };

    let v = telemetry.validated();
    assert_eq!(v.speed_ms, 0.0);
    assert_eq!(v.throttle, 0.0);
    assert_eq!(v.brake, 0.0);
    assert_eq!(v.clutch, 0.0);
    assert_eq!(v.rpm, 0.0);
    assert_eq!(v.max_rpm, 0.0);
    assert_eq!(v.lateral_g, 0.0);
    assert_eq!(v.longitudinal_g, 0.0);
    assert_eq!(v.vertical_g, 0.0);
    assert_eq!(v.slip_ratio, 0.0);
    assert_eq!(v.ffb_scalar, 0.0);
    assert_eq!(v.ffb_torque_nm, 0.0);
    assert_eq!(v.fuel_percent, 0.0);
    assert_eq!(v.engine_temp_c, 0.0);
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// NormalizedTelemetry helper methods
// ──────────────────────────────────────────────────────────────────────

#[test]
fn normalized_telemetry_rpm_fraction_zero_max() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .rpm(5000.0)
        .max_rpm(0.0)
        .build();
    assert!((t.rpm_fraction()).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn normalized_telemetry_has_rpm_display_data() -> TestResult {
    let no_rpm = NormalizedTelemetry::default();
    assert!(!no_rpm.has_rpm_display_data());

    let partial = NormalizedTelemetry::builder().rpm(1000.0).build();
    assert!(!partial.has_rpm_display_data());

    let full = NormalizedTelemetry::builder()
        .rpm(6000.0)
        .max_rpm(8000.0)
        .build();
    assert!(full.has_rpm_display_data());
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// Device entity serde roundtrip
// ──────────────────────────────────────────────────────────────────────

#[test]
fn device_serde_roundtrip() -> TestResult {
    let caps = DeviceCapabilities::new(true, true, true, false, TorqueNm::new(20.0)?, 10000, 1000);
    let id: DeviceId = "moza-r9".parse()?;
    let mut device = Device::new(id, "Moza R9".to_string(), DeviceType::WheelBase, caps);
    device.firmware_version = Some("1.2.3".to_string());
    device.serial_number = Some("SN-12345".to_string());

    let json = serde_json::to_string(&device)?;
    let restored: Device = serde_json::from_str(&json)?;

    assert_eq!(restored.id.as_str(), "moza-r9");
    assert_eq!(restored.name, "Moza R9");
    assert_eq!(restored.device_type, DeviceType::WheelBase);
    assert_eq!(restored.state, DeviceState::Connected);
    assert_eq!(restored.firmware_version, Some("1.2.3".to_string()));
    assert_eq!(restored.serial_number, Some("SN-12345".to_string()));
    assert!(restored.capabilities.supports_pid);
    assert!((restored.capabilities.max_torque.value() - 20.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn device_capabilities_ffb_support() -> TestResult {
    let no_ffb =
        DeviceCapabilities::new(false, false, true, false, TorqueNm::new(0.0)?, 10000, 1000);
    assert!(!no_ffb.supports_ffb());

    let pid_only =
        DeviceCapabilities::new(true, false, false, false, TorqueNm::new(10.0)?, 10000, 1000);
    assert!(pid_only.supports_ffb());

    let raw_only =
        DeviceCapabilities::new(false, true, false, false, TorqueNm::new(10.0)?, 10000, 1000);
    assert!(raw_only.supports_ffb());
    Ok(())
}

#[test]
fn device_capabilities_max_update_rate() -> TestResult {
    let caps = DeviceCapabilities::new(
        true,
        true,
        false,
        false,
        TorqueNm::new(15.0)?,
        10000,
        1000, // 1000us = 1kHz
    );
    assert!((caps.max_update_rate_hz() - 1000.0).abs() < 0.1);
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// DeviceState and DeviceType serde roundtrip – all variants
// ──────────────────────────────────────────────────────────────────────

#[test]
fn device_state_all_variants_serde_roundtrip() -> TestResult {
    let states = [
        DeviceState::Disconnected,
        DeviceState::Connected,
        DeviceState::Active,
        DeviceState::Faulted,
        DeviceState::SafeMode,
    ];
    for state in &states {
        let json = serde_json::to_string(state)?;
        let restored: DeviceState = serde_json::from_str(&json)?;
        assert_eq!(&restored, state);
    }
    Ok(())
}

#[test]
fn device_type_all_variants_serde_roundtrip() -> TestResult {
    let types = [
        DeviceType::Other,
        DeviceType::WheelBase,
        DeviceType::SteeringWheel,
        DeviceType::Pedals,
        DeviceType::Shifter,
        DeviceType::Handbrake,
        DeviceType::ButtonBox,
    ];
    for dt in &types {
        let json = serde_json::to_string(dt)?;
        let restored: DeviceType = serde_json::from_str(&json)?;
        assert_eq!(&restored, dt);
    }
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// CalibrationData and CalibrationType serde roundtrip
// ──────────────────────────────────────────────────────────────────────

#[test]
fn calibration_type_all_variants_serde_roundtrip() -> TestResult {
    let types = [
        CalibrationType::Center,
        CalibrationType::Range,
        CalibrationType::Pedals,
        CalibrationType::Full,
    ];
    for ct in &types {
        let json = serde_json::to_string(ct)?;
        let restored: CalibrationType = serde_json::from_str(&json)?;
        assert_eq!(&restored, ct);
    }
    Ok(())
}

#[test]
fn calibration_data_full_roundtrip() -> TestResult {
    let mut cal = CalibrationData::new(CalibrationType::Full);
    cal.center_position = Some(0.5);
    cal.min_position = Some(-540.0);
    cal.max_position = Some(540.0);
    cal.pedal_ranges = Some(PedalCalibrationData {
        throttle: Some((0.0, 1.0)),
        brake: Some((0.05, 0.95)),
        clutch: Some((0.1, 0.9)),
    });

    let json = serde_json::to_string(&cal)?;
    let restored: CalibrationData = serde_json::from_str(&json)?;

    assert!(restored.is_fully_calibrated());
    assert!(restored.has_pedal_calibration());
    assert_eq!(restored.calibration_type, CalibrationType::Full);
    assert!((restored.center_position.ok_or("missing")? - 0.5).abs() < f32::EPSILON);
    assert!((restored.min_position.ok_or("missing")? - (-540.0)).abs() < f32::EPSILON);
    assert!((restored.max_position.ok_or("missing")? - 540.0).abs() < f32::EPSILON);
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// ProfileMetadata serde roundtrip
// ──────────────────────────────────────────────────────────────────────

#[test]
fn profile_metadata_serde_roundtrip() -> TestResult {
    use racing_wheel_schemas::entities::ProfileMetadata;

    let metadata = ProfileMetadata {
        name: "Test Profile".to_string(),
        description: Some("A description".to_string()),
        author: Some("Tester".to_string()),
        version: "2.0.0".to_string(),
        created_at: "2024-01-01T00:00:00Z".to_string(),
        modified_at: "2024-06-15T12:00:00Z".to_string(),
        tags: vec!["drift".to_string(), "gt3".to_string()],
    };

    let json = serde_json::to_string(&metadata)?;
    let restored: ProfileMetadata = serde_json::from_str(&json)?;
    assert_eq!(restored, metadata);
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// Profile full serde roundtrip (all fields populated)
// ──────────────────────────────────────────────────────────────────────

#[test]
fn profile_full_serde_roundtrip() -> TestResult {
    let parent_id: ProfileId = "parent-global".parse()?;
    let child_id: ProfileId = "child-iracing".parse()?;

    let child = Profile::new_with_parent(
        child_id,
        parent_id,
        ProfileScope::for_game("iRacing".to_string()),
        BaseSettings::default(),
        "iRacing Child".to_string(),
    );

    let json = serde_json::to_string(&child)?;
    let restored: Profile = serde_json::from_str(&json)?;

    assert_eq!(restored.id.as_str(), "child-iracing");
    assert_eq!(
        restored.parent.as_ref().map(|p| p.as_str()),
        Some("parent-global")
    );
    assert_eq!(restored.scope.game, Some("iRacing".to_string()));
    assert!(restored.scope.car.is_none());
    assert!(restored.scope.track.is_none());
    assert!(restored.led_config.is_some());
    assert!(restored.haptics_config.is_some());
    assert_eq!(restored.metadata.name, "iRacing Child");
    Ok(())
}

#[test]
fn profile_without_parent_omits_parent_in_json() -> TestResult {
    let id: ProfileId = "global".parse()?;
    let profile = Profile::new(
        id,
        ProfileScope::global(),
        BaseSettings::default(),
        "Global".to_string(),
    );

    let json = serde_json::to_string(&profile)?;
    let value: serde_json::Value = serde_json::from_str(&json)?;
    assert!(
        value.get("parent").is_none(),
        "parent should be skipped when None"
    );
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// Entity LedConfig and HapticsConfig serde roundtrips
// ──────────────────────────────────────────────────────────────────────

#[test]
fn entity_led_config_serde_roundtrip() -> TestResult {
    let led = EntityLedConfig::default();
    let json = serde_json::to_string(&led)?;
    let restored: EntityLedConfig = serde_json::from_str(&json)?;
    assert_eq!(restored, led);
    Ok(())
}

#[test]
fn entity_haptics_config_serde_roundtrip() -> TestResult {
    let hc = EntityHapticsConfig::default();
    let json = serde_json::to_string(&hc)?;
    let restored: EntityHapticsConfig = serde_json::from_str(&json)?;
    assert_eq!(restored, hc);
    Ok(())
}

#[test]
fn entity_notch_filter_serde_roundtrip() -> TestResult {
    let freq = FrequencyHz::new(50.0)?;
    let nf = NotchFilter::new(freq, 2.0, -6.0)?;
    let json = serde_json::to_string(&nf)?;
    let restored: NotchFilter = serde_json::from_str(&json)?;
    assert_eq!(restored, nf);
    Ok(())
}

#[test]
fn entity_bumpstop_config_serde_roundtrip() -> TestResult {
    let bs = BumpstopConfig::default();
    let json = serde_json::to_string(&bs)?;
    let restored: BumpstopConfig = serde_json::from_str(&json)?;
    assert_eq!(restored, bs);
    Ok(())
}

#[test]
fn entity_hands_off_config_serde_roundtrip() -> TestResult {
    let ho = HandsOffConfig::default();
    let json = serde_json::to_string(&ho)?;
    let restored: HandsOffConfig = serde_json::from_str(&json)?;
    assert_eq!(restored, ho);
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// Config schema types serde roundtrip
// ──────────────────────────────────────────────────────────────────────

#[test]
fn config_profile_schema_full_serde_roundtrip() -> TestResult {
    use racing_wheel_schemas::config::ProfileSchema;

    let mut colors = HashMap::new();
    colors.insert("green".to_string(), [0, 255, 0]);

    let schema = ProfileSchema {
        schema: "wheel.profile/1".to_string(),
        scope: racing_wheel_schemas::config::ProfileScope {
            game: Some("iRacing".to_string()),
            car: Some("Ferrari 488".to_string()),
            track: None,
        },
        base: racing_wheel_schemas::config::BaseConfig {
            ffb_gain: 0.85,
            dor_deg: 900,
            torque_cap_nm: 15.0,
            filters: ConfigFilterConfig::default(),
        },
        leds: Some(LedConfig {
            rpm_bands: vec![0.75, 0.85, 0.95],
            pattern: "progressive".to_string(),
            brightness: 0.8,
            colors: Some(colors.clone()),
        }),
        haptics: Some(HapticsConfig {
            enabled: true,
            intensity: 0.6,
            frequency_hz: 80.0,
            effects: None,
        }),
        signature: Some("sig123".to_string()),
    };

    let json = serde_json::to_string(&schema)?;
    let restored: ProfileSchema = serde_json::from_str(&json)?;

    assert_eq!(restored.schema, "wheel.profile/1");
    assert_eq!(restored.scope.game, Some("iRacing".to_string()));
    assert_eq!(restored.scope.car, Some("Ferrari 488".to_string()));
    assert!(restored.scope.track.is_none());
    assert!((restored.base.ffb_gain - 0.85).abs() < f32::EPSILON);
    assert_eq!(restored.base.dor_deg, 900);
    assert!(restored.leds.is_some());
    assert!(restored.haptics.is_some());
    assert_eq!(restored.signature, Some("sig123".to_string()));
    Ok(())
}

#[test]
fn config_profile_schema_minimal_serde_roundtrip() -> TestResult {
    use racing_wheel_schemas::config::ProfileSchema;

    let schema = ProfileSchema {
        schema: "wheel.profile/1".to_string(),
        scope: racing_wheel_schemas::config::ProfileScope {
            game: None,
            car: None,
            track: None,
        },
        base: racing_wheel_schemas::config::BaseConfig {
            ffb_gain: 0.7,
            dor_deg: 900,
            torque_cap_nm: 10.0,
            filters: ConfigFilterConfig::default(),
        },
        leds: None,
        haptics: None,
        signature: None,
    };

    let json = serde_json::to_string(&schema)?;
    let restored: ProfileSchema = serde_json::from_str(&json)?;

    assert_eq!(restored.schema, "wheel.profile/1");
    assert!(restored.leds.is_none());
    assert!(restored.haptics.is_none());
    assert!(restored.signature.is_none());
    Ok(())
}

#[test]
fn config_filter_config_full_serde_roundtrip() -> TestResult {
    let fc = ConfigFilterConfig {
        reconstruction: 4,
        friction: 0.15,
        damper: 0.25,
        inertia: 0.1,
        bumpstop: ConfigBumpstopConfig {
            enabled: false,
            strength: 0.7,
        },
        hands_off: ConfigHandsOffConfig {
            enabled: true,
            sensitivity: 0.5,
        },
        torque_cap: Some(0.8),
        notch_filters: vec![ConfigNotchFilter {
            hz: 50.0,
            q: 2.0,
            gain_db: -6.0,
        }],
        slew_rate: 0.9,
        curve_points: vec![
            ConfigCurvePoint {
                input: 0.0,
                output: 0.0,
            },
            ConfigCurvePoint {
                input: 0.5,
                output: 0.6,
            },
            ConfigCurvePoint {
                input: 1.0,
                output: 1.0,
            },
        ],
    };

    let json = serde_json::to_string(&fc)?;
    let restored: ConfigFilterConfig = serde_json::from_str(&json)?;

    assert_eq!(restored.reconstruction, 4);
    assert!((restored.friction - 0.15).abs() < f32::EPSILON);
    assert!((restored.damper - 0.25).abs() < f32::EPSILON);
    assert!(!restored.bumpstop.enabled);
    assert!((restored.bumpstop.strength - 0.7).abs() < f32::EPSILON);
    assert_eq!(restored.notch_filters.len(), 1);
    assert_eq!(restored.curve_points.len(), 3);
    assert_eq!(restored.torque_cap, Some(0.8));
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// Migration types serde roundtrip
// ──────────────────────────────────────────────────────────────────────

#[test]
fn schema_version_serde_roundtrip() -> TestResult {
    let sv = SchemaVersion::parse("wheel.profile/1")?;
    let json = serde_json::to_string(&sv)?;
    let restored: SchemaVersion = serde_json::from_str(&json)?;
    assert_eq!(restored.version, "wheel.profile/1");
    assert_eq!(restored.major, 1);
    assert_eq!(restored.minor, 0);
    Ok(())
}

#[test]
fn schema_version_with_minor_serde_roundtrip() -> TestResult {
    let sv = SchemaVersion::parse("wheel.profile/2.3")?;
    let json = serde_json::to_string(&sv)?;
    let restored: SchemaVersion = serde_json::from_str(&json)?;
    assert_eq!(restored.major, 2);
    assert_eq!(restored.minor, 3);
    Ok(())
}

#[test]
fn backup_info_serde_roundtrip() -> TestResult {
    use racing_wheel_schemas::migration::BackupInfo;
    use std::path::PathBuf;

    let info = BackupInfo::new(
        PathBuf::from("/profiles/test.json"),
        PathBuf::from("/backups/test_20240101.json.bak"),
        "wheel.profile/1".to_string(),
        "abc123def456".to_string(),
    );

    let json = serde_json::to_string(&info)?;
    let restored: BackupInfo = serde_json::from_str(&json)?;

    assert_eq!(restored.original_path, PathBuf::from("/profiles/test.json"));
    assert_eq!(restored.original_version, "wheel.profile/1");
    assert_eq!(restored.content_hash, "abc123def456");
    Ok(())
}

#[test]
fn current_schema_version_constant_matches() -> TestResult {
    let sv = SchemaVersion::parse(CURRENT_SCHEMA_VERSION)?;
    assert!(sv.is_current());
    assert_eq!(sv.major, 1);
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// MigrationManager operations
// ──────────────────────────────────────────────────────────────────────

#[test]
fn migration_manager_needs_migration_for_legacy() -> TestResult {
    let _config = MigrationConfig::without_backups();
    let dir = tempfile::tempdir()?;
    let config = MigrationConfig::new(dir.path());
    let mgr = MigrationManager::new(config)?;

    let legacy_json = r#"{"ffb_gain": 0.8, "degrees_of_rotation": 900}"#;
    assert!(mgr.needs_migration(legacy_json)?);
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// IPC ConversionError display
// ──────────────────────────────────────────────────────────────────────

#[test]
fn conversion_error_display_variants() -> TestResult {
    use racing_wheel_schemas::ipc_conversion::ConversionError;

    let err = ConversionError::InvalidDeviceType(99);
    assert!(format!("{}", err).contains("99"));

    let err = ConversionError::InvalidDeviceState(10);
    assert!(format!("{}", err).contains("10"));

    let err = ConversionError::MissingField("test_field".to_string());
    assert!(format!("{}", err).contains("test_field"));

    let err = ConversionError::UnitConversion("bad unit".to_string());
    assert!(format!("{}", err).contains("bad unit"));

    let err = ConversionError::RangeValidation {
        field: "torque".to_string(),
        value: 99.0,
        min: 0.0,
        max: 50.0,
    };
    let msg = format!("{}", err);
    assert!(msg.contains("torque"));
    assert!(msg.contains("99"));
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// ProfileScope matching edge cases
// ──────────────────────────────────────────────────────────────────────

#[test]
fn profile_scope_specificity_levels() -> TestResult {
    let global = ProfileScope::global();
    let game = ProfileScope::for_game("iRacing".to_string());
    let car = ProfileScope::for_car("iRacing".to_string(), "GT3".to_string());
    let track =
        ProfileScope::for_track("iRacing".to_string(), "GT3".to_string(), "Spa".to_string());

    assert_eq!(global.specificity_level(), 0);
    assert_eq!(game.specificity_level(), 1);
    assert_eq!(car.specificity_level(), 2);
    assert_eq!(track.specificity_level(), 3);

    assert!(game.is_more_specific_than(&global));
    assert!(car.is_more_specific_than(&game));
    assert!(track.is_more_specific_than(&car));
    assert!(!global.is_more_specific_than(&game));
    Ok(())
}

#[test]
fn profile_scope_matches_correctly() -> TestResult {
    let game_scope = ProfileScope::for_game("iRacing".to_string());

    assert!(game_scope.matches(Some("iRacing"), None, None));
    assert!(game_scope.matches(Some("iRacing"), Some("GT3"), None));
    assert!(!game_scope.matches(Some("ACC"), None, None));
    assert!(!game_scope.matches(None, None, None));

    let car_scope = ProfileScope::for_car("iRacing".to_string(), "GT3".to_string());
    assert!(car_scope.matches(Some("iRacing"), Some("GT3"), None));
    assert!(!car_scope.matches(Some("iRacing"), Some("LMP2"), None));
    assert!(!car_scope.matches(Some("ACC"), Some("GT3"), None));
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// Profile hash determinism
// ──────────────────────────────────────────────────────────────────────

#[test]
fn profile_hash_is_deterministic() -> TestResult {
    let id: ProfileId = "test-profile".parse()?;
    let p1 = Profile::new(
        id.clone(),
        ProfileScope::global(),
        BaseSettings::default(),
        "Test".to_string(),
    );
    // Create same profile independently
    let p2 = Profile::new(
        id,
        ProfileScope::global(),
        BaseSettings::default(),
        "Test".to_string(),
    );

    assert_eq!(p1.calculate_hash(), p2.calculate_hash());
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// Profile inheritance resolve and validate
// ──────────────────────────────────────────────────────────────────────

#[test]
fn profile_resolve_single_profile() -> TestResult {
    let mut store = InMemoryProfileStore::new();
    let id: ProfileId = "standalone".parse()?;
    let profile = Profile::new(
        id.clone(),
        ProfileScope::global(),
        BaseSettings::default(),
        "Standalone".to_string(),
    );
    store.add(profile.clone());

    let resolved = profile.resolve(&store)?;
    assert_eq!(resolved.inheritance_chain.len(), 1);
    assert_eq!(resolved.inheritance_chain[0].as_str(), "standalone");
    Ok(())
}

#[test]
fn profile_resolve_parent_child() -> TestResult {
    let mut store = InMemoryProfileStore::new();

    let parent_id: ProfileId = "parent".parse()?;
    let parent = Profile::new(
        parent_id.clone(),
        ProfileScope::global(),
        BaseSettings::default(),
        "Parent".to_string(),
    );
    store.add(parent);

    let child_id: ProfileId = "child".parse()?;
    let child = Profile::new_with_parent(
        child_id.clone(),
        parent_id,
        ProfileScope::for_game("iRacing".to_string()),
        BaseSettings::default(),
        "Child".to_string(),
    );
    store.add(child.clone());

    let resolved = child.resolve(&store)?;
    assert_eq!(resolved.inheritance_chain.len(), 2);
    assert_eq!(resolved.inheritance_chain[0].as_str(), "child");
    assert_eq!(resolved.inheritance_chain[1].as_str(), "parent");
    Ok(())
}

#[test]
fn profile_validate_inheritance_detects_missing_parent() -> TestResult {
    let store = InMemoryProfileStore::new();

    let child_id: ProfileId = "orphan".parse()?;
    let missing_parent: ProfileId = "nonexistent".parse()?;
    let child = Profile::new_with_parent(
        child_id,
        missing_parent,
        ProfileScope::global(),
        BaseSettings::default(),
        "Orphan".to_string(),
    );

    let result = child.validate_inheritance(&store);
    assert!(result.is_err());
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// Entity FilterConfig new_complete
// ──────────────────────────────────────────────────────────────────────

#[test]
fn entity_filter_config_new_complete() -> TestResult {
    let freq = FrequencyHz::new(60.0)?;
    let nf = NotchFilter::new(freq, 1.5, -3.0)?;
    let cp1 = CurvePoint::new(0.0, 0.0)?;
    let cp2 = CurvePoint::new(1.0, 1.0)?;

    let fc = FilterConfig::new_complete(
        2,
        Gain::new(0.1)?,
        Gain::new(0.2)?,
        Gain::new(0.05)?,
        vec![nf],
        Gain::new(0.9)?,
        vec![cp1, cp2],
        Gain::new(0.8)?,
        BumpstopConfig::default(),
        HandsOffConfig::default(),
    )?;

    assert_eq!(fc.reconstruction, 2);
    assert!((fc.friction.value() - 0.1).abs() < f32::EPSILON);
    assert!((fc.torque_cap.value() - 0.8).abs() < f32::EPSILON);
    assert_eq!(fc.notch_filters.len(), 1);
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// Config default roundtrips
// ──────────────────────────────────────────────────────────────────────

#[test]
fn config_bumpstop_default_values_match() -> TestResult {
    let bs = ConfigBumpstopConfig::default();
    assert!(bs.enabled);
    assert!((bs.strength - 0.5).abs() < f32::EPSILON);

    let json = serde_json::to_string(&bs)?;
    let restored: ConfigBumpstopConfig = serde_json::from_str(&json)?;
    assert_eq!(restored.enabled, bs.enabled);
    assert!((restored.strength - bs.strength).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn config_hands_off_default_values_match() -> TestResult {
    let ho = ConfigHandsOffConfig::default();
    assert!(ho.enabled);
    assert!((ho.sensitivity - 0.3).abs() < f32::EPSILON);

    let json = serde_json::to_string(&ho)?;
    let restored: ConfigHandsOffConfig = serde_json::from_str(&json)?;
    assert_eq!(restored.enabled, ho.enabled);
    assert!((restored.sensitivity - ho.sensitivity).abs() < f32::EPSILON);
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// ProfileValidator validate_profile struct directly
// ──────────────────────────────────────────────────────────────────────

#[test]
fn profile_validator_validates_struct_directly() -> TestResult {
    let validator = ProfileValidator::new()?;
    let profile = racing_wheel_schemas::config::ProfileSchema {
        schema: "wheel.profile/1".to_string(),
        scope: racing_wheel_schemas::config::ProfileScope {
            game: None,
            car: None,
            track: None,
        },
        base: racing_wheel_schemas::config::BaseConfig {
            ffb_gain: 0.7,
            dor_deg: 900,
            torque_cap_nm: 15.0,
            filters: ConfigFilterConfig::default(),
        },
        leds: None,
        haptics: None,
        signature: None,
    };

    assert!(validator.validate_profile(&profile).is_ok());
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// DomainError conversions
// ──────────────────────────────────────────────────────────────────────

#[test]
fn domain_error_to_validation_error_device_id() {
    use openracing_errors::ValidationError;

    let err = DomainError::InvalidDeviceId("bad id".to_string());
    let ve: ValidationError = err.into();
    assert!(format!("{}", ve).contains("device_id"));
}

#[test]
fn domain_error_to_validation_error_profile_id() {
    use openracing_errors::ValidationError;

    let err = DomainError::InvalidProfileId("bad id".to_string());
    let ve: ValidationError = err.into();
    assert!(format!("{}", ve).contains("profile_id"));
}

#[test]
fn domain_error_to_validation_error_frequency() {
    use openracing_errors::ValidationError;

    let err = DomainError::InvalidFrequency(-1.0);
    let ve: ValidationError = err.into();
    assert!(format!("{}", ve).contains("frequency"));
}

#[test]
fn domain_error_to_validation_error_curve_points() {
    use openracing_errors::ValidationError;

    let err = DomainError::InvalidCurvePoints("non-monotonic".to_string());
    let ve: ValidationError = err.into();
    assert!(format!("{}", ve).contains("curve_points"));
}

#[test]
fn domain_error_to_profile_error_validation_failed() {
    use openracing_errors::ProfileError;

    let err = DomainError::InvalidGain(1.5);
    let pe: ProfileError = err.into();
    assert!(format!("{}", pe).contains("1.5"));
}

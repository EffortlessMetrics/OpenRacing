//! Extended snapshot tests for schema types — telemetry, device descriptors,
//! calibration, profile scopes, config schema types, and schema errors.
//!
//! Complements `snapshot_expansion_tests.rs` by covering NormalizedTelemetry
//! JSON serialization, DeviceState/DeviceType enums, CalibrationData,
//! TelemetryFlags, TelemetrySnapshot, config module types, and SchemaError display.

use racing_wheel_schemas::config::SchemaError;
use racing_wheel_schemas::domain::{DomainError, TorqueNm};
use racing_wheel_schemas::entities::{
    CalibrationData, CalibrationType, DeviceCapabilities, DeviceState, DeviceType, ProfileScope,
};
use racing_wheel_schemas::telemetry::{
    NormalizedTelemetry, TelemetryData, TelemetryFlags, TelemetrySnapshot, TelemetryValue,
};

type BoxErr = Box<dyn std::error::Error + Send + Sync>;

// =========================================================================
// NormalizedTelemetry JSON snapshots
// =========================================================================

#[test]
fn snapshot_normalized_telemetry_default_json() -> Result<(), BoxErr> {
    let telemetry = NormalizedTelemetry::default();
    let json = serde_json::to_string_pretty(&telemetry)?;
    insta::assert_snapshot!("normalized_telemetry_default_json", json);
    Ok(())
}

#[test]
fn snapshot_normalized_telemetry_populated_json() -> Result<(), BoxErr> {
    let telemetry = NormalizedTelemetry::builder()
        .speed_ms(45.0)
        .rpm(6500.0)
        .max_rpm(8000.0)
        .gear(4)
        .num_gears(6)
        .throttle(0.8)
        .brake(0.1)
        .clutch(0.0)
        .steering_angle(0.15)
        .lateral_g(1.2)
        .longitudinal_g(-0.3)
        .slip_ratio(0.05)
        .ffb_scalar(0.75)
        .ffb_torque_nm(8.5)
        .tire_temps_c([85, 90, 80, 82])
        .tire_pressures_psi([26.5, 27.0, 25.0, 25.5])
        .car_id("porsche-911-gt3-r")
        .track_id("spa-francorchamps")
        .build();
    let json = serde_json::to_string_pretty(&telemetry)?;
    insta::assert_snapshot!("normalized_telemetry_populated_json", json);
    Ok(())
}

// =========================================================================
// TelemetryFlags JSON snapshots
// =========================================================================

#[test]
fn snapshot_telemetry_flags_default_json() -> Result<(), BoxErr> {
    let flags = TelemetryFlags::default();
    let json = serde_json::to_string_pretty(&flags)?;
    insta::assert_snapshot!("telemetry_flags_default_json", json);
    Ok(())
}

#[test]
fn snapshot_telemetry_flags_active_race_json() -> Result<(), BoxErr> {
    let flags = TelemetryFlags {
        yellow_flag: true,
        red_flag: false,
        blue_flag: false,
        checkered_flag: false,
        green_flag: true,
        pit_limiter: false,
        in_pits: false,
        drs_available: true,
        drs_active: false,
        abs_active: true,
        traction_control: true,
        ..TelemetryFlags::default()
    };
    let json = serde_json::to_string_pretty(&flags)?;
    insta::assert_snapshot!("telemetry_flags_active_race_json", json);
    Ok(())
}

// =========================================================================
// TelemetryValue variants
// =========================================================================

#[test]
fn snapshot_telemetry_value_all_variants_json() -> Result<(), BoxErr> {
    let values: &[(&str, TelemetryValue)] = &[
        ("float", TelemetryValue::Float(42.5)),
        ("integer", TelemetryValue::Integer(42)),
        ("boolean", TelemetryValue::Boolean(true)),
        ("string", TelemetryValue::String("track-temp".into())),
    ];
    let formatted: Vec<String> = values
        .iter()
        .map(|(label, v)| {
            let json = serde_json::to_string(v).unwrap_or_default();
            format!("{label}: {json}")
        })
        .collect();
    insta::assert_snapshot!("telemetry_value_variants_json", formatted.join("\n"));
    Ok(())
}

// =========================================================================
// TelemetrySnapshot JSON
// =========================================================================

#[test]
fn snapshot_telemetry_snapshot_json() -> Result<(), BoxErr> {
    let snap = TelemetrySnapshot {
        timestamp_ns: 1_000_000_000,
        speed_ms: 55.0,
        steering_angle: -0.1,
        throttle: 1.0,
        brake: 0.0,
        clutch: 0.0,
        rpm: 7200.0,
        max_rpm: 8500.0,
        gear: 5,
        num_gears: 7,
        lateral_g: 0.8,
        longitudinal_g: 0.5,
        vertical_g: 0.0,
        slip_ratio: 0.02,
        slip_angle_fl: 0.0,
        slip_angle_fr: 0.0,
        slip_angle_rl: 0.0,
        slip_angle_rr: 0.0,
        ffb_scalar: -0.6,
        ffb_torque_nm: 0.0,
        flags: TelemetryFlags::default(),
        position: 3,
        lap: 12,
        current_lap_time_s: 82.5,
        fuel_percent: 0.65,
        sequence: 42,
    };
    let json = serde_json::to_string_pretty(&snap)?;
    insta::assert_snapshot!("telemetry_snapshot_json", json);
    Ok(())
}

// =========================================================================
// TelemetryData JSON
// =========================================================================

#[test]
fn snapshot_telemetry_data_json() -> Result<(), BoxErr> {
    let data = TelemetryData {
        wheel_angle_deg: 90.0,
        wheel_speed_rad_s: 2.5,
        temperature_c: 45,
        fault_flags: 0,
        hands_on: true,
        timestamp: 123456789,
    };
    let json = serde_json::to_string_pretty(&data)?;
    insta::assert_snapshot!("telemetry_data_json", json);
    Ok(())
}

// =========================================================================
// DeviceState and DeviceType enums
// =========================================================================

#[test]
fn snapshot_device_state_all_variants_json() -> Result<(), BoxErr> {
    let states = [
        DeviceState::Disconnected,
        DeviceState::Connected,
        DeviceState::Active,
        DeviceState::Faulted,
        DeviceState::SafeMode,
    ];
    let formatted: Vec<String> = states
        .iter()
        .map(|s| {
            let json = serde_json::to_string(s).unwrap_or_default();
            format!("{s:?}: {json}")
        })
        .collect();
    insta::assert_snapshot!("device_state_variants_json", formatted.join("\n"));
    Ok(())
}

#[test]
fn snapshot_device_type_all_variants_json() -> Result<(), BoxErr> {
    let types = [
        DeviceType::Other,
        DeviceType::WheelBase,
        DeviceType::SteeringWheel,
        DeviceType::Pedals,
        DeviceType::Shifter,
        DeviceType::Handbrake,
        DeviceType::ButtonBox,
    ];
    let formatted: Vec<String> = types
        .iter()
        .map(|t| {
            let json = serde_json::to_string(t).unwrap_or_default();
            format!("{t:?}: {json}")
        })
        .collect();
    insta::assert_snapshot!("device_type_variants_json", formatted.join("\n"));
    Ok(())
}

// =========================================================================
// DeviceCapabilities JSON snapshot
// =========================================================================

#[test]
fn snapshot_device_capabilities_dd_wheelbase_json() -> Result<(), DomainError> {
    let caps = DeviceCapabilities::new(false, true, true, true, TorqueNm::new(25.0)?, 65535, 1000);
    let json = serde_json::to_string_pretty(&caps)
        .map_err(|_| DomainError::InvalidCurvePoints("serialization failed".into()))?;
    insta::assert_snapshot!("device_capabilities_dd_json", json);
    Ok(())
}

#[test]
fn snapshot_device_capabilities_belt_drive_json() -> Result<(), DomainError> {
    let caps = DeviceCapabilities::new(true, false, false, false, TorqueNm::new(3.0)?, 4096, 4000);
    let json = serde_json::to_string_pretty(&caps)
        .map_err(|_| DomainError::InvalidCurvePoints("serialization failed".into()))?;
    insta::assert_snapshot!("device_capabilities_belt_json", json);
    Ok(())
}

// =========================================================================
// CalibrationData JSON snapshot
// =========================================================================

#[test]
fn snapshot_calibration_data_center_json() -> Result<(), BoxErr> {
    let mut cal = CalibrationData {
        center_position: Some(0.0),
        min_position: None,
        max_position: None,
        pedal_ranges: None,
        calibrated_at: Some("2024-01-15T12:00:00Z".into()),
        calibration_type: CalibrationType::Center,
    };
    cal.center_position = Some(0.5);
    let json = serde_json::to_string_pretty(&cal)?;
    insta::assert_snapshot!("calibration_data_center_json", json);
    Ok(())
}

#[test]
fn snapshot_calibration_type_all_variants_json() -> Result<(), BoxErr> {
    let types = [
        CalibrationType::Center,
        CalibrationType::Range,
        CalibrationType::Pedals,
        CalibrationType::Full,
    ];
    let formatted: Vec<String> = types
        .iter()
        .map(|t| {
            let json = serde_json::to_string(t).unwrap_or_default();
            format!("{t:?}: {json}")
        })
        .collect();
    insta::assert_snapshot!("calibration_type_variants_json", formatted.join("\n"));
    Ok(())
}

// =========================================================================
// ProfileScope JSON snapshots
// =========================================================================

#[test]
fn snapshot_profile_scope_global_json() -> Result<(), BoxErr> {
    let scope = ProfileScope::global();
    let json = serde_json::to_string_pretty(&scope)?;
    insta::assert_snapshot!("profile_scope_global_json", json);
    Ok(())
}

#[test]
fn snapshot_profile_scope_game_car_track_json() -> Result<(), BoxErr> {
    let scope = ProfileScope::for_track("iRacing".into(), "porsche-911-gt3-r".into(), "spa".into());
    let json = serde_json::to_string_pretty(&scope)?;
    insta::assert_snapshot!("profile_scope_game_car_track_json", json);
    Ok(())
}

// =========================================================================
// SchemaError Display snapshots
// =========================================================================

#[test]
fn snapshot_schema_error_unsupported_version() {
    let err = SchemaError::UnsupportedSchemaVersion("wheel.profile/99".into());
    insta::assert_snapshot!("schema_error_unsupported_version", format!("{err}"));
}

#[test]
fn snapshot_schema_error_validation_error() {
    let err = SchemaError::ValidationError {
        path: "base.filters.friction".into(),
        message: "value must be between 0.0 and 1.0".into(),
    };
    insta::assert_snapshot!("schema_error_validation", format!("{err}"));
}

#[test]
fn snapshot_schema_error_non_monotonic_curve() {
    let err = SchemaError::NonMonotonicCurve;
    insta::assert_snapshot!("schema_error_non_monotonic", format!("{err}"));
}

#[test]
fn snapshot_schema_error_schema_compilation() {
    let err = SchemaError::SchemaCompilationError("invalid $ref pointer".into());
    insta::assert_snapshot!("schema_error_compilation", format!("{err}"));
}

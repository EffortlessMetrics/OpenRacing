//! Snapshot tests for schemas — domain errors, config types, and domain value objects.

use racing_wheel_schemas::domain::{Degrees, DeviceId, DomainError, Gain, TorqueNm};
use racing_wheel_schemas::entities::{BaseSettings, BumpstopConfig, FilterConfig, HandsOffConfig};
use racing_wheel_schemas::ipc_conversion::ConversionError;
use racing_wheel_schemas::migration::MigrationError;

// --- DomainError Display (all 10 variants) ---

#[test]
fn snapshot_domain_error_invalid_torque() {
    let err = DomainError::InvalidTorque(55.0, 50.0);
    insta::assert_snapshot!("domain_error_invalid_torque", format!("{}", err));
}

#[test]
fn snapshot_domain_error_invalid_degrees() {
    let err = DomainError::InvalidDegrees(100.0, 180.0, 2160.0);
    insta::assert_snapshot!("domain_error_invalid_degrees", format!("{}", err));
}

#[test]
fn snapshot_domain_error_invalid_device_id() {
    let err = DomainError::InvalidDeviceId("has spaces".to_string());
    insta::assert_snapshot!("domain_error_invalid_device_id", format!("{}", err));
}

#[test]
fn snapshot_domain_error_invalid_profile_id() {
    let err = DomainError::InvalidProfileId("".to_string());
    insta::assert_snapshot!("domain_error_invalid_profile_id", format!("{}", err));
}

#[test]
fn snapshot_domain_error_invalid_gain() {
    let err = DomainError::InvalidGain(1.5);
    insta::assert_snapshot!("domain_error_invalid_gain", format!("{}", err));
}

#[test]
fn snapshot_domain_error_invalid_frequency() {
    let err = DomainError::InvalidFrequency(-10.0);
    insta::assert_snapshot!("domain_error_invalid_frequency", format!("{}", err));
}

#[test]
fn snapshot_domain_error_invalid_curve_points() {
    let err = DomainError::InvalidCurvePoints("curve must be monotonic".to_string());
    insta::assert_snapshot!("domain_error_invalid_curve_points", format!("{}", err));
}

#[test]
fn snapshot_domain_error_inheritance_depth() {
    let err = DomainError::InheritanceDepthExceeded {
        depth: 11,
        max_depth: 10,
    };
    insta::assert_snapshot!("domain_error_inheritance_depth", format!("{}", err));
}

#[test]
fn snapshot_domain_error_circular_inheritance() {
    let err = DomainError::CircularInheritance {
        profile_id: "my-profile".to_string(),
    };
    insta::assert_snapshot!("domain_error_circular_inheritance", format!("{}", err));
}

#[test]
fn snapshot_domain_error_parent_not_found() {
    let err = DomainError::ParentProfileNotFound {
        profile_id: "base-profile".to_string(),
    };
    insta::assert_snapshot!("domain_error_parent_not_found", format!("{}", err));
}

// --- MigrationError Display (constructible variants) ---

#[test]
fn snapshot_migration_error_unknown_version() {
    let err = MigrationError::UnknownSchemaVersion("wheel.profile/99".to_string());
    insta::assert_snapshot!("migration_error_unknown_version", format!("{}", err));
}

#[test]
fn snapshot_migration_error_migration_failed() {
    let err = MigrationError::MigrationFailed {
        from: "wheel.profile/1".to_string(),
        to: "wheel.profile/2".to_string(),
        reason: "incompatible filter format".to_string(),
    };
    insta::assert_snapshot!("migration_error_migration_failed", format!("{}", err));
}

#[test]
fn snapshot_migration_error_backup_failed() {
    let err = MigrationError::BackupFailed("disk full".to_string());
    insta::assert_snapshot!("migration_error_backup_failed", format!("{}", err));
}

#[test]
fn snapshot_migration_error_restore_failed() {
    let err = MigrationError::RestoreFailed("backup corrupt".to_string());
    insta::assert_snapshot!("migration_error_restore_failed", format!("{}", err));
}

#[test]
fn snapshot_migration_error_schema_version_not_found() {
    insta::assert_snapshot!(
        "migration_error_version_not_found",
        format!("{}", MigrationError::SchemaVersionNotFound)
    );
}

#[test]
fn snapshot_migration_error_validation_failed() {
    let err = MigrationError::ValidationFailed("torque_cap out of range".to_string());
    insta::assert_snapshot!("migration_error_validation_failed", format!("{}", err));
}

#[test]
fn snapshot_migration_error_no_path() {
    let err = MigrationError::NoMigrationPath {
        from: "wheel.profile/1".to_string(),
        to: "wheel.profile/5".to_string(),
    };
    insta::assert_snapshot!("migration_error_no_path", format!("{}", err));
}

// --- ConversionError Display ---

#[test]
fn snapshot_conversion_error_invalid_device_type() {
    let err = ConversionError::InvalidDeviceType(99);
    insta::assert_snapshot!("conversion_error_invalid_device_type", format!("{}", err));
}

#[test]
fn snapshot_conversion_error_invalid_device_state() {
    let err = ConversionError::InvalidDeviceState(-1);
    insta::assert_snapshot!("conversion_error_invalid_device_state", format!("{}", err));
}

#[test]
fn snapshot_conversion_error_missing_field() {
    let err = ConversionError::MissingField("device_id".to_string());
    insta::assert_snapshot!("conversion_error_missing_field", format!("{}", err));
}

#[test]
fn snapshot_conversion_error_unit_conversion() {
    let err = ConversionError::UnitConversion("cannot convert cNm to Nm: overflow".to_string());
    insta::assert_snapshot!("conversion_error_unit_conversion", format!("{}", err));
}

#[test]
fn snapshot_conversion_error_range_validation() {
    let err = ConversionError::RangeValidation {
        field: "torque".to_string(),
        value: 75.0,
        min: 0.0,
        max: 50.0,
    };
    insta::assert_snapshot!("conversion_error_range_validation", format!("{}", err));
}

// --- Domain Value Objects Display ---

#[test]
fn snapshot_torque_nm_display() -> Result<(), DomainError> {
    let torque = TorqueNm::new(12.5)?;
    insta::assert_snapshot!("torque_nm_display", format!("{}", torque));
    Ok(())
}

#[test]
fn snapshot_torque_nm_zero_display() {
    insta::assert_snapshot!("torque_nm_zero_display", format!("{}", TorqueNm::ZERO));
}

#[test]
fn snapshot_degrees_display() -> Result<(), DomainError> {
    let deg = Degrees::new_dor(900.0)?;
    insta::assert_snapshot!("degrees_dor_display", format!("{}", deg));
    Ok(())
}

#[test]
fn snapshot_degrees_angle_display() -> Result<(), DomainError> {
    let deg = Degrees::new_angle(45.0)?;
    insta::assert_snapshot!("degrees_angle_display", format!("{}", deg));
    Ok(())
}

#[test]
fn snapshot_device_id_display() -> Result<(), DomainError> {
    let id: DeviceId = "Moza-R9".parse()?;
    insta::assert_snapshot!("device_id_display", format!("{}", id));
    Ok(())
}

#[test]
fn snapshot_device_id_normalization() -> Result<(), DomainError> {
    let id: DeviceId = "  SimuCube-2  ".parse()?;
    insta::assert_snapshot!("device_id_normalized", format!("{}", id));
    Ok(())
}

// --- Domain Value Objects Debug ---

#[test]
fn snapshot_torque_nm_debug() -> Result<(), DomainError> {
    let torque = TorqueNm::new(15.0)?;
    insta::assert_debug_snapshot!("torque_nm_debug", torque);
    Ok(())
}

#[test]
fn snapshot_degrees_debug() -> Result<(), DomainError> {
    let deg = Degrees::new_dor(540.0)?;
    insta::assert_debug_snapshot!("degrees_dor_debug", deg);
    Ok(())
}

#[test]
fn snapshot_gain_debug() -> Result<(), DomainError> {
    let gain = Gain::new(0.75)?;
    insta::assert_debug_snapshot!("gain_debug", gain);
    Ok(())
}

#[test]
fn snapshot_device_id_debug() -> Result<(), DomainError> {
    let id = DeviceId::new("fanatec-dd1".to_string())?;
    insta::assert_debug_snapshot!("device_id_debug", id);
    Ok(())
}

// --- Config types JSON serialization ---

#[test]
fn snapshot_bumpstop_config_default_json() {
    insta::assert_json_snapshot!("bumpstop_config_default", BumpstopConfig::default());
}

#[test]
fn snapshot_hands_off_config_default_json() {
    insta::assert_json_snapshot!("hands_off_config_default", HandsOffConfig::default());
}

#[test]
fn snapshot_filter_config_default_json() {
    insta::assert_json_snapshot!("filter_config_default", FilterConfig::default());
}

#[test]
fn snapshot_base_settings_default_json() {
    insta::assert_json_snapshot!("base_settings_default", BaseSettings::default());
}

// --- Config types Debug ---

#[test]
fn snapshot_bumpstop_config_default_debug() {
    insta::assert_debug_snapshot!("bumpstop_config_default_debug", BumpstopConfig::default());
}

#[test]
fn snapshot_hands_off_config_default_debug() {
    insta::assert_debug_snapshot!("hands_off_config_default_debug", HandsOffConfig::default());
}

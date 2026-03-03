//! Snapshot tests for error message formatting.
//!
//! These tests verify that error messages are formatted consistently
//! and remain stable across changes. Covers all error variants, source
//! chains, category classifications, and severity levels.

use openracing_errors::{
    common::{ErrorCategory, ErrorContext, ErrorSeverity, OpenRacingError},
    device::DeviceError,
    profile::ProfileError,
    rt::RTError,
    validation::ValidationError,
};

// ---------------------------------------------------------------------------
// RT error Display snapshots — every variant
// ---------------------------------------------------------------------------
mod rt_error_snapshots {
    use super::*;
    use insta::assert_snapshot;

    #[test]
    fn test_device_disconnected() {
        assert_snapshot!(RTError::DeviceDisconnected.to_string());
    }

    #[test]
    fn test_torque_limit() {
        assert_snapshot!(RTError::TorqueLimit.to_string());
    }

    #[test]
    fn test_pipeline_fault() {
        assert_snapshot!(RTError::PipelineFault.to_string());
    }

    #[test]
    fn test_timing_violation() {
        assert_snapshot!(RTError::TimingViolation.to_string());
    }

    #[test]
    fn test_rt_setup_failed() {
        assert_snapshot!(RTError::RTSetupFailed.to_string());
    }

    #[test]
    fn test_safety_interlock() {
        assert_snapshot!(RTError::SafetyInterlock.to_string());
    }

    #[test]
    fn test_invalid_config() {
        assert_snapshot!(RTError::InvalidConfig.to_string());
    }

    #[test]
    fn test_buffer_overflow() {
        assert_snapshot!(RTError::BufferOverflow.to_string());
    }

    #[test]
    fn test_deadline_missed() {
        assert_snapshot!(RTError::DeadlineMissed.to_string());
    }

    #[test]
    fn test_resource_unavailable() {
        assert_snapshot!(RTError::ResourceUnavailable.to_string());
    }
}

// ---------------------------------------------------------------------------
// Device error Display snapshots — every variant
// ---------------------------------------------------------------------------
mod device_error_snapshots {
    use super::*;
    use insta::assert_snapshot;

    #[test]
    fn test_not_found() {
        assert_snapshot!(DeviceError::not_found("moza-r9").to_string());
    }

    #[test]
    fn test_disconnected() {
        assert_snapshot!(DeviceError::disconnected("fanatec-csl-elite").to_string());
    }

    #[test]
    fn test_timeout() {
        assert_snapshot!(DeviceError::timeout("logitech-g27", 5000).to_string());
    }

    #[test]
    fn test_unsupported_device() {
        assert_snapshot!(DeviceError::unsupported(0x1234, 0x5678).to_string());
    }

    #[test]
    fn test_communication_error() {
        assert_snapshot!(
            DeviceError::CommunicationError {
                device: "moza-r9".into(),
                message: "failed to read HID report".into(),
            }
            .to_string()
        );
    }

    #[test]
    fn test_connection_failed() {
        assert_snapshot!(DeviceError::ConnectionFailed("usb-hub-3".into()).to_string());
    }

    #[test]
    fn test_hid_error() {
        assert_snapshot!(DeviceError::HidError("descriptor parse failed".into()).to_string());
    }

    #[test]
    fn test_invalid_response() {
        assert_snapshot!(
            DeviceError::InvalidResponse {
                device: "moza-r9".into(),
                expected: 64,
                actual: 32,
            }
            .to_string()
        );
    }

    #[test]
    fn test_busy() {
        assert_snapshot!(DeviceError::Busy("fanatec-dd-pro".into()).to_string());
    }

    #[test]
    fn test_permission_denied() {
        assert_snapshot!(DeviceError::PermissionDenied("/dev/hidraw0".into()).to_string());
    }

    #[test]
    fn test_initialization_failed() {
        assert_snapshot!(
            DeviceError::InitializationFailed {
                device: "moza-r9".into(),
                reason: "firmware handshake timeout".into(),
            }
            .to_string()
        );
    }

    #[test]
    fn test_firmware_error() {
        assert_snapshot!(
            DeviceError::FirmwareError {
                device: "fanatec-csl-elite".into(),
                message: "CRC mismatch in firmware blob".into(),
            }
            .to_string()
        );
    }

    #[test]
    fn test_feature_not_supported() {
        assert_snapshot!(
            DeviceError::FeatureNotSupported {
                device: "logitech-g27".into(),
                feature: "direct-drive".into(),
            }
            .to_string()
        );
    }
}

// ---------------------------------------------------------------------------
// Profile error Display snapshots — every variant
// ---------------------------------------------------------------------------
mod profile_error_snapshots {
    use super::*;
    use insta::assert_snapshot;

    #[test]
    fn test_not_found() {
        assert_snapshot!(ProfileError::not_found("default-profile").to_string());
    }

    #[test]
    fn test_circular_inheritance() {
        assert_snapshot!(
            ProfileError::circular_inheritance("sport -> expert -> sport").to_string()
        );
    }

    #[test]
    fn test_version_mismatch() {
        assert_snapshot!(ProfileError::version_mismatch("2.0.0", "1.0.0").to_string());
    }

    #[test]
    fn test_inheritance_depth_exceeded() {
        assert_snapshot!(
            ProfileError::InheritanceDepthExceeded {
                depth: 10,
                max_depth: 5,
            }
            .to_string()
        );
    }

    #[test]
    fn test_missing_field() {
        assert_snapshot!(
            ProfileError::MissingField {
                profile: "my-profile".into(),
                field: "torque_limit".into(),
            }
            .to_string()
        );
    }

    #[test]
    fn test_already_exists() {
        assert_snapshot!(ProfileError::AlreadyExists("drift-pro".into()).to_string());
    }

    #[test]
    fn test_invalid_format() {
        assert_snapshot!(
            ProfileError::invalid_format("profiles/gt3.yaml", "missing name field").to_string()
        );
    }

    #[test]
    fn test_validation_failed() {
        assert_snapshot!(
            ProfileError::ValidationFailed("torque_limit must be > 0".into()).to_string()
        );
    }

    #[test]
    fn test_save_failed() {
        assert_snapshot!(
            ProfileError::SaveFailed {
                profile: "my-profile".into(),
                reason: "disk full".into(),
            }
            .to_string()
        );
    }

    #[test]
    fn test_load_failed() {
        assert_snapshot!(
            ProfileError::LoadFailed {
                path: "/data/profiles/gt3.yaml".into(),
                reason: "permission denied".into(),
            }
            .to_string()
        );
    }

    #[test]
    fn test_parent_not_found() {
        assert_snapshot!(
            ProfileError::ParentNotFound {
                parent_id: "base-gt3".into(),
            }
            .to_string()
        );
    }

    #[test]
    fn test_invalid_id() {
        assert_snapshot!(ProfileError::InvalidId("has spaces!!".into()).to_string());
    }

    #[test]
    fn test_conflict() {
        assert_snapshot!(ProfileError::Conflict("concurrent edit detected".into()).to_string());
    }

    #[test]
    fn test_locked() {
        assert_snapshot!(ProfileError::Locked("factory-default".into()).to_string());
    }

    #[test]
    fn test_invalid_device_mapping() {
        assert_snapshot!(
            ProfileError::InvalidDeviceMapping {
                profile: "my-profile".into(),
                device: "phantom-wheel".into(),
            }
            .to_string()
        );
    }
}

// ---------------------------------------------------------------------------
// Validation error Display snapshots — every variant
// ---------------------------------------------------------------------------
mod validation_error_snapshots {
    use super::*;
    use insta::assert_snapshot;

    #[test]
    fn test_required() {
        assert_snapshot!(ValidationError::required("profile_id").to_string());
    }

    #[test]
    fn test_out_of_range() {
        assert_snapshot!(
            ValidationError::out_of_range("torque", 1.5_f32, -1.0_f32, 1.0_f32).to_string()
        );
    }

    #[test]
    fn test_too_long() {
        assert_snapshot!(ValidationError::too_long("name", 150, 100).to_string());
    }

    #[test]
    fn test_too_short() {
        assert_snapshot!(ValidationError::too_short("password", 4, 8).to_string());
    }

    #[test]
    fn test_invalid_enum_value() {
        assert_snapshot!(
            ValidationError::invalid_enum("ffb_mode", "invalid", "pid, raw, telemetry").to_string()
        );
    }

    #[test]
    fn test_constraint_violation() {
        assert_snapshot!(ValidationError::constraint("gain must be positive").to_string());
    }

    #[test]
    fn test_invalid_format() {
        assert_snapshot!(
            ValidationError::invalid_format("email", "missing @ symbol").to_string()
        );
    }

    #[test]
    fn test_invalid_characters() {
        assert_snapshot!(
            ValidationError::InvalidCharacters {
                field: "profile_id".into(),
                reason: "contains whitespace".into(),
            }
            .to_string()
        );
    }

    #[test]
    fn test_not_unique() {
        assert_snapshot!(
            ValidationError::NotUnique {
                field: "profile_name".into(),
                value: "drift-pro".into(),
            }
            .to_string()
        );
    }

    #[test]
    fn test_dependency_not_met() {
        assert_snapshot!(
            ValidationError::DependencyNotMet {
                field: "damping".into(),
                dependency: "spring_rate".into(),
            }
            .to_string()
        );
    }

    #[test]
    fn test_invalid_type() {
        assert_snapshot!(
            ValidationError::InvalidType {
                field: "gain".into(),
                expected: "f32".into(),
                actual: "string".into(),
            }
            .to_string()
        );
    }

    #[test]
    fn test_numeric_overflow() {
        assert_snapshot!(
            ValidationError::NumericOverflow {
                field: "torque_nm".into(),
            }
            .to_string()
        );
    }

    #[test]
    fn test_custom() {
        assert_snapshot!(ValidationError::custom("profile name cannot start with a dot").to_string());
    }
}

// ---------------------------------------------------------------------------
// OpenRacingError wrapper Display snapshots — every variant
// ---------------------------------------------------------------------------
mod openracing_error_snapshots {
    use super::*;
    use insta::assert_snapshot;

    #[test]
    fn test_wrapped_rt_error() {
        let err: OpenRacingError = RTError::DeviceDisconnected.into();
        assert_snapshot!(err.to_string());
    }

    #[test]
    fn test_wrapped_device_error() {
        let err: OpenRacingError = DeviceError::not_found("test-device").into();
        assert_snapshot!(err.to_string());
    }

    #[test]
    fn test_wrapped_profile_error() {
        let err: OpenRacingError = ProfileError::not_found("test-profile").into();
        assert_snapshot!(err.to_string());
    }

    #[test]
    fn test_wrapped_validation_error() {
        let err: OpenRacingError = ValidationError::required("test-field").into();
        assert_snapshot!(err.to_string());
    }

    #[test]
    fn test_config_error() {
        assert_snapshot!(
            OpenRacingError::config("missing required field: torque_limit").to_string()
        );
    }

    #[test]
    fn test_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err: OpenRacingError = io_err.into();
        assert_snapshot!(err.to_string());
    }

    #[test]
    fn test_other_error() {
        assert_snapshot!(OpenRacingError::other("unexpected internal state").to_string());
    }
}

// ---------------------------------------------------------------------------
// Error context snapshots
// ---------------------------------------------------------------------------
mod error_context_snapshots {
    use super::*;
    use insta::assert_snapshot;

    #[test]
    fn test_simple_context() {
        let ctx = ErrorContext::new("load_profile");
        assert_snapshot!(ctx.to_string());
    }

    #[test]
    fn test_context_with_kv() {
        let ctx = ErrorContext::new("load_profile")
            .with("profile_id", "test-123")
            .with("device", "moza-r9");
        assert_snapshot!(ctx.to_string());
    }

    #[test]
    fn test_context_with_location() {
        let ctx = ErrorContext::new("apply_config")
            .with("setting", "torque_limit")
            .at("profile_service.rs", 142);
        assert_snapshot!(ctx.to_string());
    }
}

// ---------------------------------------------------------------------------
// Severity and category Display snapshots
// ---------------------------------------------------------------------------
mod severity_and_category_snapshots {
    use super::*;
    use insta::assert_snapshot;

    #[test]
    fn test_all_severities() {
        let severities = format!(
            "{} {} {} {}",
            ErrorSeverity::Info,
            ErrorSeverity::Warning,
            ErrorSeverity::Error,
            ErrorSeverity::Critical
        );
        assert_snapshot!(severities);
    }

    #[test]
    fn test_all_categories() {
        let categories = format!(
            "{} {} {} {} {} {} {} {} {}",
            ErrorCategory::RT,
            ErrorCategory::Device,
            ErrorCategory::Profile,
            ErrorCategory::Config,
            ErrorCategory::IO,
            ErrorCategory::Validation,
            ErrorCategory::Plugin,
            ErrorCategory::Telemetry,
            ErrorCategory::Other
        );
        assert_snapshot!(categories);
    }
}

// ---------------------------------------------------------------------------
// Error source chain snapshots — verifies std::error::Error::source()
// ---------------------------------------------------------------------------
mod source_chain_snapshots {
    use super::*;
    use insta::assert_snapshot;

    /// Walk the error source chain and format it as a multi-line string.
    fn format_source_chain(err: &dyn std::error::Error) -> String {
        let mut chain = vec![err.to_string()];
        let mut current = err.source();
        while let Some(src) = current {
            chain.push(format!("  caused by: {}", src));
            current = src.source();
        }
        chain.join("\n")
    }

    #[test]
    fn test_rt_error_source_chain() {
        let err: OpenRacingError = RTError::TimingViolation.into();
        assert_snapshot!(format_source_chain(&err));
    }

    #[test]
    fn test_device_error_source_chain() {
        let err: OpenRacingError = DeviceError::not_found("moza-r9").into();
        assert_snapshot!(format_source_chain(&err));
    }

    #[test]
    fn test_profile_error_source_chain() {
        let err: OpenRacingError =
            ProfileError::circular_inheritance("a -> b -> a").into();
        assert_snapshot!(format_source_chain(&err));
    }

    #[test]
    fn test_validation_error_source_chain() {
        let err: OpenRacingError = ValidationError::required("field").into();
        assert_snapshot!(format_source_chain(&err));
    }

    #[test]
    fn test_io_error_source_chain() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
        let err: OpenRacingError = io_err.into();
        assert_snapshot!(format_source_chain(&err));
    }
}

// ---------------------------------------------------------------------------
// Error category classification snapshots — every OpenRacingError variant
// ---------------------------------------------------------------------------
mod category_classification_snapshots {
    use super::*;
    use insta::assert_snapshot;

    #[test]
    fn test_all_variant_categories() {
        let pairs: Vec<(&str, ErrorCategory)> = vec![
            ("RT", OpenRacingError::RT(RTError::PipelineFault).category()),
            (
                "Device",
                OpenRacingError::Device(DeviceError::not_found("x")).category(),
            ),
            (
                "Profile",
                OpenRacingError::Profile(ProfileError::not_found("x")).category(),
            ),
            (
                "Validation",
                OpenRacingError::Validation(ValidationError::required("x")).category(),
            ),
            (
                "Io",
                OpenRacingError::Io(std::io::Error::other("test"))
                .category(),
            ),
            ("Config", OpenRacingError::config("x").category()),
            ("Other", OpenRacingError::other("x").category()),
        ];
        let formatted: Vec<String> = pairs
            .iter()
            .map(|(label, cat)| format!("{label}: {cat}"))
            .collect();
        assert_snapshot!(formatted.join("\n"));
    }
}

// ---------------------------------------------------------------------------
// Error severity level snapshots — every RT and Device variant
// ---------------------------------------------------------------------------
mod severity_level_snapshots {
    use super::*;
    use insta::assert_snapshot;

    #[test]
    fn test_rt_variant_severities() {
        let variants = [
            RTError::DeviceDisconnected,
            RTError::TorqueLimit,
            RTError::PipelineFault,
            RTError::TimingViolation,
            RTError::RTSetupFailed,
            RTError::InvalidConfig,
            RTError::SafetyInterlock,
            RTError::BufferOverflow,
            RTError::DeadlineMissed,
            RTError::ResourceUnavailable,
        ];
        let formatted: Vec<String> = variants
            .iter()
            .map(|v| format!("{v}: {}", v.severity()))
            .collect();
        assert_snapshot!(formatted.join("\n"));
    }

    #[test]
    fn test_device_variant_severities() {
        let variants: Vec<(&str, ErrorSeverity)> = vec![
            ("NotFound", DeviceError::not_found("x").severity()),
            ("Disconnected", DeviceError::disconnected("x").severity()),
            (
                "ConnectionFailed",
                DeviceError::ConnectionFailed("x".into()).severity(),
            ),
            (
                "CommunicationError",
                DeviceError::CommunicationError {
                    device: "x".into(),
                    message: "m".into(),
                }
                .severity(),
            ),
            ("HidError", DeviceError::HidError("x".into()).severity()),
            (
                "InvalidResponse",
                DeviceError::InvalidResponse {
                    device: "x".into(),
                    expected: 64,
                    actual: 32,
                }
                .severity(),
            ),
            ("Timeout", DeviceError::timeout("x", 100).severity()),
            (
                "UnsupportedDevice",
                DeviceError::unsupported(0, 0).severity(),
            ),
            ("Busy", DeviceError::Busy("x".into()).severity()),
            (
                "PermissionDenied",
                DeviceError::PermissionDenied("x".into()).severity(),
            ),
            (
                "InitializationFailed",
                DeviceError::InitializationFailed {
                    device: "x".into(),
                    reason: "r".into(),
                }
                .severity(),
            ),
            (
                "FirmwareError",
                DeviceError::FirmwareError {
                    device: "x".into(),
                    message: "m".into(),
                }
                .severity(),
            ),
            (
                "FeatureNotSupported",
                DeviceError::FeatureNotSupported {
                    device: "x".into(),
                    feature: "f".into(),
                }
                .severity(),
            ),
        ];
        let formatted: Vec<String> = variants
            .iter()
            .map(|(label, sev)| format!("{label}: {sev}"))
            .collect();
        assert_snapshot!(formatted.join("\n"));
    }

    #[test]
    fn test_profile_variant_severities() {
        let variants: Vec<(&str, ErrorSeverity)> = vec![
            ("NotFound", ProfileError::not_found("x").severity()),
            (
                "AlreadyExists",
                ProfileError::AlreadyExists("x".into()).severity(),
            ),
            (
                "InvalidFormat",
                ProfileError::invalid_format("p", "r").severity(),
            ),
            (
                "ValidationFailed",
                ProfileError::ValidationFailed("x".into()).severity(),
            ),
            (
                "SaveFailed",
                ProfileError::SaveFailed {
                    profile: "x".into(),
                    reason: "r".into(),
                }
                .severity(),
            ),
            (
                "LoadFailed",
                ProfileError::LoadFailed {
                    path: "p".into(),
                    reason: "r".into(),
                }
                .severity(),
            ),
            (
                "CircularInheritance",
                ProfileError::circular_inheritance("a -> b").severity(),
            ),
            (
                "InheritanceDepthExceeded",
                ProfileError::InheritanceDepthExceeded {
                    depth: 10,
                    max_depth: 5,
                }
                .severity(),
            ),
            (
                "ParentNotFound",
                ProfileError::ParentNotFound {
                    parent_id: "x".into(),
                }
                .severity(),
            ),
            ("InvalidId", ProfileError::InvalidId("x".into()).severity()),
            ("Conflict", ProfileError::Conflict("x".into()).severity()),
            (
                "VersionMismatch",
                ProfileError::version_mismatch("2", "1").severity(),
            ),
            (
                "MissingField",
                ProfileError::MissingField {
                    profile: "p".into(),
                    field: "f".into(),
                }
                .severity(),
            ),
            ("Locked", ProfileError::Locked("x".into()).severity()),
            (
                "InvalidDeviceMapping",
                ProfileError::InvalidDeviceMapping {
                    profile: "p".into(),
                    device: "d".into(),
                }
                .severity(),
            ),
        ];
        let formatted: Vec<String> = variants
            .iter()
            .map(|(label, sev)| format!("{label}: {sev}"))
            .collect();
        assert_snapshot!(formatted.join("\n"));
    }

    #[test]
    fn test_validation_severity_is_uniform() {
        // All validation errors have the same severity; snapshot for lockdown.
        let samples = [
            ValidationError::required("x").severity(),
            ValidationError::out_of_range("x", 0_i32, 0_i32, 1_i32).severity(),
            ValidationError::too_long("x", 10, 5).severity(),
            ValidationError::too_short("x", 1, 5).severity(),
            ValidationError::invalid_enum("x", "v", "a, b").severity(),
            ValidationError::constraint("c").severity(),
            ValidationError::invalid_format("x", "r").severity(),
            ValidationError::custom("c").severity(),
        ];
        let all_same = samples.iter().all(|s| *s == ErrorSeverity::Error);
        assert_snapshot!(format!("all_validation_severity_error: {all_same}"));
    }
}

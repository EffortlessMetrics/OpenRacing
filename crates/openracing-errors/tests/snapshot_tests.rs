//! Snapshot tests for error message formatting.
//!
//! These tests verify that error messages are formatted consistently
//! and remain stable across changes.

use openracing_errors::{
    common::{ErrorCategory, ErrorContext, ErrorSeverity, OpenRacingError},
    device::DeviceError,
    profile::ProfileError,
    rt::RTError,
    validation::ValidationError,
};

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
}

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
}

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
}

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
}

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
}

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

//! Unit tests for all error variants.
//!
//! Tests Display implementations, std::error::Error implementations,
//! and From conversions.

use openracing_errors::{
    Result,
    common::{ErrorCategory, ErrorContext, ErrorSeverity, OpenRacingError, ResultExt},
    device::DeviceError,
    profile::ProfileError,
    rt::RTError,
    validation::ValidationError,
};

mod rt_error_tests {
    use super::*;

    #[test]
    fn test_all_variants_display() -> Result<()> {
        let variants: Vec<RTError> = vec![
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

        for variant in variants {
            let msg = variant.to_string();
            assert!(
                !msg.is_empty(),
                "RTError variant should have display message"
            );
        }
        Ok(())
    }

    #[test]
    fn test_std_error_impl() -> Result<()> {
        let err = RTError::DeviceDisconnected;
        let _: &dyn std::error::Error = &err;
        Ok(())
    }

    #[test]
    fn test_error_codes() -> Result<()> {
        assert_eq!(RTError::DeviceDisconnected.code(), 1);
        assert_eq!(RTError::TorqueLimit.code(), 2);
        assert_eq!(RTError::from_code(1), Some(RTError::DeviceDisconnected));
        assert_eq!(RTError::from_code(255), None);
        Ok(())
    }

    #[test]
    fn test_severity_classification() -> Result<()> {
        assert_eq!(
            RTError::DeviceDisconnected.severity(),
            ErrorSeverity::Critical
        );
        assert_eq!(RTError::TimingViolation.severity(), ErrorSeverity::Warning);
        assert!(RTError::TorqueLimit.requires_safety_action());
        assert!(!RTError::InvalidConfig.requires_safety_action());
        Ok(())
    }
}

mod device_error_tests {
    use super::*;

    #[test]
    fn test_all_variants_display() -> Result<()> {
        let err = DeviceError::not_found("moza-r9");
        assert!(err.to_string().contains("moza-r9"));

        let err = DeviceError::timeout("moza-r9", 1000);
        assert!(err.to_string().contains("1000"));

        let err = DeviceError::unsupported(0x1234, 0x5678);
        assert!(err.to_string().contains("1234"));

        Ok(())
    }

    #[test]
    fn test_std_error_impl() -> Result<()> {
        let err = DeviceError::not_found("test");
        let _: &dyn std::error::Error = &err;
        Ok(())
    }

    #[test]
    fn test_retryable() -> Result<()> {
        assert!(DeviceError::timeout("test", 100).is_retryable());
        assert!(DeviceError::Busy("test".into()).is_retryable());
        assert!(!DeviceError::not_found("test").is_retryable());
        Ok(())
    }
}

mod profile_error_tests {
    use super::*;

    #[test]
    fn test_all_variants_display() -> Result<()> {
        let err = ProfileError::not_found("my-profile");
        assert!(err.to_string().contains("my-profile"));

        let err = ProfileError::version_mismatch("2.0", "1.0");
        assert!(err.to_string().contains("2.0"));

        let err = ProfileError::circular_inheritance("a -> b -> a");
        assert!(err.to_string().contains("a -> b -> a"));

        Ok(())
    }

    #[test]
    fn test_std_error_impl() -> Result<()> {
        let err = ProfileError::not_found("test");
        let _: &dyn std::error::Error = &err;
        Ok(())
    }

    #[test]
    fn test_inheritance_errors() -> Result<()> {
        assert!(ProfileError::circular_inheritance("a->b->a").is_inheritance_error());
        assert!(
            ProfileError::ParentNotFound {
                parent_id: "p".into()
            }
            .is_inheritance_error()
        );
        assert!(!ProfileError::not_found("test").is_inheritance_error());
        Ok(())
    }
}

mod validation_error_tests {
    use super::*;

    #[test]
    fn test_all_variants_display() -> Result<()> {
        let err = ValidationError::out_of_range("torque", 1.5_f32, -1.0_f32, 1.0_f32);
        assert!(err.to_string().contains("torque"));

        let err = ValidationError::required("profile_id");
        assert!(err.to_string().contains("profile_id"));

        let err = ValidationError::too_long("name", 100, 50);
        assert!(err.to_string().contains("100"));

        Ok(())
    }

    #[test]
    fn test_std_error_impl() -> Result<()> {
        let err = ValidationError::required("test");
        let _: &dyn std::error::Error = &err;
        Ok(())
    }

    #[test]
    fn test_equality() -> Result<()> {
        let err1 = ValidationError::required("field");
        let err2 = ValidationError::required("field");
        assert_eq!(err1, err2);
        Ok(())
    }
}

mod openracing_error_tests {
    use super::*;

    #[test]
    fn test_from_implementations() -> Result<()> {
        let rt_err: OpenRacingError = RTError::DeviceDisconnected.into();
        assert_eq!(rt_err.category(), ErrorCategory::RT);

        let device_err: OpenRacingError = DeviceError::not_found("test").into();
        assert_eq!(device_err.category(), ErrorCategory::Device);

        let profile_err: OpenRacingError = ProfileError::not_found("test").into();
        assert_eq!(profile_err.category(), ErrorCategory::Profile);

        let validation_err: OpenRacingError = ValidationError::required("test").into();
        assert_eq!(validation_err.category(), ErrorCategory::Validation);

        Ok(())
    }

    #[test]
    fn test_config_and_other() -> Result<()> {
        let err = OpenRacingError::config("missing file");
        assert_eq!(err.category(), ErrorCategory::Config);

        let err = OpenRacingError::other("something went wrong");
        assert_eq!(err.category(), ErrorCategory::Other);

        Ok(())
    }

    #[test]
    fn test_is_recoverable() -> Result<()> {
        let critical_err: OpenRacingError = RTError::DeviceDisconnected.into();
        assert!(!critical_err.is_recoverable());

        let warning_err: OpenRacingError = RTError::TimingViolation.into();
        assert!(warning_err.is_recoverable());

        Ok(())
    }
}

mod error_context_tests {
    use super::*;

    #[test]
    fn test_context_building() -> Result<()> {
        let ctx = ErrorContext::new("load_profile")
            .with("profile_id", "test-123")
            .with("device", "moza-r9")
            .at("main.rs", 42);

        let msg = ctx.to_string();
        assert!(msg.contains("load_profile"));
        assert!(msg.contains("profile_id"));
        assert!(msg.contains("main.rs:42"));

        Ok(())
    }
}

mod result_ext_tests {
    use super::*;

    #[test]
    fn test_result_ext_context() -> Result<()> {
        let result: std::result::Result<(), RTError> = Err(RTError::DeviceDisconnected);
        let result = result.with_context("test_operation");

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("test_operation"));

        Ok(())
    }
}

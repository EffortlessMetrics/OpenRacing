//! Tests for error chaining, From conversions, Display/Debug output,
//! Error::source() traversal, categorization, severity, and recoverability.

use std::error::Error;

use openracing_errors::{
    DeviceError, OpenRacingError, ProfileError, RTError, ValidationError,
    common::{ErrorCategory, ErrorContext, ErrorSeverity, ResultExt},
};
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// Error chain: create → wrap → add context → extract original
// ---------------------------------------------------------------------------

mod error_chain {
    use super::*;

    #[test]
    fn wrap_rt_error_and_add_context() -> Result<(), OpenRacingError> {
        let original = RTError::TimingViolation;
        let wrapped: OpenRacingError = original.into();
        let result: Result<(), OpenRacingError> = Err(wrapped);

        let ctx = ErrorContext::new("process_frame")
            .with("tick", "42")
            .with("device", "moza-r9");
        let err = result.context(ctx).unwrap_err();

        assert!(err.to_string().contains("process_frame"));
        assert!(err.to_string().contains("tick"));
        assert!(err.to_string().contains("42"));
        assert!(err.to_string().contains("Real-time timing violation"));
        Ok(())
    }

    #[test]
    fn wrap_device_error_with_context_preserves_message() -> Result<(), OpenRacingError> {
        let original = DeviceError::not_found("wheel-1");
        let wrapped: OpenRacingError = original.into();
        let result: Result<(), OpenRacingError> = Err(wrapped);
        let err = result.with_context("enumerate_devices").unwrap_err();

        assert!(err.to_string().contains("enumerate_devices"));
        assert!(err.to_string().contains("wheel-1"));
        Ok(())
    }

    #[test]
    fn wrap_validation_error_with_context() -> Result<(), OpenRacingError> {
        let original = ValidationError::out_of_range("gain", 2.0_f32, 0.0_f32, 1.0_f32);
        let wrapped: OpenRacingError = original.into();
        let result: Result<(), OpenRacingError> = Err(wrapped);
        let err = result.with_context("apply_settings").unwrap_err();

        assert!(err.to_string().contains("apply_settings"));
        assert!(err.to_string().contains("gain"));
        Ok(())
    }

    #[test]
    fn wrap_profile_error_with_context() -> Result<(), OpenRacingError> {
        let original = ProfileError::not_found("gt3-preset");
        let wrapped: OpenRacingError = original.into();
        let result: Result<(), OpenRacingError> = Err(wrapped);
        let err = result.with_context("load_profile").unwrap_err();

        assert!(err.to_string().contains("load_profile"));
        assert!(err.to_string().contains("gt3-preset"));
        Ok(())
    }

    #[test]
    fn context_with_location_info() -> Result<(), OpenRacingError> {
        let ctx = ErrorContext::new("init")
            .with("stage", "bootstrap")
            .at("engine.rs", 99);
        let result: Result<(), OpenRacingError> = Err(OpenRacingError::config("bad value"));
        let err = result.context(ctx).unwrap_err();

        assert!(err.to_string().contains("engine.rs:99"));
        assert!(err.to_string().contains("stage"));
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// From conversions for all error types
// ---------------------------------------------------------------------------

mod from_conversions {
    use super::*;

    #[test]
    fn from_rt_error() -> Result<(), OpenRacingError> {
        let err: OpenRacingError = RTError::PipelineFault.into();
        assert!(matches!(err, OpenRacingError::RT(RTError::PipelineFault)));
        Ok(())
    }

    #[test]
    fn from_device_error() -> Result<(), OpenRacingError> {
        let err: OpenRacingError = DeviceError::not_found("dev").into();
        assert!(matches!(
            err,
            OpenRacingError::Device(DeviceError::NotFound(_))
        ));
        Ok(())
    }

    #[test]
    fn from_profile_error() -> Result<(), OpenRacingError> {
        let err: OpenRacingError = ProfileError::not_found("p1").into();
        assert!(matches!(
            err,
            OpenRacingError::Profile(ProfileError::NotFound(_))
        ));
        Ok(())
    }

    #[test]
    fn from_validation_error() -> Result<(), OpenRacingError> {
        let err: OpenRacingError = ValidationError::required("field").into();
        assert!(matches!(
            err,
            OpenRacingError::Validation(ValidationError::Required(_))
        ));
        Ok(())
    }

    #[test]
    fn from_io_error() -> Result<(), OpenRacingError> {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let err: OpenRacingError = io_err.into();
        assert!(matches!(err, OpenRacingError::Io(_)));
        Ok(())
    }

    #[test]
    fn question_mark_propagation_from_rt() -> Result<(), OpenRacingError> {
        fn inner() -> Result<(), RTError> {
            Err(RTError::BufferOverflow)
        }
        fn outer() -> Result<(), OpenRacingError> {
            inner()?;
            Ok(())
        }
        let err = outer().unwrap_err();
        assert!(matches!(err, OpenRacingError::RT(RTError::BufferOverflow)));
        Ok(())
    }

    #[test]
    fn question_mark_propagation_from_io() -> Result<(), OpenRacingError> {
        fn inner() -> Result<(), std::io::Error> {
            Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "broken",
            ))
        }
        fn outer() -> Result<(), OpenRacingError> {
            inner()?;
            Ok(())
        }
        let err = outer().unwrap_err();
        assert!(matches!(err, OpenRacingError::Io(_)));
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Display output for all error variants
// ---------------------------------------------------------------------------

mod display_output {
    use super::*;

    #[test]
    fn openracing_error_rt_display() -> Result<(), OpenRacingError> {
        let err: OpenRacingError = RTError::DeviceDisconnected.into();
        assert!(err.to_string().contains("Device disconnected"));
        Ok(())
    }

    #[test]
    fn openracing_error_device_display() -> Result<(), OpenRacingError> {
        let err: OpenRacingError = DeviceError::timeout("wheel", 500).into();
        assert!(err.to_string().contains("wheel"));
        assert!(err.to_string().contains("500"));
        Ok(())
    }

    #[test]
    fn openracing_error_profile_display() -> Result<(), OpenRacingError> {
        let err: OpenRacingError =
            ProfileError::invalid_format("config.yaml", "missing name").into();
        assert!(err.to_string().contains("config.yaml"));
        assert!(err.to_string().contains("missing name"));
        Ok(())
    }

    #[test]
    fn openracing_error_validation_display() -> Result<(), OpenRacingError> {
        let err: OpenRacingError = ValidationError::too_long("name", 200, 100).into();
        assert!(err.to_string().contains("200"));
        assert!(err.to_string().contains("100"));
        Ok(())
    }

    #[test]
    fn openracing_error_io_display() -> Result<(), OpenRacingError> {
        let io = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
        let err: OpenRacingError = io.into();
        assert!(err.to_string().contains("access denied"));
        Ok(())
    }

    #[test]
    fn openracing_error_config_display() -> Result<(), OpenRacingError> {
        let err = OpenRacingError::config("missing section");
        assert!(err.to_string().contains("missing section"));
        Ok(())
    }

    #[test]
    fn openracing_error_other_display() -> Result<(), OpenRacingError> {
        let err = OpenRacingError::other("unexpected state");
        assert!(err.to_string().contains("unexpected state"));
        Ok(())
    }

    #[test]
    fn all_rt_variants_display_nonempty() -> Result<(), OpenRacingError> {
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
        for v in &variants {
            assert!(
                !v.to_string().is_empty(),
                "RTError::{v:?} should have non-empty display"
            );
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Debug output for all error variants
// ---------------------------------------------------------------------------

mod debug_output {
    use super::*;

    #[test]
    fn rt_error_debug_contains_variant_name() -> Result<(), OpenRacingError> {
        let err = RTError::SafetyInterlock;
        let dbg = format!("{err:?}");
        assert!(dbg.contains("SafetyInterlock"));
        Ok(())
    }

    #[test]
    fn device_error_debug_contains_variant_name() -> Result<(), OpenRacingError> {
        let err = DeviceError::not_found("wheel");
        let dbg = format!("{err:?}");
        assert!(dbg.contains("NotFound"));
        Ok(())
    }

    #[test]
    fn profile_error_debug_contains_variant_name() -> Result<(), OpenRacingError> {
        let err = ProfileError::circular_inheritance("a -> b -> a");
        let dbg = format!("{err:?}");
        assert!(dbg.contains("CircularInheritance"));
        Ok(())
    }

    #[test]
    fn validation_error_debug_contains_variant_name() -> Result<(), OpenRacingError> {
        let err = ValidationError::constraint("must be positive");
        let dbg = format!("{err:?}");
        assert!(dbg.contains("ConstraintViolation"));
        Ok(())
    }

    #[test]
    fn openracing_error_debug_contains_wrapper() -> Result<(), OpenRacingError> {
        let err: OpenRacingError = RTError::DeadlineMissed.into();
        let dbg = format!("{err:?}");
        assert!(dbg.contains("RT"));
        assert!(dbg.contains("DeadlineMissed"));
        Ok(())
    }

    #[test]
    fn all_rt_variants_debug_nonempty() -> Result<(), OpenRacingError> {
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
        for v in &variants {
            assert!(
                !format!("{v:?}").is_empty(),
                "RTError::{v:?} should have non-empty debug"
            );
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Error::source() chain traversal
// ---------------------------------------------------------------------------

mod source_chain {
    use super::*;

    #[test]
    fn rt_error_has_no_source() -> Result<(), OpenRacingError> {
        let err = RTError::TimingViolation;
        let dyn_err: &dyn Error = &err;
        assert!(dyn_err.source().is_none());
        Ok(())
    }

    #[test]
    fn openracing_rt_has_rt_source() -> Result<(), OpenRacingError> {
        let err: OpenRacingError = RTError::PipelineFault.into();
        let dyn_err: &dyn Error = &err;
        let source = dyn_err.source();
        assert!(source.is_some());
        let source_msg = source.map(|s| s.to_string()).unwrap_or_default();
        assert!(source_msg.contains("Pipeline processing fault"));
        Ok(())
    }

    #[test]
    fn openracing_device_has_device_source() -> Result<(), OpenRacingError> {
        let err: OpenRacingError = DeviceError::disconnected("wheel").into();
        let dyn_err: &dyn Error = &err;
        let source = dyn_err.source();
        assert!(source.is_some());
        let source_msg = source.map(|s| s.to_string()).unwrap_or_default();
        assert!(source_msg.contains("wheel"));
        Ok(())
    }

    #[test]
    fn openracing_io_has_io_source() -> Result<(), OpenRacingError> {
        let io = std::io::Error::new(std::io::ErrorKind::NotFound, "not found");
        let err: OpenRacingError = io.into();
        let dyn_err: &dyn Error = &err;
        let source = dyn_err.source();
        assert!(source.is_some());
        Ok(())
    }

    #[test]
    fn config_error_has_no_source() -> Result<(), OpenRacingError> {
        let err = OpenRacingError::config("bad");
        let dyn_err: &dyn Error = &err;
        assert!(dyn_err.source().is_none());
        Ok(())
    }

    #[test]
    fn other_error_has_no_source() -> Result<(), OpenRacingError> {
        let err = OpenRacingError::other("misc");
        let dyn_err: &dyn Error = &err;
        assert!(dyn_err.source().is_none());
        Ok(())
    }

    #[test]
    fn source_chain_depth() -> Result<(), OpenRacingError> {
        let err: OpenRacingError = RTError::DeviceDisconnected.into();
        let dyn_err: &dyn Error = &err;

        // Walk the source chain and count depth
        let mut depth = 0u32;
        let mut current: Option<&dyn Error> = Some(dyn_err);
        while let Some(e) = current {
            depth += 1;
            current = e.source();
        }
        // OpenRacingError::RT(RTError) -> RTError -> None => depth >= 2
        assert!(
            depth >= 2,
            "source chain depth should be at least 2, got {depth}"
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Error categorization for all variants
// ---------------------------------------------------------------------------

mod categorization {
    use super::*;

    #[test]
    fn rt_errors_categorize_as_rt() -> Result<(), OpenRacingError> {
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
        for v in variants {
            let err: OpenRacingError = v.into();
            assert_eq!(
                err.category(),
                ErrorCategory::RT,
                "RTError::{v:?} should be RT"
            );
        }
        Ok(())
    }

    #[test]
    fn device_errors_categorize_as_device() -> Result<(), OpenRacingError> {
        let errors: Vec<OpenRacingError> = vec![
            DeviceError::not_found("x").into(),
            DeviceError::disconnected("x").into(),
            DeviceError::timeout("x", 100).into(),
            DeviceError::unsupported(0x1234, 0x5678).into(),
            DeviceError::Busy("x".into()).into(),
            DeviceError::PermissionDenied("x".into()).into(),
            DeviceError::HidError("x".into()).into(),
        ];
        for err in &errors {
            assert_eq!(err.category(), ErrorCategory::Device);
        }
        Ok(())
    }

    #[test]
    fn profile_errors_categorize_as_profile() -> Result<(), OpenRacingError> {
        let errors: Vec<OpenRacingError> = vec![
            ProfileError::not_found("p").into(),
            ProfileError::circular_inheritance("a -> b").into(),
            ProfileError::version_mismatch("2.0", "1.0").into(),
            ProfileError::Locked("p".into()).into(),
        ];
        for err in &errors {
            assert_eq!(err.category(), ErrorCategory::Profile);
        }
        Ok(())
    }

    #[test]
    fn validation_errors_categorize_as_validation() -> Result<(), OpenRacingError> {
        let errors: Vec<OpenRacingError> = vec![
            ValidationError::required("f").into(),
            ValidationError::out_of_range("f", 5, 0, 3).into(),
            ValidationError::invalid_format("f", "bad").into(),
            ValidationError::constraint("fail").into(),
            ValidationError::custom("custom").into(),
        ];
        for err in &errors {
            assert_eq!(err.category(), ErrorCategory::Validation);
        }
        Ok(())
    }

    #[test]
    fn io_error_categorizes_as_io() -> Result<(), OpenRacingError> {
        let io = std::io::Error::other("test");
        let err: OpenRacingError = io.into();
        assert_eq!(err.category(), ErrorCategory::IO);
        Ok(())
    }

    #[test]
    fn config_error_categorizes_as_config() -> Result<(), OpenRacingError> {
        let err = OpenRacingError::config("test");
        assert_eq!(err.category(), ErrorCategory::Config);
        Ok(())
    }

    #[test]
    fn other_error_categorizes_as_other() -> Result<(), OpenRacingError> {
        let err = OpenRacingError::other("test");
        assert_eq!(err.category(), ErrorCategory::Other);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Severity assignment for all variants
// ---------------------------------------------------------------------------

mod severity {
    use super::*;

    #[test]
    fn rt_critical_errors() -> Result<(), OpenRacingError> {
        let critical = [
            RTError::DeviceDisconnected,
            RTError::TorqueLimit,
            RTError::RTSetupFailed,
            RTError::SafetyInterlock,
            RTError::DeadlineMissed,
        ];
        for v in critical {
            let err: OpenRacingError = v.into();
            assert_eq!(
                err.severity(),
                ErrorSeverity::Critical,
                "RTError::{v:?} should be Critical"
            );
        }
        Ok(())
    }

    #[test]
    fn rt_warning_errors() -> Result<(), OpenRacingError> {
        let warnings = [RTError::TimingViolation, RTError::BufferOverflow];
        for v in warnings {
            let err: OpenRacingError = v.into();
            assert_eq!(
                err.severity(),
                ErrorSeverity::Warning,
                "RTError::{v:?} should be Warning"
            );
        }
        Ok(())
    }

    #[test]
    fn rt_error_severity_errors() -> Result<(), OpenRacingError> {
        let errors = [
            RTError::PipelineFault,
            RTError::InvalidConfig,
            RTError::ResourceUnavailable,
        ];
        for v in errors {
            let err: OpenRacingError = v.into();
            assert_eq!(
                err.severity(),
                ErrorSeverity::Error,
                "RTError::{v:?} should be Error"
            );
        }
        Ok(())
    }

    #[test]
    fn device_disconnected_is_critical() -> Result<(), OpenRacingError> {
        let err: OpenRacingError = DeviceError::disconnected("dev").into();
        assert_eq!(err.severity(), ErrorSeverity::Critical);
        Ok(())
    }

    #[test]
    fn device_timeout_is_warning() -> Result<(), OpenRacingError> {
        let err: OpenRacingError = DeviceError::timeout("dev", 100).into();
        assert_eq!(err.severity(), ErrorSeverity::Warning);
        Ok(())
    }

    #[test]
    fn device_feature_not_supported_is_info() -> Result<(), OpenRacingError> {
        let err: OpenRacingError = DeviceError::FeatureNotSupported {
            device: "wheel".into(),
            feature: "led".into(),
        }
        .into();
        assert_eq!(err.severity(), ErrorSeverity::Info);
        Ok(())
    }

    #[test]
    fn config_error_severity_is_error() -> Result<(), OpenRacingError> {
        let err = OpenRacingError::config("bad");
        assert_eq!(err.severity(), ErrorSeverity::Error);
        Ok(())
    }

    #[test]
    fn io_error_severity_is_error() -> Result<(), OpenRacingError> {
        let err: OpenRacingError = std::io::Error::other("fail").into();
        assert_eq!(err.severity(), ErrorSeverity::Error);
        Ok(())
    }

    #[test]
    fn severity_display_values() -> Result<(), OpenRacingError> {
        assert_eq!(ErrorSeverity::Info.to_string(), "INFO");
        assert_eq!(ErrorSeverity::Warning.to_string(), "WARN");
        assert_eq!(ErrorSeverity::Error.to_string(), "ERROR");
        assert_eq!(ErrorSeverity::Critical.to_string(), "CRITICAL");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Recoverability determination
// ---------------------------------------------------------------------------

mod recoverability {
    use super::*;

    #[test]
    fn critical_rt_errors_are_not_recoverable() -> Result<(), OpenRacingError> {
        let critical = [
            RTError::DeviceDisconnected,
            RTError::TorqueLimit,
            RTError::RTSetupFailed,
            RTError::SafetyInterlock,
            RTError::DeadlineMissed,
        ];
        for v in critical {
            let err: OpenRacingError = v.into();
            assert!(
                !err.is_recoverable(),
                "RTError::{v:?} should NOT be recoverable"
            );
        }
        Ok(())
    }

    #[test]
    fn warning_and_error_severity_are_recoverable() -> Result<(), OpenRacingError> {
        let recoverable = [
            RTError::TimingViolation,
            RTError::PipelineFault,
            RTError::InvalidConfig,
            RTError::BufferOverflow,
            RTError::ResourceUnavailable,
        ];
        for v in recoverable {
            let err: OpenRacingError = v.into();
            assert!(err.is_recoverable(), "RTError::{v:?} should be recoverable");
        }
        Ok(())
    }

    #[test]
    fn config_errors_are_recoverable() -> Result<(), OpenRacingError> {
        let err = OpenRacingError::config("bad");
        assert!(err.is_recoverable());
        Ok(())
    }

    #[test]
    fn other_errors_are_recoverable() -> Result<(), OpenRacingError> {
        let err = OpenRacingError::other("misc");
        assert!(err.is_recoverable());
        Ok(())
    }

    #[test]
    fn device_disconnected_not_recoverable() -> Result<(), OpenRacingError> {
        let err: OpenRacingError = DeviceError::disconnected("dev").into();
        assert!(!err.is_recoverable());
        Ok(())
    }

    #[test]
    fn device_timeout_is_recoverable() -> Result<(), OpenRacingError> {
        let err: OpenRacingError = DeviceError::timeout("dev", 100).into();
        assert!(err.is_recoverable());
        Ok(())
    }

    #[test]
    fn rt_error_is_recoverable_method() -> Result<(), OpenRacingError> {
        assert!(RTError::TimingViolation.is_recoverable());
        assert!(RTError::BufferOverflow.is_recoverable());
        assert!(RTError::ResourceUnavailable.is_recoverable());
        assert!(!RTError::DeviceDisconnected.is_recoverable());
        assert!(!RTError::TorqueLimit.is_recoverable());
        assert!(!RTError::PipelineFault.is_recoverable());
        Ok(())
    }

    #[test]
    fn rt_requires_safety_action() -> Result<(), OpenRacingError> {
        assert!(RTError::DeviceDisconnected.requires_safety_action());
        assert!(RTError::TorqueLimit.requires_safety_action());
        assert!(RTError::SafetyInterlock.requires_safety_action());
        assert!(RTError::DeadlineMissed.requires_safety_action());
        assert!(!RTError::TimingViolation.requires_safety_action());
        assert!(!RTError::PipelineFault.requires_safety_action());
        assert!(!RTError::InvalidConfig.requires_safety_action());
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Property tests: any error can be displayed without panicking
// ---------------------------------------------------------------------------

mod property_tests {
    use super::*;

    fn arb_rt_error() -> impl Strategy<Value = RTError> {
        prop_oneof![
            Just(RTError::DeviceDisconnected),
            Just(RTError::TorqueLimit),
            Just(RTError::PipelineFault),
            Just(RTError::TimingViolation),
            Just(RTError::RTSetupFailed),
            Just(RTError::InvalidConfig),
            Just(RTError::SafetyInterlock),
            Just(RTError::BufferOverflow),
            Just(RTError::DeadlineMissed),
            Just(RTError::ResourceUnavailable),
        ]
    }

    fn arb_openracing_error() -> impl Strategy<Value = OpenRacingError> {
        prop_oneof![
            arb_rt_error().prop_map(OpenRacingError::from),
            ".*".prop_map(|s| DeviceError::not_found(s).into()),
            ".*".prop_map(|s| ProfileError::not_found(s).into()),
            ".*".prop_map(|s| ValidationError::required(s).into()),
            ".*".prop_map(OpenRacingError::config),
            ".*".prop_map(OpenRacingError::other),
        ]
    }

    proptest! {
        #[test]
        fn any_error_can_be_displayed(err in arb_openracing_error()) {
            // Should never panic
            let _display = err.to_string();
        }

        #[test]
        fn any_error_can_be_debug_printed(err in arb_openracing_error()) {
            let dbg = format!("{err:?}");
            prop_assert!(!dbg.is_empty());
        }

        #[test]
        fn any_error_has_a_valid_category(err in arb_openracing_error()) {
            let cat = err.category();
            prop_assert!(matches!(
                cat,
                ErrorCategory::RT
                    | ErrorCategory::Device
                    | ErrorCategory::Profile
                    | ErrorCategory::Validation
                    | ErrorCategory::IO
                    | ErrorCategory::Config
                    | ErrorCategory::Other
            ));
        }

        #[test]
        fn any_error_has_a_valid_severity(err in arb_openracing_error()) {
            let sev = err.severity();
            prop_assert!(matches!(
                sev,
                ErrorSeverity::Info
                    | ErrorSeverity::Warning
                    | ErrorSeverity::Error
                    | ErrorSeverity::Critical
            ));
        }

        #[test]
        fn recoverability_consistent_with_severity(err in arb_openracing_error()) {
            let recoverable = err.is_recoverable();
            let severity = err.severity();
            if severity == ErrorSeverity::Critical {
                prop_assert!(!recoverable, "Critical errors should not be recoverable");
            } else {
                prop_assert!(recoverable, "Non-critical errors should be recoverable");
            }
        }

        #[test]
        fn category_display_is_nonempty(err in arb_openracing_error()) {
            let cat_str = err.category().to_string();
            prop_assert!(!cat_str.is_empty());
        }
    }
}

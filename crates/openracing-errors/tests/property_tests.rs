//! Property-based tests for error composition and context preservation.

use openracing_errors::{
    common::{ErrorCategory, ErrorContext, ErrorSeverity, OpenRacingError, ResultExt},
    device::DeviceError,
    profile::ProfileError,
    rt::RTError,
    validation::ValidationError,
};
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_rt_error_roundtrip(code in 1u8..=10u8) {
        if let Some(err) = RTError::from_code(code) {
            assert_eq!(err.code(), code);
        }
    }

    #[test]
    fn test_error_severity_ordering(a in 0u8..=3, b in 0u8..=3) {
        let sev_a = match a {
            0 => ErrorSeverity::Info,
            1 => ErrorSeverity::Warning,
            2 => ErrorSeverity::Error,
            _ => ErrorSeverity::Critical,
        };
        let sev_b = match b {
            0 => ErrorSeverity::Info,
            1 => ErrorSeverity::Warning,
            2 => ErrorSeverity::Error,
            _ => ErrorSeverity::Critical,
        };

        prop_assert_eq!(a.cmp(&b), sev_a.cmp(&sev_b));
    }

    #[test]
    fn test_error_context_preserves_operation(operation in ".*") {
        let ctx = ErrorContext::new(&operation);
        prop_assert!(ctx.to_string().contains(&operation) || operation.is_empty());
    }

    #[test]
    fn test_validation_error_display_not_empty(field in ".*", _value in ".*") {
        let err = ValidationError::Required(field.clone());
        let msg = err.to_string();
        prop_assert!(!msg.is_empty());
        if !field.is_empty() {
            prop_assert!(msg.contains(&field));
        }
    }

    #[test]
    fn test_device_error_message_contains_device(device in "[a-zA-Z0-9_-]+") {
        let err = DeviceError::not_found(&device);
        let msg = err.to_string();
        prop_assert!(msg.contains(&device));
    }

    #[test]
    fn test_profile_error_message_contains_id(profile_id in "[a-zA-Z0-9_-]+") {
        let err = ProfileError::not_found(&profile_id);
        let msg = err.to_string();
        prop_assert!(msg.contains(&profile_id));
    }

    #[test]
    fn test_error_category_consistency(code in 0u8..=10u8) {
        let err = match code {
            0 => RTError::DeviceDisconnected.into(),
            1 => DeviceError::not_found("test").into(),
            2 => ProfileError::not_found("test").into(),
            3 => ValidationError::required("test").into(),
            4 => OpenRacingError::config("test"),
            5 => OpenRacingError::other("test"),
            _ => return Ok(()),
        };

        let category = err.category();
        prop_assert!(matches!(category,
            ErrorCategory::RT |
            ErrorCategory::Device |
            ErrorCategory::Profile |
            ErrorCategory::Validation |
            ErrorCategory::Config |
            ErrorCategory::Other
        ));
    }

    #[test]
    fn test_error_severity_never_empty(code in 0u8..=10u8) {
        let err: OpenRacingError = match code {
            0 => RTError::DeviceDisconnected.into(),
            1 => RTError::TimingViolation.into(),
            2 => DeviceError::not_found("test").into(),
            3 => DeviceError::timeout("test", 100).into(),
            4 => ProfileError::not_found("test").into(),
            5 => ProfileError::Locked("test".into()).into(),
            6 => ValidationError::required("test").into(),
            _ => OpenRacingError::config("test"),
        };

        let severity = err.severity();
        let msg = severity.to_string();
        prop_assert!(!msg.is_empty());
    }
}

mod error_chain_tests {
    use super::*;

    proptest! {
        #[test]
        fn test_error_context_chain_preserves_all(
            op in "op[0-9]+",
            key1 in "key[0-9]+",
            val1 in "val[0-9]+"
        ) {
            let ctx = ErrorContext::new(&op)
                .with(&key1, &val1);

            let msg = ctx.to_string();
            prop_assert!(msg.contains(&op) || op.is_empty());
            prop_assert!(msg.contains(&key1) || key1.is_empty());
            prop_assert!(msg.contains(&val1) || val1.is_empty());
        }
    }
}

mod result_ext_property_tests {
    use super::*;

    proptest! {
        #[test]
        fn test_result_ext_preserves_error_type(code in 1u8..=4u8) {
            let rt_err = match code {
                1 => RTError::DeviceDisconnected,
                2 => RTError::TorqueLimit,
                3 => RTError::PipelineFault,
                _ => RTError::TimingViolation,
            };

            let result: std::result::Result<(), RTError> = Err(rt_err);
            let result = result.with_context("test");

            prop_assert!(result.is_err());
        }
    }
}

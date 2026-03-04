//! Property-based tests for the openracing-errors crate.
//!
//! These tests verify critical error invariants that must hold:
//! - Display is never empty for any error variant
//! - Error source chain is finite
//! - Severity is always defined
//! - Error code roundtrips

use openracing_errors::{
    DeviceError, ErrorCategory, ErrorSeverity, OpenRacingError, ProfileError, RTError,
    ValidationError,
};
use proptest::prelude::*;

/// proptest config with 200 cases per test
fn config() -> ProptestConfig {
    ProptestConfig {
        cases: 200,
        ..Default::default()
    }
}

/// Strategy producing all RTError variants
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

/// Strategy producing arbitrary DeviceError variants
fn arb_device_error() -> impl Strategy<Value = DeviceError> {
    prop_oneof![
        "[a-z0-9_-]{1,30}".prop_map(DeviceError::NotFound),
        "[a-z0-9_-]{1,30}".prop_map(DeviceError::Disconnected),
        "[a-z0-9_-]{1,30}".prop_map(DeviceError::ConnectionFailed),
        ("\\PC{1,20}", "\\PC{1,30}")
            .prop_map(|(device, message)| { DeviceError::CommunicationError { device, message } }),
        "\\PC{1,30}".prop_map(DeviceError::HidError),
        ("\\PC{1,20}", 0usize..1000, 0usize..1000).prop_map(|(device, expected, actual)| {
            DeviceError::InvalidResponse {
                device,
                expected,
                actual,
            }
        }),
        ("\\PC{1,20}", 0u64..60_000)
            .prop_map(|(device, timeout_ms)| DeviceError::Timeout { device, timeout_ms }),
        (any::<u16>(), any::<u16>()).prop_map(|(vendor_id, product_id)| {
            DeviceError::UnsupportedDevice {
                vendor_id,
                product_id,
            }
        }),
        "\\PC{1,20}".prop_map(DeviceError::Busy),
        "\\PC{1,20}".prop_map(DeviceError::PermissionDenied),
        ("\\PC{1,20}", "\\PC{1,30}")
            .prop_map(|(device, reason)| { DeviceError::InitializationFailed { device, reason } }),
        ("\\PC{1,20}", "\\PC{1,30}")
            .prop_map(|(device, message)| DeviceError::FirmwareError { device, message }),
        ("\\PC{1,20}", "\\PC{1,30}")
            .prop_map(|(device, feature)| { DeviceError::FeatureNotSupported { device, feature } }),
    ]
}

/// Strategy producing arbitrary ProfileError variants
fn arb_profile_error() -> impl Strategy<Value = ProfileError> {
    prop_oneof![
        "\\PC{1,30}".prop_map(ProfileError::NotFound),
        "\\PC{1,30}".prop_map(ProfileError::AlreadyExists),
        ("\\PC{1,20}", "\\PC{1,30}")
            .prop_map(|(path, reason)| ProfileError::InvalidFormat { path, reason }),
        "\\PC{1,30}".prop_map(ProfileError::ValidationFailed),
        ("\\PC{1,20}", "\\PC{1,30}")
            .prop_map(|(profile, reason)| ProfileError::SaveFailed { profile, reason }),
        ("\\PC{1,20}", "\\PC{1,30}")
            .prop_map(|(path, reason)| ProfileError::LoadFailed { path, reason }),
        "\\PC{1,30}".prop_map(|chain| ProfileError::CircularInheritance { chain }),
        (1usize..100, 1usize..100).prop_map(|(depth, max_depth)| {
            ProfileError::InheritanceDepthExceeded { depth, max_depth }
        }),
        "\\PC{1,30}".prop_map(|parent_id| ProfileError::ParentNotFound { parent_id }),
        "\\PC{1,30}".prop_map(ProfileError::InvalidId),
        "\\PC{1,30}".prop_map(ProfileError::Conflict),
        ("\\PC{1,10}", "\\PC{1,10}")
            .prop_map(|(expected, found)| { ProfileError::VersionMismatch { expected, found } }),
        ("\\PC{1,20}", "\\PC{1,20}")
            .prop_map(|(profile, field)| ProfileError::MissingField { profile, field }),
        "\\PC{1,30}".prop_map(ProfileError::Locked),
        ("\\PC{1,20}", "\\PC{1,20}").prop_map(|(profile, device)| {
            ProfileError::InvalidDeviceMapping { profile, device }
        }),
    ]
}

/// Strategy producing arbitrary ValidationError variants
fn arb_validation_error() -> impl Strategy<Value = ValidationError> {
    prop_oneof![
        ("\\PC{1,20}", "\\PC{1,10}", "\\PC{1,10}", "\\PC{1,10}").prop_map(
            |(field, val, min, max)| ValidationError::OutOfRange {
                field,
                value: val,
                min,
                max,
            }
        ),
        "\\PC{1,30}".prop_map(ValidationError::Required),
        ("\\PC{1,20}", "\\PC{1,30}")
            .prop_map(|(field, reason)| ValidationError::InvalidFormat { field, reason }),
        ("\\PC{1,20}", 1usize..1000, 1usize..1000)
            .prop_map(|(field, actual, max)| { ValidationError::TooLong { field, actual, max } }),
        ("\\PC{1,20}", 0usize..100, 1usize..100)
            .prop_map(|(field, actual, min)| { ValidationError::TooShort { field, actual, min } }),
        ("\\PC{1,20}", "\\PC{1,20}", "\\PC{1,30}").prop_map(|(field, value, expected)| {
            ValidationError::InvalidEnumValue {
                field,
                value,
                expected,
            }
        }),
        "\\PC{1,30}".prop_map(ValidationError::ConstraintViolation),
        ("\\PC{1,20}", "\\PC{1,30}")
            .prop_map(|(field, reason)| { ValidationError::InvalidCharacters { field, reason } }),
        ("\\PC{1,20}", "\\PC{1,30}")
            .prop_map(|(field, value)| ValidationError::NotUnique { field, value }),
        ("\\PC{1,20}", "\\PC{1,30}").prop_map(|(field, dep)| ValidationError::DependencyNotMet {
            field,
            dependency: dep,
        }),
        ("\\PC{1,20}", "\\PC{1,20}", "\\PC{1,20}").prop_map(|(field, expected, actual)| {
            ValidationError::InvalidType {
                field,
                expected,
                actual,
            }
        }),
        "\\PC{1,20}".prop_map(|field| ValidationError::NumericOverflow { field }),
        "\\PC{1,30}".prop_map(ValidationError::Custom),
    ]
}

// ---------------------------------------------------------------------------
// RTError invariants
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(config())]

    /// RTError Display is never empty.
    #[test]
    fn rt_error_display_never_empty(err in arb_rt_error()) {
        let display = err.to_string();
        prop_assert!(!display.is_empty(), "RTError Display was empty for {:?}", err);
    }

    /// RTError code roundtrips via from_code.
    #[test]
    fn rt_error_code_roundtrip(err in arb_rt_error()) {
        let code = err.code();
        let recovered = RTError::from_code(code);
        prop_assert_eq!(recovered, Some(err));
    }

    /// RTError severity is always defined (one of the known severity levels).
    #[test]
    fn rt_error_severity_always_defined(err in arb_rt_error()) {
        let severity = err.severity();
        prop_assert!(
            matches!(
                severity,
                ErrorSeverity::Info
                    | ErrorSeverity::Warning
                    | ErrorSeverity::Error
                    | ErrorSeverity::Critical
            ),
            "Unexpected severity: {:?}",
            severity
        );
    }

    /// RTError code is always in [1, 10].
    #[test]
    fn rt_error_code_in_range(err in arb_rt_error()) {
        let code = err.code();
        prop_assert!((1..=10).contains(&code), "Code out of range: {}", code);
    }

    /// from_code returns None for any code outside [1, 10].
    #[test]
    fn rt_error_from_invalid_code(code in 11u8..=255) {
        prop_assert_eq!(RTError::from_code(code), None);
    }
}

// ---------------------------------------------------------------------------
// DeviceError invariants
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(config())]

    /// DeviceError Display is never empty.
    #[test]
    fn device_error_display_never_empty(err in arb_device_error()) {
        let display = err.to_string();
        prop_assert!(!display.is_empty(), "DeviceError Display was empty");
    }

    /// DeviceError severity is always a valid ErrorSeverity.
    #[test]
    fn device_error_severity_valid(err in arb_device_error()) {
        let severity = err.severity();
        prop_assert!(
            matches!(
                severity,
                ErrorSeverity::Info
                    | ErrorSeverity::Warning
                    | ErrorSeverity::Error
                    | ErrorSeverity::Critical
            ),
        );
    }

    /// DeviceError source chain terminates (no infinite loops).
    #[test]
    fn device_error_source_chain_finite(err in arb_device_error()) {
        let err_ref: &dyn std::error::Error = &err;
        let mut chain_len = 0;
        let mut current: Option<&dyn std::error::Error> = Some(err_ref);
        while let Some(e) = current {
            chain_len += 1;
            prop_assert!(chain_len < 100, "Error source chain too long (possible loop)");
            current = e.source();
        }
    }
}

// ---------------------------------------------------------------------------
// ProfileError invariants
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(config())]

    /// ProfileError Display is never empty.
    #[test]
    fn profile_error_display_never_empty(err in arb_profile_error()) {
        let display = err.to_string();
        prop_assert!(!display.is_empty(), "ProfileError Display was empty");
    }

    /// ProfileError severity is always a valid ErrorSeverity.
    #[test]
    fn profile_error_severity_valid(err in arb_profile_error()) {
        let severity = err.severity();
        prop_assert!(
            matches!(
                severity,
                ErrorSeverity::Info
                    | ErrorSeverity::Warning
                    | ErrorSeverity::Error
                    | ErrorSeverity::Critical
            ),
        );
    }
}

// ---------------------------------------------------------------------------
// ValidationError invariants
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(config())]

    /// ValidationError Display is never empty.
    #[test]
    fn validation_error_display_never_empty(err in arb_validation_error()) {
        let display = err.to_string();
        prop_assert!(!display.is_empty(), "ValidationError Display was empty");
    }

    /// ValidationError severity is always Error.
    #[test]
    fn validation_error_severity_always_error(err in arb_validation_error()) {
        prop_assert_eq!(err.severity(), ErrorSeverity::Error);
    }
}

// ---------------------------------------------------------------------------
// OpenRacingError wrapper invariants
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(config())]

    /// OpenRacingError wrapping RTError preserves category.
    #[test]
    fn openracing_rt_category(err in arb_rt_error()) {
        let wrapped: OpenRacingError = err.into();
        prop_assert_eq!(wrapped.category(), ErrorCategory::RT);
    }

    /// OpenRacingError wrapping DeviceError preserves category.
    #[test]
    fn openracing_device_category(err in arb_device_error()) {
        let wrapped: OpenRacingError = err.into();
        prop_assert_eq!(wrapped.category(), ErrorCategory::Device);
    }

    /// OpenRacingError wrapping ProfileError preserves category.
    #[test]
    fn openracing_profile_category(err in arb_profile_error()) {
        let wrapped: OpenRacingError = err.into();
        prop_assert_eq!(wrapped.category(), ErrorCategory::Profile);
    }

    /// OpenRacingError wrapping ValidationError preserves category.
    #[test]
    fn openracing_validation_category(err in arb_validation_error()) {
        let wrapped: OpenRacingError = err.into();
        prop_assert_eq!(wrapped.category(), ErrorCategory::Validation);
    }

    /// OpenRacingError Display is never empty for any variant.
    #[test]
    fn openracing_display_never_empty(err in arb_rt_error()) {
        let wrapped: OpenRacingError = err.into();
        prop_assert!(!wrapped.to_string().is_empty());
    }

    /// OpenRacingError severity propagates from inner error.
    #[test]
    fn openracing_severity_propagates(err in arb_rt_error()) {
        let expected_severity = err.severity();
        let wrapped: OpenRacingError = err.into();
        prop_assert_eq!(wrapped.severity(), expected_severity);
    }

    /// OpenRacingError source chain is finite.
    #[test]
    fn openracing_source_chain_finite(err in arb_rt_error()) {
        let wrapped: OpenRacingError = err.into();
        let err_ref: &dyn std::error::Error = &wrapped;
        let mut chain_len = 0;
        let mut current: Option<&dyn std::error::Error> = Some(err_ref);
        while let Some(e) = current {
            chain_len += 1;
            prop_assert!(chain_len < 100, "Source chain too long");
            current = e.source();
        }
    }
}

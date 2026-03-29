//! Exhaustiveness tests for the error crate.
//!
//! These tests verify that every error variant has coverage for:
//! - Display output (non-empty)
//! - `From` impls for external error types
//! - Category classification
//! - Severity level assignment

use openracing_errors::{
    common::{ErrorCategory, ErrorSeverity, OpenRacingError},
    device::DeviceError,
    profile::ProfileError,
    rt::RTError,
    validation::ValidationError,
};

// ---------------------------------------------------------------------------
// Helper: all RT variants (single source of truth)
// ---------------------------------------------------------------------------
fn all_rt_variants() -> Vec<RTError> {
    vec![
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
        RTError::AccessViolation,
    ]
}

// ---------------------------------------------------------------------------
// Helper: all Device variants
// ---------------------------------------------------------------------------
fn all_device_variants() -> Vec<DeviceError> {
    vec![
        DeviceError::NotFound("x".into()),
        DeviceError::Disconnected("x".into()),
        DeviceError::ConnectionFailed("x".into()),
        DeviceError::CommunicationError {
            device: "x".into(),
            message: "m".into(),
        },
        DeviceError::HidError("x".into()),
        DeviceError::InvalidResponse {
            device: "x".into(),
            expected: 64,
            actual: 32,
        },
        DeviceError::Timeout {
            device: "x".into(),
            timeout_ms: 100,
        },
        DeviceError::UnsupportedDevice {
            vendor_id: 0,
            product_id: 0,
        },
        DeviceError::Busy("x".into()),
        DeviceError::PermissionDenied("x".into()),
        DeviceError::InitializationFailed {
            device: "x".into(),
            reason: "r".into(),
        },
        DeviceError::FirmwareError {
            device: "x".into(),
            message: "m".into(),
        },
        DeviceError::FeatureNotSupported {
            device: "x".into(),
            feature: "f".into(),
        },
    ]
}

// ---------------------------------------------------------------------------
// Helper: all Profile variants
// ---------------------------------------------------------------------------
fn all_profile_variants() -> Vec<ProfileError> {
    vec![
        ProfileError::NotFound("x".into()),
        ProfileError::AlreadyExists("x".into()),
        ProfileError::InvalidFormat {
            path: "p".into(),
            reason: "r".into(),
        },
        ProfileError::ValidationFailed("x".into()),
        ProfileError::SaveFailed {
            profile: "x".into(),
            reason: "r".into(),
        },
        ProfileError::LoadFailed {
            path: "p".into(),
            reason: "r".into(),
        },
        ProfileError::CircularInheritance {
            chain: "a -> b".into(),
        },
        ProfileError::InheritanceDepthExceeded {
            depth: 10,
            max_depth: 5,
        },
        ProfileError::ParentNotFound {
            parent_id: "x".into(),
        },
        ProfileError::InvalidId("x".into()),
        ProfileError::Conflict("x".into()),
        ProfileError::VersionMismatch {
            expected: "2".into(),
            found: "1".into(),
        },
        ProfileError::MissingField {
            profile: "p".into(),
            field: "f".into(),
        },
        ProfileError::Locked("x".into()),
        ProfileError::InvalidDeviceMapping {
            profile: "p".into(),
            device: "d".into(),
        },
    ]
}

// ---------------------------------------------------------------------------
// Helper: all Validation variants
// ---------------------------------------------------------------------------
fn all_validation_variants() -> Vec<ValidationError> {
    vec![
        ValidationError::OutOfRange {
            field: "f".into(),
            value: "1".into(),
            min: "0".into(),
            max: "2".into(),
        },
        ValidationError::Required("f".into()),
        ValidationError::InvalidFormat {
            field: "f".into(),
            reason: "r".into(),
        },
        ValidationError::TooLong {
            field: "f".into(),
            actual: 10,
            max: 5,
        },
        ValidationError::TooShort {
            field: "f".into(),
            actual: 1,
            min: 5,
        },
        ValidationError::InvalidEnumValue {
            field: "f".into(),
            value: "v".into(),
            expected: "a, b".into(),
        },
        ValidationError::ConstraintViolation("c".into()),
        ValidationError::InvalidCharacters {
            field: "f".into(),
            reason: "r".into(),
        },
        ValidationError::NotUnique {
            field: "f".into(),
            value: "v".into(),
        },
        ValidationError::DependencyNotMet {
            field: "f".into(),
            dependency: "d".into(),
        },
        ValidationError::InvalidType {
            field: "f".into(),
            expected: "i32".into(),
            actual: "str".into(),
        },
        ValidationError::NumericOverflow { field: "f".into() },
        ValidationError::Custom("c".into()),
    ]
}

// ---------------------------------------------------------------------------
// 1. Every variant produces a non-empty Display message
// ---------------------------------------------------------------------------
mod display_exhaustiveness {
    use super::*;

    #[test]
    fn rt_all_variants_display_non_empty() -> openracing_errors::Result<()> {
        for variant in all_rt_variants() {
            let msg = variant.to_string();
            assert!(!msg.is_empty(), "RTError::{variant:?} has empty Display");
        }
        Ok(())
    }

    #[test]
    fn device_all_variants_display_non_empty() -> openracing_errors::Result<()> {
        for variant in all_device_variants() {
            let msg = variant.to_string();
            assert!(
                !msg.is_empty(),
                "DeviceError::{variant:?} has empty Display"
            );
        }
        Ok(())
    }

    #[test]
    fn profile_all_variants_display_non_empty() -> openracing_errors::Result<()> {
        for variant in all_profile_variants() {
            let msg = variant.to_string();
            assert!(
                !msg.is_empty(),
                "ProfileError::{variant:?} has empty Display"
            );
        }
        Ok(())
    }

    #[test]
    fn validation_all_variants_display_non_empty() -> openracing_errors::Result<()> {
        for variant in all_validation_variants() {
            let msg = variant.to_string();
            assert!(
                !msg.is_empty(),
                "ValidationError::{variant:?} has empty Display"
            );
        }
        Ok(())
    }

    #[test]
    fn rt_variant_count_matches() -> openracing_errors::Result<()> {
        // Ensure all codes 1..=10 are covered by from_code
        let expected_count = all_rt_variants().len();
        let from_code_count = (0..=u8::MAX)
            .filter(|c| RTError::from_code(*c).is_some())
            .count();
        assert_eq!(
            expected_count, from_code_count,
            "all_rt_variants() count ({expected_count}) != from_code coverage ({from_code_count})"
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 2. From impls for all external error types into OpenRacingError
// ---------------------------------------------------------------------------
mod from_impl_exhaustiveness {
    use super::*;

    #[test]
    fn from_rt_error() -> openracing_errors::Result<()> {
        let orig = RTError::PipelineFault;
        let wrapped: OpenRacingError = orig.into();
        assert!(
            matches!(wrapped, OpenRacingError::RT(RTError::PipelineFault)),
            "From<RTError> did not produce OpenRacingError::RT"
        );
        Ok(())
    }

    #[test]
    fn from_device_error() -> openracing_errors::Result<()> {
        let orig = DeviceError::not_found("dev");
        let wrapped: OpenRacingError = orig.into();
        assert!(
            matches!(wrapped, OpenRacingError::Device(_)),
            "From<DeviceError> did not produce OpenRacingError::Device"
        );
        Ok(())
    }

    #[test]
    fn from_profile_error() -> openracing_errors::Result<()> {
        let orig = ProfileError::not_found("prof");
        let wrapped: OpenRacingError = orig.into();
        assert!(
            matches!(wrapped, OpenRacingError::Profile(_)),
            "From<ProfileError> did not produce OpenRacingError::Profile"
        );
        Ok(())
    }

    #[test]
    fn from_validation_error() -> openracing_errors::Result<()> {
        let orig = ValidationError::required("fld");
        let wrapped: OpenRacingError = orig.into();
        assert!(
            matches!(wrapped, OpenRacingError::Validation(_)),
            "From<ValidationError> did not produce OpenRacingError::Validation"
        );
        Ok(())
    }

    #[test]
    fn from_io_error() -> openracing_errors::Result<()> {
        let orig = std::io::Error::other("io");
        let wrapped: OpenRacingError = orig.into();
        assert!(
            matches!(wrapped, OpenRacingError::Io(_)),
            "From<io::Error> did not produce OpenRacingError::Io"
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 3. Category classification for every variant
// ---------------------------------------------------------------------------
mod category_exhaustiveness {
    use super::*;

    #[test]
    fn rt_variants_have_rt_category() -> openracing_errors::Result<()> {
        for variant in all_rt_variants() {
            let wrapped: OpenRacingError = variant.into();
            assert_eq!(
                wrapped.category(),
                ErrorCategory::RT,
                "RTError::{variant:?} should map to ErrorCategory::RT"
            );
        }
        Ok(())
    }

    #[test]
    fn device_variants_have_device_category() -> openracing_errors::Result<()> {
        for variant in all_device_variants() {
            let wrapped: OpenRacingError = variant.into();
            assert_eq!(
                wrapped.category(),
                ErrorCategory::Device,
                "DeviceError should map to ErrorCategory::Device"
            );
        }
        Ok(())
    }

    #[test]
    fn profile_variants_have_profile_category() -> openracing_errors::Result<()> {
        for variant in all_profile_variants() {
            let wrapped: OpenRacingError = variant.into();
            assert_eq!(
                wrapped.category(),
                ErrorCategory::Profile,
                "ProfileError should map to ErrorCategory::Profile"
            );
        }
        Ok(())
    }

    #[test]
    fn validation_variants_have_validation_category() -> openracing_errors::Result<()> {
        for variant in all_validation_variants() {
            let wrapped: OpenRacingError = variant.into();
            assert_eq!(
                wrapped.category(),
                ErrorCategory::Validation,
                "ValidationError should map to ErrorCategory::Validation"
            );
        }
        Ok(())
    }

    #[test]
    fn config_has_config_category() -> openracing_errors::Result<()> {
        assert_eq!(
            OpenRacingError::config("x").category(),
            ErrorCategory::Config
        );
        Ok(())
    }

    #[test]
    fn io_has_io_category() -> openracing_errors::Result<()> {
        let err: OpenRacingError = std::io::Error::other("x").into();
        assert_eq!(err.category(), ErrorCategory::IO);
        Ok(())
    }

    #[test]
    fn other_has_other_category() -> openracing_errors::Result<()> {
        assert_eq!(OpenRacingError::other("x").category(), ErrorCategory::Other);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 4. Severity for every variant (verifies .severity() does not panic)
// ---------------------------------------------------------------------------
mod severity_exhaustiveness {
    use super::*;

    #[test]
    fn rt_all_variants_have_severity() -> openracing_errors::Result<()> {
        for variant in all_rt_variants() {
            let _sev = variant.severity();
        }
        Ok(())
    }

    #[test]
    fn device_all_variants_have_severity() -> openracing_errors::Result<()> {
        for variant in all_device_variants() {
            let _sev = variant.severity();
        }
        Ok(())
    }

    #[test]
    fn profile_all_variants_have_severity() -> openracing_errors::Result<()> {
        for variant in all_profile_variants() {
            let _sev = variant.severity();
        }
        Ok(())
    }

    #[test]
    fn validation_all_variants_have_severity() -> openracing_errors::Result<()> {
        for variant in all_validation_variants() {
            let _sev = variant.severity();
        }
        Ok(())
    }

    #[test]
    fn openracing_all_variants_have_severity() -> openracing_errors::Result<()> {
        let variants: Vec<OpenRacingError> = vec![
            RTError::PipelineFault.into(),
            DeviceError::not_found("x").into(),
            ProfileError::not_found("x").into(),
            ValidationError::required("x").into(),
            std::io::Error::other("x").into(),
            OpenRacingError::config("x"),
            OpenRacingError::other("x"),
        ];
        for variant in &variants {
            let _sev = variant.severity();
        }
        Ok(())
    }

    #[test]
    fn rt_critical_variants_not_recoverable() -> openracing_errors::Result<()> {
        for variant in all_rt_variants() {
            let wrapped: OpenRacingError = variant.into();
            if variant.severity() == ErrorSeverity::Critical {
                assert!(
                    !wrapped.is_recoverable(),
                    "RTError::{variant:?} is Critical but marked recoverable"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn rt_non_critical_variants_are_recoverable() -> openracing_errors::Result<()> {
        for variant in all_rt_variants() {
            let wrapped: OpenRacingError = variant.into();
            if variant.severity() < ErrorSeverity::Critical {
                assert!(
                    wrapped.is_recoverable(),
                    "RTError::{variant:?} is non-Critical but marked non-recoverable"
                );
            }
        }
        Ok(())
    }
}

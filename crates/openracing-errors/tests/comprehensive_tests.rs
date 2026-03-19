#![allow(clippy::manual_range_contains)]
//! Comprehensive tests for openracing-errors.
//!
//! Covers: error variant construction, Display formatting, From impls,
//! error source chain, and category/severity classification exhaustiveness.

use openracing_errors::{
    OpenRacingError, RTResult, Result,
    common::{ErrorCategory, ErrorContext, ErrorSeverity, ResultExt},
    device::DeviceError,
    profile::ProfileError,
    rt::RTError,
    validation::ValidationError,
};

// ---------------------------------------------------------------------------
// 1. Error variant construction & Display formatting
// ---------------------------------------------------------------------------

mod rt_error_display {
    use super::*;

    #[test]
    fn all_variants_have_nonempty_display() -> Result<()> {
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
            let msg = v.to_string();
            assert!(!msg.is_empty(), "RTError::{v:?} display must not be empty");
        }
        Ok(())
    }

    #[test]
    fn specific_display_messages() -> Result<()> {
        assert_eq!(RTError::TorqueLimit.to_string(), "Torque limit exceeded");
        assert_eq!(
            RTError::SafetyInterlock.to_string(),
            "Safety interlock triggered"
        );
        assert_eq!(RTError::BufferOverflow.to_string(), "RT buffer overflow");
        assert_eq!(RTError::DeadlineMissed.to_string(), "RT deadline missed");
        assert_eq!(
            RTError::ResourceUnavailable.to_string(),
            "RT resource unavailable"
        );
        assert_eq!(
            RTError::RTSetupFailed.to_string(),
            "Failed to apply real-time setup"
        );
        assert_eq!(
            RTError::InvalidConfig.to_string(),
            "Invalid configuration parameter"
        );
        assert_eq!(
            RTError::PipelineFault.to_string(),
            "Pipeline processing fault"
        );
        Ok(())
    }
}

mod device_error_display {
    use super::*;

    #[test]
    fn all_variants_display() -> Result<()> {
        let variants: Vec<DeviceError> = vec![
            DeviceError::NotFound("dev".into()),
            DeviceError::Disconnected("dev".into()),
            DeviceError::ConnectionFailed("dev".into()),
            DeviceError::CommunicationError {
                device: "dev".into(),
                message: "msg".into(),
            },
            DeviceError::HidError("hid".into()),
            DeviceError::InvalidResponse {
                device: "dev".into(),
                expected: 64,
                actual: 32,
            },
            DeviceError::Timeout {
                device: "dev".into(),
                timeout_ms: 500,
            },
            DeviceError::UnsupportedDevice {
                vendor_id: 0x1234,
                product_id: 0x5678,
            },
            DeviceError::Busy("dev".into()),
            DeviceError::PermissionDenied("dev".into()),
            DeviceError::InitializationFailed {
                device: "dev".into(),
                reason: "reason".into(),
            },
            DeviceError::FirmwareError {
                device: "dev".into(),
                message: "fw err".into(),
            },
            DeviceError::FeatureNotSupported {
                device: "dev".into(),
                feature: "led".into(),
            },
        ];

        for v in &variants {
            let msg = v.to_string();
            assert!(
                !msg.is_empty(),
                "DeviceError::{v:?} display must not be empty"
            );
        }
        Ok(())
    }

    #[test]
    fn display_contains_embedded_fields() -> Result<()> {
        let err = DeviceError::CommunicationError {
            device: "moza-r9".into(),
            message: "timeout reading HID".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("moza-r9"), "should contain device name");
        assert!(
            msg.contains("timeout reading HID"),
            "should contain message"
        );

        let err = DeviceError::InvalidResponse {
            device: "fanatec".into(),
            expected: 64,
            actual: 32,
        };
        let msg = err.to_string();
        assert!(msg.contains("64"));
        assert!(msg.contains("32"));

        let err = DeviceError::FeatureNotSupported {
            device: "simagic".into(),
            feature: "led_strip".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("simagic"));
        assert!(msg.contains("led_strip"));

        let err = DeviceError::InitializationFailed {
            device: "vrs".into(),
            reason: "firmware mismatch".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("vrs"));
        assert!(msg.contains("firmware mismatch"));

        let err = DeviceError::FirmwareError {
            device: "moza".into(),
            message: "crc failed".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("moza"));
        assert!(msg.contains("crc failed"));

        Ok(())
    }

    #[test]
    fn severity_exhaustive() -> Result<()> {
        let cases: Vec<(DeviceError, ErrorSeverity)> = vec![
            (DeviceError::not_found("d"), ErrorSeverity::Error),
            (DeviceError::disconnected("d"), ErrorSeverity::Critical),
            (
                DeviceError::ConnectionFailed("d".into()),
                ErrorSeverity::Error,
            ),
            (
                DeviceError::CommunicationError {
                    device: "d".into(),
                    message: "m".into(),
                },
                ErrorSeverity::Error,
            ),
            (DeviceError::HidError("h".into()), ErrorSeverity::Error),
            (
                DeviceError::InvalidResponse {
                    device: "d".into(),
                    expected: 1,
                    actual: 0,
                },
                ErrorSeverity::Error,
            ),
            (DeviceError::timeout("d", 100), ErrorSeverity::Warning),
            (DeviceError::unsupported(0, 0), ErrorSeverity::Error),
            (DeviceError::Busy("d".into()), ErrorSeverity::Warning),
            (
                DeviceError::PermissionDenied("d".into()),
                ErrorSeverity::Error,
            ),
            (
                DeviceError::InitializationFailed {
                    device: "d".into(),
                    reason: "r".into(),
                },
                ErrorSeverity::Error,
            ),
            (
                DeviceError::FirmwareError {
                    device: "d".into(),
                    message: "m".into(),
                },
                ErrorSeverity::Error,
            ),
            (
                DeviceError::FeatureNotSupported {
                    device: "d".into(),
                    feature: "f".into(),
                },
                ErrorSeverity::Info,
            ),
        ];
        for (err, expected) in &cases {
            assert_eq!(err.severity(), *expected, "Severity mismatch for {:?}", err);
        }
        Ok(())
    }

    #[test]
    fn permission_denied_is_device_unavailable() -> Result<()> {
        assert!(DeviceError::PermissionDenied("d".into()).is_device_unavailable());
        assert!(!DeviceError::Busy("d".into()).is_device_unavailable());
        assert!(!DeviceError::HidError("h".into()).is_device_unavailable());
        Ok(())
    }
}

mod profile_error_display {
    use super::*;

    #[test]
    fn all_variants_display() -> Result<()> {
        let variants: Vec<ProfileError> = vec![
            ProfileError::NotFound("p".into()),
            ProfileError::AlreadyExists("p".into()),
            ProfileError::InvalidFormat {
                path: "/p".into(),
                reason: "r".into(),
            },
            ProfileError::ValidationFailed("v".into()),
            ProfileError::SaveFailed {
                profile: "p".into(),
                reason: "r".into(),
            },
            ProfileError::LoadFailed {
                path: "/p".into(),
                reason: "r".into(),
            },
            ProfileError::CircularInheritance {
                chain: "a->b->a".into(),
            },
            ProfileError::InheritanceDepthExceeded {
                depth: 10,
                max_depth: 5,
            },
            ProfileError::ParentNotFound {
                parent_id: "parent".into(),
            },
            ProfileError::InvalidId("x".into()),
            ProfileError::Conflict("c".into()),
            ProfileError::VersionMismatch {
                expected: "2".into(),
                found: "1".into(),
            },
            ProfileError::MissingField {
                profile: "p".into(),
                field: "f".into(),
            },
            ProfileError::Locked("p".into()),
            ProfileError::InvalidDeviceMapping {
                profile: "p".into(),
                device: "d".into(),
            },
        ];

        for v in &variants {
            let msg = v.to_string();
            assert!(
                !msg.is_empty(),
                "ProfileError::{v:?} display must not be empty"
            );
        }
        Ok(())
    }

    #[test]
    fn severity_exhaustive() -> Result<()> {
        assert_eq!(
            ProfileError::AlreadyExists("p".into()).severity(),
            ErrorSeverity::Error
        );
        assert_eq!(
            ProfileError::Conflict("c".into()).severity(),
            ErrorSeverity::Warning
        );
        assert_eq!(
            ProfileError::Locked("p".into()).severity(),
            ErrorSeverity::Warning
        );
        assert_eq!(
            ProfileError::InvalidId("x".into()).severity(),
            ErrorSeverity::Error
        );
        assert_eq!(
            ProfileError::InvalidDeviceMapping {
                profile: "p".into(),
                device: "d".into()
            }
            .severity(),
            ErrorSeverity::Error
        );
        Ok(())
    }

    #[test]
    fn display_contains_fields() -> Result<()> {
        let err = ProfileError::SaveFailed {
            profile: "my-profile".into(),
            reason: "disk full".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("my-profile"));
        assert!(msg.contains("disk full"));

        let err = ProfileError::LoadFailed {
            path: "/profiles/gt3.yaml".into(),
            reason: "parse error".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("/profiles/gt3.yaml"));
        assert!(msg.contains("parse error"));

        let err = ProfileError::InheritanceDepthExceeded {
            depth: 10,
            max_depth: 5,
        };
        let msg = err.to_string();
        assert!(msg.contains("10"));
        assert!(msg.contains("5"));

        let err = ProfileError::InvalidDeviceMapping {
            profile: "drift".into(),
            device: "wheel-x".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("drift"));
        assert!(msg.contains("wheel-x"));

        Ok(())
    }
}

mod validation_error_display {
    use super::*;

    #[test]
    fn all_variants_display() -> Result<()> {
        let variants: Vec<ValidationError> = vec![
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
                actual: 100,
                max: 50,
            },
            ValidationError::TooShort {
                field: "f".into(),
                actual: 2,
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
                expected: "u32".into(),
                actual: "string".into(),
            },
            ValidationError::NumericOverflow { field: "f".into() },
            ValidationError::Custom("custom msg".into()),
        ];

        for v in &variants {
            let msg = v.to_string();
            assert!(
                !msg.is_empty(),
                "ValidationError::{v:?} display must not be empty"
            );
        }
        Ok(())
    }

    #[test]
    fn display_contains_fields() -> Result<()> {
        let err = ValidationError::InvalidCharacters {
            field: "username".into(),
            reason: "contains whitespace".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("username"));
        assert!(msg.contains("contains whitespace"));

        let err = ValidationError::NotUnique {
            field: "email".into(),
            value: "user@example.com".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("email"));
        assert!(msg.contains("user@example.com"));

        let err = ValidationError::DependencyNotMet {
            field: "gain".into(),
            dependency: "mode".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("gain"));
        assert!(msg.contains("mode"));

        let err = ValidationError::InvalidType {
            field: "count".into(),
            expected: "integer".into(),
            actual: "float".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("count"));
        assert!(msg.contains("integer"));
        assert!(msg.contains("float"));

        let err = ValidationError::NumericOverflow {
            field: "tick_count".into(),
        };
        assert!(err.to_string().contains("tick_count"));

        let err = ValidationError::TooShort {
            field: "password".into(),
            actual: 3,
            min: 8,
        };
        let msg = err.to_string();
        assert!(msg.contains("3"));
        assert!(msg.contains("8"));

        Ok(())
    }

    #[test]
    fn all_variants_severity_is_error() -> Result<()> {
        let variants: Vec<ValidationError> = vec![
            ValidationError::required("f"),
            ValidationError::out_of_range("f", 1.0_f32, 0.0_f32, 0.5_f32),
            ValidationError::invalid_format("f", "r"),
            ValidationError::too_long("f", 10, 5),
            ValidationError::too_short("f", 1, 5),
            ValidationError::invalid_enum("f", "v", "a, b"),
            ValidationError::constraint("c"),
            ValidationError::custom("c"),
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
                expected: "e".into(),
                actual: "a".into(),
            },
            ValidationError::NumericOverflow { field: "f".into() },
        ];
        for v in &variants {
            assert_eq!(
                v.severity(),
                ErrorSeverity::Error,
                "All ValidationError variants should have Error severity"
            );
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 2. Error conversion (From impls)
// ---------------------------------------------------------------------------

mod from_impls {
    use super::*;

    #[test]
    fn rt_error_into_openracing_error() -> Result<()> {
        let all_rt = [
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
        for rt in &all_rt {
            let ore: OpenRacingError = (*rt).into();
            assert_eq!(ore.category(), ErrorCategory::RT);
            // The wrapping should preserve the inner display
            assert!(ore.to_string().contains(&rt.to_string()));
        }
        Ok(())
    }

    #[test]
    fn device_error_into_openracing_error() -> Result<()> {
        let err: OpenRacingError = DeviceError::not_found("usb-wheel").into();
        assert_eq!(err.category(), ErrorCategory::Device);
        assert!(err.to_string().contains("usb-wheel"));
        Ok(())
    }

    #[test]
    fn profile_error_into_openracing_error() -> Result<()> {
        let err: OpenRacingError = ProfileError::not_found("my-profile").into();
        assert_eq!(err.category(), ErrorCategory::Profile);
        assert!(err.to_string().contains("my-profile"));
        Ok(())
    }

    #[test]
    fn validation_error_into_openracing_error() -> Result<()> {
        let err: OpenRacingError = ValidationError::required("torque_limit").into();
        assert_eq!(err.category(), ErrorCategory::Validation);
        assert!(err.to_string().contains("torque_limit"));
        Ok(())
    }

    #[test]
    fn io_error_into_openracing_error() -> Result<()> {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let ore: OpenRacingError = io_err.into();
        assert_eq!(ore.category(), ErrorCategory::IO);
        assert!(ore.to_string().contains("file not found"));
        Ok(())
    }

    #[test]
    fn io_error_various_kinds() -> Result<()> {
        let kinds = [
            std::io::ErrorKind::NotFound,
            std::io::ErrorKind::PermissionDenied,
            std::io::ErrorKind::TimedOut,
            std::io::ErrorKind::WouldBlock,
            std::io::ErrorKind::BrokenPipe,
        ];
        for kind in &kinds {
            let io_err = std::io::Error::new(*kind, "test");
            let ore: OpenRacingError = io_err.into();
            assert_eq!(ore.category(), ErrorCategory::IO);
            assert_eq!(ore.severity(), ErrorSeverity::Error);
        }
        Ok(())
    }

    #[test]
    fn question_mark_operator_propagation() {
        fn might_fail_rt() -> std::result::Result<(), RTError> {
            Err(RTError::TimingViolation)
        }
        fn caller() -> Result<()> {
            might_fail_rt()?;
            Ok(())
        }
        let result = caller();
        assert!(result.is_err());
        if let Err(e) = result {
            assert_eq!(e.category(), ErrorCategory::RT);
        }
    }

    #[test]
    fn question_mark_io_propagation() {
        fn might_fail_io() -> std::result::Result<(), std::io::Error> {
            Err(std::io::Error::other("disk error"))
        }
        fn caller() -> Result<()> {
            might_fail_io()?;
            Ok(())
        }
        let result = caller();
        assert!(result.is_err());
        if let Err(e) = result {
            assert_eq!(e.category(), ErrorCategory::IO);
        }
    }
}

// ---------------------------------------------------------------------------
// 3. Error source chain (std::error::Error::source())
// ---------------------------------------------------------------------------

mod error_source_chain {
    use super::*;
    use std::error::Error;

    #[test]
    fn rt_error_has_no_source() -> Result<()> {
        let err = RTError::DeviceDisconnected;
        assert!(err.source().is_none(), "RTError should have no source");
        Ok(())
    }

    #[test]
    fn device_error_has_no_source() -> Result<()> {
        let err = DeviceError::not_found("test");
        assert!(err.source().is_none(), "DeviceError should have no source");
        Ok(())
    }

    #[test]
    fn profile_error_has_no_source() -> Result<()> {
        let err = ProfileError::not_found("test");
        assert!(err.source().is_none(), "ProfileError should have no source");
        Ok(())
    }

    #[test]
    fn validation_error_has_no_source() -> Result<()> {
        let err = ValidationError::required("test");
        assert!(
            err.source().is_none(),
            "ValidationError should have no source"
        );
        Ok(())
    }

    #[test]
    fn openracing_error_rt_source_is_rt_error() -> Result<()> {
        let ore: OpenRacingError = RTError::TimingViolation.into();
        let source = ore.source();
        assert!(source.is_some(), "Wrapped RT error should have source");
        let source = source.ok_or(OpenRacingError::other("expected source"))?;
        // The source's display should match the inner RTError
        assert_eq!(source.to_string(), "Real-time timing violation");
        Ok(())
    }

    #[test]
    fn openracing_error_device_source_is_device_error() -> Result<()> {
        let ore: OpenRacingError = DeviceError::not_found("test-dev").into();
        let source = ore.source();
        assert!(source.is_some(), "Wrapped Device error should have source");
        let source = source.ok_or(OpenRacingError::other("expected source"))?;
        assert!(source.to_string().contains("test-dev"));
        Ok(())
    }

    #[test]
    fn openracing_error_io_source_is_io_error() -> Result<()> {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "missing file");
        let ore: OpenRacingError = io_err.into();
        let source = ore.source();
        assert!(source.is_some(), "Wrapped IO error should have source");
        let source = source.ok_or(OpenRacingError::other("expected source"))?;
        assert!(source.to_string().contains("missing file"));
        Ok(())
    }

    #[test]
    fn openracing_error_config_has_no_source() -> Result<()> {
        let ore = OpenRacingError::config("bad config");
        assert!(ore.source().is_none(), "Config error should have no source");
        Ok(())
    }

    #[test]
    fn openracing_error_other_has_no_source() -> Result<()> {
        let ore = OpenRacingError::other("something");
        assert!(ore.source().is_none(), "Other error should have no source");
        Ok(())
    }

    #[test]
    fn error_chain_walkable() -> Result<()> {
        let ore: OpenRacingError = RTError::PipelineFault.into();
        let mut chain_len = 0u32;
        let mut current: Option<&dyn Error> = Some(&ore);
        while let Some(err) = current {
            chain_len += 1;
            current = err.source();
        }
        // OpenRacingError::RT -> RTError (2 in chain)
        assert_eq!(chain_len, 2, "Expected chain of length 2");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 4. ErrorCategory and ErrorSeverity
// ---------------------------------------------------------------------------

mod category_and_severity {
    use super::*;

    #[test]
    fn all_categories_display() -> Result<()> {
        let cases = [
            (ErrorCategory::RT, "RT"),
            (ErrorCategory::Device, "Device"),
            (ErrorCategory::Profile, "Profile"),
            (ErrorCategory::Config, "Config"),
            (ErrorCategory::IO, "IO"),
            (ErrorCategory::Validation, "Validation"),
            (ErrorCategory::Plugin, "Plugin"),
            (ErrorCategory::Telemetry, "Telemetry"),
            (ErrorCategory::Other, "Other"),
        ];
        for (cat, expected) in &cases {
            assert_eq!(cat.to_string(), *expected);
        }
        Ok(())
    }

    #[test]
    fn all_severities_display() -> Result<()> {
        let cases = [
            (ErrorSeverity::Info, "INFO"),
            (ErrorSeverity::Warning, "WARN"),
            (ErrorSeverity::Error, "ERROR"),
            (ErrorSeverity::Critical, "CRITICAL"),
        ];
        for (sev, expected) in &cases {
            assert_eq!(sev.to_string(), *expected);
        }
        Ok(())
    }

    #[test]
    fn severity_total_order() -> Result<()> {
        assert!(ErrorSeverity::Info < ErrorSeverity::Warning);
        assert!(ErrorSeverity::Warning < ErrorSeverity::Error);
        assert!(ErrorSeverity::Error < ErrorSeverity::Critical);
        // Reflexive
        assert_eq!(ErrorSeverity::Info, ErrorSeverity::Info);
        assert_eq!(ErrorSeverity::Critical, ErrorSeverity::Critical);
        Ok(())
    }

    #[test]
    fn category_equality_and_clone() -> Result<()> {
        let cat = ErrorCategory::Device;
        let cloned = cat;
        assert_eq!(cat, cloned);
        Ok(())
    }

    #[test]
    fn category_repr_values() -> Result<()> {
        assert_eq!(ErrorCategory::RT as u8, 0);
        assert_eq!(ErrorCategory::Device as u8, 1);
        assert_eq!(ErrorCategory::Profile as u8, 2);
        assert_eq!(ErrorCategory::Config as u8, 3);
        assert_eq!(ErrorCategory::IO as u8, 4);
        assert_eq!(ErrorCategory::Validation as u8, 5);
        assert_eq!(ErrorCategory::Plugin as u8, 6);
        assert_eq!(ErrorCategory::Telemetry as u8, 7);
        assert_eq!(ErrorCategory::Other as u8, 255);
        Ok(())
    }

    #[test]
    fn severity_repr_values() -> Result<()> {
        assert_eq!(ErrorSeverity::Info as u8, 0);
        assert_eq!(ErrorSeverity::Warning as u8, 1);
        assert_eq!(ErrorSeverity::Error as u8, 2);
        assert_eq!(ErrorSeverity::Critical as u8, 3);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 5. OpenRacingError::is_recoverable
// ---------------------------------------------------------------------------

mod recoverability {
    use super::*;

    #[test]
    fn recoverable_errors() -> Result<()> {
        // Warning-severity errors are recoverable
        let timing: OpenRacingError = RTError::TimingViolation.into();
        assert!(timing.is_recoverable());
        let buffer: OpenRacingError = RTError::BufferOverflow.into();
        assert!(buffer.is_recoverable());
        // Error-severity: recoverable (below Critical)
        let config = OpenRacingError::config("x");
        assert!(config.is_recoverable());
        let other = OpenRacingError::other("y");
        assert!(other.is_recoverable());
        // Io errors have Error severity → recoverable
        let io: OpenRacingError = std::io::Error::other("disk").into();
        assert!(io.is_recoverable());
        Ok(())
    }

    #[test]
    fn non_recoverable_errors() -> Result<()> {
        let critical_rt = [
            RTError::DeviceDisconnected,
            RTError::TorqueLimit,
            RTError::RTSetupFailed,
            RTError::SafetyInterlock,
            RTError::DeadlineMissed,
        ];
        for rt in &critical_rt {
            let ore: OpenRacingError = (*rt).into();
            assert!(
                !ore.is_recoverable(),
                "RTError::{rt:?} should not be recoverable"
            );
        }

        let disconnected: OpenRacingError = DeviceError::disconnected("dev").into();
        assert!(!disconnected.is_recoverable());
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 6. RTError numeric code round-trip exhaustive
// ---------------------------------------------------------------------------

mod rt_error_codes {
    use super::*;

    #[test]
    fn all_known_codes_roundtrip() -> Result<()> {
        for code in 1..=11u8 {
            let err = RTError::from_code(code);
            assert!(
                err.is_some(),
                "Code {code} should map to a known RTError variant"
            );
            let err = err.ok_or(OpenRacingError::other("missing variant"))?;
            assert_eq!(err.code(), code, "Round-trip failed for code {code}");
        }
        Ok(())
    }

    #[test]
    fn unknown_codes_return_none() -> Result<()> {
        for code in [0u8, 12, 50, 100, 128, 255] {
            assert!(
                RTError::from_code(code).is_none(),
                "Code {code} should return None"
            );
        }
        Ok(())
    }

    #[test]
    fn rt_error_is_copy() -> Result<()> {
        let err = RTError::PipelineFault;
        let copy = err;
        assert_eq!(err, copy);
        Ok(())
    }

    #[test]
    fn rt_error_safety_action_classification() -> Result<()> {
        let requires = [
            RTError::DeviceDisconnected,
            RTError::TorqueLimit,
            RTError::SafetyInterlock,
            RTError::DeadlineMissed,
        ];
        let does_not = [
            RTError::PipelineFault,
            RTError::TimingViolation,
            RTError::RTSetupFailed,
            RTError::InvalidConfig,
            RTError::BufferOverflow,
            RTError::ResourceUnavailable,
        ];
        for rt in &requires {
            assert!(
                rt.requires_safety_action(),
                "RTError::{rt:?} should require safety action"
            );
        }
        for rt in &does_not {
            assert!(
                !rt.requires_safety_action(),
                "RTError::{rt:?} should NOT require safety action"
            );
        }
        Ok(())
    }

    #[test]
    fn rt_error_recoverability_classification() -> Result<()> {
        let recoverable = [
            RTError::TimingViolation,
            RTError::BufferOverflow,
            RTError::ResourceUnavailable,
        ];
        let not_recoverable = [
            RTError::DeviceDisconnected,
            RTError::TorqueLimit,
            RTError::PipelineFault,
            RTError::RTSetupFailed,
            RTError::InvalidConfig,
            RTError::SafetyInterlock,
            RTError::DeadlineMissed,
        ];
        for rt in &recoverable {
            assert!(rt.is_recoverable(), "RTError::{rt:?} should be recoverable");
        }
        for rt in &not_recoverable {
            assert!(
                !rt.is_recoverable(),
                "RTError::{rt:?} should NOT be recoverable"
            );
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 7. ErrorContext
// ---------------------------------------------------------------------------

mod error_context_tests {
    use super::*;

    #[test]
    fn empty_context() -> Result<()> {
        let ctx = ErrorContext::new("op");
        let msg = ctx.to_string();
        assert!(msg.contains("operation: op"));
        Ok(())
    }

    #[test]
    fn context_with_multiple_kvs() -> Result<()> {
        let ctx = ErrorContext::new("load")
            .with("file", "config.yaml")
            .with("line", "42")
            .with("reason", "parse error");
        let msg = ctx.to_string();
        assert!(msg.contains("load"));
        assert!(msg.contains("file"));
        assert!(msg.contains("config.yaml"));
        assert!(msg.contains("line"));
        assert!(msg.contains("42"));
        assert!(msg.contains("reason"));
        assert!(msg.contains("parse error"));
        Ok(())
    }

    #[test]
    fn context_with_location() -> Result<()> {
        let ctx = ErrorContext::new("save").at("service.rs", 99);
        let msg = ctx.to_string();
        assert!(msg.contains("service.rs:99"));
        Ok(())
    }

    #[test]
    fn context_without_location_has_no_at() -> Result<()> {
        let ctx = ErrorContext::new("test_op");
        let msg = ctx.to_string();
        assert!(!msg.contains(" at "));
        Ok(())
    }

    #[test]
    fn context_is_clonable() -> Result<()> {
        let ctx = ErrorContext::new("op").with("k", "v").at("f.rs", 1);
        let cloned = ctx.clone();
        assert_eq!(ctx.to_string(), cloned.to_string());
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 8. ResultExt trait
// ---------------------------------------------------------------------------

mod result_ext_tests {
    use super::*;

    #[test]
    fn with_context_wraps_error_message() -> Result<()> {
        let result: std::result::Result<(), RTError> = Err(RTError::PipelineFault);
        let wrapped = result.with_context("frame_processing");
        assert!(wrapped.is_err());
        if let Err(e) = wrapped {
            let msg = e.to_string();
            assert!(msg.contains("frame_processing"));
            assert!(msg.contains("Pipeline processing fault"));
        }
        Ok(())
    }

    #[test]
    fn context_wraps_with_full_context() -> Result<()> {
        let result: std::result::Result<(), DeviceError> = Err(DeviceError::not_found("wheel"));
        let ctx = ErrorContext::new("device_scan")
            .with("port", "USB-1")
            .at("scanner.rs", 55);
        let wrapped = result.context(ctx);
        assert!(wrapped.is_err());
        if let Err(e) = wrapped {
            let msg = e.to_string();
            assert!(msg.contains("device_scan"));
            assert!(msg.contains("USB-1"));
            assert!(msg.contains("scanner.rs:55"));
        }
        Ok(())
    }

    #[test]
    fn with_context_on_ok_passes_through() -> Result<()> {
        let result: std::result::Result<u32, RTError> = Ok(42);
        let wrapped = result.with_context("should_not_wrap");
        assert!(wrapped.is_ok());
        let val = wrapped.map_err(|_| OpenRacingError::other("should not reach"))?;
        assert_eq!(val, 42);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 9. RTResult type alias
// ---------------------------------------------------------------------------

mod rt_result_tests {
    use super::*;

    #[test]
    fn rt_result_unit_default() -> Result<()> {
        fn check_timing() -> RTResult {
            Ok(())
        }
        assert!(check_timing().is_ok());
        Ok(())
    }

    #[test]
    fn rt_result_with_value() -> Result<()> {
        fn compute() -> RTResult<f32> {
            Ok(0.5)
        }
        let val = compute().map_err(|e| -> OpenRacingError { e.into() })?;
        assert!((val - 0.5_f32).abs() < f32::EPSILON);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 10. Macros
// ---------------------------------------------------------------------------

mod macro_tests {
    use super::*;
    use openracing_errors::{error_context, require, validate, validate_range};

    #[test]
    fn error_context_macro_empty() -> Result<()> {
        let ctx = error_context!("op",);
        assert!(ctx.to_string().contains("op"));
        Ok(())
    }

    #[test]
    fn error_context_macro_with_trailing_comma() -> Result<()> {
        let ctx = error_context!("op", "k1" => "v1", "k2" => "v2",);
        let msg = ctx.to_string();
        assert!(msg.contains("k1"));
        assert!(msg.contains("v2"));
        Ok(())
    }

    #[test]
    fn require_macro_returns_validation_error() -> Result<()> {
        let err = require!("profile_name");
        assert_eq!(err.to_string(), "Required field 'profile_name' is missing");
        Ok(())
    }

    #[test]
    fn validate_macro_passes_on_true() -> Result<()> {
        fn check() -> Result<()> {
            validate!(true, ValidationError::required("field"));
            Ok(())
        }
        assert!(check().is_ok());
        Ok(())
    }

    #[test]
    fn validate_macro_fails_on_false() -> Result<()> {
        fn check() -> Result<()> {
            validate!(false, ValidationError::required("field"));
            Ok(())
        }
        assert!(check().is_err());
        Ok(())
    }

    #[test]
    fn validate_range_in_bounds() -> Result<()> {
        #[allow(clippy::manual_range_contains)]
        fn check(v: f32) -> Result<()> {
            validate_range!("gain", v, 0.0_f32, 1.0_f32);
            Ok(())
        }
        assert!(check(0.5).is_ok());
        assert!(check(0.0).is_ok());
        assert!(check(1.0).is_ok());
        Ok(())
    }

    #[test]
    fn validate_range_out_of_bounds() -> Result<()> {
        #[allow(clippy::manual_range_contains)]
        fn check(v: f32) -> Result<()> {
            validate_range!("gain", v, 0.0_f32, 1.0_f32);
            Ok(())
        }
        assert!(check(-0.1).is_err());
        assert!(check(1.1).is_err());
        Ok(())
    }
}

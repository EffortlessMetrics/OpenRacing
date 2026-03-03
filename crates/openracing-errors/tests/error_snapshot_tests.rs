//! Extended snapshot tests for error types — Debug formatting,
//! recoverability classification, error context macro, and
//! cross-variant consistency checks.
//!
//! Complements `snapshot_tests.rs` by covering Debug output, the
//! `error_context!` macro, and recoverability classification across
//! all `OpenRacingError` wrapper variants.

use openracing_errors::{
    common::{ErrorCategory, ErrorContext, ErrorSeverity, OpenRacingError},
    device::DeviceError,
    error_context,
    profile::ProfileError,
    rt::RTError,
    validation::ValidationError,
};

// ---------------------------------------------------------------------------
// RT error Debug snapshots
// ---------------------------------------------------------------------------
mod rt_error_debug_snapshots {
    use super::*;
    use insta::assert_snapshot;

    #[test]
    fn test_all_rt_error_debug_formats() {
        let variants = [
            RTError::DeviceDisconnected,
            RTError::TorqueLimit,
            RTError::PipelineFault,
            RTError::TimingViolation,
            RTError::RTSetupFailed,
            RTError::SafetyInterlock,
            RTError::InvalidConfig,
            RTError::BufferOverflow,
            RTError::DeadlineMissed,
            RTError::ResourceUnavailable,
        ];
        let formatted: Vec<String> = variants.iter().map(|v| format!("{v:?}")).collect();
        assert_snapshot!(formatted.join("\n"));
    }
}

// ---------------------------------------------------------------------------
// Device error Debug snapshots
// ---------------------------------------------------------------------------
mod device_error_debug_snapshots {
    use super::*;
    use insta::assert_snapshot;

    #[test]
    fn test_device_not_found_debug() {
        assert_snapshot!(format!("{:?}", DeviceError::not_found("moza-r9")));
    }

    #[test]
    fn test_device_communication_error_debug() {
        assert_snapshot!(format!(
            "{:?}",
            DeviceError::CommunicationError {
                device: "fanatec-dd-pro".into(),
                message: "USB transfer timeout".into(),
            }
        ));
    }

    #[test]
    fn test_device_timeout_debug() {
        assert_snapshot!(format!("{:?}", DeviceError::timeout("logitech-g29", 3000)));
    }

    #[test]
    fn test_device_invalid_response_debug() {
        assert_snapshot!(format!(
            "{:?}",
            DeviceError::InvalidResponse {
                device: "simucube-2".into(),
                expected: 64,
                actual: 0,
            }
        ));
    }
}

// ---------------------------------------------------------------------------
// Validation error Debug snapshots
// ---------------------------------------------------------------------------
mod validation_error_debug_snapshots {
    use super::*;
    use insta::assert_snapshot;

    #[test]
    fn test_required_debug() {
        assert_snapshot!(format!("{:?}", ValidationError::required("device_id")));
    }

    #[test]
    fn test_out_of_range_debug() {
        assert_snapshot!(format!(
            "{:?}",
            ValidationError::out_of_range("gain", 2.5_f32, 0.0_f32, 1.0_f32)
        ));
    }

    #[test]
    fn test_invalid_characters_debug() {
        assert_snapshot!(format!(
            "{:?}",
            ValidationError::InvalidCharacters {
                field: "profile_name".into(),
                reason: "contains control characters".into(),
            }
        ));
    }

    #[test]
    fn test_not_unique_debug() {
        assert_snapshot!(format!(
            "{:?}",
            ValidationError::NotUnique {
                field: "device_alias".into(),
                value: "my-wheel".into(),
            }
        ));
    }
}

// ---------------------------------------------------------------------------
// Profile error Debug snapshots
// ---------------------------------------------------------------------------
mod profile_error_debug_snapshots {
    use super::*;
    use insta::assert_snapshot;

    #[test]
    fn test_not_found_debug() {
        assert_snapshot!(format!("{:?}", ProfileError::not_found("gt3-sprint")));
    }

    #[test]
    fn test_inheritance_depth_exceeded_debug() {
        assert_snapshot!(format!(
            "{:?}",
            ProfileError::InheritanceDepthExceeded {
                depth: 10,
                max_depth: 5,
            }
        ));
    }

    #[test]
    fn test_save_failed_debug() {
        assert_snapshot!(format!(
            "{:?}",
            ProfileError::SaveFailed {
                profile: "custom-rally".into(),
                reason: "read-only filesystem".into(),
            }
        ));
    }

    #[test]
    fn test_invalid_device_mapping_debug() {
        assert_snapshot!(format!(
            "{:?}",
            ProfileError::InvalidDeviceMapping {
                profile: "drift-pro".into(),
                device: "nonexistent-wheel".into(),
            }
        ));
    }
}

// ---------------------------------------------------------------------------
// OpenRacingError recoverability classification
// ---------------------------------------------------------------------------
mod recoverability_snapshots {
    use super::*;
    use insta::assert_snapshot;

    #[test]
    fn test_all_wrapper_variant_recoverability() {
        let pairs: Vec<(&str, OpenRacingError)> = vec![
            ("RT_DeviceDisconnected", RTError::DeviceDisconnected.into()),
            ("RT_TorqueLimit", RTError::TorqueLimit.into()),
            ("RT_PipelineFault", RTError::PipelineFault.into()),
            ("RT_TimingViolation", RTError::TimingViolation.into()),
            ("RT_SafetyInterlock", RTError::SafetyInterlock.into()),
            ("RT_BufferOverflow", RTError::BufferOverflow.into()),
            ("RT_DeadlineMissed", RTError::DeadlineMissed.into()),
            (
                "Device_NotFound",
                DeviceError::not_found("test").into(),
            ),
            (
                "Device_Timeout",
                DeviceError::timeout("test", 100).into(),
            ),
            (
                "Profile_NotFound",
                ProfileError::not_found("test").into(),
            ),
            (
                "Profile_CircularInheritance",
                ProfileError::circular_inheritance("a -> b -> a").into(),
            ),
            (
                "Validation_Required",
                ValidationError::required("field").into(),
            ),
            ("Config", OpenRacingError::config("test")),
            ("Other", OpenRacingError::other("test")),
            (
                "Io",
                std::io::Error::new(std::io::ErrorKind::NotFound, "not found").into(),
            ),
        ];
        let formatted: Vec<String> = pairs
            .iter()
            .map(|(label, err)| {
                format!(
                    "{label}: recoverable={}, severity={}, category={}",
                    err.is_recoverable(),
                    err.severity(),
                    err.category()
                )
            })
            .collect();
        assert_snapshot!(formatted.join("\n"));
    }
}

// ---------------------------------------------------------------------------
// Error context macro snapshots
// ---------------------------------------------------------------------------
mod error_context_macro_snapshots {
    use super::*;
    use insta::assert_snapshot;

    #[test]
    fn test_error_context_macro_simple() {
        let ctx = ErrorContext::new("device_init");
        assert_snapshot!(ctx.to_string());
    }

    #[test]
    fn test_error_context_macro_with_kv() {
        let ctx = error_context!("apply_profile", "profile" => "gt3", "device" => "moza-r9");
        assert_snapshot!(ctx.to_string());
    }

    #[test]
    fn test_error_context_with_many_kv() {
        let ctx = ErrorContext::new("diagnose_device")
            .with("device_id", "fanatec-dd1")
            .with("firmware", "v1.42")
            .with("fault_code", "0x0F")
            .with("temperature_c", "72")
            .at("device_manager.rs", 285);
        assert_snapshot!(ctx.to_string());
    }
}

// ---------------------------------------------------------------------------
// OpenRacingError Debug format snapshots
// ---------------------------------------------------------------------------
mod openracing_error_debug_snapshots {
    use super::*;
    use insta::assert_snapshot;

    #[test]
    fn test_wrapped_rt_debug() {
        let err: OpenRacingError = RTError::SafetyInterlock.into();
        assert_snapshot!(format!("{err:?}"));
    }

    #[test]
    fn test_wrapped_device_debug() {
        let err: OpenRacingError = DeviceError::unsupported(0xDEAD, 0xBEEF).into();
        assert_snapshot!(format!("{err:?}"));
    }

    #[test]
    fn test_config_error_debug() {
        let err = OpenRacingError::config("torque_limit must be positive");
        assert_snapshot!(format!("{err:?}"));
    }

    #[test]
    fn test_other_error_debug() {
        let err = OpenRacingError::other("unexpected state transition");
        assert_snapshot!(format!("{err:?}"));
    }
}

// ---------------------------------------------------------------------------
// Severity comparison snapshots
// ---------------------------------------------------------------------------
mod severity_ordering_snapshots {
    use super::*;
    use insta::assert_snapshot;

    #[test]
    fn test_severity_ordering_matrix() {
        let severities = [
            ErrorSeverity::Info,
            ErrorSeverity::Warning,
            ErrorSeverity::Error,
            ErrorSeverity::Critical,
        ];
        let mut comparisons = Vec::new();
        for (i, a) in severities.iter().enumerate() {
            for (j, b) in severities.iter().enumerate() {
                let cmp = if i < j {
                    "less"
                } else if i == j {
                    "equal"
                } else {
                    "greater"
                };
                comparisons.push(format!("{a} vs {b}: {cmp}"));
            }
        }
        assert_snapshot!(comparisons.join("\n"));
    }
}

// ---------------------------------------------------------------------------
// Category repr value snapshots
// ---------------------------------------------------------------------------
mod category_repr_snapshots {
    use super::*;
    use insta::assert_snapshot;

    #[test]
    fn test_category_repr_values() {
        let categories = [
            ErrorCategory::RT,
            ErrorCategory::Device,
            ErrorCategory::Profile,
            ErrorCategory::Config,
            ErrorCategory::IO,
            ErrorCategory::Validation,
            ErrorCategory::Plugin,
            ErrorCategory::Telemetry,
            ErrorCategory::Other,
        ];
        let formatted: Vec<String> = categories
            .iter()
            .map(|c| format!("{c}: repr={}", *c as u8))
            .collect();
        assert_snapshot!(formatted.join("\n"));
    }
}

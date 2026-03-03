//! Deep tests for error handling across the OpenRacing error hierarchy.
//!
//! Covers:
//! - Display impl for every error variant
//! - Error conversion chains (From impls)
//! - Error context preservation through layers
//! - Error categorization (recoverable vs fatal)
//! - Error serialization round-trip for IPC transport
//! - Error downcasting
//! - RT error code round-trips
//! - Macro-generated errors

use openracing_errors::{
    DeviceError, ErrorCategory, ErrorContext, ErrorSeverity, OpenRacingError, ProfileError,
    RTError, ResultExt, ValidationError, error_context,
};

// ---------------------------------------------------------------------------
// 1. Display impl for every variant
// ---------------------------------------------------------------------------

#[test]
fn display_all_rt_error_variants() {
    let variants: &[RTError] = &[
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
        assert!(!msg.is_empty(), "RTError::{variant:?} has empty Display");
    }
}

#[test]
fn display_all_device_error_variants() {
    let variants: Vec<DeviceError> = vec![
        DeviceError::not_found("dev"),
        DeviceError::disconnected("dev"),
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
        DeviceError::timeout("dev", 500),
        DeviceError::unsupported(0x1234, 0x5678),
        DeviceError::Busy("dev".into()),
        DeviceError::PermissionDenied("dev".into()),
        DeviceError::InitializationFailed {
            device: "dev".into(),
            reason: "reason".into(),
        },
        DeviceError::FirmwareError {
            device: "dev".into(),
            message: "fw".into(),
        },
        DeviceError::FeatureNotSupported {
            device: "dev".into(),
            feature: "feat".into(),
        },
    ];
    for variant in &variants {
        let msg = variant.to_string();
        assert!(
            !msg.is_empty(),
            "DeviceError::{variant:?} has empty Display"
        );
    }
}

#[test]
fn display_all_profile_error_variants() {
    let variants: Vec<ProfileError> = vec![
        ProfileError::not_found("p"),
        ProfileError::AlreadyExists("p".into()),
        ProfileError::invalid_format("path", "reason"),
        ProfileError::ValidationFailed("msg".into()),
        ProfileError::SaveFailed {
            profile: "p".into(),
            reason: "r".into(),
        },
        ProfileError::LoadFailed {
            path: "path".into(),
            reason: "r".into(),
        },
        ProfileError::circular_inheritance("a -> b -> a"),
        ProfileError::InheritanceDepthExceeded {
            depth: 10,
            max_depth: 5,
        },
        ProfileError::ParentNotFound {
            parent_id: "parent".into(),
        },
        ProfileError::InvalidId("bad-id".into()),
        ProfileError::Conflict("conflict".into()),
        ProfileError::version_mismatch("2.0", "1.0"),
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
    for variant in &variants {
        let msg = variant.to_string();
        assert!(
            !msg.is_empty(),
            "ProfileError::{variant:?} has empty Display"
        );
    }
}

#[test]
fn display_all_validation_error_variants() {
    let variants: Vec<ValidationError> = vec![
        ValidationError::out_of_range("f", 2.0_f32, 0.0_f32, 1.0_f32),
        ValidationError::required("f"),
        ValidationError::invalid_format("f", "reason"),
        ValidationError::too_long("f", 100, 50),
        ValidationError::too_short("f", 1, 5),
        ValidationError::invalid_enum("f", "bad", "a, b, c"),
        ValidationError::constraint("msg"),
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
            actual: "string".into(),
        },
        ValidationError::NumericOverflow {
            field: "f".into(),
        },
        ValidationError::custom("custom msg"),
    ];
    for variant in &variants {
        let msg = variant.to_string();
        assert!(
            !msg.is_empty(),
            "ValidationError::{variant:?} has empty Display"
        );
    }
}

#[test]
fn display_all_openracing_error_variants() {
    let variants: Vec<OpenRacingError> = vec![
        RTError::TimingViolation.into(),
        DeviceError::not_found("dev").into(),
        ProfileError::not_found("p").into(),
        ValidationError::required("f").into(),
        OpenRacingError::from(std::io::Error::new(std::io::ErrorKind::NotFound, "file")),
        OpenRacingError::config("cfg msg"),
        OpenRacingError::other("other msg"),
    ];
    for variant in &variants {
        let msg = variant.to_string();
        assert!(!msg.is_empty(), "OpenRacingError has empty Display");
    }
}

// ---------------------------------------------------------------------------
// 2. Error conversion chains (From impls)
// ---------------------------------------------------------------------------

#[test]
fn rt_error_converts_to_openracing_error() {
    let rt = RTError::PipelineFault;
    let ore: OpenRacingError = rt.into();
    assert_eq!(ore.category(), ErrorCategory::RT);
}

#[test]
fn device_error_converts_to_openracing_error() {
    let dev = DeviceError::not_found("moza-r9");
    let ore: OpenRacingError = dev.into();
    assert_eq!(ore.category(), ErrorCategory::Device);
}

#[test]
fn profile_error_converts_to_openracing_error() {
    let prof = ProfileError::not_found("iracing-gt3");
    let ore: OpenRacingError = prof.into();
    assert_eq!(ore.category(), ErrorCategory::Profile);
}

#[test]
fn validation_error_converts_to_openracing_error() {
    let val = ValidationError::required("gain");
    let ore: OpenRacingError = val.into();
    assert_eq!(ore.category(), ErrorCategory::Validation);
}

#[test]
fn io_error_converts_to_openracing_error() {
    let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
    let ore: OpenRacingError = io_err.into();
    assert_eq!(ore.category(), ErrorCategory::IO);
    assert!(ore.to_string().contains("access denied"));
}

#[test]
fn conversion_chain_validation_through_openracing() -> Result<(), OpenRacingError> {
    fn inner() -> Result<(), ValidationError> {
        Err(ValidationError::required("name"))
    }
    // Verify the ? operator chains: ValidationError -> OpenRacingError
    let result: Result<(), OpenRacingError> = inner().map_err(OpenRacingError::from);
    assert!(result.is_err());
    if let Err(e) = result {
        assert_eq!(e.category(), ErrorCategory::Validation);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 3. Error context preservation through layers
// ---------------------------------------------------------------------------

#[test]
fn error_context_preserves_operation_name() {
    let ctx = ErrorContext::new("apply_profile");
    assert!(ctx.to_string().contains("apply_profile"));
}

#[test]
fn error_context_preserves_key_value_pairs() {
    let ctx = ErrorContext::new("load")
        .with("profile_id", "gt3")
        .with("device", "moza-r9");
    let display = ctx.to_string();
    assert!(display.contains("profile_id"));
    assert!(display.contains("gt3"));
    assert!(display.contains("device"));
    assert!(display.contains("moza-r9"));
}

#[test]
fn error_context_preserves_location() {
    let ctx = ErrorContext::new("load").at("profiles.rs", 42);
    assert!(ctx.to_string().contains("profiles.rs:42"));
}

#[test]
fn result_ext_with_context_preserves_original_error() {
    let result: Result<(), RTError> = Err(RTError::TimingViolation);
    let with_ctx = result.with_context("processing_frame");
    if let Err(e) = with_ctx {
        let msg = e.to_string();
        assert!(msg.contains("processing_frame"));
        assert!(msg.contains("timing violation") || msg.contains("Timing"));
    }
}

#[test]
fn result_ext_context_with_full_context_object() {
    let result: Result<(), DeviceError> = Err(DeviceError::timeout("moza-r9", 500));
    let ctx = ErrorContext::new("write_torque")
        .with("device", "moza-r9")
        .with("attempt", "3");
    let with_ctx = result.context(ctx);
    if let Err(e) = with_ctx {
        let msg = e.to_string();
        assert!(msg.contains("write_torque"));
        assert!(msg.contains("moza-r9"));
    }
}

#[test]
fn error_context_macro_creates_context() {
    let ctx = error_context!("apply_profile", "profile" => "gt3", "device" => "moza-r9");
    let display = ctx.to_string();
    assert!(display.contains("apply_profile"));
    assert!(display.contains("gt3"));
    assert!(display.contains("moza-r9"));
}

// ---------------------------------------------------------------------------
// 4. Error categorization (recoverable vs fatal)
// ---------------------------------------------------------------------------

#[test]
fn critical_rt_errors_are_not_recoverable() {
    let non_recoverable = [
        RTError::DeviceDisconnected,
        RTError::TorqueLimit,
        RTError::RTSetupFailed,
        RTError::SafetyInterlock,
        RTError::DeadlineMissed,
    ];
    for err in non_recoverable {
        let ore: OpenRacingError = err.into();
        assert!(
            !ore.is_recoverable(),
            "RTError::{err:?} should not be recoverable"
        );
        assert_eq!(ore.severity(), ErrorSeverity::Critical);
    }
}

#[test]
fn warning_rt_errors_are_recoverable() {
    let recoverable = [RTError::TimingViolation, RTError::BufferOverflow];
    for err in recoverable {
        let ore: OpenRacingError = err.into();
        assert!(
            ore.is_recoverable(),
            "RTError::{err:?} should be recoverable"
        );
    }
}

#[test]
fn config_errors_are_recoverable() {
    let err = OpenRacingError::config("bad value");
    assert!(err.is_recoverable());
    assert_eq!(err.severity(), ErrorSeverity::Error);
}

#[test]
fn device_disconnected_is_critical() {
    let err: OpenRacingError = DeviceError::disconnected("moza-r9").into();
    assert_eq!(err.severity(), ErrorSeverity::Critical);
    assert!(!err.is_recoverable());
}

#[test]
fn device_timeout_is_retryable_and_recoverable() {
    let dev = DeviceError::timeout("moza-r9", 500);
    assert!(dev.is_retryable());
    let ore: OpenRacingError = dev.into();
    assert!(ore.is_recoverable());
}

// ---------------------------------------------------------------------------
// 5. Error serialization for IPC transport
// ---------------------------------------------------------------------------

#[test]
fn rt_error_round_trips_via_code() {
    for code in 1..=10u8 {
        let err = RTError::from_code(code);
        assert!(err.is_some(), "code {code} should map to an RTError");
        if let Some(e) = err {
            assert_eq!(e.code(), code);
        }
    }
}

#[test]
fn rt_error_from_invalid_code_returns_none() {
    assert!(RTError::from_code(0).is_none());
    assert!(RTError::from_code(11).is_none());
    assert!(RTError::from_code(255).is_none());
}

#[test]
fn error_display_is_stable_for_ipc_serialization() {
    // Error messages that appear in IPC/log output should be deterministic
    let err1 = DeviceError::timeout("moza-r9", 500);
    let err2 = DeviceError::timeout("moza-r9", 500);
    assert_eq!(err1.to_string(), err2.to_string());
}

#[test]
fn error_category_has_repr_u8_for_wire_format() {
    // Verify ErrorCategory repr values are stable
    assert_eq!(ErrorCategory::RT as u8, 0);
    assert_eq!(ErrorCategory::Device as u8, 1);
    assert_eq!(ErrorCategory::Profile as u8, 2);
    assert_eq!(ErrorCategory::Config as u8, 3);
    assert_eq!(ErrorCategory::IO as u8, 4);
    assert_eq!(ErrorCategory::Validation as u8, 5);
    assert_eq!(ErrorCategory::Plugin as u8, 6);
    assert_eq!(ErrorCategory::Telemetry as u8, 7);
    assert_eq!(ErrorCategory::Other as u8, 255);
}

#[test]
fn error_severity_has_repr_u8_for_wire_format() {
    assert_eq!(ErrorSeverity::Info as u8, 0);
    assert_eq!(ErrorSeverity::Warning as u8, 1);
    assert_eq!(ErrorSeverity::Error as u8, 2);
    assert_eq!(ErrorSeverity::Critical as u8, 3);
}

// ---------------------------------------------------------------------------
// 6. Error downcasting
// ---------------------------------------------------------------------------

#[test]
fn downcast_openracing_error_to_rt_error() {
    let ore: OpenRacingError = RTError::PipelineFault.into();
    match &ore {
        OpenRacingError::RT(rt) => assert_eq!(*rt, RTError::PipelineFault),
        other => panic!("expected RT variant, got {other:?}"),
    }
}

#[test]
fn downcast_openracing_error_to_device_error() {
    let ore: OpenRacingError = DeviceError::not_found("dev").into();
    match &ore {
        OpenRacingError::Device(DeviceError::NotFound(name)) => assert_eq!(name, "dev"),
        other => panic!("expected Device::NotFound variant, got {other:?}"),
    }
}

#[test]
fn downcast_through_std_error_trait() {
    let ore: OpenRacingError = RTError::TimingViolation.into();
    let dyn_err: &dyn std::error::Error = &ore;
    // Verify source chain works: OpenRacingError -> RTError
    let source = dyn_err.source();
    assert!(source.is_some(), "OpenRacingError::RT should have a source");
}

#[test]
fn rt_error_has_no_further_source() {
    let err = RTError::TimingViolation;
    let dyn_err: &dyn std::error::Error = &err;
    assert!(dyn_err.source().is_none(), "RTError should have no source");
}

// ---------------------------------------------------------------------------
// 7. Additional coverage
// ---------------------------------------------------------------------------

#[test]
fn error_severity_ordering_is_total() {
    let severities = [
        ErrorSeverity::Info,
        ErrorSeverity::Warning,
        ErrorSeverity::Error,
        ErrorSeverity::Critical,
    ];
    for i in 0..severities.len() {
        for j in (i + 1)..severities.len() {
            assert!(severities[i] < severities[j]);
        }
    }
}

#[test]
fn error_category_display_all_variants() {
    let categories = [
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
    for (cat, expected) in categories {
        assert_eq!(cat.to_string(), expected);
    }
}

#[test]
fn error_severity_display_all_variants() {
    let severities = [
        (ErrorSeverity::Info, "INFO"),
        (ErrorSeverity::Warning, "WARN"),
        (ErrorSeverity::Error, "ERROR"),
        (ErrorSeverity::Critical, "CRITICAL"),
    ];
    for (sev, expected) in severities {
        assert_eq!(sev.to_string(), expected);
    }
}

#[test]
fn device_error_unavailable_classification() {
    assert!(DeviceError::not_found("dev").is_device_unavailable());
    assert!(DeviceError::disconnected("dev").is_device_unavailable());
    assert!(DeviceError::PermissionDenied("dev".into()).is_device_unavailable());
    // All other variants should NOT be classified as unavailable
    assert!(!DeviceError::timeout("dev", 100).is_device_unavailable());
    assert!(!DeviceError::Busy("dev".into()).is_device_unavailable());
    assert!(!DeviceError::HidError("err".into()).is_device_unavailable());
}

#[test]
fn profile_error_inheritance_classification() {
    assert!(ProfileError::circular_inheritance("a -> b -> a").is_inheritance_error());
    assert!(
        ProfileError::InheritanceDepthExceeded {
            depth: 10,
            max_depth: 5
        }
        .is_inheritance_error()
    );
    assert!(
        ProfileError::ParentNotFound {
            parent_id: "p".into()
        }
        .is_inheritance_error()
    );
    assert!(!ProfileError::not_found("p").is_inheritance_error());
    assert!(!ProfileError::Locked("p".into()).is_inheritance_error());
}

#[test]
fn profile_error_storage_classification() {
    assert!(
        ProfileError::SaveFailed {
            profile: "p".into(),
            reason: "r".into()
        }
        .is_storage_error()
    );
    assert!(
        ProfileError::LoadFailed {
            path: "path".into(),
            reason: "r".into()
        }
        .is_storage_error()
    );
    assert!(!ProfileError::not_found("p").is_storage_error());
}

#[test]
fn rt_error_safety_action_classification() {
    let needs_action = [
        RTError::DeviceDisconnected,
        RTError::TorqueLimit,
        RTError::SafetyInterlock,
        RTError::DeadlineMissed,
    ];
    let no_action = [
        RTError::PipelineFault,
        RTError::TimingViolation,
        RTError::RTSetupFailed,
        RTError::InvalidConfig,
        RTError::BufferOverflow,
        RTError::ResourceUnavailable,
    ];
    for err in needs_action {
        assert!(
            err.requires_safety_action(),
            "RTError::{err:?} should require safety action"
        );
    }
    for err in no_action {
        assert!(
            !err.requires_safety_action(),
            "RTError::{err:?} should NOT require safety action"
        );
    }
}

#[test]
fn rt_error_is_copy() {
    let err = RTError::TimingViolation;
    let copy = err;
    assert_eq!(err, copy);
}

#[test]
fn validation_error_equality() {
    let a = ValidationError::required("name");
    let b = ValidationError::required("name");
    assert_eq!(a, b);

    let c = ValidationError::required("other");
    assert_ne!(a, c);
}

#[test]
fn error_context_empty_is_valid() {
    let ctx = ErrorContext::new("op");
    assert!(ctx.context.is_empty());
    assert!(ctx.location.is_none());
    assert!(ctx.to_string().contains("op"));
}

#[test]
fn openracing_error_implements_std_error() {
    fn assert_std_error<T: std::error::Error>() {}
    assert_std_error::<OpenRacingError>();
    assert_std_error::<RTError>();
    assert_std_error::<DeviceError>();
    assert_std_error::<ProfileError>();
    assert_std_error::<ValidationError>();
}

#[test]
fn openracing_error_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<OpenRacingError>();
}

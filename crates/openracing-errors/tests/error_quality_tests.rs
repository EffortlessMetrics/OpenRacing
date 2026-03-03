//! Error message quality and usability tests.
//!
//! Validates that every error variant produces helpful, user-friendly messages
//! with sufficient context for diagnosis and resolution.

use openracing_errors::{
    DeviceError, ErrorCategory, ErrorContext, ErrorSeverity, OpenRacingError, ProfileError,
    RTError, ResultExt, ValidationError, error_context,
};
use std::collections::HashSet;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Assert a Display message is non-empty and contains the given substrings.
fn assert_message_quality(msg: &str, required_substrings: &[&str]) {
    assert!(!msg.is_empty(), "Error message must not be empty");
    assert!(
        msg.len() >= 5,
        "Error message too terse (<5 chars): '{msg}'"
    );
    for sub in required_substrings {
        assert!(
            msg.contains(sub),
            "Expected '{sub}' in error message: '{msg}'"
        );
    }
}

// =========================================================================
// 1. Every error variant has a non-empty Display impl
// =========================================================================

#[test]
fn rt_error_variants_have_nonempty_display() {
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

    for variant in &variants {
        let msg = variant.to_string();
        assert!(
            !msg.is_empty(),
            "RTError::{variant:?} has empty Display output"
        );
        assert!(
            msg.len() >= 5,
            "RTError::{variant:?} message too terse: '{msg}'"
        );
    }
}

#[test]
fn device_error_variants_have_nonempty_display() {
    let variants: Vec<DeviceError> = vec![
        DeviceError::not_found("moza-r9"),
        DeviceError::disconnected("moza-r9"),
        DeviceError::ConnectionFailed("handshake failed".into()),
        DeviceError::CommunicationError {
            device: "moza-r9".into(),
            message: "CRC mismatch".into(),
        },
        DeviceError::HidError("descriptor parse failed".into()),
        DeviceError::InvalidResponse {
            device: "moza-r9".into(),
            expected: 64,
            actual: 32,
        },
        DeviceError::timeout("moza-r9", 500),
        DeviceError::unsupported(0x346E, 0x0005),
        DeviceError::Busy("moza-r9".into()),
        DeviceError::PermissionDenied("moza-r9".into()),
        DeviceError::InitializationFailed {
            device: "moza-r9".into(),
            reason: "firmware too old".into(),
        },
        DeviceError::FirmwareError {
            device: "moza-r9".into(),
            message: "CRC check failed".into(),
        },
        DeviceError::FeatureNotSupported {
            device: "moza-r9".into(),
            feature: "LED bus".into(),
        },
    ];

    for variant in &variants {
        let msg = variant.to_string();
        assert!(
            !msg.is_empty(),
            "DeviceError::{variant:?} has empty Display output"
        );
    }
}

#[test]
fn validation_error_variants_have_nonempty_display() {
    let variants: Vec<ValidationError> = vec![
        ValidationError::out_of_range("torque", 1.5_f32, -1.0_f32, 1.0_f32),
        ValidationError::required("profile_id"),
        ValidationError::invalid_format("email", "missing @ symbol"),
        ValidationError::too_long("name", 200, 50),
        ValidationError::too_short("name", 1, 3),
        ValidationError::invalid_enum("mode", "unknown", "pid, raw, telemetry"),
        ValidationError::constraint("values must be monotonic"),
        ValidationError::Custom("custom validation".into()),
        ValidationError::InvalidCharacters {
            field: "name".into(),
            reason: "contains null byte".into(),
        },
        ValidationError::NotUnique {
            field: "profile_id".into(),
            value: "gt3".into(),
        },
        ValidationError::DependencyNotMet {
            field: "gain".into(),
            dependency: "mode".into(),
        },
        ValidationError::InvalidType {
            field: "torque".into(),
            expected: "f32".into(),
            actual: "string".into(),
        },
        ValidationError::NumericOverflow {
            field: "counter".into(),
        },
    ];

    for variant in &variants {
        let msg = variant.to_string();
        assert!(
            !msg.is_empty(),
            "ValidationError::{variant:?} has empty Display output"
        );
    }
}

#[test]
fn profile_error_variants_have_nonempty_display() {
    let variants: Vec<ProfileError> = vec![
        ProfileError::not_found("iracing-gt3"),
        ProfileError::AlreadyExists("iracing-gt3".into()),
        ProfileError::invalid_format("/profiles/gt3.yaml", "missing name"),
        ProfileError::ValidationFailed("torque out of range".into()),
        ProfileError::SaveFailed {
            profile: "gt3".into(),
            reason: "disk full".into(),
        },
        ProfileError::LoadFailed {
            path: "/profiles/gt3.yaml".into(),
            reason: "permission denied".into(),
        },
        ProfileError::circular_inheritance("a -> b -> a"),
        ProfileError::InheritanceDepthExceeded {
            depth: 10,
            max_depth: 5,
        },
        ProfileError::ParentNotFound {
            parent_id: "base-profile".into(),
        },
        ProfileError::InvalidId("inv@lid".into()),
        ProfileError::Conflict("concurrent edit".into()),
        ProfileError::version_mismatch("2.0", "1.0"),
        ProfileError::MissingField {
            profile: "gt3".into(),
            field: "max_torque".into(),
        },
        ProfileError::Locked("gt3".into()),
        ProfileError::InvalidDeviceMapping {
            profile: "gt3".into(),
            device: "unknown-device".into(),
        },
    ];

    for variant in &variants {
        let msg = variant.to_string();
        assert!(
            !msg.is_empty(),
            "ProfileError::{variant:?} has empty Display output"
        );
    }
}

#[test]
fn openracing_error_wrapper_variants_have_nonempty_display() {
    let variants: Vec<OpenRacingError> = vec![
        RTError::TimingViolation.into(),
        DeviceError::not_found("wheel").into(),
        ProfileError::not_found("gt3").into(),
        ValidationError::required("field").into(),
        OpenRacingError::from(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "file missing",
        )),
        OpenRacingError::config("missing section"),
        OpenRacingError::other("unexpected state"),
    ];

    for variant in &variants {
        let msg = variant.to_string();
        assert!(
            !msg.is_empty(),
            "OpenRacingError variant has empty Display: {variant:?}"
        );
    }
}

// =========================================================================
// 2. Error messages are user-friendly (no internal jargon without context)
// =========================================================================

#[test]
fn device_errors_include_device_identity() {
    let device_name = "moza-r9";

    let errors: Vec<DeviceError> = vec![
        DeviceError::not_found(device_name),
        DeviceError::disconnected(device_name),
        DeviceError::timeout(device_name, 500),
        DeviceError::CommunicationError {
            device: device_name.into(),
            message: "CRC mismatch".into(),
        },
        DeviceError::InvalidResponse {
            device: device_name.into(),
            expected: 64,
            actual: 32,
        },
        DeviceError::FirmwareError {
            device: device_name.into(),
            message: "CRC check failed".into(),
        },
        DeviceError::FeatureNotSupported {
            device: device_name.into(),
            feature: "LED bus".into(),
        },
        DeviceError::InitializationFailed {
            device: device_name.into(),
            reason: "firmware too old".into(),
        },
    ];

    for err in &errors {
        assert_message_quality(&err.to_string(), &[device_name]);
    }
}

#[test]
fn unsupported_device_includes_vid_pid() {
    let err = DeviceError::unsupported(0x346E, 0x0005);
    let msg = err.to_string();
    assert!(
        msg.contains("346e") || msg.contains("346E"),
        "Missing vendor ID in: '{msg}'"
    );
    assert!(
        msg.contains("0005"),
        "Missing product ID in: '{msg}'"
    );
}

#[test]
fn timeout_error_includes_duration() {
    let err = DeviceError::timeout("wheel", 500);
    assert_message_quality(&err.to_string(), &["500"]);
}

#[test]
fn invalid_response_includes_byte_counts() {
    let err = DeviceError::InvalidResponse {
        device: "wheel".into(),
        expected: 64,
        actual: 32,
    };
    assert_message_quality(&err.to_string(), &["64", "32"]);
}

// =========================================================================
// 3. Validation errors include the field name
// =========================================================================

#[test]
fn validation_out_of_range_includes_field_and_bounds() {
    let err = ValidationError::out_of_range("torque", 1.5_f32, -1.0_f32, 1.0_f32);
    let msg = err.to_string();
    assert_message_quality(&msg, &["torque"]);
    assert!(msg.contains("1.5"), "Missing invalid value in: '{msg}'");
}

#[test]
fn validation_required_includes_field_name() {
    let err = ValidationError::required("profile_id");
    assert_message_quality(&err.to_string(), &["profile_id"]);
}

#[test]
fn validation_invalid_enum_includes_expected_values() {
    let err = ValidationError::invalid_enum("mode", "unknown", "pid, raw, telemetry");
    let msg = err.to_string();
    assert_message_quality(&msg, &["mode", "unknown", "pid, raw, telemetry"]);
}

// =========================================================================
// 4. Profile errors include file paths where relevant
// =========================================================================

#[test]
fn profile_load_failed_includes_path() {
    let err = ProfileError::LoadFailed {
        path: "/home/user/.config/openracing/gt3.yaml".into(),
        reason: "file not found".into(),
    };
    assert_message_quality(&err.to_string(), &["gt3.yaml", "file not found"]);
}

#[test]
fn profile_invalid_format_includes_path() {
    let err = ProfileError::invalid_format("/profiles/broken.yaml", "unexpected token");
    assert_message_quality(&err.to_string(), &["broken.yaml", "unexpected token"]);
}

#[test]
fn profile_version_mismatch_includes_versions() {
    let err = ProfileError::version_mismatch("2.0", "1.0");
    let msg = err.to_string();
    assert_message_quality(&msg, &["2.0", "1.0"]);
}

// =========================================================================
// 5. Error chains preserve context (source errors accessible)
// =========================================================================

#[test]
fn openracing_error_rt_source_is_accessible() {
    let inner = RTError::TimingViolation;
    let outer: OpenRacingError = inner.into();
    // The source chain should include the inner error's message
    let msg = outer.to_string();
    assert!(
        msg.contains("timing") || msg.contains("Timing"),
        "Wrapped RT error should mention timing: '{msg}'"
    );
}

#[test]
fn openracing_error_device_source_preserves_detail() {
    let inner = DeviceError::timeout("moza-r9", 500);
    let outer: OpenRacingError = inner.into();
    let msg = outer.to_string();
    assert_message_quality(&msg, &["moza-r9", "500"]);
}

#[test]
fn io_error_wraps_correctly() -> std::result::Result<(), String> {
    let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
    let or_err: OpenRacingError = io_err.into();
    let msg = or_err.to_string();
    assert!(
        msg.contains("access denied"),
        "IO error detail lost: '{msg}'"
    );
    // source() should return the std::io::Error
    let source = std::error::Error::source(&or_err);
    assert!(source.is_some(), "IO error source() should be Some");
    Ok(())
}

// =========================================================================
// 6. Error downcasting works correctly
// =========================================================================

#[test]
fn downcast_rt_error_from_openracing_error() {
    let err: OpenRacingError = RTError::PipelineFault.into();
    match &err {
        OpenRacingError::RT(rt) => assert_eq!(*rt, RTError::PipelineFault),
        other => panic!("Expected RT variant, got: {other:?}"),
    }
}

#[test]
fn downcast_device_error_from_openracing_error() {
    let err: OpenRacingError = DeviceError::not_found("wheel").into();
    match &err {
        OpenRacingError::Device(DeviceError::NotFound(name)) => {
            assert_eq!(name, "wheel");
        }
        other => panic!("Expected Device::NotFound, got: {other:?}"),
    }
}

#[test]
fn downcast_validation_error_from_openracing_error() {
    let err: OpenRacingError = ValidationError::required("field").into();
    match &err {
        OpenRacingError::Validation(ValidationError::Required(f)) => {
            assert_eq!(f, "field");
        }
        other => panic!("Expected Validation::Required, got: {other:?}"),
    }
}

#[test]
fn downcast_profile_error_from_openracing_error() {
    let err: OpenRacingError = ProfileError::not_found("gt3").into();
    match &err {
        OpenRacingError::Profile(ProfileError::NotFound(id)) => {
            assert_eq!(id, "gt3");
        }
        other => panic!("Expected Profile::NotFound, got: {other:?}"),
    }
}

// =========================================================================
// 7. RT error codes are unique
// =========================================================================

#[test]
fn rt_error_codes_are_unique() {
    let all_codes: Vec<(RTError, u8)> = vec![
        (RTError::DeviceDisconnected, RTError::DeviceDisconnected.code()),
        (RTError::TorqueLimit, RTError::TorqueLimit.code()),
        (RTError::PipelineFault, RTError::PipelineFault.code()),
        (RTError::TimingViolation, RTError::TimingViolation.code()),
        (RTError::RTSetupFailed, RTError::RTSetupFailed.code()),
        (RTError::InvalidConfig, RTError::InvalidConfig.code()),
        (RTError::SafetyInterlock, RTError::SafetyInterlock.code()),
        (RTError::BufferOverflow, RTError::BufferOverflow.code()),
        (RTError::DeadlineMissed, RTError::DeadlineMissed.code()),
        (RTError::ResourceUnavailable, RTError::ResourceUnavailable.code()),
    ];

    let mut seen = HashSet::new();
    for (variant, code) in &all_codes {
        assert!(
            seen.insert(code),
            "Duplicate RT error code {code} for {variant:?}"
        );
    }
    // Also verify codes are non-zero (0 is often reserved for "no error")
    for (variant, code) in &all_codes {
        assert_ne!(*code, 0, "RTError::{variant:?} has code 0 (reserved)");
    }
}

#[test]
fn rt_error_code_roundtrip() {
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

    for variant in &variants {
        let code = variant.code();
        let roundtripped = RTError::from_code(code);
        assert_eq!(
            roundtripped,
            Some(*variant),
            "RTError code roundtrip failed for {variant:?} (code={code})"
        );
    }
}

// =========================================================================
// 8. Error category classification is consistent
// =========================================================================

#[test]
fn error_category_values_are_unique() {
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

    let mut seen = HashSet::new();
    for cat in &categories {
        let val = *cat as u8;
        assert!(
            seen.insert(val),
            "Duplicate category discriminant {val} for {cat:?}"
        );
    }
}

#[test]
fn error_category_matches_variant() {
    assert_eq!(
        OpenRacingError::from(RTError::TimingViolation).category(),
        ErrorCategory::RT
    );
    assert_eq!(
        OpenRacingError::from(DeviceError::not_found("x")).category(),
        ErrorCategory::Device
    );
    assert_eq!(
        OpenRacingError::from(ProfileError::not_found("x")).category(),
        ErrorCategory::Profile
    );
    assert_eq!(
        OpenRacingError::from(ValidationError::required("x")).category(),
        ErrorCategory::Validation
    );
    assert_eq!(
        OpenRacingError::config("x").category(),
        ErrorCategory::Config
    );
    assert_eq!(
        OpenRacingError::other("x").category(),
        ErrorCategory::Other
    );
    assert_eq!(
        OpenRacingError::from(std::io::Error::other("x")).category(),
        ErrorCategory::IO
    );
}

// =========================================================================
// 9. Severity classification is coherent
// =========================================================================

#[test]
fn critical_errors_are_not_recoverable() {
    let critical_errors: Vec<OpenRacingError> = vec![
        RTError::DeviceDisconnected.into(),
        RTError::TorqueLimit.into(),
        RTError::SafetyInterlock.into(),
        RTError::DeadlineMissed.into(),
        DeviceError::disconnected("wheel").into(),
    ];

    for err in &critical_errors {
        assert_eq!(
            err.severity(),
            ErrorSeverity::Critical,
            "{err:?} should be Critical"
        );
        assert!(
            !err.is_recoverable(),
            "Critical error should not be recoverable: {err:?}"
        );
    }
}

#[test]
fn recoverable_errors_are_not_critical() {
    let recoverable: Vec<OpenRacingError> = vec![
        RTError::TimingViolation.into(),
        RTError::BufferOverflow.into(),
        OpenRacingError::config("bad value"),
        OpenRacingError::other("transient"),
        DeviceError::timeout("wheel", 500).into(),
    ];

    for err in &recoverable {
        assert!(
            err.is_recoverable(),
            "Expected recoverable: {err:?} (severity={:?})",
            err.severity()
        );
    }
}

// =========================================================================
// 10. Error context preserves diagnostic info
// =========================================================================

#[test]
fn error_context_preserves_operation_name() {
    let ctx = ErrorContext::new("apply_profile");
    let display = ctx.to_string();
    assert!(
        display.contains("apply_profile"),
        "Context should contain operation: '{display}'"
    );
}

#[test]
fn error_context_preserves_key_value_pairs() {
    let ctx = ErrorContext::new("load_config")
        .with("path", "/etc/openracing/config.yaml")
        .with("device", "moza-r9");
    let display = ctx.to_string();
    assert_message_quality(&display, &["load_config", "path", "config.yaml", "moza-r9"]);
}

#[test]
fn error_context_preserves_source_location() {
    let ctx = ErrorContext::new("tick")
        .with("frame", "12345")
        .at("engine.rs", 42);
    let display = ctx.to_string();
    assert_message_quality(&display, &["tick", "12345", "engine.rs:42"]);
}

#[test]
fn error_context_macro_builds_correctly() {
    let ctx = error_context!(
        "apply_profile",
        "profile" => "gt3",
        "device" => "moza-r9"
    );
    let display = ctx.to_string();
    assert_message_quality(&display, &["apply_profile", "gt3", "moza-r9"]);
}

// =========================================================================
// 11. ResultExt preserves error context across boundaries
// =========================================================================

#[test]
fn result_ext_with_context_wraps_error_message() -> std::result::Result<(), String> {
    let result: Result<(), RTError> = Err(RTError::TimingViolation);
    let wrapped = result.with_context("processing_frame");
    match wrapped {
        Err(err) => {
            let msg = err.to_string();
            assert!(
                msg.contains("processing_frame"),
                "Context lost in: '{msg}'"
            );
            assert!(
                msg.to_lowercase().contains("timing"),
                "Inner error lost in: '{msg}'"
            );
            Ok(())
        }
        Ok(()) => Err("Expected error".into()),
    }
}

#[test]
fn result_ext_context_preserves_kv_pairs() -> std::result::Result<(), String> {
    let result: Result<(), DeviceError> = Err(DeviceError::not_found("wheel"));
    let ctx = ErrorContext::new("device_init")
        .with("vendor", "moza")
        .at("ports.rs", 100);
    let wrapped = result.context(ctx);
    match wrapped {
        Err(err) => {
            let msg = err.to_string();
            assert!(msg.contains("device_init"), "Operation lost in: '{msg}'");
            assert!(msg.contains("moza"), "KV pair lost in: '{msg}'");
            Ok(())
        }
        Ok(()) => Err("Expected error".into()),
    }
}

// =========================================================================
// 12. Safety-related errors clearly communicate urgency
// =========================================================================

#[test]
fn safety_requiring_errors_have_critical_severity() {
    let safety_errors = [
        RTError::DeviceDisconnected,
        RTError::TorqueLimit,
        RTError::SafetyInterlock,
        RTError::DeadlineMissed,
    ];

    for err in &safety_errors {
        assert!(
            err.requires_safety_action(),
            "{err:?} should require safety action"
        );
        assert_eq!(
            err.severity(),
            ErrorSeverity::Critical,
            "{err:?} requiring safety action should be Critical"
        );
    }
}

#[test]
fn non_safety_errors_do_not_require_action() {
    let non_safety = [
        RTError::TimingViolation,
        RTError::InvalidConfig,
        RTError::BufferOverflow,
        RTError::ResourceUnavailable,
        RTError::PipelineFault,
        RTError::RTSetupFailed,
    ];

    for err in &non_safety {
        assert!(
            !err.requires_safety_action(),
            "{err:?} should NOT require safety action"
        );
    }
}

// =========================================================================
// 13. Device retryability classification
// =========================================================================

#[test]
fn retryable_device_errors_are_classified_correctly() {
    assert!(DeviceError::timeout("wheel", 500).is_retryable());
    assert!(DeviceError::Busy("wheel".into()).is_retryable());

    assert!(!DeviceError::not_found("wheel").is_retryable());
    assert!(!DeviceError::disconnected("wheel").is_retryable());
    assert!(!DeviceError::PermissionDenied("wheel".into()).is_retryable());
    assert!(!DeviceError::unsupported(0x1234, 0x5678).is_retryable());
}

// =========================================================================
// 14. Severity ordering is correct
// =========================================================================

#[test]
fn severity_ordering_is_monotonic() {
    let levels = [
        ErrorSeverity::Info,
        ErrorSeverity::Warning,
        ErrorSeverity::Error,
        ErrorSeverity::Critical,
    ];

    for window in levels.windows(2) {
        assert!(
            window[0] < window[1],
            "{:?} should be less than {:?}",
            window[0],
            window[1]
        );
    }
}

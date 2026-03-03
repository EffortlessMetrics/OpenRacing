//! Error propagation integration tests.
//!
//! Covers:
//! - Error propagation from device layer → engine → service → CLI
//! - Error formatting for user display vs log output
//! - Errors don't leak sensitive information
//! - Error recovery paths (retry, fallback, graceful degradation)
//! - Timeout error handling
//! - Network/IO error handling
//! - Cross-layer error category preservation

use openracing_errors::{
    DeviceError, ErrorCategory, ErrorSeverity, OpenRacingError, ProfileError,
    RTError, ResultExt, ValidationError,
};
use racing_wheel_engine::safety::{FaultType, SafetyService};
use std::time::Duration;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn create_test_safety_service() -> SafetyService {
    SafetyService::with_timeouts(
        5.0,
        25.0,
        Duration::from_secs(3),
        Duration::from_secs(2),
    )
}

/// Simulate a function that propagates device errors through the engine layer.
fn engine_process_device_input(connected: bool) -> Result<f32, OpenRacingError> {
    if !connected {
        return Err(DeviceError::disconnected("moza-r9").into());
    }
    Ok(0.5)
}

/// Simulate service layer wrapping engine errors with context.
fn service_handle_engine_result(connected: bool) -> Result<f32, OpenRacingError> {
    engine_process_device_input(connected).with_context("service_process_tick")
}

/// Simulate CLI layer formatting errors for user display.
fn cli_format_error(err: &OpenRacingError) -> String {
    format!("[{}] {}: {}", err.severity(), err.category(), err)
}

/// Simulate a retryable operation with fallback.
fn retryable_device_read(
    attempts: &mut u32,
    succeed_on: u32,
) -> Result<Vec<u8>, OpenRacingError> {
    *attempts += 1;
    if *attempts < succeed_on {
        Err(DeviceError::timeout("moza-r9", 100).into())
    } else {
        Ok(vec![0x01, 0x02, 0x03])
    }
}

// =========================================================================
// 1. Error propagation from device → engine → service → CLI
// =========================================================================

#[test]
fn device_error_propagates_to_engine_layer() {
    let result = engine_process_device_input(false);
    assert!(result.is_err());
    if let Err(e) = result {
        assert_eq!(e.category(), ErrorCategory::Device);
    }
}

#[test]
fn engine_error_propagates_to_service_with_context() {
    let result = service_handle_engine_result(false);
    assert!(result.is_err());
    if let Err(e) = result {
        let msg = e.to_string();
        assert!(
            msg.contains("service_process_tick"),
            "Service context missing from error: {msg}"
        );
    }
}

#[test]
fn service_error_formats_for_cli_display() {
    let err: OpenRacingError = DeviceError::disconnected("moza-r9").into();
    let formatted = cli_format_error(&err);
    assert!(formatted.contains("CRITICAL"));
    assert!(formatted.contains("Device"));
    assert!(formatted.contains("disconnected"));
}

#[test]
fn full_propagation_chain_preserves_message() {
    // device → engine → service → cli
    let device_err = DeviceError::not_found("simucube-2");
    let engine_err: OpenRacingError = device_err.into();
    let service_result: Result<(), OpenRacingError> = Err(engine_err);
    let with_ctx = service_result.with_context("resolve_device");
    if let Err(e) = with_ctx {
        let formatted = cli_format_error(&e);
        assert!(
            formatted.contains("simucube-2") || formatted.contains("resolve_device"),
            "Propagation chain lost information: {formatted}"
        );
    }
}

// =========================================================================
// 2. Error formatting for user display vs log output
// =========================================================================

#[test]
fn user_display_includes_severity_and_category() {
    let err: OpenRacingError = RTError::TimingViolation.into();
    let user_msg = cli_format_error(&err);
    assert!(user_msg.contains("WARN"));
    assert!(user_msg.contains("RT"));
}

#[test]
fn debug_format_includes_variant_details() {
    let err = DeviceError::InvalidResponse {
        device: "moza-r9".into(),
        expected: 64,
        actual: 32,
    };
    let debug = format!("{err:?}");
    assert!(debug.contains("64"));
    assert!(debug.contains("32"));
    assert!(debug.contains("moza-r9"));
}

#[test]
fn display_format_is_human_readable() {
    let err = DeviceError::InvalidResponse {
        device: "moza-r9".into(),
        expected: 64,
        actual: 32,
    };
    let display = err.to_string();
    // Should read like a sentence, not a debug dump
    assert!(display.contains("expected"));
    assert!(display.contains("got"));
}

// =========================================================================
// 3. Errors don't leak sensitive information
// =========================================================================

#[test]
fn device_error_does_not_leak_raw_memory() {
    let err = DeviceError::CommunicationError {
        device: "moza-r9".into(),
        message: "write failed".into(),
    };
    let msg = err.to_string();
    // Should not contain pointer addresses or raw hex dumps
    assert!(
        !msg.contains("0x7ff"),
        "Error message may leak memory addresses: {msg}"
    );
}

#[test]
fn profile_error_does_not_leak_file_system_paths_beyond_context() {
    let err = ProfileError::not_found("user-profile-123");
    let msg = err.to_string();
    // Should only contain the profile ID, not full filesystem paths
    assert!(
        !msg.contains("C:\\") && !msg.contains("/home/"),
        "Error message may leak filesystem paths: {msg}"
    );
}

#[test]
fn config_error_does_not_contain_secrets() {
    let err = OpenRacingError::config("invalid API key format");
    let msg = err.to_string();
    assert!(
        !msg.contains("sk-") && !msg.contains("password"),
        "Error message may leak secrets: {msg}"
    );
}

#[test]
fn io_error_wraps_safely() {
    let io_err = std::io::Error::new(
        std::io::ErrorKind::PermissionDenied,
        "access denied for /etc/shadow",
    );
    let ore: OpenRacingError = io_err.into();
    let msg = ore.to_string();
    // The IO error message is included but the wrapper prevents raw stack info
    assert!(msg.contains("I/O error"));
}

// =========================================================================
// 4. Error recovery paths
// =========================================================================

#[test]
fn retry_on_timeout_succeeds_eventually() {
    let mut attempts = 0u32;
    let mut last_err = None;
    for _ in 0..5 {
        match retryable_device_read(&mut attempts, 3) {
            Ok(data) => {
                assert_eq!(data.len(), 3);
                last_err = None;
                break;
            }
            Err(e) => {
                last_err = Some(e);
            }
        }
    }
    assert!(last_err.is_none(), "Should have succeeded after retries");
    assert_eq!(attempts, 3);
}

#[test]
fn non_retryable_error_aborts_immediately() {
    let err = DeviceError::not_found("nonexistent");
    assert!(
        !err.is_retryable(),
        "NotFound should not be retryable"
    );
}

#[test]
fn fallback_to_safe_torque_on_critical_error() {
    let mut service = create_test_safety_service();

    // Simulate critical fault
    service.report_fault(FaultType::UsbStall);

    // Fallback: clamp torque to zero
    let safe_torque = service.clamp_torque_nm(20.0);
    assert!(
        safe_torque.abs() < f32::EPSILON,
        "Fallback should produce zero torque, got {safe_torque}"
    );
}

#[test]
fn graceful_degradation_on_recoverable_error() {
    // A timing violation is recoverable — system continues with warning
    let err = RTError::TimingViolation;
    assert!(err.is_recoverable());
    // In degraded mode, engine would skip the violated tick and continue
    let ore: OpenRacingError = err.into();
    assert_eq!(ore.severity(), ErrorSeverity::Warning);
}

// =========================================================================
// 5. Timeout error handling
// =========================================================================

#[test]
fn timeout_error_contains_duration() {
    let err = DeviceError::timeout("moza-r9", 500);
    let msg = err.to_string();
    assert!(msg.contains("500"), "Timeout message should contain duration: {msg}");
}

#[test]
fn timeout_error_is_retryable() {
    let err = DeviceError::timeout("moza-r9", 500);
    assert!(err.is_retryable());
}

#[test]
fn timeout_error_has_warning_severity() {
    let err = DeviceError::timeout("moza-r9", 500);
    assert_eq!(err.severity(), ErrorSeverity::Warning);
}

// =========================================================================
// 6. Network/IO error handling
// =========================================================================

#[test]
fn io_not_found_converts_to_openracing() {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
    let ore: OpenRacingError = io_err.into();
    assert_eq!(ore.category(), ErrorCategory::IO);
    assert!(ore.is_recoverable());
}

#[test]
fn io_permission_denied_converts_to_openracing() {
    let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
    let ore: OpenRacingError = io_err.into();
    assert_eq!(ore.category(), ErrorCategory::IO);
    assert!(ore.to_string().contains("access denied"));
}

#[test]
fn io_connection_refused_converts_to_openracing() {
    let io_err = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "conn refused");
    let ore: OpenRacingError = io_err.into();
    assert_eq!(ore.category(), ErrorCategory::IO);
}

// =========================================================================
// 7. Cross-layer error category preservation
// =========================================================================

#[test]
fn error_context_wrapping_changes_category_to_other() {
    let result: Result<(), RTError> = Err(RTError::PipelineFault);
    let wrapped = result.with_context("engine_tick");
    if let Err(e) = wrapped {
        // ResultExt wraps into OpenRacingError::Other
        assert_eq!(e.category(), ErrorCategory::Other);
        // But the original error info is preserved in the message
        assert!(e.to_string().contains("Pipeline") || e.to_string().contains("pipeline"));
    }
}

#[test]
fn direct_from_conversion_preserves_category() {
    let errors_and_categories: Vec<(OpenRacingError, ErrorCategory)> = vec![
        (RTError::TimingViolation.into(), ErrorCategory::RT),
        (DeviceError::not_found("d").into(), ErrorCategory::Device),
        (ProfileError::not_found("p").into(), ErrorCategory::Profile),
        (ValidationError::required("f").into(), ErrorCategory::Validation),
        (
            std::io::Error::other("x").into(),
            ErrorCategory::IO,
        ),
    ];
    for (err, expected_cat) in &errors_and_categories {
        assert_eq!(
            err.category(),
            *expected_cat,
            "Category mismatch for {err:?}"
        );
    }
}

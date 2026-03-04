//! Deep error propagation tests for the engine crate.
//!
//! Validates that errors from the device/safety/pipeline layers propagate
//! correctly, preserve context, and support recovery classification.

use openracing_errors::{
    DeviceError, ErrorCategory, ErrorSeverity, OpenRacingError, RTError, ResultExt, ValidationError,
};
use racing_wheel_engine::pipeline::PipelineError;
use racing_wheel_engine::safety::{FaultType, SafetyService, SafetyState, WatchdogError};
use racing_wheel_engine::tracing::TracingError;
use racing_wheel_engine::{ProfileRepoError, SafetyViolation};
use std::time::Duration;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn create_test_safety_service() -> SafetyService {
    SafetyService::with_timeouts(5.0, 25.0, Duration::from_secs(3), Duration::from_secs(2))
}

/// Simulate a device-layer function returning an RTError.
fn device_write(connected: bool, torque: f32, max: f32) -> Result<(), RTError> {
    if !connected {
        return Err(RTError::DeviceDisconnected);
    }
    if torque.abs() > max {
        return Err(RTError::TorqueLimit);
    }
    Ok(())
}

/// Simulate an engine-level function that wraps device errors.
fn engine_process_tick(connected: bool, torque: f32, max: f32) -> Result<(), OpenRacingError> {
    device_write(connected, torque, max).with_context("engine_process_tick")?;
    Ok(())
}

/// Simulate a service-level function that wraps engine errors.
fn service_handle_frame(connected: bool, torque: f32, max: f32) -> Result<(), OpenRacingError> {
    engine_process_tick(connected, torque, max).with_context("service_handle_frame")?;
    Ok(())
}

// =========================================================================
// 1. Device errors propagate through engine to service layer
// =========================================================================

#[test]
fn device_disconnect_propagates_to_service() -> Result<(), String> {
    let result = service_handle_frame(false, 5.0, 25.0);
    match result {
        Err(err) => {
            let msg = err.to_string();
            assert!(
                msg.to_lowercase().contains("disconnect"),
                "Service error should mention disconnect: '{msg}'"
            );
            Ok(())
        }
        Ok(()) => Err("Expected disconnect error".into()),
    }
}

#[test]
fn torque_limit_propagates_to_service() -> Result<(), String> {
    let result = service_handle_frame(true, 30.0, 25.0);
    match result {
        Err(err) => {
            let msg = err.to_string();
            assert!(
                msg.to_lowercase().contains("torque"),
                "Service error should mention torque: '{msg}'"
            );
            Ok(())
        }
        Ok(()) => Err("Expected torque limit error".into()),
    }
}

#[test]
fn successful_operation_propagates_ok() {
    let result = service_handle_frame(true, 5.0, 25.0);
    assert!(result.is_ok());
}

// =========================================================================
// 2. Error context is preserved across crate boundaries
// =========================================================================

#[test]
fn context_chain_preserves_all_layers() -> Result<(), String> {
    let inner = RTError::PipelineFault;
    let mid: Result<(), RTError> = Err(inner);
    let engine_result = mid.with_context("compile_pipeline");
    let service_result = engine_result.with_context("apply_profile");

    match service_result {
        Err(err) => {
            let msg = err.to_string();
            assert!(
                msg.contains("apply_profile"),
                "Missing service context: '{msg}'"
            );
            Ok(())
        }
        Ok(()) => Err("Expected error".into()),
    }
}

#[test]
fn device_error_preserves_identity_through_wrapping() -> Result<(), String> {
    let device_err = DeviceError::timeout("moza-r9", 500);
    let result: Result<(), DeviceError> = Err(device_err);
    let wrapped = result.with_context("poll_device");

    match wrapped {
        Err(err) => {
            let msg = err.to_string();
            assert!(msg.contains("moza-r9"), "Device identity lost: '{msg}'");
            assert!(msg.contains("500"), "Timeout detail lost: '{msg}'");
            Ok(())
        }
        Ok(()) => Err("Expected error".into()),
    }
}

#[test]
fn validation_error_preserves_field_through_wrapping() -> Result<(), String> {
    let val_err = ValidationError::out_of_range("gain", 2.0_f32, 0.0_f32, 1.0_f32);
    let result: Result<(), ValidationError> = Err(val_err);
    let wrapped = result.with_context("load_profile");

    match wrapped {
        Err(err) => {
            let msg = err.to_string();
            assert!(msg.contains("gain"), "Field name lost: '{msg}'");
            Ok(())
        }
        Ok(()) => Err("Expected error".into()),
    }
}

// =========================================================================
// 3. Multiple simultaneous errors don't lose information
// =========================================================================

#[test]
fn multiple_errors_collected_independently() {
    let errors: Vec<OpenRacingError> = vec![
        RTError::TimingViolation.into(),
        DeviceError::timeout("wheel-1", 100).into(),
        DeviceError::timeout("wheel-2", 200).into(),
        ValidationError::required("profile_id").into(),
    ];

    // Verify each error retains its identity
    assert_eq!(errors[0].category(), ErrorCategory::RT);
    assert_eq!(errors[1].category(), ErrorCategory::Device);
    assert_eq!(errors[2].category(), ErrorCategory::Device);
    assert_eq!(errors[3].category(), ErrorCategory::Validation);

    // Verify distinct device errors retain their device names
    assert!(errors[1].to_string().contains("wheel-1"));
    assert!(errors[2].to_string().contains("wheel-2"));
}

#[test]
fn error_vec_preserves_ordering_and_severity() {
    let mut errors: Vec<OpenRacingError> = vec![
        OpenRacingError::config("minor issue"),
        RTError::DeviceDisconnected.into(),
        DeviceError::timeout("wheel", 100).into(),
    ];

    // Sort by severity (highest first)
    errors.sort_by_key(|e| std::cmp::Reverse(e.severity()));

    assert_eq!(errors[0].severity(), ErrorSeverity::Critical);
    assert!(errors[0].to_string().to_lowercase().contains("disconnect"),);
}

// =========================================================================
// 4. Error recovery paths (retryable vs fatal)
// =========================================================================

#[test]
fn rt_recoverable_vs_fatal_classification() {
    let recoverable = [
        RTError::TimingViolation,
        RTError::BufferOverflow,
        RTError::ResourceUnavailable,
    ];
    let fatal = [
        RTError::DeviceDisconnected,
        RTError::TorqueLimit,
        RTError::PipelineFault,
        RTError::RTSetupFailed,
        RTError::InvalidConfig,
        RTError::SafetyInterlock,
        RTError::DeadlineMissed,
    ];

    for err in &recoverable {
        assert!(err.is_recoverable(), "{err:?} should be recoverable");
    }
    for err in &fatal {
        assert!(!err.is_recoverable(), "{err:?} should NOT be recoverable");
    }
}

#[test]
fn device_retryable_vs_permanent() {
    let retryable = [
        DeviceError::timeout("w", 100),
        DeviceError::Busy("w".into()),
    ];
    let permanent = [
        DeviceError::not_found("w"),
        DeviceError::disconnected("w"),
        DeviceError::PermissionDenied("w".into()),
        DeviceError::unsupported(0, 0),
    ];

    for err in &retryable {
        assert!(err.is_retryable(), "{err:?} should be retryable");
    }
    for err in &permanent {
        assert!(!err.is_retryable(), "{err:?} should NOT be retryable");
    }
}

#[test]
fn tracing_recoverable_vs_fatal() {
    assert!(TracingError::BufferOverflow(100).is_recoverable());
    assert!(TracingError::NotInitialized.is_recoverable());
    assert!(TracingError::EmissionFailed("x".into()).is_recoverable());

    assert!(!TracingError::PlatformNotSupported.is_recoverable());
    assert!(!TracingError::InitializationFailed("x".into()).is_recoverable());
    assert!(!TracingError::InvalidConfiguration("x".into()).is_recoverable());
}

// =========================================================================
// 5. Safety fault error propagation
// =========================================================================

#[test]
fn usb_stall_fault_transitions_to_faulted() {
    let mut service = create_test_safety_service();
    service.report_fault(FaultType::UsbStall);
    assert!(
        matches!(service.state(), SafetyState::Faulted { .. }),
        "USB stall should trigger Faulted state"
    );
}

#[test]
fn thermal_limit_fault_transitions_to_faulted() {
    let mut service = create_test_safety_service();
    service.report_fault(FaultType::ThermalLimit);
    assert!(
        matches!(service.state(), SafetyState::Faulted { .. }),
        "Thermal limit should trigger Faulted state"
    );
}

#[test]
fn faulted_state_zeroes_max_torque() {
    let mut service = create_test_safety_service();
    service.report_fault(FaultType::UsbStall);
    let max_torque = service.max_torque_nm();
    assert!(
        max_torque.abs() < f32::EPSILON,
        "Faulted state should zero torque, got {max_torque}"
    );
}

// =========================================================================
// 6. Pipeline error messages are diagnostic
// =========================================================================

#[test]
fn pipeline_invalid_config_includes_reason() {
    let err = PipelineError::InvalidConfig("damper coefficient negative".into());
    let msg = err.to_string();
    assert!(
        msg.contains("damper coefficient negative"),
        "Missing config detail: '{msg}'"
    );
}

#[test]
fn pipeline_error_is_std_error() {
    let err = PipelineError::CompilationFailed("unknown filter".into());
    let _: &dyn std::error::Error = &err;
    assert!(!err.to_string().is_empty());
}

// =========================================================================
// 7. Watchdog errors preserve diagnostic context
// =========================================================================

#[test]
fn watchdog_hardware_error_includes_message() {
    let err = WatchdogError::HardwareError("USB bus reset".into());
    assert!(
        err.to_string().contains("USB bus reset"),
        "Detail lost: '{}'",
        err
    );
}

#[test]
fn watchdog_invalid_config_includes_detail() {
    let err = WatchdogError::InvalidConfiguration("timeout must be > 0".into());
    assert!(
        err.to_string().contains("timeout must be > 0"),
        "Detail lost: '{}'",
        err
    );
}

#[test]
fn watchdog_error_is_std_error() {
    let err = WatchdogError::TimedOut;
    let _: &dyn std::error::Error = &err;
    assert!(!err.to_string().is_empty());
}

// =========================================================================
// 8. Profile repo errors include context
// =========================================================================

#[test]
fn profile_repo_serialization_error_preserves_detail() {
    let err = ProfileRepoError::SerializationError("invalid TOML at line 42".into());
    let msg = err.to_string();
    assert!(
        msg.contains("line 42"),
        "Serialization detail lost: '{msg}'"
    );
}

#[test]
fn profile_repo_corruption_error_preserves_detail() {
    let err = ProfileRepoError::CorruptionError("checksum mismatch in header".into());
    let msg = err.to_string();
    assert!(
        msg.contains("checksum mismatch"),
        "Corruption detail lost: '{msg}'"
    );
}

#[test]
fn profile_repo_lock_error_is_descriptive() {
    let err = ProfileRepoError::LockError;
    let msg = err.to_string();
    assert!(
        msg.to_lowercase().contains("lock"),
        "Lock error should mention locking: '{msg}'"
    );
}

// =========================================================================
// 9. Safety violation errors include operational context
// =========================================================================

#[test]
fn temperature_violation_includes_values() {
    let err = SafetyViolation::TemperatureTooHigh {
        current: 85,
        limit: 80,
    };
    let msg = err.to_string();
    assert!(msg.contains("85"), "Current temp missing: '{msg}'");
    assert!(msg.contains("80"), "Limit temp missing: '{msg}'");
}

#[test]
fn hands_off_violation_includes_durations() {
    let err = SafetyViolation::HandsOffTooLong {
        duration: Duration::from_secs(10),
        limit: Duration::from_secs(5),
    };
    let msg = err.to_string();
    assert!(msg.contains("10"), "Duration missing: '{msg}'");
    assert!(msg.contains("5"), "Limit missing: '{msg}'");
}

#[test]
fn rate_limited_violation_includes_timing() {
    let err = SafetyViolation::RateLimited {
        elapsed: Duration::from_millis(500),
        required: Duration::from_secs(2),
    };
    let msg = err.to_string();
    assert!(msg.contains("500"), "Elapsed missing: '{msg}'");
}

// =========================================================================
// 10. Cross-layer error scenario: device fault → safety → engine
// =========================================================================

#[test]
fn device_fault_triggers_safety_cascade() {
    let mut service = create_test_safety_service();

    // Step 1: Device reports USB stall
    service.report_fault(FaultType::UsbStall);
    assert!(matches!(service.state(), SafetyState::Faulted { .. }));

    // Step 2: Torque should be zeroed
    let max_torque = service.max_torque_nm();
    assert!(
        max_torque.abs() < f32::EPSILON,
        "Torque should be zero after fault cascade"
    );

    // Step 3: The fault type should be accessible for logging
    if let SafetyState::Faulted { fault, .. } = service.state() {
        let fault_msg = format!("{fault:?}");
        assert!(
            !fault_msg.is_empty(),
            "Fault debug representation should be non-empty"
        );
    }
}

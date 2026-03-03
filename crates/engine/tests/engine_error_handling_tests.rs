//! Engine error handling tests.
//!
//! Covers:
//! - Engine error recovery from device disconnection
//! - Engine error handling during RT processing
//! - Engine behavior when config is invalid
//! - Error aggregation (multiple errors in one cycle)
//! - Pipeline compilation errors
//! - Safety fault error propagation
//! - RT error code semantics in engine context

use openracing_errors::{
    DeviceError, ErrorCategory, ErrorSeverity, OpenRacingError, RTError, ValidationError,
};
use racing_wheel_engine::pipeline::PipelineError;
use racing_wheel_engine::rt::Frame;
use racing_wheel_engine::safety::{
    FaultType, SafetyService, SafetyState, WatchdogError,
    SoftwareWatchdog, HardwareWatchdog,
};
use std::time::Duration;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn create_test_safety_service() -> SafetyService {
    SafetyService::with_timeouts(
        5.0,                    // max_safe_torque_nm
        25.0,                   // max_high_torque_nm
        Duration::from_secs(3), // hands_off_timeout
        Duration::from_secs(2), // combo_hold_duration
    )
}

// =========================================================================
// 1. Engine error recovery from device disconnection
// =========================================================================

#[test]
fn device_disconnected_rt_error_is_critical() {
    let err = RTError::DeviceDisconnected;
    assert_eq!(err.severity(), ErrorSeverity::Critical);
    assert!(err.requires_safety_action());
    assert!(!err.is_recoverable());
}

#[test]
fn device_disconnect_triggers_safety_fault() {
    let mut service = create_test_safety_service();
    service.report_fault(FaultType::UsbStall);
    let state = service.state();
    assert!(
        matches!(state, SafetyState::Faulted { .. }),
        "USB stall should trigger Faulted state, got {state:?}"
    );
}

#[test]
fn safety_service_clamps_to_zero_in_faulted_state() {
    let mut service = create_test_safety_service();
    service.report_fault(FaultType::UsbStall);

    // In faulted state, torque should be clamped to zero
    let clamped = service.clamp_torque_nm(25.0);
    assert!(
        clamped.abs() < f32::EPSILON,
        "Torque should be zero in faulted state, got {clamped}"
    );
}

// =========================================================================
// 2. Engine error handling during RT processing
// =========================================================================

#[test]
fn rt_timing_violation_is_recoverable() {
    let err = RTError::TimingViolation;
    assert!(err.is_recoverable());
    assert_eq!(err.severity(), ErrorSeverity::Warning);
}

#[test]
fn rt_buffer_overflow_is_recoverable() {
    let err = RTError::BufferOverflow;
    assert!(err.is_recoverable());
    assert_eq!(err.severity(), ErrorSeverity::Warning);
}

#[test]
fn rt_pipeline_fault_is_error_severity() {
    let err = RTError::PipelineFault;
    assert_eq!(err.severity(), ErrorSeverity::Error);
    assert!(!err.is_recoverable());
}

#[test]
fn frame_defaults_to_safe_values() {
    let frame = Frame::default();
    assert!(frame.ffb_in.abs() < f32::EPSILON, "default ffb_in should be 0");
    assert!(
        frame.torque_out.abs() < f32::EPSILON,
        "default torque_out should be 0"
    );
    assert!(!frame.hands_off, "default hands_off should be false");
}

// =========================================================================
// 3. Engine behavior when config is invalid
// =========================================================================

#[test]
fn pipeline_error_invalid_config_has_message() {
    let err = PipelineError::InvalidConfig("missing filter type".into());
    let msg = err.to_string();
    assert!(msg.contains("missing filter type"));
}

#[test]
fn pipeline_error_non_monotonic_curve() {
    let err = PipelineError::NonMonotonicCurve;
    let msg = err.to_string();
    assert!(
        msg.contains("monotonic") || msg.contains("Monotonic"),
        "PipelineError::NonMonotonicCurve display missing keyword: {msg}"
    );
}

#[test]
fn pipeline_error_compilation_failed() {
    let err = PipelineError::CompilationFailed("invalid filter chain".into());
    assert!(err.to_string().contains("invalid filter chain"));
}

#[test]
fn rt_invalid_config_severity() {
    let err = RTError::InvalidConfig;
    assert_eq!(err.severity(), ErrorSeverity::Error);
    assert!(!err.is_recoverable());
}

// =========================================================================
// 4. Error aggregation (multiple errors in one cycle)
// =========================================================================

#[test]
fn multiple_faults_keep_first_fault_state() {
    let mut service = create_test_safety_service();
    service.report_fault(FaultType::TimingViolation);

    // Report a second fault — state remains Faulted with the first fault
    service.report_fault(FaultType::UsbStall);

    let state = service.state();
    assert!(
        matches!(state, SafetyState::Faulted { .. }),
        "Should remain in Faulted state after multiple faults"
    );
}

#[test]
fn collect_multiple_error_types_in_vec() {
    let errors: Vec<OpenRacingError> = vec![
        RTError::TimingViolation.into(),
        DeviceError::timeout("dev", 100).into(),
        ValidationError::required("field").into(),
    ];
    assert_eq!(errors.len(), 3);
    assert_eq!(errors[0].category(), ErrorCategory::RT);
    assert_eq!(errors[1].category(), ErrorCategory::Device);
    assert_eq!(errors[2].category(), ErrorCategory::Validation);
}

#[test]
fn error_severity_filtering() {
    let errors: Vec<OpenRacingError> = vec![
        RTError::TimingViolation.into(),       // Warning
        RTError::DeviceDisconnected.into(),    // Critical
        OpenRacingError::config("bad"),        // Error
        DeviceError::timeout("dev", 100).into(), // Warning
    ];
    let critical_count = errors
        .iter()
        .filter(|e| e.severity() == ErrorSeverity::Critical)
        .count();
    assert_eq!(critical_count, 1);

    let recoverable_count = errors.iter().filter(|e| e.is_recoverable()).count();
    assert_eq!(recoverable_count, 3);
}

// =========================================================================
// 5. Safety and watchdog error propagation
// =========================================================================

#[test]
fn watchdog_error_not_armed_display() {
    let err = WatchdogError::NotArmed;
    assert!(err.to_string().contains("not armed"));
}

#[test]
fn watchdog_error_timed_out_display() {
    let err = WatchdogError::TimedOut;
    assert!(err.to_string().contains("timed out"));
}

#[test]
fn software_watchdog_feed_without_arm_errors() {
    let mut watchdog = SoftwareWatchdog::new(100);
    let result = watchdog.feed();
    assert!(
        result.is_err(),
        "Feeding unarmed watchdog should return an error"
    );
    if let Err(e) = result {
        assert_eq!(e, WatchdogError::NotArmed);
    }
}

#[test]
fn software_watchdog_double_arm_errors() {
    let mut watchdog = SoftwareWatchdog::new(100);
    let first = watchdog.arm();
    assert!(first.is_ok(), "First arm should succeed");
    let second = watchdog.arm();
    assert!(second.is_err(), "Double arm should fail");
    if let Err(e) = second {
        assert_eq!(e, WatchdogError::AlreadyArmed);
    }
}

// =========================================================================
// 6. RT error code round-trip in engine context
// =========================================================================

#[test]
fn all_rt_error_codes_round_trip() {
    for code in 1..=10u8 {
        let err = RTError::from_code(code);
        assert!(err.is_some(), "code {code} should map to RTError");
        if let Some(e) = err {
            assert_eq!(e.code(), code, "round-trip failed for code {code}");
        }
    }
}

#[test]
fn rt_error_as_openracing_preserves_severity() {
    let rt_variants = [
        RTError::DeviceDisconnected,
        RTError::TorqueLimit,
        RTError::PipelineFault,
        RTError::TimingViolation,
    ];
    for rt in rt_variants {
        let ore: OpenRacingError = rt.into();
        assert_eq!(
            ore.severity(),
            rt.severity(),
            "Severity mismatch for RTError::{rt:?}"
        );
    }
}

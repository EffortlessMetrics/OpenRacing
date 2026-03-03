//! Recovery procedure integration tests.

use openracing_fmea::prelude::*;
use std::time::Duration;

#[test]
fn test_recovery_procedure_creation() {
    let procedure = RecoveryProcedure::default_for(FaultType::UsbStall);

    assert_eq!(procedure.fault_type, FaultType::UsbStall);
    assert!(procedure.automatic);
    assert!(!procedure.steps.is_empty());
}

#[test]
fn test_recovery_context_execution() {
    let mut ctx = RecoveryContext::new(FaultType::UsbStall);
    ctx.start(Duration::ZERO);

    assert_eq!(ctx.attempt, 1);
    assert_eq!(ctx.current_step, 0);

    // Advance through steps
    while !ctx.is_complete() {
        ctx.advance_step(Duration::from_millis(100));
    }

    assert!(ctx.is_complete());
}

#[test]
fn test_recovery_retry_logic() {
    let mut ctx = RecoveryContext::new(FaultType::UsbStall);
    ctx.procedure.max_attempts = 3;
    ctx.start(Duration::ZERO);

    // First attempt
    assert_eq!(ctx.attempt, 1);
    assert!(ctx.can_retry());

    // Start retry
    assert!(ctx.start_retry(Duration::from_millis(100)));
    assert_eq!(ctx.attempt, 2);

    // Another retry
    assert!(ctx.start_retry(Duration::from_millis(200)));
    assert_eq!(ctx.attempt, 3);

    // No more retries
    assert!(!ctx.can_retry());
    assert!(!ctx.start_retry(Duration::from_millis(300)));
}

#[test]
fn test_recovery_timeout_detection() {
    let mut ctx = RecoveryContext::new(FaultType::UsbStall);
    ctx.procedure.timeout = Duration::from_millis(100);
    ctx.start(Duration::ZERO);

    assert!(!ctx.is_timed_out(Duration::from_millis(50)));
    assert!(ctx.is_timed_out(Duration::from_millis(150)));
}

#[test]
fn test_recovery_step_timeout() {
    let procedure = RecoveryProcedure::default_for(FaultType::UsbStall);
    let mut ctx = RecoveryContext::with_procedure(procedure);
    ctx.start(Duration::ZERO);

    // Get first step timeout
    if let Some(step) = ctx.current_step() {
        let timeout = step.timeout;
        assert!(!ctx.is_step_timed_out(Duration::from_millis(10)));
        assert!(ctx.is_step_timed_out(timeout + Duration::from_millis(1)));
    }
}

#[test]
fn test_recovery_cancellation() {
    let mut ctx = RecoveryContext::new(FaultType::UsbStall);
    ctx.start(Duration::ZERO);

    assert!(!ctx.cancelled);

    ctx.cancel();
    assert!(ctx.cancelled);
}

#[test]
fn test_non_recoverable_fault() -> Result<(), Box<dyn std::error::Error>> {
    let mut fmea = FmeaSystem::new();

    // EncoderNaN is not recoverable
    fmea.handle_fault(FaultType::EncoderNaN, 10.0)?;

    assert!(!fmea.can_recover());

    Ok(())
}

#[test]
fn test_recoverable_fault() -> Result<(), Box<dyn std::error::Error>> {
    let mut fmea = FmeaSystem::new();

    // USB stall is recoverable
    fmea.handle_fault(FaultType::UsbStall, 10.0)?;
    fmea.update_soft_stop(Duration::from_millis(100));

    assert!(fmea.can_recover());
    assert!(fmea.recovery_procedure().is_some());

    Ok(())
}

#[test]
fn test_recovery_procedure_steps() {
    let procedure = RecoveryProcedure::default_for(FaultType::ThermalLimit);

    // Should have cooldown step
    let has_cooldown = procedure
        .steps
        .iter()
        .any(|s| s.name.as_str().contains("cooldown") || s.name.as_str().contains("cool"));
    assert!(has_cooldown);
}

#[test]
fn test_recovery_procedure_manual_only() {
    let procedure = RecoveryProcedure::default_for(FaultType::EncoderNaN);

    // Encoder calibration requires manual intervention
    assert!(!procedure.automatic);
}

#[test]
fn test_all_fault_types_have_recovery() {
    let fault_types = [
        FaultType::UsbStall,
        FaultType::EncoderNaN,
        FaultType::ThermalLimit,
        FaultType::Overcurrent,
        FaultType::PluginOverrun,
        FaultType::TimingViolation,
        FaultType::SafetyInterlockViolation,
        FaultType::HandsOffTimeout,
        FaultType::PipelineFault,
    ];

    for fault_type in fault_types {
        let procedure = RecoveryProcedure::default_for(fault_type);
        assert_eq!(procedure.fault_type, fault_type);
        assert!(!procedure.steps.is_empty() || fault_type == FaultType::TimingViolation);
    }
}

#[test]
fn test_recovery_result_success() {
    let result = RecoveryResult::success(Duration::from_millis(100), 1);

    assert!(result.is_success());
    assert_eq!(result.status, RecoveryStatus::Completed);
    assert!(result.error.is_none());
}

#[test]
fn test_recovery_result_failure() {
    let result = RecoveryResult::failed(Duration::from_millis(50), 3, "Connection failed");

    assert!(!result.is_success());
    assert_eq!(result.status, RecoveryStatus::Failed);
    assert!(result.error.is_some());
}

#[test]
fn test_recovery_result_timeout() {
    let result = RecoveryResult::timeout(Duration::from_secs(10), 2);

    assert_eq!(result.status, RecoveryStatus::Timeout);
    assert!(!result.is_success());
}

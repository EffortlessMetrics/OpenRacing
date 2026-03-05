#![allow(clippy::result_large_err)]
//! Deep tests for fault injection and safety interlock integration.
//!
//! Covers:
//! 1. Communication loss → fault detection
//! 2. Watchdog starvation → safe state transition
//! 3. Corrupted commands → rejection
//! 4. Multiple simultaneous faults
//! 5. Fault recovery sequences
//! 6. Safety system with mock device pipeline
//! 7. Torque limiting during fault conditions
//! 8. Safe state always results in zero torque output

use openracing_fmea::prelude::*;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn new_fmea() -> FmeaSystem {
    FmeaSystem::new()
}

fn new_fmea_conservative() -> FmeaSystem {
    FmeaSystem::with_thresholds(FaultThresholds::conservative())
}

// ===========================================================================
// 1. Communication loss → verify fault detection
// ===========================================================================

/// USB stall detected after exceeding consecutive failure threshold.
#[test]
fn comm_loss_usb_stall_consecutive_failures() -> TestResult {
    let mut fmea = new_fmea();
    // Default threshold: 3 consecutive failures
    let result = fmea.detect_usb_fault(3, Some(Duration::ZERO));
    assert_eq!(result, Some(FaultType::UsbStall));
    Ok(())
}

/// USB stall detected via timeout (old last-success timestamp).
#[test]
fn comm_loss_usb_stall_timeout() -> TestResult {
    let mut fmea = new_fmea();
    // Advance time past the USB timeout (default 10ms)
    fmea.update_time(Duration::from_millis(50));
    let result = fmea.detect_usb_fault(0, Some(Duration::from_millis(10)));
    assert_eq!(result, Some(FaultType::UsbStall));
    Ok(())
}

/// No USB fault when below threshold.
#[test]
fn comm_loss_no_fault_below_threshold() {
    let mut fmea = new_fmea();
    let result = fmea.detect_usb_fault(2, Some(Duration::ZERO));
    assert!(result.is_none());
}

/// No USB fault when communication is recent.
#[test]
fn comm_loss_no_fault_recent_success() {
    let mut fmea = new_fmea();
    fmea.update_time(Duration::from_millis(5));
    let result = fmea.detect_usb_fault(0, Some(Duration::from_millis(3)));
    assert!(result.is_none());
}

/// USB fault with None last-success (no timeout check, only count).
#[test]
fn comm_loss_usb_no_last_success() {
    let mut fmea = new_fmea();
    // With None, only consecutive count matters
    let result = fmea.detect_usb_fault(2, None);
    assert!(result.is_none());
    let result = fmea.detect_usb_fault(3, None);
    assert_eq!(result, Some(FaultType::UsbStall));
}

/// Conservative thresholds trigger sooner.
#[test]
fn comm_loss_conservative_triggers_sooner() {
    let mut fmea = new_fmea_conservative();
    // Conservative: max 2 consecutive failures
    let result = fmea.detect_usb_fault(2, Some(Duration::ZERO));
    assert_eq!(result, Some(FaultType::UsbStall));
}

// ===========================================================================
// 2. Watchdog starvation → verify safe state transition
// ===========================================================================

/// When a fault is handled with SoftStop action, soft-stop becomes active.
#[test]
fn starvation_fault_triggers_soft_stop() -> TestResult {
    let mut fmea = new_fmea();

    // Detect USB stall
    let fault = fmea.detect_usb_fault(3, Some(Duration::ZERO));
    assert_eq!(fault, Some(FaultType::UsbStall));

    // Handle fault — default action for UsbStall is SoftStop
    fmea.handle_fault(FaultType::UsbStall, 10.0)?;
    assert!(fmea.has_active_fault());
    assert!(fmea.is_soft_stop_active());
    Ok(())
}

/// Soft-stop ramps torque to zero over its duration.
#[test]
fn starvation_soft_stop_ramps_to_zero() -> TestResult {
    let mut fmea = new_fmea();
    fmea.handle_fault(FaultType::UsbStall, 10.0)?;

    // Update soft-stop through its ramp
    let torque_mid = fmea.update_soft_stop(Duration::from_millis(25));
    assert!(torque_mid < 10.0, "torque should decrease during ramp");
    assert!(torque_mid > 0.0, "torque should not be zero mid-ramp");

    // Complete the ramp
    let torque_end = fmea.update_soft_stop(Duration::from_millis(50));
    assert!(
        torque_end.abs() < f32::EPSILON,
        "torque should be zero after ramp completes"
    );
    assert!(!fmea.is_soft_stop_active());
    Ok(())
}

/// Overcurrent fault also triggers soft-stop with zero torque goal.
#[test]
fn starvation_overcurrent_triggers_soft_stop() -> TestResult {
    let mut fmea = new_fmea();
    fmea.handle_fault(FaultType::Overcurrent, 15.0)?;
    assert!(fmea.is_soft_stop_active());
    assert_eq!(fmea.soft_stop().start_torque(), 15.0);
    assert!(fmea.soft_stop().target_torque().abs() < f32::EPSILON);
    Ok(())
}

/// Thermal fault triggers soft-stop.
#[test]
fn starvation_thermal_triggers_soft_stop() -> TestResult {
    let mut fmea = new_fmea();
    let fault = fmea.detect_thermal_fault(85.0, false);
    assert_eq!(fault, Some(FaultType::ThermalLimit));

    fmea.handle_fault(FaultType::ThermalLimit, 8.0)?;
    assert!(fmea.is_soft_stop_active());
    Ok(())
}

// ===========================================================================
// 3. Corrupted commands → verify rejection
// ===========================================================================

/// Encoder NaN values are detected and trigger fault after threshold.
#[test]
fn corrupted_encoder_nan_detected() -> TestResult {
    let mut fmea = new_fmea();

    // Below threshold: no fault
    for _ in 0..4 {
        assert!(fmea.detect_encoder_fault(f32::NAN).is_none());
    }
    // At threshold: fault triggered
    let fault = fmea.detect_encoder_fault(f32::NAN);
    assert_eq!(fault, Some(FaultType::EncoderNaN));
    Ok(())
}

/// Encoder infinity is also treated as NaN-like.
#[test]
fn corrupted_encoder_infinity_detected() -> TestResult {
    let mut fmea = new_fmea();
    for _ in 0..4 {
        assert!(fmea.detect_encoder_fault(f32::INFINITY).is_none());
    }
    let fault = fmea.detect_encoder_fault(f32::INFINITY);
    assert_eq!(fault, Some(FaultType::EncoderNaN));
    Ok(())
}

/// Encoder negative infinity also triggers.
#[test]
fn corrupted_encoder_neg_infinity_detected() -> TestResult {
    let mut fmea = new_fmea();
    for _ in 0..4 {
        assert!(fmea.detect_encoder_fault(f32::NEG_INFINITY).is_none());
    }
    let fault = fmea.detect_encoder_fault(f32::NEG_INFINITY);
    assert_eq!(fault, Some(FaultType::EncoderNaN));
    Ok(())
}

/// Normal encoder values never trigger fault.
#[test]
fn corrupted_normal_encoder_values_pass() {
    let mut fmea = new_fmea();
    let values = [0.0f32, 1.0, -1.0, 100.0, -100.0, f32::MIN, f32::MAX];
    for &v in &values {
        assert!(
            fmea.detect_encoder_fault(v).is_none(),
            "Value {v} should not trigger fault"
        );
    }
}

/// Plugin overrun detected after sufficient violations.
#[test]
fn corrupted_plugin_overrun_after_threshold() -> TestResult {
    let mut fmea = new_fmea();
    // Default: plugin_max_overruns = 10, plugin_timeout_us = 100
    for i in 0..9 {
        let r = fmea.detect_plugin_overrun("p1", 500);
        assert!(r.is_none(), "Should not trigger on iteration {i}");
    }
    let fault = fmea.detect_plugin_overrun("p1", 500);
    assert_eq!(fault, Some(FaultType::PluginOverrun));
    Ok(())
}

/// Plugin under budget does not accumulate violations.
#[test]
fn corrupted_plugin_under_budget_no_fault() {
    let mut fmea = new_fmea();
    for _ in 0..100 {
        assert!(fmea.detect_plugin_overrun("p1", 50).is_none());
    }
}

/// Timing violation after threshold count.
#[test]
fn corrupted_timing_violation_after_threshold() -> TestResult {
    let mut fmea = new_fmea();
    // Default: timing_max_violations = 100, threshold_us = 250
    for i in 0..99 {
        let r = fmea.detect_timing_violation(500);
        assert!(r.is_none(), "Should not trigger on iteration {i}");
    }
    let fault = fmea.detect_timing_violation(500);
    assert_eq!(fault, Some(FaultType::TimingViolation));
    Ok(())
}

/// Timing jitter within threshold does not count as violation.
#[test]
fn corrupted_timing_within_threshold_no_fault() {
    let mut fmea = new_fmea();
    for _ in 0..200 {
        assert!(fmea.detect_timing_violation(200).is_none());
    }
}

// ===========================================================================
// 4. Multiple simultaneous faults
// ===========================================================================

/// Second fault overwrites the first active fault.
#[test]
fn multi_fault_second_overwrites_first() -> TestResult {
    let mut fmea = new_fmea();
    fmea.handle_fault(FaultType::UsbStall, 10.0)?;
    assert_eq!(fmea.active_fault(), Some(FaultType::UsbStall));

    fmea.handle_fault(FaultType::ThermalLimit, 8.0)?;
    assert_eq!(fmea.active_fault(), Some(FaultType::ThermalLimit));
    Ok(())
}

/// Handling all fault types sequentially (each replaces previous).
#[test]
fn multi_fault_all_types_handled() -> TestResult {
    let mut fmea = new_fmea();
    let all_faults = [
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

    for &ft in &all_faults {
        fmea.handle_fault(ft, 5.0)?;
        assert_eq!(fmea.active_fault(), Some(ft));
    }
    Ok(())
}

/// Clearing fault then handling a new one works cleanly.
#[test]
fn multi_fault_clear_then_new() -> TestResult {
    let mut fmea = new_fmea();
    fmea.handle_fault(FaultType::UsbStall, 10.0)?;
    fmea.clear_fault()?;
    assert!(!fmea.has_active_fault());

    fmea.handle_fault(FaultType::Overcurrent, 12.0)?;
    assert_eq!(fmea.active_fault(), Some(FaultType::Overcurrent));
    Ok(())
}

/// Clearing when no fault is active returns error.
#[test]
fn multi_fault_clear_no_active_fails() {
    let mut fmea = new_fmea();
    let result = fmea.clear_fault();
    assert!(matches!(result, Err(FmeaError::NoActiveFault)));
}

/// Detect and handle USB fault, then detect and handle thermal fault.
#[test]
fn multi_fault_detect_handle_sequence() -> TestResult {
    let mut fmea = new_fmea();

    // USB fault
    let fault = fmea.detect_usb_fault(3, Some(Duration::ZERO));
    assert_eq!(fault, Some(FaultType::UsbStall));
    fmea.handle_fault(FaultType::UsbStall, 10.0)?;

    // While USB fault is active, thermal fault also detected
    let fault = fmea.detect_thermal_fault(90.0, false);
    assert_eq!(fault, Some(FaultType::ThermalLimit));
    fmea.handle_fault(FaultType::ThermalLimit, 10.0)?;

    // Thermal is now the active fault
    assert_eq!(fmea.active_fault(), Some(FaultType::ThermalLimit));
    Ok(())
}

// ===========================================================================
// 5. Fault recovery sequences
// ===========================================================================

/// Recovery context lifecycle: start → advance → complete.
#[test]
fn recovery_context_full_lifecycle() -> TestResult {
    let mut ctx = RecoveryContext::new(FaultType::UsbStall);
    ctx.start(Duration::from_millis(0));

    assert!(!ctx.is_complete());
    assert!(!ctx.is_timed_out(Duration::from_millis(100)));

    // Advance through all steps
    let step_count = ctx.procedure.steps.len();
    for i in 0..step_count {
        assert!(!ctx.is_complete(), "Should not be complete at step {i}");
        ctx.advance_step(Duration::from_millis((i as u64 + 1) * 100));
    }

    assert!(ctx.is_complete());
    Ok(())
}

/// Recovery timeout detection.
#[test]
fn recovery_timeout_detected() {
    let mut ctx = RecoveryContext::new(FaultType::UsbStall);
    ctx.procedure.timeout = Duration::from_millis(100);
    ctx.start(Duration::ZERO);

    assert!(!ctx.is_timed_out(Duration::from_millis(50)));
    assert!(ctx.is_timed_out(Duration::from_millis(150)));
}

/// Recovery retry mechanism.
#[test]
fn recovery_retry_mechanism() {
    let mut ctx = RecoveryContext::new(FaultType::UsbStall);
    ctx.procedure.max_attempts = 3;
    ctx.start(Duration::ZERO);

    assert!(ctx.can_retry());
    assert!(ctx.start_retry(Duration::from_millis(100)));
    assert_eq!(ctx.attempt, 2);

    assert!(ctx.can_retry());
    assert!(ctx.start_retry(Duration::from_millis(200)));
    assert_eq!(ctx.attempt, 3);

    assert!(!ctx.can_retry());
    assert!(!ctx.start_retry(Duration::from_millis(300)));
}

/// Recovery cancel.
#[test]
fn recovery_cancel() {
    let mut ctx = RecoveryContext::new(FaultType::UsbStall);
    ctx.start(Duration::ZERO);
    assert!(!ctx.cancelled);

    ctx.cancel();
    assert!(ctx.cancelled);
}

/// RecoveryResult constructors.
#[test]
fn recovery_results_constructors() {
    let success = RecoveryResult::success(Duration::from_millis(50), 1);
    assert!(success.is_success());
    assert_eq!(success.status, RecoveryStatus::Completed);
    assert_eq!(success.attempts, 1);

    let failed = RecoveryResult::failed(Duration::from_millis(100), 3, "test");
    assert!(!failed.is_success());
    assert_eq!(failed.status, RecoveryStatus::Failed);
    assert!(failed.error.is_some());

    let timeout = RecoveryResult::timeout(Duration::from_secs(10), 2);
    assert_eq!(timeout.status, RecoveryStatus::Timeout);
    assert!(!timeout.is_success());
}

/// Each fault type has a default recovery procedure.
#[test]
fn recovery_default_procedures_for_all_types() {
    let all_faults = [
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

    for &ft in &all_faults {
        let proc = RecoveryProcedure::default_for(ft);
        assert_eq!(proc.fault_type, ft);
        assert!(proc.max_attempts > 0, "No attempts for {ft:?}");
        assert!(!proc.steps.is_empty(), "No recovery steps for {ft:?}");
    }
}

/// Recoverable faults can be auto-recovered; non-recoverable require manual.
#[test]
fn recovery_auto_vs_manual() {
    // Auto-recoverable
    let proc = RecoveryProcedure::default_for(FaultType::UsbStall);
    assert!(proc.automatic);
    let proc = RecoveryProcedure::default_for(FaultType::ThermalLimit);
    assert!(proc.automatic);
    let proc = RecoveryProcedure::default_for(FaultType::PluginOverrun);
    assert!(proc.automatic);

    // Manual required
    let proc = RecoveryProcedure::default_for(FaultType::EncoderNaN);
    assert!(!proc.automatic);
    let proc = RecoveryProcedure::default_for(FaultType::Overcurrent);
    assert!(!proc.automatic);
    let proc = RecoveryProcedure::default_for(FaultType::SafetyInterlockViolation);
    assert!(!proc.automatic);
}

/// can_recover reflects the active fault's recoverability.
#[test]
fn recovery_can_recover_reflects_active_fault() -> TestResult {
    let mut fmea = new_fmea();
    assert!(!fmea.can_recover()); // No active fault

    fmea.handle_fault(FaultType::UsbStall, 10.0)?;
    assert!(fmea.can_recover()); // UsbStall is recoverable

    fmea.clear_fault()?;
    fmea.handle_fault(FaultType::EncoderNaN, 5.0)?;
    assert!(!fmea.can_recover()); // EncoderNaN is not auto-recoverable
    Ok(())
}

// ===========================================================================
// 6. Safety system with mock device pipeline
// ===========================================================================

/// FMEA matrix contains all expected fault types by default.
#[test]
fn pipeline_fmea_matrix_complete() {
    let fmea = new_fmea();
    let matrix = fmea.fmea_matrix();

    let expected = [
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

    for &ft in &expected {
        assert!(matrix.contains(ft), "Matrix missing {ft:?}");
        let entry = matrix.get(ft);
        assert!(entry.is_some(), "No entry for {ft:?}");
        if let Some(e) = entry {
            assert!(e.enabled, "{ft:?} should be enabled by default");
        }
    }
}

/// Each fault type has a valid non-zero response time.
#[test]
fn pipeline_response_times_valid() {
    let fmea = new_fmea();
    let matrix = fmea.fmea_matrix();

    for ft in matrix.fault_types() {
        let entry = matrix.get(ft);
        if let Some(e) = entry {
            assert!(e.max_response_time_ms > 0, "{ft:?} has zero response time");
        }
    }
}

/// Critical faults require immediate response (≤50ms).
#[test]
fn pipeline_critical_faults_have_fast_response() {
    let critical = [FaultType::Overcurrent, FaultType::SafetyInterlockViolation];
    for &ft in &critical {
        let ms = ft.default_max_response_time_ms();
        assert!(ms <= 50, "{ft:?} response time {ms}ms exceeds 50ms limit");
    }
}

/// Disabled entries are not handled.
#[test]
fn pipeline_disabled_entry_not_handled() -> TestResult {
    let mut fmea = new_fmea();

    // Disable USB stall entry
    if let Some(entry) = fmea.fmea_matrix_mut().get_mut(FaultType::UsbStall) {
        entry.enabled = false;
    }

    fmea.handle_fault(FaultType::UsbStall, 10.0)?;
    // Fault is not activated because entry is disabled
    assert!(!fmea.has_active_fault());
    Ok(())
}

/// Fault statistics reflect detection state.
#[test]
fn pipeline_fault_statistics() -> TestResult {
    let mut fmea = new_fmea();

    // Trigger some detections
    let _ = fmea.detect_usb_fault(2, Some(Duration::ZERO));
    fmea.update_time(Duration::from_millis(5));
    let _ = fmea.detect_usb_fault(3, Some(Duration::ZERO));

    let stats: Vec<_> = fmea.fault_statistics().collect();
    assert!(!stats.is_empty());

    // Find USB stats
    let usb_stat = stats.iter().find(|(ft, _, _)| *ft == FaultType::UsbStall);
    assert!(usb_stat.is_some());
    Ok(())
}

/// reset_detection_state clears counts for a specific fault type.
#[test]
fn pipeline_reset_detection_state() -> TestResult {
    let mut fmea = new_fmea();

    // Accumulate some USB failures
    let _ = fmea.detect_usb_fault(2, Some(Duration::ZERO));
    fmea.reset_detection_state(FaultType::UsbStall);

    // After reset, should not trigger at the same count
    let result = fmea.detect_usb_fault(2, Some(Duration::ZERO));
    assert!(result.is_none());
    Ok(())
}

/// reset_all_detection_states clears everything.
#[test]
fn pipeline_reset_all_detection_states() -> TestResult {
    let mut fmea = new_fmea();

    // Trigger multiple detections
    let _ = fmea.detect_usb_fault(2, Some(Duration::ZERO));
    for _ in 0..3 {
        let _ = fmea.detect_encoder_fault(f32::NAN);
    }

    fmea.reset_all_detection_states();

    // All stats should show zero counts
    for (_, count, _) in fmea.fault_statistics() {
        assert_eq!(count, 0, "Detection state not fully reset");
    }
    Ok(())
}

// ===========================================================================
// 7. Torque limiting during fault conditions
// ===========================================================================

/// SoftStop starts at current torque and targets zero.
#[test]
fn torque_soft_stop_starts_at_current() -> TestResult {
    let mut fmea = new_fmea();
    fmea.handle_fault(FaultType::UsbStall, 12.5)?;

    let ss = fmea.soft_stop();
    assert!((ss.start_torque() - 12.5).abs() < f32::EPSILON);
    assert!(ss.target_torque().abs() < f32::EPSILON);
    assert!(ss.is_active());
    Ok(())
}

/// During ramp, torque monotonically decreases toward zero.
#[test]
fn torque_monotonic_decrease_during_ramp() -> TestResult {
    let mut fmea = new_fmea();
    fmea.handle_fault(FaultType::UsbStall, 10.0)?;

    let mut prev_torque = 10.0f32;
    let tick = Duration::from_millis(1);

    for _ in 0..50 {
        let t = fmea.update_soft_stop(tick);
        assert!(
            t <= prev_torque + f32::EPSILON,
            "Torque increased: {prev_torque} -> {t}"
        );
        prev_torque = t;
    }
    Ok(())
}

/// Force stop immediately sets torque to zero.
#[test]
fn torque_force_stop_immediate_zero() -> TestResult {
    let mut fmea = new_fmea();
    fmea.handle_fault(FaultType::Overcurrent, 15.0)?;
    assert!(fmea.is_soft_stop_active());

    fmea.force_stop_soft_stop();
    assert!(!fmea.is_soft_stop_active());
    assert!(fmea.soft_stop().current_torque().abs() < f32::EPSILON);
    Ok(())
}

/// Starting a new fault during active soft-stop restarts the ramp.
#[test]
fn torque_new_fault_restarts_ramp() -> TestResult {
    let mut fmea = new_fmea();
    fmea.handle_fault(FaultType::UsbStall, 10.0)?;

    // Partially ramp
    let _ = fmea.update_soft_stop(Duration::from_millis(25));
    let mid_torque = fmea.soft_stop().current_torque();
    assert!(mid_torque < 10.0);
    assert!(mid_torque > 0.0);

    // New fault with higher torque restarts ramp
    fmea.handle_fault(FaultType::Overcurrent, 20.0)?;
    assert!(fmea.is_soft_stop_active());
    assert!((fmea.soft_stop().start_torque() - 20.0).abs() < f32::EPSILON);
    Ok(())
}

/// SoftStopController with zero duration completes immediately.
#[test]
fn torque_zero_duration_instant_stop() {
    let mut ss = SoftStopController::new();
    ss.start_soft_stop_with_duration(10.0, Duration::ZERO);

    let torque = ss.update(Duration::ZERO);
    assert!(torque.abs() < f32::EPSILON);
    assert!(!ss.is_active());
}

/// SoftStopController with zero start torque.
#[test]
fn torque_zero_start_torque() {
    let mut ss = SoftStopController::new();
    ss.start_soft_stop(0.0);

    let torque = ss.update(Duration::from_millis(25));
    assert!(torque.abs() < f32::EPSILON);
    assert_eq!(ss.current_multiplier(), 0.0);
}

/// SoftStopController negative torque (reverse direction).
#[test]
fn torque_negative_start_ramps_to_zero() {
    let mut ss = SoftStopController::new();
    ss.start_soft_stop(-10.0);

    let torque = ss.update(Duration::from_millis(25));
    assert!(torque > -10.0, "Torque should increase toward zero");
    assert!(torque < 0.0, "Torque should still be negative mid-ramp");

    // Complete ramp
    let torque = ss.update(Duration::from_millis(50));
    assert!(torque.abs() < f32::EPSILON);
}

// ===========================================================================
// 8. Safe state always results in zero torque output
// ===========================================================================

/// After complete soft-stop ramp, torque is zero.
#[test]
fn safe_state_zero_torque_after_ramp() -> TestResult {
    let mut fmea = new_fmea();
    fmea.handle_fault(FaultType::UsbStall, 10.0)?;

    // Run the ramp to completion
    let mut torque = 10.0f32;
    for _ in 0..100 {
        torque = fmea.update_soft_stop(Duration::from_millis(1));
        if !fmea.is_soft_stop_active() {
            break;
        }
    }
    assert!(
        torque.abs() < f32::EPSILON,
        "Torque should be zero after ramp: {torque}"
    );
    Ok(())
}

/// Force stop results in zero torque.
#[test]
fn safe_state_force_stop_zero_torque() -> TestResult {
    let mut fmea = new_fmea();
    fmea.handle_fault(FaultType::Overcurrent, 15.0)?;

    fmea.force_stop_soft_stop();
    assert!(fmea.soft_stop().current_torque().abs() < f32::EPSILON);
    Ok(())
}

/// After fault clear, soft-stop is reset and torque is zero.
#[test]
fn safe_state_clear_fault_resets_soft_stop() -> TestResult {
    let mut fmea = new_fmea();
    fmea.handle_fault(FaultType::UsbStall, 10.0)?;

    // Partially ramp
    let _ = fmea.update_soft_stop(Duration::from_millis(10));

    // Clear fault resets soft-stop
    fmea.clear_fault()?;
    assert!(!fmea.is_soft_stop_active());
    assert!(fmea.soft_stop().current_torque().abs() < f32::EPSILON);
    Ok(())
}

/// All fault types with SoftStop or SafeMode action result in zero torque.
#[test]
fn safe_state_all_torque_affecting_faults_reach_zero() -> TestResult {
    let torque_faults = [
        FaultType::UsbStall,
        FaultType::EncoderNaN,
        FaultType::ThermalLimit,
        FaultType::Overcurrent,
        FaultType::SafetyInterlockViolation,
        FaultType::HandsOffTimeout,
    ];

    for &ft in &torque_faults {
        let mut fmea = new_fmea();
        fmea.handle_fault(ft, 10.0)?;

        // Run ramp to completion
        for _ in 0..200 {
            let _ = fmea.update_soft_stop(Duration::from_millis(1));
            if !fmea.is_soft_stop_active() {
                break;
            }
        }
        assert!(
            fmea.soft_stop().current_torque().abs() < f32::EPSILON,
            "{ft:?}: torque not zero after ramp"
        );
    }
    Ok(())
}

/// Fault types with non-torque actions (LogAndContinue, Quarantine) do NOT
/// activate soft-stop.
#[test]
fn safe_state_non_torque_faults_no_soft_stop() -> TestResult {
    let non_torque_faults = [FaultType::TimingViolation, FaultType::PluginOverrun];

    for &ft in &non_torque_faults {
        let mut fmea = new_fmea();
        fmea.handle_fault(ft, 5.0)?;
        assert!(
            !fmea.is_soft_stop_active(),
            "{ft:?}: should not activate soft-stop"
        );
    }
    Ok(())
}

// ===========================================================================
// 9. Threshold validation
// ===========================================================================

/// Default thresholds pass validation.
#[test]
fn thresholds_default_valid() {
    let t = FaultThresholds::default();
    assert!(t.validate().is_ok());
}

/// Conservative thresholds pass validation.
#[test]
fn thresholds_conservative_valid() {
    let t = FaultThresholds::conservative();
    assert!(t.validate().is_ok());
}

/// Relaxed thresholds pass validation.
#[test]
fn thresholds_relaxed_valid() {
    let t = FaultThresholds::relaxed();
    assert!(t.validate().is_ok());
}

/// Zero USB timeout is invalid.
#[test]
fn thresholds_zero_usb_timeout_invalid() {
    let t = FaultThresholds {
        usb_timeout_ms: 0,
        ..FaultThresholds::default()
    };
    assert!(t.validate().is_err());
}

/// Thermal limit below 40°C is invalid.
#[test]
fn thresholds_thermal_below_40_invalid() {
    let t = FaultThresholds {
        thermal_limit_celsius: 30.0,
        ..FaultThresholds::default()
    };
    assert!(t.validate().is_err());
}

/// Thermal limit above 120°C is invalid.
#[test]
fn thresholds_thermal_above_120_invalid() {
    let t = FaultThresholds {
        thermal_limit_celsius: 130.0,
        ..FaultThresholds::default()
    };
    assert!(t.validate().is_err());
}

/// Negative overcurrent limit is invalid.
#[test]
fn thresholds_negative_overcurrent_invalid() {
    let t = FaultThresholds {
        overcurrent_limit_a: -1.0,
        ..FaultThresholds::default()
    };
    assert!(t.validate().is_err());
}

/// Custom thresholds are applied to the FMEA system.
#[test]
fn thresholds_custom_applied() -> TestResult {
    let thresholds = FaultThresholds {
        usb_max_consecutive_failures: 5,
        ..FaultThresholds::default()
    };

    let mut fmea = FmeaSystem::with_thresholds(thresholds);

    // 3 failures should NOT trigger with threshold=5
    let result = fmea.detect_usb_fault(3, Some(Duration::ZERO));
    assert!(result.is_none());

    // 5 failures SHOULD trigger
    let result = fmea.detect_usb_fault(5, Some(Duration::ZERO));
    assert_eq!(result, Some(FaultType::UsbStall));
    Ok(())
}

// ===========================================================================
// 10. Fault classification properties
// ===========================================================================

/// Every fault type has severity in 1..=4.
#[test]
fn classification_severity_range() {
    let all_faults = [
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
    for &ft in &all_faults {
        let s = ft.severity();
        assert!((1..=4).contains(&s), "{ft:?} severity {s} out of range");
    }
}

/// Faults requiring immediate response are severity 1 or 2.
#[test]
fn classification_immediate_response_high_severity() {
    let all_faults = [
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
    for &ft in &all_faults {
        if ft.requires_immediate_response() {
            assert!(
                ft.severity() <= 2,
                "{ft:?} requires immediate response but has severity {}",
                ft.severity()
            );
        }
    }
}

/// FaultAction properties are self-consistent.
#[test]
fn classification_fault_action_consistency() {
    let actions = [
        FaultAction::SoftStop,
        FaultAction::Quarantine,
        FaultAction::LogAndContinue,
        FaultAction::Restart,
        FaultAction::SafeMode,
    ];

    for &action in &actions {
        // An action can't both affect torque and allow operation
        // (SoftStop/SafeMode affect torque and don't allow operation)
        if action.affects_torque() {
            assert!(
                !action.allows_operation(),
                "{action:?} affects torque but allows operation"
            );
        }
    }
}

/// FaultMarker creation and field population.
#[test]
fn classification_fault_marker_creation() {
    let mut marker = FaultMarker::new(FaultType::UsbStall, Duration::from_millis(100));
    assert_eq!(marker.fault_type, FaultType::UsbStall);
    assert_eq!(marker.timestamp, Duration::from_millis(100));

    assert!(marker.add_device_state("usb_status", "disconnected"));
    assert!(marker.add_plugin_state("plugin1", "running"));
    assert!(marker.add_recovery_action("reset_usb"));
    assert_eq!(marker.device_state.len(), 1);
    assert_eq!(marker.plugin_states.len(), 1);
    assert_eq!(marker.recovery_actions.len(), 1);
}

/// FmeaError properties.
#[test]
fn error_properties() {
    // Recoverable errors
    assert!(FmeaError::timeout("test", 100).is_recoverable());
    assert!(FmeaError::recovery_failed(FaultType::UsbStall, "reason").is_recoverable());
    assert!(FmeaError::quarantine_error("p1", "reason").is_recoverable());

    // Non-recoverable errors
    assert!(!FmeaError::configuration_error("bad").is_recoverable());
    assert!(!FmeaError::NoActiveFault.is_recoverable());

    // Requires attention
    assert!(FmeaError::soft_stop_failed("reason").requires_immediate_attention());
    assert!(FmeaError::configuration_error("bad").requires_immediate_attention());
    assert!(!FmeaError::timeout("test", 100).requires_immediate_attention());
}

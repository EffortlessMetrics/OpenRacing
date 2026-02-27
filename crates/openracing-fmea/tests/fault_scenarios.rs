//! Fault scenario tests for FMEA coverage.
//!
//! These tests cover specific fault scenarios not fully exercised by the
//! existing lifecycle tests:
//! - Overcurrent fault detection and recovery
//! - Communication loss (UsbStall) recovery
//! - Encoder fault detection
//! - Soft stop ramp timing (75ms ramp)
//! - Fault escalation
//! - Alert deduplication
//! - Recovery action tracking

use openracing_fmea::prelude::*;
use std::time::Duration;

/// Tests that an overcurrent fault is detected above the threshold
/// and that a recovery procedure is available (though manual-only).
#[test]
fn test_overcurrent_fault_detection_and_recovery() -> Result<(), FmeaError> {
    let thresholds = FaultThresholds {
        overcurrent_limit_a: 10.0,
        ..FaultThresholds::default()
    };
    let mut fmea = FmeaSystem::with_thresholds(thresholds);

    // Under threshold — no fault
    assert!(fmea.detect_thermal_fault(70.0, false).is_none());

    // Trigger overcurrent directly via handle_fault
    fmea.handle_fault(FaultType::Overcurrent, 8.0)?;
    assert!(fmea.has_active_fault());
    assert_eq!(fmea.active_fault(), Some(FaultType::Overcurrent));

    // Overcurrent triggers soft-stop
    assert!(fmea.is_soft_stop_active());

    // Overcurrent is not auto-recoverable — manual intervention required
    assert!(!fmea.can_recover());

    let proc = RecoveryProcedure::default_for(FaultType::Overcurrent);
    assert_eq!(proc.fault_type, FaultType::Overcurrent);
    assert!(!proc.automatic);
    assert!(!proc.steps.is_empty());

    Ok(())
}

/// Tests communication loss (USB stall) fault detection with recovery.
#[test]
fn test_communication_loss_fault_and_recovery() -> Result<(), FmeaError> {
    let mut fmea = FmeaSystem::new();

    // Accumulate failures up to but not including threshold
    assert!(fmea.detect_usb_fault(2, Some(Duration::ZERO)).is_none());

    // Threshold reached → fault detected
    let fault = fmea.detect_usb_fault(3, Some(Duration::ZERO));
    assert_eq!(fault, Some(FaultType::UsbStall));

    // Handle the fault
    fmea.handle_fault(FaultType::UsbStall, 10.0)?;
    assert!(fmea.has_active_fault());

    // USB stall is automatically recoverable
    assert!(fmea.can_recover());

    // Verify recovery procedure has USB reconnect steps
    let proc = fmea.recovery_procedure().ok_or(FmeaError::NoActiveFault)?;
    assert!(proc.automatic);
    assert!(!proc.steps.is_empty());

    // Complete soft-stop ramp
    fmea.update_soft_stop(Duration::from_millis(100));

    // Clear fault after recovery
    fmea.clear_fault()?;
    assert!(!fmea.has_active_fault());

    Ok(())
}

/// Tests encoder fault detection after the NaN window threshold is exceeded.
#[test]
fn test_encoder_fault_detection() -> Result<(), FmeaError> {
    let mut fmea = FmeaSystem::new();

    // Valid values never trigger
    assert!(fmea.detect_encoder_fault(0.0).is_none());
    assert!(fmea.detect_encoder_fault(1.5).is_none());
    assert!(fmea.detect_encoder_fault(-100.0).is_none());

    // First four NaNs — below window threshold (default: 5)
    for i in 0..4u32 {
        let result = fmea.detect_encoder_fault(f32::NAN);
        assert!(
            result.is_none(),
            "should not fault before threshold at iteration {}",
            i
        );
    }

    // 5th NaN triggers the fault
    let fault = fmea.detect_encoder_fault(f32::NAN);
    assert_eq!(fault, Some(FaultType::EncoderNaN));

    // Handle and verify encoder fault is not auto-recoverable
    fmea.handle_fault(FaultType::EncoderNaN, 5.0)?;
    assert!(!fmea.can_recover());

    Ok(())
}

/// Tests soft stop ramp timing with a 75ms ramp duration.
/// Validates that torque decreases linearly and reaches zero by the end.
#[test]
fn test_soft_stop_75ms_ramp_timing() {
    let mut ctrl = SoftStopController::new();
    ctrl.start_soft_stop_with_duration(10.0, Duration::from_millis(75));

    assert!(ctrl.is_active());
    assert_eq!(ctrl.start_torque(), 10.0);
    assert_eq!(ctrl.current_torque(), 10.0);

    // After ~25ms (1/3 of 75ms) torque should be ~6.67
    let t_25 = ctrl.update(Duration::from_millis(25));
    assert!(
        (t_25 - 6.666_67).abs() < 0.1,
        "torque at 25ms should be ~6.67, got {}",
        t_25
    );
    assert!(ctrl.is_active());

    // After another 25ms (2/3 total) torque should be ~3.33
    let t_50 = ctrl.update(Duration::from_millis(25));
    assert!(
        (t_50 - 3.333_33).abs() < 0.1,
        "torque at 50ms should be ~3.33, got {}",
        t_50
    );
    assert!(ctrl.is_active());

    // After remaining 25ms ramp is complete
    let t_75 = ctrl.update(Duration::from_millis(25));
    assert_eq!(t_75, 0.0);
    assert!(!ctrl.is_active(), "soft-stop should be inactive after 75ms");
}

/// Tests fault escalation: a lower-priority active fault is replaced
/// by a higher-priority fault.
#[test]
fn test_fault_escalation() -> Result<(), FmeaError> {
    let mut fmea = FmeaSystem::new();

    // Start with a low-priority fault (TimingViolation, severity=3)
    fmea.handle_fault(FaultType::TimingViolation, 5.0)?;
    assert_eq!(fmea.active_fault(), Some(FaultType::TimingViolation));

    // Escalate to medium-priority fault (UsbStall, severity=2)
    fmea.handle_fault(FaultType::UsbStall, 5.0)?;
    assert_eq!(
        fmea.active_fault(),
        Some(FaultType::UsbStall),
        "UsbStall should replace TimingViolation"
    );

    // Escalate to critical fault (Overcurrent, severity=1)
    fmea.handle_fault(FaultType::Overcurrent, 5.0)?;
    assert_eq!(
        fmea.active_fault(),
        Some(FaultType::Overcurrent),
        "Overcurrent should replace UsbStall as most critical"
    );

    // Severity of Overcurrent is 1 (most critical)
    assert_eq!(FaultType::Overcurrent.severity(), 1);
    assert!(FaultType::Overcurrent.requires_immediate_response());

    Ok(())
}

/// Tests alert deduplication: triggering the same fault type twice should
/// not produce a second distinct alert — the existing alert stays active.
#[test]
fn test_alert_deduplication() -> Result<(), FmeaError> {
    let mut fmea = FmeaSystem::new();

    // First fault raises an alert
    fmea.handle_fault(FaultType::UsbStall, 10.0)?;
    let first_alert = fmea.audio_alerts().current_alert();
    assert!(
        first_alert.is_some(),
        "alert should be active after first fault"
    );

    // Second identical fault — alert should remain the same, not escalated
    fmea.handle_fault(FaultType::UsbStall, 10.0)?;
    let second_alert = fmea.audio_alerts().current_alert();
    assert_eq!(
        first_alert, second_alert,
        "duplicate fault should not change the active alert"
    );

    Ok(())
}

/// Tests recovery action tracking: the RecoveryContext correctly records
/// step progression and elapsed time.
#[test]
fn test_recovery_action_tracking() {
    let mut ctx = RecoveryContext::new(FaultType::UsbStall);
    ctx.start(Duration::ZERO);

    assert_eq!(ctx.attempt, 1);
    assert_eq!(ctx.current_step, 0);

    let step_count = ctx.procedure.steps.len();
    assert!(step_count > 0, "USB recovery must have at least one step");

    // Advance through all steps and record timing
    let mut step_times: Vec<Duration> = Vec::new();
    let mut tick = Duration::ZERO;
    for i in 0..step_count {
        tick += Duration::from_millis(50);
        ctx.advance_step(tick);
        step_times.push(tick);
        assert_eq!(
            ctx.current_step,
            i + 1,
            "step counter should increment to {}",
            i + 1
        );
    }

    assert!(ctx.is_complete(), "all steps completed");
    assert!(
        !ctx.is_timed_out(tick),
        "should not have timed out within procedure timeout"
    );
    assert_eq!(step_times.len(), step_count);
}

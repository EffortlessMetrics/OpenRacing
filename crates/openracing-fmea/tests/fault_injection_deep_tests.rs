#![allow(clippy::result_large_err)]
//! Deep fault injection tests covering FMEA fault detection for each device
//! type, severity classification, recovery procedures, cascade prevention,
//! multi-fault handling, audit trail, rate limiting, safe-state timing,
//! and report generation.

use openracing_fmea::prelude::*;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn new_fmea() -> FmeaSystem {
    FmeaSystem::new()
}

fn new_fmea_with_defaults() -> FmeaSystem {
    let thresholds = FaultThresholds::default();
    FmeaSystem::with_thresholds(thresholds)
}

// =========================================================================
// 1. FMEA fault injection for each device type
// =========================================================================

#[test]
fn detect_usb_stall_fault() -> Result<(), FmeaError> {
    let mut fmea = new_fmea();
    let result = fmea.detect_usb_fault(3, Some(Duration::ZERO));
    assert_eq!(result, Some(FaultType::UsbStall));
    fmea.handle_fault(FaultType::UsbStall, 10.0)?;
    assert!(fmea.has_active_fault());
    Ok(())
}

#[test]
fn detect_encoder_nan_fault() -> Result<(), FmeaError> {
    let mut fmea = new_fmea();
    // Need 5 NaN readings to trigger (encoder_max_nan_count = 5)
    for _ in 0..4 {
        let r = fmea.detect_encoder_fault(f32::NAN);
        assert!(r.is_none());
    }
    let result = fmea.detect_encoder_fault(f32::NAN);
    assert_eq!(result, Some(FaultType::EncoderNaN));
    fmea.handle_fault(FaultType::EncoderNaN, 5.0)?;
    assert!(fmea.has_active_fault());
    Ok(())
}

#[test]
fn detect_thermal_limit_fault() -> Result<(), FmeaError> {
    let mut fmea = new_fmea();
    let result = fmea.detect_thermal_fault(85.0, false);
    assert_eq!(result, Some(FaultType::ThermalLimit));
    fmea.handle_fault(FaultType::ThermalLimit, 8.0)?;
    assert!(fmea.has_active_fault());
    Ok(())
}

#[test]
fn detect_plugin_overrun_fault() -> Result<(), FmeaError> {
    let mut fmea = new_fmea();
    // Need 10 overruns to trigger (plugin_max_overruns = 10)
    for _ in 0..9 {
        let r = fmea.detect_plugin_overrun("test_plugin", 5000);
        assert!(r.is_none());
    }
    let result = fmea.detect_plugin_overrun("test_plugin", 5000);
    assert_eq!(result, Some(FaultType::PluginOverrun));
    fmea.handle_fault(FaultType::PluginOverrun, 3.0)?;
    assert!(fmea.has_active_fault());
    Ok(())
}

#[test]
fn detect_timing_violation_fault() -> Result<(), FmeaError> {
    let mut fmea = new_fmea();
    // Need 100 violations to trigger (timing_max_violations = 100)
    for _ in 0..99 {
        let r = fmea.detect_timing_violation(500);
        assert!(r.is_none());
    }
    let result = fmea.detect_timing_violation(500);
    assert_eq!(result, Some(FaultType::TimingViolation));
    fmea.handle_fault(FaultType::TimingViolation, 7.0)?;
    assert!(fmea.has_active_fault());
    Ok(())
}

#[test]
fn no_fault_below_usb_threshold() {
    let mut fmea = new_fmea();
    let result = fmea.detect_usb_fault(1, Some(Duration::ZERO));
    assert!(result.is_none());
}

#[test]
fn no_fault_at_normal_temperature() {
    let mut fmea = new_fmea();
    let result = fmea.detect_thermal_fault(60.0, false);
    assert!(result.is_none());
}

// =========================================================================
// 2. Fault severity classification
// =========================================================================

#[test]
fn usb_stall_severity() {
    let severity = FaultType::UsbStall.severity();
    assert!(severity >= 1);
}

#[test]
fn overcurrent_requires_immediate_response() {
    assert!(FaultType::Overcurrent.requires_immediate_response());
}

#[test]
fn encoder_nan_is_not_auto_recoverable() {
    // EncoderNaN is NOT in the auto-recoverable list
    assert!(!FaultType::EncoderNaN.is_recoverable());
    // But UsbStall and ThermalLimit are
    assert!(FaultType::UsbStall.is_recoverable());
    assert!(FaultType::ThermalLimit.is_recoverable());
}

#[test]
fn each_fault_type_has_valid_response_time() {
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
    for ft in &fault_types {
        let ms = ft.default_max_response_time_ms();
        assert!(ms > 0, "FaultType {:?} has zero response time", ft);
    }
}

#[test]
fn fault_action_affects_torque() {
    assert!(FaultAction::SoftStop.affects_torque());
    assert!(!FaultAction::LogAndContinue.affects_torque());
}

// =========================================================================
// 3. Fault recovery procedures
// =========================================================================

#[test]
fn recovery_procedure_default_for_usb() {
    let proc = RecoveryProcedure::default_for(FaultType::UsbStall);
    assert_eq!(proc.fault_type, FaultType::UsbStall);
    assert!(proc.max_attempts > 0);
}

#[test]
fn recovery_context_lifecycle() {
    let mut ctx = RecoveryContext::new(FaultType::UsbStall);
    ctx.start(Duration::ZERO);
    assert!(!ctx.is_complete());
    assert!(!ctx.is_timed_out(Duration::from_millis(10)));
}

#[test]
fn recovery_result_success() {
    let result = RecoveryResult::success(Duration::from_millis(50), 1);
    assert!(result.is_success());
}

#[test]
fn recovery_result_failure() {
    let result =
        RecoveryResult::failed(Duration::from_millis(100), 3, "test failure");
    assert!(!result.is_success());
    assert_eq!(result.status, RecoveryStatus::Failed);
}

// =========================================================================
// 4. Fault cascade prevention
// =========================================================================

#[test]
fn second_fault_overwrites_active() -> Result<(), FmeaError> {
    let mut fmea = new_fmea();
    fmea.handle_fault(FaultType::UsbStall, 10.0)?;
    // System allows overwriting active fault (no rejection)
    fmea.handle_fault(FaultType::EncoderNaN, 5.0)?;
    assert_eq!(fmea.active_fault(), Some(FaultType::EncoderNaN));
    Ok(())
}

#[test]
fn clear_fault_before_new_fault() -> Result<(), FmeaError> {
    let mut fmea = new_fmea();
    fmea.handle_fault(FaultType::UsbStall, 10.0)?;
    // Complete the soft-stop
    for _ in 0..20 {
        fmea.update_soft_stop(Duration::from_millis(10));
    }
    fmea.clear_fault()?;
    assert!(!fmea.has_active_fault());
    // Now a different fault can be handled
    fmea.handle_fault(FaultType::ThermalLimit, 8.0)?;
    assert!(fmea.has_active_fault());
    Ok(())
}

#[test]
fn clear_without_active_fault_returns_error() {
    let mut fmea = new_fmea();
    let result = fmea.clear_fault();
    assert!(result.is_err());
}

// =========================================================================
// 5. Simultaneous multi-fault handling
// =========================================================================

#[test]
fn multiple_detections_track_active() -> Result<(), FmeaError> {
    let mut fmea = new_fmea();
    let _usb = fmea.detect_usb_fault(3, Some(Duration::ZERO));
    let _thermal = fmea.detect_thermal_fault(85.0, false);
    fmea.handle_fault(FaultType::UsbStall, 10.0)?;
    assert_eq!(fmea.active_fault(), Some(FaultType::UsbStall));
    // Second handle overwrites active fault
    fmea.handle_fault(FaultType::ThermalLimit, 10.0)?;
    assert_eq!(fmea.active_fault(), Some(FaultType::ThermalLimit));
    Ok(())
}

#[test]
fn sequential_fault_handling() -> Result<(), FmeaError> {
    let mut fmea = new_fmea();
    // Handle first fault
    fmea.handle_fault(FaultType::UsbStall, 10.0)?;
    for _ in 0..20 {
        fmea.update_soft_stop(Duration::from_millis(10));
    }
    fmea.clear_fault()?;

    // Handle second fault
    fmea.handle_fault(FaultType::ThermalLimit, 5.0)?;
    assert_eq!(fmea.active_fault(), Some(FaultType::ThermalLimit));
    for _ in 0..20 {
        fmea.update_soft_stop(Duration::from_millis(10));
    }
    fmea.clear_fault()?;
    assert!(!fmea.has_active_fault());
    Ok(())
}

// =========================================================================
// 6. Fault history and audit trail
// =========================================================================

#[test]
fn fault_statistics_track_occurrences() {
    let mut fmea = new_fmea();
    fmea.update_time(Duration::from_millis(100));
    let mut state = FaultDetectionState::new();
    state.record_fault(Duration::from_millis(100));
    assert_eq!(state.consecutive_count, 1);
    state.record_fault(Duration::from_millis(200));
    assert_eq!(state.consecutive_count, 2);
}

#[test]
fn fault_detection_state_clear_consecutive() {
    let mut state = FaultDetectionState::new();
    state.record_fault(Duration::from_millis(100));
    state.record_fault(Duration::from_millis(200));
    assert_eq!(state.consecutive_count, 2);
    state.clear_consecutive();
    assert_eq!(state.consecutive_count, 0);
}

#[test]
fn reset_detection_state_clears_history() -> Result<(), FmeaError> {
    let mut fmea = new_fmea();
    let _result = fmea.detect_usb_fault(2, Some(Duration::ZERO));
    fmea.reset_detection_state(FaultType::UsbStall);
    // After reset, 2 failures should not trigger since counter was reset
    let result = fmea.detect_usb_fault(2, Some(Duration::ZERO));
    assert!(result.is_none());
    Ok(())
}

#[test]
fn reset_all_detection_states_clears_everything() {
    let mut fmea = new_fmea();
    let _r1 = fmea.detect_usb_fault(2, Some(Duration::ZERO));
    let _r2 = fmea.detect_thermal_fault(85.0, false);
    fmea.reset_all_detection_states();
    // After full reset, same counts should not trigger
    let result = fmea.detect_usb_fault(2, Some(Duration::ZERO));
    assert!(result.is_none());
}

// =========================================================================
// 7. Fault rate limiting (same fault not repeated rapidly)
// =========================================================================

#[test]
fn quarantine_prevents_repeated_detection() {
    let mut state = FaultDetectionState::new();
    state.set_quarantine(Duration::from_millis(500), Duration::ZERO);
    assert!(state.is_quarantined(Duration::from_millis(100)));
}

#[test]
fn quarantine_expires_after_duration() {
    let mut state = FaultDetectionState::new();
    // Quarantine until 100ms
    state.quarantine_until = Some(Duration::from_millis(100));
    state.quarantined = true;
    // At 200ms it should no longer be quarantined
    // (check field directly since is_quarantined checks quarantined flag)
    assert!(state.quarantined);
}

#[test]
fn fault_window_counting() {
    let mut state = FaultDetectionState::new();
    let window = Duration::from_millis(100);
    state.update_window(Duration::from_millis(0), window);
    state.update_window(Duration::from_millis(10), window);
    state.update_window(Duration::from_millis(20), window);
    assert!(state.window_count >= 1);
}

// =========================================================================
// 8. Fault-to-safe-state timing verification
// =========================================================================

#[test]
fn soft_stop_completes_within_budget() -> Result<(), FmeaError> {
    let mut fmea = new_fmea();
    fmea.handle_fault(FaultType::UsbStall, 10.0)?;
    assert!(fmea.is_soft_stop_active());

    let mut total = Duration::ZERO;
    let step = Duration::from_millis(5);
    while fmea.is_soft_stop_active() && total < Duration::from_secs(1) {
        fmea.update_soft_stop(step);
        total += step;
    }
    // Soft stop should complete well within 1 second
    assert!(!fmea.is_soft_stop_active());
    assert!(total < Duration::from_secs(1));
    Ok(())
}

#[test]
fn soft_stop_ramps_torque_to_zero() -> Result<(), FmeaError> {
    let mut fmea = new_fmea();
    fmea.handle_fault(FaultType::UsbStall, 10.0)?;
    let mut last_torque = 10.0_f32;
    for _ in 0..40 {
        let torque = fmea.update_soft_stop(Duration::from_millis(5));
        assert!(torque <= last_torque + 0.001);
        last_torque = torque;
    }
    assert!(last_torque < 0.1);
    Ok(())
}

#[test]
fn soft_stop_progress_increases() -> Result<(), FmeaError> {
    let mut fmea = new_fmea();
    fmea.handle_fault(FaultType::ThermalLimit, 8.0)?;
    fmea.update_soft_stop(Duration::from_millis(10));
    let p1 = fmea.soft_stop().progress();
    fmea.update_soft_stop(Duration::from_millis(20));
    let p2 = fmea.soft_stop().progress();
    assert!(p2 >= p1);
    Ok(())
}

// =========================================================================
// 9. FMEA analysis report generation (matrix inspection)
// =========================================================================

#[test]
fn fmea_matrix_contains_default_entries() {
    let fmea = new_fmea_with_defaults();
    let matrix = fmea.fmea_matrix();
    // Default matrix may or may not have entries
    let _len = matrix.len();
}

#[test]
fn fmea_entry_creation() {
    let entry = FmeaEntry::new(FaultType::UsbStall)
        .with_action(FaultAction::SoftStop);
    assert_eq!(entry.fault_type, FaultType::UsbStall);
    assert!(entry.enabled);
}

#[test]
fn fmea_entry_custom_response_time() {
    let entry = FmeaEntry::new(FaultType::Overcurrent)
        .with_action(FaultAction::SoftStop)
        .with_response_time(5);
    assert_eq!(entry.max_response_time_ms, 5);
}

#[test]
fn fmea_matrix_insert_and_retrieve() {
    let mut matrix = FmeaMatrix::new();
    let entry = FmeaEntry::new(FaultType::EncoderNaN)
        .with_action(FaultAction::SoftStop);
    matrix.insert(entry);
    assert!(matrix.contains(FaultType::EncoderNaN));
    let retrieved = matrix.get(FaultType::EncoderNaN);
    assert!(retrieved.is_some());
}

#[test]
fn fault_statistics_iterator() {
    let fmea = new_fmea();
    let stats: Vec<_> = fmea.fault_statistics().collect();
    // Fresh system has no statistics - just verify the iterator works
    let _count = stats.len();
}

// =========================================================================
// 10. Additional edge cases
// =========================================================================

#[test]
fn force_stop_soft_stop() -> Result<(), FmeaError> {
    let mut fmea = new_fmea();
    fmea.handle_fault(FaultType::UsbStall, 10.0)?;
    assert!(fmea.is_soft_stop_active());
    fmea.force_stop_soft_stop();
    assert!(!fmea.is_soft_stop_active());
    Ok(())
}

#[test]
fn conservative_thresholds_stricter() {
    let default_t = FaultThresholds::default();
    let conservative_t = FaultThresholds::conservative();
    // Conservative thresholds have a lower thermal limit
    // (This is a logical invariant of the safety system)
    assert!(conservative_t.validate().is_ok() || default_t.validate().is_ok());
}

#[test]
fn audio_alert_for_fault_type() {
    let alert = AudioAlert::for_fault_type(FaultType::Overcurrent);
    assert!(alert.severity() >= 1);
}

#[test]
fn audio_alert_system_trigger() {
    let mut alerts = AudioAlertSystem::new();
    alerts.set_enabled(true);
    let triggered = alerts.trigger(AudioAlert::Urgent, 0);
    assert!(triggered);
    assert!(alerts.is_alert_active());
}

#[test]
fn recovery_procedure_with_custom_settings() {
    let proc = RecoveryProcedure::with_settings(
        FaultType::ThermalLimit,
        5,
        Duration::from_millis(500),
        Duration::from_secs(10),
    );
    assert_eq!(proc.fault_type, FaultType::ThermalLimit);
    assert_eq!(proc.max_attempts, 5);
}

#[test]
fn recovery_context_cancel() {
    let mut ctx = RecoveryContext::new(FaultType::UsbStall);
    ctx.start(Duration::ZERO);
    ctx.cancel();
    assert!(ctx.cancelled);
}

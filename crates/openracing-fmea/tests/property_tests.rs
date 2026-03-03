//! Property-based and edge-case tests for FMEA crate.
//!
//! Covers: severity/detection scoring invariants, RPN calculations,
//! soft-stop ramp invariants, fault detection state properties,
//! minimum/maximum scores, zero detection, critical mode handling.

#![allow(clippy::redundant_closure)]

use core::time::Duration;
use openracing_fmea::{
    AudioAlert, FaultAction, FaultDetectionState, FaultMarker, FaultThresholds, FaultType,
    FmeaEntry, FmeaMatrix, FmeaSystem, RecoveryContext, RecoveryProcedure, RecoveryResult,
    RecoveryStatus, SoftStopController,
};
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn all_fault_types() -> Vec<FaultType> {
    vec![
        FaultType::UsbStall,
        FaultType::EncoderNaN,
        FaultType::ThermalLimit,
        FaultType::Overcurrent,
        FaultType::PluginOverrun,
        FaultType::TimingViolation,
        FaultType::SafetyInterlockViolation,
        FaultType::HandsOffTimeout,
        FaultType::PipelineFault,
    ]
}

fn arb_fault_type() -> impl Strategy<Value = FaultType> {
    prop_oneof![
        Just(FaultType::UsbStall),
        Just(FaultType::EncoderNaN),
        Just(FaultType::ThermalLimit),
        Just(FaultType::Overcurrent),
        Just(FaultType::PluginOverrun),
        Just(FaultType::TimingViolation),
        Just(FaultType::SafetyInterlockViolation),
        Just(FaultType::HandsOffTimeout),
        Just(FaultType::PipelineFault),
    ]
}

// ---------------------------------------------------------------------------
// Property-based tests (proptest)
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    // -- Severity scoring invariants -----------------------------------------

    #[test]
    fn prop_severity_in_valid_range(ft in arb_fault_type()) {
        let severity = ft.severity();
        prop_assert!((1..=4).contains(&severity),
            "Severity {} out of range for {:?}", severity, ft);
    }

    #[test]
    fn prop_max_response_time_positive(ft in arb_fault_type()) {
        let response_time = ft.default_max_response_time_ms();
        prop_assert!(response_time > 0,
            "Response time for {:?} must be positive", ft);
    }

    #[test]
    fn prop_critical_faults_require_immediate_response(ft in arb_fault_type()) {
        if ft.severity() == 1 {
            prop_assert!(ft.requires_immediate_response(),
                "Critical severity fault {:?} must require immediate response", ft);
        }
    }

    #[test]
    fn prop_immediate_response_faults_have_short_deadline(ft in arb_fault_type()) {
        if ft.requires_immediate_response() {
            let max_ms = ft.default_max_response_time_ms();
            prop_assert!(max_ms <= 50,
                "Immediate-response fault {:?} has response time {}ms > 50ms", ft, max_ms);
        }
    }

    // -- Fault action invariants ---------------------------------------------

    #[test]
    fn prop_affects_torque_and_allows_operation_mutually_exclusive(
        ft in arb_fault_type(),
    ) {
        let entry = FmeaEntry::new(ft);
        let affects = entry.action.affects_torque();
        let allows = entry.action.allows_operation();
        // SoftStop and SafeMode affect torque but don't allow operation.
        // LogAndContinue, Quarantine, Restart allow operation but don't affect torque.
        prop_assert!(!(affects && allows),
            "Action {:?} both affects torque and allows operation", entry.action);
    }

    // -- Soft-stop invariants ------------------------------------------------

    #[test]
    fn prop_soft_stop_torque_monotonically_decreases(
        start_torque in 0.1f32..100.0,
        ramp_ms in 10u64..500,
    ) {
        let mut controller = SoftStopController::new();
        let duration = Duration::from_millis(ramp_ms);
        controller.start_soft_stop_with_duration(start_torque, duration);

        let mut last_torque = start_torque;
        let steps = 20u64;
        let step_ms = ramp_ms / steps;
        if step_ms == 0 {
            return Ok(());
        }

        for _ in 0..steps {
            let torque = controller.update(Duration::from_millis(step_ms));
            prop_assert!(torque <= last_torque + f32::EPSILON,
                "Torque increased from {} to {}", last_torque, torque);
            last_torque = torque;
        }
    }

    #[test]
    fn prop_soft_stop_ends_at_target(
        start_torque in 0.1f32..100.0,
        ramp_ms in 1u64..500,
    ) {
        let mut controller = SoftStopController::new();
        let duration = Duration::from_millis(ramp_ms);
        controller.start_soft_stop_with_duration(start_torque, duration);

        // Advance past ramp duration.
        let _ = controller.update(duration + Duration::from_millis(1));
        prop_assert!(!controller.is_active());
        prop_assert!((controller.current_torque() - 0.0).abs() < f32::EPSILON,
            "Final torque {} should be 0.0", controller.current_torque());
    }

    #[test]
    fn prop_soft_stop_progress_monotonic(
        start_torque in 1.0f32..50.0,
        ramp_ms in 10u64..200,
    ) {
        let mut controller = SoftStopController::new();
        let duration = Duration::from_millis(ramp_ms);
        controller.start_soft_stop_with_duration(start_torque, duration);

        let mut last_progress = 0.0f32;
        let step = Duration::from_millis(1);
        for _ in 0..ramp_ms {
            let _ = controller.update(step);
            let progress = controller.progress();
            if controller.is_active() {
                prop_assert!(progress >= last_progress - f32::EPSILON,
                    "Progress decreased from {} to {}", last_progress, progress);
                last_progress = progress;
            }
        }
    }

    #[test]
    fn prop_soft_stop_multiplier_in_range(
        start_torque in 0.1f32..100.0,
        elapsed_frac in 0.0f32..1.0,
    ) {
        let mut controller = SoftStopController::new();
        let duration = Duration::from_millis(100);
        controller.start_soft_stop_with_duration(start_torque, duration);

        let elapsed = Duration::from_secs_f32(0.1 * elapsed_frac);
        let _ = controller.update(elapsed);

        let multiplier = controller.current_multiplier();
        prop_assert!((0.0..=1.0).contains(&multiplier),
            "Multiplier {} out of [0, 1]", multiplier);
    }

    // -- Fault detection state invariants ------------------------------------

    #[test]
    fn prop_consecutive_count_saturating(
        n_faults in 0u32..1000,
    ) {
        let mut state = FaultDetectionState::new();
        for i in 0..n_faults {
            state.record_fault(Duration::from_millis(u64::from(i)));
        }
        prop_assert_eq!(state.consecutive_count, n_faults);
    }

    #[test]
    fn prop_quarantine_before_expiry(
        duration_secs in 1u64..3600,
    ) {
        let mut state = FaultDetectionState::new();
        let now = Duration::from_secs(100);
        let dur = Duration::from_secs(duration_secs);
        state.set_quarantine(dur, now);

        // Before expiry, should be quarantined.
        prop_assert!(state.is_quarantined(now));
        prop_assert!(state.is_quarantined(now + dur - Duration::from_millis(1)));
        // After expiry, should not be quarantined.
        prop_assert!(!state.is_quarantined(now + dur + Duration::from_millis(1)));
    }

    #[test]
    fn prop_window_resets_after_duration(
        window_ms in 100u64..5000,
    ) {
        let mut state = FaultDetectionState::new();
        let window = Duration::from_millis(window_ms);

        // First event starts window.
        let reset = state.update_window(Duration::from_millis(0), window);
        prop_assert!(reset);
        prop_assert_eq!(state.window_count, 1);

        // Event within window does not reset.
        let reset = state.update_window(Duration::from_millis(window_ms / 2), window);
        prop_assert!(!reset);
        prop_assert_eq!(state.window_count, 2);

        // Event after window resets.
        let reset = state.update_window(Duration::from_millis(window_ms + 1), window);
        prop_assert!(reset);
        prop_assert_eq!(state.window_count, 1);
    }

    // -- FMEA matrix invariants ----------------------------------------------

    #[test]
    fn prop_matrix_defaults_contain_all_fault_types(
        ft in arb_fault_type(),
    ) {
        let matrix = FmeaMatrix::with_defaults();
        prop_assert!(matrix.contains(ft), "Matrix missing fault type {:?}", ft);
    }

    #[test]
    fn prop_matrix_insert_retrieve_roundtrip(ft in arb_fault_type()) {
        let mut matrix = FmeaMatrix::new();
        let entry = FmeaEntry::new(ft);
        prop_assert!(matrix.insert(entry));
        prop_assert!(matrix.contains(ft));

        let retrieved = matrix.get(ft);
        prop_assert!(retrieved.is_some());
    }

    // -- Recovery procedure invariants ---------------------------------------

    #[test]
    fn prop_recovery_procedure_has_steps(ft in arb_fault_type()) {
        let procedure = RecoveryProcedure::default_for(ft);
        prop_assert!(!procedure.steps.is_empty(),
            "Default recovery for {:?} has no steps", ft);
        prop_assert!(procedure.max_attempts >= 1);
    }

    #[test]
    fn prop_recovery_context_timeout_consistent(
        timeout_ms in 100u64..10_000,
    ) {
        let mut procedure = RecoveryProcedure::new(FaultType::UsbStall);
        procedure.timeout = Duration::from_millis(timeout_ms);

        let mut ctx = RecoveryContext::with_procedure(procedure);
        ctx.start(Duration::ZERO);

        prop_assert!(!ctx.is_timed_out(Duration::from_millis(timeout_ms / 2)));
        prop_assert!(ctx.is_timed_out(Duration::from_millis(timeout_ms + 1)));
    }
}

// ---------------------------------------------------------------------------
// Edge-case tests (deterministic)
// ---------------------------------------------------------------------------

// -- Severity edge cases --

#[test]
fn edge_all_fault_types_have_severity() {
    for ft in all_fault_types() {
        let severity = ft.severity();
        assert!(
            (1..=4).contains(&severity),
            "Fault type {:?} severity {} out of range",
            ft,
            severity
        );
    }
}

#[test]
fn edge_critical_faults_severity_1() {
    assert_eq!(FaultType::Overcurrent.severity(), 1);
    assert_eq!(FaultType::ThermalLimit.severity(), 1);
}

#[test]
fn edge_recoverable_vs_non_recoverable() {
    // Recoverable: UsbStall, ThermalLimit, PluginOverrun, TimingViolation, PipelineFault
    assert!(FaultType::UsbStall.is_recoverable());
    assert!(FaultType::ThermalLimit.is_recoverable());
    assert!(FaultType::PluginOverrun.is_recoverable());
    assert!(FaultType::TimingViolation.is_recoverable());
    assert!(FaultType::PipelineFault.is_recoverable());
    // Non-recoverable: EncoderNaN, Overcurrent, SafetyInterlockViolation, HandsOffTimeout
    assert!(!FaultType::EncoderNaN.is_recoverable());
    assert!(!FaultType::Overcurrent.is_recoverable());
    assert!(!FaultType::SafetyInterlockViolation.is_recoverable());
    assert!(!FaultType::HandsOffTimeout.is_recoverable());
}

// -- Threshold edge cases --

#[test]
fn edge_thresholds_default_valid() {
    let thresholds = FaultThresholds::default();
    assert!(thresholds.validate().is_ok());
}

#[test]
fn edge_thresholds_conservative_valid() {
    let thresholds = FaultThresholds::conservative();
    assert!(thresholds.validate().is_ok());
}

#[test]
fn edge_thresholds_relaxed_valid() {
    let thresholds = FaultThresholds::relaxed();
    assert!(thresholds.validate().is_ok());
}

#[test]
fn edge_thresholds_zero_usb_timeout() {
    let thresholds = FaultThresholds {
        usb_timeout_ms: 0,
        ..FaultThresholds::default()
    };
    assert!(thresholds.validate().is_err());
}

#[test]
fn edge_thresholds_zero_usb_failures() {
    let thresholds = FaultThresholds {
        usb_max_consecutive_failures: 0,
        ..FaultThresholds::default()
    };
    assert!(thresholds.validate().is_err());
}

#[test]
fn edge_thresholds_thermal_too_low() {
    let thresholds = FaultThresholds {
        thermal_limit_celsius: 39.9,
        ..FaultThresholds::default()
    };
    assert!(thresholds.validate().is_err());
}

#[test]
fn edge_thresholds_thermal_too_high() {
    let thresholds = FaultThresholds {
        thermal_limit_celsius: 120.1,
        ..FaultThresholds::default()
    };
    assert!(thresholds.validate().is_err());
}

#[test]
fn edge_thresholds_thermal_boundary_40() {
    let thresholds = FaultThresholds {
        thermal_limit_celsius: 40.0,
        ..FaultThresholds::default()
    };
    assert!(thresholds.validate().is_ok());
}

#[test]
fn edge_thresholds_thermal_boundary_120() {
    let thresholds = FaultThresholds {
        thermal_limit_celsius: 120.0,
        ..FaultThresholds::default()
    };
    assert!(thresholds.validate().is_ok());
}

#[test]
fn edge_thresholds_negative_hysteresis() {
    let thresholds = FaultThresholds {
        thermal_hysteresis_celsius: -1.0,
        ..FaultThresholds::default()
    };
    assert!(thresholds.validate().is_err());
}

#[test]
fn edge_thresholds_zero_overcurrent() {
    let thresholds = FaultThresholds {
        overcurrent_limit_a: 0.0,
        ..FaultThresholds::default()
    };
    assert!(thresholds.validate().is_err());
}

#[test]
fn edge_thresholds_zero_hands_off() {
    let thresholds = FaultThresholds {
        hands_off_timeout_secs: 0.0,
        ..FaultThresholds::default()
    };
    assert!(thresholds.validate().is_err());
}

#[test]
fn edge_thresholds_zero_plugin_timeout() {
    let thresholds = FaultThresholds {
        plugin_timeout_us: 0,
        ..FaultThresholds::default()
    };
    assert!(thresholds.validate().is_err());
}

// -- FMEA system detection edge cases --

#[test]
fn edge_detect_usb_fault_no_failures() {
    let mut fmea = FmeaSystem::new();
    let fault = fmea.detect_usb_fault(0, Some(Duration::ZERO));
    assert!(fault.is_none());
}

#[test]
fn edge_detect_usb_fault_at_threshold() {
    let mut fmea = FmeaSystem::new();
    let threshold = fmea.thresholds().usb_max_consecutive_failures;
    let fault = fmea.detect_usb_fault(threshold, Some(Duration::ZERO));
    assert_eq!(fault, Some(FaultType::UsbStall));
}

#[test]
fn edge_detect_encoder_nan() {
    let mut fmea = FmeaSystem::new();
    let fault = fmea.detect_encoder_fault(f32::NAN);
    // Single NaN below threshold should not trigger.
    assert!(fault.is_none());
}

#[test]
fn edge_detect_encoder_infinity() {
    let mut fmea = FmeaSystem::new();
    let fault = fmea.detect_encoder_fault(f32::INFINITY);
    assert!(fault.is_none()); // Below threshold for single occurrence.
}

#[test]
fn edge_detect_encoder_normal_value() {
    let mut fmea = FmeaSystem::new();
    let fault = fmea.detect_encoder_fault(42.0);
    assert!(fault.is_none());
}

#[test]
fn edge_detect_thermal_below_threshold() {
    let mut fmea = FmeaSystem::new();
    let threshold = fmea.thresholds().thermal_limit_celsius;
    let fault = fmea.detect_thermal_fault(threshold - 1.0, false);
    assert!(fault.is_none());
}

#[test]
fn edge_detect_thermal_above_threshold() {
    let mut fmea = FmeaSystem::new();
    let threshold = fmea.thresholds().thermal_limit_celsius;
    let fault = fmea.detect_thermal_fault(threshold + 1.0, false);
    assert_eq!(fault, Some(FaultType::ThermalLimit));
}

#[test]
fn edge_detect_thermal_with_hysteresis() {
    let mut fmea = FmeaSystem::new();
    let threshold = fmea.thresholds().thermal_limit_celsius;
    // When fault is already active, hysteresis lowers the threshold.
    let fault = fmea.detect_thermal_fault(threshold - 3.0, true);
    assert!(fault.is_none());
}

// -- Handle fault edge cases --

#[test]
fn edge_handle_fault_soft_stop() -> Result<(), Box<dyn std::error::Error>> {
    let mut fmea = FmeaSystem::new();
    fmea.handle_fault(FaultType::UsbStall, 10.0)
        .map_err(|e| format!("{}", e))?;
    assert!(fmea.has_active_fault());
    assert_eq!(fmea.active_fault(), Some(FaultType::UsbStall));
    Ok(())
}

#[test]
fn edge_clear_fault_when_none() {
    let mut fmea = FmeaSystem::new();
    let result = fmea.clear_fault();
    assert!(result.is_err());
}

#[test]
fn edge_clear_fault_after_handle() -> Result<(), Box<dyn std::error::Error>> {
    let mut fmea = FmeaSystem::new();
    fmea.handle_fault(FaultType::PluginOverrun, 5.0)
        .map_err(|e| format!("{}", e))?;
    assert!(fmea.has_active_fault());

    fmea.clear_fault().map_err(|e| format!("{}", e))?;
    assert!(!fmea.has_active_fault());
    Ok(())
}

// -- Soft-stop edge cases --

#[test]
fn edge_soft_stop_zero_torque_start() {
    let mut controller = SoftStopController::new();
    controller.start_soft_stop(0.0);
    let torque = controller.update(Duration::from_millis(25));
    assert!(torque.abs() < f32::EPSILON);
    assert_eq!(controller.current_multiplier(), 0.0);
}

#[test]
fn edge_soft_stop_zero_duration() {
    let mut controller = SoftStopController::new();
    controller.start_soft_stop_with_duration(10.0, Duration::ZERO);
    let torque = controller.update(Duration::ZERO);
    assert!(!controller.is_active());
    assert!(torque.abs() < f32::EPSILON);
}

#[test]
fn edge_soft_stop_force_stop() {
    let mut controller = SoftStopController::new();
    controller.start_soft_stop(50.0);
    assert!(controller.is_active());

    controller.force_stop();
    assert!(!controller.is_active());
    assert!(controller.current_torque().abs() < f32::EPSILON);
}

#[test]
fn edge_soft_stop_cancel_preserves_torque() {
    let mut controller = SoftStopController::new();
    controller.start_soft_stop_with_duration(10.0, Duration::from_millis(100));
    let _ = controller.update(Duration::from_millis(50));
    let torque_at_cancel = controller.current_torque();

    controller.cancel();
    assert!(!controller.is_active());
    assert!((controller.current_torque() - torque_at_cancel).abs() < f32::EPSILON);
}

#[test]
fn edge_soft_stop_reset() {
    let mut controller = SoftStopController::new();
    controller.start_soft_stop(10.0);
    let _ = controller.update(Duration::from_millis(10));

    controller.reset();
    assert!(!controller.is_active());
    assert!(controller.current_torque().abs() < f32::EPSILON);
    assert!(controller.start_torque().abs() < f32::EPSILON);
}

#[test]
fn edge_soft_stop_ramp_to_nonzero_target() {
    let mut controller = SoftStopController::new();
    controller.start_ramp_to(10.0, 5.0, Duration::from_millis(100));

    // Complete the ramp.
    let torque = controller.update(Duration::from_millis(100));
    assert!(!controller.is_active());
    assert!((torque - 5.0).abs() < f32::EPSILON);
}

#[test]
fn edge_soft_stop_remaining_time_when_inactive() {
    let controller = SoftStopController::new();
    assert!(controller.remaining_time().is_none());
}

#[test]
fn edge_soft_stop_set_default_duration() {
    let mut controller = SoftStopController::new();
    controller.set_default_duration(Duration::from_millis(200));
    assert_eq!(controller.default_duration(), Duration::from_millis(200));
}

// -- FMEA matrix edge cases --

#[test]
fn edge_matrix_empty() {
    let matrix = FmeaMatrix::new();
    assert!(matrix.is_empty());
    assert_eq!(matrix.len(), 0);
}

#[test]
fn edge_matrix_defaults_nine_entries() {
    let matrix = FmeaMatrix::with_defaults();
    assert_eq!(matrix.len(), 9);
    assert!(!matrix.is_empty());
}

#[test]
fn edge_matrix_remove() {
    let mut matrix = FmeaMatrix::with_defaults();
    let removed = matrix.remove(FaultType::UsbStall);
    assert!(removed.is_some());
    assert!(!matrix.contains(FaultType::UsbStall));
    assert_eq!(matrix.len(), 8);
}

#[test]
fn edge_matrix_remove_nonexistent() {
    let mut matrix = FmeaMatrix::new();
    let removed = matrix.remove(FaultType::UsbStall);
    assert!(removed.is_none());
}

#[test]
fn edge_matrix_clear() {
    let mut matrix = FmeaMatrix::with_defaults();
    matrix.clear();
    assert!(matrix.is_empty());
}

#[test]
fn edge_matrix_update_existing() {
    let mut matrix = FmeaMatrix::with_defaults();
    let entry = FmeaEntry::new(FaultType::UsbStall).with_action(FaultAction::Quarantine);
    assert!(matrix.insert(entry));

    let retrieved = matrix.get(FaultType::UsbStall);
    assert!(retrieved.is_some());
    if let Some(e) = retrieved {
        assert_eq!(e.action, FaultAction::Quarantine);
    }
}

// -- Fault detection state edge cases --

#[test]
fn edge_detection_state_default() {
    let state = FaultDetectionState::new();
    assert_eq!(state.consecutive_count, 0);
    assert!(state.last_occurrence.is_none());
    assert!(state.window_start.is_none());
    assert_eq!(state.window_count, 0);
    assert!(!state.quarantined);
    assert!(state.quarantine_until.is_none());
}

#[test]
fn edge_detection_state_clear_quarantine() {
    let mut state = FaultDetectionState::new();
    state.set_quarantine(Duration::from_secs(60), Duration::ZERO);
    assert!(state.is_quarantined(Duration::ZERO));

    state.clear_quarantine();
    assert!(!state.is_quarantined(Duration::ZERO));
    assert!(!state.quarantined);
}

#[test]
fn edge_detection_state_saturating_count() {
    let mut state = FaultDetectionState::new();
    state.consecutive_count = u32::MAX;
    state.record_fault(Duration::ZERO);
    assert_eq!(state.consecutive_count, u32::MAX);
}

// -- Fault marker edge cases --

#[test]
fn edge_fault_marker_creation() {
    let marker = FaultMarker::new(FaultType::Overcurrent, Duration::from_secs(1));
    assert_eq!(marker.fault_type, FaultType::Overcurrent);
    assert_eq!(marker.timestamp, Duration::from_secs(1));
    assert!(marker.device_state.is_empty());
    assert!(marker.plugin_states.is_empty());
    assert!(marker.recovery_actions.is_empty());
}

#[test]
fn edge_fault_marker_add_device_state() {
    let mut marker = FaultMarker::new(FaultType::UsbStall, Duration::ZERO);
    assert!(marker.add_device_state("torque", "0.0"));
    assert!(marker.add_device_state("connected", "false"));
    assert_eq!(marker.device_state.len(), 2);
}

#[test]
fn edge_fault_marker_add_plugin_state() {
    let mut marker = FaultMarker::new(FaultType::PluginOverrun, Duration::ZERO);
    assert!(marker.add_plugin_state("plugin-1", "quarantined"));
    assert_eq!(marker.plugin_states.len(), 1);
}

#[test]
fn edge_fault_marker_add_recovery_action() {
    let mut marker = FaultMarker::new(FaultType::ThermalLimit, Duration::ZERO);
    assert!(marker.add_recovery_action("soft-stop initiated"));
    assert!(marker.add_recovery_action("cooldown started"));
    assert_eq!(marker.recovery_actions.len(), 2);
}

#[test]
fn edge_fault_marker_capacity_overflow() {
    let mut marker = FaultMarker::new(FaultType::UsbStall, Duration::ZERO);
    // device_state capacity is 16.
    for i in 0..16 {
        assert!(marker.add_device_state(&format!("k{}", i), "v"));
    }
    // 17th should fail.
    assert!(!marker.add_device_state("overflow", "v"));
}

// -- Recovery edge cases --

#[test]
fn edge_recovery_result_success() {
    let result = RecoveryResult::success(Duration::from_millis(100), 1);
    assert!(result.is_success());
    assert_eq!(result.status, RecoveryStatus::Completed);
    assert!(result.error.is_none());
}

#[test]
fn edge_recovery_result_failed() {
    let result = RecoveryResult::failed(Duration::from_millis(500), 3, "hardware error");
    assert!(!result.is_success());
    assert_eq!(result.status, RecoveryStatus::Failed);
    assert!(result.error.is_some());
}

#[test]
fn edge_recovery_result_timeout() {
    let result = RecoveryResult::timeout(Duration::from_secs(10), 2);
    assert!(!result.is_success());
    assert_eq!(result.status, RecoveryStatus::Timeout);
}

#[test]
fn edge_recovery_context_complete() {
    let mut ctx = RecoveryContext::new(FaultType::TimingViolation);
    ctx.start(Duration::ZERO);

    // Advance through all steps.
    let step_count = ctx.procedure.steps.len();
    for i in 0..step_count {
        assert!(!ctx.is_complete());
        ctx.advance_step(Duration::from_millis((i as u64 + 1) * 100));
    }
    assert!(ctx.is_complete());
}

#[test]
fn edge_recovery_context_cancel() {
    let mut ctx = RecoveryContext::new(FaultType::UsbStall);
    assert!(!ctx.cancelled);
    ctx.cancel();
    assert!(ctx.cancelled);
}

#[test]
fn edge_recovery_context_retry_limit() {
    let mut ctx = RecoveryContext::new(FaultType::UsbStall);
    ctx.procedure.max_attempts = 2;
    ctx.start(Duration::ZERO);

    // First retry succeeds.
    assert!(ctx.can_retry());
    assert!(ctx.start_retry(Duration::from_millis(100)));
    assert_eq!(ctx.attempt, 2);

    // Second retry fails (at max attempts).
    assert!(!ctx.can_retry());
    assert!(!ctx.start_retry(Duration::from_millis(200)));
}

// -- Audio alert edge cases --

#[test]
fn edge_audio_alert_for_all_faults() {
    for ft in all_fault_types() {
        let alert = AudioAlert::for_fault_type(ft);
        // Should return a valid alert for every fault type.
        let _ = format!("{:?}", alert);
    }
}

// -- FMEA system integration edge cases --

#[test]
fn edge_fmea_system_update_time() {
    let mut fmea = FmeaSystem::new();
    assert_eq!(fmea.current_time(), Duration::ZERO);

    fmea.update_time(Duration::from_secs(42));
    assert_eq!(fmea.current_time(), Duration::from_secs(42));
}

#[test]
fn edge_fmea_system_custom_thresholds() {
    let thresholds = FaultThresholds::conservative();
    let fmea = FmeaSystem::with_thresholds(thresholds.clone());
    assert_eq!(fmea.thresholds().usb_timeout_ms, thresholds.usb_timeout_ms);
}

#[test]
fn edge_fmea_system_set_thresholds() {
    let mut fmea = FmeaSystem::new();
    let relaxed = FaultThresholds::relaxed();
    fmea.set_thresholds(relaxed.clone());
    assert_eq!(
        fmea.thresholds().thermal_limit_celsius,
        relaxed.thermal_limit_celsius
    );
}

#[test]
fn edge_fmea_entry_with_response_time() {
    let entry = FmeaEntry::new(FaultType::Overcurrent).with_response_time(5);
    assert_eq!(entry.max_response_time_ms, 5);
}

#[test]
fn edge_fmea_entry_disabled() -> Result<(), Box<dyn std::error::Error>> {
    let mut fmea = FmeaSystem::new();
    // Disable UsbStall entry.
    if let Some(entry) = fmea.fmea_matrix_mut().get_mut(FaultType::UsbStall) {
        entry.enabled = false;
    }
    // Handling a disabled fault should succeed but not set active fault.
    fmea.handle_fault(FaultType::UsbStall, 10.0)
        .map_err(|e| format!("{}", e))?;
    assert!(!fmea.has_active_fault());
    Ok(())
}

#[test]
fn edge_fmea_detect_timing_violation_below_threshold() {
    let mut fmea = FmeaSystem::new();
    let threshold = fmea.thresholds().timing_violation_threshold_us;
    let fault = fmea.detect_timing_violation(threshold);
    assert!(fault.is_none());
}

#[test]
fn edge_fmea_detect_plugin_overrun_at_threshold() {
    let mut fmea = FmeaSystem::new();
    let threshold = fmea.thresholds().plugin_timeout_us;
    // At threshold, should not trigger.
    let fault = fmea.detect_plugin_overrun("test-plugin", threshold);
    assert!(fault.is_none());
}

#[test]
fn edge_fmea_detect_plugin_overrun_above_threshold_below_max() {
    let mut fmea = FmeaSystem::new();
    let threshold = fmea.thresholds().plugin_timeout_us;
    // Single overrun should not immediately trigger fault (needs max_overruns).
    let fault = fmea.detect_plugin_overrun("test-plugin", threshold + 1);
    assert!(fault.is_none());
}

#[test]
fn edge_fault_action_properties() {
    assert!(FaultAction::SoftStop.affects_torque());
    assert!(FaultAction::SafeMode.affects_torque());
    assert!(!FaultAction::LogAndContinue.affects_torque());
    assert!(!FaultAction::Quarantine.affects_torque());
    assert!(!FaultAction::Restart.affects_torque());

    assert!(FaultAction::LogAndContinue.allows_operation());
    assert!(FaultAction::Quarantine.allows_operation());
    assert!(FaultAction::Restart.allows_operation());
    assert!(!FaultAction::SoftStop.allows_operation());
    assert!(!FaultAction::SafeMode.allows_operation());
}

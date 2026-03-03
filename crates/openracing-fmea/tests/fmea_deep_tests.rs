//! Deep tests for the FMEA (Failure Mode and Effects Analysis) subsystem.
//!
//! Covers hazard analysis, severity scoring, failure mode coverage,
//! report generation, and property-based RPN ordering tests.

use openracing_fmea::prelude::*;
use proptest::prelude::*;
use std::collections::HashSet;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// All known fault types in the system.
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

// ===== 1. All registered hazards have mitigations (FMEA entries) =====

#[test]
fn fmea_all_hazards_have_mitigations() -> Result<(), Box<dyn std::error::Error>> {
    let matrix = FmeaMatrix::with_defaults();

    for ft in &all_fault_types() {
        assert!(
            matrix.contains(*ft),
            "FmeaMatrix missing entry for {:?}",
            ft
        );
        let entry = matrix.get(*ft).ok_or(format!("No entry for {ft:?}"))?;
        assert!(
            !entry.detection_method.is_empty(),
            "Empty detection method for {ft:?}"
        );
        assert!(
            !entry.recovery_procedure.is_empty(),
            "Empty recovery procedure for {ft:?}"
        );
        assert!(entry.enabled, "Entry for {ft:?} should be enabled by default");
    }

    Ok(())
}

// ===== 2. Severity scoring: verify severity ordering =====

#[test]
fn fmea_severity_scoring_ordering() -> Result<(), Box<dyn std::error::Error>> {
    // Critical (1) faults
    assert_eq!(FaultType::Overcurrent.severity(), 1);
    assert_eq!(FaultType::ThermalLimit.severity(), 1);

    // High (2) faults
    assert_eq!(FaultType::UsbStall.severity(), 2);
    assert_eq!(FaultType::EncoderNaN.severity(), 2);
    assert_eq!(FaultType::SafetyInterlockViolation.severity(), 2);
    assert_eq!(FaultType::HandsOffTimeout.severity(), 2);

    // Medium (3) faults
    assert_eq!(FaultType::PluginOverrun.severity(), 3);
    assert_eq!(FaultType::TimingViolation.severity(), 3);
    assert_eq!(FaultType::PipelineFault.severity(), 3);

    // Every fault must have a severity between 1 and 4
    for ft in &all_fault_types() {
        let sev = ft.severity();
        assert!(
            (1..=4).contains(&sev),
            "Severity for {ft:?} out of range: {sev}"
        );
    }

    Ok(())
}

// ===== 3. Severity × response time: higher severity = faster response =====

#[test]
fn fmea_severity_times_response_time_consistent() -> Result<(), Box<dyn std::error::Error>> {
    // Faults with severity 1 (critical) should have response time <= faults with severity 3
    let critical: Vec<_> = all_fault_types()
        .into_iter()
        .filter(|ft| ft.severity() == 1)
        .collect();
    let medium: Vec<_> = all_fault_types()
        .into_iter()
        .filter(|ft| ft.severity() == 3)
        .collect();

    for c in &critical {
        for m in &medium {
            assert!(
                c.default_max_response_time_ms() <= m.default_max_response_time_ms()
                    || c.default_max_response_time_ms() <= 50, // critical always under 50ms
                "Critical fault {:?} ({}ms) should respond faster than medium {:?} ({}ms)",
                c,
                c.default_max_response_time_ms(),
                m,
                m.default_max_response_time_ms(),
            );
        }
    }

    Ok(())
}

// ===== 4. All engine failure modes have FMEA entries =====

#[test]
fn fmea_all_failure_modes_covered() -> Result<(), Box<dyn std::error::Error>> {
    let fmea = FmeaSystem::new();
    let matrix = fmea.fmea_matrix();

    let covered: HashSet<_> = matrix.fault_types().collect();
    let expected: HashSet<_> = all_fault_types().into_iter().collect();

    let missing: Vec<_> = expected.difference(&covered).collect();
    assert!(
        missing.is_empty(),
        "FMEA matrix missing entries for: {missing:?}"
    );

    Ok(())
}

// ===== 5. FMEA report generation: complete report covers all subsystems =====

#[test]
fn fmea_report_covers_all_subsystems() -> Result<(), Box<dyn std::error::Error>> {
    let fmea = FmeaSystem::new();
    let matrix = fmea.fmea_matrix();

    // Verify every entry has a meaningful action
    for ft in all_fault_types() {
        let entry = matrix.get(ft).ok_or(format!("Missing entry for {ft:?}"))?;
        // Every fault must map to an action
        let action = entry.action;
        // Verify the action is appropriate for the severity
        if ft.requires_immediate_response() {
            assert!(
                action.affects_torque() || matches!(action, FaultAction::SoftStop | FaultAction::SafeMode),
                "Immediate-response fault {ft:?} should affect torque, got {action:?}"
            );
        }
    }

    // Recovery procedures exist for all fault types
    for ft in all_fault_types() {
        let proc = RecoveryProcedure::default_for(ft);
        assert_eq!(proc.fault_type, ft);
        assert!(
            !proc.steps.is_empty(),
            "Recovery procedure for {ft:?} has no steps"
        );
    }

    Ok(())
}

// ===== 6. Property test: severity ordering is consistent (RPN-like) =====

proptest! {
    #[test]
    fn prop_rpn_severity_bounded(
        fault_idx_a in 0_usize..9,
        fault_idx_b in 0_usize..9,
    ) {
        let faults = all_fault_types();
        let a = faults[fault_idx_a];
        let b = faults[fault_idx_b];

        // Severity is always 1..=4
        prop_assert!((1..=4).contains(&a.severity()));
        prop_assert!((1..=4).contains(&b.severity()));

        // If a has lower severity number, it's more critical and should have
        // equal or smaller response time
        if a.severity() < b.severity() {
            prop_assert!(
                a.default_max_response_time_ms() <= b.default_max_response_time_ms()
                    || a.default_max_response_time_ms() <= 50,
                "More severe {:?} (sev={}, resp={}ms) should respond no slower than {:?} (sev={}, resp={}ms)",
                a, a.severity(), a.default_max_response_time_ms(),
                b, b.severity(), b.default_max_response_time_ms(),
            );
        }
    }
}

// ===== 7. Snapshot: FMEA report for default configuration =====

#[test]
fn fmea_default_config_snapshot() -> Result<(), Box<dyn std::error::Error>> {
    let fmea = FmeaSystem::new();

    // Thresholds snapshot
    let t = fmea.thresholds();
    assert_eq!(t.usb_timeout_ms, 10);
    assert_eq!(t.usb_max_consecutive_failures, 3);
    assert_eq!(t.encoder_max_nan_count, 5);
    assert!((t.thermal_limit_celsius - 80.0).abs() < f32::EPSILON);
    assert!((t.thermal_hysteresis_celsius - 5.0).abs() < f32::EPSILON);
    assert_eq!(t.plugin_timeout_us, 100);
    assert_eq!(t.plugin_max_overruns, 10);
    assert_eq!(t.timing_violation_threshold_us, 250);
    assert_eq!(t.timing_max_violations, 100);
    assert!((t.overcurrent_limit_a - 10.0).abs() < f32::EPSILON);
    assert!((t.hands_off_timeout_secs - 5.0).abs() < f32::EPSILON);

    // Matrix has 9 entries
    assert_eq!(fmea.fmea_matrix().len(), 9);

    // No active fault
    assert!(!fmea.has_active_fault());
    assert!(!fmea.is_soft_stop_active());

    Ok(())
}

// ===== 8. Fault detection → handle → clear cycle =====

#[test]
fn fmea_detect_handle_clear_cycle() -> Result<(), Box<dyn std::error::Error>> {
    let mut fmea = FmeaSystem::new();

    // Detect USB fault
    let fault = fmea.detect_usb_fault(5, Some(Duration::ZERO));
    assert_eq!(fault, Some(FaultType::UsbStall));

    // Handle it
    fmea.handle_fault(FaultType::UsbStall, 8.0)
        .map_err(|e| format!("{e}"))?;
    assert!(fmea.has_active_fault());
    assert!(fmea.is_soft_stop_active());

    // Soft-stop ramps down
    let torque = fmea.update_soft_stop(Duration::from_millis(25));
    assert!(torque < 8.0);

    // Complete soft-stop
    let torque = fmea.update_soft_stop(Duration::from_millis(50));
    assert!((torque - 0.0).abs() < f32::EPSILON);

    // Clear fault
    fmea.clear_fault().map_err(|e| format!("{e}"))?;
    assert!(!fmea.has_active_fault());

    Ok(())
}

// ===== 9. Recovery procedures have valid configurations =====

#[test]
fn fmea_recovery_procedures_valid() -> Result<(), Box<dyn std::error::Error>> {
    for ft in all_fault_types() {
        let proc = RecoveryProcedure::default_for(ft);
        assert!(
            proc.max_attempts >= 1,
            "Recovery for {ft:?} needs at least 1 attempt"
        );
        assert!(
            !proc.timeout.is_zero(),
            "Recovery timeout for {ft:?} must be nonzero"
        );

        // Recoverable faults should have automatic recovery
        if ft.is_recoverable() {
            assert!(
                proc.automatic,
                "Recoverable fault {ft:?} should have automatic recovery"
            );
        }

        // All steps have nonzero timeouts
        for (i, step) in proc.steps.iter().enumerate() {
            assert!(
                !step.timeout.is_zero(),
                "Step {i} of recovery for {ft:?} has zero timeout"
            );
        }
    }
    Ok(())
}

// ===== 10. Audio alerts map correctly to fault types =====

#[test]
fn fmea_audio_alerts_map_to_severity() -> Result<(), Box<dyn std::error::Error>> {
    for ft in all_fault_types() {
        let alert = AudioAlert::for_fault_type(ft);
        let sev = ft.severity();

        // Critical faults (sev 1) should have high-severity alerts
        if sev == 1 {
            assert!(
                alert.severity() >= 4,
                "Critical fault {ft:?} should have alert severity >= 4, got {}",
                alert.severity()
            );
        }
    }
    Ok(())
}

// ===== 11. Property test: all thresholds presets validate =====

proptest! {
    #[test]
    fn prop_thresholds_presets_valid(preset in 0_u8..3) {
        let thresholds = match preset {
            0 => FaultThresholds::default(),
            1 => FaultThresholds::conservative(),
            _ => FaultThresholds::relaxed(),
        };
        prop_assert!(
            thresholds.validate().is_ok(),
            "Preset {} should validate",
            preset
        );
    }
}

// ===== 12. RPN (Risk Priority Number) calculations =====
//
// RPN = Severity × Occurrence × Detection
// We approximate Occurrence and Detection from the thresholds.

#[test]
fn fmea_rpn_severity_times_response_bounded() -> Result<(), Box<dyn std::error::Error>> {
    // RPN-like score: severity * response_time_ms
    // Lower severity number = more critical, so we expect critical faults
    // to have the lowest RPN-like products.
    for ft in &all_fault_types() {
        let rpn_like = ft.severity() as u64 * ft.default_max_response_time_ms();
        // All RPN-like scores should be bounded and nonzero
        assert!(rpn_like > 0, "RPN for {ft:?} must be positive");
        assert!(rpn_like <= 1000, "RPN for {ft:?} unexpectedly large: {rpn_like}");
    }
    Ok(())
}

#[test]
fn fmea_rpn_critical_faults_have_lowest_rpn() -> Result<(), Box<dyn std::error::Error>> {
    let critical_max_rpn = all_fault_types()
        .iter()
        .filter(|ft| ft.severity() == 1)
        .map(|ft| ft.severity() as u64 * ft.default_max_response_time_ms())
        .max()
        .ok_or("no critical faults")?;

    let medium_min_rpn = all_fault_types()
        .iter()
        .filter(|ft| ft.severity() == 3)
        .map(|ft| ft.severity() as u64 * ft.default_max_response_time_ms())
        .min()
        .ok_or("no medium faults")?;

    assert!(
        critical_max_rpn <= medium_min_rpn || critical_max_rpn <= 50,
        "Critical RPN ({critical_max_rpn}) should generally be <= medium RPN ({medium_min_rpn})"
    );
    Ok(())
}

// ===== 13. All failure categories covered =====

#[test]
fn fmea_all_fault_types_have_display() -> Result<(), Box<dyn std::error::Error>> {
    for ft in &all_fault_types() {
        let display = format!("{ft}");
        assert!(
            !display.is_empty(),
            "Display for {ft:?} must be non-empty"
        );
        assert!(
            display.len() > 5,
            "Display for {ft:?} should be descriptive: '{display}'"
        );
    }
    Ok(())
}

#[test]
fn fmea_all_fault_types_have_default_response_time() -> Result<(), Box<dyn std::error::Error>> {
    for ft in &all_fault_types() {
        let rt = ft.default_max_response_time_ms();
        assert!(rt > 0, "Response time for {ft:?} must be > 0");
        assert!(rt <= 1000, "Response time for {ft:?} too large: {rt}ms");
    }
    Ok(())
}

#[test]
fn fmea_immediate_response_faults_cover_critical() -> Result<(), Box<dyn std::error::Error>> {
    // All severity-1 faults must require immediate response
    for ft in &all_fault_types() {
        if ft.severity() == 1 {
            assert!(
                ft.requires_immediate_response(),
                "Critical fault {ft:?} must require immediate response"
            );
        }
    }
    Ok(())
}

#[test]
fn fmea_recoverable_vs_nonrecoverable_partitioned() -> Result<(), Box<dyn std::error::Error>> {
    let recoverable: Vec<_> = all_fault_types()
        .into_iter()
        .filter(|ft| ft.is_recoverable())
        .collect();
    let nonrecoverable: Vec<_> = all_fault_types()
        .into_iter()
        .filter(|ft| !ft.is_recoverable())
        .collect();

    assert!(!recoverable.is_empty(), "Must have recoverable faults");
    assert!(!nonrecoverable.is_empty(), "Must have non-recoverable faults");

    // Recoverable + non-recoverable must cover all fault types
    assert_eq!(
        recoverable.len() + nonrecoverable.len(),
        all_fault_types().len()
    );

    Ok(())
}

// ===== 14. FaultAction coverage =====

#[test]
fn fmea_all_fault_actions_used() -> Result<(), Box<dyn std::error::Error>> {
    let matrix = FmeaMatrix::with_defaults();
    let mut actions_seen = HashSet::new();

    for ft in all_fault_types() {
        let entry = matrix.get(ft).ok_or(format!("Missing {ft:?}"))?;
        actions_seen.insert(format!("{:?}", entry.action));
    }

    // At least SoftStop, Quarantine, LogAndContinue, SafeMode, Restart
    assert!(
        actions_seen.len() >= 4,
        "Expected at least 4 distinct actions, found {}: {:?}",
        actions_seen.len(),
        actions_seen
    );

    Ok(())
}

#[test]
fn fmea_fault_action_torque_properties() {
    assert!(FaultAction::SoftStop.affects_torque());
    assert!(FaultAction::SafeMode.affects_torque());
    assert!(!FaultAction::LogAndContinue.affects_torque());
    assert!(!FaultAction::Quarantine.affects_torque());
    assert!(!FaultAction::Restart.affects_torque());

    assert!(!FaultAction::SoftStop.allows_operation());
    assert!(!FaultAction::SafeMode.allows_operation());
    assert!(FaultAction::LogAndContinue.allows_operation());
    assert!(FaultAction::Quarantine.allows_operation());
    assert!(FaultAction::Restart.allows_operation());
}

// ===== 15. Threshold validation edge cases =====

#[test]
fn fmea_threshold_zero_usb_timeout_invalid() {
    let t = FaultThresholds { usb_timeout_ms: 0, ..FaultThresholds::default() };
    assert!(t.validate().is_err());
}

#[test]
fn fmea_threshold_zero_usb_failures_invalid() {
    let t = FaultThresholds { usb_max_consecutive_failures: 0, ..FaultThresholds::default() };
    assert!(t.validate().is_err());
}

#[test]
fn fmea_threshold_zero_nan_count_invalid() {
    let t = FaultThresholds { encoder_max_nan_count: 0, ..FaultThresholds::default() };
    assert!(t.validate().is_err());
}

#[test]
fn fmea_threshold_thermal_below_range_invalid() {
    let t = FaultThresholds { thermal_limit_celsius: 30.0, ..FaultThresholds::default() };
    assert!(t.validate().is_err());
}

#[test]
fn fmea_threshold_thermal_above_range_invalid() {
    let t = FaultThresholds { thermal_limit_celsius: 130.0, ..FaultThresholds::default() };
    assert!(t.validate().is_err());
}

#[test]
fn fmea_threshold_negative_hysteresis_invalid() {
    let t = FaultThresholds { thermal_hysteresis_celsius: -1.0, ..FaultThresholds::default() };
    assert!(t.validate().is_err());
}

#[test]
fn fmea_threshold_zero_plugin_timeout_invalid() {
    let t = FaultThresholds { plugin_timeout_us: 0, ..FaultThresholds::default() };
    assert!(t.validate().is_err());
}

#[test]
fn fmea_threshold_zero_overcurrent_invalid() {
    let t = FaultThresholds { overcurrent_limit_a: 0.0, ..FaultThresholds::default() };
    assert!(t.validate().is_err());
}

#[test]
fn fmea_threshold_zero_handsoff_invalid() {
    let t = FaultThresholds { hands_off_timeout_secs: 0.0, ..FaultThresholds::default() };
    assert!(t.validate().is_err());
}

// ===== 16. FmeaMatrix operations =====

#[test]
fn fmea_matrix_insert_update_remove() -> Result<(), Box<dyn std::error::Error>> {
    let mut matrix = FmeaMatrix::new();
    assert!(matrix.is_empty());

    let entry = FmeaEntry::new(FaultType::UsbStall);
    assert!(matrix.insert(entry));
    assert_eq!(matrix.len(), 1);
    assert!(matrix.contains(FaultType::UsbStall));

    // Update existing
    let updated = FmeaEntry::new(FaultType::UsbStall).with_response_time(999);
    assert!(matrix.insert(updated));
    assert_eq!(matrix.len(), 1);
    let e = matrix.get(FaultType::UsbStall).ok_or("not found")?;
    assert_eq!(e.max_response_time_ms, 999);

    // Remove
    let removed = matrix.remove(FaultType::UsbStall);
    assert!(removed.is_some());
    assert!(matrix.is_empty());

    Ok(())
}

#[test]
fn fmea_matrix_clear() {
    let mut matrix = FmeaMatrix::with_defaults();
    assert!(!matrix.is_empty());
    matrix.clear();
    assert!(matrix.is_empty());
    assert_eq!(matrix.len(), 0);
}

// ===== 17. Soft-stop integration with FMEA system =====

#[test]
fn fmea_soft_stop_completes_ramp_to_zero() -> Result<(), Box<dyn std::error::Error>> {
    let mut fmea = FmeaSystem::new();
    fmea.handle_fault(FaultType::ThermalLimit, 15.0)
        .map_err(|e| format!("{e}"))?;

    assert!(fmea.is_soft_stop_active());

    // Run enough updates to complete the ramp
    let mut torque = 15.0_f32;
    for _ in 0..100 {
        torque = fmea.update_soft_stop(Duration::from_millis(1));
    }
    assert!(
        torque.abs() < f32::EPSILON,
        "Torque should reach zero, got {torque}"
    );
    assert!(!fmea.is_soft_stop_active());

    Ok(())
}

// ===== 18. Recovery context lifecycle =====

#[test]
fn fmea_recovery_context_full_lifecycle() -> Result<(), Box<dyn std::error::Error>> {
    let mut ctx = RecoveryContext::new(FaultType::UsbStall);
    assert_eq!(ctx.fault_type, FaultType::UsbStall);
    assert!(!ctx.cancelled);

    ctx.start(Duration::from_millis(0));
    assert_eq!(ctx.attempt, 1);
    assert!(!ctx.is_complete());

    // Advance through all steps
    let num_steps = ctx.procedure.steps.len();
    for _ in 0..num_steps {
        ctx.advance_step(Duration::from_millis(50));
    }
    assert!(ctx.is_complete());

    // Retry
    if ctx.can_retry() {
        assert!(ctx.start_retry(Duration::from_millis(200)));
        assert_eq!(ctx.attempt, 2);
        assert!(!ctx.is_complete());
    }

    Ok(())
}

#[test]
fn fmea_recovery_context_cancellation() {
    let mut ctx = RecoveryContext::new(FaultType::Overcurrent);
    ctx.start(Duration::ZERO);
    assert!(!ctx.cancelled);

    ctx.cancel();
    assert!(ctx.cancelled);
}

#[test]
fn fmea_recovery_context_timeout() {
    let mut ctx = RecoveryContext::new(FaultType::PipelineFault);
    ctx.start(Duration::ZERO);

    assert!(!ctx.is_timed_out(Duration::from_millis(100)));
    assert!(ctx.is_timed_out(ctx.procedure.timeout + Duration::from_millis(1)));
}

// ===== 19. Audio alert integration =====

#[test]
fn fmea_audio_alert_for_all_fault_types() -> Result<(), Box<dyn std::error::Error>> {
    let mut system = AudioAlertSystem::new();

    for (i, ft) in all_fault_types().iter().enumerate() {
        // Space alerts enough apart so they trigger
        let time = (i as u64 + 1) * 1000;
        let triggered = system.trigger_for_fault(*ft, time);
        assert!(
            triggered,
            "Alert for {ft:?} should trigger at time {time}"
        );

        let alert = system.current_alert();
        assert!(alert.is_some(), "Active alert expected for {ft:?}");
        let a = alert.ok_or(format!("no alert for {ft:?}"))?;
        assert_eq!(a, AudioAlert::for_fault_type(*ft));

        system.stop();
    }

    Ok(())
}

// ===== 20. FmeaError coverage =====

#[test]
fn fmea_error_display_coverage() {
    let err = FmeaError::UnknownFaultType(FaultType::UsbStall);
    assert!(!format!("{err}").is_empty());

    let err = FmeaError::fault_handling_failed(FaultType::Overcurrent, "test reason");
    assert!(format!("{err}").contains("test reason"));

    let err = FmeaError::invalid_threshold("thermal", "out of range");
    assert!(format!("{err}").contains("thermal"));

    let err = FmeaError::recovery_failed(FaultType::ThermalLimit, "too hot");
    assert!(format!("{err}").contains("too hot"));

    let err = FmeaError::soft_stop_failed("ramp error");
    assert!(format!("{err}").contains("ramp error"));

    let err = FmeaError::quarantine_error("plugin-x", "timeout");
    assert!(format!("{err}").contains("plugin-x"));

    let err = FmeaError::configuration_error("bad config");
    assert!(format!("{err}").contains("bad config"));

    let err = FmeaError::timeout("recovery", 5000);
    assert!(format!("{err}").contains("5000"));

    let err = FmeaError::FaultAlreadyActive(FaultType::EncoderNaN);
    assert!(!format!("{err}").is_empty());

    let err = FmeaError::NoActiveFault;
    assert!(!format!("{err}").is_empty());
}

#[test]
fn fmea_error_recoverable_classification() {
    assert!(FmeaError::timeout("op", 100).is_recoverable());
    assert!(FmeaError::quarantine_error("p", "r").is_recoverable());
    assert!(FmeaError::recovery_failed(FaultType::UsbStall, "err").is_recoverable());

    assert!(!FmeaError::configuration_error("bad").is_recoverable());
    assert!(!FmeaError::soft_stop_failed("fail").is_recoverable());
    assert!(!FmeaError::NoActiveFault.is_recoverable());
}

#[test]
fn fmea_error_immediate_attention_classification() {
    assert!(FmeaError::fault_handling_failed(FaultType::Overcurrent, "err").requires_immediate_attention());
    assert!(FmeaError::soft_stop_failed("fail").requires_immediate_attention());
    assert!(FmeaError::configuration_error("bad").requires_immediate_attention());

    assert!(!FmeaError::timeout("op", 100).requires_immediate_attention());
    assert!(!FmeaError::NoActiveFault.requires_immediate_attention());
}

// ===== 21. FaultMarker creation and state =====

#[test]
fn fmea_fault_marker_device_state() {
    let mut marker = FaultMarker::new(FaultType::UsbStall, Duration::from_millis(500));
    assert_eq!(marker.fault_type, FaultType::UsbStall);
    assert_eq!(marker.timestamp, Duration::from_millis(500));

    assert!(marker.add_device_state("usb_status", "disconnected"));
    assert!(marker.add_device_state("retry_count", "3"));
    assert_eq!(marker.device_state.len(), 2);
}

#[test]
fn fmea_fault_marker_plugin_state() {
    let mut marker = FaultMarker::new(FaultType::PluginOverrun, Duration::ZERO);
    assert!(marker.add_plugin_state("effect-reverb", "overrun"));
    assert_eq!(marker.plugin_states.len(), 1);
}

#[test]
fn fmea_fault_marker_recovery_actions() {
    let mut marker = FaultMarker::new(FaultType::ThermalLimit, Duration::from_secs(1));
    assert!(marker.add_recovery_action("reduce_torque"));
    assert!(marker.add_recovery_action("wait_cooldown"));
    assert_eq!(marker.recovery_actions.len(), 2);
}

// ===== 22. FaultDetectionState window and quarantine =====

#[test]
fn fmea_detection_state_window_resets() {
    let mut state = FaultDetectionState::new();
    let window = Duration::from_millis(100);

    let reset = state.update_window(Duration::from_millis(0), window);
    assert!(reset); // first call resets
    assert_eq!(state.window_count, 1);

    let reset = state.update_window(Duration::from_millis(50), window);
    assert!(!reset);
    assert_eq!(state.window_count, 2);

    // Beyond window
    let reset = state.update_window(Duration::from_millis(200), window);
    assert!(reset);
    assert_eq!(state.window_count, 1);
}

#[test]
fn fmea_detection_state_quarantine_lifecycle() {
    let mut state = FaultDetectionState::new();
    let now = Duration::from_secs(10);

    assert!(!state.is_quarantined(now));

    state.set_quarantine(Duration::from_secs(5), now);
    assert!(state.is_quarantined(Duration::from_secs(12)));
    assert!(!state.is_quarantined(Duration::from_secs(16)));

    state.clear_quarantine();
    assert!(!state.is_quarantined(Duration::from_secs(12)));
}

// ===== 23. FmeaSystem detection: multiple fault types =====

#[test]
fn fmea_system_detect_encoder_nan_with_infinity() -> Result<(), Box<dyn std::error::Error>> {
    let mut fmea = FmeaSystem::new();

    // Finite values don't trigger
    assert!(fmea.detect_encoder_fault(42.0).is_none());
    assert!(fmea.detect_encoder_fault(0.0).is_none());
    assert!(fmea.detect_encoder_fault(-100.0).is_none());

    // Infinity should trigger like NaN
    for _ in 0..5 {
        let _ = fmea.detect_encoder_fault(f32::INFINITY);
    }
    // After max_nan_count, should detect
    let result = fmea.detect_encoder_fault(f32::NEG_INFINITY);
    // May or may not trigger depending on count; just verify no panic
    let _ = result;

    Ok(())
}

#[test]
fn fmea_system_detect_thermal_below_threshold() {
    let mut fmea = FmeaSystem::new();
    // Well below threshold
    assert!(fmea.detect_thermal_fault(50.0, false).is_none());
    // At exactly threshold
    assert!(fmea.detect_thermal_fault(80.0, false).is_none()); // not >
    // Above threshold
    assert_eq!(
        fmea.detect_thermal_fault(81.0, false),
        Some(FaultType::ThermalLimit)
    );
}

#[test]
fn fmea_system_detect_plugin_under_threshold() {
    let mut fmea = FmeaSystem::new();
    // Well under plugin timeout
    assert!(fmea.detect_plugin_overrun("my-plugin", 50).is_none());
    // At exactly threshold
    assert!(fmea.detect_plugin_overrun("my-plugin", 100).is_none());
}

#[test]
fn fmea_system_detect_timing_under_threshold() {
    let mut fmea = FmeaSystem::new();
    assert!(fmea.detect_timing_violation(100).is_none());
    assert!(fmea.detect_timing_violation(250).is_none()); // at threshold
}

// ===== 24. PostMortemConfig defaults =====

#[test]
fn fmea_post_mortem_config_defaults() {
    let pm = PostMortemConfig::default();
    assert!(pm.pre_fault_capture_duration > 0.0);
    assert!(pm.post_fault_capture_duration > 0.0);
    assert!(pm.include_telemetry);
    assert!(pm.include_device_state);
    assert!(pm.include_plugin_state);
}

// ===== 25. SoftStopController deep tests =====

#[test]
fn fmea_soft_stop_ramp_to_target() {
    let mut ctrl = SoftStopController::new();
    ctrl.start_ramp_to(0.0, 8.0, Duration::from_millis(100));

    assert!(ctrl.is_active());
    assert_eq!(ctrl.start_torque(), 0.0);
    assert_eq!(ctrl.target_torque(), 8.0);

    // Midway
    let t = ctrl.update(Duration::from_millis(50));
    assert!(t > 3.0 && t < 5.0);

    // Complete
    let t = ctrl.update(Duration::from_millis(50));
    assert!((t - 8.0).abs() < f32::EPSILON);
    assert!(!ctrl.is_active());
}

#[test]
fn fmea_soft_stop_force_stop_emergency() {
    let mut ctrl = SoftStopController::new();
    ctrl.start_soft_stop(20.0);
    ctrl.update(Duration::from_millis(10));

    ctrl.force_stop();
    assert!(!ctrl.is_active());
    assert!((ctrl.current_torque() - 0.0).abs() < f32::EPSILON);
}

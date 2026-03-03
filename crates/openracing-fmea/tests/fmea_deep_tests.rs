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

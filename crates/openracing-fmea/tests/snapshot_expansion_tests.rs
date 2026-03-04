//! Snapshot tests for FMEA types — error messages, fault types, config, and safety state output.

use core::time::Duration;
use openracing_fmea::{
    FaultAction, FaultDetectionState, FaultMarker, FaultThresholds, FaultType, FmeaEntry,
    FmeaError, PostMortemConfig, RecoveryContext, RecoveryProcedure, RecoveryResult,
    RecoveryStatus,
};

// --- FaultType Display (all 9 variants) ---

#[test]
fn snapshot_fault_type_usb_stall_display() {
    insta::assert_snapshot!("fault_type_usb_stall", format!("{}", FaultType::UsbStall));
}

#[test]
fn snapshot_fault_type_encoder_nan_display() {
    insta::assert_snapshot!(
        "fault_type_encoder_nan",
        format!("{}", FaultType::EncoderNaN)
    );
}

#[test]
fn snapshot_fault_type_thermal_limit_display() {
    insta::assert_snapshot!(
        "fault_type_thermal_limit",
        format!("{}", FaultType::ThermalLimit)
    );
}

#[test]
fn snapshot_fault_type_overcurrent_display() {
    insta::assert_snapshot!(
        "fault_type_overcurrent",
        format!("{}", FaultType::Overcurrent)
    );
}

#[test]
fn snapshot_fault_type_plugin_overrun_display() {
    insta::assert_snapshot!(
        "fault_type_plugin_overrun",
        format!("{}", FaultType::PluginOverrun)
    );
}

#[test]
fn snapshot_fault_type_timing_violation_display() {
    insta::assert_snapshot!(
        "fault_type_timing_violation",
        format!("{}", FaultType::TimingViolation)
    );
}

#[test]
fn snapshot_fault_type_safety_interlock_display() {
    insta::assert_snapshot!(
        "fault_type_safety_interlock",
        format!("{}", FaultType::SafetyInterlockViolation)
    );
}

#[test]
fn snapshot_fault_type_hands_off_display() {
    insta::assert_snapshot!(
        "fault_type_hands_off",
        format!("{}", FaultType::HandsOffTimeout)
    );
}

#[test]
fn snapshot_fault_type_pipeline_fault_display() {
    insta::assert_snapshot!(
        "fault_type_pipeline_fault",
        format!("{}", FaultType::PipelineFault)
    );
}

// --- FmeaError Display (all 10 variants) ---

#[test]
fn snapshot_fmea_error_unknown_fault_type() {
    let err = FmeaError::UnknownFaultType(FaultType::UsbStall);
    insta::assert_snapshot!("fmea_error_unknown_fault_type", format!("{}", err));
}

#[test]
fn snapshot_fmea_error_fault_handling_failed() {
    let err =
        FmeaError::fault_handling_failed(FaultType::ThermalLimit, "temperature sensor offline");
    insta::assert_snapshot!("fmea_error_fault_handling_failed", format!("{}", err));
}

#[test]
fn snapshot_fmea_error_invalid_threshold() {
    let err = FmeaError::invalid_threshold("thermal_limit", "must be between 40 and 120");
    insta::assert_snapshot!("fmea_error_invalid_threshold", format!("{}", err));
}

#[test]
fn snapshot_fmea_error_recovery_failed() {
    let err = FmeaError::recovery_failed(FaultType::UsbStall, "device not responding");
    insta::assert_snapshot!("fmea_error_recovery_failed", format!("{}", err));
}

#[test]
fn snapshot_fmea_error_soft_stop_failed() {
    let err = FmeaError::soft_stop_failed("ramp function returned NaN");
    insta::assert_snapshot!("fmea_error_soft_stop_failed", format!("{}", err));
}

#[test]
fn snapshot_fmea_error_quarantine_error() {
    let err = FmeaError::quarantine_error("my-plugin", "already quarantined");
    insta::assert_snapshot!("fmea_error_quarantine", format!("{}", err));
}

#[test]
fn snapshot_fmea_error_configuration_error() {
    let err = FmeaError::configuration_error("missing FMEA entry for fault type");
    insta::assert_snapshot!("fmea_error_configuration", format!("{}", err));
}

#[test]
fn snapshot_fmea_error_fault_already_active() {
    let err = FmeaError::FaultAlreadyActive(FaultType::Overcurrent);
    insta::assert_snapshot!("fmea_error_fault_already_active", format!("{}", err));
}

#[test]
fn snapshot_fmea_error_no_active_fault() {
    insta::assert_snapshot!(
        "fmea_error_no_active_fault",
        format!("{}", FmeaError::NoActiveFault)
    );
}

#[test]
fn snapshot_fmea_error_timeout() {
    let err = FmeaError::timeout("recovery", 5000);
    insta::assert_snapshot!("fmea_error_timeout", format!("{}", err));
}

// --- FaultAction Debug (all 5 variants) ---

#[test]
fn snapshot_fault_action_all_variants() {
    insta::assert_debug_snapshot!("fault_action_soft_stop", FaultAction::SoftStop);
    insta::assert_debug_snapshot!("fault_action_quarantine", FaultAction::Quarantine);
    insta::assert_debug_snapshot!("fault_action_log_continue", FaultAction::LogAndContinue);
    insta::assert_debug_snapshot!("fault_action_restart", FaultAction::Restart);
    insta::assert_debug_snapshot!("fault_action_safe_mode", FaultAction::SafeMode);
}

// --- FaultThresholds Debug (default, conservative, relaxed) ---

#[test]
fn snapshot_fault_thresholds_default() {
    insta::assert_debug_snapshot!("fault_thresholds_default", FaultThresholds::default());
}

#[test]
fn snapshot_fault_thresholds_conservative() {
    insta::assert_debug_snapshot!(
        "fault_thresholds_conservative",
        FaultThresholds::conservative()
    );
}

#[test]
fn snapshot_fault_thresholds_relaxed() {
    insta::assert_debug_snapshot!("fault_thresholds_relaxed", FaultThresholds::relaxed());
}

// --- PostMortemConfig Debug ---

#[test]
fn snapshot_post_mortem_config_default() {
    insta::assert_debug_snapshot!("post_mortem_config_default", PostMortemConfig::default());
}

// --- RecoveryStatus Debug (all variants) ---

#[test]
fn snapshot_recovery_status_all_variants() {
    insta::assert_debug_snapshot!("recovery_status_pending", RecoveryStatus::Pending);
    insta::assert_debug_snapshot!("recovery_status_in_progress", RecoveryStatus::InProgress);
    insta::assert_debug_snapshot!("recovery_status_completed", RecoveryStatus::Completed);
    insta::assert_debug_snapshot!("recovery_status_failed", RecoveryStatus::Failed);
    insta::assert_debug_snapshot!("recovery_status_cancelled", RecoveryStatus::Cancelled);
    insta::assert_debug_snapshot!("recovery_status_timeout", RecoveryStatus::Timeout);
}

// --- RecoveryResult Debug ---

#[test]
fn snapshot_recovery_result_success() {
    let result = RecoveryResult::success(Duration::from_millis(250), 1);
    insta::assert_debug_snapshot!("recovery_result_success", result);
}

#[test]
fn snapshot_recovery_result_failed() {
    let result = RecoveryResult::failed(Duration::from_millis(5000), 3, "device unresponsive");
    insta::assert_debug_snapshot!("recovery_result_failed", result);
}

#[test]
fn snapshot_recovery_result_timeout() {
    let result = RecoveryResult::timeout(Duration::from_secs(10), 2);
    insta::assert_debug_snapshot!("recovery_result_timeout", result);
}

// --- FaultDetectionState Debug ---

#[test]
fn snapshot_fault_detection_state_default() {
    insta::assert_debug_snapshot!(
        "fault_detection_state_default",
        FaultDetectionState::default()
    );
}

#[test]
fn snapshot_fault_detection_state_with_fault() {
    let mut state = FaultDetectionState::new();
    state.record_fault(Duration::from_millis(100));
    state.record_fault(Duration::from_millis(200));
    insta::assert_debug_snapshot!("fault_detection_state_with_faults", state);
}

// --- FaultMarker Debug ---

#[test]
fn snapshot_fault_marker_basic() {
    let marker = FaultMarker::new(FaultType::UsbStall, Duration::from_millis(1500));
    insta::assert_debug_snapshot!("fault_marker_basic", marker);
}

#[test]
fn snapshot_fault_marker_with_context() {
    let mut marker = FaultMarker::new(FaultType::ThermalLimit, Duration::from_secs(30));
    marker.add_device_state("temperature", "85.5C");
    marker.add_device_state("fan_speed", "4200rpm");
    marker.add_plugin_state("telemetry-plugin", "running");
    marker.add_recovery_action("reduce_torque");
    insta::assert_debug_snapshot!("fault_marker_with_context", marker);
}

// --- FmeaEntry Debug ---

#[test]
fn snapshot_fmea_entry_usb_stall() {
    let entry = FmeaEntry::new(FaultType::UsbStall);
    insta::assert_debug_snapshot!("fmea_entry_usb_stall", entry);
}

#[test]
fn snapshot_fmea_entry_overcurrent() {
    let entry = FmeaEntry::new(FaultType::Overcurrent);
    insta::assert_debug_snapshot!("fmea_entry_overcurrent", entry);
}

#[test]
fn snapshot_fmea_entry_plugin_overrun() {
    let entry = FmeaEntry::new(FaultType::PluginOverrun);
    insta::assert_debug_snapshot!("fmea_entry_plugin_overrun", entry);
}

// --- RecoveryProcedure Debug ---

#[test]
fn snapshot_recovery_procedure_usb_stall() {
    let proc = RecoveryProcedure::default_for(FaultType::UsbStall);
    insta::assert_debug_snapshot!("recovery_procedure_usb_stall", proc);
}

#[test]
fn snapshot_recovery_procedure_thermal() {
    let proc = RecoveryProcedure::default_for(FaultType::ThermalLimit);
    insta::assert_debug_snapshot!("recovery_procedure_thermal", proc);
}

#[test]
fn snapshot_recovery_procedure_overcurrent() {
    let proc = RecoveryProcedure::default_for(FaultType::Overcurrent);
    insta::assert_debug_snapshot!("recovery_procedure_overcurrent", proc);
}

// --- RecoveryContext Debug ---

#[test]
fn snapshot_recovery_context_new() {
    let ctx = RecoveryContext::new(FaultType::UsbStall);
    insta::assert_debug_snapshot!("recovery_context_usb_stall", ctx);
}

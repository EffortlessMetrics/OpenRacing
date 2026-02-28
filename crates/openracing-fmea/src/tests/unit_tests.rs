//! Unit tests for FMEA components.

use crate::*;

#[test]
fn test_fault_type_display() {
    assert_eq!(
        format!("{}", FaultType::UsbStall),
        "USB communication stall"
    );
    assert_eq!(
        format!("{}", FaultType::EncoderNaN),
        "Encoder returned invalid data"
    );
}

#[test]
fn test_fault_type_properties() {
    // Severity levels
    assert!(FaultType::Overcurrent.severity() < FaultType::PluginOverrun.severity());

    // Immediate response
    assert!(FaultType::Overcurrent.requires_immediate_response());
    assert!(!FaultType::TimingViolation.requires_immediate_response());

    // Recoverability
    assert!(FaultType::UsbStall.is_recoverable());
    assert!(!FaultType::EncoderNaN.is_recoverable());
}

#[test]
fn test_fault_thresholds_defaults() {
    let t = FaultThresholds::default();
    assert!(t.validate().is_ok());
}

#[test]
fn test_fault_thresholds_validation() {
    let mut t = FaultThresholds::default();
    assert!(t.validate().is_ok());

    t.thermal_limit_celsius = 20.0; // Too low
    assert!(t.validate().is_err());

    t.thermal_limit_celsius = 130.0; // Too high
    assert!(t.validate().is_err());
}

#[test]
fn test_fault_action_properties() {
    assert!(FaultAction::SoftStop.affects_torque());
    assert!(!FaultAction::LogAndContinue.affects_torque());

    assert!(FaultAction::Quarantine.allows_operation());
    assert!(!FaultAction::SafeMode.allows_operation());
}

#[test]
fn test_audio_alert_severity() {
    assert!(AudioAlert::Urgent.severity() > AudioAlert::SingleBeep.severity());
    assert!(AudioAlert::ContinuousBeep.is_continuous());
    assert!(!AudioAlert::DoubleBeep.is_continuous());
}

#[test]
fn test_audio_alert_for_fault() {
    assert_eq!(
        AudioAlert::for_fault_type(FaultType::Overcurrent),
        AudioAlert::Urgent
    );
    assert_eq!(
        AudioAlert::for_fault_type(FaultType::ThermalLimit),
        AudioAlert::ContinuousBeep
    );
}

#[test]
fn test_soft_stop_basic() {
    let mut ctrl = SoftStopController::new();
    assert!(!ctrl.is_active());

    ctrl.start_soft_stop(10.0);
    assert!(ctrl.is_active());
    assert_eq!(ctrl.start_torque(), 10.0);
    assert_eq!(ctrl.target_torque(), 0.0);
}

#[test]
fn test_soft_stop_ramp() {
    let mut ctrl = SoftStopController::new();
    ctrl.start_soft_stop_with_duration(10.0, core::time::Duration::from_millis(100));

    // At start
    assert_eq!(ctrl.current_torque(), 10.0);
    assert_eq!(ctrl.progress(), 0.0);

    // After 50ms (50% progress)
    let t = ctrl.update(core::time::Duration::from_millis(50));
    assert!((t - 5.0).abs() < 0.1);
    assert!((ctrl.progress() - 0.5).abs() < 0.01);

    // After 100ms (complete)
    let t = ctrl.update(core::time::Duration::from_millis(50));
    assert!(!ctrl.is_active());
    assert_eq!(t, 0.0);
}

#[test]
fn test_soft_stop_force_stop() {
    let mut ctrl = SoftStopController::new();
    ctrl.start_soft_stop(10.0);
    assert!(ctrl.is_active());

    ctrl.force_stop();
    assert!(!ctrl.is_active());
    assert_eq!(ctrl.current_torque(), 0.0);
}

#[test]
fn test_recovery_procedure_defaults() {
    let p = RecoveryProcedure::default_for(FaultType::UsbStall);
    assert_eq!(p.fault_type, FaultType::UsbStall);
    assert!(p.automatic);
    assert!(!p.steps.is_empty());
}

#[test]
fn test_recovery_procedure_encoder() {
    let p = RecoveryProcedure::default_for(FaultType::EncoderNaN);
    assert!(!p.automatic); // Requires manual calibration
}

#[test]
fn test_recovery_context() {
    let mut ctx = RecoveryContext::new(FaultType::UsbStall);
    ctx.start(core::time::Duration::ZERO);

    assert_eq!(ctx.attempt, 1);
    assert_eq!(ctx.current_step, 0);

    ctx.advance_step(core::time::Duration::from_millis(100));
    assert_eq!(ctx.current_step, 1);
}

#[test]
fn test_fmea_entry() {
    let entry = FmeaEntry::new(FaultType::UsbStall);
    assert_eq!(entry.fault_type, FaultType::UsbStall);
    assert!(entry.enabled);
}

#[test]
fn test_fmea_matrix() {
    let matrix = FmeaMatrix::with_defaults();
    assert!(matrix.contains(FaultType::UsbStall));
    assert!(matrix.contains(FaultType::EncoderNaN));
    assert!(matrix.get(FaultType::UsbStall).is_some());
}

#[test]
fn test_fmea_system_initialization() {
    let fmea = FmeaSystem::new();
    assert!(!fmea.has_active_fault());
    assert!(!fmea.is_soft_stop_active());
}

#[test]
fn test_fmea_usb_detection() {
    let mut fmea = FmeaSystem::new();

    // No fault initially
    let result = fmea.detect_usb_fault(0, Some(core::time::Duration::ZERO));
    assert!(result.is_none());

    // Threshold fault
    let result = fmea.detect_usb_fault(3, Some(core::time::Duration::ZERO));
    assert_eq!(result, Some(FaultType::UsbStall));
}

#[test]
fn test_fmea_thermal_detection() {
    let mut fmea = FmeaSystem::new();

    // Normal temp
    assert!(fmea.detect_thermal_fault(70.0, false).is_none());

    // Over limit
    assert_eq!(
        fmea.detect_thermal_fault(85.0, false),
        Some(FaultType::ThermalLimit)
    );
}

#[test]
fn test_fmea_fault_handling() {
    let mut fmea = FmeaSystem::new();

    let result = fmea.handle_fault(FaultType::UsbStall, 10.0);
    assert!(result.is_ok());
    assert!(fmea.has_active_fault());
    assert!(fmea.is_soft_stop_active());
}

#[test]
fn test_fmea_clear_fault() -> Result<(), Box<dyn std::error::Error>> {
    let mut fmea = FmeaSystem::new();
    fmea.handle_fault(FaultType::UsbStall, 10.0)?;

    let result = fmea.clear_fault();
    assert!(result.is_ok());
    assert!(!fmea.has_active_fault());
    Ok(())
}

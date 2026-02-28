//! State machine tests for FMEA system.

use crate::*;
use core::time::Duration;

/// Represents the state of the FMEA system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FmeaState {
    Normal,
    Faulted,
    SoftStopping,
    Recovering,
}

impl FmeaSystem {
    fn state(&self) -> FmeaState {
        if self.has_active_fault() {
            if self.is_soft_stop_active() {
                FmeaState::SoftStopping
            } else if self.can_recover() {
                FmeaState::Recovering
            } else {
                FmeaState::Faulted
            }
        } else {
            FmeaState::Normal
        }
    }
}

#[test]
fn test_state_machine_initial_state() {
    let fmea = FmeaSystem::new();
    assert_eq!(fmea.state(), FmeaState::Normal);
}

#[test]
fn test_state_machine_fault_transition() -> Result<(), Box<FmeaError>> {
    let mut fmea = FmeaSystem::new();
    assert_eq!(fmea.state(), FmeaState::Normal);

    // Trigger a fault
    fmea.handle_fault(FaultType::UsbStall, 10.0)?;

    // Should be in soft-stopping state
    assert_eq!(fmea.state(), FmeaState::SoftStopping);
    Ok(())
}

#[test]
fn test_state_machine_soft_stop_completion() -> Result<(), Box<FmeaError>> {
    let mut fmea = FmeaSystem::new();
    fmea.handle_fault(FaultType::UsbStall, 10.0)?;
    assert_eq!(fmea.state(), FmeaState::SoftStopping);

    // Complete soft-stop
    fmea.update_soft_stop(Duration::from_millis(100));
    assert!(!fmea.is_soft_stop_active());

    // USBStall is recoverable, so should be in Recovering state
    assert_eq!(fmea.state(), FmeaState::Recovering);
    Ok(())
}

#[test]
fn test_state_machine_clear_fault() -> Result<(), Box<FmeaError>> {
    let mut fmea = FmeaSystem::new();
    fmea.handle_fault(FaultType::UsbStall, 10.0)?;
    fmea.update_soft_stop(Duration::from_millis(100));

    // USBStall is recoverable
    assert_eq!(fmea.state(), FmeaState::Recovering);

    // Clear fault
    fmea.clear_fault()?;
    assert_eq!(fmea.state(), FmeaState::Normal);
    Ok(())
}

#[test]
fn test_state_machine_multiple_faults() -> Result<(), Box<FmeaError>> {
    let mut fmea = FmeaSystem::new();

    // First fault
    fmea.handle_fault(FaultType::UsbStall, 10.0)?;
    assert!(fmea.has_active_fault());

    // Second fault (should not change active fault)
    let result = fmea.handle_fault(FaultType::ThermalLimit, 5.0);
    assert!(result.is_ok());
    assert_eq!(fmea.active_fault(), Some(FaultType::ThermalLimit));
    Ok(())
}

#[test]
fn test_state_machine_recoverable_vs_non_recoverable() -> Result<(), Box<FmeaError>> {
    let mut fmea1 = FmeaSystem::new();
    fmea1.handle_fault(FaultType::UsbStall, 10.0)?;
    fmea1.update_soft_stop(Duration::from_millis(100));
    assert!(fmea1.can_recover());

    let mut fmea2 = FmeaSystem::new();
    fmea2.handle_fault(FaultType::EncoderNaN, 10.0)?;
    fmea2.update_soft_stop(Duration::from_millis(100));
    assert!(!fmea2.can_recover());
    Ok(())
}

#[test]
fn test_state_machine_audio_alerts() -> Result<(), Box<FmeaError>> {
    let mut fmea = FmeaSystem::new();
    fmea.handle_fault(FaultType::Overcurrent, 10.0)?;

    // Urgent alert should be triggered
    let alert = fmea.audio_alerts().current_alert();
    assert_eq!(alert, Some(AudioAlert::Urgent));
    Ok(())
}

#[test]
fn test_state_machine_detection_reset_on_clear() -> Result<(), Box<FmeaError>> {
    let mut fmea = FmeaSystem::new();

    // Trigger detection (but not full fault)
    fmea.detect_usb_fault(2, Some(Duration::ZERO));

    // Clear fault should reset detection state
    fmea.handle_fault(FaultType::UsbStall, 10.0)?;
    fmea.clear_fault()?;

    // Detection state should be reset
    let stats: Vec<_> = fmea.fault_statistics().collect();
    let usb = stats.iter().find(|(ft, _, _)| *ft == FaultType::UsbStall);
    assert_eq!(usb.map(|entry| entry.1), Some(0));
    Ok(())
}

#[test]
fn test_state_machine_timing_violation_accumulation() {
    let mut fmea = FmeaSystem::new();

    // Accumulate violations
    for _ in 0..99 {
        assert!(fmea.detect_timing_violation(500).is_none());
    }

    // 100th should trigger
    assert_eq!(
        fmea.detect_timing_violation(500),
        Some(FaultType::TimingViolation)
    );
}

#[test]
fn test_state_machine_soft_stop_cancellation() -> Result<(), Box<FmeaError>> {
    let mut fmea = FmeaSystem::new();
    fmea.handle_fault(FaultType::UsbStall, 10.0)?;
    assert!(fmea.is_soft_stop_active());

    // Force stop soft-stop
    fmea.soft_stop_mut().force_stop();
    assert!(!fmea.is_soft_stop_active());

    // Fault should still be active
    assert!(fmea.has_active_fault());
    Ok(())
}

#[test]
fn test_state_machine_time_updates() {
    let mut fmea = FmeaSystem::new();
    assert_eq!(fmea.current_time(), Duration::ZERO);

    fmea.update_time(Duration::from_millis(100));
    assert_eq!(fmea.current_time(), Duration::from_millis(100));
}

#[test]
fn test_state_machine_threshold_changes() {
    let mut fmea = FmeaSystem::new();
    let new_thresholds = FaultThresholds {
        usb_max_consecutive_failures: 1,
        ..Default::default()
    };

    fmea.set_thresholds(new_thresholds);

    // Should fault after just 1 failure now
    let result = fmea.detect_usb_fault(1, Some(Duration::ZERO));
    assert_eq!(result, Some(FaultType::UsbStall));
}

#[test]
fn test_state_matrix_modification() {
    let mut fmea = FmeaSystem::new();

    // Disable a fault type
    if let Some(entry) = fmea.fmea_matrix_mut().get_mut(FaultType::TimingViolation) {
        entry.enabled = false;
    }

    // Fault handling should succeed but not affect state
    let result = fmea.handle_fault(FaultType::TimingViolation, 10.0);
    assert!(result.is_ok());
    assert!(!fmea.has_active_fault()); // Disabled, so no fault recorded
}

#[test]
fn test_state_machine_all_fault_types() {
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
        let mut fmea = FmeaSystem::new();
        let result = fmea.handle_fault(fault_type, 10.0);
        assert!(result.is_ok(), "Failed to handle {:?}", fault_type);
        assert!(fmea.has_active_fault());
        assert_eq!(fmea.active_fault(), Some(fault_type));
    }
}

#[test]
fn test_state_machine_recovery_procedure_availability() -> Result<(), Box<FmeaError>> {
    let mut fmea = FmeaSystem::new();
    fmea.handle_fault(FaultType::UsbStall, 10.0)?;

    let procedure = fmea.recovery_procedure();
    assert!(procedure.is_some());
    assert_eq!(procedure.map(|p| p.fault_type), Some(FaultType::UsbStall));
    Ok(())
}

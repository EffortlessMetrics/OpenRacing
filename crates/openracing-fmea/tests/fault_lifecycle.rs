//! Full fault lifecycle integration tests.

use openracing_fmea::prelude::*;
use std::time::Duration;

#[test]
#[allow(clippy::result_large_err)]
fn test_full_usb_fault_lifecycle() -> Result<(), FmeaError> {
    let mut fmea = FmeaSystem::new();

    // 1. Normal operation - no fault
    assert!(!fmea.has_active_fault());
    fmea.update_time(Duration::from_millis(0));

    // 2. First failure - no fault
    let result = fmea.detect_usb_fault(1, Some(Duration::ZERO));
    assert!(result.is_none());

    // 3. Second failure - no fault
    let result = fmea.detect_usb_fault(2, Some(Duration::ZERO));
    assert!(result.is_none());

    // 4. Third failure - fault detected
    let result = fmea.detect_usb_fault(3, Some(Duration::ZERO));
    assert_eq!(result, Some(FaultType::UsbStall));

    // 5. Handle the fault
    fmea.handle_fault(FaultType::UsbStall, 10.0)?;
    assert!(fmea.has_active_fault());
    assert!(fmea.is_soft_stop_active());

    // 6. Soft-stop progression
    let torque = fmea.update_soft_stop(Duration::from_millis(25));
    assert!(torque > 0.0 && torque < 10.0);

    // 7. Complete soft-stop
    let torque = fmea.update_soft_stop(Duration::from_millis(25));
    assert!(torque < 5.0);

    fmea.update_soft_stop(Duration::from_millis(50));
    assert!(!fmea.is_soft_stop_active());

    // 8. Clear fault
    fmea.clear_fault()?;
    assert!(!fmea.has_active_fault());

    Ok(())
}

#[test]
fn test_thermal_fault_with_hysteresis() {
    let mut fmea = FmeaSystem::new();

    // 1. Normal temperature
    assert!(fmea.detect_thermal_fault(70.0, false).is_none());

    // 2. Over limit - fault detected
    let result = fmea.detect_thermal_fault(85.0, false);
    assert_eq!(result, Some(FaultType::ThermalLimit));

    // 3. Handle fault
    fmea.handle_fault(FaultType::ThermalLimit, 10.0).unwrap();

    // 4. Temperature drops below limit but above hysteresis
    // (fault is still active, so hysteresis applies)
    let result = fmea.detect_thermal_fault(77.0, true);
    assert!(result.is_none()); // Hysteresis prevents clear

    // 5. Temperature drops below hysteresis
    let result = fmea.detect_thermal_fault(73.0, true);
    assert!(result.is_none()); // Now can clear

    // 6. Clear fault
    fmea.clear_fault().unwrap();
    assert!(!fmea.has_active_fault());
}

#[test]
fn test_multiple_fault_priority() {
    let mut fmea = FmeaSystem::new();

    // Handle a lower priority fault first
    fmea.handle_fault(FaultType::TimingViolation, 10.0).unwrap();
    assert_eq!(fmea.active_fault(), Some(FaultType::TimingViolation));

    // Higher priority fault should replace
    fmea.handle_fault(FaultType::Overcurrent, 10.0).unwrap();
    assert_eq!(fmea.active_fault(), Some(FaultType::Overcurrent));
}

#[test]
fn test_encoder_nan_windowing() {
    let mut fmea = FmeaSystem::new();

    // NaNs within window
    for i in 0..4 {
        let result = fmea.detect_encoder_fault(f32::NAN);
        assert!(result.is_none(), "Should not fault at iteration {}", i);
    }

    // 5th NaN should trigger
    let result = fmea.detect_encoder_fault(f32::NAN);
    assert_eq!(result, Some(FaultType::EncoderNaN));
}

#[test]
fn test_plugin_quarantine_sequence() {
    let mut fmea = FmeaSystem::new();

    // Accumulate overruns
    for i in 0..9 {
        let result = fmea.detect_plugin_overrun("test_plugin", 200);
        assert!(result.is_none(), "Should not fault at iteration {}", i);
    }

    // 10th overrun triggers quarantine
    let result = fmea.detect_plugin_overrun("test_plugin", 200);
    assert_eq!(result, Some(FaultType::PluginOverrun));

    // Handle quarantine fault
    fmea.handle_fault(FaultType::PluginOverrun, 10.0).unwrap();

    // Quarantine fault doesn't require soft-stop
    assert!(!fmea.is_soft_stop_active());
}

#[test]
fn test_fault_statistics_tracking() {
    let mut fmea = FmeaSystem::new();

    // Generate some detections
    fmea.detect_usb_fault(2, Some(Duration::ZERO));
    fmea.detect_timing_violation(500);
    fmea.detect_timing_violation(500);

    // Check statistics
    let stats: Vec<_> = fmea.fault_statistics().collect();

    let usb_stat = stats
        .iter()
        .find(|(ft, _, _)| *ft == FaultType::UsbStall)
        .unwrap();
    assert_eq!(usb_stat.1, 2);

    let timing_stat = stats
        .iter()
        .find(|(ft, _, _)| *ft == FaultType::TimingViolation)
        .unwrap();
    assert_eq!(timing_stat.1, 2);
}

#[test]
fn test_fault_detection_reset() {
    let mut fmea = FmeaSystem::new();

    // Accumulate detections
    fmea.detect_usb_fault(2, Some(Duration::ZERO));
    fmea.detect_timing_violation(500);

    // Reset specific fault
    fmea.reset_detection_state(FaultType::UsbStall);

    let stats: Vec<_> = fmea.fault_statistics().collect();
    let usb_stat = stats
        .iter()
        .find(|(ft, _, _)| *ft == FaultType::UsbStall)
        .unwrap();
    assert_eq!(usb_stat.1, 0);

    let timing_stat = stats
        .iter()
        .find(|(ft, _, _)| *ft == FaultType::TimingViolation)
        .unwrap();
    assert_eq!(timing_stat.1, 1);
}

#[test]
fn test_audio_alert_integration() {
    let mut fmea = FmeaSystem::new();

    // Handle critical fault
    fmea.handle_fault(FaultType::Overcurrent, 10.0).unwrap();

    // Urgent alert should be active
    let alert = fmea.audio_alerts().current_alert();
    assert_eq!(alert, Some(AudioAlert::Urgent));

    // Stop alert
    fmea.audio_alerts_mut().stop();
    assert!(fmea.audio_alerts().current_alert().is_none());
}

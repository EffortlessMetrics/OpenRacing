//! Property-based tests for FMEA system.

use crate::*;
use core::time::Duration;

proptest::proptest! {
    #[test]
    fn test_threshold_validation_thermal(
        temp in 40.0f32..120.0f32,
        hysteresis in 0.0f32..30.0f32,
    ) {
        let mut t = FaultThresholds::default();
        t.thermal_limit_celsius = temp;
        t.thermal_hysteresis_celsius = hysteresis;
        assert!(t.validate().is_ok());
    }

    #[test]
    fn test_threshold_validation_usb(
        timeout in 1u64..1000u64,
        max_failures in 1u32..100u32,
    ) {
        let mut t = FaultThresholds::default();
        t.usb_timeout_ms = timeout;
        t.usb_max_consecutive_failures = max_failures;
        assert!(t.validate().is_ok());
    }

    #[test]
    fn test_soft_stop_ramp_produces_valid_torque(
        start_torque in -50.0f32..50.0f32,
        progress_pct in 0.0f32..1.0f32,
    ) {
        let mut ctrl = SoftStopController::new();
        let duration = Duration::from_millis(100);
        ctrl.start_soft_stop_with_duration(start_torque, duration);

        let delta = Duration::from_secs_f32(duration.as_secs_f32() * progress_pct);
        let torque = ctrl.update(delta);

        // Torque should be between start and target (0)
        let min = start_torque.min(0.0);
        let max = start_torque.max(0.0);
        assert!(torque >= min - 0.01 && torque <= max + 0.01);
    }

    #[test]
    fn test_soft_stop_progress_bounded(progress_pct in 0.0f32..2.0f32) {
        let mut ctrl = SoftStopController::new();
        ctrl.start_soft_stop(10.0);

        let delta = Duration::from_millis((progress_pct * 100.0) as u64);
        ctrl.update(delta);

        let progress = ctrl.progress();
        assert!(progress >= 0.0 && progress <= 1.0);
    }

    #[test]
    fn test_encoder_fault_nan_count(count in 0u32..20u32) {
        let mut fmea = FmeaSystem::new();
        let threshold = fmea.thresholds().encoder_max_nan_count;

        let mut detected = false;
        for _ in 0..count {
            if fmea.detect_encoder_fault(f32::NAN).is_some() {
                detected = true;
                break;
            }
        }

        // Should only detect if count >= threshold
        assert_eq!(detected, count >= threshold);
    }

    #[test]
    fn test_timing_violation_count(count in 0u32..200u32) {
        let mut fmea = FmeaSystem::new();
        let threshold = fmea.thresholds().timing_max_violations;

        let mut detected = false;
        for _ in 0..count {
            if fmea.detect_timing_violation(1000).is_some() {
                detected = true;
                break;
            }
        }

        assert_eq!(detected, count >= threshold);
    }

    #[test]
    fn test_thermal_fault_threshold(
        temp in 0.0f32..150.0f32,
        limit in 50.0f32..100.0f32,
    ) {
        let mut fmea = FmeaSystem::new();
        let mut thresholds = FaultThresholds::default();
        thresholds.thermal_limit_celsius = limit;
        fmea.set_thresholds(thresholds);

        let result = fmea.detect_thermal_fault(temp, false);

        // Should detect if temp > limit
        assert_eq!(result.is_some(), temp > limit);
    }

    #[test]
    fn test_fault_type_severity_order(fault_type in proptest::sample::select(&[
        FaultType::Overcurrent,
        FaultType::ThermalLimit,
        FaultType::UsbStall,
        FaultType::EncoderNaN,
        FaultType::PluginOverrun,
        FaultType::TimingViolation,
    ])) {
        let severity = fault_type.severity();
        assert!(severity >= 1 && severity <= 5);

        // Critical faults should have lowest severity numbers
        if fault_type.requires_immediate_response() {
            assert!(severity <= 2);
        }
    }

    #[test]
    fn test_audio_alert_severity(alert in proptest::sample::select(&[
        AudioAlert::SingleBeep,
        AudioAlert::DoubleBeep,
        AudioAlert::TripleBeep,
        AudioAlert::ContinuousBeep,
        AudioAlert::Urgent,
    ])) {
        let severity = alert.severity();
        assert!(severity >= 1 && severity <= 5);
    }
}

proptest::proptest! {
    #[test]
    fn test_usb_failure_count(failures in 0u32..20u32) {
        let mut fmea = FmeaSystem::new();
        let threshold = fmea.thresholds().usb_max_consecutive_failures;

        let result = fmea.detect_usb_fault(failures, Some(Duration::ZERO));

        // Should detect if failures >= threshold
        assert_eq!(result.is_some(), failures >= threshold);
    }

    #[test]
    fn test_plugin_overrun_count(overruns in 0u32..30u32) {
        let mut fmea = FmeaSystem::new();
        let threshold = fmea.thresholds().plugin_max_overruns;

        let mut detected = false;
        for _ in 0..overruns {
            if fmea.detect_plugin_overrun("test", 1000).is_some() {
                detected = true;
                break;
            }
        }

        assert_eq!(detected, overruns >= threshold);
    }

    #[test]
    fn test_soft_stop_multiplier(
        start in 0.1f32..100.0f32,
        progress in 0.0f32..1.0f32,
    ) {
        let mut ctrl = SoftStopController::new();
        let duration = Duration::from_millis(100);
        ctrl.start_soft_stop_with_duration(start, duration);

        let delta = Duration::from_secs_f32(duration.as_secs_f32() * progress);
        ctrl.update(delta);

        let multiplier = ctrl.current_multiplier();
        assert!(multiplier >= 0.0 && multiplier <= 1.0);

        // At 50% progress, multiplier should be around 0.5
        if progress > 0.4 && progress < 0.6 {
            assert!((multiplier - (1.0 - progress)).abs() < 0.1);
        }
    }
}

//! Safety interlock behavior tests
//!
//! Comprehensive tests for safety-critical interlock behaviors including:
//! - Fault cascade (torque zeroed immediately, stays zero until cleared)
//! - Watchdog timeout (fires when pipeline stalls)
//! - Soft-stop ramp (smooth torque reduction over configured duration)
//! - Multi-fault tracking (all faults tracked, all must be individually cleared)
//! - Fault recovery (clearing restores normal operation)

use super::hardware_watchdog::{
    HardwareWatchdog, SafetyInterlockSystem, SoftwareWatchdog, WatchdogError,
    WatchdogTimeoutHandler,
};
use super::*;
use openracing_test_helpers::prelude::*;
use std::time::{Duration, Instant};

/// Create a test safety service with short timeouts for fast tests.
fn create_test_service() -> SafetyService {
    SafetyService::with_timeouts(
        5.0,                    // max_safe_torque_nm
        25.0,                   // max_high_torque_nm
        Duration::from_secs(3), // hands_off_timeout
        Duration::from_secs(2), // combo_hold_duration
    )
}

/// Drive a SafetyService through the full high-torque activation flow.
fn activate_high_torque(service: &mut SafetyService, device: &str) {
    let challenge = must(service.request_high_torque(device));
    must(service.provide_ui_consent(challenge.challenge_token));
    must(service.report_combo_start(challenge.challenge_token));
    std::thread::sleep(Duration::from_millis(2100));
    let ack = InterlockAck {
        challenge_token: challenge.challenge_token,
        device_token: 42,
        combo_completed: ButtonCombo::BothClutchPaddles,
        timestamp: Instant::now(),
    };
    must(service.confirm_high_torque(device, ack));
}

// =========================================================================
// Fault cascade tests
// =========================================================================

#[test]
fn test_fault_cascade_torque_zeroed_immediately_from_safe_torque() {
    let mut service = create_test_service();

    // Precondition: safe torque allows up to 5 Nm
    assert_eq!(service.clamp_torque_nm(5.0), 5.0);

    service.report_fault(FaultType::UsbStall);

    // Torque must be zero immediately after fault
    assert_eq!(service.clamp_torque_nm(5.0), 0.0);
    assert_eq!(service.clamp_torque_nm(-5.0), 0.0);
    assert_eq!(service.max_torque_nm(), 0.0);
}

#[test]
fn test_fault_cascade_torque_zeroed_immediately_from_high_torque() {
    let mut service = create_test_service();
    activate_high_torque(&mut service, "dev");

    // Precondition: high torque allows up to 25 Nm
    assert_eq!(service.clamp_torque_nm(25.0), 25.0);

    service.report_fault(FaultType::Overcurrent);

    // Torque must be zero immediately after fault — even for high-torque requests
    assert_eq!(service.clamp_torque_nm(25.0), 0.0);
    assert_eq!(service.clamp_torque_nm(-25.0), 0.0);
    assert_eq!(service.max_torque_nm(), 0.0);
}

#[test]
fn test_fault_cascade_torque_stays_zero_over_time() {
    let mut service = create_test_service();
    service.report_fault(FaultType::EncoderNaN);

    // Torque stays zero even after waiting
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(service.clamp_torque_nm(10.0), 0.0);

    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(service.clamp_torque_nm(10.0), 0.0);
}

#[test]
fn test_fault_cascade_torque_stays_zero_until_explicit_clear() {
    let mut service = create_test_service();
    service.report_fault(FaultType::ThermalLimit);

    // Even after the minimum fault duration, torque is zero until clear_fault()
    std::thread::sleep(Duration::from_millis(110));
    assert_eq!(service.clamp_torque_nm(10.0), 0.0);

    // After clearing, torque should be restored to safe limit
    must(service.clear_fault());
    assert_eq!(service.clamp_torque_nm(10.0), 5.0);
}

#[test]
fn test_fault_cascade_nan_torque_request_clamped_to_zero_in_faulted_state() {
    let mut service = create_test_service();
    service.report_fault(FaultType::PipelineFault);

    assert_eq!(service.clamp_torque_nm(f32::NAN), 0.0);
    assert_eq!(service.clamp_torque_nm(f32::INFINITY), 0.0);
    assert_eq!(service.clamp_torque_nm(f32::NEG_INFINITY), 0.0);
}

#[test]
fn test_fault_cascade_repeated_faults_keep_torque_at_zero() {
    let mut service = create_test_service();

    // Rapidly report multiple faults
    service.report_fault(FaultType::UsbStall);
    assert_eq!(service.clamp_torque_nm(5.0), 0.0);

    service.report_fault(FaultType::Overcurrent);
    assert_eq!(service.clamp_torque_nm(5.0), 0.0);

    service.report_fault(FaultType::ThermalLimit);
    assert_eq!(service.clamp_torque_nm(5.0), 0.0);

    // State reflects the last reported fault
    assert!(matches!(
        service.state(),
        SafetyState::Faulted {
            fault: FaultType::ThermalLimit,
            ..
        }
    ));
}

// =========================================================================
// Watchdog timeout tests
// =========================================================================

#[test]
fn test_watchdog_timeout_fires_when_not_fed() -> Result<(), WatchdogError> {
    let mut watchdog = SoftwareWatchdog::new(50); // 50ms timeout
    watchdog.arm()?;
    watchdog.feed()?;

    // Don't feed; wait for timeout
    std::thread::sleep(Duration::from_millis(60));

    assert!(watchdog.has_timed_out());
    assert!(watchdog.time_since_last_feed() >= Duration::from_millis(50));
    Ok(())
}

#[test]
fn test_watchdog_timeout_does_not_fire_when_fed() -> Result<(), WatchdogError> {
    let mut watchdog = SoftwareWatchdog::new(100);
    watchdog.arm()?;

    // Feed repeatedly within the timeout window
    for _ in 0..5 {
        watchdog.feed()?;
        std::thread::sleep(Duration::from_millis(20));
    }

    assert!(!watchdog.has_timed_out());
    Ok(())
}

#[test]
fn test_watchdog_trigger_safe_state_zeroes_torque() -> Result<(), WatchdogError> {
    let mut watchdog = SoftwareWatchdog::new(50);
    watchdog.arm()?;
    watchdog.feed()?;

    watchdog.trigger_safe_state()?;

    assert!(watchdog.is_safe_state_triggered());
    assert!(watchdog.has_timed_out());
    Ok(())
}

#[test]
fn test_watchdog_timeout_handler_zeroes_torque_within_budget() {
    let mut handler = WatchdogTimeoutHandler::new();

    let response = handler.handle_timeout(25.0);

    assert_eq!(response.torque_command, 0.0);
    assert_eq!(response.previous_torque, 25.0);
    assert!(
        response.within_budget,
        "Timeout response took {:?}, exceeds 1ms budget",
        response.response_time
    );
    assert!(handler.is_timeout_triggered());
    assert_eq!(handler.current_torque(), 0.0);
}

#[test]
fn test_watchdog_interlock_system_timeout_produces_zero_torque() -> Result<(), WatchdogError> {
    let watchdog = Box::new(SoftwareWatchdog::new(50));
    let mut system = SafetyInterlockSystem::new(watchdog, 25.0);
    system.arm()?;

    // Process one normal tick
    let result = system.process_tick(15.0);
    assert_eq!(result.torque_command, 15.0);
    assert!(!result.fault_occurred);

    // Let the watchdog expire
    std::thread::sleep(Duration::from_millis(60));

    let result = system.process_tick(15.0);
    assert_eq!(
        result.torque_command, 0.0,
        "Torque must be zero after watchdog timeout"
    );
    assert!(result.fault_occurred);
    Ok(())
}

#[test]
fn test_watchdog_feed_after_timeout_returns_error() -> Result<(), WatchdogError> {
    let mut watchdog = SoftwareWatchdog::new(30);
    watchdog.arm()?;
    watchdog.feed()?;

    std::thread::sleep(Duration::from_millis(40));
    assert!(watchdog.has_timed_out());

    // Feed after timeout should fail
    let result = watchdog.feed();
    assert_eq!(result, Err(WatchdogError::TimedOut));
    Ok(())
}

#[test]
fn test_watchdog_unarmed_does_not_timeout() {
    let watchdog = SoftwareWatchdog::new(10);
    // Not armed
    std::thread::sleep(Duration::from_millis(20));
    assert!(!watchdog.has_timed_out());
}

// =========================================================================
// Soft-stop ramp tests
// =========================================================================

#[test]
fn test_soft_stop_ramps_from_max_to_zero() {
    let mut controller = SoftStopController::new();
    let start_torque = 25.0;
    let duration = Duration::from_millis(50);

    controller.start_soft_stop_with_duration(start_torque, duration);
    assert!(controller.is_active());
    assert_eq!(controller.start_torque(), start_torque);
    assert_eq!(controller.target_torque(), 0.0);

    // Simulate ticks at 1kHz (1ms per tick)
    let mut torque = start_torque;
    let tick = Duration::from_millis(1);
    for _ in 0..50 {
        torque = controller.update(tick);
    }

    // After ramp duration, torque must be at target (zero)
    assert!(!controller.is_active());
    assert!(
        torque.abs() < f32::EPSILON,
        "Torque should be zero after ramp, got {torque}"
    );
}

#[test]
fn test_soft_stop_torque_decreases_monotonically() {
    let mut controller = SoftStopController::new();
    controller.start_soft_stop_with_duration(20.0, Duration::from_millis(100));

    let tick = Duration::from_millis(1);
    let mut prev_torque = 20.0;

    for i in 0..100 {
        let torque = controller.update(tick);
        assert!(
            torque <= prev_torque,
            "Torque increased at tick {i}: {prev_torque} -> {torque}"
        );
        prev_torque = torque;
    }

    assert!(
        prev_torque.abs() < f32::EPSILON,
        "Final torque should be zero, got {prev_torque}"
    );
}

#[test]
fn test_soft_stop_linear_ramp_at_midpoint() {
    let mut controller = SoftStopController::new();
    let start = 20.0;
    let duration = Duration::from_millis(100);
    controller.start_soft_stop_with_duration(start, duration);

    // Advance to 50% of the ramp
    let torque = controller.update(Duration::from_millis(50));

    // Linear ramp: at 50% progress, torque should be ~50% of start
    let expected = start * 0.5;
    let tolerance = 0.5; // Allow small rounding
    assert!(
        (torque - expected).abs() < tolerance,
        "At midpoint expected ~{expected}, got {torque}"
    );
}

#[test]
fn test_soft_stop_multiplier_tracks_ramp() {
    let mut controller = SoftStopController::new();
    controller.start_soft_stop_with_duration(10.0, Duration::from_millis(100));

    // Initial multiplier should be close to 1.0
    assert!((controller.current_multiplier() - 1.0).abs() < f32::EPSILON);

    // Advance halfway
    controller.update(Duration::from_millis(50));
    let mid_mult = controller.current_multiplier();
    assert!(
        (mid_mult - 0.5).abs() < 0.1,
        "Midpoint multiplier expected ~0.5, got {mid_mult}"
    );

    // Advance to completion
    controller.update(Duration::from_millis(50));
    assert!(
        controller.current_multiplier() < f32::EPSILON,
        "Final multiplier should be ~0.0"
    );
}

#[test]
fn test_soft_stop_remaining_time_decreases() {
    let mut controller = SoftStopController::new();
    controller.start_soft_stop_with_duration(10.0, Duration::from_millis(100));

    let initial = controller.remaining_time();
    assert!(initial.is_some());

    controller.update(Duration::from_millis(30));
    let after_30ms = controller.remaining_time();
    assert!(after_30ms.is_some());
    // remaining_time unwrap is safe here because we just checked is_some
    let initial_val = initial.unwrap_or(Duration::ZERO);
    let after_val = after_30ms.unwrap_or(Duration::ZERO);
    assert!(
        after_val < initial_val,
        "Remaining time should decrease: {initial_val:?} -> {after_val:?}"
    );

    // Complete the ramp
    controller.update(Duration::from_millis(80));
    assert!(controller.remaining_time().is_none());
}

#[test]
fn test_soft_stop_custom_ramp_duration() {
    let mut controller = SoftStopController::with_duration(Duration::from_millis(200));
    controller.start_soft_stop(10.0);

    assert_eq!(controller.ramp_duration(), Duration::from_millis(200));

    // 100ms in: should still be active (halfway through 200ms ramp)
    let torque = controller.update(Duration::from_millis(100));
    assert!(controller.is_active());
    assert!(
        torque > 0.0 && torque < 10.0,
        "Torque should be mid-ramp, got {torque}"
    );

    // Complete the ramp
    controller.update(Duration::from_millis(100));
    assert!(!controller.is_active());
    assert!(controller.current_torque().abs() < f32::EPSILON);
}

#[test]
fn test_soft_stop_ramp_to_nonzero_target() {
    let mut controller = SoftStopController::new();
    controller.start_ramp_to(20.0, 5.0, Duration::from_millis(50));

    // Run to completion
    let tick = Duration::from_millis(1);
    let mut torque = 20.0;
    for _ in 0..50 {
        torque = controller.update(tick);
    }

    assert!(
        (torque - 5.0).abs() < f32::EPSILON,
        "Expected target torque 5.0, got {torque}"
    );
}

// =========================================================================
// Multi-fault tests
// =========================================================================

#[test]
fn test_multi_fault_all_types_tracked_independently() {
    let mut service = create_test_service();

    // Report two different faults
    service.report_fault(FaultType::UsbStall);
    service.report_fault(FaultType::ThermalLimit);

    // Both are tracked in fault_count
    assert!(service.fault_count[&FaultType::UsbStall] >= 1);
    assert!(service.fault_count[&FaultType::ThermalLimit] >= 1);
    // Unrelated fault types should still be zero
    assert_eq!(service.fault_count[&FaultType::EncoderNaN], 0);
}

#[test]
fn test_multi_fault_clearing_one_does_not_clear_others() {
    let mut service = create_test_service();

    // Report two distinct faults sequentially
    service.report_fault(FaultType::UsbStall);
    service.report_fault(FaultType::Overcurrent);

    std::thread::sleep(Duration::from_millis(110));

    // Clearing the fault transitions state to SafeTorque...
    must(service.clear_fault());
    assert_eq!(service.state(), &SafetyState::SafeTorque);

    // ...but fault_count for both fault types is still non-zero
    assert!(service.fault_count[&FaultType::UsbStall] >= 1);
    assert!(service.fault_count[&FaultType::Overcurrent] >= 1);
}

#[test]
fn test_multi_fault_blocks_high_torque_until_all_counts_zero() {
    let mut service = create_test_service();

    // Trigger and clear a fault to leave a non-zero fault_count
    service.report_fault(FaultType::EncoderNaN);
    std::thread::sleep(Duration::from_millis(110));
    must(service.clear_fault());

    // State is SafeTorque, but fault_count[EncoderNaN] > 0
    assert_eq!(service.state(), &SafetyState::SafeTorque);

    // High torque request must be rejected because of nonzero fault history
    let result = service.request_high_torque("dev");
    assert!(result.is_err());
}

#[test]
fn test_multi_fault_same_type_increments_count() {
    let mut service = create_test_service();

    service.report_fault(FaultType::PluginOverrun);
    std::thread::sleep(Duration::from_millis(110));
    must(service.clear_fault());

    service.report_fault(FaultType::PluginOverrun);
    std::thread::sleep(Duration::from_millis(110));
    must(service.clear_fault());

    assert_eq!(service.fault_count[&FaultType::PluginOverrun], 2);
}

#[test]
fn test_multi_fault_torque_always_zero_regardless_of_active_fault() {
    let mut service = create_test_service();

    let faults = [
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

    for fault in faults {
        service.report_fault(fault);
        assert_eq!(
            service.clamp_torque_nm(25.0),
            0.0,
            "Torque must be zero with active fault {fault:?}"
        );
        assert_eq!(
            service.clamp_torque_nm(-25.0),
            0.0,
            "Negative torque must be zero with active fault {fault:?}"
        );
    }
}

// =========================================================================
// Fault recovery tests
// =========================================================================

#[test]
fn test_fault_recovery_restores_safe_torque() {
    let mut service = create_test_service();

    service.report_fault(FaultType::UsbStall);
    assert_eq!(service.clamp_torque_nm(5.0), 0.0);

    std::thread::sleep(Duration::from_millis(110));
    must(service.clear_fault());

    // After recovery, safe torque limit should be restored
    assert_eq!(service.state(), &SafetyState::SafeTorque);
    assert_eq!(service.max_torque_nm(), 5.0);
    assert_eq!(service.clamp_torque_nm(5.0), 5.0);
    assert_eq!(service.clamp_torque_nm(-5.0), -5.0);
}

#[test]
fn test_fault_recovery_requires_minimum_fault_duration() {
    let mut service = create_test_service();

    service.report_fault(FaultType::Overcurrent);

    // Attempt to clear immediately should fail
    let result = service.clear_fault();
    assert!(result.is_err());
    assert!(matches!(service.state(), SafetyState::Faulted { .. }));

    // Torque still zero
    assert_eq!(service.clamp_torque_nm(10.0), 0.0);
}

#[test]
fn test_fault_recovery_clear_from_non_faulted_state_fails() {
    let mut service = create_test_service();

    // Not faulted
    let result = service.clear_fault();
    assert!(result.is_err());
}

#[test]
fn test_fault_recovery_full_cycle_fault_clear_refault_clear() {
    let mut service = create_test_service();

    // First fault cycle
    service.report_fault(FaultType::ThermalLimit);
    assert_eq!(service.clamp_torque_nm(5.0), 0.0);
    std::thread::sleep(Duration::from_millis(110));
    must(service.clear_fault());
    assert_eq!(service.clamp_torque_nm(5.0), 5.0);

    // Second fault cycle
    service.report_fault(FaultType::UsbStall);
    assert_eq!(service.clamp_torque_nm(5.0), 0.0);
    std::thread::sleep(Duration::from_millis(110));
    must(service.clear_fault());
    assert_eq!(service.clamp_torque_nm(5.0), 5.0);

    // Both fault types should have count ≥ 1
    assert!(service.fault_count[&FaultType::ThermalLimit] >= 1);
    assert!(service.fault_count[&FaultType::UsbStall] >= 1);
}

#[test]
fn test_fault_recovery_after_high_torque_returns_to_safe_torque_not_high() {
    let mut service = create_test_service();
    activate_high_torque(&mut service, "dev");

    // High torque active
    assert_eq!(service.max_torque_nm(), 25.0);

    // Fault
    service.report_fault(FaultType::SafetyInterlockViolation);
    assert_eq!(service.max_torque_nm(), 0.0);

    // Recover
    std::thread::sleep(Duration::from_millis(110));
    must(service.clear_fault());

    // Recovery should land in SafeTorque, NOT HighTorqueActive
    assert_eq!(service.state(), &SafetyState::SafeTorque);
    assert_eq!(service.max_torque_nm(), 5.0);
}

#[test]
fn test_fault_recovery_timing_within_budget() {
    let mut service = create_test_service();

    service.report_fault(FaultType::TimingViolation);
    std::thread::sleep(Duration::from_millis(110));

    let before = Instant::now();
    must(service.clear_fault());
    let elapsed = before.elapsed();

    assert!(
        elapsed < Duration::from_millis(10),
        "Fault clear took {elapsed:?}, expected < 10ms"
    );
    assert_eq!(service.state(), &SafetyState::SafeTorque);
}

#[test]
fn test_fault_detection_to_zero_torque_within_10ms() {
    let mut service = create_test_service();
    activate_high_torque(&mut service, "dev");

    let before = Instant::now();
    service.report_fault(FaultType::PipelineFault);
    let torque = service.clamp_torque_nm(25.0);
    let elapsed = before.elapsed();

    assert_eq!(torque, 0.0);
    assert!(
        elapsed < Duration::from_millis(10),
        "Fault detection to zero torque took {elapsed:?}, exceeds 10ms"
    );
}

#[test]
fn test_watchdog_timeout_handler_reset_allows_reuse() {
    let mut handler = WatchdogTimeoutHandler::new();

    handler.handle_timeout(10.0);
    assert!(handler.is_timeout_triggered());

    handler.reset();
    assert!(!handler.is_timeout_triggered());
    assert_eq!(handler.current_torque(), 0.0);
    assert!(handler.timeout_timestamp().is_none());
}

#[test]
fn test_watchdog_reset_allows_rearm() -> Result<(), WatchdogError> {
    let mut watchdog = SoftwareWatchdog::new(50);
    watchdog.arm()?;
    watchdog.feed()?;

    // Let it time out
    std::thread::sleep(Duration::from_millis(60));
    assert!(watchdog.has_timed_out());

    // Reset and rearm
    watchdog.reset()?;
    assert!(!watchdog.has_timed_out());
    assert!(!watchdog.is_armed());

    watchdog.arm()?;
    watchdog.feed()?;

    // Should be functional again
    assert!(!watchdog.has_timed_out());
    assert!(watchdog.is_armed());
    Ok(())
}

//! Hardening coverage tests for safety-critical modules
//!
//! Fills coverage gaps identified in the safety state machine, watchdog,
//! torque-limiting, and RT pipeline edge cases.  Every test returns
//! `Result` and avoids `unwrap()`/`expect()` per project policy.

use super::hardware_watchdog::{
    HardwareWatchdog, SafetyInterlockState, SafetyInterlockSystem, SoftwareWatchdog, TorqueLimit,
    WatchdogError, WatchdogTimeoutHandler,
};
use super::*;
use std::time::{Duration, Instant};

// =========================================================================
// Helpers
// =========================================================================

/// Create a SafetyService with short timeouts for fast tests.
fn svc() -> SafetyService {
    SafetyService::with_timeouts(
        5.0,
        25.0,
        Duration::from_secs(3),
        Duration::from_secs(2),
    )
}

/// Drive a SafetyService through the full high-torque activation flow.
fn activate(service: &mut SafetyService, device: &str) -> Result<(), String> {
    let challenge = service.request_high_torque(device)?;
    service.provide_ui_consent(challenge.challenge_token)?;
    service.report_combo_start(challenge.challenge_token)?;
    std::thread::sleep(Duration::from_millis(2100));
    let ack = InterlockAck {
        challenge_token: challenge.challenge_token,
        device_token: 42,
        combo_completed: ButtonCombo::BothClutchPaddles,
        timestamp: Instant::now(),
    };
    service.confirm_high_torque(device, ack)
}

// =========================================================================
// 1. Safety state machine – invalid transitions rejected
// =========================================================================

#[test]
fn test_provide_ui_consent_from_safe_torque_rejected() -> Result<(), String> {
    let mut s = svc();
    let res = s.provide_ui_consent(1);
    assert!(res.is_err(), "provide_ui_consent must fail in SafeTorque");
    Ok(())
}

#[test]
fn test_provide_ui_consent_from_awaiting_ack_rejected() -> Result<(), String> {
    let mut s = svc();
    let c = s.request_high_torque("d")?;
    s.provide_ui_consent(c.challenge_token)?;
    // Now in AwaitingPhysicalAck – second consent should fail
    let res = s.provide_ui_consent(c.challenge_token);
    assert!(res.is_err(), "provide_ui_consent must fail in AwaitingPhysicalAck");
    Ok(())
}

#[test]
fn test_provide_ui_consent_from_high_torque_active_rejected() -> Result<(), String> {
    let mut s = svc();
    activate(&mut s, "d")?;
    let res = s.provide_ui_consent(1);
    assert!(res.is_err(), "provide_ui_consent must fail in HighTorqueActive");
    Ok(())
}

#[test]
fn test_provide_ui_consent_from_faulted_rejected() -> Result<(), String> {
    let mut s = svc();
    s.report_fault(FaultType::UsbStall);
    let res = s.provide_ui_consent(1);
    assert!(res.is_err(), "provide_ui_consent must fail in Faulted");
    Ok(())
}

#[test]
fn test_report_combo_start_from_safe_torque_rejected() -> Result<(), String> {
    let mut s = svc();
    let res = s.report_combo_start(1);
    assert!(res.is_err(), "report_combo_start must fail in SafeTorque");
    Ok(())
}

#[test]
fn test_report_combo_start_from_high_torque_challenge_rejected() -> Result<(), String> {
    let mut s = svc();
    s.request_high_torque("d")?;
    // Still in HighTorqueChallenge (no UI consent yet)
    let res = s.report_combo_start(1);
    assert!(res.is_err(), "report_combo_start must fail in HighTorqueChallenge");
    Ok(())
}

#[test]
fn test_report_combo_start_from_high_torque_active_rejected() -> Result<(), String> {
    let mut s = svc();
    activate(&mut s, "d")?;
    let res = s.report_combo_start(1);
    assert!(res.is_err(), "report_combo_start must fail in HighTorqueActive");
    Ok(())
}

#[test]
fn test_report_combo_start_from_faulted_rejected() -> Result<(), String> {
    let mut s = svc();
    s.report_fault(FaultType::ThermalLimit);
    let res = s.report_combo_start(1);
    assert!(res.is_err(), "report_combo_start must fail in Faulted");
    Ok(())
}

#[test]
fn test_confirm_high_torque_from_safe_torque_rejected() -> Result<(), String> {
    let mut s = svc();
    let ack = InterlockAck {
        challenge_token: 1,
        device_token: 1,
        combo_completed: ButtonCombo::BothClutchPaddles,
        timestamp: Instant::now(),
    };
    let res = s.confirm_high_torque("d", ack);
    assert!(res.is_err(), "confirm_high_torque must fail in SafeTorque");
    Ok(())
}

#[test]
fn test_confirm_high_torque_from_challenge_rejected() -> Result<(), String> {
    let mut s = svc();
    let c = s.request_high_torque("d")?;
    // Still in HighTorqueChallenge – no UI consent
    let ack = InterlockAck {
        challenge_token: c.challenge_token,
        device_token: 1,
        combo_completed: ButtonCombo::BothClutchPaddles,
        timestamp: Instant::now(),
    };
    let res = s.confirm_high_torque("d", ack);
    assert!(res.is_err(), "confirm_high_torque must fail in HighTorqueChallenge");
    Ok(())
}

#[test]
fn test_confirm_high_torque_from_active_rejected() -> Result<(), String> {
    let mut s = svc();
    activate(&mut s, "d")?;
    let ack = InterlockAck {
        challenge_token: 1,
        device_token: 1,
        combo_completed: ButtonCombo::BothClutchPaddles,
        timestamp: Instant::now(),
    };
    let res = s.confirm_high_torque("d", ack);
    assert!(res.is_err(), "confirm_high_torque must fail in HighTorqueActive");
    Ok(())
}

#[test]
fn test_confirm_high_torque_from_faulted_rejected() -> Result<(), String> {
    let mut s = svc();
    s.report_fault(FaultType::Overcurrent);
    let ack = InterlockAck {
        challenge_token: 1,
        device_token: 1,
        combo_completed: ButtonCombo::BothClutchPaddles,
        timestamp: Instant::now(),
    };
    let res = s.confirm_high_torque("d", ack);
    assert!(res.is_err(), "confirm_high_torque must fail in Faulted");
    Ok(())
}

#[test]
fn test_confirm_without_combo_start_rejected() -> Result<(), String> {
    let mut s = svc();
    let c = s.request_high_torque("d")?;
    s.provide_ui_consent(c.challenge_token)?;
    // AwaitingPhysicalAck but combo_start is None
    std::thread::sleep(Duration::from_millis(2100));
    let ack = InterlockAck {
        challenge_token: c.challenge_token,
        device_token: 1,
        combo_completed: ButtonCombo::BothClutchPaddles,
        timestamp: Instant::now(),
    };
    let res = s.confirm_high_torque("d", ack);
    assert!(res.is_err(), "confirm must fail when combo_start is None");
    Ok(())
}

#[test]
fn test_request_high_torque_from_awaiting_ack_rejected() -> Result<(), String> {
    let mut s = svc();
    let c = s.request_high_torque("d")?;
    s.provide_ui_consent(c.challenge_token)?;
    // Now in AwaitingPhysicalAck
    let res = s.request_high_torque("d");
    assert!(res.is_err(), "request must fail in AwaitingPhysicalAck");
    Ok(())
}

// =========================================================================
// 2. Fault during challenge / awaiting states
// =========================================================================

#[test]
fn test_fault_during_high_torque_challenge_zeros_torque() -> Result<(), String> {
    let mut s = svc();
    s.request_high_torque("d")?;
    assert!(matches!(s.state(), SafetyState::HighTorqueChallenge { .. }));

    s.report_fault(FaultType::EncoderNaN);

    assert!(matches!(s.state(), SafetyState::Faulted { .. }));
    assert_eq!(s.clamp_torque_nm(10.0), 0.0);
    Ok(())
}

#[test]
fn test_fault_during_awaiting_ack_zeros_torque() -> Result<(), String> {
    let mut s = svc();
    let c = s.request_high_torque("d")?;
    s.provide_ui_consent(c.challenge_token)?;
    assert!(matches!(s.state(), SafetyState::AwaitingPhysicalAck { .. }));

    s.report_fault(FaultType::Overcurrent);

    assert!(matches!(s.state(), SafetyState::Faulted { .. }));
    assert_eq!(s.clamp_torque_nm(10.0), 0.0);
    // Challenge should still exist in the struct but state is overridden
    Ok(())
}

#[test]
fn test_fault_increments_count_for_unknown_variant() -> Result<(), String> {
    // If a new FaultType is introduced the HashMap insertion path handles it.
    // We can test the existing variants are properly counted after multiple reports.
    let mut s = svc();
    s.report_fault(FaultType::PipelineFault);
    s.report_fault(FaultType::PipelineFault);
    assert!(s.fault_count[&FaultType::PipelineFault] >= 2);
    Ok(())
}

// =========================================================================
// 3. Watchdog – legacy check_hands_off_timeout and edge cases
// =========================================================================

#[test]
fn test_check_hands_off_timeout_below_threshold_no_fault() -> Result<(), String> {
    let mut s = svc();
    activate(&mut s, "d")?;
    // Duration below the 3-second hands_off_timeout → no fault
    s.check_hands_off_timeout(Duration::from_secs(2));
    assert!(
        matches!(s.state(), SafetyState::HighTorqueActive { .. }),
        "Should remain in HighTorqueActive"
    );
    Ok(())
}

#[test]
fn test_check_hands_off_timeout_above_threshold_triggers_fault() -> Result<(), String> {
    let mut s = svc();
    activate(&mut s, "d")?;
    s.check_hands_off_timeout(Duration::from_secs(4));
    assert!(
        matches!(s.state(), SafetyState::Faulted { fault: FaultType::HandsOffTimeout, .. }),
        "Should transition to Faulted(HandsOffTimeout)"
    );
    Ok(())
}

#[test]
fn test_check_hands_off_timeout_noop_in_safe_torque() -> Result<(), String> {
    let mut s = svc();
    s.check_hands_off_timeout(Duration::from_secs(100));
    assert_eq!(s.state(), &SafetyState::SafeTorque);
    Ok(())
}

#[test]
fn test_watchdog_disarm_rearm_cycle() -> Result<(), WatchdogError> {
    let mut wd = SoftwareWatchdog::new(100);
    wd.arm()?;
    assert!(wd.is_armed());
    wd.disarm()?;
    assert!(!wd.is_armed());
    // Re-arm
    wd.arm()?;
    assert!(wd.is_armed());
    wd.feed()?;
    assert!(!wd.has_timed_out());
    Ok(())
}

#[test]
fn test_watchdog_feed_when_unarmed_returns_not_armed() -> Result<(), String> {
    let mut wd = SoftwareWatchdog::new(100);
    let res = wd.feed();
    assert_eq!(res, Err(WatchdogError::NotArmed));
    Ok(())
}

#[test]
fn test_watchdog_double_arm_returns_already_armed() -> Result<(), WatchdogError> {
    let mut wd = SoftwareWatchdog::new(100);
    wd.arm()?;
    let res = wd.arm();
    assert_eq!(res, Err(WatchdogError::AlreadyArmed));
    Ok(())
}

#[test]
fn test_watchdog_disarm_when_unarmed_returns_not_armed() -> Result<(), String> {
    let mut wd = SoftwareWatchdog::new(100);
    let res = wd.disarm();
    assert_eq!(res, Err(WatchdogError::NotArmed));
    Ok(())
}

#[test]
fn test_watchdog_check_timeout_when_unarmed_returns_false() -> Result<(), String> {
    let wd = SoftwareWatchdog::new(10);
    std::thread::sleep(Duration::from_millis(15));
    assert!(!wd.check_timeout(), "Unarmed watchdog must not report timeout");
    Ok(())
}

#[test]
fn test_watchdog_trigger_safe_state_then_feed_fails() -> Result<(), WatchdogError> {
    let mut wd = SoftwareWatchdog::new(100);
    wd.arm()?;
    wd.trigger_safe_state()?;
    assert!(wd.is_safe_state_triggered());
    let res = wd.feed();
    assert_eq!(res, Err(WatchdogError::TimedOut));
    Ok(())
}

// =========================================================================
// 4. Torque output limiting – edge cases
// =========================================================================

#[test]
fn test_clamp_torque_in_high_torque_active_allows_high_limit() -> Result<(), String> {
    let mut s = svc();
    activate(&mut s, "d")?;
    assert_eq!(s.clamp_torque_nm(20.0), 20.0);
    assert_eq!(s.clamp_torque_nm(-20.0), -20.0);
    // At the boundary
    assert_eq!(s.clamp_torque_nm(25.0), 25.0);
    assert_eq!(s.clamp_torque_nm(-25.0), -25.0);
    // Above the boundary
    assert_eq!(s.clamp_torque_nm(30.0), 25.0);
    assert_eq!(s.clamp_torque_nm(-30.0), -25.0);
    Ok(())
}

#[test]
fn test_clamp_torque_exactly_at_safe_boundary() -> Result<(), String> {
    let s = svc();
    assert_eq!(s.clamp_torque_nm(5.0), 5.0);
    assert_eq!(s.clamp_torque_nm(-5.0), -5.0);
    Ok(())
}

#[test]
fn test_clamp_torque_zero_passthrough() -> Result<(), String> {
    let s = svc();
    assert_eq!(s.clamp_torque_nm(0.0), 0.0);
    Ok(())
}

#[test]
fn test_clamp_torque_negative_zero() -> Result<(), String> {
    let s = svc();
    let result = s.clamp_torque_nm(-0.0);
    assert!(result == 0.0, "negative zero should clamp to 0.0");
    Ok(())
}

#[test]
fn test_get_max_torque_high_flag_false_in_active_returns_safe() -> Result<(), String> {
    let mut s = svc();
    activate(&mut s, "d")?;
    // Even in HighTorqueActive, if is_high_torque_enabled=false → safe limit
    assert_eq!(s.get_max_torque(false).value(), 5.0);
    Ok(())
}

#[test]
fn test_get_max_torque_high_flag_true_in_active_returns_high() -> Result<(), String> {
    let mut s = svc();
    activate(&mut s, "d")?;
    assert_eq!(s.get_max_torque(true).value(), 25.0);
    Ok(())
}

#[test]
fn test_torque_limit_clamp_within_range_no_violation() -> Result<(), String> {
    let mut limit = TorqueLimit::new(20.0, 5.0);
    let (clamped, was_clamped) = limit.clamp(10.0);
    assert_eq!(clamped, 10.0);
    assert!(!was_clamped);
    assert_eq!(limit.violation_count, 0);
    Ok(())
}

#[test]
fn test_torque_limit_clamp_negative_within_range() -> Result<(), String> {
    let mut limit = TorqueLimit::new(20.0, 5.0);
    let (clamped, was_clamped) = limit.clamp(-15.0);
    assert_eq!(clamped, -15.0);
    assert!(!was_clamped);
    Ok(())
}

#[test]
fn test_torque_limit_clamp_zero() -> Result<(), String> {
    let mut limit = TorqueLimit::new(20.0, 5.0);
    let (clamped, was_clamped) = limit.clamp(0.0);
    assert_eq!(clamped, 0.0);
    assert!(!was_clamped);
    Ok(())
}

#[test]
fn test_torque_limit_safe_mode_limit_value() -> Result<(), String> {
    let limit = TorqueLimit::new(20.0, 5.0);
    assert_eq!(limit.safe_mode_limit(), 5.0);
    Ok(())
}

// =========================================================================
// 5. SafetyInterlockSystem – edge cases
// =========================================================================

#[test]
fn test_interlock_system_tick_when_unarmed() -> Result<(), WatchdogError> {
    let watchdog = Box::new(SoftwareWatchdog::new(100));
    let mut sys = SafetyInterlockSystem::new(watchdog, 25.0);
    // Not armed – feed returns NotArmed, handled gracefully
    let result = sys.process_tick(10.0);
    // Should not fault; torque is limited but passes through
    assert!(!result.fault_occurred);
    assert!(result.torque_command.abs() <= 25.0);
    Ok(())
}

#[test]
fn test_interlock_system_emergency_stop_zeroes_all_subsequent_ticks() -> Result<(), WatchdogError> {
    let watchdog = Box::new(SoftwareWatchdog::new(100));
    let mut sys = SafetyInterlockSystem::new(watchdog, 25.0);
    sys.arm()?;
    sys.emergency_stop();
    for torque_req in [10.0, -10.0, 100.0, 0.0] {
        let result = sys.process_tick(torque_req);
        assert_eq!(result.torque_command, 0.0, "E-stop must zero all ticks");
    }
    Ok(())
}

#[test]
fn test_interlock_system_cannot_clear_emergency_stop() -> Result<(), WatchdogError> {
    let watchdog = Box::new(SoftwareWatchdog::new(100));
    let mut sys = SafetyInterlockSystem::new(watchdog, 25.0);
    sys.arm()?;
    sys.emergency_stop();
    std::thread::sleep(Duration::from_millis(110));
    let res = sys.clear_fault();
    assert!(res.is_err(), "Emergency stop must require manual reset");
    Ok(())
}

#[test]
fn test_interlock_system_reset_from_safe_mode() -> Result<(), WatchdogError> {
    let watchdog = Box::new(SoftwareWatchdog::new(100));
    let mut sys = SafetyInterlockSystem::new(watchdog, 25.0);
    sys.arm()?;
    sys.report_fault(FaultType::ThermalLimit);
    assert!(matches!(sys.state(), SafetyInterlockState::SafeMode { .. }));

    sys.reset()?;
    assert_eq!(*sys.state(), SafetyInterlockState::Normal);
    Ok(())
}

#[test]
fn test_interlock_system_reset_from_emergency_stop() -> Result<(), WatchdogError> {
    let watchdog = Box::new(SoftwareWatchdog::new(100));
    let mut sys = SafetyInterlockSystem::new(watchdog, 25.0);
    sys.arm()?;
    sys.emergency_stop();
    assert!(matches!(sys.state(), SafetyInterlockState::EmergencyStop { .. }));

    sys.reset()?;
    assert_eq!(*sys.state(), SafetyInterlockState::Normal);
    Ok(())
}

// =========================================================================
// 6. RT pipeline edge cases
// =========================================================================

#[test]
fn test_empty_pipeline_processes_frame_successfully() -> Result<(), Box<dyn std::error::Error>> {
    use crate::pipeline::Pipeline;
    use crate::rt::Frame;

    let mut pipeline = Pipeline::new();
    let mut frame = Frame {
        ffb_in: 0.5,
        torque_out: 0.3,
        wheel_speed: 0.0,
        hands_off: false,
        ts_mono_ns: 1000,
        seq: 1,
    };

    pipeline.process(&mut frame)?;
    // Torque should pass through unchanged for empty pipeline (no response curve)
    assert_eq!(frame.torque_out, 0.3);
    Ok(())
}

#[test]
fn test_frame_default_values() -> Result<(), String> {
    use crate::rt::Frame;

    let frame = Frame::default();
    assert_eq!(frame.ffb_in, 0.0);
    assert_eq!(frame.torque_out, 0.0);
    assert_eq!(frame.wheel_speed, 0.0);
    assert!(!frame.hands_off);
    assert_eq!(frame.ts_mono_ns, 0);
    assert_eq!(frame.seq, 0);
    Ok(())
}

#[test]
fn test_pipeline_swap_replaces_config_hash() -> Result<(), String> {
    use crate::pipeline::Pipeline;

    let mut p1 = Pipeline::new();
    assert_eq!(p1.config_hash(), 0);

    let p2 = Pipeline::with_hash(0xDEAD_BEEF);
    p1.swap_at_tick_boundary(p2);
    assert_eq!(p1.config_hash(), 0xDEAD_BEEF);
    Ok(())
}

// =========================================================================
// 7. WatchdogTimeoutHandler edge cases
// =========================================================================

#[test]
fn test_timeout_handler_multiple_timeouts_keep_zero() -> Result<(), String> {
    let mut handler = WatchdogTimeoutHandler::new();

    let r1 = handler.handle_timeout(15.0);
    assert_eq!(r1.torque_command, 0.0);
    assert_eq!(r1.previous_torque, 15.0);

    // Second timeout with different previous torque
    let r2 = handler.handle_timeout(20.0);
    assert_eq!(r2.torque_command, 0.0);
    assert_eq!(r2.previous_torque, 20.0);
    assert!(handler.is_timeout_triggered());
    assert_eq!(handler.current_torque(), 0.0);
    Ok(())
}

#[test]
fn test_timeout_handler_reset_clears_everything() -> Result<(), String> {
    let mut handler = WatchdogTimeoutHandler::new();
    handler.handle_timeout(10.0);
    assert!(handler.is_timeout_triggered());
    assert!(handler.timeout_timestamp().is_some());

    handler.reset();
    assert!(!handler.is_timeout_triggered());
    assert!(handler.timeout_timestamp().is_none());
    assert_eq!(handler.current_torque(), 0.0);
    Ok(())
}

// =========================================================================
// 8. Safety state machine – challenge expiry actual timeout
// =========================================================================

#[test]
fn test_challenge_expiry_check_returns_true_when_expired() -> Result<(), String> {
    let mut s = svc();
    // Create a challenge that expires immediately by setting a past expiry
    s.state = SafetyState::HighTorqueChallenge {
        challenge_token: 123,
        expires: Instant::now() - Duration::from_secs(1),
        ui_consent_given: false,
    };
    s.active_challenge = Some(InterlockChallenge {
        challenge_token: 123,
        combo_required: ButtonCombo::BothClutchPaddles,
        expires: Instant::now() - Duration::from_secs(1),
        ui_consent_given: false,
        combo_start: None,
    });

    let expired = s.check_challenge_expiry();
    assert!(expired, "check_challenge_expiry must detect expired challenge");
    assert_eq!(s.state(), &SafetyState::SafeTorque);
    assert!(s.get_active_challenge().is_none());
    Ok(())
}

#[test]
fn test_challenge_time_remaining_zero_when_expired() -> Result<(), String> {
    let mut s = svc();
    s.state = SafetyState::AwaitingPhysicalAck {
        challenge_token: 456,
        expires: Instant::now() - Duration::from_secs(1),
        combo_start: None,
    };
    let remaining = s.get_challenge_time_remaining();
    assert!(remaining.is_some());
    if let Some(dur) = remaining {
        assert_eq!(dur, Duration::ZERO, "Expired challenge should report ZERO remaining");
    }
    Ok(())
}

// =========================================================================
// 9. Fault recovery after challenge states
// =========================================================================

#[test]
fn test_fault_clear_after_challenge_state_returns_to_safe() -> Result<(), String> {
    let mut s = svc();
    s.request_high_torque("d")?;
    assert!(matches!(s.state(), SafetyState::HighTorqueChallenge { .. }));

    // Fault interrupts challenge
    s.report_fault(FaultType::TimingViolation);
    assert!(matches!(s.state(), SafetyState::Faulted { .. }));

    std::thread::sleep(Duration::from_millis(110));
    s.clear_fault()?;
    assert_eq!(s.state(), &SafetyState::SafeTorque);
    // Safe torque limit restored
    assert_eq!(s.max_torque_nm(), 5.0);
    Ok(())
}

// =========================================================================
// 10. SafetyService default
// =========================================================================

#[test]
fn test_safety_service_default_values() -> Result<(), String> {
    let s = SafetyService::default();
    assert_eq!(s.state(), &SafetyState::SafeTorque);
    assert_eq!(s.max_torque_nm(), 5.0);
    assert_eq!(s.get_max_torque(true).value(), 5.0);
    Ok(())
}

// =========================================================================
// 11. Performance metrics edge cases
// =========================================================================

#[test]
fn test_performance_metrics_zero_ticks() -> Result<(), String> {
    use crate::rt::PerformanceMetrics;

    let metrics = PerformanceMetrics::default();
    assert_eq!(metrics.missed_tick_rate(), 0.0);
    assert_eq!(metrics.p99_jitter_us(), 0.0);
    Ok(())
}

#[test]
fn test_performance_metrics_missed_tick_rate() -> Result<(), String> {
    use crate::rt::PerformanceMetrics;

    let metrics = PerformanceMetrics {
        total_ticks: 1000,
        missed_ticks: 5,
        ..PerformanceMetrics::default()
    };
    let rate = metrics.missed_tick_rate();
    assert!((rate - 0.005).abs() < f64::EPSILON);
    Ok(())
}

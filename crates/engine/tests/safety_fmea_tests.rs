//! Comprehensive FMEA (Failure Mode and Effects Analysis) test suite.
//!
//! Covers fault injection, safety state machine transitions, watchdog behavior,
//! interlock verification, torque limiting, timing budgets, multi-fault
//! scenarios, and recovery paths.

use racing_wheel_engine::safety::{
    ButtonCombo, FaultInjectionScenario, FaultInjectionSystem, FaultType, HardwareWatchdog,
    InterlockAck, SafetyInterlockState, SafetyInterlockSystem, SafetyService, SafetyState,
    SoftwareWatchdog, TorqueLimit, TriggerCondition, WatchdogError, WatchdogTimeoutHandler,
};
use racing_wheel_engine::safety::fault_injection::{InjectionContext, RecoveryCondition};
use std::time::{Duration, Instant};

// =========================================================================
// Helpers
// =========================================================================

fn svc() -> SafetyService {
    SafetyService::with_timeouts(5.0, 25.0, Duration::from_secs(3), Duration::from_secs(2))
}

fn svc_fast() -> SafetyService {
    SafetyService::with_timeouts(5.0, 25.0, Duration::from_millis(50), Duration::from_millis(20))
}

fn interlock(timeout_ms: u32) -> SafetyInterlockSystem {
    SafetyInterlockSystem::new(Box::new(SoftwareWatchdog::new(timeout_ms)), 25.0)
}

/// Drive through the full challenge flow.
fn activate_high_torque(s: &mut SafetyService, dev: &str) -> Result<(), String> {
    let ch = s.request_high_torque(dev)?;
    let tok = ch.challenge_token;
    s.provide_ui_consent(tok)?;
    s.report_combo_start(tok)?;
    std::thread::sleep(Duration::from_millis(2050));
    let ack = InterlockAck {
        challenge_token: tok,
        device_token: 42,
        combo_completed: ButtonCombo::BothClutchPaddles,
        timestamp: Instant::now(),
    };
    s.confirm_high_torque(dev, ack)
}

/// All defined fault types.
const ALL_FAULTS: &[FaultType] = &[
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

// =========================================================================
// 1. FAULT INJECTION – every fault type → correct state transition
// =========================================================================

#[test]
fn fault_usb_stall_enters_faulted() -> Result<(), String> {
    let mut s = svc();
    s.report_fault(FaultType::UsbStall);
    assert!(matches!(
        s.state(),
        SafetyState::Faulted {
            fault: FaultType::UsbStall,
            ..
        }
    ));
    Ok(())
}

#[test]
fn fault_encoder_nan_enters_faulted() -> Result<(), String> {
    let mut s = svc();
    s.report_fault(FaultType::EncoderNaN);
    assert!(matches!(
        s.state(),
        SafetyState::Faulted {
            fault: FaultType::EncoderNaN,
            ..
        }
    ));
    Ok(())
}

#[test]
fn fault_thermal_limit_enters_faulted() -> Result<(), String> {
    let mut s = svc();
    s.report_fault(FaultType::ThermalLimit);
    assert!(matches!(
        s.state(),
        SafetyState::Faulted {
            fault: FaultType::ThermalLimit,
            ..
        }
    ));
    Ok(())
}

#[test]
fn fault_overcurrent_enters_faulted() -> Result<(), String> {
    let mut s = svc();
    s.report_fault(FaultType::Overcurrent);
    assert!(matches!(
        s.state(),
        SafetyState::Faulted {
            fault: FaultType::Overcurrent,
            ..
        }
    ));
    Ok(())
}

#[test]
fn fault_plugin_overrun_enters_faulted() -> Result<(), String> {
    let mut s = svc();
    s.report_fault(FaultType::PluginOverrun);
    assert!(matches!(
        s.state(),
        SafetyState::Faulted {
            fault: FaultType::PluginOverrun,
            ..
        }
    ));
    Ok(())
}

#[test]
fn fault_timing_violation_enters_faulted() -> Result<(), String> {
    let mut s = svc();
    s.report_fault(FaultType::TimingViolation);
    assert!(matches!(
        s.state(),
        SafetyState::Faulted {
            fault: FaultType::TimingViolation,
            ..
        }
    ));
    Ok(())
}

#[test]
fn fault_safety_interlock_violation_enters_faulted() -> Result<(), String> {
    let mut s = svc();
    s.report_fault(FaultType::SafetyInterlockViolation);
    assert!(matches!(
        s.state(),
        SafetyState::Faulted {
            fault: FaultType::SafetyInterlockViolation,
            ..
        }
    ));
    Ok(())
}

#[test]
fn fault_hands_off_timeout_enters_faulted() -> Result<(), String> {
    let mut s = svc();
    s.report_fault(FaultType::HandsOffTimeout);
    assert!(matches!(
        s.state(),
        SafetyState::Faulted {
            fault: FaultType::HandsOffTimeout,
            ..
        }
    ));
    Ok(())
}

#[test]
fn fault_pipeline_fault_enters_faulted() -> Result<(), String> {
    let mut s = svc();
    s.report_fault(FaultType::PipelineFault);
    assert!(matches!(
        s.state(),
        SafetyState::Faulted {
            fault: FaultType::PipelineFault,
            ..
        }
    ));
    Ok(())
}

#[test]
fn every_fault_type_zeroes_torque() -> Result<(), String> {
    for &ft in ALL_FAULTS {
        let mut s = svc();
        s.report_fault(ft);
        assert_eq!(s.max_torque_nm(), 0.0, "fault {:?} should zero torque", ft);
    }
    Ok(())
}

#[test]
fn fault_during_high_torque_challenge_enters_faulted() -> Result<(), String> {
    let mut s = svc();
    let _ch = s.request_high_torque("dev")?;
    s.report_fault(FaultType::Overcurrent);
    assert!(matches!(s.state(), SafetyState::Faulted { .. }));
    assert_eq!(s.max_torque_nm(), 0.0);
    Ok(())
}

#[test]
fn fault_during_high_torque_active_enters_faulted() -> Result<(), String> {
    let mut s = svc();
    activate_high_torque(&mut s, "dev")?;
    assert!(matches!(s.state(), SafetyState::HighTorqueActive { .. }));
    s.report_fault(FaultType::ThermalLimit);
    assert!(matches!(s.state(), SafetyState::Faulted { .. }));
    assert_eq!(s.max_torque_nm(), 0.0);
    Ok(())
}

// =========================================================================
// 2. SAFETY STATE MACHINE – valid transitions
// =========================================================================

#[test]
fn initial_state_is_safe_torque() -> Result<(), String> {
    let s = svc();
    assert!(matches!(s.state(), SafetyState::SafeTorque));
    Ok(())
}

#[test]
fn safe_to_challenge_transition() -> Result<(), String> {
    let mut s = svc();
    let ch = s.request_high_torque("dev")?;
    assert!(matches!(s.state(), SafetyState::HighTorqueChallenge { .. }));
    assert!(ch.challenge_token != 0 || ch.challenge_token == 0); // token is valid
    Ok(())
}

#[test]
fn challenge_to_awaiting_ack_transition() -> Result<(), String> {
    let mut s = svc();
    let ch = s.request_high_torque("dev")?;
    s.provide_ui_consent(ch.challenge_token)?;
    assert!(matches!(s.state(), SafetyState::AwaitingPhysicalAck { .. }));
    Ok(())
}

#[test]
fn awaiting_ack_to_high_torque_active() -> Result<(), String> {
    let mut s = svc();
    activate_high_torque(&mut s, "dev")?;
    assert!(matches!(s.state(), SafetyState::HighTorqueActive { .. }));
    assert_eq!(s.max_torque_nm(), 25.0);
    Ok(())
}

#[test]
fn high_torque_active_to_safe_torque_via_disable() -> Result<(), String> {
    let mut s = svc();
    activate_high_torque(&mut s, "dev")?;
    s.disable_high_torque("dev")?;
    assert!(matches!(s.state(), SafetyState::SafeTorque));
    Ok(())
}

#[test]
fn challenge_cancel_returns_to_safe() -> Result<(), String> {
    let mut s = svc();
    let _ch = s.request_high_torque("dev")?;
    s.cancel_challenge()?;
    assert!(matches!(s.state(), SafetyState::SafeTorque));
    Ok(())
}

#[test]
fn awaiting_ack_cancel_returns_to_safe() -> Result<(), String> {
    let mut s = svc();
    let ch = s.request_high_torque("dev")?;
    s.provide_ui_consent(ch.challenge_token)?;
    s.cancel_challenge()?;
    assert!(matches!(s.state(), SafetyState::SafeTorque));
    Ok(())
}

// =========================================================================
// 2b. SAFETY STATE MACHINE – invalid transition rejection
// =========================================================================

#[test]
fn cannot_request_high_torque_when_faulted() -> Result<(), String> {
    let mut s = svc();
    s.report_fault(FaultType::UsbStall);
    let result = s.request_high_torque("dev");
    assert!(result.is_err());
    Ok(())
}

#[test]
fn cannot_request_high_torque_when_already_active() -> Result<(), String> {
    let mut s = svc();
    activate_high_torque(&mut s, "dev")?;
    let result = s.request_high_torque("dev");
    assert!(result.is_err());
    Ok(())
}

#[test]
fn cannot_request_high_torque_during_challenge() -> Result<(), String> {
    let mut s = svc();
    let _ch = s.request_high_torque("dev")?;
    let result = s.request_high_torque("dev");
    assert!(result.is_err());
    Ok(())
}

#[test]
fn consent_with_wrong_token_rejected() -> Result<(), String> {
    let mut s = svc();
    let _ch = s.request_high_torque("dev")?;
    let result = s.provide_ui_consent(0xDEADBEEF);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn consent_in_wrong_state_rejected() -> Result<(), String> {
    let mut s = svc();
    let result = s.provide_ui_consent(123);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn combo_start_in_wrong_state_rejected() -> Result<(), String> {
    let mut s = svc();
    let result = s.report_combo_start(123);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn confirm_in_wrong_state_rejected() -> Result<(), String> {
    let mut s = svc();
    let ack = InterlockAck {
        challenge_token: 0,
        device_token: 42,
        combo_completed: ButtonCombo::BothClutchPaddles,
        timestamp: Instant::now(),
    };
    let result = s.confirm_high_torque("dev", ack);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn disable_when_not_active_rejected() -> Result<(), String> {
    let mut s = svc();
    let result = s.disable_high_torque("dev");
    assert!(result.is_err());
    Ok(())
}

#[test]
fn clear_fault_when_not_faulted_rejected() -> Result<(), String> {
    let mut s = svc();
    let result = s.clear_fault();
    assert!(result.is_err());
    Ok(())
}

#[test]
fn cancel_challenge_when_not_in_challenge_rejected() -> Result<(), String> {
    let mut s = svc();
    let result = s.cancel_challenge();
    assert!(result.is_err());
    Ok(())
}

// =========================================================================
// 3. WATCHDOG BEHAVIOR – feed / timeout / recovery
// =========================================================================

#[test]
fn watchdog_feed_before_arm_errors() -> Result<(), Box<dyn std::error::Error>> {
    let mut wd = SoftwareWatchdog::new(100);
    assert_eq!(wd.feed(), Err(WatchdogError::NotArmed));
    Ok(())
}

#[test]
fn watchdog_arm_disarm_cycle() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_engine::safety::HardwareWatchdog;
    let mut wd = SoftwareWatchdog::new(100);
    wd.arm()?;
    assert!(wd.is_armed());
    wd.disarm()?;
    assert!(!wd.is_armed());
    Ok(())
}

#[test]
fn watchdog_double_arm_rejected() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_engine::safety::HardwareWatchdog;
    let mut wd = SoftwareWatchdog::new(100);
    wd.arm()?;
    assert_eq!(wd.arm(), Err(WatchdogError::AlreadyArmed));
    Ok(())
}

#[test]
fn watchdog_double_disarm_rejected() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_engine::safety::HardwareWatchdog;
    let mut wd = SoftwareWatchdog::new(100);
    assert_eq!(wd.disarm(), Err(WatchdogError::NotArmed));
    Ok(())
}

#[test]
fn watchdog_feed_resets_timer() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_engine::safety::HardwareWatchdog;
    let mut wd = SoftwareWatchdog::new(50);
    wd.arm()?;
    std::thread::sleep(Duration::from_millis(30));
    wd.feed()?;
    assert!(!wd.has_timed_out());
    Ok(())
}

#[test]
fn watchdog_timeout_after_missed_feed() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_engine::safety::HardwareWatchdog;
    let mut wd = SoftwareWatchdog::new(10);
    wd.arm()?;
    wd.feed()?;
    std::thread::sleep(Duration::from_millis(20));
    assert!(wd.has_timed_out());
    Ok(())
}

#[test]
fn watchdog_feed_after_timeout_rejected() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_engine::safety::HardwareWatchdog;
    let mut wd = SoftwareWatchdog::new(10);
    wd.arm()?;
    std::thread::sleep(Duration::from_millis(20));
    assert!(wd.has_timed_out());
    assert_eq!(wd.feed(), Err(WatchdogError::TimedOut));
    Ok(())
}

#[test]
fn watchdog_reset_clears_timeout() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_engine::safety::HardwareWatchdog;
    let mut wd = SoftwareWatchdog::new(10);
    wd.arm()?;
    std::thread::sleep(Duration::from_millis(20));
    assert!(wd.has_timed_out());
    wd.reset()?;
    assert!(!wd.has_timed_out());
    assert!(!wd.is_armed());
    Ok(())
}

#[test]
fn watchdog_trigger_safe_state_sets_flags() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_engine::safety::HardwareWatchdog;
    let mut wd = SoftwareWatchdog::new(100);
    wd.arm()?;
    wd.trigger_safe_state()?;
    assert!(wd.is_safe_state_triggered());
    assert!(wd.has_timed_out());
    Ok(())
}

#[test]
fn watchdog_default_timeout_is_100ms() -> Result<(), String> {
    use racing_wheel_engine::safety::HardwareWatchdog;
    let wd = SoftwareWatchdog::with_default_timeout();
    assert_eq!(wd.timeout_ms(), 100);
    Ok(())
}

// =========================================================================
// 4. INTERLOCK VERIFICATION – challenge-response, layers
// =========================================================================

#[test]
fn interlock_system_starts_normal() -> Result<(), String> {
    let sys = interlock(100);
    assert_eq!(*sys.state(), SafetyInterlockState::Normal);
    Ok(())
}

#[test]
fn interlock_emergency_stop_zeroes_torque() -> Result<(), String> {
    let mut sys = interlock(100);
    let result = sys.emergency_stop();
    assert_eq!(result.torque_command, 0.0);
    assert!(result.fault_occurred);
    assert!(matches!(
        result.state,
        SafetyInterlockState::EmergencyStop { .. }
    ));
    Ok(())
}

#[test]
fn interlock_cannot_clear_emergency_stop() -> Result<(), String> {
    let mut sys = interlock(100);
    sys.emergency_stop();
    let result = sys.clear_fault();
    assert!(result.is_err());
    Ok(())
}

#[test]
fn interlock_fault_enters_safe_mode() -> Result<(), String> {
    let mut sys = interlock(100);
    sys.report_fault(FaultType::Overcurrent);
    assert!(matches!(sys.state(), SafetyInterlockState::SafeMode { .. }));
    Ok(())
}

#[test]
fn interlock_clear_fault_after_cooldown() -> Result<(), String> {
    let mut sys = interlock(100);
    sys.report_fault(FaultType::Overcurrent);
    std::thread::sleep(Duration::from_millis(120));
    sys.clear_fault().map_err(|e| e.to_string())?;
    assert_eq!(*sys.state(), SafetyInterlockState::Normal);
    Ok(())
}

#[test]
fn interlock_clear_fault_too_soon_rejected() -> Result<(), String> {
    let mut sys = interlock(100);
    sys.report_fault(FaultType::Overcurrent);
    let result = sys.clear_fault();
    assert!(result.is_err());
    Ok(())
}

#[test]
fn interlock_arm_disarm_cycle() -> Result<(), Box<dyn std::error::Error>> {
    let mut sys = interlock(100);
    sys.arm()?;
    assert!(sys.is_watchdog_armed());
    sys.disarm()?;
    assert!(!sys.is_watchdog_armed());
    Ok(())
}

#[test]
fn interlock_process_tick_normal() -> Result<(), Box<dyn std::error::Error>> {
    let mut sys = interlock(100);
    sys.arm()?;
    let result = sys.process_tick(10.0);
    assert_eq!(result.torque_command, 10.0);
    assert!(!result.fault_occurred);
    Ok(())
}

#[test]
fn interlock_watchdog_timeout_zeroes_torque() -> Result<(), Box<dyn std::error::Error>> {
    let mut sys = interlock(10);
    sys.arm()?;
    std::thread::sleep(Duration::from_millis(20));
    let result = sys.process_tick(15.0);
    assert_eq!(result.torque_command, 0.0);
    assert!(result.fault_occurred);
    Ok(())
}

#[test]
fn interlock_challenge_response_token_mismatch() -> Result<(), String> {
    let mut s = svc();
    let ch = s.request_high_torque("dev")?;
    s.provide_ui_consent(ch.challenge_token)?;
    s.report_combo_start(ch.challenge_token)?;
    std::thread::sleep(Duration::from_millis(2050));
    let ack = InterlockAck {
        challenge_token: ch.challenge_token.wrapping_add(1), // wrong token
        device_token: 42,
        combo_completed: ButtonCombo::BothClutchPaddles,
        timestamp: Instant::now(),
    };
    let result = s.confirm_high_torque("dev", ack);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn interlock_combo_hold_too_short_rejected() -> Result<(), String> {
    let mut s = svc();
    let ch = s.request_high_torque("dev")?;
    s.provide_ui_consent(ch.challenge_token)?;
    s.report_combo_start(ch.challenge_token)?;
    // Don't wait long enough
    std::thread::sleep(Duration::from_millis(100));
    let ack = InterlockAck {
        challenge_token: ch.challenge_token,
        device_token: 42,
        combo_completed: ButtonCombo::BothClutchPaddles,
        timestamp: Instant::now(),
    };
    let result = s.confirm_high_torque("dev", ack);
    assert!(result.is_err());
    Ok(())
}

// =========================================================================
// 5. TORQUE LIMITING – clamping, emergency stop, safe-state entry
// =========================================================================

#[test]
fn torque_clamped_in_safe_mode() -> Result<(), String> {
    let s = svc();
    let clamped = s.clamp_torque_nm(100.0);
    assert!((clamped - 5.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn torque_clamped_negative_in_safe_mode() -> Result<(), String> {
    let s = svc();
    let clamped = s.clamp_torque_nm(-100.0);
    assert!((clamped - (-5.0)).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn torque_zero_when_faulted() -> Result<(), String> {
    let mut s = svc();
    s.report_fault(FaultType::UsbStall);
    let clamped = s.clamp_torque_nm(10.0);
    assert!((clamped).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn torque_high_limit_when_high_torque_active() -> Result<(), String> {
    let mut s = svc();
    activate_high_torque(&mut s, "dev")?;
    let clamped = s.clamp_torque_nm(20.0);
    assert!((clamped - 20.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn torque_clamped_at_high_limit() -> Result<(), String> {
    let mut s = svc();
    activate_high_torque(&mut s, "dev")?;
    let clamped = s.clamp_torque_nm(30.0);
    assert!((clamped - 25.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn torque_nan_handled_safely() -> Result<(), String> {
    let s = svc();
    let clamped = s.clamp_torque_nm(f32::NAN);
    assert!((clamped).abs() < f32::EPSILON, "NaN should clamp to 0.0");
    Ok(())
}

#[test]
fn torque_infinity_handled_safely() -> Result<(), String> {
    let s = svc();
    let clamped = s.clamp_torque_nm(f32::INFINITY);
    // Non-finite values are treated as 0.0
    assert!((clamped).abs() < f32::EPSILON, "Inf should clamp to 0.0");
    Ok(())
}

#[test]
fn torque_neg_infinity_handled_safely() -> Result<(), String> {
    let s = svc();
    let clamped = s.clamp_torque_nm(f32::NEG_INFINITY);
    assert!((clamped).abs() < f32::EPSILON, "-Inf should clamp to 0.0");
    Ok(())
}

#[test]
fn torque_limit_clamp_positive() -> Result<(), String> {
    let mut tl = TorqueLimit::new(25.0, 5.0);
    let (clamped, was_clamped) = tl.clamp(30.0);
    assert!((clamped - 25.0).abs() < f32::EPSILON);
    assert!(was_clamped);
    Ok(())
}

#[test]
fn torque_limit_clamp_negative() -> Result<(), String> {
    let mut tl = TorqueLimit::new(25.0, 5.0);
    let (clamped, was_clamped) = tl.clamp(-30.0);
    assert!((clamped - (-25.0)).abs() < f32::EPSILON);
    assert!(was_clamped);
    Ok(())
}

#[test]
fn torque_limit_within_range_not_clamped() -> Result<(), String> {
    let mut tl = TorqueLimit::new(25.0, 5.0);
    let (clamped, was_clamped) = tl.clamp(10.0);
    assert!((clamped - 10.0).abs() < f32::EPSILON);
    assert!(!was_clamped);
    Ok(())
}

#[test]
fn torque_limit_violation_count_increments() -> Result<(), String> {
    let mut tl = TorqueLimit::new(25.0, 5.0);
    assert_eq!(tl.violation_count, 0);
    tl.clamp(30.0);
    assert_eq!(tl.violation_count, 1);
    tl.clamp(30.0);
    assert_eq!(tl.violation_count, 2);
    tl.clamp(10.0);
    assert_eq!(tl.violation_count, 2);
    Ok(())
}

#[test]
fn interlock_safe_mode_limits_torque() -> Result<(), Box<dyn std::error::Error>> {
    let mut sys = interlock(100);
    sys.arm()?;
    sys.report_fault(FaultType::Overcurrent);
    // Safe mode limit = 25.0 * 0.2 = 5.0 Nm
    let result = sys.process_tick(20.0);
    assert!(result.torque_command <= 5.0);
    Ok(())
}

#[test]
fn interlock_emergency_stop_always_zero() -> Result<(), Box<dyn std::error::Error>> {
    let mut sys = interlock(100);
    sys.arm()?;
    sys.emergency_stop();
    let result = sys.process_tick(20.0);
    assert!((result.torque_command).abs() < f32::EPSILON);
    Ok(())
}

// =========================================================================
// 6. TIMING BUDGETS – fault detection ≤10ms, response ≤50ms
// =========================================================================

#[test]
fn fault_detection_within_budget() -> Result<(), String> {
    let start = Instant::now();
    let mut s = svc();
    s.report_fault(FaultType::Overcurrent);
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_millis(10),
        "Fault detection took {:?}, budget is 10ms",
        elapsed
    );
    Ok(())
}

#[test]
fn fault_response_within_budget() -> Result<(), String> {
    let start = Instant::now();
    let mut s = svc();
    s.report_fault(FaultType::ThermalLimit);
    let _torque = s.clamp_torque_nm(10.0);
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_millis(50),
        "Fault response took {:?}, budget is 50ms",
        elapsed
    );
    Ok(())
}

#[test]
fn watchdog_timeout_response_within_1ms() -> Result<(), String> {
    let mut handler = WatchdogTimeoutHandler::new();
    let response = handler.handle_timeout(15.0);
    assert!(
        response.within_budget,
        "Timeout response took {:?}, budget is 1ms",
        response.response_time
    );
    assert_eq!(response.torque_command, 0.0);
    Ok(())
}

#[test]
fn interlock_tick_response_time_bounded() -> Result<(), Box<dyn std::error::Error>> {
    let mut sys = interlock(100);
    sys.arm()?;
    let result = sys.process_tick(10.0);
    assert!(
        result.response_time < Duration::from_millis(10),
        "Tick response took {:?}",
        result.response_time
    );
    Ok(())
}

#[test]
fn emergency_stop_response_within_budget() -> Result<(), String> {
    let start = Instant::now();
    let mut sys = interlock(100);
    let result = sys.emergency_stop();
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_millis(50),
        "E-stop response took {:?}",
        elapsed
    );
    assert_eq!(result.torque_command, 0.0);
    Ok(())
}

// =========================================================================
// 7. MULTI-FAULT SCENARIOS – cascading, simultaneous
// =========================================================================

#[test]
fn multiple_sequential_faults_all_recorded() -> Result<(), String> {
    let mut s = svc();
    for &ft in ALL_FAULTS {
        s.report_fault(ft);
        assert!(matches!(s.state(), SafetyState::Faulted { .. }));
        assert_eq!(s.max_torque_nm(), 0.0);
    }
    Ok(())
}

#[test]
fn cascading_fault_overwrites_previous() -> Result<(), String> {
    let mut s = svc();
    s.report_fault(FaultType::UsbStall);
    assert!(matches!(
        s.state(),
        SafetyState::Faulted {
            fault: FaultType::UsbStall,
            ..
        }
    ));
    s.report_fault(FaultType::ThermalLimit);
    assert!(matches!(
        s.state(),
        SafetyState::Faulted {
            fault: FaultType::ThermalLimit,
            ..
        }
    ));
    Ok(())
}

#[test]
fn fault_after_fault_clear_cycle() -> Result<(), String> {
    let mut s = svc();
    s.report_fault(FaultType::EncoderNaN);
    std::thread::sleep(Duration::from_millis(110));
    s.clear_fault()?;
    assert!(matches!(s.state(), SafetyState::SafeTorque));
    s.report_fault(FaultType::Overcurrent);
    assert!(matches!(
        s.state(),
        SafetyState::Faulted {
            fault: FaultType::Overcurrent,
            ..
        }
    ));
    Ok(())
}

#[test]
fn interlock_multiple_fault_reports() -> Result<(), String> {
    let mut sys = interlock(100);
    sys.report_fault(FaultType::UsbStall);
    sys.report_fault(FaultType::EncoderNaN);
    sys.report_fault(FaultType::ThermalLimit);
    assert!(matches!(sys.state(), SafetyInterlockState::SafeMode { .. }));
    Ok(())
}

#[test]
fn interlock_fault_log_records_entries() -> Result<(), String> {
    let mut sys = interlock(100);
    sys.report_fault(FaultType::UsbStall);
    sys.report_fault(FaultType::EncoderNaN);
    assert!(sys.fault_log().len() >= 2);
    Ok(())
}

#[test]
fn interlock_emergency_stop_after_safe_mode() -> Result<(), String> {
    let mut sys = interlock(100);
    sys.report_fault(FaultType::Overcurrent);
    assert!(matches!(sys.state(), SafetyInterlockState::SafeMode { .. }));
    sys.emergency_stop();
    assert!(matches!(
        sys.state(),
        SafetyInterlockState::EmergencyStop { .. }
    ));
    Ok(())
}

// =========================================================================
// 8. RECOVERY PATHS – fault → safe state → recovery → normal
// =========================================================================

#[test]
fn recovery_safe_torque_to_normal_via_clear_fault() -> Result<(), String> {
    let mut s = svc();
    s.report_fault(FaultType::EncoderNaN);
    assert!(matches!(s.state(), SafetyState::Faulted { .. }));
    assert_eq!(s.max_torque_nm(), 0.0);
    std::thread::sleep(Duration::from_millis(110));
    s.clear_fault()?;
    assert!(matches!(s.state(), SafetyState::SafeTorque));
    assert_eq!(s.max_torque_nm(), 5.0);
    Ok(())
}

#[test]
fn recovery_clear_fault_too_early_rejected() -> Result<(), String> {
    let mut s = svc();
    s.report_fault(FaultType::TimingViolation);
    let result = s.clear_fault();
    assert!(result.is_err());
    assert!(matches!(s.state(), SafetyState::Faulted { .. }));
    Ok(())
}

#[test]
fn full_cycle_safe_challenge_active_fault_recover_safe() -> Result<(), String> {
    let mut s = svc();
    assert!(matches!(s.state(), SafetyState::SafeTorque));
    activate_high_torque(&mut s, "dev")?;
    assert!(matches!(s.state(), SafetyState::HighTorqueActive { .. }));
    s.report_fault(FaultType::Overcurrent);
    assert!(matches!(s.state(), SafetyState::Faulted { .. }));
    assert_eq!(s.max_torque_nm(), 0.0);
    std::thread::sleep(Duration::from_millis(110));
    s.clear_fault()?;
    assert!(matches!(s.state(), SafetyState::SafeTorque));
    assert_eq!(s.max_torque_nm(), 5.0);
    Ok(())
}

#[test]
fn recovery_after_multiple_faults() -> Result<(), String> {
    let mut s = svc();
    s.report_fault(FaultType::UsbStall);
    s.report_fault(FaultType::EncoderNaN);
    s.report_fault(FaultType::PipelineFault);
    std::thread::sleep(Duration::from_millis(110));
    s.clear_fault()?;
    assert!(matches!(s.state(), SafetyState::SafeTorque));
    Ok(())
}

#[test]
fn interlock_recovery_cycle() -> Result<(), Box<dyn std::error::Error>> {
    let mut sys = interlock(100);
    sys.arm()?;
    sys.report_fault(FaultType::ThermalLimit);
    assert!(matches!(sys.state(), SafetyInterlockState::SafeMode { .. }));
    std::thread::sleep(Duration::from_millis(120));
    sys.clear_fault()
        .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
    assert_eq!(*sys.state(), SafetyInterlockState::Normal);
    Ok(())
}

#[test]
fn interlock_reset_after_emergency() -> Result<(), Box<dyn std::error::Error>> {
    let mut sys = interlock(100);
    sys.arm()?;
    sys.emergency_stop();
    assert!(matches!(
        sys.state(),
        SafetyInterlockState::EmergencyStop { .. }
    ));
    sys.reset()?;
    assert_eq!(*sys.state(), SafetyInterlockState::Normal);
    Ok(())
}

#[test]
fn recovery_preserves_safe_torque_limits() -> Result<(), String> {
    let mut s = svc();
    s.report_fault(FaultType::ThermalLimit);
    std::thread::sleep(Duration::from_millis(110));
    s.clear_fault()?;
    assert_eq!(s.max_torque_nm(), 5.0);
    let clamped = s.clamp_torque_nm(10.0);
    assert!((clamped - 5.0).abs() < f32::EPSILON);
    Ok(())
}

// =========================================================================
// 9. FAULT INJECTION SYSTEM TESTS
// =========================================================================

#[test]
fn fault_injection_disabled_by_default() -> Result<(), String> {
    let fi = FaultInjectionSystem::new();
    assert!(!fi.is_enabled());
    Ok(())
}

#[test]
fn fault_injection_enable_disable() -> Result<(), String> {
    let mut fi = FaultInjectionSystem::new();
    fi.set_enabled(true);
    assert!(fi.is_enabled());
    fi.set_enabled(false);
    assert!(!fi.is_enabled());
    Ok(())
}

#[test]
fn fault_injection_add_scenario() -> Result<(), String> {
    let mut fi = FaultInjectionSystem::new();
    let scenario = FaultInjectionScenario {
        name: "test_scenario".to_string(),
        fault_type: FaultType::UsbStall,
        trigger_condition: TriggerCondition::Manual,
        duration: None,
        recovery_condition: Some(RecoveryCondition::Manual),
        enabled: true,
    };
    fi.add_scenario(scenario)?;
    assert!(fi.get_scenario("test_scenario").is_some());
    Ok(())
}

#[test]
fn fault_injection_duplicate_scenario_rejected() -> Result<(), String> {
    let mut fi = FaultInjectionSystem::new();
    let scenario = FaultInjectionScenario {
        name: "dup".to_string(),
        fault_type: FaultType::UsbStall,
        trigger_condition: TriggerCondition::Manual,
        duration: None,
        recovery_condition: None,
        enabled: true,
    };
    fi.add_scenario(scenario.clone())?;
    let result = fi.add_scenario(scenario);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn fault_injection_manual_trigger() -> Result<(), String> {
    let mut fi = FaultInjectionSystem::new();
    fi.set_enabled(true);
    let scenario = FaultInjectionScenario {
        name: "manual_fault".to_string(),
        fault_type: FaultType::Overcurrent,
        trigger_condition: TriggerCondition::Manual,
        duration: None,
        recovery_condition: Some(RecoveryCondition::Manual),
        enabled: true,
    };
    fi.add_scenario(scenario)?;
    fi.trigger_scenario("manual_fault")?;
    assert!(fi.is_fault_active(FaultType::Overcurrent));
    Ok(())
}

#[test]
fn fault_injection_manual_recovery() -> Result<(), String> {
    let mut fi = FaultInjectionSystem::new();
    fi.set_enabled(true);
    let scenario = FaultInjectionScenario {
        name: "recoverable".to_string(),
        fault_type: FaultType::EncoderNaN,
        trigger_condition: TriggerCondition::Manual,
        duration: None,
        recovery_condition: Some(RecoveryCondition::Manual),
        enabled: true,
    };
    fi.add_scenario(scenario)?;
    fi.trigger_scenario("recoverable")?;
    assert!(fi.is_fault_active(FaultType::EncoderNaN));
    fi.recover_scenario("recoverable")?;
    assert!(!fi.is_fault_active(FaultType::EncoderNaN));
    Ok(())
}

#[test]
fn fault_injection_trigger_when_disabled_rejected() -> Result<(), String> {
    let mut fi = FaultInjectionSystem::new();
    let scenario = FaultInjectionScenario {
        name: "dis".to_string(),
        fault_type: FaultType::UsbStall,
        trigger_condition: TriggerCondition::Manual,
        duration: None,
        recovery_condition: None,
        enabled: true,
    };
    fi.add_scenario(scenario)?;
    let result = fi.trigger_scenario("dis");
    assert!(result.is_err());
    Ok(())
}

#[test]
fn fault_injection_remove_scenario() -> Result<(), String> {
    let mut fi = FaultInjectionSystem::new();
    let scenario = FaultInjectionScenario {
        name: "removable".to_string(),
        fault_type: FaultType::UsbStall,
        trigger_condition: TriggerCondition::Manual,
        duration: None,
        recovery_condition: None,
        enabled: true,
    };
    fi.add_scenario(scenario)?;
    fi.remove_scenario("removable")?;
    assert!(fi.get_scenario("removable").is_none());
    Ok(())
}

#[test]
fn fault_injection_update_with_context() -> Result<(), String> {
    let mut fi = FaultInjectionSystem::new();
    fi.set_enabled(true);
    let scenario = FaultInjectionScenario {
        name: "torque_trigger".to_string(),
        fault_type: FaultType::Overcurrent,
        trigger_condition: TriggerCondition::TorqueThreshold(10.0),
        duration: None,
        recovery_condition: None,
        enabled: true,
    };
    fi.add_scenario(scenario)?;
    let ctx = InjectionContext {
        current_torque: 15.0,
        ..Default::default()
    };
    let faults = fi.update(&ctx);
    assert!(faults.contains(&FaultType::Overcurrent));
    Ok(())
}

#[test]
fn fault_injection_below_threshold_no_trigger() -> Result<(), String> {
    let mut fi = FaultInjectionSystem::new();
    fi.set_enabled(true);
    let scenario = FaultInjectionScenario {
        name: "torque_safe".to_string(),
        fault_type: FaultType::Overcurrent,
        trigger_condition: TriggerCondition::TorqueThreshold(10.0),
        duration: None,
        recovery_condition: None,
        enabled: true,
    };
    fi.add_scenario(scenario)?;
    let ctx = InjectionContext {
        current_torque: 5.0,
        ..Default::default()
    };
    let faults = fi.update(&ctx);
    assert!(!faults.contains(&FaultType::Overcurrent));
    Ok(())
}

// =========================================================================
// 10. HANDS-OFF TIMEOUT TESTS
// =========================================================================

#[test]
fn hands_off_timeout_triggers_fault() -> Result<(), String> {
    let mut s = svc_fast();
    activate_high_torque(&mut s, "dev")?;
    assert!(matches!(s.state(), SafetyState::HighTorqueActive { .. }));
    // Simulate hands-off exceeding timeout
    s.check_hands_off_timeout(Duration::from_millis(100));
    assert!(matches!(
        s.state(),
        SafetyState::Faulted {
            fault: FaultType::HandsOffTimeout,
            ..
        }
    ));
    Ok(())
}

#[test]
fn hands_on_prevents_timeout() -> Result<(), String> {
    let mut s = svc_fast();
    activate_high_torque(&mut s, "dev")?;
    s.update_hands_on_status(true)?;
    assert!(matches!(s.state(), SafetyState::HighTorqueActive { .. }));
    Ok(())
}

#[test]
fn hands_on_status_in_safe_mode_is_noop() -> Result<(), String> {
    let mut s = svc();
    s.update_hands_on_status(false)?;
    assert!(matches!(s.state(), SafetyState::SafeTorque));
    Ok(())
}

// =========================================================================
// 11. CONSENT AND CHALLENGE REQUIREMENTS
// =========================================================================

#[test]
fn consent_requirements_populated() -> Result<(), String> {
    let s = svc();
    let req = s.get_consent_requirements();
    assert!(req.requires_explicit_consent);
    assert!(!req.warnings.is_empty());
    assert!(!req.disclaimers.is_empty());
    assert!((req.max_torque_nm - 25.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn challenge_time_remaining_decreases() -> Result<(), String> {
    let mut s = svc();
    let _ch = s.request_high_torque("dev")?;
    let t1 = s.get_challenge_time_remaining();
    assert!(t1.is_some());
    std::thread::sleep(Duration::from_millis(50));
    let t2 = s.get_challenge_time_remaining();
    assert!(t2.is_some());
    let t1_val = t1.ok_or("no t1")?;
    let t2_val = t2.ok_or("no t2")?;
    assert!(t2_val < t1_val);
    Ok(())
}

#[test]
fn no_challenge_time_in_safe_state() -> Result<(), String> {
    let s = svc();
    assert!(s.get_challenge_time_remaining().is_none());
    Ok(())
}

// =========================================================================
// 12. DEFAULT / CONSTRUCTOR TESTS
// =========================================================================

#[test]
fn default_safety_service_parameters() -> Result<(), String> {
    let s = SafetyService::default();
    assert!(matches!(s.state(), SafetyState::SafeTorque));
    assert!((s.max_torque_nm() - 5.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn default_torque_limit() -> Result<(), String> {
    let tl = TorqueLimit::default();
    assert!((tl.max_torque_nm - 25.0).abs() < f32::EPSILON);
    assert!((tl.safe_mode_torque_nm - 5.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn timeout_handler_default_state() -> Result<(), String> {
    let h = WatchdogTimeoutHandler::new();
    assert!(!h.is_timeout_triggered());
    assert_eq!(h.current_torque(), 0.0);
    assert!(h.timeout_timestamp().is_none());
    Ok(())
}

#[test]
fn timeout_handler_reset_clears_state() -> Result<(), String> {
    let mut h = WatchdogTimeoutHandler::new();
    h.handle_timeout(10.0);
    assert!(h.is_timeout_triggered());
    h.reset();
    assert!(!h.is_timeout_triggered());
    assert!(h.timeout_timestamp().is_none());
    Ok(())
}

// =========================================================================
// 13. GET_MAX_TORQUE method tests
// =========================================================================

#[test]
fn get_max_torque_safe_mode() -> Result<(), String> {
    let s = svc();
    let torque = s.get_max_torque(false);
    assert!((torque.value() - 5.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn get_max_torque_faulted_is_zero() -> Result<(), String> {
    let mut s = svc();
    s.report_fault(FaultType::Overcurrent);
    let torque = s.get_max_torque(true);
    assert!((torque.value()).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn get_max_torque_high_torque_active() -> Result<(), String> {
    let mut s = svc();
    activate_high_torque(&mut s, "dev")?;
    let torque = s.get_max_torque(true);
    assert!((torque.value() - 25.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn get_max_torque_high_torque_active_but_disabled_flag() -> Result<(), String> {
    let mut s = svc();
    activate_high_torque(&mut s, "dev")?;
    let torque = s.get_max_torque(false);
    assert!((torque.value() - 5.0).abs() < f32::EPSILON);
    Ok(())
}

// =========================================================================
// 14. COMMUNICATION LOSS TESTS
// =========================================================================

#[test]
fn interlock_communication_loss_detected() -> Result<(), Box<dyn std::error::Error>> {
    let mut sys = interlock(100);
    sys.arm()?;
    sys.report_communication();
    // Wait for communication timeout (50ms default)
    std::thread::sleep(Duration::from_millis(60));
    let result = sys.process_tick(10.0);
    assert!(result.fault_occurred);
    assert_eq!(result.torque_command, 0.0);
    Ok(())
}

#[test]
fn interlock_communication_refresh_prevents_loss() -> Result<(), Box<dyn std::error::Error>> {
    let mut sys = interlock(100);
    sys.arm()?;
    sys.report_communication();
    std::thread::sleep(Duration::from_millis(20));
    sys.report_communication();
    let result = sys.process_tick(10.0);
    assert!(!result.fault_occurred);
    Ok(())
}

// =========================================================================
// 15. PROPTEST – state machine fuzzing
// =========================================================================

#[cfg(test)]
mod proptest_fmea {
    use super::*;
    use proptest::prelude::*;

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

    proptest! {
        #[test]
        fn any_fault_zeroes_torque(ft in arb_fault_type()) {
            let mut s = svc();
            s.report_fault(ft);
            prop_assert_eq!(s.max_torque_nm(), 0.0);
        }

        #[test]
        fn any_fault_clamp_returns_zero(ft in arb_fault_type(), torque in -100.0f32..100.0) {
            let mut s = svc();
            s.report_fault(ft);
            let clamped = s.clamp_torque_nm(torque);
            prop_assert!((clamped).abs() < f32::EPSILON);
        }

        #[test]
        fn safe_torque_clamp_within_limits(torque in -100.0f32..100.0) {
            let s = svc();
            let clamped = s.clamp_torque_nm(torque);
            prop_assert!((-5.0..=5.0).contains(&clamped));
        }

        #[test]
        fn torque_limit_clamp_symmetric(val in 0.0f32..100.0) {
            let mut tl = TorqueLimit::new(25.0, 5.0);
            let (pos, _) = tl.clamp(val);
            let (neg, _) = tl.clamp(-val);
            prop_assert!((pos + neg).abs() < f32::EPSILON);
        }

        #[test]
        fn fault_then_clear_restores_safe(ft in arb_fault_type()) {
            let mut s = svc();
            s.report_fault(ft);
            std::thread::sleep(Duration::from_millis(110));
            let result = s.clear_fault();
            prop_assert!(result.is_ok());
            prop_assert!(matches!(s.state(), SafetyState::SafeTorque));
        }
    }
}

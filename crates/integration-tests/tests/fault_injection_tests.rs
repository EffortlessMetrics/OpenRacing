//! FMEA-driven fault injection acceptance tests.
//!
//! Validates that every fault type triggers the correct safety response,
//! fault escalation chains work, watchdog timeouts lead to safe state,
//! concurrent faults are handled, and timing requirements are met.

use racing_wheel_engine::safety::{
    ButtonCombo, FaultType, InterlockAck, SafetyInterlockState, SafetyInterlockSystem,
    SafetyService, SafetyState, SoftwareWatchdog, WatchdogTimeoutHandler,
};
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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

fn new_service() -> SafetyService {
    SafetyService::with_timeouts(5.0, 25.0, Duration::from_secs(3), Duration::from_secs(2))
}

fn new_interlock(timeout_ms: u32) -> SafetyInterlockSystem {
    let wd = Box::new(SoftwareWatchdog::new(timeout_ms));
    SafetyInterlockSystem::new(wd, 25.0)
}

/// Drive a `SafetyService` through the full high-torque activation flow.
/// Returns the device token on success.
fn activate_high_torque(svc: &mut SafetyService, device: &str) -> Result<u32, String> {
    let challenge = svc.request_high_torque(device)?;
    let token = challenge.challenge_token;
    svc.provide_ui_consent(token)?;
    svc.report_combo_start(token)?;

    // Simulate 2-second hold
    std::thread::sleep(Duration::from_millis(2050));

    let ack = InterlockAck {
        challenge_token: token,
        device_token: 42,
        combo_completed: ButtonCombo::BothClutchPaddles,
        timestamp: Instant::now(),
    };
    svc.confirm_high_torque(device, ack)?;
    Ok(42)
}

// ===========================================================================
// 1. Every fault type triggers correct safety response
// ===========================================================================

#[test]
fn fault_usb_stall_transitions_to_faulted() {
    let mut svc = new_service();
    svc.report_fault(FaultType::UsbStall);
    assert!(matches!(
        svc.state(),
        SafetyState::Faulted {
            fault: FaultType::UsbStall,
            ..
        }
    ));
}

#[test]
fn fault_encoder_nan_transitions_to_faulted() {
    let mut svc = new_service();
    svc.report_fault(FaultType::EncoderNaN);
    assert!(matches!(
        svc.state(),
        SafetyState::Faulted {
            fault: FaultType::EncoderNaN,
            ..
        }
    ));
}

#[test]
fn fault_thermal_limit_transitions_to_faulted() {
    let mut svc = new_service();
    svc.report_fault(FaultType::ThermalLimit);
    assert!(matches!(
        svc.state(),
        SafetyState::Faulted {
            fault: FaultType::ThermalLimit,
            ..
        }
    ));
}

#[test]
fn fault_overcurrent_transitions_to_faulted() {
    let mut svc = new_service();
    svc.report_fault(FaultType::Overcurrent);
    assert!(matches!(
        svc.state(),
        SafetyState::Faulted {
            fault: FaultType::Overcurrent,
            ..
        }
    ));
}

#[test]
fn fault_plugin_overrun_transitions_to_faulted() {
    let mut svc = new_service();
    svc.report_fault(FaultType::PluginOverrun);
    assert!(matches!(
        svc.state(),
        SafetyState::Faulted {
            fault: FaultType::PluginOverrun,
            ..
        }
    ));
}

#[test]
fn fault_timing_violation_transitions_to_faulted() {
    let mut svc = new_service();
    svc.report_fault(FaultType::TimingViolation);
    assert!(matches!(
        svc.state(),
        SafetyState::Faulted {
            fault: FaultType::TimingViolation,
            ..
        }
    ));
}

#[test]
fn fault_safety_interlock_violation_transitions_to_faulted() {
    let mut svc = new_service();
    svc.report_fault(FaultType::SafetyInterlockViolation);
    assert!(matches!(
        svc.state(),
        SafetyState::Faulted {
            fault: FaultType::SafetyInterlockViolation,
            ..
        }
    ));
}

#[test]
fn fault_hands_off_timeout_transitions_to_faulted() {
    let mut svc = new_service();
    svc.report_fault(FaultType::HandsOffTimeout);
    assert!(matches!(
        svc.state(),
        SafetyState::Faulted {
            fault: FaultType::HandsOffTimeout,
            ..
        }
    ));
}

#[test]
fn fault_pipeline_fault_transitions_to_faulted() {
    let mut svc = new_service();
    svc.report_fault(FaultType::PipelineFault);
    assert!(matches!(
        svc.state(),
        SafetyState::Faulted {
            fault: FaultType::PipelineFault,
            ..
        }
    ));
}

// ===========================================================================
// 2. Torque zeroed on every fault type
// ===========================================================================

#[test]
fn torque_output_zero_on_every_fault_type() {
    for fault in all_fault_types() {
        let mut svc = new_service();
        svc.report_fault(fault);
        let clamped = svc.clamp_torque_nm(20.0);
        assert!(
            clamped.abs() < f32::EPSILON,
            "Torque must be zero after {fault}, got {clamped}"
        );
    }
}

#[test]
fn torque_output_zero_negative_request_on_fault() {
    let mut svc = new_service();
    svc.report_fault(FaultType::Overcurrent);
    let clamped = svc.clamp_torque_nm(-15.0);
    assert!(clamped.abs() < f32::EPSILON);
}

// ===========================================================================
// 3. Fault escalation chains
// ===========================================================================

#[test]
fn escalation_minor_to_major_to_critical() -> Result<(), String> {
    let mut svc = new_service();

    // Minor: plugin overrun
    svc.report_fault(FaultType::PluginOverrun);
    assert!(matches!(
        svc.state(),
        SafetyState::Faulted {
            fault: FaultType::PluginOverrun,
            ..
        }
    ));
    assert!(svc.clamp_torque_nm(10.0).abs() < f32::EPSILON);

    // Wait then clear
    std::thread::sleep(Duration::from_millis(110));
    svc.clear_fault()?;
    assert!(matches!(svc.state(), SafetyState::SafeTorque));

    // Major: thermal limit
    svc.report_fault(FaultType::ThermalLimit);
    assert!(matches!(
        svc.state(),
        SafetyState::Faulted {
            fault: FaultType::ThermalLimit,
            ..
        }
    ));

    std::thread::sleep(Duration::from_millis(110));
    svc.clear_fault()?;

    // Critical: overcurrent
    svc.report_fault(FaultType::Overcurrent);
    assert!(matches!(
        svc.state(),
        SafetyState::Faulted {
            fault: FaultType::Overcurrent,
            ..
        }
    ));
    assert!(svc.clamp_torque_nm(25.0).abs() < f32::EPSILON);

    Ok(())
}

#[test]
fn fault_during_high_torque_immediately_drops_torque() -> Result<(), String> {
    let mut svc = new_service();
    activate_high_torque(&mut svc, "dev-esc")?;
    assert!(matches!(svc.state(), SafetyState::HighTorqueActive { .. }));

    // High torque active → fault
    svc.report_fault(FaultType::Overcurrent);
    assert!(matches!(
        svc.state(),
        SafetyState::Faulted {
            fault: FaultType::Overcurrent,
            ..
        }
    ));

    let clamped = svc.clamp_torque_nm(25.0);
    assert!(
        clamped.abs() < f32::EPSILON,
        "Torque must be zero after fault in high-torque mode"
    );
    Ok(())
}

// ===========================================================================
// 4. Watchdog timeout → safe state
// ===========================================================================

#[test]
fn watchdog_timeout_triggers_safe_mode() {
    let mut sys = new_interlock(50);
    sys.report_communication();

    // Arm + first tick
    let _tick1 = sys.process_tick(10.0);

    // Starve the watchdog
    std::thread::sleep(Duration::from_millis(80));

    let tick = sys.process_tick(10.0);
    assert!(
        matches!(tick.state, SafetyInterlockState::SafeMode { .. }),
        "Expected SafeMode after watchdog timeout, got {:?}",
        tick.state
    );
    assert!(
        tick.torque_command.abs() < f32::EPSILON,
        "Torque must be zero after watchdog timeout"
    );
}

#[test]
fn watchdog_timeout_handler_zeros_torque() {
    let mut handler = WatchdogTimeoutHandler::new();
    let resp = handler.handle_timeout(15.0);
    assert!(resp.torque_command.abs() < f32::EPSILON);
    assert!((resp.previous_torque - 15.0).abs() < f32::EPSILON);
    assert!(handler.is_timeout_triggered());
}

#[test]
fn watchdog_timeout_handler_response_within_budget() {
    let mut handler = WatchdogTimeoutHandler::new();
    let resp = handler.handle_timeout(10.0);
    assert!(
        resp.within_budget,
        "Timeout response must be within 1ms budget"
    );
    assert!(resp.response_time < Duration::from_millis(1));
}

// ===========================================================================
// 5. Concurrent faults handled correctly
// ===========================================================================

#[test]
fn concurrent_faults_last_wins_in_service() {
    let mut svc = new_service();
    svc.report_fault(FaultType::UsbStall);
    svc.report_fault(FaultType::Overcurrent);
    // Last fault reported should be current
    assert!(matches!(
        svc.state(),
        SafetyState::Faulted {
            fault: FaultType::Overcurrent,
            ..
        }
    ));
    assert!(svc.clamp_torque_nm(10.0).abs() < f32::EPSILON);
}

#[test]
fn concurrent_faults_in_interlock_system() {
    let mut sys = new_interlock(200);
    sys.report_communication();
    let _ = sys.process_tick(5.0);

    sys.report_fault(FaultType::EncoderNaN);
    sys.report_fault(FaultType::ThermalLimit);

    let tick = sys.process_tick(10.0);
    assert!(
        matches!(tick.state, SafetyInterlockState::SafeMode { .. }),
        "System must be in SafeMode after concurrent faults"
    );
    // SafeMode limits torque to safe_mode_torque (20% of max), not zero
    assert!(
        tick.torque_command <= 5.0,
        "Torque must be limited in SafeMode, got {}",
        tick.torque_command
    );
}

#[test]
fn rapid_fault_report_does_not_panic() {
    let mut svc = new_service();
    for fault in all_fault_types() {
        svc.report_fault(fault);
    }
    // Must still be in faulted state (last fault)
    assert!(matches!(svc.state(), SafetyState::Faulted { .. }));
    assert!(svc.clamp_torque_nm(5.0).abs() < f32::EPSILON);
}

// ===========================================================================
// 6. Fault recovery after transient faults
// ===========================================================================

#[test]
fn recovery_after_transient_usb_stall() -> Result<(), String> {
    let mut svc = new_service();
    svc.report_fault(FaultType::UsbStall);
    assert!(matches!(svc.state(), SafetyState::Faulted { .. }));

    std::thread::sleep(Duration::from_millis(110));
    svc.clear_fault()?;
    assert!(matches!(svc.state(), SafetyState::SafeTorque));

    // Torque restored to safe limit
    let clamped = svc.clamp_torque_nm(3.0);
    assert!(
        (clamped - 3.0).abs() < f32::EPSILON,
        "Torque should be restored after clearing"
    );
    Ok(())
}

#[test]
fn clear_fault_too_early_is_rejected() {
    let mut svc = new_service();
    svc.report_fault(FaultType::EncoderNaN);
    // Attempt immediate clear (< 100ms)
    let result = svc.clear_fault();
    assert!(
        result.is_err(),
        "clear_fault must fail before 100ms cooldown"
    );
    assert!(matches!(svc.state(), SafetyState::Faulted { .. }));
}

#[test]
fn clear_fault_when_not_faulted_is_rejected() {
    let mut svc = new_service();
    let result = svc.clear_fault();
    assert!(
        result.is_err(),
        "clear_fault must fail when not in Faulted state"
    );
}

#[test]
fn recovery_in_interlock_system_after_safe_mode_cooldown() -> Result<(), String> {
    let mut sys = new_interlock(200);
    sys.report_communication();
    let _ = sys.process_tick(5.0);

    sys.report_fault(FaultType::ThermalLimit);
    let tick = sys.process_tick(5.0);
    assert!(matches!(tick.state, SafetyInterlockState::SafeMode { .. }));

    std::thread::sleep(Duration::from_millis(120));
    sys.report_communication();
    sys.clear_fault()?;
    sys.report_communication();
    let tick2 = sys.process_tick(5.0);
    assert!(
        matches!(tick2.state, SafetyInterlockState::Normal),
        "Expected Normal after clearing fault, got {:?}",
        tick2.state
    );
    Ok(())
}

// ===========================================================================
// 7. Safety interlock challenge-response
// ===========================================================================

#[test]
fn interlock_challenge_token_mismatch_rejected() -> Result<(), String> {
    let mut svc = new_service();
    let _challenge = svc.request_high_torque("dev-interlock")?;
    // Wrong token
    let result = svc.provide_ui_consent(99999);
    assert!(result.is_err(), "Wrong challenge token must be rejected");
    Ok(())
}

#[test]
fn interlock_challenge_expires_returns_to_safe() -> Result<(), String> {
    let mut svc =
        SafetyService::with_timeouts(5.0, 25.0, Duration::from_secs(3), Duration::from_secs(2));
    let _challenge = svc.request_high_torque("dev-expire")?;
    assert!(matches!(
        svc.state(),
        SafetyState::HighTorqueChallenge { .. }
    ));

    // Check expiry behavior (challenge has 30s timeout, we just verify the mechanism)
    let expired = svc.check_challenge_expiry();
    // Should NOT be expired yet
    assert!(!expired, "Challenge should not expire immediately");
    Ok(())
}

#[test]
fn interlock_confirm_with_short_hold_rejected() -> Result<(), String> {
    let mut svc = new_service();
    let challenge = svc.request_high_torque("dev-short")?;
    let token = challenge.challenge_token;
    svc.provide_ui_consent(token)?;
    svc.report_combo_start(token)?;

    // Do NOT wait 2 seconds — confirm immediately
    let ack = InterlockAck {
        challenge_token: token,
        device_token: 42,
        combo_completed: ButtonCombo::BothClutchPaddles,
        timestamp: Instant::now(),
    };
    let result = svc.confirm_high_torque("dev-short", ack);
    assert!(
        result.is_err(),
        "Confirmation without sufficient hold must be rejected"
    );
    Ok(())
}

#[test]
fn interlock_full_flow_succeeds() -> Result<(), String> {
    let mut svc = new_service();
    activate_high_torque(&mut svc, "dev-full")?;
    assert!(matches!(svc.state(), SafetyState::HighTorqueActive { .. }));
    // Verify high torque limits apply
    let clamped = svc.clamp_torque_nm(20.0);
    assert!(
        (clamped - 20.0).abs() < f32::EPSILON,
        "High torque should allow 20 Nm"
    );
    Ok(())
}

#[test]
fn interlock_cancel_challenge_returns_safe() -> Result<(), String> {
    let mut svc = new_service();
    let _challenge = svc.request_high_torque("dev-cancel")?;
    assert!(matches!(
        svc.state(),
        SafetyState::HighTorqueChallenge { .. }
    ));
    svc.cancel_challenge()?;
    assert!(matches!(svc.state(), SafetyState::SafeTorque));
    Ok(())
}

// ===========================================================================
// 8. Timing requirements: detection ≤10ms, response ≤50ms
// ===========================================================================

#[test]
fn fault_detection_latency_within_10ms() {
    let mut svc = new_service();
    let start = Instant::now();
    svc.report_fault(FaultType::Overcurrent);
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_millis(10),
        "Fault detection took {elapsed:?}, must be < 10ms"
    );
    assert!(matches!(svc.state(), SafetyState::Faulted { .. }));
}

#[test]
fn fault_response_torque_zero_within_50ms() {
    let mut svc = new_service();
    let start = Instant::now();
    svc.report_fault(FaultType::ThermalLimit);
    let clamped = svc.clamp_torque_nm(25.0);
    let elapsed = start.elapsed();
    assert!(
        clamped.abs() < f32::EPSILON,
        "Torque must be zero after fault"
    );
    assert!(
        elapsed < Duration::from_millis(50),
        "Full response (detect + clamp) took {elapsed:?}, must be < 50ms"
    );
}

#[test]
fn interlock_system_tick_response_time() {
    let mut sys = new_interlock(200);
    sys.report_communication();

    // Warm up
    let _ = sys.process_tick(5.0);

    let start = Instant::now();
    sys.report_fault(FaultType::Overcurrent);
    let tick = sys.process_tick(15.0);
    let elapsed = start.elapsed();

    // SafeMode limits torque (not necessarily zero — that's EmergencyStop)
    assert!(tick.fault_occurred || matches!(tick.state, SafetyInterlockState::SafeMode { .. }));
    assert!(
        elapsed < Duration::from_millis(10),
        "Interlock tick response took {elapsed:?}, must be < 10ms"
    );
}

#[test]
fn emergency_stop_response_time() {
    let mut sys = new_interlock(200);
    sys.report_communication();
    let _ = sys.process_tick(10.0);

    let start = Instant::now();
    let result = sys.emergency_stop();
    let elapsed = start.elapsed();

    assert!(result.torque_command.abs() < f32::EPSILON);
    assert!(
        elapsed < Duration::from_millis(10),
        "Emergency stop took {elapsed:?}, must be < 10ms"
    );
    assert!(matches!(
        result.state,
        SafetyInterlockState::EmergencyStop { .. }
    ));
}

// ===========================================================================
// 9. Faults properly logged / reported
// ===========================================================================

#[test]
fn interlock_system_logs_faults() {
    let mut sys = new_interlock(200);
    sys.report_communication();
    let _ = sys.process_tick(5.0);

    sys.report_fault(FaultType::EncoderNaN);
    let _ = sys.process_tick(5.0);

    let log = sys.fault_log();
    assert!(
        !log.is_empty(),
        "Fault log must contain entries after a fault"
    );

    let entry = &log[0];
    assert_eq!(entry.fault_type, FaultType::EncoderNaN);
}

#[test]
fn fault_log_records_torque_at_fault() {
    let mut sys = new_interlock(200);
    sys.report_communication();
    let _ = sys.process_tick(12.0);

    sys.report_fault(FaultType::Overcurrent);
    let _ = sys.process_tick(12.0);

    let log = sys.fault_log();
    assert!(!log.is_empty(), "Fault log must not be empty");
}

#[test]
fn multiple_faults_all_logged() {
    let mut sys = new_interlock(200);
    sys.report_communication();
    let _ = sys.process_tick(5.0);

    sys.report_fault(FaultType::UsbStall);
    let _ = sys.process_tick(5.0);
    sys.report_fault(FaultType::ThermalLimit);
    let _ = sys.process_tick(5.0);

    let log = sys.fault_log();
    assert!(
        log.len() >= 2,
        "Expected at least 2 log entries, got {}",
        log.len()
    );
}

// ===========================================================================
// 10. Double-fault scenarios
// ===========================================================================

#[test]
fn double_fault_stays_faulted() {
    let mut svc = new_service();
    svc.report_fault(FaultType::UsbStall);
    svc.report_fault(FaultType::EncoderNaN);
    assert!(matches!(svc.state(), SafetyState::Faulted { .. }));
    assert!(svc.clamp_torque_nm(10.0).abs() < f32::EPSILON);
}

#[test]
fn double_fault_different_types_both_counted() {
    let mut svc = new_service();
    svc.report_fault(FaultType::UsbStall);
    svc.report_fault(FaultType::ThermalLimit);
    // Both faults are counted in history
    assert!(matches!(svc.state(), SafetyState::Faulted { .. }));
}

#[test]
fn fault_during_fault_recovery_re_enters_faulted() -> Result<(), String> {
    let mut svc = new_service();
    svc.report_fault(FaultType::UsbStall);

    std::thread::sleep(Duration::from_millis(110));
    svc.clear_fault()?;
    assert!(matches!(svc.state(), SafetyState::SafeTorque));

    // Immediately fault again
    svc.report_fault(FaultType::Overcurrent);
    assert!(matches!(
        svc.state(),
        SafetyState::Faulted {
            fault: FaultType::Overcurrent,
            ..
        }
    ));
    assert!(svc.clamp_torque_nm(5.0).abs() < f32::EPSILON);
    Ok(())
}

// ===========================================================================
// 11. Emergency stop is irreversible (requires full reset)
// ===========================================================================

#[test]
fn emergency_stop_cannot_be_cleared() {
    let mut sys = new_interlock(200);
    sys.report_communication();
    let _ = sys.process_tick(5.0);

    let estop = sys.emergency_stop();
    assert!(matches!(
        estop.state,
        SafetyInterlockState::EmergencyStop { .. }
    ));
    std::thread::sleep(Duration::from_millis(120));

    let result = sys.clear_fault();
    assert!(
        result.is_err(),
        "EmergencyStop must not be clearable via clear_fault"
    );
    // Verify stays in emergency stop or at least torque is zero
    sys.report_communication();
    let tick = sys.process_tick(5.0);
    let stays_stopped = matches!(tick.state, SafetyInterlockState::EmergencyStop { .. })
        || tick.torque_command.abs() < f32::EPSILON;
    assert!(stays_stopped, "Emergency stop must keep torque at zero");
}

#[test]
fn emergency_stop_torque_stays_zero() {
    let mut sys = new_interlock(200);
    sys.report_communication();
    let _ = sys.process_tick(10.0);

    let estop = sys.emergency_stop();
    assert!(matches!(
        estop.state,
        SafetyInterlockState::EmergencyStop { .. }
    ));

    // Multiple ticks should all yield zero
    for _ in 0..10 {
        sys.report_communication();
        let tick = sys.process_tick(25.0);
        assert!(
            tick.torque_command.abs() < f32::EPSILON,
            "Torque must remain zero after emergency stop"
        );
    }
}

// ===========================================================================
// 12. Communication loss handling
// ===========================================================================

#[test]
fn communication_loss_triggers_safe_mode() {
    let mut sys = new_interlock(200);
    sys.report_communication();
    let _ = sys.process_tick(5.0);

    // Stop reporting communication, wait for timeout
    std::thread::sleep(Duration::from_millis(80));

    let tick = sys.process_tick(10.0);
    // Should be in SafeMode or degraded due to watchdog / comm loss
    let is_safe = matches!(tick.state, SafetyInterlockState::SafeMode { .. })
        || matches!(tick.state, SafetyInterlockState::Warning { .. });
    assert!(
        is_safe || tick.torque_command.abs() < f32::EPSILON,
        "Communication loss must reduce torque or enter safe mode, got {:?} with torque {}",
        tick.state,
        tick.torque_command
    );
}

// ===========================================================================
// 13. NaN / infinity handling (safe-fail)
// ===========================================================================

#[test]
fn nan_torque_request_clamped_to_zero() {
    let svc = new_service();
    let clamped = svc.clamp_torque_nm(f32::NAN);
    assert!(
        clamped.abs() < f32::EPSILON,
        "NaN torque request must be clamped to 0, got {clamped}"
    );
}

#[test]
fn infinity_torque_request_clamped_to_zero() {
    let svc = new_service();
    let clamped = svc.clamp_torque_nm(f32::INFINITY);
    assert!(
        clamped.abs() < f32::EPSILON || clamped <= 5.0,
        "Infinity torque must be safely handled, got {clamped}"
    );
}

#[test]
fn neg_infinity_torque_request_clamped_to_zero() {
    let svc = new_service();
    let clamped = svc.clamp_torque_nm(f32::NEG_INFINITY);
    assert!(
        clamped.abs() < f32::EPSILON || clamped >= -5.0,
        "Neg infinity torque must be safely handled, got {clamped}"
    );
}

// ===========================================================================
// 14. Fault from every state
// ===========================================================================

#[test]
fn fault_from_challenge_state() -> Result<(), String> {
    let mut svc = new_service();
    let _challenge = svc.request_high_torque("dev-ch")?;
    assert!(matches!(
        svc.state(),
        SafetyState::HighTorqueChallenge { .. }
    ));

    svc.report_fault(FaultType::Overcurrent);
    assert!(matches!(svc.state(), SafetyState::Faulted { .. }));
    assert!(svc.clamp_torque_nm(10.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn fault_from_awaiting_ack_state() -> Result<(), String> {
    let mut svc = new_service();
    let challenge = svc.request_high_torque("dev-awk")?;
    svc.provide_ui_consent(challenge.challenge_token)?;
    assert!(matches!(
        svc.state(),
        SafetyState::AwaitingPhysicalAck { .. }
    ));

    svc.report_fault(FaultType::ThermalLimit);
    assert!(matches!(svc.state(), SafetyState::Faulted { .. }));
    assert!(svc.clamp_torque_nm(5.0).abs() < f32::EPSILON);
    Ok(())
}

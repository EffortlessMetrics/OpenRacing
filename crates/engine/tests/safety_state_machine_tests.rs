//! Exhaustive safety state machine transition tests.
//!
//! Tests every valid transition, rejects every invalid transition,
//! and validates state persistence invariants.

use racing_wheel_engine::safety::{
    ButtonCombo, FaultType, InterlockAck, SafetyInterlockState, SafetyInterlockSystem, SafetyService,
    SafetyState, SoftwareWatchdog, WatchdogTimeoutHandler,
};
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn new_service() -> SafetyService {
    SafetyService::with_timeouts(5.0, 25.0, Duration::from_secs(3), Duration::from_secs(2))
}

fn new_interlock(timeout_ms: u32) -> SafetyInterlockSystem {
    let wd = Box::new(SoftwareWatchdog::new(timeout_ms));
    SafetyInterlockSystem::new(wd, 25.0)
}

/// Drive through the full challenge flow, returning Ok on success.
fn activate_high_torque(svc: &mut SafetyService, device: &str) -> Result<(), String> {
    let challenge = svc.request_high_torque(device)?;
    let token = challenge.challenge_token;
    svc.provide_ui_consent(token)?;
    svc.report_combo_start(token)?;
    std::thread::sleep(Duration::from_millis(2050));
    let ack = InterlockAck {
        challenge_token: token,
        device_token: 42,
        combo_completed: ButtonCombo::BothClutchPaddles,
        timestamp: Instant::now(),
    };
    svc.confirm_high_torque(device, ack)
}

// ===========================================================================
// 1. Valid transitions: SafeTorque → HighTorqueChallenge
// ===========================================================================

#[test]
fn transition_safe_to_challenge() -> Result<(), String> {
    let mut svc = new_service();
    assert!(matches!(svc.state(), SafetyState::SafeTorque));

    let challenge = svc.request_high_torque("dev-a")?;
    assert!(challenge.challenge_token != 0);
    assert!(matches!(svc.state(), SafetyState::HighTorqueChallenge { .. }));
    Ok(())
}

// ===========================================================================
// 2. Valid: HighTorqueChallenge → AwaitingPhysicalAck (via UI consent)
// ===========================================================================

#[test]
fn transition_challenge_to_awaiting_ack() -> Result<(), String> {
    let mut svc = new_service();
    let challenge = svc.request_high_torque("dev-b")?;
    let token = challenge.challenge_token;

    svc.provide_ui_consent(token)?;
    assert!(matches!(svc.state(), SafetyState::AwaitingPhysicalAck { .. }));
    Ok(())
}

// ===========================================================================
// 3. Valid: AwaitingPhysicalAck → HighTorqueActive (confirm after hold)
// ===========================================================================

#[test]
fn transition_awaiting_ack_to_high_torque_active() -> Result<(), String> {
    let mut svc = new_service();
    activate_high_torque(&mut svc, "dev-c")?;
    assert!(matches!(svc.state(), SafetyState::HighTorqueActive { .. }));
    Ok(())
}

// ===========================================================================
// 4. Valid: HighTorqueActive → SafeTorque (disable)
// ===========================================================================

#[test]
fn transition_high_torque_to_safe_via_disable() -> Result<(), String> {
    let mut svc = new_service();
    activate_high_torque(&mut svc, "dev-d")?;
    assert!(matches!(svc.state(), SafetyState::HighTorqueActive { .. }));

    svc.disable_high_torque("dev-d")?;
    assert!(matches!(svc.state(), SafetyState::SafeTorque));
    Ok(())
}

// ===========================================================================
// 5. Valid: Any → Faulted (report_fault from each state)
// ===========================================================================

#[test]
fn transition_safe_torque_to_faulted() {
    let mut svc = new_service();
    svc.report_fault(FaultType::Overcurrent);
    assert!(matches!(svc.state(), SafetyState::Faulted { .. }));
}

#[test]
fn transition_challenge_to_faulted() -> Result<(), String> {
    let mut svc = new_service();
    let _c = svc.request_high_torque("dev-e")?;
    svc.report_fault(FaultType::UsbStall);
    assert!(matches!(svc.state(), SafetyState::Faulted { .. }));
    Ok(())
}

#[test]
fn transition_awaiting_ack_to_faulted() -> Result<(), String> {
    let mut svc = new_service();
    let c = svc.request_high_torque("dev-f")?;
    svc.provide_ui_consent(c.challenge_token)?;
    svc.report_fault(FaultType::EncoderNaN);
    assert!(matches!(svc.state(), SafetyState::Faulted { .. }));
    Ok(())
}

#[test]
fn transition_high_torque_active_to_faulted() -> Result<(), String> {
    let mut svc = new_service();
    activate_high_torque(&mut svc, "dev-g")?;
    svc.report_fault(FaultType::ThermalLimit);
    assert!(matches!(svc.state(), SafetyState::Faulted { .. }));
    Ok(())
}

// ===========================================================================
// 6. Valid: Faulted → SafeTorque (clear after cooldown)
// ===========================================================================

#[test]
fn transition_faulted_to_safe_after_cooldown() -> Result<(), String> {
    let mut svc = new_service();
    svc.report_fault(FaultType::PipelineFault);
    std::thread::sleep(Duration::from_millis(110));
    svc.clear_fault()?;
    assert!(matches!(svc.state(), SafetyState::SafeTorque));
    Ok(())
}

// ===========================================================================
// 7. Valid: HighTorqueChallenge → SafeTorque (cancel)
// ===========================================================================

#[test]
fn transition_challenge_to_safe_via_cancel() -> Result<(), String> {
    let mut svc = new_service();
    let _c = svc.request_high_torque("dev-h")?;
    svc.cancel_challenge()?;
    assert!(matches!(svc.state(), SafetyState::SafeTorque));
    Ok(())
}

// ===========================================================================
// 8. Invalid: SafeTorque → HighTorqueActive (skip challenge)
// ===========================================================================

#[test]
fn invalid_safe_to_high_torque_directly() {
    let mut svc = new_service();
    let ack = InterlockAck {
        challenge_token: 12345,
        device_token: 42,
        combo_completed: ButtonCombo::BothClutchPaddles,
        timestamp: Instant::now(),
    };
    let result = svc.confirm_high_torque("dev-bad", ack);
    assert!(result.is_err(), "Direct safe→high-torque must be rejected");
    assert!(matches!(svc.state(), SafetyState::SafeTorque));
}

// ===========================================================================
// 9. Invalid: consent with wrong token
// ===========================================================================

#[test]
fn invalid_consent_wrong_token() -> Result<(), String> {
    let mut svc = new_service();
    let _c = svc.request_high_torque("dev-i")?;
    let result = svc.provide_ui_consent(0xDEAD_BEEF);
    assert!(result.is_err(), "Wrong token must be rejected");
    // State unchanged
    assert!(matches!(svc.state(), SafetyState::HighTorqueChallenge { .. }));
    Ok(())
}

// ===========================================================================
// 10. Invalid: combo start with wrong token
// ===========================================================================

#[test]
fn invalid_combo_start_wrong_token() -> Result<(), String> {
    let mut svc = new_service();
    let c = svc.request_high_torque("dev-j")?;
    svc.provide_ui_consent(c.challenge_token)?;
    let result = svc.report_combo_start(0xCAFE);
    assert!(result.is_err(), "Wrong token for combo start must be rejected");
    Ok(())
}

// ===========================================================================
// 11. Invalid: confirm with insufficient hold
// ===========================================================================

#[test]
fn invalid_confirm_insufficient_hold() -> Result<(), String> {
    let mut svc = new_service();
    let c = svc.request_high_torque("dev-k")?;
    let token = c.challenge_token;
    svc.provide_ui_consent(token)?;
    svc.report_combo_start(token)?;
    // No wait
    let ack = InterlockAck {
        challenge_token: token,
        device_token: 42,
        combo_completed: ButtonCombo::BothClutchPaddles,
        timestamp: Instant::now(),
    };
    let result = svc.confirm_high_torque("dev-k", ack);
    assert!(result.is_err(), "Confirm without 2s hold must fail");
    Ok(())
}

// ===========================================================================
// 12. Invalid: clear_fault when not faulted
// ===========================================================================

#[test]
fn invalid_clear_fault_not_faulted() {
    let mut svc = new_service();
    let result = svc.clear_fault();
    assert!(result.is_err());
}

// ===========================================================================
// 13. Invalid: clear_fault before cooldown
// ===========================================================================

#[test]
fn invalid_clear_fault_before_cooldown() {
    let mut svc = new_service();
    svc.report_fault(FaultType::Overcurrent);
    let result = svc.clear_fault();
    assert!(result.is_err(), "clear_fault before 100ms must fail");
}

// ===========================================================================
// 14. Invalid: disable_high_torque from SafeTorque
// ===========================================================================

#[test]
fn invalid_disable_high_torque_from_safe() {
    let mut svc = new_service();
    let result = svc.disable_high_torque("dev-l");
    assert!(result.is_err(), "disable_high_torque from SafeTorque must fail");
}

// ===========================================================================
// 15. Invalid: cancel_challenge from SafeTorque
// ===========================================================================

#[test]
fn invalid_cancel_challenge_from_safe() {
    let mut svc = new_service();
    let result = svc.cancel_challenge();
    assert!(result.is_err(), "cancel_challenge from SafeTorque must fail");
}

// ===========================================================================
// 16. Torque limits reflect state correctly
// ===========================================================================

#[test]
fn torque_limit_safe_state() {
    let svc = new_service();
    let clamped = svc.clamp_torque_nm(10.0);
    assert!(clamped <= 5.0, "Safe torque must not exceed 5 Nm, got {clamped}");
}

#[test]
fn torque_limit_high_torque_state() -> Result<(), String> {
    let mut svc = new_service();
    activate_high_torque(&mut svc, "dev-m")?;
    let clamped = svc.clamp_torque_nm(20.0);
    assert!((clamped - 20.0).abs() < f32::EPSILON, "High torque should allow 20 Nm, got {clamped}");
    Ok(())
}

#[test]
fn torque_limit_faulted_state() {
    let mut svc = new_service();
    svc.report_fault(FaultType::Overcurrent);
    let clamped = svc.clamp_torque_nm(5.0);
    assert!(clamped.abs() < f32::EPSILON, "Faulted torque must be zero");
}

// ===========================================================================
// 17. SafetyInterlockSystem state machine: Normal → SafeMode → Normal
// ===========================================================================

#[test]
fn interlock_normal_to_safe_mode_via_fault() {
    let mut sys = new_interlock(200);
    sys.report_communication();
    let tick = sys.process_tick(5.0);
    assert!(matches!(tick.state, SafetyInterlockState::Normal));

    sys.report_fault(FaultType::EncoderNaN);
    let tick = sys.process_tick(5.0);
    assert!(matches!(tick.state, SafetyInterlockState::SafeMode { .. }));
}

#[test]
fn interlock_safe_mode_to_normal_after_clear() -> Result<(), String> {
    let mut sys = new_interlock(200);
    sys.report_communication();
    let _ = sys.process_tick(5.0);

    sys.report_fault(FaultType::PipelineFault);
    let _ = sys.process_tick(5.0);

    std::thread::sleep(Duration::from_millis(120));
    sys.report_communication();
    sys.clear_fault()?;

    sys.report_communication();
    let tick = sys.process_tick(5.0);
    assert!(
        matches!(tick.state, SafetyInterlockState::Normal),
        "Expected Normal, got {:?}",
        tick.state
    );
    Ok(())
}

// ===========================================================================
// 18. SafetyInterlockSystem: Normal → EmergencyStop (irreversible)
// ===========================================================================

#[test]
fn interlock_normal_to_emergency_stop() {
    let mut sys = new_interlock(200);
    sys.report_communication();
    let _ = sys.process_tick(5.0);

    let result = sys.emergency_stop();
    assert!(matches!(result.state, SafetyInterlockState::EmergencyStop { .. }));
    assert!(result.torque_command.abs() < f32::EPSILON);
}

#[test]
fn interlock_emergency_stop_is_irreversible() {
    let mut sys = new_interlock(200);
    sys.report_communication();
    let _ = sys.process_tick(5.0);

    let estop = sys.emergency_stop();
    assert!(matches!(estop.state, SafetyInterlockState::EmergencyStop { .. }));
    std::thread::sleep(Duration::from_millis(120));

    let clear_result = sys.clear_fault();
    assert!(clear_result.is_err(), "Emergency stop must not be clearable");

    sys.report_communication();
    let tick = sys.process_tick(5.0);
    // After emergency stop the system must not return to Normal
    let stays_stopped = matches!(tick.state, SafetyInterlockState::EmergencyStop { .. })
        || tick.torque_command.abs() < f32::EPSILON;
    assert!(
        stays_stopped,
        "Emergency stop must keep torque at zero, got {:?} with torque {}",
        tick.state, tick.torque_command
    );
}

// ===========================================================================
// 19. Watchdog timeout handler reset
// ===========================================================================

#[test]
fn watchdog_handler_reset_clears_state() {
    let mut handler = WatchdogTimeoutHandler::new();
    let _resp = handler.handle_timeout(10.0);
    assert!(handler.is_timeout_triggered());

    handler.reset();
    assert!(!handler.is_timeout_triggered());
    assert!(handler.timeout_timestamp().is_none());
}

// ===========================================================================
// 20. Round-trip: fault → clear → same fault again
// ===========================================================================

#[test]
fn round_trip_fault_clear_refault() -> Result<(), String> {
    let mut svc = new_service();

    svc.report_fault(FaultType::UsbStall);
    assert!(matches!(svc.state(), SafetyState::Faulted { fault: FaultType::UsbStall, .. }));

    std::thread::sleep(Duration::from_millis(110));
    svc.clear_fault()?;
    assert!(matches!(svc.state(), SafetyState::SafeTorque));

    // Same fault again
    svc.report_fault(FaultType::UsbStall);
    assert!(matches!(svc.state(), SafetyState::Faulted { fault: FaultType::UsbStall, .. }));
    assert!(svc.clamp_torque_nm(5.0).abs() < f32::EPSILON);
    Ok(())
}

// ===========================================================================
// 21. Default constructor yields SafeTorque
// ===========================================================================

#[test]
fn default_service_starts_in_safe_torque() {
    let svc = SafetyService::default();
    assert!(matches!(svc.state(), SafetyState::SafeTorque));
}

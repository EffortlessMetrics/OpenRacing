//! Deep tests for the engine safety subsystem.
//!
//! Covers:
//! - Safety state machine transitions (all edges)
//! - Torque limiting under all conditions
//! - Fault detection and response timing (ADR-0006)
//! - E-stop behavior
//! - Safety interlock chain (challenge-response)
//! - Multi-layer safety system (watchdog + interlock + torque clamping)

use racing_wheel_engine::safety::{
    ButtonCombo, FaultType, HardwareWatchdog, InterlockAck, SafetyInterlockState,
    SafetyInterlockSystem, SafetyService, SafetyState, SafetyTrigger,
    SoftwareWatchdog, TorqueLimit, WatchdogError, WatchdogTimeoutHandler,
};
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn create_test_service() -> SafetyService {
    SafetyService::with_timeouts(
        5.0,                    // max_safe_torque_nm
        25.0,                   // max_high_torque_nm
        Duration::from_secs(3), // hands_off_timeout
        Duration::from_secs(2), // combo_hold_duration
    )
}

fn create_interlock_system(timeout_ms: u32) -> SafetyInterlockSystem {
    let watchdog = Box::new(SoftwareWatchdog::new(timeout_ms));
    SafetyInterlockSystem::new(watchdog, 25.0)
}

/// Drive a SafetyService through the full high-torque activation flow.
fn activate_high_torque(
    service: &mut SafetyService,
    device: &str,
) -> Result<(), Box<dyn std::error::Error>> {
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
    service.confirm_high_torque(device, ack)?;
    Ok(())
}

// =========================================================================
// 1. Safety state machine transitions
// =========================================================================

#[test]
fn safe_torque_is_default_state() {
    let service = create_test_service();
    assert_eq!(service.state(), &SafetyState::SafeTorque);
}

#[test]
fn safe_torque_to_high_torque_challenge() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_service();
    let challenge = service.request_high_torque("dev")?;
    assert!(matches!(
        service.state(),
        SafetyState::HighTorqueChallenge { .. }
    ));
    assert!(challenge.challenge_token != 0);
    Ok(())
}

#[test]
fn high_torque_challenge_to_awaiting_physical_ack() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_service();
    let challenge = service.request_high_torque("dev")?;
    service.provide_ui_consent(challenge.challenge_token)?;
    assert!(matches!(
        service.state(),
        SafetyState::AwaitingPhysicalAck { .. }
    ));
    Ok(())
}

#[test]
fn awaiting_ack_to_high_torque_active() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_service();
    activate_high_torque(&mut service, "dev")?;
    assert!(matches!(
        service.state(),
        SafetyState::HighTorqueActive { .. }
    ));
    Ok(())
}

#[test]
fn high_torque_active_to_faulted() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_service();
    activate_high_torque(&mut service, "dev")?;
    service.report_fault(FaultType::ThermalLimit);
    assert!(matches!(service.state(), SafetyState::Faulted { .. }));
    Ok(())
}

#[test]
fn faulted_to_safe_torque_after_minimum_duration() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_service();
    service.report_fault(FaultType::UsbStall);
    std::thread::sleep(Duration::from_millis(150));
    service.clear_fault()?;
    assert_eq!(service.state(), &SafetyState::SafeTorque);
    Ok(())
}

#[test]
fn high_torque_challenge_to_safe_torque_via_cancel() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_service();
    service.request_high_torque("dev")?;
    service.cancel_challenge()?;
    assert_eq!(service.state(), &SafetyState::SafeTorque);
    assert!(service.get_active_challenge().is_none());
    Ok(())
}

#[test]
fn awaiting_ack_to_safe_torque_via_cancel() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_service();
    let challenge = service.request_high_torque("dev")?;
    service.provide_ui_consent(challenge.challenge_token)?;
    service.cancel_challenge()?;
    assert_eq!(service.state(), &SafetyState::SafeTorque);
    Ok(())
}

#[test]
fn high_torque_active_to_safe_torque_via_disable() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_service();
    activate_high_torque(&mut service, "dev")?;
    service.disable_high_torque("dev")?;
    assert_eq!(service.state(), &SafetyState::SafeTorque);
    assert!(!service.has_valid_token("dev"));
    Ok(())
}

#[test]
fn full_state_machine_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_service();

    // SafeTorque → HighTorqueChallenge → AwaitingPhysicalAck → HighTorqueActive
    activate_high_torque(&mut service, "dev")?;
    assert!(matches!(
        service.state(),
        SafetyState::HighTorqueActive { .. }
    ));

    // HighTorqueActive → Faulted
    service.report_fault(FaultType::Overcurrent);
    assert!(matches!(service.state(), SafetyState::Faulted { .. }));

    // Faulted → SafeTorque
    std::thread::sleep(Duration::from_millis(150));
    service.clear_fault()?;
    assert_eq!(service.state(), &SafetyState::SafeTorque);
    Ok(())
}

// =========================================================================
// 2. Torque limiting under all conditions
// =========================================================================

#[test]
fn torque_clamped_to_safe_limit_in_safe_torque() {
    let service = create_test_service();
    assert_eq!(service.clamp_torque_nm(10.0), 5.0);
    assert_eq!(service.clamp_torque_nm(-10.0), -5.0);
    assert_eq!(service.clamp_torque_nm(3.0), 3.0);
    assert_eq!(service.clamp_torque_nm(-3.0), -3.0);
}

#[test]
fn torque_clamped_to_safe_limit_during_challenge() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_service();
    service.request_high_torque("dev")?;
    assert_eq!(service.clamp_torque_nm(20.0), 5.0);
    assert_eq!(service.max_torque_nm(), 5.0);
    Ok(())
}

#[test]
fn torque_clamped_to_safe_limit_during_awaiting_ack() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_service();
    let challenge = service.request_high_torque("dev")?;
    service.provide_ui_consent(challenge.challenge_token)?;
    assert_eq!(service.clamp_torque_nm(20.0), 5.0);
    assert_eq!(service.max_torque_nm(), 5.0);
    Ok(())
}

#[test]
fn torque_allowed_to_high_limit_when_active() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_service();
    activate_high_torque(&mut service, "dev")?;
    assert_eq!(service.max_torque_nm(), 25.0);
    assert_eq!(service.clamp_torque_nm(20.0), 20.0);
    assert_eq!(service.clamp_torque_nm(30.0), 25.0);
    Ok(())
}

#[test]
fn torque_zero_in_faulted_state() {
    let mut service = create_test_service();
    service.report_fault(FaultType::EncoderNaN);
    assert_eq!(service.clamp_torque_nm(25.0), 0.0);
    assert_eq!(service.clamp_torque_nm(-25.0), 0.0);
    assert_eq!(service.max_torque_nm(), 0.0);
}

#[test]
fn nan_torque_request_sanitized_to_zero() {
    let service = create_test_service();
    assert_eq!(service.clamp_torque_nm(f32::NAN), 0.0);
}

#[test]
fn infinity_torque_request_sanitized_to_zero() {
    let service = create_test_service();
    assert_eq!(service.clamp_torque_nm(f32::INFINITY), 0.0);
    assert_eq!(service.clamp_torque_nm(f32::NEG_INFINITY), 0.0);
}

#[test]
fn negative_zero_torque_treated_as_zero() {
    let service = create_test_service();
    assert_eq!(service.clamp_torque_nm(-0.0), 0.0);
}

#[test]
fn torque_limit_get_max_torque_respects_high_torque_flag() -> Result<(), Box<dyn std::error::Error>>
{
    let mut service = create_test_service();
    // In SafeTorque, the flag is ignored
    assert_eq!(service.get_max_torque(true).value(), 5.0);
    assert_eq!(service.get_max_torque(false).value(), 5.0);

    // In HighTorqueActive, the flag is respected
    activate_high_torque(&mut service, "dev")?;
    assert_eq!(service.get_max_torque(true).value(), 25.0);
    assert_eq!(service.get_max_torque(false).value(), 5.0);
    Ok(())
}

#[test]
fn faulted_get_max_torque_ignores_high_torque_flag() {
    let mut service = create_test_service();
    service.report_fault(FaultType::SafetyInterlockViolation);
    assert_eq!(service.get_max_torque(true).value(), 0.0);
    assert_eq!(service.get_max_torque(false).value(), 0.0);
}

#[test]
fn torque_limit_struct_clamp_logs_violations() {
    let mut limit = TorqueLimit::new(25.0, 5.0);
    let (clamped, was_clamped) = limit.clamp(30.0);
    assert_eq!(clamped, 25.0);
    assert!(was_clamped);
    assert_eq!(limit.violation_count, 1);

    let (clamped2, was_clamped2) = limit.clamp(10.0);
    assert_eq!(clamped2, 10.0);
    assert!(!was_clamped2);
    assert_eq!(limit.violation_count, 1); // no new violation
}

#[test]
fn torque_limit_negative_clamped_symmetrically() {
    let mut limit = TorqueLimit::new(20.0, 4.0);
    let (clamped, _) = limit.clamp(-25.0);
    assert_eq!(clamped, -20.0);
}

// =========================================================================
// 3. Fault detection and response timing (ADR-0006)
// =========================================================================

#[test]
fn fault_detection_within_10ms() {
    let mut service = create_test_service();
    let before = Instant::now();
    service.report_fault(FaultType::Overcurrent);
    let elapsed = before.elapsed();
    assert!(
        elapsed < Duration::from_millis(10),
        "Fault detection took {elapsed:?}, exceeds 10ms budget"
    );
    assert!(matches!(service.state(), SafetyState::Faulted { .. }));
}

#[test]
fn fault_to_zero_torque_within_50ms() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_service();
    activate_high_torque(&mut service, "dev")?;
    assert_eq!(service.max_torque_nm(), 25.0);

    let before = Instant::now();
    service.report_fault(FaultType::UsbStall);
    let clamped = service.clamp_torque_nm(25.0);
    let elapsed = before.elapsed();

    assert_eq!(clamped, 0.0);
    assert!(
        elapsed < Duration::from_millis(50),
        "Fault→zero took {elapsed:?}, exceeds 50ms budget"
    );
    Ok(())
}

#[test]
fn all_fault_types_transition_and_zero_torque() {
    let all_faults = [
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
    for fault in all_faults {
        let mut service = create_test_service();
        service.report_fault(fault);
        assert!(
            matches!(service.state(), SafetyState::Faulted { .. }),
            "Fault {fault:?} did not transition"
        );
        assert_eq!(
            service.clamp_torque_nm(100.0),
            0.0,
            "Fault {fault:?} did not zero torque"
        );
    }
}

#[test]
fn clear_fault_too_soon_is_rejected() {
    let mut service = create_test_service();
    service.report_fault(FaultType::Overcurrent);
    let result = service.clear_fault();
    assert!(result.is_err());
    assert!(matches!(service.state(), SafetyState::Faulted { .. }));
}

#[test]
fn clear_fault_from_non_faulted_state_is_rejected() {
    let mut service = create_test_service();
    assert!(service.clear_fault().is_err());
}

#[test]
fn multi_fault_last_type_wins_torque_stays_zero() {
    let mut service = create_test_service();
    service.report_fault(FaultType::UsbStall);
    service.report_fault(FaultType::ThermalLimit);
    match service.state() {
        SafetyState::Faulted { fault, .. } => assert_eq!(*fault, FaultType::ThermalLimit),
        other => panic!("Expected Faulted, got {other:?}"),
    }
    assert_eq!(service.max_torque_nm(), 0.0);
}

#[test]
fn rapid_fault_clear_cycling_preserves_invariants() {
    let mut service = create_test_service();
    for _ in 0..10 {
        service.report_fault(FaultType::EncoderNaN);
        assert_eq!(service.clamp_torque_nm(10.0), 0.0);
        std::thread::sleep(Duration::from_millis(110));
        let result = service.clear_fault();
        assert!(result.is_ok());
        assert_eq!(service.state(), &SafetyState::SafeTorque);
    }
}

#[test]
fn fault_during_challenge_aborts_challenge() {
    let mut service = create_test_service();
    let _ = service.request_high_torque("dev");
    service.report_fault(FaultType::UsbStall);
    assert!(matches!(service.state(), SafetyState::Faulted { .. }));
    assert_eq!(service.clamp_torque_nm(10.0), 0.0);
}

// =========================================================================
// 4. E-stop behavior
// =========================================================================

#[test]
fn emergency_stop_zeros_torque_immediately() {
    let mut interlock = create_interlock_system(100);
    let result = interlock.emergency_stop();
    assert_eq!(result.torque_command, 0.0);
    assert!(result.fault_occurred);
    assert!(matches!(
        result.state,
        SafetyInterlockState::EmergencyStop { .. }
    ));
}

#[test]
fn emergency_stop_within_timing_budget() {
    let mut interlock = create_interlock_system(100);
    let before = Instant::now();
    let result = interlock.emergency_stop();
    let elapsed = before.elapsed();
    assert_eq!(result.torque_command, 0.0);
    assert!(
        elapsed < Duration::from_millis(1),
        "E-stop took {elapsed:?}, exceeds 1ms budget"
    );
}

#[test]
fn emergency_stop_cannot_be_cleared_with_clear_fault() {
    let mut interlock = create_interlock_system(100);
    interlock.emergency_stop();
    let result = interlock.clear_fault();
    assert!(result.is_err());
    assert!(matches!(
        interlock.state(),
        SafetyInterlockState::EmergencyStop { .. }
    ));
}

#[test]
fn emergency_stop_requires_manual_reset() -> Result<(), WatchdogError> {
    let mut interlock = create_interlock_system(100);
    interlock.emergency_stop();
    interlock.reset()?;
    assert_eq!(interlock.state(), &SafetyInterlockState::Normal);
    Ok(())
}

#[test]
fn emergency_stop_during_normal_operation() {
    let mut interlock = create_interlock_system(100);
    // First, arm and process a normal tick to establish baseline
    let _ = interlock.arm();
    interlock.report_communication();
    let tick = interlock.process_tick(10.0);
    assert!(!tick.fault_occurred);

    // Now emergency stop
    let result = interlock.emergency_stop();
    assert_eq!(result.torque_command, 0.0);
    assert!(result.fault_occurred);
}

#[test]
fn emergency_stop_logs_fault() {
    let mut interlock = create_interlock_system(100);
    interlock.emergency_stop();
    let log = interlock.fault_log();
    assert!(!log.is_empty());
    let last = &log[log.len() - 1];
    assert_eq!(last.trigger, SafetyTrigger::EmergencyStopCommand);
}

// =========================================================================
// 5. Safety interlock chain
// =========================================================================

#[test]
fn challenge_token_must_match_at_every_step() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_service();
    let challenge = service.request_high_torque("dev")?;
    let bad = challenge.challenge_token.wrapping_add(1);

    // Wrong token for UI consent
    assert!(service.provide_ui_consent(bad).is_err());

    // Correct UI consent
    service.provide_ui_consent(challenge.challenge_token)?;

    // Wrong token for combo start
    assert!(service.report_combo_start(bad).is_err());

    // Correct combo start
    service.report_combo_start(challenge.challenge_token)?;
    std::thread::sleep(Duration::from_millis(2100));

    // Wrong token for confirmation
    let bad_ack = InterlockAck {
        challenge_token: bad,
        device_token: 1,
        combo_completed: ButtonCombo::BothClutchPaddles,
        timestamp: Instant::now(),
    };
    assert!(service.confirm_high_torque("dev", bad_ack).is_err());

    // Should still be awaiting — not promoted
    assert!(matches!(
        service.state(),
        SafetyState::AwaitingPhysicalAck { .. }
    ));
    Ok(())
}

#[test]
fn insufficient_combo_hold_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_service();
    let challenge = service.request_high_torque("dev")?;
    service.provide_ui_consent(challenge.challenge_token)?;
    service.report_combo_start(challenge.challenge_token)?;

    // Only wait 500ms (need 2000ms)
    std::thread::sleep(Duration::from_millis(500));

    let ack = InterlockAck {
        challenge_token: challenge.challenge_token,
        device_token: 1,
        combo_completed: ButtonCombo::BothClutchPaddles,
        timestamp: Instant::now(),
    };
    let result = service.confirm_high_torque("dev", ack);
    assert!(result.is_err());

    // Still awaiting, not promoted
    assert!(matches!(
        service.state(),
        SafetyState::AwaitingPhysicalAck { .. }
    ));
    Ok(())
}

#[test]
fn confirm_without_combo_start_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_service();
    let challenge = service.request_high_torque("dev")?;
    service.provide_ui_consent(challenge.challenge_token)?;
    // No report_combo_start

    let ack = InterlockAck {
        challenge_token: challenge.challenge_token,
        device_token: 1,
        combo_completed: ButtonCombo::BothClutchPaddles,
        timestamp: Instant::now(),
    };
    let result = service.confirm_high_torque("dev", ack);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn cannot_request_high_torque_with_active_fault_history() {
    let mut service = create_test_service();
    // Report and clear a fault so the count is non-zero
    service.report_fault(FaultType::TimingViolation);
    std::thread::sleep(Duration::from_millis(110));
    let _ = service.clear_fault();
    let result = service.request_high_torque("dev");
    assert!(result.is_err());
}

#[test]
fn cannot_request_high_torque_while_faulted() {
    let mut service = create_test_service();
    service.report_fault(FaultType::ThermalLimit);
    let result = service.request_high_torque("dev");
    assert!(result.is_err());
}

#[test]
fn cannot_request_high_torque_when_already_active() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_service();
    activate_high_torque(&mut service, "dev")?;
    let result = service.request_high_torque("dev");
    assert!(result.is_err());
    Ok(())
}

#[test]
fn consent_requirements_populated() {
    let service = create_test_service();
    let reqs = service.get_consent_requirements();
    assert_eq!(reqs.max_torque_nm, 25.0);
    assert!(reqs.requires_explicit_consent);
    assert!(!reqs.warnings.is_empty());
    assert!(!reqs.disclaimers.is_empty());
}

// =========================================================================
// 6. Challenge-response protocol
// =========================================================================

#[test]
fn challenge_time_remaining_decreases() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_service();
    service.request_high_torque("dev")?;
    let first = service
        .get_challenge_time_remaining()
        .ok_or("expected time")?;

    std::thread::sleep(Duration::from_millis(200));

    let second = service
        .get_challenge_time_remaining()
        .ok_or("expected time")?;
    assert!(second < first);
    Ok(())
}

#[test]
fn no_challenge_time_remaining_when_no_challenge() {
    let service = create_test_service();
    assert!(service.get_challenge_time_remaining().is_none());
}

#[test]
fn active_challenge_tracks_ui_consent() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_service();
    let challenge = service.request_high_torque("dev")?;

    let active = service
        .get_active_challenge()
        .ok_or("expected active challenge")?;
    assert!(!active.ui_consent_given);

    service.provide_ui_consent(challenge.challenge_token)?;

    let active = service
        .get_active_challenge()
        .ok_or("expected active challenge")?;
    assert!(active.ui_consent_given);
    Ok(())
}

#[test]
fn device_token_persists_after_high_torque_activation() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_service();
    activate_high_torque(&mut service, "dev1")?;
    assert!(service.has_valid_token("dev1"));
    assert!(!service.has_valid_token("dev2"));
    Ok(())
}

#[test]
fn device_token_cleared_on_disable() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_service();
    activate_high_torque(&mut service, "dev")?;
    assert!(service.has_valid_token("dev"));
    service.disable_high_torque("dev")?;
    assert!(!service.has_valid_token("dev"));
    Ok(())
}

#[test]
fn cancel_from_non_challenge_state_errors() {
    let mut service = create_test_service();
    assert!(service.cancel_challenge().is_err());
}

#[test]
fn disable_from_non_active_state_errors() {
    let mut service = create_test_service();
    assert!(service.disable_high_torque("dev").is_err());
}

// =========================================================================
// 7. Multi-layer safety system (interlock system integration)
// =========================================================================

#[test]
fn interlock_system_normal_tick_passes_torque() -> Result<(), WatchdogError> {
    let mut sys = create_interlock_system(100);
    sys.arm()?;
    sys.report_communication();
    let result = sys.process_tick(10.0);
    assert!(!result.fault_occurred);
    assert!(result.torque_command.abs() <= 25.0);
    assert_eq!(result.state, SafetyInterlockState::Normal);
    Ok(())
}

#[test]
fn interlock_system_watchdog_timeout_zeros_torque() -> Result<(), WatchdogError> {
    let mut sys = create_interlock_system(10); // 10ms timeout
    sys.arm()?;
    sys.report_communication();
    let _tick = sys.process_tick(10.0);

    std::thread::sleep(Duration::from_millis(15));
    let result = sys.process_tick(10.0);
    assert!(result.fault_occurred);
    assert_eq!(result.torque_command, 0.0);
    assert!(matches!(
        result.state,
        SafetyInterlockState::SafeMode { .. }
    ));
    Ok(())
}

#[test]
fn interlock_system_communication_loss_zeros_torque() {
    let mut sys = create_interlock_system(500);
    // Report communication, then let it go stale
    sys.report_communication();
    std::thread::sleep(Duration::from_millis(60));
    let result = sys.process_tick(10.0);
    assert!(result.fault_occurred);
    assert_eq!(result.torque_command, 0.0);
}

#[test]
fn interlock_system_torque_clamped_in_normal() -> Result<(), WatchdogError> {
    let mut sys = create_interlock_system(100);
    sys.arm()?;
    sys.report_communication();
    let result = sys.process_tick(50.0); // request exceeds 25Nm
    assert!(result.torque_command <= 25.0);
    Ok(())
}

#[test]
fn interlock_system_safe_mode_uses_reduced_limit() -> Result<(), WatchdogError> {
    let mut sys = create_interlock_system(100);
    sys.report_fault(FaultType::ThermalLimit);
    sys.report_communication();
    let result = sys.process_tick(20.0);
    // safe_mode_torque_nm = 25.0 * 0.2 = 5.0
    assert!(result.torque_command <= 5.0);
    Ok(())
}

#[test]
fn interlock_system_clear_fault_after_minimum_duration() {
    let mut sys = create_interlock_system(100);
    sys.report_fault(FaultType::UsbStall);
    assert!(sys.clear_fault().is_err()); // too soon
    std::thread::sleep(Duration::from_millis(110));
    assert!(sys.clear_fault().is_ok());
    assert_eq!(sys.state(), &SafetyInterlockState::Normal);
}

#[test]
fn interlock_system_arm_disarm_cycle() -> Result<(), WatchdogError> {
    let mut sys = create_interlock_system(100);
    assert!(!sys.is_watchdog_armed());
    sys.arm()?;
    assert!(sys.is_watchdog_armed());
    sys.disarm()?;
    assert!(!sys.is_watchdog_armed());
    Ok(())
}

#[test]
fn interlock_system_fault_log_populated_on_faults() {
    let mut sys = create_interlock_system(100);
    assert!(sys.fault_log().is_empty());
    sys.report_fault(FaultType::EncoderNaN);
    assert!(!sys.fault_log().is_empty());
    let entry = &sys.fault_log()[0];
    assert_eq!(entry.trigger, SafetyTrigger::FaultDetected(FaultType::EncoderNaN));
}

#[test]
fn interlock_system_reset_restores_normal() -> Result<(), WatchdogError> {
    let mut sys = create_interlock_system(100);
    sys.emergency_stop();
    sys.reset()?;
    assert_eq!(sys.state(), &SafetyInterlockState::Normal);
    assert!(!sys.is_watchdog_armed());
    Ok(())
}

#[test]
fn watchdog_timeout_handler_zeros_torque_within_1ms() {
    let mut handler = WatchdogTimeoutHandler::new();
    let response = handler.handle_timeout(15.0);
    assert_eq!(response.torque_command, 0.0);
    assert_eq!(response.previous_torque, 15.0);
    assert!(response.within_budget);
    assert!(handler.is_timeout_triggered());
}

#[test]
fn watchdog_timeout_handler_reset_clears_state() {
    let mut handler = WatchdogTimeoutHandler::new();
    handler.handle_timeout(10.0);
    assert!(handler.is_timeout_triggered());
    handler.reset();
    assert!(!handler.is_timeout_triggered());
    assert!(handler.timeout_timestamp().is_none());
}

#[test]
fn software_watchdog_feed_before_arm_errors() {
    let mut wd = SoftwareWatchdog::new(100);
    assert_eq!(wd.feed(), Err(WatchdogError::NotArmed));
}

#[test]
fn software_watchdog_double_arm_errors() -> Result<(), WatchdogError> {
    let mut wd = SoftwareWatchdog::new(100);
    wd.arm()?;
    assert_eq!(wd.arm(), Err(WatchdogError::AlreadyArmed));
    Ok(())
}

#[test]
fn software_watchdog_disarm_when_not_armed_errors() {
    let mut wd = SoftwareWatchdog::new(100);
    assert_eq!(wd.disarm(), Err(WatchdogError::NotArmed));
}

#[test]
fn software_watchdog_feed_after_timeout_errors() -> Result<(), WatchdogError> {
    let mut wd = SoftwareWatchdog::new(10);
    wd.arm()?;
    wd.feed()?;
    std::thread::sleep(Duration::from_millis(15));
    assert!(wd.has_timed_out());
    assert_eq!(wd.feed(), Err(WatchdogError::TimedOut));
    Ok(())
}

#[test]
fn software_watchdog_trigger_safe_state_marks_timeout() -> Result<(), WatchdogError> {
    let mut wd = SoftwareWatchdog::new(100);
    wd.arm()?;
    wd.trigger_safe_state()?;
    assert!(wd.is_safe_state_triggered());
    assert!(wd.has_timed_out());
    Ok(())
}

#[test]
fn software_watchdog_reset_clears_everything() -> Result<(), WatchdogError> {
    let mut wd = SoftwareWatchdog::new(10);
    wd.arm()?;
    std::thread::sleep(Duration::from_millis(15));
    assert!(wd.has_timed_out());
    wd.reset()?;
    assert!(!wd.is_armed());
    assert!(!wd.has_timed_out());
    Ok(())
}

#[test]
fn software_watchdog_time_since_last_feed_increases() -> Result<(), WatchdogError> {
    let mut wd = SoftwareWatchdog::new(500);
    wd.arm()?;
    wd.feed()?;
    std::thread::sleep(Duration::from_millis(50));
    let elapsed = wd.time_since_last_feed();
    assert!(elapsed >= Duration::from_millis(40));
    Ok(())
}

#[test]
fn interlock_system_emergency_stop_in_every_state() -> Result<(), WatchdogError> {
    // Normal → EmergencyStop
    let mut sys = create_interlock_system(100);
    let result = sys.emergency_stop();
    assert_eq!(result.torque_command, 0.0);
    assert!(matches!(
        sys.state(),
        SafetyInterlockState::EmergencyStop { .. }
    ));

    // SafeMode → EmergencyStop
    let mut sys2 = create_interlock_system(100);
    sys2.report_fault(FaultType::UsbStall);
    let result = sys2.emergency_stop();
    assert_eq!(result.torque_command, 0.0);
    assert!(matches!(
        sys2.state(),
        SafetyInterlockState::EmergencyStop { .. }
    ));
    Ok(())
}

#[test]
fn hands_off_timeout_only_in_high_torque_active() {
    let mut service = create_test_service();
    // In SafeTorque, no error on hands-off
    let result = service.update_hands_on_status(false);
    assert!(result.is_ok());
    assert_eq!(service.state(), &SafetyState::SafeTorque);
}

#[test]
fn hands_on_resets_timeout_timer() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_service();
    activate_high_torque(&mut service, "dev")?;

    // Hands off briefly
    service.update_hands_on_status(false)?;
    std::thread::sleep(Duration::from_millis(2000));

    // Hands back on resets
    service.update_hands_on_status(true)?;

    // Wait past original timeout — should be fine
    std::thread::sleep(Duration::from_millis(2000));
    let result = service.update_hands_on_status(true);
    assert!(result.is_ok());
    assert!(matches!(
        service.state(),
        SafetyState::HighTorqueActive { .. }
    ));
    Ok(())
}

#[test]
fn hands_off_exceeding_timeout_faults() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_service();
    activate_high_torque(&mut service, "dev")?;
    service.update_hands_on_status(false)?;
    std::thread::sleep(Duration::from_millis(3100));
    let result = service.update_hands_on_status(false);
    assert!(result.is_err());
    assert!(matches!(service.state(), SafetyState::Faulted { .. }));
    Ok(())
}

#[test]
fn torque_limit_safe_mode_limit_returns_configured_value() {
    let limit = TorqueLimit::new(20.0, 4.0);
    assert_eq!(limit.safe_mode_limit(), 4.0);
}

#[test]
fn torque_limit_default_values() {
    let limit = TorqueLimit::default();
    assert_eq!(limit.max_torque_nm, 25.0);
    assert_eq!(limit.safe_mode_torque_nm, 5.0);
    assert!(limit.log_violations);
    assert_eq!(limit.violation_count, 0);
}

#[test]
fn watchdog_default_timeout_is_100ms() {
    let wd = SoftwareWatchdog::with_default_timeout();
    assert_eq!(wd.timeout_ms(), 100);
}

#[test]
fn interlock_system_watchdog_timeout_ms_readable() {
    let sys = create_interlock_system(42);
    assert_eq!(sys.watchdog_timeout_ms(), 42);
}

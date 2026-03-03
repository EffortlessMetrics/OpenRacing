#![allow(clippy::redundant_closure)]
//! Safety interlock state machine integration tests.
//!
//! Tests cover:
//! - Safety interlock state machine transitions
//! - Fault detection → response time guarantees
//! - Multi-layer safety (physical + software + fault detection)
//! - Emergency stop sequences
//! - Proptest: interlock state never reaches invalid combinations

use proptest::prelude::*;
use racing_wheel_engine::safety::{
    ButtonCombo, FaultType, InterlockAck, SafetyInterlockState, SafetyInterlockSystem,
    SafetyService, SafetyState, SafetyTrigger, SoftwareWatchdog, WatchdogTimeoutHandler,
};
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn create_safety_service() -> SafetyService {
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

/// Drive through the full challenge flow and activate high torque.
fn activate_high_torque(service: &mut SafetyService, device: &str) -> Result<(), String> {
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
// State machine transitions
// =========================================================================

#[test]
fn initial_state_is_safe_torque() {
    let service = create_safety_service();
    assert_eq!(service.state(), &SafetyState::SafeTorque);
}

#[test]
fn request_high_torque_transitions_to_challenge() -> Result<(), String> {
    let mut service = create_safety_service();
    let challenge = service.request_high_torque("dev-1")?;
    assert!(matches!(
        service.state(),
        SafetyState::HighTorqueChallenge { .. }
    ));
    let _token = challenge.challenge_token; // token is random, just verify it was generated
    Ok(())
}

#[test]
fn ui_consent_transitions_to_awaiting_ack() -> Result<(), String> {
    let mut service = create_safety_service();
    let challenge = service.request_high_torque("dev-1")?;
    service.provide_ui_consent(challenge.challenge_token)?;
    assert!(matches!(
        service.state(),
        SafetyState::AwaitingPhysicalAck { .. }
    ));
    Ok(())
}

#[test]
fn full_challenge_flow_activates_high_torque() -> Result<(), String> {
    let mut service = create_safety_service();
    activate_high_torque(&mut service, "dev-1")?;
    assert!(matches!(
        service.state(),
        SafetyState::HighTorqueActive { .. }
    ));
    Ok(())
}

#[test]
fn cancel_challenge_returns_to_safe_torque() -> Result<(), String> {
    let mut service = create_safety_service();
    let _challenge = service.request_high_torque("dev-1")?;
    service.cancel_challenge()?;
    assert_eq!(service.state(), &SafetyState::SafeTorque);
    Ok(())
}

#[test]
fn disable_high_torque_returns_to_safe_torque() -> Result<(), String> {
    let mut service = create_safety_service();
    activate_high_torque(&mut service, "dev-1")?;
    service.disable_high_torque("dev-1")?;
    assert_eq!(service.state(), &SafetyState::SafeTorque);
    Ok(())
}

#[test]
fn cannot_request_high_torque_when_already_active() -> Result<(), String> {
    let mut service = create_safety_service();
    activate_high_torque(&mut service, "dev-1")?;
    let result = service.request_high_torque("dev-1");
    assert!(result.is_err());
    Ok(())
}

// =========================================================================
// Fault detection → response time guarantees
// =========================================================================

#[test]
fn fault_report_transitions_to_faulted_state() {
    let mut service = create_safety_service();
    service.report_fault(FaultType::UsbStall);
    assert!(matches!(service.state(), SafetyState::Faulted { .. }));
}

#[test]
fn faulted_state_clamps_torque_to_zero() {
    let mut service = create_safety_service();
    service.report_fault(FaultType::EncoderNaN);
    assert_eq!(service.clamp_torque_nm(10.0), 0.0);
    assert_eq!(service.clamp_torque_nm(-10.0), 0.0);
}

#[test]
fn fault_during_high_torque_zeroes_output() -> Result<(), String> {
    let mut service = create_safety_service();
    activate_high_torque(&mut service, "dev-1")?;
    service.report_fault(FaultType::Overcurrent);
    assert_eq!(service.clamp_torque_nm(25.0), 0.0);
    Ok(())
}

#[test]
fn watchdog_timeout_response_within_budget() {
    let mut handler = WatchdogTimeoutHandler::new();
    let response = handler.handle_timeout(15.0);
    assert_eq!(response.torque_command, 0.0);
    assert!(
        response.within_budget,
        "Timeout response took {:?}, exceeded 1ms budget",
        response.response_time
    );
}

#[test]
fn interlock_system_emergency_stop_zeroes_torque() {
    let mut system = create_interlock_system(100);
    let result = system.emergency_stop();
    assert_eq!(result.torque_command, 0.0);
    assert!(result.fault_occurred);
    assert!(matches!(
        result.state,
        SafetyInterlockState::EmergencyStop { .. }
    ));
}

#[test]
fn interlock_system_emergency_stop_response_time() {
    let mut system = create_interlock_system(100);
    let result = system.emergency_stop();
    assert!(
        result.response_time < Duration::from_millis(10),
        "Emergency stop response time {:?} exceeded 10ms budget",
        result.response_time
    );
}

// =========================================================================
// Multi-layer safety (physical + software + fault detection)
// =========================================================================

#[test]
fn interlock_system_watchdog_timeout_transitions_to_safe_mode(
) -> Result<(), racing_wheel_engine::safety::WatchdogError> {
    let watchdog = Box::new(SoftwareWatchdog::new(10)); // 10ms timeout
    let mut system = SafetyInterlockSystem::new(watchdog, 25.0);
    system.arm()?;

    // Feed once
    let _ = system.process_tick(10.0);

    // Wait for timeout
    std::thread::sleep(Duration::from_millis(15));

    let result = system.process_tick(10.0);
    assert_eq!(result.torque_command, 0.0);
    assert!(result.fault_occurred);
    assert!(matches!(
        result.state,
        SafetyInterlockState::SafeMode {
            triggered_by: SafetyTrigger::WatchdogTimeout,
            ..
        }
    ));
    Ok(())
}

#[test]
fn interlock_system_report_fault_enters_safe_mode() {
    let mut system = create_interlock_system(100);
    system.report_fault(FaultType::ThermalLimit);
    assert!(matches!(
        system.state(),
        SafetyInterlockState::SafeMode {
            triggered_by: SafetyTrigger::FaultDetected(FaultType::ThermalLimit),
            ..
        }
    ));
}

#[test]
fn interlock_system_torque_clamped_in_safe_mode() {
    let mut system = create_interlock_system(100);
    system.report_fault(FaultType::PipelineFault);

    // In safe mode, torque is clamped to safe limit (25.0 * 0.2 = 5.0)
    let result = system.process_tick(20.0);
    assert!(result.torque_command <= 5.0);
}

#[test]
fn interlock_system_fault_is_logged() {
    let mut system = create_interlock_system(100);
    assert!(system.fault_log().is_empty());

    system.report_fault(FaultType::Overcurrent);
    assert!(!system.fault_log().is_empty());

    let entry = &system.fault_log()[0];
    assert_eq!(entry.fault_type, FaultType::Overcurrent);
}

#[test]
fn safety_service_cannot_enable_high_torque_with_active_faults() {
    let mut service = create_safety_service();
    service.report_fault(FaultType::UsbStall);
    let result = service.request_high_torque("dev-1");
    assert!(result.is_err());
}

#[test]
fn clear_fault_requires_minimum_duration() {
    let mut service = create_safety_service();
    service.report_fault(FaultType::ThermalLimit);

    // Immediate clear should fail
    let result = service.clear_fault();
    assert!(result.is_err());
}

#[test]
fn clear_fault_succeeds_after_minimum_duration() {
    let mut service = create_safety_service();
    service.report_fault(FaultType::ThermalLimit);

    std::thread::sleep(Duration::from_millis(110));
    let result = service.clear_fault();
    assert!(result.is_ok());
    assert_eq!(service.state(), &SafetyState::SafeTorque);
}

// =========================================================================
// Emergency stop sequences
// =========================================================================

#[test]
fn emergency_stop_from_normal_zeroes_torque() {
    let mut system = create_interlock_system(100);
    let result = system.emergency_stop();
    assert_eq!(result.torque_command, 0.0);
    assert!(matches!(
        result.state,
        SafetyInterlockState::EmergencyStop { .. }
    ));
}

#[test]
fn emergency_stop_cannot_be_cleared() {
    let mut system = create_interlock_system(100);
    system.emergency_stop();
    let result = system.clear_fault();
    assert!(result.is_err());
}

#[test]
fn emergency_stop_torque_always_zero(
) -> Result<(), racing_wheel_engine::safety::WatchdogError> {
    let mut system = create_interlock_system(100);
    system.arm()?;
    system.emergency_stop();

    // Even with armed watchdog, emergency stop should still produce zero torque
    let result = system.process_tick(20.0);
    assert_eq!(result.torque_command, 0.0);
    Ok(())
}

#[test]
fn emergency_stop_requires_manual_reset(
) -> Result<(), racing_wheel_engine::safety::WatchdogError> {
    let mut system = create_interlock_system(100);
    system.emergency_stop();

    // clear_fault cannot clear emergency stop
    let clear_result = system.clear_fault();
    assert!(clear_result.is_err());

    // Full reset restores normal state
    system.reset()?;
    assert_eq!(system.state(), &SafetyInterlockState::Normal);
    Ok(())
}

// =========================================================================
// Hands-off timeout
// =========================================================================

#[test]
fn hands_off_timeout_faults_during_high_torque() -> Result<(), String> {
    let mut service = SafetyService::with_timeouts(
        5.0,
        25.0,
        Duration::from_millis(50), // very short for test
        Duration::from_secs(2),
    );
    activate_high_torque(&mut service, "dev-1")?;

    // Simulate hands off for longer than timeout
    std::thread::sleep(Duration::from_millis(60));
    let result = service.update_hands_on_status(false);
    assert!(result.is_err());
    assert!(matches!(service.state(), SafetyState::Faulted { .. }));
    Ok(())
}

#[test]
fn hands_on_resets_timeout_timer() -> Result<(), String> {
    let mut service = create_safety_service();
    activate_high_torque(&mut service, "dev-1")?;

    // Report hands-on multiple times should not error
    let result = service.update_hands_on_status(true);
    assert!(result.is_ok());
    assert!(matches!(
        service.state(),
        SafetyState::HighTorqueActive { .. }
    ));
    Ok(())
}

// =========================================================================
// Proptest: interlock state never reaches invalid combinations
// =========================================================================

/// Actions that can be applied to the SafetyService state machine
#[derive(Debug, Clone)]
enum SafetyAction {
    RequestHighTorque,
    ReportFault(u8), // mapped to FaultType variants
    CancelChallenge,
    DisableHighTorque,
    ClearFault,
    UpdateHandsOn(bool),
}

fn arb_fault_index() -> impl Strategy<Value = u8> {
    0u8..9
}

fn fault_from_index(idx: u8) -> FaultType {
    match idx % 9 {
        0 => FaultType::UsbStall,
        1 => FaultType::EncoderNaN,
        2 => FaultType::ThermalLimit,
        3 => FaultType::Overcurrent,
        4 => FaultType::PluginOverrun,
        5 => FaultType::TimingViolation,
        6 => FaultType::SafetyInterlockViolation,
        7 => FaultType::HandsOffTimeout,
        _ => FaultType::PipelineFault,
    }
}

fn arb_safety_action() -> impl Strategy<Value = SafetyAction> {
    prop_oneof![
        Just(SafetyAction::RequestHighTorque),
        arb_fault_index().prop_map(SafetyAction::ReportFault),
        Just(SafetyAction::CancelChallenge),
        Just(SafetyAction::DisableHighTorque),
        Just(SafetyAction::ClearFault),
        proptest::bool::ANY.prop_map(SafetyAction::UpdateHandsOn),
    ]
}

/// Validate that the safety state is always one of the defined variants
/// and torque limits are consistent with the state.
fn validate_state_invariants(service: &SafetyService) -> bool {
    let max_torque = service.max_torque_nm();
    match service.state() {
        SafetyState::SafeTorque
        | SafetyState::HighTorqueChallenge { .. }
        | SafetyState::AwaitingPhysicalAck { .. } => max_torque <= 5.0 + f32::EPSILON,
        SafetyState::HighTorqueActive { .. } => max_torque <= 25.0 + f32::EPSILON,
        SafetyState::Faulted { .. } => max_torque == 0.0,
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn interlock_state_invariants_hold(
        actions in proptest::collection::vec(arb_safety_action(), 1..50)
    ) {
        let mut service = create_safety_service();
        assert!(validate_state_invariants(&service));

        for action in actions {
            match action {
                SafetyAction::RequestHighTorque => {
                    let _ = service.request_high_torque("dev-1");
                }
                SafetyAction::ReportFault(idx) => {
                    service.report_fault(fault_from_index(idx));
                }
                SafetyAction::CancelChallenge => {
                    let _ = service.cancel_challenge();
                }
                SafetyAction::DisableHighTorque => {
                    let _ = service.disable_high_torque("dev-1");
                }
                SafetyAction::ClearFault => {
                    let _ = service.clear_fault();
                }
                SafetyAction::UpdateHandsOn(on) => {
                    let _ = service.update_hands_on_status(on);
                }
            }
            assert!(
                validate_state_invariants(&service),
                "Invariant violated in state {:?}",
                service.state()
            );
        }
    }

    #[test]
    fn faulted_torque_is_always_zero(fault_idx in 0u8..9, torque in -50.0f32..50.0) {
        let mut service = create_safety_service();
        service.report_fault(fault_from_index(fault_idx));
        assert_eq!(service.clamp_torque_nm(torque), 0.0);
    }

    #[test]
    fn safe_torque_clamp_within_bounds(torque in -100.0f32..100.0) {
        let service = create_safety_service();
        let clamped = service.clamp_torque_nm(torque);
        assert!((-5.0 - f32::EPSILON..=5.0 + f32::EPSILON).contains(&clamped),
            "Clamped torque {} is out of safe bounds for input {}",
            clamped, torque);
    }
}

//! Edge-case tests for SafetyService that complement the existing unit and
//! interlock behavior tests.
//!
//! Focus areas:
//! - `clamp_torque_nm` with NaN/Inf in **non-faulted** states (SafeTorque,
//!   HighTorqueActive)
//! - Legacy `check_hands_off_timeout` method
//! - `SafetyService::default()` construction
//! - `get_max_torque` flag interactions across states

use racing_wheel_engine::safety::{
    ButtonCombo, FaultType, InterlockAck, SafetyService, SafetyState,
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

/// Drive a SafetyService through the full high-torque activation flow.
fn activate_high_torque(service: &mut SafetyService, device: &str) {
    let challenge = service
        .request_high_torque(device)
        .expect("request_high_torque failed in test setup");
    service
        .provide_ui_consent(challenge.challenge_token)
        .expect("provide_ui_consent failed in test setup");
    service
        .report_combo_start(challenge.challenge_token)
        .expect("report_combo_start failed in test setup");
    std::thread::sleep(Duration::from_millis(2100));
    let ack = InterlockAck {
        challenge_token: challenge.challenge_token,
        device_token: 42,
        combo_completed: ButtonCombo::BothClutchPaddles,
        timestamp: Instant::now(),
    };
    service
        .confirm_high_torque(device, ack)
        .expect("confirm_high_torque failed in test setup");
}

// =========================================================================
// clamp_torque_nm: NaN / Inf in non-faulted states
// =========================================================================

#[test]
fn clamp_nan_in_safe_torque_state_yields_zero() {
    let service = create_test_service();
    assert_eq!(service.state(), &SafetyState::SafeTorque);

    assert_eq!(service.clamp_torque_nm(f32::NAN), 0.0);
}

#[test]
fn clamp_positive_inf_in_safe_torque_state_yields_zero() {
    let service = create_test_service();
    assert_eq!(service.clamp_torque_nm(f32::INFINITY), 0.0);
}

#[test]
fn clamp_negative_inf_in_safe_torque_state_yields_zero() {
    let service = create_test_service();
    assert_eq!(service.clamp_torque_nm(f32::NEG_INFINITY), 0.0);
}

#[test]
fn clamp_nan_in_high_torque_active_state_yields_zero() {
    let mut service = create_test_service();
    activate_high_torque(&mut service, "dev");
    assert!(matches!(
        service.state(),
        SafetyState::HighTorqueActive { .. }
    ));

    assert_eq!(service.clamp_torque_nm(f32::NAN), 0.0);
}

#[test]
fn clamp_positive_inf_in_high_torque_active_state_yields_zero() {
    let mut service = create_test_service();
    activate_high_torque(&mut service, "dev");

    assert_eq!(service.clamp_torque_nm(f32::INFINITY), 0.0);
}

#[test]
fn clamp_negative_inf_in_high_torque_active_state_yields_zero() {
    let mut service = create_test_service();
    activate_high_torque(&mut service, "dev");

    assert_eq!(service.clamp_torque_nm(f32::NEG_INFINITY), 0.0);
}

// =========================================================================
// SafetyService::default() construction
// =========================================================================

#[test]
fn default_service_starts_in_safe_torque() {
    let service = SafetyService::default();
    assert_eq!(service.state(), &SafetyState::SafeTorque);
}

#[test]
fn default_service_safe_torque_limit_is_five() {
    let service = SafetyService::default();
    assert_eq!(service.max_torque_nm(), 5.0);
}

#[test]
fn default_service_clamps_within_safe_limit() {
    let service = SafetyService::default();
    assert_eq!(service.clamp_torque_nm(100.0), 5.0);
    assert_eq!(service.clamp_torque_nm(-100.0), -5.0);
}

// =========================================================================
// get_max_torque flag interactions
// =========================================================================

#[test]
fn get_max_torque_high_torque_flag_ignored_in_safe_torque_state() {
    let service = create_test_service();
    // Even with is_high_torque_enabled=true, SafeTorque returns safe limit
    assert_eq!(service.get_max_torque(true).value(), 5.0);
    assert_eq!(service.get_max_torque(false).value(), 5.0);
}

#[test]
fn get_max_torque_high_torque_flag_respected_in_active_state() {
    let mut service = create_test_service();
    activate_high_torque(&mut service, "dev");

    assert_eq!(service.get_max_torque(true).value(), 25.0);
    // When flag is false, even in HighTorqueActive, should return safe limit
    assert_eq!(service.get_max_torque(false).value(), 5.0);
}

#[test]
fn get_max_torque_faulted_always_zero_regardless_of_flag() {
    let mut service = create_test_service();
    service.report_fault(FaultType::ThermalLimit);

    assert_eq!(service.get_max_torque(true).value(), 0.0);
    assert_eq!(service.get_max_torque(false).value(), 0.0);
}

// =========================================================================
// Legacy check_hands_off_timeout method
// =========================================================================

#[test]
fn legacy_check_hands_off_timeout_triggers_fault_in_high_torque() {
    let mut service = create_test_service();
    activate_high_torque(&mut service, "dev");
    assert!(matches!(
        service.state(),
        SafetyState::HighTorqueActive { .. }
    ));

    // Exceeds the 3-second hands_off_timeout configured in create_test_service
    service.check_hands_off_timeout(Duration::from_secs(4));

    assert!(matches!(service.state(), SafetyState::Faulted { .. }));
    assert_eq!(service.max_torque_nm(), 0.0);
}

#[test]
fn legacy_check_hands_off_timeout_does_not_fault_in_safe_torque() {
    let mut service = create_test_service();
    assert_eq!(service.state(), &SafetyState::SafeTorque);

    // Even a huge duration should be ignored in SafeTorque state
    service.check_hands_off_timeout(Duration::from_secs(999));
    assert_eq!(service.state(), &SafetyState::SafeTorque);
}

#[test]
fn legacy_check_hands_off_timeout_within_limit_does_not_fault() {
    let mut service = create_test_service();
    activate_high_torque(&mut service, "dev");

    // Within the 3-second limit
    service.check_hands_off_timeout(Duration::from_secs(2));

    assert!(matches!(
        service.state(),
        SafetyState::HighTorqueActive { .. }
    ));
}

// =========================================================================
// clamp_torque_nm during challenge states
// =========================================================================

#[test]
fn clamp_torque_during_high_torque_challenge_uses_safe_limit() {
    let mut service = create_test_service();
    let _challenge = service
        .request_high_torque("dev")
        .expect("request failed in test setup");

    assert!(matches!(
        service.state(),
        SafetyState::HighTorqueChallenge { .. }
    ));
    assert_eq!(service.clamp_torque_nm(20.0), 5.0);
    assert_eq!(service.clamp_torque_nm(-20.0), -5.0);
}

#[test]
fn clamp_torque_during_awaiting_physical_ack_uses_safe_limit() {
    let mut service = create_test_service();
    let challenge = service
        .request_high_torque("dev")
        .expect("request failed in test setup");
    service
        .provide_ui_consent(challenge.challenge_token)
        .expect("consent failed in test setup");

    assert!(matches!(
        service.state(),
        SafetyState::AwaitingPhysicalAck { .. }
    ));
    assert_eq!(service.clamp_torque_nm(20.0), 5.0);
    assert_eq!(service.clamp_torque_nm(-20.0), -5.0);
}

// =========================================================================
// Negative-value torque in high-torque-active state
// =========================================================================

#[test]
fn clamp_torque_preserves_sign_in_high_torque_active() {
    let mut service = create_test_service();
    activate_high_torque(&mut service, "dev");

    assert_eq!(service.clamp_torque_nm(10.0), 10.0);
    assert_eq!(service.clamp_torque_nm(-10.0), -10.0);
    assert_eq!(service.clamp_torque_nm(30.0), 25.0);
    assert_eq!(service.clamp_torque_nm(-30.0), -25.0);
}

// =========================================================================
// Zero-torque passthrough
// =========================================================================

#[test]
fn clamp_torque_zero_passes_through_in_all_states() {
    let mut service = create_test_service();
    assert_eq!(service.clamp_torque_nm(0.0), 0.0);

    activate_high_torque(&mut service, "dev");
    assert_eq!(service.clamp_torque_nm(0.0), 0.0);

    service.report_fault(FaultType::UsbStall);
    assert_eq!(service.clamp_torque_nm(0.0), 0.0);
}

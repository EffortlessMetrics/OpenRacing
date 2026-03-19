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
fn clamp_nan_in_high_torque_active_state_yields_zero() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_service();
    activate_high_torque(&mut service, "dev")?;
    assert!(matches!(
        service.state(),
        SafetyState::HighTorqueActive { .. }
    ));

    assert_eq!(service.clamp_torque_nm(f32::NAN), 0.0);
    Ok(())
}

#[test]
fn clamp_positive_inf_in_high_torque_active_state_yields_zero()
-> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_service();
    activate_high_torque(&mut service, "dev")?;

    assert_eq!(service.clamp_torque_nm(f32::INFINITY), 0.0);
    Ok(())
}

#[test]
fn clamp_negative_inf_in_high_torque_active_state_yields_zero()
-> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_service();
    activate_high_torque(&mut service, "dev")?;

    assert_eq!(service.clamp_torque_nm(f32::NEG_INFINITY), 0.0);
    Ok(())
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
fn get_max_torque_high_torque_flag_respected_in_active_state()
-> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_service();
    activate_high_torque(&mut service, "dev")?;

    assert_eq!(service.get_max_torque(true).value(), 25.0);
    // When flag is false, even in HighTorqueActive, should return safe limit
    assert_eq!(service.get_max_torque(false).value(), 5.0);
    Ok(())
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
fn legacy_check_hands_off_timeout_triggers_fault_in_high_torque()
-> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_service();
    activate_high_torque(&mut service, "dev")?;
    assert!(matches!(
        service.state(),
        SafetyState::HighTorqueActive { .. }
    ));

    // Exceeds the 3-second hands_off_timeout configured in create_test_service
    service.check_hands_off_timeout(Duration::from_secs(4));

    assert!(matches!(service.state(), SafetyState::Faulted { .. }));
    assert_eq!(service.max_torque_nm(), 0.0);
    Ok(())
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
fn legacy_check_hands_off_timeout_within_limit_does_not_fault()
-> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_service();
    activate_high_torque(&mut service, "dev")?;

    // Within the 3-second limit
    service.check_hands_off_timeout(Duration::from_secs(2));

    assert!(matches!(
        service.state(),
        SafetyState::HighTorqueActive { .. }
    ));
    Ok(())
}

// =========================================================================
// clamp_torque_nm during challenge states
// =========================================================================

#[test]
fn clamp_torque_during_high_torque_challenge_uses_safe_limit()
-> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_service();
    let _challenge = service.request_high_torque("dev")?;

    assert!(matches!(
        service.state(),
        SafetyState::HighTorqueChallenge { .. }
    ));
    assert_eq!(service.clamp_torque_nm(20.0), 5.0);
    assert_eq!(service.clamp_torque_nm(-20.0), -5.0);
    Ok(())
}

#[test]
fn clamp_torque_during_awaiting_physical_ack_uses_safe_limit()
-> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_service();
    let challenge = service.request_high_torque("dev")?;
    service.provide_ui_consent(challenge.challenge_token)?;

    assert!(matches!(
        service.state(),
        SafetyState::AwaitingPhysicalAck { .. }
    ));
    assert_eq!(service.clamp_torque_nm(20.0), 5.0);
    assert_eq!(service.clamp_torque_nm(-20.0), -5.0);
    Ok(())
}

// =========================================================================
// Negative-value torque in high-torque-active state
// =========================================================================

#[test]
fn clamp_torque_preserves_sign_in_high_torque_active() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_service();
    activate_high_torque(&mut service, "dev")?;

    assert_eq!(service.clamp_torque_nm(10.0), 10.0);
    assert_eq!(service.clamp_torque_nm(-10.0), -10.0);
    assert_eq!(service.clamp_torque_nm(30.0), 25.0);
    assert_eq!(service.clamp_torque_nm(-30.0), -25.0);
    Ok(())
}

// =========================================================================
// Zero-torque passthrough
// =========================================================================

#[test]
fn clamp_torque_zero_passes_through_in_all_states() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_service();
    assert_eq!(service.clamp_torque_nm(0.0), 0.0);

    activate_high_torque(&mut service, "dev")?;
    assert_eq!(service.clamp_torque_nm(0.0), 0.0);

    service.report_fault(FaultType::UsbStall);
    assert_eq!(service.clamp_torque_nm(0.0), 0.0);
    Ok(())
}

// =========================================================================
// ADR-0006: Extended Edge Cases
// =========================================================================

#[test]
fn latching_fault_requires_explicit_recovery() {
    let mut service = create_test_service();
    service.report_fault(FaultType::EncoderNaN);

    assert!(matches!(service.state(), SafetyState::Faulted { .. }));
    assert_eq!(service.max_torque_nm(), 0.0);

    // Fault should persist across multiple calls/ticks
    for _ in 0..10 {
        assert_eq!(service.clamp_torque_nm(10.0), 0.0);
    }

    // Should NOT be able to clear fault immediately (minimum duration not met)
    let res = service.clear_fault();
    assert!(res.is_err(), "Should not clear fault before 100ms");

    // Simulate time passing for the fault (manual state transition for testing)
    // We'll just verify that clear_fault is the only way out.
    assert!(matches!(service.state(), SafetyState::Faulted { .. }));
}

#[test]
fn multiple_simultaneous_faults_behavior() {
    let mut service = create_test_service();
    service.report_fault(FaultType::ThermalLimit);

    if let SafetyState::Faulted { fault, .. } = service.state() {
        assert_eq!(*fault, FaultType::ThermalLimit);
    } else {
        panic!("Expected Faulted state");
    }

    // Report a second, potentially "more severe" or different fault
    service.report_fault(FaultType::Overcurrent);

    // State should still be faulted, reflecting the LATEST fault reported
    if let SafetyState::Faulted { fault, .. } = service.state() {
        assert_eq!(*fault, FaultType::Overcurrent);
    } else {
        panic!("Expected Faulted state after second fault");
    }
}

#[test]
fn challenge_expiry_logic_safety_transition() {
    let mut service = create_test_service();

    // 1. SafeTorque -> HighTorqueChallenge
    let _ = service.request_high_torque("test").unwrap();
    assert!(matches!(
        service.state(),
        SafetyState::HighTorqueChallenge { .. }
    ));

    // 2. Mocking expiration by calling check_challenge_expiry
    // Since we can't easily warp time, we verify that if it's NOT expired, it stays in state
    let expired = service.check_challenge_expiry();
    assert!(!expired);
    assert!(matches!(
        service.state(),
        SafetyState::HighTorqueChallenge { .. }
    ));

    // 3. Requesting a second challenge should fail (precondition check)
    let res = service.request_high_torque("test");
    assert!(res.is_err());
}

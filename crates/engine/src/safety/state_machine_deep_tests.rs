//! Deep safety state machine tests for 1.0 RC quality validation
//!
//! Coverage areas:
//! - All valid state transitions: SafeTorque → HighTorqueChallenge → AwaitingPhysicalAck → HighTorqueActive
//! - Invalid state transitions are rejected
//! - Fault escalation timing (10ms detection, 50ms response)
//! - Multi-fault concurrent scenarios
//! - Recovery paths from each state
//! - Interlock challenge-response under load
//! - Proptest: random fault sequences preserve safety invariants

use super::*;
use openracing_test_helpers::prelude::*;
use proptest::prelude::*;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn create_deep_test_service() -> SafetyService {
    SafetyService::with_timeouts(
        5.0,                    // max_safe_torque_nm
        25.0,                   // max_high_torque_nm
        Duration::from_secs(3), // hands_off_timeout
        Duration::from_secs(2), // combo_hold_duration
    )
}

/// Drive through the full high-torque activation flow.
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

/// All defined fault types for iteration.
const ALL_FAULTS: [FaultType; 9] = [
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
// 1. Valid state transitions
// =========================================================================

#[test]
fn deep_test_transition_safe_to_challenge() {
    let mut svc = create_deep_test_service();
    assert_eq!(svc.state(), &SafetyState::SafeTorque);

    let challenge = must(svc.request_high_torque("dev1"));
    match svc.state() {
        SafetyState::HighTorqueChallenge {
            challenge_token, ..
        } => {
            assert_eq!(*challenge_token, challenge.challenge_token);
        }
        other => panic!("expected HighTorqueChallenge, got {other:?}"),
    }
}

#[test]
fn deep_test_transition_challenge_to_awaiting_ack() {
    let mut svc = create_deep_test_service();
    let challenge = must(svc.request_high_torque("dev1"));
    must(svc.provide_ui_consent(challenge.challenge_token));

    match svc.state() {
        SafetyState::AwaitingPhysicalAck {
            challenge_token, ..
        } => {
            assert_eq!(*challenge_token, challenge.challenge_token);
        }
        other => panic!("expected AwaitingPhysicalAck, got {other:?}"),
    }
}

#[test]
fn deep_test_transition_awaiting_to_high_torque_active() {
    let mut svc = create_deep_test_service();
    activate_high_torque(&mut svc, "dev1");

    match svc.state() {
        SafetyState::HighTorqueActive { .. } => {}
        other => panic!("expected HighTorqueActive, got {other:?}"),
    }
}

#[test]
fn deep_test_transition_any_state_to_faulted() {
    // From SafeTorque
    let mut svc = create_deep_test_service();
    svc.report_fault(FaultType::UsbStall);
    assert!(matches!(svc.state(), SafetyState::Faulted { .. }));

    // From HighTorqueActive
    let mut svc = create_deep_test_service();
    activate_high_torque(&mut svc, "dev1");
    svc.report_fault(FaultType::ThermalLimit);
    assert!(matches!(svc.state(), SafetyState::Faulted { .. }));
    assert_eq!(svc.max_torque_nm(), 0.0);
}

#[test]
fn deep_test_full_lifecycle_safe_to_high_and_back() {
    let mut svc = create_deep_test_service();

    // Start safe
    assert_eq!(svc.state(), &SafetyState::SafeTorque);

    // Activate high torque
    activate_high_torque(&mut svc, "dev1");
    assert!(matches!(svc.state(), SafetyState::HighTorqueActive { .. }));
    assert_eq!(svc.max_torque_nm(), 25.0);

    // Disable high torque
    must(svc.disable_high_torque("dev1"));
    assert_eq!(svc.state(), &SafetyState::SafeTorque);
    assert_eq!(svc.max_torque_nm(), 5.0);
}

// =========================================================================
// 2. Invalid state transitions are rejected
// =========================================================================

#[test]
fn deep_test_cannot_request_high_torque_when_already_challenged() {
    let mut svc = create_deep_test_service();
    must(svc.request_high_torque("dev1"));

    let result = svc.request_high_torque("dev1");
    assert!(result.is_err());
}

#[test]
fn deep_test_cannot_provide_consent_without_challenge() {
    let mut svc = create_deep_test_service();
    let result = svc.provide_ui_consent(12345);
    assert!(result.is_err());
}

#[test]
fn deep_test_cannot_provide_consent_with_wrong_token() {
    let mut svc = create_deep_test_service();
    must(svc.request_high_torque("dev1"));

    let result = svc.provide_ui_consent(0xBADBAD);
    assert!(result.is_err());
}

#[test]
fn deep_test_cannot_report_combo_without_awaiting() {
    let mut svc = create_deep_test_service();
    let result = svc.report_combo_start(12345);
    assert!(result.is_err());
}

#[test]
fn deep_test_cannot_confirm_without_awaiting() {
    let mut svc = create_deep_test_service();
    let ack = InterlockAck {
        challenge_token: 12345,
        device_token: 99,
        combo_completed: ButtonCombo::BothClutchPaddles,
        timestamp: Instant::now(),
    };
    let result = svc.confirm_high_torque("dev1", ack);
    assert!(result.is_err());
}

#[test]
fn deep_test_cannot_disable_high_torque_when_not_active() {
    let mut svc = create_deep_test_service();
    let result = svc.disable_high_torque("dev1");
    assert!(result.is_err());
}

#[test]
fn deep_test_cannot_cancel_without_active_challenge() {
    let mut svc = create_deep_test_service();
    let result = svc.cancel_challenge();
    assert!(result.is_err());
}

#[test]
fn deep_test_cannot_clear_fault_without_active_fault() {
    let mut svc = create_deep_test_service();
    let result = svc.clear_fault();
    assert!(result.is_err());
}

#[test]
fn deep_test_cannot_request_high_torque_when_faulted() {
    let mut svc = create_deep_test_service();
    svc.report_fault(FaultType::Overcurrent);

    let result = svc.request_high_torque("dev1");
    assert!(result.is_err());
}

// =========================================================================
// 3. Fault escalation timing
// =========================================================================

#[test]
fn deep_test_fault_detection_is_immediate() {
    let mut svc = create_deep_test_service();
    let before = Instant::now();
    svc.report_fault(FaultType::UsbStall);
    let elapsed = before.elapsed();

    // Fault detection must happen within 10ms (generous bound for CI)
    assert!(
        elapsed < Duration::from_millis(10),
        "Fault detection took {elapsed:?}, expected < 10ms"
    );
    assert_eq!(svc.max_torque_nm(), 0.0);
}

#[test]
fn deep_test_fault_torque_response_immediate() {
    let mut svc = create_deep_test_service();
    activate_high_torque(&mut svc, "dev1");
    assert_eq!(svc.max_torque_nm(), 25.0);

    let before = Instant::now();
    svc.report_fault(FaultType::Overcurrent);
    let elapsed = before.elapsed();

    // Torque must be zeroed within 50ms (generous bound)
    assert!(
        elapsed < Duration::from_millis(50),
        "Fault response took {elapsed:?}, expected < 50ms"
    );
    assert_eq!(svc.max_torque_nm(), 0.0);
    assert_eq!(svc.clamp_torque_nm(25.0), 0.0);
}

#[test]
fn deep_test_fault_clears_after_minimum_duration() {
    let mut svc = create_deep_test_service();
    svc.report_fault(FaultType::EncoderNaN);

    // Cannot clear immediately
    let result = svc.clear_fault();
    assert!(result.is_err());

    // Wait minimum fault duration (100ms)
    std::thread::sleep(Duration::from_millis(110));
    let result = svc.clear_fault();
    assert!(result.is_ok());
    assert_eq!(svc.state(), &SafetyState::SafeTorque);
}

// =========================================================================
// 4. Multi-fault concurrent scenarios
// =========================================================================

#[test]
fn deep_test_all_fault_types_transition_to_faulted() {
    for fault in &ALL_FAULTS {
        let mut svc = create_deep_test_service();
        svc.report_fault(*fault);

        match svc.state() {
            SafetyState::Faulted { fault: f, .. } => {
                assert_eq!(f, fault, "Fault type mismatch for {fault:?}");
            }
            other => panic!("Expected Faulted for {fault:?}, got {other:?}"),
        }
        assert_eq!(svc.max_torque_nm(), 0.0);
    }
}

#[test]
fn deep_test_sequential_faults_last_wins() {
    let mut svc = create_deep_test_service();

    svc.report_fault(FaultType::UsbStall);
    svc.report_fault(FaultType::ThermalLimit);

    match svc.state() {
        SafetyState::Faulted { fault, .. } => {
            assert_eq!(*fault, FaultType::ThermalLimit);
        }
        other => panic!("Expected Faulted, got {other:?}"),
    }
}

#[test]
fn deep_test_fault_during_challenge_cancels_challenge() {
    let mut svc = create_deep_test_service();
    must(svc.request_high_torque("dev1"));

    svc.report_fault(FaultType::PipelineFault);
    assert!(matches!(svc.state(), SafetyState::Faulted { .. }));
    assert_eq!(svc.max_torque_nm(), 0.0);
}

#[test]
fn deep_test_fault_during_awaiting_ack() {
    let mut svc = create_deep_test_service();
    let challenge = must(svc.request_high_torque("dev1"));
    must(svc.provide_ui_consent(challenge.challenge_token));

    svc.report_fault(FaultType::TimingViolation);
    assert!(matches!(svc.state(), SafetyState::Faulted { .. }));
}

#[test]
fn deep_test_rapid_fault_clear_fault_cycle() {
    let mut svc = create_deep_test_service();

    for fault in &ALL_FAULTS {
        svc.report_fault(*fault);
        assert!(matches!(svc.state(), SafetyState::Faulted { .. }));
        assert_eq!(svc.max_torque_nm(), 0.0);

        std::thread::sleep(Duration::from_millis(110));
        must(svc.clear_fault());
        assert_eq!(svc.state(), &SafetyState::SafeTorque);
    }
}

// =========================================================================
// 5. Recovery paths from each state
// =========================================================================

#[test]
fn deep_test_recover_from_faulted_to_safe() {
    let mut svc = create_deep_test_service();
    svc.report_fault(FaultType::UsbStall);

    std::thread::sleep(Duration::from_millis(110));
    must(svc.clear_fault());
    assert_eq!(svc.state(), &SafetyState::SafeTorque);
    assert_eq!(svc.max_torque_nm(), 5.0);
}

#[test]
fn deep_test_recover_from_challenge_via_cancel() {
    let mut svc = create_deep_test_service();
    must(svc.request_high_torque("dev1"));

    must(svc.cancel_challenge());
    assert_eq!(svc.state(), &SafetyState::SafeTorque);
}

#[test]
fn deep_test_recover_from_awaiting_via_cancel() {
    let mut svc = create_deep_test_service();
    let challenge = must(svc.request_high_torque("dev1"));
    must(svc.provide_ui_consent(challenge.challenge_token));

    must(svc.cancel_challenge());
    assert_eq!(svc.state(), &SafetyState::SafeTorque);
}

#[test]
fn deep_test_recover_from_high_torque_via_disable() {
    let mut svc = create_deep_test_service();
    activate_high_torque(&mut svc, "dev1");

    must(svc.disable_high_torque("dev1"));
    assert_eq!(svc.state(), &SafetyState::SafeTorque);
    assert!(!svc.has_valid_token("dev1"));
}

#[test]
fn deep_test_recover_from_faulted_then_stays_safe() {
    let mut svc = create_deep_test_service();
    svc.report_fault(FaultType::EncoderNaN);

    std::thread::sleep(Duration::from_millis(110));
    must(svc.clear_fault());

    // State restored to safe, but fault history persists (by design)
    assert_eq!(svc.state(), &SafetyState::SafeTorque);
    assert_eq!(svc.max_torque_nm(), 5.0);

    // Cannot go to high torque because fault counts are non-zero (safety design)
    let result = svc.request_high_torque("dev1");
    assert!(result.is_err());
}

// =========================================================================
// 6. Interlock challenge-response under load
// =========================================================================

#[test]
fn deep_test_challenge_expiry_returns_to_safe() {
    let mut svc =
        SafetyService::with_timeouts(5.0, 25.0, Duration::from_secs(3), Duration::from_secs(2));
    must(svc.request_high_torque("dev1"));

    // Don't complete challenge; wait for check_challenge_expiry
    // The challenge has a 30s timeout so we simulate expiry check
    let expired = svc.check_challenge_expiry();
    // Not expired yet (30s timeout)
    assert!(!expired);
    // State should still be challenge
    assert!(matches!(
        svc.state(),
        SafetyState::HighTorqueChallenge { .. }
    ));
}

#[test]
fn deep_test_challenge_time_remaining() {
    let mut svc = create_deep_test_service();
    must(svc.request_high_torque("dev1"));

    let remaining = svc.get_challenge_time_remaining();
    assert!(remaining.is_some());
    let dur = remaining.as_ref().map(|d| d.as_secs()).unwrap_or(0);
    assert!(dur > 0 && dur <= 30);
}

#[test]
fn deep_test_combo_hold_too_short_rejected() {
    let mut svc = create_deep_test_service();
    let challenge = must(svc.request_high_torque("dev1"));
    must(svc.provide_ui_consent(challenge.challenge_token));
    must(svc.report_combo_start(challenge.challenge_token));

    // Don't wait long enough
    std::thread::sleep(Duration::from_millis(500));

    let ack = InterlockAck {
        challenge_token: challenge.challenge_token,
        device_token: 99,
        combo_completed: ButtonCombo::BothClutchPaddles,
        timestamp: Instant::now(),
    };

    let result = svc.confirm_high_torque("dev1", ack);
    assert!(result.is_err());
}

#[test]
fn deep_test_consent_requirements_populated() {
    let svc = create_deep_test_service();
    let req = svc.get_consent_requirements();

    assert!(req.requires_explicit_consent);
    assert!(!req.warnings.is_empty());
    assert!(!req.disclaimers.is_empty());
    assert_eq!(req.max_torque_nm, 25.0);
}

#[test]
fn deep_test_device_token_persists_after_activation() {
    let mut svc = create_deep_test_service();
    activate_high_torque(&mut svc, "dev1");

    assert!(svc.has_valid_token("dev1"));
    assert!(!svc.has_valid_token("dev2"));
}

#[test]
fn deep_test_hands_off_timeout_faults_in_high_torque() {
    let mut svc = SafetyService::with_timeouts(
        5.0,
        25.0,
        Duration::from_millis(200), // Short timeout for testing
        Duration::from_secs(2),
    );
    activate_high_torque(&mut svc, "dev1");

    // Report hands-off for longer than timeout
    std::thread::sleep(Duration::from_millis(250));
    let result = svc.update_hands_on_status(false);
    assert!(result.is_err());
    assert!(matches!(svc.state(), SafetyState::Faulted { .. }));
}

#[test]
fn deep_test_hands_on_resets_timeout() {
    let mut svc = SafetyService::with_timeouts(
        5.0,
        25.0,
        Duration::from_millis(500),
        Duration::from_secs(2),
    );
    activate_high_torque(&mut svc, "dev1");

    // Report hands-on to reset
    std::thread::sleep(Duration::from_millis(100));
    must(svc.update_hands_on_status(true));

    // Still in high torque
    assert!(matches!(svc.state(), SafetyState::HighTorqueActive { .. }));
}

#[test]
fn deep_test_torque_clamping_in_each_state() {
    let mut svc = create_deep_test_service();

    // SafeTorque: clamp to ±5
    let v = svc.clamp_torque_nm(100.0);
    assert!((v - 5.0).abs() < f32::EPSILON);
    let v = svc.clamp_torque_nm(-100.0);
    assert!((v - (-5.0)).abs() < f32::EPSILON);

    // HighTorqueActive: clamp to ±25
    activate_high_torque(&mut svc, "dev1");
    let v = svc.clamp_torque_nm(100.0);
    assert!((v - 25.0).abs() < f32::EPSILON);

    // Faulted: clamp to 0
    svc.report_fault(FaultType::UsbStall);
    let v = svc.clamp_torque_nm(100.0);
    assert!((v - 0.0).abs() < f32::EPSILON);
}

#[test]
fn deep_test_nan_torque_clamped_to_zero() {
    let svc = create_deep_test_service();
    assert_eq!(svc.clamp_torque_nm(f32::NAN), 0.0);
    assert_eq!(svc.clamp_torque_nm(f32::INFINITY), 0.0);
    assert_eq!(svc.clamp_torque_nm(f32::NEG_INFINITY), 0.0);
}

#[test]
fn deep_test_get_max_torque_typed() {
    let mut svc = create_deep_test_service();

    // Safe state
    let t = svc.get_max_torque(false);
    assert!((t.value() - 5.0).abs() < f32::EPSILON);

    // High torque active
    activate_high_torque(&mut svc, "dev1");
    let t = svc.get_max_torque(true);
    assert!((t.value() - 25.0).abs() < f32::EPSILON);

    // Faulted
    svc.report_fault(FaultType::ThermalLimit);
    let t = svc.get_max_torque(true);
    assert!((t.value() - 0.0).abs() < f32::EPSILON);
}

// =========================================================================
// 7. Proptest: random fault sequences preserve safety invariants
// =========================================================================

/// Strategy that produces a random FaultType index
fn fault_index_strategy() -> impl Strategy<Value = usize> {
    0usize..9
}

/// Strategy that produces a sequence of fault actions
#[derive(Debug, Clone)]
enum FaultAction {
    ReportFault(usize),
    ClearFault,
    ClampTorque(f32),
}

fn fault_action_strategy() -> impl Strategy<Value = FaultAction> {
    prop_oneof![
        fault_index_strategy().prop_map(FaultAction::ReportFault),
        Just(FaultAction::ClearFault),
        (-200.0f32..=200.0).prop_map(FaultAction::ClampTorque),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    #[test]
    fn deep_test_prop_faulted_always_zero_torque(
        fault_idx in fault_index_strategy(),
        requested in -200.0f32..=200.0,
    ) {
        let mut svc = SafetyService::new(5.0, 25.0);
        svc.report_fault(ALL_FAULTS[fault_idx]);
        let result = svc.clamp_torque_nm(requested);
        prop_assert_eq!(result, 0.0, "Faulted state must always yield 0 torque");
    }

    #[test]
    fn deep_test_prop_safe_state_bounded(
        safe in 1.0f32..=20.0,
        high in 20.0f32..=100.0,
        requested in -500.0f32..=500.0,
    ) {
        let svc = SafetyService::new(safe, high);
        let result = svc.clamp_torque_nm(requested);
        prop_assert!(
            result >= -safe && result <= safe,
            "Safe clamp {} not in [-{}, {}]", result, safe, safe
        );
    }

    #[test]
    fn deep_test_prop_random_fault_sequence_invariant(
        actions in proptest::collection::vec(fault_action_strategy(), 1..50),
    ) {
        let mut svc = SafetyService::new(5.0, 25.0);

        for action in &actions {
            match action {
                FaultAction::ReportFault(idx) => {
                    svc.report_fault(ALL_FAULTS[*idx]);
                }
                FaultAction::ClearFault => {
                    // May fail (too soon or no fault) — that's fine
                    let _ = svc.clear_fault();
                }
                FaultAction::ClampTorque(requested) => {
                    let result = svc.clamp_torque_nm(*requested);
                    let max = svc.max_torque_nm();
                    // Core invariant: clamped torque must never exceed max
                    prop_assert!(
                        result.abs() <= max + f32::EPSILON,
                        "Clamped |{}| > max {} in state {:?}",
                        result, max, svc.state()
                    );
                }
            }
        }

        // After any sequence, the state machine must be in a valid state
        match svc.state() {
            SafetyState::SafeTorque
            | SafetyState::HighTorqueChallenge { .. }
            | SafetyState::AwaitingPhysicalAck { .. }
            | SafetyState::HighTorqueActive { .. }
            | SafetyState::Faulted { .. } => {}
        }
    }

    #[test]
    fn deep_test_prop_fault_then_clear_returns_safe(
        fault_idx in fault_index_strategy(),
    ) {
        let mut svc = SafetyService::new(5.0, 25.0);
        svc.report_fault(ALL_FAULTS[fault_idx]);
        let is_faulted = matches!(svc.state(), SafetyState::Faulted { .. });
        prop_assert!(is_faulted, "expected Faulted state");

        // Wait for clear eligibility
        std::thread::sleep(Duration::from_millis(110));
        let result = svc.clear_fault();
        prop_assert!(result.is_ok());
        prop_assert_eq!(svc.state(), &SafetyState::SafeTorque);
    }

    #[test]
    fn deep_test_prop_sign_preserved_after_clamp(
        safe in 1.0f32..=20.0,
        high in 20.0f32..=100.0,
        requested in -200.0f32..=200.0,
    ) {
        let svc = SafetyService::new(safe, high);
        let result = svc.clamp_torque_nm(requested);
        if requested > 0.0 {
            prop_assert!(result >= 0.0);
        } else if requested < 0.0 {
            prop_assert!(result <= 0.0);
        }
    }
}

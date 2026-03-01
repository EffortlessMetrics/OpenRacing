//! Tests for safety interlock system

use super::*;
use crate::hid::MozaInputState;
use crate::input::{KsClutchMode, KsReportSnapshot};
use openracing_test_helpers::prelude::*;
use std::time::{Duration, Instant};

/// Create a test safety service
fn create_test_service() -> SafetyService {
    SafetyService::with_timeouts(
        5.0,                    // max_safe_torque_nm
        25.0,                   // max_high_torque_nm
        Duration::from_secs(3), // hands_off_timeout
        Duration::from_secs(2), // combo_hold_duration
    )
}

#[test]
fn test_initial_state() {
    let service = create_test_service();

    assert_eq!(service.state(), &SafetyState::SafeTorque);
    assert_eq!(service.get_max_torque(false).value(), 5.0);
    assert!(!service.has_valid_token("test_device"));
    assert!(service.get_active_challenge().is_none());
}

#[test]
fn test_request_high_torque_success() {
    let mut service = create_test_service();

    let challenge = must(service.request_high_torque("test_device"));

    assert!(challenge.challenge_token != 0);
    assert_eq!(challenge.combo_required, ButtonCombo::BothClutchPaddles);
    assert!(!challenge.ui_consent_given);
    assert!(challenge.combo_start.is_none());

    // State should be HighTorqueChallenge
    match service.state() {
        SafetyState::HighTorqueChallenge {
            challenge_token,
            ui_consent_given,
            ..
        } => {
            assert_eq!(*challenge_token, challenge.challenge_token);
            assert!(!ui_consent_given);
        }
        _ => panic!("Expected HighTorqueChallenge state"),
    }
}

#[test]
fn test_request_high_torque_already_active() {
    let mut service = create_test_service();

    // First request should succeed
    let challenge = must(service.request_high_torque("test_device"));
    must(service.provide_ui_consent(challenge.challenge_token));

    let _ack = InterlockAck {
        challenge_token: challenge.challenge_token,
        device_token: 12345,
        combo_completed: ButtonCombo::BothClutchPaddles,
        timestamp: Instant::now(),
    };

    must(service.report_combo_start(challenge.challenge_token));
    std::thread::sleep(Duration::from_millis(2100)); // Wait for combo duration

    // Update the ack timestamp to be after the combo duration
    let ack = InterlockAck {
        challenge_token: challenge.challenge_token,
        device_token: 12345,
        combo_completed: ButtonCombo::BothClutchPaddles,
        timestamp: Instant::now(),
    };

    must(service.confirm_high_torque("test_device", ack));

    // Second request should fail
    let result = service.request_high_torque("test_device");
    assert!(result.is_err());
    assert!(must_some(result.err(), "expected error").contains("already active"));
}

#[test]
fn test_ui_consent_flow() {
    let mut service = create_test_service();

    let challenge = must(service.request_high_torque("test_device"));

    // Provide UI consent
    must(service.provide_ui_consent(challenge.challenge_token));

    // State should transition to AwaitingPhysicalAck
    match service.state() {
        SafetyState::AwaitingPhysicalAck {
            challenge_token, ..
        } => {
            assert_eq!(*challenge_token, challenge.challenge_token);
        }
        _ => panic!("Expected AwaitingPhysicalAck state"),
    }

    // Active challenge should be updated
    let active_challenge = must_some(service.get_active_challenge(), "expected active challenge");
    assert!(active_challenge.ui_consent_given);
}

#[test]
fn test_ui_consent_invalid_token() {
    let mut service = create_test_service();

    must(service.request_high_torque("test_device"));

    // Try to provide consent with wrong token
    let result = service.provide_ui_consent(99999);
    assert!(result.is_err());
    assert!(must_some(result.err(), "expected error").contains("Invalid challenge token"));
}

#[test]
fn test_physical_combo_flow() {
    let mut service = create_test_service();

    let challenge = must(service.request_high_torque("test_device"));
    must(service.provide_ui_consent(challenge.challenge_token));

    // Report combo start
    must(service.report_combo_start(challenge.challenge_token));

    // Active challenge should be updated
    let active_challenge = must_some(service.get_active_challenge(), "expected active challenge");
    assert!(active_challenge.combo_start.is_some());

    // Wait for combo duration
    std::thread::sleep(Duration::from_millis(2100));

    // Confirm with device acknowledgment
    let ack = InterlockAck {
        challenge_token: challenge.challenge_token,
        device_token: 12345,
        combo_completed: ButtonCombo::BothClutchPaddles,
        timestamp: Instant::now(),
    };

    must(service.confirm_high_torque("test_device", ack));

    // Should be in HighTorqueActive state
    match service.state() {
        SafetyState::HighTorqueActive { device_token, .. } => {
            assert_eq!(*device_token, 12345);
        }
        _ => panic!("Expected HighTorqueActive state"),
    }

    // Device should have valid token
    assert!(service.has_valid_token("test_device"));
    assert_eq!(service.get_max_torque(true).value(), 25.0);
}

#[test]
fn test_combo_insufficient_hold_duration() {
    let mut service = create_test_service();

    let challenge = must(service.request_high_torque("test_device"));
    must(service.provide_ui_consent(challenge.challenge_token));
    must(service.report_combo_start(challenge.challenge_token));

    // Don't wait long enough
    std::thread::sleep(Duration::from_millis(500));

    let ack = InterlockAck {
        challenge_token: challenge.challenge_token,
        device_token: 12345,
        combo_completed: ButtonCombo::BothClutchPaddles,
        timestamp: Instant::now(),
    };

    let result = service.confirm_high_torque("test_device", ack);
    assert!(result.is_err());
    assert!(must_some(result.err(), "expected error").contains("held for only"));
}

#[test]
fn test_challenge_expiry() {
    let mut service = create_test_service();

    let challenge = must(service.request_high_torque("test_device"));

    // Manually expire the challenge by setting state
    service.state = SafetyState::SafeTorque;
    service.active_challenge = None;

    // Check expiry
    assert!(!service.check_challenge_expiry()); // Should return false since already expired
    assert_eq!(service.state(), &SafetyState::SafeTorque);
    assert!(service.get_active_challenge().is_none());

    // Trying to provide consent should fail
    let result = service.provide_ui_consent(challenge.challenge_token);
    assert!(result.is_err());
    assert!(must_some(result.err(), "expected error").contains("No active challenge"));
}

#[test]
fn test_hands_on_timeout() {
    let mut service = create_test_service();

    // Activate high torque
    let challenge = must(service.request_high_torque("test_device"));
    must(service.provide_ui_consent(challenge.challenge_token));
    must(service.report_combo_start(challenge.challenge_token));
    std::thread::sleep(Duration::from_millis(2100));

    let ack = InterlockAck {
        challenge_token: challenge.challenge_token,
        device_token: 12345,
        combo_completed: ButtonCombo::BothClutchPaddles,
        timestamp: Instant::now(),
    };

    must(service.confirm_high_torque("test_device", ack));

    // Simulate hands-off for too long
    must(service.update_hands_on_status(false)); // Initial hands-off is OK

    // Wait for timeout
    std::thread::sleep(Duration::from_millis(3100));

    // Next update should trigger fault
    let result = service.update_hands_on_status(false);
    assert!(result.is_err());
    assert!(must_some(result.err(), "expected error").contains("Hands-off timeout"));

    // Should be in faulted state
    match service.state() {
        SafetyState::Faulted { fault, .. } => {
            assert_eq!(*fault, FaultType::HandsOffTimeout);
        }
        _ => panic!("Expected Faulted state"),
    }
}

#[test]
fn test_hands_on_reset_timeout() {
    let mut service = create_test_service();

    // Activate high torque
    let challenge = must(service.request_high_torque("test_device"));
    must(service.provide_ui_consent(challenge.challenge_token));
    must(service.report_combo_start(challenge.challenge_token));
    std::thread::sleep(Duration::from_millis(2100));

    let ack = InterlockAck {
        challenge_token: challenge.challenge_token,
        device_token: 12345,
        combo_completed: ButtonCombo::BothClutchPaddles,
        timestamp: Instant::now(),
    };

    must(service.confirm_high_torque("test_device", ack));

    // Hands-off for a while
    must(service.update_hands_on_status(false));
    std::thread::sleep(Duration::from_millis(2000));

    // Hands back on should reset timeout
    must(service.update_hands_on_status(true));

    // Wait again, but should not timeout since hands are on
    std::thread::sleep(Duration::from_millis(3100));
    must(service.update_hands_on_status(true)); // Should still be OK

    // Should still be in HighTorqueActive state
    assert!(matches!(
        service.state(),
        SafetyState::HighTorqueActive { .. }
    ));
}

#[test]
fn test_cancel_challenge() {
    let mut service = create_test_service();

    let challenge = must(service.request_high_torque("test_device"));
    must(service.provide_ui_consent(challenge.challenge_token));

    // Cancel the challenge
    must(service.cancel_challenge());

    assert_eq!(service.state(), &SafetyState::SafeTorque);
    assert!(service.get_active_challenge().is_none());
}

#[test]
fn test_disable_high_torque() {
    let mut service = create_test_service();

    // Activate high torque
    let challenge = must(service.request_high_torque("test_device"));
    must(service.provide_ui_consent(challenge.challenge_token));
    must(service.report_combo_start(challenge.challenge_token));
    std::thread::sleep(Duration::from_millis(2100));

    let ack = InterlockAck {
        challenge_token: challenge.challenge_token,
        device_token: 12345,
        combo_completed: ButtonCombo::BothClutchPaddles,
        timestamp: Instant::now(),
    };

    must(service.confirm_high_torque("test_device", ack));
    assert!(service.has_valid_token("test_device"));

    // Disable high torque
    must(service.disable_high_torque("test_device"));

    assert_eq!(service.state(), &SafetyState::SafeTorque);
    assert!(!service.has_valid_token("test_device"));
    assert_eq!(service.get_max_torque(false).value(), 5.0);
}

#[test]
fn test_fault_handling() {
    let mut service = create_test_service();

    // Report a fault
    service.report_fault(FaultType::ThermalLimit);

    match service.state() {
        SafetyState::Faulted { fault, .. } => {
            assert_eq!(*fault, FaultType::ThermalLimit);
        }
        _ => panic!("Expected Faulted state"),
    }

    assert_eq!(service.get_max_torque(false).value(), 0.0);

    // Cannot request high torque while faulted
    let result = service.request_high_torque("test_device");
    assert!(result.is_err());
    let error_msg = must_some(result.err(), "expected error");
    assert!(error_msg.contains("faulted") || error_msg.contains("active faults"));
}

#[test]
fn test_faulted_state_forces_zero_torque_even_when_high_torque_enabled() {
    let mut service = create_test_service();
    service.report_fault(FaultType::SafetyInterlockViolation);

    assert_eq!(service.get_max_torque(true).value(), 0.0);
    assert_eq!(service.get_max_torque(false).value(), 0.0);
}

#[test]
fn test_clamp_torque_nm_faulted_state_forces_zero() {
    let mut service = create_test_service();
    service.report_fault(FaultType::UsbStall);

    assert_eq!(service.clamp_torque_nm(25.0), 0.0);
    assert_eq!(service.clamp_torque_nm(-25.0), 0.0);
}

#[test]
fn test_clamp_torque_nm_respects_safe_limit() {
    let service = create_test_service();

    assert_eq!(service.clamp_torque_nm(10.0), 5.0);
    assert_eq!(service.clamp_torque_nm(-10.0), -5.0);
}

#[test]
fn test_clear_fault() {
    let mut service = create_test_service();

    service.report_fault(FaultType::UsbStall);

    // Wait minimum fault duration
    std::thread::sleep(Duration::from_millis(150));

    must(service.clear_fault());
    assert_eq!(service.state(), &SafetyState::SafeTorque);
}

#[test]
fn test_clear_fault_too_soon() {
    let mut service = create_test_service();

    service.report_fault(FaultType::UsbStall);

    // Try to clear immediately
    let result = service.clear_fault();
    assert!(result.is_err());
    assert!(must_some(result.err(), "expected error").contains("too short"));
}

#[test]
fn test_consent_requirements() {
    let service = create_test_service();
    let requirements = service.get_consent_requirements();

    assert_eq!(requirements.max_torque_nm, 25.0);
    assert!(requirements.requires_explicit_consent);
    assert!(!requirements.warnings.is_empty());
    assert!(!requirements.disclaimers.is_empty());
}

#[test]
fn test_challenge_time_remaining() {
    let mut service = create_test_service();

    // No active challenge
    assert!(service.get_challenge_time_remaining().is_none());

    // Start challenge
    must(service.request_high_torque("test_device"));

    // Should have time remaining
    let remaining = must_some(
        service.get_challenge_time_remaining(),
        "expected time remaining",
    );
    assert!(remaining > Duration::from_secs(25)); // Should be close to 30 seconds
    assert!(remaining <= Duration::from_secs(30));
}

#[test]
fn test_multiple_devices() {
    let mut service = create_test_service();

    // Activate high torque for device 1
    let challenge1 = must(service.request_high_torque("device1"));
    must(service.provide_ui_consent(challenge1.challenge_token));
    must(service.report_combo_start(challenge1.challenge_token));
    std::thread::sleep(Duration::from_millis(2100));

    let ack1 = InterlockAck {
        challenge_token: challenge1.challenge_token,
        device_token: 11111,
        combo_completed: ButtonCombo::BothClutchPaddles,
        timestamp: Instant::now(),
    };

    must(service.confirm_high_torque("device1", ack1));

    // Device 1 should have token, device 2 should not
    assert!(service.has_valid_token("device1"));
    assert!(!service.has_valid_token("device2"));

    // Disable for device 1
    must(service.disable_high_torque("device1"));
    assert!(!service.has_valid_token("device1"));
}

#[test]
fn test_button_combo_types() {
    let combo1 = ButtonCombo::BothClutchPaddles;
    let combo2 = ButtonCombo::CustomSequence(12345);

    assert_ne!(combo1, combo2);

    // Test serialization/deserialization would go here if needed
}

#[test]
fn test_interlock_ack_validation() {
    let mut service = create_test_service();

    let challenge = must(service.request_high_torque("test_device"));
    must(service.provide_ui_consent(challenge.challenge_token));
    must(service.report_combo_start(challenge.challenge_token));
    std::thread::sleep(Duration::from_millis(2100));

    // Wrong challenge token
    let wrong_ack = InterlockAck {
        challenge_token: 99999,
        device_token: 12345,
        combo_completed: ButtonCombo::BothClutchPaddles,
        timestamp: Instant::now(),
    };

    let result = service.confirm_high_torque("test_device", wrong_ack);
    assert!(result.is_err());
    assert!(must_some(result.err(), "expected error").contains("Invalid challenge token"));
}

#[test]
fn test_fault_type_display() {
    assert_eq!(FaultType::UsbStall.to_string(), "USB communication stall");
    assert_eq!(
        FaultType::HandsOffTimeout.to_string(),
        "Hands-off timeout exceeded"
    );
    assert_eq!(
        FaultType::SafetyInterlockViolation.to_string(),
        "Safety interlock violation"
    );
}

#[test]
fn test_state_transitions() {
    let mut service = create_test_service();

    // SafeTorque -> HighTorqueChallenge
    must(service.request_high_torque("test_device"));
    assert!(matches!(
        service.state(),
        SafetyState::HighTorqueChallenge { .. }
    ));

    // HighTorqueChallenge -> AwaitingPhysicalAck
    let challenge = must_some(service.get_active_challenge(), "expected active challenge");
    let challenge_token = challenge.challenge_token;
    must(service.provide_ui_consent(challenge_token));
    assert!(matches!(
        service.state(),
        SafetyState::AwaitingPhysicalAck { .. }
    ));

    // AwaitingPhysicalAck -> HighTorqueActive
    must(service.report_combo_start(challenge_token));
    std::thread::sleep(Duration::from_millis(2100));

    let ack = InterlockAck {
        challenge_token,
        device_token: 12345,
        combo_completed: ButtonCombo::BothClutchPaddles,
        timestamp: Instant::now(),
    };

    must(service.confirm_high_torque("test_device", ack));
    assert!(matches!(
        service.state(),
        SafetyState::HighTorqueActive { .. }
    ));

    // HighTorqueActive -> Faulted
    service.report_fault(FaultType::ThermalLimit);
    assert!(matches!(service.state(), SafetyState::Faulted { .. }));

    // Faulted -> SafeTorque
    std::thread::sleep(Duration::from_millis(150));
    must(service.clear_fault());
    assert_eq!(service.state(), &SafetyState::SafeTorque);
}

#[test]
fn test_moza_process_clutch_combo_starts_hold_timer() {
    let mut service = create_test_service();
    let challenge = must(service.request_high_torque("test_device"));
    must(service.provide_ui_consent(challenge.challenge_token));

    let mut input = MozaInputState::empty(0);
    input.clutch_u16 = 40_000;
    input.handbrake_u16 = 40_000;

    let progressed = service.process_moza_interlock_inputs("test_device", input, 30_000, 99);
    assert!(progressed);

    let active = must_some(service.get_active_challenge(), "expected active challenge");
    assert!(active.combo_start.is_some());
}

#[test]
fn test_moza_process_clutch_combo_confirms_high_torque_after_hold() {
    let mut service = create_test_service();
    let challenge = must(service.request_high_torque("test_device"));
    must(service.provide_ui_consent(challenge.challenge_token));

    let mut input = MozaInputState::empty(0);
    input.clutch_u16 = 40_000;
    input.handbrake_u16 = 40_000;

    assert!(service.process_moza_interlock_inputs("test_device", input, 30_000, 99));

    std::thread::sleep(Duration::from_millis(2100));

    assert!(service.process_moza_interlock_inputs("test_device", input, 30_000, 99));

    match service.state() {
        SafetyState::HighTorqueActive { device_token, .. } => {
            assert_eq!(*device_token, 99);
        }
        _ => panic!("expected high torque active after clutch hold"),
    }
}

#[test]
fn test_moza_interlock_combo_hold_cleared_when_released() {
    let mut service = create_test_service();
    let challenge = must(service.request_high_torque("test_device"));
    must(service.provide_ui_consent(challenge.challenge_token));

    let mut input = MozaInputState::empty(0);
    input.clutch_u16 = 40_000;
    input.handbrake_u16 = 40_000;

    assert!(service.process_moza_interlock_inputs("test_device", input, 30_000, 99));

    let active = must_some(service.get_active_challenge(), "expected active challenge");
    assert!(active.combo_start.is_some());

    let input_released = MozaInputState::empty(0);
    assert!(!service.process_moza_interlock_inputs("test_device", input_released, 30_000, 99));
    let active = must_some(service.get_active_challenge(), "expected active challenge");
    assert!(active.combo_start.is_none());
}

#[test]
fn test_moza_interlock_inputs_stale_resets_combo() {
    let mut service = create_test_service();
    let challenge = must(service.request_high_torque("test_device"));
    must(service.provide_ui_consent(challenge.challenge_token));

    let mut input = MozaInputState::empty(0);
    input.clutch_u16 = 40_000;
    input.handbrake_u16 = 40_000;

    assert!(service.process_moza_interlock_inputs("test_device", input, 30_000, 99));

    let active = must_some(service.get_active_challenge(), "expected active challenge");
    assert!(active.combo_start.is_some());

    service.process_moza_interlock_inputs_stale();
    let active = must_some(service.get_active_challenge(), "expected active challenge");
    assert!(active.combo_start.is_none());
}

#[test]
fn test_moza_process_ks_combined_axis_clutch_mode_for_interlock() {
    let mut service = create_test_service();
    let challenge = must(service.request_high_torque("test_device"));
    must(service.provide_ui_consent(challenge.challenge_token));

    let mut input = MozaInputState::empty(0);
    input.ks_snapshot = KsReportSnapshot {
        clutch_mode: KsClutchMode::CombinedAxis,
        clutch_combined: Some(40_000),
        ..KsReportSnapshot::default()
    };

    assert!(service.process_moza_interlock_inputs("test_device", input, 30_000, 99));

    let active = must_some(service.get_active_challenge(), "expected active challenge");
    assert!(active.combo_start.is_some());
}

#[test]
fn test_moza_process_ks_independent_axis_clutch_mode_for_interlock() {
    let mut service = create_test_service();
    let challenge = must(service.request_high_torque("test_device"));
    must(service.provide_ui_consent(challenge.challenge_token));

    let mut input = MozaInputState::empty(0);
    input.ks_snapshot = KsReportSnapshot {
        clutch_mode: KsClutchMode::IndependentAxis,
        clutch_left: Some(40_000),
        clutch_right: Some(40_000),
        ..KsReportSnapshot::default()
    };

    assert!(service.process_moza_interlock_inputs("test_device", input, 30_000, 99));
}

#[test]
fn test_moza_process_ks_button_mode_clutch_for_interlock() {
    let mut service = create_test_service();
    let challenge = must(service.request_high_torque("test_device"));
    must(service.provide_ui_consent(challenge.challenge_token));

    let mut input = MozaInputState::empty(0);
    input.ks_snapshot = KsReportSnapshot {
        clutch_mode: KsClutchMode::Button,
        clutch_left_button: Some(true),
        clutch_right_button: Some(true),
        ..KsReportSnapshot::default()
    };

    assert!(service.process_moza_interlock_inputs("test_device", input, 30_000, 99));

    let active = must_some(service.get_active_challenge(), "expected active challenge");
    assert!(active.combo_start.is_some());
}

// ---------------------------------------------------------------------------
// Safety-hardening tests: fault detection timing, multi-fault, edge cases
// ---------------------------------------------------------------------------

/// Helper: drive a SafetyService through the full high-torque activation flow.
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

#[test]
fn test_fault_detection_transitions_within_10ms() {
    // ADR-0006: Fault detection time ≤ 10ms
    let mut service = create_test_service();
    let before = Instant::now();
    service.report_fault(FaultType::Overcurrent);
    let elapsed = before.elapsed();

    assert!(
        elapsed < Duration::from_millis(10),
        "Fault detection took {elapsed:?}, exceeds 10ms budget"
    );
    assert!(matches!(service.state(), SafetyState::Faulted { .. }));
    assert_eq!(service.max_torque_nm(), 0.0);
}

#[test]
fn test_fault_to_safe_state_torque_zero_within_50ms() {
    // ADR-0006: Fault → safe-state (zero torque) within 50ms
    let mut service = create_test_service();
    activate_high_torque(&mut service, "dev");

    assert_eq!(service.max_torque_nm(), 25.0);

    let before = Instant::now();
    service.report_fault(FaultType::UsbStall);
    let clamped = service.clamp_torque_nm(25.0);
    let elapsed = before.elapsed();

    assert_eq!(clamped, 0.0, "Torque must be zero after fault");
    assert!(
        elapsed < Duration::from_millis(50),
        "Fault-to-safe-state took {elapsed:?}, exceeds 50ms budget"
    );
}

#[test]
fn test_multi_fault_all_reported_and_last_wins() {
    let mut service = create_test_service();

    service.report_fault(FaultType::UsbStall);
    assert!(matches!(
        service.state(),
        SafetyState::Faulted {
            fault: FaultType::UsbStall,
            ..
        }
    ));

    // Second fault overwrites the first (both recorded, last active)
    service.report_fault(FaultType::ThermalLimit);
    match service.state() {
        SafetyState::Faulted { fault, .. } => {
            assert_eq!(*fault, FaultType::ThermalLimit);
        }
        other => panic!("Expected Faulted, got {other:?}"),
    }
    // Torque remains zero regardless of which fault is active
    assert_eq!(service.max_torque_nm(), 0.0);
}

#[test]
fn test_rapid_fault_clear_cycling() {
    // Rapidly fault → wait → clear → fault should always keep safety invariants
    let mut service = create_test_service();

    for _ in 0..10 {
        service.report_fault(FaultType::EncoderNaN);
        assert_eq!(service.clamp_torque_nm(10.0), 0.0);

        std::thread::sleep(Duration::from_millis(110));
        must(service.clear_fault());
        assert_eq!(service.state(), &SafetyState::SafeTorque);
        assert!(service.clamp_torque_nm(3.0).abs() <= 5.0);
    }
}

#[test]
fn test_cannot_escalate_to_high_torque_with_any_active_fault_count() {
    let mut service = create_test_service();

    // Report and clear a fault so the count is non-zero
    service.report_fault(FaultType::TimingViolation);
    std::thread::sleep(Duration::from_millis(110));
    must(service.clear_fault());

    // Attempting high torque should fail because fault_count > 0
    let result = service.request_high_torque("dev");
    assert!(result.is_err());
}

#[test]
fn test_all_fault_types_transition_to_faulted_state() {
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
            "Fault {fault:?} did not transition to Faulted"
        );
        assert_eq!(
            service.clamp_torque_nm(100.0),
            0.0,
            "Fault {fault:?} did not zero torque"
        );
    }
}

#[test]
fn test_challenge_response_wrong_token_at_every_step() {
    let mut service = create_test_service();
    let challenge = must(service.request_high_torque("dev"));

    // Wrong token for UI consent
    assert!(
        service
            .provide_ui_consent(challenge.challenge_token.wrapping_add(1))
            .is_err()
    );

    // Correct UI consent to proceed
    must(service.provide_ui_consent(challenge.challenge_token));

    // Wrong token for combo start
    assert!(
        service
            .report_combo_start(challenge.challenge_token.wrapping_add(1))
            .is_err()
    );

    // Correct combo start
    must(service.report_combo_start(challenge.challenge_token));
    std::thread::sleep(Duration::from_millis(2100));

    // Wrong token for confirmation
    let bad_ack = InterlockAck {
        challenge_token: challenge.challenge_token.wrapping_add(1),
        device_token: 1,
        combo_completed: ButtonCombo::BothClutchPaddles,
        timestamp: Instant::now(),
    };
    assert!(service.confirm_high_torque("dev", bad_ack).is_err());

    // Should still be in AwaitingPhysicalAck – not promoted
    assert!(matches!(
        service.state(),
        SafetyState::AwaitingPhysicalAck { .. }
    ));
}

#[test]
fn test_interlock_passthrough_safe_torque_during_challenge() {
    // During the challenge flow, torque must remain at safe limit
    let mut service = create_test_service();
    let challenge = must(service.request_high_torque("dev"));

    // In HighTorqueChallenge
    assert_eq!(service.max_torque_nm(), 5.0);
    assert_eq!(service.clamp_torque_nm(20.0), 5.0);

    must(service.provide_ui_consent(challenge.challenge_token));

    // In AwaitingPhysicalAck
    assert_eq!(service.max_torque_nm(), 5.0);
    assert_eq!(service.clamp_torque_nm(20.0), 5.0);
}

#[test]
fn test_clear_fault_before_minimum_duration_rejected() {
    let mut service = create_test_service();
    service.report_fault(FaultType::Overcurrent);

    // Immediate clear should fail (< 100ms)
    let result = service.clear_fault();
    assert!(result.is_err());
    assert!(matches!(service.state(), SafetyState::Faulted { .. }));
}

#[test]
fn test_hands_off_timeout_only_in_high_torque_active() {
    let mut service = create_test_service();

    // In SafeTorque, hands-off update should be a no-op (Ok)
    must(service.update_hands_on_status(false));
    assert_eq!(service.state(), &SafetyState::SafeTorque);
}

#[test]
fn test_high_torque_flag_ignored_outside_active_state() {
    let service = create_test_service();

    // Even with is_high_torque_enabled=true, SafeTorque returns safe limit
    assert_eq!(service.get_max_torque(true).value(), 5.0);
}

#[test]
fn test_cancel_challenge_from_high_torque_challenge_state() {
    let mut service = create_test_service();
    must(service.request_high_torque("dev"));

    // Cancel before providing UI consent
    must(service.cancel_challenge());
    assert_eq!(service.state(), &SafetyState::SafeTorque);
    assert!(service.get_active_challenge().is_none());
}

#[test]
fn test_cancel_from_non_challenge_state_returns_error() {
    let mut service = create_test_service();
    assert!(service.cancel_challenge().is_err());
}

#[test]
fn test_disable_high_torque_from_non_active_state_returns_error() {
    let mut service = create_test_service();
    assert!(service.disable_high_torque("dev").is_err());
}

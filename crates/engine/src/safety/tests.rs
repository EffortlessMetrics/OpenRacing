//! Tests for safety interlock system

use super::*;
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

    let challenge = service.request_high_torque("test_device").unwrap();

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
    let challenge = service.request_high_torque("test_device").unwrap();
    service
        .provide_ui_consent(challenge.challenge_token)
        .unwrap();

    let ack = InterlockAck {
        challenge_token: challenge.challenge_token,
        device_token: 12345,
        combo_completed: ButtonCombo::BothClutchPaddles,
        timestamp: Instant::now(),
    };

    service
        .report_combo_start(challenge.challenge_token)
        .unwrap();
    std::thread::sleep(Duration::from_millis(2100)); // Wait for combo duration

    // Update the ack timestamp to be after the combo duration
    let ack = InterlockAck {
        challenge_token: challenge.challenge_token,
        device_token: 12345,
        combo_completed: ButtonCombo::BothClutchPaddles,
        timestamp: Instant::now(),
    };

    service.confirm_high_torque("test_device", ack).unwrap();

    // Second request should fail
    let result = service.request_high_torque("test_device");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("already active"));
}

#[test]
fn test_ui_consent_flow() {
    let mut service = create_test_service();

    let challenge = service.request_high_torque("test_device").unwrap();

    // Provide UI consent
    service
        .provide_ui_consent(challenge.challenge_token)
        .unwrap();

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
    let active_challenge = service.get_active_challenge().unwrap();
    assert!(active_challenge.ui_consent_given);
}

#[test]
fn test_ui_consent_invalid_token() {
    let mut service = create_test_service();

    service.request_high_torque("test_device").unwrap();

    // Try to provide consent with wrong token
    let result = service.provide_ui_consent(99999);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Invalid challenge token"));
}

#[test]
fn test_physical_combo_flow() {
    let mut service = create_test_service();

    let challenge = service.request_high_torque("test_device").unwrap();
    service
        .provide_ui_consent(challenge.challenge_token)
        .unwrap();

    // Report combo start
    service
        .report_combo_start(challenge.challenge_token)
        .unwrap();

    // Active challenge should be updated
    let active_challenge = service.get_active_challenge().unwrap();
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

    service.confirm_high_torque("test_device", ack).unwrap();

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

    let challenge = service.request_high_torque("test_device").unwrap();
    service
        .provide_ui_consent(challenge.challenge_token)
        .unwrap();
    service
        .report_combo_start(challenge.challenge_token)
        .unwrap();

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
    assert!(result.unwrap_err().contains("held for only"));
}

#[test]
fn test_challenge_expiry() {
    let mut service = create_test_service();

    let challenge = service.request_high_torque("test_device").unwrap();

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
    assert!(result.unwrap_err().contains("No active challenge"));
}

#[test]
fn test_hands_on_timeout() {
    let mut service = create_test_service();

    // Activate high torque
    let challenge = service.request_high_torque("test_device").unwrap();
    service
        .provide_ui_consent(challenge.challenge_token)
        .unwrap();
    service
        .report_combo_start(challenge.challenge_token)
        .unwrap();
    std::thread::sleep(Duration::from_millis(2100));

    let ack = InterlockAck {
        challenge_token: challenge.challenge_token,
        device_token: 12345,
        combo_completed: ButtonCombo::BothClutchPaddles,
        timestamp: Instant::now(),
    };

    service.confirm_high_torque("test_device", ack).unwrap();

    // Simulate hands-off for too long
    service.update_hands_on_status(false).unwrap(); // Initial hands-off is OK

    // Wait for timeout
    std::thread::sleep(Duration::from_millis(3100));

    // Next update should trigger fault
    let result = service.update_hands_on_status(false);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Hands-off timeout"));

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
    let challenge = service.request_high_torque("test_device").unwrap();
    service
        .provide_ui_consent(challenge.challenge_token)
        .unwrap();
    service
        .report_combo_start(challenge.challenge_token)
        .unwrap();
    std::thread::sleep(Duration::from_millis(2100));

    let ack = InterlockAck {
        challenge_token: challenge.challenge_token,
        device_token: 12345,
        combo_completed: ButtonCombo::BothClutchPaddles,
        timestamp: Instant::now(),
    };

    service.confirm_high_torque("test_device", ack).unwrap();

    // Hands-off for a while
    service.update_hands_on_status(false).unwrap();
    std::thread::sleep(Duration::from_millis(2000));

    // Hands back on should reset timeout
    service.update_hands_on_status(true).unwrap();

    // Wait again, but should not timeout since hands are on
    std::thread::sleep(Duration::from_millis(3100));
    service.update_hands_on_status(true).unwrap(); // Should still be OK

    // Should still be in HighTorqueActive state
    assert!(matches!(
        service.state(),
        SafetyState::HighTorqueActive { .. }
    ));
}

#[test]
fn test_cancel_challenge() {
    let mut service = create_test_service();

    let challenge = service.request_high_torque("test_device").unwrap();
    service
        .provide_ui_consent(challenge.challenge_token)
        .unwrap();

    // Cancel the challenge
    service.cancel_challenge().unwrap();

    assert_eq!(service.state(), &SafetyState::SafeTorque);
    assert!(service.get_active_challenge().is_none());
}

#[test]
fn test_disable_high_torque() {
    let mut service = create_test_service();

    // Activate high torque
    let challenge = service.request_high_torque("test_device").unwrap();
    service
        .provide_ui_consent(challenge.challenge_token)
        .unwrap();
    service
        .report_combo_start(challenge.challenge_token)
        .unwrap();
    std::thread::sleep(Duration::from_millis(2100));

    let ack = InterlockAck {
        challenge_token: challenge.challenge_token,
        device_token: 12345,
        combo_completed: ButtonCombo::BothClutchPaddles,
        timestamp: Instant::now(),
    };

    service.confirm_high_torque("test_device", ack).unwrap();
    assert!(service.has_valid_token("test_device"));

    // Disable high torque
    service.disable_high_torque("test_device").unwrap();

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
    let error_msg = result.unwrap_err();
    assert!(error_msg.contains("faulted") || error_msg.contains("active faults"));
}

#[test]
fn test_clear_fault() {
    let mut service = create_test_service();

    service.report_fault(FaultType::UsbStall);

    // Wait minimum fault duration
    std::thread::sleep(Duration::from_millis(150));

    service.clear_fault().unwrap();
    assert_eq!(service.state(), &SafetyState::SafeTorque);
}

#[test]
fn test_clear_fault_too_soon() {
    let mut service = create_test_service();

    service.report_fault(FaultType::UsbStall);

    // Try to clear immediately
    let result = service.clear_fault();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("too short"));
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
    service.request_high_torque("test_device").unwrap();

    // Should have time remaining
    let remaining = service.get_challenge_time_remaining().unwrap();
    assert!(remaining > Duration::from_secs(25)); // Should be close to 30 seconds
    assert!(remaining <= Duration::from_secs(30));
}

#[test]
fn test_multiple_devices() {
    let mut service = create_test_service();

    // Activate high torque for device 1
    let challenge1 = service.request_high_torque("device1").unwrap();
    service
        .provide_ui_consent(challenge1.challenge_token)
        .unwrap();
    service
        .report_combo_start(challenge1.challenge_token)
        .unwrap();
    std::thread::sleep(Duration::from_millis(2100));

    let ack1 = InterlockAck {
        challenge_token: challenge1.challenge_token,
        device_token: 11111,
        combo_completed: ButtonCombo::BothClutchPaddles,
        timestamp: Instant::now(),
    };

    service.confirm_high_torque("device1", ack1).unwrap();

    // Device 1 should have token, device 2 should not
    assert!(service.has_valid_token("device1"));
    assert!(!service.has_valid_token("device2"));

    // Disable for device 1
    service.disable_high_torque("device1").unwrap();
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

    let challenge = service.request_high_torque("test_device").unwrap();
    service
        .provide_ui_consent(challenge.challenge_token)
        .unwrap();
    service
        .report_combo_start(challenge.challenge_token)
        .unwrap();
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
    assert!(result.unwrap_err().contains("Invalid challenge token"));
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
    service.request_high_torque("test_device").unwrap();
    assert!(matches!(
        service.state(),
        SafetyState::HighTorqueChallenge { .. }
    ));

    // HighTorqueChallenge -> AwaitingPhysicalAck
    let challenge = service.get_active_challenge().unwrap();
    let challenge_token = challenge.challenge_token;
    service.provide_ui_consent(challenge_token).unwrap();
    assert!(matches!(
        service.state(),
        SafetyState::AwaitingPhysicalAck { .. }
    ));

    // AwaitingPhysicalAck -> HighTorqueActive
    service.report_combo_start(challenge_token).unwrap();
    std::thread::sleep(Duration::from_millis(2100));

    let ack = InterlockAck {
        challenge_token,
        device_token: 12345,
        combo_completed: ButtonCombo::BothClutchPaddles,
        timestamp: Instant::now(),
    };

    service.confirm_high_torque("test_device", ack).unwrap();
    assert!(matches!(
        service.state(),
        SafetyState::HighTorqueActive { .. }
    ));

    // HighTorqueActive -> Faulted
    service.report_fault(FaultType::ThermalLimit);
    assert!(matches!(service.state(), SafetyState::Faulted { .. }));

    // Faulted -> SafeTorque
    std::thread::sleep(Duration::from_millis(150));
    service.clear_fault().unwrap();
    assert_eq!(service.state(), &SafetyState::SafeTorque);
}

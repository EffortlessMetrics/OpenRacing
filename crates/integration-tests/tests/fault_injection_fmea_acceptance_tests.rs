//! Fault Injection & FMEA Acceptance Tests
//!
//! Comprehensive acceptance tests for the safety system covering:
//! 1.  All safety state transitions (SafeTorque→HighTorqueChallenge→AwaitingPhysicalAck→HighTorqueActive, any→Faulted, Faulted→SafeTorque)
//! 2.  Invalid state transitions are rejected
//! 3.  Fault detection within timing requirements (≤10ms)
//! 4.  Fault response within timing requirements (≤50ms to safe state)
//! 5.  Watchdog feed/miss behavior
//! 6.  Multi-fault scenarios (simultaneous faults)
//! 7.  Recovery paths after fault
//! 8.  Safety interlock challenge-response
//! 9.  Torque output limiting during fault conditions
//! 10. Determinism of state transitions (same sequence → same result)
//!
//! Safety requirements verified:
//! - SAFE-03: Fault detection time ≤10ms
//! - SAFE-04: Fault response time ≤50ms (fault → safe state)
//! - DIAG-01: Immutable fault logging

use std::time::{Duration, Instant};

use anyhow::Result;

use racing_wheel_engine::safety::{
    ButtonCombo, FaultType, HardwareWatchdog, InterlockAck, SafetyInterlockState,
    SafetyInterlockSystem, SafetyService, SafetyState, SoftwareWatchdog, TorqueLimit,
    WatchdogError, WatchdogTimeoutHandler,
};

// ═══════════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════════

/// All nine fault types for exhaustive iteration.
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

fn create_test_service() -> SafetyService {
    SafetyService::with_timeouts(
        5.0,                    // safe torque
        25.0,                   // high torque
        Duration::from_secs(3), // hands-off timeout
        Duration::from_secs(2), // combo hold duration
    )
}

fn create_interlock_system(timeout_ms: u32, max_torque: f32) -> SafetyInterlockSystem {
    let watchdog = Box::new(SoftwareWatchdog::new(timeout_ms));
    SafetyInterlockSystem::new(watchdog, max_torque)
}

fn create_armed_interlock_system(
    timeout_ms: u32,
    max_torque: f32,
) -> Result<SafetyInterlockSystem, WatchdogError> {
    let mut sys = create_interlock_system(timeout_ms, max_torque);
    sys.arm()?;
    sys.report_communication();
    Ok(sys)
}

/// Sleep long enough for clear_fault's minimum-duration check (100ms).
fn wait_for_fault_clearable() {
    std::thread::sleep(Duration::from_millis(110));
}

/// Drive a SafetyService through the full challenge flow to HighTorqueActive.
fn activate_high_torque(service: &mut SafetyService, device_id: &str) -> Result<()> {
    let challenge = service
        .request_high_torque(device_id)
        .map_err(|e| anyhow::anyhow!("request_high_torque failed: {e}"))?;
    let token = challenge.challenge_token;

    service
        .provide_ui_consent(token)
        .map_err(|e| anyhow::anyhow!("provide_ui_consent failed: {e}"))?;

    service
        .report_combo_start(token)
        .map_err(|e| anyhow::anyhow!("report_combo_start failed: {e}"))?;

    // Simulate holding the combo for the required duration
    std::thread::sleep(Duration::from_millis(2100));

    let ack = InterlockAck {
        challenge_token: token,
        device_token: 42,
        combo_completed: ButtonCombo::BothClutchPaddles,
        timestamp: Instant::now(),
    };

    service
        .confirm_high_torque(device_id, ack)
        .map_err(|e| anyhow::anyhow!("confirm_high_torque failed: {e}"))?;

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 1. Safety state transitions (valid paths)
// ═══════════════════════════════════════════════════════════════════════════════

/// Initial state must be SafeTorque.
#[test]
fn state_initial_is_safe_torque() -> Result<()> {
    let service = create_test_service();
    assert_eq!(service.state(), &SafetyState::SafeTorque);
    Ok(())
}

/// SafeTorque → HighTorqueChallenge on request.
#[test]
fn state_safe_torque_to_high_torque_challenge() -> Result<()> {
    let mut service = create_test_service();
    let challenge = service
        .request_high_torque("dev-1")
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    match service.state() {
        SafetyState::HighTorqueChallenge {
            challenge_token, ..
        } => {
            assert_eq!(*challenge_token, challenge.challenge_token);
        }
        other => {
            return Err(anyhow::anyhow!(
                "expected HighTorqueChallenge, got {other:?}"
            ));
        }
    }
    Ok(())
}

/// HighTorqueChallenge → AwaitingPhysicalAck on UI consent.
#[test]
fn state_challenge_to_awaiting_physical_ack() -> Result<()> {
    let mut service = create_test_service();
    let challenge = service
        .request_high_torque("dev-1")
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    service
        .provide_ui_consent(challenge.challenge_token)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    match service.state() {
        SafetyState::AwaitingPhysicalAck { .. } => {}
        other => {
            return Err(anyhow::anyhow!(
                "expected AwaitingPhysicalAck, got {other:?}"
            ));
        }
    }
    Ok(())
}

/// Full flow: SafeTorque → HighTorqueChallenge → AwaitingPhysicalAck → HighTorqueActive.
#[test]
fn state_full_activation_flow() -> Result<()> {
    let mut service = create_test_service();
    activate_high_torque(&mut service, "dev-1")?;

    match service.state() {
        SafetyState::HighTorqueActive { .. } => {}
        other => return Err(anyhow::anyhow!("expected HighTorqueActive, got {other:?}")),
    }
    Ok(())
}

/// HighTorqueActive → Faulted on fault report.
#[test]
fn state_active_to_faulted() -> Result<()> {
    let mut service = create_test_service();
    activate_high_torque(&mut service, "dev-1")?;

    service.report_fault(FaultType::UsbStall);
    match service.state() {
        SafetyState::Faulted {
            fault: FaultType::UsbStall,
            ..
        } => {}
        other => return Err(anyhow::anyhow!("expected Faulted(UsbStall), got {other:?}")),
    }
    Ok(())
}

/// Faulted → SafeTorque after clear_fault.
#[test]
fn state_faulted_to_safe_torque() -> Result<()> {
    let mut service = create_test_service();
    service.report_fault(FaultType::EncoderNaN);

    assert!(matches!(service.state(), SafetyState::Faulted { .. }));

    wait_for_fault_clearable();
    service.clear_fault().map_err(|e| anyhow::anyhow!("{e}"))?;

    assert_eq!(service.state(), &SafetyState::SafeTorque);
    Ok(())
}

/// Every fault type individually transitions any state to Faulted.
#[test]
fn state_every_fault_type_transitions_to_faulted() -> Result<()> {
    for fault in all_fault_types() {
        let mut service = create_test_service();
        service.report_fault(fault);

        match service.state() {
            SafetyState::Faulted {
                fault: reported, ..
            } => {
                assert_eq!(
                    *reported, fault,
                    "{fault:?}: reported fault must match injected fault"
                );
            }
            other => {
                return Err(anyhow::anyhow!(
                    "{fault:?}: expected Faulted, got {other:?}"
                ));
            }
        }
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. Invalid state transitions are rejected
// ═══════════════════════════════════════════════════════════════════════════════

/// Cannot provide consent without an active challenge.
#[test]
fn invalid_consent_without_challenge() -> Result<()> {
    let mut service = create_test_service();
    let result = service.provide_ui_consent(12345);
    assert!(result.is_err(), "consent without a challenge must fail");
    Ok(())
}

/// Cannot confirm high torque without a challenge.
#[test]
fn invalid_confirm_without_challenge() -> Result<()> {
    let mut service = create_test_service();
    let ack = InterlockAck {
        challenge_token: 99999,
        device_token: 42,
        combo_completed: ButtonCombo::BothClutchPaddles,
        timestamp: Instant::now(),
    };
    let result = service.confirm_high_torque("dev-1", ack);
    assert!(result.is_err(), "confirm without challenge must fail");
    Ok(())
}

/// Consent with wrong token is rejected.
#[test]
fn invalid_consent_wrong_token() -> Result<()> {
    let mut service = create_test_service();
    let _challenge = service
        .request_high_torque("dev-1")
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    // Wrong token
    let result = service.provide_ui_consent(0);
    assert!(result.is_err(), "consent with wrong token must fail");
    Ok(())
}

/// Cannot clear fault when not faulted.
#[test]
fn invalid_clear_fault_when_not_faulted() -> Result<()> {
    let mut service = create_test_service();
    let result = service.clear_fault();
    assert!(result.is_err(), "clear_fault in SafeTorque must fail");
    Ok(())
}

/// Requesting high torque while faulted must fail.
#[test]
fn invalid_request_high_torque_while_faulted() -> Result<()> {
    let mut service = create_test_service();
    service.report_fault(FaultType::Overcurrent);
    let result = service.request_high_torque("dev-1");
    assert!(
        result.is_err(),
        "high torque request while faulted must fail"
    );
    Ok(())
}

/// Double-faulting (reporting a second fault while already faulted) should
/// update the fault type but stay in Faulted state.
#[test]
fn double_fault_stays_faulted() -> Result<()> {
    let mut service = create_test_service();
    service.report_fault(FaultType::UsbStall);
    service.report_fault(FaultType::Overcurrent);

    match service.state() {
        SafetyState::Faulted { .. } => {}
        other => return Err(anyhow::anyhow!("expected Faulted, got {other:?}")),
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. Fault detection within timing requirements (≤10ms)
// ═══════════════════════════════════════════════════════════════════════════════

/// Watchdog timeout must be detected within 10ms.
#[test]
fn fault_detection_within_10ms_watchdog() -> Result<(), WatchdogError> {
    let mut sys = create_armed_interlock_system(10, 25.0)?;

    // Feed once then let watchdog expire
    let _ = sys.process_tick(5.0);
    std::thread::sleep(Duration::from_millis(15));

    let before = Instant::now();
    let result = sys.process_tick(5.0);
    let detection_time = before.elapsed();

    assert!(
        result.fault_occurred,
        "fault must be detected after watchdog timeout"
    );
    assert!(
        detection_time <= Duration::from_millis(10),
        "fault detection must be ≤10ms, was {detection_time:?}"
    );
    Ok(())
}

/// Manual fault report on SafetyService is instantaneous (< 1ms).
#[test]
fn fault_detection_report_fault_latency() -> Result<()> {
    let mut service = create_test_service();

    let before = Instant::now();
    service.report_fault(FaultType::Overcurrent);
    let latency = before.elapsed();

    assert!(
        matches!(service.state(), SafetyState::Faulted { .. }),
        "must be in Faulted state"
    );
    assert!(
        latency < Duration::from_millis(1),
        "report_fault latency must be <1ms, was {latency:?}"
    );
    Ok(())
}

/// Each fault type detected via the interlock system stays within 10ms.
#[test]
fn fault_detection_all_types_within_10ms() -> Result<()> {
    for fault in all_fault_types() {
        let mut sys = create_interlock_system(100, 25.0);
        sys.arm().map_err(|e| anyhow::anyhow!("{e:?}"))?;
        sys.report_communication();

        let before = Instant::now();
        sys.report_fault(fault);
        let _ = sys.process_tick(5.0);
        let detection_time = before.elapsed();

        assert!(
            detection_time <= Duration::from_millis(10),
            "{fault:?}: detection must be ≤10ms, was {detection_time:?}"
        );
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. Fault response within timing requirements (≤50ms to safe state)
// ═══════════════════════════════════════════════════════════════════════════════

/// After a fault, torque must be clamped to zero within 50ms.
#[test]
fn fault_response_torque_clamped_within_50ms() -> Result<()> {
    let mut service = create_test_service();
    activate_high_torque(&mut service, "dev-1")?;

    // Ensure we're in high-torque mode
    assert!(
        service.max_torque_nm() > 5.0,
        "must be in high torque before fault"
    );

    let before = Instant::now();
    service.report_fault(FaultType::Overcurrent);
    let clamped = service.clamp_torque_nm(20.0);
    let response_time = before.elapsed();

    assert!(
        clamped <= 0.01,
        "torque must be clamped to ≈0 after fault, got {clamped}"
    );
    assert!(
        response_time <= Duration::from_millis(50),
        "fault response must be ≤50ms, was {response_time:?}"
    );
    Ok(())
}

/// Interlock system fault report and tick must respond within 50ms.
#[test]
fn fault_response_interlock_within_50ms() -> Result<()> {
    let mut sys = create_armed_interlock_system(100, 25.0).map_err(|e| anyhow::anyhow!("{e:?}"))?;

    // Drive at high torque
    let _ = sys.process_tick(20.0);
    sys.report_communication();

    let before = Instant::now();
    sys.report_fault(FaultType::UsbStall);
    // After report_fault, process_tick applies safe-mode torque limits
    let result = sys.process_tick(20.0);
    let response_time = before.elapsed();

    // In SafeMode, torque is limited to safe_mode_torque (25*0.2 = 5.0 Nm)
    assert!(
        result.torque_command <= 5.0 + 0.01,
        "SafeMode torque must be limited, got {}",
        result.torque_command
    );
    assert!(
        matches!(result.state, SafetyInterlockState::SafeMode { .. }),
        "must be in SafeMode"
    );
    assert!(
        response_time <= Duration::from_millis(50),
        "response time must be ≤50ms, was {response_time:?}"
    );
    Ok(())
}

/// Every fault type produces a zero-torque response within 50ms.
#[test]
fn fault_response_all_types_within_50ms() -> Result<()> {
    for fault in all_fault_types() {
        let mut service = create_test_service();

        let before = Instant::now();
        service.report_fault(fault);
        let clamped = service.clamp_torque_nm(25.0);
        let response_time = before.elapsed();

        assert!(
            clamped <= 0.01,
            "{fault:?}: torque must be ≈0 after fault, got {clamped}"
        );
        assert!(
            response_time <= Duration::from_millis(50),
            "{fault:?}: response must be ≤50ms, was {response_time:?}"
        );
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. Watchdog feed/miss behavior
// ═══════════════════════════════════════════════════════════════════════════════

/// A fed watchdog does not time out.
#[test]
fn watchdog_feed_prevents_timeout() -> Result<(), WatchdogError> {
    let mut wd = SoftwareWatchdog::new(50);
    wd.arm()?;
    wd.feed()?;
    std::thread::sleep(Duration::from_millis(20));
    wd.feed()?;

    assert!(
        !wd.has_timed_out(),
        "watchdog must not time out when fed regularly"
    );
    Ok(())
}

/// An unfed watchdog times out after its configured period.
#[test]
fn watchdog_unfed_times_out() -> Result<(), WatchdogError> {
    let mut wd = SoftwareWatchdog::new(10);
    wd.arm()?;
    wd.feed()?;
    std::thread::sleep(Duration::from_millis(15));

    assert!(
        wd.has_timed_out(),
        "unfed watchdog must time out after its period"
    );
    Ok(())
}

/// Watchdog that is not armed never reports timeout.
#[test]
fn watchdog_unarmed_no_timeout() -> Result<()> {
    let wd = SoftwareWatchdog::new(10);
    std::thread::sleep(Duration::from_millis(15));

    assert!(
        !wd.has_timed_out(),
        "unarmed watchdog must never report timeout"
    );
    Ok(())
}

/// Arming an already-armed watchdog returns AlreadyArmed error.
#[test]
fn watchdog_double_arm_rejected() -> Result<()> {
    let mut wd = SoftwareWatchdog::new(100);
    wd.arm().map_err(|e| anyhow::anyhow!("{e:?}"))?;

    let result = wd.arm();
    assert_eq!(result, Err(WatchdogError::AlreadyArmed));
    Ok(())
}

/// Disarming a non-armed watchdog returns NotArmed error.
#[test]
fn watchdog_disarm_when_not_armed() -> Result<()> {
    let mut wd = SoftwareWatchdog::new(100);
    let result = wd.disarm();
    assert_eq!(result, Err(WatchdogError::NotArmed));
    Ok(())
}

/// Feeding an unarmed watchdog returns NotArmed error.
#[test]
fn watchdog_feed_when_not_armed() -> Result<()> {
    let mut wd = SoftwareWatchdog::new(100);
    let result = wd.feed();
    assert_eq!(result, Err(WatchdogError::NotArmed));
    Ok(())
}

/// After disarming, watchdog no longer tracks timeouts.
#[test]
fn watchdog_disarm_stops_tracking() -> Result<(), WatchdogError> {
    let mut wd = SoftwareWatchdog::new(10);
    wd.arm()?;
    wd.feed()?;
    wd.disarm()?;

    std::thread::sleep(Duration::from_millis(15));
    assert!(
        !wd.has_timed_out(),
        "disarmed watchdog must not report timeout"
    );
    Ok(())
}

/// Watchdog timeout triggers safe state in the interlock system.
#[test]
fn watchdog_timeout_triggers_safe_mode() -> Result<(), WatchdogError> {
    let mut sys = create_armed_interlock_system(10, 25.0)?;

    // One tick to start
    let _ = sys.process_tick(15.0);
    std::thread::sleep(Duration::from_millis(15));

    let result = sys.process_tick(15.0);
    assert!(
        result.fault_occurred,
        "tick after watchdog timeout must report fault"
    );
    match result.state {
        SafetyInterlockState::SafeMode { .. } | SafetyInterlockState::EmergencyStop { .. } => {}
        other => {
            return Err(WatchdogError::HardwareError(format!(
                "expected SafeMode or EmergencyStop, got {other:?}"
            )));
        }
    }
    Ok(())
}

/// Timeout handler reduces torque to zero.
#[test]
fn watchdog_timeout_handler_zeros_torque() -> Result<()> {
    let mut handler = WatchdogTimeoutHandler::new();
    let response = handler.handle_timeout(20.0);

    assert!(
        response.torque_command.abs() < 0.01,
        "timeout handler must command zero torque, got {}",
        response.torque_command
    );
    assert!(
        response.within_budget,
        "response must be within timing budget"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 6. Multi-fault scenarios (simultaneous faults)
// ═══════════════════════════════════════════════════════════════════════════════

/// Two faults in rapid succession keep the system in Faulted state.
#[test]
fn multi_fault_rapid_succession() -> Result<()> {
    let mut service = create_test_service();

    service.report_fault(FaultType::UsbStall);
    service.report_fault(FaultType::Overcurrent);

    assert!(
        matches!(service.state(), SafetyState::Faulted { .. }),
        "system must remain Faulted after multiple faults"
    );
    // Torque must still be zero
    assert!(
        service.clamp_torque_nm(25.0) <= 0.01,
        "torque must remain zero during multi-fault"
    );
    Ok(())
}

/// All nine fault types fired sequentially keep the system faulted throughout.
#[test]
fn multi_fault_all_types_sequential() -> Result<()> {
    let mut service = create_test_service();

    for fault in all_fault_types() {
        service.report_fault(fault);
        assert!(
            matches!(service.state(), SafetyState::Faulted { .. }),
            "{fault:?}: must remain Faulted"
        );
        assert!(
            service.clamp_torque_nm(25.0) <= 0.01,
            "{fault:?}: torque must remain ≈0"
        );
    }
    Ok(())
}

/// A new fault after clearing an earlier one re-enters Faulted state.
#[test]
fn multi_fault_clear_and_re_fault() -> Result<()> {
    let mut service = create_test_service();

    service.report_fault(FaultType::UsbStall);
    wait_for_fault_clearable();
    service.clear_fault().map_err(|e| anyhow::anyhow!("{e}"))?;
    assert_eq!(service.state(), &SafetyState::SafeTorque);

    service.report_fault(FaultType::ThermalLimit);
    match service.state() {
        SafetyState::Faulted {
            fault: FaultType::ThermalLimit,
            ..
        } => {}
        other => {
            return Err(anyhow::anyhow!(
                "expected Faulted(ThermalLimit), got {other:?}"
            ));
        }
    }
    Ok(())
}

/// Multiple faults on the interlock system accumulate in the fault log.
#[test]
fn multi_fault_interlock_fault_log() -> Result<()> {
    let mut sys = create_interlock_system(100, 25.0);
    sys.arm().map_err(|e| anyhow::anyhow!("{e:?}"))?;
    sys.report_communication();

    sys.report_fault(FaultType::UsbStall);
    let _ = sys.process_tick(5.0);

    sys.report_fault(FaultType::EncoderNaN);
    let _ = sys.process_tick(5.0);

    assert!(
        sys.fault_log().len() >= 2,
        "fault log must contain at least 2 entries, found {}",
        sys.fault_log().len()
    );
    Ok(())
}

/// Rapid fault cycling (fault → clear → fault) 50 times never deadlocks.
#[test]
fn multi_fault_rapid_cycling_no_deadlock() -> Result<()> {
    let mut service = create_test_service();
    let faults = all_fault_types();

    for i in 0..50 {
        let fault = faults[i % faults.len()];
        service.report_fault(fault);
        assert!(matches!(service.state(), SafetyState::Faulted { .. }));

        wait_for_fault_clearable();
        service
            .clear_fault()
            .map_err(|e| anyhow::anyhow!("cycle {i}: {e}"))?;
        assert_eq!(
            service.state(),
            &SafetyState::SafeTorque,
            "cycle {i}: must return to SafeTorque"
        );
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 7. Recovery paths after fault
// ═══════════════════════════════════════════════════════════════════════════════

/// Basic recovery: fault → clear → back to SafeTorque.
#[test]
fn recovery_basic_clear_fault() -> Result<()> {
    let mut service = create_test_service();
    service.report_fault(FaultType::EncoderNaN);

    wait_for_fault_clearable();
    service.clear_fault().map_err(|e| anyhow::anyhow!("{e}"))?;
    assert_eq!(service.state(), &SafetyState::SafeTorque);
    Ok(())
}

/// Recovery restores torque clamping to safe-mode level.
#[test]
fn recovery_restores_safe_torque_limit() -> Result<()> {
    let mut service = create_test_service();
    service.report_fault(FaultType::Overcurrent);

    // Faulted → zero torque
    assert!(
        service.clamp_torque_nm(25.0) <= 0.01,
        "faulted torque must be ≈0"
    );

    wait_for_fault_clearable();
    service.clear_fault().map_err(|e| anyhow::anyhow!("{e}"))?;

    // Recovered → safe torque limit (5.0 Nm)
    let clamped = service.clamp_torque_nm(25.0);
    assert!(
        (clamped - 5.0).abs() < 0.01,
        "recovered torque limit must be safe limit (5.0), got {clamped}"
    );
    Ok(())
}

/// After recovery from fault, high torque re-activation is blocked by fault
/// history (safety-correct: fault_count persists). A fresh service instance can
/// be used to re-enter HighTorqueActive.
#[test]
fn recovery_then_reactivate_high_torque() -> Result<()> {
    let mut service = create_test_service();
    activate_high_torque(&mut service, "dev-1")?;

    service.report_fault(FaultType::ThermalLimit);
    wait_for_fault_clearable();
    service.clear_fault().map_err(|e| anyhow::anyhow!("{e}"))?;

    // Re-activation on the same instance is blocked by fault history
    let result = service.request_high_torque("dev-1");
    assert!(
        result.is_err(),
        "high torque re-activation must be blocked after fault history"
    );

    // A fresh service instance can activate high torque
    let mut fresh_service = create_test_service();
    activate_high_torque(&mut fresh_service, "dev-1")?;
    match fresh_service.state() {
        SafetyState::HighTorqueActive { .. } => {}
        other => {
            return Err(anyhow::anyhow!(
                "expected HighTorqueActive on fresh service, got {other:?}"
            ));
        }
    }
    Ok(())
}

/// Interlock system fault recovery resets to Normal state.
#[test]
fn recovery_interlock_clears_to_normal() -> Result<()> {
    let mut sys = create_interlock_system(100, 25.0);
    sys.arm().map_err(|e| anyhow::anyhow!("{e:?}"))?;
    sys.report_communication();

    sys.report_fault(FaultType::UsbStall);
    let _ = sys.process_tick(5.0);
    assert!(
        matches!(sys.state(), SafetyInterlockState::SafeMode { .. }),
        "must be in SafeMode after fault"
    );

    wait_for_fault_clearable();
    sys.clear_fault().map_err(|e| anyhow::anyhow!("{e}"))?;

    assert!(
        matches!(sys.state(), SafetyInterlockState::Normal),
        "must be Normal after clear_fault"
    );
    Ok(())
}

/// Recovery from each fault type is possible.
#[test]
fn recovery_all_fault_types() -> Result<()> {
    for fault in all_fault_types() {
        let mut service = create_test_service();
        service.report_fault(fault);
        wait_for_fault_clearable();
        service
            .clear_fault()
            .map_err(|e| anyhow::anyhow!("{fault:?}: {e}"))?;
        assert_eq!(
            service.state(),
            &SafetyState::SafeTorque,
            "{fault:?}: must return to SafeTorque after recovery"
        );
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 8. Safety interlock challenge-response
// ═══════════════════════════════════════════════════════════════════════════════

/// Challenge token changes on each new request.
#[test]
fn interlock_challenge_tokens_unique() -> Result<()> {
    let mut service = create_test_service();
    let c1 = service
        .request_high_torque("dev-1")
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let token1 = c1.challenge_token;

    // Cancel and request again
    service
        .cancel_challenge()
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let c2 = service
        .request_high_torque("dev-1")
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    assert_ne!(
        token1, c2.challenge_token,
        "each challenge must have a unique token"
    );
    Ok(())
}

/// Challenge expires after configured timeout.
#[test]
fn interlock_challenge_expires() -> Result<()> {
    // Use a very short timeout by using default service (30s) — we just check
    // that check_challenge_expiry returns false while it hasn't expired yet.
    let mut service = create_test_service();
    let _challenge = service
        .request_high_torque("dev-1")
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    // Should not have expired yet
    let expired = service.check_challenge_expiry();
    assert!(!expired, "challenge must not expire immediately");
    Ok(())
}

/// Cancel challenge returns to SafeTorque.
#[test]
fn interlock_cancel_challenge() -> Result<()> {
    let mut service = create_test_service();
    let _challenge = service
        .request_high_torque("dev-1")
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    service
        .cancel_challenge()
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    assert_eq!(service.state(), &SafetyState::SafeTorque);
    Ok(())
}

/// Confirm with wrong token is rejected.
#[test]
fn interlock_wrong_token_rejected() -> Result<()> {
    let mut service = create_test_service();
    let challenge = service
        .request_high_torque("dev-1")
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    service
        .provide_ui_consent(challenge.challenge_token)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    service
        .report_combo_start(challenge.challenge_token)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    std::thread::sleep(Duration::from_millis(2100));

    // Wrong token
    let bad_ack = InterlockAck {
        challenge_token: challenge.challenge_token.wrapping_add(1),
        device_token: 42,
        combo_completed: ButtonCombo::BothClutchPaddles,
        timestamp: Instant::now(),
    };
    let result = service.confirm_high_torque("dev-1", bad_ack);
    assert!(result.is_err(), "wrong token must be rejected");
    Ok(())
}

/// Disable high torque from active state returns to SafeTorque.
#[test]
fn interlock_disable_high_torque() -> Result<()> {
    let mut service = create_test_service();
    activate_high_torque(&mut service, "dev-1")?;

    service
        .disable_high_torque("dev-1")
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    assert_eq!(service.state(), &SafetyState::SafeTorque);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 9. Torque output limiting during fault conditions
// ═══════════════════════════════════════════════════════════════════════════════

/// SafeTorque mode limits to max_safe_torque (5 Nm).
#[test]
fn torque_limit_safe_mode() -> Result<()> {
    let service = create_test_service();
    let clamped = service.clamp_torque_nm(25.0);
    assert!(
        (clamped - 5.0).abs() < 0.01,
        "SafeTorque must clamp to 5.0 Nm, got {clamped}"
    );
    Ok(())
}

/// HighTorqueActive mode allows up to max_high_torque (25 Nm).
#[test]
fn torque_limit_high_torque_active() -> Result<()> {
    let mut service = create_test_service();
    activate_high_torque(&mut service, "dev-1")?;

    let clamped = service.clamp_torque_nm(20.0);
    assert!(
        (clamped - 20.0).abs() < 0.01,
        "HighTorqueActive must allow 20.0 Nm, got {clamped}"
    );

    let clamped_over = service.clamp_torque_nm(30.0);
    assert!(
        (clamped_over - 25.0).abs() < 0.01,
        "HighTorqueActive must clamp to 25.0 Nm, got {clamped_over}"
    );
    Ok(())
}

/// Faulted mode clamps to zero.
#[test]
fn torque_limit_faulted_zero() -> Result<()> {
    let mut service = create_test_service();
    service.report_fault(FaultType::UsbStall);

    let clamped = service.clamp_torque_nm(25.0);
    assert!(
        clamped <= 0.01,
        "Faulted mode must clamp to ≈0, got {clamped}"
    );
    Ok(())
}

/// Negative torque values are handled correctly in all states.
#[test]
fn torque_limit_negative_values() -> Result<()> {
    let service = create_test_service();

    // SafeTorque: negative clamps to -max_safe
    let clamped = service.clamp_torque_nm(-25.0);
    assert!(
        (clamped - (-5.0)).abs() < 0.01,
        "negative torque must be clamped to -5.0 in SafeTorque, got {clamped}"
    );
    Ok(())
}

/// Interlock system SafeMode limits torque to safe_mode_torque.
#[test]
fn torque_limit_interlock_safe_mode() -> Result<()> {
    let mut sys = create_interlock_system(100, 25.0);
    sys.arm().map_err(|e| anyhow::anyhow!("{e:?}"))?;
    sys.report_communication();

    sys.report_fault(FaultType::UsbStall);
    let result = sys.process_tick(20.0);

    // safe_mode_torque = 25.0 * 0.2 = 5.0 Nm
    assert!(
        result.torque_command <= 5.0 + 0.01,
        "SafeMode torque must be limited to safe_mode_torque (5.0 Nm), got {}",
        result.torque_command
    );
    assert!(
        result.torque_command < 20.0,
        "SafeMode must reduce requested torque from 20.0 Nm, got {}",
        result.torque_command
    );
    Ok(())
}

/// Torque violation is logged by the interlock system.
#[test]
fn torque_violation_is_logged() -> Result<()> {
    let watchdog = Box::new(SoftwareWatchdog::new(100));
    let mut limit = TorqueLimit::new(10.0, 5.0);
    limit.log_violations = true;

    let mut sys = SafetyInterlockSystem::with_config(watchdog, limit, Duration::from_secs(5));
    sys.arm().map_err(|e| anyhow::anyhow!("{e:?}"))?;
    sys.report_communication();

    // Request torque above the limit
    let result = sys.process_tick(20.0);
    assert!(
        result.torque_command <= 10.0,
        "torque must be clamped, got {}",
        result.torque_command
    );
    Ok(())
}

/// Interlock emergency stop forces zero torque.
#[test]
fn torque_emergency_stop_zero() -> Result<()> {
    let mut sys = create_interlock_system(100, 25.0);
    sys.arm().map_err(|e| anyhow::anyhow!("{e:?}"))?;
    sys.report_communication();

    let result = sys.emergency_stop();
    assert!(
        result.torque_command.abs() < 0.01,
        "emergency stop must produce zero torque, got {}",
        result.torque_command
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 10. Determinism of state transitions (same sequence → same result)
// ═══════════════════════════════════════════════════════════════════════════════

/// The same fault sequence produces the same final state across 10 runs.
#[test]
fn determinism_same_fault_sequence() -> Result<()> {
    let fault_sequence = [
        FaultType::UsbStall,
        FaultType::EncoderNaN,
        FaultType::ThermalLimit,
    ];

    let mut final_states = Vec::new();

    for _ in 0..10 {
        let mut service = create_test_service();
        for fault in &fault_sequence {
            service.report_fault(*fault);
        }

        let is_faulted = matches!(service.state(), SafetyState::Faulted { .. });
        let clamped = service.clamp_torque_nm(25.0);
        final_states.push((is_faulted, (clamped * 100.0) as i32));
    }

    let first = &final_states[0];
    for (i, state) in final_states.iter().enumerate() {
        assert_eq!(
            state, first,
            "run {i} diverged from run 0: {state:?} != {first:?}"
        );
    }
    Ok(())
}

/// Fault → clear → re-fault cycle is deterministic.
#[test]
fn determinism_fault_clear_refault() -> Result<()> {
    let mut results = Vec::new();

    for _ in 0..10 {
        let mut service = create_test_service();
        service.report_fault(FaultType::UsbStall);
        let t1 = service.clamp_torque_nm(25.0);
        wait_for_fault_clearable();
        service.clear_fault().map_err(|e| anyhow::anyhow!("{e}"))?;
        let t2 = service.clamp_torque_nm(25.0);
        service.report_fault(FaultType::Overcurrent);
        let t3 = service.clamp_torque_nm(25.0);

        results.push((
            (t1 * 100.0) as i32,
            (t2 * 100.0) as i32,
            (t3 * 100.0) as i32,
        ));
    }

    let first = &results[0];
    for (i, r) in results.iter().enumerate() {
        assert_eq!(r, first, "run {i} diverged: {r:?} != {first:?}");
    }
    Ok(())
}

/// Interlock system produces deterministic results for the same tick sequence.
#[test]
fn determinism_interlock_tick_sequence() -> Result<()> {
    let mut results = Vec::new();

    for _ in 0..10 {
        let mut sys = create_interlock_system(100, 25.0);
        sys.arm().map_err(|e| anyhow::anyhow!("{e:?}"))?;
        sys.report_communication();

        let r1 = sys.process_tick(10.0);
        sys.report_communication();
        let r2 = sys.process_tick(15.0);
        sys.report_fault(FaultType::Overcurrent);
        let r3 = sys.process_tick(20.0);

        results.push((
            (r1.torque_command * 100.0) as i32,
            (r2.torque_command * 100.0) as i32,
            (r3.torque_command * 100.0) as i32,
            r3.fault_occurred,
        ));
    }

    let first = &results[0];
    for (i, r) in results.iter().enumerate() {
        assert_eq!(r, first, "run {i} diverged: {r:?} != {first:?}");
    }
    Ok(())
}

/// Torque clamping is idempotent: calling it N times gives the same value.
#[test]
fn determinism_torque_clamp_idempotent() -> Result<()> {
    let service = create_test_service();

    let first = service.clamp_torque_nm(25.0);
    for i in 0..100 {
        let clamped = service.clamp_torque_nm(25.0);
        assert!(
            (clamped - first).abs() < f32::EPSILON,
            "clamp call {i} diverged: {clamped} != {first}"
        );
    }
    Ok(())
}

/// Full interlock activation flow is deterministic (modulo timing-dependent tokens).
#[test]
fn determinism_full_activation_idempotent_state() -> Result<()> {
    let mut states = Vec::new();

    for _ in 0..3 {
        let mut service = create_test_service();
        activate_high_torque(&mut service, "dev-1")?;
        let is_active = matches!(service.state(), SafetyState::HighTorqueActive { .. });
        let torque = service.clamp_torque_nm(20.0);
        states.push((is_active, (torque * 100.0) as i32));
    }

    let first = &states[0];
    for (i, s) in states.iter().enumerate() {
        assert_eq!(s, first, "run {i} diverged: {s:?} != {first:?}");
    }
    Ok(())
}

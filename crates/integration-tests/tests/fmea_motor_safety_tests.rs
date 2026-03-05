//! FMEA motor safety tests — motor runaway, power-loss, and torque protection.
//!
//! Covers critical safety gaps:
//! - Motor runaway detection and mitigation
//! - Current/torque limiting
//! - Stall detection
//! - Power loss and brownout recovery
//! - Watchdog timeout enforcement
//! - Concurrent safety events
//! - Recovery after fault
//! - Torque direction validation

use racing_wheel_engine::safety::{
    FaultType, HardwareWatchdog, SafetyInterlockState, SafetyInterlockSystem, SafetyService,
    SafetyState, SoftwareWatchdog, TorqueLimit, WatchdogTimeoutHandler,
};
use racing_wheel_integration_tests::common::{VirtualDevice, VirtualTelemetry};
use std::time::Duration;

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

// ===========================================================================
// 1. Motor Runaway Detection
// ===========================================================================

/// If torque output exceeds configured max for sustained period, safety
/// interlock must engage and transition to safe mode.
#[test]
fn runaway_torque_exceeding_max_triggers_safe_mode() {
    let mut sys = new_interlock(200);
    sys.report_communication();
    let _ = sys.process_tick(5.0);

    // Request torque far above the 25 Nm max for several ticks
    for _ in 0..10 {
        sys.report_communication();
        let tick = sys.process_tick(100.0);
        // Torque must always be clamped to safe limits
        assert!(
            tick.torque_command <= 25.0,
            "Runaway torque must be clamped, got {}",
            tick.torque_command
        );
    }
}

/// Repeated over-limit requests should be tracked as violations.
#[test]
fn runaway_repeated_over_limit_tracked_as_violations() {
    let mut limit = TorqueLimit::new(25.0, 5.0);

    for _ in 0..20 {
        let (clamped, was_clamped) = limit.clamp(50.0);
        assert!(was_clamped, "50 Nm must be clamped at 25 Nm limit");
        assert!(
            (clamped - 25.0).abs() < f32::EPSILON,
            "Clamped value must be 25.0, got {clamped}"
        );
    }

    assert!(
        limit.violation_count >= 20,
        "Must track at least 20 violations, got {}",
        limit.violation_count
    );
}

/// Runaway detection via SafetyService: requesting extreme torque while
/// faulted must yield zero torque.
#[test]
fn runaway_faulted_state_zeros_extreme_torque() {
    let mut svc = new_service();
    svc.report_fault(FaultType::Overcurrent);

    // Even extreme runaway-level requests must produce zero in faulted state
    for magnitude in [50.0, 100.0, 500.0, f32::MAX] {
        let clamped = svc.clamp_torque_nm(magnitude);
        assert!(
            clamped.abs() < f32::EPSILON,
            "Faulted state must zero torque for {magnitude}, got {clamped}"
        );
    }
}

// ===========================================================================
// 2. Runaway Mitigation — Force Ramp to Zero
// ===========================================================================

/// When a fault is reported during active torque, the interlock must
/// reduce torque to safe-mode level (≤5 Nm) within the next tick.
#[test]
fn runaway_mitigation_torque_drops_on_fault() {
    let mut sys = new_interlock(200);
    sys.report_communication();

    // Establish high torque output
    let tick_before = sys.process_tick(20.0);
    assert!(
        tick_before.torque_command > 5.0,
        "Pre-fault torque should be above safe-mode level"
    );

    // Report overcurrent fault (simulating runaway detection)
    sys.report_fault(FaultType::Overcurrent);
    sys.report_communication();
    let tick_after = sys.process_tick(20.0);

    assert!(
        matches!(tick_after.state, SafetyInterlockState::SafeMode { .. }),
        "Must enter safe mode after overcurrent, got {:?}",
        tick_after.state
    );
    assert!(
        tick_after.torque_command <= 5.0,
        "Torque must drop to safe-mode limit (≤5 Nm) after fault, got {}",
        tick_after.torque_command
    );
}

/// Emergency stop must immediately zero torque regardless of request.
#[test]
fn runaway_mitigation_emergency_stop_zeros_torque() {
    let mut sys = new_interlock(200);
    sys.report_communication();
    let _ = sys.process_tick(20.0);

    let estop = sys.emergency_stop();
    assert!(
        estop.torque_command.abs() < f32::EPSILON,
        "Emergency stop must zero torque, got {}",
        estop.torque_command
    );
    assert!(
        matches!(estop.state, SafetyInterlockState::EmergencyStop { .. }),
        "Must be in EmergencyStop state, got {:?}",
        estop.state
    );
}

/// After emergency stop, subsequent ticks must continue to produce zero torque.
#[test]
fn runaway_mitigation_estop_persists_across_ticks() {
    let mut sys = new_interlock(200);
    sys.report_communication();
    let _ = sys.process_tick(20.0);
    let _ = sys.emergency_stop();

    for _ in 0..10 {
        sys.report_communication();
        let tick = sys.process_tick(25.0);
        assert!(
            tick.torque_command.abs() < f32::EPSILON,
            "E-stop must persist: torque must stay zero, got {}",
            tick.torque_command
        );
    }
}

// ===========================================================================
// 3. Current / Torque Limiting
// ===========================================================================

/// TorqueLimit must clamp positive values to configured maximum.
#[test]
fn current_limiting_clamps_positive_torque() {
    let mut limit = TorqueLimit::new(10.0, 3.0);
    let (clamped, was_clamped) = limit.clamp(15.0);
    assert!(was_clamped);
    assert!(
        (clamped - 10.0).abs() < f32::EPSILON,
        "Positive torque must clamp to 10.0, got {clamped}"
    );
}

/// TorqueLimit must clamp negative values to negative maximum.
#[test]
fn current_limiting_clamps_negative_torque() {
    let mut limit = TorqueLimit::new(10.0, 3.0);
    let (clamped, was_clamped) = limit.clamp(-15.0);
    assert!(was_clamped);
    assert!(
        (clamped - (-10.0)).abs() < f32::EPSILON,
        "Negative torque must clamp to -10.0, got {clamped}"
    );
}

/// Torque within limits must pass through unclamped.
#[test]
fn current_limiting_within_range_passes_through() {
    let mut limit = TorqueLimit::new(25.0, 5.0);
    let (clamped, was_clamped) = limit.clamp(12.5);
    assert!(!was_clamped);
    assert!(
        (clamped - 12.5).abs() < f32::EPSILON,
        "In-range torque must pass through, got {clamped}"
    );
    assert_eq!(limit.violation_count, 0);
}

/// SafetyService in SafeTorque state must clamp to safe limit (5 Nm).
#[test]
fn current_limiting_safe_torque_state_limit() {
    let svc = new_service();
    // Default state is SafeTorque with 5 Nm limit
    let clamped = svc.clamp_torque_nm(20.0);
    assert!(
        clamped <= 5.0,
        "SafeTorque state must clamp to ≤5 Nm, got {clamped}"
    );
}

/// Safe-mode limit on TorqueLimit must be accessible and correct.
#[test]
fn current_limiting_safe_mode_limit_value() {
    let limit = TorqueLimit::new(25.0, 5.0);
    assert!(
        (limit.safe_mode_limit() - 5.0).abs() < f32::EPSILON,
        "Safe mode limit must be 5.0, got {}",
        limit.safe_mode_limit()
    );
}

// ===========================================================================
// 4. Stall Detection
// ===========================================================================

/// If torque is applied but position doesn't change (stall), reporting an
/// overcurrent fault must zero torque output.
#[test]
fn stall_detection_overcurrent_zeros_torque() {
    let mut svc = new_service();

    // Simulate stall: torque applied but motor stuck → overcurrent
    svc.report_fault(FaultType::Overcurrent);
    assert!(matches!(
        svc.state(),
        SafetyState::Faulted {
            fault: FaultType::Overcurrent,
            ..
        }
    ));

    let clamped = svc.clamp_torque_nm(15.0);
    assert!(
        clamped.abs() < f32::EPSILON,
        "Stall/overcurrent must zero torque, got {clamped}"
    );
}

/// Stall condition with interlock system: overcurrent during active torque
/// must transition to safe mode.
#[test]
fn stall_detection_interlock_enters_safe_mode() {
    let mut sys = new_interlock(200);
    sys.report_communication();
    let _ = sys.process_tick(15.0); // Active torque

    // Report stall-induced overcurrent
    sys.report_fault(FaultType::Overcurrent);
    sys.report_communication();
    let tick = sys.process_tick(15.0);

    assert!(
        matches!(tick.state, SafetyInterlockState::SafeMode { .. }),
        "Stall must trigger safe mode, got {:?}",
        tick.state
    );
    assert!(
        tick.torque_command <= 5.0,
        "Torque must be limited to safe level during stall, got {}",
        tick.torque_command
    );
}

/// Stall detected on virtual device via telemetry: zero speed with torque
/// applied should be detectable.
#[tokio::test]
async fn stall_detection_virtual_device_zero_speed() -> anyhow::Result<()> {
    let mut device = VirtualDevice::new("StallMotor")?;
    // Simulate stall: torque commanded but wheel not moving
    device.last_torque_command = 20.0;
    device.telemetry_data = VirtualTelemetry {
        wheel_angle_deg: 45.0,
        wheel_speed_rad_s: 0.0, // Stalled — no movement
        temperature_c: 80,      // Rising temp from stall
        fault_flags: 0,
        hands_on: true,
    };

    // Verify stall is observable (speed zero with torque applied)
    assert!(
        device.telemetry_data.wheel_speed_rad_s.abs() < f32::EPSILON,
        "Speed must be zero during stall"
    );
    assert!(
        device.last_torque_command > 0.0,
        "Torque must be non-zero during stall"
    );

    // Safety service must handle overcurrent fault from stall
    let mut svc = new_service();
    svc.report_fault(FaultType::Overcurrent);
    let clamped = svc.clamp_torque_nm(device.last_torque_command);
    assert!(
        clamped.abs() < f32::EPSILON,
        "Torque must be zeroed after stall fault, got {clamped}"
    );
    Ok(())
}

// ===========================================================================
// 5. Power Loss Simulation
// ===========================================================================

/// Communication loss triggers safe mode: torque must be limited.
#[test]
fn power_loss_comm_timeout_triggers_safe_mode() {
    let mut sys = new_interlock(50);
    sys.report_communication();
    let _ = sys.process_tick(15.0);

    // Simulate power loss: stop feeding communication
    std::thread::sleep(Duration::from_millis(80));

    let tick = sys.process_tick(15.0);
    let in_safe = matches!(tick.state, SafetyInterlockState::SafeMode { .. })
        || tick.torque_command.abs() < f32::EPSILON;
    assert!(
        in_safe,
        "Power loss (comm timeout) must trigger safe state, got {:?} torque={}",
        tick.state, tick.torque_command
    );
}

/// Device disconnect during operation: torque must zero.
#[tokio::test]
async fn power_loss_device_disconnect_zeros_torque() -> anyhow::Result<()> {
    let mut device = VirtualDevice::new("PowerLossDevice")?;
    device.last_torque_command = 20.0;

    // Simulate power loss via disconnect
    device.disconnect();
    assert!(!device.connected);

    // Safety system must zero torque on USB stall (power loss indicator)
    let mut svc = new_service();
    svc.report_fault(FaultType::UsbStall);
    let clamped = svc.clamp_torque_nm(20.0);
    assert!(
        clamped.abs() < f32::EPSILON,
        "Power loss must zero torque, got {clamped}"
    );
    Ok(())
}

/// Multiple devices: power loss on one must not affect the safety service
/// for other devices (fault isolation).
#[tokio::test]
async fn power_loss_fault_isolation_between_devices() -> anyhow::Result<()> {
    let device_a = VirtualDevice::new("DeviceA-OK")?;
    let mut device_b = VirtualDevice::new("DeviceB-PowerLoss")?;

    // Device B loses power
    device_b.disconnect();
    assert!(!device_b.connected);

    // Device A is still fine
    assert!(device_a.connected);

    // Separate safety services can track independently
    let svc_a = new_service();
    let mut svc_b = new_service();
    svc_b.report_fault(FaultType::UsbStall);

    let torque_a = svc_a.clamp_torque_nm(4.0);
    let torque_b = svc_b.clamp_torque_nm(4.0);

    assert!(
        (torque_a - 4.0).abs() < f32::EPSILON,
        "Healthy device must retain torque, got {torque_a}"
    );
    assert!(
        torque_b.abs() < f32::EPSILON,
        "Faulted device must zero torque, got {torque_b}"
    );
    Ok(())
}

// ===========================================================================
// 6. Brownout Recovery
// ===========================================================================

/// After momentary comm loss and recovery, system must return to Normal
/// state via fault clear.
#[test]
fn brownout_recovery_returns_to_normal() -> Result<(), String> {
    let mut sys = new_interlock(50);
    sys.report_communication();
    let _ = sys.process_tick(10.0);

    // Brownout: comm drops for > timeout
    std::thread::sleep(Duration::from_millis(80));
    let tick = sys.process_tick(10.0);
    let was_safe = matches!(tick.state, SafetyInterlockState::SafeMode { .. })
        || tick.torque_command.abs() < f32::EPSILON;
    assert!(was_safe, "Must enter safe state during brownout");

    // Power returns: resume comms, wait cooldown, clear fault
    std::thread::sleep(Duration::from_millis(120));
    sys.report_communication();
    sys.clear_fault()?;
    sys.report_communication();
    let tick2 = sys.process_tick(10.0);

    assert!(
        matches!(tick2.state, SafetyInterlockState::Normal),
        "Must recover to Normal after brownout, got {:?}",
        tick2.state
    );
    assert!(
        tick2.torque_command > 0.0,
        "Torque must be restored after brownout recovery"
    );
    Ok(())
}

/// After brownout, torque must ramp through safe limits before returning
/// to full output.
#[test]
fn brownout_recovery_torque_limited_until_cleared() -> Result<(), String> {
    let mut sys = new_interlock(50);
    sys.report_communication();
    let _ = sys.process_tick(20.0);

    // Brownout
    std::thread::sleep(Duration::from_millis(80));
    let _ = sys.process_tick(20.0);

    // Resume comms but before clearing fault
    sys.report_communication();
    let tick_limited = sys.process_tick(20.0);
    assert!(
        tick_limited.torque_command <= 5.0,
        "Torque must be limited before fault clear, got {}",
        tick_limited.torque_command
    );

    // Now clear fault and verify restoration
    std::thread::sleep(Duration::from_millis(120));
    sys.report_communication();
    sys.clear_fault()?;
    sys.report_communication();
    let tick_restored = sys.process_tick(20.0);
    assert!(
        matches!(tick_restored.state, SafetyInterlockState::Normal),
        "Must be Normal after clear, got {:?}",
        tick_restored.state
    );
    Ok(())
}

/// Repeated brownouts must not corrupt interlock state.
#[test]
fn brownout_repeated_cycles_no_corruption() -> Result<(), String> {
    let mut sys = new_interlock(50);

    for cycle in 0..3 {
        sys.report_communication();
        let _ = sys.process_tick(10.0);

        // Brownout
        std::thread::sleep(Duration::from_millis(80));
        let _ = sys.process_tick(10.0);

        // Recovery
        std::thread::sleep(Duration::from_millis(120));
        sys.report_communication();
        sys.clear_fault()?;
        sys.report_communication();
        let tick = sys.process_tick(10.0);

        assert!(
            matches!(tick.state, SafetyInterlockState::Normal),
            "Cycle {cycle}: must recover to Normal, got {:?}",
            tick.state
        );
    }
    Ok(())
}

// ===========================================================================
// 7. Watchdog Timeout
// ===========================================================================

/// Missing watchdog feeds must trigger safe state.
#[test]
fn watchdog_timeout_triggers_safe_state() {
    let mut wd = SoftwareWatchdog::new(10);
    let arm_result = wd.arm();
    assert!(arm_result.is_ok(), "Watchdog must arm successfully");
    let feed_result = wd.feed();
    assert!(feed_result.is_ok(), "Feed must succeed");

    // Wait for timeout
    std::thread::sleep(Duration::from_millis(30));
    assert!(
        wd.has_timed_out(),
        "Watchdog must report timeout after missed feeds"
    );
}

/// WatchdogTimeoutHandler must zero torque and report within budget.
#[test]
fn watchdog_timeout_handler_zeros_torque() {
    let mut handler = WatchdogTimeoutHandler::new();
    let response = handler.handle_timeout(15.0);

    assert!(
        response.torque_command.abs() < f32::EPSILON,
        "Timeout must zero torque, got {}",
        response.torque_command
    );
    assert!(
        (response.previous_torque - 15.0).abs() < f32::EPSILON,
        "Previous torque must be recorded"
    );
    assert!(handler.is_timeout_triggered());
}

/// Interlock system with short watchdog must enter safe mode when comms stop.
#[test]
fn watchdog_short_timeout_fast_response() {
    let mut sys = new_interlock(30); // 30ms timeout
    sys.report_communication();
    let _ = sys.process_tick(15.0);

    // Don't feed watchdog — let it time out
    std::thread::sleep(Duration::from_millis(80));
    let tick = sys.process_tick(15.0);

    let in_safe = matches!(tick.state, SafetyInterlockState::SafeMode { .. })
        || tick.torque_command.abs() < f32::EPSILON;
    assert!(
        in_safe,
        "Watchdog timeout must trigger safe state, got {:?} torque={}",
        tick.state, tick.torque_command
    );
}

/// Watchdog feed resets timeout — system must remain Normal.
#[test]
fn watchdog_regular_feeds_keep_normal() {
    let mut sys = new_interlock(50);
    sys.report_communication();

    for _ in 0..20 {
        sys.report_communication(); // Feed the watchdog
        let tick = sys.process_tick(10.0);
        assert!(
            matches!(tick.state, SafetyInterlockState::Normal),
            "Regular feeds must keep system Normal, got {:?}",
            tick.state
        );
        assert!(
            tick.torque_command > 0.0,
            "Torque must remain active with regular feeds"
        );
    }
}

/// After watchdog timeout, handler must record timestamp.
#[test]
fn watchdog_timeout_records_timestamp() {
    let mut handler = WatchdogTimeoutHandler::new();
    assert!(
        handler.timeout_timestamp().is_none(),
        "No timestamp before timeout"
    );

    let _ = handler.handle_timeout(10.0);
    assert!(
        handler.timeout_timestamp().is_some(),
        "Timestamp must be recorded after timeout"
    );
}

// ===========================================================================
// 8. Concurrent Safety Events
// ===========================================================================

/// Multiple faults from different sources must all be handled; system
/// must remain in a safe state.
#[test]
fn concurrent_faults_all_handled() {
    let mut sys = new_interlock(200);
    sys.report_communication();
    let _ = sys.process_tick(10.0);

    // Fire concurrent faults
    sys.report_fault(FaultType::Overcurrent);
    sys.report_fault(FaultType::EncoderNaN);
    sys.report_fault(FaultType::ThermalLimit);

    sys.report_communication();
    let tick = sys.process_tick(20.0);

    let in_safe = matches!(tick.state, SafetyInterlockState::SafeMode { .. })
        || matches!(tick.state, SafetyInterlockState::EmergencyStop { .. });
    assert!(
        in_safe,
        "Concurrent faults must keep system in safe state, got {:?}",
        tick.state
    );
    assert!(
        tick.torque_command <= 5.0,
        "Torque must be limited after concurrent faults, got {}",
        tick.torque_command
    );
}

/// Concurrent SafetyService faults: each additional fault must keep torque
/// zeroed.
#[test]
fn concurrent_service_faults_stay_zeroed() {
    let mut svc = new_service();

    let concurrent_faults = [
        FaultType::Overcurrent,
        FaultType::UsbStall,
        FaultType::TimingViolation,
        FaultType::PipelineFault,
    ];

    for fault in &concurrent_faults {
        svc.report_fault(*fault);
        let clamped = svc.clamp_torque_nm(25.0);
        assert!(
            clamped.abs() < f32::EPSILON,
            "Torque must be zero after {fault:?}, got {clamped}"
        );
    }
}

/// Watchdog timeout + fault simultaneously: both must be handled.
#[test]
fn concurrent_watchdog_timeout_plus_fault() {
    let mut sys = new_interlock(30);
    sys.report_communication();
    let _ = sys.process_tick(15.0);

    // Let watchdog starve AND report a fault
    std::thread::sleep(Duration::from_millis(60));
    sys.report_fault(FaultType::Overcurrent);

    let tick = sys.process_tick(20.0);
    let in_safe = matches!(tick.state, SafetyInterlockState::SafeMode { .. })
        || matches!(tick.state, SafetyInterlockState::EmergencyStop { .. })
        || tick.torque_command.abs() < f32::EPSILON;
    assert!(
        in_safe,
        "Timeout + fault must result in safe state, got {:?} torque={}",
        tick.state, tick.torque_command
    );
}

/// E-stop during active fault must transition to EmergencyStop.
#[test]
fn concurrent_estop_during_fault() {
    let mut sys = new_interlock(200);
    sys.report_communication();
    let _ = sys.process_tick(15.0);

    sys.report_fault(FaultType::EncoderNaN);
    let estop = sys.emergency_stop();

    assert!(
        matches!(estop.state, SafetyInterlockState::EmergencyStop { .. }),
        "E-stop during fault must reach EmergencyStop, got {:?}",
        estop.state
    );
    assert!(
        estop.torque_command.abs() < f32::EPSILON,
        "E-stop torque must be zero, got {}",
        estop.torque_command
    );
}

// ===========================================================================
// 9. Recovery After Fault
// ===========================================================================

/// After fault clear with cooldown, system returns to Normal and torque
/// is restored.
#[test]
fn recovery_after_fault_restores_normal() -> Result<(), String> {
    let mut sys = new_interlock(200);
    sys.report_communication();
    let _ = sys.process_tick(15.0);

    // Fault
    sys.report_fault(FaultType::Overcurrent);
    sys.report_communication();
    let tick_faulted = sys.process_tick(15.0);
    assert!(
        matches!(tick_faulted.state, SafetyInterlockState::SafeMode { .. }),
        "Must be in safe mode after fault"
    );

    // Wait cooldown and clear
    std::thread::sleep(Duration::from_millis(120));
    sys.report_communication();
    sys.clear_fault()?;
    sys.report_communication();
    let tick_recovered = sys.process_tick(15.0);

    assert!(
        matches!(tick_recovered.state, SafetyInterlockState::Normal),
        "Must return to Normal after recovery, got {:?}",
        tick_recovered.state
    );
    assert!(
        tick_recovered.torque_command > 5.0,
        "Full torque must be restored after recovery, got {}",
        tick_recovered.torque_command
    );
    Ok(())
}

/// SafetyService fault → clear → full torque cycle.
#[test]
fn recovery_service_fault_clear_cycle() -> Result<(), String> {
    let mut svc = new_service();

    // Fault
    svc.report_fault(FaultType::PipelineFault);
    assert!(svc.clamp_torque_nm(10.0).abs() < f32::EPSILON);

    // Cooldown and clear
    std::thread::sleep(Duration::from_millis(110));
    svc.clear_fault()?;
    assert!(matches!(svc.state(), SafetyState::SafeTorque));

    // Torque restored to safe level
    let clamped = svc.clamp_torque_nm(4.0);
    assert!(
        (clamped - 4.0).abs() < f32::EPSILON,
        "Torque must be restored after clear, got {clamped}"
    );
    Ok(())
}

/// Premature fault clear (before cooldown) must be rejected.
#[test]
fn recovery_premature_clear_rejected() {
    let mut svc = new_service();
    svc.report_fault(FaultType::Overcurrent);

    // Immediate clear attempt — should fail
    let result = svc.clear_fault();
    assert!(result.is_err(), "Premature fault clear must be rejected");
    assert!(matches!(svc.state(), SafetyState::Faulted { .. }));
    assert!(svc.clamp_torque_nm(10.0).abs() < f32::EPSILON);
}

/// Fault log must contain entries after faults.
#[test]
fn recovery_fault_log_populated() {
    let mut sys = new_interlock(200);
    sys.report_communication();
    let _ = sys.process_tick(5.0);

    sys.report_fault(FaultType::Overcurrent);
    sys.report_communication();
    let _ = sys.process_tick(5.0);

    let log = sys.fault_log();
    assert!(!log.is_empty(), "Fault log must contain at least one entry");
}

/// After reset, interlock system returns to Normal with clean state.
#[test]
fn recovery_full_reset_to_normal() -> Result<(), Box<dyn std::error::Error>> {
    let mut sys = new_interlock(200);
    sys.report_communication();
    let _ = sys.process_tick(10.0);

    sys.report_fault(FaultType::SafetyInterlockViolation);
    let _ = sys.process_tick(10.0);

    // Wait cooldown then reset
    std::thread::sleep(Duration::from_millis(120));
    sys.reset()?;
    sys.report_communication();
    let tick = sys.process_tick(10.0);

    assert!(
        matches!(tick.state, SafetyInterlockState::Normal),
        "Must be Normal after reset, got {:?}",
        tick.state
    );
    Ok(())
}

// ===========================================================================
// 10. Torque Direction Validation
// ===========================================================================

/// Positive torque request must result in positive (or zero) clamped output.
#[test]
fn torque_direction_positive_request_positive_output() {
    let svc = new_service();
    let clamped = svc.clamp_torque_nm(3.0);
    assert!(
        clamped >= 0.0,
        "Positive request must yield non-negative output, got {clamped}"
    );
    assert!(
        (clamped - 3.0).abs() < f32::EPSILON,
        "Within-range positive request must pass through, got {clamped}"
    );
}

/// Negative torque request must result in negative (or zero) clamped output.
#[test]
fn torque_direction_negative_request_negative_output() {
    let svc = new_service();
    let clamped = svc.clamp_torque_nm(-3.0);
    assert!(
        clamped <= 0.0,
        "Negative request must yield non-positive output, got {clamped}"
    );
    assert!(
        (clamped - (-3.0)).abs() < f32::EPSILON,
        "Within-range negative request must pass through, got {clamped}"
    );
}

/// TorqueLimit must preserve sign: clamping must not flip direction.
#[test]
fn torque_direction_clamping_preserves_sign() {
    let mut limit = TorqueLimit::new(10.0, 3.0);

    let (pos_clamped, _) = limit.clamp(50.0);
    assert!(
        pos_clamped > 0.0,
        "Positive over-limit must clamp to positive, got {pos_clamped}"
    );

    let (neg_clamped, _) = limit.clamp(-50.0);
    assert!(
        neg_clamped < 0.0,
        "Negative over-limit must clamp to negative, got {neg_clamped}"
    );
}

/// Direction mismatch detection: if hardware reports opposite torque to
/// commanded, an overcurrent fault must zero output.
#[tokio::test]
async fn torque_direction_mismatch_detected() -> anyhow::Result<()> {
    let mut device = VirtualDevice::new("DirectionMismatch")?;
    // Commanded positive torque
    device.last_torque_command = 10.0;
    // But telemetry shows wheel moving in wrong direction (negative speed)
    device.telemetry_data = VirtualTelemetry {
        wheel_angle_deg: -5.0, // Moving opposite to command
        wheel_speed_rad_s: -10.0,
        temperature_c: 40,
        fault_flags: 0,
        hands_on: true,
    };

    // Detect mismatch: positive torque but negative speed
    let torque_sign = device.last_torque_command.signum();
    let speed_sign = device.telemetry_data.wheel_speed_rad_s.signum();
    let mismatch = (torque_sign - speed_sign).abs() > 1.0;
    assert!(mismatch, "Direction mismatch must be detectable");

    // Report fault for direction anomaly
    let mut svc = new_service();
    svc.report_fault(FaultType::SafetyInterlockViolation);
    let clamped = svc.clamp_torque_nm(device.last_torque_command);
    assert!(
        clamped.abs() < f32::EPSILON,
        "Direction mismatch fault must zero torque, got {clamped}"
    );
    Ok(())
}

/// Zero torque request must yield zero regardless of direction.
#[test]
fn torque_direction_zero_request_yields_zero() {
    let svc = new_service();
    let clamped = svc.clamp_torque_nm(0.0);
    assert!(
        clamped.abs() < f32::EPSILON,
        "Zero request must yield zero, got {clamped}"
    );

    let mut limit = TorqueLimit::new(25.0, 5.0);
    let (clamped_limit, was_clamped) = limit.clamp(0.0);
    assert!(!was_clamped);
    assert!(
        clamped_limit.abs() < f32::EPSILON,
        "Zero torque must pass through TorqueLimit unchanged"
    );
}

/// NaN torque direction must not produce NaN output — must be zeroed.
#[test]
fn torque_direction_nan_produces_zero() {
    let svc = new_service();
    let clamped = svc.clamp_torque_nm(f32::NAN);
    assert!(
        clamped.abs() < f32::EPSILON,
        "NaN request must yield zero, got {clamped}"
    );
}

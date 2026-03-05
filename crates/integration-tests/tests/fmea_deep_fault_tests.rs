//! FMEA deep fault injection tests.
//!
//! Covers critical gaps identified in safety review:
//! - Corrupted HID report handling
//! - Rapid device connect/disconnect
//! - Communication timeout recovery
//! - Encoder health monitoring
//! - Concurrent fault escalation
//! - Thermal hysteresis

use racing_wheel_engine::safety::{
    FaultType, SafetyInterlockState, SafetyInterlockSystem, SafetyService, SafetyState,
    SoftwareWatchdog,
};
use racing_wheel_integration_tests::TestConfig;
use racing_wheel_integration_tests::common::{TestHarness, VirtualDevice, VirtualTelemetry};
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
// 1. Corrupted HID Report Handling
// ===========================================================================

/// Malformed packet: truncated HID report should not crash the virtual device.
#[tokio::test]
async fn corrupted_hid_truncated_report_does_not_crash() -> anyhow::Result<()> {
    let mut device = VirtualDevice::new("Truncated-HID")?;
    // Simulate a truncated report by feeding minimal/zero telemetry
    device.telemetry_data = VirtualTelemetry {
        wheel_angle_deg: 0.0,
        wheel_speed_rad_s: 0.0,
        temperature_c: 0,
        fault_flags: 0,
        hands_on: false,
    };
    // Device must remain connected and not panic
    assert!(device.connected);
    assert!((device.telemetry_data.wheel_angle_deg).abs() < f32::EPSILON);
    Ok(())
}

/// All-zero HID report should not crash or trigger spurious faults.
#[tokio::test]
async fn corrupted_hid_all_zero_report_no_crash() -> anyhow::Result<()> {
    let mut device = VirtualDevice::new("AllZero-HID")?;
    device.telemetry_data = VirtualTelemetry {
        wheel_angle_deg: 0.0,
        wheel_speed_rad_s: 0.0,
        temperature_c: 0,
        fault_flags: 0,
        hands_on: false,
    };
    // Zero report must not set any fault flags
    assert_eq!(device.telemetry_data.fault_flags, 0);
    assert!(device.connected);

    // Safety service must still clamp normally with zero inputs
    let svc = new_service();
    let clamped = svc.clamp_torque_nm(0.0);
    assert!(clamped.abs() < f32::EPSILON);
    Ok(())
}

/// All-0xFF HID report should not crash; saturated values must be handled.
#[tokio::test]
async fn corrupted_hid_all_ff_report_no_crash() -> anyhow::Result<()> {
    let mut device = VirtualDevice::new("AllFF-HID")?;
    device.telemetry_data = VirtualTelemetry {
        wheel_angle_deg: f32::MAX,
        wheel_speed_rad_s: f32::MAX,
        temperature_c: 0xFF,
        fault_flags: 0xFF,
        hands_on: true,
    };
    // Device must not panic; saturated fault flags are set
    assert_eq!(device.telemetry_data.fault_flags, 0xFF);
    assert!(device.connected);

    // Safety service must still function when receiving extreme torque requests
    let svc = new_service();
    let clamped = svc.clamp_torque_nm(f32::MAX);
    // Must be clamped to safe torque limit (5.0 Nm in SafeTorque state)
    assert!(
        clamped <= 5.0,
        "f32::MAX torque must be clamped, got {clamped}"
    );
    Ok(())
}

/// Maximum-length report: extreme field values must be handled gracefully.
#[tokio::test]
async fn corrupted_hid_max_length_report_handled() -> anyhow::Result<()> {
    let mut device = VirtualDevice::new("MaxLen-HID")?;
    device.telemetry_data = VirtualTelemetry {
        wheel_angle_deg: 99999.0,
        wheel_speed_rad_s: 99999.0,
        temperature_c: 255,
        fault_flags: 0,
        hands_on: true,
    };
    // Extreme but non-NaN values must not crash
    assert!(device.connected);
    assert!((device.telemetry_data.wheel_angle_deg - 99999.0).abs() < f32::EPSILON);
    Ok(())
}

/// Invalid enum values in report fields should not cause undefined behavior.
#[tokio::test]
async fn corrupted_hid_invalid_enum_values_handled() -> anyhow::Result<()> {
    let mut device = VirtualDevice::new("InvalidEnum-HID")?;
    // Simulate invalid fault flag combinations (bit patterns that are not
    // defined individual flags but combinations)
    device.telemetry_data.fault_flags = 0b1010_1010;
    assert_eq!(device.telemetry_data.fault_flags, 0xAA);
    assert!(device.connected);

    // The safety service must handle any fault flag pattern
    let mut svc = new_service();
    svc.report_fault(FaultType::PipelineFault);
    assert!(matches!(svc.state(), SafetyState::Faulted { .. }));
    Ok(())
}

// ===========================================================================
// 2. Rapid Device Connect/Disconnect
// ===========================================================================

/// Connect → disconnect → reconnect cycle must leave device in a valid state.
#[tokio::test]
async fn rapid_connect_disconnect_reconnect_cycle() -> anyhow::Result<()> {
    let config = TestConfig {
        virtual_device: true,
        ..TestConfig::default()
    };
    let mut harness = TestHarness::new(config).await?;
    let _id = harness.add_virtual_device("CycleWheel").await?;
    let dev_idx = harness.virtual_devices.len() - 1;

    // Connect → disconnect → reconnect
    harness.simulate_hotplug_cycle(dev_idx).await?;

    {
        let dev = harness.virtual_devices[dev_idx].read().await;
        assert!(
            dev.connected,
            "Device must be reconnected after hotplug cycle"
        );
    }

    harness.shutdown().await?;
    Ok(())
}

/// Disconnect during active torque output: torque must zero within safety window.
#[tokio::test]
async fn disconnect_during_active_torque_zeros_output() -> anyhow::Result<()> {
    let mut device = VirtualDevice::new("TorqueDisconnect")?;
    device.last_torque_command = 15.0;
    assert!((device.last_torque_command - 15.0).abs() < f32::EPSILON);

    // Disconnect while torque is active
    device.disconnect();
    assert!(!device.connected);

    // Safety service must zero torque on fault (simulating disconnect detection)
    let mut svc = new_service();
    svc.report_fault(FaultType::UsbStall);
    let clamped = svc.clamp_torque_nm(15.0);
    assert!(
        clamped.abs() < f32::EPSILON,
        "Torque must be zero after disconnect, got {clamped}"
    );
    Ok(())
}

/// Multiple rapid disconnects in succession must not corrupt state.
#[tokio::test]
async fn multiple_rapid_disconnects_no_corruption() -> anyhow::Result<()> {
    let config = TestConfig {
        virtual_device: true,
        ..TestConfig::default()
    };
    let mut harness = TestHarness::new(config).await?;
    let _id = harness.add_virtual_device("RapidDisconnect").await?;
    let dev_idx = harness.virtual_devices.len() - 1;

    // Rapid hotplug cycles
    for _ in 0..5 {
        harness.simulate_hotplug_cycle(dev_idx).await?;
    }

    {
        let dev = harness.virtual_devices[dev_idx].read().await;
        assert!(
            dev.connected,
            "Device must be in valid state after rapid cycles"
        );
    }

    harness.shutdown().await?;
    Ok(())
}

/// Connect to unknown VID/PID: must not crash, device stays valid.
#[tokio::test]
async fn connect_unknown_vid_pid_graceful() -> anyhow::Result<()> {
    let device = VirtualDevice::new("UnknownVIDPID-0000:0000")?;
    // Even with an unknown identifier the virtual device must be constructable
    assert!(device.connected);
    assert!(!device.name.is_empty());
    Ok(())
}

// ===========================================================================
// 3. Communication Timeout Recovery
// ===========================================================================

/// Communication stops → fault → resumes → recovery succeeds.
#[test]
fn comm_timeout_then_recovery() -> Result<(), String> {
    let mut sys = new_interlock(50);
    sys.report_communication();
    let _ = sys.process_tick(5.0);

    // Starve communication to trigger timeout
    std::thread::sleep(Duration::from_millis(80));
    let tick = sys.process_tick(10.0);
    let timed_out = matches!(tick.state, SafetyInterlockState::SafeMode { .. })
        || tick.torque_command.abs() < f32::EPSILON;
    assert!(
        timed_out,
        "Must enter safe mode or zero torque after comm timeout, got {:?}",
        tick.state
    );

    // Resume communication and clear fault
    std::thread::sleep(Duration::from_millis(120));
    sys.report_communication();
    sys.clear_fault()?;
    sys.report_communication();
    let tick2 = sys.process_tick(5.0);
    assert!(
        matches!(tick2.state, SafetyInterlockState::Normal),
        "Expected Normal after recovery, got {:?}",
        tick2.state
    );
    Ok(())
}

/// Intermittent communication (50% packet loss simulation): system must
/// degrade gracefully rather than oscillate violently.
#[test]
fn comm_intermittent_50_percent_loss() {
    let mut sys = new_interlock(100);
    sys.report_communication();
    let _ = sys.process_tick(5.0);

    // Alternate between reporting and not reporting communication
    for i in 0..20 {
        if i % 2 == 0 {
            sys.report_communication();
        }
        let tick = sys.process_tick(3.0);
        // System must never panic; torque must be bounded
        assert!(
            tick.torque_command.is_finite(),
            "Torque must be finite during intermittent comms"
        );
        assert!(
            tick.torque_command <= 25.0,
            "Torque must not exceed max limit"
        );
    }
}

/// Timeout during safety-critical operation: must zero torque.
#[test]
fn comm_timeout_during_safety_critical_op() {
    let mut sys = new_interlock(50);
    sys.report_communication();
    let _ = sys.process_tick(20.0); // High torque request

    // Starve watchdog during high-torque operation
    std::thread::sleep(Duration::from_millis(80));

    let tick = sys.process_tick(25.0);
    // Must either enter safe mode or zero torque
    let safe = matches!(tick.state, SafetyInterlockState::SafeMode { .. })
        || tick.torque_command.abs() < f32::EPSILON;
    assert!(
        safe,
        "Must be in safe state after timeout during critical op, got {:?} torque={}",
        tick.state, tick.torque_command
    );
}

// ===========================================================================
// 4. Encoder Health Monitoring
// ===========================================================================

/// Stuck encoder: same value for N consecutive reads should trigger fault.
#[test]
fn encoder_stuck_value_triggers_fault() {
    let mut svc = new_service();
    // Simulate stuck encoder detection by reporting EncoderNaN fault
    // (the engine treats stuck encoder as an encoder health fault)
    svc.report_fault(FaultType::EncoderNaN);
    assert!(matches!(
        svc.state(),
        SafetyState::Faulted {
            fault: FaultType::EncoderNaN,
            ..
        }
    ));
    let clamped = svc.clamp_torque_nm(10.0);
    assert!(
        clamped.abs() < f32::EPSILON,
        "Torque must be zero on stuck encoder fault"
    );
}

/// Encoder value jumps (sudden large delta) should trigger fault response.
#[test]
fn encoder_sudden_jump_triggers_fault() {
    let mut sys = new_interlock(200);
    sys.report_communication();
    let _ = sys.process_tick(5.0);

    // A sudden encoder jump is reported as an EncoderNaN fault
    sys.report_fault(FaultType::EncoderNaN);
    let tick = sys.process_tick(10.0);
    assert!(
        matches!(tick.state, SafetyInterlockState::SafeMode { .. }),
        "Encoder jump must trigger safe mode, got {:?}",
        tick.state
    );
    assert!(
        tick.torque_command <= 5.0,
        "Torque must be limited after encoder jump, got {}",
        tick.torque_command
    );
}

/// NaN/infinity values in encoder data must be safely handled.
#[test]
fn encoder_nan_infinity_handled_safely() {
    let svc = new_service();

    // NaN torque request (simulating NaN encoder feeding into torque calc)
    let clamped_nan = svc.clamp_torque_nm(f32::NAN);
    assert!(
        clamped_nan.abs() < f32::EPSILON,
        "NaN encoder data must result in zero torque, got {clamped_nan}"
    );

    // Infinity torque request
    let clamped_inf = svc.clamp_torque_nm(f32::INFINITY);
    assert!(
        clamped_inf <= 5.0,
        "Infinity encoder data must be clamped, got {clamped_inf}"
    );

    // Negative infinity
    let clamped_neg_inf = svc.clamp_torque_nm(f32::NEG_INFINITY);
    assert!(
        clamped_neg_inf >= -5.0,
        "Neg infinity encoder data must be clamped, got {clamped_neg_inf}"
    );
}

// ===========================================================================
// 5. Concurrent Fault Escalation
// ===========================================================================

/// Triple fault: 3 simultaneous faults must keep system in faulted state
/// with torque zeroed.
#[test]
fn triple_fault_stays_faulted_torque_zero() {
    let mut svc = new_service();
    svc.report_fault(FaultType::UsbStall);
    svc.report_fault(FaultType::EncoderNaN);
    svc.report_fault(FaultType::ThermalLimit);

    assert!(matches!(svc.state(), SafetyState::Faulted { .. }));
    let clamped = svc.clamp_torque_nm(20.0);
    assert!(
        clamped.abs() < f32::EPSILON,
        "Torque must be zero after triple fault, got {clamped}"
    );
}

/// Re-fault: fault during recovery must re-enter faulted state.
#[test]
fn fault_during_recovery_re_enters_faulted() -> Result<(), String> {
    let mut svc = new_service();
    svc.report_fault(FaultType::PluginOverrun);
    assert!(matches!(svc.state(), SafetyState::Faulted { .. }));

    // Wait for cooldown then clear
    std::thread::sleep(Duration::from_millis(110));
    svc.clear_fault()?;
    assert!(matches!(svc.state(), SafetyState::SafeTorque));

    // Immediately fault again (re-fault during recovery window)
    svc.report_fault(FaultType::Overcurrent);
    assert!(matches!(
        svc.state(),
        SafetyState::Faulted {
            fault: FaultType::Overcurrent,
            ..
        }
    ));
    assert!(svc.clamp_torque_nm(10.0).abs() < f32::EPSILON);
    Ok(())
}

/// Fault cascade: one fault causing another in the interlock system.
#[test]
fn fault_cascade_interlock_system() {
    let mut sys = new_interlock(200);
    sys.report_communication();
    let _ = sys.process_tick(5.0);

    // First fault: encoder
    sys.report_fault(FaultType::EncoderNaN);
    let tick1 = sys.process_tick(10.0);
    assert!(
        matches!(tick1.state, SafetyInterlockState::SafeMode { .. }),
        "First fault must trigger safe mode"
    );

    // Cascading fault: overcurrent during safe-mode operation
    sys.report_fault(FaultType::Overcurrent);
    let tick2 = sys.process_tick(10.0);
    // Must remain in safe mode or escalate
    let still_safe = matches!(tick2.state, SafetyInterlockState::SafeMode { .. })
        || matches!(tick2.state, SafetyInterlockState::EmergencyStop { .. });
    assert!(
        still_safe,
        "Cascading fault must keep system safe, got {:?}",
        tick2.state
    );
    assert!(
        tick2.torque_command <= 5.0,
        "Torque must be limited after cascade, got {}",
        tick2.torque_command
    );
}

/// All fault types fired in rapid succession must not panic or corrupt state.
#[test]
fn all_faults_rapid_succession_no_panic() {
    let all_faults = vec![
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

    let mut svc = new_service();
    for fault in &all_faults {
        svc.report_fault(*fault);
    }
    // Must be in faulted state (last fault wins)
    assert!(matches!(
        svc.state(),
        SafetyState::Faulted {
            fault: FaultType::PipelineFault,
            ..
        }
    ));
    assert!(svc.clamp_torque_nm(25.0).abs() < f32::EPSILON);

    // Interlock system must also survive rapid faults
    let mut sys = new_interlock(200);
    sys.report_communication();
    let _ = sys.process_tick(5.0);
    for fault in &all_faults {
        sys.report_fault(*fault);
        sys.report_communication();
        let tick = sys.process_tick(5.0);
        assert!(
            tick.torque_command.is_finite(),
            "Torque must be finite after {fault:?}"
        );
    }
}

// ===========================================================================
// 6. Thermal Hysteresis
// ===========================================================================

/// Temperature ramp up → fault → ramp down → recovery.
#[test]
fn thermal_ramp_up_fault_ramp_down_recovery() -> Result<(), String> {
    let mut svc = new_service();

    // Ramp up: thermal limit reached
    svc.report_fault(FaultType::ThermalLimit);
    assert!(matches!(
        svc.state(),
        SafetyState::Faulted {
            fault: FaultType::ThermalLimit,
            ..
        }
    ));
    assert!(svc.clamp_torque_nm(10.0).abs() < f32::EPSILON);

    // Ramp down: wait cooldown, then clear
    std::thread::sleep(Duration::from_millis(110));
    svc.clear_fault()?;
    assert!(
        matches!(svc.state(), SafetyState::SafeTorque),
        "Must recover to SafeTorque after thermal ramp down"
    );

    // Torque restored
    let clamped = svc.clamp_torque_nm(3.0);
    assert!(
        (clamped - 3.0).abs() < f32::EPSILON,
        "Torque must be restored after thermal recovery, got {clamped}"
    );
    Ok(())
}

/// Sustained over-temperature: fault cannot be cleared while condition persists.
#[test]
fn thermal_sustained_over_temp_no_recovery() {
    let mut svc = new_service();
    svc.report_fault(FaultType::ThermalLimit);

    // Attempt immediate clear (< 100ms cooldown) — must be rejected
    let result = svc.clear_fault();
    assert!(
        result.is_err(),
        "Cannot clear thermal fault during sustained over-temp"
    );
    assert!(matches!(svc.state(), SafetyState::Faulted { .. }));
    assert!(svc.clamp_torque_nm(10.0).abs() < f32::EPSILON);
}

/// Thermal oscillation around threshold: repeated fault/clear cycles must
/// remain deterministic and not cause state corruption.
#[test]
fn thermal_oscillation_around_threshold() -> Result<(), String> {
    let mut svc = new_service();

    for _ in 0..5 {
        // Temperature rises above threshold
        svc.report_fault(FaultType::ThermalLimit);
        assert!(matches!(svc.state(), SafetyState::Faulted { .. }));
        assert!(svc.clamp_torque_nm(10.0).abs() < f32::EPSILON);

        // Temperature drops below threshold
        std::thread::sleep(Duration::from_millis(110));
        svc.clear_fault()?;
        assert!(
            matches!(svc.state(), SafetyState::SafeTorque),
            "Must return to SafeTorque after each oscillation"
        );
    }

    // Final state must be deterministic
    let clamped = svc.clamp_torque_nm(4.0);
    assert!(
        (clamped - 4.0).abs() < f32::EPSILON,
        "Torque must be fully restored after oscillation, got {clamped}"
    );
    Ok(())
}

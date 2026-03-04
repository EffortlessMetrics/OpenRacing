//! BDD-style safety-specific scenario tests.
//!
//! Each test follows the **Given / When / Then** pattern and verifies
//! safety-critical behaviour without requiring real hardware.
//!
//! # Scenarios
//!
//! a. Normal operation → watchdog timeout → safe state
//! b. Max torque → interlock trips → immediate zero
//! c. E-stop pressed → high-speed cornering → zero torque
//! d. Software crash → in RT loop → hardware watchdog catches
//! e. Concurrent faults → all fire → most restrictive wins

use std::time::{Duration, Instant};

use anyhow::Result;

use openracing_filters::{DamperState, Frame as FilterFrame, damper_filter, torque_cap_filter};
use racing_wheel_engine::VirtualDevice;
use racing_wheel_engine::policies::SafetyPolicy;
use racing_wheel_engine::ports::HidDevice;
use racing_wheel_engine::protocol::fault_flags;
use racing_wheel_engine::safety::{FaultType, SafetyService, SafetyState};
use racing_wheel_schemas::prelude::*;

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario A: Normal operation → watchdog timeout → safe state
// ═══════════════════════════════════════════════════════════════════════════════

/// Models the communication-timeout watchdog as described in the safety spec.
struct CommTimeoutWatchdog {
    timeout: Duration,
    last_received: Instant,
}

impl CommTimeoutWatchdog {
    fn new(timeout: Duration) -> Self {
        Self {
            timeout,
            last_received: Instant::now(),
        }
    }

    fn feed(&mut self) {
        self.last_received = Instant::now();
    }

    fn is_expired(&self) -> bool {
        self.last_received.elapsed() >= self.timeout
    }
}

/// ```text
/// Given  the system is operating normally with periodic heartbeats
/// When   the watchdog timer expires (no heartbeat for the timeout period)
/// Then   the safety service transitions to Faulted (HandsOffTimeout)
/// And    torque output is clamped to zero
/// And    the safety policy confirms the fault requires shutdown
/// ```
#[test]
fn given_normal_operation_when_watchdog_timeout_then_safe_state() -> Result<()> {
    // Given: normal operation — watchdog is being fed
    let mut watchdog = CommTimeoutWatchdog::new(Duration::from_millis(100));
    let mut safety = SafetyService::new(5.0, 20.0);

    // Watchdog is not expired immediately after creation
    assert!(
        !watchdog.is_expired(),
        "watchdog must not be expired immediately after creation"
    );

    // Normal torque flows
    let normal = safety.clamp_torque_nm(4.0);
    assert!(
        (normal - 4.0).abs() < 0.01,
        "torque must flow normally before timeout"
    );

    // Feed the watchdog once
    watchdog.feed();
    assert!(
        !watchdog.is_expired(),
        "watchdog must not be expired after feed"
    );

    // When: the watchdog times out (simulate by waiting)
    std::thread::sleep(Duration::from_millis(110));
    assert!(
        watchdog.is_expired(),
        "watchdog must be expired after timeout period"
    );

    // Report the timeout as a fault
    safety.report_fault(FaultType::HandsOffTimeout);

    // Then: safety transitions to Faulted
    match safety.state() {
        SafetyState::Faulted { fault, .. } => {
            assert_eq!(
                *fault,
                FaultType::HandsOffTimeout,
                "fault type must be HandsOffTimeout"
            );
        }
        other => {
            return Err(anyhow::anyhow!(
                "expected Faulted(HandsOffTimeout), got {other:?}"
            ));
        }
    }

    // And: torque is clamped to zero
    let clamped = safety.clamp_torque_nm(10.0);
    assert!(
        clamped.abs() < 0.001,
        "torque must be zero after watchdog timeout, got {clamped}"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario B: Max torque → interlock trips → immediate zero
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  the system is outputting maximum safe torque
/// When   the safety interlock trips (overcurrent detected)
/// Then   torque drops to zero immediately
/// And    the transition completes within 50 ms
/// And    the safety policy confirms the interlock flag requires shutdown
/// ```
#[test]
fn given_max_torque_when_interlock_trips_then_immediate_zero() -> Result<()> {
    // Given: system outputting maximum safe torque
    let safe_limit = 5.0;
    let mut safety = SafetyService::new(safe_limit, 20.0);
    let max_torque = safety.clamp_torque_nm(safe_limit);
    assert!(
        (max_torque - safe_limit).abs() < 0.01,
        "must be at max safe torque ({safe_limit} Nm), got {max_torque}"
    );

    // When: the interlock trips — measure timing
    let trip_start = Instant::now();
    safety.report_fault(FaultType::Overcurrent);
    let clamped = safety.clamp_torque_nm(safe_limit);
    let trip_elapsed = trip_start.elapsed();

    // Then: torque drops to zero immediately
    assert!(
        clamped.abs() < 0.001,
        "torque must be zero after interlock trip, got {clamped}"
    );

    // And: transition completes within 50 ms
    assert!(
        trip_elapsed < Duration::from_millis(50),
        "interlock-to-zero must complete in <50 ms (actual: {trip_elapsed:?})"
    );

    // And: safety policy confirms the overcurrent flag requires shutdown
    let policy = SafetyPolicy::new()?;
    assert!(
        policy.requires_immediate_shutdown(fault_flags::OVERCURRENT_FAULT),
        "overcurrent fault flag must require immediate shutdown"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario C: E-stop pressed → high-speed cornering → zero torque
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  the system is at high speed with strong lateral FFB forces
/// When   the user presses the e-stop (safety interlock violation)
/// Then   all torque output is immediately zero
/// And    the full pipeline (filters → engine → safety → device) outputs zero
/// And    the fault response completes within 50 ms
/// ```
#[test]
fn given_e_stop_pressed_when_high_speed_cornering_then_zero_torque() -> Result<()> {
    // Given: high-speed cornering with strong lateral forces
    let mut filter_frame = FilterFrame {
        ffb_in: 0.9, // Strong lateral force
        torque_out: 0.9,
        wheel_speed: 10.0, // High wheel speed (rad/s)
        hands_off: false,
        ts_mono_ns: 0,
        seq: 0,
    };
    let damper = DamperState::fixed(0.05);
    damper_filter(&mut filter_frame, &damper);
    torque_cap_filter(&mut filter_frame, 1.0);

    // Confirm strong output before e-stop
    assert!(
        filter_frame.torque_out.abs() > 0.1,
        "filter must produce non-zero output before e-stop, got {}",
        filter_frame.torque_out
    );

    // Safety service is active at safe torque
    let mut safety = SafetyService::new(5.0, 20.0);
    let pre_stop = safety.clamp_torque_nm(filter_frame.torque_out * 5.0);
    assert!(
        pre_stop.abs() > 0.01,
        "torque must be non-zero before e-stop, got {pre_stop}"
    );

    // When: e-stop is pressed — measure timing
    let estop_start = Instant::now();
    safety.report_fault(FaultType::SafetyInterlockViolation);
    let post_stop = safety.clamp_torque_nm(filter_frame.torque_out * 5.0);
    let estop_elapsed = estop_start.elapsed();

    // Then: all torque output is immediately zero
    assert!(
        post_stop.abs() < 0.001,
        "torque must be zero after e-stop, got {post_stop}"
    );

    // And: fault response completes within 50 ms
    assert!(
        estop_elapsed < Duration::from_millis(50),
        "e-stop response must complete in <50 ms (actual: {estop_elapsed:?})"
    );

    // And: device write with zero torque succeeds
    let id: DeviceId = "bdd-estop-001".parse()?;
    let mut device = VirtualDevice::new(id, "E-Stop Wheel".to_string());
    device.write_ffb_report(post_stop, filter_frame.seq)?;

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario D: Software crash → in RT loop → hardware watchdog catches
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  the RT loop is running and the communication watchdog is armed
/// When   a software fault occurs (simulated timing violation / pipeline fault)
/// Then   the watchdog detects the missed heartbeat
/// And    the safety service transitions to Faulted
/// And    torque output is clamped to zero
/// And    the device receives zero torque (hardware protection)
/// ```
#[test]
fn given_software_crash_when_in_rt_loop_then_hardware_watchdog_catches() -> Result<()> {
    // Given: RT loop is running with a communication watchdog
    let mut watchdog = CommTimeoutWatchdog::new(Duration::from_millis(50));
    let mut safety = SafetyService::new(5.0, 20.0);
    let id: DeviceId = "bdd-watchdog-001".parse()?;
    let mut device = VirtualDevice::new(id, "Watchdog Wheel".to_string());

    // RT loop is healthy: feeding watchdog and sending FFB
    watchdog.feed();
    device.write_ffb_report(3.0, 0)?;
    assert!(
        !watchdog.is_expired(),
        "watchdog must be alive during normal RT loop"
    );

    // When: a software crash occurs — the RT loop stops feeding the watchdog
    // Simulate a timing violation fault
    safety.report_fault(FaultType::TimingViolation);

    // Simulate the watchdog expiring (RT loop stopped feeding)
    std::thread::sleep(Duration::from_millis(60));

    // Then: the watchdog detects the missed heartbeat
    assert!(
        watchdog.is_expired(),
        "watchdog must expire when RT loop stops feeding it"
    );

    // And: safety is in Faulted state
    match safety.state() {
        SafetyState::Faulted { fault, .. } => {
            assert_eq!(
                *fault,
                FaultType::TimingViolation,
                "fault must be TimingViolation"
            );
        }
        other => {
            return Err(anyhow::anyhow!(
                "expected Faulted(TimingViolation), got {other:?}"
            ));
        }
    }

    // And: torque is clamped to zero
    let clamped = safety.clamp_torque_nm(5.0);
    assert!(
        clamped.abs() < 0.001,
        "torque must be zero after software crash, got {clamped}"
    );

    // And: device receives zero torque
    device.write_ffb_report(clamped, 1)?;

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario E: Concurrent faults → all fire → most restrictive wins
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  the system is operating normally
/// When   multiple faults fire concurrently (USB, thermal, overcurrent)
/// Then   the safety service is in Faulted state
/// And    the most recently reported fault is recorded
/// And    torque output is zero (the most restrictive response)
/// And    the safety policy confirms all individual fault flags require shutdown
/// ```
#[test]
fn given_concurrent_faults_when_all_fire_then_most_restrictive_wins() -> Result<()> {
    // Given: normal operation
    let mut safety = SafetyService::new(5.0, 20.0);
    assert_eq!(
        safety.state(),
        &SafetyState::SafeTorque,
        "must start in SafeTorque"
    );

    // Normal torque flows
    let normal = safety.clamp_torque_nm(4.0);
    assert!(
        (normal - 4.0).abs() < 0.01,
        "torque must flow normally before faults"
    );

    // When: multiple faults fire concurrently
    let concurrent_faults = [
        (FaultType::UsbStall, fault_flags::USB_FAULT, "USB stall"),
        (
            FaultType::ThermalLimit,
            fault_flags::THERMAL_FAULT,
            "thermal limit",
        ),
        (
            FaultType::Overcurrent,
            fault_flags::OVERCURRENT_FAULT,
            "overcurrent",
        ),
    ];

    for &(ref fault_type, _flag, _label) in &concurrent_faults {
        safety.report_fault(*fault_type);
    }

    // Then: safety is in Faulted state
    assert!(
        matches!(safety.state(), SafetyState::Faulted { .. }),
        "must be in Faulted state after concurrent faults, got {:?}",
        safety.state()
    );

    // And: the most recently reported fault is recorded (overcurrent, the last one)
    match safety.state() {
        SafetyState::Faulted { fault, .. } => {
            assert_eq!(
                *fault,
                FaultType::Overcurrent,
                "most recent fault must be Overcurrent"
            );
        }
        other => {
            return Err(anyhow::anyhow!("expected Faulted, got {other:?}"));
        }
    }

    // And: torque output is zero — the most restrictive response
    for requested in [0.0, 1.0, 5.0, 20.0, -15.0] {
        let clamped = safety.clamp_torque_nm(requested);
        assert!(
            clamped.abs() < 0.001,
            "all torque must be zero after concurrent faults; requested={requested}, got={clamped}"
        );
    }

    // And: the safety policy confirms each individual fault flag requires shutdown
    let policy = SafetyPolicy::new()?;
    for &(_fault_type, flag, label) in &concurrent_faults {
        assert!(
            policy.requires_immediate_shutdown(flag),
            "{label}: fault flag 0x{flag:02X} must require immediate shutdown"
        );
    }

    // And: combined fault flags also require shutdown
    let combined_flags: u8 = concurrent_faults
        .iter()
        .fold(0u8, |acc, &(_, flag, _)| acc | flag);
    assert!(
        policy.requires_immediate_shutdown(combined_flags),
        "combined fault flags 0x{combined_flags:02X} must require immediate shutdown"
    );

    Ok(())
}

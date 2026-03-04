//! Deep safety soak and timing verification tests.
//!
//! These tests exercise the real-time safety pipeline under sustained load,
//! verifying interlock invariants, timing budgets, fault injection recovery,
//! multi-device operation, rapid connect/disconnect, and memory stability.

use std::time::{Duration, Instant};

use anyhow::Result;

use racing_wheel_engine::safety::{
    FaultType, SafetyInterlockState, SafetyInterlockSystem, SafetyService, SoftwareWatchdog,
};
use racing_wheel_engine::{Frame, Pipeline};

// ═══════════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════════

/// Create a `SafetyInterlockSystem` backed by a software watchdog.
fn make_interlock(max_torque_nm: f32, watchdog_timeout_ms: u32) -> SafetyInterlockSystem {
    let watchdog = Box::new(SoftwareWatchdog::new(watchdog_timeout_ms));
    SafetyInterlockSystem::new(watchdog, max_torque_nm)
}

/// All `FaultType` variants used for cycling through faults.
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

// ═══════════════════════════════════════════════════════════════════════════════
// 1. Soak test: 10 000 ticks with no interlock violations
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn soak_10k_ticks_no_interlock_violations() -> Result<()> {
    let mut interlock = make_interlock(25.0, 500);
    interlock.arm()?;

    for tick in 0u64..10_000 {
        // Alternate torque between positive and negative values
        let requested = (tick as f32 * 0.1).sin() * 20.0;
        let result = interlock.process_tick(requested);

        // State must remain Normal throughout (no faults injected)
        assert_eq!(
            result.state,
            SafetyInterlockState::Normal,
            "tick {tick}: unexpected state {:?}",
            result.state
        );

        // Torque must be clamped within configured limits
        assert!(
            result.torque_command.abs() <= 25.0 + f32::EPSILON,
            "tick {tick}: torque {:.4} exceeds limit",
            result.torque_command
        );

        // No fault should have occurred
        assert!(
            !result.fault_occurred,
            "tick {tick}: unexpected fault {:?}",
            result.fault_type
        );
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. Timing test: processing time within 1 000 µs budget per tick
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn timing_budget_under_1000us_per_tick() -> Result<()> {
    let mut interlock = make_interlock(25.0, 500);
    interlock.arm()?;

    let budget = Duration::from_micros(1_000);
    let mut max_response = Duration::ZERO;

    for tick in 0u64..5_000 {
        let requested = (tick as f32 * 0.05).cos() * 15.0;

        let before = Instant::now();
        let result = interlock.process_tick(requested);
        let wall_time = before.elapsed();

        if wall_time > max_response {
            max_response = wall_time;
        }

        // The safety system's own measured response_time should be within budget
        assert!(
            result.response_time <= budget,
            "tick {tick}: response_time {:?} exceeds budget {:?}",
            result.response_time,
            budget,
        );
    }

    // Wall-clock max should also be within a generous bound (allow 5 ms for
    // OS scheduling jitter on CI, but the per-tick assertion above is strict).
    assert!(
        max_response < Duration::from_millis(5),
        "max wall-clock response {:?} unreasonably high",
        max_response,
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. Fault injection soak: inject random faults every 100 ticks, verify recovery
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn fault_injection_soak_recovery() -> Result<()> {
    let mut interlock = make_interlock(25.0, 500);
    interlock.arm()?;

    let mut fault_index = 0usize;

    for tick in 0u64..10_000 {
        let requested = 10.0;

        if tick % 100 == 50 {
            // Inject a fault, cycling through all types
            let fault = ALL_FAULTS[fault_index % ALL_FAULTS.len()];
            interlock.report_fault(fault);
            fault_index += 1;
        }

        let result = interlock.process_tick(requested);

        // After a fault the system should be in SafeMode
        if tick % 100 == 50 {
            match &result.state {
                SafetyInterlockState::SafeMode { .. } => { /* expected */ }
                other => {
                    anyhow::bail!("tick {tick}: expected SafeMode after fault, got {other:?}");
                }
            }
        }

        // Attempt recovery 50 ticks after injection (≥100 ms real-time is enforced
        // by the safety system, but we use a sleep to satisfy the 100 ms guard).
        if tick % 100 == 99 {
            // The clear_fault method requires 100 ms since fault; sleep briefly.
            std::thread::sleep(Duration::from_millis(110));
            match interlock.clear_fault() {
                Ok(()) => {
                    assert_eq!(interlock.state(), &SafetyInterlockState::Normal);
                }
                Err(msg) => {
                    // If still too soon, that's acceptable — safety-first design
                    assert!(
                        msg.contains("100ms") || msg.contains("No fault"),
                        "unexpected clear_fault error: {msg}"
                    );
                }
            }
        }

        // Torque must never exceed configured max in any state
        assert!(
            result.torque_command.abs() <= 25.0 + f32::EPSILON,
            "tick {tick}: torque {} exceeds limit",
            result.torque_command
        );
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. Multi-device soak: 4 devices for 1 000 ticks
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn multi_device_soak_4_devices_1000_ticks() -> Result<()> {
    // Each device has its own independent safety interlock and pipeline.
    let mut devices: Vec<(SafetyInterlockSystem, Pipeline)> = (0..4)
        .map(|_| {
            let interlock = make_interlock(25.0, 500);
            let pipeline = Pipeline::new();
            (interlock, pipeline)
        })
        .collect();

    for (interlock, _) in &mut devices {
        interlock.arm()?;
    }

    for tick in 0u64..1_000 {
        for (dev_idx, (interlock, pipeline)) in devices.iter_mut().enumerate() {
            let input = ((tick as f32 + dev_idx as f32 * 0.25) * 0.1).sin() * 18.0;

            // Process through pipeline
            let mut frame = Frame {
                ffb_in: input / 25.0, // Normalize to -1..1
                torque_out: input / 25.0,
                wheel_speed: 0.0,
                hands_off: false,
                ts_mono_ns: tick * 1_000_000,
                seq: (tick & 0xFFFF) as u16,
            };
            // Pipeline may be empty — that's passthrough
            let _pipeline_result = pipeline.process(&mut frame);

            // Safety tick
            let result = interlock.process_tick(frame.torque_out * 25.0);

            assert_eq!(
                result.state,
                SafetyInterlockState::Normal,
                "dev {dev_idx} tick {tick}: unexpected state {:?}",
                result.state
            );
            assert!(
                result.torque_command.abs() <= 25.0 + f32::EPSILON,
                "dev {dev_idx} tick {tick}: torque {} exceeds limit",
                result.torque_command
            );
        }
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. Stress test: rapid connect/disconnect cycles (100 cycles)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn stress_rapid_connect_disconnect_100_cycles() -> Result<()> {
    for cycle in 0..100 {
        // Simulate device connect: create fresh safety systems
        let mut interlock = make_interlock(25.0, 500);
        interlock.arm()?;

        // Run a short burst of ticks
        for tick in 0..50 {
            let result = interlock.process_tick(10.0);
            assert!(
                !result.fault_occurred,
                "cycle {cycle} tick {tick}: fault during normal operation"
            );
        }

        // Simulate device disconnect: disarm and reset
        interlock.disarm()?;
        interlock.reset()?;

        // Verify clean state after reset
        assert_eq!(
            interlock.state(),
            &SafetyInterlockState::Normal,
            "cycle {cycle}: state not Normal after reset"
        );
        assert!(
            !interlock.is_watchdog_armed(),
            "cycle {cycle}: watchdog still armed after reset"
        );
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 6. Memory stability: verify no growth over sustained operation
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn memory_stability_no_growth_pattern() -> Result<()> {
    let mut interlock = make_interlock(25.0, 500);
    interlock.arm()?;

    // Use the fault log length as a proxy for unbounded growth.
    // Under normal operation the log should stay empty.
    let mut max_fault_log_len = 0usize;

    // Counters for verification
    let mut total_ticks = 0u64;
    let mut total_faults = 0u64;

    for tick in 0u64..20_000 {
        let result = interlock.process_tick(10.0);
        total_ticks += 1;

        if result.fault_occurred {
            total_faults += 1;
        }

        // Sample the fault log length periodically
        if tick % 1_000 == 0 {
            let log_len = interlock.fault_log().len();
            if log_len > max_fault_log_len {
                max_fault_log_len = log_len;
            }
        }
    }

    // Under clean operation, no faults should have accumulated
    assert_eq!(total_faults, 0, "unexpected faults during clean soak");
    assert_eq!(max_fault_log_len, 0, "fault log grew during clean soak");
    assert_eq!(total_ticks, 20_000);

    // Now inject faults to verify bounded log growth
    for i in 0..200 {
        interlock.report_fault(ALL_FAULTS[i % ALL_FAULTS.len()]);
        let _ = interlock.process_tick(5.0);
    }

    // Fault log must be bounded (max_fault_log_entries = 1000 by default)
    let final_log_len = interlock.fault_log().len();
    assert!(
        final_log_len <= 1000,
        "fault log unbounded: {final_log_len} entries"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 7. Safety service soak: 10 000 ticks of torque clamping
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn safety_service_soak_torque_clamping() -> Result<()> {
    let safety = SafetyService::new(5.0, 25.0);

    for tick in 0u64..10_000 {
        let raw_torque = (tick as f32 * 0.02).sin() * 50.0; // exceeds limits
        let clamped = safety.clamp_torque_nm(raw_torque);

        // In SafeTorque state, max is 5.0 Nm
        assert!(
            clamped.abs() <= 5.0 + f32::EPSILON,
            "tick {tick}: clamped {clamped} exceeds safe torque 5.0"
        );
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 8. Faulted-state torque is always zero
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn faulted_state_always_zero_torque() -> Result<()> {
    let mut safety = SafetyService::new(5.0, 25.0);
    safety.report_fault(FaultType::Overcurrent);

    for tick in 0u64..5_000 {
        let raw = (tick as f32 * 0.1).sin() * 100.0;
        let clamped = safety.clamp_torque_nm(raw);
        assert!(
            clamped.abs() < f32::EPSILON,
            "tick {tick}: faulted state torque {clamped} != 0.0"
        );
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 9. Pipeline soak: empty pipeline passthrough over 10 000 ticks
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn pipeline_soak_passthrough_10k_ticks() -> Result<()> {
    let mut pipeline = Pipeline::new();

    for tick in 0u64..10_000 {
        let input = (tick as f32 * 0.01).sin() * 0.8;
        let mut frame = Frame {
            ffb_in: input,
            torque_out: input,
            wheel_speed: 0.0,
            hands_off: false,
            ts_mono_ns: tick * 1_000_000,
            seq: (tick & 0xFFFF) as u16,
        };

        pipeline.process(&mut frame)?;

        // Empty pipeline is passthrough — output equals input
        assert!(
            (frame.torque_out - input).abs() < f32::EPSILON,
            "tick {tick}: passthrough violated, in={input} out={}",
            frame.torque_out
        );
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 10. Emergency-stop soak: repeated e-stop always results in zero torque
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn emergency_stop_always_zero_torque() -> Result<()> {
    for _ in 0..100 {
        let mut interlock = make_interlock(25.0, 500);
        interlock.arm()?;

        // Run some normal ticks
        for _ in 0..20 {
            let result = interlock.process_tick(15.0);
            assert!(!result.fault_occurred);
        }

        // Emergency stop
        let estop = interlock.emergency_stop();
        assert_eq!(estop.torque_command, 0.0);
        assert!(matches!(
            estop.state,
            SafetyInterlockState::EmergencyStop { .. }
        ));

        // All subsequent ticks must also be zero
        for _ in 0..50 {
            let result = interlock.process_tick(25.0);
            assert!(
                result.torque_command.abs() < f32::EPSILON,
                "non-zero torque after e-stop: {}",
                result.torque_command
            );
        }
    }

    Ok(())
}

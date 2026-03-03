//! End-to-end safety system integration tests.
//!
//! Cross-crate coverage: engine (SafetyService, SafetyState, FaultType, Pipeline,
//! VirtualDevice) × schemas (TorqueNm, DeviceCapabilities) × filters (torque_cap_filter,
//! damper_filter) × service (WheelService).
//!
//! Scenarios:
//! 1. Safety system responds to simulated faults
//! 2. Torque limiting under fault conditions
//! 3. Watchdog timeout triggers safe state
//! 4. Recovery from safe state

use std::time::Duration;

use anyhow::Result;

use racing_wheel_engine::ports::HidDevice;
use racing_wheel_engine::safety::{FaultType, SafetyService, SafetyState};
use racing_wheel_engine::{Frame, Pipeline, VirtualDevice};
use racing_wheel_schemas::prelude::*;

use openracing_filters::{
    DamperState, Frame as FilterFrame, FrictionState, damper_filter, friction_filter,
    torque_cap_filter,
};

// ═══════════════════════════════════════════════════════════════════════════════
// 1. Safety system responds to simulated faults
// ═══════════════════════════════════════════════════════════════════════════════

/// Each FaultType variant must transition the safety service into Faulted state.
#[test]
fn safety_all_fault_types_transition_to_faulted() -> Result<()> {
    let fault_types = [
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

    for fault in &fault_types {
        let mut safety = SafetyService::new(5.0, 20.0);
        assert_eq!(
            safety.state(),
            &SafetyState::SafeTorque,
            "{fault:?}: initial state must be SafeTorque"
        );

        safety.report_fault(*fault);

        match safety.state() {
            SafetyState::Faulted {
                fault: reported, ..
            } => {
                assert_eq!(
                    reported, fault,
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

/// A USB stall fault detected during an active pipeline cycle must cause
/// the safety service to immediately clamp torque to zero. This tests the
/// cross-crate flow: engine Pipeline → SafetyService → device write.
#[test]
fn safety_usb_stall_clamps_pipeline_output_to_zero() -> Result<()> {
    let mut pipeline = Pipeline::new();
    let mut safety = SafetyService::new(5.0, 20.0);

    // Normal pipeline cycle
    let mut frame = Frame {
        ffb_in: 0.8,
        torque_out: 0.8,
        wheel_speed: 1.0,
        hands_off: false,
        ts_mono_ns: 1_000_000,
        seq: 0,
    };
    pipeline.process(&mut frame)?;
    let normal_torque = safety.clamp_torque_nm(frame.torque_out * 5.0);
    assert!(
        normal_torque.abs() > 0.01,
        "normal torque must be non-zero, got {normal_torque}"
    );

    // Report USB stall (simulating hot-unplug)
    safety.report_fault(FaultType::UsbStall);

    // Same pipeline output must now be clamped to zero
    let faulted_torque = safety.clamp_torque_nm(frame.torque_out * 5.0);
    assert!(
        faulted_torque.abs() < 0.001,
        "faulted torque must be zero, got {faulted_torque}"
    );

    Ok(())
}

/// A device with injected fault flags must be detectable, and the safety
/// service must react accordingly when fed the fault information.
#[test]
fn safety_device_fault_flags_trigger_safety_response() -> Result<()> {
    let id: DeviceId = "safety-fault-001".parse()?;
    let mut device = VirtualDevice::new(id, "Safety Fault Wheel".to_string());
    let mut safety = SafetyService::new(5.0, 20.0);

    // Normal operation
    device.write_ffb_report(3.0, 0)?;
    let telem = device
        .read_telemetry()
        .ok_or_else(|| anyhow::anyhow!("telemetry missing"))?;
    assert_eq!(telem.fault_flags, 0, "no faults initially");

    // Inject fault into device
    device.inject_fault(0x01); // bit 0 = overtemperature
    let telem_faulted = device
        .read_telemetry()
        .ok_or_else(|| anyhow::anyhow!("telemetry missing after fault injection"))?;
    assert_ne!(telem_faulted.fault_flags, 0, "fault flag must be set");

    // Safety service reacts to the detected fault
    safety.report_fault(FaultType::ThermalLimit);
    let clamped = safety.clamp_torque_nm(5.0);
    assert!(
        clamped.abs() < 0.001,
        "torque must be zero after thermal fault, got {clamped}"
    );

    Ok(())
}

/// Multiple faults reported in sequence must keep the service in Faulted state
/// with the most recent fault recorded.
#[test]
fn safety_sequential_faults_keep_faulted_state() -> Result<()> {
    let mut safety = SafetyService::new(5.0, 20.0);

    safety.report_fault(FaultType::UsbStall);
    safety.report_fault(FaultType::EncoderNaN);
    safety.report_fault(FaultType::ThermalLimit);

    match safety.state() {
        SafetyState::Faulted { fault, .. } => {
            assert_eq!(
                *fault,
                FaultType::ThermalLimit,
                "most recent fault must be ThermalLimit"
            );
        }
        other => {
            return Err(anyhow::anyhow!("expected Faulted, got {other:?}"));
        }
    }

    // Torque still clamped
    let clamped = safety.clamp_torque_nm(10.0);
    assert!(clamped.abs() < 0.001);

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. Torque limiting under fault conditions
// ═══════════════════════════════════════════════════════════════════════════════

/// In SafeTorque state, torque must be clamped to the safe limit (not the
/// high-torque limit).
#[test]
fn torque_limiting_safe_torque_state() -> Result<()> {
    let safe_limit = 5.0;
    let high_limit = 20.0;
    let safety = SafetyService::new(safe_limit, high_limit);

    // Request above safe limit
    let clamped = safety.clamp_torque_nm(15.0);
    assert!(
        clamped <= safe_limit,
        "SafeTorque must clamp to {safe_limit}, got {clamped}"
    );

    // Negative torque clamped symmetrically
    let clamped_neg = safety.clamp_torque_nm(-15.0);
    assert!(
        clamped_neg >= -safe_limit,
        "SafeTorque must clamp to -{safe_limit}, got {clamped_neg}"
    );

    // Within-limit request passes through
    let pass_through = safety.clamp_torque_nm(3.0);
    assert!(
        (pass_through - 3.0).abs() < 0.01,
        "within-limit torque must pass through, got {pass_through}"
    );

    Ok(())
}

/// Faulted state must clamp all torque to zero regardless of request magnitude.
#[test]
fn torque_limiting_faulted_state_zeroes_all() -> Result<()> {
    let mut safety = SafetyService::new(5.0, 20.0);
    safety.report_fault(FaultType::Overcurrent);

    for requested in [0.0, 1.0, 5.0, 20.0, -3.0, -20.0] {
        let clamped = safety.clamp_torque_nm(requested);
        assert!(
            clamped.abs() < 0.001,
            "faulted state must zero torque; requested={requested}, got={clamped}"
        );
    }

    Ok(())
}

/// Cross-crate torque flow: filter pipeline (openracing-filters) → engine
/// pipeline → safety clamp → device write.  Under fault the final torque
/// written to the device must be zero even though the filters produce non-zero
/// output.
#[test]
fn torque_limiting_full_pipeline_to_device_under_fault() -> Result<()> {
    // Filter pipeline (openracing-filters crate)
    let mut filter_frame = FilterFrame {
        ffb_in: 0.7,
        torque_out: 0.7,
        wheel_speed: 2.0,
        hands_off: false,
        ts_mono_ns: 0,
        seq: 0,
    };
    let damper = DamperState::fixed(0.02);
    let friction = FrictionState::fixed(0.01);
    damper_filter(&mut filter_frame, &damper);
    friction_filter(&mut filter_frame, &friction);
    torque_cap_filter(&mut filter_frame, 1.0);

    assert!(
        filter_frame.torque_out.is_finite(),
        "filter output must be finite"
    );
    assert!(
        filter_frame.torque_out.abs() <= 1.0,
        "filter output must be in [-1, 1]"
    );

    // Engine pipeline
    let mut engine_frame = Frame {
        ffb_in: filter_frame.torque_out,
        torque_out: filter_frame.torque_out,
        wheel_speed: filter_frame.wheel_speed,
        hands_off: false,
        ts_mono_ns: 1_000_000,
        seq: 1,
    };
    let mut pipeline = Pipeline::new();
    pipeline.process(&mut engine_frame)?;

    // Safety in faulted state
    let mut safety = SafetyService::new(5.0, 20.0);
    safety.report_fault(FaultType::PipelineFault);
    let final_torque = safety.clamp_torque_nm(engine_frame.torque_out * 5.0);
    assert!(
        final_torque.abs() < 0.001,
        "faulted pipeline must produce zero torque, got {final_torque}"
    );

    // Device write with zero torque succeeds
    let id: DeviceId = "torque-limit-device-001".parse()?;
    let mut device = VirtualDevice::new(id, "Torque Limit Wheel".to_string());
    device.write_ffb_report(final_torque, engine_frame.seq)?;

    Ok(())
}

/// Torque at exactly the safe limit boundary must be accepted (edge case).
#[test]
fn torque_limiting_boundary_values() -> Result<()> {
    let safe_limit = 5.0;
    let safety = SafetyService::new(safe_limit, 20.0);

    let at_limit = safety.clamp_torque_nm(safe_limit);
    assert!(
        (at_limit - safe_limit).abs() < 0.01,
        "torque at exactly safe limit must pass through, got {at_limit}"
    );

    let at_neg_limit = safety.clamp_torque_nm(-safe_limit);
    assert!(
        (at_neg_limit - (-safe_limit)).abs() < 0.01,
        "negative torque at exactly safe limit must pass through, got {at_neg_limit}"
    );

    let zero = safety.clamp_torque_nm(0.0);
    assert!(
        zero.abs() < 0.001,
        "zero torque must remain zero, got {zero}"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. Watchdog timeout triggers safe state
// ═══════════════════════════════════════════════════════════════════════════════

/// HandsOffTimeout fault type must transition to Faulted and zero torque.
#[test]
fn watchdog_hands_off_timeout_faults() -> Result<()> {
    let mut safety = SafetyService::new(5.0, 20.0);

    safety.report_fault(FaultType::HandsOffTimeout);

    match safety.state() {
        SafetyState::Faulted { fault, .. } => {
            assert_eq!(*fault, FaultType::HandsOffTimeout);
        }
        other => {
            return Err(anyhow::anyhow!(
                "expected Faulted(HandsOffTimeout), got {other:?}"
            ));
        }
    }

    let clamped = safety.clamp_torque_nm(10.0);
    assert!(
        clamped.abs() < 0.001,
        "hands-off timeout must zero torque, got {clamped}"
    );

    Ok(())
}

/// TimingViolation fault (simulating watchdog / jitter breach) must
/// immediately transition to faulted state.
#[test]
fn watchdog_timing_violation_faults() -> Result<()> {
    let mut safety = SafetyService::new(5.0, 20.0);

    safety.report_fault(FaultType::TimingViolation);

    match safety.state() {
        SafetyState::Faulted { fault, .. } => {
            assert_eq!(*fault, FaultType::TimingViolation);
        }
        other => {
            return Err(anyhow::anyhow!(
                "expected Faulted(TimingViolation), got {other:?}"
            ));
        }
    }

    // Even a small torque request is zeroed
    let clamped = safety.clamp_torque_nm(0.1);
    assert!(clamped.abs() < 0.001);

    Ok(())
}

/// Simulate a full watchdog scenario: normal operation → timing fault →
/// safety clamps torque → device receives zero.
#[test]
fn watchdog_full_scenario_normal_to_fault_to_zero_torque() -> Result<()> {
    let id: DeviceId = "watchdog-scenario-001".parse()?;
    let mut device = VirtualDevice::new(id, "Watchdog Wheel".to_string());
    let mut pipeline = Pipeline::new();
    let mut safety = SafetyService::new(5.0, 20.0);

    // Normal tick
    let mut frame = Frame {
        ffb_in: 0.5,
        torque_out: 0.5,
        wheel_speed: 1.0,
        hands_off: false,
        ts_mono_ns: 1_000_000,
        seq: 0,
    };
    pipeline.process(&mut frame)?;
    let torque = safety.clamp_torque_nm(frame.torque_out * 5.0);
    device.write_ffb_report(torque, 0)?;
    assert!(
        torque.abs() > 0.01,
        "normal torque must be non-zero, got {torque}"
    );

    // Watchdog fires: timing violation
    safety.report_fault(FaultType::TimingViolation);

    // Next tick: pipeline still produces output but safety zeros it
    let mut frame2 = Frame {
        ffb_in: 0.5,
        torque_out: 0.5,
        wheel_speed: 1.0,
        hands_off: false,
        ts_mono_ns: 2_000_000,
        seq: 1,
    };
    pipeline.process(&mut frame2)?;
    let faulted_torque = safety.clamp_torque_nm(frame2.torque_out * 5.0);
    assert!(
        faulted_torque.abs() < 0.001,
        "post-fault torque must be zero, got {faulted_torque}"
    );

    // Device receives zero
    device.write_ffb_report(faulted_torque, 1)?;

    Ok(())
}

/// SafetyInterlockViolation fault from the interlock subsystem must trigger
/// safe state.
#[test]
fn watchdog_interlock_violation_faults() -> Result<()> {
    let mut safety = SafetyService::new(5.0, 20.0);

    safety.report_fault(FaultType::SafetyInterlockViolation);

    assert!(
        matches!(safety.state(), SafetyState::Faulted { .. }),
        "interlock violation must fault the safety service"
    );
    assert!(safety.clamp_torque_nm(5.0).abs() < 0.001);

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. Recovery from safe state
// ═══════════════════════════════════════════════════════════════════════════════

/// After the minimum hold period, clearing a fault must restore SafeTorque
/// state and allow torque to flow again.
#[test]
fn recovery_clear_fault_restores_safe_torque() -> Result<()> {
    let mut safety = SafetyService::new(5.0, 20.0);

    safety.report_fault(FaultType::UsbStall);
    assert!(matches!(safety.state(), SafetyState::Faulted { .. }));

    // Wait for minimum hold period
    std::thread::sleep(Duration::from_millis(120));
    safety.clear_fault().map_err(|e| anyhow::anyhow!("{e}"))?;

    assert_eq!(safety.state(), &SafetyState::SafeTorque);

    // Torque flows again (within safe limit)
    let torque = safety.clamp_torque_nm(3.0);
    assert!(
        (torque - 3.0).abs() < 0.01,
        "torque must flow after recovery, got {torque}"
    );

    Ok(())
}

/// Clearing fault before the minimum hold period must fail.
#[test]
fn recovery_clear_fault_too_early_fails() -> Result<()> {
    let mut safety = SafetyService::new(5.0, 20.0);

    safety.report_fault(FaultType::EncoderNaN);
    let result = safety.clear_fault();
    assert!(result.is_err(), "clearing fault immediately must fail");
    assert!(
        matches!(safety.state(), SafetyState::Faulted { .. }),
        "state must remain Faulted after early clear attempt"
    );

    Ok(())
}

/// Full recovery scenario: fault → wait → clear → pipeline processes → device
/// receives torque again.
#[test]
fn recovery_full_scenario_fault_clear_resume() -> Result<()> {
    let id: DeviceId = "recovery-full-001".parse()?;
    let mut device = VirtualDevice::new(id, "Recovery Wheel".to_string());
    let mut pipeline = Pipeline::new();
    let mut safety = SafetyService::new(5.0, 20.0);

    // Normal operation
    let mut frame = Frame {
        ffb_in: 0.6,
        torque_out: 0.6,
        wheel_speed: 1.0,
        hands_off: false,
        ts_mono_ns: 1_000_000,
        seq: 0,
    };
    pipeline.process(&mut frame)?;
    let torque = safety.clamp_torque_nm(frame.torque_out * 5.0);
    device.write_ffb_report(torque, 0)?;

    // Fault
    safety.report_fault(FaultType::Overcurrent);
    let faulted_torque = safety.clamp_torque_nm(frame.torque_out * 5.0);
    device.write_ffb_report(faulted_torque, 1)?;
    assert!(faulted_torque.abs() < 0.001, "faulted torque must be zero");

    // Wait and clear
    std::thread::sleep(Duration::from_millis(120));
    safety.clear_fault().map_err(|e| anyhow::anyhow!("{e}"))?;

    // Resume
    let mut frame2 = Frame {
        ffb_in: 0.4,
        torque_out: 0.4,
        wheel_speed: 0.5,
        hands_off: false,
        ts_mono_ns: 2_000_000,
        seq: 2,
    };
    pipeline.process(&mut frame2)?;
    let resumed_torque = safety.clamp_torque_nm(frame2.torque_out * 5.0);
    assert!(
        resumed_torque.abs() > 0.01,
        "resumed torque must be non-zero, got {resumed_torque}"
    );
    device.write_ffb_report(resumed_torque, 2)?;

    // Device telemetry still valid
    let telem = device
        .read_telemetry()
        .ok_or_else(|| anyhow::anyhow!("telemetry missing after recovery"))?;
    assert!(telem.temperature_c <= 150);

    Ok(())
}

/// Recovery after multiple fault types: each must be individually clearable.
#[test]
fn recovery_multiple_faults_clear_independently() -> Result<()> {
    let faults = [
        FaultType::UsbStall,
        FaultType::EncoderNaN,
        FaultType::ThermalLimit,
        FaultType::Overcurrent,
    ];

    for fault in &faults {
        let mut safety = SafetyService::new(5.0, 20.0);
        safety.report_fault(*fault);
        assert!(matches!(safety.state(), SafetyState::Faulted { .. }));

        std::thread::sleep(Duration::from_millis(120));
        safety.clear_fault().map_err(|e| anyhow::anyhow!("{e}"))?;
        assert_eq!(
            safety.state(),
            &SafetyState::SafeTorque,
            "{fault:?}: must recover to SafeTorque"
        );

        // Torque flows
        let torque = safety.clamp_torque_nm(2.0);
        assert!(
            (torque - 2.0).abs() < 0.01,
            "{fault:?}: torque must flow after recovery"
        );
    }

    Ok(())
}

/// Clearing fault when not faulted must return an error (no-op safety).
#[test]
fn recovery_clear_when_not_faulted_returns_error() -> Result<()> {
    let mut safety = SafetyService::new(5.0, 20.0);
    assert_eq!(safety.state(), &SafetyState::SafeTorque);

    let result = safety.clear_fault();
    assert!(
        result.is_err(),
        "clearing fault when not faulted must return error"
    );

    Ok(())
}

/// Cross-crate integration: filter pipeline output + safety recovery path.
/// After fault + recovery, the full filter → engine → safety → device path
/// must produce correct, non-zero output.
#[test]
fn recovery_cross_crate_filter_pipeline_after_fault() -> Result<()> {
    let mut safety = SafetyService::new(5.0, 20.0);

    // Fault and recover
    safety.report_fault(FaultType::PipelineFault);
    std::thread::sleep(Duration::from_millis(120));
    safety.clear_fault().map_err(|e| anyhow::anyhow!("{e}"))?;

    // Filter pipeline (openracing-filters)
    let mut filter_frame = FilterFrame {
        ffb_in: 0.5,
        torque_out: 0.5,
        wheel_speed: 1.0,
        hands_off: false,
        ts_mono_ns: 0,
        seq: 0,
    };
    damper_filter(&mut filter_frame, &DamperState::fixed(0.01));
    friction_filter(&mut filter_frame, &FrictionState::fixed(0.01));
    torque_cap_filter(&mut filter_frame, 1.0);

    // Engine pipeline
    let mut engine_frame = Frame {
        ffb_in: filter_frame.torque_out,
        torque_out: filter_frame.torque_out,
        wheel_speed: 1.0,
        hands_off: false,
        ts_mono_ns: 1_000_000,
        seq: 1,
    };
    let mut pipeline = Pipeline::new();
    pipeline.process(&mut engine_frame)?;

    // Safety clamp (recovered)
    let torque = safety.clamp_torque_nm(engine_frame.torque_out * 5.0);
    assert!(
        torque.abs() > 0.01,
        "cross-crate pipeline torque must be non-zero after recovery, got {torque}"
    );
    assert!(
        torque.abs() <= 5.0,
        "torque must not exceed safe limit, got {torque}"
    );

    // Device write
    let id: DeviceId = "recovery-xcrate-001".parse()?;
    let mut device = VirtualDevice::new(id, "Cross-Crate Recovery Wheel".to_string());
    device.write_ffb_report(torque, 1)?;

    Ok(())
}

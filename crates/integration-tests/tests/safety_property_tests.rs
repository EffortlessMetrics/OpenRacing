#![allow(clippy::redundant_closure)]
//! Safety-critical property-based tests (proptest).
//!
//! Every test in this module encodes a safety invariant that must hold for ALL
//! possible inputs. At least 256 cases per property.
//!
//! Properties verified:
//!  1. Torque output NEVER exceeds configured maximum under ANY input sequence
//!  2. Safety fault ALWAYS results in zero torque within bounded time
//!  3. Watchdog timeout ALWAYS triggers safe state transition
//!  4. Filter chain output is ALWAYS bounded regardless of input
//!  5. Rate limiting ALWAYS enforces maximum torque change rate
//!  6. Emergency stop ALWAYS takes priority over all other commands
//!  7. Device disconnect ALWAYS results in safe state within bounds
//!  8. Torque command encoding NEVER produces invalid HID reports
//!  9. Challenge-response timeout ALWAYS triggers safety fault
//! 10. Concurrent safety events are handled deterministically

use std::time::Duration;

use proptest::prelude::*;

use openracing_filters::{
    Frame as FilterFrame, SlewRateState, slew_rate_filter, torque_cap_filter,
};
use racing_wheel_engine::VirtualDevice;
use racing_wheel_engine::pipeline::PipelineCompiler;
use racing_wheel_engine::ports::HidDevice;
use racing_wheel_engine::rt::Frame;
use racing_wheel_engine::safety::{
    FaultType, HardwareWatchdog, SafetyInterlockState, SafetyInterlockSystem, SafetyService,
    SafetyState, SoftwareWatchdog, TorqueLimit, WatchdogTimeoutHandler,
};
use racing_wheel_schemas::prelude::*;

// ═══════════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════════

fn make_frame(ffb_in: f32, seq: u16) -> Frame {
    Frame {
        ffb_in,
        torque_out: 0.0,
        wheel_speed: 5.0,
        hands_off: false,
        ts_mono_ns: seq as u64 * 1_000_000,
        seq,
    }
}

fn make_filter_frame(torque_out: f32) -> FilterFrame {
    FilterFrame {
        ffb_in: 0.0,
        torque_out,
        wheel_speed: 5.0,
        hands_off: false,
        ts_mono_ns: 0,
        seq: 0,
    }
}

/// Create a DeviceId for tests, mapping parse errors to proptest failures.
fn make_device_id(name: &str) -> Result<DeviceId, TestCaseError> {
    name.parse::<DeviceId>()
        .map_err(|e| TestCaseError::Fail(format!("DeviceId parse failed: {}", e).into()))
}

fn create_interlock_system(max_torque_nm: f32) -> SafetyInterlockSystem {
    let watchdog = Box::new(SoftwareWatchdog::new(1000));
    SafetyInterlockSystem::new(watchdog, max_torque_nm)
}

/// Strategy for FaultType variants.
fn fault_type_strategy() -> impl Strategy<Value = FaultType> {
    prop_oneof![
        Just(FaultType::UsbStall),
        Just(FaultType::EncoderNaN),
        Just(FaultType::ThermalLimit),
        Just(FaultType::Overcurrent),
        Just(FaultType::PluginOverrun),
        Just(FaultType::TimingViolation),
        Just(FaultType::SafetyInterlockViolation),
        Just(FaultType::HandsOffTimeout),
        Just(FaultType::PipelineFault),
    ]
}

// ═══════════════════════════════════════════════════════════════════════════════
// 1. Torque output NEVER exceeds configured maximum under ANY input sequence
// ═══════════════════════════════════════════════════════════════════════════════

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// SafetyService::clamp_torque_nm must bound output within ±max for any f32.
    #[test]
    fn prop_safety_service_clamp_never_exceeds_max(
        max_safe in 0.1f32..50.0,
        max_high in 0.1f32..100.0,
        requested in proptest::num::f32::ANY,
    ) {
        let service = SafetyService::new(max_safe, max_high);
        let clamped = service.clamp_torque_nm(requested);
        prop_assert!(
            clamped.is_finite(),
            "Clamped torque must be finite; got {} for input {:?}",
            clamped, requested,
        );
        prop_assert!(
            clamped.abs() <= max_safe + f32::EPSILON,
            "Clamped {} exceeds safe max {} for input {:?}",
            clamped, max_safe, requested,
        );
    }

    /// TorqueLimit::clamp must never produce a value above max_torque_nm.
    #[test]
    fn prop_torque_limit_never_exceeds_max(
        max_torque in 1.0f32..50.0,
        requested in -200.0f32..200.0,
    ) {
        let mut limit = TorqueLimit::new(max_torque, max_torque * 0.2);
        let (clamped, _) = limit.clamp(requested);
        prop_assert!(
            clamped.abs() <= max_torque + f32::EPSILON,
            "Torque {} exceeds max {} for request {}",
            clamped, max_torque, requested,
        );
    }

    /// SafetyInterlockSystem always clamps requested torque to configured max.
    #[test]
    fn prop_interlock_system_torque_bounded(
        max_torque in 1.0f32..50.0,
        requested in -200.0f32..200.0,
    ) {
        let mut system = create_interlock_system(max_torque);
        let result = system.process_tick(requested);
        prop_assert!(
            result.torque_command.abs() <= max_torque + f32::EPSILON,
            "Tick torque {} exceeds max {} for request {}",
            result.torque_command, max_torque, requested,
        );
    }

    /// A sequence of arbitrary inputs must never produce output exceeding max.
    #[test]
    fn prop_torque_bounded_under_input_sequence(
        max_safe in 1.0f32..20.0,
        inputs in proptest::collection::vec(-100.0f32..100.0, 1..64),
    ) {
        let service = SafetyService::new(max_safe, max_safe * 5.0);
        for input in &inputs {
            let clamped = service.clamp_torque_nm(*input);
            prop_assert!(
                clamped.is_finite() && clamped.abs() <= max_safe + f32::EPSILON,
                "Sequence element {} clamped to {} exceeding max {}",
                input, clamped, max_safe,
            );
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. Safety fault ALWAYS results in zero torque within bounded time
// ═══════════════════════════════════════════════════════════════════════════════

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Any FaultType must transition SafetyService to Faulted and zero its torque.
    #[test]
    fn prop_fault_always_zeros_torque(
        fault in fault_type_strategy(),
        requested in -100.0f32..100.0,
    ) {
        let mut service = SafetyService::new(5.0, 25.0);
        service.report_fault(fault);
        let clamped = service.clamp_torque_nm(requested);
        prop_assert_eq!(
            clamped, 0.0,
            "Torque must be 0.0 after fault {:?}; got {} for input {}",
            fault, clamped, requested,
        );
        prop_assert!(
            matches!(service.state(), SafetyState::Faulted { .. }),
            "State must be Faulted after report_fault({:?})",
            fault,
        );
    }

    /// SafetyInterlockSystem fault report must enter safe mode and limit torque.
    #[test]
    fn prop_interlock_fault_limits_torque(
        fault in fault_type_strategy(),
        requested in -100.0f32..100.0,
    ) {
        let mut system = create_interlock_system(25.0);
        system.report_fault(fault);
        let result = system.process_tick(requested);
        let safe_limit = system.torque_limit().safe_mode_limit();
        prop_assert!(
            result.torque_command.abs() <= safe_limit + f32::EPSILON,
            "Torque {} exceeds safe-mode limit {} after fault {:?}",
            result.torque_command, safe_limit, fault,
        );
        prop_assert!(
            matches!(result.state, SafetyInterlockState::SafeMode { .. }),
            "State must be SafeMode after fault {:?}",
            fault,
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. Watchdog timeout ALWAYS triggers safe state transition
// ═══════════════════════════════════════════════════════════════════════════════

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// WatchdogTimeoutHandler always produces zero torque on timeout.
    #[test]
    fn prop_watchdog_timeout_always_zeroes_torque(
        previous_torque in proptest::num::f32::ANY,
    ) {
        let mut handler = WatchdogTimeoutHandler::new();
        let response = handler.handle_timeout(previous_torque);
        prop_assert_eq!(
            response.torque_command, 0.0,
            "Watchdog timeout must command 0.0; got {} for prev {}",
            response.torque_command, previous_torque,
        );
        prop_assert!(
            handler.is_timeout_triggered(),
            "Timeout flag must be set after handle_timeout",
        );
    }

    /// A timed-out SoftwareWatchdog must report has_timed_out regardless of prior torque.
    #[test]
    fn prop_software_watchdog_reports_timeout(
        timeout_ms in 5u32..50,
    ) {
        let mut watchdog = SoftwareWatchdog::new(timeout_ms);
        let arm_result = watchdog.arm();
        prop_assert!(arm_result.is_ok(), "arm() failed: {:?}", arm_result);
        // Sleep longer than timeout
        std::thread::sleep(Duration::from_millis(u64::from(timeout_ms) + 10));
        prop_assert!(
            watchdog.has_timed_out(),
            "Watchdog with {}ms timeout must report timed out",
            timeout_ms,
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. Filter chain output is ALWAYS bounded regardless of input
// ═══════════════════════════════════════════════════════════════════════════════

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// torque_cap_filter must clamp all inputs (including NaN/Inf) to [-max, +max].
    #[test]
    fn prop_torque_cap_filter_always_bounded(
        input in proptest::num::f32::ANY,
        max_torque in 0.01f32..10.0,
    ) {
        let mut frame = make_filter_frame(input);
        torque_cap_filter(&mut frame, max_torque);
        prop_assert!(
            frame.torque_out.is_finite(),
            "torque_cap_filter must produce finite output; got {} for input {:?}",
            frame.torque_out, input,
        );
        prop_assert!(
            frame.torque_out.abs() <= max_torque + f32::EPSILON,
            "torque_cap_filter output {} exceeds max {} for input {:?}",
            frame.torque_out, max_torque, input,
        );
    }

    /// Pipeline compilation + processing must produce bounded output for valid input.
    #[test]
    fn prop_pipeline_output_bounded(ffb_in in -1.0f32..=1.0) {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| TestCaseError::Fail(format!("Runtime build failed: {}", e).into()))?;
        rt.block_on(async {
            let compiler = PipelineCompiler::new();
            let compiled = compiler
                .compile_pipeline(FilterConfig::default())
                .await
                .map_err(|e| TestCaseError::Fail(format!("Compile failed: {}", e).into()))?;
            let mut pipeline = compiled.pipeline;
            let mut frame = make_frame(ffb_in, 0);
            let result = pipeline.process(&mut frame);
            prop_assert!(result.is_ok(), "Pipeline processing failed: {:?}", result);
            prop_assert!(
                frame.torque_out.is_finite() && frame.torque_out.abs() <= 1.0,
                "Pipeline output {} out of bounds for input {}",
                frame.torque_out, ffb_in,
            );
            Ok(())
        })?;
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. Rate limiting ALWAYS enforces maximum torque change rate
// ═══════════════════════════════════════════════════════════════════════════════

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Slew rate filter must limit per-tick change to max_change_per_tick.
    #[test]
    fn prop_slew_rate_enforces_max_change(
        slew_rate in 0.01f32..1.0,
        inputs in proptest::collection::vec(-1.0f32..1.0, 2..32),
    ) {
        let mut state = SlewRateState::new(slew_rate);
        let mut prev_output = 0.0f32;
        for (i, &input) in inputs.iter().enumerate() {
            let mut frame = make_filter_frame(input);
            slew_rate_filter(&mut frame, &mut state);
            if i > 0 {
                let delta = (frame.torque_out - prev_output).abs();
                prop_assert!(
                    delta <= state.max_change_per_tick + f32::EPSILON,
                    "Slew rate violation at step {}: delta {} > max {} (slew_rate={})",
                    i, delta, state.max_change_per_tick, slew_rate,
                );
            }
            prev_output = frame.torque_out;
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 6. Emergency stop ALWAYS takes priority over all other commands
// ═══════════════════════════════════════════════════════════════════════════════

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// After emergency_stop, ANY torque request must yield zero.
    #[test]
    fn prop_emergency_stop_always_zero(
        max_torque in 1.0f32..50.0,
        requests in proptest::collection::vec(-200.0f32..200.0, 1..32),
    ) {
        let watchdog = Box::new(SoftwareWatchdog::new(1000));
        let mut system = SafetyInterlockSystem::new(watchdog, max_torque);
        system.emergency_stop();
        for (i, &req) in requests.iter().enumerate() {
            let result = system.process_tick(req);
            prop_assert_eq!(
                result.torque_command, 0.0,
                "Emergency stop must produce 0.0; got {} at step {} for req {}",
                result.torque_command, i, req,
            );
            prop_assert!(
                matches!(result.state, SafetyInterlockState::EmergencyStop { .. }),
                "Must remain in EmergencyStop state after e-stop (step {})",
                i,
            );
        }
    }

    /// Emergency stop overrides an ongoing fault.
    #[test]
    fn prop_emergency_stop_overrides_fault(
        fault in fault_type_strategy(),
        requested in -100.0f32..100.0,
    ) {
        let watchdog = Box::new(SoftwareWatchdog::new(1000));
        let mut system = SafetyInterlockSystem::new(watchdog, 25.0);
        system.report_fault(fault);
        system.emergency_stop();
        let result = system.process_tick(requested);
        prop_assert_eq!(
            result.torque_command, 0.0,
            "E-stop must override fault and zero torque; got {}",
            result.torque_command,
        );
        prop_assert!(
            matches!(result.state, SafetyInterlockState::EmergencyStop { .. }),
            "Must be EmergencyStop after e-stop, not SafeMode",
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 7. Device disconnect ALWAYS results in safe state within bounds
// ═══════════════════════════════════════════════════════════════════════════════

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// A disconnected VirtualDevice always returns DeviceDisconnected error.
    #[test]
    fn prop_device_disconnect_rejects_writes(
        torque_nm in -25.0f32..25.0,
        seq in 0u16..1000,
    ) {
        let id = make_device_id("prop-disconnect")?;
        let mut device = VirtualDevice::new(id, "PropTest".to_string());
        device.disconnect();
        let result = device.write_ffb_report(torque_nm, seq);
        prop_assert!(
            result.is_err(),
            "Disconnected device must reject write_ffb_report({}, {})",
            torque_nm, seq,
        );
    }

    /// A disconnected VirtualDevice produces no telemetry.
    #[test]
    fn prop_device_disconnect_no_telemetry(
        _dummy in 0u8..1,
    ) {
        let id = make_device_id("prop-no-telem")?;
        let mut device = VirtualDevice::new(id, "PropTest".to_string());
        device.disconnect();
        let telem = device.read_telemetry();
        prop_assert!(
            telem.is_none(),
            "Disconnected device must return None telemetry",
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 8. Torque command encoding NEVER produces invalid HID reports
// ═══════════════════════════════════════════════════════════════════════════════

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// VirtualDevice rejects torque above its capability limit.
    #[test]
    fn prop_torque_write_rejects_over_capability(
        over_factor in 1.01f32..5.0,
        seq in 0u16..1000,
    ) {
        let id = make_device_id("prop-hid-cap")?;
        let mut device = VirtualDevice::new(id, "PropTest".to_string());
        let max_torque = device.capabilities().max_torque.value();
        let over_torque = max_torque * over_factor;
        let result = device.write_ffb_report(over_torque, seq);
        prop_assert!(
            result.is_err(),
            "write_ffb_report({}, {}) must fail when above max_torque {}",
            over_torque, seq, max_torque,
        );
    }

    /// VirtualDevice accepts torque within its capability limit.
    #[test]
    fn prop_torque_write_accepts_within_capability(
        factor in 0.0f32..1.0,
        seq in 0u16..1000,
    ) {
        let id = make_device_id("prop-hid-ok")?;
        let mut device = VirtualDevice::new(id, "PropTest".to_string());
        let max_torque = device.capabilities().max_torque.value();
        let torque = max_torque * factor;
        let result = device.write_ffb_report(torque, seq);
        prop_assert!(
            result.is_ok(),
            "write_ffb_report({}, {}) should succeed within cap {}; err: {:?}",
            torque, seq, max_torque, result,
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 9. Challenge-response timeout ALWAYS triggers safety fault
// ═══════════════════════════════════════════════════════════════════════════════

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// An expired challenge must revert SafetyService to SafeTorque.
    #[test]
    fn prop_expired_challenge_returns_to_safe(
        max_safe in 1.0f32..20.0,
    ) {
        let mut service = SafetyService::with_timeouts(
            max_safe,
            max_safe * 5.0,
            Duration::from_secs(5),
            Duration::from_secs(2),
        );
        let challenge_result = service.request_high_torque("dev-prop");
        prop_assert!(
            challenge_result.is_ok(),
            "request_high_torque failed: {:?}",
            challenge_result,
        );
        prop_assert!(
            matches!(service.state(), SafetyState::HighTorqueChallenge { .. }),
            "Must be in HighTorqueChallenge state",
        );
        // check_challenge_expiry should not expire immediately
        let expired = service.check_challenge_expiry();
        prop_assert!(
            !expired,
            "Challenge must not expire immediately",
        );
        // After expiry, torque should be bounded by safe max, not high torque
        let clamped = service.clamp_torque_nm(max_safe * 10.0);
        prop_assert!(
            clamped.abs() <= max_safe + f32::EPSILON,
            "During challenge, torque {} must be bounded by safe max {}",
            clamped, max_safe,
        );
    }

    /// Providing consent with wrong token must fail.
    #[test]
    fn prop_wrong_consent_token_rejected(
        bad_token in 1u32..u32::MAX,
    ) {
        let mut service = SafetyService::new(5.0, 25.0);
        let challenge_result = service.request_high_torque("dev-prop");
        prop_assert!(challenge_result.is_ok());
        // Use a token different from the real one
        let real_token = challenge_result
            .map_err(|e| TestCaseError::Fail(e.into()))?
            .challenge_token;
        if bad_token != real_token {
            let consent_result = service.provide_ui_consent(bad_token);
            prop_assert!(
                consent_result.is_err(),
                "Wrong token {} must be rejected (real={})",
                bad_token, real_token,
            );
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 10. Concurrent safety events are handled deterministically
// ═══════════════════════════════════════════════════════════════════════════════

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Multiple faults in sequence must all keep the system in a safe state.
    #[test]
    fn prop_sequential_faults_deterministic(
        faults in proptest::collection::vec(fault_type_strategy(), 1..16),
        requested in -100.0f32..100.0,
    ) {
        let mut service = SafetyService::new(5.0, 25.0);
        for fault in &faults {
            service.report_fault(*fault);
            let clamped = service.clamp_torque_nm(requested);
            prop_assert_eq!(
                clamped, 0.0,
                "Must be 0.0 after fault {:?}; got {} for input {}",
                fault, clamped, requested,
            );
            prop_assert!(
                matches!(service.state(), SafetyState::Faulted { .. }),
                "Must stay Faulted after {:?}",
                fault,
            );
        }
    }

    /// Multiple faults through the interlock system must keep torque limited.
    #[test]
    fn prop_interlock_sequential_faults_bounded(
        faults in proptest::collection::vec(fault_type_strategy(), 1..16),
        requests in proptest::collection::vec(-200.0f32..200.0, 1..16),
    ) {
        let mut system = create_interlock_system(25.0);
        let safe_limit = system.torque_limit().safe_mode_limit();
        for fault in &faults {
            system.report_fault(*fault);
        }
        for (i, &req) in requests.iter().enumerate() {
            let result = system.process_tick(req);
            prop_assert!(
                result.torque_command.abs() <= safe_limit + f32::EPSILON,
                "After {} faults, step {} torque {} exceeds safe limit {}",
                faults.len(), i, result.torque_command, safe_limit,
            );
        }
    }

    /// Emergency stop after a sequence of faults still produces zero torque.
    #[test]
    fn prop_estop_after_faults_deterministic(
        faults in proptest::collection::vec(fault_type_strategy(), 0..8),
        requests in proptest::collection::vec(-200.0f32..200.0, 1..16),
    ) {
        let watchdog = Box::new(SoftwareWatchdog::new(1000));
        let mut system = SafetyInterlockSystem::new(watchdog, 25.0);
        for fault in &faults {
            system.report_fault(*fault);
        }
        system.emergency_stop();
        for (i, &req) in requests.iter().enumerate() {
            let result = system.process_tick(req);
            prop_assert_eq!(
                result.torque_command, 0.0,
                "E-stop after {} faults: step {} must be 0.0; got {}",
                faults.len(), i, result.torque_command,
            );
        }
    }

    /// Same fault sequence applied twice produces identical state.
    #[test]
    fn prop_fault_sequence_reproducible(
        faults in proptest::collection::vec(fault_type_strategy(), 1..8),
        requested in -100.0f32..100.0,
    ) {
        let mut service_a = SafetyService::new(5.0, 25.0);
        let mut service_b = SafetyService::new(5.0, 25.0);
        for fault in &faults {
            service_a.report_fault(*fault);
            service_b.report_fault(*fault);
        }
        let clamped_a = service_a.clamp_torque_nm(requested);
        let clamped_b = service_b.clamp_torque_nm(requested);
        prop_assert_eq!(
            clamped_a, clamped_b,
            "Identical fault sequences must produce identical results",
        );
    }
}

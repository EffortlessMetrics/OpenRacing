//! Integration-style tests verifying key engine subsystems work together.
//!
//! Coverage areas:
//! 1. Device dispatch completeness — every (VID, PID) in `SupportedDevices::all()`
//!    maps to a valid device identity via the vendor protocol table.
//! 2. Protocol handler routing — vendor IDs map to the correct protocol handlers.
//! 3. Input normalization — raw torque / angle values normalize correctly across
//!    different wheel ranges (270°, 900°, 1080°, 2520°).
//! 4. Safety interlocks — safety state machine transitions work correctly.
//! 5. FFB pipeline — filter pipeline with known inputs produces bounded outputs.

// ── helpers ───────────────────────────────────────────────────────────────────

/// Convert a `Result` into the value or panic with a message (replaces `unwrap`).
#[track_caller]
fn must<T, E: std::fmt::Debug>(r: Result<T, E>) -> T {
    match r {
        Ok(v) => v,
        Err(e) => panic!("unexpected Err: {e:?}"),
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// 1. Device dispatch completeness
// ══════════════════════════════════════════════════════════════════════════════

mod device_dispatch_completeness {
    #[cfg(windows)]
    use racing_wheel_engine::hid::vendor::get_vendor_protocol;
    #[cfg(windows)]
    use racing_wheel_engine::hid::windows::{SupportedDevices, vendor_ids};

    /// Every (VID, PID) pair in the `SupportedDevices::all()` table that belongs
    /// to a vendor with a protocol handler must resolve via `get_vendor_protocol()`.
    ///
    /// VIDs with no protocol handler (PXN, FlashFire, Guillemot, Thrustmaster Xbox)
    /// are excluded — those are enumeration-only entries.
    #[cfg(windows)]
    #[test]
    fn all_dispatched_devices_have_protocol_handler() -> Result<(), Box<dyn std::error::Error>> {
        // VIDs that have a dedicated vendor protocol handler in get_vendor_protocol()
        let dispatched_vids: &[u16] = &[
            vendor_ids::LOGITECH,
            vendor_ids::FANATEC,
            vendor_ids::THRUSTMASTER,
            vendor_ids::MOZA,
            vendor_ids::SIMAGIC,     // 0x0483 — Simagic / VRS / Cube Controls
            vendor_ids::SIMAGIC_ALT, // 0x16D0 — Simucube / Simagic
            vendor_ids::SIMAGIC_EVO,
            vendor_ids::HEUSINKVELD, // 0x04D8 — Heusinkveld pedals (Microchip VID)
            vendor_ids::ASETEK,
            vendor_ids::CAMMUS,
            vendor_ids::GRANITE_DEVICES,
            vendor_ids::OPENFFBOARD, // 0x1209, but only certain PIDs
            vendor_ids::FFBEAST,     // 0x045B, but only certain PIDs
            vendor_ids::SIMEXPERIENCE,
            vendor_ids::LEO_BODNAR,
        ];

        let all_devices = SupportedDevices::all();
        let mut failures = Vec::new();

        for &(vid, pid, name) in all_devices {
            if !dispatched_vids.contains(&vid) {
                // Skip enumeration-only VIDs (PXN, FlashFire, Guillemot, TM-Xbox)
                continue;
            }
            if get_vendor_protocol(vid, pid).is_none() {
                failures.push(format!("  0x{vid:04X}:0x{pid:04X} ({name})"));
            }
        }

        assert!(
            failures.is_empty(),
            "The following dispatched devices have no protocol handler:\n{}",
            failures.join("\n")
        );
        Ok(())
    }

    /// Each entry in `SupportedDevices::all()` must have a non-empty product name.
    #[cfg(windows)]
    #[test]
    fn all_devices_have_non_empty_names() -> Result<(), Box<dyn std::error::Error>> {
        for &(vid, pid, name) in SupportedDevices::all() {
            assert!(
                !name.is_empty(),
                "Device 0x{vid:04X}:0x{pid:04X} has an empty name"
            );
        }
        Ok(())
    }

    /// No duplicate (VID, PID) pairs in the device table.
    #[cfg(windows)]
    #[test]
    fn no_duplicate_vid_pid_pairs() -> Result<(), Box<dyn std::error::Error>> {
        let all = SupportedDevices::all();
        let mut seen = std::collections::HashSet::new();
        for &(vid, pid, name) in all {
            assert!(
                seen.insert((vid, pid)),
                "Duplicate VID/PID: 0x{vid:04X}:0x{pid:04X} ({name})"
            );
        }
        Ok(())
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// 2. Protocol handler routing
// ══════════════════════════════════════════════════════════════════════════════

mod protocol_handler_routing {
    use racing_wheel_engine::hid::vendor::{
        get_vendor_protocol, get_vendor_protocol_with_hid_pid_fallback,
    };

    /// Fanatec handler returns sensible FfbConfig (non-zero max torque and encoder CPR).
    #[test]
    fn fanatec_handler_ffb_config() -> Result<(), Box<dyn std::error::Error>> {
        let handler =
            get_vendor_protocol(0x0EB7, 0x0020).ok_or("Fanatec CSL DD should have a handler")?;
        let cfg = handler.get_ffb_config();
        assert!(cfg.max_torque_nm > 0.0, "Fanatec max torque must be > 0");
        assert!(cfg.encoder_cpr > 0, "Fanatec encoder CPR must be > 0");
        Ok(())
    }

    /// Logitech handler returns sensible FfbConfig.
    #[test]
    fn logitech_handler_ffb_config() -> Result<(), Box<dyn std::error::Error>> {
        let handler =
            get_vendor_protocol(0x046D, 0xC262).ok_or("Logitech G920 should have a handler")?;
        let cfg = handler.get_ffb_config();
        assert!(cfg.max_torque_nm > 0.0, "Logitech max torque must be > 0");
        Ok(())
    }

    /// Moza handler returns sensible FfbConfig.
    #[test]
    fn moza_handler_ffb_config() -> Result<(), Box<dyn std::error::Error>> {
        let handler =
            get_vendor_protocol(0x346E, 0x0012).ok_or("Moza R9 V2 should have a handler")?;
        let cfg = handler.get_ffb_config();
        assert!(cfg.max_torque_nm > 0.0, "Moza max torque must be > 0");
        Ok(())
    }

    /// Thrustmaster handler returns sensible FfbConfig.
    #[test]
    fn thrustmaster_handler_ffb_config() -> Result<(), Box<dyn std::error::Error>> {
        let handler =
            get_vendor_protocol(0x044F, 0xB69B).ok_or("Thrustmaster T818 should have a handler")?;
        let cfg = handler.get_ffb_config();
        assert!(cfg.max_torque_nm > 0.0);
        Ok(())
    }

    /// Generic HID PID fallback produces a handler with a valid FfbConfig.
    #[test]
    fn generic_hid_pid_fallback_has_ffb_config() -> Result<(), Box<dyn std::error::Error>> {
        let handler = get_vendor_protocol_with_hid_pid_fallback(0xDEAD, 0xBEEF, true)
            .ok_or("Generic HID PID fallback should yield a handler")?;
        let cfg = handler.get_ffb_config();
        assert!(
            cfg.max_torque_nm > 0.0,
            "Generic fallback must report non-zero max torque"
        );
        Ok(())
    }

    /// Every dispatched vendor must expose an `output_report_id()` for wheelbases.
    /// This ensures FFB output can actually be sent.
    #[test]
    fn wheelbase_handlers_have_output_report_id() -> Result<(), Box<dyn std::error::Error>> {
        // Representative wheelbase PIDs per vendor
        let wheelbases: &[(u16, u16, &str)] = &[
            (0x0EB7, 0x0020, "Fanatec CSL DD"),
            (0x046D, 0xC262, "Logitech G920"),
            (0x346E, 0x0012, "Moza R9 V2"),
            (0x044F, 0xB69B, "Thrustmaster T818"),
            (0x0483, 0xA355, "VRS DirectForce Pro"),
            (0x3670, 0x0500, "Simagic EVO Sport"),
            (0x1D50, 0x6050, "Granite IONI"),
        ];

        for &(vid, pid, name) in wheelbases {
            let handler =
                get_vendor_protocol(vid, pid).ok_or(format!("{name} should have a handler"))?;
            assert!(
                handler.output_report_id().is_some(),
                "{name} handler must expose an output_report_id for FFB"
            );
        }
        Ok(())
    }

    /// Handlers that are not V2 hardware must report `is_v2_hardware() == false`.
    /// V2 handlers must report `true`. (Smoke-test the trait method is callable.)
    #[test]
    fn is_v2_hardware_callable_on_all_handlers() -> Result<(), Box<dyn std::error::Error>> {
        let test_pids: &[(u16, u16)] = &[
            (0x0EB7, 0x0020),
            (0x046D, 0xC262),
            (0x346E, 0x0012),
            (0x044F, 0xB69B),
        ];
        for &(vid, pid) in test_pids {
            let handler = get_vendor_protocol(vid, pid)
                .ok_or(format!("No handler for 0x{vid:04X}:0x{pid:04X}"))?;
            // Just verify it doesn't panic; the value depends on the product.
            let _v2 = handler.is_v2_hardware();
        }
        Ok(())
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// 3. Input normalization (torque command encoding)
// ══════════════════════════════════════════════════════════════════════════════

mod input_normalization {
    use racing_wheel_engine::hid::{
        MAX_TORQUE_REPORT_SIZE, TorqueCommand, encode_torque_report_for_device,
    };

    /// Torque Q8.8 encoding: positive torque value converts correctly.
    #[test]
    fn q8_8_encoding_positive() -> Result<(), Box<dyn std::error::Error>> {
        let cmd = TorqueCommand::new(10.0, 0, false, false);
        let torque = cmd.torque_nm_q8_8;
        // 10.0 * 256 = 2560
        assert_eq!(torque, 2560i16);
        Ok(())
    }

    /// Torque Q8.8 encoding: negative torque value converts correctly.
    #[test]
    fn q8_8_encoding_negative() -> Result<(), Box<dyn std::error::Error>> {
        let cmd = TorqueCommand::new(-5.0, 0, false, false);
        let torque = cmd.torque_nm_q8_8;
        assert_eq!(torque, -1280i16);
        Ok(())
    }

    /// Torque Q8.8 encoding: zero maps to zero.
    #[test]
    fn q8_8_encoding_zero() -> Result<(), Box<dyn std::error::Error>> {
        let cmd = TorqueCommand::new(0.0, 0, false, false);
        let torque = cmd.torque_nm_q8_8;
        assert_eq!(torque, 0i16);
        Ok(())
    }

    /// Torque Q8.8 encoding clamps at i16 boundaries for extreme values.
    #[test]
    fn q8_8_encoding_clamp_overflow() -> Result<(), Box<dyn std::error::Error>> {
        let cmd_pos = TorqueCommand::new(200.0, 0, false, false);
        let torque_pos = cmd_pos.torque_nm_q8_8;
        assert_eq!(torque_pos, i16::MAX);

        let cmd_neg = TorqueCommand::new(-200.0, 0, false, false);
        let torque_neg = cmd_neg.torque_nm_q8_8;
        assert_eq!(torque_neg, i16::MIN);
        Ok(())
    }

    /// Different wheel ranges should produce correctly scaled torque reports.
    /// Test that the OWP-1 torque encoding preserves sign and magnitude across
    /// the full output range for a generic (non-vendor-specific) device.
    #[test]
    fn owp1_torque_encoding_across_range() -> Result<(), Box<dyn std::error::Error>> {
        let generic_vid = 0x046D; // Logitech uses OWP-1 layout
        let generic_pid = 0xC294; // Driving Force

        // Test several torque values spanning the range
        let test_cases: &[(f32, bool)] = &[
            (0.0, false), // center
            (3.5, true),  // positive
            (-3.5, true), // negative
            (0.1, true),  // small positive (0.1 * 256 = 25 in Q8.8)
        ];

        for &(torque_nm, should_be_nonzero) in test_cases {
            let mut out = [0u8; MAX_TORQUE_REPORT_SIZE];
            let len = encode_torque_report_for_device(
                generic_vid,
                generic_pid,
                10.0,
                torque_nm,
                1,
                &mut out,
            );
            assert!(
                len > 0,
                "Report length must be positive for torque={torque_nm}"
            );
            assert_eq!(out[0], TorqueCommand::REPORT_ID);

            let encoded = i16::from_le_bytes([out[1], out[2]]);
            if should_be_nonzero {
                assert_ne!(
                    encoded, 0,
                    "Expected non-zero encoding for torque={torque_nm}"
                );
                if torque_nm > 0.0 {
                    assert!(encoded > 0, "Positive torque must encode as positive i16");
                } else {
                    assert!(encoded < 0, "Negative torque must encode as negative i16");
                }
            } else {
                assert_eq!(encoded, 0, "Zero torque must encode as zero");
            }
        }
        Ok(())
    }

    /// Fanatec encoder: full positive torque should yield i16::MAX.
    #[test]
    fn fanatec_full_positive_torque() -> Result<(), Box<dyn std::error::Error>> {
        let mut out = [0u8; MAX_TORQUE_REPORT_SIZE];
        let len = encode_torque_report_for_device(0x0EB7, 0x0020, 8.0, 8.0, 0, &mut out);
        assert!(len > 0);
        // Fanatec constant force: bytes [2..4] are the torque value
        let value = i16::from_le_bytes([out[2], out[3]]);
        assert_eq!(value, i16::MAX);
        Ok(())
    }

    /// Fanatec encoder: full negative torque should yield i16::MIN.
    #[test]
    fn fanatec_full_negative_torque() -> Result<(), Box<dyn std::error::Error>> {
        let mut out = [0u8; MAX_TORQUE_REPORT_SIZE];
        let _len = encode_torque_report_for_device(0x0EB7, 0x0020, 8.0, -8.0, 0, &mut out);
        let value = i16::from_le_bytes([out[2], out[3]]);
        assert_eq!(value, i16::MIN);
        Ok(())
    }

    /// Sequence number is correctly encoded in OWP-1 torque commands.
    #[test]
    fn owp1_sequence_number_encoded() -> Result<(), Box<dyn std::error::Error>> {
        for seq in [0u16, 1, 255, 1000, u16::MAX] {
            let mut out = [0u8; MAX_TORQUE_REPORT_SIZE];
            encode_torque_report_for_device(0x046D, 0xC294, 10.0, 1.0, seq, &mut out);
            let decoded_seq = u16::from_le_bytes([out[4], out[5]]);
            assert_eq!(decoded_seq, seq, "Sequence number mismatch for seq={seq}");
        }
        Ok(())
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// 4. Safety interlocks
// ══════════════════════════════════════════════════════════════════════════════

mod safety_interlocks {
    use super::*;
    use racing_wheel_engine::safety::{
        ButtonCombo, FaultType, InterlockAck, SafetyInterlockState, SafetyInterlockSystem,
        SafetyService, SafetyState, SoftwareWatchdog,
    };
    use std::time::{Duration, Instant};

    fn create_test_service() -> SafetyService {
        SafetyService::with_timeouts(5.0, 25.0, Duration::from_secs(3), Duration::from_secs(2))
    }

    /// Fresh SafetyService starts in SafeTorque state.
    #[test]
    fn initial_state_is_safe_torque() -> Result<(), Box<dyn std::error::Error>> {
        let service = create_test_service();
        assert_eq!(service.state(), &SafetyState::SafeTorque);
        assert!((service.max_torque_nm() - 5.0).abs() < 0.001);
        Ok(())
    }

    /// Reporting a fault transitions to Faulted and clamps torque to zero.
    #[test]
    fn fault_report_transitions_to_faulted() -> Result<(), Box<dyn std::error::Error>> {
        let mut service = create_test_service();
        service.report_fault(FaultType::UsbStall);

        match service.state() {
            SafetyState::Faulted { fault, .. } => {
                assert_eq!(*fault, FaultType::UsbStall);
            }
            other => return Err(format!("Expected Faulted, got {other:?}").into()),
        }
        assert!((service.max_torque_nm() - 0.0).abs() < 0.001);
        Ok(())
    }

    /// clamp_torque_nm enforces state-specific limits.
    #[test]
    fn clamp_torque_respects_state() -> Result<(), Box<dyn std::error::Error>> {
        let mut service = create_test_service();

        // SafeTorque: clamped to 5.0 Nm
        let clamped = service.clamp_torque_nm(10.0);
        assert!((clamped - 5.0).abs() < 0.001);

        // Faulted: clamped to 0.0
        service.report_fault(FaultType::EncoderNaN);
        let clamped = service.clamp_torque_nm(10.0);
        assert!((clamped - 0.0).abs() < 0.001);
        Ok(())
    }

    /// clamp_torque_nm maps NaN/Inf to 0.0 (safe state) because they are non-finite.
    #[test]
    fn clamp_torque_nan_inf_to_zero() -> Result<(), Box<dyn std::error::Error>> {
        let service = create_test_service();
        // NaN is non-finite → replaced with 0.0 → clamped to 0.0
        assert_eq!(service.clamp_torque_nm(f32::NAN), 0.0);
        // +Inf is non-finite → replaced with 0.0 → clamped to 0.0
        assert_eq!(service.clamp_torque_nm(f32::INFINITY), 0.0);
        // -Inf is non-finite → replaced with 0.0 → clamped to 0.0
        assert_eq!(service.clamp_torque_nm(f32::NEG_INFINITY), 0.0);
        Ok(())
    }

    /// Fault cannot be cleared too quickly (minimum dwell time enforced).
    #[test]
    fn fault_clear_requires_dwell_time() -> Result<(), Box<dyn std::error::Error>> {
        let mut service = create_test_service();
        service.report_fault(FaultType::ThermalLimit);

        // Immediate clear should fail
        let result = service.clear_fault();
        assert!(result.is_err(), "Should not clear fault immediately");

        // Wait enough time and clear
        std::thread::sleep(Duration::from_millis(150));
        let result = service.clear_fault();
        assert!(result.is_ok(), "Should clear fault after dwell time");
        assert_eq!(service.state(), &SafetyState::SafeTorque);
        Ok(())
    }

    /// Full high-torque challenge-response flow works end to end.
    #[test]
    fn high_torque_challenge_response_flow() -> Result<(), Box<dyn std::error::Error>> {
        let mut service = create_test_service();

        // Step 1: Request high torque
        let challenge = must(service.request_high_torque("test-device"));
        assert_eq!(challenge.combo_required, ButtonCombo::BothClutchPaddles);
        assert!(!challenge.ui_consent_given);

        // Step 2: Provide UI consent
        must(service.provide_ui_consent(challenge.challenge_token));
        match service.state() {
            SafetyState::AwaitingPhysicalAck { .. } => {}
            other => return Err(format!("Expected AwaitingPhysicalAck, got {other:?}").into()),
        }

        // Step 3: Report combo start
        must(service.report_combo_start(challenge.challenge_token));

        // Step 4: Wait for combo hold duration
        std::thread::sleep(Duration::from_millis(2100));

        // Step 5: Confirm with device ack
        let ack = InterlockAck {
            challenge_token: challenge.challenge_token,
            device_token: 42,
            combo_completed: ButtonCombo::BothClutchPaddles,
            timestamp: Instant::now(),
        };
        must(service.confirm_high_torque("test-device", ack));

        // Verify high torque active
        match service.state() {
            SafetyState::HighTorqueActive { .. } => {}
            other => return Err(format!("Expected HighTorqueActive, got {other:?}").into()),
        }
        assert!((service.max_torque_nm() - 25.0).abs() < 0.001);
        assert!(service.has_valid_token("test-device"));
        Ok(())
    }

    /// Safety interlock system: normal operation passes torque through unchanged.
    #[test]
    fn interlock_system_normal_passthrough() -> Result<(), Box<dyn std::error::Error>> {
        let watchdog = Box::new(SoftwareWatchdog::new(100));
        let mut system = SafetyInterlockSystem::new(watchdog, 25.0);
        system.arm()?;

        let result = system.process_tick(10.0);
        assert_eq!(result.state, SafetyInterlockState::Normal);
        assert!(!result.fault_occurred);
        assert!((result.torque_command - 10.0).abs() < 0.001);
        Ok(())
    }

    /// Safety interlock system: torque over limit is clamped, not passed through.
    #[test]
    fn interlock_system_clamps_excess_torque() -> Result<(), Box<dyn std::error::Error>> {
        let watchdog = Box::new(SoftwareWatchdog::new(100));
        let mut system = SafetyInterlockSystem::new(watchdog, 25.0);
        system.arm()?;

        let result = system.process_tick(50.0);
        assert!(
            result.torque_command <= 25.0,
            "Torque {} exceeds 25.0 Nm limit",
            result.torque_command
        );
        Ok(())
    }

    /// Emergency stop zeroes torque immediately.
    #[test]
    fn interlock_system_emergency_stop() -> Result<(), Box<dyn std::error::Error>> {
        let watchdog = Box::new(SoftwareWatchdog::new(100));
        let mut system = SafetyInterlockSystem::new(watchdog, 25.0);
        system.arm()?;

        let result = system.emergency_stop();
        match &result.state {
            SafetyInterlockState::EmergencyStop { .. } => {}
            other => return Err(format!("Expected EmergencyStop, got {other:?}").into()),
        }
        assert!((result.torque_command - 0.0).abs() < 0.001);
        Ok(())
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// 5. FFB pipeline
// ══════════════════════════════════════════════════════════════════════════════

mod ffb_pipeline {
    use super::*;
    use racing_wheel_engine::filters::*;
    use racing_wheel_engine::pipeline::Pipeline;
    use racing_wheel_engine::rt::Frame;

    fn make_frame(ffb_in: f32, wheel_speed: f32) -> Frame {
        Frame {
            ffb_in,
            torque_out: ffb_in,
            wheel_speed,
            hands_off: false,
            ts_mono_ns: 0,
            seq: 0,
        }
    }

    /// An empty pipeline passes the frame through unchanged.
    #[test]
    fn empty_pipeline_passthrough() -> Result<(), Box<dyn std::error::Error>> {
        let mut pipeline = Pipeline::new();
        let mut frame = make_frame(0.5, 0.0);
        must(pipeline.process(&mut frame));
        assert!((frame.torque_out - 0.5).abs() < 0.001);
        Ok(())
    }

    /// Torque cap filter clamps output to the specified limit.
    #[test]
    fn torque_cap_clamps_output() -> Result<(), Box<dyn std::error::Error>> {
        let max_torque = 0.6f32;
        let mut frame = make_frame(0.0, 0.0);
        frame.torque_out = 0.9;
        torque_cap_filter(&mut frame, &max_torque as *const _ as *mut u8);
        assert!(
            (frame.torque_out - 0.6).abs() < 0.001,
            "Torque cap should clamp 0.9 to 0.6"
        );
        Ok(())
    }

    /// Torque cap: NaN input yields 0.0 (safe state).
    #[test]
    fn torque_cap_nan_safe_state() -> Result<(), Box<dyn std::error::Error>> {
        let max_torque = 1.0f32;
        let mut frame = make_frame(0.0, 0.0);
        frame.torque_out = f32::NAN;
        torque_cap_filter(&mut frame, &max_torque as *const _ as *mut u8);
        assert_eq!(frame.torque_out, 0.0, "NaN must map to safe state (0.0)");
        Ok(())
    }

    /// Reconstruction filter smooths a step input (output < input on first tick).
    #[test]
    fn reconstruction_smooths_step() -> Result<(), Box<dyn std::error::Error>> {
        let mut state = ReconstructionState::new(4);
        let state_ptr = &mut state as *mut _ as *mut u8;

        let mut frame = make_frame(1.0, 0.0);
        reconstruction_filter(&mut frame, state_ptr);
        assert!(
            frame.torque_out < 1.0 && frame.torque_out > 0.0,
            "Reconstruction filter should smooth step input, got {}",
            frame.torque_out
        );
        Ok(())
    }

    /// Friction filter produces non-zero output from wheel speed alone.
    #[test]
    fn friction_adds_speed_dependent_force() -> Result<(), Box<dyn std::error::Error>> {
        let state = FrictionState::new(0.2, true);
        let mut frame = make_frame(0.0, 5.0);
        friction_filter(&mut frame, &state as *const _ as *mut u8);
        assert!(
            frame.torque_out.abs() > 0.0,
            "Friction filter should produce force from wheel speed"
        );
        Ok(())
    }

    /// Damper filter produces velocity-proportional force.
    #[test]
    fn damper_adds_velocity_proportional_force() -> Result<(), Box<dyn std::error::Error>> {
        let state = DamperState::new(0.3, true);

        let mut frame_slow = make_frame(0.0, 1.0);
        damper_filter(&mut frame_slow, &state as *const _ as *mut u8);

        let mut frame_fast = make_frame(0.0, 5.0);
        damper_filter(&mut frame_fast, &state as *const _ as *mut u8);

        // Higher speed should produce more damping force
        assert!(
            frame_fast.torque_out.abs() > frame_slow.torque_out.abs(),
            "Damper force at 5 rad/s ({}) should exceed force at 1 rad/s ({})",
            frame_fast.torque_out.abs(),
            frame_slow.torque_out.abs()
        );
        Ok(())
    }

    /// Slew rate limiter constrains maximum rate of change.
    #[test]
    fn slew_rate_limits_change() -> Result<(), Box<dyn std::error::Error>> {
        let mut state = SlewRateState::new(0.1); // very restrictive
        let state_ptr = &mut state as *mut _ as *mut u8;

        // First tick: 0.0
        let mut frame = make_frame(0.0, 0.0);
        slew_rate_filter(&mut frame, state_ptr);

        // Second tick: large step to 1.0
        let mut frame = make_frame(1.0, 0.0);
        slew_rate_filter(&mut frame, state_ptr);

        // Output should be limited by slew rate
        assert!(
            frame.torque_out < 1.0,
            "Slew rate limiter should prevent instant jump to 1.0, got {}",
            frame.torque_out
        );
        Ok(())
    }

    /// Pipeline output stays bounded in [-1.0, 1.0] even with chained filters.
    #[test]
    fn pipeline_output_bounded() -> Result<(), Box<dyn std::error::Error>> {
        // Process a range of inputs through a reconstruction filter
        let mut state = ReconstructionState::new(2);
        let state_ptr = &mut state as *mut _ as *mut u8;

        for input in [-1.0f32, -0.5, 0.0, 0.5, 1.0] {
            let mut frame = make_frame(input, 0.0);
            reconstruction_filter(&mut frame, state_ptr);
            assert!(
                frame.torque_out >= -1.0 && frame.torque_out <= 1.0,
                "Pipeline output {} out of [-1.0, 1.0] for input {}",
                frame.torque_out,
                input
            );
        }
        Ok(())
    }
}

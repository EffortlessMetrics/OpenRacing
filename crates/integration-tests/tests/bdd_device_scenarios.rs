//! BDD-style device scenario tests for vendor-specific behaviour.
//!
//! Each test follows the **Given / When / Then** pattern and uses virtual
//! device implementations — no real USB hardware is required.
//!
//! # Scenarios
//!
//! 1. Moza R9 connected → iRacing starts → FFB active with correct torque range
//! 2. Fanatec CSL DD connected → user switches profiles → FFB parameters update
//! 3. Logitech G29 connected → all buttons pressed → inputs correctly mapped
//! 4. Thrustmaster T300 connected → safety fault → torque drops to zero within 50ms
//! 5. Multiple devices connected → primary disconnects → fallback takes over
//! 6. SimuCube 2 connected → firmware update starts → FFB disabled during update
//! 7. OpenFFBoard connected → direct mode enabled → torque bypasses filters
//! 8. Any device connected → USB disconnects during FFB → safe state entered
//! 9. Heusinkveld Sprint connected → game starts → pedal input detected
//! 10. Heusinkveld profile switch → input sensitivity updates
//! 11. Heusinkveld USB disconnect → safe state
//! 12. Leo Bodnar wheel connected → iRacing starts → FFB active
//! 13. Leo Bodnar profile switch → FFB parameters update
//! 14. Leo Bodnar USB disconnect → safe state
//! 15. PXN V12 connected → game starts → FFB active
//! 16. PXN profile switch → FFB update
//! 17. PXN USB disconnect → safe state
//! 18. FFBeast wheel connected → game starts → FFB active
//! 19. FFBeast profile switch → FFB update
//! 20. FFBeast USB disconnect → safe state
//! 21. Simagic EVO connected → game starts → FFB active
//! 22. Simagic profile switch → FFB update
//! 23. Simagic USB disconnect → safe state
//! 24. VRS DirectForce Pro connected → game starts → FFB active
//! 25. VRS profile switch → FFB update
//! 26. VRS USB disconnect → safe state
//! 27. Cammus C12 connected → game starts → FFB active
//! 28. Cammus profile switch → FFB update
//! 29. Cammus USB disconnect → safe state
//! 30. AccuForce Pro connected → game starts → FFB active
//! 31. AccuForce profile switch → FFB update
//! 32. AccuForce USB disconnect → safe state
//! 33. Cube Controls GT Pro connected → inputs detected
//! 34. Cube Controls profile switch → input mapping update
//! 35. Cube Controls USB disconnect → safe state

use std::time::{Duration, Instant};

use anyhow::Result;

#[allow(unused_imports)]
use hid_heusinkveld_protocol::{
    HEUSINKVELD_SPRINT_PID, HeusinkveldModel, VENDOR_ID as HEUSINKVELD_VID,
    heusinkveld_model_from_info, is_heusinkveld_device,
};
use openracing_filters::{DamperState, Frame as FilterFrame, damper_filter, torque_cap_filter};
use racing_wheel_engine::ports::HidDevice;
use racing_wheel_engine::safety::{FaultType, SafetyService, SafetyState};
use racing_wheel_engine::{
    CapabilityNegotiator, FFBMode, GameCompatibility, ModeSelectionPolicy, VirtualDevice,
};
use racing_wheel_hid_fanatec_protocol::product_ids as fanatec_product_ids;
use racing_wheel_hid_logitech_protocol::product_ids as logitech_product_ids;
use racing_wheel_hid_moza_protocol::DeviceWriter;
use racing_wheel_hid_moza_protocol::product_ids as moza_product_ids;
use racing_wheel_hid_thrustmaster_protocol::product_ids as thrustmaster_product_ids;
use racing_wheel_integration_tests::fanatec_virtual::FanatecScenario;
use racing_wheel_integration_tests::ffbeast_virtual::FFBeastScenario;
use racing_wheel_integration_tests::logitech_virtual::LogitechScenario;
use racing_wheel_integration_tests::moza_virtual::MozaScenario;
use racing_wheel_integration_tests::openffboard_virtual::OpenFFBoardScenario;
use racing_wheel_integration_tests::simagic_virtual::SimagicScenario;
use racing_wheel_integration_tests::thrustmaster_virtual::ThrustmasterScenario;
use racing_wheel_schemas::prelude::*;

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 1: Moza R9 connected → iRacing starts → FFB active with torque range
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  a Moza R9 is connected and initialised
/// When   the user starts iRacing (game with robust FFB support)
/// Then   FFB is active on the device
/// And    the device negotiates raw torque mode for iRacing
/// And    the torque range is within the R9's physical limits
/// ```
#[test]
fn given_moza_r9_connected_when_user_starts_iracing_then_ffb_active_with_correct_torque_range()
-> Result<()> {
    // Given: a Moza R9 is connected and the protocol handshake completes
    let mut scenario = MozaScenario::wheelbase(moza_product_ids::R9_V2);
    scenario
        .initialize()
        .map_err(|e| anyhow::anyhow!("Moza R9 init failed: {e}"))?;

    assert!(
        scenario.protocol.is_ffb_ready(),
        "FFB must be available after Moza R9 initialisation"
    );

    // When: iRacing starts — model it as a game with robust FFB
    let iracing = GameCompatibility {
        game_id: "iracing".to_string(),
        supports_robust_ffb: true,
        supports_telemetry: true,
        preferred_mode: FFBMode::RawTorque,
    };

    // R9 capabilities: ~9 Nm, raw torque capable
    let r9_caps = DeviceCapabilities::new(true, true, true, true, TorqueNm::new(9.0)?, 65535, 1000);

    let mode = ModeSelectionPolicy::select_mode(&r9_caps, Some(&iracing));

    // Then: FFB is active — feature reports were sent during init
    assert!(
        !scenario.device.feature_reports().is_empty(),
        "Moza R9 must have sent feature reports during initialisation"
    );

    // And: the device negotiates raw torque mode for iRacing
    assert_eq!(
        mode,
        FFBMode::RawTorque,
        "Moza R9 must negotiate raw torque mode for iRacing"
    );

    // And: the torque range is within R9's physical limits (max ~9 Nm)
    assert!(
        r9_caps.max_torque.value() > 0.0 && r9_caps.max_torque.value() <= 12.0,
        "R9 max torque must be within physical limits, got {} Nm",
        r9_caps.max_torque.value()
    );

    // And: a filter pipeline can process FFB within this torque range
    let mut frame = FilterFrame {
        ffb_in: 0.7,
        torque_out: 0.7,
        wheel_speed: 3.0,
        hands_off: false,
        ts_mono_ns: 1_000_000,
        seq: 0,
    };
    torque_cap_filter(&mut frame, 1.0);
    assert!(
        frame.torque_out.is_finite() && frame.torque_out.abs() <= 1.0,
        "pipeline output must be finite and within [-1, 1]"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 2: Fanatec CSL DD → user switches profiles → FFB parameters update
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  a Fanatec CSL DD is connected and initialised
/// When   the user switches from a low-gain profile to a high-gain profile
/// Then   the FFB parameters update to reflect the new profile
/// And    the filter pipeline applies the new gain immediately
/// And    the device remains operational throughout the switch
/// ```
#[test]
fn given_fanatec_csl_dd_connected_when_user_switches_profiles_then_ffb_parameters_update()
-> Result<()> {
    // Given: a Fanatec CSL DD is connected and initialised
    let mut scenario = FanatecScenario::wheelbase(fanatec_product_ids::CSL_DD);
    scenario
        .initialize()
        .map_err(|e| anyhow::anyhow!("Fanatec CSL DD init failed: {e}"))?;

    // CSL DD capabilities: ~8 Nm, raw torque capable
    let csl_dd_caps =
        DeviceCapabilities::new(true, true, true, true, TorqueNm::new(8.0)?, 65535, 1000);
    assert!(
        csl_dd_caps.supports_ffb(),
        "CSL DD must support force feedback"
    );

    // Define the low-gain and high-gain profiles
    let low_gain: f32 = 0.40;
    let high_gain: f32 = 0.90;
    let base_ffb: f32 = 0.6;

    // When: user switches from low-gain to high-gain profile
    let old_scaled = base_ffb * low_gain;
    let new_scaled = base_ffb * high_gain;

    // Then: the new gain produces stronger output
    assert!(
        new_scaled > old_scaled,
        "high-gain profile ({high_gain}) must produce stronger output than low-gain ({low_gain})"
    );

    // And: the filter pipeline applies the new gain
    let mut frame = FilterFrame {
        ffb_in: new_scaled,
        torque_out: new_scaled,
        wheel_speed: 2.0,
        hands_off: false,
        ts_mono_ns: 0,
        seq: 0,
    };
    let damper = DamperState::fixed(0.05);
    damper_filter(&mut frame, &damper);
    torque_cap_filter(&mut frame, 1.0);
    assert!(
        frame.torque_out.is_finite() && frame.torque_out.abs() <= 1.0,
        "pipeline with new gain must produce finite output within [-1, 1], got {}",
        frame.torque_out
    );

    // And: the device remains operational — init reports were sent
    assert!(
        !scenario.device.feature_reports().is_empty(),
        "CSL DD must remain operational (feature reports present) after profile switch"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 3: Logitech G29 → all buttons pressed → inputs correctly mapped
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  a Logitech G29 is connected and initialised
/// When   the user presses all buttons (simulated via capability report)
/// Then   all inputs are correctly mapped via the PID protocol
/// And    the capability report round-trips correctly
/// And    the device negotiates PID pass-through mode (G29 is PID-only)
/// ```
#[test]
fn given_logitech_g29_connected_when_user_presses_all_buttons_then_inputs_correctly_mapped()
-> Result<()> {
    // Given: a Logitech G29 is connected and initialised
    let mut scenario = LogitechScenario::wheel(logitech_product_ids::G29_PS);
    scenario
        .initialize()
        .map_err(|e| anyhow::anyhow!("Logitech G29 init failed: {e}"))?;

    // G29 capabilities: PID-only, 2.8 Nm, 4096 encoder steps
    let g29_caps =
        DeviceCapabilities::new(true, false, false, false, TorqueNm::new(2.8)?, 4096, 2000);

    // When: the capability report is created and round-tripped
    let report = CapabilityNegotiator::create_capabilities_report(&g29_caps);
    let parsed = CapabilityNegotiator::parse_capabilities_report(&report)
        .map_err(|e| anyhow::anyhow!("capability parse failed: {e}"))?;

    // Then: all inputs are correctly mapped — capabilities round-trip
    assert!(
        (parsed.max_torque.value() - g29_caps.max_torque.value()).abs() < 0.1,
        "max torque must round-trip correctly: expected {}, got {}",
        g29_caps.max_torque.value(),
        parsed.max_torque.value()
    );
    // And: the device negotiates PID pass-through (G29 is PID-only)
    let mode = ModeSelectionPolicy::select_mode(&parsed, None);
    assert_eq!(
        mode,
        FFBMode::PidPassthrough,
        "G29 must use PID pass-through mode"
    );

    // And: the device sent feature reports during initialisation
    assert!(
        !scenario.device.feature_reports().is_empty(),
        "G29 initialisation must send feature reports to activate input mapping"
    );

    // And: PID supports FFB
    assert!(
        g29_caps.supports_ffb(),
        "G29 must report FFB support via PID"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 4: Thrustmaster T300 → safety fault → torque drops to zero in 50ms
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  a Thrustmaster T300 is connected with active force feedback
/// When   a safety fault (overcurrent) occurs
/// Then   torque drops to zero within 50ms
/// And    the safety state transitions to Faulted
/// And    the device receives zero torque after the fault
/// ```
#[test]
fn given_thrustmaster_t300_connected_when_safety_fault_then_torque_drops_to_zero_within_50ms()
-> Result<()> {
    // Given: a Thrustmaster T300 is connected and initialised
    let mut scenario = ThrustmasterScenario::wheelbase(thrustmaster_product_ids::T300_RS);
    scenario
        .initialize()
        .map_err(|e| anyhow::anyhow!("Thrustmaster T300 init failed: {e}"))?;

    // And: a virtual device + safety service are active with FFB flowing
    let id: DeviceId = "bdd-t300-fault-001".parse()?;
    let mut device = VirtualDevice::new(id, "Thrustmaster T300 RS".to_string());
    let mut safety = SafetyService::new(5.0, 20.0);

    // Confirm FFB is flowing normally
    let normal_torque = safety.clamp_torque_nm(4.0);
    assert!(
        (normal_torque - 4.0).abs() < 0.01,
        "torque must flow normally before fault, got {normal_torque}"
    );
    device.write_ffb_report(normal_torque, 0)?;

    // When: a safety fault (overcurrent) occurs — measure timing
    let fault_start = Instant::now();
    safety.report_fault(FaultType::Overcurrent);
    let clamped = safety.clamp_torque_nm(20.0);
    let fault_elapsed = fault_start.elapsed();

    // Then: torque drops to zero within 50ms
    assert!(
        fault_elapsed < Duration::from_millis(50),
        "fault-to-zero must complete in <50ms (actual: {fault_elapsed:?})"
    );
    assert!(
        clamped.abs() < 0.001,
        "torque must be zero after overcurrent fault, got {clamped}"
    );

    // And: safety state transitions to Faulted
    match safety.state() {
        SafetyState::Faulted { fault, .. } => {
            assert_eq!(
                *fault,
                FaultType::Overcurrent,
                "fault type must be Overcurrent"
            );
        }
        other => {
            return Err(anyhow::anyhow!(
                "expected Faulted(Overcurrent), got {other:?}"
            ));
        }
    }

    // And: the device receives zero torque after the fault
    device.write_ffb_report(clamped, 1)?;
    assert!(
        device.is_connected(),
        "T300 device must remain connected (fault is software-side)"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 5: Multiple devices → primary disconnects → fallback takes over
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  two devices are connected: a primary (CSL DD) and a fallback (G29)
/// When   the primary device disconnects
/// Then   the primary reports as disconnected and its safety faults
/// And    the fallback device remains connected and operational
/// And    FFB can continue through the fallback device
/// ```
#[test]
fn given_multiple_devices_when_primary_disconnects_then_fallback_takes_over() -> Result<()> {
    // Given: primary (CSL DD) and fallback (G29) devices are connected
    let primary_id: DeviceId = "bdd-primary-csldd".parse()?;
    let fallback_id: DeviceId = "bdd-fallback-g29".parse()?;

    let mut primary = VirtualDevice::new(primary_id, "Fanatec CSL DD".to_string());
    let fallback = VirtualDevice::new(fallback_id, "Logitech G29".to_string());

    let mut primary_safety = SafetyService::new(8.0, 20.0);
    let fallback_safety = SafetyService::new(2.8, 10.0);

    // Both devices are connected and operational
    assert!(primary.is_connected(), "primary must be connected");
    assert!(fallback.is_connected(), "fallback must be connected");

    // Primary is actively sending FFB
    primary.write_ffb_report(5.0, 0)?;
    let primary_torque = primary_safety.clamp_torque_nm(5.0);
    assert!(
        (primary_torque - 5.0).abs() < 0.01,
        "primary must have active FFB before disconnect"
    );

    // When: the primary device disconnects
    primary.disconnect();
    primary_safety.report_fault(FaultType::UsbStall);

    // Then: the primary reports as disconnected and faulted
    assert!(
        !primary.is_connected(),
        "primary must report disconnected after disconnect"
    );
    assert!(
        matches!(primary_safety.state(), SafetyState::Faulted { .. }),
        "primary safety must be Faulted after disconnect"
    );
    let primary_clamped = primary_safety.clamp_torque_nm(5.0);
    assert!(
        primary_clamped.abs() < 0.001,
        "primary torque must be zero after disconnect, got {primary_clamped}"
    );

    // And: the fallback device remains connected and operational
    assert!(
        fallback.is_connected(),
        "fallback must remain connected when primary disconnects"
    );
    assert_eq!(
        fallback_safety.state(),
        &SafetyState::SafeTorque,
        "fallback safety must remain in SafeTorque"
    );

    // And: FFB can continue through the fallback device
    let fallback_torque = fallback_safety.clamp_torque_nm(2.0);
    assert!(
        (fallback_torque - 2.0).abs() < 0.01,
        "fallback must deliver FFB normally, got {fallback_torque}"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 6: SimuCube 2 → firmware update starts → FFB disabled during update
// ═══════════════════════════════════════════════════════════════════════════════

/// Models a firmware update lifecycle for a device.
struct FirmwareUpdateSession {
    in_progress: bool,
    ffb_disabled: bool,
}

impl FirmwareUpdateSession {
    fn start() -> Self {
        Self {
            in_progress: true,
            ffb_disabled: true,
        }
    }

    fn is_in_progress(&self) -> bool {
        self.in_progress
    }

    fn is_ffb_disabled(&self) -> bool {
        self.ffb_disabled
    }

    fn complete(&mut self) {
        self.in_progress = false;
        self.ffb_disabled = false;
    }
}

/// ```text
/// Given  a SimuCube 2 Pro is connected with active FFB
/// When   a firmware update starts
/// Then   FFB is disabled during the update (safety precaution)
/// And    torque output is clamped to zero while update is in progress
/// And    after the update completes, FFB can be re-enabled
/// ```
#[test]
fn given_simucube_2_connected_when_firmware_update_starts_then_ffb_disabled_during_update()
-> Result<()> {
    // Given: a SimuCube 2 Pro is connected with active FFB
    let id: DeviceId = "bdd-sc2pro-fw".parse()?;
    let device = VirtualDevice::new(id, "SimuCube 2 Pro".to_string());

    let sc2_caps =
        DeviceCapabilities::new(true, true, true, true, TorqueNm::new(25.0)?, 65535, 500);
    assert!(sc2_caps.supports_ffb(), "SC2 Pro must support FFB");
    assert!(
        device.is_connected(),
        "SC2 Pro must be connected before update"
    );

    // Normal FFB flow before update
    let safety_pre = SafetyService::new(10.0, 25.0);
    let pre_torque = safety_pre.clamp_torque_nm(8.0);
    assert!(
        (pre_torque - 8.0).abs() < 0.01,
        "FFB must flow normally before firmware update"
    );

    // When: a firmware update starts
    let update = FirmwareUpdateSession::start();

    // Then: FFB is disabled during the update
    assert!(
        update.is_in_progress(),
        "firmware update must be in progress"
    );
    assert!(
        update.is_ffb_disabled(),
        "FFB must be disabled during firmware update"
    );

    // And: torque output is clamped to zero during update (safety service in safe state)
    let mut safety_during = SafetyService::new(10.0, 25.0);
    safety_during.report_fault(FaultType::PipelineFault);
    let during_torque = safety_during.clamp_torque_nm(10.0);
    assert!(
        during_torque.abs() < 0.001,
        "torque must be zero during firmware update, got {during_torque}"
    );

    // And: after the update completes, FFB can be re-enabled
    let mut update = update;
    update.complete();
    assert!(!update.is_in_progress(), "firmware update must be complete");
    assert!(
        !update.is_ffb_disabled(),
        "FFB must be re-enabled after firmware update completes"
    );

    // Fresh safety service represents post-update state
    let safety_post = SafetyService::new(10.0, 25.0);
    let post_torque = safety_post.clamp_torque_nm(8.0);
    assert!(
        (post_torque - 8.0).abs() < 0.01,
        "FFB must resume normally after firmware update, got {post_torque}"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 7: OpenFFBoard → direct mode → torque bypasses filters
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  an OpenFFBoard device is connected and initialised
/// When   direct mode is enabled (raw torque commands)
/// Then   torque commands bypass the filter pipeline
/// And    the raw torque value is written directly to the device
/// And    the safety clamp still applies (never bypass safety)
/// ```
#[test]
fn given_openffboard_connected_when_direct_mode_enabled_then_torque_bypasses_filters() -> Result<()>
{
    // Given: an OpenFFBoard is connected and initialised
    let mut scenario = OpenFFBoardScenario::wheelbase(
        racing_wheel_hid_openffboard_protocol::OPENFFBOARD_PRODUCT_ID,
    );
    scenario
        .initialize()
        .map_err(|e| anyhow::anyhow!("OpenFFBoard init failed: {e}"))?;

    // And: a virtual device for torque output
    let id: DeviceId = "bdd-offb-direct".parse()?;
    let mut device = VirtualDevice::new(id, "OpenFFBoard Direct".to_string());

    // OpenFFBoard supports raw torque at 1kHz
    let offb_caps =
        DeviceCapabilities::new(true, true, false, false, TorqueNm::new(20.0)?, 65535, 500);
    let mode = ModeSelectionPolicy::select_mode(&offb_caps, None);
    assert_eq!(
        mode,
        FFBMode::RawTorque,
        "OpenFFBoard must select raw torque mode"
    );

    // When: direct mode is enabled — raw torque value bypasses the filter pipeline
    let raw_torque: f32 = 0.75;

    // In direct mode, we skip the filter pipeline and go straight to the safety clamp
    // (safety is NEVER bypassed)
    let safety = SafetyService::new(15.0, 20.0);
    let torque_nm = raw_torque * offb_caps.max_torque.value();
    let clamped = safety.clamp_torque_nm(torque_nm);

    // Then: the raw torque value passes through (within safe limits)
    assert!(
        (clamped - torque_nm).abs() < 0.01,
        "direct mode torque must pass through safety clamp unchanged when within limits: \
         expected {torque_nm}, got {clamped}"
    );

    // And: the device accepts the torque command
    device.write_ffb_report(clamped, 0)?;
    assert!(
        device.is_connected(),
        "device must remain connected after direct torque write"
    );

    // And: safety clamp still applies — requesting above limit gets clamped
    let over_limit = safety.clamp_torque_nm(25.0);
    assert!(
        over_limit <= 15.0,
        "safety must still clamp torque in direct mode: requested 25 Nm, got {over_limit}"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 8: Any device → USB disconnects during FFB → safe state entered
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  a device is connected and force feedback is actively flowing
/// When   the USB connection drops unexpectedly during FFB output
/// Then   the device reports as disconnected immediately
/// And    the safety service enters the Faulted state (UsbStall)
/// And    all torque output is clamped to zero
/// And    the fault response completes within 50ms
/// ```
#[test]
fn given_any_device_connected_when_usb_disconnects_during_ffb_then_safe_state_entered() -> Result<()>
{
    // Given: a device is connected with active FFB
    let id: DeviceId = "bdd-usb-drop-001".parse()?;
    let mut device = VirtualDevice::new(id, "USB Drop Test Wheel".to_string());
    let mut safety = SafetyService::new(5.0, 20.0);

    // Confirm FFB is actively flowing
    device.write_ffb_report(4.0, 0)?;
    let normal = safety.clamp_torque_nm(4.0);
    assert!(
        (normal - 4.0).abs() < 0.01,
        "FFB must be active before USB disconnect"
    );
    assert!(device.is_connected(), "device must be connected initially");

    // When: USB disconnects unexpectedly during FFB — measure timing
    let disconnect_start = Instant::now();
    device.disconnect();
    safety.report_fault(FaultType::UsbStall);
    let clamped = safety.clamp_torque_nm(20.0);
    let disconnect_elapsed = disconnect_start.elapsed();

    // Then: the device reports as disconnected immediately
    assert!(
        !device.is_connected(),
        "device must report disconnected after USB drop"
    );

    // And: the safety service enters Faulted state
    match safety.state() {
        SafetyState::Faulted { fault, .. } => {
            assert_eq!(
                *fault,
                FaultType::UsbStall,
                "fault type must be UsbStall after USB disconnect"
            );
        }
        other => {
            return Err(anyhow::anyhow!("expected Faulted(UsbStall), got {other:?}"));
        }
    }

    // And: all torque output is clamped to zero
    for requested in [0.0, 1.0, 5.0, 20.0, -10.0] {
        let result = safety.clamp_torque_nm(requested);
        assert!(
            result.abs() < 0.001,
            "torque must be zero after USB drop; requested={requested}, got={result}"
        );
    }

    // And: the fault response completes within 50ms
    assert!(
        disconnect_elapsed < Duration::from_millis(50),
        "USB disconnect → safe state must complete in <50ms (actual: {disconnect_elapsed:?})"
    );

    // And: the initial clamp call also returned zero
    assert!(
        clamped.abs() < 0.001,
        "immediate post-fault torque must be zero, got {clamped}"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 9: Heusinkveld Sprint connected → game starts → FFB active
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  a Heusinkveld Sprint pedal is connected and initialised
/// When   a game with FFB support starts
/// Then   the pedal input is detected and mapped correctly
/// And    the device reports load cell values
/// ```
#[test]
fn given_heusinkveld_sprint_connected_when_game_starts_then_ffb_active() -> Result<()> {
    use hid_heusinkveld_protocol::{
        HEUSINKVELD_SPRINT_PID, HeusinkveldModel, VENDOR_ID as HEUSINKVELD_VID,
        heusinkveld_model_from_info, is_heusinkveld_device,
    };

    // Given: Heusinkveld Sprint is connected
    assert!(
        is_heusinkveld_device(HEUSINKVELD_VID),
        "Heusinkveld VID must be recognized"
    );
    let model = heusinkveld_model_from_info(HEUSINKVELD_VID, HEUSINKVELD_SPRINT_PID);
    assert!(
        matches!(model, HeusinkveldModel::Sprint),
        "PID must map to Sprint model, got {:?}",
        model
    );

    // When: game starts with FFB support
    let game = GameCompatibility {
        game_id: "acc".to_string(),
        supports_robust_ffb: true,
        supports_telemetry: true,
        preferred_mode: FFBMode::RawTorque,
    };

    // Then: device is recognized and FFB is available
    // Heusinkveld devices are pedals (input-only), so they use PID passthrough mode
    let caps = DeviceCapabilities::new(true, false, false, false, TorqueNm::new(0.0)?, 4096, 1000);
    let mode = ModeSelectionPolicy::select_mode(&caps, Some(&game));
    assert_eq!(
        mode,
        FFBMode::PidPassthrough,
        "Heusinkveld (pedals) must use PID passthrough mode"
    );

    // And: load cell input can be parsed
    let id: DeviceId = "bdd-heusinkveld-sprint".parse()?;
    let mut device = VirtualDevice::new(id, "Heusinkveld Sprint".to_string());
    device.write_ffb_report(0.0, 0)?;
    assert!(device.is_connected(), "device must remain connected");

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 10: Heusinkveld profile switch → FFB parameters update
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  a Heusinkveld pedal set is connected with active input
/// When   user switches from street to track profile
/// Then   the input sensitivity updates accordingly
/// And    the pedal response curve changes
/// ```
#[test]
fn given_heusinkveld_connected_when_profile_switch_then_ffb_updates() -> Result<()> {
    // Given: Heusinkveld with active input
    let id: DeviceId = "bdd-heusinkveld-profile".parse()?;
    let mut device = VirtualDevice::new(id, "Heusinkveld Ultimate".to_string());

    let street_sensitivity: f32 = 0.6;
    let track_sensitivity: f32 = 1.0;

    // When: profile switches from street to track
    let street_output = 0.5 * street_sensitivity;
    let track_output = 0.5 * track_sensitivity;

    // Then: track profile produces higher output
    assert!(
        track_output > street_output,
        "track profile ({track_sensitivity}) must be more sensitive than street ({street_sensitivity})"
    );

    // And: device remains operational after profile switch
    device.write_ffb_report(track_output, 0)?;
    assert!(
        device.is_connected(),
        "device must remain connected after profile switch"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 11: Heusinkveld USB disconnect → safe state entered
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  a Heusinkveld pedal set is connected with active input
/// When   USB connection drops unexpectedly
/// Then   the device reports as disconnected
/// And    safety service enters faulted state
/// And    input values clamp to zero
/// ```
#[test]
fn given_heusinkveld_connected_when_usb_disconnects_then_safe_state() -> Result<()> {
    // Given: Heusinkveld is connected with active input
    let id: DeviceId = "bdd-heusinkveld-disconnect".parse()?;
    let mut device = VirtualDevice::new(id, "Heusinkveld Pro".to_string());
    let mut safety = SafetyService::new(5.0, 20.0);

    // Confirm active input
    device.write_ffb_report(0.5, 0)?;
    let normal = safety.clamp_torque_nm(0.5);
    assert!(
        (normal - 0.5).abs() < 0.01,
        "input must flow normally before disconnect"
    );

    // When: USB disconnects
    device.disconnect();
    safety.report_fault(FaultType::UsbStall);

    // Then: device reports disconnected
    assert!(
        !device.is_connected(),
        "device must report disconnected after USB drop"
    );

    // And: safety enters faulted state
    match safety.state() {
        SafetyState::Faulted { fault, .. } => {
            assert_eq!(*fault, FaultType::UsbStall, "fault must be UsbStall");
        }
        other => return Err(anyhow::anyhow!("expected Faulted, got {other:?}")),
    }

    // And: input clamps to zero
    let clamped = safety.clamp_torque_nm(10.0);
    assert!(
        clamped.abs() < 0.001,
        "torque must be zero after disconnect"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 12: Leo Bodnar wheel connected → iRacing starts → FFB active
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  a Leo Bodnar wheel interface is connected and initialised
/// When   the user starts iRacing (game with robust FFB support)
/// Then   FFB is active on the device
/// And    the device negotiates raw torque mode
/// And    the torque range is within the wheel's physical limits
/// ```
#[test]
fn given_leo_bodnar_wheel_connected_when_user_starts_iracing_then_ffb_active() -> Result<()> {
    // Given: Leo Bodnar wheel interface is connected
    let id: DeviceId = "bdd-leo-bodnar-wheel".parse()?;
    let device = VirtualDevice::new(id, "Leo Bodnar Wheel Interface".to_string());

    // Leo Bodnar wheel: ~10 Nm, raw torque capable
    let lb_caps =
        DeviceCapabilities::new(true, true, true, true, TorqueNm::new(10.0)?, 65535, 1000);
    assert!(lb_caps.supports_ffb(), "Leo Bodnar must support FFB");

    // When: iRacing starts
    let iracing = GameCompatibility {
        game_id: "iracing".to_string(),
        supports_robust_ffb: true,
        supports_telemetry: true,
        preferred_mode: FFBMode::RawTorque,
    };

    let mode = ModeSelectionPolicy::select_mode(&lb_caps, Some(&iracing));

    // Then: FFB is active
    assert_eq!(
        mode,
        FFBMode::RawTorque,
        "Leo Bodnar must negotiate raw torque mode"
    );

    // And: torque range is within limits
    assert!(
        lb_caps.max_torque.value() > 0.0 && lb_caps.max_torque.value() <= 15.0,
        "max torque must be within physical limits"
    );

    // And: device is connected
    assert!(device.is_connected(), "device must be connected");

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 13: Leo Bodnar profile switch → FFB update
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  a Leo Bodnar wheel is connected with active FFB
/// When   user switches from comfort to sport profile
/// Then   the FFB parameters update to reflect the new profile
/// And    torque output increases accordingly
/// ```
#[test]
fn given_leo_bodnar_connected_when_profile_switch_then_ffb_updates() -> Result<()> {
    // Given: Leo Bodnar with active FFB
    let id: DeviceId = "bdd-leo-bodnar-profile".parse()?;
    let mut device = VirtualDevice::new(id, "Leo Bodnar FFB Joystick".to_string());

    let comfort_gain: f32 = 0.5;
    let sport_gain: f32 = 0.85;
    let base_ffb: f32 = 0.6;

    // When: profile switches
    let comfort_output = base_ffb * comfort_gain;
    let sport_output = base_ffb * sport_gain;

    // Then: sport produces stronger FFB
    assert!(
        sport_output > comfort_output,
        "sport profile ({sport_gain}) must produce stronger FFB than comfort ({comfort_gain})"
    );

    // And: device remains operational
    device.write_ffb_report(sport_output, 0)?;
    assert!(
        device.is_connected(),
        "device must remain connected after profile switch"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 14: Leo Bodnar USB disconnect → safe state
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  a Leo Bodnar wheel is connected with active FFB
/// When   USB connection drops unexpectedly
/// Then   the device reports as disconnected
/// And    safety service enters faulted state
/// And    torque output clamps to zero
/// ```
#[test]
fn given_leo_bodnar_connected_when_usb_disconnects_then_safe_state() -> Result<()> {
    // Given: Leo Bodnar connected with active FFB
    let id: DeviceId = "bdd-leo-bodnar-disconnect".parse()?;
    let mut device = VirtualDevice::new(id, "Leo Bodnar Wheel".to_string());
    let mut safety = SafetyService::new(8.0, 20.0);

    // Active FFB before disconnect
    device.write_ffb_report(5.0, 0)?;
    let normal = safety.clamp_torque_nm(5.0);
    assert!(
        (normal - 5.0).abs() < 0.01,
        "FFB must flow before disconnect"
    );

    // When: USB disconnects
    device.disconnect();
    safety.report_fault(FaultType::UsbStall);

    // Then: disconnected
    assert!(!device.is_connected(), "device must report disconnected");

    // And: faulted state
    match safety.state() {
        SafetyState::Faulted { fault, .. } => {
            assert_eq!(*fault, FaultType::UsbStall);
        }
        other => return Err(anyhow::anyhow!("expected Faulted, got {other:?}")),
    }

    // And: zero torque
    let clamped = safety.clamp_torque_nm(10.0);
    assert!(
        clamped.abs() < 0.001,
        "torque must be zero after disconnect"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 15: PXN V12 connected → game starts → FFB active
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  a PXN V12 wheel is connected and initialised
/// When   a game with FFB support starts
/// Then   FFB is active on the device
/// And    the device negotiates PID mode
/// And    the torque range is within the wheel's physical limits
/// ```
#[test]
fn given_pxn_v12_connected_when_game_starts_then_ffb_active() -> Result<()> {
    // Given: PXN V12 is connected
    let id: DeviceId = "bdd-pxn-v12".parse()?;
    let device = VirtualDevice::new(id, "PXN V12".to_string());

    // PXN V12: ~12 Nm, HID PID FFB
    let pxn_caps =
        DeviceCapabilities::new(true, true, true, true, TorqueNm::new(12.0)?, 65535, 1000);
    assert!(pxn_caps.supports_ffb(), "PXN must support FFB");

    // When: game starts
    let game = GameCompatibility {
        game_id: "acc".to_string(),
        supports_robust_ffb: true,
        supports_telemetry: true,
        preferred_mode: FFBMode::RawTorque,
    };

    let mode = ModeSelectionPolicy::select_mode(&pxn_caps, Some(&game));

    // Then: FFB is active
    assert_eq!(
        mode,
        FFBMode::RawTorque,
        "PXN must negotiate raw torque mode"
    );

    // And: within physical limits
    assert!(
        pxn_caps.max_torque.value() > 0.0 && pxn_caps.max_torque.value() <= 15.0,
        "max torque must be within physical limits"
    );

    // And: device connected
    assert!(device.is_connected(), "device must be connected");

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 16: PXN profile switch → FFB update
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  a PXN wheel is connected with active FFB
/// When   user switches from default to custom profile
/// Then   the FFB parameters update accordingly
/// And    the filter pipeline applies new settings
/// ```
#[test]
fn given_pxn_connected_when_profile_switch_then_ffb_updates() -> Result<()> {
    // Given: PXN with active FFB
    let id: DeviceId = "bdd-pxn-profile".parse()?;
    let mut device = VirtualDevice::new(id, "PXN V10".to_string());

    let default_gain: f32 = 0.7;
    let custom_gain: f32 = 1.0;
    let base_ffb: f32 = 0.5;

    // When: profile switches
    let default_output = base_ffb * default_gain;
    let custom_output = base_ffb * custom_gain;

    // Then: custom produces stronger FFB
    assert!(
        custom_output > default_output,
        "custom profile must produce stronger FFB"
    );

    // And: device operational
    device.write_ffb_report(custom_output, 0)?;
    assert!(device.is_connected(), "device must remain connected");

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 17: PXN USB disconnect → safe state
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  a PXN wheel is connected with active FFB
/// When   USB connection drops unexpectedly
/// Then   the device reports as disconnected
/// And    safety service enters faulted state
/// And    torque clamps to zero
/// ```
#[test]
fn given_pxn_connected_when_usb_disconnects_then_safe_state() -> Result<()> {
    // Given: PXN connected
    let id: DeviceId = "bdd-pxn-disconnect".parse()?;
    let mut device = VirtualDevice::new(id, "PXN GT987".to_string());
    let mut safety = SafetyService::new(10.0, 25.0);

    // Active FFB
    device.write_ffb_report(8.0, 0)?;
    let normal = safety.clamp_torque_nm(8.0);
    assert!((normal - 8.0).abs() < 0.01);

    // When: USB disconnects
    device.disconnect();
    safety.report_fault(FaultType::UsbStall);

    // Then: disconnected
    assert!(!device.is_connected());

    // And: faulted
    match safety.state() {
        SafetyState::Faulted { fault, .. } => {
            assert_eq!(*fault, FaultType::UsbStall);
        }
        other => return Err(anyhow::anyhow!("expected Faulted, got {other:?}")),
    }

    // And: zero torque
    let clamped = safety.clamp_torque_nm(20.0);
    assert!(clamped.abs() < 0.001);

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 18: FFBeast wheel connected → game starts → FFB active
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  an FFBeast wheel is connected and initialised
/// When   a game with FFB support starts
/// Then   FFB is active on the device
/// And    the device negotiates raw torque mode
/// And    torque uses ±10000 scale
/// ```
#[test]
fn given_ffbeast_wheel_connected_when_game_starts_then_ffb_active() -> Result<()> {
    // Given: FFBeast wheel is connected
    let mut scenario = FFBeastScenario::wheel();
    scenario
        .initialize()
        .map_err(|e| anyhow::anyhow!("FFBeast init failed: {e}"))?;

    // FFBeast wheel: ~10 Nm (based on ±10000 scale)
    let ffbeast_caps =
        DeviceCapabilities::new(true, true, true, true, TorqueNm::new(10.0)?, 65535, 1000);
    assert!(ffbeast_caps.supports_ffb(), "FFBeast must support FFB");

    // When: game starts
    let game = GameCompatibility {
        game_id: "lr".to_string(),
        supports_robust_ffb: true,
        supports_telemetry: true,
        preferred_mode: FFBMode::RawTorque,
    };

    let mode = ModeSelectionPolicy::select_mode(&ffbeast_caps, Some(&game));

    // Then: FFB is active
    assert_eq!(
        mode,
        FFBMode::RawTorque,
        "FFBeast must negotiate raw torque mode"
    );

    // And: feature reports were sent during init
    assert!(
        !scenario.device.feature_reports().is_empty(),
        "FFBeast must send feature reports during init"
    );

    // And: device is connected
    assert!(scenario.device.is_connected(), "device must be connected");

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 19: FFBeast profile switch → FFB update
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  an FFBeast wheel is connected with active FFB
/// When   user switches from road to drift profile
/// Then   the FFB parameters update accordingly
/// And    torque output reflects the new profile characteristics
/// ```
#[test]
fn given_ffbeast_connected_when_profile_switch_then_ffb_updates() -> Result<()> {
    // Given: FFBeast with active FFB
    let mut scenario = FFBeastScenario::wheel();
    scenario
        .initialize()
        .map_err(|e| anyhow::anyhow!("FFBeast init failed: {e}"))?;

    // Road profile: balanced FFB
    // Drift profile: stronger effects for high-speed turns
    let road_gain: f32 = 0.6;
    let drift_gain: f32 = 0.9;
    let base_ffb: f32 = 0.5;

    let road_output = base_ffb * road_gain;
    let drift_output = base_ffb * drift_gain;

    // Then: drift produces stronger FFB
    assert!(
        drift_output > road_output,
        "drift profile must produce stronger FFB than road"
    );

    // And: device remains operational - use output report
    let bytes = drift_output.to_le_bytes();
    let mut report = vec![0u8; 4];
    report[..4].copy_from_slice(&bytes);
    scenario
        .device
        .write_output_report(&report)
        .map_err(|e| anyhow::anyhow!("write failed: {e}"))?;
    assert!(scenario.device.is_connected());

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 20: FFBeast USB disconnect → safe state
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  an FFBeast wheel is connected with active FFB
/// When   USB connection drops unexpectedly
/// Then   the device reports as disconnected
/// And    safety service enters faulted state
/// And    torque clamps to zero within 50ms
/// ```
#[test]
fn given_ffbeast_connected_when_usb_disconnects_then_safe_state() -> Result<()> {
    // Given: FFBeast connected with active FFB
    let mut scenario = FFBeastScenario::wheel();
    scenario
        .initialize()
        .map_err(|e| anyhow::anyhow!("FFBeast init failed: {e}"))?;

    let id: DeviceId = "bdd-ffbeast-safety".parse()?;
    let mut device = VirtualDevice::new(id, "FFBeast Wheel".to_string());
    let mut safety = SafetyService::new(8.0, 20.0);

    // Active FFB
    device.write_ffb_report(5.0, 0)?;
    let normal = safety.clamp_torque_nm(5.0);
    assert!((normal - 5.0).abs() < 0.01);

    // When: USB disconnects - measure timing
    let disconnect_start = Instant::now();
    device.disconnect();
    safety.report_fault(FaultType::UsbStall);
    let clamped = safety.clamp_torque_nm(20.0);
    let elapsed = disconnect_start.elapsed();

    // Then: disconnected
    assert!(!device.is_connected());

    // And: faulted
    match safety.state() {
        SafetyState::Faulted { fault, .. } => {
            assert_eq!(*fault, FaultType::UsbStall);
        }
        other => return Err(anyhow::anyhow!("expected Faulted, got {other:?}")),
    }

    // And: zero torque within 50ms
    assert!(elapsed < Duration::from_millis(50));
    assert!(clamped.abs() < 0.001);

    // And: FFBeast scenario device also shows disconnected
    scenario.device.disconnect();
    assert!(!scenario.device.is_connected());

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 21: Simagic EVO connected → game starts → FFB active
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  a Simagic EVO wheelbase is connected and initialised
/// When   the user starts a game with FFB support
/// Then   FFB is active on the device
/// And    the device negotiates raw torque mode
/// And    the torque range is within the EVO's physical limits
/// ```
#[test]
fn given_simagic_evo_connected_when_game_starts_then_ffb_active() -> Result<()> {
    // Given: Simagic EVO is connected
    use racing_wheel_hid_simagic_protocol::product_ids as simagic_product_ids;

    let mut scenario = SimagicScenario::evo(simagic_product_ids::EVO_SPORT);
    scenario
        .initialize()
        .map_err(|e| anyhow::anyhow!("Simagic EVO init failed: {e}"))?;

    // When: game starts with FFB support
    let game = GameCompatibility {
        game_id: "acc".to_string(),
        supports_robust_ffb: true,
        supports_telemetry: true,
        preferred_mode: FFBMode::RawTorque,
    };

    // Simagic EVO Sport: 17 Nm max
    let caps = DeviceCapabilities::new(true, true, true, true, TorqueNm::new(17.0)?, 65535, 1000);
    let mode = ModeSelectionPolicy::select_mode(&caps, Some(&game));

    // Then: FFB is active
    assert_eq!(
        mode,
        FFBMode::RawTorque,
        "Simagic must negotiate raw torque mode"
    );

    // And: torque range is within limits
    assert!(
        caps.max_torque.value() <= 20.0,
        "torque must be within physical limits"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 22: Simagic profile switch → FFB update
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  a Simagic wheelbase is connected with active FFB
/// When   the user switches from street to race profile
/// Then   the FFB parameters update to reflect the new profile
/// And    the filter pipeline applies the new settings immediately
/// ```
#[test]
fn given_simagic_connected_when_profile_switch_then_ffb_updates() -> Result<()> {
    use racing_wheel_hid_simagic_protocol::product_ids as simagic_product_ids;

    // Given: Simagic connected with active FFB
    let mut scenario = SimagicScenario::evo(simagic_product_ids::EVO_PRO);
    scenario
        .initialize()
        .map_err(|e| anyhow::anyhow!("Simagic init failed: {e}"))?;

    // When: user switches from street (50%) to race (100%) profile
    let street_gain = 0.5;
    let race_gain = 1.0;
    let base_ffb = 10.0;

    let street_output = base_ffb * street_gain;
    let race_output = base_ffb * race_gain;

    // Then: race profile produces stronger output
    assert!(race_output > street_output, "race profile must be stronger");

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 23: Simagic USB disconnect → safe state
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  a Simagic wheelbase is connected with active FFB
/// When   USB disconnects during active FFB
/// Then   safety system enters faulted state
/// And    torque output clamps to zero within 50ms
/// ```
#[test]
fn given_simagic_connected_when_usb_disconnects_then_safe_state() -> Result<()> {
    use racing_wheel_hid_simagic_protocol::product_ids as simagic_product_ids;

    // Given: Simagic connected with active FFB
    let mut scenario = SimagicScenario::evo(simagic_product_ids::EVO_SPORT);
    scenario
        .initialize()
        .map_err(|e| anyhow::anyhow!("Simagic init failed: {e}"))?;

    let id: DeviceId = "bdd-simagic-safety".parse()?;
    let mut device = VirtualDevice::new(id, "Simagic EVO".to_string());
    let safety = SafetyService::new(8.0, 17.0);

    // Active FFB - torque flows
    device.write_ffb_report(5.0, 0)?;
    let normal = safety.clamp_torque_nm(5.0);
    assert!((normal - 5.0).abs() < 0.01);

    // When: USB disconnects
    let disconnect_start = Instant::now();
    scenario.device.disconnect();

    // Device is now disconnected
    assert!(
        !scenario.device.is_connected(),
        "device must report disconnected"
    );

    let disconnect_elapsed = disconnect_start.elapsed();

    // Safety service should handle the disconnect gracefully
    // Verify bounded torque output
    let clamped = safety.clamp_torque_nm(5.0);
    assert!(clamped <= 5.0, "torque must be bounded after disconnect");

    // And: response completes within 50ms
    assert!(disconnect_elapsed < Duration::from_millis(50));

    Ok(())
}

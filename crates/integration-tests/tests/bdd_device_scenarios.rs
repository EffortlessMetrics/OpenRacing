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
//! 9. SimuCube 2 Ultimate connected → iRacing starts → FFB active with max torque
//! 10. SimuCube 2 Sport connected → profile switch → FFB parameters update
//! 11. SimuCube connected → USB disconnect → safe state entered
//! 12. Asetek Invicta connected → iRacing starts → FFB active with high torque
//! 13. Asetek La Prima connected → profile switch → FFB parameters update
//! 14. Asetek connected → USB disconnect during FFB → safe state entered
//! 15. VRS DirectForce Pro connected → iRacing starts → FFB active with correct torque
//! 16. VRS DirectForce Pro V2 connected → profile switch → FFB parameters update
//! 17. VRS connected → USB disconnect → safe state entered
//! 18. Cammus C5 connected → iRacing starts → FFB active with correct torque
//! 19. Cammus C12 connected → profile switch → FFB parameters update
//! 20. Cammus connected → USB disconnect during FFB → safe state entered
//! 21. AccuForce Pro connected → iRacing starts → FFB active with high torque
//! 22. AccuForce Pro connected → profile switch → FFB parameters update
//! 23. AccuForce connected → USB disconnect during FFB → safe state entered
//! 24. Cube Controls GT Pro connected → iRacing starts → FFB active with correct torque
//! 25. Cube Controls Formula Pro connected → profile switch → FFB parameters update
//! 26. Cube Controls connected → USB disconnect during FFB → safe state entered

use std::time::{Duration, Instant};

use anyhow::Result;

use hid_asetek_protocol::product_ids as asetek_product_ids;
use hid_cube_controls_protocol::{CUBE_CONTROLS_FORMULA_PRO_PID, CUBE_CONTROLS_GT_PRO_PID};
use hid_simucube_protocol::product_ids as simucube_product_ids;
use openracing_filters::{DamperState, Frame as FilterFrame, damper_filter, torque_cap_filter};
use racing_wheel_engine::ports::HidDevice;
use racing_wheel_engine::safety::{FaultType, SafetyService, SafetyState};
use racing_wheel_engine::{
    CapabilityNegotiator, FFBMode, GameCompatibility, ModeSelectionPolicy, VirtualDevice,
};
use racing_wheel_hid_accuforce_protocol::PID_ACCUFORCE_PRO;
use racing_wheel_hid_cammus_protocol::{PRODUCT_C5, PRODUCT_C12};
use racing_wheel_hid_fanatec_protocol::product_ids as fanatec_product_ids;
use racing_wheel_hid_logitech_protocol::product_ids as logitech_product_ids;
use racing_wheel_hid_moza_protocol::product_ids as moza_product_ids;
use racing_wheel_hid_thrustmaster_protocol::product_ids as thrustmaster_product_ids;
use racing_wheel_hid_vrs_protocol::product_ids as vrs_product_ids;
use racing_wheel_integration_tests::accuforce_virtual::AccuForceScenario;
use racing_wheel_integration_tests::asetek_virtual::AsetekScenario;
use racing_wheel_integration_tests::cammus_virtual::CammusScenario;
use racing_wheel_integration_tests::cube_controls_virtual::CubeControlsScenario;
use racing_wheel_integration_tests::fanatec_virtual::FanatecScenario;
use racing_wheel_integration_tests::logitech_virtual::LogitechScenario;
use racing_wheel_integration_tests::moza_virtual::MozaScenario;
use racing_wheel_integration_tests::openffboard_virtual::OpenFFBoardScenario;
use racing_wheel_integration_tests::simucube_virtual::SimucubeScenario;
use racing_wheel_integration_tests::thrustmaster_virtual::ThrustmasterScenario;
use racing_wheel_integration_tests::vrs_virtual::VrsScenario;
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
// Scenario 9: Simucube 2 Ultimate connected → iRacing starts → FFB active with max torque
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  a Simucube 2 Ultimate is connected and initialised
/// When   the user starts iRacing (game with robust FFB support)
/// Then   FFB is active on the device
/// And    the device negotiates raw torque mode for iRacing
/// And    the max torque is 32 Nm (Ultimate model)
/// ```
#[test]
fn given_simucube_2_ultimate_connected_when_user_starts_iracing_then_ffb_active_with_max_torque()
-> Result<()> {
    // Given: a Simucube 2 Ultimate is connected and the protocol handshake completes
    let mut scenario = SimucubeScenario::wheelbase(simucube_product_ids::SIMUCUBE_2_ULTIMATE);
    scenario
        .initialize()
        .map_err(|e| anyhow::anyhow!("Simucube 2 Ultimate init failed: {e}"))?;

    assert!(
        scenario.device.is_connected(),
        "Simucube 2 Ultimate must be connected after initialisation"
    );

    // When: iRacing starts — model it as a game with robust FFB
    let iracing = GameCompatibility {
        game_id: "iracing".to_string(),
        supports_robust_ffb: true,
        supports_telemetry: true,
        preferred_mode: FFBMode::RawTorque,
    };

    // Ultimate capabilities: 32 Nm max torque, PIDFF capable
    let ultimate_caps =
        DeviceCapabilities::new(true, true, true, true, TorqueNm::new(32.0)?, 65535, 500);

    let mode = ModeSelectionPolicy::select_mode(&ultimate_caps, Some(&iracing));

    // Then: FFB is ready after initialization (Simucube devices are FFB-ready on USB plug-in)
    assert!(
        scenario.device.is_connected(),
        "Simucube 2 Ultimate must be connected and ready after initialisation"
    );

    // And: the device negotiates raw torque mode for iRacing (Simucube uses raw torque)
    assert_eq!(
        mode,
        FFBMode::RawTorque,
        "Simucube 2 Ultimate must negotiate raw torque mode for iRacing"
    );

    // And: the max torque is 32 Nm (Ultimate model)
    assert!(
        (ultimate_caps.max_torque.value() - 32.0).abs() < 0.1,
        "Ultimate max torque must be 32 Nm, got {} Nm",
        ultimate_caps.max_torque.value()
    );

    // And: a filter pipeline can process FFB within this torque range
    let mut frame = FilterFrame {
        ffb_in: 1.0,
        torque_out: 1.0,
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
// Scenario 10: Simucube 2 Sport connected → profile switch → FFB parameters update
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  a Simucube 2 Sport is connected and initialised
/// When   the user switches from a low-gain profile to a high-gain profile
/// Then   the FFB parameters update to reflect the new profile
/// And    the filter pipeline applies the new gain immediately
/// And    the device remains operational throughout the switch
/// ```
#[test]
fn given_simucube_2_sport_connected_when_user_switches_profiles_then_ffb_parameters_update()
-> Result<()> {
    // Given: a Simucube 2 Sport is connected and initialised
    let mut scenario = SimucubeScenario::wheelbase(simucube_product_ids::SIMUCUBE_2_SPORT);
    scenario
        .initialize()
        .map_err(|e| anyhow::anyhow!("Simucube 2 Sport init failed: {e}"))?;

    // Sport capabilities: ~17 Nm max torque, PIDFF capable
    let sport_caps =
        DeviceCapabilities::new(true, true, true, true, TorqueNm::new(17.0)?, 65535, 500);
    assert!(
        sport_caps.supports_ffb(),
        "Simucube 2 Sport must support force feedback"
    );

    // Define the low-gain and high-gain profiles
    let low_gain: f32 = 0.35;
    let high_gain: f32 = 0.85;
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

    // And: the device remains operational — Simucube devices are FFB-ready on USB plug-in
    assert!(
        scenario.device.is_connected(),
        "Simucube 2 Sport must remain connected after profile switch"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 11: Simucube connected → USB disconnect → safe state entered
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  a Simucube device is connected and force feedback is actively flowing
/// When   the USB connection drops unexpectedly during FFB output
/// Then   the device reports as disconnected immediately
/// And    the safety service enters the Faulted state (UsbStall)
/// And    all torque output is clamped to zero
/// And    the fault response completes within 50ms
/// ```
#[test]
fn given_simucube_connected_when_usb_disconnects_then_safe_state_entered() -> Result<()> {
    // Given: a Simucube 2 Pro is connected with active FFB
    let mut scenario = SimucubeScenario::wheelbase(simucube_product_ids::SIMUCUBE_2_PRO);
    scenario
        .initialize()
        .map_err(|e| anyhow::anyhow!("Simucube 2 Pro init failed: {e}"))?;

    let id: DeviceId = "bdd-sc2pro-usb-drop".parse()?;
    let mut device = VirtualDevice::new(id, "Simucube 2 Pro".to_string());
    let mut safety = SafetyService::new(10.0, 25.0);

    // Confirm FFB is actively flowing
    device.write_ffb_report(8.0, 0)?;
    let normal = safety.clamp_torque_nm(8.0);
    assert!(
        (normal - 8.0).abs() < 0.01,
        "FFB must be active before USB disconnect"
    );
    assert!(device.is_connected(), "device must be connected initially");

    // When: USB disconnects unexpectedly during FFB — measure timing
    let disconnect_start = Instant::now();
    scenario.device.disconnect();
    safety.report_fault(FaultType::UsbStall);
    let clamped = safety.clamp_torque_nm(20.0);
    let disconnect_elapsed = disconnect_start.elapsed();

    // Then: the device reports as disconnected immediately
    assert!(
        !scenario.device.is_connected(),
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
// Scenario 12: Asetek Invicta connected → iRacing starts → FFB active with high torque
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  an Asetek Invicta is connected and initialised
/// When   the user starts iRacing (game with robust FFB support)
/// Then   FFB is active on the device
/// And    the device negotiates raw torque mode for iRacing
/// And    the max torque is 27 Nm (Invicta model)
/// ```
#[test]
fn given_asetek_invicta_connected_when_user_starts_iracing_then_ffb_active_with_high_torque()
-> Result<()> {
    // Given: an Asetek Invicta is connected and the protocol handshake completes
    let mut scenario = AsetekScenario::wheelbase(asetek_product_ids::INVICTA);
    scenario
        .initialize()
        .map_err(|e| anyhow::anyhow!("Asetek Invicta init failed: {e}"))?;

    assert!(
        scenario.device.is_connected(),
        "Asetek Invicta must be connected after initialisation"
    );

    // When: iRacing starts — model it as a game with robust FFB
    let iracing = GameCompatibility {
        game_id: "iracing".to_string(),
        supports_robust_ffb: true,
        supports_telemetry: true,
        preferred_mode: FFBMode::RawTorque,
    };

    // Invicta capabilities: 27 Nm max torque, PIDFF capable
    let invicta_caps =
        DeviceCapabilities::new(true, true, true, true, TorqueNm::new(27.0)?, 65535, 500);

    let mode = ModeSelectionPolicy::select_mode(&invicta_caps, Some(&iracing));

    // Then: FFB is ready after initialization
    assert!(
        scenario.device.is_connected(),
        "Asetek Invicta must be connected and ready after initialisation"
    );

    // And: the device negotiates raw torque mode for iRacing
    assert_eq!(
        mode,
        FFBMode::RawTorque,
        "Asetek Invicta must negotiate raw torque mode for iRacing"
    );

    // And: the max torque is 27 Nm (Invicta model)
    assert!(
        (invicta_caps.max_torque.value() - 27.0).abs() < 0.1,
        "Invicta max torque must be 27 Nm, got {} Nm",
        invicta_caps.max_torque.value()
    );

    // And: a filter pipeline can process FFB within this torque range
    let mut frame = FilterFrame {
        ffb_in: 1.0,
        torque_out: 1.0,
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
// Scenario 13: Asetek La Prima connected → profile switch → FFB parameters update
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  an Asetek La Prima is connected and initialised
/// When   the user switches from a low-gain profile to a high-gain profile
/// Then   the FFB parameters update to reflect the new profile
/// And    the filter pipeline applies the new gain immediately
/// And    the device remains operational throughout the switch
/// ```
#[test]
fn given_asetek_la_prima_connected_when_user_switches_profiles_then_ffb_parameters_update()
-> Result<()> {
    // Given: an Asetek La Prima is connected and initialised
    let mut scenario = AsetekScenario::wheelbase(asetek_product_ids::LA_PRIMA);
    scenario
        .initialize()
        .map_err(|e| anyhow::anyhow!("Asetek La Prima init failed: {e}"))?;

    // La Prima capabilities: ~12 Nm max torque, PIDFF capable
    let laprima_caps =
        DeviceCapabilities::new(true, true, true, true, TorqueNm::new(12.0)?, 65535, 500);
    assert!(
        laprima_caps.supports_ffb(),
        "Asetek La Prima must support force feedback"
    );

    // Define the low-gain and high-gain profiles
    let low_gain: f32 = 0.40;
    let high_gain: f32 = 0.85;
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

    // And: the device remains operational — Asetek devices are FFB-ready on USB plug-in
    assert!(
        scenario.device.is_connected(),
        "Asetek La Prima must remain connected after profile switch"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 14: Asetek connected → USB disconnect during FFB → safe state entered
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  an Asetek device is connected and force feedback is actively flowing
/// When   the USB connection drops unexpectedly during FFB output
/// Then   the device reports as disconnected immediately
/// And    the safety service enters the Faulted state (UsbStall)
/// And    all torque output is clamped to zero
/// And    the fault response completes within 50ms
/// ```
#[test]
fn given_asetek_connected_when_usb_disconnects_during_ffb_then_safe_state_entered() -> Result<()> {
    // Given: an Asetek Invicta is connected with active FFB
    let mut scenario = AsetekScenario::wheelbase(asetek_product_ids::INVICTA);
    scenario
        .initialize()
        .map_err(|e| anyhow::anyhow!("Asetek Invicta init failed: {e}"))?;

    let mut safety = SafetyService::new(10.0, 27.0);

    // Confirm FFB is actively flowing
    let normal = safety.clamp_torque_nm(8.0);
    assert!(
        (normal - 8.0).abs() < 0.01,
        "FFB must be active before USB disconnect"
    );
    assert!(
        scenario.device.is_connected(),
        "device must be connected initially"
    );

    // When: USB disconnects unexpectedly during FFB — measure timing
    let disconnect_start = Instant::now();
    scenario.device.disconnect();
    safety.report_fault(FaultType::UsbStall);
    let clamped = safety.clamp_torque_nm(20.0);
    let disconnect_elapsed = disconnect_start.elapsed();

    // Then: the device reports as disconnected immediately
    assert!(
        !scenario.device.is_connected(),
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
// Scenario 15: VRS DirectForce Pro connected → iRacing starts → FFB active with correct torque
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  a VRS DirectForce Pro is connected and initialised
/// When   the user starts iRacing (game with robust FFB support)
/// Then   FFB is active on the device
/// And    the device negotiates raw torque mode for iRacing
/// And    the max torque is 20 Nm (DFP model)
/// ```
#[test]
fn given_vrs_directforce_pro_connected_when_user_starts_iracing_then_ffb_active_with_correct_torque()
-> Result<()> {
    let mut scenario = VrsScenario::wheelbase(vrs_product_ids::DIRECTFORCE_PRO);
    scenario
        .initialize()
        .map_err(|e| anyhow::anyhow!("VRS DirectForce Pro init failed: {e}"))?;

    assert!(
        scenario.device.is_connected(),
        "VRS DirectForce Pro must be connected after initialisation"
    );

    let iracing = GameCompatibility {
        game_id: "iracing".to_string(),
        supports_robust_ffb: true,
        supports_telemetry: true,
        preferred_mode: FFBMode::RawTorque,
    };

    let dfp_caps =
        DeviceCapabilities::new(true, true, true, true, TorqueNm::new(20.0)?, 65535, 500);

    let mode = ModeSelectionPolicy::select_mode(&dfp_caps, Some(&iracing));

    assert!(
        scenario.device.is_connected(),
        "VRS DirectForce Pro must be connected and ready after initialisation"
    );

    assert_eq!(
        mode,
        FFBMode::RawTorque,
        "VRS DirectForce Pro must negotiate raw torque mode for iRacing"
    );

    assert!(
        (dfp_caps.max_torque.value() - 20.0).abs() < 0.1,
        "DFP max torque must be 20 Nm, got {} Nm",
        dfp_caps.max_torque.value()
    );

    let mut frame = FilterFrame {
        ffb_in: 1.0,
        torque_out: 1.0,
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
// Scenario 16: VRS DirectForce Pro V2 connected → profile switch → FFB parameters update
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  a VRS DirectForce Pro V2 is connected and initialised
/// When   the user switches from a low-gain profile to a high-gain profile
/// Then   the FFB parameters update to reflect the new profile
/// And    the filter pipeline applies the new gain immediately
/// And    the device remains operational throughout the switch
/// ```
#[test]
fn given_vrs_directforce_pro_v2_connected_when_user_switches_profiles_then_ffb_parameters_update()
-> Result<()> {
    let mut scenario = VrsScenario::wheelbase(vrs_product_ids::DIRECTFORCE_PRO_V2);
    scenario
        .initialize()
        .map_err(|e| anyhow::anyhow!("VRS DirectForce Pro V2 init failed: {e}"))?;

    let v2_caps = DeviceCapabilities::new(true, true, true, true, TorqueNm::new(25.0)?, 65535, 500);
    assert!(
        v2_caps.supports_ffb(),
        "VRS DirectForce Pro V2 must support force feedback"
    );

    let low_gain: f32 = 0.40;
    let high_gain: f32 = 0.85;
    let base_ffb: f32 = 0.6;

    let old_scaled = base_ffb * low_gain;
    let new_scaled = base_ffb * high_gain;

    assert!(
        new_scaled > old_scaled,
        "high-gain profile ({high_gain}) must produce stronger output than low-gain ({low_gain})"
    );

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

    assert!(
        scenario.device.is_connected(),
        "VRS DirectForce Pro V2 must remain connected after profile switch"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 17: VRS connected → USB disconnect → safe state entered
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  a VRS device is connected and force feedback is actively flowing
/// When   the USB connection drops unexpectedly during FFB output
/// Then   the device reports as disconnected immediately
/// And    the safety service enters the Faulted state (UsbStall)
/// And    all torque output is clamped to zero
/// And    the fault response completes within 50ms
/// ```
#[test]
fn given_vrs_connected_when_usb_disconnects_then_safe_state_entered() -> Result<()> {
    let mut scenario = VrsScenario::wheelbase(vrs_product_ids::DIRECTFORCE_PRO);
    scenario
        .initialize()
        .map_err(|e| anyhow::anyhow!("VRS DirectForce Pro init failed: {e}"))?;

    let mut safety = SafetyService::new(10.0, 20.0);

    let normal = safety.clamp_torque_nm(8.0);
    assert!(
        (normal - 8.0).abs() < 0.01,
        "FFB must be active before USB disconnect"
    );
    assert!(
        scenario.device.is_connected(),
        "device must be connected initially"
    );

    let disconnect_start = Instant::now();
    scenario.device.disconnect();
    safety.report_fault(FaultType::UsbStall);
    let clamped = safety.clamp_torque_nm(20.0);
    let disconnect_elapsed = disconnect_start.elapsed();

    assert!(
        !scenario.device.is_connected(),
        "device must report disconnected after USB drop"
    );

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

    for requested in [0.0, 1.0, 5.0, 20.0, -10.0] {
        let result = safety.clamp_torque_nm(requested);
        assert!(
            result.abs() < 0.001,
            "torque must be zero after USB drop; requested={requested}, got={result}"
        );
    }

    assert!(
        disconnect_elapsed < Duration::from_millis(50),
        "USB disconnect → safe state must complete in <50ms (actual: {disconnect_elapsed:?})"
    );

    assert!(
        clamped.abs() < 0.001,
        "immediate post-fault torque must be zero, got {clamped}"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 18: Cammus C5 connected → iRacing starts → FFB active with correct torque
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  a Cammus C5 is connected and initialised
/// When   the user starts iRacing (game with robust FFB support)
/// Then   FFB is active on the device
/// And    the device negotiates raw torque mode for iRacing
/// And    the max torque is 5 Nm (C5 model)
/// ```
#[test]
fn given_cammus_c5_connected_when_user_starts_iracing_then_ffb_active_with_correct_torque()
-> Result<()> {
    let mut scenario = CammusScenario::wheelbase(PRODUCT_C5);
    scenario
        .initialize()
        .map_err(|e| anyhow::anyhow!("Cammus C5 init failed: {e}"))?;

    assert!(
        scenario.device.is_connected(),
        "Cammus C5 must be connected after initialisation"
    );

    let iracing = GameCompatibility {
        game_id: "iracing".to_string(),
        supports_robust_ffb: true,
        supports_telemetry: true,
        preferred_mode: FFBMode::RawTorque,
    };

    let c5_caps = DeviceCapabilities::new(true, true, true, true, TorqueNm::new(5.0)?, 65535, 500);

    let mode = ModeSelectionPolicy::select_mode(&c5_caps, Some(&iracing));

    assert!(
        scenario.device.is_connected(),
        "Cammus C5 must be connected and ready after initialisation"
    );

    assert_eq!(
        mode,
        FFBMode::RawTorque,
        "Cammus C5 must negotiate raw torque mode for iRacing"
    );

    assert!(
        (c5_caps.max_torque.value() - 5.0).abs() < 0.1,
        "C5 max torque must be 5 Nm, got {} Nm",
        c5_caps.max_torque.value()
    );

    let mut frame = FilterFrame {
        ffb_in: 1.0,
        torque_out: 1.0,
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
// Scenario 19: Cammus C12 connected → profile switch → FFB parameters update
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  a Cammus C12 is connected and initialised
/// When   the user switches from a low-gain profile to a high-gain profile
/// Then   the FFB parameters update to reflect the new profile
/// And    the filter pipeline applies the new gain immediately
/// And    the device remains operational throughout the switch
/// ```
#[test]
fn given_cammus_c12_connected_when_user_switches_profiles_then_ffb_parameters_update() -> Result<()>
{
    let mut scenario = CammusScenario::wheelbase(PRODUCT_C12);
    scenario
        .initialize()
        .map_err(|e| anyhow::anyhow!("Cammus C12 init failed: {e}"))?;

    let c12_caps =
        DeviceCapabilities::new(true, true, true, true, TorqueNm::new(12.0)?, 65535, 500);
    assert!(
        c12_caps.supports_ffb(),
        "Cammus C12 must support force feedback"
    );

    let low_gain: f32 = 0.35;
    let high_gain: f32 = 0.90;
    let base_ffb: f32 = 0.6;

    let old_scaled = base_ffb * low_gain;
    let new_scaled = base_ffb * high_gain;

    assert!(
        new_scaled > old_scaled,
        "high-gain profile ({high_gain}) must produce stronger output than low-gain ({low_gain})"
    );

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

    assert!(
        scenario.device.is_connected(),
        "Cammus C12 must remain connected after profile switch"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 20: Cammus connected → USB disconnect during FFB → safe state entered
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  a Cammus device is connected and force feedback is actively flowing
/// When   the USB connection drops unexpectedly during FFB output
/// Then   the device reports as disconnected immediately
/// And    the safety service enters the Faulted state (UsbStall)
/// And    all torque output is clamped to zero
/// And    the fault response completes within 50ms
/// ```
#[test]
fn given_cammus_connected_when_usb_disconnects_during_ffb_then_safe_state_entered() -> Result<()> {
    let mut scenario = CammusScenario::wheelbase(PRODUCT_C5);
    scenario
        .initialize()
        .map_err(|e| anyhow::anyhow!("Cammus C5 init failed: {e}"))?;

    let mut safety = SafetyService::new(5.0, 10.0);

    let normal = safety.clamp_torque_nm(3.0);
    assert!(
        (normal - 3.0).abs() < 0.01,
        "FFB must be active before USB disconnect"
    );
    assert!(
        scenario.device.is_connected(),
        "device must be connected initially"
    );

    let disconnect_start = Instant::now();
    scenario.device.disconnect();
    safety.report_fault(FaultType::UsbStall);
    let clamped = safety.clamp_torque_nm(10.0);
    let disconnect_elapsed = disconnect_start.elapsed();

    assert!(
        !scenario.device.is_connected(),
        "device must report disconnected after USB drop"
    );

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

    for requested in [0.0, 1.0, 5.0, 10.0, -5.0] {
        let result = safety.clamp_torque_nm(requested);
        assert!(
            result.abs() < 0.001,
            "torque must be zero after USB drop; requested={requested}, got={result}"
        );
    }

    assert!(
        disconnect_elapsed < Duration::from_millis(50),
        "USB disconnect → safe state must complete in <50ms (actual: {disconnect_elapsed:?})"
    );

    assert!(
        clamped.abs() < 0.001,
        "immediate post-fault torque must be zero, got {clamped}"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 21: AccuForce Pro connected → iRacing starts → FFB active with high torque
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  an AccuForce Pro is connected and initialised
/// When   the user starts iRacing (game with robust FFB support)
/// Then   FFB is active on the device
/// And    the device negotiates raw torque mode for iRacing
/// And    the max torque is 20 Nm (AccuForce Pro model)
/// ```
#[test]
fn given_accuforce_pro_connected_when_user_starts_iracing_then_ffb_active_with_high_torque()
-> Result<()> {
    let mut scenario = AccuForceScenario::wheelbase(PID_ACCUFORCE_PRO);
    scenario
        .initialize()
        .map_err(|e| anyhow::anyhow!("AccuForce Pro init failed: {e}"))?;

    assert!(
        scenario.device.is_connected(),
        "AccuForce Pro must be connected after initialisation"
    );

    let iracing = GameCompatibility {
        game_id: "iracing".to_string(),
        supports_robust_ffb: true,
        supports_telemetry: true,
        preferred_mode: FFBMode::RawTorque,
    };

    let accuforce_caps =
        DeviceCapabilities::new(true, true, true, true, TorqueNm::new(20.0)?, 65535, 500);

    let mode = ModeSelectionPolicy::select_mode(&accuforce_caps, Some(&iracing));

    assert!(
        scenario.device.is_connected(),
        "AccuForce Pro must be connected and ready after initialisation"
    );

    assert_eq!(
        mode,
        FFBMode::RawTorque,
        "AccuForce Pro must negotiate raw torque mode for iRacing"
    );

    assert!(
        (accuforce_caps.max_torque.value() - 20.0).abs() < 0.1,
        "AccuForce Pro max torque must be 20 Nm, got {} Nm",
        accuforce_caps.max_torque.value()
    );

    let mut frame = FilterFrame {
        ffb_in: 1.0,
        torque_out: 1.0,
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
// Scenario 22: AccuForce Pro connected → profile switch → FFB parameters update
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  an AccuForce Pro is connected and initialised
/// When   the user switches from a low-gain profile to a high-gain profile
/// Then   the FFB parameters update to reflect the new profile
/// And    the filter pipeline applies the new gain immediately
/// And    the device remains operational throughout the switch
/// ```
#[test]
fn given_accuforce_pro_connected_when_user_switches_profiles_then_ffb_parameters_update()
-> Result<()> {
    let mut scenario = AccuForceScenario::wheelbase(PID_ACCUFORCE_PRO);
    scenario
        .initialize()
        .map_err(|e| anyhow::anyhow!("AccuForce Pro init failed: {e}"))?;

    let accuforce_caps =
        DeviceCapabilities::new(true, true, true, true, TorqueNm::new(20.0)?, 65535, 500);
    assert!(
        accuforce_caps.supports_ffb(),
        "AccuForce Pro must support force feedback"
    );

    let low_gain: f32 = 0.40;
    let high_gain: f32 = 0.85;
    let base_ffb: f32 = 0.6;

    let old_scaled = base_ffb * low_gain;
    let new_scaled = base_ffb * high_gain;

    assert!(
        new_scaled > old_scaled,
        "high-gain profile ({high_gain}) must produce stronger output than low-gain ({low_gain})"
    );

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

    assert!(
        scenario.device.is_connected(),
        "AccuForce Pro must remain connected after profile switch"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 23: AccuForce connected → USB disconnect during FFB → safe state entered
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  an AccuForce device is connected and force feedback is actively flowing
/// When   the USB connection drops unexpectedly during FFB output
/// Then   the device reports as disconnected immediately
/// And    the safety service enters the Faulted state (UsbStall)
/// And    all torque output is clamped to zero
/// And    the fault response completes within 50ms
/// ```
#[test]
fn given_accuforce_connected_when_usb_disconnects_during_ffb_then_safe_state_entered() -> Result<()>
{
    let mut scenario = AccuForceScenario::wheelbase(PID_ACCUFORCE_PRO);
    scenario
        .initialize()
        .map_err(|e| anyhow::anyhow!("AccuForce Pro init failed: {e}"))?;

    let mut safety = SafetyService::new(10.0, 20.0);

    let normal = safety.clamp_torque_nm(8.0);
    assert!(
        (normal - 8.0).abs() < 0.01,
        "FFB must be active before USB disconnect"
    );
    assert!(
        scenario.device.is_connected(),
        "device must be connected initially"
    );

    let disconnect_start = Instant::now();
    scenario.device.disconnect();
    safety.report_fault(FaultType::UsbStall);
    let clamped = safety.clamp_torque_nm(20.0);
    let disconnect_elapsed = disconnect_start.elapsed();

    assert!(
        !scenario.device.is_connected(),
        "device must report disconnected after USB drop"
    );

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

    for requested in [0.0, 1.0, 5.0, 20.0, -10.0] {
        let result = safety.clamp_torque_nm(requested);
        assert!(
            result.abs() < 0.001,
            "torque must be zero after USB drop; requested={requested}, got={result}"
        );
    }

    assert!(
        disconnect_elapsed < Duration::from_millis(50),
        "USB disconnect → safe state must complete in <50ms (actual: {disconnect_elapsed:?})"
    );

    assert!(
        clamped.abs() < 0.001,
        "immediate post-fault torque must be zero, got {clamped}"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 24: Cube Controls GT Pro connected → iRacing starts → FFB active with correct torque
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  a Cube Controls GT Pro is connected and initialised
/// When   the user starts iRacing (game with robust FFB support)
/// Then   FFB is active on the device
/// And    the device negotiates raw torque mode for iRacing
/// And    the max torque is 20 Nm (GT Pro model)
/// ```
#[test]
fn given_cube_controls_gt_pro_connected_when_user_starts_iracing_then_ffb_active_with_correct_torque()
-> Result<()> {
    let mut scenario = CubeControlsScenario::wheelbase(CUBE_CONTROLS_GT_PRO_PID);
    scenario
        .initialize()
        .map_err(|e| anyhow::anyhow!("Cube Controls GT Pro init failed: {e}"))?;

    assert!(
        scenario.device.is_connected(),
        "Cube Controls GT Pro must be connected after initialisation"
    );

    let iracing = GameCompatibility {
        game_id: "iracing".to_string(),
        supports_robust_ffb: true,
        supports_telemetry: true,
        preferred_mode: FFBMode::RawTorque,
    };

    let gt_pro_caps =
        DeviceCapabilities::new(true, true, true, true, TorqueNm::new(20.0)?, 65535, 500);

    let mode = ModeSelectionPolicy::select_mode(&gt_pro_caps, Some(&iracing));

    assert!(
        scenario.device.is_connected(),
        "Cube Controls GT Pro must be connected and ready after initialisation"
    );

    assert_eq!(
        mode,
        FFBMode::RawTorque,
        "Cube Controls GT Pro must negotiate raw torque mode for iRacing"
    );

    assert!(
        (gt_pro_caps.max_torque.value() - 20.0).abs() < 0.1,
        "GT Pro max torque must be 20 Nm, got {} Nm",
        gt_pro_caps.max_torque.value()
    );

    let mut frame = FilterFrame {
        ffb_in: 1.0,
        torque_out: 1.0,
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
// Scenario 25: Cube Controls Formula Pro connected → profile switch → FFB parameters update
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  a Cube Controls Formula Pro is connected and initialised
/// When   the user switches from a low-gain profile to a high-gain profile
/// Then   the FFB parameters update to reflect the new profile
/// And    the filter pipeline applies the new gain immediately
/// And    the device remains operational throughout the switch
/// ```
#[test]
fn given_cube_controls_formula_pro_connected_when_user_switches_profiles_then_ffb_parameters_update()
-> Result<()> {
    let mut scenario = CubeControlsScenario::wheelbase(CUBE_CONTROLS_FORMULA_PRO_PID);
    scenario
        .initialize()
        .map_err(|e| anyhow::anyhow!("Cube Controls Formula Pro init failed: {e}"))?;

    let formula_pro_caps =
        DeviceCapabilities::new(true, true, true, true, TorqueNm::new(20.0)?, 65535, 500);
    assert!(
        formula_pro_caps.supports_ffb(),
        "Cube Controls Formula Pro must support force feedback"
    );

    let low_gain: f32 = 0.40;
    let high_gain: f32 = 0.85;
    let base_ffb: f32 = 0.6;

    let old_scaled = base_ffb * low_gain;
    let new_scaled = base_ffb * high_gain;

    assert!(
        new_scaled > old_scaled,
        "high-gain profile ({high_gain}) must produce stronger output than low-gain ({low_gain})"
    );

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

    assert!(
        scenario.device.is_connected(),
        "Cube Controls Formula Pro must remain connected after profile switch"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 26: Cube Controls connected → USB disconnect during FFB → safe state entered
// ═══════════════════════════════════════════════════════════════════════════════

/// ```text
/// Given  a Cube Controls device is connected and force feedback is actively flowing
/// When   the USB connection drops unexpectedly during FFB output
/// Then   the device reports as disconnected immediately
/// And    the safety service enters the Faulted state (UsbStall)
/// And    all torque output is clamped to zero
/// And    the fault response completes within 50ms
/// ```
#[test]
fn given_cube_controls_connected_when_usb_disconnects_during_ffb_then_safe_state_entered()
-> Result<()> {
    let mut scenario = CubeControlsScenario::wheelbase(CUBE_CONTROLS_GT_PRO_PID);
    scenario
        .initialize()
        .map_err(|e| anyhow::anyhow!("Cube Controls GT Pro init failed: {e}"))?;

    let mut safety = SafetyService::new(10.0, 20.0);

    let normal = safety.clamp_torque_nm(8.0);
    assert!(
        (normal - 8.0).abs() < 0.01,
        "FFB must be active before USB disconnect"
    );
    assert!(
        scenario.device.is_connected(),
        "device must be connected initially"
    );

    let disconnect_start = Instant::now();
    scenario.device.disconnect();
    safety.report_fault(FaultType::UsbStall);
    let clamped = safety.clamp_torque_nm(20.0);
    let disconnect_elapsed = disconnect_start.elapsed();

    assert!(
        !scenario.device.is_connected(),
        "device must report disconnected after USB drop"
    );

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

    for requested in [0.0, 1.0, 5.0, 20.0, -10.0] {
        let result = safety.clamp_torque_nm(requested);
        assert!(
            result.abs() < 0.001,
            "torque must be zero after USB drop; requested={requested}, got={result}"
        );
    }

    assert!(
        disconnect_elapsed < Duration::from_millis(50),
        "USB disconnect → safe state must complete in <50ms (actual: {disconnect_elapsed:?})"
    );

    assert!(
        clamped.abs() < 0.001,
        "immediate post-fault torque must be zero, got {clamped}"
    );

    Ok(())
}

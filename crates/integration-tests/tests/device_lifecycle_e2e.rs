//! End-to-end device lifecycle integration tests.
//!
//! Cross-crate coverage: engine (VirtualDevice, VirtualHidPort, Pipeline, HidDevice)
//! × schemas (DeviceId, DeviceCapabilities, TorqueNm) × service (WheelService, profiles).
//!
//! Scenarios:
//! 1. Full lifecycle: discover → connect → configure → run → disconnect
//! 2. Hot-plug: device appears mid-session
//! 3. Hot-unplug: device disappears during active use
//! 4. Recovery: device reconnects after disconnect

use std::time::Duration;

use anyhow::Result;

use racing_wheel_engine::ports::{HidDevice, HidPort};
use racing_wheel_engine::safety::{FaultType, SafetyService, SafetyState};
use racing_wheel_engine::{
    CapabilityNegotiator, FFBMode, Frame, ModeSelectionPolicy, Pipeline, VirtualDevice,
    VirtualHidPort,
};
use racing_wheel_schemas::prelude::*;

// ═══════════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════════

/// Create a VirtualHidPort pre-loaded with one virtual device and return
/// both the port and the device ID.
fn make_port_with_device(
    id_str: &str,
    name: &str,
) -> Result<(VirtualHidPort, DeviceId)> {
    let id: DeviceId = id_str.parse()?;
    let device = VirtualDevice::new(id.clone(), name.to_string());
    let mut port = VirtualHidPort::new();
    port.add_device(device)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok((port, id))
}

// ═══════════════════════════════════════════════════════════════════════════════
// 1. Full lifecycle: discover → connect → configure → run → disconnect
// ═══════════════════════════════════════════════════════════════════════════════

/// Exercises the full device lifecycle through engine and schemas crates:
/// enumerate from HidPort → inspect DeviceCapabilities → negotiate FFB mode →
/// process frames through Pipeline → write torque → read telemetry → disconnect.
#[tokio::test]
async fn lifecycle_discover_connect_configure_run_disconnect() -> Result<()> {
    // Discover
    let (port, id) = make_port_with_device("lifecycle-full-001", "Lifecycle Wheel")?;
    let devices = port
        .list_devices()
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    assert_eq!(devices.len(), 1, "exactly one device expected");
    assert!(devices[0].is_connected, "device must be connected");

    // Connect (open)
    let mut dev = port
        .open_device(&id)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    // Configure: negotiate FFB mode from capabilities (cross-crate: engine ↔ schemas)
    let caps = dev.capabilities().clone();
    let mode = ModeSelectionPolicy::select_mode(&caps, None);
    assert_eq!(
        mode,
        FFBMode::RawTorque,
        "virtual DD device must negotiate RawTorque"
    );

    // Run: process a frame through the pipeline then write to device
    let mut pipeline = Pipeline::new();
    let mut frame = Frame {
        ffb_in: 0.4,
        torque_out: 0.4,
        wheel_speed: 1.0,
        hands_off: false,
        ts_mono_ns: 1_000_000,
        seq: 1,
    };
    pipeline.process(&mut frame)?;
    assert!(frame.torque_out.is_finite(), "pipeline output must be finite");

    dev.write_ffb_report(frame.torque_out, frame.seq)?;
    let telem = dev
        .read_telemetry()
        .ok_or_else(|| anyhow::anyhow!("telemetry missing while connected"))?;
    assert!(telem.temperature_c <= 150, "temperature in sane range");

    // Disconnect
    // (open_device returns an independent clone – simulate disconnect via a fresh VirtualDevice)
    let id2: DeviceId = "lifecycle-full-001".parse()?;
    let mut standalone = VirtualDevice::new(id2, "Lifecycle Wheel".to_string());
    standalone.disconnect();
    assert!(!standalone.is_connected(), "device must report disconnected");

    // FFB write after disconnect must fail
    let err = standalone.write_ffb_report(1.0, 2);
    assert!(err.is_err(), "FFB write must fail after disconnect");

    Ok(())
}

/// Verifying that capability report round-trips correctly through
/// CapabilityNegotiator (engine crate) and DeviceCapabilities (schemas crate).
#[test]
fn lifecycle_capability_report_round_trip() -> Result<()> {
    let original = DeviceCapabilities::new(
        false,
        true,
        true,
        true,
        TorqueNm::new(25.0)?,
        10000,
        1000,
    );

    let report = CapabilityNegotiator::create_capabilities_report(&original);
    let parsed = CapabilityNegotiator::parse_capabilities_report(&report)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    assert_eq!(
        parsed.supports_raw_torque_1khz,
        original.supports_raw_torque_1khz
    );
    assert_eq!(parsed.supports_pid, original.supports_pid);
    assert_eq!(parsed.supports_health_stream, original.supports_health_stream);
    assert_eq!(parsed.supports_led_bus, original.supports_led_bus);
    assert_eq!(parsed.encoder_cpr, original.encoder_cpr);
    assert_eq!(parsed.min_report_period_us, original.min_report_period_us);

    // Torque round-trip through centi-Newton-meters encoding
    let torque_diff = (parsed.max_torque.value() - original.max_torque.value()).abs();
    assert!(
        torque_diff < 0.01,
        "torque round-trip delta {torque_diff} exceeds 0.01 Nm"
    );

    Ok(())
}

/// Multiple sequential pipeline + device write cycles must not accumulate errors.
#[test]
fn lifecycle_repeated_pipeline_cycles() -> Result<()> {
    let id: DeviceId = "lifecycle-repeat-001".parse()?;
    let mut device = VirtualDevice::new(id, "Repeat Lifecycle Wheel".to_string());
    let mut pipeline = Pipeline::new();

    for seq in 0u16..100 {
        let ffb_in = ((seq as f32) * 0.01).sin() * 0.5;
        let mut frame = Frame {
            ffb_in,
            torque_out: ffb_in,
            wheel_speed: 0.5,
            hands_off: false,
            ts_mono_ns: u64::from(seq) * 1_000_000,
            seq,
        };
        pipeline.process(&mut frame)?;
        assert!(
            frame.torque_out.is_finite(),
            "frame {seq}: torque_out must be finite"
        );

        device.write_ffb_report(frame.torque_out, seq)?;
    }

    let telem = device
        .read_telemetry()
        .ok_or_else(|| anyhow::anyhow!("telemetry missing after 100 cycles"))?;
    assert!(telem.temperature_c <= 150);

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. Hot-plug: device appears mid-session
// ═══════════════════════════════════════════════════════════════════════════════

/// A port starts empty, then a device is added (hot-plug).  The newly added
/// device must be discoverable and fully functional.
#[tokio::test]
async fn hotplug_device_appears_mid_session() -> Result<()> {
    let mut port = VirtualHidPort::new();

    // Session starts with no devices
    let empty = port
        .list_devices()
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    assert!(empty.is_empty(), "port should start empty");

    // Hot-plug a device
    let id: DeviceId = "hotplug-mid-001".parse()?;
    let device = VirtualDevice::new(id.clone(), "Hot-Plugged Wheel".to_string());
    port.add_device(device)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    // Verify it is now listed
    let devices = port
        .list_devices()
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    assert_eq!(devices.len(), 1, "hot-plugged device must appear");
    assert!(devices[0].is_connected);

    // Open and use the device
    let mut dev = port
        .open_device(&id)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    dev.write_ffb_report(1.5, 0)?;
    assert!(
        dev.read_telemetry().is_some(),
        "hot-plugged device must produce telemetry"
    );

    Ok(())
}

/// Hot-plugging a second device while the first is already active must not
/// interfere with the first device's operation.
#[tokio::test]
async fn hotplug_second_device_does_not_disrupt_first() -> Result<()> {
    let (mut port, id_a) =
        make_port_with_device("hotplug-first-001", "First Wheel")?;

    // Open and use the first device
    let mut dev_a = port
        .open_device(&id_a)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    dev_a.write_ffb_report(2.0, 0)?;

    // Hot-plug a second device
    let id_b: DeviceId = "hotplug-second-001".parse()?;
    port.add_device(VirtualDevice::new(id_b.clone(), "Second Wheel".to_string()))
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    // First device is still functional
    dev_a.write_ffb_report(3.0, 1)?;
    assert!(
        dev_a.read_telemetry().is_some(),
        "first device telemetry must remain available after second hot-plug"
    );

    // Both devices appear in enumeration
    let devices = port
        .list_devices()
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    assert_eq!(devices.len(), 2, "both devices must be enumerated");

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. Hot-unplug: device disappears during active use
// ═══════════════════════════════════════════════════════════════════════════════

/// After a device is disconnected mid-FFB, writes must fail cleanly (no panic)
/// and reads must return `None`.
#[test]
fn hotunplug_device_disappears_during_active_use() -> Result<()> {
    let id: DeviceId = "unplug-active-001".parse()?;
    let mut device = VirtualDevice::new(id, "Unplugged Wheel".to_string());

    // Normal operation
    device.write_ffb_report(4.0, 0)?;
    assert!(device.read_telemetry().is_some());

    // Simulate hot-unplug
    device.disconnect();

    // Writes must fail
    let write_result = device.write_ffb_report(4.0, 1);
    assert!(
        write_result.is_err(),
        "FFB write must fail after hot-unplug"
    );

    // Reads must return None
    assert!(
        device.read_telemetry().is_none(),
        "telemetry must be None after hot-unplug"
    );

    // Device reports disconnected
    assert!(!device.is_connected());

    Ok(())
}

/// Safety service must transition to faulted state when a hot-unplug is
/// detected (simulated via FaultType::UsbStall).
#[test]
fn hotunplug_triggers_safety_fault() -> Result<()> {
    let mut safety = SafetyService::new(5.0, 20.0);

    // Normal state
    assert_eq!(safety.state(), &SafetyState::SafeTorque);

    // Simulate hot-unplug by reporting a USB stall fault
    safety.report_fault(FaultType::UsbStall);

    match safety.state() {
        SafetyState::Faulted { fault, .. } => {
            assert_eq!(*fault, FaultType::UsbStall, "fault type must be UsbStall");
        }
        other => {
            return Err(anyhow::anyhow!(
                "expected Faulted state, got {other:?}"
            ));
        }
    }

    // Torque must be clamped to zero in faulted state
    let clamped = safety.clamp_torque_nm(10.0);
    assert!(
        clamped.abs() < 0.001,
        "faulted safety must clamp torque to zero, got {clamped}"
    );

    Ok(())
}

/// Disconnecting one device in a multi-device port must not affect the other.
#[tokio::test]
async fn hotunplug_one_device_other_unaffected() -> Result<()> {
    let mut port = VirtualHidPort::new();

    let id_a: DeviceId = "unplug-pair-a".parse()?;
    let id_b: DeviceId = "unplug-pair-b".parse()?;

    port.add_device(VirtualDevice::new(id_a.clone(), "Wheel A".to_string()))
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    port.add_device(VirtualDevice::new(id_b.clone(), "Wheel B".to_string()))
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let mut dev_a = port
        .open_device(&id_a)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut dev_b = port
        .open_device(&id_b)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    // Both work initially
    dev_a.write_ffb_report(1.0, 0)?;
    dev_b.write_ffb_report(1.0, 0)?;

    // Simulate removing device from port
    port.remove_device(&id_a)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    // Device B must remain fully functional
    assert!(dev_b.is_connected(), "device B must remain connected");
    dev_b.write_ffb_report(2.0, 1)?;
    assert!(
        dev_b.read_telemetry().is_some(),
        "device B telemetry must remain available"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. Recovery: device reconnects after disconnect
// ═══════════════════════════════════════════════════════════════════════════════

/// After disconnect → reconnect, device must be fully functional:
/// FFB writes succeed, telemetry is available, capabilities are unchanged.
#[test]
fn recovery_reconnect_restores_full_functionality() -> Result<()> {
    let id: DeviceId = "recovery-full-001".parse()?;
    let mut device = VirtualDevice::new(id, "Recovery Wheel".to_string());

    // Capture capabilities before disconnect
    let caps_before = device.capabilities().clone();

    // Normal operation
    device.write_ffb_report(3.0, 0)?;

    // Disconnect
    device.disconnect();
    assert!(!device.is_connected());
    assert!(device.write_ffb_report(1.0, 1).is_err());

    // Reconnect
    device.reconnect();
    assert!(device.is_connected());

    // Full functionality restored
    device.write_ffb_report(4.0, 2)?;
    assert!(
        device.read_telemetry().is_some(),
        "telemetry must be available after reconnect"
    );

    // Capabilities unchanged
    let caps_after = device.capabilities().clone();
    assert_eq!(
        caps_before.supports_raw_torque_1khz,
        caps_after.supports_raw_torque_1khz
    );
    assert_eq!(caps_before.encoder_cpr, caps_after.encoder_cpr);

    Ok(())
}

/// Safety service fault can be cleared after the minimum hold period,
/// simulating recovery from a transient USB stall.
#[test]
fn recovery_safety_clears_after_hold_period() -> Result<()> {
    let mut safety = SafetyService::new(5.0, 20.0);

    // Fault
    safety.report_fault(FaultType::UsbStall);
    assert!(matches!(safety.state(), SafetyState::Faulted { .. }));

    // Attempting to clear immediately must fail (hold period not elapsed)
    let early_clear = safety.clear_fault();
    assert!(
        early_clear.is_err(),
        "clearing fault before hold period must fail"
    );

    // Wait for the minimum hold period (100ms per SafetyService implementation)
    std::thread::sleep(Duration::from_millis(120));

    // Now clearing must succeed
    safety
        .clear_fault()
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    assert_eq!(
        safety.state(),
        &SafetyState::SafeTorque,
        "state must return to SafeTorque after clear"
    );

    // Torque is no longer clamped to zero
    let clamped = safety.clamp_torque_nm(3.0);
    assert!(
        (clamped - 3.0).abs() < 0.01,
        "torque must pass through in SafeTorque state, got {clamped}"
    );

    Ok(())
}

/// Multiple disconnect/reconnect cycles must not corrupt device state.
#[test]
fn recovery_repeated_disconnect_reconnect_cycles() -> Result<()> {
    let id: DeviceId = "recovery-cycle-001".parse()?;
    let mut device = VirtualDevice::new(id, "Cycle Wheel".to_string());

    for cycle in 0u16..10 {
        // Connected: write + read succeeds
        device.write_ffb_report(1.0, cycle)?;
        assert!(
            device.read_telemetry().is_some(),
            "cycle {cycle}: telemetry must be available while connected"
        );

        // Disconnect
        device.disconnect();
        assert!(!device.is_connected());
        assert!(device.write_ffb_report(1.0, cycle).is_err());

        // Reconnect
        device.reconnect();
        assert!(device.is_connected());
    }

    // Final functional check
    device.write_ffb_report(5.0, 100)?;
    let telem = device
        .read_telemetry()
        .ok_or_else(|| anyhow::anyhow!("telemetry missing after 10 cycles"))?;
    assert!(telem.temperature_c <= 150);

    Ok(())
}

/// Pipeline + safety + device integration across a disconnect/reconnect
/// boundary: the pipeline must produce valid output and the safety service
/// must allow torque flow after recovery.
#[test]
fn recovery_pipeline_and_safety_across_reconnect() -> Result<()> {
    let id: DeviceId = "recovery-pipeline-001".parse()?;
    let mut device = VirtualDevice::new(id, "Pipeline Recovery Wheel".to_string());
    let mut pipeline = Pipeline::new();
    let mut safety = SafetyService::new(5.0, 20.0);

    // Normal: pipeline → safety clamp → device write
    let mut frame = Frame {
        ffb_in: 0.6,
        torque_out: 0.6,
        wheel_speed: 1.0,
        hands_off: false,
        ts_mono_ns: 1_000_000,
        seq: 0,
    };
    pipeline.process(&mut frame)?;
    let torque_nm = safety.clamp_torque_nm(frame.torque_out * 5.0);
    device.write_ffb_report(torque_nm, frame.seq)?;

    // Simulate disconnect → fault
    device.disconnect();
    safety.report_fault(FaultType::UsbStall);
    assert!(matches!(safety.state(), SafetyState::Faulted { .. }));

    // Wait for hold period then clear fault
    std::thread::sleep(Duration::from_millis(120));
    safety
        .clear_fault()
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    // Reconnect device
    device.reconnect();

    // Pipeline still produces valid output
    let mut frame2 = Frame {
        ffb_in: 0.3,
        torque_out: 0.3,
        wheel_speed: 0.5,
        hands_off: false,
        ts_mono_ns: 2_000_000,
        seq: 1,
    };
    pipeline.process(&mut frame2)?;
    assert!(frame2.torque_out.is_finite());

    let torque_nm2 = safety.clamp_torque_nm(frame2.torque_out * 5.0);
    assert!(
        torque_nm2.abs() > 0.0,
        "safety must allow non-zero torque after recovery"
    );
    device.write_ffb_report(torque_nm2, frame2.seq)?;

    Ok(())
}

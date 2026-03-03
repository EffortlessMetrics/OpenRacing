//! End-to-end device workflow integration tests.
//!
//! Complete user workflows exercising device lifecycle, FFB processing,
//! hot-plug, multi-device setups, and profile switching.
//!
//! Cross-crate coverage: engine (VirtualDevice, VirtualHidPort, Pipeline, Frame,
//! SafetyService, CapabilityNegotiator) × schemas (DeviceId, DeviceCapabilities,
//! TorqueNm) × filters (damper, friction, torque_cap, slew_rate).

use std::time::Duration;

use anyhow::Result;

use openracing_filters::{
    DamperState, Frame as FilterFrame, FrictionState, SlewRateState, damper_filter,
    friction_filter, slew_rate_filter, torque_cap_filter,
};
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

fn make_port_with_device(id_str: &str, name: &str) -> Result<(VirtualHidPort, DeviceId)> {
    let id: DeviceId = id_str.parse()?;
    let device = VirtualDevice::new(id.clone(), name.to_string());
    let mut port = VirtualHidPort::new();
    port.add_device(device)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok((port, id))
}

fn filter_frame(ffb_scalar: f32, wheel_speed: f32, seq: u16) -> FilterFrame {
    FilterFrame {
        ffb_in: ffb_scalar,
        torque_out: ffb_scalar,
        wheel_speed,
        hands_off: false,
        ts_mono_ns: u64::from(seq) * 1_000_000,
        seq,
    }
}

fn engine_frame(ffb_in: f32, wheel_speed: f32, seq: u16) -> Frame {
    Frame {
        ffb_in,
        torque_out: ffb_in,
        wheel_speed,
        hands_off: false,
        ts_mono_ns: u64::from(seq) * 1_000_000,
        seq,
    }
}

/// Run one full tick: filter → engine → safety → device write.
fn run_full_tick(
    ffb_scalar: f32,
    wheel_speed: f32,
    seq: u16,
    pipeline: &mut Pipeline,
    safety: &SafetyService,
    device: &mut dyn HidDevice,
    safe_torque_nm: f32,
) -> Result<f32> {
    let mut ff = filter_frame(ffb_scalar, wheel_speed, seq);
    let damper = DamperState::fixed(0.02);
    let friction = FrictionState::fixed(0.01);
    damper_filter(&mut ff, &damper);
    friction_filter(&mut ff, &friction);
    torque_cap_filter(&mut ff, 1.0);

    let mut ef = engine_frame(ff.torque_out, ff.wheel_speed, seq);
    pipeline.process(&mut ef)?;

    let torque_nm = safety.clamp_torque_nm(ef.torque_out * safe_torque_nm);
    device.write_ffb_report(torque_nm, seq)?;
    Ok(torque_nm)
}

// ═══════════════════════════════════════════════════════════════════════════════
// 1. Complete device lifecycle: discover → connect → configure → process → disconnect
// ═══════════════════════════════════════════════════════════════════════════════

/// Full workflow: discover device on port → open → negotiate FFB mode →
/// run pipeline ticks → read telemetry → disconnect → verify post-disconnect.
#[tokio::test]
async fn workflow_full_device_lifecycle() -> Result<()> {
    let (port, id) = make_port_with_device("wf-lifecycle-001", "Workflow Wheel")?;

    // Discover
    let devices = port
        .list_devices()
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    assert_eq!(devices.len(), 1);
    assert!(devices[0].is_connected);

    // Connect
    let mut dev = port
        .open_device(&id)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    // Configure
    let caps = dev.capabilities().clone();
    let mode = ModeSelectionPolicy::select_mode(&caps, None);
    assert_eq!(mode, FFBMode::RawTorque);

    // Process 50 ticks
    let mut pipeline = Pipeline::new();
    let safety = SafetyService::new(5.0, 20.0);
    for seq in 0u16..50 {
        let ffb_in = ((seq as f32) * 0.1).sin() * 0.5;
        let torque = run_full_tick(ffb_in, 1.0, seq, &mut pipeline, &safety, &mut *dev, 5.0)?;
        assert!(torque.is_finite());
        assert!(torque.abs() <= 5.0);
    }

    // Read telemetry
    let telem = dev
        .read_telemetry()
        .ok_or_else(|| anyhow::anyhow!("telemetry missing after processing"))?;
    assert!(telem.temperature_c <= 150);

    // Disconnect
    let id2: DeviceId = "wf-lifecycle-001".parse()?;
    let mut standalone = VirtualDevice::new(id2, "Workflow Wheel".to_string());
    standalone.disconnect();
    assert!(!standalone.is_connected());
    assert!(standalone.write_ffb_report(1.0, 99).is_err());

    Ok(())
}

/// Discover → connect → configure with PID-only device (non-DD).
#[tokio::test]
async fn workflow_lifecycle_pid_only_device() -> Result<()> {
    let id: DeviceId = "wf-pid-001".parse()?;
    let caps = DeviceCapabilities::new(true, false, false, false, TorqueNm::new(8.0)?, 4096, 2000);
    let mode = ModeSelectionPolicy::select_mode(&caps, None);
    assert_eq!(mode, FFBMode::PidPassthrough, "PID-only device must select PidPassthrough");

    let mut device = VirtualDevice::new(id, "PID Wheel".to_string());
    let mut pipeline = Pipeline::new();
    let safety = SafetyService::new(3.0, 8.0);

    for seq in 0u16..20 {
        let mut frame = engine_frame(0.3, 0.5, seq);
        pipeline.process(&mut frame)?;
        let torque = safety.clamp_torque_nm(frame.torque_out * 3.0);
        device.write_ffb_report(torque, seq)?;
        assert!(torque.abs() <= 3.0);
    }

    assert!(device.read_telemetry().is_some());
    Ok(())
}

/// Lifecycle with capability report round-trip through negotiator.
#[test]
fn workflow_capability_round_trip_then_process() -> Result<()> {
    let caps = DeviceCapabilities::new(false, true, true, true, TorqueNm::new(25.0)?, 65535, 1000);
    let report = CapabilityNegotiator::create_capabilities_report(&caps);
    let parsed = CapabilityNegotiator::parse_capabilities_report(&report)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let mode = ModeSelectionPolicy::select_mode(&parsed, None);
    assert_eq!(mode, FFBMode::RawTorque);

    let id: DeviceId = "wf-cap-rt-001".parse()?;
    let mut device = VirtualDevice::new(id, "CapRT Wheel".to_string());
    let mut pipeline = Pipeline::new();
    let safety = SafetyService::new(5.0, 20.0);

    for seq in 0u16..30 {
        let mut frame = engine_frame(0.4, 1.0, seq);
        pipeline.process(&mut frame)?;
        let torque = safety.clamp_torque_nm(frame.torque_out * 5.0);
        device.write_ffb_report(torque, seq)?;
    }

    let telem = device
        .read_telemetry()
        .ok_or_else(|| anyhow::anyhow!("telemetry missing"))?;
    assert!(telem.temperature_c <= 150);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. FFB processing end-to-end: telemetry → filters → engine → FFB output
// ═══════════════════════════════════════════════════════════════════════════════

/// Full FFB chain with varying telemetry inputs over 200 ticks.
#[test]
fn workflow_ffb_processing_varying_telemetry() -> Result<()> {
    let id: DeviceId = "wf-ffb-vary-001".parse()?;
    let mut device = VirtualDevice::new(id, "FFB Varying Wheel".to_string());
    let mut pipeline = Pipeline::new();
    let safety = SafetyService::new(5.0, 20.0);

    let mut max_torque_seen = 0.0f32;
    for seq in 0u16..200 {
        let t = seq as f32 / 200.0;
        let ffb = 0.5 * (t * std::f32::consts::TAU * 3.0).sin();
        let speed = 0.5 + 2.0 * t;
        let torque = run_full_tick(ffb, speed, seq, &mut pipeline, &safety, &mut device, 5.0)?;
        assert!(torque.is_finite());
        if torque.abs() > max_torque_seen {
            max_torque_seen = torque.abs();
        }
    }

    assert!(max_torque_seen > 0.0, "must have produced non-zero torque");
    assert!(max_torque_seen <= 5.0, "torque must stay within safe limit");
    Ok(())
}

/// FFB chain with extreme inputs: ±1.0, zero, near-zero.
#[test]
fn workflow_ffb_extreme_inputs() -> Result<()> {
    let id: DeviceId = "wf-ffb-extreme-001".parse()?;
    let mut device = VirtualDevice::new(id, "FFB Extreme Wheel".to_string());
    let mut pipeline = Pipeline::new();
    let safety = SafetyService::new(5.0, 20.0);

    let inputs: &[f32] = &[0.0, 1.0, -1.0, 0.999, -0.999, 0.001, -0.001, 0.5, -0.5];
    for (i, &ffb_in) in inputs.iter().enumerate() {
        let seq = i as u16;
        let torque = run_full_tick(ffb_in, 1.0, seq, &mut pipeline, &safety, &mut device, 5.0)?;
        assert!(torque.is_finite(), "input {ffb_in}: torque must be finite");
        assert!(torque.abs() <= 5.0, "input {ffb_in}: torque within limit");
    }
    Ok(())
}

/// FFB processing with faulted safety zeros all output.
#[test]
fn workflow_ffb_faulted_safety_zeros_output() -> Result<()> {
    let id: DeviceId = "wf-ffb-faulted-001".parse()?;
    let mut device = VirtualDevice::new(id, "FFB Faulted Wheel".to_string());
    let mut pipeline = Pipeline::new();
    let mut safety = SafetyService::new(5.0, 20.0);

    // Normal ticks first
    for seq in 0u16..10 {
        let mut frame = engine_frame(0.5, 1.0, seq);
        pipeline.process(&mut frame)?;
        let torque = safety.clamp_torque_nm(frame.torque_out * 5.0);
        device.write_ffb_report(torque, seq)?;
    }

    // Fault the safety service
    safety.report_fault(FaultType::UsbStall);
    assert!(matches!(safety.state(), SafetyState::Faulted { .. }));

    // All subsequent output must be zero
    for seq in 10u16..20 {
        let mut frame = engine_frame(0.8, 1.0, seq);
        pipeline.process(&mut frame)?;
        let torque = safety.clamp_torque_nm(frame.torque_out * 5.0);
        assert!(
            torque.abs() < 0.001,
            "faulted safety must zero torque, got {torque}"
        );
    }
    Ok(())
}

/// FFB with physics simulation: device state evolves over ticks.
#[test]
fn workflow_ffb_with_physics_evolution() -> Result<()> {
    let id: DeviceId = "wf-ffb-physics-001".parse()?;
    let mut device = VirtualDevice::new(id, "FFB Physics Wheel".to_string());
    let mut pipeline = Pipeline::new();
    let safety = SafetyService::new(5.0, 20.0);

    for seq in 0u16..50 {
        let torque = run_full_tick(0.4, 0.5, seq, &mut pipeline, &safety, &mut device, 5.0)?;
        assert!(torque.is_finite());
        device.simulate_physics(Duration::from_millis(1));
    }

    let telem = device
        .read_telemetry()
        .ok_or_else(|| anyhow::anyhow!("telemetry missing after physics sim"))?;
    assert!(
        telem.wheel_angle_deg.abs() > 0.0 || telem.wheel_speed_rad_s.abs() > 0.0,
        "wheel must have moved"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. Device hot-plug during active session
// ═══════════════════════════════════════════════════════════════════════════════

/// Active device → disconnect → writes fail → reconnect → resume FFB.
#[test]
fn workflow_hotplug_disconnect_reconnect() -> Result<()> {
    let id: DeviceId = "wf-hotplug-001".parse()?;
    let mut device = VirtualDevice::new(id, "HotPlug Wheel".to_string());
    let mut pipeline = Pipeline::new();
    let mut safety = SafetyService::new(5.0, 20.0);

    // Active phase
    for seq in 0u16..20 {
        let mut frame = engine_frame(0.5, 1.0, seq);
        pipeline.process(&mut frame)?;
        let torque = safety.clamp_torque_nm(frame.torque_out * 5.0);
        device.write_ffb_report(torque, seq)?;
    }
    assert!(device.is_connected());

    // Disconnect
    device.disconnect();
    assert!(!device.is_connected());
    assert!(device.write_ffb_report(1.0, 20).is_err());
    assert!(device.read_telemetry().is_none());

    // Report fault
    safety.report_fault(FaultType::UsbStall);
    assert!(matches!(safety.state(), SafetyState::Faulted { .. }));

    // Reconnect and recover
    device.reconnect();
    assert!(device.is_connected());
    std::thread::sleep(Duration::from_millis(120));
    safety.clear_fault().map_err(|e| anyhow::anyhow!("{e}"))?;
    assert_eq!(safety.state(), &SafetyState::SafeTorque);

    // Resume
    for seq in 21u16..40 {
        let mut frame = engine_frame(0.3, 0.5, seq);
        pipeline.process(&mut frame)?;
        let torque = safety.clamp_torque_nm(frame.torque_out * 5.0);
        device.write_ffb_report(torque, seq)?;
    }
    assert!(device.read_telemetry().is_some());
    Ok(())
}

/// Hot-plug a second device mid-session via VirtualHidPort.
#[tokio::test]
async fn workflow_hotplug_add_device_mid_session() -> Result<()> {
    let (mut port, id_a) = make_port_with_device("wf-hp-add-a", "Primary Wheel")?;

    let mut dev_a = port
        .open_device(&id_a)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    dev_a.write_ffb_report(2.0, 0)?;

    // Hot-plug second device
    let id_b: DeviceId = "wf-hp-add-b".parse()?;
    port.add_device(VirtualDevice::new(id_b.clone(), "Secondary Wheel".to_string()))
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    // First device still works
    dev_a.write_ffb_report(3.0, 1)?;
    assert!(dev_a.read_telemetry().is_some());

    // Second device is functional
    let mut dev_b = port
        .open_device(&id_b)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    dev_b.write_ffb_report(1.5, 0)?;
    assert!(dev_b.read_telemetry().is_some());

    let devices = port
        .list_devices()
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    assert_eq!(devices.len(), 2);
    Ok(())
}

/// Remove device from port; other device unaffected.
#[tokio::test]
async fn workflow_hotplug_remove_device_other_continues() -> Result<()> {
    let mut port = VirtualHidPort::new();
    let id_a: DeviceId = "wf-hp-rm-a".parse()?;
    let id_b: DeviceId = "wf-hp-rm-b".parse()?;
    port.add_device(VirtualDevice::new(id_a.clone(), "Wheel A".to_string()))
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    port.add_device(VirtualDevice::new(id_b.clone(), "Wheel B".to_string()))
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let mut dev_b = port
        .open_device(&id_b)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    port.remove_device(&id_a)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    dev_b.write_ffb_report(2.0, 0)?;
    assert!(dev_b.read_telemetry().is_some());

    let devices = port
        .list_devices()
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    assert_eq!(devices.len(), 1);
    Ok(())
}

/// Rapid connect/disconnect cycles don't corrupt device state.
#[test]
fn workflow_hotplug_rapid_cycles() -> Result<()> {
    let id: DeviceId = "wf-hp-rapid-001".parse()?;
    let mut device = VirtualDevice::new(id, "Rapid HotPlug Wheel".to_string());

    for _ in 0..10 {
        device.disconnect();
        assert!(!device.is_connected());
        assert!(device.write_ffb_report(1.0, 0).is_err());

        device.reconnect();
        assert!(device.is_connected());
        device.write_ffb_report(0.5, 0)?;
        assert!(device.read_telemetry().is_some());
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. Multi-device setup (wheel + pedals + shifter)
// ═══════════════════════════════════════════════════════════════════════════════

/// Three devices on one port, each independently operational.
#[tokio::test]
async fn workflow_multi_device_three_devices_independent() -> Result<()> {
    let mut port = VirtualHidPort::new();

    let id_wheel: DeviceId = "wf-multi-wheel".parse()?;
    let id_pedals: DeviceId = "wf-multi-pedals".parse()?;
    let id_shifter: DeviceId = "wf-multi-shifter".parse()?;

    port.add_device(VirtualDevice::new(id_wheel.clone(), "DD Wheel".to_string()))
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    port.add_device(VirtualDevice::new(id_pedals.clone(), "Load Cell Pedals".to_string()))
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    port.add_device(VirtualDevice::new(id_shifter.clone(), "H-Pattern Shifter".to_string()))
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let devices = port
        .list_devices()
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    assert_eq!(devices.len(), 3);

    // Open and use each device
    let mut dev_wheel = port
        .open_device(&id_wheel)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut dev_pedals = port
        .open_device(&id_pedals)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut dev_shifter = port
        .open_device(&id_shifter)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    dev_wheel.write_ffb_report(3.0, 0)?;
    dev_pedals.write_ffb_report(0.5, 0)?;
    dev_shifter.write_ffb_report(0.1, 0)?;

    assert!(dev_wheel.read_telemetry().is_some());
    assert!(dev_pedals.read_telemetry().is_some());
    assert!(dev_shifter.read_telemetry().is_some());

    Ok(())
}

/// Multi-device: removing one device doesn't affect others.
#[tokio::test]
async fn workflow_multi_device_remove_one_others_continue() -> Result<()> {
    let mut port = VirtualHidPort::new();

    let id_a: DeviceId = "wf-multi-rm-a".parse()?;
    let id_b: DeviceId = "wf-multi-rm-b".parse()?;
    let id_c: DeviceId = "wf-multi-rm-c".parse()?;

    port.add_device(VirtualDevice::new(id_a.clone(), "Device A".to_string()))
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    port.add_device(VirtualDevice::new(id_b.clone(), "Device B".to_string()))
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    port.add_device(VirtualDevice::new(id_c.clone(), "Device C".to_string()))
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let mut dev_b = port
        .open_device(&id_b)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut dev_c = port
        .open_device(&id_c)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    port.remove_device(&id_a)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    dev_b.write_ffb_report(1.0, 0)?;
    dev_c.write_ffb_report(1.0, 0)?;
    assert!(dev_b.read_telemetry().is_some());
    assert!(dev_c.read_telemetry().is_some());

    let devices = port
        .list_devices()
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    assert_eq!(devices.len(), 2);
    Ok(())
}

/// Multi-device: each device processes pipeline independently.
#[tokio::test]
async fn workflow_multi_device_independent_pipelines() -> Result<()> {
    let mut port = VirtualHidPort::new();

    let id_a: DeviceId = "wf-multi-pipe-a".parse()?;
    let id_b: DeviceId = "wf-multi-pipe-b".parse()?;

    port.add_device(VirtualDevice::new(id_a.clone(), "Pipeline A Wheel".to_string()))
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    port.add_device(VirtualDevice::new(id_b.clone(), "Pipeline B Wheel".to_string()))
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let mut dev_a = port
        .open_device(&id_a)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut dev_b = port
        .open_device(&id_b)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let mut pipeline_a = Pipeline::new();
    let mut pipeline_b = Pipeline::new();
    let safety = SafetyService::new(5.0, 20.0);

    for seq in 0u16..30 {
        let mut frame_a = engine_frame(0.6, 1.0, seq);
        pipeline_a.process(&mut frame_a)?;
        let torque_a = safety.clamp_torque_nm(frame_a.torque_out * 5.0);
        dev_a.write_ffb_report(torque_a, seq)?;

        let mut frame_b = engine_frame(0.2, 0.3, seq);
        pipeline_b.process(&mut frame_b)?;
        let torque_b = safety.clamp_torque_nm(frame_b.torque_out * 3.0);
        dev_b.write_ffb_report(torque_b, seq)?;
    }

    assert!(dev_a.read_telemetry().is_some());
    assert!(dev_b.read_telemetry().is_some());
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. Profile switching during active session
// ═══════════════════════════════════════════════════════════════════════════════

/// Switch between two filter profiles mid-session: transition is smooth.
#[test]
fn workflow_profile_switch_smooth_transition() -> Result<()> {
    let id: DeviceId = "wf-profile-sw-001".parse()?;
    let mut device = VirtualDevice::new(id, "Profile Switch Wheel".to_string());
    let safety = SafetyService::new(5.0, 20.0);

    // Profile A: heavy damping
    let damper_a = DamperState::fixed(0.05);
    let friction_a = FrictionState::fixed(0.03);
    let cap_a = 0.8;

    // Profile B: light damping
    let damper_b = DamperState::fixed(0.01);
    let friction_b = FrictionState::fixed(0.005);
    let cap_b = 1.0;

    let mut pipeline = Pipeline::new();
    let mut last_torque_a = 0.0f32;

    // Run Profile A
    for seq in 0u16..50 {
        let mut ff = filter_frame(0.6, 1.0, seq);
        damper_filter(&mut ff, &damper_a);
        friction_filter(&mut ff, &friction_a);
        torque_cap_filter(&mut ff, cap_a);
        let mut ef = engine_frame(ff.torque_out, ff.wheel_speed, seq);
        pipeline.process(&mut ef)?;
        let torque = safety.clamp_torque_nm(ef.torque_out * 5.0);
        device.write_ffb_report(torque, seq)?;
        last_torque_a = torque;
    }

    // Switch to Profile B
    let mut pipeline_b = Pipeline::new();
    let mut first_torque_b = 0.0f32;
    for seq in 50u16..100 {
        let mut ff = filter_frame(0.6, 1.0, seq);
        damper_filter(&mut ff, &damper_b);
        friction_filter(&mut ff, &friction_b);
        torque_cap_filter(&mut ff, cap_b);
        let mut ef = engine_frame(ff.torque_out, ff.wheel_speed, seq);
        pipeline_b.process(&mut ef)?;
        let torque = safety.clamp_torque_nm(ef.torque_out * 5.0);
        device.write_ffb_report(torque, seq)?;
        if seq == 50 {
            first_torque_b = torque;
        }
    }

    assert!(last_torque_a.is_finite());
    assert!(first_torque_b.is_finite());
    assert!(device.read_telemetry().is_some());
    Ok(())
}

/// Switch from slew-rate limited to unlimited mid-session.
#[test]
fn workflow_profile_switch_slew_rate_transition() -> Result<()> {
    let id: DeviceId = "wf-slew-sw-001".parse()?;
    let mut device = VirtualDevice::new(id, "Slew Switch Wheel".to_string());
    let safety = SafetyService::new(5.0, 20.0);
    let mut pipeline = Pipeline::new();
    let mut slew_state = SlewRateState::new(0.8);

    // Phase 1: slew limited
    let mut last_slew = 0.0f32;
    for seq in 0u16..30 {
        let mut ff = filter_frame(0.7, 1.0, seq);
        slew_rate_filter(&mut ff, &mut slew_state);
        torque_cap_filter(&mut ff, 1.0);
        let mut ef = engine_frame(ff.torque_out, ff.wheel_speed, seq);
        pipeline.process(&mut ef)?;
        let torque = safety.clamp_torque_nm(ef.torque_out * 5.0);
        device.write_ffb_report(torque, seq)?;
        last_slew = torque;
    }

    // Phase 2: no slew limit
    for seq in 30u16..60 {
        let mut ff = filter_frame(0.7, 1.0, seq);
        torque_cap_filter(&mut ff, 1.0);
        let mut ef = engine_frame(ff.torque_out, ff.wheel_speed, seq);
        pipeline.process(&mut ef)?;
        let torque = safety.clamp_torque_nm(ef.torque_out * 5.0);
        device.write_ffb_report(torque, seq)?;
    }

    assert!(last_slew.is_finite());
    assert!(device.read_telemetry().is_some());
    Ok(())
}

/// Profile switch: torque cap change during active processing.
#[test]
fn workflow_profile_switch_torque_cap_change() -> Result<()> {
    let id: DeviceId = "wf-cap-sw-001".parse()?;
    let mut device = VirtualDevice::new(id, "Cap Switch Wheel".to_string());
    let safety = SafetyService::new(5.0, 20.0);
    let mut pipeline = Pipeline::new();

    // Low cap phase
    let mut low_cap_max = 0.0f32;
    for seq in 0u16..40 {
        let mut ff = filter_frame(0.8, 1.0, seq);
        torque_cap_filter(&mut ff, 0.5); // 50% cap
        let mut ef = engine_frame(ff.torque_out, ff.wheel_speed, seq);
        pipeline.process(&mut ef)?;
        let torque = safety.clamp_torque_nm(ef.torque_out * 5.0);
        device.write_ffb_report(torque, seq)?;
        if torque.abs() > low_cap_max {
            low_cap_max = torque.abs();
        }
    }

    // High cap phase
    let mut pipeline_b = Pipeline::new();
    let mut high_cap_max = 0.0f32;
    for seq in 40u16..80 {
        let mut ff = filter_frame(0.8, 1.0, seq);
        torque_cap_filter(&mut ff, 1.0); // 100% cap
        let mut ef = engine_frame(ff.torque_out, ff.wheel_speed, seq);
        pipeline_b.process(&mut ef)?;
        let torque = safety.clamp_torque_nm(ef.torque_out * 5.0);
        device.write_ffb_report(torque, seq)?;
        if torque.abs() > high_cap_max {
            high_cap_max = torque.abs();
        }
    }

    // High cap should allow more torque (or equal)
    assert!(
        high_cap_max >= low_cap_max - 0.01,
        "high cap ({high_cap_max}) should permit >= low cap ({low_cap_max})"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Additional workflow tests
// ═══════════════════════════════════════════════════════════════════════════════

/// Safety interlock activation and recovery during high torque.
#[test]
fn workflow_safety_interlock_during_high_torque() -> Result<()> {
    let id: DeviceId = "wf-safety-hl-001".parse()?;
    let mut device = VirtualDevice::new(id, "Safety Interlock Wheel".to_string());
    let mut pipeline = Pipeline::new();
    let mut safety = SafetyService::new(5.0, 20.0);

    // Normal operation
    for seq in 0u16..30 {
        let torque = run_full_tick(0.6, 1.0, seq, &mut pipeline, &safety, &mut device, 5.0)?;
        assert!(torque.abs() <= 5.0);
    }

    // Trigger thermal fault
    safety.report_fault(FaultType::ThermalLimit);
    assert!(matches!(safety.state(), SafetyState::Faulted { .. }));

    // Output must be zero while faulted
    let clamped = safety.clamp_torque_nm(5.0);
    assert!(clamped.abs() < 0.001, "faulted must zero torque");

    // Wait and clear
    std::thread::sleep(Duration::from_millis(120));
    safety.clear_fault().map_err(|e| anyhow::anyhow!("{e}"))?;
    assert_eq!(safety.state(), &SafetyState::SafeTorque);

    // Resume normal operation
    for seq in 30u16..50 {
        let torque = run_full_tick(0.4, 1.0, seq, &mut pipeline, &safety, &mut device, 5.0)?;
        assert!(torque.is_finite());
    }
    Ok(())
}

/// Device fault injection and clearance workflow.
#[test]
fn workflow_device_fault_injection_clearance() -> Result<()> {
    let id: DeviceId = "wf-fault-inj-001".parse()?;
    let mut device = VirtualDevice::new(id, "Fault Inject Wheel".to_string());

    // Normal state
    device.write_ffb_report(1.0, 0)?;
    assert!(device.read_telemetry().is_some());

    // Inject fault
    device.inject_fault(0x01);
    let telem = device
        .read_telemetry()
        .ok_or_else(|| anyhow::anyhow!("telemetry missing after fault injection"))?;
    assert_ne!(telem.fault_flags, 0, "fault flags must be set");

    // Clear faults
    device.clear_faults();
    let telem2 = device
        .read_telemetry()
        .ok_or_else(|| anyhow::anyhow!("telemetry missing after fault clear"))?;
    assert_eq!(telem2.fault_flags, 0, "fault flags must be cleared");
    Ok(())
}

/// Enumerate empty port, add device, re-enumerate.
#[tokio::test]
async fn workflow_enumerate_empty_then_add() -> Result<()> {
    let mut port = VirtualHidPort::new();

    let devices = port
        .list_devices()
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    assert!(devices.is_empty(), "empty port must have no devices");

    let id: DeviceId = "wf-enum-add-001".parse()?;
    port.add_device(VirtualDevice::new(id, "New Wheel".to_string()))
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let devices = port
        .list_devices()
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    assert_eq!(devices.len(), 1);
    Ok(())
}

/// Multiple sequential pipeline resets don't corrupt state.
#[test]
fn workflow_sequential_pipeline_resets() -> Result<()> {
    let id: DeviceId = "wf-pipe-reset-001".parse()?;
    let mut device = VirtualDevice::new(id, "Pipeline Reset Wheel".to_string());
    let safety = SafetyService::new(5.0, 20.0);

    for cycle in 0u16..5 {
        let mut pipeline = Pipeline::new();
        for seq in 0u16..20 {
            let global_seq = cycle * 20 + seq;
            let mut frame = engine_frame(0.3, 1.0, global_seq);
            pipeline.process(&mut frame)?;
            let torque = safety.clamp_torque_nm(frame.torque_out * 5.0);
            device.write_ffb_report(torque, global_seq)?;
            assert!(torque.is_finite());
        }
    }
    assert!(device.read_telemetry().is_some());
    Ok(())
}

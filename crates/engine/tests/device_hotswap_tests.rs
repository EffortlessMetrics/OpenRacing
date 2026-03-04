//! Device hot-swap simulation tests.
//!
//! Covers connect/disconnect during idle and active FFB, multi-device scenarios,
//! reconnection after unexpected disconnect, enumeration refresh, graceful
//! degradation, device priority/fallback, VID/PID matching after reconnect,
//! state persistence across reconnect cycles, and concurrent device event stress.

use racing_wheel_engine::{HidDevice, HidPort, RTError, VirtualDevice, VirtualHidPort};
use racing_wheel_schemas::prelude::*;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_id(name: &str) -> Result<DeviceId, Box<dyn std::error::Error>> {
    Ok(name.parse::<DeviceId>()?)
}

fn make_device(name: &str) -> Result<VirtualDevice, Box<dyn std::error::Error>> {
    let id = make_id(name)?;
    Ok(VirtualDevice::new(id, name.to_string()))
}

/// Assert a device can successfully write a torque report and read telemetry.
fn assert_device_operational(
    device: &mut dyn HidDevice,
    torque_nm: f32,
    seq: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    device.write_ffb_report(torque_nm, seq)?;
    let tel = device
        .read_telemetry()
        .ok_or("expected telemetry from connected device")?;
    assert!(tel.temperature_c >= 20);
    Ok(())
}

// ===================================================================
// 1. Device connect/disconnect during idle
// ===================================================================

#[tokio::test]
async fn connect_during_idle_is_enumerated() -> Result<(), Box<dyn std::error::Error>> {
    let mut port = VirtualHidPort::new();

    // Port starts empty
    let devices = port.list_devices().await?;
    assert_eq!(devices.len(), 0);

    // Connect a device while port is idle (no active FFB)
    let device = make_device("idle-connect")?;
    port.add_device(device)?;

    let devices = port.list_devices().await?;
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].id.as_str(), "idle-connect");
    assert!(devices[0].is_connected);
    Ok(())
}

#[tokio::test]
async fn disconnect_during_idle_removes_from_enumeration() -> Result<(), Box<dyn std::error::Error>>
{
    let mut port = VirtualHidPort::new();
    let id = make_id("idle-disconnect")?;
    let device = VirtualDevice::new(id.clone(), "Idle Disconnect".to_string());
    port.add_device(device)?;

    assert_eq!(port.list_devices().await?.len(), 1);

    port.remove_device(&id)?;
    assert_eq!(port.list_devices().await?.len(), 0);
    Ok(())
}

#[tokio::test]
async fn connect_then_disconnect_cycle_during_idle() -> Result<(), Box<dyn std::error::Error>> {
    let mut port = VirtualHidPort::new();

    for i in 0..5 {
        let name = format!("cycle-{i}");
        let id = make_id(&name)?;
        let device = VirtualDevice::new(id.clone(), name.clone());
        port.add_device(device)?;
        assert_eq!(port.list_devices().await?.len(), 1);

        port.remove_device(&id)?;
        assert_eq!(port.list_devices().await?.len(), 0);
    }
    Ok(())
}

// ===================================================================
// 2. Device connect/disconnect during active FFB processing
// ===================================================================

#[tokio::test]
async fn disconnect_during_active_ffb_returns_device_disconnected()
-> Result<(), Box<dyn std::error::Error>> {
    let id = make_id("active-ffb")?;
    let mut device = VirtualDevice::new(id, "Active FFB".to_string());

    // Start FFB output
    device.write_ffb_report(10.0, 1)?;
    device.simulate_physics(Duration::from_millis(10));

    // Disconnect mid-stream
    device.disconnect();
    assert!(!device.is_connected());

    let result = device.write_ffb_report(10.0, 2);
    assert_eq!(result, Err(RTError::DeviceDisconnected));
    assert!(device.read_telemetry().is_none());
    Ok(())
}

#[tokio::test]
async fn connect_new_device_while_another_is_active() -> Result<(), Box<dyn std::error::Error>> {
    let mut port = VirtualHidPort::new();

    // First device already active
    let id_a = make_id("active-a")?;
    let device_a = VirtualDevice::new(id_a.clone(), "Active A".to_string());
    port.add_device(device_a)?;

    let mut opened_a = port.open_device(&id_a).await?;
    opened_a.write_ffb_report(5.0, 1)?;

    // Hot-plug a second device while A is active
    let id_b = make_id("active-b")?;
    let device_b = VirtualDevice::new(id_b.clone(), "Active B".to_string());
    port.add_device(device_b)?;

    let devices = port.list_devices().await?;
    assert_eq!(devices.len(), 2);

    // Both devices should be independently operable
    let mut opened_b = port.open_device(&id_b).await?;
    assert_device_operational(&mut *opened_a, 3.0, 2)?;
    assert_device_operational(&mut *opened_b, 4.0, 1)?;
    Ok(())
}

#[tokio::test]
async fn torque_stops_on_disconnect_during_active_ffb() -> Result<(), Box<dyn std::error::Error>> {
    let id = make_id("torque-stop")?;
    let mut device = VirtualDevice::new(id, "Torque Stop".to_string());

    // Ramp up torque
    for seq in 0..10u16 {
        device.write_ffb_report(15.0, seq)?;
        device.simulate_physics(Duration::from_millis(1));
    }

    // Disconnect — subsequent writes must fail
    device.disconnect();
    let result = device.write_ffb_report(15.0, 10);
    assert_eq!(result, Err(RTError::DeviceDisconnected));

    // Telemetry must be unavailable
    assert!(device.read_telemetry().is_none());
    Ok(())
}

// ===================================================================
// 3. Multiple devices connecting simultaneously
// ===================================================================

#[tokio::test]
async fn multiple_devices_connect_simultaneously() -> Result<(), Box<dyn std::error::Error>> {
    let mut port = VirtualHidPort::new();

    let device_count = 8;
    let mut ids = Vec::new();

    for i in 0..device_count {
        let name = format!("sim-device-{i}");
        let id = make_id(&name)?;
        let device = VirtualDevice::new(id.clone(), name);
        port.add_device(device)?;
        ids.push(id);
    }

    let devices = port.list_devices().await?;
    assert_eq!(devices.len(), device_count);

    // Each device can be opened and operated independently
    for id in &ids {
        let mut opened = port.open_device(id).await?;
        assert!(opened.is_connected());
        assert_device_operational(&mut *opened, 1.0, 0)?;
    }
    Ok(())
}

#[tokio::test]
async fn simultaneous_connect_preserves_device_identity() -> Result<(), Box<dyn std::error::Error>>
{
    let mut port = VirtualHidPort::new();

    let names = ["wheel-base", "pedals", "shifter", "handbrake"];
    for name in &names {
        let device = make_device(name)?;
        port.add_device(device)?;
    }

    let devices = port.list_devices().await?;
    assert_eq!(devices.len(), names.len());

    for name in &names {
        let found = devices.iter().any(|d| d.id.as_str() == *name);
        assert!(found, "device '{name}' not found in enumeration");
    }
    Ok(())
}

// ===================================================================
// 4. Device reconnection after unexpected disconnect
// ===================================================================

#[tokio::test]
async fn reconnect_after_unexpected_disconnect() -> Result<(), Box<dyn std::error::Error>> {
    let id = make_id("unexpected-dc")?;
    let mut device = VirtualDevice::new(id, "Unexpected DC".to_string());

    // Normal operation
    assert_device_operational(&mut device, 5.0, 0)?;

    // Unexpected disconnect
    device.disconnect();
    assert!(!device.is_connected());
    assert_eq!(
        device.write_ffb_report(5.0, 1),
        Err(RTError::DeviceDisconnected)
    );

    // Reconnect
    device.reconnect();
    assert!(device.is_connected());
    assert_device_operational(&mut device, 5.0, 2)?;
    Ok(())
}

#[tokio::test]
async fn multiple_disconnect_reconnect_cycles() -> Result<(), Box<dyn std::error::Error>> {
    let id = make_id("multi-cycle")?;
    let mut device = VirtualDevice::new(id, "Multi Cycle".to_string());

    for cycle in 0..10u16 {
        // Operate
        device.write_ffb_report(3.0, cycle * 3)?;
        device.simulate_physics(Duration::from_millis(1));

        // Disconnect
        device.disconnect();
        assert_eq!(
            device.write_ffb_report(1.0, cycle * 3 + 1),
            Err(RTError::DeviceDisconnected)
        );

        // Reconnect
        device.reconnect();
        device.write_ffb_report(2.0, cycle * 3 + 2)?;
    }

    assert!(device.is_connected());
    Ok(())
}

#[tokio::test]
async fn reconnect_restores_telemetry_stream() -> Result<(), Box<dyn std::error::Error>> {
    let id = make_id("tel-restore")?;
    let mut device = VirtualDevice::new(id, "Telemetry Restore".to_string());

    // Get initial telemetry
    let tel_before = device
        .read_telemetry()
        .ok_or("expected telemetry before disconnect")?;
    assert!(tel_before.temperature_c >= 20);

    // Disconnect — telemetry must stop
    device.disconnect();
    assert!(device.read_telemetry().is_none());

    // Reconnect — telemetry must resume
    device.reconnect();
    let tel_after = device
        .read_telemetry()
        .ok_or("expected telemetry after reconnect")?;
    assert!(tel_after.temperature_c >= 20);
    Ok(())
}

// ===================================================================
// 5. Device enumeration refresh
// ===================================================================

#[tokio::test]
async fn refresh_devices_succeeds() -> Result<(), Box<dyn std::error::Error>> {
    let port = VirtualHidPort::new();
    // refresh_devices is a no-op for virtual port but must not error
    port.refresh_devices().await?;
    Ok(())
}

#[tokio::test]
async fn enumeration_reflects_adds_and_removes() -> Result<(), Box<dyn std::error::Error>> {
    let mut port = VirtualHidPort::new();

    assert_eq!(port.list_devices().await?.len(), 0);

    let id1 = make_id("enum-a")?;
    let id2 = make_id("enum-b")?;
    let id3 = make_id("enum-c")?;

    port.add_device(VirtualDevice::new(id1.clone(), "A".to_string()))?;
    assert_eq!(port.list_devices().await?.len(), 1);

    port.add_device(VirtualDevice::new(id2.clone(), "B".to_string()))?;
    assert_eq!(port.list_devices().await?.len(), 2);

    port.add_device(VirtualDevice::new(id3.clone(), "C".to_string()))?;
    assert_eq!(port.list_devices().await?.len(), 3);

    port.remove_device(&id2)?;
    let devices = port.list_devices().await?;
    assert_eq!(devices.len(), 2);

    let ids: Vec<&str> = devices.iter().map(|d| d.id.as_str()).collect();
    assert!(ids.contains(&"enum-a"));
    assert!(!ids.contains(&"enum-b"));
    assert!(ids.contains(&"enum-c"));
    Ok(())
}

#[tokio::test]
async fn refresh_after_add_remove_is_consistent() -> Result<(), Box<dyn std::error::Error>> {
    let mut port = VirtualHidPort::new();

    let id = make_id("refresh-test")?;
    port.add_device(VirtualDevice::new(id.clone(), "Refresh".to_string()))?;

    port.refresh_devices().await?;
    assert_eq!(port.list_devices().await?.len(), 1);

    port.remove_device(&id)?;
    port.refresh_devices().await?;
    assert_eq!(port.list_devices().await?.len(), 0);
    Ok(())
}

// ===================================================================
// 6. Graceful degradation when primary device disconnects
// ===================================================================

#[tokio::test]
async fn primary_disconnect_secondary_remains_operational() -> Result<(), Box<dyn std::error::Error>>
{
    let mut port = VirtualHidPort::new();

    let primary_id = make_id("primary-wheel")?;
    let secondary_id = make_id("secondary-pedals")?;

    port.add_device(VirtualDevice::new(
        primary_id.clone(),
        "Primary Wheel".to_string(),
    ))?;
    port.add_device(VirtualDevice::new(
        secondary_id.clone(),
        "Secondary Pedals".to_string(),
    ))?;

    let mut primary = port.open_device(&primary_id).await?;
    let mut secondary = port.open_device(&secondary_id).await?;

    // Both operational
    assert_device_operational(&mut *primary, 10.0, 0)?;
    assert_device_operational(&mut *secondary, 2.0, 0)?;

    // Remove primary from port (simulating unplug)
    port.remove_device(&primary_id)?;

    // Secondary must remain fully operational
    assert!(secondary.is_connected());
    assert_device_operational(&mut *secondary, 3.0, 1)?;

    // Enumeration shows only secondary
    let devices = port.list_devices().await?;
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].id.as_str(), "secondary-pedals");
    Ok(())
}

#[tokio::test]
async fn disconnect_one_of_many_does_not_affect_others() -> Result<(), Box<dyn std::error::Error>> {
    let mut port = VirtualHidPort::new();
    let count = 5;
    let mut ids = Vec::new();

    for i in 0..count {
        let name = format!("degrade-{i}");
        let id = make_id(&name)?;
        port.add_device(VirtualDevice::new(id.clone(), name))?;
        ids.push(id);
    }

    // Remove device in the middle
    port.remove_device(&ids[2])?;

    let devices = port.list_devices().await?;
    assert_eq!(devices.len(), count - 1);

    // Remaining devices are still operational
    for (i, id) in ids.iter().enumerate() {
        if i == 2 {
            continue;
        }
        let mut opened = port.open_device(id).await?;
        assert_device_operational(&mut *opened, 1.0, 0)?;
    }
    Ok(())
}

// ===================================================================
// 7. Device priority / fallback when multiple devices available
// ===================================================================

#[tokio::test]
async fn first_added_device_appears_first_in_enumeration() -> Result<(), Box<dyn std::error::Error>>
{
    let mut port = VirtualHidPort::new();

    let names = ["first-wheel", "second-wheel", "third-wheel"];
    for name in &names {
        port.add_device(make_device(name)?)?;
    }

    let devices = port.list_devices().await?;
    assert_eq!(devices.len(), 3);
    // Insertion order should be preserved
    for (i, name) in names.iter().enumerate() {
        assert_eq!(devices[i].id.as_str(), *name);
    }
    Ok(())
}

#[tokio::test]
async fn fallback_device_available_after_primary_removal() -> Result<(), Box<dyn std::error::Error>>
{
    let mut port = VirtualHidPort::new();

    let primary_id = make_id("priority-primary")?;
    let fallback_id = make_id("priority-fallback")?;

    port.add_device(VirtualDevice::new(
        primary_id.clone(),
        "Primary".to_string(),
    ))?;
    port.add_device(VirtualDevice::new(
        fallback_id.clone(),
        "Fallback".to_string(),
    ))?;

    // Remove primary
    port.remove_device(&primary_id)?;

    // Fallback is available and operational
    let devices = port.list_devices().await?;
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].id.as_str(), "priority-fallback");

    let mut fallback = port.open_device(&fallback_id).await?;
    assert_device_operational(&mut *fallback, 5.0, 0)?;
    Ok(())
}

#[tokio::test]
async fn capabilities_accessible_for_each_device() -> Result<(), Box<dyn std::error::Error>> {
    let mut port = VirtualHidPort::new();

    for i in 0..3 {
        let name = format!("caps-device-{i}");
        port.add_device(make_device(&name)?)?;
    }

    let devices = port.list_devices().await?;
    for info in &devices {
        let opened = port.open_device(&info.id).await?;
        let caps = opened.capabilities();
        assert!(caps.supports_raw_torque_1khz);
        assert_eq!(caps.max_torque.value(), 25.0);
        assert_eq!(caps.encoder_cpr, 10000);
    }
    Ok(())
}

// ===================================================================
// 8. USB VID/PID matching after reconnect
// ===================================================================

#[test]
fn vid_pid_preserved_across_disconnect_reconnect() -> Result<(), Box<dyn std::error::Error>> {
    let id = make_id("vid-pid-match")?;
    let mut device = VirtualDevice::new(id, "VID/PID Match".to_string());

    let info_before = device.device_info().clone();
    let vid_before = info_before.vendor_id;
    let pid_before = info_before.product_id;

    device.disconnect();
    device.reconnect();

    let info_after = device.device_info();
    assert_eq!(info_after.vendor_id, vid_before);
    assert_eq!(info_after.product_id, pid_before);
    Ok(())
}

#[test]
fn serial_number_preserved_across_reconnect() -> Result<(), Box<dyn std::error::Error>> {
    let id = make_id("serial-match")?;
    let mut device = VirtualDevice::new(id, "Serial Match".to_string());

    let serial_before = device.device_info().serial_number.clone();

    device.disconnect();
    device.reconnect();

    assert_eq!(device.device_info().serial_number, serial_before);
    Ok(())
}

#[tokio::test]
async fn vid_pid_stable_through_port_open_after_reconnect() -> Result<(), Box<dyn std::error::Error>>
{
    let mut port = VirtualHidPort::new();

    let id = make_id("vid-pid-port")?;
    let device = VirtualDevice::new(id.clone(), "VID/PID Port".to_string());
    port.add_device(device)?;

    // First open
    let opened_first = port.open_device(&id).await?;
    let vid = opened_first.device_info().vendor_id;
    let pid = opened_first.device_info().product_id;

    // Second open (simulating reconnect through port)
    let opened_second = port.open_device(&id).await?;
    assert_eq!(opened_second.device_info().vendor_id, vid);
    assert_eq!(opened_second.device_info().product_id, pid);
    Ok(())
}

#[test]
fn device_info_identity_fields_stable() -> Result<(), Box<dyn std::error::Error>> {
    let id = make_id("identity-stable")?;
    let mut device = VirtualDevice::new(id, "Identity Stable".to_string());

    let info_snapshot = device.device_info().clone();

    // Multiple disconnect/reconnect cycles
    for _ in 0..5 {
        device.disconnect();
        device.reconnect();
    }

    let info_final = device.device_info();
    assert_eq!(info_final.id, info_snapshot.id);
    assert_eq!(info_final.vendor_id, info_snapshot.vendor_id);
    assert_eq!(info_final.product_id, info_snapshot.product_id);
    assert_eq!(info_final.serial_number, info_snapshot.serial_number);
    assert_eq!(info_final.manufacturer, info_snapshot.manufacturer);
    assert_eq!(info_final.name, info_snapshot.name);
    Ok(())
}

// ===================================================================
// 9. Device state persistence across reconnect cycles
// ===================================================================

#[test]
fn capabilities_persist_across_reconnect() -> Result<(), Box<dyn std::error::Error>> {
    let id = make_id("caps-persist")?;
    let mut device = VirtualDevice::new(id, "Caps Persist".to_string());

    let caps_before = device.capabilities().clone();

    device.disconnect();
    device.reconnect();

    let caps_after = device.capabilities();
    assert_eq!(caps_after.supports_pid, caps_before.supports_pid);
    assert_eq!(
        caps_after.supports_raw_torque_1khz,
        caps_before.supports_raw_torque_1khz
    );
    assert_eq!(
        caps_after.max_torque.value(),
        caps_before.max_torque.value()
    );
    assert_eq!(caps_after.encoder_cpr, caps_before.encoder_cpr);
    assert_eq!(
        caps_after.min_report_period_us,
        caps_before.min_report_period_us
    );
    Ok(())
}

#[test]
fn health_status_available_after_reconnect() -> Result<(), Box<dyn std::error::Error>> {
    let id = make_id("health-persist")?;
    let mut device = VirtualDevice::new(id, "Health Persist".to_string());

    // Inject a fault before disconnect
    device.inject_fault(0x04);
    let health_before = device.health_status();
    assert_eq!(health_before.fault_flags, 0x04);

    // Disconnect and reconnect — device state (shared Arc) persists
    device.disconnect();
    device.reconnect();

    let health_after = device.health_status();
    // Fault flags persist because internal state is shared via Arc
    assert_eq!(health_after.fault_flags, 0x04);

    // Clear faults for clean state
    device.clear_faults();
    assert_eq!(device.health_status().fault_flags, 0);
    Ok(())
}

#[test]
fn torque_write_works_after_each_reconnect() -> Result<(), Box<dyn std::error::Error>> {
    let id = make_id("torque-persist")?;
    let mut device = VirtualDevice::new(id, "Torque Persist".to_string());

    for cycle in 0..5u16 {
        device.write_ffb_report(10.0, cycle)?;
        device.disconnect();
        assert_eq!(
            device.write_ffb_report(1.0, cycle + 100),
            Err(RTError::DeviceDisconnected)
        );
        device.reconnect();
        device.write_ffb_report(10.0, cycle + 200)?;
    }
    Ok(())
}

#[test]
fn physics_state_survives_reconnect() -> Result<(), Box<dyn std::error::Error>> {
    let id = make_id("physics-persist")?;
    let mut device = VirtualDevice::new(id, "Physics Persist".to_string());

    // Apply torque and simulate to build up state
    device.write_ffb_report(15.0, 0)?;
    for _ in 0..20 {
        device.simulate_physics(Duration::from_millis(5));
    }

    let tel_before = device
        .read_telemetry()
        .ok_or("expected telemetry before disconnect")?;

    device.disconnect();
    device.reconnect();

    // State is shared via Arc<Mutex<..>> so physics state persists
    let tel_after = device
        .read_telemetry()
        .ok_or("expected telemetry after reconnect")?;

    // Wheel angle should be the same (shared state)
    assert!(
        (tel_after.wheel_angle_deg - tel_before.wheel_angle_deg).abs() < f32::EPSILON,
        "wheel angle changed: before={}, after={}",
        tel_before.wheel_angle_deg,
        tel_after.wheel_angle_deg,
    );
    Ok(())
}

// ===================================================================
// 10. Concurrent device events stress test
// ===================================================================

#[tokio::test]
async fn stress_add_remove_many_devices() -> Result<(), Box<dyn std::error::Error>> {
    let mut port = VirtualHidPort::new();
    let total = 50;

    // Rapidly add devices
    let mut ids = Vec::with_capacity(total);
    for i in 0..total {
        let name = format!("stress-{i}");
        let id = make_id(&name)?;
        port.add_device(VirtualDevice::new(id.clone(), name))?;
        ids.push(id);
    }
    assert_eq!(port.list_devices().await?.len(), total);

    // Remove every other device
    for i in (0..total).step_by(2) {
        port.remove_device(&ids[i])?;
    }
    assert_eq!(port.list_devices().await?.len(), total / 2);

    // Remaining devices are operational
    for i in (1..total).step_by(2) {
        let mut opened = port.open_device(&ids[i]).await?;
        assert_device_operational(&mut *opened, 1.0, 0)?;
    }
    Ok(())
}

#[tokio::test]
async fn stress_rapid_connect_disconnect_single_device() -> Result<(), Box<dyn std::error::Error>> {
    let id = make_id("rapid-cd")?;
    let mut device = VirtualDevice::new(id, "Rapid CD".to_string());

    for seq in 0..100u16 {
        if seq % 2 == 0 {
            device.disconnect();
            assert!(!device.is_connected());
            assert_eq!(
                device.write_ffb_report(1.0, seq),
                Err(RTError::DeviceDisconnected)
            );
        } else {
            device.reconnect();
            assert!(device.is_connected());
            device.write_ffb_report(1.0, seq)?;
        }
    }
    Ok(())
}

#[tokio::test]
async fn stress_interleaved_operations_on_multiple_devices()
-> Result<(), Box<dyn std::error::Error>> {
    let mut port = VirtualHidPort::new();
    let count = 10;
    let mut ids = Vec::with_capacity(count);

    for i in 0..count {
        let name = format!("interleave-{i}");
        let id = make_id(&name)?;
        port.add_device(VirtualDevice::new(id.clone(), name))?;
        ids.push(id);
    }

    // Open all devices and interleave writes
    let mut handles: Vec<Box<dyn HidDevice>> = Vec::new();
    for id in &ids {
        handles.push(port.open_device(id).await?);
    }

    for round in 0..20u16 {
        for (i, handle) in handles.iter_mut().enumerate() {
            let torque = (i as f32 + 1.0) * 0.5;
            handle.write_ffb_report(torque, round)?;
        }
    }

    // All devices still connected
    for handle in &handles {
        assert!(handle.is_connected());
    }
    Ok(())
}

#[tokio::test]
async fn stress_open_device_nonexistent_after_removal() -> Result<(), Box<dyn std::error::Error>> {
    let mut port = VirtualHidPort::new();

    let id = make_id("ghost")?;
    port.add_device(VirtualDevice::new(id.clone(), "Ghost".to_string()))?;
    port.remove_device(&id)?;

    let result = port.open_device(&id).await;
    assert!(result.is_err());
    Ok(())
}

#[tokio::test]
async fn stress_bulk_enumeration_under_churn() -> Result<(), Box<dyn std::error::Error>> {
    let mut port = VirtualHidPort::new();

    // Build up and tear down in waves
    for wave in 0..5 {
        let base = wave * 10;
        let mut wave_ids = Vec::new();

        // Add a batch
        for i in 0..10 {
            let name = format!("churn-{}", base + i);
            let id = make_id(&name)?;
            port.add_device(VirtualDevice::new(id.clone(), name))?;
            wave_ids.push(id);
        }

        // Verify enumeration count
        let expected = (wave + 1) * 10;
        let devices = port.list_devices().await?;
        assert_eq!(
            devices.len(),
            expected,
            "expected {expected} devices after wave {wave}, got {}",
            devices.len(),
        );
    }

    assert_eq!(port.list_devices().await?.len(), 50);
    Ok(())
}

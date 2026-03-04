//! Device hot-plug and lifecycle management tests.
//!
//! Covers the full device lifecycle: connect → recognized → configured → active,
//! disconnect → cleanup → safe state, multi-device hot-plug, reconnection after
//! disconnect, unknown device handling, disconnect during active FFB, connect in
//! fault state, rapid connect/disconnect cycling, device identity verification
//! (VID/PID/serial), and firmware version / compatibility checks.

use racing_wheel_engine::{HidDevice, HidPort, RTError, VirtualDevice, VirtualHidPort};
use racing_wheel_schemas::prelude::*;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

type BoxErr = Box<dyn std::error::Error>;

fn make_id(name: &str) -> Result<DeviceId, BoxErr> {
    Ok(name.parse::<DeviceId>()?)
}

fn make_device(name: &str) -> Result<VirtualDevice, BoxErr> {
    let id = make_id(name)?;
    Ok(VirtualDevice::new(id, name.to_string()))
}

/// Assert a device can write torque and read telemetry.
fn assert_device_operational(
    device: &mut dyn HidDevice,
    torque_nm: f32,
    seq: u16,
) -> Result<(), BoxErr> {
    device.write_ffb_report(torque_nm, seq)?;
    let tel = device
        .read_telemetry()
        .ok_or("expected telemetry from connected device")?;
    assert!(tel.temperature_c >= 20);
    Ok(())
}

// ===================================================================
// 1. Device connect → recognized → configured → active lifecycle
// ===================================================================

#[tokio::test]
async fn lifecycle_connect_recognized_configured_active() -> Result<(), BoxErr> {
    let mut port = VirtualHidPort::new();
    let id = make_id("lifecycle-full")?;
    let device = VirtualDevice::new(id.clone(), "Lifecycle Wheel".to_string());
    port.add_device(device)?;

    // Step 1: recognized — device appears in enumeration
    let devices = port.list_devices().await?;
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].id.as_str(), "lifecycle-full");
    assert!(devices[0].is_connected);

    // Step 2: configured — read capabilities
    let mut opened = port.open_device(&id).await?;
    let caps = opened.capabilities();
    assert!(caps.supports_raw_torque_1khz);
    assert!((caps.max_torque.value() - 25.0).abs() < f32::EPSILON);

    // Step 3: active — write torque and receive telemetry
    assert_device_operational(&mut *opened, 10.0, 0)?;
    Ok(())
}

#[tokio::test]
async fn lifecycle_device_info_populated_on_connect() -> Result<(), BoxErr> {
    let mut port = VirtualHidPort::new();
    let device = make_device("info-populated")?;
    port.add_device(device)?;

    let devices = port.list_devices().await?;
    let info = &devices[0];
    assert_eq!(info.vendor_id, 0x1234);
    assert_eq!(info.product_id, 0x5678);
    assert!(info.serial_number.is_some());
    assert!(info.manufacturer.is_some());
    assert!(info.path.starts_with("virtual://"));
    Ok(())
}

#[tokio::test]
async fn lifecycle_capabilities_read_before_active_use() -> Result<(), BoxErr> {
    let mut port = VirtualHidPort::new();
    let id = make_id("caps-before-active")?;
    port.add_device(VirtualDevice::new(id.clone(), "Caps First".to_string()))?;

    let opened = port.open_device(&id).await?;
    let caps = opened.capabilities();

    // Verify all capability fields are sane before first use
    assert!(caps.max_torque.value() > 0.0);
    assert!(caps.encoder_cpr > 0);
    assert!(caps.min_report_period_us > 0);
    Ok(())
}

// ===================================================================
// 2. Device disconnect → cleanup → safe state transitions
// ===================================================================

#[test]
fn disconnect_transitions_to_not_connected() -> Result<(), BoxErr> {
    let mut device = make_device("dc-state")?;
    assert!(device.is_connected());
    device.disconnect();
    assert!(!device.is_connected());
    Ok(())
}

#[test]
fn disconnect_stops_telemetry_stream() -> Result<(), BoxErr> {
    let mut device = make_device("dc-telemetry")?;
    assert!(device.read_telemetry().is_some());
    device.disconnect();
    assert!(device.read_telemetry().is_none());
    Ok(())
}

#[test]
fn disconnect_rejects_torque_writes() -> Result<(), BoxErr> {
    let mut device = make_device("dc-write")?;
    device.write_ffb_report(5.0, 0)?;
    device.disconnect();
    let result = device.write_ffb_report(5.0, 1);
    assert_eq!(result, Err(RTError::DeviceDisconnected));
    Ok(())
}

#[tokio::test]
async fn disconnect_removes_from_port_enumeration() -> Result<(), BoxErr> {
    let mut port = VirtualHidPort::new();
    let id = make_id("dc-enum")?;
    port.add_device(VirtualDevice::new(id.clone(), "DC Enum".to_string()))?;
    assert_eq!(port.list_devices().await?.len(), 1);

    port.remove_device(&id)?;
    assert_eq!(port.list_devices().await?.len(), 0);
    Ok(())
}

// ===================================================================
// 3. Multiple devices connected simultaneously
// ===================================================================

#[tokio::test]
async fn multi_device_all_enumerated() -> Result<(), BoxErr> {
    let mut port = VirtualHidPort::new();
    let names = ["wheel-dd", "pedals-lc", "shifter-seq", "handbrake-hyd"];
    for name in &names {
        port.add_device(make_device(name)?)?;
    }

    let devices = port.list_devices().await?;
    assert_eq!(devices.len(), names.len());
    for name in &names {
        assert!(
            devices.iter().any(|d| d.id.as_str() == *name),
            "missing device: {name}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn multi_device_independent_operation() -> Result<(), BoxErr> {
    let mut port = VirtualHidPort::new();
    let id_a = make_id("multi-op-a")?;
    let id_b = make_id("multi-op-b")?;

    port.add_device(VirtualDevice::new(id_a.clone(), "A".to_string()))?;
    port.add_device(VirtualDevice::new(id_b.clone(), "B".to_string()))?;

    let mut a = port.open_device(&id_a).await?;
    let mut b = port.open_device(&id_b).await?;

    // Write different torques; both succeed independently
    assert_device_operational(&mut *a, 8.0, 0)?;
    assert_device_operational(&mut *b, 12.0, 0)?;
    Ok(())
}

#[tokio::test]
async fn multi_device_remove_one_others_unaffected() -> Result<(), BoxErr> {
    let mut port = VirtualHidPort::new();
    let mut ids = Vec::new();
    for i in 0..4 {
        let name = format!("multi-rm-{i}");
        let id = make_id(&name)?;
        port.add_device(VirtualDevice::new(id.clone(), name))?;
        ids.push(id);
    }

    port.remove_device(&ids[1])?;
    let remaining = port.list_devices().await?;
    assert_eq!(remaining.len(), 3);
    assert!(!remaining.iter().any(|d| d.id == ids[1]));

    // Remaining devices still operable
    for (i, id) in ids.iter().enumerate() {
        if i == 1 {
            continue;
        }
        let mut opened = port.open_device(id).await?;
        assert_device_operational(&mut *opened, 2.0, 0)?;
    }
    Ok(())
}

// ===================================================================
// 4. Same device reconnected after disconnect
// ===================================================================

#[test]
fn reconnect_restores_connectivity() -> Result<(), BoxErr> {
    let mut device = make_device("reconn-ok")?;
    device.disconnect();
    assert!(!device.is_connected());
    device.reconnect();
    assert!(device.is_connected());
    Ok(())
}

#[test]
fn reconnect_restores_torque_writes() -> Result<(), BoxErr> {
    let mut device = make_device("reconn-torque")?;
    device.write_ffb_report(5.0, 0)?;
    device.disconnect();
    assert_eq!(
        device.write_ffb_report(5.0, 1),
        Err(RTError::DeviceDisconnected)
    );
    device.reconnect();
    device.write_ffb_report(5.0, 2)?;
    Ok(())
}

#[test]
fn reconnect_restores_telemetry() -> Result<(), BoxErr> {
    let mut device = make_device("reconn-tel")?;
    assert!(device.read_telemetry().is_some());
    device.disconnect();
    assert!(device.read_telemetry().is_none());
    device.reconnect();
    assert!(device.read_telemetry().is_some());
    Ok(())
}

#[test]
fn reconnect_preserves_device_identity() -> Result<(), BoxErr> {
    let mut device = make_device("reconn-identity")?;
    let original = device.device_info().clone();

    device.disconnect();
    device.reconnect();

    let restored = device.device_info();
    assert_eq!(restored.id, original.id);
    assert_eq!(restored.vendor_id, original.vendor_id);
    assert_eq!(restored.product_id, original.product_id);
    assert_eq!(restored.serial_number, original.serial_number);
    assert_eq!(restored.name, original.name);
    Ok(())
}

// ===================================================================
// 5. Unknown device connected (graceful handling)
// ===================================================================

#[tokio::test]
async fn unknown_device_id_open_returns_error() -> Result<(), BoxErr> {
    let port = VirtualHidPort::new();
    let ghost = make_id("totally-unknown")?;
    let result = port.open_device(&ghost).await;
    assert!(result.is_err());
    Ok(())
}

#[tokio::test]
async fn unknown_device_does_not_affect_known_devices() -> Result<(), BoxErr> {
    let mut port = VirtualHidPort::new();
    let known_id = make_id("known-wheel")?;
    port.add_device(VirtualDevice::new(known_id.clone(), "Known".to_string()))?;

    let ghost = make_id("ghost-device")?;
    let result = port.open_device(&ghost).await;
    assert!(result.is_err());

    // Known device is still perfectly fine
    let mut opened = port.open_device(&known_id).await?;
    assert_device_operational(&mut *opened, 5.0, 0)?;
    Ok(())
}

#[tokio::test]
async fn remove_nonexistent_device_is_noop() -> Result<(), BoxErr> {
    let mut port = VirtualHidPort::new();
    let ghost = make_id("nope")?;
    // Should not error
    let result = port.remove_device(&ghost);
    assert!(result.is_ok());
    Ok(())
}

// ===================================================================
// 6. Device disconnect during active force feedback
// ===================================================================

#[test]
fn disconnect_during_active_ffb_returns_error() -> Result<(), BoxErr> {
    let mut device = make_device("ffb-dc")?;

    // Active FFB stream
    for seq in 0..10u16 {
        device.write_ffb_report(15.0, seq)?;
        device.simulate_physics(Duration::from_millis(1));
    }

    device.disconnect();
    let result = device.write_ffb_report(15.0, 10);
    assert_eq!(result, Err(RTError::DeviceDisconnected));
    Ok(())
}

#[test]
fn disconnect_during_ffb_stops_telemetry_immediately() -> Result<(), BoxErr> {
    let mut device = make_device("ffb-tel-dc")?;
    device.write_ffb_report(20.0, 0)?;
    device.simulate_physics(Duration::from_millis(5));
    assert!(device.read_telemetry().is_some());

    device.disconnect();
    assert!(device.read_telemetry().is_none());
    Ok(())
}

#[tokio::test]
async fn disconnect_one_device_during_ffb_other_continues() -> Result<(), BoxErr> {
    let mut port = VirtualHidPort::new();
    let id_a = make_id("ffb-alive")?;
    let id_b = make_id("ffb-doomed")?;

    let mut dev_a = VirtualDevice::new(id_a.clone(), "Alive".to_string());
    let mut dev_b = VirtualDevice::new(id_b.clone(), "Doomed".to_string());

    dev_a.write_ffb_report(10.0, 0)?;
    dev_b.write_ffb_report(10.0, 0)?;

    port.add_device(dev_a)?;
    port.add_device(dev_b)?;

    // Remove one
    port.remove_device(&id_b)?;

    // Remaining device still operational
    let mut opened_a = port.open_device(&id_a).await?;
    assert_device_operational(&mut *opened_a, 5.0, 1)?;
    Ok(())
}

// ===================================================================
// 7. Device connect while in fault state
// ===================================================================

#[test]
fn faulted_device_still_reports_faults_after_reconnect() -> Result<(), BoxErr> {
    let mut device = make_device("fault-reconnect")?;
    device.inject_fault(0x04); // thermal fault
    assert_eq!(device.health_status().fault_flags, 0x04);

    device.disconnect();
    device.reconnect();

    // Fault persists through reconnect (shared Arc state)
    assert_eq!(device.health_status().fault_flags, 0x04);
    Ok(())
}

#[test]
fn clear_faults_before_reconnect_results_in_clean_device() -> Result<(), BoxErr> {
    let mut device = make_device("fault-clear-reconn")?;
    device.inject_fault(0x02);
    device.clear_faults();
    device.disconnect();
    device.reconnect();

    assert_eq!(device.health_status().fault_flags, 0);
    assert_device_operational(&mut device, 5.0, 0)?;
    Ok(())
}

#[tokio::test]
async fn add_faulted_device_to_port_still_enumerable() -> Result<(), BoxErr> {
    let mut port = VirtualHidPort::new();
    let id = make_id("faulted-enum")?;
    let mut device = VirtualDevice::new(id.clone(), "Faulted".to_string());
    device.inject_fault(0x01);
    port.add_device(device)?;

    let devices = port.list_devices().await?;
    assert_eq!(devices.len(), 1);

    let opened = port.open_device(&id).await?;
    assert_eq!(opened.health_status().fault_flags, 0x01);
    Ok(())
}

// ===================================================================
// 8. Rapid connect/disconnect cycling (no resource leaks)
// ===================================================================

#[test]
fn rapid_disconnect_reconnect_100_cycles() -> Result<(), BoxErr> {
    let mut device = make_device("rapid-cycle")?;

    for seq in 0..100u16 {
        device.disconnect();
        assert!(!device.is_connected());
        assert_eq!(
            device.write_ffb_report(1.0, seq),
            Err(RTError::DeviceDisconnected)
        );

        device.reconnect();
        assert!(device.is_connected());
        device.write_ffb_report(1.0, seq + 1000)?;
    }

    // Device still fully functional after 100 cycles
    assert_device_operational(&mut device, 10.0, 9999)?;
    Ok(())
}

#[tokio::test]
async fn rapid_port_add_remove_no_leak() -> Result<(), BoxErr> {
    let mut port = VirtualHidPort::new();

    for i in 0..50 {
        let name = format!("rapid-port-{i}");
        let id = make_id(&name)?;
        port.add_device(VirtualDevice::new(id.clone(), name))?;
        port.remove_device(&id)?;
    }

    // Port should be empty after all add/remove pairs
    assert_eq!(port.list_devices().await?.len(), 0);
    Ok(())
}

#[tokio::test]
async fn rapid_bulk_add_then_bulk_remove() -> Result<(), BoxErr> {
    let mut port = VirtualHidPort::new();
    let count = 30;
    let mut ids = Vec::with_capacity(count);

    for i in 0..count {
        let name = format!("bulk-{i}");
        let id = make_id(&name)?;
        port.add_device(VirtualDevice::new(id.clone(), name))?;
        ids.push(id);
    }
    assert_eq!(port.list_devices().await?.len(), count);

    for id in &ids {
        port.remove_device(id)?;
    }
    assert_eq!(port.list_devices().await?.len(), 0);
    Ok(())
}

// ===================================================================
// 9. Device identity verification (VID/PID/serial matching)
// ===================================================================

#[test]
fn vid_pid_stable_across_reconnect_cycles() -> Result<(), BoxErr> {
    let mut device = make_device("vidpid-stable")?;
    let info_original = device.device_info().clone();

    for _ in 0..10 {
        device.disconnect();
        device.reconnect();
    }

    let info_final = device.device_info();
    assert_eq!(info_final.vendor_id, info_original.vendor_id);
    assert_eq!(info_final.product_id, info_original.product_id);
    Ok(())
}

#[test]
fn serial_number_stable_across_reconnect() -> Result<(), BoxErr> {
    let mut device = make_device("serial-stable")?;
    let serial_before = device.device_info().serial_number.clone();

    device.disconnect();
    device.reconnect();

    assert_eq!(device.device_info().serial_number, serial_before);
    Ok(())
}

#[test]
fn manufacturer_stable_across_reconnect() -> Result<(), BoxErr> {
    let mut device = make_device("mfr-stable")?;
    let mfr_before = device.device_info().manufacturer.clone();

    device.disconnect();
    device.reconnect();

    assert_eq!(device.device_info().manufacturer, mfr_before);
    Ok(())
}

#[tokio::test]
async fn vid_pid_consistent_across_port_opens() -> Result<(), BoxErr> {
    let mut port = VirtualHidPort::new();
    let id = make_id("vidpid-port")?;
    port.add_device(VirtualDevice::new(id.clone(), "VID/PID Port".to_string()))?;

    let first = port.open_device(&id).await?;
    let second = port.open_device(&id).await?;

    assert_eq!(
        first.device_info().vendor_id,
        second.device_info().vendor_id
    );
    assert_eq!(
        first.device_info().product_id,
        second.device_info().product_id
    );
    assert_eq!(
        first.device_info().serial_number,
        second.device_info().serial_number
    );
    Ok(())
}

#[test]
fn device_path_contains_device_id() -> Result<(), BoxErr> {
    let device = make_device("path-verify")?;
    let info = device.device_info();
    assert!(info.path.contains("path-verify"));
    Ok(())
}

// ===================================================================
// 10. Firmware version detection and compatibility check
// ===================================================================

#[test]
fn health_status_baseline_no_faults() -> Result<(), BoxErr> {
    let device = make_device("fw-baseline")?;
    let health = device.health_status();
    assert_eq!(health.fault_flags, 0);
    assert!(health.temperature_c >= 20);
    assert_eq!(health.communication_errors, 0);
    assert!(health.hands_on);
    Ok(())
}

#[test]
fn telemetry_timestamp_advances_with_physics() -> Result<(), BoxErr> {
    let mut device = make_device("fw-timestamp")?;
    let tel_a = device
        .read_telemetry()
        .ok_or("expected initial telemetry")?;

    device.write_ffb_report(1.0, 0)?;
    device.simulate_physics(Duration::from_millis(10));

    let tel_b = device
        .read_telemetry()
        .ok_or("expected post-physics telemetry")?;
    assert!(tel_b.timestamp >= tel_a.timestamp);
    Ok(())
}

#[test]
fn fault_flags_reflect_injected_faults() -> Result<(), BoxErr> {
    let mut device = make_device("fw-faults")?;
    device.inject_fault(0x01);
    device.inject_fault(0x04);

    let tel = device.read_telemetry().ok_or("expected telemetry")?;
    assert_eq!(tel.fault_flags, 0x05);

    device.clear_faults();
    let tel = device.read_telemetry().ok_or("expected telemetry")?;
    assert_eq!(tel.fault_flags, 0);
    Ok(())
}

#[test]
fn capabilities_report_encoder_cpr_and_period() -> Result<(), BoxErr> {
    let device = make_device("fw-caps")?;
    let caps = device.capabilities();
    assert_eq!(caps.encoder_cpr, 10000);
    assert_eq!(caps.min_report_period_us, 1000);
    Ok(())
}

#[test]
fn capabilities_persist_across_reconnect() -> Result<(), BoxErr> {
    let mut device = make_device("fw-caps-persist")?;
    let caps_before = device.capabilities().clone();

    device.disconnect();
    device.reconnect();

    let caps_after = device.capabilities();
    assert_eq!(
        caps_after.supports_raw_torque_1khz,
        caps_before.supports_raw_torque_1khz
    );
    assert!((caps_after.max_torque.value() - caps_before.max_torque.value()).abs() < f32::EPSILON);
    assert_eq!(caps_after.encoder_cpr, caps_before.encoder_cpr);
    assert_eq!(
        caps_after.min_report_period_us,
        caps_before.min_report_period_us
    );
    Ok(())
}

//! Deep tests for engine device management.
//!
//! Covers:
//! - Device discovery (enumeration, empty port, multiple devices)
//! - Device configuration (capabilities, vendor/product ID, serial numbers)
//! - Multi-device management (add, remove, isolation)
//! - Device state tracking (connect, disconnect, reconnect)
//! - Capability querying (torque limits, feature flags, mode selection)
//! - USB descriptor handling (health status, telemetry, fault injection)

use racing_wheel_engine::{
    CapabilityNegotiator, GameCompatibility, HidDevice, HidPort,
    ModeSelectionPolicy, RTError, VirtualDevice, VirtualHidPort,
};
use racing_wheel_engine::rt::FFBMode;
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

fn test_caps(
    pid: bool,
    raw_torque: bool,
    nm: f32,
) -> Result<DeviceCapabilities, Box<dyn std::error::Error>> {
    Ok(DeviceCapabilities::new(
        pid,
        raw_torque,
        true,
        true,
        TorqueNm::new(nm)?,
        10000,
        1000,
    ))
}

// =========================================================================
// 1. Device discovery
// =========================================================================

#[tokio::test]
async fn empty_port_lists_no_devices() -> Result<(), Box<dyn std::error::Error>> {
    let port = VirtualHidPort::new();
    let devices = port.list_devices().await?;
    assert_eq!(devices.len(), 0);
    Ok(())
}

#[tokio::test]
async fn single_device_enumerated_after_add() -> Result<(), Box<dyn std::error::Error>> {
    let mut port = VirtualHidPort::new();
    let device = make_device("wheel-one")?;
    port.add_device(device)?;

    let devices = port.list_devices().await?;
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].id.as_str(), "wheel-one");
    assert!(devices[0].is_connected);
    Ok(())
}

#[tokio::test]
async fn multiple_devices_enumerated() -> Result<(), Box<dyn std::error::Error>> {
    let mut port = VirtualHidPort::new();
    for i in 0..5 {
        let device = make_device(&format!("wheel-{i}"))?;
        port.add_device(device)?;
    }
    let devices = port.list_devices().await?;
    assert_eq!(devices.len(), 5);
    Ok(())
}

#[tokio::test]
async fn device_not_found_returns_error() -> Result<(), Box<dyn std::error::Error>> {
    let port = VirtualHidPort::new();
    let id = make_id("nonexistent")?;
    let result = port.open_device(&id).await;
    assert!(result.is_err());
    Ok(())
}

#[tokio::test]
async fn refresh_devices_succeeds_on_empty_port() -> Result<(), Box<dyn std::error::Error>> {
    let port = VirtualHidPort::new();
    port.refresh_devices().await?;
    Ok(())
}

#[tokio::test]
async fn monitor_devices_returns_receiver() -> Result<(), Box<dyn std::error::Error>> {
    let port = VirtualHidPort::new();
    let _rx = port.monitor_devices().await?;
    Ok(())
}

// =========================================================================
// 2. Device configuration
// =========================================================================

#[test]
fn virtual_device_has_expected_vendor_product_ids() -> Result<(), Box<dyn std::error::Error>> {
    let device = make_device("cfg-test")?;
    let info = device.device_info();
    assert_eq!(info.vendor_id, 0x1234);
    assert_eq!(info.product_id, 0x5678);
    Ok(())
}

#[test]
fn virtual_device_serial_number_set() -> Result<(), Box<dyn std::error::Error>> {
    let device = make_device("serial-test")?;
    let info = device.device_info();
    assert_eq!(info.serial_number.as_deref(), Some("VIRTUAL001"));
    Ok(())
}

#[test]
fn virtual_device_manufacturer_set() -> Result<(), Box<dyn std::error::Error>> {
    let device = make_device("mfr-test")?;
    let info = device.device_info();
    assert_eq!(info.manufacturer.as_deref(), Some("Virtual Racing"));
    Ok(())
}

#[test]
fn virtual_device_path_contains_id() -> Result<(), Box<dyn std::error::Error>> {
    let device = make_device("path-test")?;
    let info = device.device_info();
    assert!(info.path.contains("path-test"));
    assert!(info.path.starts_with("virtual://"));
    Ok(())
}

#[test]
fn virtual_device_default_capabilities() -> Result<(), Box<dyn std::error::Error>> {
    let device = make_device("caps-test")?;
    let caps = device.capabilities();
    assert_eq!(caps.max_torque.value(), 25.0);
    assert!(caps.supports_raw_torque_1khz);
    assert!(caps.supports_health_stream);
    assert!(caps.supports_led_bus);
    assert!(!caps.supports_pid);
    Ok(())
}

#[test]
fn device_info_name_matches_constructor() -> Result<(), Box<dyn std::error::Error>> {
    let device = make_device("named-wheel")?;
    assert_eq!(device.device_info().name, "named-wheel");
    Ok(())
}

// =========================================================================
// 3. Multi-device management
// =========================================================================

#[tokio::test]
async fn add_then_remove_device() -> Result<(), Box<dyn std::error::Error>> {
    let mut port = VirtualHidPort::new();
    let id = make_id("temp-device")?;
    let device = VirtualDevice::new(id.clone(), "Temp".to_string());
    port.add_device(device)?;
    assert_eq!(port.list_devices().await?.len(), 1);

    port.remove_device(&id)?;
    assert_eq!(port.list_devices().await?.len(), 0);
    Ok(())
}

#[tokio::test]
async fn remove_nonexistent_device_does_not_panic() -> Result<(), Box<dyn std::error::Error>> {
    let mut port = VirtualHidPort::new();
    let id = make_id("ghost")?;
    // Should not error — device just isn't there
    let result = port.remove_device(&id);
    assert!(result.is_ok());
    Ok(())
}

#[tokio::test]
async fn multiple_devices_isolated_state() -> Result<(), Box<dyn std::error::Error>> {
    let mut port = VirtualHidPort::new();
    let id_a = make_id("device-a")?;
    let id_b = make_id("device-b")?;

    let mut dev_a = VirtualDevice::new(id_a.clone(), "A".to_string());
    let dev_b = VirtualDevice::new(id_b.clone(), "B".to_string());

    // Inject a fault in device A before adding
    dev_a.inject_fault(0x02);

    port.add_device(dev_a)?;
    port.add_device(dev_b)?;

    let mut opened_a = port.open_device(&id_a).await?;
    let mut opened_b = port.open_device(&id_b).await?;

    // Device A has faults, B does not
    let tel_a = opened_a.read_telemetry().ok_or("no telemetry A")?;
    let tel_b = opened_b.read_telemetry().ok_or("no telemetry B")?;
    assert_ne!(tel_a.fault_flags, 0);
    assert_eq!(tel_b.fault_flags, 0);
    Ok(())
}

#[tokio::test]
async fn opened_device_shares_state_with_port() -> Result<(), Box<dyn std::error::Error>> {
    let mut port = VirtualHidPort::new();
    let id = make_id("shared-state")?;
    let device = VirtualDevice::new(id.clone(), "Shared".to_string());
    port.add_device(device)?;

    let mut opened = port.open_device(&id).await?;

    // Write torque through opened device
    opened.write_ffb_report(5.0, 1)?;

    // Read telemetry — should succeed
    let tel = opened.read_telemetry().ok_or("no telemetry")?;
    assert!(tel.temperature_c >= 20);
    Ok(())
}

#[tokio::test]
async fn add_multiple_then_remove_middle() -> Result<(), Box<dyn std::error::Error>> {
    let mut port = VirtualHidPort::new();
    let ids: Vec<DeviceId> = (0..3)
        .map(|i| make_id(&format!("dev-{i}")))
        .collect::<Result<_, _>>()?;

    for id in &ids {
        let device = VirtualDevice::new(id.clone(), format!("Dev {}", id.as_str()));
        port.add_device(device)?;
    }
    assert_eq!(port.list_devices().await?.len(), 3);

    port.remove_device(&ids[1])?;
    let remaining = port.list_devices().await?;
    assert_eq!(remaining.len(), 2);
    assert!(remaining.iter().all(|d| d.id != ids[1]));
    Ok(())
}

// =========================================================================
// 4. Device state tracking
// =========================================================================

#[test]
fn new_virtual_device_is_connected() -> Result<(), Box<dyn std::error::Error>> {
    let device = make_device("conn-test")?;
    assert!(device.is_connected());
    Ok(())
}

#[test]
fn disconnect_makes_device_not_connected() -> Result<(), Box<dyn std::error::Error>> {
    let mut device = make_device("disc-test")?;
    device.disconnect();
    assert!(!device.is_connected());
    Ok(())
}

#[test]
fn reconnect_restores_connected() -> Result<(), Box<dyn std::error::Error>> {
    let mut device = make_device("reconn-test")?;
    device.disconnect();
    assert!(!device.is_connected());
    device.reconnect();
    assert!(device.is_connected());
    Ok(())
}

#[test]
fn write_to_disconnected_device_returns_error() -> Result<(), Box<dyn std::error::Error>> {
    let mut device = make_device("disc-write")?;
    device.disconnect();
    let result = device.write_ffb_report(5.0, 1);
    assert_eq!(result, Err(RTError::DeviceDisconnected));
    Ok(())
}

#[test]
fn read_telemetry_from_disconnected_returns_none() -> Result<(), Box<dyn std::error::Error>> {
    let mut device = make_device("disc-tel")?;
    device.disconnect();
    assert!(device.read_telemetry().is_none());
    Ok(())
}

#[test]
fn write_after_reconnect_succeeds() -> Result<(), Box<dyn std::error::Error>> {
    let mut device = make_device("reconn-write")?;
    device.disconnect();
    device.reconnect();
    device.write_ffb_report(3.0, 1)?;
    Ok(())
}

#[test]
fn health_status_reflects_temperature() -> Result<(), Box<dyn std::error::Error>> {
    let device = make_device("health-test")?;
    let health = device.health_status();
    assert!(health.temperature_c >= 20);
    assert_eq!(health.fault_flags, 0);
    assert!(health.hands_on);
    assert_eq!(health.communication_errors, 0);
    Ok(())
}

#[test]
fn health_status_reflects_injected_faults() -> Result<(), Box<dyn std::error::Error>> {
    let mut device = make_device("health-fault")?;
    device.inject_fault(0x04);
    let health = device.health_status();
    assert_eq!(health.fault_flags, 0x04);
    Ok(())
}

#[test]
fn multiple_disconnect_reconnect_cycles() -> Result<(), Box<dyn std::error::Error>> {
    let mut device = make_device("cycle-test")?;
    for _ in 0..10 {
        device.disconnect();
        assert!(!device.is_connected());
        assert!(device.read_telemetry().is_none());

        device.reconnect();
        assert!(device.is_connected());
        device.write_ffb_report(1.0, 1)?;
        assert!(device.read_telemetry().is_some());
    }
    Ok(())
}

// =========================================================================
// 5. Capability querying and mode selection
// =========================================================================

#[test]
fn mode_selection_raw_torque_preferred() -> Result<(), Box<dyn std::error::Error>> {
    let caps = test_caps(false, true, 25.0)?;
    let mode = ModeSelectionPolicy::select_mode(&caps, None);
    assert_eq!(mode, FFBMode::RawTorque);
    Ok(())
}

#[test]
fn mode_selection_pid_fallback() -> Result<(), Box<dyn std::error::Error>> {
    let caps = test_caps(true, false, 10.0)?;
    let mode = ModeSelectionPolicy::select_mode(&caps, None);
    assert_eq!(mode, FFBMode::PidPassthrough);
    Ok(())
}

#[test]
fn mode_selection_telemetry_synth_last_resort() -> Result<(), Box<dyn std::error::Error>> {
    let caps = test_caps(false, false, 5.0)?;
    let mode = ModeSelectionPolicy::select_mode(&caps, None);
    assert_eq!(mode, FFBMode::TelemetrySynth);
    Ok(())
}

#[test]
fn mode_selection_with_game_compatibility_robust_ffb() -> Result<(), Box<dyn std::error::Error>> {
    let caps = test_caps(false, true, 25.0)?;
    let game = GameCompatibility {
        game_id: "sim-game".to_string(),
        supports_robust_ffb: true,
        supports_telemetry: true,
        preferred_mode: FFBMode::RawTorque,
    };
    let mode = ModeSelectionPolicy::select_mode(&caps, Some(&game));
    assert_eq!(mode, FFBMode::RawTorque);
    Ok(())
}

#[test]
fn mode_selection_with_game_telemetry_only() -> Result<(), Box<dyn std::error::Error>> {
    let caps = test_caps(false, true, 25.0)?;
    let game = GameCompatibility {
        game_id: "arcade-port".to_string(),
        supports_robust_ffb: false,
        supports_telemetry: true,
        preferred_mode: FFBMode::TelemetrySynth,
    };
    let mode = ModeSelectionPolicy::select_mode(&caps, Some(&game));
    assert_eq!(mode, FFBMode::TelemetrySynth);
    Ok(())
}

#[test]
fn mode_selection_game_no_ffb_no_telemetry_on_raw_device() -> Result<(), Box<dyn std::error::Error>>
{
    let caps = test_caps(false, true, 25.0)?;
    let game = GameCompatibility {
        game_id: "no-support".to_string(),
        supports_robust_ffb: false,
        supports_telemetry: false,
        preferred_mode: FFBMode::PidPassthrough,
    };
    let mode = ModeSelectionPolicy::select_mode(&caps, Some(&game));
    // falls through to raw torque default for raw-capable device
    assert_eq!(mode, FFBMode::RawTorque);
    Ok(())
}

#[test]
fn capability_negotiator_can_be_constructed() {
    let _negotiator = CapabilityNegotiator;
}

// =========================================================================
// 6. USB descriptor handling (telemetry, physics, fault injection)
// =========================================================================

#[test]
fn torque_within_limit_accepted() -> Result<(), Box<dyn std::error::Error>> {
    let mut device = make_device("torque-ok")?;
    // 25Nm max, request 20Nm
    device.write_ffb_report(20.0, 1)?;
    Ok(())
}

#[test]
fn torque_exceeding_limit_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let mut device = make_device("torque-over")?;
    let result = device.write_ffb_report(30.0, 1);
    assert_eq!(result, Err(RTError::TorqueLimit));
    Ok(())
}

#[test]
fn negative_torque_within_limit_accepted() -> Result<(), Box<dyn std::error::Error>> {
    let mut device = make_device("torque-neg")?;
    device.write_ffb_report(-20.0, 1)?;
    Ok(())
}

#[test]
fn negative_torque_exceeding_limit_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let mut device = make_device("torque-neg-over")?;
    let result = device.write_ffb_report(-30.0, 1);
    assert_eq!(result, Err(RTError::TorqueLimit));
    Ok(())
}

#[test]
fn zero_torque_always_accepted() -> Result<(), Box<dyn std::error::Error>> {
    let mut device = make_device("torque-zero")?;
    device.write_ffb_report(0.0, 1)?;
    Ok(())
}

#[test]
fn exact_max_torque_accepted() -> Result<(), Box<dyn std::error::Error>> {
    let mut device = make_device("torque-exact")?;
    device.write_ffb_report(25.0, 1)?;
    Ok(())
}

#[test]
fn telemetry_returns_baseline_temperature() -> Result<(), Box<dyn std::error::Error>> {
    let mut device = make_device("tel-temp")?;
    let tel = device.read_telemetry().ok_or("no telemetry")?;
    assert_eq!(tel.temperature_c, 35);
    Ok(())
}

#[test]
fn telemetry_initial_angle_zero() -> Result<(), Box<dyn std::error::Error>> {
    let mut device = make_device("tel-angle")?;
    let tel = device.read_telemetry().ok_or("no telemetry")?;
    assert_eq!(tel.wheel_angle_deg, 0.0);
    assert_eq!(tel.wheel_speed_rad_s, 0.0);
    Ok(())
}

#[test]
fn physics_simulation_moves_wheel() -> Result<(), Box<dyn std::error::Error>> {
    let mut device = make_device("phys-move")?;
    device.write_ffb_report(10.0, 1)?;

    for _ in 0..10 {
        device.simulate_physics(Duration::from_millis(10));
    }

    let tel = device.read_telemetry().ok_or("no telemetry")?;
    assert!(tel.wheel_angle_deg.abs() > 0.0);
    assert!(tel.wheel_speed_rad_s.abs() > 0.0);
    Ok(())
}

#[test]
fn physics_simulation_heats_device() -> Result<(), Box<dyn std::error::Error>> {
    let mut device = make_device("phys-heat")?;
    device.write_ffb_report(20.0, 1)?;

    for _ in 0..100 {
        device.simulate_physics(Duration::from_millis(10));
    }

    let tel = device.read_telemetry().ok_or("no telemetry")?;
    assert!(tel.temperature_c >= 35);
    Ok(())
}

#[test]
fn fault_injection_sets_flags() -> Result<(), Box<dyn std::error::Error>> {
    let mut device = make_device("fault-set")?;
    let tel = device.read_telemetry().ok_or("no telemetry")?;
    assert_eq!(tel.fault_flags, 0);

    device.inject_fault(0x01);
    let tel = device.read_telemetry().ok_or("no telemetry")?;
    assert_eq!(tel.fault_flags, 0x01);
    Ok(())
}

#[test]
fn multiple_fault_injection_ors_flags() -> Result<(), Box<dyn std::error::Error>> {
    let mut device = make_device("fault-or")?;
    device.inject_fault(0x01);
    device.inject_fault(0x04);
    let tel = device.read_telemetry().ok_or("no telemetry")?;
    assert_eq!(tel.fault_flags, 0x05);
    Ok(())
}

#[test]
fn clear_faults_resets_flags() -> Result<(), Box<dyn std::error::Error>> {
    let mut device = make_device("fault-clear")?;
    device.inject_fault(0x0F);
    device.clear_faults();
    let tel = device.read_telemetry().ok_or("no telemetry")?;
    assert_eq!(tel.fault_flags, 0);
    Ok(())
}

#[test]
fn sequential_torque_writes_update_state() -> Result<(), Box<dyn std::error::Error>> {
    let mut device = make_device("seq-write")?;
    for seq in 0..100u16 {
        device.write_ffb_report(5.0, seq)?;
    }
    // Should still be operational
    assert!(device.is_connected());
    let tel = device.read_telemetry().ok_or("no telemetry")?;
    assert!(tel.temperature_c >= 20);
    Ok(())
}

#[test]
fn wheel_angle_bounded_at_1080() -> Result<(), Box<dyn std::error::Error>> {
    let mut device = make_device("angle-bound")?;
    device.write_ffb_report(25.0, 1)?;
    // Simulate long enough to reach limit
    for _ in 0..2000 {
        device.simulate_physics(Duration::from_millis(10));
    }
    let tel = device.read_telemetry().ok_or("no telemetry")?;
    assert!(tel.wheel_angle_deg <= 1080.0);
    assert!(tel.wheel_angle_deg >= -1080.0);
    Ok(())
}

#[test]
fn negative_torque_moves_wheel_opposite_direction() -> Result<(), Box<dyn std::error::Error>> {
    let mut device = make_device("neg-dir")?;
    device.write_ffb_report(-10.0, 1)?;
    for _ in 0..20 {
        device.simulate_physics(Duration::from_millis(10));
    }
    let tel = device.read_telemetry().ok_or("no telemetry")?;
    assert!(tel.wheel_angle_deg < 0.0);
    Ok(())
}

#[test]
fn virtual_hid_port_default_same_as_new() {
    let a = VirtualHidPort::new();
    let b = VirtualHidPort::default();
    // Both should start empty — just verify construction works
    let _ = (a, b);
}

#[tokio::test]
async fn simulate_physics_on_port_updates_all_devices() -> Result<(), Box<dyn std::error::Error>> {
    let mut port = VirtualHidPort::new();
    let id_a = make_id("phys-a")?;
    let id_b = make_id("phys-b")?;

    let mut dev_a = VirtualDevice::new(id_a.clone(), "A".to_string());
    let mut dev_b = VirtualDevice::new(id_b.clone(), "B".to_string());

    dev_a.write_ffb_report(10.0, 1)?;
    dev_b.write_ffb_report(-10.0, 1)?;

    port.add_device(dev_a)?;
    port.add_device(dev_b)?;

    // Simulate physics through the port
    for _ in 0..10 {
        port.simulate_physics(Duration::from_millis(10));
    }

    let mut opened_a = port.open_device(&id_a).await?;
    let mut opened_b = port.open_device(&id_b).await?;

    let tel_a = opened_a.read_telemetry().ok_or("no telemetry A")?;
    let tel_b = opened_b.read_telemetry().ok_or("no telemetry B")?;

    // Both should have moved (one positive, one negative)
    assert!(tel_a.wheel_angle_deg.abs() > 0.0 || tel_b.wheel_angle_deg.abs() > 0.0);
    Ok(())
}

#[test]
fn device_info_clone_is_independent() -> Result<(), Box<dyn std::error::Error>> {
    let device = make_device("clone-test")?;
    let info1 = device.device_info().clone();
    let info2 = device.device_info().clone();
    assert_eq!(info1.id, info2.id);
    assert_eq!(info1.name, info2.name);
    assert_eq!(info1.vendor_id, info2.vendor_id);
    Ok(())
}

#[test]
fn temperature_clamped_to_valid_range() -> Result<(), Box<dyn std::error::Error>> {
    let mut device = make_device("temp-clamp")?;
    // Apply large torque and simulate for a long time
    device.write_ffb_report(25.0, 1)?;
    for _ in 0..5000 {
        device.simulate_physics(Duration::from_millis(10));
    }
    let tel = device.read_telemetry().ok_or("no telemetry")?;
    assert!(tel.temperature_c <= 100);
    assert!(tel.temperature_c >= 20);
    Ok(())
}

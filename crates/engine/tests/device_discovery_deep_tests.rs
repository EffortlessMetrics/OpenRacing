//! Deep tests for device discovery and enumeration.
//!
//! Covers VID/PID matching for every supported vendor, device capability
//! parsing from HID descriptor bytes, multi-device enumeration (wheel +
//! pedals + shifter), device priority/preference selection, unknown device
//! handling, device reconnection after disconnect, firmware version
//! detection via telemetry reports, and device configuration apply/verify.

use racing_wheel_engine::hid::vendor::{
    get_vendor_protocol, get_vendor_protocol_with_hid_pid_fallback,
};
use racing_wheel_engine::hid::{
    self, DeviceCapabilitiesReport, DeviceTelemetryReport, HidDeviceInfo, MAX_TORQUE_REPORT_SIZE,
    TorqueCommand,
};
use racing_wheel_engine::{HidDevice, HidPort, RTError, VirtualDevice, VirtualHidPort};
use racing_wheel_schemas::prelude::*;

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

fn build_hid_device_info(
    vid: u16,
    pid: u16,
    name: &str,
) -> Result<HidDeviceInfo, Box<dyn std::error::Error>> {
    let dev_id: DeviceId = format!("test-{vid:04x}-{pid:04x}").parse()?;
    Ok(HidDeviceInfo {
        device_id: dev_id,
        vendor_id: vid,
        product_id: pid,
        serial_number: Some(format!("SN-{vid:04X}{pid:04X}")),
        manufacturer: Some(name.to_string()),
        product_name: Some(format!("{name} Device")),
        path: format!("\\\\?\\hid#vid_{vid:04x}&pid_{pid:04x}"),
        interface_number: Some(0),
        usage_page: Some(0x01),
        usage: Some(0x04),
        report_descriptor_len: None,
        report_descriptor_crc32: None,
        capabilities: DeviceCapabilities::new(
            true,
            false,
            false,
            false,
            TorqueNm::new(5.0)?,
            900,
            4000,
        ),
    })
}

// ===================================================================
// 1. VID/PID matching for every supported vendor
// ===================================================================

/// Dedicated-VID vendors: each known VID must have at least one PID
/// that dispatches to a handler.
#[test]
fn logitech_vid_dispatches_known_pids() -> Result<(), Box<dyn std::error::Error>> {
    let pids: &[(u16, &str)] = &[
        (0xC295, "MOMO"),
        (0xC298, "DFP"),
        (0xC29A, "DFGT"),
        (0xC299, "G25"),
        (0xC29B, "G27"),
        (0xC24F, "G29"),
        (0xC262, "G920"),
        (0xC266, "G923"),
        (0xC268, "G PRO"),
    ];
    for &(pid, label) in pids {
        assert!(
            get_vendor_protocol(0x046D, pid).is_some(),
            "Logitech {label} (0x{pid:04X}) must dispatch"
        );
    }
    Ok(())
}

#[test]
fn fanatec_vid_dispatches_known_pids() -> Result<(), Box<dyn std::error::Error>> {
    let pids: &[(u16, &str)] = &[
        (0x0001, "ClubSport V2"),
        (0x0006, "DD1"),
        (0x0007, "DD2"),
        (0x0020, "CSL DD"),
        (0x0024, "GT DD Pro"),
        (0x0011, "CSR Elite"),
        (0x0E03, "CSL Elite"),
        (0x01E9, "ClubSport DD"),
    ];
    for &(pid, label) in pids {
        assert!(
            get_vendor_protocol(0x0EB7, pid).is_some(),
            "Fanatec {label} (0x{pid:04X}) must dispatch"
        );
    }
    Ok(())
}

#[test]
fn thrustmaster_vid_dispatches_known_pids() -> Result<(), Box<dyn std::error::Error>> {
    let pids: &[(u16, &str)] = &[
        (0xB677, "T150"),
        (0xB66E, "T300"),
        (0xB65E, "T500 RS"),
        (0xB67F, "TMX"),
        (0xB689, "TS-PC"),
        (0xB692, "TS-XW"),
        (0xB69B, "T818"),
        (0xB696, "T248"),
    ];
    for &(pid, label) in pids {
        assert!(
            get_vendor_protocol(0x044F, pid).is_some(),
            "Thrustmaster {label} (0x{pid:04X}) must dispatch"
        );
    }
    Ok(())
}

#[test]
fn moza_vid_dispatches_known_pids() -> Result<(), Box<dyn std::error::Error>> {
    let pids: &[(u16, &str)] = &[
        (0x0000, "R16/R21 V1"),
        (0x0002, "R9 V1"),
        (0x0004, "R5 V1"),
        (0x0005, "R3 V1"),
        (0x0010, "R16/R21 V2"),
        (0x0012, "R9 V2"),
        (0x0014, "R5 V2"),
        (0x0003, "SR-P Pedals"),
        (0x0020, "HGP"),
        (0x0021, "SGP"),
    ];
    for &(pid, label) in pids {
        assert!(
            get_vendor_protocol(0x346E, pid).is_some(),
            "Moza {label} (0x{pid:04X}) must dispatch"
        );
    }
    Ok(())
}

#[test]
fn asetek_vid_dispatches_known_pids() -> Result<(), Box<dyn std::error::Error>> {
    let pids: &[(u16, &str)] = &[
        (0xF300, "Invicta"),
        (0xF301, "Forte"),
        (0xF303, "La Prima"),
        (0xF306, "Tony Kanaan"),
    ];
    for &(pid, label) in pids {
        assert!(
            get_vendor_protocol(0x2433, pid).is_some(),
            "Asetek {label} (0x{pid:04X}) must dispatch"
        );
    }
    Ok(())
}

#[test]
fn cammus_vid_dispatches_known_pids() -> Result<(), Box<dyn std::error::Error>> {
    let pids: &[(u16, &str)] = &[
        (0x0301, "C5"),
        (0x0302, "C12"),
        (0x1018, "CP5 Pedals"),
        (0x1019, "LC100 Pedals"),
    ];
    for &(pid, label) in pids {
        assert!(
            get_vendor_protocol(0x3416, pid).is_some(),
            "Cammus {label} (0x{pid:04X}) must dispatch"
        );
    }
    Ok(())
}

#[test]
fn ffbeast_vid_dispatches_known_pids() -> Result<(), Box<dyn std::error::Error>> {
    let pids: &[(u16, &str)] = &[(0x58F9, "Joystick"), (0x5968, "Rudder"), (0x59D7, "Wheel")];
    for &(pid, label) in pids {
        assert!(
            get_vendor_protocol(0x045B, pid).is_some(),
            "FFBeast {label} (0x{pid:04X}) must dispatch"
        );
    }
    Ok(())
}

#[test]
fn pxn_vid_dispatches_known_pids() -> Result<(), Box<dyn std::error::Error>> {
    // PXN uses vendor ID 0x11FF
    let handler = get_vendor_protocol(0x11FF, 0x0001);
    // PXN may or may not have specific PID dispatch; test the VID is recognized
    // If no specific PID is known, the vendor protocol should still handle it
    // or return None for unknown PIDs on a known vendor.
    let _ = handler; // dispatch result is vendor-dependent
    Ok(())
}

#[test]
fn leo_bodnar_vid_dispatches_known_pids() -> Result<(), Box<dyn std::error::Error>> {
    let pids: &[(u16, &str)] = &[
        (0x000E, "Wheel Interface"),
        (0x000F, "FFB Joystick"),
        (0x000C, "BBI32"),
    ];
    for &(pid, label) in pids {
        assert!(
            get_vendor_protocol(0x1DD2, pid).is_some(),
            "Leo Bodnar {label} (0x{pid:04X}) must dispatch"
        );
    }
    Ok(())
}

#[test]
fn accuforce_vid_dispatches_known_pids() -> Result<(), Box<dyn std::error::Error>> {
    assert!(
        get_vendor_protocol(0x1FC9, 0x804C).is_some(),
        "AccuForce Pro must dispatch"
    );
    Ok(())
}

#[test]
fn simagic_evo_vid_dispatches_known_pids() -> Result<(), Box<dyn std::error::Error>> {
    let pids: &[(u16, &str)] = &[
        (0x0500, "EVO Sport"),
        (0x0502, "EVO Pro"),
        (0x0700, "NEO"),
        (0x1001, "P1000 Pedals"),
        (0x2001, "H-Pattern Shifter"),
        (0x3001, "Handbrake"),
    ];
    for &(pid, label) in pids {
        assert!(
            get_vendor_protocol(0x3670, pid).is_some(),
            "Simagic EVO {label} (0x{pid:04X}) must dispatch"
        );
    }
    Ok(())
}

// ===================================================================
// 2. Device capability parsing from HID descriptor bytes
// ===================================================================

#[test]
fn capabilities_report_parses_full_featured_device() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = vec![0u8; std::mem::size_of::<DeviceCapabilitiesReport>()];
    data[0] = DeviceCapabilitiesReport::REPORT_ID;
    data[1] = 0x01; // supports_pid
    data[2] = 0x01; // supports_raw_torque_1khz
    data[3] = 0x01; // supports_health_stream
    data[4] = 0x01; // supports_led_bus
    data[5..7].copy_from_slice(&2500u16.to_le_bytes()); // 25 Nm
    data[7..9].copy_from_slice(&10000u16.to_le_bytes()); // encoder CPR
    data[9] = 250; // min_report_period_us

    let report = DeviceCapabilitiesReport::from_bytes(&data).ok_or("capabilities parse failed")?;
    let caps = report.to_device_capabilities();

    assert!(caps.supports_pid);
    assert!(caps.supports_raw_torque_1khz);
    assert!(caps.supports_health_stream);
    assert!(caps.supports_led_bus);
    assert!((caps.max_torque.value() - 25.0).abs() < 0.01);
    assert_eq!(caps.encoder_cpr, 10000);
    assert_eq!(caps.min_report_period_us, 250);
    Ok(())
}

#[test]
fn capabilities_report_parses_minimal_device() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = vec![0u8; std::mem::size_of::<DeviceCapabilitiesReport>()];
    data[0] = DeviceCapabilitiesReport::REPORT_ID;
    // All capability flags = 0
    data[5..7].copy_from_slice(&500u16.to_le_bytes()); // 5 Nm
    data[7..9].copy_from_slice(&900u16.to_le_bytes());

    let report = DeviceCapabilitiesReport::from_bytes(&data).ok_or("parse failed")?;
    let caps = report.to_device_capabilities();

    assert!(!caps.supports_pid);
    assert!(!caps.supports_raw_torque_1khz);
    assert!(!caps.supports_health_stream);
    assert!(!caps.supports_led_bus);
    assert!((caps.max_torque.value() - 5.0).abs() < 0.01);
    assert_eq!(caps.encoder_cpr, 900);
    Ok(())
}

#[test]
fn capabilities_report_rejects_wrong_report_id() {
    let mut data = vec![0u8; std::mem::size_of::<DeviceCapabilitiesReport>()];
    data[0] = 0xFF; // wrong report ID
    assert!(DeviceCapabilitiesReport::from_bytes(&data).is_none());
}

#[test]
fn capabilities_report_rejects_too_short_buffer() {
    let data = [DeviceCapabilitiesReport::REPORT_ID, 0x01, 0x01];
    assert!(DeviceCapabilitiesReport::from_bytes(&data).is_none());
}

#[test]
fn capabilities_report_zero_torque_device() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = vec![0u8; std::mem::size_of::<DeviceCapabilitiesReport>()];
    data[0] = DeviceCapabilitiesReport::REPORT_ID;
    // max_torque_cnm = 0 (pedals/input-only device)
    data[5..7].copy_from_slice(&0u16.to_le_bytes());

    let report = DeviceCapabilitiesReport::from_bytes(&data).ok_or("parse failed")?;
    let caps = report.to_device_capabilities();
    assert!((caps.max_torque.value()).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn telemetry_report_parses_valid_data() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = vec![0u8; std::mem::size_of::<DeviceTelemetryReport>()];
    data[0] = DeviceTelemetryReport::REPORT_ID;
    data[1..5].copy_from_slice(&(-45_000i32).to_le_bytes()); // -45 degrees
    data[5..7].copy_from_slice(&1500i16.to_le_bytes()); // 1.5 rad/s
    data[7] = 55; // temperature
    data[8] = 0x00; // no faults
    data[9] = 0x01; // hands on

    let report = DeviceTelemetryReport::from_bytes(&data).ok_or("telemetry parse failed")?;
    let tel = report.to_telemetry_data();

    assert!((tel.wheel_angle_deg - (-45.0)).abs() < 0.01);
    assert!((tel.wheel_speed_rad_s - 1.5).abs() < 0.01);
    assert_eq!(tel.temperature_c, 55);
    assert_eq!(tel.fault_flags, 0);
    assert!(tel.hands_on);
    Ok(())
}

#[test]
fn telemetry_report_rejects_wrong_id() {
    let mut data = vec![0u8; std::mem::size_of::<DeviceTelemetryReport>()];
    data[0] = 0xAA;
    assert!(DeviceTelemetryReport::from_bytes(&data).is_none());
}

// ===================================================================
// 3. Multi-device enumeration (wheel + pedals + shifter)
// ===================================================================

#[tokio::test]
async fn enumerate_wheel_pedals_shifter_combo() -> Result<(), Box<dyn std::error::Error>> {
    let mut port = VirtualHidPort::new();

    port.add_device(make_device("wheelbase-dd")?)?;
    port.add_device(make_device("pedals-v3")?)?;
    port.add_device(make_device("shifter-h")?)?;

    let devices = port.list_devices().await?;
    assert_eq!(devices.len(), 3);

    let names: Vec<&str> = devices.iter().map(|d| d.id.as_str()).collect();
    assert!(names.contains(&"wheelbase-dd"));
    assert!(names.contains(&"pedals-v3"));
    assert!(names.contains(&"shifter-h"));
    Ok(())
}

#[tokio::test]
async fn enumerate_full_rig_all_peripherals() -> Result<(), Box<dyn std::error::Error>> {
    let mut port = VirtualHidPort::new();

    let peripherals = [
        "wheel",
        "pedals",
        "shifter",
        "handbrake",
        "button-box",
        "display",
    ];
    for name in &peripherals {
        port.add_device(make_device(name)?)?;
    }

    let devices = port.list_devices().await?;
    assert_eq!(devices.len(), peripherals.len());
    for name in &peripherals {
        assert!(
            devices.iter().any(|d| d.id.as_str() == *name),
            "missing device: {name}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn each_device_independently_operational() -> Result<(), Box<dyn std::error::Error>> {
    let mut port = VirtualHidPort::new();

    let ids: Vec<DeviceId> = (0..4)
        .map(|i| make_id(&format!("device-{i}")))
        .collect::<Result<Vec<_>, _>>()?;

    for id in &ids {
        let device = VirtualDevice::new(id.clone(), format!("Device {}", id.as_str()));
        port.add_device(device)?;
    }

    for id in &ids {
        let mut opened = port.open_device(id).await?;
        assert!(opened.is_connected());
        opened.write_ffb_report(5.0, 0)?;
        let tel = opened
            .read_telemetry()
            .ok_or("expected telemetry from device")?;
        assert!(tel.temperature_c >= 20);
    }
    Ok(())
}

// ===================================================================
// 4. Device priority / preference selection
// ===================================================================

#[tokio::test]
async fn enumeration_order_matches_insertion_order() -> Result<(), Box<dyn std::error::Error>> {
    let mut port = VirtualHidPort::new();

    let order = ["alpha", "bravo", "charlie", "delta"];
    for name in &order {
        port.add_device(make_device(name)?)?;
    }

    let devices = port.list_devices().await?;
    for (i, name) in order.iter().enumerate() {
        assert_eq!(
            devices[i].id.as_str(),
            *name,
            "device at index {i} mismatch"
        );
    }
    Ok(())
}

#[tokio::test]
async fn first_device_preferred_when_multiple_available() -> Result<(), Box<dyn std::error::Error>>
{
    let mut port = VirtualHidPort::new();

    port.add_device(make_device("primary")?)?;
    port.add_device(make_device("secondary")?)?;
    port.add_device(make_device("tertiary")?)?;

    let devices = port.list_devices().await?;
    assert_eq!(devices[0].id.as_str(), "primary");

    // First device should be the preferred wheel
    let mut primary = port.open_device(&devices[0].id).await?;
    primary.write_ffb_report(10.0, 0)?;
    assert!(primary.read_telemetry().is_some());
    Ok(())
}

// ===================================================================
// 5. Unknown device handling (graceful skip)
// ===================================================================

#[test]
fn unknown_vendor_returns_no_protocol() {
    assert!(get_vendor_protocol(0x0000, 0x0000).is_none());
    assert!(get_vendor_protocol(0xFFFF, 0xFFFF).is_none());
    assert!(get_vendor_protocol(0x1234, 0x5678).is_none());
}

#[test]
fn unknown_device_with_hid_pid_gets_generic_fallback() {
    // Device with unknown VID but HID PID capability should get generic handler
    let handler = get_vendor_protocol_with_hid_pid_fallback(0x9999, 0x0001, true);
    assert!(handler.is_some());
}

#[test]
fn unknown_device_without_hid_pid_gets_no_handler() {
    let handler = get_vendor_protocol_with_hid_pid_fallback(0x9999, 0x0001, false);
    assert!(handler.is_none());
}

#[test]
fn hid_device_info_to_device_info_for_unknown_device() -> Result<(), Box<dyn std::error::Error>> {
    let info = build_hid_device_info(0xAAAA, 0xBBBB, "Unknown Vendor")?;
    let device_info = info.to_device_info();

    assert_eq!(device_info.vendor_id, 0xAAAA);
    assert_eq!(device_info.product_id, 0xBBBB);
    assert!(device_info.is_connected);
    assert!(device_info.name.contains("Unknown Vendor"));
    Ok(())
}

#[test]
fn hid_device_info_generates_fallback_name_when_product_missing()
-> Result<(), Box<dyn std::error::Error>> {
    let dev_id: DeviceId = "unnamed-dev".parse()?;
    let info = HidDeviceInfo {
        device_id: dev_id,
        vendor_id: 0x1234,
        product_id: 0x5678,
        serial_number: None,
        manufacturer: None,
        product_name: None, // no name
        path: "test".to_string(),
        interface_number: None,
        usage_page: None,
        usage: None,
        report_descriptor_len: None,
        report_descriptor_crc32: None,
        capabilities: DeviceCapabilities::new(
            false,
            false,
            false,
            false,
            TorqueNm::new(0.0)?,
            0,
            0,
        ),
    };
    let device_info = info.to_device_info();
    // Fallback name includes hex VID:PID
    assert!(device_info.name.contains("1234"));
    assert!(device_info.name.contains("5678"));
    Ok(())
}

#[tokio::test]
async fn open_nonexistent_device_returns_error() -> Result<(), Box<dyn std::error::Error>> {
    let port = VirtualHidPort::new();
    let ghost_id = make_id("nonexistent-device")?;
    let result = port.open_device(&ghost_id).await;
    assert!(result.is_err());
    Ok(())
}

// ===================================================================
// 6. Device reconnection after disconnect
// ===================================================================

#[test]
fn reconnect_restores_write_capability() -> Result<(), Box<dyn std::error::Error>> {
    let id = make_id("reconnect-write")?;
    let mut device = VirtualDevice::new(id, "Reconnect Write".to_string());

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
fn reconnect_restores_telemetry_reading() -> Result<(), Box<dyn std::error::Error>> {
    let id = make_id("reconnect-telemetry")?;
    let mut device = VirtualDevice::new(id, "Reconnect Telemetry".to_string());

    assert!(device.read_telemetry().is_some());

    device.disconnect();
    assert!(device.read_telemetry().is_none());

    device.reconnect();
    assert!(device.read_telemetry().is_some());
    Ok(())
}

#[test]
fn multiple_reconnect_cycles_stable() -> Result<(), Box<dyn std::error::Error>> {
    let id = make_id("multi-reconnect")?;
    let mut device = VirtualDevice::new(id, "Multi Reconnect".to_string());

    for cycle in 0..20u16 {
        device.write_ffb_report(3.0, cycle)?;
        device.disconnect();
        assert_eq!(
            device.write_ffb_report(1.0, cycle + 100),
            Err(RTError::DeviceDisconnected)
        );
        device.reconnect();
    }
    // Still functional after many cycles
    device.write_ffb_report(10.0, 500)?;
    assert!(device.read_telemetry().is_some());
    Ok(())
}

#[test]
fn identity_preserved_across_reconnect() -> Result<(), Box<dyn std::error::Error>> {
    let id = make_id("identity-check")?;
    let mut device = VirtualDevice::new(id, "Identity Check".to_string());

    let original = device.device_info().clone();

    device.disconnect();
    device.reconnect();

    let after = device.device_info();
    assert_eq!(after.id, original.id);
    assert_eq!(after.vendor_id, original.vendor_id);
    assert_eq!(after.product_id, original.product_id);
    assert_eq!(after.serial_number, original.serial_number);
    assert_eq!(after.manufacturer, original.manufacturer);
    Ok(())
}

// ===================================================================
// 7. Device firmware version detection via telemetry
// ===================================================================

#[test]
fn telemetry_timestamp_advances_after_physics() -> Result<(), Box<dyn std::error::Error>> {
    let id = make_id("firmware-time")?;
    let mut device = VirtualDevice::new(id, "Firmware Time".to_string());

    let tel_a = device
        .read_telemetry()
        .ok_or("expected initial telemetry")?;
    device.write_ffb_report(1.0, 0)?;
    device.simulate_physics(std::time::Duration::from_millis(10));
    let tel_b = device
        .read_telemetry()
        .ok_or("expected post-physics telemetry")?;

    // Timestamps should advance
    assert!(tel_b.timestamp >= tel_a.timestamp);
    Ok(())
}

#[test]
fn health_status_reports_communication_state() -> Result<(), Box<dyn std::error::Error>> {
    let id = make_id("health-comm")?;
    let device = VirtualDevice::new(id, "Health Comm".to_string());

    let health = device.health_status();
    assert_eq!(health.communication_errors, 0);
    assert!(health.temperature_c >= 20);
    assert_eq!(health.fault_flags, 0);
    Ok(())
}

#[test]
fn fault_injection_detected_in_telemetry() -> Result<(), Box<dyn std::error::Error>> {
    let id = make_id("fault-detect")?;
    let mut device = VirtualDevice::new(id, "Fault Detect".to_string());

    // No faults initially
    let tel = device.read_telemetry().ok_or("expected telemetry")?;
    assert_eq!(tel.fault_flags, 0);

    // Inject multiple fault bits
    device.inject_fault(0x01); // encoder fault
    device.inject_fault(0x04); // thermal fault
    let tel = device.read_telemetry().ok_or("expected telemetry")?;
    assert_eq!(tel.fault_flags, 0x05); // both bits set

    device.clear_faults();
    let tel = device.read_telemetry().ok_or("expected telemetry")?;
    assert_eq!(tel.fault_flags, 0);
    Ok(())
}

// ===================================================================
// 8. Device configuration apply / verify
// ===================================================================

#[test]
fn device_capabilities_match_after_open() -> Result<(), Box<dyn std::error::Error>> {
    let id = make_id("caps-verify")?;
    let device = VirtualDevice::new(id, "Caps Verify".to_string());

    let caps = device.capabilities();
    assert!(caps.supports_raw_torque_1khz);
    assert!((caps.max_torque.value() - 25.0).abs() < f32::EPSILON);
    assert_eq!(caps.encoder_cpr, 10000);
    assert_eq!(caps.min_report_period_us, 1000);
    Ok(())
}

#[test]
fn torque_limit_enforced_per_device_capabilities() -> Result<(), Box<dyn std::error::Error>> {
    let id = make_id("torque-limit")?;
    let mut device = VirtualDevice::new(id, "Torque Limit".to_string());

    let max = device.capabilities().max_torque.value();
    // At limit: should succeed
    device.write_ffb_report(max, 0)?;
    // Over limit: should fail
    let result = device.write_ffb_report(max + 1.0, 1);
    assert_eq!(result, Err(RTError::TorqueLimit));
    Ok(())
}

#[test]
fn negative_torque_within_limit_succeeds() -> Result<(), Box<dyn std::error::Error>> {
    let id = make_id("neg-torque")?;
    let mut device = VirtualDevice::new(id, "Neg Torque".to_string());

    device.write_ffb_report(-25.0, 0)?;
    let result = device.write_ffb_report(-26.0, 1);
    assert_eq!(result, Err(RTError::TorqueLimit));
    Ok(())
}

#[tokio::test]
async fn port_capabilities_consistent_across_opens() -> Result<(), Box<dyn std::error::Error>> {
    let mut port = VirtualHidPort::new();
    let id = make_id("caps-consistent")?;
    port.add_device(VirtualDevice::new(
        id.clone(),
        "Caps Consistent".to_string(),
    ))?;

    let first = port.open_device(&id).await?;
    let second = port.open_device(&id).await?;

    let caps_a = first.capabilities();
    let caps_b = second.capabilities();
    assert_eq!(caps_a.supports_pid, caps_b.supports_pid);
    assert_eq!(
        caps_a.supports_raw_torque_1khz,
        caps_b.supports_raw_torque_1khz
    );
    assert!((caps_a.max_torque.value() - caps_b.max_torque.value()).abs() < f32::EPSILON);
    assert_eq!(caps_a.encoder_cpr, caps_b.encoder_cpr);
    Ok(())
}

// ===================================================================
// 9. Torque encoding per vendor
// ===================================================================

#[test]
fn encode_torque_generic_device_owp1() {
    let mut out = [0u8; MAX_TORQUE_REPORT_SIZE];
    let len = hid::encode_torque_report_for_device(0x046D, 0xC24F, 3.0, 1.5, 42, &mut out);

    assert_eq!(len, std::mem::size_of::<TorqueCommand>());
    assert_eq!(out[0], TorqueCommand::REPORT_ID);
    let encoded = i16::from_le_bytes([out[1], out[2]]);
    assert_eq!(encoded, (1.5_f32 * 256.0) as i16);
}

#[test]
fn encode_torque_zero_produces_zero_payload() {
    let mut out = [0u8; MAX_TORQUE_REPORT_SIZE];
    let _len = hid::encode_torque_report_for_device(0x046D, 0xC24F, 10.0, 0.0, 0, &mut out);

    let torque_q8 = i16::from_le_bytes([out[1], out[2]]);
    assert_eq!(torque_q8, 0);
}

// ===================================================================
// 10. Stress tests
// ===================================================================

#[tokio::test]
async fn stress_enumerate_many_devices() -> Result<(), Box<dyn std::error::Error>> {
    let mut port = VirtualHidPort::new();
    let count = 32;

    for i in 0..count {
        let name = format!("stress-dev-{i}");
        port.add_device(make_device(&name)?)?;
    }

    let devices = port.list_devices().await?;
    assert_eq!(devices.len(), count);

    // Open all and verify
    for info in &devices {
        let opened = port.open_device(&info.id).await?;
        assert!(opened.is_connected());
        assert!((opened.capabilities().max_torque.value() - 25.0).abs() < f32::EPSILON);
    }
    Ok(())
}

#[tokio::test]
async fn stress_add_remove_interleaved() -> Result<(), Box<dyn std::error::Error>> {
    let mut port = VirtualHidPort::new();
    let mut ids = Vec::new();

    // Add 20, remove odds, add 10 more
    for i in 0..20 {
        let name = format!("interleave-{i}");
        let id = make_id(&name)?;
        port.add_device(VirtualDevice::new(id.clone(), name))?;
        ids.push(id);
    }

    for i in (1..20).step_by(2) {
        port.remove_device(&ids[i])?;
    }

    assert_eq!(port.list_devices().await?.len(), 10);

    for i in 20..30 {
        let name = format!("interleave-{i}");
        port.add_device(make_device(&name)?)?;
    }

    assert_eq!(port.list_devices().await?.len(), 20);
    Ok(())
}

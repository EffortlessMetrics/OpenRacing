//! Cross-platform HID transport abstraction and device enumeration tests.
//!
//! These tests validate the platform-independent HID layer: trait contracts,
//! mock backends, VID/PID matching, device connection/disconnection handling,
//! report descriptor parsing, error handling, and hot-plug behaviour.
//!
//! All tests use mocks — no real HID hardware is required.

use super::macos::{
    self, HIDElement, IOHIDElementType, IOKitDeviceDescriptor, IOKitMatchingDict, MacHidError,
    ReportKind, classify_report, device_matching_dict, racing_wheel_matching_dict,
    validate_report_length,
};
use super::quirks::{ConditionalCoefficients, DeviceQuirks};
use super::rt_stream::{RtIoError, StreamConfig, TorqueMailbox};
use super::vendor::get_vendor_protocol;
use super::{
    DeviceCapabilitiesReport, DeviceTelemetryReport, HidDeviceInfo, MAX_TORQUE_REPORT_SIZE,
    TorqueCommand, encode_torque_report_for_device,
};
use crate::RTError;
use crate::device::{DeviceEvent, DeviceInfo, VirtualDevice, VirtualHidPort};
use crate::ports::{HidDevice, HidPort};
use racing_wheel_schemas::prelude::*;
use std::sync::atomic::Ordering;
use std::time::Duration;

// =========================================================================
// Helpers
// =========================================================================

/// Parse a `DeviceId` without unwrap.
fn device_id(s: &str) -> Result<DeviceId, Box<dyn std::error::Error>> {
    Ok(s.parse::<DeviceId>()?)
}

/// Build a minimal `IOKitDeviceDescriptor` for a racing wheel.
fn make_test_descriptor(vid: u16, pid: u16) -> IOKitDeviceDescriptor {
    IOKitDeviceDescriptor {
        vendor_id: vid,
        product_id: pid,
        version_number: 0x0100,
        manufacturer: Some("Test Vendor".to_string()),
        product: Some("Test Wheel".to_string()),
        serial_number: Some("SN-001".to_string()),
        transport: Some("USB".to_string()),
        primary_usage_page: macos::usage_page::GENERIC_DESKTOP,
        primary_usage: macos::usage::WHEEL,
        location_id: 0xAABB_0000,
        elements: vec![
            steering_element(),
            button_element(1),
            button_element(2),
            pid_output_element(),
        ],
    }
}

fn steering_element() -> HIDElement {
    HIDElement {
        element_type: IOHIDElementType::InputAxis,
        usage_page: macos::usage_page::GENERIC_DESKTOP,
        usage: macos::usage::X,
        logical_min: 0,
        logical_max: 65535,
        physical_min: -900,
        physical_max: 900,
        report_size: 16,
        report_count: 1,
        report_id: 1,
    }
}

fn button_element(index: u32) -> HIDElement {
    HIDElement {
        element_type: IOHIDElementType::InputButton,
        usage_page: 0x09,
        usage: index,
        logical_min: 0,
        logical_max: 1,
        physical_min: 0,
        physical_max: 1,
        report_size: 1,
        report_count: 1,
        report_id: 1,
    }
}

fn pid_output_element() -> HIDElement {
    HIDElement {
        element_type: IOHIDElementType::Output,
        usage_page: macos::usage_page::PID,
        usage: 0x25,
        logical_min: 0,
        logical_max: 255,
        physical_min: 0,
        physical_max: 255,
        report_size: 8,
        report_count: 1,
        report_id: 2,
    }
}

fn feature_element(usage_page: u32, usage: u32) -> HIDElement {
    HIDElement {
        element_type: IOHIDElementType::Feature,
        usage_page,
        usage,
        logical_min: 0,
        logical_max: 255,
        physical_min: 0,
        physical_max: 255,
        report_size: 8,
        report_count: 1,
        report_id: 3,
    }
}

// =========================================================================
// 1. HID trait implementation correctness
// =========================================================================

#[test]
fn virtual_device_implements_hid_device_contract() -> Result<(), Box<dyn std::error::Error>> {
    let id = device_id("virt-trait-test")?;
    let mut dev = VirtualDevice::new(id, "Trait Test Wheel".to_string());

    // capabilities() returns non-zero max torque
    assert!(dev.capabilities().max_torque.value() > 0.0);
    // device_info() has correct id
    assert_eq!(dev.device_info().id.as_str(), "virt-trait-test");
    // is_connected() starts true
    assert!(dev.is_connected());
    // write_ffb_report succeeds within limits
    assert!(dev.write_ffb_report(1.0, 1).is_ok());
    // read_telemetry returns Some while connected
    assert!(dev.read_telemetry().is_some());
    // health_status returns reasonable defaults
    let health = dev.health_status();
    assert_eq!(health.communication_errors, 0);
    assert!(health.temperature_c > 0);
    Ok(())
}

#[test]
fn virtual_device_write_returns_error_when_disconnected() -> Result<(), Box<dyn std::error::Error>>
{
    let id = device_id("disc-write")?;
    let mut dev = VirtualDevice::new(id, "Disc Wheel".to_string());
    dev.disconnect();
    let err = dev.write_ffb_report(1.0, 1);
    assert_eq!(err, Err(RTError::DeviceDisconnected));
    Ok(())
}

#[test]
fn virtual_device_read_telemetry_returns_none_when_disconnected()
-> Result<(), Box<dyn std::error::Error>> {
    let id = device_id("disc-telem")?;
    let mut dev = VirtualDevice::new(id, "Disc Telem".to_string());
    dev.disconnect();
    assert!(dev.read_telemetry().is_none());
    Ok(())
}

#[test]
fn virtual_device_torque_limit_enforcement() -> Result<(), Box<dyn std::error::Error>> {
    let id = device_id("torque-limit")?;
    let mut dev = VirtualDevice::new(id, "Limit Wheel".to_string());
    let max = dev.capabilities().max_torque.value();

    // Within limit
    assert!(dev.write_ffb_report(max - 0.1, 1).is_ok());
    // At limit (abs > max)
    let result = dev.write_ffb_report(max + 0.1, 2);
    assert_eq!(result, Err(RTError::TorqueLimit));
    // Negative beyond limit
    let result = dev.write_ffb_report(-(max + 0.1), 3);
    assert_eq!(result, Err(RTError::TorqueLimit));
    Ok(())
}

#[test]
fn virtual_device_health_status_reflects_faults() -> Result<(), Box<dyn std::error::Error>> {
    let id = device_id("fault-health")?;
    let mut dev = VirtualDevice::new(id, "Fault Wheel".to_string());

    assert_eq!(dev.health_status().fault_flags, 0);
    dev.inject_fault(0x02);
    assert_eq!(dev.health_status().fault_flags, 0x02);
    dev.inject_fault(0x08);
    assert_eq!(dev.health_status().fault_flags, 0x0A);
    dev.clear_faults();
    assert_eq!(dev.health_status().fault_flags, 0);
    Ok(())
}

// =========================================================================
// 2. Device enumeration with mock backends
// =========================================================================

#[tokio::test]
async fn virtual_port_list_empty_initially() -> Result<(), Box<dyn std::error::Error>> {
    let port = VirtualHidPort::new();
    let devices = port.list_devices().await?;
    assert!(devices.is_empty());
    Ok(())
}

#[tokio::test]
async fn virtual_port_list_reflects_added_devices() -> Result<(), Box<dyn std::error::Error>> {
    let mut port = VirtualHidPort::new();
    let id1 = device_id("enum-dev-1")?;
    let id2 = device_id("enum-dev-2")?;
    port.add_device(VirtualDevice::new(id1, "Wheel 1".into()))?;
    port.add_device(VirtualDevice::new(id2, "Wheel 2".into()))?;

    let devices = port.list_devices().await?;
    assert_eq!(devices.len(), 2);
    Ok(())
}

#[tokio::test]
async fn virtual_port_open_returns_working_device() -> Result<(), Box<dyn std::error::Error>> {
    let mut port = VirtualHidPort::new();
    let id = device_id("open-dev")?;
    port.add_device(VirtualDevice::new(id.clone(), "Open Wheel".into()))?;

    let mut dev = port.open_device(&id).await?;
    assert!(dev.is_connected());
    assert!(dev.write_ffb_report(0.5, 1).is_ok());
    assert!(dev.read_telemetry().is_some());
    Ok(())
}

#[tokio::test]
async fn virtual_port_open_nonexistent_device_errors() -> Result<(), Box<dyn std::error::Error>> {
    let port = VirtualHidPort::new();
    let id = device_id("ghost")?;
    let result = port.open_device(&id).await;
    assert!(result.is_err());
    Ok(())
}

#[tokio::test]
async fn virtual_port_refresh_succeeds() -> Result<(), Box<dyn std::error::Error>> {
    let port = VirtualHidPort::new();
    // refresh_devices is a no-op on VirtualHidPort but should not error
    port.refresh_devices().await?;
    Ok(())
}

#[tokio::test]
async fn virtual_port_monitor_returns_receiver() -> Result<(), Box<dyn std::error::Error>> {
    let port = VirtualHidPort::new();
    let _rx = port.monitor_devices().await?;
    Ok(())
}

#[tokio::test]
async fn virtual_port_device_info_fields_populated() -> Result<(), Box<dyn std::error::Error>> {
    let mut port = VirtualHidPort::new();
    let id = device_id("info-check")?;
    port.add_device(VirtualDevice::new(id.clone(), "Info Wheel".into()))?;

    let devices = port.list_devices().await?;
    assert_eq!(devices.len(), 1);
    let info = &devices[0];
    assert_eq!(info.id, id);
    assert_eq!(info.name, "Info Wheel");
    assert!(info.vendor_id != 0);
    assert!(info.product_id != 0);
    assert!(info.serial_number.is_some());
    assert!(info.manufacturer.is_some());
    assert!(info.is_connected);
    Ok(())
}

// =========================================================================
// 3. VID/PID matching and device identification
// =========================================================================

#[test]
fn matching_dict_vid_pid_exact_match() -> Result<(), Box<dyn std::error::Error>> {
    let dict = IOKitMatchingDict::new()
        .with_vendor_id(0x0EB7)
        .with_product_id(0x0024);

    assert!(dict.matches_device(
        0x0EB7,
        0x0024,
        macos::usage_page::GENERIC_DESKTOP,
        macos::usage::WHEEL
    ));
    // Wrong VID
    assert!(!dict.matches_device(
        0x346E,
        0x0024,
        macos::usage_page::GENERIC_DESKTOP,
        macos::usage::WHEEL
    ));
    // Wrong PID
    assert!(!dict.matches_device(
        0x0EB7,
        0x9999,
        macos::usage_page::GENERIC_DESKTOP,
        macos::usage::WHEEL
    ));
    Ok(())
}

#[test]
fn matching_dict_usage_page_and_usage_filter() -> Result<(), Box<dyn std::error::Error>> {
    let dict = IOKitMatchingDict::new()
        .with_usage_page(macos::usage_page::GENERIC_DESKTOP)
        .with_usage(macos::usage::WHEEL);

    // Racing wheel matches
    assert!(dict.matches_device(
        0x346E,
        0x0004,
        macos::usage_page::GENERIC_DESKTOP,
        macos::usage::WHEEL
    ));
    // Joystick usage doesn't match a WHEEL filter
    assert!(!dict.matches_device(
        0x346E,
        0x0004,
        macos::usage_page::GENERIC_DESKTOP,
        macos::usage::JOYSTICK
    ));
    // Keyboard usage page doesn't match
    assert!(!dict.matches_device(0x046D, 0xC32B, 0x07, 0x06));
    Ok(())
}

#[test]
fn matching_dict_empty_matches_any_device() -> Result<(), Box<dyn std::error::Error>> {
    let dict = IOKitMatchingDict::new();
    assert!(dict.matches_device(0x1234, 0x5678, 0x01, 0x04));
    assert!(dict.matches_device(0xFFFF, 0xFFFF, 0xFF, 0xFF));
    Ok(())
}

#[test]
fn vendor_protocol_dispatch_known_vendors() -> Result<(), Box<dyn std::error::Error>> {
    // Fanatec
    assert!(get_vendor_protocol(0x0EB7, 0x0024).is_some());
    // Moza
    assert!(get_vendor_protocol(0x346E, 0x0004).is_some());
    // Logitech
    assert!(get_vendor_protocol(0x046D, 0xC266).is_some());
    // Thrustmaster
    assert!(get_vendor_protocol(0x044F, 0xB66E).is_some());
    // Unknown vendor
    assert!(get_vendor_protocol(0x0001, 0x0001).is_none());
    Ok(())
}

#[test]
fn device_quirks_moza_wheelbase_has_conditional_fix() -> Result<(), Box<dyn std::error::Error>> {
    let quirks = DeviceQuirks::for_device(0x346E, 0x0005);
    assert!(quirks.fix_conditional_direction);
    assert!(quirks.uses_vendor_usage_page);
    assert!(quirks.requires_init_handshake);
    assert_eq!(quirks.required_b_interval, Some(1));
    Ok(())
}

#[test]
fn device_quirks_moza_pedals_no_ffb_quirks() -> Result<(), Box<dyn std::error::Error>> {
    let quirks = DeviceQuirks::for_device(0x346E, 0x0003);
    assert!(!quirks.fix_conditional_direction);
    assert!(!quirks.requires_init_handshake);
    Ok(())
}

#[test]
fn device_quirks_unknown_device_has_no_quirks() -> Result<(), Box<dyn std::error::Error>> {
    let quirks = DeviceQuirks::for_device(0xDEAD, 0xBEEF);
    assert!(!quirks.has_quirks());
    Ok(())
}

#[test]
fn device_quirks_fanatec_wheelbase_requires_handshake() -> Result<(), Box<dyn std::error::Error>> {
    // GT DD Pro
    let quirks = DeviceQuirks::for_device(0x0EB7, 0x0024);
    assert!(quirks.requires_init_handshake);
    assert_eq!(quirks.required_b_interval, Some(1));
    Ok(())
}

#[test]
fn device_quirks_simagic_has_polling_interval() -> Result<(), Box<dyn std::error::Error>> {
    let quirks = DeviceQuirks::for_device(0x3670, 0x0001);
    assert!(quirks.required_b_interval.is_some());
    assert!(!quirks.requires_init_handshake);
    Ok(())
}

// =========================================================================
// 4. Platform abstraction — API consistency across platforms
// =========================================================================

#[test]
fn hid_device_info_to_device_info_conversion() -> Result<(), Box<dyn std::error::Error>> {
    let hid_info = HidDeviceInfo {
        device_id: device_id("xplat-conv")?,
        vendor_id: 0x346E,
        product_id: 0x0004,
        serial_number: Some("SN-123".to_string()),
        manufacturer: Some("Moza".to_string()),
        product_name: Some("R5".to_string()),
        path: "/dev/hidraw0".to_string(),
        interface_number: Some(0),
        usage_page: Some(0x01),
        usage: Some(0x38),
        report_descriptor_len: Some(128),
        report_descriptor_crc32: Some(0xDEADBEEF),
        capabilities: DeviceCapabilities::new(
            true,
            true,
            false,
            false,
            TorqueNm::new(5.5)?,
            4096,
            1000,
        ),
    };

    let info = hid_info.to_device_info();
    assert_eq!(info.id, device_id("xplat-conv")?);
    assert_eq!(info.name, "R5");
    assert_eq!(info.vendor_id, 0x346E);
    assert_eq!(info.product_id, 0x0004);
    assert_eq!(info.serial_number.as_deref(), Some("SN-123"));
    assert_eq!(info.manufacturer.as_deref(), Some("Moza"));
    assert_eq!(info.path, "/dev/hidraw0");
    assert!(info.is_connected);
    assert!(info.capabilities.supports_pid);
    Ok(())
}

#[test]
fn hid_device_info_fallback_name_when_product_none() -> Result<(), Box<dyn std::error::Error>> {
    let hid_info = HidDeviceInfo {
        device_id: device_id("fallback-name")?,
        vendor_id: 0xABCD,
        product_id: 0x1234,
        serial_number: None,
        manufacturer: None,
        product_name: None,
        path: "test://device".to_string(),
        interface_number: None,
        usage_page: None,
        usage: None,
        report_descriptor_len: None,
        report_descriptor_crc32: None,
        capabilities: DeviceCapabilities::new(false, false, false, false, TorqueNm::ZERO, 0, 8000),
    };

    let info = hid_info.to_device_info();
    assert!(info.name.contains("ABCD"));
    assert!(info.name.contains("1234"));
    Ok(())
}

#[test]
fn iokit_descriptor_to_hid_device_info_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let desc = make_test_descriptor(0x0EB7, 0x0024);
    let hid_info = desc.to_hid_device_info()?;
    let device_info = hid_info.to_device_info();

    assert_eq!(device_info.vendor_id, 0x0EB7);
    assert_eq!(device_info.product_id, 0x0024);
    assert_eq!(device_info.name, "Test Wheel");
    assert!(device_info.is_connected);
    assert!(hid_info.capabilities.supports_pid);
    Ok(())
}

#[test]
fn device_capabilities_ffb_detection() -> Result<(), Box<dyn std::error::Error>> {
    let with_ffb =
        DeviceCapabilities::new(true, true, false, false, TorqueNm::new(10.0)?, 4096, 1000);
    assert!(with_ffb.supports_ffb());

    let without_ffb = DeviceCapabilities::new(false, false, false, false, TorqueNm::ZERO, 0, 8000);
    assert!(!without_ffb.supports_ffb());
    Ok(())
}

#[test]
fn device_capabilities_max_update_rate() -> Result<(), Box<dyn std::error::Error>> {
    let caps = DeviceCapabilities::new(true, true, false, false, TorqueNm::new(5.0)?, 4096, 1000);
    let rate = caps.max_update_rate_hz();
    assert!((rate - 1000.0).abs() < 0.1);
    Ok(())
}

// =========================================================================
// 5. Device connection/disconnection handling
// =========================================================================

#[test]
fn virtual_device_disconnect_reconnect_cycle() -> Result<(), Box<dyn std::error::Error>> {
    let id = device_id("cycle-dev")?;
    let mut dev = VirtualDevice::new(id, "Cycle Wheel".into());

    assert!(dev.is_connected());
    assert!(dev.write_ffb_report(1.0, 1).is_ok());

    dev.disconnect();
    assert!(!dev.is_connected());
    assert_eq!(
        dev.write_ffb_report(1.0, 2),
        Err(RTError::DeviceDisconnected)
    );
    assert!(dev.read_telemetry().is_none());

    dev.reconnect();
    assert!(dev.is_connected());
    assert!(dev.write_ffb_report(1.0, 3).is_ok());
    assert!(dev.read_telemetry().is_some());
    Ok(())
}

#[tokio::test]
async fn virtual_port_add_remove_device() -> Result<(), Box<dyn std::error::Error>> {
    let mut port = VirtualHidPort::new();
    let id = device_id("add-remove")?;
    port.add_device(VirtualDevice::new(id.clone(), "AR Wheel".into()))?;
    assert_eq!(port.list_devices().await?.len(), 1);

    port.remove_device(&id)?;
    assert!(port.list_devices().await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn virtual_port_multiple_devices_independent() -> Result<(), Box<dyn std::error::Error>> {
    let mut port = VirtualHidPort::new();
    let id1 = device_id("multi-1")?;
    let id2 = device_id("multi-2")?;
    port.add_device(VirtualDevice::new(id1.clone(), "Wheel A".into()))?;
    port.add_device(VirtualDevice::new(id2.clone(), "Wheel B".into()))?;

    let mut dev1 = port.open_device(&id1).await?;
    let mut dev2 = port.open_device(&id2).await?;

    // Both work independently
    assert!(dev1.write_ffb_report(2.0, 1).is_ok());
    assert!(dev2.write_ffb_report(3.0, 1).is_ok());
    assert_eq!(dev1.device_info().name, "Wheel A");
    assert_eq!(dev2.device_info().name, "Wheel B");
    Ok(())
}

#[test]
fn device_event_variants() -> Result<(), Box<dyn std::error::Error>> {
    let id = device_id("event-dev")?;
    let info = DeviceInfo {
        id: id.clone(),
        name: "Event Wheel".into(),
        vendor_id: 0x1234,
        product_id: 0x5678,
        serial_number: None,
        manufacturer: None,
        path: "test://event".into(),
        capabilities: DeviceCapabilities::new(false, false, false, false, TorqueNm::ZERO, 0, 8000),
        is_connected: true,
    };

    let connected = DeviceEvent::Connected(info.clone());
    let disconnected = DeviceEvent::Disconnected(info.clone());

    match &connected {
        DeviceEvent::Connected(i) => assert_eq!(i.id, id),
        _ => return Err("expected Connected variant".into()),
    }
    match &disconnected {
        DeviceEvent::Disconnected(i) => assert_eq!(i.id, id),
        _ => return Err("expected Disconnected variant".into()),
    }
    Ok(())
}

// =========================================================================
// 6. Report descriptor parsing
// =========================================================================

#[test]
fn classify_report_all_categories() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        classify_report(&[0x01, 0x00]),
        Some(ReportKind::Capabilities)
    );
    for id in 0x02u8..=0x0F {
        assert_eq!(classify_report(&[id]), Some(ReportKind::Input));
    }
    assert_eq!(classify_report(&[0x20]), Some(ReportKind::Input));
    assert_eq!(classify_report(&[0x21]), Some(ReportKind::Telemetry));
    assert_eq!(
        classify_report(&[0x80]),
        Some(ReportKind::VendorSpecific(0x80))
    );
    assert_eq!(
        classify_report(&[0xFE]),
        Some(ReportKind::VendorSpecific(0xFE))
    );
    assert_eq!(classify_report(&[0x00]), Some(ReportKind::Unknown(0x00)));
    assert_eq!(classify_report(&[0xFF]), Some(ReportKind::Unknown(0xFF)));
    assert_eq!(classify_report(&[]), None);
    Ok(())
}

#[test]
fn telemetry_report_deserialization() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = vec![0u8; std::mem::size_of::<DeviceTelemetryReport>()];
    data[0] = DeviceTelemetryReport::REPORT_ID;

    // 45000 millidegrees = 45 degrees
    let angle_bytes = 45000i32.to_le_bytes();
    data[1..5].copy_from_slice(&angle_bytes);
    // 1500 mrad/s
    let speed_bytes = 1500i16.to_le_bytes();
    data[5..7].copy_from_slice(&speed_bytes);
    data[7] = 42; // temp
    data[8] = 0x03; // faults
    data[9] = 1; // hands_on

    let report =
        DeviceTelemetryReport::from_bytes(&data).ok_or("telemetry deserialization failed")?;
    let telemetry = report.to_telemetry_data();

    assert!((telemetry.wheel_angle_deg - 45.0).abs() < 0.01);
    assert!((telemetry.wheel_speed_rad_s - 1.5).abs() < 0.01);
    assert_eq!(telemetry.temperature_c, 42);
    assert_eq!(telemetry.fault_flags, 0x03);
    assert!(telemetry.hands_on);
    Ok(())
}

#[test]
fn telemetry_report_rejects_wrong_id() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = vec![0u8; std::mem::size_of::<DeviceTelemetryReport>()];
    data[0] = 0xFF; // Wrong report ID
    assert!(DeviceTelemetryReport::from_bytes(&data).is_none());
    Ok(())
}

#[test]
fn telemetry_report_rejects_short_buffer() -> Result<(), Box<dyn std::error::Error>> {
    let data = vec![DeviceTelemetryReport::REPORT_ID, 0x00]; // Too short
    assert!(DeviceTelemetryReport::from_bytes(&data).is_none());
    Ok(())
}

#[test]
fn capabilities_report_deserialization() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = vec![0u8; std::mem::size_of::<DeviceCapabilitiesReport>()];
    data[0] = DeviceCapabilitiesReport::REPORT_ID;
    data[1] = 0x01; // supports_pid
    data[2] = 0x01; // supports_raw_torque_1khz
    data[3] = 0x00; // no health stream
    data[4] = 0x01; // supports_led_bus
    data[5..7].copy_from_slice(&1500u16.to_le_bytes()); // 15.0 Nm in cNm
    data[7..9].copy_from_slice(&8192u16.to_le_bytes()); // encoder CPR
    data[9] = 50; // min_report_period_us

    let report =
        DeviceCapabilitiesReport::from_bytes(&data).ok_or("capabilities deserialization failed")?;
    let caps = report.to_device_capabilities();

    assert!(caps.supports_pid);
    assert!(caps.supports_raw_torque_1khz);
    assert!(!caps.supports_health_stream);
    assert!(caps.supports_led_bus);
    assert!((caps.max_torque.value() - 15.0).abs() < 0.01);
    assert_eq!(caps.encoder_cpr, 8192);
    assert_eq!(caps.min_report_period_us, 50);
    Ok(())
}

#[test]
fn capabilities_report_rejects_wrong_id() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = vec![0u8; std::mem::size_of::<DeviceCapabilitiesReport>()];
    data[0] = 0xAA;
    assert!(DeviceCapabilitiesReport::from_bytes(&data).is_none());
    Ok(())
}

#[test]
fn capabilities_report_rejects_short_buffer() -> Result<(), Box<dyn std::error::Error>> {
    let data = vec![DeviceCapabilitiesReport::REPORT_ID];
    assert!(DeviceCapabilitiesReport::from_bytes(&data).is_none());
    Ok(())
}

#[test]
fn iokit_descriptor_element_counts() -> Result<(), Box<dyn std::error::Error>> {
    let desc = make_test_descriptor(0x346E, 0x0004);
    assert_eq!(desc.count_elements(IOHIDElementType::InputAxis), 1);
    assert_eq!(desc.count_elements(IOHIDElementType::InputButton), 2);
    assert_eq!(desc.count_elements(IOHIDElementType::Output), 1);
    assert_eq!(desc.count_elements(IOHIDElementType::Feature), 0);
    Ok(())
}

#[test]
fn iokit_descriptor_racing_wheel_detection() -> Result<(), Box<dyn std::error::Error>> {
    let wheel = make_test_descriptor(0x346E, 0x0004);
    assert!(wheel.is_racing_wheel());

    // Joystick usage also counts
    let joystick = IOKitDeviceDescriptor {
        primary_usage: macos::usage::JOYSTICK,
        ..make_test_descriptor(0x046D, 0xC294)
    };
    assert!(joystick.is_racing_wheel());

    // Game pad
    let gamepad = IOKitDeviceDescriptor {
        primary_usage: macos::usage::GAME_PAD,
        ..make_test_descriptor(0x046D, 0xC294)
    };
    assert!(gamepad.is_racing_wheel());

    // Multi-axis controller
    let multi = IOKitDeviceDescriptor {
        primary_usage: macos::usage::MULTI_AXIS_CONTROLLER,
        ..make_test_descriptor(0x046D, 0xC294)
    };
    assert!(multi.is_racing_wheel());

    // Keyboard is NOT a racing wheel
    let keyboard = IOKitDeviceDescriptor {
        primary_usage_page: 0x07,
        primary_usage: 0x06,
        ..make_test_descriptor(0x046D, 0xC32B)
    };
    assert!(!keyboard.is_racing_wheel());
    Ok(())
}

#[test]
fn iokit_descriptor_pid_output_detection() -> Result<(), Box<dyn std::error::Error>> {
    let with_pid = make_test_descriptor(0x346E, 0x0004);
    assert!(with_pid.has_pid_outputs());

    let without_pid = IOKitDeviceDescriptor {
        elements: vec![steering_element(), button_element(1)],
        ..make_test_descriptor(0x346E, 0x0004)
    };
    assert!(!without_pid.has_pid_outputs());
    Ok(())
}

#[test]
fn iokit_descriptor_steering_element_lookup() -> Result<(), Box<dyn std::error::Error>> {
    let desc = make_test_descriptor(0x346E, 0x0004);
    let steering = desc.steering_element().ok_or("no steering element")?;
    assert_eq!(steering.usage_page, macos::usage_page::GENERIC_DESKTOP);
    assert_eq!(steering.usage, macos::usage::X);
    assert_eq!(steering.bit_width(), 16);
    Ok(())
}

#[test]
fn hid_element_normalization() -> Result<(), Box<dyn std::error::Error>> {
    let elem = steering_element();

    // Boundaries
    let at_min = elem.normalize(0).ok_or("None at min")?;
    assert!(at_min.abs() < f64::EPSILON);

    let at_max = elem.normalize(65535).ok_or("None at max")?;
    assert!((at_max - 1.0).abs() < f64::EPSILON);

    // Mid
    let at_mid = elem.normalize(32767).ok_or("None at mid")?;
    assert!((at_mid - 0.5).abs() < 0.001);

    // Clamping
    let below = elem.normalize(-100).ok_or("None below")?;
    assert!(below.abs() < f64::EPSILON);
    let above = elem.normalize(70000).ok_or("None above")?;
    assert!((above - 1.0).abs() < f64::EPSILON);
    Ok(())
}

#[test]
fn hid_element_signed_normalization() -> Result<(), Box<dyn std::error::Error>> {
    let elem = steering_element();

    let left = elem.normalize_signed(0).ok_or("None at left")?;
    assert!((left - (-1.0)).abs() < f64::EPSILON);

    let right = elem.normalize_signed(65535).ok_or("None at right")?;
    assert!((right - 1.0).abs() < f64::EPSILON);

    let center = elem.normalize_signed(32767).ok_or("None at center")?;
    assert!(center.abs() < 0.001);
    Ok(())
}

#[test]
fn hid_element_degenerate_range_returns_none() -> Result<(), Box<dyn std::error::Error>> {
    let elem = HIDElement {
        logical_min: 5,
        logical_max: 5,
        ..steering_element()
    };
    assert!(!elem.has_range());
    assert!(elem.normalize(5).is_none());
    assert!(elem.normalize_signed(5).is_none());
    Ok(())
}

// =========================================================================
// 7. Error handling for invalid devices, permission errors, etc.
// =========================================================================

#[test]
fn mac_hid_error_display_strings() -> Result<(), Box<dyn std::error::Error>> {
    let io_err = MacHidError::IOReturn(0x0E000001);
    assert!(format!("{io_err}").contains("IOKit error"));

    let removed = MacHidError::DeviceRemoved;
    assert_eq!(format!("{removed}"), "HID device removed");

    let invalid = MacHidError::InvalidMatchingDict("bad key".into());
    assert!(format!("{invalid}").contains("bad key"));

    let missing = MacHidError::MissingElement {
        usage_page: 0x0F,
        usage: 0x25,
    };
    let msg = format!("{missing}");
    assert!(msg.contains("000F"));
    assert!(msg.contains("0025"));

    let malformed = MacHidError::MalformedReport {
        expected_min: 16,
        actual: 3,
    };
    let msg = format!("{malformed}");
    assert!(msg.contains("16"));
    assert!(msg.contains("3"));
    Ok(())
}

#[test]
fn mac_hid_error_implements_std_error() -> Result<(), Box<dyn std::error::Error>> {
    let err: Box<dyn std::error::Error> = Box::new(MacHidError::DeviceRemoved);
    assert!(!err.to_string().is_empty());
    Ok(())
}

#[test]
fn validate_report_length_accepts_exact() -> Result<(), Box<dyn std::error::Error>> {
    validate_report_length(&[1, 2, 3], 3)?;
    Ok(())
}

#[test]
fn validate_report_length_accepts_longer() -> Result<(), Box<dyn std::error::Error>> {
    validate_report_length(&[1, 2, 3, 4, 5], 3)?;
    Ok(())
}

#[test]
fn validate_report_length_rejects_shorter() -> Result<(), Box<dyn std::error::Error>> {
    let result = validate_report_length(&[1, 2], 5);
    assert!(result.is_err());
    let err = result.err().ok_or("expected error")?;
    assert_eq!(
        err,
        MacHidError::MalformedReport {
            expected_min: 5,
            actual: 2,
        }
    );
    Ok(())
}

#[test]
fn validate_report_length_empty_buffer() -> Result<(), Box<dyn std::error::Error>> {
    let result = validate_report_length(&[], 1);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn rt_io_error_variants_distinct() -> Result<(), Box<dyn std::error::Error>> {
    assert_ne!(RtIoError::WouldBlock, RtIoError::Disconnected);
    assert_ne!(RtIoError::WouldBlock, RtIoError::WatchdogTimeout);
    assert_ne!(RtIoError::Disconnected, RtIoError::Other);
    Ok(())
}

#[test]
fn torque_command_zero_torque() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = TorqueCommand::new(0.0, 0, false, false);
    assert_eq!(cmd.report_id, TorqueCommand::REPORT_ID);
    let torque = cmd.torque_nm_q8_8;
    assert_eq!(torque, 0);
    let flags = cmd.flags;
    assert_eq!(flags, 0);
    Ok(())
}

#[test]
fn torque_command_negative_torque() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = TorqueCommand::new(-3.0, 99, false, false);
    let torque = cmd.torque_nm_q8_8;
    assert_eq!(torque, (-3.0f32 * 256.0) as i16);
    Ok(())
}

#[test]
fn torque_command_saturation_clamp() -> Result<(), Box<dyn std::error::Error>> {
    // Very large value should clamp to i16::MAX
    let cmd = TorqueCommand::new(200.0, 0, false, false);
    let torque = cmd.torque_nm_q8_8;
    assert_eq!(torque, i16::MAX);

    // Very negative value should clamp to i16::MIN
    let cmd_neg = TorqueCommand::new(-200.0, 0, false, false);
    let torque_neg = cmd_neg.torque_nm_q8_8;
    assert_eq!(torque_neg, i16::MIN);
    Ok(())
}

#[test]
fn torque_command_flags_encoding() -> Result<(), Box<dyn std::error::Error>> {
    let both = TorqueCommand::new(0.0, 0, true, true);
    let flags = both.flags;
    assert_eq!(flags, 0x03);

    let none = TorqueCommand::new(0.0, 0, false, false);
    let flags = none.flags;
    assert_eq!(flags, 0x00);

    let hands_only = TorqueCommand::new(0.0, 0, true, false);
    let flags = hands_only.flags;
    assert_eq!(flags, 0x01);

    let sat_only = TorqueCommand::new(0.0, 0, false, true);
    let flags = sat_only.flags;
    assert_eq!(flags, 0x02);
    Ok(())
}

#[test]
fn torque_command_serialization_length() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = TorqueCommand::new(1.0, 1, true, false);
    let bytes = cmd.as_bytes();
    assert_eq!(bytes.len(), std::mem::size_of::<TorqueCommand>());
    assert_eq!(bytes[0], TorqueCommand::REPORT_ID);
    Ok(())
}

// =========================================================================
// 8. Hot-plug detection behaviour
// =========================================================================

#[tokio::test]
async fn virtual_port_add_device_while_listing() -> Result<(), Box<dyn std::error::Error>> {
    let mut port = VirtualHidPort::new();
    let id1 = device_id("hotplug-1")?;
    port.add_device(VirtualDevice::new(id1, "Wheel 1".into()))?;

    // Snapshot before second add
    let before = port.list_devices().await?;
    assert_eq!(before.len(), 1);

    let id2 = device_id("hotplug-2")?;
    port.add_device(VirtualDevice::new(id2, "Wheel 2".into()))?;

    // After adding, list shows both
    let after = port.list_devices().await?;
    assert_eq!(after.len(), 2);
    Ok(())
}

#[tokio::test]
async fn virtual_port_remove_then_open_errors() -> Result<(), Box<dyn std::error::Error>> {
    let mut port = VirtualHidPort::new();
    let id = device_id("remove-open")?;
    port.add_device(VirtualDevice::new(id.clone(), "Remove Wheel".into()))?;

    port.remove_device(&id)?;
    let result = port.open_device(&id).await;
    assert!(result.is_err());
    Ok(())
}

#[test]
fn virtual_device_physics_changes_telemetry() -> Result<(), Box<dyn std::error::Error>> {
    let id = device_id("physics-dev")?;
    let mut dev = VirtualDevice::new(id, "Physics Wheel".into());

    let before = dev.read_telemetry().ok_or("no telemetry before")?;
    assert!((before.wheel_angle_deg).abs() < 0.001);

    // Apply torque and simulate
    assert!(dev.write_ffb_report(10.0, 1).is_ok());
    for _ in 0..20 {
        dev.simulate_physics(Duration::from_millis(10));
    }

    let after = dev.read_telemetry().ok_or("no telemetry after")?;
    assert!(after.wheel_angle_deg.abs() > 0.0);
    assert!(after.wheel_speed_rad_s.abs() > 0.0);
    Ok(())
}

// =========================================================================
// Additional: Torque report encoding per vendor
// =========================================================================

#[test]
fn encode_torque_report_generic_device() -> Result<(), Box<dyn std::error::Error>> {
    let mut out = [0u8; MAX_TORQUE_REPORT_SIZE];
    let len = encode_torque_report_for_device(0x9999, 0x1111, 10.0, 5.0, 42, &mut out);

    assert_eq!(len, std::mem::size_of::<TorqueCommand>());
    assert_eq!(out[0], TorqueCommand::REPORT_ID);
    let torque = i16::from_le_bytes([out[1], out[2]]);
    assert_eq!(torque, (5.0f32 * 256.0) as i16);
    let seq = u16::from_le_bytes([out[4], out[5]]);
    assert_eq!(seq, 42);
    Ok(())
}

#[test]
fn encode_torque_report_fanatec_device() -> Result<(), Box<dyn std::error::Error>> {
    let mut out = [0u8; MAX_TORQUE_REPORT_SIZE];
    let len = encode_torque_report_for_device(0x0EB7, 0x0024, 8.0, 4.0, 1, &mut out);

    assert!(len > 0);
    assert_eq!(out[0], 0x01); // Fanatec FFB report ID
    assert_eq!(out[1], 0x01); // Constant force command
    Ok(())
}

#[test]
fn encode_torque_report_moza_device() -> Result<(), Box<dyn std::error::Error>> {
    let mut out = [0u8; MAX_TORQUE_REPORT_SIZE];
    let len = encode_torque_report_for_device(0x346E, 0x0004, 5.5, 2.0, 7, &mut out);

    assert_eq!(len, MAX_TORQUE_REPORT_SIZE);
    assert_eq!(out[0], super::vendor::moza::report_ids::DIRECT_TORQUE);
    Ok(())
}

#[test]
fn encode_torque_report_zero_torque_all_vendors() -> Result<(), Box<dyn std::error::Error>> {
    // Generic
    let mut out = [0u8; MAX_TORQUE_REPORT_SIZE];
    encode_torque_report_for_device(0x9999, 0x1111, 10.0, 0.0, 0, &mut out);
    let torque = i16::from_le_bytes([out[1], out[2]]);
    assert_eq!(torque, 0);

    // Fanatec zero
    let mut out = [0u8; MAX_TORQUE_REPORT_SIZE];
    encode_torque_report_for_device(0x0EB7, 0x0020, 8.0, 0.0, 0, &mut out);
    assert_eq!(out[2], 0x00);
    assert_eq!(out[3], 0x00);
    Ok(())
}

// =========================================================================
// Additional: Conditional coefficient direction fix
// =========================================================================

#[test]
fn conditional_direction_fix_swaps_coefficients() -> Result<(), Box<dyn std::error::Error>> {
    let coeffs = ConditionalCoefficients {
        positive_coefficient: 200,
        negative_coefficient: -100,
        positive_saturation: 2000,
        negative_saturation: 1000,
        dead_band: 50,
        center: 0,
    };

    let fixed = coeffs.apply_direction_fix(true);
    assert_eq!(fixed.positive_coefficient, -100);
    assert_eq!(fixed.negative_coefficient, 200);
    assert_eq!(fixed.positive_saturation, 1000);
    assert_eq!(fixed.negative_saturation, 2000);
    assert_eq!(fixed.dead_band, 50);
    assert_eq!(fixed.center, 0);
    Ok(())
}

#[test]
fn conditional_direction_fix_noop_when_disabled() -> Result<(), Box<dyn std::error::Error>> {
    let coeffs = ConditionalCoefficients {
        positive_coefficient: 200,
        negative_coefficient: -100,
        positive_saturation: 2000,
        negative_saturation: 1000,
        dead_band: 50,
        center: 0,
    };

    let same = coeffs.apply_direction_fix(false);
    assert_eq!(same.positive_coefficient, 200);
    assert_eq!(same.negative_coefficient, -100);
    Ok(())
}

// =========================================================================
// Additional: RT stream mailbox and watchdog
// =========================================================================

#[test]
fn torque_mailbox_defaults_disarmed() -> Result<(), Box<dyn std::error::Error>> {
    let mb = TorqueMailbox::new();
    assert!(!mb.armed.load(Ordering::Relaxed));
    assert_eq!(mb.torque.load(Ordering::Relaxed), 0);
    assert_eq!(mb.seq.load(Ordering::Relaxed), 0);
    assert_eq!(mb.flags.load(Ordering::Relaxed), 0);
    Ok(())
}

#[test]
fn stream_config_defaults_reasonable() -> Result<(), Box<dyn std::error::Error>> {
    let config = StreamConfig::default();
    assert_eq!(config.user_abs_limit, i16::MAX);
    assert!(config.watchdog_max_stale_ticks > 0);
    Ok(())
}

// =========================================================================
// Additional: IOKit matching dictionary edge cases
// =========================================================================

#[test]
fn matching_dict_get_integer_missing_key_returns_none() -> Result<(), Box<dyn std::error::Error>> {
    let dict = IOKitMatchingDict::new().with_vendor_id(0x1234);
    assert!(dict.get_integer("VendorID").is_some());
    assert!(dict.get_integer("ProductID").is_none());
    assert!(dict.get_integer("NonExistent").is_none());
    Ok(())
}

#[test]
fn matching_dict_string_entries_match_unconditionally() -> Result<(), Box<dyn std::error::Error>> {
    let dict = IOKitMatchingDict::new()
        .with_transport("USB")
        .with_vendor_id(0x0EB7);

    // Transport string entry passes through — only numeric keys are checked
    assert!(dict.matches_device(
        0x0EB7,
        0x0001,
        macos::usage_page::GENERIC_DESKTOP,
        macos::usage::WHEEL,
    ));
    Ok(())
}

#[test]
fn racing_wheel_matching_dict_preset() -> Result<(), Box<dyn std::error::Error>> {
    let dict = racing_wheel_matching_dict();
    assert_eq!(
        dict.get_integer("DeviceUsagePage"),
        Some(u64::from(macos::usage_page::GENERIC_DESKTOP))
    );
    assert_eq!(
        dict.get_integer("DeviceUsage"),
        Some(u64::from(macos::usage::WHEEL))
    );
    Ok(())
}

#[test]
fn device_matching_dict_preset() -> Result<(), Box<dyn std::error::Error>> {
    let dict = device_matching_dict(0x0EB7, 0x0024);
    assert_eq!(dict.get_integer("VendorID"), Some(0x0EB7));
    assert_eq!(dict.get_integer("ProductID"), Some(0x0024));

    let has_transport = dict.entries.iter().any(|(k, _)| k == "Transport");
    assert!(has_transport);
    Ok(())
}

// =========================================================================
// Additional: IOKitDeviceDescriptor capability heuristics
// =========================================================================

#[test]
fn descriptor_with_pid_detects_ffb_capable() -> Result<(), Box<dyn std::error::Error>> {
    let desc = make_test_descriptor(0x346E, 0x0004);
    let info = desc.to_hid_device_info()?;
    assert!(info.capabilities.supports_pid);
    assert!(info.capabilities.supports_raw_torque_1khz);
    assert_eq!(info.capabilities.min_report_period_us, 1000);
    Ok(())
}

#[test]
fn descriptor_without_pid_no_ffb() -> Result<(), Box<dyn std::error::Error>> {
    let desc = IOKitDeviceDescriptor {
        elements: vec![steering_element(), button_element(1)],
        ..make_test_descriptor(0x046D, 0xC294)
    };
    let info = desc.to_hid_device_info()?;
    assert!(!info.capabilities.supports_pid);
    assert!(!info.capabilities.supports_raw_torque_1khz);
    assert_eq!(info.capabilities.min_report_period_us, 8000);
    Ok(())
}

#[test]
fn descriptor_empty_elements_minimal_caps() -> Result<(), Box<dyn std::error::Error>> {
    let desc = IOKitDeviceDescriptor {
        elements: vec![],
        ..make_test_descriptor(0x1234, 0x5678)
    };
    assert!(!desc.has_pid_outputs());
    assert!(desc.steering_element().is_none());
    assert!(desc.button_elements().is_empty());

    let info = desc.to_hid_device_info()?;
    assert!(!info.capabilities.supports_pid);
    Ok(())
}

#[test]
fn descriptor_feature_elements_counted() -> Result<(), Box<dyn std::error::Error>> {
    let desc = IOKitDeviceDescriptor {
        elements: vec![
            steering_element(),
            feature_element(macos::usage_page::PID, 0x21),
            feature_element(macos::usage_page::PID, 0x22),
        ],
        ..make_test_descriptor(0x346E, 0x0004)
    };
    assert_eq!(desc.count_elements(IOHIDElementType::Feature), 2);
    assert_eq!(desc.count_elements(IOHIDElementType::InputAxis), 1);
    Ok(())
}

#[test]
fn descriptor_device_path_format() -> Result<(), Box<dyn std::error::Error>> {
    let desc = IOKitDeviceDescriptor {
        location_id: 0x00FF_1234,
        ..make_test_descriptor(0x346E, 0x0004)
    };
    assert_eq!(desc.device_path(), "IOService:/AppleUSBDevice@00FF1234");
    Ok(())
}

// =========================================================================
// Additional: element type classification
// =========================================================================

#[test]
fn element_type_input_variants() -> Result<(), Box<dyn std::error::Error>> {
    assert!(IOHIDElementType::InputMisc.is_input());
    assert!(IOHIDElementType::InputButton.is_input());
    assert!(IOHIDElementType::InputAxis.is_input());
    assert!(!IOHIDElementType::Output.is_input());
    assert!(!IOHIDElementType::Feature.is_input());
    assert!(!IOHIDElementType::Collection.is_input());
    Ok(())
}

#[test]
fn element_type_output_variant() -> Result<(), Box<dyn std::error::Error>> {
    assert!(IOHIDElementType::Output.is_output());
    assert!(!IOHIDElementType::InputAxis.is_output());
    assert!(!IOHIDElementType::Feature.is_output());
    assert!(!IOHIDElementType::Collection.is_output());
    Ok(())
}

#[test]
fn element_type_from_raw_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    let types = [
        (1u32, IOHIDElementType::InputMisc),
        (2, IOHIDElementType::InputButton),
        (3, IOHIDElementType::InputAxis),
        (129, IOHIDElementType::Output),
        (257, IOHIDElementType::Feature),
        (513, IOHIDElementType::Collection),
    ];
    for (raw, expected) in types {
        let parsed = IOHIDElementType::from_raw(raw).ok_or("from_raw returned None")?;
        assert_eq!(parsed, expected);
    }
    // Invalid values
    assert!(IOHIDElementType::from_raw(0).is_none());
    assert!(IOHIDElementType::from_raw(999).is_none());
    Ok(())
}

#[test]
fn element_bit_width_multi_count() -> Result<(), Box<dyn std::error::Error>> {
    let elem = HIDElement {
        report_size: 8,
        report_count: 4,
        ..steering_element()
    };
    assert_eq!(elem.bit_width(), 32);
    Ok(())
}

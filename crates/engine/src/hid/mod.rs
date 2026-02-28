//! HID adapter implementations with OS-specific RT optimizations
//!
//! This module provides platform-specific HID device adapters that implement
//! the HidPort and HidDevice traits with real-time optimizations for each OS.

use crate::input::SnapshotMailbox as Seqlock;
use crate::ports::HidPort;
use crate::{DeviceInfo, TelemetryData};
use racing_wheel_schemas::prelude::*;

pub use racing_wheel_hid_moza_protocol::MozaInputState;

pub mod quirks;
pub mod rt_stream;
pub mod vendor;
pub mod virtual_device;

#[cfg(windows)]
pub mod windows;

#[cfg(windows)]
#[cfg(test)]
mod windows_property_tests;

#[cfg(unix)]
pub mod linux;

/// Platform-specific HID port factory
pub fn create_hid_port() -> Result<Box<dyn HidPort>, Box<dyn std::error::Error>> {
    #[cfg(windows)]
    {
        Ok(Box::new(windows::WindowsHidPort::new()?))
    }

    #[cfg(unix)]
    {
        Ok(Box::new(linux::LinuxHidPort::new()?))
    }

    #[cfg(not(any(windows, unix)))]
    {
        Err("Unsupported platform for HID operations".into())
    }
}

/// Common HID device implementation details
#[derive(Debug, Clone)]
pub struct HidDeviceInfo {
    pub device_id: DeviceId,
    pub vendor_id: u16,
    pub product_id: u16,
    pub serial_number: Option<String>,
    pub manufacturer: Option<String>,
    pub product_name: Option<String>,
    pub path: String,
    /// HID interface number from the OS (used for multi-interface devices).
    pub interface_number: Option<i32>,
    /// HID usage page from the device descriptor (top-level collection).
    pub usage_page: Option<u16>,
    /// HID usage from the device descriptor (top-level collection).
    pub usage: Option<u16>,
    /// Length of the raw HID report descriptor in bytes (Linux/capture-ids).
    pub report_descriptor_len: Option<u32>,
    /// CRC32 of the raw HID report descriptor (Linux/capture-ids, for allowlisting).
    pub report_descriptor_crc32: Option<u32>,
    pub capabilities: DeviceCapabilities,
}

impl HidDeviceInfo {
    pub fn to_device_info(&self) -> DeviceInfo {
        DeviceInfo {
            id: self.device_id.clone(),
            name: self.product_name.clone().unwrap_or_else(|| {
                format!(
                    "Racing Wheel {:04X}:{:04X}",
                    self.vendor_id, self.product_id
                )
            }),
            vendor_id: self.vendor_id,
            product_id: self.product_id,
            serial_number: self.serial_number.clone(),
            manufacturer: self.manufacturer.clone(),
            path: self.path.clone(),
            capabilities: self.capabilities.clone(),
            is_connected: true,
        }
    }
}

/// HID report structures for OWP-1 protocol
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct TorqueCommand {
    pub report_id: u8,       // 0x20
    pub torque_nm_q8_8: i16, // Q8.8 fixed point, Newton-meters
    pub flags: u8,           // bit0: hands_on_hint, bit1: sat_warn
    pub seq: u16,            // sequence number, wraps
}

impl TorqueCommand {
    pub const REPORT_ID: u8 = 0x20;

    pub fn new(torque_nm: f32, seq: u16, hands_on_hint: bool, sat_warn: bool) -> Self {
        // Convert torque from Nm to Q8.8 fixed point
        let torque_nm_q8_8 = (torque_nm * 256.0).clamp(i16::MIN as f32, i16::MAX as f32) as i16;

        let mut flags = 0u8;
        if hands_on_hint {
            flags |= 0x01;
        }
        if sat_warn {
            flags |= 0x02;
        }

        Self {
            report_id: Self::REPORT_ID,
            torque_nm_q8_8,
            flags,
            seq,
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(
                self as *const Self as *const u8,
                std::mem::size_of::<Self>(),
            )
        }
    }
}

/// Maximum wire size of torque output reports used by current device encoders.
pub const MAX_TORQUE_REPORT_SIZE: usize = 8;

/// Encode a torque report for the target device.
///
/// - Fanatec wheelbases: vendor-native constant-force report (`0x01`, 8 bytes)
/// - Moza wheelbases: vendor-native direct torque report (`0x20`, 8 bytes)
/// - Other devices: legacy OWP-1 torque command layout
pub fn encode_torque_report_for_device(
    vendor_id: u16,
    product_id: u16,
    max_torque_nm: f32,
    torque_nm: f32,
    seq: u16,
    out: &mut [u8; MAX_TORQUE_REPORT_SIZE],
) -> usize {
    if vendor_id == 0x0EB7 && vendor::fanatec::is_wheelbase_product(product_id) {
        let encoder = vendor::fanatec::FanatecConstantForceEncoder::new(max_torque_nm);
        return encoder.encode(torque_nm, seq, out);
    }

    if vendor_id == 0x346E && vendor::moza::is_wheelbase_product(product_id) {
        let encoder = vendor::moza_direct::MozaDirectTorqueEncoder::new(max_torque_nm);
        return encoder.encode(torque_nm, seq, out);
    }

    let command = TorqueCommand::new(torque_nm, seq, true, false);
    let bytes = command.as_bytes();
    out[..bytes.len()].copy_from_slice(bytes);
    bytes.len()
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct DeviceTelemetryReport {
    pub report_id: u8,           // 0x21
    pub wheel_angle_mdeg: i32,   // millidegrees
    pub wheel_speed_mrad_s: i16, // milliradians per second
    pub temp_c: u8,              // temperature in Celsius
    pub faults: u8,              // fault bitfield
    pub hands_on: u8,            // 0/1 if device can detect
    pub reserved: [u8; 2],       // padding for alignment
}

impl DeviceTelemetryReport {
    pub const REPORT_ID: u8 = 0x21;

    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < std::mem::size_of::<Self>() || data[0] != Self::REPORT_ID {
            return None;
        }

        unsafe { Some(std::ptr::read_unaligned(data.as_ptr() as *const Self)) }
    }

    pub fn to_telemetry_data(&self) -> TelemetryData {
        TelemetryData {
            wheel_angle_deg: self.wheel_angle_mdeg as f32 / 1000.0,
            wheel_speed_rad_s: self.wheel_speed_mrad_s as f32 / 1000.0,
            temperature_c: self.temp_c,
            fault_flags: self.faults,
            hands_on: self.hands_on != 0,
            timestamp: std::time::Instant::now(),
        }
    }
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct DeviceCapabilitiesReport {
    pub report_id: u8,                // 0x01
    pub supports_pid: u8,             // bit0: PID support
    pub supports_raw_torque_1khz: u8, // bit0: raw torque @ 1kHz
    pub supports_health_stream: u8,   // bit0: health telemetry
    pub supports_led_bus: u8,         // bit0: LED control
    pub max_torque_cnm: u16,          // centinewton-meters
    pub encoder_cpr: u16,             // counts per revolution
    pub min_report_period_us: u8,     // minimum report period in microseconds
    pub reserved: [u8; 6],            // padding
}

impl DeviceCapabilitiesReport {
    pub const REPORT_ID: u8 = 0x01;

    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < std::mem::size_of::<Self>() || data[0] != Self::REPORT_ID {
            return None;
        }

        unsafe { Some(std::ptr::read_unaligned(data.as_ptr() as *const Self)) }
    }

    #[allow(clippy::expect_used)]
    pub fn to_device_capabilities(&self) -> DeviceCapabilities {
        // Convert cNm to Nm, clamping to valid range
        let nm = (self.max_torque_cnm as f32) / 100.0;
        let clamped_nm = nm.clamp(0.0, TorqueNm::MAX_TORQUE);

        DeviceCapabilities {
            supports_pid: (self.supports_pid & 0x01) != 0,
            supports_raw_torque_1khz: (self.supports_raw_torque_1khz & 0x01) != 0,
            supports_health_stream: (self.supports_health_stream & 0x01) != 0,
            supports_led_bus: (self.supports_led_bus & 0x01) != 0,
            max_torque: TorqueNm::new(clamped_nm).expect("clamped value is valid"),
            encoder_cpr: self.encoder_cpr,
            min_report_period_us: self.min_report_period_us as u16,
        }
    }
}

/// RT setup utilities for platform-specific optimizations
pub struct RTSetup;

impl RTSetup {
    /// Apply platform-specific RT optimizations
    pub fn apply_rt_optimizations() -> Result<(), Box<dyn std::error::Error>> {
        #[cfg(windows)]
        {
            windows::apply_windows_rt_setup()?;
        }

        #[cfg(unix)]
        {
            linux::apply_linux_rt_setup()?;
        }

        Ok(())
    }

    /// Revert RT optimizations (cleanup)
    pub fn revert_rt_optimizations() -> Result<(), Box<dyn std::error::Error>> {
        #[cfg(windows)]
        {
            windows::revert_windows_rt_setup()?;
        }

        #[cfg(unix)]
        {
            linux::revert_linux_rt_setup()?;
        }

        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_torque_command_creation() {
        let cmd = TorqueCommand::new(5.0, 123, true, false);

        assert_eq!(cmd.report_id, TorqueCommand::REPORT_ID);
        // Copy packed fields to avoid alignment issues
        let seq = cmd.seq;
        let flags = cmd.flags;
        let torque = cmd.torque_nm_q8_8;

        assert_eq!(seq, 123);
        assert_eq!(flags, 0x01); // hands_on_hint set

        // Test torque conversion: 5.0 Nm -> 1280 (Q8.8 fixed point)
        assert_eq!(torque, 1280i16);
    }

    #[test]
    fn test_torque_command_serialization() {
        let cmd = TorqueCommand::new(2.5, 456, false, true);
        let bytes = cmd.as_bytes();

        assert_eq!(bytes.len(), std::mem::size_of::<TorqueCommand>());
        assert_eq!(bytes[0], TorqueCommand::REPORT_ID);
    }

    #[test]
    fn test_device_telemetry_deserialization() {
        let mut data = vec![0u8; std::mem::size_of::<DeviceTelemetryReport>()];
        data[0] = DeviceTelemetryReport::REPORT_ID;

        // Set wheel angle to 90 degrees (90000 millidegrees)
        let angle_bytes = 90000i32.to_le_bytes();
        data[1..5].copy_from_slice(&angle_bytes);

        let report = DeviceTelemetryReport::from_bytes(&data).unwrap();
        assert_eq!(report.report_id, DeviceTelemetryReport::REPORT_ID);

        // Copy packed field to avoid alignment issues
        let wheel_angle = report.wheel_angle_mdeg;
        assert_eq!(wheel_angle, 90000);

        let telemetry = report.to_telemetry_data();
        assert!((telemetry.wheel_angle_deg - 90.0).abs() < 0.001);
    }

    #[test]
    fn test_device_capabilities_deserialization() {
        let mut data = vec![0u8; std::mem::size_of::<DeviceCapabilitiesReport>()];
        data[0] = DeviceCapabilitiesReport::REPORT_ID;
        data[1] = 0x01; // supports_pid
        data[2] = 0x01; // supports_raw_torque_1khz
        data[3] = 0x01; // supports_health_stream
        data[4] = 0x01; // supports_led_bus

        // Set max torque to 25.0 Nm (2500 cNm)
        let torque_bytes = 2500u16.to_le_bytes();
        data[5..7].copy_from_slice(&torque_bytes);

        // Set encoder CPR to 4096
        let cpr_bytes = 4096u16.to_le_bytes();
        data[7..9].copy_from_slice(&cpr_bytes);

        data[9] = 100; // min_report_period_us = 100us (10kHz max)

        let report = DeviceCapabilitiesReport::from_bytes(&data).unwrap();
        let caps = report.to_device_capabilities();

        assert!(caps.supports_pid);
        assert!(caps.supports_raw_torque_1khz);
        assert!(caps.supports_health_stream);
        assert!(caps.supports_led_bus);
        assert!((caps.max_torque.value() - 25.0).abs() < 0.001);
        assert_eq!(caps.encoder_cpr, 4096);
        assert_eq!(caps.min_report_period_us, 100);
    }

    #[test]
    fn test_encode_torque_report_generic_device_uses_owp1_layout() {
        let mut out = [0u8; MAX_TORQUE_REPORT_SIZE];
        let len = encode_torque_report_for_device(0x046D, 0xC294, 5.0, 2.0, 77, &mut out);

        assert_eq!(len, std::mem::size_of::<TorqueCommand>());
        assert_eq!(out[0], TorqueCommand::REPORT_ID);
        let encoded = i16::from_le_bytes([out[1], out[2]]);
        assert_eq!(encoded, (2.0 * 256.0) as i16);
        let seq = u16::from_le_bytes([out[4], out[5]]);
        assert_eq!(seq, 77);
    }

    #[test]
    fn test_encode_torque_report_moza_uses_direct_layout() {
        let mut out = [0u8; MAX_TORQUE_REPORT_SIZE];
        let len = encode_torque_report_for_device(0x346E, 0x0004, 5.5, 5.5, 11, &mut out);

        assert_eq!(len, MAX_TORQUE_REPORT_SIZE);
        assert_eq!(out[0], vendor::moza::report_ids::DIRECT_TORQUE);
        assert_eq!(i16::from_le_bytes([out[1], out[2]]), i16::MAX);
    }

    #[test]
    fn test_encode_torque_report_fanatec_uses_constant_force_layout() {
        let mut out = [0u8; MAX_TORQUE_REPORT_SIZE];
        // GT DD Pro at 0x0024 (8 Nm), full positive torque
        let len = encode_torque_report_for_device(0x0EB7, 0x0024, 8.0, 8.0, 0, &mut out);

        assert_eq!(len, vendor::fanatec::CONSTANT_FORCE_REPORT_LEN);
        assert_eq!(out[0], 0x01); // FFB output report ID
        assert_eq!(out[1], 0x01); // constant force command
        assert_eq!(i16::from_le_bytes([out[2], out[3]]), i16::MAX);
    }

    #[test]
    fn test_encode_torque_report_fanatec_zero_torque() {
        let mut out = [0u8; MAX_TORQUE_REPORT_SIZE];
        let len = encode_torque_report_for_device(0x0EB7, 0x0020, 8.0, 0.0, 0, &mut out);

        assert_eq!(len, vendor::fanatec::CONSTANT_FORCE_REPORT_LEN);
        assert_eq!(out[0], 0x01);
        assert_eq!(out[1], 0x01);
        assert_eq!(out[2], 0x00);
        assert_eq!(out[3], 0x00);
    }
}

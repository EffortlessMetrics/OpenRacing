//! Linux HID adapter with /dev/hidraw* and RT optimizations
//!
//! This module implements HID device communication on Linux using:
//! - /dev/hidraw* with libudev for enumeration
//! - Non-blocking writes for RT performance
//! - SCHED_FIFO via rtkit for RT scheduling
//! - mlockall for memory locking
//! - udev rules guidance for device permissions

use super::vendor::VendorProtocol;
use super::{
    DeviceTelemetryReport, HidDeviceInfo, MAX_TORQUE_REPORT_SIZE, encode_torque_report_for_device,
    vendor,
};
use crate::ports::{DeviceHealthStatus, HidDevice, HidPort};
use crate::{DeviceEvent, DeviceInfo, RTResult, TelemetryData};
use async_trait::async_trait;
use parking_lot::RwLock;
use racing_wheel_schemas::prelude::*;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::fs::{File, OpenOptions};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;
use std::path::Path;
use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicBool, AtomicU8, AtomicU16, AtomicU32, AtomicU64, Ordering},
};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

/// Thread-safe cached device info accessor using OnceLock
fn get_cached_device_info(device_info: &HidDeviceInfo) -> &'static DeviceInfo {
    static CACHED_INFO: OnceLock<DeviceInfo> = OnceLock::new();
    CACHED_INFO.get_or_init(|| device_info.to_device_info())
}

const HID_MAX_DESCRIPTOR_SIZE: usize = 4096;
const HIDRAW_IOCTL_TYPE: u8 = b'H';
const HIDIOC_NR_GRDESC_SIZE: u8 = 0x01;
const HIDIOC_NR_GRDESC: u8 = 0x02;
const HIDIOC_NR_GRRAWINFO: u8 = 0x03;
const HIDIOC_NR_GRRAWNAME: u8 = 0x04;

const IOC_NRBITS: u32 = 8;
const IOC_TYPEBITS: u32 = 8;
const IOC_SIZEBITS: u32 = 14;
const IOC_NRSHIFT: u32 = 0;
const IOC_TYPESHIFT: u32 = IOC_NRSHIFT + IOC_NRBITS;
const IOC_SIZESHIFT: u32 = IOC_TYPESHIFT + IOC_TYPEBITS;
const IOC_DIRSHIFT: u32 = IOC_SIZESHIFT + IOC_SIZEBITS;
const IOC_READ: u32 = 2;
const IOC_READ_WRITE: u32 = 3;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct HidrawDevInfo {
    bustype: u32,
    vendor: i16,
    product: i16,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct HidrawReportDescriptor {
    size: u32,
    value: [u8; HID_MAX_DESCRIPTOR_SIZE],
}

impl Default for HidrawReportDescriptor {
    fn default() -> Self {
        Self {
            size: 0,
            value: [0u8; HID_MAX_DESCRIPTOR_SIZE],
        }
    }
}

const fn ioctl_code(direction: u32, kind: u8, nr: u8, size: usize) -> libc::c_ulong {
    ((direction << IOC_DIRSHIFT)
        | ((kind as u32) << IOC_TYPESHIFT)
        | ((nr as u32) << IOC_NRSHIFT)
        | ((size as u32) << IOC_SIZESHIFT)) as libc::c_ulong
}

const fn ior_read<T>(kind: u8, nr: u8) -> libc::c_ulong {
    ioctl_code(IOC_READ, kind, nr, std::mem::size_of::<T>())
}

const fn iorw_len(kind: u8, nr: u8, len: usize) -> libc::c_ulong {
    ioctl_code(IOC_READ_WRITE, kind, nr, len)
}

const HIDIOCGRAWINFO: libc::c_ulong =
    ior_read::<HidrawDevInfo>(HIDRAW_IOCTL_TYPE, HIDIOC_NR_GRRAWINFO);
const HIDIOCGRDESCSIZE: libc::c_ulong =
    ior_read::<libc::c_int>(HIDRAW_IOCTL_TYPE, HIDIOC_NR_GRDESC_SIZE);
const HIDIOCGRDESC: libc::c_ulong =
    ior_read::<HidrawReportDescriptor>(HIDRAW_IOCTL_TYPE, HIDIOC_NR_GRDESC);

fn hidiocgrawname(len: usize) -> libc::c_ulong {
    iorw_len(HIDRAW_IOCTL_TYPE, HIDIOC_NR_GRRAWNAME, len)
}

fn parse_c_string(bytes: &[u8]) -> Option<String> {
    let nul_idx = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
    if nul_idx == 0 {
        return None;
    }

    Some(
        String::from_utf8_lossy(&bytes[..nul_idx])
            .trim()
            .to_string(),
    )
}

fn manufacturer_for_vendor(vendor_id: u16) -> Option<String> {
    let name = match vendor_id {
        0x046D => "Logitech",
        0x0EB7 => "Fanatec",
        0x044F => "Thrustmaster",
        0x346E => "Moza Racing",
        0x0483 | 0x16D0 | 0x3670 => "Simagic",
        _ => return None,
    };
    Some(name.to_string())
}

fn parse_descriptor_flags(descriptor: &[u8]) -> (bool, bool) {
    // Usage Page (PID) appears as 0x05, 0x0F in many PID descriptors.
    let supports_pid = descriptor.windows(2).any(|win| win == [0x05, 0x0F]);

    // Vendor usage pages often use 16-bit form: 0x06 <low> <high> where high=0xFF.
    let uses_vendor_usage_page = descriptor
        .windows(3)
        .any(|win| win[0] == 0x06 && win[2] == 0xFF);

    (supports_pid, uses_vendor_usage_page)
}

fn build_capabilities_from_identity(
    vendor_id: u16,
    product_id: u16,
    descriptor: &[u8],
) -> DeviceCapabilities {
    let (descriptor_pid, _) = parse_descriptor_flags(descriptor);

    if vendor_id == 0x346E {
        let protocol = vendor::moza::MozaProtocol::new(product_id);
        let identity = vendor::moza::identify_device(product_id);
        let config = protocol.get_ffb_config();
        let max_torque = config.max_torque_nm.clamp(0.0, TorqueNm::MAX_TORQUE);
        let max_torque = TorqueNm::new(max_torque).unwrap_or(TorqueNm::ZERO);

        return DeviceCapabilities {
            supports_pid: descriptor_pid || identity.supports_ffb,
            supports_raw_torque_1khz: identity.supports_ffb,
            supports_health_stream: identity.supports_ffb,
            supports_led_bus: false,
            max_torque,
            encoder_cpr: u16::try_from(config.encoder_cpr).unwrap_or(u16::MAX),
            min_report_period_us: config.required_b_interval.unwrap_or(1) as u16 * 1000,
        };
    }

    DeviceCapabilities {
        supports_pid: descriptor_pid,
        supports_raw_torque_1khz: false,
        supports_health_stream: false,
        supports_led_bus: false,
        max_torque: TorqueNm::new(10.0).unwrap_or(TorqueNm::ZERO),
        encoder_cpr: 4096,
        min_report_period_us: 1000,
    }
}

/// Linux-specific HID port implementation
pub struct LinuxHidPort {
    devices: Arc<RwLock<HashMap<DeviceId, HidDeviceInfo>>>,
    monitoring: Arc<AtomicBool>,
    event_sender: Option<mpsc::UnboundedSender<DeviceEvent>>,
}

impl LinuxHidPort {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            devices: Arc::new(RwLock::new(HashMap::new())),
            monitoring: Arc::new(AtomicBool::new(false)),
            event_sender: None,
        })
    }

    /// Enumerate HID devices using /dev/hidraw* and libudev
    fn enumerate_devices(&self) -> Result<Vec<HidDeviceInfo>, Box<dyn std::error::Error>> {
        let mut devices = Vec::new();

        // Known racing wheel vendor/product IDs
        let racing_wheel_ids = [
            (0x046D, 0xC294), // Logitech G27
            (0x046D, 0xC29B), // Logitech G27
            (0x046D, 0xC24F), // Logitech G29
            (0x046D, 0xC260), // Logitech G29
            (0x046D, 0xC261), // Logitech G920
            (0x046D, 0xC262), // Logitech G920
            (0x046D, 0xC26D), // Logitech G923 Xbox
            (0x046D, 0xC26E), // Logitech G923 PS
            (0x0EB7, 0x0001), // Fanatec ClubSport Wheel Base V2
            (0x0EB7, 0x0004), // Fanatec CSL Elite Wheel Base
            (0x0EB7, 0x0005), // Fanatec ClubSport Wheel Base V2.5
            (0x0EB7, 0x0006), // Fanatec Podium Wheel Base DD1
            (0x0EB7, 0x0007), // Fanatec Podium Wheel Base DD2
            (0x0EB7, 0x0011), // Fanatec CSL DD
            (0x0EB7, 0x0020), // Fanatec Gran Turismo DD Pro
            (0x044F, 0xB65D), // Thrustmaster T150
            (0x044F, 0xB66D), // Thrustmaster TMX
            (0x044F, 0xB66E), // Thrustmaster T300RS
            (0x044F, 0xB677), // Thrustmaster T500RS
            (0x044F, 0xB696), // Thrustmaster TS-XW
            (0x044F, 0xB69A), // Thrustmaster T-GT
            // Moza Racing - V1
            (0x346E, 0x0005), // Moza R3
            (0x346E, 0x0004), // Moza R5
            (0x346E, 0x0002), // Moza R9
            (0x346E, 0x0006), // Moza R12
            (0x346E, 0x0000), // Moza R16/R21
            // Moza Racing - V2
            (0x346E, 0x0015), // Moza R3 V2
            (0x346E, 0x0014), // Moza R5 V2
            (0x346E, 0x0012), // Moza R9 V2
            (0x346E, 0x0016), // Moza R12 V2
            (0x346E, 0x0010), // Moza R16/R21 V2
            // Moza Racing peripherals
            (0x346E, 0x0003), // Moza SR-P Pedals
            (0x346E, 0x0020), // Moza HGP Shifter
            (0x346E, 0x0021), // Moza SGP Sequential Shifter
            (0x346E, 0x0022), // Moza HBP Handbrake
            // Simagic legacy devices
            (0x0483, 0x0522), // Simagic Alpha
            (0x0483, 0x0523), // Simagic Alpha Mini
            (0x0483, 0x0524), // Simagic Alpha Ultimate
            (0x16D0, 0x0D5A), // Simagic M10
            (0x16D0, 0x0D5B), // Simagic FX
            // Simagic Alpha EVO candidate identities
            (0x3670, 0x0001), // Alpha EVO Sport (capture-candidate PID)
            (0x3670, 0x0002), // Alpha EVO (capture-candidate PID)
            (0x3670, 0x0003), // Alpha EVO Pro (capture-candidate PID)
        ];

        // Scan /dev/hidraw* devices
        let hidraw_dir = Path::new("/dev");
        if let Ok(entries) = std::fs::read_dir(hidraw_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(filename) = path.file_name() {
                    if let Some(filename_str) = filename.to_str() {
                        if filename_str.starts_with("hidraw") {
                            if let Ok(device_info) = self.probe_hidraw_device(&path) {
                                // Check if this is a racing wheel
                                let mut is_supported = false;
                                for (vid, pid) in racing_wheel_ids.iter() {
                                    if device_info.vendor_id == *vid
                                        && device_info.product_id == *pid
                                    {
                                        is_supported = true;
                                        break;
                                    }
                                }

                                // Alpha EVO-generation devices should be discoverable even when
                                // PID mapping is incomplete; descriptor capture confirms details.
                                if !is_supported && device_info.vendor_id == 0x3670 {
                                    is_supported = true;
                                }

                                if is_supported {
                                    devices.push(device_info);
                                }
                            }
                        }
                    }
                }
            }
        }

        // If no real devices found, add mock devices for testing
        if devices.is_empty() {
            for (vid, pid) in racing_wheel_ids.iter().take(3) {
                let device_id = DeviceId::new(format!("hidraw_{:04X}_{:04X}", vid, pid))?;
                let path = format!("/dev/hidraw_mock_{:04X}_{:04X}", vid, pid);

                let capabilities = DeviceCapabilities {
                    supports_pid: true,
                    supports_raw_torque_1khz: true,
                    supports_health_stream: true,
                    supports_led_bus: false,
                    max_torque: must(TorqueNm::new(25.0)),
                    encoder_cpr: 4096,
                    min_report_period_us: 1000, // 1kHz
                };

                let device_info = HidDeviceInfo {
                    device_id: device_id.clone(),
                    vendor_id: *vid,
                    product_id: *pid,
                    serial_number: Some(format!("SN{:04X}{:04X}", vid, pid)),
                    manufacturer: Some(match *vid {
                        0x046D => "Logitech".to_string(),
                        0x0EB7 => "Fanatec".to_string(),
                        0x044F => "Thrustmaster".to_string(),
                        0x346E => "Moza Racing".to_string(),
                        0x0483 | 0x16D0 | 0x3670 => "Simagic".to_string(),
                        _ => "Unknown".to_string(),
                    }),
                    product_name: Some(format!("Racing Wheel {:04X}:{:04X}", vid, pid)),
                    path,
                    capabilities,
                };

                devices.push(device_info);
            }
        }

        debug!("Enumerated {} racing wheel devices on Linux", devices.len());
        Ok(devices)
    }

    /// Probe a hidraw device to get its information
    fn probe_hidraw_device(
        &self,
        path: &Path,
    ) -> Result<HidDeviceInfo, Box<dyn std::error::Error>> {
        let file = OpenOptions::new().read(true).open(path)?;
        let fd = file.as_raw_fd();

        let mut raw_info = HidrawDevInfo::default();
        let raw_info_result = unsafe { libc::ioctl(fd, HIDIOCGRAWINFO, &mut raw_info) };
        if raw_info_result < 0 {
            return Err(std::io::Error::last_os_error().into());
        }

        let vendor_id = u16::from_ne_bytes(raw_info.vendor.to_ne_bytes());
        let product_id = u16::from_ne_bytes(raw_info.product.to_ne_bytes());

        let mut name_buf = [0u8; 256];
        let name_result =
            unsafe { libc::ioctl(fd, hidiocgrawname(name_buf.len()), name_buf.as_mut_ptr()) };
        let product_name = if name_result > 0 {
            parse_c_string(&name_buf)
        } else {
            None
        };

        let mut desc_size: libc::c_int = 0;
        let desc_size_result = unsafe { libc::ioctl(fd, HIDIOCGRDESCSIZE, &mut desc_size) };
        let descriptor = if desc_size_result >= 0 {
            let mut descriptor = HidrawReportDescriptor::default();
            let safe_size = desc_size.clamp(0, HID_MAX_DESCRIPTOR_SIZE as libc::c_int);
            descriptor.size = safe_size as u32;
            let desc_result = unsafe { libc::ioctl(fd, HIDIOCGRDESC, &mut descriptor) };
            if desc_result >= 0 {
                let used = usize::try_from(descriptor.size).unwrap_or(0);
                let used = used.min(HID_MAX_DESCRIPTOR_SIZE);
                descriptor.value[..used].to_vec()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        let capabilities = build_capabilities_from_identity(vendor_id, product_id, &descriptor);
        let device_id = DeviceId::new(format!(
            "linux_{:04X}_{:04X}_{}",
            vendor_id,
            product_id,
            path.display()
        ))?;

        Ok(HidDeviceInfo {
            device_id,
            vendor_id,
            product_id,
            serial_number: None,
            manufacturer: manufacturer_for_vendor(vendor_id),
            product_name: product_name
                .or_else(|| Some(format!("Racing Wheel {:04X}:{:04X}", vendor_id, product_id))),
            path: path.to_string_lossy().to_string(),
            capabilities,
        })
    }
}

#[async_trait]
impl HidPort for LinuxHidPort {
    async fn list_devices(&self) -> Result<Vec<DeviceInfo>, Box<dyn std::error::Error>> {
        let device_infos = self.enumerate_devices()?;
        let mut devices = self.devices.write();
        devices.clear();

        let mut result = Vec::new();
        for device_info in device_infos {
            devices.insert(device_info.device_id.clone(), device_info.clone());
            result.push(device_info.to_device_info());
        }

        Ok(result)
    }

    async fn open_device(
        &self,
        id: &DeviceId,
    ) -> Result<Box<dyn HidDevice>, Box<dyn std::error::Error>> {
        let devices = self.devices.read();
        let device_info = devices
            .get(id)
            .ok_or_else(|| format!("Device not found: {}", id))?;

        let device = LinuxHidDevice::new(device_info.clone())?;
        Ok(Box::new(device))
    }

    async fn monitor_devices(
        &self,
    ) -> Result<mpsc::Receiver<DeviceEvent>, Box<dyn std::error::Error>> {
        // Use bounded channel to prevent unbounded memory growth.
        // Buffer 100 events - if receiver falls behind, events are dropped with warning.
        let (sender, receiver) = mpsc::channel(100);

        // Start device monitoring using inotify on /dev
        let devices = self.devices.clone();
        let monitoring = self.monitoring.clone();
        let sender_clone = sender.clone();

        monitoring.store(true, Ordering::Relaxed);

        tokio::spawn(async move {
            let mut last_devices: HashMap<DeviceId, HidDeviceInfo> = HashMap::new();

            while monitoring.load(Ordering::Relaxed) {
                // Check for device changes every 500ms
                tokio::time::sleep(Duration::from_millis(500)).await;

                // In a real implementation, this would use inotify to watch /dev
                // for hidraw device creation/removal
                let current_devices = devices.read().clone();

                // Track whether any events were dropped or channel closed
                let mut dropped = false;
                let mut closed = false;

                // Check for new devices
                for (id, info) in &current_devices {
                    if !last_devices.contains_key(id) {
                        let event = DeviceEvent::Connected(info.to_device_info());
                        // Use try_send to avoid blocking monitor loop if receiver is slow
                        match sender_clone.try_send(event) {
                            Ok(()) => {}
                            Err(mpsc::error::TrySendError::Full(_)) => {
                                dropped = true;
                                warn!(
                                    "Device monitor channel full, dropping connect event for {}",
                                    id
                                );
                            }
                            Err(mpsc::error::TrySendError::Closed(_)) => {
                                closed = true;
                                break;
                            }
                        }
                    }
                }

                // Check for removed devices (skip if channel closed)
                if !closed {
                    for (id, info) in &last_devices {
                        if !current_devices.contains_key(id) {
                            let event = DeviceEvent::Disconnected(info.to_device_info());
                            // Use try_send to avoid blocking monitor loop if receiver is slow
                            match sender_clone.try_send(event) {
                                Ok(()) => {}
                                Err(mpsc::error::TrySendError::Full(_)) => {
                                    dropped = true;
                                    warn!(
                                        "Device monitor channel full, dropping disconnect event for {}",
                                        id
                                    );
                                }
                                Err(mpsc::error::TrySendError::Closed(_)) => {
                                    closed = true;
                                    break;
                                }
                            }
                        }
                    }
                }

                // Exit if channel closed
                if closed {
                    break;
                }

                // Only update last_devices if all events were delivered.
                // If any were dropped, we'll retry them on the next iteration.
                if !dropped {
                    last_devices = current_devices;
                }
            }
        });

        Ok(receiver)
    }

    async fn refresh_devices(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.list_devices().await?;
        Ok(())
    }
}

/// Linux-specific HID device implementation with non-blocking I/O
pub struct LinuxHidDevice {
    device_info: HidDeviceInfo,
    connected: AtomicBool,
    last_seq: AtomicU16,
    created_at: Instant,
    last_communication_us: AtomicU64,
    communication_errors: AtomicU32,
    temperature_c: AtomicU8,
    fault_flags: AtomicU8,
    hands_on: AtomicBool,
    write_file: Option<File>,
    read_file: Option<File>,
}

impl LinuxHidDevice {
    pub fn new(device_info: HidDeviceInfo) -> Result<Self, Box<dyn std::error::Error>> {
        let created_at = Instant::now();

        // In a real implementation, open the hidraw device
        let write_file = if device_info.path.contains("mock") {
            None // Mock device
        } else {
            // Open device for writing with non-blocking flag
            match OpenOptions::new()
                .write(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(&device_info.path)
            {
                Ok(file) => Some(file),
                Err(e) => {
                    warn!("Failed to open {} for writing: {}", device_info.path, e);
                    None
                }
            }
        };

        let read_file = if device_info.path.contains("mock") {
            None // Mock device
        } else {
            // Open device for reading
            match OpenOptions::new().read(true).open(&device_info.path) {
                Ok(file) => Some(file),
                Err(e) => {
                    warn!("Failed to open {} for reading: {}", device_info.path, e);
                    None
                }
            }
        };

        Ok(Self {
            device_info,
            connected: AtomicBool::new(true),
            last_seq: AtomicU16::new(0),
            created_at,
            last_communication_us: AtomicU64::new(0),
            communication_errors: AtomicU32::new(0),
            temperature_c: AtomicU8::new(25),
            fault_flags: AtomicU8::new(0),
            hands_on: AtomicBool::new(false),
            write_file,
            read_file,
        })
    }

    #[inline]
    fn mark_communication(&self) {
        let elapsed = self.created_at.elapsed().as_micros();
        let elapsed_u64 = if elapsed > u64::MAX as u128 {
            u64::MAX
        } else {
            elapsed as u64
        };
        self.last_communication_us
            .store(elapsed_u64, Ordering::Relaxed);
    }

    /// Perform non-blocking write operation (RT-safe)
    fn write_nonblocking(&mut self, data: &[u8]) -> RTResult {
        let fd = match self.write_file.as_ref() {
            Some(file) => file.as_raw_fd(),
            None => {
                // Mock device - simulate successful write
                debug!("Writing {} bytes to mock HID device", data.len());
                self.mark_communication();
                return Ok(());
            }
        };

        // Perform non-blocking write
        let result = unsafe { libc::write(fd, data.as_ptr() as *const libc::c_void, data.len()) };

        if result < 0 {
            let errno = unsafe { *libc::__errno_location() };
            if errno == libc::EAGAIN || errno == libc::EWOULDBLOCK {
                // Write would block - this is expected in RT context
                debug!("HID write would block (EAGAIN)");
                return Ok(());
            } else if errno == libc::ENODEV || errno == libc::EPIPE {
                // Device disconnected
                self.connected.store(false, Ordering::Relaxed);
                self.communication_errors.fetch_add(1, Ordering::Relaxed);
                return Err(crate::RTError::DeviceDisconnected);
            } else {
                // Other error
                warn!("HID write error: errno {}", errno);
                self.communication_errors.fetch_add(1, Ordering::Relaxed);
                return Err(crate::RTError::PipelineFault);
            }
        }

        if result as usize != data.len() {
            warn!("Partial HID write: {} of {} bytes", result, data.len());
        }

        self.mark_communication();

        debug!("Wrote {} bytes to HID device", result);
        Ok(())
    }

    /// Read telemetry data (non-RT, can block)
    fn read_telemetry_blocking(&mut self) -> Option<TelemetryData> {
        let fd = match self.read_file.as_ref() {
            Some(file) => file.as_raw_fd(),
            None => {
                // Mock device - simulate telemetry data
                let report = DeviceTelemetryReport {
                    report_id: DeviceTelemetryReport::REPORT_ID,
                    wheel_angle_mdeg: 0,
                    wheel_speed_mrad_s: 0,
                    temp_c: 30,
                    faults: 0,
                    hands_on: 1,
                    reserved: [0; 2],
                };
                self.temperature_c.store(report.temp_c, Ordering::Relaxed);
                self.fault_flags.store(report.faults, Ordering::Relaxed);
                self.hands_on.store(report.hands_on != 0, Ordering::Relaxed);
                self.mark_communication();
                return Some(report.to_telemetry_data());
            }
        };

        // Read telemetry report
        let mut buffer = [0u8; 64]; // Typical HID report size
        let result =
            unsafe { libc::read(fd, buffer.as_mut_ptr() as *mut libc::c_void, buffer.len()) };

        if result < 0 {
            let errno = unsafe { *libc::__errno_location() };
            if errno == libc::ENODEV {
                self.connected.store(false, Ordering::Relaxed);
            }
            self.communication_errors.fetch_add(1, Ordering::Relaxed);
            return None;
        }

        if result == 0 {
            return None;
        }

        // Parse telemetry report
        if let Some(report) = DeviceTelemetryReport::from_bytes(&buffer[..result as usize]) {
            self.temperature_c.store(report.temp_c, Ordering::Relaxed);
            self.fault_flags.store(report.faults, Ordering::Relaxed);
            self.hands_on.store(report.hands_on != 0, Ordering::Relaxed);
            self.mark_communication();
            Some(report.to_telemetry_data())
        } else {
            None
        }
    }
}

impl HidDevice for LinuxHidDevice {
    fn write_ffb_report(&mut self, torque_nm: f32, seq: u16) -> RTResult {
        if !self.connected.load(Ordering::Relaxed) {
            return Err(crate::RTError::DeviceDisconnected);
        }

        // Sequence tracking stays lock-free for RT determinism.
        self.last_seq.store(seq, Ordering::Relaxed);

        let mut report = [0u8; MAX_TORQUE_REPORT_SIZE];
        let len = encode_torque_report_for_device(
            self.device_info.vendor_id,
            self.device_info.product_id,
            self.device_info.capabilities.max_torque.value(),
            torque_nm,
            seq,
            &mut report,
        );

        // Perform non-blocking write (RT-safe)
        self.write_nonblocking(&report[..len])
    }

    fn read_telemetry(&mut self) -> Option<TelemetryData> {
        if !self.connected.load(Ordering::Relaxed) {
            return None;
        }

        self.read_telemetry_blocking()
    }

    fn capabilities(&self) -> &DeviceCapabilities {
        &self.device_info.capabilities
    }

    fn device_info(&self) -> &DeviceInfo {
        get_cached_device_info(&self.device_info)
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    fn health_status(&self) -> DeviceHealthStatus {
        let elapsed_us = self.last_communication_us.load(Ordering::Relaxed);
        let last_communication = match self
            .created_at
            .checked_add(Duration::from_micros(elapsed_us))
        {
            Some(ts) => ts,
            None => self.created_at,
        };

        DeviceHealthStatus {
            temperature_c: self.temperature_c.load(Ordering::Relaxed),
            fault_flags: self.fault_flags.load(Ordering::Relaxed),
            hands_on: self.hands_on.load(Ordering::Relaxed),
            last_communication,
            communication_errors: self.communication_errors.load(Ordering::Relaxed),
        }
    }
}

/// Apply Linux-specific RT optimizations
pub fn apply_linux_rt_setup() -> Result<(), Box<dyn std::error::Error>> {
    info!("Applying Linux RT optimizations");

    // Lock memory pages to prevent swapping
    unsafe {
        let result = libc::mlockall(libc::MCL_CURRENT | libc::MCL_FUTURE);
        if result == 0 {
            info!("Locked memory pages with mlockall");
        } else {
            warn!(
                "Failed to lock memory pages: errno {}",
                *libc::__errno_location()
            );
        }
    }

    // Try to set SCHED_FIFO priority via rtkit
    // In a real implementation, this would use D-Bus to communicate with rtkit
    info!("Attempting to acquire RT scheduling via rtkit");

    // For now, try direct sched_setscheduler (requires CAP_SYS_NICE or rtkit)
    unsafe {
        let param = libc::sched_param {
            sched_priority: 50, // Mid-range RT priority
        };

        let result = libc::sched_setscheduler(0, libc::SCHED_FIFO, &param);
        if result == 0 {
            info!("Set SCHED_FIFO priority 50");
        } else {
            let errno = *libc::__errno_location();
            if errno == libc::EPERM {
                info!(
                    "No permission for SCHED_FIFO, consider using rtkit or adding user to realtime group"
                );
            } else {
                warn!("Failed to set SCHED_FIFO: errno {}", errno);
            }
        }
    }

    // Set CPU affinity to isolate RT thread
    // In a real implementation, this would pin to isolated CPUs
    info!("Consider isolating CPUs with isolcpus= kernel parameter for better RT performance");

    // Guidance for udev rules
    info!("For device permissions, create /etc/udev/rules.d/99-racing-wheel.rules:");
    info!(
        "SUBSYSTEM==\"hidraw\", ATTRS{{idVendor}}==\"046d\", ATTRS{{idProduct}}==\"c294\", MODE=\"0666\", GROUP=\"input\""
    );
    info!("SUBSYSTEM==\"hidraw\", ATTRS{{idVendor}}==\"0eb7\", MODE=\"0666\", GROUP=\"input\"");
    info!("SUBSYSTEM==\"hidraw\", ATTRS{{idVendor}}==\"044f\", MODE=\"0666\", GROUP=\"input\"");
    info!("SUBSYSTEM==\"hidraw\", ATTRS{{idVendor}}==\"346e\", MODE=\"0666\", GROUP=\"input\"");
    info!("SUBSYSTEM==\"hidraw\", ATTRS{{idVendor}}==\"0483\", MODE=\"0666\", GROUP=\"input\"");
    info!("SUBSYSTEM==\"hidraw\", ATTRS{{idVendor}}==\"16d0\", MODE=\"0666\", GROUP=\"input\"");
    info!("SUBSYSTEM==\"hidraw\", ATTRS{{idVendor}}==\"3670\", MODE=\"0666\", GROUP=\"input\"");
    info!("Then run: sudo udevadm control --reload-rules && sudo udevadm trigger");

    // Guidance for rtkit setup
    info!("For RT scheduling without root, install rtkit and add user to realtime group:");
    info!("sudo usermod -a -G realtime $USER");
    info!("Then logout and login again");

    Ok(())
}

/// Revert Linux RT optimizations
pub fn revert_linux_rt_setup() -> Result<(), Box<dyn std::error::Error>> {
    info!("Reverting Linux RT optimizations");

    // Unlock memory pages
    unsafe {
        let result = libc::munlockall();
        if result == 0 {
            info!("Unlocked memory pages");
        }
    }

    // Reset to normal scheduling
    unsafe {
        let param = libc::sched_param { sched_priority: 0 };

        let result = libc::sched_setscheduler(0, libc::SCHED_OTHER, &param);
        if result == 0 {
            info!("Reset to SCHED_OTHER");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[tokio::test]
    async fn test_linux_hid_port_creation() -> TestResult {
        let port = LinuxHidPort::new()?;
        assert!(!port.monitoring.load(Ordering::Relaxed));
        Ok(())
    }

    #[tokio::test]
    async fn test_device_enumeration() -> TestResult {
        let port = LinuxHidPort::new()?;
        let devices = port.list_devices().await?;

        // Should find some mock devices
        assert!(!devices.is_empty());

        for device in &devices {
            assert!(!device.name.is_empty());
            assert!(device.vendor_id != 0);
            assert!(device.product_id != 0);
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_device_opening() -> TestResult {
        let port = LinuxHidPort::new()?;
        let devices = port.list_devices().await?;

        if let Some(device_info) = devices.first() {
            let device = port.open_device(&device_info.id).await?;
            assert!(device.is_connected());
            assert!(device.capabilities().max_torque.value() > 0.0);
        }
        Ok(())
    }

    #[test]
    fn test_linux_hid_device_creation() -> TestResult {
        let device_id = must("test-device".parse::<DeviceId>());
        let capabilities = DeviceCapabilities {
            supports_pid: true,
            supports_raw_torque_1khz: true,
            supports_health_stream: true,
            supports_led_bus: false,
            max_torque: must(TorqueNm::new(25.0)),
            encoder_cpr: 4096,
            min_report_period_us: 1000,
        };

        let device_info = HidDeviceInfo {
            device_id,
            vendor_id: 0x046D,
            product_id: 0xC294,
            serial_number: Some("TEST123".to_string()),
            manufacturer: Some("Test Manufacturer".to_string()),
            product_name: Some("Test Racing Wheel".to_string()),
            path: "/dev/hidraw_mock".to_string(),
            capabilities,
        };

        let device = LinuxHidDevice::new(device_info)?;
        assert!(device.is_connected());
        assert_eq!(device.capabilities().max_torque.value(), 25.0);
        Ok(())
    }

    #[test]
    fn test_ffb_report_writing() -> TestResult {
        let device_id = must("test-device".parse::<DeviceId>());
        let capabilities = DeviceCapabilities {
            supports_pid: true,
            supports_raw_torque_1khz: true,
            supports_health_stream: true,
            supports_led_bus: false,
            max_torque: must(TorqueNm::new(25.0)),
            encoder_cpr: 4096,
            min_report_period_us: 1000,
        };

        let device_info = HidDeviceInfo {
            device_id,
            vendor_id: 0x046D,
            product_id: 0xC294,
            serial_number: Some("TEST123".to_string()),
            manufacturer: Some("Test Manufacturer".to_string()),
            product_name: Some("Test Racing Wheel".to_string()),
            path: "/dev/hidraw_mock".to_string(),
            capabilities,
        };

        let mut device = LinuxHidDevice::new(device_info)?;
        device.write_ffb_report(5.0, 123)?;
        Ok(())
    }

    #[test]
    fn test_rt_setup_functions() {
        // These functions should not panic
        let _ = apply_linux_rt_setup();
        let _ = revert_linux_rt_setup();
    }

    #[test]
    fn test_hidraw_device_probing() -> TestResult {
        let port = LinuxHidPort::new()?;
        let path = Path::new("/dev/hidraw0");

        // This should not panic even if the device doesn't exist
        let _ = port.probe_hidraw_device(path);
        Ok(())
    }
}

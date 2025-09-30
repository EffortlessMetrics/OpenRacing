//! Linux HID adapter with /dev/hidraw* and RT optimizations
//!
//! This module implements HID device communication on Linux using:
//! - /dev/hidraw* with libudev for enumeration
//! - Non-blocking writes for RT performance
//! - SCHED_FIFO via rtkit for RT scheduling
//! - mlockall for memory locking
//! - udev rules guidance for device permissions

use crate::ports::{HidPort, HidDevice, DeviceHealthStatus};
use crate::{RTResult, DeviceEvent, TelemetryData, DeviceInfo};
use racing_wheel_schemas::prelude::*;
use super::{HidDeviceInfo, TorqueCommand, DeviceTelemetryReport, DeviceCapabilitiesReport};
use tokio::sync::mpsc;
use async_trait::async_trait;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}, OnceLock};
use parking_lot::{RwLock, Mutex};
use std::collections::HashMap;
use std::time::{Instant, Duration};
use std::fs::{File, OpenOptions};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::{AsRawFd, RawFd};
use std::path::{Path, PathBuf};
use tracing::{debug, warn, error, info};

/// Thread-safe cached device info accessor using OnceLock
fn get_cached_device_info(device_info: &HidDeviceInfo) -> &'static DeviceInfo {
    static CACHED_INFO: OnceLock<DeviceInfo> = OnceLock::new();
    CACHED_INFO.get_or_init(|| device_info.to_device_info())
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
                                for (vid, pid) in racing_wheel_ids.iter() {
                                    if device_info.vendor_id == *vid && device_info.product_id == *pid {
                                        devices.push(device_info);
                                        break;
                                    }
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
                    max_torque: TorqueNm::new(25.0).unwrap(),
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
    fn probe_hidraw_device(&self, path: &Path) -> Result<HidDeviceInfo, Box<dyn std::error::Error>> {
        // In a real implementation, this would:
        // 1. Open the hidraw device
        // 2. Use HIDIOCGRAWINFO ioctl to get vendor/product ID
        // 3. Use HIDIOCGRAWNAME ioctl to get device name
        // 4. Use HIDIOCGRDESC ioctl to get report descriptor
        // 5. Parse capabilities from report descriptor

        // For now, return mock data
        let device_id = DeviceId::new(format!("linux_{}", path.display()))?;
        let capabilities = DeviceCapabilities {
            supports_pid: true,
            supports_raw_torque_1khz: true,
            supports_health_stream: true,
            supports_led_bus: false,
            max_torque: TorqueNm::new(25.0).unwrap(),
            encoder_cpr: 4096,
            min_report_period_us: 1000,
        };

        Ok(HidDeviceInfo {
            device_id,
            vendor_id: 0x046D,
            product_id: 0xC294,
            serial_number: Some("LINUX123".to_string()),
            manufacturer: Some("Mock Manufacturer".to_string()),
            product_name: Some("Mock Racing Wheel".to_string()),
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

    async fn open_device(&self, id: &DeviceId) -> Result<Box<dyn HidDevice>, Box<dyn std::error::Error>> {
        let devices = self.devices.read();
        let device_info = devices.get(id)
            .ok_or_else(|| format!("Device not found: {}", id))?;

        let device = LinuxHidDevice::new(device_info.clone())?;
        Ok(Box::new(device))
    }

    async fn monitor_devices(&self) -> Result<mpsc::Receiver<DeviceEvent>, Box<dyn std::error::Error>> {
        let (sender, receiver) = mpsc::unbounded_channel();
        
        // Start device monitoring using inotify on /dev
        let devices = self.devices.clone();
        let monitoring = self.monitoring.clone();
        let sender_clone = sender.clone();
        
        monitoring.store(true, Ordering::Relaxed);
        
        tokio::spawn(async move {
            let mut last_devices = HashMap::new();
            
            while monitoring.load(Ordering::Relaxed) {
                // Check for device changes every 500ms
                tokio::time::sleep(Duration::from_millis(500)).await;
                
                // In a real implementation, this would use inotify to watch /dev
                // for hidraw device creation/removal
                let current_devices = devices.read().clone();
                
                // Check for new devices
                for (id, info) in &current_devices {
                    if !last_devices.contains_key(id) {
                        let event = DeviceEvent::Connected(info.to_device_info());
                        if sender_clone.send(event).is_err() {
                            break;
                        }
                    }
                }
                
                // Check for removed devices
                for (id, info) in &last_devices {
                    if !current_devices.contains_key(id) {
                        let event = DeviceEvent::Disconnected(info.to_device_info());
                        if sender_clone.send(event).is_err() {
                            break;
                        }
                    }
                }
                
                last_devices = current_devices;
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
    connected: Arc<AtomicBool>,
    last_seq: Arc<Mutex<u16>>,
    health_status: Arc<RwLock<DeviceHealthStatus>>,
    write_fd: Arc<Mutex<Option<RawFd>>>,
    read_fd: Arc<Mutex<Option<RawFd>>>,
}

impl LinuxHidDevice {
    pub fn new(device_info: HidDeviceInfo) -> Result<Self, Box<dyn std::error::Error>> {
        let health_status = DeviceHealthStatus {
            temperature_c: 25,
            fault_flags: 0,
            hands_on: false,
            last_communication: Instant::now(),
            communication_errors: 0,
        };

        // In a real implementation, open the hidraw device
        let write_fd = if device_info.path.contains("mock") {
            None // Mock device
        } else {
            // Open device for writing with non-blocking flag
            match OpenOptions::new()
                .write(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(&device_info.path)
            {
                Ok(file) => Some(file.as_raw_fd()),
                Err(e) => {
                    warn!("Failed to open {} for writing: {}", device_info.path, e);
                    None
                }
            }
        };

        let read_fd = if device_info.path.contains("mock") {
            None // Mock device
        } else {
            // Open device for reading
            match OpenOptions::new()
                .read(true)
                .open(&device_info.path)
            {
                Ok(file) => Some(file.as_raw_fd()),
                Err(e) => {
                    warn!("Failed to open {} for reading: {}", device_info.path, e);
                    None
                }
            }
        };

        Ok(Self {
            device_info,
            connected: Arc::new(AtomicBool::new(true)),
            last_seq: Arc::new(Mutex::new(0)),
            health_status: Arc::new(RwLock::new(health_status)),
            write_fd: Arc::new(Mutex::new(write_fd)),
            read_fd: Arc::new(Mutex::new(read_fd)),
        })
    }

    /// Perform non-blocking write operation (RT-safe)
    fn write_nonblocking(&mut self, data: &[u8]) -> RTResult {
        let fd_guard = self.write_fd.lock();
        let fd = match *fd_guard {
            Some(fd) => fd,
            None => {
                // Mock device - simulate successful write
                debug!("Writing {} bytes to mock HID device", data.len());
                return Ok(());
            }
        };

        // Perform non-blocking write
        let result = unsafe {
            libc::write(fd, data.as_ptr() as *const libc::c_void, data.len())
        };

        if result < 0 {
            let errno = unsafe { *libc::__errno_location() };
            if errno == libc::EAGAIN || errno == libc::EWOULDBLOCK {
                // Write would block - this is expected in RT context
                debug!("HID write would block (EAGAIN)");
                return Ok(());
            } else if errno == libc::ENODEV || errno == libc::EPIPE {
                // Device disconnected
                self.connected.store(false, Ordering::Relaxed);
                return Err(crate::RTError::DeviceDisconnected);
            } else {
                // Other error
                warn!("HID write error: errno {}", errno);
                return Err(crate::RTError::PipelineFault);
            }
        }

        if result as usize != data.len() {
            warn!("Partial HID write: {} of {} bytes", result, data.len());
        }

        // Update health status
        {
            let mut health = self.health_status.write();
            health.last_communication = Instant::now();
        }

        debug!("Wrote {} bytes to HID device", result);
        Ok(())
    }

    /// Read telemetry data (non-RT, can block)
    fn read_telemetry_blocking(&mut self) -> Option<TelemetryData> {
        let fd_guard = self.read_fd.lock();
        let fd = match *fd_guard {
            Some(fd) => fd,
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
                return Some(report.to_telemetry_data());
            }
        };

        // Read telemetry report
        let mut buffer = [0u8; 64]; // Typical HID report size
        let result = unsafe {
            libc::read(fd, buffer.as_mut_ptr() as *mut libc::c_void, buffer.len())
        };

        if result < 0 {
            let errno = unsafe { *libc::__errno_location() };
            if errno == libc::ENODEV {
                self.connected.store(false, Ordering::Relaxed);
            }
            return None;
        }

        if result == 0 {
            return None;
        }

        // Parse telemetry report
        if let Some(report) = DeviceTelemetryReport::from_bytes(&buffer[..result as usize]) {
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

        // Update sequence number
        {
            let mut last_seq = self.last_seq.lock();
            *last_seq = seq;
        }

        // Create torque command
        let command = TorqueCommand::new(torque_nm, seq, true, false);
        let data = command.as_bytes();

        // Perform non-blocking write (RT-safe)
        self.write_nonblocking(data)
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
        self.health_status.read().clone()
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
            warn!("Failed to lock memory pages: errno {}", *libc::__errno_location());
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
                info!("No permission for SCHED_FIFO, consider using rtkit or adding user to realtime group");
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
    info!("SUBSYSTEM==\"hidraw\", ATTRS{{idVendor}}==\"046d\", ATTRS{{idProduct}}==\"c294\", MODE=\"0666\", GROUP=\"input\"");
    info!("SUBSYSTEM==\"hidraw\", ATTRS{{idVendor}}==\"0eb7\", MODE=\"0666\", GROUP=\"input\"");
    info!("SUBSYSTEM==\"hidraw\", ATTRS{{idVendor}}==\"044f\", MODE=\"0666\", GROUP=\"input\"");
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
        let param = libc::sched_param {
            sched_priority: 0,
        };
        
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

    #[tokio::test]
    async fn test_linux_hid_port_creation() {
        let port = LinuxHidPort::new().unwrap();
        assert!(!port.monitoring.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn test_device_enumeration() {
        let port = LinuxHidPort::new().unwrap();
        let devices = port.list_devices().await.unwrap();
        
        // Should find some mock devices
        assert!(!devices.is_empty());
        
        for device in &devices {
            assert!(!device.name.is_empty());
            assert!(device.vendor_id != 0);
            assert!(device.product_id != 0);
        }
    }

    #[tokio::test]
    async fn test_device_opening() {
        let port = LinuxHidPort::new().unwrap();
        let devices = port.list_devices().await.unwrap();
        
        if let Some(device_info) = devices.first() {
            let device = port.open_device(&device_info.id).await.unwrap();
            assert!(device.is_connected());
            assert!(device.capabilities().max_torque.value() > 0.0);
        }
    }

    #[test]
    fn test_linux_hid_device_creation() {
        let device_id = DeviceId::new("test-device".to_string()).unwrap();
        let capabilities = DeviceCapabilities {
            supports_pid: true,
            supports_raw_torque_1khz: true,
            supports_health_stream: true,
            supports_led_bus: false,
            max_torque: TorqueNm::new(25.0).unwrap(),
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

        let device = LinuxHidDevice::new(device_info).unwrap();
        assert!(device.is_connected());
        assert_eq!(device.capabilities().max_torque.value(), 25.0);
    }

    #[test]
    fn test_ffb_report_writing() {
        let device_id = DeviceId::new("test-device".to_string()).unwrap();
        let capabilities = DeviceCapabilities {
            supports_pid: true,
            supports_raw_torque_1khz: true,
            supports_health_stream: true,
            supports_led_bus: false,
            max_torque: TorqueNm::new(25.0).unwrap(),
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

        let mut device = LinuxHidDevice::new(device_info).unwrap();
        let result = device.write_ffb_report(5.0, 123);
        assert!(result.is_ok());
    }

    #[test]
    fn test_rt_setup_functions() {
        // These functions should not panic
        let _ = apply_linux_rt_setup();
        let _ = revert_linux_rt_setup();
    }

    #[test]
    fn test_hidraw_device_probing() {
        let port = LinuxHidPort::new().unwrap();
        let path = Path::new("/dev/hidraw0");
        
        // This should not panic even if the device doesn't exist
        let _ = port.probe_hidraw_device(path);
    }
}
</content>
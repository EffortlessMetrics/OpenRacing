//! Windows HID adapter with overlapped I/O and RT optimizations
//!
//! This module implements HID device communication on Windows using:
//! - hidapi with overlapped I/O for non-blocking writes
//! - MMCSS "Games" category for RT thread priority
//! - Process power throttling disabled
//! - Guidance for USB selective suspend

use crate::ports::{HidPort, HidDevice, DeviceHealthStatus};
use crate::{TelemetryData, DeviceInfo, DeviceEvent, RTResult};
use racing_wheel_schemas::{DeviceId, DeviceCapabilities, TorqueNm};
use super::{HidDeviceInfo, TorqueCommand, DeviceTelemetryReport};
use tokio::sync::mpsc;
use async_trait::async_trait;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}, OnceLock};
use parking_lot::{RwLock, Mutex};
use std::collections::HashMap;
use std::time::{Instant, Duration};
use tracing::{debug, warn, info};

use windows::{
    core::*,
    Win32::Foundation::*,
    Win32::System::Threading::*,
};

/// Thread-safe cached device info accessor using OnceLock
fn get_cached_device_info(device_info: &HidDeviceInfo) -> &'static DeviceInfo {
    static CACHED_INFO: OnceLock<DeviceInfo> = OnceLock::new();
    CACHED_INFO.get_or_init(|| device_info.to_device_info())
}

/// Windows-specific HID port implementation
pub struct WindowsHidPort {
    devices: Arc<RwLock<HashMap<DeviceId, HidDeviceInfo>>>,
    monitoring: Arc<AtomicBool>,
    /// TODO: Used for future device event notification implementation
    #[allow(dead_code)]
    event_sender: Option<mpsc::UnboundedSender<DeviceEvent>>,
}

impl WindowsHidPort {
    pub fn new() -> std::result::Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            devices: Arc::new(RwLock::new(HashMap::new())),
            monitoring: Arc::new(AtomicBool::new(false)),
            event_sender: None,
        })
    }

    /// Enumerate HID devices using Windows HID API
    fn enumerate_devices(&self) -> std::result::Result<Vec<HidDeviceInfo>, Box<dyn std::error::Error>> {
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

        // Use hidapi to enumerate devices
        // Note: In a real implementation, you would use the hidapi crate
        // For now, we'll simulate device discovery
        for (vid, pid) in racing_wheel_ids.iter() {
            let device_id = DeviceId::new(format!("HID_VID_{:04X}_PID_{:04X}", vid, pid))
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
            let path = format!("HID_VID_{:04X}_PID_{:04X}_device-path", vid, pid);
            
            // Create mock capabilities for demonstration
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

        debug!("Enumerated {} racing wheel devices", devices.len());
        Ok(devices)
    }
}

#[async_trait]
impl HidPort for WindowsHidPort {
    async fn list_devices(&self) -> std::result::Result<Vec<DeviceInfo>, Box<dyn std::error::Error>> {
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

    async fn open_device(&self, id: &DeviceId) -> std::result::Result<Box<dyn HidDevice>, Box<dyn std::error::Error>> {
        let devices = self.devices.read();
        let device_info = devices.get(id)
            .ok_or_else(|| Box::new(std::io::Error::new(
                std::io::ErrorKind::NotFound, 
                format!("Device not found: {}", id)
            )) as Box<dyn std::error::Error>)?;

        let device = WindowsHidDevice::new(device_info.clone())?;
        Ok(Box::new(device))
    }

    async fn monitor_devices(&self) -> std::result::Result<mpsc::Receiver<DeviceEvent>, Box<dyn std::error::Error>> {
        let (sender, receiver) = mpsc::channel(100);
        
        // Start device monitoring thread
        let devices = self.devices.clone();
        let monitoring = self.monitoring.clone();
        let sender_clone = sender.clone();
        
        monitoring.store(true, Ordering::Relaxed);
        
        tokio::spawn(async move {
            let mut last_devices: HashMap<DeviceId, HidDeviceInfo> = HashMap::new();
            
            while monitoring.load(Ordering::Relaxed) {
                // Check for device changes every 500ms
                tokio::time::sleep(Duration::from_millis(500)).await;
                
                // In a real implementation, this would use Windows device notification APIs
                // For now, we'll simulate by checking the device list
                let current_devices = devices.read().clone();
                
                // Check for new devices
                for (id, info) in &current_devices {
                    if !last_devices.contains_key(id) {
                        let event = DeviceEvent::Connected(info.to_device_info());
                        if sender_clone.send(event).await.is_err() {
                            break;
                        }
                    }
                }
                
                // Check for removed devices
                for (id, info) in &last_devices {
                    if !current_devices.contains_key(id) {
                        let event = DeviceEvent::Disconnected(info.to_device_info());
                        if sender_clone.send(event).await.is_err() {
                            break;
                        }
                    }
                }
                
                last_devices = current_devices;
            }
        });
        
        Ok(receiver)
    }

    async fn refresh_devices(&self) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let _ = self.list_devices().await;
        Ok(())
    }
}

/// Windows-specific HID device implementation with overlapped I/O
pub struct WindowsHidDevice {
    device_info: HidDeviceInfo,
    connected: Arc<AtomicBool>,
    last_seq: Arc<Mutex<u16>>,
    health_status: Arc<RwLock<DeviceHealthStatus>>,
    // In a real implementation, these would be Windows HANDLE types
    write_handle: Arc<Mutex<Option<usize>>>, // Simulated handle
    read_handle: Arc<Mutex<Option<usize>>>,  // Simulated handle
}

impl WindowsHidDevice {
    pub fn new(device_info: HidDeviceInfo) -> std::result::Result<Self, Box<dyn std::error::Error>> {
        let health_status = DeviceHealthStatus {
            temperature_c: 25,
            fault_flags: 0,
            hands_on: false,
            last_communication: Instant::now(),
            communication_errors: 0,
        };

        Ok(Self {
            device_info,
            connected: Arc::new(AtomicBool::new(true)),
            last_seq: Arc::new(Mutex::new(0)),
            health_status: Arc::new(RwLock::new(health_status)),
            write_handle: Arc::new(Mutex::new(Some(0x1234))), // Mock handle
            read_handle: Arc::new(Mutex::new(Some(0x5678))),  // Mock handle
        })
    }

    /// Perform overlapped write operation (RT-safe)
    fn write_overlapped(&mut self, data: &[u8]) -> RTResult {
        // In a real implementation, this would use WriteFile with OVERLAPPED
        // and avoid blocking the RT thread
        
        let handle = self.write_handle.lock();
        if handle.is_none() {
            return Err(crate::RTError::DeviceDisconnected);
        }

        // Simulate non-blocking write
        // Real implementation would:
        // 1. Use WriteFile with OVERLAPPED structure
        // 2. Check for immediate completion
        // 3. Return without waiting if operation is pending
        // 4. Use GetOverlappedResult in a separate thread to check completion

        debug!("Writing {} bytes to HID device (overlapped)", data.len());
        
        // Update health status
        {
            let mut health = self.health_status.write();
            health.last_communication = Instant::now();
        }

        Ok(())
    }

    /// Read telemetry data (non-RT, can block)
    fn read_telemetry_blocking(&mut self) -> Option<TelemetryData> {
        let handle = self.read_handle.lock();
        if handle.is_none() {
            return None;
        }

        // In a real implementation, this would use ReadFile
        // For now, simulate telemetry data
        let report = DeviceTelemetryReport {
            report_id: DeviceTelemetryReport::REPORT_ID,
            wheel_angle_mdeg: 0,
            wheel_speed_mrad_s: 0,
            temp_c: 30,
            faults: 0,
            hands_on: 1,
            reserved: [0; 2],
        };

        Some(report.to_telemetry_data())
    }
}

impl HidDevice for WindowsHidDevice {
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

        // Perform overlapped write (RT-safe)
        self.write_overlapped(data)
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

/// Apply Windows-specific RT optimizations
pub fn apply_windows_rt_setup() -> std::result::Result<(), Box<dyn std::error::Error>> {
    info!("Applying Windows RT optimizations");

    // Join MMCSS "Games" category for RT thread priority
    unsafe {
        let task_name = w!("Games");
        let mut task_index = 0u32;
        
        let handle = AvSetMmThreadCharacteristicsW(task_name, &mut task_index);
        if let Ok(handle) = handle {
            if handle.is_invalid() {
            warn!("Failed to join MMCSS Games category");
            } else {
                info!("Joined MMCSS Games category with task index {}", task_index);
            }
        } else {
            warn!("Failed to join MMCSS Games category");
        }
    }

    // Disable process power throttling
    unsafe {
        let process_handle = GetCurrentProcess();
        let power_throttling = PROCESS_POWER_THROTTLING_STATE {
            Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
            ControlMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
            StateMask: 0, // Disable throttling
        };

        let result = SetProcessInformation(
            process_handle,
            ProcessPowerThrottling,
            &power_throttling as *const _ as *const _,
            std::mem::size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
        );

        if result.is_ok() {
            info!("Disabled process power throttling");
        } else {
            warn!("Failed to disable process power throttling");
        }
    }

    // Set high priority class
    unsafe {
        let process_handle = GetCurrentProcess();
        let result = SetPriorityClass(process_handle, HIGH_PRIORITY_CLASS);
        if result.is_ok() {
            info!("Set process to high priority class");
        } else {
            warn!("Failed to set high priority class");
        }
    }

    // Guidance for USB selective suspend (logged for user)
    info!("For optimal performance, consider disabling USB selective suspend:");
    info!("1. Open Device Manager");
    info!("2. Expand 'Universal Serial Bus controllers'");
    info!("3. Right-click each 'USB Root Hub' and select Properties");
    info!("4. Go to Power Management tab");
    info!("5. Uncheck 'Allow the computer to turn off this device to save power'");

    Ok(())
}

/// Revert Windows RT optimizations
pub fn revert_windows_rt_setup() -> std::result::Result<(), Box<dyn std::error::Error>> {
    info!("Reverting Windows RT optimizations");

    // Leave MMCSS category
    unsafe {
        let result = AvRevertMmThreadCharacteristics(HANDLE::default());
        if result.is_ok() {
            info!("Left MMCSS category");
        }
    }

    // Re-enable process power throttling
    unsafe {
        let process_handle = GetCurrentProcess();
        let power_throttling = PROCESS_POWER_THROTTLING_STATE {
            Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
            ControlMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
            StateMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED, // Enable throttling
        };

        let result = SetProcessInformation(
            process_handle,
            ProcessPowerThrottling,
            &power_throttling as *const _ as *const _,
            std::mem::size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
        );

        if result.is_ok() {
            info!("Re-enabled process power throttling");
        }
    }

    // Reset to normal priority class
    unsafe {
        let process_handle = GetCurrentProcess();
        let result = SetPriorityClass(process_handle, NORMAL_PRIORITY_CLASS);
        if result.is_ok() {
            info!("Reset process to normal priority class");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_windows_hid_port_creation() {
        let port = WindowsHidPort::new().unwrap();
        assert!(!port.monitoring.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn test_device_enumeration() {
        let port = WindowsHidPort::new().unwrap();
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
        let port = WindowsHidPort::new().unwrap();
        let devices = port.list_devices().await.unwrap();
        
        if let Some(device_info) = devices.first() {
            let device = port.open_device(&device_info.id).await.unwrap();
            assert!(device.is_connected());
            assert!(device.capabilities().max_torque.value() > 0.0);
        }
    }

    #[test]
    fn test_windows_hid_device_creation() {
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
            path: "test-path".to_string(),
            capabilities,
        };

        let device = WindowsHidDevice::new(device_info).unwrap();
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
            path: "test-path".to_string(),
            capabilities,
        };

        let mut device = WindowsHidDevice::new(device_info).unwrap();
        let result = device.write_ffb_report(5.0, 123);
        assert!(result.is_ok());
    }

    #[test]
    fn test_rt_setup_functions() {
        // These functions should not panic
        let _ = apply_windows_rt_setup();
        let _ = revert_windows_rt_setup();
    }
}
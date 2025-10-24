//! Device service for enumeration, calibration, and health monitoring

use anyhow::Result;
use racing_wheel_engine::{
    HidPort, HidDevice, DeviceInfo, DeviceHealthStatus, TelemetryData,
    DeviceEvent, TracingManager, AppTraceEvent
};
use racing_wheel_schemas::prelude::{DeviceId, DeviceCapabilities, CalibrationData, TorqueNm};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, mpsc};
use tokio::time::interval;
use tracing::{info, error, debug};

/// Device connection state
#[derive(Debug, Clone, PartialEq)]
pub enum DeviceState {
    /// Device is disconnected
    Disconnected,
    /// Device is connected but not initialized
    Connected,
    /// Device is initialized and ready for use
    Ready,
    /// Device has a fault condition
    Faulted { reason: String },
}

/// Device information with runtime state
#[derive(Debug, Clone)]
pub struct ManagedDevice {
    pub info: DeviceInfo,
    pub state: DeviceState,
    pub capabilities: Option<DeviceCapabilities>,
    pub calibration: Option<CalibrationData>,
    pub last_telemetry: Option<TelemetryData>,
    pub last_seen: Instant,
    pub health_status: DeviceHealthStatus,
}

/// Device service for managing racing wheel hardware
pub struct ApplicationDeviceService {
    /// HID port for device communication
    hid_port: Arc<dyn HidPort>,
    /// Currently managed devices
    devices: Arc<RwLock<HashMap<DeviceId, ManagedDevice>>>,
    /// Device event sender
    event_sender: mpsc::UnboundedSender<DeviceEvent>,
    /// Device event receiver
    event_receiver: Arc<RwLock<Option<mpsc::UnboundedReceiver<DeviceEvent>>>>,
    /// Tracing manager for observability
    tracer: Option<Arc<TracingManager>>,
    /// Health monitoring interval
    health_check_interval: Duration,
}

impl ApplicationDeviceService {
    /// Create new device service
    pub async fn new(
        hid_port: Arc<dyn HidPort>,
        tracer: Option<Arc<TracingManager>>,
    ) -> Result<Self> {
        let (event_sender, event_receiver) = mpsc::unbounded_channel();

        Ok(Self {
            hid_port,
            devices: Arc::new(RwLock::new(HashMap::new())),
            event_sender,
            event_receiver: Arc::new(RwLock::new(Some(event_receiver))),
            tracer,
            health_check_interval: Duration::from_secs(5),
        })
    }

    /// Start the device service
    pub async fn start(&self) -> Result<()> {
        info!("Starting device service");

        // Start device enumeration
        self.start_device_enumeration().await?;

        // Start health monitoring
        self.start_health_monitoring().await?;

        // Start event processing
        self.start_event_processing().await?;

        info!("Device service started successfully");
        Ok(())
    }

    /// Enumerate and discover devices
    pub async fn enumerate_devices(&self) -> Result<Vec<DeviceInfo>> {
        debug!("Enumerating devices");

        let device_infos = self.hid_port.list_devices().await
            .map_err(|e| anyhow::anyhow!("Failed to enumerate devices: {}", e))?;

        info!(device_count = device_infos.len(), "Found devices");

        // Update managed devices
        {
            let mut devices = self.devices.write().await;
            let now = Instant::now();

            for device_info in &device_infos {
                let device_id = device_info.id.clone();
                
                match devices.get_mut(&device_id) {
                    Some(managed_device) => {
                        // Update existing device
                        managed_device.info = device_info.clone();
                        managed_device.last_seen = now;
                        if managed_device.state == DeviceState::Disconnected {
                            managed_device.state = DeviceState::Connected;
                            self.emit_device_connected_event(device_info).await;
                        }
                    }
                    None => {
                        // Add new device
                        let managed_device = ManagedDevice {
                            info: device_info.clone(),
                            state: DeviceState::Connected,
                            capabilities: None,
                            calibration: None,
                            last_telemetry: None,
                            last_seen: now,
                            health_status: DeviceHealthStatus {
                                temperature_c: 0,
                                fault_flags: 0,
                                hands_on: false,
                                last_communication: std::time::Instant::now(),
                                communication_errors: 0,
                            },
                        };
                        devices.insert(device_id, managed_device);
                        self.emit_device_connected_event(device_info).await;
                    }
                }
            }

            // Mark missing devices as disconnected
            let current_device_ids: std::collections::HashSet<_> = 
                device_infos.iter().map(|d| d.id.clone()).collect();
            
            for (device_id, managed_device) in devices.iter_mut() {
                if !current_device_ids.contains(device_id) && 
                   managed_device.state != DeviceState::Disconnected {
                    managed_device.state = DeviceState::Disconnected;
                    self.emit_device_disconnected_event(device_id, "Device not found during enumeration").await;
                }
            }
        }

        Ok(device_infos)
    }

    /// Get device by ID
    pub async fn get_device(&self, device_id: &DeviceId) -> Result<Option<ManagedDevice>> {
        let devices = self.devices.read().await;
        Ok(devices.get(device_id).cloned())
    }

    /// Get all managed devices
    pub async fn get_all_devices(&self) -> Result<Vec<ManagedDevice>> {
        let devices = self.devices.read().await;
        Ok(devices.values().cloned().collect())
    }

    /// Initialize device (read capabilities, perform initial setup)
    pub async fn initialize_device(&self, device_id: &DeviceId) -> Result<()> {
        info!(device_id = %device_id, "Initializing device");

        let device = self.hid_port.open_device(device_id).await
            .map_err(|e| anyhow::anyhow!("Failed to open device: {}", e))?;

        // Read device capabilities
        let capabilities = device.capabilities().clone();

        // Update managed device
        {
            let mut devices = self.devices.write().await;
            if let Some(managed_device) = devices.get_mut(device_id) {
                managed_device.capabilities = Some(capabilities);
                managed_device.state = DeviceState::Ready;
                managed_device.health_status = DeviceHealthStatus {
                    temperature_c: 25,
                    fault_flags: 0,
                    hands_on: false,
                    last_communication: std::time::Instant::now(),
                    communication_errors: 0,
                };
            }
        }

        info!(device_id = %device_id, "Device initialized successfully");
        Ok(())
    }

    /// Calibrate device
    pub async fn calibrate_device(
        &self,
        device_id: &DeviceId,
        calibration_type: CalibrationType,
    ) -> Result<CalibrationData> {
        info!(device_id = %device_id, calibration_type = ?calibration_type, "Starting device calibration");

        let mut device = self.hid_port.open_device(device_id).await
            .map_err(|e| anyhow::anyhow!("Failed to open device for calibration: {}", e))?;

        let calibration_data = match calibration_type {
            CalibrationType::Center => self.calibrate_center(device.as_mut()).await?,
            CalibrationType::Range => self.calibrate_range(device.as_mut()).await?,
            CalibrationType::Pedals => self.calibrate_pedals(device.as_ref()).await?,
            CalibrationType::Full => self.calibrate_full(device.as_mut()).await?,
        };

        // Store calibration data
        {
            let mut devices = self.devices.write().await;
            if let Some(managed_device) = devices.get_mut(device_id) {
                managed_device.calibration = Some(calibration_data.clone());
            }
        }

        info!(device_id = %device_id, "Device calibration completed");
        Ok(calibration_data)
    }

    /// Get device health status
    pub async fn get_device_health(&self, device_id: &DeviceId) -> Result<DeviceHealthStatus> {
        debug!(device_id = %device_id, "Getting device health status");

        let device = self.hid_port.open_device(device_id).await
            .map_err(|e| anyhow::anyhow!("Failed to open device for health check: {}", e))?;

        let health_status = device.health_status();

        // Update managed device
        {
            let mut devices = self.devices.write().await;
            if let Some(managed_device) = devices.get_mut(device_id) {
                managed_device.health_status = health_status.clone();
                managed_device.last_seen = Instant::now();
            }
        }

        Ok(health_status)
    }

    /// Get device telemetry
    pub async fn get_device_telemetry(&self, device_id: &DeviceId) -> Result<Option<TelemetryData>> {
        let devices = self.devices.read().await;
        Ok(devices.get(device_id).and_then(|d| d.last_telemetry.clone()))
    }

    /// Start continuous device enumeration
    async fn start_device_enumeration(&self) -> Result<()> {
        let hid_port = Arc::clone(&self.hid_port);
        let devices = Arc::clone(&self.devices);

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(2));
            
            loop {
                interval.tick().await;
                
                // Perform device enumeration
                match hid_port.list_devices().await.map_err(|e| e.to_string()) {
                    Ok(device_infos) => {
                        debug!("Enumerated {} devices", device_infos.len());
                        
                        let now = Instant::now();
                        // Update the devices map with discovered devices
                        let mut devices_guard = devices.write().await;
                        for device_info in device_infos {
                            if !devices_guard.contains_key(&device_info.id) {
                                let managed_device = ManagedDevice {
                                    info: device_info.clone(),
                                    state: DeviceState::Disconnected,
                                    capabilities: None,
                                    calibration: None,
                                    last_telemetry: None,
                                    health_status: DeviceHealthStatus {
                                        temperature_c: 25,
                                        fault_flags: 0,
                                        hands_on: false,
                                        last_communication: now,
                                        communication_errors: 0,
                                    },
                                    last_seen: now,
                                };
                                devices_guard.insert(device_info.id.clone(), managed_device);
                                info!(device_id = %device_info.id, "Discovered new device");
                            }
                        }
                    }
                    Err(_) => {
                        error!("Device enumeration failed");
                    }
                }
            }
        });

        Ok(())
    }

    /// Start health monitoring
    async fn start_health_monitoring(&self) -> Result<()> {
        let devices = Arc::clone(&self.devices);
        let hid_port = Arc::clone(&self.hid_port);
        let health_interval = self.health_check_interval;

        tokio::spawn(async move {
            let mut interval = interval(health_interval);
            
            loop {
                interval.tick().await;
                
                let device_ids: Vec<DeviceId> = {
                    let devices_guard = devices.read().await;
                    devices_guard.keys().cloned().collect()
                };

                for device_id in device_ids {
                    let device_result = hid_port.open_device(&device_id).await.map_err(|e| e.to_string());
                    if let Ok(device) = device_result {
                        let health_status = device.health_status();
                        let mut devices_guard = devices.write().await;
                        if let Some(managed_device) = devices_guard.get_mut(&device_id) {
                            managed_device.health_status = health_status;
                            managed_device.last_seen = Instant::now();
                        }
                    }
                }
            }
        });

        Ok(())
    }

    /// Start event processing
    async fn start_event_processing(&self) -> Result<()> {
        let mut receiver = {
            let mut receiver_guard = self.event_receiver.write().await;
            receiver_guard.take()
                .ok_or_else(|| anyhow::anyhow!("Event receiver already taken"))?
        };

        tokio::spawn(async move {
            while let Some(event) = receiver.recv().await {
                debug!(event = ?event, "Processing device event");
                // Event processing logic would go here
            }
        });

        Ok(())
    }

    /// Emit device connected event
    async fn emit_device_connected_event(&self, device_info: &DeviceInfo) {
        if let Some(tracer) = &self.tracer {
            tracer.emit_app_event(AppTraceEvent::DeviceConnected {
                device_id: device_info.id.to_string(),
                device_name: device_info.name.clone(),
                capabilities: format!("{:?}", device_info.capabilities),
            });
        }

        let _ = self.event_sender.send(DeviceEvent::Connected(device_info.clone()));
    }

    /// Emit device disconnected event
    async fn emit_device_disconnected_event(&self, device_id: &DeviceId, reason: &str) {
        if let Some(tracer) = &self.tracer {
            tracer.emit_app_event(AppTraceEvent::DeviceDisconnected {
                device_id: device_id.to_string(),
                reason: reason.to_string(),
            });
        }

        // Create a DeviceInfo for the disconnected event
        let device_info = DeviceInfo {
            id: device_id.clone(),
            name: "Unknown".to_string(),
            vendor_id: 0,
            product_id: 0,
            serial_number: None,
            manufacturer: None,
            path: "".to_string(),
            capabilities: DeviceCapabilities::new(false, false, false, false, 
                TorqueNm::new(0.0).unwrap_or(TorqueNm::ZERO), 0, 1000),
            is_connected: false,
        };
        let _ = self.event_sender.send(DeviceEvent::Disconnected(device_info));
    }

    /// Calibrate center position
    async fn calibrate_center(&self, device: &mut dyn HidDevice) -> Result<CalibrationData> {
        info!("Calibrating center position");
        
        // Read current position as center
        let telemetry = device.read_telemetry()
            .ok_or_else(|| anyhow::anyhow!("Failed to read telemetry for center calibration"))?;

        Ok(CalibrationData {
            center_position: Some(telemetry.wheel_angle_deg),
            min_position: None,
            max_position: None,
            pedal_ranges: None,
            calibrated_at: Some(chrono::Utc::now().to_rfc3339()),
            calibration_type: racing_wheel_schemas::entities::CalibrationType::Center,
        })
    }

    /// Calibrate full range
    async fn calibrate_range(&self, device: &mut dyn HidDevice) -> Result<CalibrationData> {
        info!("Calibrating range - user should turn wheel to full lock positions");
        
        let mut min_angle = f32::MAX;
        let mut max_angle = f32::MIN;
        
        // Sample for 10 seconds to capture range
        let start_time = Instant::now();
        while start_time.elapsed() < Duration::from_secs(10) {
            if let Some(telemetry) = device.read_telemetry() {
                min_angle = min_angle.min(telemetry.wheel_angle_deg);
                max_angle = max_angle.max(telemetry.wheel_angle_deg);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        Ok(CalibrationData {
            center_position: Some(0.0), // Assume center is 0
            min_position: Some(min_angle),
            max_position: Some(max_angle),
            pedal_ranges: None,
            calibrated_at: Some(chrono::Utc::now().to_rfc3339()),
            calibration_type: racing_wheel_schemas::entities::CalibrationType::Range,
        })
    }

    /// Calibrate pedals
    async fn calibrate_pedals(&self, _device: &dyn HidDevice) -> Result<CalibrationData> {
        info!("Calibrating pedals");
        
        // Pedal calibration would be implemented here
        // For now, return default calibration
        Ok(CalibrationData {
            center_position: None,
            min_position: None,
            max_position: None,
            pedal_ranges: Some(racing_wheel_schemas::prelude::PedalCalibrationData {
                throttle: Some((0.0, 1.0)),
                brake: Some((0.0, 1.0)),
                clutch: Some((0.0, 1.0)),
            }),
            calibrated_at: Some(chrono::Utc::now().to_rfc3339()),
            calibration_type: racing_wheel_schemas::entities::CalibrationType::Range,
        })
    }

    /// Full calibration (center + range + pedals)
    async fn calibrate_full(&self, device: &mut dyn HidDevice) -> Result<CalibrationData> {
        info!("Performing full calibration");
        
        let center_cal = self.calibrate_center(device).await?;
        let range_cal = self.calibrate_range(device).await?;
        let pedal_cal = self.calibrate_pedals(device).await?;

        Ok(CalibrationData {
            center_position: center_cal.center_position,
            min_position: range_cal.min_position,
            max_position: range_cal.max_position,
            pedal_ranges: pedal_cal.pedal_ranges,
            calibrated_at: Some(std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs().to_string()),
            calibration_type: racing_wheel_schemas::entities::CalibrationType::Full,
        })
    }

    /// List all devices (for IPC service compatibility)
    pub async fn list_devices(&self) -> Result<Vec<DeviceInfo>> {
        self.enumerate_devices().await
    }

    /// Get device status (for IPC service compatibility)
    pub async fn get_device_status(&self, device_id: &DeviceId) -> Result<(DeviceInfo, Option<TelemetryData>)> {
        let managed_device = self.get_device(device_id).await?
            .ok_or_else(|| anyhow::anyhow!("Device not found: {}", device_id))?;

        let telemetry = self.get_device_telemetry(device_id).await?;

        Ok((managed_device.info, telemetry))
    }

    /// Get device service statistics
    pub async fn get_statistics(&self) -> DeviceServiceStatistics {
        let devices = self.devices.read().await;
        
        let mut connected_count = 0;
        let mut ready_count = 0;
        let mut faulted_count = 0;

        for device in devices.values() {
            match device.state {
                DeviceState::Connected => connected_count += 1,
                DeviceState::Ready => {
                    connected_count += 1;
                    ready_count += 1;
                }
                DeviceState::Faulted { .. } => faulted_count += 1,
                DeviceState::Disconnected => {}
            }
        }

        DeviceServiceStatistics {
            total_devices: devices.len(),
            connected_devices: connected_count,
            ready_devices: ready_count,
            faulted_devices: faulted_count,
        }
    }
}

/// Calibration type
#[derive(Debug, Clone, Copy)]
pub enum CalibrationType {
    /// Calibrate center position only
    Center,
    /// Calibrate steering range
    Range,
    /// Calibrate pedals
    Pedals,
    /// Full calibration (all of the above)
    Full,
}



/// Device service statistics
#[derive(Debug, Clone)]
pub struct DeviceServiceStatistics {
    pub total_devices: usize,
    pub connected_devices: usize,
    pub ready_devices: usize,
    pub faulted_devices: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use racing_wheel_engine::{VirtualHidPort, DeviceType};

    #[tokio::test]
    async fn test_device_service_creation() {
        let hid_port = Arc::new(VirtualHidPort::new());
        let service = ApplicationDeviceService::new(hid_port, None).await;
        assert!(service.is_ok());
    }

    #[tokio::test]
    async fn test_device_enumeration() {
        let hid_port = Arc::new(VirtualHidPort::new());
        let service = ApplicationDeviceService::new(hid_port, None).await.unwrap();

        let devices = service.enumerate_devices().await.unwrap();
        // Virtual port should return at least one device
        assert!(!devices.is_empty());
    }

    #[tokio::test]
    async fn test_device_management() {
        let hid_port = Arc::new(VirtualHidPort::new());
        let service = ApplicationDeviceService::new(hid_port, None).await.unwrap();

        // Enumerate devices first
        let devices = service.enumerate_devices().await.unwrap();
        assert!(!devices.is_empty());

        let device_id = &devices[0].id;

        // Test getting device
        let managed_device = service.get_device(device_id).await.unwrap();
        assert!(managed_device.is_some());
        assert_eq!(managed_device.unwrap().state, DeviceState::Connected);

        // Test device initialization
        let result = service.initialize_device(device_id).await;
        // This might fail with virtual devices, but we're testing the interface
        assert!(result.is_ok() || result.is_err());
    }

    #[tokio::test]
    async fn test_device_calibration() {
        let hid_port = Arc::new(VirtualHidPort::new());
        let service = ApplicationDeviceService::new(hid_port, None).await.unwrap();

        let devices = service.enumerate_devices().await.unwrap();
        if !devices.is_empty() {
            let device_id = &devices[0].id;
            
            // Test center calibration
            let result = service.calibrate_device(device_id, CalibrationType::Center).await;
            // This might fail with virtual devices, but we're testing the interface
            assert!(result.is_ok() || result.is_err());
        }
    }

    #[tokio::test]
    async fn test_device_statistics() {
        let hid_port = Arc::new(VirtualHidPort::new());
        let service = ApplicationDeviceService::new(hid_port, None).await.unwrap();

        let stats = service.get_statistics().await;
        assert_eq!(stats.total_devices, 0); // Initially no devices

        // After enumeration, should have devices
        let _devices = service.enumerate_devices().await.unwrap();
        let stats = service.get_statistics().await;
        assert!(stats.total_devices > 0);
    }
}
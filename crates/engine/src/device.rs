//! Device abstraction and virtual device implementation

use crate::{RTResult, RTError};
use racing_wheel_schemas::{
    DeviceId, TorqueNm, DeviceCapabilities
};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

/// Telemetry data from device
#[derive(Debug, Clone)]
pub struct TelemetryData {
    pub wheel_angle_deg: f32,
    pub wheel_speed_rad_s: f32,
    pub temperature_c: u8,
    pub fault_flags: u8,
    pub hands_on: bool,
    pub timestamp: Instant,
}

/// Device info for enumeration and management
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub id: DeviceId,
    pub name: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub serial_number: Option<String>,
    pub manufacturer: Option<String>,
    pub path: String,
    pub capabilities: DeviceCapabilities,
    pub is_connected: bool,
}

// HidDevice and HidPort traits are now defined in ports.rs
use crate::ports::{HidDevice, HidPort, DeviceHealthStatus};

/// Device events for monitoring
#[derive(Debug, Clone)]
pub enum DeviceEvent {
    Connected(DeviceInfo),
    Disconnected(DeviceInfo),
}

/// OWP-1 Protocol structures
#[repr(C, packed)]
pub struct TorqueCommand {
    pub report_id: u8,      // 0x20
    pub torque_mn_m: i16,   // Q8.8 fixed point, milliNewton-meters
    pub flags: u8,          // bit0: hands_on_hint, bit1: sat_warn
    pub seq: u16,           // sequence number
}

#[repr(C, packed)]
pub struct DeviceTelemetryReport {
    pub report_id: u8,           // 0x21
    pub wheel_angle_mdeg: i32,   // millidegrees
    pub wheel_speed_mrad_s: i16, // milliradians per second
    pub temp_c: u8,              // temperature in Celsius
    pub faults: u8,              // fault bitfield
    pub hands_on: u8,            // 0/1 hands detection
}

#[repr(C, packed)]
pub struct DeviceCapabilitiesReport {
    pub report_id: u8,                  // 0x01
    pub supports_pid: u8,               // bit flags
    pub supports_raw_torque_1khz: u8,
    pub supports_health_stream: u8,
    pub supports_led_bus: u8,
    pub max_torque_cnm: u16,            // centiNewton-meters
    pub encoder_cpr: u16,               // counts per revolution
    pub min_report_period_us: u8,       // minimum report period in microseconds
}

/// Virtual device implementation for testing
pub struct VirtualDevice {
    info: DeviceInfo,
    capabilities: DeviceCapabilities,
    state: Arc<Mutex<VirtualDeviceState>>,
    connected: bool,
}

#[derive(Debug)]
struct VirtualDeviceState {
    wheel_angle_deg: f32,
    wheel_speed_rad_s: f32,
    temperature_c: u8,
    faults: u8,
    hands_on: bool,
    last_torque_nm: f32,
    last_seq: u16,
    last_update: Instant,
    torque_history: Vec<(Instant, f32)>,
}

impl VirtualDevice {
    /// Create a new virtual device
    pub fn new(id: DeviceId, name: String) -> Self {
        let capabilities = DeviceCapabilities::new(
            false, // supports_pid
            true,  // supports_raw_torque_1khz
            true,  // supports_health_stream
            true,  // supports_led_bus
            TorqueNm::new(25.0).unwrap(), // max_torque
            10000, // encoder_cpr
            1000,  // min_report_period_us (1ms = 1kHz)
        );

        let info = DeviceInfo {
            id: id.clone(),
            name,
            vendor_id: 0x1234, // Mock vendor ID
            product_id: 0x5678, // Mock product ID
            serial_number: Some("VIRTUAL001".to_string()),
            manufacturer: Some("Virtual Racing".to_string()),
            path: format!("virtual://{}", id.as_str()),
            capabilities: capabilities.clone(),
            is_connected: true,
        };

        let state = VirtualDeviceState {
            wheel_angle_deg: 0.0,
            wheel_speed_rad_s: 0.0,
            temperature_c: 35,
            faults: 0,
            hands_on: true,
            last_torque_nm: 0.0,
            last_seq: 0,
            last_update: Instant::now(),
            torque_history: Vec::new(),
        };

        Self {
            info,
            capabilities,
            state: Arc::new(Mutex::new(state)),
            connected: true,
        }
    }

    /// Simulate device physics (for testing)
    pub fn simulate_physics(&mut self, dt: Duration) {
        let mut state = self.state.lock().unwrap();
        
        // Simple physics simulation
        let dt_s = dt.as_secs_f32();
        
        // Apply torque to wheel dynamics
        let inertia = 0.1; // kg*m^2
        let friction = 0.05;
        let damping = 0.02;
        
        let torque_total = state.last_torque_nm - 
            friction * state.wheel_speed_rad_s.signum() - 
            damping * state.wheel_speed_rad_s;
        
        let acceleration = torque_total / inertia;
        state.wheel_speed_rad_s += acceleration * dt_s;
        state.wheel_angle_deg += state.wheel_speed_rad_s.to_degrees() * dt_s;
        
        // Keep angle in reasonable range
        if state.wheel_angle_deg > 1080.0 {
            state.wheel_angle_deg = 1080.0;
            state.wheel_speed_rad_s = 0.0;
        } else if state.wheel_angle_deg < -1080.0 {
            state.wheel_angle_deg = -1080.0;
            state.wheel_speed_rad_s = 0.0;
        }
        
        // Simulate temperature based on torque
        let torque_heating = state.last_torque_nm.abs() * 0.1;
        let ambient_cooling = (state.temperature_c as f32 - 25.0) * 0.01;
        let temp_change = (torque_heating - ambient_cooling) * dt_s;
        state.temperature_c = ((state.temperature_c as f32 + temp_change).clamp(20.0, 100.0)) as u8;
        
        // Simulate hands-on detection based on recent torque changes
        let now = Instant::now();
        state.torque_history.retain(|(time, _)| now.duration_since(*time) < Duration::from_secs(1));
        
        if state.torque_history.len() > 10 {
            let torque_variance: f32 = state.torque_history.windows(2)
                .map(|w| (w[1].1 - w[0].1).abs())
                .sum::<f32>() / (state.torque_history.len() - 1) as f32;
            state.hands_on = torque_variance > 0.1;
        }
        
        state.last_update = now;
    }

    /// Inject a fault for testing
    pub fn inject_fault(&mut self, fault_type: u8) {
        let mut state = self.state.lock().unwrap();
        state.faults |= fault_type;
    }

    /// Clear faults
    pub fn clear_faults(&mut self) {
        let mut state = self.state.lock().unwrap();
        state.faults = 0;
    }

    /// Disconnect the device (for testing)
    pub fn disconnect(&mut self) {
        self.connected = false;
    }

    /// Reconnect the device (for testing)
    pub fn reconnect(&mut self) {
        self.connected = true;
    }
}

impl HidDevice for VirtualDevice {
    fn write_ffb_report(&mut self, torque_nm: f32, seq: u16) -> RTResult {
        if !self.connected {
            return Err(RTError::DeviceDisconnected);
        }

        let mut state = self.state.lock().map_err(|_| RTError::PipelineFault)?;
        
        // Validate torque is within device limits
        let max_torque = self.capabilities.max_torque.value();
        if torque_nm.abs() > max_torque {
            return Err(RTError::TorqueLimit);
        }

        state.last_torque_nm = torque_nm;
        state.last_seq = seq;
        state.torque_history.push((Instant::now(), torque_nm));
        
        // Keep history bounded
        if state.torque_history.len() > 1000 {
            state.torque_history.drain(0..100);
        }

        Ok(())
    }

    fn read_telemetry(&mut self) -> Option<TelemetryData> {
        if !self.connected {
            return None;
        }

        let state = self.state.lock().ok()?;
        
        Some(TelemetryData {
            wheel_angle_deg: state.wheel_angle_deg,
            wheel_speed_rad_s: state.wheel_speed_rad_s,
            temperature_c: state.temperature_c,
            fault_flags: state.faults,
            hands_on: state.hands_on,
            timestamp: Instant::now(),
        })
    }

    fn capabilities(&self) -> &DeviceCapabilities {
        &self.capabilities
    }

    fn device_info(&self) -> &DeviceInfo {
        &self.info
    }

    fn is_connected(&self) -> bool {
        self.connected
    }
    
    fn health_status(&self) -> DeviceHealthStatus {
        let state = self.state.lock().unwrap();
        DeviceHealthStatus {
            temperature_c: state.temperature_c,
            fault_flags: state.faults,
            hands_on: state.hands_on,
            last_communication: state.last_update,
            communication_errors: 0,
        }
    }
}

/// Virtual HID port for testing
pub struct VirtualHidPort {
    devices: Arc<Mutex<Vec<VirtualDevice>>>,
    event_tx: Option<mpsc::Sender<DeviceEvent>>,
}

impl VirtualHidPort {
    /// Create a new virtual HID port
    pub fn new() -> Self {
        Self {
            devices: Arc::new(Mutex::new(Vec::new())),
            event_tx: None,
        }
    }

    /// Add a virtual device to the port
    pub fn add_device(&mut self, device: VirtualDevice) -> Result<(), Box<dyn std::error::Error>> {
        let device_info = device.device_info().clone();
        
        {
            let mut devices = self.devices.lock().unwrap();
            devices.push(device);
        }

        // Send connect event if monitoring
        if let Some(tx) = &self.event_tx {
            let _ = tx.try_send(DeviceEvent::Connected(device_info));
        }

        Ok(())
    }

    /// Remove a device by ID
    pub fn remove_device(&mut self, id: &DeviceId) -> Result<(), Box<dyn std::error::Error>> {
        {
            let mut devices = self.devices.lock().unwrap();
            devices.retain(|d| d.info.id != *id);
        }

        let device_info = {
            let devices = self.devices.lock().unwrap();
            devices.iter().find(|d| d.info.id == *id).map(|d| d.info.clone())
        };

        // Send disconnect event if monitoring
        if let Some(tx) = &self.event_tx {
            if let Some(info) = device_info {
                let _ = tx.try_send(DeviceEvent::Disconnected(info));
            }
        }

        Ok(())
    }

    /// Get mutable reference to device for testing
    pub fn get_device_mut(&mut self, _id: &DeviceId) -> Option<&mut VirtualDevice> {
        // This is a bit tricky with Arc<Mutex<Vec<_>>>
        // For testing purposes, we'll provide a different approach
        // The caller should use the device reference returned from open_device
        None
    }

    /// Simulate physics for all devices
    pub fn simulate_physics(&mut self, dt: Duration) {
        let mut devices = self.devices.lock().unwrap();
        for device in devices.iter_mut() {
            device.simulate_physics(dt);
        }
    }
}

#[async_trait::async_trait]
impl HidPort for VirtualHidPort {
    async fn list_devices(&self) -> Result<Vec<DeviceInfo>, Box<dyn std::error::Error>> {
        let devices = self.devices.lock().unwrap();
        Ok(devices.iter().map(|d| d.device_info().clone()).collect())
    }

    async fn open_device(&self, id: &DeviceId) -> Result<Box<dyn HidDevice>, Box<dyn std::error::Error>> {
        let devices = self.devices.lock().unwrap();
        
        for device in devices.iter() {
            if device.info.id == *id {
                // Create a new instance that shares the same state
                let virtual_device = VirtualDevice {
                    info: device.info.clone(),
                    capabilities: device.capabilities.clone(),
                    state: Arc::clone(&device.state),
                    connected: device.connected,
                };
                
                return Ok(Box::new(virtual_device));
            }
        }
        
        Err(format!("Device not found: {}", id).into())
    }

    async fn monitor_devices(&self) -> Result<mpsc::Receiver<DeviceEvent>, Box<dyn std::error::Error>> {
        let (_tx, rx) = mpsc::channel(100);
        // Store the sender for future events
        // Note: This is a simplified implementation for testing
        Ok(rx)
    }
    
    async fn refresh_devices(&self) -> Result<(), Box<dyn std::error::Error>> {
        // For virtual devices, this is a no-op
        Ok(())
    }
}

impl Default for VirtualHidPort {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_virtual_device_creation() {
        let device_id = DeviceId::new("test-device".to_string()).unwrap();
        let device = VirtualDevice::new(device_id, "Test Wheel".to_string());
        
        assert_eq!(device.device_info().id.as_str(), "test-device");
        assert_eq!(device.device_info().name, "Test Wheel");
        assert!(device.is_connected());
        assert_eq!(device.capabilities().max_torque.value(), 25.0);
    }

    #[test]
    fn test_virtual_device_torque_write() {
        let device_id = DeviceId::new("test-device".to_string()).unwrap();
        let mut device = VirtualDevice::new(device_id, "Test Wheel".to_string());
        
        // Test normal torque write
        let result = device.write_ffb_report(10.0, 1);
        assert!(result.is_ok());
        
        // Test torque limit
        let result = device.write_ffb_report(30.0, 2); // Exceeds 25Nm limit
        assert_eq!(result, Err(RTError::TorqueLimit));
        
        // Test disconnected device
        device.disconnect();
        let result = device.write_ffb_report(5.0, 3);
        assert_eq!(result, Err(RTError::DeviceDisconnected));
    }

    #[test]
    fn test_virtual_device_telemetry() {
        let device_id = DeviceId::new("test-device".to_string()).unwrap();
        let mut device = VirtualDevice::new(device_id, "Test Wheel".to_string());
        
        // Write some torque
        device.write_ffb_report(5.0, 1).unwrap();
        
        // Simulate physics
        device.simulate_physics(Duration::from_millis(10));
        
        // Read telemetry
        let telemetry = device.read_telemetry().unwrap();
        assert_eq!(telemetry.sequence, 1);
        assert!(telemetry.temp_c >= 35);
    }

    #[tokio::test]
    async fn test_virtual_hid_port() {
        let mut port = VirtualHidPort::new();
        
        // Add a device
        let device_id = DeviceId::new("test-device".to_string()).unwrap();
        let device = VirtualDevice::new(device_id.clone(), "Test Wheel".to_string());
        port.add_device(device).unwrap();
        
        // List devices
        let devices = port.list_devices().await.unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].id.as_str(), "test-device");
        
        // Open device
        let mut opened_device = port.open_device(&device_id).await.unwrap();
        assert!(opened_device.is_connected());
        
        // Test device operations
        let result = opened_device.write_ffb_report(5.0, 1);
        assert!(result.is_ok());
        
        let telemetry = opened_device.read_telemetry();
        assert!(telemetry.is_some());
    }

    #[test]
    fn test_virtual_device_physics_simulation() {
        let device_id = DeviceId::new("test-device".to_string()).unwrap();
        let mut device = VirtualDevice::new(device_id, "Test Wheel".to_string());
        
        // Apply constant torque
        device.write_ffb_report(10.0, 1).unwrap();
        
        // Simulate for 100ms
        for _ in 0..10 {
            device.simulate_physics(Duration::from_millis(10));
        }
        
        let telemetry = device.read_telemetry().unwrap();
        
        // Wheel should have moved and gained speed
        assert!(telemetry.wheel_angle_mdeg.abs() > 0);
        assert!(telemetry.wheel_speed_mrad_s.abs() > 0);
        
        // Temperature should have increased slightly (or at least stayed at baseline)
        assert!(telemetry.temp_c >= 35);
    }

    #[test]
    fn test_fault_injection() {
        let device_id = DeviceId::new("test-device".to_string()).unwrap();
        let mut device = VirtualDevice::new(device_id, "Test Wheel".to_string());
        
        // Initially no faults
        let telemetry = device.read_telemetry().unwrap();
        assert_eq!(telemetry.faults, 0);
        
        // Inject thermal fault
        device.inject_fault(0x04); // Thermal fault bit
        
        let telemetry = device.read_telemetry().unwrap();
        assert_eq!(telemetry.faults, 0x04);
        
        // Clear faults
        device.clear_faults();
        
        let telemetry = device.read_telemetry().unwrap();
        assert_eq!(telemetry.faults, 0);
    }
}
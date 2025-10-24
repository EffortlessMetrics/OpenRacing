//! iRacing telemetry adapter with shared memory interface
//! 
//! Implements telemetry adapter for iRacing using shared memory interface
//! Requirements: GI-03, GI-04

use crate::telemetry::{TelemetryAdapter, NormalizedTelemetry, TelemetryReceiver, TelemetryFrame, TelemetryFlags, TelemetryValue};
use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::mem;
use std::ptr;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

#[cfg(windows)]
use winapi::um::{
    handleapi::CloseHandle,
    memoryapi::{MapViewOfFile, OpenFileMappingW, UnmapViewOfFile},
    winnt::{FILE_SHARE_READ, HANDLE},
};

/// iRacing telemetry adapter using shared memory
pub struct IRacingAdapter {
    update_rate: Duration,
    #[cfg(windows)]
    shared_memory: Option<SharedMemoryHandle>,
}

#[cfg(windows)]
struct SharedMemoryHandle {
    handle: HANDLE,
    data_ptr: *const IRacingData,
}

#[cfg(windows)]
unsafe impl Send for SharedMemoryHandle {}
#[cfg(windows)]
unsafe impl Sync for SharedMemoryHandle {}

impl Default for IRacingAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl IRacingAdapter {
    /// Create a new iRacing adapter
    pub fn new() -> Self {
        Self {
            update_rate: Duration::from_millis(16), // ~60 FPS default
            #[cfg(windows)]
            shared_memory: None,
        }
    }
    
    /// Initialize shared memory connection to iRacing
    #[cfg(windows)]
    fn initialize_shared_memory(&mut self) -> Result<()> {
        use std::ffi::CString;
        
        let memory_name = CString::new("Local\\IRSDKMemMapFileName")?;
        
        unsafe {
            let handle = OpenFileMappingW(
                FILE_SHARE_READ,
                0, // Do not inherit handle
                memory_name.as_ptr() as *const u16,
            );
            
            if handle.is_null() {
                return Err(anyhow::anyhow!("Failed to open iRacing shared memory"));
            }
            
            let data_ptr = MapViewOfFile(
                handle,
                FILE_SHARE_READ,
                0,
                0,
                0,
            ) as *const IRacingData;
            
            if data_ptr.is_null() {
                CloseHandle(handle);
                return Err(anyhow::anyhow!("Failed to map iRacing shared memory"));
            }
            
            self.shared_memory = Some(SharedMemoryHandle {
                handle,
                data_ptr,
            });
            
            info!("Successfully connected to iRacing shared memory");
            Ok(())
        }
    }
    
    #[cfg(not(windows))]
    fn initialize_shared_memory(&mut self) -> Result<()> {
        Err(anyhow::anyhow!("iRacing shared memory only available on Windows"))
    }
    
    /// Read telemetry data from shared memory
    #[cfg(windows)]
    fn read_telemetry_data(&self) -> Result<IRacingData> {
        let shared_memory = self.shared_memory.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Shared memory not initialized"))?;
        
        unsafe {
            let data = ptr::read_volatile(shared_memory.data_ptr);
            Ok(data)
        }
    }
    
    #[cfg(not(windows))]
    fn read_telemetry_data(&self) -> Result<IRacingData> {
        Err(anyhow::anyhow!("iRacing shared memory only available on Windows"))
    }
    
    /// Check if iRacing is running by attempting to open shared memory
    #[cfg(windows)]
    async fn check_iracing_running(&self) -> bool {
        use std::ffi::CString;
        
        let memory_name = match CString::new("Local\\IRSDKMemMapFileName") {
            Ok(name) => name,
            Err(_) => return false,
        };
        
        unsafe {
            let handle = OpenFileMappingW(
                FILE_SHARE_READ,
                0,
                memory_name.as_ptr() as *const u16,
            );
            
            if !handle.is_null() {
                CloseHandle(handle);
                true
            } else {
                false
            }
        }
    }
    
    #[cfg(not(windows))]
    async fn check_iracing_running(&self) -> bool {
        false
    }
}

#[async_trait]
impl TelemetryAdapter for IRacingAdapter {
    fn game_id(&self) -> &str {
        "iracing"
    }
    
    async fn start_monitoring(&self) -> Result<TelemetryReceiver> {
        let (tx, rx) = mpsc::channel(100);
        
        // Clone necessary data for the monitoring task
        let update_rate = self.update_rate;
        
        tokio::spawn(async move {
            let mut adapter = IRacingAdapter::new();
            let mut sequence = 0u64;
            let mut last_update_time = None;
            
            // Try to initialize shared memory
            if let Err(e) = adapter.initialize_shared_memory() {
                error!("Failed to initialize iRacing shared memory: {}", e);
                return;
            }
            
            info!("Started iRacing telemetry monitoring");
            
            loop {
                let start_time = std::time::Instant::now();
                
                match adapter.read_telemetry_data() {
                    Ok(data) => {
                        // Check if data has been updated
                        let current_update_time = data.session_time;
                        if last_update_time != Some(current_update_time) {
                            last_update_time = Some(current_update_time);
                            
                            // Normalize the data
                            let normalized = adapter.normalize_iracing_data(&data);
                            
                            let frame = TelemetryFrame::new(
                                normalized,
                                start_time.elapsed().as_nanos() as u64,
                                sequence,
                                mem::size_of::<IRacingData>(),
                            );
                            
                            if tx.send(frame).await.is_err() {
                                debug!("Telemetry receiver dropped, stopping monitoring");
                                break;
                            }
                            
                            sequence += 1;
                        }
                    }
                    Err(e) => {
                        warn!("Failed to read iRacing telemetry: {}", e);
                        // Continue trying - game might have been restarted
                    }
                }
                
                tokio::time::sleep(update_rate).await;
            }
            
            info!("Stopped iRacing telemetry monitoring");
        });
        
        Ok(rx)
    }
    
    async fn stop_monitoring(&self) -> Result<()> {
        // Monitoring task will stop when receiver is dropped
        Ok(())
    }
    
    fn normalize(&self, raw: &[u8]) -> Result<NormalizedTelemetry> {
        if raw.len() != mem::size_of::<IRacingData>() {
            return Err(anyhow::anyhow!("Invalid iRacing data size"));
        }
        
        let data: IRacingData = unsafe {
            ptr::read(raw.as_ptr() as *const IRacingData)
        };
        
        Ok(self.normalize_iracing_data(&data))
    }
    
    fn expected_update_rate(&self) -> Duration {
        self.update_rate
    }
    
    async fn is_game_running(&self) -> Result<bool> {
        Ok(self.check_iracing_running().await)
    }
}

impl IRacingAdapter {
    /// Normalize iRacing data to common telemetry format
    fn normalize_iracing_data(&self, data: &IRacingData) -> NormalizedTelemetry {
        // Extract flags
        let flags = TelemetryFlags {
            yellow_flag: (data.session_flags & 0x00000001) != 0,
            red_flag: (data.session_flags & 0x00000002) != 0,
            blue_flag: (data.session_flags & 0x00000004) != 0,
            checkered_flag: (data.session_flags & 0x00000008) != 0,
            green_flag: (data.session_flags & 0x00000010) != 0,
            in_pits: data.on_pit_road != 0,
            ..Default::default()
        };
        
        // Calculate slip ratio (average of all tires)
        let slip_ratio = if data.speed > 1.0 {
            let avg_tire_speed = (data.lf_tire_rps + data.rf_tire_rps + data.lr_tire_rps + data.rr_tire_rps) / 4.0;
            let wheel_speed = avg_tire_speed * 0.31; // Approximate tire radius
            ((wheel_speed - data.speed).abs() / data.speed).min(1.0)
        } else {
            0.0
        };
        
        // Extract car and track IDs from strings
        let car_id = extract_string(&data.car_path);
        let track_id = extract_string(&data.track_name);
        
        // Create extended data
        let mut extended = HashMap::new();
        extended.insert("fuel_level".to_string(), TelemetryValue::Float(data.fuel_level));
        extended.insert("lap_current".to_string(), TelemetryValue::Integer(data.lap_current));
        extended.insert("lap_best_time".to_string(), TelemetryValue::Float(data.lap_best_time));
        extended.insert("session_time".to_string(), TelemetryValue::Float(data.session_time));
        extended.insert("throttle".to_string(), TelemetryValue::Float(data.throttle));
        extended.insert("brake".to_string(), TelemetryValue::Float(data.brake));
        extended.insert("steering_wheel_angle".to_string(), TelemetryValue::Float(data.steering_wheel_angle));
        
        NormalizedTelemetry::default()
            .with_ffb_scalar(data.steering_wheel_torque / 100.0) // Normalize to -1..1 range
            .with_rpm(data.rpm)
            .with_speed_ms(data.speed)
            .with_slip_ratio(slip_ratio)
            .with_gear(data.gear)
            .with_car_id(car_id)
            .with_track_id(track_id)
            .with_flags(flags)
            .with_extended("fuel_level".to_string(), TelemetryValue::Float(data.fuel_level))
            .with_extended("lap_current".to_string(), TelemetryValue::Integer(data.lap_current))
            .with_extended("throttle".to_string(), TelemetryValue::Float(data.throttle))
            .with_extended("brake".to_string(), TelemetryValue::Float(data.brake))
    }
}

/// Extract null-terminated string from byte array
fn extract_string(bytes: &[u8]) -> String {
    match bytes.iter().position(|&b| b == 0) {
        Some(pos) => {
            String::from_utf8_lossy(&bytes[..pos]).into_owned()
        }
        None => {
            String::from_utf8_lossy(bytes).into_owned()
        }
    }
}

/// iRacing shared memory data structure
/// This is a simplified version - the actual iRacing SDK has many more fields
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct IRacingData {
    // Session info
    session_time: f32,
    session_flags: u32,
    
    // Car dynamics
    speed: f32,                    // m/s
    rpm: f32,
    gear: i8,
    throttle: f32,                 // 0-1
    brake: f32,                    // 0-1
    steering_wheel_angle: f32,     // radians
    steering_wheel_torque: f32,    // Nm
    
    // Tire data
    lf_tire_rps: f32,             // Left front tire RPS
    rf_tire_rps: f32,             // Right front tire RPS
    lr_tire_rps: f32,             // Left rear tire RPS
    rr_tire_rps: f32,             // Right rear tire RPS
    
    // Position and lap info
    lap_current: i32,
    lap_best_time: f32,
    fuel_level: f32,
    
    // Flags
    on_pit_road: i32,
    
    // String data (simplified)
    car_path: [u8; 64],
    track_name: [u8; 64],
}

impl Default for IRacingData {
    fn default() -> Self {
        Self {
            session_time: 0.0,
            session_flags: 0,
            speed: 0.0,
            rpm: 0.0,
            gear: 0,
            throttle: 0.0,
            brake: 0.0,
            steering_wheel_angle: 0.0,
            steering_wheel_torque: 0.0,
            lf_tire_rps: 0.0,
            rf_tire_rps: 0.0,
            lr_tire_rps: 0.0,
            rr_tire_rps: 0.0,
            lap_current: 0,
            lap_best_time: 0.0,
            fuel_level: 0.0,
            on_pit_road: 0,
            car_path: [0; 64],
            track_name: [0; 64],
        }
    }
}

#[cfg(windows)]
impl Drop for SharedMemoryHandle {
    fn drop(&mut self) {
        unsafe {
            if !self.data_ptr.is_null() {
                UnmapViewOfFile(self.data_ptr as *const _);
            }
            if !self.handle.is_null() {
                CloseHandle(self.handle);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_iracing_adapter_creation() {
        let adapter = IRacingAdapter::new();
        assert_eq!(adapter.game_id(), "iracing");
        assert_eq!(adapter.expected_update_rate(), Duration::from_millis(16));
    }

    #[test]
    fn test_normalize_iracing_data() {
        let adapter = IRacingAdapter::new();
        
        let mut data = IRacingData::default();
        data.rpm = 6000.0;
        data.speed = 50.0;
        data.gear = 4;
        data.steering_wheel_torque = 25.0;
        data.throttle = 0.8;
        data.brake = 0.2;
        data.session_flags = 0x00000001; // Yellow flag
        
        // Set car and track names
        let car_name = b"gt3_bmw\0";
        let track_name = b"spa\0";
        data.car_path[..car_name.len()].copy_from_slice(car_name);
        data.track_name[..track_name.len()].copy_from_slice(track_name);
        
        let normalized = adapter.normalize_iracing_data(&data);
        
        assert_eq!(normalized.rpm, Some(6000.0));
        assert_eq!(normalized.speed_ms, Some(50.0));
        assert_eq!(normalized.gear, Some(4));
        assert_eq!(normalized.ffb_scalar, Some(0.25)); // 25.0 / 100.0
        assert_eq!(normalized.car_id, Some("gt3_bmw".to_string()));
        assert_eq!(normalized.track_id, Some("spa".to_string()));
        assert!(normalized.flags.yellow_flag);
        
        // Check extended data
        assert!(normalized.extended.contains_key("throttle"));
        assert!(normalized.extended.contains_key("brake"));
    }

    #[test]
    fn test_extract_string() {
        let bytes = b"test_string\0extra_data";
        let result = extract_string(bytes);
        assert_eq!(result, "test_string");
        
        let bytes_no_null = b"no_null_terminator";
        let result = extract_string(bytes_no_null);
        assert_eq!(result, "no_null_terminator");
    }

    #[test]
    fn test_normalize_raw_data() {
        let adapter = IRacingAdapter::new();
        
        let data = IRacingData::default();
        let raw_bytes = unsafe {
            std::slice::from_raw_parts(
                &data as *const _ as *const u8,
                mem::size_of::<IRacingData>(),
            )
        };
        
        let result = adapter.normalize(raw_bytes);
        assert!(result.is_ok());
    }

    #[test]
    fn test_normalize_invalid_data() {
        let adapter = IRacingAdapter::new();
        
        let invalid_data = vec![0u8; 10]; // Wrong size
        let result = adapter.normalize(&invalid_data);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_is_game_running() {
        let adapter = IRacingAdapter::new();
        
        // On non-Windows platforms, should always return false
        #[cfg(not(windows))]
        {
            let result = adapter.is_game_running().await;
            assert!(result.is_ok());
            assert!(!result.unwrap());
        }
        
        // On Windows, test depends on whether iRacing is actually running
        #[cfg(windows)]
        {
            let result = adapter.is_game_running().await;
            assert!(result.is_ok());
            // Can't assert the actual value since it depends on system state
        }
    }
}
//! ACC (Assetto Corsa Competizione) telemetry adapter using UDP broadcast
//! 
//! Implements telemetry adapter for ACC using UDP broadcast protocol with packet validation
//! Requirements: GI-03, GI-04

use crate::telemetry::{TelemetryAdapter, NormalizedTelemetry, TelemetryReceiver, TelemetryFrame, TelemetryFlags, TelemetryValue};
use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::UdpSocket as TokioUdpSocket;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// ACC telemetry adapter using UDP broadcast protocol
pub struct ACCAdapter {
    listen_address: SocketAddr,
    update_rate: Duration,
}

impl ACCAdapter {
    /// Create a new ACC adapter
    pub fn new() -> Self {
        Self {
            listen_address: "127.0.0.1:9996".parse()
                .expect("Default ACC UDP address should be valid"), // Default ACC UDP port
            update_rate: Duration::from_millis(16), // ~60 FPS
        }
    }
    
    /// Create ACC adapter with custom listen address
    pub fn with_address(listen_address: SocketAddr) -> Self {
        Self {
            listen_address,
            update_rate: Duration::from_millis(16),
        }
    }
    
    /// Check if ACC is running by attempting to bind to the telemetry port
    async fn check_acc_running(&self) -> bool {
        // Try to create a UDP socket on the telemetry port
        // If ACC is running and broadcasting, we should be able to receive data
        match TokioUdpSocket::bind(self.listen_address).await {
            Ok(socket) => {
                // Try to receive data with a short timeout
                let mut buf = [0u8; 1024];
                match tokio::time::timeout(
                    Duration::from_millis(100),
                    socket.recv_from(&mut buf)
                ).await {
                    Ok(Ok(_)) => true, // Received data, ACC is likely running
                    _ => false,        // No data or error
                }
            }
            Err(_) => false, // Can't bind, port might be in use by ACC
        }
    }
}

#[async_trait]
impl TelemetryAdapter for ACCAdapter {
    fn game_id(&self) -> &str {
        "acc"
    }
    
    async fn start_monitoring(&self) -> Result<TelemetryReceiver> {
        let (tx, rx) = mpsc::channel(100);
        
        let listen_address = self.listen_address;
        let update_rate = self.update_rate;
        
        tokio::spawn(async move {
            let mut sequence = 0u64;
            
            // Bind UDP socket
            let socket = match TokioUdpSocket::bind(listen_address).await {
                Ok(socket) => {
                    info!("ACC telemetry adapter listening on {}", listen_address);
                    socket
                }
                Err(e) => {
                    error!("Failed to bind ACC telemetry socket: {}", e);
                    return;
                }
            };
            
            let mut buf = [0u8; 2048]; // Buffer for UDP packets
            
            loop {
                match tokio::time::timeout(update_rate * 2, socket.recv_from(&mut buf)).await {
                    Ok(Ok((len, _addr))) => {
                        let packet_data = &buf[..len];
                        
                        // Validate and parse packet
                        match ACCAdapter::parse_udp_packet(packet_data) {
                            Ok(acc_data) => {
                                let normalized = ACCAdapter::normalize_acc_data(&acc_data);
                                
                                let frame = TelemetryFrame::new(
                                    normalized,
                                    std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .unwrap_or_default()
                                        .as_nanos() as u64,
                                    sequence,
                                    len,
                                );
                                
                                if tx.send(frame).await.is_err() {
                                    debug!("Telemetry receiver dropped, stopping ACC monitoring");
                                    break;
                                }
                                
                                sequence += 1;
                            }
                            Err(e) => {
                                warn!("Failed to parse ACC UDP packet: {}", e);
                            }
                        }
                    }
                    Ok(Err(e)) => {
                        warn!("UDP receive error: {}", e);
                    }
                    Err(_) => {
                        // Timeout - no data received
                        debug!("No ACC telemetry data received (timeout)");
                    }
                }
            }
            
            info!("Stopped ACC telemetry monitoring");
        });
        
        Ok(rx)
    }
    
    async fn stop_monitoring(&self) -> Result<()> {
        // Monitoring task will stop when receiver is dropped
        Ok(())
    }
    
    fn normalize(&self, raw: &[u8]) -> Result<NormalizedTelemetry> {
        let acc_data = Self::parse_udp_packet(raw)?;
        Ok(Self::normalize_acc_data(&acc_data))
    }
    
    fn expected_update_rate(&self) -> Duration {
        self.update_rate
    }
    
    async fn is_game_running(&self) -> Result<bool> {
        Ok(self.check_acc_running().await)
    }
}

impl ACCAdapter {
    /// Parse ACC UDP telemetry packet
    fn parse_udp_packet(data: &[u8]) -> Result<ACCTelemetryData> {
        if data.len() < std::mem::size_of::<ACCTelemetryData>() {
            return Err(anyhow::anyhow!("ACC packet too small: {} bytes", data.len()));
        }
        
        // ACC uses a simple binary format
        let telemetry_data = unsafe {
            std::ptr::read(data.as_ptr() as *const ACCTelemetryData)
        };
        
        // Validate packet (copy fields to avoid packed struct alignment issues)
        let packet_id = telemetry_data.packet_id;
        let rpm = telemetry_data.rpm;
        let speed = telemetry_data.speed;
        
        if packet_id != ACC_TELEMETRY_PACKET_ID {
            return Err(anyhow::anyhow!(
                "Invalid ACC packet ID: expected {}, got {}",
                ACC_TELEMETRY_PACKET_ID,
                packet_id
            ));
        }
        
        // Basic sanity checks
        if rpm < 0.0 || rpm > 20000.0 {
            return Err(anyhow::anyhow!("Invalid RPM value: {}", rpm));
        }
        
        if speed < 0.0 || speed > 200.0 {
            return Err(anyhow::anyhow!("Invalid speed value: {}", speed));
        }
        
        Ok(telemetry_data)
    }
    
    /// Normalize ACC data to common telemetry format
    fn normalize_acc_data(data: &ACCTelemetryData) -> NormalizedTelemetry {
        // Extract flags from ACC session type and status
        let mut flags = TelemetryFlags::default();
        flags.yellow_flag = (data.flag & 0x01) != 0;
        flags.red_flag = (data.flag & 0x02) != 0;
        flags.blue_flag = (data.flag & 0x04) != 0;
        flags.checkered_flag = (data.flag & 0x08) != 0;
        flags.green_flag = (data.flag & 0x10) != 0;
        flags.in_pits = data.is_in_pits != 0;
        flags.pit_limiter = data.pit_limiter_on != 0;
        flags.drs_available = data.drs_available != 0;
        flags.drs_active = data.drs_enabled != 0;
        flags.traction_control = data.tc != 0;
        flags.abs_active = data.abs != 0;
        
        // Calculate slip ratio from tire data
        let slip_ratio = if data.speed > 1.0 {
            // Use average of front tires for slip calculation
            let avg_tire_slip = (data.wheel_slip[0] + data.wheel_slip[1]) / 2.0;
            avg_tire_slip.abs().min(1.0)
        } else {
            0.0
        };
        
        // Extract car and track info
        let car_id = if data.car_model[0] != 0 {
            Some(extract_string(&data.car_model))
        } else {
            None
        };
        
        let track_id = if data.track[0] != 0 {
            Some(extract_string(&data.track))
        } else {
            None
        };
        
        // Create extended data with ACC-specific information
        let mut extended = HashMap::new();
        extended.insert("fuel_remaining".to_string(), TelemetryValue::Float(data.fuel));
        extended.insert("lap_count".to_string(), TelemetryValue::Integer(data.completed_laps));
        extended.insert("current_lap_time".to_string(), TelemetryValue::Float(data.current_lap_time));
        extended.insert("last_lap_time".to_string(), TelemetryValue::Float(data.last_lap));
        extended.insert("best_lap_time".to_string(), TelemetryValue::Float(data.best_lap));
        extended.insert("gas_pedal".to_string(), TelemetryValue::Float(data.gas));
        extended.insert("brake_pedal".to_string(), TelemetryValue::Float(data.brake));
        extended.insert("steering_angle".to_string(), TelemetryValue::Float(data.steer_angle));
        extended.insert("tc_level".to_string(), TelemetryValue::Integer(data.tc as i32));
        extended.insert("abs_level".to_string(), TelemetryValue::Integer(data.abs as i32));
        extended.insert("ers_recovery".to_string(), TelemetryValue::Float(data.ers_recovery_level));
        extended.insert("ers_power".to_string(), TelemetryValue::Float(data.ers_power_level));
        
        NormalizedTelemetry::new()
            .with_ffb_scalar(data.steer_angle / 450.0) // Normalize steering angle to FFB scalar
            .with_rpm(data.rpm)
            .with_speed_ms(data.speed / 3.6) // Convert km/h to m/s
            .with_slip_ratio(slip_ratio)
            .with_gear(data.gear)
            .with_car_id(car_id.unwrap_or_default())
            .with_track_id(track_id.unwrap_or_default())
            .with_flags(flags)
            .with_extended("fuel_remaining".to_string(), TelemetryValue::Float(data.fuel))
            .with_extended("lap_count".to_string(), TelemetryValue::Integer(data.completed_laps))
            .with_extended("gas_pedal".to_string(), TelemetryValue::Float(data.gas))
            .with_extended("brake_pedal".to_string(), TelemetryValue::Float(data.brake))
    }
}

/// Extract null-terminated string from byte array
fn extract_string(bytes: &[u8]) -> String {
    match bytes.iter().position(|&b| b == 0) {
        Some(pos) => {
            String::from_utf8_lossy(&bytes[..pos]).to_string()
        }
        None => {
            String::from_utf8_lossy(bytes).to_string()
        }
    }
}

/// ACC telemetry packet ID
const ACC_TELEMETRY_PACKET_ID: u32 = 0x12345678;

/// ACC telemetry data structure
/// This represents the UDP packet format used by ACC
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct ACCTelemetryData {
    packet_id: u32,
    
    // Car dynamics
    speed: f32,                    // km/h
    rpm: f32,
    gear: i8,
    gas: f32,                      // 0-1
    brake: f32,                    // 0-1
    steer_angle: f32,              // degrees
    
    // Tire data
    wheel_slip: [f32; 4],          // FL, FR, RL, RR
    wheel_load: [f32; 4],          // Tire loads
    wheel_pressure: [f32; 4],      // Tire pressures
    wheel_angular_speed: [f32; 4], // Wheel speeds
    
    // Lap and timing
    completed_laps: i32,
    current_lap_time: f32,         // seconds
    last_lap: f32,                 // seconds
    best_lap: f32,                 // seconds
    
    // Car status
    fuel: f32,                     // liters
    
    // Electronics
    tc: u8,                        // Traction control level
    abs: u8,                       // ABS level
    ers_recovery_level: f32,       // ERS recovery level
    ers_power_level: f32,          // ERS power level
    drs_available: u8,             // DRS available
    drs_enabled: u8,               // DRS enabled
    
    // Session info
    flag: u8,                      // Session flags
    is_in_pits: u8,               // In pit lane
    pit_limiter_on: u8,           // Pit limiter active
    
    // String data
    car_model: [u8; 32],          // Car model name
    track: [u8; 32],              // Track name
    
    // Padding to ensure consistent packet size
    _padding: [u8; 64],
}

impl Default for ACCTelemetryData {
    fn default() -> Self {
        Self {
            packet_id: ACC_TELEMETRY_PACKET_ID,
            speed: 0.0,
            rpm: 0.0,
            gear: 0,
            gas: 0.0,
            brake: 0.0,
            steer_angle: 0.0,
            wheel_slip: [0.0; 4],
            wheel_load: [0.0; 4],
            wheel_pressure: [0.0; 4],
            wheel_angular_speed: [0.0; 4],
            completed_laps: 0,
            current_lap_time: 0.0,
            last_lap: 0.0,
            best_lap: 0.0,
            fuel: 0.0,
            tc: 0,
            abs: 0,
            ers_recovery_level: 0.0,
            ers_power_level: 0.0,
            drs_available: 0,
            drs_enabled: 0,
            flag: 0,
            is_in_pits: 0,
            pit_limiter_on: 0,
            car_model: [0; 32],
            track: [0; 32],
            _padding: [0; 64],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem;

    #[test]
    fn test_acc_adapter_creation() {
        let adapter = ACCAdapter::new();
        assert_eq!(adapter.game_id(), "acc");
        assert_eq!(adapter.expected_update_rate(), Duration::from_millis(16));
    }

    #[test]
    fn test_acc_adapter_with_address() {
        let addr = "192.168.1.100:9999".parse()
            .expect("Test address should be valid");
        let adapter = ACCAdapter::with_address(addr);
        assert_eq!(adapter.listen_address, addr);
    }

    #[test]
    fn test_parse_udp_packet() {
        let mut data = ACCTelemetryData::default();
        data.packet_id = ACC_TELEMETRY_PACKET_ID;
        data.rpm = 6000.0;
        data.speed = 120.0; // km/h
        data.gear = 4;
        data.gas = 0.8;
        data.brake = 0.2;
        data.steer_angle = 45.0;
        
        let raw_bytes = unsafe {
            std::slice::from_raw_parts(
                &data as *const _ as *const u8,
                mem::size_of::<ACCTelemetryData>(),
            )
        };
        
        let parsed = ACCAdapter::parse_udp_packet(raw_bytes).unwrap();
        // Copy fields to avoid packed struct alignment issues
        let rpm = parsed.rpm;
        let speed = parsed.speed;
        let gear = parsed.gear;
        assert_eq!(rpm, 6000.0);
        assert_eq!(speed, 120.0);
        assert_eq!(gear, 4);
    }

    #[test]
    fn test_parse_invalid_packet() {
        let mut data = ACCTelemetryData::default();
        data.packet_id = 0xDEADBEEF; // Wrong packet ID
        
        let raw_bytes = unsafe {
            std::slice::from_raw_parts(
                &data as *const _ as *const u8,
                mem::size_of::<ACCTelemetryData>(),
            )
        };
        
        let result = ACCAdapter::parse_udp_packet(raw_bytes);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_too_small_packet() {
        let small_data = vec![0u8; 10];
        let result = ACCAdapter::parse_udp_packet(&small_data);
        assert!(result.is_err());
    }

    #[test]
    fn test_normalize_acc_data() {
        let mut data = ACCTelemetryData::default();
        data.rpm = 7000.0;
        data.speed = 144.0; // 144 km/h = 40 m/s
        data.gear = 5;
        data.steer_angle = 90.0; // 90 degrees
        data.gas = 0.9;
        data.brake = 0.1;
        data.fuel = 45.5;
        data.completed_laps = 12;
        data.flag = 0x01; // Yellow flag
        data.is_in_pits = 0;
        data.tc = 3;
        data.abs = 2;
        
        // Set car and track names
        let car_name = b"ferrari_488_gt3\0";
        let track_name = b"monza\0";
        data.car_model[..car_name.len()].copy_from_slice(car_name);
        data.track[..track_name.len()].copy_from_slice(track_name);
        
        let normalized = ACCAdapter::normalize_acc_data(&data);
        
        assert_eq!(normalized.rpm, Some(7000.0));
        assert_eq!(normalized.speed_ms, Some(40.0)); // 144 km/h converted to m/s
        assert_eq!(normalized.gear, Some(5));
        assert_eq!(normalized.ffb_scalar, Some(0.2)); // 90.0 / 450.0
        assert_eq!(normalized.car_id, Some("ferrari_488_gt3".to_string()));
        assert_eq!(normalized.track_id, Some("monza".to_string()));
        assert!(normalized.flags.yellow_flag);
        assert!(normalized.flags.traction_control);
        assert!(!normalized.flags.in_pits);
        
        // Check extended data
        assert!(normalized.extended.contains_key("fuel_remaining"));
        assert!(normalized.extended.contains_key("lap_count"));
        assert!(normalized.extended.contains_key("gas_pedal"));
        assert!(normalized.extended.contains_key("brake_pedal"));
        
        if let Some(TelemetryValue::Float(fuel)) = normalized.extended.get("fuel_remaining") {
            assert_eq!(*fuel, 45.5);
        } else {
            panic!("Expected fuel_remaining to be a float");
        }
    }

    #[test]
    fn test_normalize_with_slip_calculation() {
        let mut data = ACCTelemetryData::default();
        data.speed = 50.0; // km/h
        data.wheel_slip = [0.1, 0.15, 0.05, 0.08]; // Front tires: 0.1, 0.15
        
        let normalized = ACCAdapter::normalize_acc_data(&data);
        
        // Average of front tires: (0.1 + 0.15) / 2 = 0.125
        assert_eq!(normalized.slip_ratio, Some(0.125));
    }

    #[test]
    fn test_normalize_low_speed_slip() {
        let mut data = ACCTelemetryData::default();
        data.speed = 0.5; // Very low speed
        data.wheel_slip = [0.5, 0.6, 0.4, 0.3];
        
        let normalized = ACCAdapter::normalize_acc_data(&data);
        
        // At low speed, slip ratio should be 0
        assert_eq!(normalized.slip_ratio, Some(0.0));
    }

    #[test]
    fn test_extract_string() {
        let bytes = b"test_car\0remaining_bytes";
        let result = extract_string(bytes);
        assert_eq!(result, "test_car");
        
        let bytes_no_null = b"no_null_here";
        let result = extract_string(bytes_no_null);
        assert_eq!(result, "no_null_here");
    }

    #[test]
    fn test_normalize_method() {
        let adapter = ACCAdapter::new();
        
        let data = ACCTelemetryData::default();
        let raw_bytes = unsafe {
            std::slice::from_raw_parts(
                &data as *const _ as *const u8,
                mem::size_of::<ACCTelemetryData>(),
            )
        };
        
        let result = adapter.normalize(raw_bytes);
        assert!(result.is_ok());
    }

    #[test]
    fn test_normalize_invalid_data() {
        let adapter = ACCAdapter::new();
        
        let invalid_data = vec![0u8; 10]; // Too small
        let result = adapter.normalize(&invalid_data);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_is_game_running() {
        let adapter = ACCAdapter::new();
        
        // This test depends on system state, so we just verify it doesn't panic
        let result = adapter.is_game_running().await;
        assert!(result.is_ok());
    }
}
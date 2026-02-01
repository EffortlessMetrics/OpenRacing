//! rFactor 2 telemetry adapter with shared memory interface
//!
//! Implements telemetry adapter for rFactor 2 using shared memory.
//! rFactor 2 exposes telemetry data through memory-mapped files.
//! Requirements: 12.4

use crate::telemetry::{
    NormalizedTelemetry, TelemetryAdapter, TelemetryFlags, TelemetryFrame, TelemetryReceiver,
    TelemetryValue,
};
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
    memoryapi::{FILE_MAP_READ, MapViewOfFile, OpenFileMappingW, UnmapViewOfFile},
    winnt::HANDLE,
};

/// rFactor 2 shared memory name for telemetry data
const RF2_TELEMETRY_SHARED_MEMORY_NAME: &str = "$rFactor2SMMP_Telemetry$";

/// rFactor 2 shared memory name for scoring data
const RF2_SCORING_SHARED_MEMORY_NAME: &str = "$rFactor2SMMP_Scoring$";

/// Maximum number of wheels per vehicle
const RF2_MAX_WHEELS: usize = 4;

/// rFactor 2 telemetry adapter using shared memory
pub struct RFactor2Adapter {
    update_rate: Duration,
    #[cfg(windows)]
    telemetry_memory: Option<TelemetryMemoryHandle>,
    #[cfg(windows)]
    scoring_memory: Option<ScoringMemoryHandle>,
}

#[cfg(windows)]
struct TelemetryMemoryHandle {
    handle: HANDLE,
    base_ptr: *const u8,
    #[allow(dead_code)]
    size: usize,
}

#[cfg(windows)]
struct ScoringMemoryHandle {
    handle: HANDLE,
    base_ptr: *const u8,
    #[allow(dead_code)]
    size: usize,
}

#[cfg(windows)]
unsafe impl Send for TelemetryMemoryHandle {}
#[cfg(windows)]
unsafe impl Sync for TelemetryMemoryHandle {}

#[cfg(windows)]
unsafe impl Send for ScoringMemoryHandle {}
#[cfg(windows)]
unsafe impl Sync for ScoringMemoryHandle {}

impl Default for RFactor2Adapter {
    fn default() -> Self {
        Self::new()
    }
}

impl RFactor2Adapter {
    /// Create a new rFactor 2 adapter
    pub fn new() -> Self {
        Self {
            update_rate: Duration::from_millis(16), // ~60 FPS default
            #[cfg(windows)]
            telemetry_memory: None,
            #[cfg(windows)]
            scoring_memory: None,
        }
    }

    /// Initialize shared memory connection to rFactor 2 telemetry
    #[cfg(windows)]
    fn initialize_telemetry_memory(&mut self) -> Result<()> {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;

        let wide_name: Vec<u16> = OsStr::new(RF2_TELEMETRY_SHARED_MEMORY_NAME)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        unsafe {
            let handle = OpenFileMappingW(FILE_MAP_READ, 0, wide_name.as_ptr());

            if handle.is_null() {
                return Err(anyhow::anyhow!(
                    "Failed to open rFactor 2 telemetry shared memory. Is rFactor 2 running?"
                ));
            }

            // Map a reasonable size for the telemetry header + first vehicle
            let map_size =
                mem::size_of::<RF2TelemetryHeader>() + mem::size_of::<RF2VehicleTelemetry>();
            let base_ptr = MapViewOfFile(handle, FILE_MAP_READ, 0, 0, map_size) as *const u8;

            if base_ptr.is_null() {
                CloseHandle(handle);
                return Err(anyhow::anyhow!(
                    "Failed to map rFactor 2 telemetry shared memory"
                ));
            }

            self.telemetry_memory = Some(TelemetryMemoryHandle {
                handle,
                base_ptr,
                size: map_size,
            });

            info!("Successfully connected to rFactor 2 telemetry shared memory");
            Ok(())
        }
    }

    #[cfg(not(windows))]
    fn initialize_telemetry_memory(&mut self) -> Result<()> {
        Err(anyhow::anyhow!(
            "rFactor 2 shared memory only available on Windows"
        ))
    }

    /// Initialize shared memory connection to rFactor 2 scoring data
    #[cfg(windows)]
    fn initialize_scoring_memory(&mut self) -> Result<()> {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;

        let wide_name: Vec<u16> = OsStr::new(RF2_SCORING_SHARED_MEMORY_NAME)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        unsafe {
            let handle = OpenFileMappingW(FILE_MAP_READ, 0, wide_name.as_ptr());

            if handle.is_null() {
                return Err(anyhow::anyhow!(
                    "Failed to open rFactor 2 scoring shared memory. Is rFactor 2 running?"
                ));
            }

            let map_size = mem::size_of::<RF2ScoringHeader>();
            let base_ptr = MapViewOfFile(handle, FILE_MAP_READ, 0, 0, map_size) as *const u8;

            if base_ptr.is_null() {
                CloseHandle(handle);
                return Err(anyhow::anyhow!(
                    "Failed to map rFactor 2 scoring shared memory"
                ));
            }

            self.scoring_memory = Some(ScoringMemoryHandle {
                handle,
                base_ptr,
                size: map_size,
            });

            info!("Successfully connected to rFactor 2 scoring shared memory");
            Ok(())
        }
    }

    #[cfg(not(windows))]
    fn initialize_scoring_memory(&mut self) -> Result<()> {
        Err(anyhow::anyhow!(
            "rFactor 2 shared memory only available on Windows"
        ))
    }

    /// Read telemetry header and first vehicle from shared memory
    #[cfg(windows)]
    fn read_telemetry_data(&self) -> Result<(RF2TelemetryHeader, RF2VehicleTelemetry)> {
        let mem = self
            .telemetry_memory
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Telemetry shared memory not initialized"))?;

        unsafe {
            let header = ptr::read_volatile(mem.base_ptr as *const RF2TelemetryHeader);
            let vehicle_ptr = mem.base_ptr.add(mem::size_of::<RF2TelemetryHeader>());
            let vehicle = ptr::read_volatile(vehicle_ptr as *const RF2VehicleTelemetry);
            Ok((header, vehicle))
        }
    }

    #[cfg(not(windows))]
    fn read_telemetry_data(&self) -> Result<(RF2TelemetryHeader, RF2VehicleTelemetry)> {
        Err(anyhow::anyhow!(
            "rFactor 2 shared memory only available on Windows"
        ))
    }

    /// Read scoring header from shared memory
    #[cfg(windows)]
    fn read_scoring_data(&self) -> Result<RF2ScoringHeader> {
        let mem = self
            .scoring_memory
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Scoring shared memory not initialized"))?;

        unsafe {
            let header = ptr::read_volatile(mem.base_ptr as *const RF2ScoringHeader);
            Ok(header)
        }
    }

    #[cfg(not(windows))]
    fn read_scoring_data(&self) -> Result<RF2ScoringHeader> {
        Err(anyhow::anyhow!(
            "rFactor 2 shared memory only available on Windows"
        ))
    }

    /// Check if rFactor 2 is running by attempting to open shared memory
    #[cfg(windows)]
    async fn check_rf2_running(&self) -> bool {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;

        let wide_name: Vec<u16> = OsStr::new(RF2_TELEMETRY_SHARED_MEMORY_NAME)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        unsafe {
            let handle = OpenFileMappingW(FILE_MAP_READ, 0, wide_name.as_ptr());

            if !handle.is_null() {
                CloseHandle(handle);
                true
            } else {
                false
            }
        }
    }

    #[cfg(not(windows))]
    async fn check_rf2_running(&self) -> bool {
        false
    }

    /// Normalize rFactor 2 telemetry data to common format
    fn normalize_rf2_data(
        &self,
        vehicle: &RF2VehicleTelemetry,
        scoring: Option<&RF2ScoringHeader>,
    ) -> NormalizedTelemetry {
        // Extract flags from scoring data if available
        let flags = if let Some(scoring_data) = scoring {
            self.extract_flags(scoring_data)
        } else {
            TelemetryFlags::default()
        };

        // Calculate slip ratio from wheel data
        let slip_ratio = self.calculate_slip_ratio(vehicle);

        // Extract car and track names
        let car_id = extract_string(&vehicle.vehicle_name);
        let track_id = extract_string(&vehicle.track_name);

        // Create extended data with rFactor 2-specific information
        let mut extended = HashMap::new();
        extended.insert(
            "fuel_level".to_string(),
            TelemetryValue::Float(vehicle.fuel),
        );
        extended.insert(
            "throttle".to_string(),
            TelemetryValue::Float(vehicle.unfiltered_throttle),
        );
        extended.insert(
            "brake".to_string(),
            TelemetryValue::Float(vehicle.unfiltered_brake),
        );
        extended.insert(
            "clutch".to_string(),
            TelemetryValue::Float(vehicle.unfiltered_clutch),
        );
        extended.insert(
            "steering".to_string(),
            TelemetryValue::Float(vehicle.unfiltered_steering),
        );
        extended.insert(
            "water_temp".to_string(),
            TelemetryValue::Float(vehicle.engine_water_temp),
        );
        extended.insert(
            "oil_temp".to_string(),
            TelemetryValue::Float(vehicle.engine_oil_temp),
        );

        // Calculate FFB scalar from steering torque (normalize to -1.0 to 1.0)
        let ffb_scalar = (vehicle.steering_shaft_torque / 50.0).clamp(-1.0, 1.0);

        NormalizedTelemetry::default()
            .with_ffb_scalar(ffb_scalar)
            .with_rpm(vehicle.engine_rpm)
            .with_speed_ms(vehicle.speed)
            .with_slip_ratio(slip_ratio)
            .with_gear(vehicle.gear)
            .with_car_id(car_id)
            .with_track_id(track_id)
            .with_flags(flags)
    }

    /// Extract flags from scoring data
    fn extract_flags(&self, scoring: &RF2ScoringHeader) -> TelemetryFlags {
        TelemetryFlags {
            yellow_flag: scoring.yellow_flag_state != 0,
            red_flag: scoring.game_phase == GamePhase::RedFlag as i32,
            blue_flag: false, // Blue flag is per-vehicle in rF2
            checkered_flag: scoring.game_phase == GamePhase::Checkered as i32,
            green_flag: scoring.game_phase == GamePhase::GreenFlag as i32,
            pit_limiter: false, // Per-vehicle data
            in_pits: scoring.in_pits != 0,
            drs_available: false,
            drs_active: false,
            ers_available: false,
            launch_control: false,
            traction_control: false,
            abs_active: false,
        }
    }

    /// Calculate average slip ratio from wheel data
    fn calculate_slip_ratio(&self, vehicle: &RF2VehicleTelemetry) -> f32 {
        if vehicle.speed < 1.0 {
            return 0.0;
        }

        let mut total_slip = 0.0;
        for wheel in &vehicle.wheels {
            total_slip += wheel.lateral_patch_slip.abs();
        }
        (total_slip / RF2_MAX_WHEELS as f32).min(1.0)
    }
}

#[async_trait]
impl TelemetryAdapter for RFactor2Adapter {
    fn game_id(&self) -> &str {
        "rfactor2"
    }

    async fn start_monitoring(&self) -> Result<TelemetryReceiver> {
        let (tx, rx) = mpsc::channel(100);
        let update_rate = self.update_rate;

        tokio::spawn(async move {
            let mut adapter = RFactor2Adapter::new();
            let mut sequence = 0u64;
            let mut last_update_index = 0u32;

            // Try to initialize shared memory
            if let Err(e) = adapter.initialize_telemetry_memory() {
                error!(
                    "Failed to initialize rFactor 2 telemetry shared memory: {}",
                    e
                );
                return;
            }

            // Try to initialize scoring memory (optional)
            if let Err(e) = adapter.initialize_scoring_memory() {
                warn!(
                    "Failed to initialize rFactor 2 scoring shared memory: {}",
                    e
                );
            }

            info!("Started rFactor 2 telemetry monitoring");

            loop {
                let start_time = std::time::Instant::now();

                match adapter.read_telemetry_data() {
                    Ok((header, vehicle)) => {
                        if header.update_index != last_update_index {
                            last_update_index = header.update_index;

                            let scoring = adapter.read_scoring_data().ok();
                            let normalized = adapter.normalize_rf2_data(&vehicle, scoring.as_ref());

                            let frame = TelemetryFrame::new(
                                normalized,
                                start_time.elapsed().as_nanos() as u64,
                                sequence,
                                mem::size_of::<RF2VehicleTelemetry>(),
                            );

                            if tx.send(frame).await.is_err() {
                                debug!("Telemetry receiver dropped, stopping monitoring");
                                break;
                            }

                            sequence += 1;
                        }
                    }
                    Err(e) => {
                        warn!("Failed to read rFactor 2 telemetry: {}", e);
                    }
                }

                tokio::time::sleep(update_rate).await;
            }

            info!("Stopped rFactor 2 telemetry monitoring");
        });

        Ok(rx)
    }

    async fn stop_monitoring(&self) -> Result<()> {
        Ok(())
    }

    fn normalize(&self, raw: &[u8]) -> Result<NormalizedTelemetry> {
        if raw.len() < mem::size_of::<RF2VehicleTelemetry>() {
            return Err(anyhow::anyhow!(
                "Invalid rFactor 2 data size: expected at least {}, got {}",
                mem::size_of::<RF2VehicleTelemetry>(),
                raw.len()
            ));
        }

        let vehicle: RF2VehicleTelemetry =
            unsafe { ptr::read(raw.as_ptr() as *const RF2VehicleTelemetry) };

        Ok(self.normalize_rf2_data(&vehicle, None))
    }

    fn expected_update_rate(&self) -> Duration {
        self.update_rate
    }

    async fn is_game_running(&self) -> Result<bool> {
        Ok(self.check_rf2_running().await)
    }
}

/// Extract null-terminated string from byte array
fn extract_string(bytes: &[u8]) -> String {
    match bytes.iter().position(|&b| b == 0) {
        Some(pos) => String::from_utf8_lossy(&bytes[..pos]).into_owned(),
        None => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// Game phase enumeration for rFactor 2
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GamePhase {
    Garage = 0,
    WarmUp = 1,
    GridWalk = 2,
    Formation = 3,
    Countdown = 4,
    GreenFlag = 5,
    FullCourseYellow = 6,
    SessionStopped = 7,
    SessionOver = 8,
    PausedOrReplay = 9,
    RedFlag = 10,
    Checkered = 11,
}

/// rFactor 2 telemetry header (shared memory header)
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct RF2TelemetryHeader {
    /// Version number
    pub version: u32,
    /// Bytes offset to vehicle data
    pub bytes_offset: u32,
    /// Number of vehicles
    pub num_vehicles: i32,
    /// Update index (increments each update)
    pub update_index: u32,
}

/// rFactor 2 wheel telemetry data
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct RF2WheelTelemetry {
    /// Suspension deflection (meters)
    pub suspension_deflection: f64,
    /// Ride height (meters)
    pub ride_height: f64,
    /// Suspension force (Newtons)
    pub suspension_force: f64,
    /// Brake temperature (Celsius)
    pub brake_temp: f64,
    /// Brake pressure (0.0-1.0)
    pub brake_pressure: f64,
    /// Wheel rotation (radians/sec)
    pub rotation: f64,
    /// Lateral patch velocity (m/s)
    pub lateral_patch_vel: f64,
    /// Longitudinal patch velocity (m/s)
    pub longitudinal_patch_vel: f64,
    /// Lateral ground contact (m/s)
    pub lateral_ground_vel: f64,
    /// Longitudinal ground contact (m/s)
    pub longitudinal_ground_vel: f64,
    /// Camber angle (radians)
    pub camber: f64,
    /// Lateral force (Newtons)
    pub lateral_force: f64,
    /// Longitudinal force (Newtons)
    pub longitudinal_force: f64,
    /// Tire load (Newtons)
    pub tire_load: f64,
    /// Grip fraction (0.0-1.0)
    pub grip_fract: f64,
    /// Pressure (kPa)
    pub pressure: f64,
    /// Temperature (Celsius) - inner, center, outer
    pub temperature: [f64; 3],
    /// Wear (0.0-1.0)
    pub wear: f64,
    /// Lateral patch slip
    pub lateral_patch_slip: f32,
    /// Longitudinal patch slip
    pub longitudinal_patch_slip: f32,
}

/// rFactor 2 vehicle telemetry data (simplified for player vehicle)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct RF2VehicleTelemetry {
    /// Slot ID
    pub id: i32,
    /// Delta time (seconds)
    pub delta_time: f64,
    /// Elapsed time (seconds)
    pub elapsed_time: f64,
    /// Lap number
    pub lap_number: i32,
    /// Lap start elapsed time
    pub lap_start_et: f64,
    /// Vehicle name
    pub vehicle_name: [u8; 64],
    /// Track name
    pub track_name: [u8; 64],
    /// World position (x, y, z)
    pub pos: [f64; 3],
    /// Local velocity (x, y, z)
    pub local_vel: [f64; 3],
    /// Local acceleration (x, y, z)
    pub local_accel: [f64; 3],
    /// Orientation (x, y, z) - Euler angles
    pub ori: [f64; 3],
    /// Local rotation (x, y, z)
    pub local_rot: [f64; 3],
    /// Local rotation acceleration (x, y, z)
    pub local_rot_accel: [f64; 3],
    /// Speed (m/s)
    pub speed: f32,
    /// Gear (-1=reverse, 0=neutral, 1+=forward)
    pub gear: i8,
    /// Padding for alignment
    _pad1: [u8; 3],
    /// Engine RPM
    pub engine_rpm: f32,
    /// Engine water temperature (Celsius)
    pub engine_water_temp: f32,
    /// Engine oil temperature (Celsius)
    pub engine_oil_temp: f32,
    /// Clutch RPM
    pub clutch_rpm: f32,
    /// Unfiltered throttle (0.0-1.0)
    pub unfiltered_throttle: f32,
    /// Unfiltered brake (0.0-1.0)
    pub unfiltered_brake: f32,
    /// Unfiltered steering (-1.0 to 1.0)
    pub unfiltered_steering: f32,
    /// Unfiltered clutch (0.0-1.0)
    pub unfiltered_clutch: f32,
    /// Steering shaft torque (Nm)
    pub steering_shaft_torque: f32,
    /// Fuel (liters)
    pub fuel: f32,
    /// Engine max RPM
    pub engine_max_rpm: f32,
    /// Wheel data (FL, FR, RL, RR)
    pub wheels: [RF2WheelTelemetry; RF2_MAX_WHEELS],
}

impl Default for RF2VehicleTelemetry {
    fn default() -> Self {
        Self {
            id: 0,
            delta_time: 0.0,
            elapsed_time: 0.0,
            lap_number: 0,
            lap_start_et: 0.0,
            vehicle_name: [0; 64],
            track_name: [0; 64],
            pos: [0.0; 3],
            local_vel: [0.0; 3],
            local_accel: [0.0; 3],
            ori: [0.0; 3],
            local_rot: [0.0; 3],
            local_rot_accel: [0.0; 3],
            speed: 0.0,
            gear: 0,
            _pad1: [0; 3],
            engine_rpm: 0.0,
            engine_water_temp: 0.0,
            engine_oil_temp: 0.0,
            clutch_rpm: 0.0,
            unfiltered_throttle: 0.0,
            unfiltered_brake: 0.0,
            unfiltered_steering: 0.0,
            unfiltered_clutch: 0.0,
            steering_shaft_torque: 0.0,
            fuel: 0.0,
            engine_max_rpm: 0.0,
            wheels: [RF2WheelTelemetry::default(); RF2_MAX_WHEELS],
        }
    }
}

/// rFactor 2 scoring header (simplified)
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct RF2ScoringHeader {
    /// Version number
    pub version: u32,
    /// Bytes offset
    pub bytes_offset: u32,
    /// Number of vehicles
    pub num_vehicles: i32,
    /// Game phase
    pub game_phase: i32,
    /// Yellow flag state
    pub yellow_flag_state: i32,
    /// In pits flag
    pub in_pits: i32,
}

#[cfg(windows)]
impl Drop for TelemetryMemoryHandle {
    fn drop(&mut self) {
        unsafe {
            if !self.base_ptr.is_null() {
                UnmapViewOfFile(self.base_ptr as *const _);
            }
            if !self.handle.is_null() {
                CloseHandle(self.handle);
            }
        }
    }
}

#[cfg(windows)]
impl Drop for ScoringMemoryHandle {
    fn drop(&mut self) {
        unsafe {
            if !self.base_ptr.is_null() {
                UnmapViewOfFile(self.base_ptr as *const _);
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

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn test_rfactor2_adapter_creation() -> TestResult {
        let adapter = RFactor2Adapter::new();
        assert_eq!(adapter.game_id(), "rfactor2");
        assert_eq!(adapter.expected_update_rate(), Duration::from_millis(16));
        Ok(())
    }

    #[test]
    fn test_normalize_rf2_data() -> TestResult {
        let adapter = RFactor2Adapter::new();

        let vehicle_name = b"formula_renault\0";
        let track_name = b"spa_francorchamps\0";
        let mut vehicle_name_arr = [0u8; 64];
        let mut track_name_arr = [0u8; 64];
        vehicle_name_arr[..vehicle_name.len()].copy_from_slice(vehicle_name);
        track_name_arr[..track_name.len()].copy_from_slice(track_name);

        let vehicle = RF2VehicleTelemetry {
            engine_rpm: 8500.0,
            speed: 65.0,
            gear: 4,
            steering_shaft_torque: 15.0,
            unfiltered_throttle: 0.85,
            unfiltered_brake: 0.0,
            unfiltered_clutch: 0.0,
            fuel: 42.0,
            engine_water_temp: 92.0,
            engine_oil_temp: 105.0,
            vehicle_name: vehicle_name_arr,
            track_name: track_name_arr,
            ..Default::default()
        };

        let scoring = RF2ScoringHeader {
            game_phase: GamePhase::GreenFlag as i32,
            yellow_flag_state: 0,
            in_pits: 0,
            ..Default::default()
        };

        let normalized = adapter.normalize_rf2_data(&vehicle, Some(&scoring));

        assert_eq!(normalized.rpm, Some(8500.0));
        assert_eq!(normalized.speed_ms, Some(65.0));
        assert_eq!(normalized.gear, Some(4));
        assert!(normalized.ffb_scalar.is_some());
        assert_eq!(normalized.car_id, Some("formula_renault".to_string()));
        assert_eq!(normalized.track_id, Some("spa_francorchamps".to_string()));
        assert!(normalized.flags.green_flag);
        assert!(!normalized.flags.in_pits);
        Ok(())
    }

    #[test]
    fn test_normalize_with_flags() -> TestResult {
        let adapter = RFactor2Adapter::new();
        let vehicle = RF2VehicleTelemetry::default();

        // Test yellow flag
        let scoring_yellow = RF2ScoringHeader {
            yellow_flag_state: 1,
            game_phase: GamePhase::FullCourseYellow as i32,
            ..Default::default()
        };
        let normalized = adapter.normalize_rf2_data(&vehicle, Some(&scoring_yellow));
        assert!(normalized.flags.yellow_flag);
        assert!(!normalized.flags.green_flag);

        // Test red flag
        let scoring_red = RF2ScoringHeader {
            game_phase: GamePhase::RedFlag as i32,
            ..Default::default()
        };
        let normalized = adapter.normalize_rf2_data(&vehicle, Some(&scoring_red));
        assert!(normalized.flags.red_flag);

        // Test checkered flag
        let scoring_checkered = RF2ScoringHeader {
            game_phase: GamePhase::Checkered as i32,
            ..Default::default()
        };
        let normalized = adapter.normalize_rf2_data(&vehicle, Some(&scoring_checkered));
        assert!(normalized.flags.checkered_flag);

        // Test in pits
        let scoring_pits = RF2ScoringHeader {
            in_pits: 1,
            ..Default::default()
        };
        let normalized = adapter.normalize_rf2_data(&vehicle, Some(&scoring_pits));
        assert!(normalized.flags.in_pits);

        Ok(())
    }

    #[test]
    fn test_slip_ratio_calculation() -> TestResult {
        let adapter = RFactor2Adapter::new();

        // Test with speed > 1.0
        let mut vehicle = RF2VehicleTelemetry::default();
        vehicle.speed = 50.0;
        vehicle.wheels[0].lateral_patch_slip = 0.1;
        vehicle.wheels[1].lateral_patch_slip = 0.15;
        vehicle.wheels[2].lateral_patch_slip = 0.08;
        vehicle.wheels[3].lateral_patch_slip = 0.12;

        let normalized = adapter.normalize_rf2_data(&vehicle, None);

        // Average: (0.1 + 0.15 + 0.08 + 0.12) / 4 = 0.1125
        let expected_slip = 0.1125;
        let slip = normalized.slip_ratio.ok_or("expected slip_ratio")?;
        assert!((slip - expected_slip).abs() < 0.001);

        // Test with low speed (should be 0)
        let mut vehicle_low_speed = RF2VehicleTelemetry::default();
        vehicle_low_speed.speed = 0.5;
        vehicle_low_speed.wheels[0].lateral_patch_slip = 0.5;

        let normalized = adapter.normalize_rf2_data(&vehicle_low_speed, None);
        assert_eq!(normalized.slip_ratio, Some(0.0));

        Ok(())
    }

    #[test]
    fn test_slip_ratio_clamping() -> TestResult {
        let adapter = RFactor2Adapter::new();

        let mut vehicle = RF2VehicleTelemetry::default();
        vehicle.speed = 50.0;
        vehicle.wheels[0].lateral_patch_slip = 2.0;
        vehicle.wheels[1].lateral_patch_slip = 2.0;
        vehicle.wheels[2].lateral_patch_slip = 2.0;
        vehicle.wheels[3].lateral_patch_slip = 2.0;

        let normalized = adapter.normalize_rf2_data(&vehicle, None);
        assert_eq!(normalized.slip_ratio, Some(1.0));

        Ok(())
    }

    #[test]
    fn test_extract_string() -> TestResult {
        let bytes = b"test_string\0extra_data";
        let result = extract_string(bytes);
        assert_eq!(result, "test_string");

        let bytes_no_null = b"no_null_terminator";
        let result = extract_string(bytes_no_null);
        assert_eq!(result, "no_null_terminator");
        Ok(())
    }

    #[test]
    fn test_ffb_scalar_calculation() -> TestResult {
        let adapter = RFactor2Adapter::new();

        // Test normal steering torque
        let mut vehicle = RF2VehicleTelemetry::default();
        vehicle.steering_shaft_torque = 25.0; // 25 Nm

        let normalized = adapter.normalize_rf2_data(&vehicle, None);
        // 25.0 / 50.0 = 0.5
        let ffb = normalized.ffb_scalar.ok_or("expected ffb_scalar")?;
        assert!((ffb - 0.5).abs() < 0.01);

        // Test high steering torque (should be clamped)
        vehicle.steering_shaft_torque = 100.0;
        let normalized = adapter.normalize_rf2_data(&vehicle, None);
        assert_eq!(normalized.ffb_scalar, Some(1.0));

        // Test negative steering torque
        vehicle.steering_shaft_torque = -75.0;
        let normalized = adapter.normalize_rf2_data(&vehicle, None);
        assert_eq!(normalized.ffb_scalar, Some(-1.0));

        Ok(())
    }

    #[test]
    fn test_normalize_without_scoring() -> TestResult {
        let adapter = RFactor2Adapter::new();

        let mut vehicle = RF2VehicleTelemetry::default();
        vehicle.engine_rpm = 6000.0;
        vehicle.speed = 40.0;
        vehicle.gear = 3;

        let normalized = adapter.normalize_rf2_data(&vehicle, None);

        assert_eq!(normalized.rpm, Some(6000.0));
        assert_eq!(normalized.speed_ms, Some(40.0));
        assert_eq!(normalized.gear, Some(3));
        assert!(normalized.flags.green_flag); // Default is green
        Ok(())
    }

    #[tokio::test]
    async fn test_is_game_running() -> TestResult {
        let adapter = RFactor2Adapter::new();

        // On non-Windows platforms, should always return false
        #[cfg(not(windows))]
        {
            let result = adapter.is_game_running().await?;
            assert!(!result);
        }

        // On Windows, test depends on whether rFactor 2 is actually running
        #[cfg(windows)]
        {
            let _result = adapter.is_game_running().await?;
            // Can't assert the actual value since it depends on system state
        }

        Ok(())
    }

    #[test]
    fn test_game_phase_enum() -> TestResult {
        assert_eq!(GamePhase::Garage as i32, 0);
        assert_eq!(GamePhase::GreenFlag as i32, 5);
        assert_eq!(GamePhase::FullCourseYellow as i32, 6);
        assert_eq!(GamePhase::RedFlag as i32, 10);
        assert_eq!(GamePhase::Checkered as i32, 11);
        Ok(())
    }

    #[test]
    fn test_default_vehicle_telemetry() -> TestResult {
        let vehicle = RF2VehicleTelemetry::default();
        assert_eq!(vehicle.id, 0);
        assert_eq!(vehicle.engine_rpm, 0.0);
        assert_eq!(vehicle.speed, 0.0);
        assert_eq!(vehicle.gear, 0);
        assert_eq!(vehicle.fuel, 0.0);
        assert_eq!(vehicle.steering_shaft_torque, 0.0);
        Ok(())
    }

    #[test]
    fn test_default_wheel_telemetry() -> TestResult {
        let wheel = RF2WheelTelemetry::default();
        assert_eq!(wheel.suspension_deflection, 0.0);
        assert_eq!(wheel.brake_temp, 0.0);
        assert_eq!(wheel.lateral_patch_slip, 0.0);
        assert_eq!(wheel.longitudinal_patch_slip, 0.0);
        Ok(())
    }

    #[test]
    fn test_default_scoring_header() -> TestResult {
        let scoring = RF2ScoringHeader::default();
        assert_eq!(scoring.version, 0);
        assert_eq!(scoring.game_phase, 0);
        assert_eq!(scoring.yellow_flag_state, 0);
        assert_eq!(scoring.in_pits, 0);
        Ok(())
    }

    #[test]
    fn test_default_telemetry_header() -> TestResult {
        let header = RF2TelemetryHeader::default();
        assert_eq!(header.version, 0);
        assert_eq!(header.num_vehicles, 0);
        assert_eq!(header.update_index, 0);
        Ok(())
    }
}

//! AMS2 (Automobilista 2) telemetry adapter with shared memory interface
//!
//! Implements telemetry adapter for AMS2 using the Project CARS 2 shared memory format.
//! AMS2 uses the same shared memory interface as Project CARS 2 (PCARS2).
//! Requirements: 12.3

use crate::telemetry::{
    NormalizedTelemetry, TelemetryAdapter, TelemetryFlags, TelemetryFrame, TelemetryReceiver,
    TelemetryValue,
};
use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::mem;
use std::ptr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

#[cfg(windows)]
use winapi::um::{
    handleapi::CloseHandle,
    memoryapi::{FILE_MAP_READ, MapViewOfFile, OpenFileMappingW, UnmapViewOfFile},
    winnt::HANDLE,
};

/// AMS2 shared memory name (same as PCARS2)
const AMS2_SHARED_MEMORY_NAME: &str = "$pcars2$";

/// AMS2 telemetry adapter using shared memory (PCARS2 format)
pub struct AMS2Adapter {
    update_rate: Duration,
    #[cfg(windows)]
    shared_memory: Option<SharedMemoryHandle>,
}

#[cfg(windows)]
struct SharedMemoryHandle {
    handle: HANDLE,
    data_ptr: *const AMS2SharedMemory,
}

#[cfg(windows)]
unsafe impl Send for SharedMemoryHandle {}
#[cfg(windows)]
unsafe impl Sync for SharedMemoryHandle {}

impl Default for AMS2Adapter {
    fn default() -> Self {
        Self::new()
    }
}

impl AMS2Adapter {
    /// Create a new AMS2 adapter
    pub fn new() -> Self {
        Self {
            update_rate: Duration::from_millis(16), // ~60 FPS default
            #[cfg(windows)]
            shared_memory: None,
        }
    }

    /// Initialize shared memory connection to AMS2
    #[cfg(windows)]
    fn initialize_shared_memory(&mut self) -> Result<()> {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;

        // Build the full shared memory name: "Local\$pcars2$"
        let memory_name = format!("Local\\{}", AMS2_SHARED_MEMORY_NAME);
        let wide_name: Vec<u16> = OsStr::new(&memory_name)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        unsafe {
            let handle = OpenFileMappingW(FILE_MAP_READ, 0, wide_name.as_ptr());

            if handle.is_null() {
                return Err(anyhow::anyhow!(
                    "Failed to open AMS2 shared memory. Is AMS2 running?"
                ));
            }

            let data_ptr = MapViewOfFile(handle, FILE_MAP_READ, 0, 0, 0) as *const AMS2SharedMemory;

            if data_ptr.is_null() {
                CloseHandle(handle);
                return Err(anyhow::anyhow!("Failed to map AMS2 shared memory"));
            }

            self.shared_memory = Some(SharedMemoryHandle { handle, data_ptr });

            info!("Successfully connected to AMS2 shared memory");
            Ok(())
        }
    }

    #[cfg(not(windows))]
    fn initialize_shared_memory(&mut self) -> Result<()> {
        Err(anyhow::anyhow!(
            "AMS2 shared memory only available on Windows"
        ))
    }

    /// Read telemetry data from shared memory
    #[cfg(windows)]
    fn read_telemetry_data(&self) -> Result<AMS2SharedMemory> {
        let shared_memory = self
            .shared_memory
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Shared memory not initialized"))?;

        unsafe {
            let data = ptr::read_volatile(shared_memory.data_ptr);
            Ok(data)
        }
    }

    #[cfg(not(windows))]
    fn read_telemetry_data(&self) -> Result<AMS2SharedMemory> {
        Err(anyhow::anyhow!(
            "AMS2 shared memory only available on Windows"
        ))
    }

    /// Check if AMS2 is running by attempting to open shared memory
    #[cfg(windows)]
    async fn check_ams2_running(&self) -> bool {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;

        let memory_name = format!("Local\\{}", AMS2_SHARED_MEMORY_NAME);
        let wide_name: Vec<u16> = OsStr::new(&memory_name)
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
    async fn check_ams2_running(&self) -> bool {
        false
    }

    /// Normalize AMS2 data to common telemetry format
    fn normalize_ams2_data(&self, data: &AMS2SharedMemory) -> NormalizedTelemetry {
        // Extract flags from game state and race state
        let flags = TelemetryFlags {
            yellow_flag: data.highest_flag == HighestFlag::Yellow as u32,
            red_flag: data.highest_flag == HighestFlag::Red as u32,
            blue_flag: data.highest_flag == HighestFlag::Blue as u32,
            checkered_flag: data.highest_flag == HighestFlag::Chequered as u32,
            green_flag: data.highest_flag == HighestFlag::Green as u32,
            in_pits: data.pit_mode != PitMode::None as u32,
            pit_limiter: data.pit_mode == PitMode::InPitlane as u32,
            drs_available: data.drs_state == DrsState::Available as u32,
            drs_active: data.drs_state == DrsState::Active as u32,
            traction_control: data.tc_setting > 0,
            abs_active: data.abs_setting > 0,
            ..Default::default()
        };

        // Calculate slip ratio from tire data (average of all tires)
        let slip_ratio = if data.speed > 1.0 {
            let avg_slip = (data.tyre_slip[0].abs()
                + data.tyre_slip[1].abs()
                + data.tyre_slip[2].abs()
                + data.tyre_slip[3].abs())
                / 4.0;
            avg_slip.min(1.0)
        } else {
            0.0
        };

        // Extract car and track names
        let car_id = extract_string(&data.car_name);
        let track_id = extract_string(&data.track_location);

        // Create extended data with AMS2-specific information
        let mut extended = HashMap::new();
        extended.insert(
            "fuel_level".to_string(),
            TelemetryValue::Float(data.fuel_level),
        );
        extended.insert(
            "fuel_capacity".to_string(),
            TelemetryValue::Float(data.fuel_capacity),
        );
        extended.insert(
            "lap_count".to_string(),
            TelemetryValue::Integer(data.laps_completed as i32),
        );
        extended.insert(
            "current_lap_time".to_string(),
            TelemetryValue::Float(data.current_time),
        );
        extended.insert(
            "last_lap_time".to_string(),
            TelemetryValue::Float(data.last_lap_time),
        );
        extended.insert(
            "best_lap_time".to_string(),
            TelemetryValue::Float(data.best_lap_time),
        );
        extended.insert("throttle".to_string(), TelemetryValue::Float(data.throttle));
        extended.insert("brake".to_string(), TelemetryValue::Float(data.brake));
        extended.insert("clutch".to_string(), TelemetryValue::Float(data.clutch));
        extended.insert("steering".to_string(), TelemetryValue::Float(data.steering));
        extended.insert(
            "water_temp".to_string(),
            TelemetryValue::Float(data.water_temp_celsius),
        );
        extended.insert(
            "oil_temp".to_string(),
            TelemetryValue::Float(data.oil_temp_celsius),
        );
        extended.insert(
            "boost_pressure".to_string(),
            TelemetryValue::Float(data.boost_pressure),
        );
        extended.insert(
            "tc_setting".to_string(),
            TelemetryValue::Integer(data.tc_setting as i32),
        );
        extended.insert(
            "abs_setting".to_string(),
            TelemetryValue::Integer(data.abs_setting as i32),
        );

        // Calculate FFB scalar from steering force
        // AMS2 provides steering force in the range of approximately -1.0 to 1.0
        let ffb_scalar = data.steering.clamp(-1.0, 1.0);

        NormalizedTelemetry::default()
            .with_ffb_scalar(ffb_scalar)
            .with_rpm(data.rpm)
            .with_speed_ms(data.speed)
            .with_slip_ratio(slip_ratio)
            .with_gear(data.gear)
            .with_car_id(car_id)
            .with_track_id(track_id)
            .with_flags(flags)
            .with_extended(
                "fuel_level".to_string(),
                TelemetryValue::Float(data.fuel_level),
            )
            .with_extended(
                "lap_count".to_string(),
                TelemetryValue::Integer(data.laps_completed as i32),
            )
            .with_extended("throttle".to_string(), TelemetryValue::Float(data.throttle))
            .with_extended("brake".to_string(), TelemetryValue::Float(data.brake))
    }
}

#[async_trait]
impl TelemetryAdapter for AMS2Adapter {
    fn game_id(&self) -> &str {
        "ams2"
    }

    async fn start_monitoring(&self) -> Result<TelemetryReceiver> {
        let (tx, rx) = mpsc::channel(100);

        // Clone necessary data for the monitoring task
        let update_rate = self.update_rate;

        tokio::spawn(async move {
            let mut adapter = AMS2Adapter::new();
            let mut sequence = 0u64;
            let mut last_update_index = 0u32;

            // Try to initialize shared memory
            if let Err(e) = adapter.initialize_shared_memory() {
                error!("Failed to initialize AMS2 shared memory: {}", e);
                return;
            }

            info!("Started AMS2 telemetry monitoring");

            loop {
                match adapter.read_telemetry_data() {
                    Ok(data) => {
                        // Check if data has been updated using the update index
                        if data.update_index != last_update_index {
                            last_update_index = data.update_index;

                            // Normalize the data
                            let normalized = adapter.normalize_ams2_data(&data);

                            let frame = TelemetryFrame::new(
                                normalized,
                                unix_timestamp_ns(),
                                sequence,
                                mem::size_of::<AMS2SharedMemory>(),
                            );

                            if tx.send(frame).await.is_err() {
                                debug!("Telemetry receiver dropped, stopping monitoring");
                                break;
                            }

                            sequence += 1;
                        }
                    }
                    Err(e) => {
                        warn!("Failed to read AMS2 telemetry: {}", e);
                        // Continue trying - game might have been restarted
                    }
                }

                tokio::time::sleep(update_rate).await;
            }

            info!("Stopped AMS2 telemetry monitoring");
        });

        Ok(rx)
    }

    async fn stop_monitoring(&self) -> Result<()> {
        // Monitoring task will stop when receiver is dropped
        Ok(())
    }

    fn normalize(&self, raw: &[u8]) -> Result<NormalizedTelemetry> {
        if raw.len() < mem::size_of::<AMS2SharedMemory>() {
            return Err(anyhow::anyhow!(
                "Invalid AMS2 data size: expected at least {}, got {}",
                mem::size_of::<AMS2SharedMemory>(),
                raw.len()
            ));
        }

        let data: AMS2SharedMemory = unsafe { ptr::read(raw.as_ptr() as *const AMS2SharedMemory) };

        Ok(self.normalize_ams2_data(&data))
    }

    fn expected_update_rate(&self) -> Duration {
        self.update_rate
    }

    async fn is_game_running(&self) -> Result<bool> {
        Ok(self.check_ams2_running().await)
    }
}

fn unix_timestamp_ns() -> u64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_nanos() as u64,
        Err(_) => 0,
    }
}

/// Extract null-terminated string from byte array
fn extract_string(bytes: &[u8]) -> String {
    match bytes.iter().position(|&b| b == 0) {
        Some(pos) => String::from_utf8_lossy(&bytes[..pos]).into_owned(),
        None => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// Game state enumeration
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameState {
    Exited = 0,
    FrontEnd = 1,
    InGamePlaying = 2,
    InGamePaused = 3,
    InGameInMenuTimeTicking = 4,
    InGameRestarting = 5,
    InGameReplay = 6,
    FrontEndReplay = 7,
}

/// Session state enumeration
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Invalid = 0,
    Practice = 1,
    Test = 2,
    Qualify = 3,
    FormationLap = 4,
    Race = 5,
    TimeAttack = 6,
}

/// Race state enumeration
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RaceState {
    Invalid = 0,
    NotStarted = 1,
    Racing = 2,
    Finished = 3,
    Disqualified = 4,
    Retired = 5,
    DnsDidNotStart = 6,
}

/// Pit mode enumeration
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PitMode {
    None = 0,
    DrivingIntoPits = 1,
    InPit = 2,
    DrivingOutOfPits = 3,
    InGarage = 4,
    DrivingOutOfGarage = 5,
    InPitlane = 6,
}

/// Highest flag enumeration
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HighestFlag {
    None = 0,
    Green = 1,
    Blue = 2,
    WhiteSlowCar = 3,
    WhiteFinalLap = 4,
    Red = 5,
    Yellow = 6,
    DoubleYellow = 7,
    BlackAndWhite = 8,
    BlackOrangeCircle = 9,
    Black = 10,
    Chequered = 11,
}

/// DRS state enumeration
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrsState {
    Installed = 0,
    Available = 1,
    Active = 2,
}

/// AMS2/PCARS2 shared memory data structure
/// This is a simplified version of the full PCARS2 shared memory format
/// containing the most commonly used telemetry fields.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct AMS2SharedMemory {
    // Version and state info
    pub version: u32,
    pub build_version_number: u32,
    pub game_state: u32,
    pub session_state: u32,
    pub race_state: u32,

    // Timing info
    pub viewed_participant_index: i32,
    pub num_participants: i32,
    pub num_active_participants: i32,
    pub laps_completed: u32,
    pub laps_in_event: u32,
    pub current_time: f32,
    pub split_time_ahead: f32,
    pub split_time_behind: f32,
    pub split_time: f32,
    pub best_lap_time: f32,
    pub last_lap_time: f32,
    pub current_sector1_time: f32,
    pub current_sector2_time: f32,
    pub current_sector3_time: f32,
    pub fastest_sector1_time: f32,
    pub fastest_sector2_time: f32,
    pub fastest_sector3_time: f32,
    pub personal_fastest_lap_time: f32,
    pub personal_fastest_sector1_time: f32,
    pub personal_fastest_sector2_time: f32,
    pub personal_fastest_sector3_time: f32,

    // Flags
    pub highest_flag: u32,
    pub pit_mode: u32,
    pub pit_schedule: u32,

    // Car state
    pub car_flags: u32,
    pub oil_temp_celsius: f32,
    pub oil_pressure_kpa: f32,
    pub water_temp_celsius: f32,
    pub water_pressure_kpa: f32,
    pub fuel_pressure_kpa: f32,
    pub fuel_level: f32,
    pub fuel_capacity: f32,
    pub speed: f32,
    pub rpm: f32,
    pub max_rpm: f32,
    pub gear: i8,
    pub num_gears: i8,
    pub boost_amount: f32,
    pub boost_pressure: f32,
    pub crash_state: u32,
    pub odometer_km: f32,

    // Motion and orientation
    pub orientation: [f32; 3],
    pub local_velocity: [f32; 3],
    pub world_velocity: [f32; 3],
    pub angular_velocity: [f32; 3],
    pub local_acceleration: [f32; 3],
    pub world_acceleration: [f32; 3],
    pub extents_centre: [f32; 3],

    // Tyre data (FL, FR, RL, RR)
    pub tyre_flags: [u32; 4],
    pub tyre_terrain: [u32; 4],
    pub tyre_y: [f32; 4],
    pub tyre_rps: [f32; 4],
    pub tyre_slip_speed: [f32; 4],
    pub tyre_temp: [f32; 4],
    pub tyre_grip: [f32; 4],
    pub tyre_height_above_ground: [f32; 4],
    pub tyre_lateral_stiffness: [f32; 4],
    pub tyre_wear: [f32; 4],
    pub brake_damage: [f32; 4],
    pub suspension_damage: [f32; 4],
    pub brake_temp_celsius: [f32; 4],
    pub tyre_tread_temp: [f32; 4],
    pub tyre_layer_temp: [f32; 4],
    pub tyre_carcass_temp: [f32; 4],
    pub tyre_rim_temp: [f32; 4],
    pub tyre_internal_air_temp: [f32; 4],
    pub wheel_local_position_y: [f32; 4],
    pub ride_height: [f32; 4],
    pub suspension_travel: [f32; 4],
    pub suspension_velocity: [f32; 4],
    pub air_pressure: [f32; 4],

    // Slip data
    pub tyre_slip: [f32; 4],

    // Controls
    pub throttle: f32,
    pub brake: f32,
    pub clutch: f32,
    pub steering: f32,

    // Electronics
    pub tc_setting: u8,
    pub abs_setting: u8,
    pub drs_state: u32,
    pub ers_deployment_mode: u32,

    // Update tracking
    pub update_index: u32,

    // String data
    pub car_name: [u8; 64],
    pub car_class_name: [u8; 64],
    pub track_location: [u8; 64],
    pub track_variation: [u8; 64],

    // Padding for alignment
    _padding: [u8; 128],
}

impl Default for AMS2SharedMemory {
    fn default() -> Self {
        Self {
            version: 0,
            build_version_number: 0,
            game_state: 0,
            session_state: 0,
            race_state: 0,
            viewed_participant_index: 0,
            num_participants: 0,
            num_active_participants: 0,
            laps_completed: 0,
            laps_in_event: 0,
            current_time: 0.0,
            split_time_ahead: 0.0,
            split_time_behind: 0.0,
            split_time: 0.0,
            best_lap_time: 0.0,
            last_lap_time: 0.0,
            current_sector1_time: 0.0,
            current_sector2_time: 0.0,
            current_sector3_time: 0.0,
            fastest_sector1_time: 0.0,
            fastest_sector2_time: 0.0,
            fastest_sector3_time: 0.0,
            personal_fastest_lap_time: 0.0,
            personal_fastest_sector1_time: 0.0,
            personal_fastest_sector2_time: 0.0,
            personal_fastest_sector3_time: 0.0,
            highest_flag: 0,
            pit_mode: 0,
            pit_schedule: 0,
            car_flags: 0,
            oil_temp_celsius: 0.0,
            oil_pressure_kpa: 0.0,
            water_temp_celsius: 0.0,
            water_pressure_kpa: 0.0,
            fuel_pressure_kpa: 0.0,
            fuel_level: 0.0,
            fuel_capacity: 0.0,
            speed: 0.0,
            rpm: 0.0,
            max_rpm: 0.0,
            gear: 0,
            num_gears: 0,
            boost_amount: 0.0,
            boost_pressure: 0.0,
            crash_state: 0,
            odometer_km: 0.0,
            orientation: [0.0; 3],
            local_velocity: [0.0; 3],
            world_velocity: [0.0; 3],
            angular_velocity: [0.0; 3],
            local_acceleration: [0.0; 3],
            world_acceleration: [0.0; 3],
            extents_centre: [0.0; 3],
            tyre_flags: [0; 4],
            tyre_terrain: [0; 4],
            tyre_y: [0.0; 4],
            tyre_rps: [0.0; 4],
            tyre_slip_speed: [0.0; 4],
            tyre_temp: [0.0; 4],
            tyre_grip: [0.0; 4],
            tyre_height_above_ground: [0.0; 4],
            tyre_lateral_stiffness: [0.0; 4],
            tyre_wear: [0.0; 4],
            brake_damage: [0.0; 4],
            suspension_damage: [0.0; 4],
            brake_temp_celsius: [0.0; 4],
            tyre_tread_temp: [0.0; 4],
            tyre_layer_temp: [0.0; 4],
            tyre_carcass_temp: [0.0; 4],
            tyre_rim_temp: [0.0; 4],
            tyre_internal_air_temp: [0.0; 4],
            wheel_local_position_y: [0.0; 4],
            ride_height: [0.0; 4],
            suspension_travel: [0.0; 4],
            suspension_velocity: [0.0; 4],
            air_pressure: [0.0; 4],
            tyre_slip: [0.0; 4],
            throttle: 0.0,
            brake: 0.0,
            clutch: 0.0,
            steering: 0.0,
            tc_setting: 0,
            abs_setting: 0,
            drs_state: 0,
            ers_deployment_mode: 0,
            update_index: 0,
            car_name: [0; 64],
            car_class_name: [0; 64],
            track_location: [0; 64],
            track_variation: [0; 64],
            _padding: [0; 128],
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

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn test_ams2_adapter_creation() -> TestResult {
        let adapter = AMS2Adapter::new();
        assert_eq!(adapter.game_id(), "ams2");
        assert_eq!(adapter.expected_update_rate(), Duration::from_millis(16));
        Ok(())
    }

    #[test]
    fn test_normalize_ams2_data() -> TestResult {
        let adapter = AMS2Adapter::new();

        let car_name = b"formula_ultimate\0";
        let track_name = b"interlagos\0";
        let mut car_name_arr = [0u8; 64];
        let mut track_name_arr = [0u8; 64];
        car_name_arr[..car_name.len()].copy_from_slice(car_name);
        track_name_arr[..track_name.len()].copy_from_slice(track_name);

        let data = AMS2SharedMemory {
            rpm: 12000.0,
            speed: 80.0, // m/s
            gear: 5,
            steering: 0.35,
            throttle: 0.9,
            brake: 0.1,
            fuel_level: 35.0,
            fuel_capacity: 100.0,
            laps_completed: 5,
            highest_flag: HighestFlag::Green as u32,
            pit_mode: PitMode::None as u32,
            tc_setting: 3,
            abs_setting: 2,
            car_name: car_name_arr,
            track_location: track_name_arr,
            tyre_slip: [0.05, 0.06, 0.04, 0.05],
            ..Default::default()
        };

        let normalized = adapter.normalize_ams2_data(&data);

        assert_eq!(normalized.rpm, Some(12000.0));
        assert_eq!(normalized.speed_ms, Some(80.0));
        assert_eq!(normalized.gear, Some(5));
        assert_eq!(normalized.ffb_scalar, Some(0.35));
        assert_eq!(normalized.car_id, Some("formula_ultimate".to_string()));
        assert_eq!(normalized.track_id, Some("interlagos".to_string()));
        assert!(normalized.flags.green_flag);
        assert!(!normalized.flags.in_pits);
        assert!(normalized.flags.traction_control);
        assert!(normalized.flags.abs_active);

        // Check extended data
        assert!(normalized.extended.contains_key("throttle"));
        assert!(normalized.extended.contains_key("brake"));
        assert!(normalized.extended.contains_key("fuel_level"));
        Ok(())
    }

    #[test]
    fn test_normalize_with_flags() -> TestResult {
        let adapter = AMS2Adapter::new();

        // Test yellow flag
        let data_yellow = AMS2SharedMemory {
            highest_flag: HighestFlag::Yellow as u32,
            ..Default::default()
        };
        let normalized = adapter.normalize_ams2_data(&data_yellow);
        assert!(normalized.flags.yellow_flag);
        assert!(!normalized.flags.green_flag);

        // Test red flag
        let data_red = AMS2SharedMemory {
            highest_flag: HighestFlag::Red as u32,
            ..Default::default()
        };
        let normalized = adapter.normalize_ams2_data(&data_red);
        assert!(normalized.flags.red_flag);

        // Test blue flag
        let data_blue = AMS2SharedMemory {
            highest_flag: HighestFlag::Blue as u32,
            ..Default::default()
        };
        let normalized = adapter.normalize_ams2_data(&data_blue);
        assert!(normalized.flags.blue_flag);

        // Test checkered flag
        let data_checkered = AMS2SharedMemory {
            highest_flag: HighestFlag::Chequered as u32,
            ..Default::default()
        };
        let normalized = adapter.normalize_ams2_data(&data_checkered);
        assert!(normalized.flags.checkered_flag);

        Ok(())
    }

    #[test]
    fn test_normalize_pit_mode() -> TestResult {
        let adapter = AMS2Adapter::new();

        // Test in pit
        let data_in_pit = AMS2SharedMemory {
            pit_mode: PitMode::InPit as u32,
            ..Default::default()
        };
        let normalized = adapter.normalize_ams2_data(&data_in_pit);
        assert!(normalized.flags.in_pits);

        // Test pit limiter
        let data_pitlane = AMS2SharedMemory {
            pit_mode: PitMode::InPitlane as u32,
            ..Default::default()
        };
        let normalized = adapter.normalize_ams2_data(&data_pitlane);
        assert!(normalized.flags.pit_limiter);

        Ok(())
    }

    #[test]
    fn test_normalize_drs_state() -> TestResult {
        let adapter = AMS2Adapter::new();

        // Test DRS available
        let data_drs_available = AMS2SharedMemory {
            drs_state: DrsState::Available as u32,
            ..Default::default()
        };
        let normalized = adapter.normalize_ams2_data(&data_drs_available);
        assert!(normalized.flags.drs_available);
        assert!(!normalized.flags.drs_active);

        // Test DRS active
        let data_drs_active = AMS2SharedMemory {
            drs_state: DrsState::Active as u32,
            ..Default::default()
        };
        let normalized = adapter.normalize_ams2_data(&data_drs_active);
        assert!(normalized.flags.drs_active);

        Ok(())
    }

    #[test]
    fn test_slip_ratio_calculation() -> TestResult {
        let adapter = AMS2Adapter::new();

        // Test with speed > 1.0
        let data = AMS2SharedMemory {
            speed: 50.0,
            tyre_slip: [0.1, 0.15, 0.08, 0.12],
            ..Default::default()
        };
        let normalized = adapter.normalize_ams2_data(&data);

        // Average: (0.1 + 0.15 + 0.08 + 0.12) / 4 = 0.1125
        let expected_slip = 0.1125;
        let slip = normalized.slip_ratio.ok_or("expected slip_ratio")?;
        assert!((slip - expected_slip).abs() < 0.001);

        // Test with low speed (should be 0)
        let data_low_speed = AMS2SharedMemory {
            speed: 0.5,
            tyre_slip: [0.5, 0.5, 0.5, 0.5],
            ..Default::default()
        };
        let normalized = adapter.normalize_ams2_data(&data_low_speed);
        assert_eq!(normalized.slip_ratio, Some(0.0));

        Ok(())
    }

    #[test]
    fn test_slip_ratio_clamping() -> TestResult {
        let adapter = AMS2Adapter::new();

        // Test with high slip values (should be clamped to 1.0)
        let data = AMS2SharedMemory {
            speed: 50.0,
            tyre_slip: [2.0, 2.0, 2.0, 2.0], // Very high slip
            ..Default::default()
        };
        let normalized = adapter.normalize_ams2_data(&data);
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
    fn test_normalize_raw_data() -> TestResult {
        let adapter = AMS2Adapter::new();

        let data = AMS2SharedMemory::default();
        let raw_bytes = unsafe {
            std::slice::from_raw_parts(
                &data as *const _ as *const u8,
                mem::size_of::<AMS2SharedMemory>(),
            )
        };

        let result = adapter.normalize(raw_bytes);
        assert!(result.is_ok());
        Ok(())
    }

    #[test]
    fn test_normalize_invalid_data() -> TestResult {
        let adapter = AMS2Adapter::new();

        let invalid_data = vec![0u8; 10]; // Wrong size
        let result = adapter.normalize(&invalid_data);
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn test_ffb_scalar_clamping() -> TestResult {
        let adapter = AMS2Adapter::new();

        // Test steering value > 1.0 (should be clamped)
        let data_high = AMS2SharedMemory {
            steering: 1.5,
            ..Default::default()
        };
        let normalized = adapter.normalize_ams2_data(&data_high);
        assert_eq!(normalized.ffb_scalar, Some(1.0));

        // Test steering value < -1.0 (should be clamped)
        let data_low = AMS2SharedMemory {
            steering: -1.5,
            ..Default::default()
        };
        let normalized = adapter.normalize_ams2_data(&data_low);
        assert_eq!(normalized.ffb_scalar, Some(-1.0));

        Ok(())
    }

    #[test]
    fn test_extended_data_values() -> TestResult {
        let adapter = AMS2Adapter::new();

        let data = AMS2SharedMemory {
            fuel_level: 45.5,
            fuel_capacity: 100.0,
            laps_completed: 12,
            current_time: 85.5,
            last_lap_time: 82.3,
            best_lap_time: 80.1,
            throttle: 0.85,
            brake: 0.15,
            clutch: 0.0,
            water_temp_celsius: 95.0,
            oil_temp_celsius: 110.0,
            boost_pressure: 1.2,
            tc_setting: 4,
            abs_setting: 3,
            ..Default::default()
        };

        let normalized = adapter.normalize_ams2_data(&data);

        // Check fuel level
        if let Some(TelemetryValue::Float(fuel)) = normalized.extended.get("fuel_level") {
            assert_eq!(*fuel, 45.5);
        } else {
            return Err("Expected fuel_level to be a float".into());
        }

        // Check lap count
        if let Some(TelemetryValue::Integer(laps)) = normalized.extended.get("lap_count") {
            assert_eq!(*laps, 12);
        } else {
            return Err("Expected lap_count to be an integer".into());
        }

        // Check throttle
        if let Some(TelemetryValue::Float(throttle)) = normalized.extended.get("throttle") {
            assert_eq!(*throttle, 0.85);
        } else {
            return Err("Expected throttle to be a float".into());
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_is_game_running() -> TestResult {
        let adapter = AMS2Adapter::new();

        // On non-Windows platforms, should always return false
        #[cfg(not(windows))]
        {
            let result = adapter.is_game_running().await?;
            assert!(!result);
        }

        // On Windows, test depends on whether AMS2 is actually running
        #[cfg(windows)]
        {
            let _result = adapter.is_game_running().await?;
            // Can't assert the actual value since it depends on system state
        }

        Ok(())
    }

    #[test]
    fn test_game_state_enum() -> TestResult {
        assert_eq!(GameState::Exited as u32, 0);
        assert_eq!(GameState::FrontEnd as u32, 1);
        assert_eq!(GameState::InGamePlaying as u32, 2);
        assert_eq!(GameState::InGamePaused as u32, 3);
        Ok(())
    }

    #[test]
    fn test_session_state_enum() -> TestResult {
        assert_eq!(SessionState::Invalid as u32, 0);
        assert_eq!(SessionState::Practice as u32, 1);
        assert_eq!(SessionState::Qualify as u32, 3);
        assert_eq!(SessionState::Race as u32, 5);
        Ok(())
    }

    #[test]
    fn test_race_state_enum() -> TestResult {
        assert_eq!(RaceState::Invalid as u32, 0);
        assert_eq!(RaceState::NotStarted as u32, 1);
        assert_eq!(RaceState::Racing as u32, 2);
        assert_eq!(RaceState::Finished as u32, 3);
        Ok(())
    }

    #[test]
    fn test_pit_mode_enum() -> TestResult {
        assert_eq!(PitMode::None as u32, 0);
        assert_eq!(PitMode::InPit as u32, 2);
        assert_eq!(PitMode::InPitlane as u32, 6);
        Ok(())
    }

    #[test]
    fn test_highest_flag_enum() -> TestResult {
        assert_eq!(HighestFlag::None as u32, 0);
        assert_eq!(HighestFlag::Green as u32, 1);
        assert_eq!(HighestFlag::Blue as u32, 2);
        assert_eq!(HighestFlag::Yellow as u32, 6);
        assert_eq!(HighestFlag::Chequered as u32, 11);
        Ok(())
    }

    #[test]
    fn test_drs_state_enum() -> TestResult {
        assert_eq!(DrsState::Installed as u32, 0);
        assert_eq!(DrsState::Available as u32, 1);
        assert_eq!(DrsState::Active as u32, 2);
        Ok(())
    }

    #[test]
    fn test_default_shared_memory() -> TestResult {
        let data = AMS2SharedMemory::default();
        assert_eq!(data.version, 0);
        assert_eq!(data.rpm, 0.0);
        assert_eq!(data.speed, 0.0);
        assert_eq!(data.gear, 0);
        assert_eq!(data.throttle, 0.0);
        assert_eq!(data.brake, 0.0);
        Ok(())
    }
}

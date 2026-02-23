//! rFactor 2 telemetry adapter with shared memory interface
//!
//! Implements telemetry adapter for rFactor 2 using shared memory.
//! rFactor 2 exposes telemetry data through memory-mapped files.
//! Requirements: 12.4
#![cfg_attr(not(windows), allow(unused, dead_code))]

use crate::{
    NormalizedTelemetry, TelemetryAdapter, TelemetryFlags, TelemetryFrame, TelemetryReceiver,
    TelemetryValue, telemetry_now_ns,
};
use anyhow::Result;
use async_trait::async_trait;
use std::mem;
use std::ptr;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

#[cfg(windows)]
use winapi::um::{
    handleapi::{CloseHandle, INVALID_HANDLE_VALUE},
    memoryapi::{FILE_MAP_READ, MapViewOfFile, OpenFileMappingW, UnmapViewOfFile},
    tlhelp32::{
        CreateToolhelp32Snapshot, PROCESSENTRY32, Process32First, Process32Next, TH32CS_SNAPPROCESS,
    },
    winnt::HANDLE,
};

/// rFactor 2 shared memory name for telemetry data
const RF2_TELEMETRY_SHARED_MEMORY_NAME: &str = "$rFactor2SMMP_Telemetry$";

/// rFactor 2 shared memory name for scoring data
const RF2_SCORING_SHARED_MEMORY_NAME: &str = "$rFactor2SMMP_Scoring$";
/// rFactor 2 shared memory name for force feedback data
const RF2_FORCE_FEEDBACK_SHARED_MEMORY_NAME: &str = "$rFactor2SMMP_ForceFeedback$";
const RF2_PROCESS_NAME_PATTERNS: [&str; 2] = ["rfactor2.exe", "rfactor2 dedicated.exe"];

/// Maximum number of wheels per vehicle
const RF2_MAX_WHEELS: usize = 4;

/// rFactor 2 telemetry adapter using shared memory
pub struct RFactor2Adapter {
    update_rate: Duration,
    #[cfg(windows)]
    telemetry_memory: Option<TelemetryMemoryHandle>,
    #[cfg(windows)]
    scoring_memory: Option<ScoringMemoryHandle>,
    #[cfg(windows)]
    force_feedback_memory: Option<ForceFeedbackMemoryHandle>,
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
struct ForceFeedbackMemoryHandle {
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

#[cfg(windows)]
unsafe impl Send for ForceFeedbackMemoryHandle {}
#[cfg(windows)]
unsafe impl Sync for ForceFeedbackMemoryHandle {}

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
            #[cfg(windows)]
            force_feedback_memory: None,
        }
    }

    /// Initialize shared memory connection to rFactor 2 telemetry
    #[cfg(windows)]
    fn initialize_telemetry_memory(&mut self) -> Result<()> {
        let pid = detect_rfactor2_pid();
        let candidate_names = build_mapping_candidates(RF2_TELEMETRY_SHARED_MEMORY_NAME, pid);

        unsafe {
            let (handle, opened_name) =
                open_file_mapping_first(&candidate_names).ok_or_else(|| {
                    anyhow::anyhow!(
                        "Failed to open rFactor 2 telemetry shared memory. Tried {:?}",
                        candidate_names
                    )
                })?;

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

            info!(
                map_name = %opened_name,
                pid = ?pid,
                "Successfully connected to rFactor 2 telemetry shared memory"
            );
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
        let pid = detect_rfactor2_pid();
        let candidate_names = build_mapping_candidates(RF2_SCORING_SHARED_MEMORY_NAME, pid);

        unsafe {
            let (handle, opened_name) =
                open_file_mapping_first(&candidate_names).ok_or_else(|| {
                    anyhow::anyhow!(
                        "Failed to open rFactor 2 scoring shared memory. Tried {:?}",
                        candidate_names
                    )
                })?;

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

            info!(
                map_name = %opened_name,
                pid = ?pid,
                "Successfully connected to rFactor 2 scoring shared memory"
            );
            Ok(())
        }
    }

    #[cfg(not(windows))]
    fn initialize_scoring_memory(&mut self) -> Result<()> {
        Err(anyhow::anyhow!(
            "rFactor 2 shared memory only available on Windows"
        ))
    }

    /// Initialize shared memory connection to rFactor 2 force-feedback data
    #[cfg(windows)]
    fn initialize_force_feedback_memory(&mut self) -> Result<()> {
        let pid = detect_rfactor2_pid();
        let candidate_names = build_mapping_candidates(RF2_FORCE_FEEDBACK_SHARED_MEMORY_NAME, pid);

        unsafe {
            let (handle, opened_name) =
                open_file_mapping_first(&candidate_names).ok_or_else(|| {
                    anyhow::anyhow!(
                        "Failed to open rFactor 2 force-feedback shared memory. Tried {:?}",
                        candidate_names
                    )
                })?;

            let map_size = mem::size_of::<RF2ForceFeedback>();
            let base_ptr = MapViewOfFile(handle, FILE_MAP_READ, 0, 0, map_size) as *const u8;

            if base_ptr.is_null() {
                CloseHandle(handle);
                return Err(anyhow::anyhow!(
                    "Failed to map rFactor 2 force-feedback shared memory"
                ));
            }

            self.force_feedback_memory = Some(ForceFeedbackMemoryHandle {
                handle,
                base_ptr,
                size: map_size,
            });

            info!(
                map_name = %opened_name,
                pid = ?pid,
                "Successfully connected to rFactor 2 force-feedback shared memory"
            );
            Ok(())
        }
    }

    #[cfg(not(windows))]
    fn initialize_force_feedback_memory(&mut self) -> Result<()> {
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

    /// Read force-feedback data from shared memory
    #[cfg(windows)]
    fn read_force_feedback_data(&self) -> Result<RF2ForceFeedback> {
        let mem = self
            .force_feedback_memory
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Force-feedback shared memory not initialized"))?;

        Ok(read_force_feedback_stable(mem.base_ptr))
    }

    #[cfg(not(windows))]
    fn read_force_feedback_data(&self) -> Result<RF2ForceFeedback> {
        Err(anyhow::anyhow!(
            "rFactor 2 shared memory only available on Windows"
        ))
    }

    /// Check if rFactor 2 is running by attempting to open shared memory
    #[cfg(windows)]
    async fn check_rf2_running(&self) -> bool {
        let pid = detect_rfactor2_pid();
        let candidate_names = build_mapping_candidates(RF2_TELEMETRY_SHARED_MEMORY_NAME, pid);

        unsafe {
            if let Some((handle, _)) = open_file_mapping_first(&candidate_names) {
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
        force_feedback: Option<&RF2ForceFeedback>,
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

        let ffb_from_map = force_feedback.and_then(RF2ForceFeedback::stable_force_value);
        let (ffb_raw, ffb_source) = if let Some(force_value) = ffb_from_map {
            (force_value, "force_feedback_map")
        } else {
            (
                vehicle.steering_shaft_torque,
                "telemetry_steering_shaft_torque",
            )
        };
        let ffb_scalar = derive_ffb_scalar(ffb_raw);

        NormalizedTelemetry::default()
            .with_ffb_scalar(ffb_scalar)
            .with_rpm(vehicle.engine_rpm)
            .with_speed_ms(vehicle.speed)
            .with_slip_ratio(slip_ratio)
            .with_gear(vehicle.gear)
            .with_car_id(car_id)
            .with_track_id(track_id)
            .with_flags(flags)
            .with_extended(
                "fuel_level".to_string(),
                TelemetryValue::Float(vehicle.fuel),
            )
            .with_extended(
                "throttle".to_string(),
                TelemetryValue::Float(vehicle.unfiltered_throttle),
            )
            .with_extended(
                "brake".to_string(),
                TelemetryValue::Float(vehicle.unfiltered_brake),
            )
            .with_extended(
                "clutch".to_string(),
                TelemetryValue::Float(vehicle.unfiltered_clutch),
            )
            .with_extended(
                "steering".to_string(),
                TelemetryValue::Float(vehicle.unfiltered_steering),
            )
            .with_extended(
                "water_temp".to_string(),
                TelemetryValue::Float(vehicle.engine_water_temp),
            )
            .with_extended(
                "oil_temp".to_string(),
                TelemetryValue::Float(vehicle.engine_oil_temp),
            )
            .with_extended("ffb_raw".to_string(), TelemetryValue::Float(ffb_raw))
            .with_extended(
                "ffb_source".to_string(),
                TelemetryValue::String(ffb_source.to_string()),
            )
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

            // Try to initialize dedicated force-feedback map (optional).
            if let Err(e) = adapter.initialize_force_feedback_memory() {
                warn!(
                    "Failed to initialize rFactor 2 force-feedback shared memory: {}",
                    e
                );
            }

            info!("Started rFactor 2 telemetry monitoring");

            loop {
                match adapter.read_telemetry_data() {
                    Ok((header, vehicle)) => {
                        if header.update_index != last_update_index {
                            last_update_index = header.update_index;

                            let scoring = adapter.read_scoring_data().ok();
                            let force_feedback = adapter.read_force_feedback_data().ok();
                            let normalized = adapter.normalize_rf2_data(
                                &vehicle,
                                scoring.as_ref(),
                                force_feedback.as_ref(),
                            );
                            let raw_size = mem::size_of::<RF2VehicleTelemetry>()
                                + if force_feedback.is_some() {
                                    mem::size_of::<RF2ForceFeedback>()
                                } else {
                                    0
                                };

                            let frame = TelemetryFrame::new(
                                normalized,
                                telemetry_now_ns(),
                                sequence,
                                raw_size,
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
            unsafe { ptr::read_unaligned(raw.as_ptr() as *const RF2VehicleTelemetry) };

        Ok(self.normalize_rf2_data(&vehicle, None, None))
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

fn build_mapping_candidates(base_name: &str, pid: Option<u32>) -> Vec<String> {
    let mut candidates = Vec::with_capacity(2);
    if let Some(pid_value) = pid {
        candidates.push(format!("{base_name}{pid_value}"));
    }
    candidates.push(base_name.to_string());
    candidates
}

fn derive_ffb_scalar(ffb_raw: f32) -> f32 {
    if !ffb_raw.is_finite() {
        return 0.0;
    }

    if ffb_raw.abs() <= 1.5 {
        ffb_raw.clamp(-1.0, 1.0)
    } else {
        (ffb_raw / 50.0).clamp(-1.0, 1.0)
    }
}

fn is_rfactor2_process_name(process_name: &str) -> bool {
    let lowered = process_name.to_ascii_lowercase();
    RF2_PROCESS_NAME_PATTERNS
        .iter()
        .any(|pattern| lowered.contains(pattern))
}

#[cfg(windows)]
fn detect_rfactor2_pid() -> Option<u32> {
    use std::ffi::CStr;

    // SAFETY: Windows snapshot API requires unsafe FFI calls and pointer traversal.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return None;
        }

        let mut entry: PROCESSENTRY32 = mem::zeroed();
        entry.dwSize = mem::size_of::<PROCESSENTRY32>() as u32;

        let mut detected_pid = None;
        if Process32First(snapshot, &mut entry) != 0 {
            loop {
                let process_name = CStr::from_ptr(entry.szExeFile.as_ptr())
                    .to_string_lossy()
                    .into_owned();
                if is_rfactor2_process_name(&process_name) {
                    detected_pid = Some(entry.th32ProcessID);
                    break;
                }

                if Process32Next(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }

        CloseHandle(snapshot);
        detected_pid
    }
}

#[cfg(windows)]
fn to_wide_null_terminated(value: &str) -> Vec<u16> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    OsStr::new(value)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(windows)]
unsafe fn open_file_mapping_first(candidates: &[String]) -> Option<(HANDLE, String)> {
    for candidate in candidates {
        let wide_name = to_wide_null_terminated(candidate);
        // SAFETY: wide_name is null-terminated and lives for the duration of the call.
        let handle = unsafe { OpenFileMappingW(FILE_MAP_READ, 0, wide_name.as_ptr()) };
        if !handle.is_null() {
            return Some((handle, candidate.clone()));
        }
    }

    None
}

#[cfg(windows)]
fn read_force_feedback_stable(base_ptr: *const u8) -> RF2ForceFeedback {
    const STABLE_READ_ATTEMPTS: usize = 3;

    // SAFETY: caller provides a valid mapped view with at least RF2ForceFeedback bytes.
    unsafe {
        for _ in 0..STABLE_READ_ATTEMPTS {
            let sample = ptr::read_volatile(base_ptr as *const RF2ForceFeedback);
            if sample.version_update_begin == sample.version_update_end {
                return sample;
            }
        }

        ptr::read_volatile(base_ptr as *const RF2ForceFeedback)
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

/// rFactor 2 force-feedback shared-memory block.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct RF2ForceFeedback {
    /// Start version marker for stable reads.
    pub version_update_begin: u32,
    /// Force-feedback value from plugin output.
    pub force_value: f64,
    /// End version marker for stable reads.
    pub version_update_end: u32,
}

impl RF2ForceFeedback {
    fn stable_force_value(&self) -> Option<f32> {
        if self.version_update_begin == self.version_update_end && self.force_value.is_finite() {
            Some(self.force_value as f32)
        } else {
            None
        }
    }
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

#[cfg(windows)]
impl Drop for ForceFeedbackMemoryHandle {
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

        let normalized = adapter.normalize_rf2_data(&vehicle, Some(&scoring), None);

        assert_eq!(normalized.rpm, Some(8500.0));
        assert_eq!(normalized.speed_ms, Some(65.0));
        assert_eq!(normalized.gear, Some(4));
        assert!(normalized.ffb_scalar.is_some());
        assert_eq!(normalized.car_id, Some("formula_renault".to_string()));
        assert_eq!(normalized.track_id, Some("spa_francorchamps".to_string()));
        assert!(normalized.flags.green_flag);
        assert!(!normalized.flags.in_pits);
        assert_eq!(
            normalized.extended.get("fuel_level"),
            Some(&TelemetryValue::Float(42.0))
        );
        assert_eq!(
            normalized.extended.get("throttle"),
            Some(&TelemetryValue::Float(0.85))
        );
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
        let normalized = adapter.normalize_rf2_data(&vehicle, Some(&scoring_yellow), None);
        assert!(normalized.flags.yellow_flag);
        assert!(!normalized.flags.green_flag);

        // Test red flag
        let scoring_red = RF2ScoringHeader {
            game_phase: GamePhase::RedFlag as i32,
            ..Default::default()
        };
        let normalized = adapter.normalize_rf2_data(&vehicle, Some(&scoring_red), None);
        assert!(normalized.flags.red_flag);

        // Test checkered flag
        let scoring_checkered = RF2ScoringHeader {
            game_phase: GamePhase::Checkered as i32,
            ..Default::default()
        };
        let normalized = adapter.normalize_rf2_data(&vehicle, Some(&scoring_checkered), None);
        assert!(normalized.flags.checkered_flag);

        // Test in pits
        let scoring_pits = RF2ScoringHeader {
            in_pits: 1,
            ..Default::default()
        };
        let normalized = adapter.normalize_rf2_data(&vehicle, Some(&scoring_pits), None);
        assert!(normalized.flags.in_pits);

        Ok(())
    }

    #[test]
    fn test_slip_ratio_calculation() -> TestResult {
        let adapter = RFactor2Adapter::new();

        // Test with speed > 1.0
        let mut vehicle = RF2VehicleTelemetry {
            speed: 50.0,
            ..Default::default()
        };
        vehicle.wheels[0].lateral_patch_slip = 0.1;
        vehicle.wheels[1].lateral_patch_slip = 0.15;
        vehicle.wheels[2].lateral_patch_slip = 0.08;
        vehicle.wheels[3].lateral_patch_slip = 0.12;

        let normalized = adapter.normalize_rf2_data(&vehicle, None, None);

        // Average: (0.1 + 0.15 + 0.08 + 0.12) / 4 = 0.1125
        let expected_slip = 0.1125;
        let slip = normalized.slip_ratio.ok_or("expected slip_ratio")?;
        assert!((slip - expected_slip).abs() < 0.001);

        // Test with low speed (should be 0)
        let mut vehicle_low_speed = RF2VehicleTelemetry {
            speed: 0.5,
            ..Default::default()
        };
        vehicle_low_speed.wheels[0].lateral_patch_slip = 0.5;

        let normalized = adapter.normalize_rf2_data(&vehicle_low_speed, None, None);
        assert_eq!(normalized.slip_ratio, Some(0.0));

        Ok(())
    }

    #[test]
    fn test_slip_ratio_clamping() -> TestResult {
        let adapter = RFactor2Adapter::new();

        let mut vehicle = RF2VehicleTelemetry {
            speed: 50.0,
            ..Default::default()
        };
        vehicle.wheels[0].lateral_patch_slip = 2.0;
        vehicle.wheels[1].lateral_patch_slip = 2.0;
        vehicle.wheels[2].lateral_patch_slip = 2.0;
        vehicle.wheels[3].lateral_patch_slip = 2.0;

        let normalized = adapter.normalize_rf2_data(&vehicle, None, None);
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
        let mut vehicle = RF2VehicleTelemetry {
            steering_shaft_torque: 25.0, // 25 Nm
            ..Default::default()
        };

        let normalized = adapter.normalize_rf2_data(&vehicle, None, None);
        // 25.0 / 50.0 = 0.5
        let ffb = normalized.ffb_scalar.ok_or("expected ffb_scalar")?;
        assert!((ffb - 0.5).abs() < 0.01);

        // Test high steering torque (should be clamped)
        vehicle.steering_shaft_torque = 100.0;
        let normalized = adapter.normalize_rf2_data(&vehicle, None, None);
        assert_eq!(normalized.ffb_scalar, Some(1.0));

        // Test negative steering torque
        vehicle.steering_shaft_torque = -75.0;
        let normalized = adapter.normalize_rf2_data(&vehicle, None, None);
        assert_eq!(normalized.ffb_scalar, Some(-1.0));

        Ok(())
    }

    #[test]
    fn test_build_mapping_candidates_with_pid() -> TestResult {
        let candidates = build_mapping_candidates(RF2_TELEMETRY_SHARED_MEMORY_NAME, Some(4242));
        assert_eq!(
            candidates,
            vec![
                "$rFactor2SMMP_Telemetry$4242".to_string(),
                "$rFactor2SMMP_Telemetry$".to_string()
            ]
        );
        Ok(())
    }

    #[test]
    fn test_build_mapping_candidates_without_pid() -> TestResult {
        let candidates = build_mapping_candidates(RF2_SCORING_SHARED_MEMORY_NAME, None);
        assert_eq!(candidates, vec!["$rFactor2SMMP_Scoring$".to_string()]);
        Ok(())
    }

    #[test]
    fn test_rfactor_process_name_matching() -> TestResult {
        assert!(is_rfactor2_process_name("rFactor2.exe"));
        assert!(is_rfactor2_process_name("RFACTOR2 DEDICATED.EXE"));
        assert!(!is_rfactor2_process_name("notepad.exe"));
        Ok(())
    }

    #[test]
    fn test_derive_ffb_scalar() -> TestResult {
        assert!((derive_ffb_scalar(0.5) - 0.5).abs() < 0.001);
        assert_eq!(derive_ffb_scalar(120.0), 1.0);
        assert_eq!(derive_ffb_scalar(-120.0), -1.0);
        Ok(())
    }

    #[test]
    fn test_force_feedback_preferred_over_telemetry_torque() -> TestResult {
        let adapter = RFactor2Adapter::new();
        let vehicle = RF2VehicleTelemetry {
            steering_shaft_torque: 25.0,
            ..Default::default()
        };
        let force_feedback = RF2ForceFeedback {
            version_update_begin: 7,
            force_value: 0.35,
            version_update_end: 7,
        };

        let normalized = adapter.normalize_rf2_data(&vehicle, None, Some(&force_feedback));
        assert_eq!(normalized.ffb_scalar, Some(0.35));
        assert_eq!(
            normalized.extended.get("ffb_source"),
            Some(&TelemetryValue::String("force_feedback_map".to_string()))
        );
        Ok(())
    }

    #[test]
    fn test_force_feedback_falls_back_when_unstable() -> TestResult {
        let adapter = RFactor2Adapter::new();
        let vehicle = RF2VehicleTelemetry {
            steering_shaft_torque: 25.0,
            ..Default::default()
        };
        let force_feedback = RF2ForceFeedback {
            version_update_begin: 7,
            force_value: 0.35,
            version_update_end: 8,
        };

        let normalized = adapter.normalize_rf2_data(&vehicle, None, Some(&force_feedback));
        let ffb_scalar = normalized.ffb_scalar.ok_or("expected ffb scalar")?;
        assert!((ffb_scalar - 0.5).abs() < 0.01);
        assert_eq!(
            normalized.extended.get("ffb_source"),
            Some(&TelemetryValue::String(
                "telemetry_steering_shaft_torque".to_string()
            ))
        );
        Ok(())
    }

    #[test]
    fn test_normalize_without_scoring() -> TestResult {
        let adapter = RFactor2Adapter::new();

        let vehicle = RF2VehicleTelemetry {
            engine_rpm: 6000.0,
            speed: 40.0,
            gear: 3,
            ..Default::default()
        };

        let normalized = adapter.normalize_rf2_data(&vehicle, None, None);

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

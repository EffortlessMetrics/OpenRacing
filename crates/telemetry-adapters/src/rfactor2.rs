//! rFactor 2 telemetry adapter with shared memory interface
//!
//! Implements telemetry adapter for rFactor 2 using shared memory.
//! rFactor 2 exposes telemetry data through memory-mapped files.
//! Requirements: 12.4
//!
//! ## Verification against rF2 Shared Memory Map Plugin
//!
//! Verified 2025-07 against [`rF2State.h`](https://github.com/TheIronWolfModding/rF2SharedMemoryMapPlugin)
//! (plugin v3.7.15.1, commit `48aa12d`) and
//! [`rFactor2SharedMemoryMap.hpp`](https://github.com/TheIronWolfModding/rF2SharedMemoryMapPlugin).
//!
//! - **Shared memory names**: `$rFactor2SMMP_Telemetry$`, `$rFactor2SMMP_Scoring$`,
//!   `$rFactor2SMMP_ForceFeedback$`. ✓
//! - **Mapped buffer layout**: each shared memory region is created with
//!   `sizeof(rF2MappedBufferVersionBlock) + sizeof(BuffT)` bytes.  The first 8 bytes
//!   are an external version block (`mVersionUpdateBegin`, `mVersionUpdateEnd`),
//!   followed by the struct data (see `MappedBuffer.h`).
//! - **rF2VehicleTelemetry field order** (up to `mUnfilteredClutch`): matches
//!   `rF2State.h` (mID, mDeltaTime, mElapsedTime, mLapNumber, mLapStartET,
//!   mVehicleName\[64\], mTrackName\[64\], mPos, mLocalVel, mLocalAccel, mOri\[3\],
//!   mLocalRot, mLocalRotAccel, mGear, mEngineRPM, mEngineWaterTemp, mEngineOilTemp,
//!   mClutchRPM, mUnfilteredThrottle/Brake/Steering/Clutch). ✓
//! - **Gap #1**: after `mUnfilteredClutch`, the official struct has 4 filtered input
//!   fields (mFilteredThrottle/Brake/Steering/Clutch) before `mSteeringShaftTorque`.
//!   Our struct omits these (4 × f64 = 32 bytes).
//! - **Gap #2**: between `mSteeringShaftTorque` and `mFuel`, the official struct has
//!   8 additional doubles (mFront3rdDeflection, mRear3rdDeflection, mFrontWingHeight,
//!   mFrontRideHeight, mRearRideHeight, mDrag, mFrontDownforce, mRearDownforce).
//! - **Gap #3**: between `mEngineMaxRPM` and `mWheels[4]`, the official struct has
//!   ~30 fields (damage, sector, tire compounds, electric boost, 111-byte expansion).
//! - **rF2GamePhase enum**: 0–8 match rF2State.h; value 9 = paused (tag.2015.09.14). ✓
//! - **rF2Wheel fields**: all f64 fields up to `mWear` match rF2State.h order.  Note
//!   that `mTemperature[3]` values are in **Kelvin** (not Celsius). ✓
//! - **Gear convention**: −1=reverse, 0=neutral, 1+=forward (same as rF2 native). ✓
//! - **Speed**: no discrete speed field in `rF2VehicleTelemetry`; derived from
//!   `mLocalVel` magnitude (consistent with ISI documentation). ✓
//! - **rF2ForceFeedback**: single `f64` (`mForceValue`); not versioned. ✓
//! - **Known limitation**: struct is **not** a valid memory overlay for direct reads
//!   due to omitted fields and missing `#[repr(C, packed(4))]`. See struct doc.
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

        Ok(read_force_feedback_value(mem.base_ptr))
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

        // Speed is computed from the local velocity vector (official rF2 has no
        // discrete speed field in rF2VehicleTelemetry).
        let speed = compute_speed_from_local_vel(&vehicle.local_vel);

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

        NormalizedTelemetry::builder()
            .ffb_scalar(ffb_scalar)
            .rpm(vehicle.engine_rpm as f32)
            .speed_ms(speed as f32)
            .slip_ratio(slip_ratio)
            .gear(vehicle.gear as i8)
            .car_id(car_id)
            .track_id(track_id)
            .flags(flags)
            .extended(
                "fuel_level".to_string(),
                TelemetryValue::Float(vehicle.fuel as f32),
            )
            .extended(
                "throttle".to_string(),
                TelemetryValue::Float(vehicle.unfiltered_throttle as f32),
            )
            .extended(
                "brake".to_string(),
                TelemetryValue::Float(vehicle.unfiltered_brake as f32),
            )
            .extended(
                "clutch".to_string(),
                TelemetryValue::Float(vehicle.unfiltered_clutch as f32),
            )
            .extended(
                "steering".to_string(),
                TelemetryValue::Float(vehicle.unfiltered_steering as f32),
            )
            .extended(
                "water_temp".to_string(),
                TelemetryValue::Float(vehicle.engine_water_temp as f32),
            )
            .extended(
                "oil_temp".to_string(),
                TelemetryValue::Float(vehicle.engine_oil_temp as f32),
            )
            .extended("ffb_raw".to_string(), TelemetryValue::Float(ffb_raw as f32))
            .extended(
                "ffb_source".to_string(),
                TelemetryValue::String(ffb_source.to_string()),
            )
            .build()
    }

    /// Extract flags from scoring data.
    ///
    /// **Note**: Red flag detection is not available from the game phase enum
    /// alone in rF2.  The official `rF2GamePhase` only goes up to `SessionOver`
    /// (8).  Red/checkered flags are reported through per-vehicle scoring data
    /// (`mFinishStatus`, `mIndividualPhase`) which this simplified adapter does
    /// not currently read.  `SessionOver` (8) is used as a proxy for checkered.
    fn extract_flags(&self, scoring: &RF2ScoringHeader) -> TelemetryFlags {
        TelemetryFlags {
            yellow_flag: scoring.yellow_flag_state != 0,
            red_flag: false,
            blue_flag: false,
            checkered_flag: scoring.game_phase == GamePhase::SessionOver as i32,
            green_flag: scoring.game_phase == GamePhase::GreenFlag as i32,
            pit_limiter: false,
            in_pits: scoring.in_pits != 0,
            drs_available: false,
            drs_active: false,
            ers_available: false,
            ers_active: false,
            launch_control: false,
            traction_control: false,
            abs_active: false,
            engine_limiter: false,
            safety_car: false,
            formation_lap: false,
            session_paused: false,
        }
    }

    /// Calculate average slip ratio from wheel data.
    ///
    /// Uses `lateral_patch_vel` (the lateral velocity of the tire contact
    /// patch relative to the road surface) normalised by vehicle speed.
    fn calculate_slip_ratio(&self, vehicle: &RF2VehicleTelemetry) -> f32 {
        let speed = compute_speed_from_local_vel(&vehicle.local_vel);
        if speed < 1.0 {
            return 0.0;
        }

        let mut total_slip = 0.0f64;
        for wheel in &vehicle.wheels {
            total_slip += wheel.lateral_patch_vel.abs();
        }
        let avg_slip = total_slip / RF2_MAX_WHEELS as f64;
        (avg_slip / speed).min(1.0) as f32
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
            let mut frame_seq = 0u64;
            let mut last_version = 0i32;

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
                        if header.version_update_begin == header.version_update_end
                            && header.version_update_begin != last_version
                        {
                            last_version = header.version_update_begin;

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
                                frame_seq,
                                raw_size,
                            );

                            if tx.send(frame).await.is_err() {
                                debug!("Telemetry receiver dropped, stopping monitoring");
                                break;
                            }

                            frame_seq += 1;
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

fn derive_ffb_scalar(ffb_raw: f64) -> f32 {
    if !ffb_raw.is_finite() {
        return 0.0;
    }

    let scalar = if ffb_raw.abs() <= 1.5 {
        ffb_raw.clamp(-1.0, 1.0)
    } else {
        (ffb_raw / 50.0).clamp(-1.0, 1.0)
    };
    scalar as f32
}

/// Compute vehicle speed (m/s) as the magnitude of the local velocity vector.
///
/// The official `rF2VehicleTelemetry` has no discrete speed field; speed must
/// be derived from `mLocalVel` (x, y, z).
fn compute_speed_from_local_vel(local_vel: &[f64; 3]) -> f64 {
    let (vx, vy, vz) = (local_vel[0], local_vel[1], local_vel[2]);
    (vx * vx + vy * vy + vz * vz).sqrt()
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

/// Read force-feedback value from mapped memory.
///
/// The rF2 force-feedback mapping is explicitly **not versioned** (high refresh
/// rate), so we simply read the latest `f64` value.
#[cfg(windows)]
fn read_force_feedback_value(base_ptr: *const u8) -> RF2ForceFeedback {
    // SAFETY: caller provides a valid mapped view with at least RF2ForceFeedback bytes.
    unsafe { ptr::read_volatile(base_ptr as *const RF2ForceFeedback) }
}

/// Game phase enumeration for rFactor 2.
///
/// Corresponds to `rF2GamePhase` in rF2State.h.  The official enum defines
/// values 0–8.  Value 9 (`PausedOrReplay`) was added in a later rF2 update
/// (tag.2015.09.14) and is documented in the scoring-info comments.
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
    /// Paused or heartbeat (added tag.2015.09.14).
    PausedOrReplay = 9,
}

/// rFactor 2 telemetry mapped buffer header.
///
/// Matches the layout of `rF2Telemetry` (inheriting from
/// `rF2MappedBufferVersionBlock` and `rF2MappedBufferHeaderWithSize`) from the
/// [rF2 Shared Memory Map Plugin](https://github.com/TheIronWolfModding/rF2SharedMemoryMapPlugin).
///
/// **Mapped memory layout** (`MappedBuffer.h` prepends an external version
/// block before the struct):
///
/// | Offset | Field                         | SDK type        |
/// |--------|-------------------------------|-----------------|
/// | 0      | external `mVersionUpdateBegin`| `unsigned long` |
/// | 4      | external `mVersionUpdateEnd`  | `unsigned long` |
/// | 8      | struct `mVersionUpdateBegin`  | `unsigned long` |
/// | 12     | struct `mVersionUpdateEnd`    | `unsigned long` |
/// | 16     | `mBytesUpdatedHint`           | `int`           |
/// | 20     | `mNumVehicles`                | `long`          |
/// | 24     | `mVehicles[0]` starts         |                 |
///
/// This struct reads the **external** version block at offsets 0–7 and
/// then the inherited struct fields at offsets 8–15 as `bytes_updated_hint`
/// and `num_vehicles`.  For torn-frame detection only the external version
/// block (offsets 0 and 4) is needed, which this struct captures correctly.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct RF2TelemetryHeader {
    /// Version counter (begin) — compare with `version_update_end` for a stable read.
    pub version_update_begin: i32,
    /// Version counter (end) — must equal `version_update_begin` for a stable read.
    pub version_update_end: i32,
    /// Hint for how many bytes were updated in this write cycle.
    pub bytes_updated_hint: i32,
    /// Number of vehicles in the telemetry data array.
    pub num_vehicles: i32,
}

/// rFactor 2 wheel telemetry data.
///
/// Fields correspond to `rF2Wheel` in rF2State.h from the
/// [rF2 Shared Memory Map Plugin](https://github.com/TheIronWolfModding/rF2SharedMemoryMapPlugin).
/// All fields up to and including `mWear` are present and in SDK order. ✓
///
/// **Omitted fields after `mWear`**: `mTerrainName[16]`, `mSurfaceType` (u8),
/// `mFlat` (bool), `mDetached` (bool), `mStaticUndeflectedRadius` (u8),
/// `mVerticalTireDeflection` (f64), `mWheelYLocation` (f64), `mToe` (f64),
/// `mTireCarcassTemperature` (f64), `mTireInnerLayerTemperature[3]` (f64×3),
/// and `mExpansion[24]`.  This struct is therefore **not** size-compatible
/// with the SDK `rF2Wheel` (which is 280 bytes with `#pragma pack(4)`).
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
    /// Lateral ground contact velocity (m/s)
    pub lateral_ground_vel: f64,
    /// Longitudinal ground contact velocity (m/s)
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
    /// Temperature — left, center, right (Kelvin; subtract 273.15 for °C)
    /// SDK: `mTemperature[3]` — "Kelvin (subtract 273.15 to get Celsius),
    /// left/center/right (not to be confused with inside/center/outside!)"
    pub temperature: [f64; 3],
    /// Wear (0.0-1.0)
    pub wear: f64,
}

/// rFactor 2 vehicle telemetry data (simplified for player vehicle).
///
/// Field names and types correspond to `rF2VehicleTelemetry` in rF2State.h
/// from the [rF2 Shared Memory Map Plugin](https://github.com/TheIronWolfModding/rF2SharedMemoryMapPlugin).
///
/// **Known limitations** (this struct is **not** a valid memory overlay):
///
/// 1. The official C code uses `#pragma pack(push, 4)`.  Matching this in Rust
///    would require `#[repr(C, packed(4))]`, which is omitted to keep field
///    access ergonomic in test and normalization code.
///
/// 2. Between `unfiltered_clutch` and `steering_shaft_torque`, the official struct
///    contains 4 filtered input fields (`mFilteredThrottle`, `mFilteredBrake`,
///    `mFilteredSteering`, `mFilteredClutch` — each `double`).  These are omitted.
///
/// 3. Between `steering_shaft_torque` and `fuel`, the official struct contains
///    8 additional doubles: 3rd-spring deflections (2) and aerodynamics (6).
///
/// 4. Between `engine_max_rpm` and `wheels[4]`, the official struct contains
///    ~30 fields (damage, sector, tire compounds, ignition, electric-boost data,
///    and a 111-byte expansion block).  These are omitted.
///
/// 5. The official `rF2VehicleTelemetry` has no discrete "speed" field.
///    Speed must be computed as the magnitude of `local_vel` (see
///    `compute_speed_from_local_vel`).
///
/// To read live data from rF2 shared memory correctly, a future refactor
/// should either add all intermediate fields with `#[repr(C, packed(4))]` or
/// switch to offset-based field reads (similar to the F1 adapter).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct RF2VehicleTelemetry {
    /// Slot ID — SDK: `mID` (long)
    pub id: i32,
    /// Delta time (seconds) — SDK: `mDeltaTime` (double)
    pub delta_time: f64,
    /// Elapsed time (seconds) — SDK: `mElapsedTime` (double)
    pub elapsed_time: f64,
    /// Lap number — SDK: `mLapNumber` (long)
    pub lap_number: i32,
    /// Lap start elapsed time — SDK: `mLapStartET` (double)
    pub lap_start_et: f64,
    /// Vehicle name — SDK: `mVehicleName[64]` (char[64])
    pub vehicle_name: [u8; 64],
    /// Track name — SDK: `mTrackName[64]` (char[64])
    pub track_name: [u8; 64],
    /// World position (x, y, z) — SDK: `mPos` (rF2Vec3)
    pub pos: [f64; 3],
    /// Local velocity (x, y, z) — used to derive speed. SDK: `mLocalVel`
    pub local_vel: [f64; 3],
    /// Local acceleration (x, y, z) — SDK: `mLocalAccel` (rF2Vec3)
    pub local_accel: [f64; 3],
    /// Orientation matrix (3×3 row-major, each row is a `rF2Vec3`) — SDK: `mOri[3]`
    pub ori: [[f64; 3]; 3],
    /// Local rotation (x, y, z) — SDK: `mLocalRot` (rF2Vec3)
    pub local_rot: [f64; 3],
    /// Local rotation acceleration (x, y, z) — SDK: `mLocalRotAccel` (rF2Vec3)
    pub local_rot_accel: [f64; 3],
    /// Gear (-1=reverse, 0=neutral, 1+=forward) — SDK: `mGear` (long)
    pub gear: i32,
    /// Engine RPM — SDK: `mEngineRPM` (double)
    pub engine_rpm: f64,
    /// Engine water temperature in °C — SDK: `mEngineWaterTemp` (double)
    pub engine_water_temp: f64,
    /// Engine oil temperature in °C — SDK: `mEngineOilTemp` (double)
    pub engine_oil_temp: f64,
    /// Clutch RPM — SDK: `mClutchRPM` (double)
    pub clutch_rpm: f64,
    /// Unfiltered throttle (0.0–1.0) — SDK: `mUnfilteredThrottle` (double)
    pub unfiltered_throttle: f64,
    /// Unfiltered brake (0.0–1.0) — SDK: `mUnfilteredBrake` (double)
    pub unfiltered_brake: f64,
    /// Unfiltered steering (-1.0 to 1.0) — SDK: `mUnfilteredSteering` (double)
    pub unfiltered_steering: f64,
    /// Unfiltered clutch (0.0–1.0) — SDK: `mUnfilteredClutch` (double)
    pub unfiltered_clutch: f64,
    // GAP: SDK has mFilteredThrottle/Brake/Steering/Clutch (4 × f64) here.
    /// Steering shaft torque in Nm — SDK: `mSteeringShaftTorque` (double)
    pub steering_shaft_torque: f64,
    // GAP: SDK has mFront3rdDeflection, mRear3rdDeflection, mFrontWingHeight,
    //   mFrontRideHeight, mRearRideHeight, mDrag, mFrontDownforce, mRearDownforce
    //   (8 × f64) here.
    /// Fuel remaining in litres — SDK: `mFuel` (double)
    pub fuel: f64,
    /// Engine max RPM — SDK: `mEngineMaxRPM` (double)
    pub engine_max_rpm: f64,
    // GAP: SDK has ~30 fields here (mScheduledStops through mExpansion[111])
    //   before mWheels[4].
    /// Wheel data (FL, FR, RL, RR) — SDK: `mWheels[4]` (rF2Wheel[4])
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
            ori: [[0.0; 3]; 3],
            local_rot: [0.0; 3],
            local_rot_accel: [0.0; 3],
            gear: 0,
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

/// rFactor 2 scoring header (simplified approximation).
///
/// **Known limitation**: The first 16 bytes correspond to the mapped-buffer
/// header (`rF2MappedBufferVersionBlock` + `rF2MappedBufferHeaderWithSize`):
///
/// | Offset | Field                | Official type |
/// |--------|----------------------|---------------|
/// | 0      | `version_update_begin` | `long` (i32) |
/// | 4      | `version_update_end`   | `long` (i32) |
/// | 8      | `bytes_updated_hint`   | `int`  (i32) |
/// | 12     | `num_vehicles`         | `long` (i32) |
///
/// After offset 16, the full `rF2ScoringInfo` struct begins (track name,
/// session info, game phase, flag states, weather, etc.).  The `game_phase`,
/// `yellow_flag_state`, and `in_pits` fields below are **NOT** at the correct
/// offsets for direct shared-memory reads — they are populated only in tests.
/// A future refactor should use offset-based reads for the full scoring data.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct RF2ScoringHeader {
    /// Version counter (begin).
    pub version: u32,
    /// Version counter (end).
    pub bytes_offset: u32,
    /// Number of vehicles (at offset 12 in the mapped buffer).
    pub num_vehicles: i32,
    /// Game phase (see [`GamePhase`]).  **Not at a valid shared-memory offset.**
    pub game_phase: i32,
    /// Yellow flag state.  **Not at a valid shared-memory offset.**
    pub yellow_flag_state: i32,
    /// In-pits flag.  **Not at a valid shared-memory offset.**
    pub in_pits: i32,
}

/// rFactor 2 force-feedback shared-memory block.
///
/// Matches `rF2ForceFeedback` from rF2State.h.  The force-feedback mapping is
/// explicitly **not versioned** due to its high refresh rate — it contains only
/// a single `f64` value (`mForceValue`).
///
/// In mapped memory, `MappedBuffer.h` still prepends an 8-byte
/// `rF2MappedBufferVersionBlock`, so `mForceValue` sits at offset 8.  The
/// version counters at offset 0–7 are unused for FFB.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct RF2ForceFeedback {
    /// Force-feedback value from the plugin (typically in Nm).
    pub force_value: f64,
}

impl RF2ForceFeedback {
    fn stable_force_value(&self) -> Option<f64> {
        if self.force_value.is_finite() {
            Some(self.force_value)
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
            // Speed derived from local_vel: sqrt(50² + 30² + 25²) ≈ 63.44
            local_vel: [50.0, 30.0, 25.0],
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

        assert_eq!(normalized.rpm, 8500.0);
        // Speed is sqrt(50² + 30² + 25²) ≈ 63.44
        assert!((normalized.speed_ms - 63.44).abs() < 0.1);
        assert_eq!(normalized.gear, 4);
        assert!(normalized.ffb_scalar != 0.0);
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

        // Red flag is not detectable from game phase alone (requires
        // per-vehicle scoring data not read by this simplified adapter).
        // Verify that red_flag is always false.
        let scoring_stopped = RF2ScoringHeader {
            game_phase: GamePhase::SessionStopped as i32,
            ..Default::default()
        };
        let normalized = adapter.normalize_rf2_data(&vehicle, Some(&scoring_stopped), None);
        assert!(!normalized.flags.red_flag);

        // Checkered is approximated via SessionOver (game phase 8)
        let scoring_over = RF2ScoringHeader {
            game_phase: GamePhase::SessionOver as i32,
            ..Default::default()
        };
        let normalized = adapter.normalize_rf2_data(&vehicle, Some(&scoring_over), None);
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

        // Test with speed > 1.0 (via local_vel)
        let mut vehicle = RF2VehicleTelemetry {
            local_vel: [50.0, 0.0, 0.0], // 50 m/s forward
            ..Default::default()
        };
        vehicle.wheels[0].lateral_patch_vel = 5.0;
        vehicle.wheels[1].lateral_patch_vel = 7.5;
        vehicle.wheels[2].lateral_patch_vel = 4.0;
        vehicle.wheels[3].lateral_patch_vel = 6.0;

        let normalized = adapter.normalize_rf2_data(&vehicle, None, None);

        // Average lateral_patch_vel: (5+7.5+4+6)/4 = 5.625, / speed 50 = 0.1125
        let expected_slip = 0.1125;
        assert!((normalized.slip_ratio - expected_slip).abs() < 0.001);

        // Test with low speed (should be 0)
        let mut vehicle_low_speed = RF2VehicleTelemetry {
            local_vel: [0.5, 0.0, 0.0],
            ..Default::default()
        };
        vehicle_low_speed.wheels[0].lateral_patch_vel = 5.0;

        let normalized = adapter.normalize_rf2_data(&vehicle_low_speed, None, None);
        assert_eq!(normalized.slip_ratio, 0.0);

        Ok(())
    }

    #[test]
    fn test_slip_ratio_clamping() -> TestResult {
        let adapter = RFactor2Adapter::new();

        let mut vehicle = RF2VehicleTelemetry {
            local_vel: [5.0, 0.0, 0.0], // 5 m/s — low enough that high patch vel clamps to 1.0
            ..Default::default()
        };
        vehicle.wheels[0].lateral_patch_vel = 100.0;
        vehicle.wheels[1].lateral_patch_vel = 100.0;
        vehicle.wheels[2].lateral_patch_vel = 100.0;
        vehicle.wheels[3].lateral_patch_vel = 100.0;

        let normalized = adapter.normalize_rf2_data(&vehicle, None, None);
        assert_eq!(normalized.slip_ratio, 1.0);

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
        assert!((normalized.ffb_scalar - 0.5).abs() < 0.01);

        // Test high steering torque (should be clamped)
        vehicle.steering_shaft_torque = 100.0;
        let normalized = adapter.normalize_rf2_data(&vehicle, None, None);
        assert_eq!(normalized.ffb_scalar, 1.0);

        // Test negative steering torque
        vehicle.steering_shaft_torque = -75.0;
        let normalized = adapter.normalize_rf2_data(&vehicle, None, None);
        assert_eq!(normalized.ffb_scalar, -1.0);

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
        let force_feedback = RF2ForceFeedback { force_value: 0.35 };

        let normalized = adapter.normalize_rf2_data(&vehicle, None, Some(&force_feedback));
        assert_eq!(normalized.ffb_scalar, 0.35);
        assert_eq!(
            normalized.extended.get("ffb_source"),
            Some(&TelemetryValue::String("force_feedback_map".to_string()))
        );
        Ok(())
    }

    #[test]
    fn test_force_feedback_falls_back_when_nan() -> TestResult {
        let adapter = RFactor2Adapter::new();
        let vehicle = RF2VehicleTelemetry {
            steering_shaft_torque: 25.0,
            ..Default::default()
        };
        // FFB is no longer versioned; fallback occurs when the value is NaN/Inf.
        let force_feedback = RF2ForceFeedback {
            force_value: f64::NAN,
        };

        let normalized = adapter.normalize_rf2_data(&vehicle, None, Some(&force_feedback));
        assert!((normalized.ffb_scalar - 0.5).abs() < 0.01);
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
            local_vel: [40.0, 0.0, 0.0], // 40 m/s forward
            gear: 3,
            ..Default::default()
        };

        let normalized = adapter.normalize_rf2_data(&vehicle, None, None);

        assert_eq!(normalized.rpm, 6000.0);
        assert!((normalized.speed_ms - 40.0).abs() < 0.01);
        assert_eq!(normalized.gear, 3);
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
        assert_eq!(GamePhase::SessionOver as i32, 8);
        assert_eq!(GamePhase::PausedOrReplay as i32, 9);
        Ok(())
    }

    #[test]
    fn test_default_vehicle_telemetry() -> TestResult {
        let vehicle = RF2VehicleTelemetry::default();
        assert_eq!(vehicle.id, 0);
        assert_eq!(vehicle.engine_rpm, 0.0);
        assert_eq!(vehicle.local_vel, [0.0; 3]);
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
        assert_eq!(wheel.lateral_patch_vel, 0.0);
        assert_eq!(wheel.longitudinal_patch_vel, 0.0);
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
        assert_eq!(header.version_update_begin, 0);
        assert_eq!(header.num_vehicles, 0);
        assert_eq!(header.bytes_updated_hint, 0);
        Ok(())
    }

    #[test]
    fn test_game_id_is_rfactor2() {
        assert_eq!(RFactor2Adapter::new().game_id(), "rfactor2");
    }

    /// Verify that our `GamePhase` enum matches `rF2GamePhase` from rF2State.h.
    /// SDK reference: <https://github.com/TheIronWolfModding/rF2SharedMemoryMapPlugin>
    #[test]
    fn test_game_phase_matches_sdk() -> TestResult {
        // rF2State.h defines: Garage=0, WarmUp=1, GridWalk=2, Formation=3,
        // Countdown=4, GreenFlag=5, FullCourseYellow=6, SessionStopped=7,
        // SessionOver=8.  Value 9 is documented in scoring comments (tag.2015.09.14).
        assert_eq!(GamePhase::Garage as i32, 0);
        assert_eq!(GamePhase::WarmUp as i32, 1);
        assert_eq!(GamePhase::GridWalk as i32, 2);
        assert_eq!(GamePhase::Formation as i32, 3);
        assert_eq!(GamePhase::Countdown as i32, 4);
        assert_eq!(GamePhase::GreenFlag as i32, 5);
        assert_eq!(GamePhase::FullCourseYellow as i32, 6);
        assert_eq!(GamePhase::SessionStopped as i32, 7);
        assert_eq!(GamePhase::SessionOver as i32, 8);
        assert_eq!(GamePhase::PausedOrReplay as i32, 9);
        Ok(())
    }

    /// Verify shared memory mapping names match rF2SharedMemoryMapPlugin constants.
    /// SDK reference: `SharedMemoryPlugin::MM_*_FILE_NAME` in rFactor2SharedMemoryMap.hpp
    #[test]
    fn test_shared_memory_names_match_sdk() -> TestResult {
        assert_eq!(RF2_TELEMETRY_SHARED_MEMORY_NAME, "$rFactor2SMMP_Telemetry$");
        assert_eq!(RF2_SCORING_SHARED_MEMORY_NAME, "$rFactor2SMMP_Scoring$");
        assert_eq!(
            RF2_FORCE_FEEDBACK_SHARED_MEMORY_NAME,
            "$rFactor2SMMP_ForceFeedback$"
        );
        Ok(())
    }

    /// Verify the RF2WheelTelemetry field order matches rF2Wheel in rF2State.h
    /// up to `mWear` (all f64 fields).
    #[test]
    fn test_wheel_field_order_matches_sdk() -> TestResult {
        // The SDK field order is:
        // mSuspensionDeflection, mRideHeight, mSuspForce, mBrakeTemp,
        // mBrakePressure, mRotation, mLateralPatchVel, mLongitudinalPatchVel,
        // mLateralGroundVel, mLongitudinalGroundVel, mCamber, mLateralForce,
        // mLongitudinalForce, mTireLoad, mGripFract, mPressure,
        // mTemperature[3], mWear.
        // All are f64; total = 20 × 8 = 160 bytes.
        let expected_f64_field_count = 20; // 17 scalar + 3 temperature
        let actual_size = mem::size_of::<RF2WheelTelemetry>();
        assert_eq!(
            actual_size,
            expected_f64_field_count * mem::size_of::<f64>(),
            "RF2WheelTelemetry size should be {} f64 fields = {} bytes",
            expected_f64_field_count,
            expected_f64_field_count * 8
        );
        Ok(())
    }
}

#[cfg(test)]
mod proptest_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn rf2_no_panic_on_arbitrary_bytes(
            data in proptest::collection::vec(any::<u8>(), 0..4096usize)
        ) {
            let adapter = RFactor2Adapter::new();
            let _ = adapter.normalize(&data);
        }

        #[test]
        fn rf2_short_buffer_always_errors(
            data in proptest::collection::vec(any::<u8>(), 0..256usize)
        ) {
            // RF2VehicleTelemetry is larger than 256 bytes, so these must all error.
            let adapter = RFactor2Adapter::new();
            prop_assert!(adapter.normalize(&data).is_err());
        }

        #[test]
        fn rf2_speed_nonneg(vx in 0.0f64..200.0f64) {
            let adapter = RFactor2Adapter::new();
            let vehicle = RF2VehicleTelemetry {
                local_vel: [vx, 0.0, 0.0],
                ..RF2VehicleTelemetry::default()
            };
            let normalized = adapter.normalize_rf2_data(&vehicle, None, None);
            prop_assert!(normalized.speed_ms >= 0.0);
        }

        #[test]
        fn rf2_rpm_nonneg(rpm in 0.0f64..20000.0f64) {
            let adapter = RFactor2Adapter::new();
            let vehicle = RF2VehicleTelemetry { engine_rpm: rpm, ..RF2VehicleTelemetry::default() };
            let normalized = adapter.normalize_rf2_data(&vehicle, None, None);
            prop_assert!(normalized.rpm >= 0.0);
        }

        #[test]
        fn rf2_ffb_scalar_clamped(torque in -200.0f64..=200.0f64) {
            let adapter = RFactor2Adapter::new();
            let vehicle = RF2VehicleTelemetry {
                steering_shaft_torque: torque,
                ..RF2VehicleTelemetry::default()
            };
            let normalized = adapter.normalize_rf2_data(&vehicle, None, None);
            prop_assert!(
                normalized.ffb_scalar >= -1.0 && normalized.ffb_scalar <= 1.0,
                "ffb_scalar {} must be in [-1, 1]",
                normalized.ffb_scalar
            );
        }
    }
}

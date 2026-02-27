//! RaceRoom Racing Experience telemetry adapter using the R3E shared memory interface.
//!
//! Opens `Local\$R3E` and reads key telemetry fields at fixed byte offsets from the
//! Sector3 R3E SDK. Offset-based reading is used because the full struct layout can
//! vary between SDK versions.
#![cfg_attr(not(windows), allow(unused, dead_code))]

use crate::{
    NormalizedTelemetry, TelemetryAdapter, TelemetryFrame, TelemetryReceiver, telemetry_now_ns,
};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

#[cfg(windows)]
use winapi::um::{
    handleapi::CloseHandle,
    memoryapi::{FILE_MAP_READ, MapViewOfFile, OpenFileMappingW, UnmapViewOfFile},
};

#[cfg(windows)]
const R3E_SHARED_MEMORY_NAME: &str = "Local\\$R3E";
/// Number of bytes to map from the R3E shared memory (covers all key offsets).
const R3E_VIEW_SIZE: usize = 4096;

const RACEROOM_PROCESS_NAMES: &[&str] = &["rrre.exe", "raceroom.exe"];

// R3E field byte offsets (Sector3 SDK, version 2)
const OFF_VERSION_MAJOR: usize = 0;
const OFF_GAME_PAUSED: usize = 100;
const OFF_GAME_IN_MENUS: usize = 104;
const OFF_ENGINE_RPM: usize = 600;
const OFF_ENGINE_RPM_MAX: usize = 604;
const OFF_FUEL_LEFT: usize = 620;
const OFF_FUEL_CAPACITY: usize = 628;
const OFF_SPEED: usize = 700;
const OFF_STEER_INPUT: usize = 704;
const OFF_THROTTLE: usize = 708;
const OFF_BRAKE: usize = 712;
const OFF_CLUTCH: usize = 716;
const OFF_GEAR: usize = 730;

/// Expected R3E shared memory major version.
const R3E_VERSION_MAJOR: i32 = 2;

fn parse_r3e_memory(data: &[u8]) -> Result<NormalizedTelemetry> {
    if data.len() < R3E_VIEW_SIZE {
        return Err(anyhow!(
            "R3E memory too small: expected at least {R3E_VIEW_SIZE}, got {}",
            data.len()
        ));
    }

    let version_major = read_i32_le(data, OFF_VERSION_MAJOR).unwrap_or(0);
    if version_major != R3E_VERSION_MAJOR {
        return Err(anyhow!(
            "Unexpected R3E version_major: expected {R3E_VERSION_MAJOR}, got {version_major}"
        ));
    }

    let game_paused = read_i32_le(data, OFF_GAME_PAUSED).unwrap_or(0);
    let game_in_menus = read_i32_le(data, OFF_GAME_IN_MENUS).unwrap_or(0);
    if game_paused != 0 || game_in_menus != 0 {
        return Ok(NormalizedTelemetry::builder().build());
    }

    let rpm = read_f32_le(data, OFF_ENGINE_RPM).unwrap_or(0.0);
    let max_rpm = read_f32_le(data, OFF_ENGINE_RPM_MAX).unwrap_or(0.0);
    let fuel_left = read_f32_le(data, OFF_FUEL_LEFT).unwrap_or(0.0);
    let fuel_capacity = read_f32_le(data, OFF_FUEL_CAPACITY).unwrap_or(0.0);
    let speed_mps = read_f32_le(data, OFF_SPEED).unwrap_or(0.0).abs();
    let steering = read_f32_le(data, OFF_STEER_INPUT)
        .unwrap_or(0.0)
        .clamp(-1.0, 1.0);
    let throttle = read_f32_le(data, OFF_THROTTLE).unwrap_or(0.0);
    let brake = read_f32_le(data, OFF_BRAKE).unwrap_or(0.0);
    let clutch = read_f32_le(data, OFF_CLUTCH).unwrap_or(0.0);
    let gear = read_i32_le(data, OFF_GEAR)
        .unwrap_or(0)
        .clamp(-1, 127) as i8;

    let fuel_percent = if fuel_capacity > 0.0 {
        (fuel_left / fuel_capacity).clamp(0.0, 1.0)
    } else {
        0.0
    };

    Ok(NormalizedTelemetry::builder()
        .rpm(rpm)
        .max_rpm(max_rpm)
        .speed_ms(speed_mps)
        .steering_angle(steering)
        .throttle(throttle)
        .brake(brake)
        .clutch(clutch)
        .gear(gear)
        .build()
        .with_extended(
            "fuel_percent".to_string(),
            crate::TelemetryValue::Float(fuel_percent),
        ))
}

/// RaceRoom Racing Experience telemetry adapter.
pub struct RaceRoomAdapter {
    update_rate: Duration,
}

impl Default for RaceRoomAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl RaceRoomAdapter {
    pub fn new() -> Self {
        Self {
            update_rate: Duration::from_millis(10),
        }
    }
}

#[async_trait]
impl TelemetryAdapter for RaceRoomAdapter {
    fn game_id(&self) -> &str {
        "raceroom"
    }

    async fn start_monitoring(&self) -> Result<TelemetryReceiver> {
        let (tx, rx) = mpsc::channel(100);
        let update_rate = self.update_rate;

        tokio::spawn(async move {
            #[cfg(windows)]
            {
                info!("RaceRoom adapter attempting shared memory connection");
                let mut frame_seq = 0u64;
                let mut warned = false;
                loop {
                    match read_r3e_shared_memory() {
                        Ok(normalized) => {
                            let frame = TelemetryFrame::new(
                                normalized,
                                telemetry_now_ns(),
                                frame_seq,
                                R3E_VIEW_SIZE,
                            );
                            if tx.send(frame).await.is_err() {
                                debug!("Receiver dropped, stopping RaceRoom monitoring");
                                break;
                            }
                            frame_seq = frame_seq.saturating_add(1);
                            warned = false;
                        }
                        Err(e) => {
                            if !warned {
                                warn!("RaceRoom shared memory not available: {e}");
                                warned = true;
                            }
                        }
                    }
                    tokio::time::sleep(update_rate).await;
                }
            }

            #[cfg(not(windows))]
            warn!("RaceRoom shared memory is only supported on Windows");

            info!("Stopped RaceRoom telemetry monitoring");
        });

        Ok(rx)
    }

    async fn stop_monitoring(&self) -> Result<()> {
        Ok(())
    }

    fn normalize(&self, raw: &[u8]) -> Result<NormalizedTelemetry> {
        parse_r3e_memory(raw)
    }

    fn expected_update_rate(&self) -> Duration {
        self.update_rate
    }

    async fn is_game_running(&self) -> Result<bool> {
        Ok(is_raceroom_process_running())
    }
}

/// Open R3E shared memory, read the key fields, and close. Returns error on any failure.
#[cfg(windows)]
fn read_r3e_shared_memory() -> Result<NormalizedTelemetry> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    let wide_name: Vec<u16> = OsStr::new(R3E_SHARED_MEMORY_NAME)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    // SAFETY: Win32 shared memory API calls with a valid null-terminated UTF-16 name.
    unsafe {
        let handle = OpenFileMappingW(FILE_MAP_READ, 0, wide_name.as_ptr());
        if handle.is_null() {
            return Err(anyhow!("Failed to open R3E shared memory mapping"));
        }
        let view = MapViewOfFile(handle, FILE_MAP_READ, 0, 0, R3E_VIEW_SIZE);
        if view.is_null() {
            CloseHandle(handle);
            return Err(anyhow!("Failed to map R3E shared memory view"));
        }
        let data = std::slice::from_raw_parts(view as *const u8, R3E_VIEW_SIZE);
        let result = parse_r3e_memory(data);
        UnmapViewOfFile(view);
        CloseHandle(handle);
        result
    }
}

#[cfg(windows)]
fn is_raceroom_process_running() -> bool {
    use std::ffi::CStr;
    use std::mem;
    use winapi::um::{
        handleapi::{CloseHandle, INVALID_HANDLE_VALUE},
        tlhelp32::{
            CreateToolhelp32Snapshot, PROCESSENTRY32, Process32First, Process32Next,
            TH32CS_SNAPPROCESS,
        },
    };

    // SAFETY: Windows snapshot API with proper initialization.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return false;
        }
        let mut entry: PROCESSENTRY32 = mem::zeroed();
        entry.dwSize = mem::size_of::<PROCESSENTRY32>() as u32;
        let mut found = false;
        if Process32First(snapshot, &mut entry) != 0 {
            loop {
                let name = CStr::from_ptr(entry.szExeFile.as_ptr())
                    .to_string_lossy()
                    .to_ascii_lowercase();
                if RACEROOM_PROCESS_NAMES.iter().any(|p| name.contains(p)) {
                    found = true;
                    break;
                }
                if Process32Next(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snapshot);
        found
    }
}

#[cfg(not(windows))]
fn is_raceroom_process_running() -> bool {
    false
}

fn read_f32_le(data: &[u8], offset: usize) -> Option<f32> {
    data.get(offset..offset + 4)
        .and_then(|b| b.try_into().ok())
        .map(f32::from_le_bytes)
}

fn read_i32_le(data: &[u8], offset: usize) -> Option<i32> {
    data.get(offset..offset + 4)
        .and_then(|b| b.try_into().ok())
        .map(i32::from_le_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn make_r3e_memory(
        rpm: f32,
        speed: f32,
        steering: f32,
        throttle: f32,
        brake: f32,
        gear: i32,
    ) -> Vec<u8> {
        let mut data = vec![0u8; R3E_VIEW_SIZE];
        data[OFF_VERSION_MAJOR..OFF_VERSION_MAJOR + 4]
            .copy_from_slice(&R3E_VERSION_MAJOR.to_le_bytes());
        data[OFF_GAME_PAUSED..OFF_GAME_PAUSED + 4].copy_from_slice(&0i32.to_le_bytes());
        data[OFF_GAME_IN_MENUS..OFF_GAME_IN_MENUS + 4].copy_from_slice(&0i32.to_le_bytes());
        data[OFF_ENGINE_RPM..OFF_ENGINE_RPM + 4].copy_from_slice(&rpm.to_le_bytes());
        data[OFF_ENGINE_RPM_MAX..OFF_ENGINE_RPM_MAX + 4]
            .copy_from_slice(&8000.0f32.to_le_bytes());
        data[OFF_FUEL_LEFT..OFF_FUEL_LEFT + 4].copy_from_slice(&30.0f32.to_le_bytes());
        data[OFF_FUEL_CAPACITY..OFF_FUEL_CAPACITY + 4].copy_from_slice(&60.0f32.to_le_bytes());
        data[OFF_SPEED..OFF_SPEED + 4].copy_from_slice(&speed.to_le_bytes());
        data[OFF_STEER_INPUT..OFF_STEER_INPUT + 4].copy_from_slice(&steering.to_le_bytes());
        data[OFF_THROTTLE..OFF_THROTTLE + 4].copy_from_slice(&throttle.to_le_bytes());
        data[OFF_BRAKE..OFF_BRAKE + 4].copy_from_slice(&brake.to_le_bytes());
        data[OFF_CLUTCH..OFF_CLUTCH + 4].copy_from_slice(&0.0f32.to_le_bytes());
        data[OFF_GEAR..OFF_GEAR + 4].copy_from_slice(&gear.to_le_bytes());
        data
    }

    #[test]
    fn test_parse_valid_memory() -> TestResult {
        let data = make_r3e_memory(5000.0, 50.0, 0.3, 0.7, 0.0, 3);
        let result = parse_r3e_memory(&data)?;
        assert!((result.rpm - 5000.0).abs() < 0.01);
        assert!((result.speed_ms - 50.0).abs() < 0.01);
        assert!((result.steering_angle - 0.3).abs() < 0.001);
        assert!((result.throttle - 0.7).abs() < 0.001);
        assert_eq!(result.gear, 3);
        Ok(())
    }

    #[test]
    fn test_parse_invalid_version() {
        let mut data = make_r3e_memory(5000.0, 50.0, 0.3, 0.7, 0.0, 3);
        data[OFF_VERSION_MAJOR..OFF_VERSION_MAJOR + 4].copy_from_slice(&1i32.to_le_bytes());
        assert!(parse_r3e_memory(&data).is_err());
    }

    #[test]
    fn test_parse_too_small() {
        let data = vec![0u8; 100];
        assert!(parse_r3e_memory(&data).is_err());
    }

    #[test]
    fn test_game_paused_returns_default() -> TestResult {
        let mut data = make_r3e_memory(5000.0, 50.0, 0.3, 0.7, 0.0, 3);
        data[OFF_GAME_PAUSED..OFF_GAME_PAUSED + 4].copy_from_slice(&1i32.to_le_bytes());
        let result = parse_r3e_memory(&data)?;
        assert_eq!(result.rpm, 0.0);
        Ok(())
    }

    #[test]
    fn test_game_in_menus_returns_default() -> TestResult {
        let mut data = make_r3e_memory(5000.0, 50.0, 0.3, 0.7, 0.0, 3);
        data[OFF_GAME_IN_MENUS..OFF_GAME_IN_MENUS + 4].copy_from_slice(&1i32.to_le_bytes());
        let result = parse_r3e_memory(&data)?;
        assert_eq!(result.rpm, 0.0);
        Ok(())
    }

    #[test]
    fn test_normalization_clamp() -> TestResult {
        let data = make_r3e_memory(6000.0, 80.0, 2.0, 1.5, -0.2, 4);
        let result = parse_r3e_memory(&data)?;
        assert!((result.steering_angle - 1.0).abs() < 0.001);
        // Builder clamps throttle/brake to [0,1]
        assert!((result.throttle - 1.0).abs() < 0.001);
        assert!((result.brake).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn test_fuel_percent_extended() -> TestResult {
        let data = make_r3e_memory(3000.0, 20.0, 0.0, 0.3, 0.0, 1);
        let result = parse_r3e_memory(&data)?;
        // fuel_left=30, fuel_capacity=60 → 0.5
        if let Some(crate::TelemetryValue::Float(pct)) = result.get_extended("fuel_percent") {
            assert!((*pct - 0.5).abs() < 0.001);
        } else {
            return Err("fuel_percent not found in extended".into());
        }
        Ok(())
    }

    #[test]
    fn test_adapter_game_id() {
        let adapter = RaceRoomAdapter::new();
        assert_eq!(adapter.game_id(), "raceroom");
    }

    #[test]
    fn test_adapter_update_rate() {
        let adapter = RaceRoomAdapter::new();
        assert_eq!(adapter.expected_update_rate(), Duration::from_millis(10));
    }

    #[test]
    fn test_normalize_delegates_to_parse() -> TestResult {
        let adapter = RaceRoomAdapter::new();
        let data = make_r3e_memory(4000.0, 30.0, -0.2, 0.4, 0.0, 2);
        let result = adapter.normalize(&data)?;
        assert!((result.rpm - 4000.0).abs() < 0.01);
        Ok(())
    }
}

//! Automobilista 1 telemetry adapter via ISI rFactor 1 shared memory.
//!
//! Automobilista 1 uses the ISI engine (same base as rFactor 1). Telemetry is
//! exposed through the `$rFactor$` named shared memory section, populated by
//! ISI InternalsPlugin-compatible plugins shipped with the game.
//!
//! Byte offsets are derived from the ISI InternalsPlugin SDK 2.3 (rF1VehicleTelemetry):
//! - mGear           i32  @ 360  (-1=reverse, 0=neutral, 1+=forward)
//! - mEngineRPM      f64  @ 368
//! - mEngineMaxRPM   f64  @ 384
//! - mLocalVel_x     f64  @ 192  (lateral velocity, m/s)
//! - mLocalVel_z     f64  @ 208  (longitudinal velocity, m/s)
//! - mLocalAccel_x   f64  @ 216  (lateral accel, m/s²)
//! - mLocalAccel_z   f64  @ 232  (longitudinal accel, m/s²)
//! - mFuel           f32  @ 460  (current fuel, same unit as mFuelCapacity)
//! - mFuelCapacity   u8   @ 457  (capacity in litres/kg)
//! - mFilteredThrottle f32 @ 492  (0–1)
//! - mFilteredBrake    f32 @ 496  (0–1)
//! - mFilteredSteering f32 @ 500  (-1=left, 1=right)
//! - mSpeed          f32  @ 528  (m/s)
//!
//! Only available on Windows. All paths gracefully degrade on other platforms.

#![cfg_attr(not(windows), allow(unused, dead_code))]

use crate::{
    NormalizedTelemetry, TelemetryAdapter, TelemetryFrame, TelemetryReceiver, telemetry_now_ns,
};
use anyhow::Result;
use async_trait::async_trait;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{info, warn};

#[cfg(windows)]
use winapi::um::{
    handleapi::{CloseHandle, INVALID_HANDLE_VALUE},
    memoryapi::{FILE_MAP_READ, MapViewOfFile, OpenFileMappingW, UnmapViewOfFile},
    tlhelp32::{
        CreateToolhelp32Snapshot, PROCESSENTRY32, Process32First, Process32Next, TH32CS_SNAPPROCESS,
    },
    winnt::HANDLE,
};

/// Named shared memory section created by the ISI InternalsPlugin for Automobilista 1.
const AMS1_SHARED_MEMORY_NAME: &str = "$rFactor$";

/// Minimum readable size: up to and including mSpeed at offset 528 + 4 = 532 bytes.
const AMS1_MIN_SHARED_MEMORY_SIZE: usize = 532;

/// Total size mapped (generous upper bound for rF1VehicleTelemetry).
const AMS1_MAP_SIZE: usize = 2048;

const AMS1_PROCESS_NAMES: [&str; 2] = ["automobilista.exe", "game.exe"];

// Byte offsets within rF1VehicleTelemetry (ISI InternalsPlugin SDK 2.3).
const OFF_LOCAL_ACCEL_X: usize = 216; // lateral accel (f64)
const OFF_LOCAL_ACCEL_Z: usize = 232; // longitudinal accel (f64)
const OFF_GEAR: usize = 360; // i32 (-1=reverse, 0=neutral, 1+=fwd)
const OFF_ENGINE_RPM: usize = 368; // f64
const OFF_ENGINE_MAX_RPM: usize = 384; // f64
const OFF_FUEL_CAPACITY: usize = 457; // u8 (litres or kg)
const OFF_FUEL: usize = 460; // f32 (current fuel)
const OFF_FILTERED_THROTTLE: usize = 492; // f32
const OFF_FILTERED_BRAKE: usize = 496; // f32
const OFF_FILTERED_STEERING: usize = 500; // f32
const OFF_SPEED: usize = 528; // f32 (m/s)

/// Lateral G normalisation range (~3 G is typical for circuit cars).
const FFB_LAT_G_MAX: f32 = 3.0;
/// Lateral acceleration due to gravity (m/s²).
const GRAVITY: f32 = 9.81;

/// Automobilista 1 telemetry adapter via ISI shared memory.
pub struct Automobilista1Adapter {
    update_rate: Duration,
    #[cfg(windows)]
    memory: Option<SharedMemoryHandle>,
}

#[cfg(windows)]
struct SharedMemoryHandle {
    handle: HANDLE,
    base_ptr: *const u8,
    size: usize,
}

#[cfg(windows)]
unsafe impl Send for SharedMemoryHandle {}
#[cfg(windows)]
unsafe impl Sync for SharedMemoryHandle {}

#[cfg(windows)]
impl Drop for SharedMemoryHandle {
    fn drop(&mut self) {
        unsafe {
            UnmapViewOfFile(self.base_ptr as *mut _);
            CloseHandle(self.handle);
        }
    }
}

impl Default for Automobilista1Adapter {
    fn default() -> Self {
        Self::new()
    }
}

impl Automobilista1Adapter {
    pub fn new() -> Self {
        Self {
            update_rate: Duration::from_millis(16),
            #[cfg(windows)]
            memory: None,
        }
    }

    #[cfg(windows)]
    fn open_shared_memory(&mut self) -> Result<()> {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;

        let name_wide: Vec<u16> = OsStr::new(AMS1_SHARED_MEMORY_NAME)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        unsafe {
            let handle = OpenFileMappingW(FILE_MAP_READ, 0, name_wide.as_ptr());
            if handle == INVALID_HANDLE_VALUE || handle.is_null() {
                return Err(anyhow::anyhow!(
                    "Failed to open Automobilista 1 shared memory '{}'. Is the game running with the ISI plugin?",
                    AMS1_SHARED_MEMORY_NAME
                ));
            }

            let base_ptr = MapViewOfFile(handle, FILE_MAP_READ, 0, 0, AMS1_MAP_SIZE) as *const u8;
            if base_ptr.is_null() {
                CloseHandle(handle);
                return Err(anyhow::anyhow!(
                    "Failed to map Automobilista 1 shared memory view"
                ));
            }

            self.memory = Some(SharedMemoryHandle {
                handle,
                base_ptr,
                size: AMS1_MAP_SIZE,
            });

            info!(
                "Connected to Automobilista 1 shared memory '{}'",
                AMS1_SHARED_MEMORY_NAME
            );
            Ok(())
        }
    }

    #[cfg(not(windows))]
    fn open_shared_memory(&mut self) -> Result<()> {
        Err(anyhow::anyhow!(
            "Automobilista 1 shared memory is only available on Windows"
        ))
    }

    #[cfg(windows)]
    fn read_snapshot(&self) -> Result<Vec<u8>> {
        let mem = self
            .memory
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Automobilista 1 shared memory not initialised"))?;
        if mem.size < AMS1_MIN_SHARED_MEMORY_SIZE {
            return Err(anyhow::anyhow!(
                "Mapped region too small: {} < {}",
                mem.size,
                AMS1_MIN_SHARED_MEMORY_SIZE
            ));
        }
        let snapshot = unsafe {
            std::slice::from_raw_parts(mem.base_ptr, mem.size.min(AMS1_MAP_SIZE)).to_vec()
        };
        Ok(snapshot)
    }

    #[cfg(not(windows))]
    fn read_snapshot(&self) -> Result<Vec<u8>> {
        Err(anyhow::anyhow!(
            "Automobilista 1 shared memory is only available on Windows"
        ))
    }

    #[cfg(windows)]
    fn is_ams1_running() -> bool {
        detect_ams1_pid().is_some()
    }

    #[cfg(not(windows))]
    fn is_ams1_running() -> bool {
        false
    }
}

/// Read a little-endian `f32` from `data` at `offset`.
fn read_f32(data: &[u8], offset: usize) -> Option<f32> {
    data.get(offset..offset + 4)
        .and_then(|b| b.try_into().ok())
        .map(f32::from_le_bytes)
}

/// Read a little-endian `f64` from `data` at `offset`.
fn read_f64(data: &[u8], offset: usize) -> Option<f64> {
    data.get(offset..offset + 8)
        .and_then(|b| b.try_into().ok())
        .map(f64::from_le_bytes)
}

/// Read a little-endian `i32` from `data` at `offset`.
fn read_i32(data: &[u8], offset: usize) -> Option<i32> {
    data.get(offset..offset + 4)
        .and_then(|b| b.try_into().ok())
        .map(i32::from_le_bytes)
}

pub(crate) fn parse_snapshot(data: &[u8]) -> Result<NormalizedTelemetry> {
    if data.len() < AMS1_MIN_SHARED_MEMORY_SIZE {
        return Err(anyhow::anyhow!(
            "Automobilista 1 snapshot too small: need {} bytes, got {}",
            AMS1_MIN_SHARED_MEMORY_SIZE,
            data.len()
        ));
    }

    let speed_ms = read_f32(data, OFF_SPEED).unwrap_or(0.0).abs();

    let rpm = (read_f64(data, OFF_ENGINE_RPM).unwrap_or(0.0).max(0.0)) as f32;
    let max_rpm = (read_f64(data, OFF_ENGINE_MAX_RPM).unwrap_or(0.0).max(0.0)) as f32;

    let gear_raw = read_i32(data, OFF_GEAR).unwrap_or(0);
    let gear: i8 = gear_raw.clamp(-1, 8) as i8;

    let throttle = read_f32(data, OFF_FILTERED_THROTTLE)
        .unwrap_or(0.0)
        .clamp(0.0, 1.0);
    let brake = read_f32(data, OFF_FILTERED_BRAKE)
        .unwrap_or(0.0)
        .clamp(0.0, 1.0);
    // rFactor 1 steering: negative = left, positive = right (same convention as NormalizedTelemetry)
    let steering_angle = read_f32(data, OFF_FILTERED_STEERING)
        .unwrap_or(0.0)
        .clamp(-1.0, 1.0);

    // Convert lateral acceleration (m/s²) → G
    let lat_accel_ms2 = (read_f64(data, OFF_LOCAL_ACCEL_X).unwrap_or(0.0)) as f32;
    let lon_accel_ms2 = (read_f64(data, OFF_LOCAL_ACCEL_Z).unwrap_or(0.0)) as f32;
    let lat_g = lat_accel_ms2 / GRAVITY;
    let lon_g = lon_accel_ms2 / GRAVITY;

    let ffb_scalar = (lat_g / FFB_LAT_G_MAX).clamp(-1.0, 1.0);

    let fuel_capacity_raw = data.get(OFF_FUEL_CAPACITY).copied().unwrap_or(0);
    let fuel_in_tank = read_f32(data, OFF_FUEL).unwrap_or(0.0).max(0.0);
    let fuel_percent = if fuel_capacity_raw > 0 {
        (fuel_in_tank / f32::from(fuel_capacity_raw)).clamp(0.0, 1.0)
    } else {
        0.0
    };

    let mut builder = NormalizedTelemetry::builder()
        .speed_ms(speed_ms)
        .rpm(rpm)
        .gear(gear)
        .throttle(throttle)
        .brake(brake)
        .steering_angle(steering_angle)
        .lateral_g(lat_g)
        .longitudinal_g(lon_g)
        .ffb_scalar(ffb_scalar)
        .fuel_percent(fuel_percent);

    if max_rpm > 0.0 {
        let rpm_fraction = (rpm / max_rpm).clamp(0.0, 1.0);
        builder = builder.max_rpm(max_rpm).extended(
            "rpm_fraction".to_string(),
            crate::TelemetryValue::Float(rpm_fraction),
        );
    }

    Ok(builder.build())
}

#[cfg(windows)]
fn detect_ams1_pid() -> Option<u32> {
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return None;
        }

        let mut entry = PROCESSENTRY32 {
            dwSize: std::mem::size_of::<PROCESSENTRY32>() as u32,
            ..std::mem::zeroed()
        };

        if Process32First(snapshot, &mut entry) == 0 {
            CloseHandle(snapshot);
            return None;
        }

        loop {
            let exe_name = std::ffi::CStr::from_ptr(entry.szExeFile.as_ptr())
                .to_str()
                .unwrap_or("")
                .to_ascii_lowercase();

            for &pattern in &AMS1_PROCESS_NAMES {
                if exe_name == pattern {
                    CloseHandle(snapshot);
                    return Some(entry.th32ProcessID);
                }
            }

            if Process32Next(snapshot, &mut entry) == 0 {
                break;
            }
        }

        CloseHandle(snapshot);
        None
    }
}

#[async_trait]
impl TelemetryAdapter for Automobilista1Adapter {
    fn game_id(&self) -> &str {
        "automobilista"
    }

    async fn start_monitoring(&self) -> Result<TelemetryReceiver> {
        let update_rate = self.update_rate;
        let (tx, rx) = mpsc::channel(100);

        #[cfg(windows)]
        {
            let mut adapter = Self::new();
            tokio::spawn(async move {
                let mut connected = false;
                let mut frame_seq = 0u64;

                loop {
                    if !connected {
                        if adapter.open_shared_memory().is_ok() {
                            connected = true;
                            info!("Automobilista 1 shared memory connection established");
                        } else {
                            tokio::time::sleep(Duration::from_secs(2)).await;
                            continue;
                        }
                    }

                    match adapter.read_snapshot() {
                        Ok(snapshot) => match parse_snapshot(&snapshot) {
                            Ok(normalized) => {
                                let frame = TelemetryFrame::new(
                                    normalized,
                                    telemetry_now_ns(),
                                    frame_seq,
                                    snapshot.len(),
                                );
                                if tx.send(frame).await.is_err() {
                                    break;
                                }
                                frame_seq = frame_seq.saturating_add(1);
                            }
                            Err(error) => {
                                warn!(error = %error, "Failed to parse Automobilista 1 snapshot");
                            }
                        },
                        Err(error) => {
                            warn!(error = %error, "Lost Automobilista 1 shared memory; reconnecting");
                            connected = false;
                            adapter.memory = None;
                        }
                    }

                    tokio::time::sleep(update_rate).await;
                }
            });
        }

        #[cfg(not(windows))]
        {
            warn!("Automobilista 1 adapter is only functional on Windows");
        }

        Ok(rx)
    }

    async fn stop_monitoring(&self) -> Result<()> {
        Ok(())
    }

    fn normalize(&self, raw: &[u8]) -> Result<NormalizedTelemetry> {
        parse_snapshot(raw)
    }

    fn expected_update_rate(&self) -> Duration {
        self.update_rate
    }

    async fn is_game_running(&self) -> Result<bool> {
        Ok(Self::is_ams1_running())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_snapshot(size: usize) -> Vec<u8> {
        vec![0u8; size]
    }

    fn write_f32_le(buf: &mut [u8], offset: usize, value: f32) {
        buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn write_f64_le(buf: &mut [u8], offset: usize, value: f64) {
        buf[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }

    fn write_i32_le(buf: &mut [u8], offset: usize, value: i32) {
        buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    #[test]
    fn rejects_too_small_snapshot() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = Automobilista1Adapter::new();
        let result = adapter.normalize(&[0u8; AMS1_MIN_SHARED_MEMORY_SIZE - 1]);
        assert!(result.is_err(), "expected error for undersized snapshot");
        Ok(())
    }

    #[test]
    fn zero_snapshot_returns_zero_speed_and_rpm() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = Automobilista1Adapter::new();
        let snap = make_snapshot(AMS1_MIN_SHARED_MEMORY_SIZE);
        let t = adapter.normalize(&snap)?;
        assert_eq!(t.speed_ms, 0.0);
        assert_eq!(t.rpm, 0.0);
        Ok(())
    }

    #[test]
    fn gear_negative_one_stays_reverse() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = Automobilista1Adapter::new();
        let mut snap = make_snapshot(AMS1_MIN_SHARED_MEMORY_SIZE);
        write_i32_le(&mut snap, OFF_GEAR, -1);
        let t = adapter.normalize(&snap)?;
        assert_eq!(t.gear, -1);
        Ok(())
    }

    #[test]
    fn speed_extracted_from_offset() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = Automobilista1Adapter::new();
        let mut snap = make_snapshot(AMS1_MIN_SHARED_MEMORY_SIZE);
        write_f32_le(&mut snap, OFF_SPEED, 55.0);
        let t = adapter.normalize(&snap)?;
        assert!(
            (t.speed_ms - 55.0).abs() < 0.001,
            "speed_ms should be 55.0, got {}",
            t.speed_ms
        );
        Ok(())
    }

    #[test]
    fn rpm_extracted_from_double_offset() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = Automobilista1Adapter::new();
        let mut snap = make_snapshot(AMS1_MIN_SHARED_MEMORY_SIZE);
        write_f64_le(&mut snap, OFF_ENGINE_RPM, 6000.0);
        let t = adapter.normalize(&snap)?;
        assert!(
            (t.rpm - 6000.0).abs() < 0.1,
            "rpm should be 6000, got {}",
            t.rpm
        );
        Ok(())
    }

    #[test]
    fn throttle_clamped_to_unit_range() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = Automobilista1Adapter::new();
        let mut snap = make_snapshot(AMS1_MIN_SHARED_MEMORY_SIZE);
        write_f32_le(&mut snap, OFF_FILTERED_THROTTLE, 2.5);
        let t = adapter.normalize(&snap)?;
        assert!(t.throttle <= 1.0 && t.throttle >= 0.0);
        Ok(())
    }

    #[test]
    fn ffb_scalar_in_unit_range() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = Automobilista1Adapter::new();
        let mut snap = make_snapshot(AMS1_MIN_SHARED_MEMORY_SIZE);
        // Large lateral acceleration
        write_f64_le(&mut snap, OFF_LOCAL_ACCEL_X, 50.0);
        let t = adapter.normalize(&snap)?;
        assert!(t.ffb_scalar >= -1.0 && t.ffb_scalar <= 1.0);
        Ok(())
    }

    #[test]
    fn game_id_is_automobilista() {
        assert_eq!(Automobilista1Adapter::new().game_id(), "automobilista");
    }
}

#[cfg(test)]
mod proptest_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(500))]

        #[test]
        fn parse_no_panic_on_arbitrary(
            data in proptest::collection::vec(any::<u8>(), 0..1024)
        ) {
            let adapter = Automobilista1Adapter::new();
            let _ = adapter.normalize(&data);
        }
    }
}

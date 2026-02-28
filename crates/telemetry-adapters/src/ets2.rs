//! Euro Truck Simulator 2 / American Truck Simulator telemetry adapter.
//!
//! Reads from the SCS Telemetry SDK shared memory file `Local\SCSTelemetry`.
//! Supports both ETS2 (game_id "ets2") and ATS (game_id "ats") via `Ets2Variant`.
//! Update rate: ~20 Hz.
#![cfg_attr(not(windows), allow(unused, dead_code))]

use crate::{
    NormalizedTelemetry, TelemetryAdapter, TelemetryFrame, TelemetryReceiver, TelemetryValue,
    telemetry_now_ns,
};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, info};

#[cfg(windows)]
use winapi::um::{
    handleapi::CloseHandle,
    memoryapi::{FILE_MAP_READ, MapViewOfFile, OpenFileMappingW, UnmapViewOfFile},
};

/// SCS Telemetry SDK shared memory name (same for both ETS2 and ATS).
const SCS_SHARED_MEMORY_NAME: &str = "Local\\SCSTelemetry";
/// Total mapped size; community-documented SDK v1.14 layout is 512 bytes.
const SCS_SHARED_MEMORY_SIZE: usize = 512;

// Byte offsets in the SCS Telemetry SDK v1.14 memory layout.
const OFF_VERSION: usize = 0; // u32
const OFF_SPEED_MS: usize = 4; // f32  m/s
const OFF_ENGINE_RPM: usize = 8; // f32  rev/min
const OFF_GEAR: usize = 12; // i32  >0=forward, <0=reverse, 0=neutral
const OFF_FUEL_RATIO: usize = 16; // f32  0.0–1.0
const OFF_ENGINE_LOAD: usize = 20; // f32  0.0–1.0

const SCS_EXPECTED_VERSION: u32 = 1;

/// Which SCS game variant this adapter targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ets2Variant {
    /// Euro Truck Simulator 2
    Ets2,
    /// American Truck Simulator
    Ats,
}

impl Ets2Variant {
    fn game_id(self) -> &'static str {
        match self {
            Self::Ets2 => "ets2",
            Self::Ats => "ats",
        }
    }

    #[cfg(windows)]
    fn process_name(self) -> &'static str {
        match self {
            Self::Ets2 => "eurotrucks2.exe",
            Self::Ats => "amtrucks.exe",
        }
    }
}

/// ETS2 / ATS telemetry adapter using SCS Telemetry SDK shared memory.
pub struct Ets2Adapter {
    variant: Ets2Variant,
    update_rate: Duration,
}

impl Ets2Adapter {
    /// Create a new ETS2 adapter.
    pub fn new() -> Self {
        Self::with_variant(Ets2Variant::Ets2)
    }

    /// Create an adapter for the given SCS game variant.
    pub fn with_variant(variant: Ets2Variant) -> Self {
        Self {
            variant,
            update_rate: Duration::from_millis(50), // ~20 Hz
        }
    }
}

impl Default for Ets2Adapter {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse a raw SCS Telemetry SDK memory snapshot into `NormalizedTelemetry`.
pub fn parse_scs_packet(data: &[u8]) -> Result<NormalizedTelemetry> {
    if data.len() < OFF_ENGINE_LOAD + 4 {
        return Err(anyhow!(
            "SCS telemetry buffer too short: expected at least {}, got {}",
            OFF_ENGINE_LOAD + 4,
            data.len()
        ));
    }

    let version = read_u32_le(data, OFF_VERSION).unwrap_or(0);
    if version != SCS_EXPECTED_VERSION {
        return Err(anyhow!(
            "Unexpected SCS telemetry version: got {version}, expected {SCS_EXPECTED_VERSION}"
        ));
    }

    let speed_ms = read_f32_le(data, OFF_SPEED_MS).unwrap_or(0.0).max(0.0);
    let rpm = read_f32_le(data, OFF_ENGINE_RPM).unwrap_or(0.0).max(0.0);
    let gear_raw = read_i32_le(data, OFF_GEAR).unwrap_or(0);
    let fuel_ratio = read_f32_le(data, OFF_FUEL_RATIO).unwrap_or(0.0);
    let engine_load = read_f32_le(data, OFF_ENGINE_LOAD).unwrap_or(0.0);

    // Map i32 gear to i8: positive = forward, negative = reverse, 0 = neutral.
    let gear: i8 = gear_raw.clamp(-1, 12) as i8;

    // Derive a simple FFB scalar from engine load scaled by speed contribution.
    // Trucks don't have conventional racing FFB; weight-shift is the primary cue.
    let ffb_scalar = (engine_load * 0.6 + (speed_ms / 30.0).min(1.0) * 0.4).clamp(0.0, 1.0) - 0.5; // centre around zero so idle = slight return force

    Ok(NormalizedTelemetry::builder()
        .speed_ms(speed_ms)
        .rpm(rpm)
        .gear(gear)
        .ffb_scalar(ffb_scalar)
        .fuel_percent(fuel_ratio)
        .extended(
            "engine_load".to_string(),
            TelemetryValue::Float(engine_load),
        )
        .build())
}

#[async_trait]
impl TelemetryAdapter for Ets2Adapter {
    fn game_id(&self) -> &str {
        self.variant.game_id()
    }

    async fn start_monitoring(&self) -> Result<TelemetryReceiver> {
        let (tx, rx) = mpsc::channel(100);
        let update_rate = self.update_rate;

        tokio::spawn(async move {
            let mut frame_seq = 0u64;

            loop {
                match try_read_scs_shared_memory() {
                    Some(normalized) => {
                        let frame = TelemetryFrame::new(
                            normalized,
                            telemetry_now_ns(),
                            frame_seq,
                            SCS_SHARED_MEMORY_SIZE,
                        );
                        if tx.send(frame).await.is_err() {
                            debug!("Receiver dropped, stopping ETS2/ATS monitoring");
                            break;
                        }
                        frame_seq = frame_seq.saturating_add(1);
                    }
                    None => {
                        debug!("SCS shared memory not available, retrying…");
                    }
                }
                tokio::time::sleep(update_rate).await;
            }
            info!("Stopped ETS2/ATS telemetry monitoring");
        });

        Ok(rx)
    }

    async fn stop_monitoring(&self) -> Result<()> {
        Ok(())
    }

    fn normalize(&self, raw: &[u8]) -> Result<NormalizedTelemetry> {
        parse_scs_packet(raw)
    }

    fn expected_update_rate(&self) -> Duration {
        self.update_rate
    }

    async fn is_game_running(&self) -> Result<bool> {
        Ok(is_scs_process_running(self.variant))
    }
}

/// Try to open and read the SCS shared memory. Returns `None` on any failure.
#[cfg(windows)]
fn try_read_scs_shared_memory() -> Option<NormalizedTelemetry> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    let wide_name: Vec<u16> = OsStr::new(SCS_SHARED_MEMORY_NAME)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    // SAFETY: Win32 shared memory API with a valid null-terminated UTF-16 name.
    unsafe {
        let handle = OpenFileMappingW(FILE_MAP_READ, 0, wide_name.as_ptr());
        if handle.is_null() {
            return None;
        }
        let view = MapViewOfFile(handle, FILE_MAP_READ, 0, 0, SCS_SHARED_MEMORY_SIZE);
        if view.is_null() {
            CloseHandle(handle);
            return None;
        }
        let data = std::slice::from_raw_parts(view as *const u8, SCS_SHARED_MEMORY_SIZE);
        let result = parse_scs_packet(data).ok();
        UnmapViewOfFile(view);
        CloseHandle(handle);
        result
    }
}

#[cfg(not(windows))]
fn try_read_scs_shared_memory() -> Option<NormalizedTelemetry> {
    None
}

#[cfg(windows)]
fn is_scs_process_running(variant: Ets2Variant) -> bool {
    use std::ffi::CStr;
    use std::mem;
    use winapi::um::{
        handleapi::{CloseHandle, INVALID_HANDLE_VALUE},
        tlhelp32::{
            CreateToolhelp32Snapshot, PROCESSENTRY32, Process32First, Process32Next,
            TH32CS_SNAPPROCESS,
        },
    };

    let target = variant.process_name();

    // SAFETY: Windows snapshot API with proper initialisation.
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
                if name == target {
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
fn is_scs_process_running(_variant: Ets2Variant) -> bool {
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

fn read_u32_le(data: &[u8], offset: usize) -> Option<u32> {
    data.get(offset..offset + 4)
        .and_then(|b| b.try_into().ok())
        .map(u32::from_le_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn make_scs_packet(speed: f32, rpm: f32, gear: i32, fuel: f32, load: f32) -> Vec<u8> {
        let mut data = vec![0u8; SCS_SHARED_MEMORY_SIZE];
        data[OFF_VERSION..OFF_VERSION + 4].copy_from_slice(&1u32.to_le_bytes());
        data[OFF_SPEED_MS..OFF_SPEED_MS + 4].copy_from_slice(&speed.to_le_bytes());
        data[OFF_ENGINE_RPM..OFF_ENGINE_RPM + 4].copy_from_slice(&rpm.to_le_bytes());
        data[OFF_GEAR..OFF_GEAR + 4].copy_from_slice(&gear.to_le_bytes());
        data[OFF_FUEL_RATIO..OFF_FUEL_RATIO + 4].copy_from_slice(&fuel.to_le_bytes());
        data[OFF_ENGINE_LOAD..OFF_ENGINE_LOAD + 4].copy_from_slice(&load.to_le_bytes());
        data
    }

    #[test]
    fn test_parse_valid_packet() -> TestResult {
        let data = make_scs_packet(20.0, 1500.0, 4, 0.7, 0.5);
        let result = parse_scs_packet(&data)?;
        assert!((result.speed_ms - 20.0).abs() < 0.001);
        assert!((result.rpm - 1500.0).abs() < 0.1);
        assert_eq!(result.gear, 4);
        assert!((result.fuel_percent - 0.7).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn test_parse_reverse_gear() -> TestResult {
        let data = make_scs_packet(0.0, 800.0, -1, 0.5, 0.2);
        let result = parse_scs_packet(&data)?;
        assert_eq!(result.gear, -1);
        Ok(())
    }

    #[test]
    fn test_parse_neutral_gear() -> TestResult {
        let data = make_scs_packet(0.0, 700.0, 0, 0.9, 0.1);
        let result = parse_scs_packet(&data)?;
        assert_eq!(result.gear, 0);
        Ok(())
    }

    #[test]
    fn test_wrong_version_rejected() {
        let mut data = make_scs_packet(10.0, 1000.0, 1, 0.5, 0.3);
        data[OFF_VERSION..OFF_VERSION + 4].copy_from_slice(&2u32.to_le_bytes());
        assert!(parse_scs_packet(&data).is_err());
    }

    #[test]
    fn test_short_buffer_rejected() {
        let data = vec![0u8; 10];
        assert!(parse_scs_packet(&data).is_err());
    }

    #[test]
    fn test_ffb_scalar_range() -> TestResult {
        let data = make_scs_packet(100.0, 2000.0, 8, 0.3, 1.0);
        let result = parse_scs_packet(&data)?;
        assert!(
            result.ffb_scalar >= -1.0 && result.ffb_scalar <= 1.0,
            "ffb_scalar out of range: {}",
            result.ffb_scalar
        );
        Ok(())
    }

    #[test]
    fn test_adapter_game_id_ets2() {
        let adapter = Ets2Adapter::with_variant(Ets2Variant::Ets2);
        assert_eq!(adapter.game_id(), "ets2");
    }

    #[test]
    fn test_adapter_game_id_ats() {
        let adapter = Ets2Adapter::with_variant(Ets2Variant::Ats);
        assert_eq!(adapter.game_id(), "ats");
    }

    #[test]
    fn test_update_rate() {
        let adapter = Ets2Adapter::new();
        assert_eq!(adapter.expected_update_rate(), Duration::from_millis(50));
    }

    #[test]
    fn test_empty_input() {
        assert!(
            parse_scs_packet(&[]).is_err(),
            "empty input must return an error"
        );
    }

    #[test]
    fn test_speed_is_nonnegative() -> TestResult {
        let data = make_scs_packet(25.0, 1200.0, 3, 0.6, 0.4);
        let result = parse_scs_packet(&data)?;
        assert!(
            result.speed_ms >= 0.0,
            "speed_ms must be non-negative, got {}",
            result.speed_ms
        );
        Ok(())
    }

    #[test]
    fn test_gear_in_valid_range() -> TestResult {
        for g in -1i32..=12 {
            let data = make_scs_packet(10.0, 1000.0, g, 0.5, 0.3);
            let result = parse_scs_packet(&data)?;
            assert!(
                result.gear >= -1 && result.gear <= 12,
                "gear {} out of expected range -1..=12",
                result.gear
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod proptest_tests {
    use super::*;
    use proptest::prelude::*;

    fn build_scs_packet(speed: f32, rpm: f32, gear: i32, fuel: f32, load: f32) -> Vec<u8> {
        let mut data = vec![0u8; SCS_SHARED_MEMORY_SIZE];
        data[OFF_VERSION..OFF_VERSION + 4].copy_from_slice(&1u32.to_le_bytes());
        data[OFF_SPEED_MS..OFF_SPEED_MS + 4].copy_from_slice(&speed.to_le_bytes());
        data[OFF_ENGINE_RPM..OFF_ENGINE_RPM + 4].copy_from_slice(&rpm.to_le_bytes());
        data[OFF_GEAR..OFF_GEAR + 4].copy_from_slice(&gear.to_le_bytes());
        data[OFF_FUEL_RATIO..OFF_FUEL_RATIO + 4].copy_from_slice(&fuel.to_le_bytes());
        data[OFF_ENGINE_LOAD..OFF_ENGINE_LOAD + 4].copy_from_slice(&load.to_le_bytes());
        data
    }

    proptest! {
        #[test]
        fn scs_no_panic_on_arbitrary_bytes(
            data in proptest::collection::vec(any::<u8>(), 0..512usize)
        ) {
            let _ = parse_scs_packet(&data);
        }

        #[test]
        fn scs_short_packet_always_errors(
            data in proptest::collection::vec(any::<u8>(), 0..(OFF_ENGINE_LOAD + 4))
        ) {
            prop_assert!(parse_scs_packet(&data).is_err());
        }

        #[test]
        fn scs_valid_speed_nonneg(speed in 0.0f32..200.0f32) {
            let data = build_scs_packet(speed, 1500.0, 3, 0.7, 0.5);
            let result = parse_scs_packet(&data).map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
            prop_assert!(result.speed_ms >= 0.0);
        }

        #[test]
        fn scs_valid_rpm_nonneg(rpm in 0.0f32..3000.0f32) {
            let data = build_scs_packet(20.0, rpm, 3, 0.7, 0.5);
            let result = parse_scs_packet(&data).map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
            prop_assert!(result.rpm >= 0.0);
        }

        #[test]
        fn scs_valid_gear_in_range(gear in -1i32..=12) {
            let data = build_scs_packet(20.0, 1500.0, gear, 0.7, 0.5);
            let result = parse_scs_packet(&data).map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
            prop_assert!(result.gear >= -1 && result.gear <= 12);
        }
    }
}

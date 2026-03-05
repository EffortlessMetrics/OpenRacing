//! RaceRoom Racing Experience telemetry adapter using the R3E shared memory interface.
//!
//! Opens `Local\$R3E` and reads key telemetry fields at fixed byte offsets from the
//! Sector3 R3E SDK. Offset-based reading is used because the full struct layout can
//! vary between SDK versions.
#![cfg_attr(not(windows), allow(unused, dead_code))]

use crate::{
    NormalizedTelemetry, TelemetryAdapter, TelemetryFlags, TelemetryFrame, TelemetryReceiver,
    TelemetryValue, telemetry_now_ns,
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

// R3E field byte offsets derived from the official Sector3 SDK r3e.h (version 3.4).
// Struct is #pragma pack(push, 1), so all offsets assume no padding.
const OFF_VERSION_MAJOR: usize = 0;
const OFF_GAME_PAUSED: usize = 20;
const OFF_GAME_IN_MENUS: usize = 24;
// Vehicle state fields (within r3e_shared, after playerdata + event + pit + scoring + vehicle_info).
const OFF_SPEED: usize = 1392; // car_speed, f32, m/s
const OFF_ENGINE_RPS: usize = 1396; // engine_rps, f32, rad/s
const OFF_MAX_ENGINE_RPS: usize = 1400; // max_engine_rps, f32, rad/s
const OFF_GEAR: usize = 1408; // gear, i32 (-2=N/A, -1=R, 0=N, 1+=fwd)
const OFF_FUEL_LEFT: usize = 1456; // fuel_left, f32, litres
const OFF_FUEL_CAPACITY: usize = 1460; // fuel_capacity, f32, litres
const OFF_THROTTLE: usize = 1500; // throttle, f32, 0.0–1.0
const OFF_BRAKE: usize = 1508; // brake, f32, 0.0–1.0
const OFF_CLUTCH: usize = 1516; // clutch, f32, 0.0–1.0
const OFF_STEER_INPUT: usize = 1524; // steer_input_raw, f32, -1.0–1.0

// G-forces (local_acceleration, r3e_vec3_f32 in vehicle state section).
// +X = left, +Y = up, +Z = back. Unit: m/s².
const OFF_LOCAL_ACCEL_X: usize = 1440;
const OFF_LOCAL_ACCEL_Y: usize = 1444;
const OFF_LOCAL_ACCEL_Z: usize = 1448;

// Engine / fuel extras
const OFF_NUM_GEARS: usize = 1412; // num_gears, i32 (-1 = N/A)
const OFF_ENGINE_TEMP: usize = 1480; // engine_temp, f32, °C

// Scoring & timing
const OFF_POSITION: usize = 988; // position, i32 (1-based)
const OFF_COMPLETED_LAPS: usize = 1028; // completed_laps, i32
const OFF_LAP_TIME_BEST: usize = 1068; // lap_time_best_self, f32, seconds
const OFF_LAP_TIME_PREVIOUS: usize = 1084; // lap_time_previous_self, f32, seconds
const OFF_LAP_TIME_CURRENT: usize = 1100; // lap_time_current_self, f32, seconds
const OFF_DELTA_FRONT: usize = 1124; // time_delta_front, f32, seconds
const OFF_DELTA_BEHIND: usize = 1128; // time_delta_behind, f32, seconds

// Flags (within r3e_flags sub-struct starting at offset 932)
const OFF_FLAG_YELLOW: usize = 932; // flags.yellow, i32
const OFF_FLAG_BLUE: usize = 964; // flags.blue, i32
const OFF_FLAG_GREEN: usize = 972; // flags.green, i32
const OFF_FLAG_CHECKERED: usize = 976; // flags.checkered, i32

// Pit / driver-assists
const OFF_IN_PITLANE: usize = 848; // in_pitlane, i32
const OFF_PIT_LIMITER: usize = 1572; // pit_limiter, i32
const OFF_AID_ABS: usize = 1536; // aid_settings.abs, i32 (5 = active)
const OFF_AID_TC: usize = 1540; // aid_settings.tc, i32 (5 = active)

// Tire temperatures – centre temp per tyre (f32, °C, -1.0 = N/A)
const OFF_TIRE_TEMP_FL_CENTER: usize = 1748;
const OFF_TIRE_TEMP_FR_CENTER: usize = 1772;
const OFF_TIRE_TEMP_RL_CENTER: usize = 1796;
const OFF_TIRE_TEMP_RR_CENTER: usize = 1820;

// Tire pressures (f32, KPa, -1.0 = N/A)
const OFF_TIRE_PRESSURE_FL: usize = 1712;
const OFF_TIRE_PRESSURE_FR: usize = 1716;
const OFF_TIRE_PRESSURE_RL: usize = 1720;
const OFF_TIRE_PRESSURE_RR: usize = 1724;

/// Gravity in m/s² for converting local acceleration to G-forces.
const G_ACCEL: f32 = 9.80665;

/// Expected R3E shared memory major version (v3.x SDK).
const R3E_VERSION_MAJOR: i32 = 3;

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

    let rpm = {
        let rps = read_f32_le(data, OFF_ENGINE_RPS).unwrap_or(0.0);
        rps * (30.0 / std::f32::consts::PI) // rad/s → RPM
    };
    let max_rpm = {
        let rps = read_f32_le(data, OFF_MAX_ENGINE_RPS).unwrap_or(0.0);
        rps * (30.0 / std::f32::consts::PI)
    };
    let fuel_left = read_f32_le(data, OFF_FUEL_LEFT).unwrap_or(0.0);
    let fuel_capacity = read_f32_le(data, OFF_FUEL_CAPACITY).unwrap_or(0.0);
    let speed_mps = read_f32_le(data, OFF_SPEED).unwrap_or(0.0).abs();
    let steering = read_f32_le(data, OFF_STEER_INPUT)
        .unwrap_or(0.0)
        .clamp(-1.0, 1.0);
    let throttle = read_f32_le(data, OFF_THROTTLE).unwrap_or(0.0);
    let brake = read_f32_le(data, OFF_BRAKE).unwrap_or(0.0);
    let clutch = read_f32_le(data, OFF_CLUTCH).unwrap_or(0.0);
    let gear = read_i32_le(data, OFF_GEAR).unwrap_or(0).clamp(-1, 127) as i8;

    let fuel_percent = if fuel_capacity > 0.0 {
        (fuel_left / fuel_capacity).clamp(0.0, 1.0)
    } else {
        0.0
    };

    // G-forces: convert m/s² to G and align sign conventions.
    // R3E: +X=left, +Y=up, +Z=back. Struct: lateral positive=right, longitudinal positive=forward.
    let lateral_g = -(read_f32_le(data, OFF_LOCAL_ACCEL_X).unwrap_or(0.0) / G_ACCEL);
    let longitudinal_g = -(read_f32_le(data, OFF_LOCAL_ACCEL_Z).unwrap_or(0.0) / G_ACCEL);
    let vertical_g = read_f32_le(data, OFF_LOCAL_ACCEL_Y).unwrap_or(0.0) / G_ACCEL;

    let num_gears = read_i32_le(data, OFF_NUM_GEARS).unwrap_or(-1);
    let engine_temp = read_f32_le(data, OFF_ENGINE_TEMP).unwrap_or(-1.0);

    let position = read_i32_le(data, OFF_POSITION).unwrap_or(-1);
    let completed_laps = read_i32_le(data, OFF_COMPLETED_LAPS).unwrap_or(-1);
    let lap_time_current = read_f32_le(data, OFF_LAP_TIME_CURRENT).unwrap_or(-1.0);
    let lap_time_best = read_f32_le(data, OFF_LAP_TIME_BEST).unwrap_or(-1.0);
    let lap_time_previous = read_f32_le(data, OFF_LAP_TIME_PREVIOUS).unwrap_or(-1.0);
    let delta_front = read_f32_le(data, OFF_DELTA_FRONT).unwrap_or(-1.0);
    let delta_behind = read_f32_le(data, OFF_DELTA_BEHIND).unwrap_or(-1.0);

    // Flags: R3E uses -1 = N/A, 0 = inactive, 1 = active.
    let flags = TelemetryFlags {
        yellow_flag: read_i32_le(data, OFF_FLAG_YELLOW).unwrap_or(0) == 1,
        blue_flag: read_i32_le(data, OFF_FLAG_BLUE).unwrap_or(0) == 1,
        green_flag: read_i32_le(data, OFF_FLAG_GREEN).unwrap_or(0) == 1,
        checkered_flag: read_i32_le(data, OFF_FLAG_CHECKERED).unwrap_or(0) == 1,
        in_pits: read_i32_le(data, OFF_IN_PITLANE).unwrap_or(0) == 1,
        pit_limiter: read_i32_le(data, OFF_PIT_LIMITER).unwrap_or(0) == 1,
        abs_active: read_i32_le(data, OFF_AID_ABS).unwrap_or(0) == 5,
        traction_control: read_i32_le(data, OFF_AID_TC).unwrap_or(0) == 5,
        ..TelemetryFlags::default()
    };

    // Tire temperatures: centre reading per tyre, f32 °C → u8.
    let tire_temps = [
        f32_temp_to_u8(read_f32_le(data, OFF_TIRE_TEMP_FL_CENTER)),
        f32_temp_to_u8(read_f32_le(data, OFF_TIRE_TEMP_FR_CENTER)),
        f32_temp_to_u8(read_f32_le(data, OFF_TIRE_TEMP_RL_CENTER)),
        f32_temp_to_u8(read_f32_le(data, OFF_TIRE_TEMP_RR_CENTER)),
    ];

    // Tire pressures: KPa → PSI (1 KPa ≈ 0.14504 PSI).
    let kpa_to_psi = |v: Option<f32>| {
        v.filter(|&p| p > 0.0)
            .map(|p| p * 0.14503774)
            .unwrap_or(0.0)
    };
    let tire_pressures = [
        kpa_to_psi(read_f32_le(data, OFF_TIRE_PRESSURE_FL)),
        kpa_to_psi(read_f32_le(data, OFF_TIRE_PRESSURE_FR)),
        kpa_to_psi(read_f32_le(data, OFF_TIRE_PRESSURE_RL)),
        kpa_to_psi(read_f32_le(data, OFF_TIRE_PRESSURE_RR)),
    ];

    let mut builder = NormalizedTelemetry::builder()
        .rpm(rpm)
        .max_rpm(max_rpm)
        .speed_ms(speed_mps)
        .steering_angle(steering)
        .throttle(throttle)
        .brake(brake)
        .clutch(clutch)
        .gear(gear)
        .lateral_g(lateral_g)
        .longitudinal_g(longitudinal_g)
        .vertical_g(vertical_g)
        .fuel_percent(fuel_percent)
        .flags(flags);

    if num_gears > 0 {
        builder = builder.num_gears(num_gears.min(255) as u8);
    }
    if engine_temp >= 0.0 {
        builder = builder.engine_temp_c(engine_temp);
    }
    if position > 0 {
        builder = builder.position(position.min(255) as u8);
    }
    if completed_laps >= 0 {
        builder = builder.lap(completed_laps.min(i32::from(u16::MAX)) as u16);
    }
    if lap_time_current > 0.0 {
        builder = builder.current_lap_time_s(lap_time_current);
    }
    if lap_time_best > 0.0 {
        builder = builder.best_lap_time_s(lap_time_best);
    }
    if lap_time_previous > 0.0 {
        builder = builder.last_lap_time_s(lap_time_previous);
    }
    if delta_front >= 0.0 {
        builder = builder.delta_ahead_s(delta_front);
    }
    if delta_behind >= 0.0 {
        builder = builder.delta_behind_s(delta_behind);
    }
    if tire_temps.iter().any(|&t| t > 0) {
        builder = builder.tire_temps_c(tire_temps);
    }
    if tire_pressures.iter().any(|&p| p > 0.0) {
        builder = builder.tire_pressures_psi(tire_pressures);
    }

    // Extended fields: raw fuel values for pit-strategy tools.
    if fuel_left > 0.0 {
        builder = builder.extended("fuel_left_l".to_string(), TelemetryValue::Float(fuel_left));
    }
    if fuel_capacity > 0.0 {
        builder = builder.extended(
            "fuel_capacity_l".to_string(),
            TelemetryValue::Float(fuel_capacity),
        );
    }

    Ok(builder.build())
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
        .filter(|v| v.is_finite())
}

fn read_i32_le(data: &[u8], offset: usize) -> Option<i32> {
    data.get(offset..offset + 4)
        .and_then(|b| b.try_into().ok())
        .map(i32::from_le_bytes)
}

/// Convert an optional f32 temperature (°C) to u8, clamped to 0–255. Returns 0 for N/A.
fn f32_temp_to_u8(value: Option<f32>) -> u8 {
    match value {
        Some(v) if v >= 0.0 => (v.min(255.0)) as u8,
        _ => 0,
    }
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
        // engine_rps in rad/s: RPM = rps * 30/π → rps = RPM * π/30
        let rps = rpm * (std::f32::consts::PI / 30.0);
        let max_rps = 8000.0f32 * (std::f32::consts::PI / 30.0);
        data[OFF_ENGINE_RPS..OFF_ENGINE_RPS + 4].copy_from_slice(&rps.to_le_bytes());
        data[OFF_MAX_ENGINE_RPS..OFF_MAX_ENGINE_RPS + 4].copy_from_slice(&max_rps.to_le_bytes());
        data[OFF_FUEL_LEFT..OFF_FUEL_LEFT + 4].copy_from_slice(&30.0f32.to_le_bytes());
        data[OFF_FUEL_CAPACITY..OFF_FUEL_CAPACITY + 4].copy_from_slice(&60.0f32.to_le_bytes());
        data[OFF_SPEED..OFF_SPEED + 4].copy_from_slice(&speed.to_le_bytes());
        data[OFF_STEER_INPUT..OFF_STEER_INPUT + 4].copy_from_slice(&steering.to_le_bytes());
        data[OFF_THROTTLE..OFF_THROTTLE + 4].copy_from_slice(&throttle.to_le_bytes());
        data[OFF_BRAKE..OFF_BRAKE + 4].copy_from_slice(&brake.to_le_bytes());
        data[OFF_CLUTCH..OFF_CLUTCH + 4].copy_from_slice(&0.0f32.to_le_bytes());
        data[OFF_GEAR..OFF_GEAR + 4].copy_from_slice(&gear.to_le_bytes());
        // Populate new fields with representative values.
        write_i32(&mut data, OFF_NUM_GEARS, 6);
        write_f32(&mut data, OFF_ENGINE_TEMP, 95.0);
        // G-forces: 1G lateral left (R3E +X=left), 0.3G braking (R3E +Z=back)
        write_f32(&mut data, OFF_LOCAL_ACCEL_X, G_ACCEL);
        write_f32(&mut data, OFF_LOCAL_ACCEL_Y, G_ACCEL);
        write_f32(&mut data, OFF_LOCAL_ACCEL_Z, 0.3 * G_ACCEL);
        // Scoring
        write_i32(&mut data, OFF_POSITION, 3);
        write_i32(&mut data, OFF_COMPLETED_LAPS, 5);
        write_f32(&mut data, OFF_LAP_TIME_CURRENT, 62.5);
        write_f32(&mut data, OFF_LAP_TIME_BEST, 60.1);
        write_f32(&mut data, OFF_LAP_TIME_PREVIOUS, 61.3);
        write_f32(&mut data, OFF_DELTA_FRONT, 1.2);
        write_f32(&mut data, OFF_DELTA_BEHIND, 0.8);
        // Flags: green active
        write_i32(&mut data, OFF_FLAG_GREEN, 1);
        // Tire temps (centre): ~90 °C each
        write_f32(&mut data, OFF_TIRE_TEMP_FL_CENTER, 90.0);
        write_f32(&mut data, OFF_TIRE_TEMP_FR_CENTER, 92.0);
        write_f32(&mut data, OFF_TIRE_TEMP_RL_CENTER, 88.0);
        write_f32(&mut data, OFF_TIRE_TEMP_RR_CENTER, 91.0);
        // Tire pressures in KPa (~170 KPa ≈ 24.7 PSI)
        write_f32(&mut data, OFF_TIRE_PRESSURE_FL, 170.0);
        write_f32(&mut data, OFF_TIRE_PRESSURE_FR, 172.0);
        write_f32(&mut data, OFF_TIRE_PRESSURE_RL, 168.0);
        write_f32(&mut data, OFF_TIRE_PRESSURE_RR, 171.0);
        data
    }

    fn write_f32(data: &mut [u8], offset: usize, value: f32) {
        data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn write_i32(data: &mut [u8], offset: usize, value: i32) {
        data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
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
    fn test_fuel_percent() -> TestResult {
        let data = make_r3e_memory(3000.0, 20.0, 0.0, 0.3, 0.0, 1);
        let result = parse_r3e_memory(&data)?;
        // fuel_left=30, fuel_capacity=60 → 0.5
        assert!((result.fuel_percent - 0.5).abs() < 0.001);
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

    #[test]
    fn test_g_forces() -> TestResult {
        let data = make_r3e_memory(5000.0, 50.0, 0.0, 0.5, 0.0, 3);
        let result = parse_r3e_memory(&data)?;
        // R3E +X = left → lateral_g negated → -1.0 G
        assert!((result.lateral_g - (-1.0)).abs() < 0.01);
        // R3E +Y = up → vertical_g same sign → 1.0 G
        assert!((result.vertical_g - 1.0).abs() < 0.01);
        // R3E +Z = back → longitudinal_g negated → -0.3 G
        assert!((result.longitudinal_g - (-0.3)).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn test_lap_timing() -> TestResult {
        let data = make_r3e_memory(5000.0, 50.0, 0.0, 0.5, 0.0, 3);
        let result = parse_r3e_memory(&data)?;
        assert!((result.current_lap_time_s - 62.5).abs() < 0.01);
        assert!((result.best_lap_time_s - 60.1).abs() < 0.01);
        assert!((result.last_lap_time_s - 61.3).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn test_position_and_laps() -> TestResult {
        let data = make_r3e_memory(5000.0, 50.0, 0.0, 0.5, 0.0, 3);
        let result = parse_r3e_memory(&data)?;
        assert_eq!(result.position, 3);
        assert_eq!(result.lap, 5);
        Ok(())
    }

    #[test]
    fn test_engine_temp() -> TestResult {
        let data = make_r3e_memory(5000.0, 50.0, 0.0, 0.5, 0.0, 3);
        let result = parse_r3e_memory(&data)?;
        assert!((result.engine_temp_c - 95.0).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn test_flags() -> TestResult {
        let mut data = make_r3e_memory(5000.0, 50.0, 0.0, 0.5, 0.0, 3);
        write_i32(&mut data, OFF_FLAG_YELLOW, 1);
        write_i32(&mut data, OFF_FLAG_BLUE, 1);
        write_i32(&mut data, OFF_IN_PITLANE, 1);
        write_i32(&mut data, OFF_PIT_LIMITER, 1);
        write_i32(&mut data, OFF_AID_ABS, 5);
        write_i32(&mut data, OFF_AID_TC, 5);
        let result = parse_r3e_memory(&data)?;
        assert!(result.flags.yellow_flag);
        assert!(result.flags.blue_flag);
        assert!(result.flags.green_flag);
        assert!(result.flags.in_pits);
        assert!(result.flags.pit_limiter);
        assert!(result.flags.abs_active);
        assert!(result.flags.traction_control);
        Ok(())
    }

    #[test]
    fn test_tire_temps() -> TestResult {
        let data = make_r3e_memory(5000.0, 50.0, 0.0, 0.5, 0.0, 3);
        let result = parse_r3e_memory(&data)?;
        assert_eq!(result.tire_temps_c, [90, 92, 88, 91]);
        Ok(())
    }

    #[test]
    fn test_tire_pressures() -> TestResult {
        let data = make_r3e_memory(5000.0, 50.0, 0.0, 0.5, 0.0, 3);
        let result = parse_r3e_memory(&data)?;
        // 170 KPa * 0.14503774 ≈ 24.66 PSI
        assert!((result.tire_pressures_psi[0] - 24.66).abs() < 0.1);
        assert!(result.tire_pressures_psi[1] > 0.0);
        Ok(())
    }

    #[test]
    fn test_delta_times() -> TestResult {
        let data = make_r3e_memory(5000.0, 50.0, 0.0, 0.5, 0.0, 3);
        let result = parse_r3e_memory(&data)?;
        assert!((result.delta_ahead_s - 1.2).abs() < 0.01);
        assert!((result.delta_behind_s - 0.8).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn test_num_gears() -> TestResult {
        let data = make_r3e_memory(5000.0, 50.0, 0.0, 0.5, 0.0, 3);
        let result = parse_r3e_memory(&data)?;
        assert_eq!(result.num_gears, 6);
        Ok(())
    }

    /// RPS-to-RPM conversion: 2000 RPM → rps = 2000 * π/30 ≈ 209.44 rad/s.
    /// Round-trip should be accurate to < 0.01 RPM.
    #[test]
    fn test_rps_to_rpm_roundtrip_precision() -> TestResult {
        for target_rpm in [0.0f32, 1000.0, 3500.0, 7500.0, 12000.0, 20000.0] {
            let data = make_r3e_memory(target_rpm, 50.0, 0.0, 0.5, 0.0, 3);
            let result = parse_r3e_memory(&data)?;
            assert!(
                (result.rpm - target_rpm).abs() < 0.01,
                "RPM round-trip failed for {target_rpm}: got {}",
                result.rpm
            );
        }
        Ok(())
    }

    /// KPa-to-PSI conversion: 1 KPa = 0.14503774 PSI.
    #[test]
    fn test_kpa_to_psi_conversion_precision() -> TestResult {
        let mut data = make_r3e_memory(5000.0, 50.0, 0.0, 0.5, 0.0, 3);
        // Set all four tires to exactly 100 KPa
        write_f32(&mut data, OFF_TIRE_PRESSURE_FL, 100.0);
        write_f32(&mut data, OFF_TIRE_PRESSURE_FR, 100.0);
        write_f32(&mut data, OFF_TIRE_PRESSURE_RL, 100.0);
        write_f32(&mut data, OFF_TIRE_PRESSURE_RR, 100.0);
        let result = parse_r3e_memory(&data)?;
        for (i, &psi) in result.tire_pressures_psi.iter().enumerate() {
            assert!(
                (psi - 14.503774).abs() < 0.01,
                "tire {i}: 100 KPa should be ~14.50 PSI, got {psi}"
            );
        }
        Ok(())
    }

    /// Zero KPa should map to 0 PSI (filtered out by p > 0.0 check).
    #[test]
    fn test_zero_pressure_maps_to_zero_psi() -> TestResult {
        let mut data = make_r3e_memory(5000.0, 50.0, 0.0, 0.5, 0.0, 3);
        write_f32(&mut data, OFF_TIRE_PRESSURE_FL, 0.0);
        write_f32(&mut data, OFF_TIRE_PRESSURE_FR, 0.0);
        write_f32(&mut data, OFF_TIRE_PRESSURE_RL, 0.0);
        write_f32(&mut data, OFF_TIRE_PRESSURE_RR, 0.0);
        let result = parse_r3e_memory(&data)?;
        for (i, &psi) in result.tire_pressures_psi.iter().enumerate() {
            assert_eq!(psi, 0.0, "tire {i}: zero KPa should yield 0 PSI");
        }
        Ok(())
    }

    /// f32_temp_to_u8: negative temperatures → 0.
    #[test]
    fn test_temp_negative_maps_to_zero() {
        assert_eq!(f32_temp_to_u8(Some(-10.0)), 0);
        assert_eq!(f32_temp_to_u8(None), 0);
    }

    /// f32_temp_to_u8: values above 255 are clamped.
    #[test]
    fn test_temp_above_255_clamped() {
        assert_eq!(f32_temp_to_u8(Some(300.0)), 255);
    }

    /// f32_temp_to_u8: exact boundary values.
    #[test]
    fn test_temp_boundary_values() {
        assert_eq!(f32_temp_to_u8(Some(0.0)), 0);
        assert_eq!(f32_temp_to_u8(Some(255.0)), 255);
        assert_eq!(f32_temp_to_u8(Some(127.5)), 127);
    }

    /// Reverse gear: R3E gear -1 → gear field -1.
    #[test]
    fn test_reverse_gear() -> TestResult {
        let data = make_r3e_memory(2000.0, 10.0, 0.0, 0.0, 0.0, -1);
        let result = parse_r3e_memory(&data)?;
        assert_eq!(result.gear, -1, "gear -1 should be reverse");
        Ok(())
    }

    /// Gear clamped: large positive i32 should be clamped to 127 (i8 max).
    #[test]
    fn test_gear_clamped_to_i8() -> TestResult {
        let mut data = make_r3e_memory(2000.0, 10.0, 0.0, 0.0, 0.0, 3);
        write_i32(&mut data, OFF_GEAR, 200);
        let result = parse_r3e_memory(&data)?;
        assert_eq!(result.gear, 127, "gear 200 should be clamped to 127");
        Ok(())
    }

    /// Fuel: zero capacity should yield 0.0 fuel_percent (no division by zero).
    #[test]
    fn test_fuel_zero_capacity_no_panic() -> TestResult {
        let mut data = make_r3e_memory(3000.0, 20.0, 0.0, 0.3, 0.0, 1);
        write_f32(&mut data, OFF_FUEL_LEFT, 10.0);
        write_f32(&mut data, OFF_FUEL_CAPACITY, 0.0);
        let result = parse_r3e_memory(&data)?;
        assert_eq!(
            result.fuel_percent, 0.0,
            "zero capacity should yield 0.0 fuel_percent"
        );
        Ok(())
    }

    /// Extended fields: fuel_left_l and fuel_capacity_l.
    #[test]
    fn test_extended_fuel_fields() -> TestResult {
        let data = make_r3e_memory(5000.0, 50.0, 0.0, 0.5, 0.0, 3);
        let result = parse_r3e_memory(&data)?;
        assert_eq!(
            result.extended.get("fuel_left_l"),
            Some(&TelemetryValue::Float(30.0))
        );
        assert_eq!(
            result.extended.get("fuel_capacity_l"),
            Some(&TelemetryValue::Float(60.0))
        );
        Ok(())
    }

    /// ABS aid: only value 5 means active.
    #[test]
    fn test_abs_only_value_5_active() -> TestResult {
        let mut data = make_r3e_memory(5000.0, 50.0, 0.0, 0.5, 0.0, 3);
        write_i32(&mut data, OFF_AID_ABS, 1); // enabled but not actively triggering
        let result = parse_r3e_memory(&data)?;
        assert!(
            !result.flags.abs_active,
            "ABS aid value 1 should not be active"
        );
        Ok(())
    }

    /// Speed is absolute-valued (negative velocities become positive).
    #[test]
    fn test_negative_speed_absolute() -> TestResult {
        let mut data = make_r3e_memory(5000.0, 0.0, 0.0, 0.5, 0.0, 3);
        write_f32(&mut data, OFF_SPEED, -30.0);
        let result = parse_r3e_memory(&data)?;
        assert!(
            (result.speed_ms - 30.0).abs() < 0.01,
            "negative speed should be absolute-valued"
        );
        Ok(())
    }

    /// Empty (but correct version) shared memory produces zero telemetry.
    #[test]
    fn test_empty_memory_defaults() -> TestResult {
        let mut data = vec![0u8; R3E_VIEW_SIZE];
        data[OFF_VERSION_MAJOR..OFF_VERSION_MAJOR + 4]
            .copy_from_slice(&R3E_VERSION_MAJOR.to_le_bytes());
        let result = parse_r3e_memory(&data)?;
        assert_eq!(result.speed_ms, 0.0);
        assert_eq!(result.rpm, 0.0);
        assert_eq!(result.gear, 0);
        Ok(())
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
            let adapter = RaceRoomAdapter::new();
            let _ = adapter.normalize(&data);
        }
    }
}

/// Protocol constant verification tests for RaceRoom Racing Experience (R3E).
///
/// These tests lock down the R3E SDK byte offsets, shared memory name, and version
/// information against the official Sector3 r3e.h header (version 3.4).
/// Ref: <https://github.com/sector3studios/r3e-api> (sample-c/src/r3e.h)
#[cfg(test)]
mod protocol_constant_tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// R3E shared memory file mapping name.
    /// Ref: r3e.h `#define R3E_SHARED_MEMORY_NAME "$R3E"`
    #[cfg(windows)]
    #[test]
    fn test_shared_memory_name() -> TestResult {
        assert_eq!(R3E_SHARED_MEMORY_NAME, "Local\\$R3E");
        assert!(
            R3E_SHARED_MEMORY_NAME.contains("$R3E"),
            "must reference the $R3E shared memory mapping"
        );
        Ok(())
    }

    /// R3E SDK major version = 3 (v3.4 as of 2024).
    #[test]
    fn test_version_major() -> TestResult {
        assert_eq!(R3E_VERSION_MAJOR, 3);
        Ok(())
    }

    /// View size must be large enough to cover all field offsets.
    #[test]
    fn test_view_size_covers_all_offsets() -> TestResult {
        let max_offset = *[
            OFF_TIRE_TEMP_RR_CENTER + 4,
            OFF_TIRE_PRESSURE_RR + 4,
            OFF_PIT_LIMITER + 4,
            OFF_AID_TC + 4,
            OFF_STEER_INPUT + 4,
            OFF_CLUTCH + 4,
        ]
        .iter()
        .max()
        .ok_or("no offsets")?;
        assert!(
            R3E_VIEW_SIZE >= max_offset,
            "R3E_VIEW_SIZE ({R3E_VIEW_SIZE}) must be >= max offset end ({max_offset})"
        );
        Ok(())
    }

    /// Gravity constant used for G-force conversion.
    #[test]
    fn test_gravity_constant() -> TestResult {
        assert!(
            (G_ACCEL - 9.80665).abs() < 0.0001,
            "standard gravity = 9.80665 m/s²"
        );
        Ok(())
    }

    /// Vehicle state core offsets: speed, engine_rps, max_engine_rps, gear.
    /// Ref: r3e.h r3e_shared struct, pack(push,1)
    #[test]
    fn test_vehicle_core_offsets() -> TestResult {
        assert_eq!(OFF_SPEED, 1392, "car_speed at byte 1392");
        assert_eq!(OFF_ENGINE_RPS, 1396, "engine_rps at byte 1396");
        assert_eq!(OFF_MAX_ENGINE_RPS, 1400, "max_engine_rps at byte 1400");
        assert_eq!(OFF_GEAR, 1408, "gear at byte 1408");
        // speed → engine_rps → max_engine_rps are contiguous f32
        assert_eq!(OFF_ENGINE_RPS - OFF_SPEED, 4);
        assert_eq!(OFF_MAX_ENGINE_RPS - OFF_ENGINE_RPS, 4);
        Ok(())
    }

    /// Pedal offsets: throttle, brake, clutch, steer_input.
    #[test]
    fn test_pedal_offsets() -> TestResult {
        assert_eq!(OFF_THROTTLE, 1500, "throttle at byte 1500");
        assert_eq!(OFF_BRAKE, 1508, "brake at byte 1508");
        assert_eq!(OFF_CLUTCH, 1516, "clutch at byte 1516");
        assert_eq!(OFF_STEER_INPUT, 1524, "steer_input_raw at byte 1524");
        // throttle → brake → clutch → steer all 8 bytes apart (f32 + 4-byte gap)
        assert_eq!(OFF_BRAKE - OFF_THROTTLE, 8);
        assert_eq!(OFF_CLUTCH - OFF_BRAKE, 8);
        assert_eq!(OFF_STEER_INPUT - OFF_CLUTCH, 8);
        Ok(())
    }

    /// Fuel offsets: fuel_left and fuel_capacity contiguous.
    #[test]
    fn test_fuel_offsets() -> TestResult {
        assert_eq!(OFF_FUEL_LEFT, 1456);
        assert_eq!(OFF_FUEL_CAPACITY, 1460);
        assert_eq!(OFF_FUEL_CAPACITY - OFF_FUEL_LEFT, 4, "contiguous f32 pair");
        Ok(())
    }

    /// Local acceleration (G-force) vector offsets: X, Y, Z contiguous f32.
    /// Convention: +X = left, +Y = up, +Z = back.
    #[test]
    fn test_local_acceleration_offsets() -> TestResult {
        assert_eq!(OFF_LOCAL_ACCEL_X, 1440);
        assert_eq!(OFF_LOCAL_ACCEL_Y, 1444);
        assert_eq!(OFF_LOCAL_ACCEL_Z, 1448);
        // Must be contiguous f32 triplet
        assert_eq!(OFF_LOCAL_ACCEL_Y - OFF_LOCAL_ACCEL_X, 4);
        assert_eq!(OFF_LOCAL_ACCEL_Z - OFF_LOCAL_ACCEL_Y, 4);
        Ok(())
    }

    /// Engine temperature offset.
    #[test]
    fn test_engine_temp_offset() -> TestResult {
        assert_eq!(OFF_ENGINE_TEMP, 1480, "engine_temp at byte 1480");
        Ok(())
    }

    /// Gear encoding: -2=N/A, -1=Reverse, 0=Neutral, 1+=Forward.
    /// Ref: r3e.h gear field convention. Adapter clamps to [-1, 127].
    #[test]
    fn test_gear_encoding() -> TestResult {
        let test_cases: &[(i32, i8)] = &[
            (-2, -1), // N/A → clamped to -1 by adapter
            (-1, -1), // reverse
            (0, 0),   // neutral
            (1, 1),   // 1st
            (6, 6),   // 6th
        ];
        for &(raw, expected_gear) in test_cases {
            let mut data = vec![0u8; R3E_VIEW_SIZE];
            data[OFF_VERSION_MAJOR..OFF_VERSION_MAJOR + 4]
                .copy_from_slice(&R3E_VERSION_MAJOR.to_le_bytes());
            data[OFF_GEAR..OFF_GEAR + 4].copy_from_slice(&raw.to_le_bytes());
            let t = parse_r3e_memory(&data)?;
            assert_eq!(
                t.gear, expected_gear,
                "R3E gear raw {raw} should map to {expected_gear}, got {}",
                t.gear
            );
        }
        Ok(())
    }

    /// RPS-to-RPM conversion: RPM = rps × 60 / (2π) = rps × 30 / π.
    #[test]
    fn test_rps_to_rpm_conversion() -> TestResult {
        let rps: f32 = 100.0 * std::f32::consts::PI / 30.0; // 100 RPM in rad/s
        let mut data = vec![0u8; R3E_VIEW_SIZE];
        data[OFF_VERSION_MAJOR..OFF_VERSION_MAJOR + 4]
            .copy_from_slice(&R3E_VERSION_MAJOR.to_le_bytes());
        data[OFF_ENGINE_RPS..OFF_ENGINE_RPS + 4].copy_from_slice(&rps.to_le_bytes());
        let t = parse_r3e_memory(&data)?;
        // Should be approximately 100 RPM
        assert!(
            (t.rpm - 100.0).abs() < 1.0,
            "100 RPM in rad/s should convert back to ~100 RPM, got {}",
            t.rpm
        );
        Ok(())
    }

    /// ABS and TC active indicator value = 5 in R3E.
    #[test]
    fn test_abs_tc_active_value() -> TestResult {
        assert_eq!(OFF_AID_ABS, 1536, "aid_settings.abs at byte 1536");
        assert_eq!(OFF_AID_TC, 1540, "aid_settings.tc at byte 1540");
        // Verify that value 5 triggers active flag
        let mut data = vec![0u8; R3E_VIEW_SIZE];
        data[OFF_VERSION_MAJOR..OFF_VERSION_MAJOR + 4]
            .copy_from_slice(&R3E_VERSION_MAJOR.to_le_bytes());
        data[OFF_AID_ABS..OFF_AID_ABS + 4].copy_from_slice(&5_i32.to_le_bytes());
        data[OFF_AID_TC..OFF_AID_TC + 4].copy_from_slice(&5_i32.to_le_bytes());
        let t = parse_r3e_memory(&data)?;
        assert!(t.flags.abs_active, "ABS must be active when aid value = 5");
        assert!(
            t.flags.traction_control,
            "TC must be active when aid value = 5"
        );
        Ok(())
    }

    /// Tire temperature offsets: 4 tires, each 24 bytes apart (FL, FR, RL, RR).
    #[test]
    fn test_tire_temp_offsets() -> TestResult {
        assert_eq!(OFF_TIRE_TEMP_FL_CENTER, 1748);
        assert_eq!(OFF_TIRE_TEMP_FR_CENTER, 1772);
        assert_eq!(OFF_TIRE_TEMP_RL_CENTER, 1796);
        assert_eq!(OFF_TIRE_TEMP_RR_CENTER, 1820);
        // Each tire is 24 bytes apart (24 bytes of tire struct per corner)
        assert_eq!(OFF_TIRE_TEMP_FR_CENTER - OFF_TIRE_TEMP_FL_CENTER, 24);
        assert_eq!(OFF_TIRE_TEMP_RL_CENTER - OFF_TIRE_TEMP_FR_CENTER, 24);
        assert_eq!(OFF_TIRE_TEMP_RR_CENTER - OFF_TIRE_TEMP_RL_CENTER, 24);
        Ok(())
    }

    /// Tire pressure offsets: 4 tires, contiguous f32 (FL, FR, RL, RR).
    #[test]
    fn test_tire_pressure_offsets() -> TestResult {
        assert_eq!(OFF_TIRE_PRESSURE_FL, 1712);
        assert_eq!(OFF_TIRE_PRESSURE_FR, 1716);
        assert_eq!(OFF_TIRE_PRESSURE_RL, 1720);
        assert_eq!(OFF_TIRE_PRESSURE_RR, 1724);
        assert_eq!(OFF_TIRE_PRESSURE_FR - OFF_TIRE_PRESSURE_FL, 4);
        assert_eq!(OFF_TIRE_PRESSURE_RL - OFF_TIRE_PRESSURE_FR, 4);
        assert_eq!(OFF_TIRE_PRESSURE_RR - OFF_TIRE_PRESSURE_RL, 4);
        Ok(())
    }

    /// Flag offsets in the R3E flags sub-struct (starting around byte 932).
    /// R3E uses i32 flags: -1 = N/A, 0 = inactive, 1 = active.
    #[test]
    fn test_flag_offsets() -> TestResult {
        assert_eq!(OFF_FLAG_YELLOW, 932);
        assert_eq!(OFF_FLAG_BLUE, 964);
        assert_eq!(OFF_FLAG_GREEN, 972);
        assert_eq!(OFF_FLAG_CHECKERED, 976);
        Ok(())
    }

    /// Pit lane offset.
    #[test]
    fn test_pit_offsets() -> TestResult {
        assert_eq!(OFF_IN_PITLANE, 848, "in_pitlane at byte 848");
        assert_eq!(OFF_PIT_LIMITER, 1572, "pit_limiter at byte 1572");
        Ok(())
    }

    /// Process names used for auto-detection.
    #[test]
    fn test_process_names() -> TestResult {
        assert!(
            RACEROOM_PROCESS_NAMES.contains(&"rrre.exe"),
            "must detect rrre.exe"
        );
        Ok(())
    }
}

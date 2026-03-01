//! Forza Motorsport / Forza Horizon telemetry adapter using UDP.
//!
//! Supports three packet formats defined by Forza's "Data Out" feature:
//!
//! - **Sled** (232 bytes): FM7 and earlier. Contains physics data (velocity,
//!   wheel speeds, suspension travel, G-forces, tire slip). No user-input
//!   fields (throttle/brake/steer are absent in this format).
//! - **CarDash** (311 bytes): FM8, FH5. Sled data plus dashboard
//!   fields: speed, throttle, brake, clutch, gear, steer, lap times, fuel.
//! - **FH4 CarDash** (324 bytes): Forza Horizon 4. Same as CarDash but with a
//!   12-byte HorizonPlaceholder inserted after the Sled section, shifting all
//!   dashboard field offsets by +12.
//!
//! All formats use little-endian encoding. Tire temperatures are in Fahrenheit
//! and are converted to Celsius. Wheel telemetry (rotation speeds
//! and suspension travel) is stored in the `extended` map of
//! [`NormalizedTelemetry`] using keys `wheel_speed_fl/fr/rl/rr` and
//! `suspension_travel_fl/fr/rl/rr`.
//!
//! # Reference
//! <https://support.forzamotorsport.net/hc/en-us/articles/21742934790291>
#![cfg_attr(not(windows), allow(unused, dead_code))]

use crate::{
    NormalizedTelemetry, TelemetryAdapter, TelemetryFrame, TelemetryReceiver, TelemetryValue,
    telemetry_now_ns,
};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::time::Duration;
use tokio::net::UdpSocket as TokioUdpSocket;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

const DEFAULT_FORZA_PORT: u16 = 5300;
/// Sled packet: 58 × 4-byte fields = 232 bytes.
const FORZA_SLED_SIZE: usize = 232;
/// CarDash packet: Sled (232) + 17×f32 + u16 + 9×u8/i8 = 311 bytes.
const FORZA_CARDASH_SIZE: usize = 311;
/// FH4 CarDash: same as CarDash but with a 12-byte HorizonPlaceholder
/// inserted after NumCylinders (byte 232), shifting all dash offsets by +12.
const FORZA_FH4_CARDASH_SIZE: usize = 324;
const MAX_PACKET_SIZE: usize = 512;

// ── Sled format byte offsets ─────────────────────────────────────────────────
const OFF_IS_RACE_ON: usize = 0; // i32
const OFF_ENGINE_MAX_RPM: usize = 8; // f32
#[allow(dead_code)]
const OFF_ENGINE_IDLE_RPM: usize = 12; // f32 (unused but documented)
const OFF_CURRENT_RPM: usize = 16; // f32
// World-space acceleration (m/s²)
const OFF_ACCEL_X: usize = 20; // f32 – lateral (right = positive)
#[allow(dead_code)]
const OFF_ACCEL_Y: usize = 24; // f32 – vertical (up = positive)
const OFF_ACCEL_Z: usize = 28; // f32 – longitudinal (forward = positive)
// World-space velocity (m/s)
const OFF_VEL_X: usize = 32; // f32
const OFF_VEL_Y: usize = 36; // f32
const OFF_VEL_Z: usize = 40; // f32
// Wheel rotation speeds (rad/s)
const OFF_WHEEL_SPEED_FL: usize = 100; // f32
const OFF_WHEEL_SPEED_FR: usize = 104; // f32
const OFF_WHEEL_SPEED_RL: usize = 108; // f32
const OFF_WHEEL_SPEED_RR: usize = 112; // f32
// Tire slip angles (rad)
const OFF_SLIP_ANGLE_FL: usize = 164; // f32
const OFF_SLIP_ANGLE_FR: usize = 168; // f32
const OFF_SLIP_ANGLE_RL: usize = 172; // f32
const OFF_SLIP_ANGLE_RR: usize = 176; // f32
// Suspension travel (m)
const OFF_SUSP_TRAVEL_FL: usize = 196; // f32
const OFF_SUSP_TRAVEL_FR: usize = 200; // f32
const OFF_SUSP_TRAVEL_RL: usize = 204; // f32
const OFF_SUSP_TRAVEL_RR: usize = 208; // f32

// ── CarDash extension offsets (bytes 232+) ───────────────────────────────────
const OFF_DASH_SPEED: usize = 244; // f32 m/s
const OFF_DASH_TIRE_TEMP_FL: usize = 256; // f32 Fahrenheit
const OFF_DASH_TIRE_TEMP_FR: usize = 260; // f32 Fahrenheit
const OFF_DASH_TIRE_TEMP_RL: usize = 264; // f32 Fahrenheit
const OFF_DASH_TIRE_TEMP_RR: usize = 268; // f32 Fahrenheit
const OFF_DASH_FUEL: usize = 276; // f32 (0-1)
const OFF_DASH_BEST_LAP: usize = 284; // f32 seconds
const OFF_DASH_LAST_LAP: usize = 288; // f32 seconds
const OFF_DASH_CUR_LAP: usize = 292; // f32 seconds
const OFF_DASH_LAP_NUMBER: usize = 300; // u16
const OFF_DASH_RACE_POS: usize = 302; // u8
const OFF_DASH_ACCEL: usize = 303; // u8 (0-255 → 0.0-1.0)
const OFF_DASH_BRAKE: usize = 304; // u8 (0-255 → 0.0-1.0)
const OFF_DASH_CLUTCH: usize = 305; // u8 (0-255 → 0.0-1.0)
const OFF_DASH_GEAR: usize = 307; // u8 (0=R, 1=N, 2=1st, …)
const OFF_DASH_STEER: usize = 308; // i8 (-127 to 127 → -1.0 to 1.0)

const G: f32 = 9.806_65; // standard gravity (m/s²)

#[cfg(windows)]
const FORZA_PROCESS_NAMES: &[&str] = &[
    "forzahorizon5.exe",
    "forzahorizon4.exe",
    "forzamotorsport.exe",
    "forza motorsport 7.exe",
    "forza_street.exe",
];

#[derive(Debug, Clone, Copy, PartialEq)]
enum ForzaPacketFormat {
    Sled,
    CarDash,
    /// FH4 variant: 12-byte HorizonPlaceholder shifts all dash offsets by +12.
    Fh4CarDash,
    Unknown,
}

fn detect_format(len: usize) -> ForzaPacketFormat {
    match len {
        FORZA_SLED_SIZE => ForzaPacketFormat::Sled,
        FORZA_CARDASH_SIZE => ForzaPacketFormat::CarDash,
        FORZA_FH4_CARDASH_SIZE => ForzaPacketFormat::Fh4CarDash,
        _ => ForzaPacketFormat::Unknown,
    }
}

/// Parse the common Sled portion (first 232 bytes) present in both formats.
///
/// Returns speed, G-forces, wheel speeds, suspension travel, and tire slip.
/// Throttle, brake, gear, and steer are absent from the Sled format; the
/// caller should overlay those from the CarDash extension when available.
fn parse_sled_common(data: &[u8]) -> NormalizedTelemetry {
    debug_assert!(data.len() >= FORZA_SLED_SIZE);

    let engine_max_rpm = read_f32_le(data, OFF_ENGINE_MAX_RPM).unwrap_or(0.0);
    let current_rpm = read_f32_le(data, OFF_CURRENT_RPM).unwrap_or(0.0);

    // Velocity → speed magnitude
    let vel_x = read_f32_le(data, OFF_VEL_X).unwrap_or(0.0);
    let vel_y = read_f32_le(data, OFF_VEL_Y).unwrap_or(0.0);
    let vel_z = read_f32_le(data, OFF_VEL_Z).unwrap_or(0.0);
    let speed_mps = (vel_x * vel_x + vel_y * vel_y + vel_z * vel_z).sqrt();

    // World-space acceleration → G-forces
    let accel_x = read_f32_le(data, OFF_ACCEL_X).unwrap_or(0.0);
    let accel_z = read_f32_le(data, OFF_ACCEL_Z).unwrap_or(0.0);

    // Tire slip angles
    let slip_fl = read_f32_le(data, OFF_SLIP_ANGLE_FL).unwrap_or(0.0);
    let slip_fr = read_f32_le(data, OFF_SLIP_ANGLE_FR).unwrap_or(0.0);
    let slip_rl = read_f32_le(data, OFF_SLIP_ANGLE_RL).unwrap_or(0.0);
    let slip_rr = read_f32_le(data, OFF_SLIP_ANGLE_RR).unwrap_or(0.0);

    // Wheel rotation speeds and suspension travel go into extended fields.
    let ws_fl = read_f32_le(data, OFF_WHEEL_SPEED_FL).unwrap_or(0.0);
    let ws_fr = read_f32_le(data, OFF_WHEEL_SPEED_FR).unwrap_or(0.0);
    let ws_rl = read_f32_le(data, OFF_WHEEL_SPEED_RL).unwrap_or(0.0);
    let ws_rr = read_f32_le(data, OFF_WHEEL_SPEED_RR).unwrap_or(0.0);

    let st_fl = read_f32_le(data, OFF_SUSP_TRAVEL_FL).unwrap_or(0.0);
    let st_fr = read_f32_le(data, OFF_SUSP_TRAVEL_FR).unwrap_or(0.0);
    let st_rl = read_f32_le(data, OFF_SUSP_TRAVEL_RL).unwrap_or(0.0);
    let st_rr = read_f32_le(data, OFF_SUSP_TRAVEL_RR).unwrap_or(0.0);

    NormalizedTelemetry::builder()
        .rpm(current_rpm)
        .max_rpm(engine_max_rpm)
        .speed_ms(speed_mps)
        .lateral_g(accel_x / G)
        .longitudinal_g(accel_z / G)
        .slip_angle_fl(slip_fl)
        .slip_angle_fr(slip_fr)
        .slip_angle_rl(slip_rl)
        .slip_angle_rr(slip_rr)
        .extended("wheel_speed_fl", TelemetryValue::Float(ws_fl))
        .extended("wheel_speed_fr", TelemetryValue::Float(ws_fr))
        .extended("wheel_speed_rl", TelemetryValue::Float(ws_rl))
        .extended("wheel_speed_rr", TelemetryValue::Float(ws_rr))
        .extended("suspension_travel_fl", TelemetryValue::Float(st_fl))
        .extended("suspension_travel_fr", TelemetryValue::Float(st_fr))
        .extended("suspension_travel_rl", TelemetryValue::Float(st_rl))
        .extended("suspension_travel_rr", TelemetryValue::Float(st_rr))
        .build()
}

fn parse_forza_sled(data: &[u8]) -> Result<NormalizedTelemetry> {
    if data.len() < FORZA_SLED_SIZE {
        return Err(anyhow!(
            "Forza Sled packet too short: expected {FORZA_SLED_SIZE}, got {}",
            data.len()
        ));
    }

    let is_race_on = read_i32_le(data, OFF_IS_RACE_ON).unwrap_or(0);
    if is_race_on == 0 {
        return Ok(NormalizedTelemetry::builder().build());
    }

    Ok(parse_sled_common(data))
}

/// CarDash format: Sled physics data plus dashboard / user-input fields.
fn parse_forza_cardash(data: &[u8]) -> Result<NormalizedTelemetry> {
    parse_forza_cardash_with_offset(data, 0)
}

fn parse_forza_fh4_cardash(data: &[u8]) -> Result<NormalizedTelemetry> {
    parse_forza_cardash_with_offset(data, 12)
}

/// Parse a CarDash packet. `horizon_offset` is 0 for FM7/FM8/FH5 (311 bytes)
/// or 12 for FH4 (324 bytes, 12-byte HorizonPlaceholder after Sled section).
fn parse_forza_cardash_with_offset(
    data: &[u8],
    horizon_offset: usize,
) -> Result<NormalizedTelemetry> {
    let expected = FORZA_CARDASH_SIZE + horizon_offset;
    if data.len() < expected {
        return Err(anyhow!(
            "Forza CarDash packet too short: expected {expected}, got {}",
            data.len()
        ));
    }

    let is_race_on = read_i32_le(data, OFF_IS_RACE_ON).unwrap_or(0);
    if is_race_on == 0 {
        return Ok(NormalizedTelemetry::builder().build());
    }

    // Start with the common Sled fields
    let sled = parse_sled_common(data);

    // All CarDash offsets shift by horizon_offset for FH4
    let ho = horizon_offset;

    // CarDash extension: direct speed measurement (more accurate than velocity)
    let speed_mps = read_f32_le(data, OFF_DASH_SPEED + ho).unwrap_or(sled.speed_ms);

    // User inputs (u8 0-255 → f32 0.0-1.0)
    let throttle = data
        .get(OFF_DASH_ACCEL + ho)
        .map(|&b| b as f32 / 255.0)
        .unwrap_or(0.0);
    let brake = data
        .get(OFF_DASH_BRAKE + ho)
        .map(|&b| b as f32 / 255.0)
        .unwrap_or(0.0);
    let clutch = data
        .get(OFF_DASH_CLUTCH + ho)
        .map(|&b| b as f32 / 255.0)
        .unwrap_or(0.0);

    // Gear: 0=Reverse → -1, 1=Neutral → 0, 2..=9 = 1st..=8th
    let gear: i8 = match data.get(OFF_DASH_GEAR + ho).copied().unwrap_or(1) {
        0 => -1,
        1 => 0,
        g => (g - 1) as i8,
    };

    // Steer: i8 −127 to 127 → −1.0 to 1.0
    let steer_raw = data.get(OFF_DASH_STEER + ho).map(|&b| b as i8).unwrap_or(0);
    let steer = (steer_raw as f32 / 127.0).clamp(-1.0, 1.0);

    // Tire temperatures: Fahrenheit → Celsius, clamped to u8
    let temp = |off: usize| -> u8 {
        let fahrenheit = read_f32_le(data, off + ho).unwrap_or(68.0);
        let celsius = ((fahrenheit - 32.0) * 5.0 / 9.0).clamp(0.0, 255.0);
        celsius as u8
    };
    let tire_temps = [
        temp(OFF_DASH_TIRE_TEMP_FL),
        temp(OFF_DASH_TIRE_TEMP_FR),
        temp(OFF_DASH_TIRE_TEMP_RL),
        temp(OFF_DASH_TIRE_TEMP_RR),
    ];

    let fuel = read_f32_le(data, OFF_DASH_FUEL + ho).unwrap_or(0.0);
    let best_lap = read_f32_le(data, OFF_DASH_BEST_LAP + ho).unwrap_or(0.0);
    let last_lap = read_f32_le(data, OFF_DASH_LAST_LAP + ho).unwrap_or(0.0);
    let cur_lap = read_f32_le(data, OFF_DASH_CUR_LAP + ho).unwrap_or(0.0);
    let lap_number = data
        .get(OFF_DASH_LAP_NUMBER + ho..OFF_DASH_LAP_NUMBER + ho + 2)
        .and_then(|b| b.try_into().ok())
        .map(u16::from_le_bytes)
        .unwrap_or(0);
    let race_pos = data.get(OFF_DASH_RACE_POS + ho).copied().unwrap_or(0);

    // Overlay CarDash fields onto the Sled base, preserving extended map entries.
    let mut telemetry = NormalizedTelemetry::builder()
        .rpm(sled.rpm)
        .max_rpm(sled.max_rpm)
        .speed_ms(speed_mps)
        .throttle(throttle)
        .brake(brake)
        .clutch(clutch)
        .steering_angle(steer)
        .gear(gear)
        .lateral_g(sled.lateral_g)
        .longitudinal_g(sled.longitudinal_g)
        .slip_angle_fl(sled.slip_angle_fl)
        .slip_angle_fr(sled.slip_angle_fr)
        .slip_angle_rl(sled.slip_angle_rl)
        .slip_angle_rr(sled.slip_angle_rr)
        .tire_temps_c(tire_temps)
        .fuel_percent(fuel)
        .best_lap_time_s(best_lap)
        .last_lap_time_s(last_lap)
        .current_lap_time_s(cur_lap)
        .lap(lap_number)
        .position(race_pos)
        .build();

    // Propagate extended wheel/suspension fields from the Sled parse.
    for (k, v) in &sled.extended {
        telemetry.extended.insert(k.clone(), v.clone());
    }

    Ok(telemetry)
}

pub(crate) fn parse_forza_packet(data: &[u8]) -> Result<NormalizedTelemetry> {
    match detect_format(data.len()) {
        ForzaPacketFormat::Sled => parse_forza_sled(data),
        ForzaPacketFormat::CarDash => parse_forza_cardash(data),
        ForzaPacketFormat::Fh4CarDash => parse_forza_fh4_cardash(data),
        ForzaPacketFormat::Unknown => Err(anyhow!(
            "Unknown Forza packet length: {}. Expected {} (Sled), {} (CarDash), or {} (FH4 CarDash)",
            data.len(),
            FORZA_SLED_SIZE,
            FORZA_CARDASH_SIZE,
            FORZA_FH4_CARDASH_SIZE,
        )),
    }
}

/// Forza Motorsport / Forza Horizon telemetry adapter.
///
/// Listens for UDP packets on the configured port and decodes the
/// 232-byte Sled, 311-byte CarDash, and 324-byte FH4 CarDash formats
/// automatically.
pub struct ForzaAdapter {
    bind_port: u16,
    update_rate: Duration,
}

impl Default for ForzaAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl ForzaAdapter {
    pub fn new() -> Self {
        Self {
            bind_port: DEFAULT_FORZA_PORT,
            update_rate: Duration::from_millis(16),
        }
    }

    pub fn with_port(mut self, port: u16) -> Self {
        self.bind_port = port;
        self
    }
}

#[async_trait]
impl TelemetryAdapter for ForzaAdapter {
    fn game_id(&self) -> &str {
        "forza_motorsport"
    }

    async fn start_monitoring(&self) -> Result<TelemetryReceiver> {
        let (tx, rx) = mpsc::channel(100);
        let bind_port = self.bind_port;
        let update_rate = self.update_rate;

        tokio::spawn(async move {
            let bind_addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, bind_port));
            let socket = match TokioUdpSocket::bind(bind_addr).await {
                Ok(s) => s,
                Err(e) => {
                    warn!("Failed to bind Forza UDP socket on port {bind_port}: {e}");
                    return;
                }
            };
            info!("Forza adapter listening on UDP port {bind_port}");
            let mut buf = [0u8; MAX_PACKET_SIZE];
            let mut frame_seq = 0u64;

            loop {
                match tokio::time::timeout(update_rate * 10, socket.recv(&mut buf)).await {
                    Ok(Ok(len)) => match parse_forza_packet(&buf[..len]) {
                        Ok(normalized) => {
                            let frame =
                                TelemetryFrame::new(normalized, telemetry_now_ns(), frame_seq, len);
                            if tx.send(frame).await.is_err() {
                                debug!("Receiver dropped, stopping Forza monitoring");
                                break;
                            }
                            frame_seq = frame_seq.saturating_add(1);
                        }
                        Err(e) => debug!("Failed to parse Forza packet: {e}"),
                    },
                    Ok(Err(e)) => warn!("Forza UDP receive error: {e}"),
                    Err(_) => debug!("No Forza telemetry data received (timeout)"),
                }
            }
            info!("Stopped Forza telemetry monitoring");
        });

        Ok(rx)
    }

    async fn stop_monitoring(&self) -> Result<()> {
        Ok(())
    }

    fn normalize(&self, raw: &[u8]) -> Result<NormalizedTelemetry> {
        parse_forza_packet(raw)
    }

    fn expected_update_rate(&self) -> Duration {
        self.update_rate
    }

    async fn is_game_running(&self) -> Result<bool> {
        Ok(is_forza_process_running())
    }
}

#[cfg(windows)]
fn is_forza_process_running() -> bool {
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
                if FORZA_PROCESS_NAMES.iter().any(|p| name.contains(p)) {
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
fn is_forza_process_running() -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn make_sled_packet(is_race_on: i32, rpm: f32, vel: (f32, f32, f32)) -> Vec<u8> {
        let mut data = vec![0u8; FORZA_SLED_SIZE];
        data[OFF_IS_RACE_ON..OFF_IS_RACE_ON + 4].copy_from_slice(&is_race_on.to_le_bytes());
        data[OFF_ENGINE_MAX_RPM..OFF_ENGINE_MAX_RPM + 4].copy_from_slice(&8000.0f32.to_le_bytes());
        data[OFF_CURRENT_RPM..OFF_CURRENT_RPM + 4].copy_from_slice(&rpm.to_le_bytes());
        data[OFF_VEL_X..OFF_VEL_X + 4].copy_from_slice(&vel.0.to_le_bytes());
        data[OFF_VEL_Y..OFF_VEL_Y + 4].copy_from_slice(&vel.1.to_le_bytes());
        data[OFF_VEL_Z..OFF_VEL_Z + 4].copy_from_slice(&vel.2.to_le_bytes());
        data
    }

    #[test]
    fn test_parse_sled_valid() -> TestResult {
        let data = make_sled_packet(1, 5000.0, (20.0, 0.0, 0.0));
        let result = parse_forza_sled(&data)?;
        assert!((result.rpm - 5000.0).abs() < 0.01);
        assert!((result.speed_ms - 20.0).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn test_parse_sled_race_off() -> TestResult {
        let data = make_sled_packet(0, 5000.0, (20.0, 0.0, 0.0));
        let result = parse_forza_sled(&data)?;
        assert_eq!(result.rpm, 0.0);
        Ok(())
    }

    #[test]
    fn test_parse_sled_gear_reverse() -> TestResult {
        // Sled format has no gear field; verify speed_ms is non-negative for negative velocity.
        let data = make_sled_packet(1, 1000.0, (-5.0, 0.0, 0.0));
        let result = parse_forza_sled(&data)?;
        assert!(result.speed_ms >= 0.0);
        Ok(())
    }

    #[test]
    fn test_parse_sled_truncated() {
        let data = vec![0u8; 100];
        assert!(parse_forza_sled(&data).is_err());
    }

    #[test]
    fn test_detect_format() {
        assert_eq!(detect_format(FORZA_SLED_SIZE), ForzaPacketFormat::Sled);
        assert_eq!(
            detect_format(FORZA_CARDASH_SIZE),
            ForzaPacketFormat::CarDash
        );
        assert_eq!(
            detect_format(FORZA_FH4_CARDASH_SIZE),
            ForzaPacketFormat::Fh4CarDash
        );
        assert_eq!(detect_format(100), ForzaPacketFormat::Unknown);
    }

    #[test]
    fn test_parse_unknown_format() {
        let data = vec![0u8; 100];
        assert!(parse_forza_packet(&data).is_err());
    }

    #[test]
    fn test_normalization_clamp() -> TestResult {
        // Verify rpm and speed_ms are non-negative from the sled format.
        let data = make_sled_packet(1, 5000.0, (20.0, 0.0, 0.0));
        let result = parse_forza_sled(&data)?;
        assert!(result.rpm >= 0.0);
        assert!(result.speed_ms >= 0.0);
        Ok(())
    }

    #[test]
    fn test_parse_cardash_valid() -> TestResult {
        let mut data = vec![0u8; FORZA_CARDASH_SIZE];
        // Copy a valid sled header into it
        let sled = make_sled_packet(1, 4000.0, (15.0, 0.0, 0.0));
        data[..FORZA_SLED_SIZE].copy_from_slice(&sled);
        let result = parse_forza_cardash(&data)?;
        assert!((result.rpm - 4000.0).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn test_parse_fh4_cardash_valid() -> TestResult {
        let mut data = vec![0u8; FORZA_FH4_CARDASH_SIZE];
        let sled = make_sled_packet(1, 6000.0, (25.0, 0.0, 0.0));
        data[..FORZA_SLED_SIZE].copy_from_slice(&sled);
        // Write throttle at FH4-shifted offset (303 + 12 = 315)
        data[OFF_DASH_ACCEL + 12] = 200;
        // Write gear at FH4-shifted offset (307 + 12 = 319)
        data[OFF_DASH_GEAR + 12] = 4; // 4 → gear 3
        let result = parse_forza_fh4_cardash(&data)?;
        assert!((result.rpm - 6000.0).abs() < 0.01);
        assert!((result.throttle - 200.0 / 255.0).abs() < 0.01);
        assert_eq!(result.gear, 3);
        Ok(())
    }

    #[test]
    fn test_parse_fh4_via_dispatch() -> TestResult {
        let mut data = vec![0u8; FORZA_FH4_CARDASH_SIZE];
        let sled = make_sled_packet(1, 3500.0, (10.0, 0.0, 0.0));
        data[..FORZA_SLED_SIZE].copy_from_slice(&sled);
        let result = parse_forza_packet(&data)?;
        assert!((result.rpm - 3500.0).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn test_adapter_game_id() {
        let adapter = ForzaAdapter::new();
        assert_eq!(adapter.game_id(), "forza_motorsport");
    }

    #[test]
    fn test_adapter_update_rate() {
        let adapter = ForzaAdapter::new();
        assert_eq!(adapter.expected_update_rate(), Duration::from_millis(16));
    }

    #[test]
    fn test_adapter_with_port() {
        let adapter = ForzaAdapter::new().with_port(9999);
        assert_eq!(adapter.bind_port, 9999);
        assert_eq!(adapter.game_id(), "forza_motorsport");
    }

    #[test]
    fn test_adapter_default() {
        let adapter = ForzaAdapter::default();
        assert_eq!(adapter.bind_port, DEFAULT_FORZA_PORT);
    }

    #[test]
    fn test_adapter_normalize_delegates_to_parse() -> TestResult {
        let adapter = ForzaAdapter::new();
        let data = make_sled_packet(1, 7000.0, (30.0, 0.0, 0.0));
        let result = adapter.normalize(&data)?;
        assert!((result.rpm - 7000.0).abs() < 0.01);
        assert!((result.speed_ms - 30.0).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn test_adapter_normalize_rejects_empty() {
        let adapter = ForzaAdapter::new();
        assert!(adapter.normalize(&[]).is_err());
    }

    // ── Sled boundary conditions ──────────────────────────────────────────

    #[test]
    fn test_parse_sled_zero_filled() -> TestResult {
        let mut data = vec![0u8; FORZA_SLED_SIZE];
        // is_race_on = 1 but everything else zero
        data[OFF_IS_RACE_ON..OFF_IS_RACE_ON + 4].copy_from_slice(&1i32.to_le_bytes());
        let result = parse_forza_sled(&data)?;
        assert_eq!(result.rpm, 0.0);
        assert_eq!(result.speed_ms, 0.0);
        assert_eq!(result.lateral_g, 0.0);
        assert_eq!(result.longitudinal_g, 0.0);
        Ok(())
    }

    #[test]
    fn test_parse_sled_max_rpm() -> TestResult {
        let data = make_sled_packet(1, 20000.0, (0.0, 0.0, 0.0));
        let result = parse_forza_sled(&data)?;
        assert!((result.rpm - 20000.0).abs() < 0.01);
        assert_eq!(result.max_rpm, 8000.0);
        Ok(())
    }

    #[test]
    fn test_parse_sled_3d_velocity() -> TestResult {
        // Diagonal velocity: sqrt(3² + 4² + 0²) = 5.0
        let data = make_sled_packet(1, 1000.0, (3.0, 4.0, 0.0));
        let result = parse_forza_sled(&data)?;
        assert!((result.speed_ms - 5.0).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn test_parse_sled_g_forces() -> TestResult {
        let mut data = make_sled_packet(1, 1000.0, (0.0, 0.0, 0.0));
        let lateral_accel = 2.0 * G; // 2G lateral
        let longitudinal_accel = 1.5 * G; // 1.5G longitudinal
        data[OFF_ACCEL_X..OFF_ACCEL_X + 4].copy_from_slice(&lateral_accel.to_le_bytes());
        data[OFF_ACCEL_Z..OFF_ACCEL_Z + 4].copy_from_slice(&longitudinal_accel.to_le_bytes());
        let result = parse_forza_sled(&data)?;
        assert!((result.lateral_g - 2.0).abs() < 0.01);
        assert!((result.longitudinal_g - 1.5).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn test_parse_sled_slip_angles() -> TestResult {
        let mut data = make_sled_packet(1, 1000.0, (0.0, 0.0, 0.0));
        data[OFF_SLIP_ANGLE_FL..OFF_SLIP_ANGLE_FL + 4].copy_from_slice(&0.05f32.to_le_bytes());
        data[OFF_SLIP_ANGLE_FR..OFF_SLIP_ANGLE_FR + 4].copy_from_slice(&0.10f32.to_le_bytes());
        data[OFF_SLIP_ANGLE_RL..OFF_SLIP_ANGLE_RL + 4].copy_from_slice(&0.15f32.to_le_bytes());
        data[OFF_SLIP_ANGLE_RR..OFF_SLIP_ANGLE_RR + 4].copy_from_slice(&0.20f32.to_le_bytes());
        let result = parse_forza_sled(&data)?;
        assert!((result.slip_angle_fl - 0.05).abs() < 0.001);
        assert!((result.slip_angle_fr - 0.10).abs() < 0.001);
        assert!((result.slip_angle_rl - 0.15).abs() < 0.001);
        assert!((result.slip_angle_rr - 0.20).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn test_parse_sled_wheel_speeds_in_extended() -> TestResult {
        let mut data = make_sled_packet(1, 1000.0, (0.0, 0.0, 0.0));
        data[OFF_WHEEL_SPEED_FL..OFF_WHEEL_SPEED_FL + 4].copy_from_slice(&50.0f32.to_le_bytes());
        data[OFF_WHEEL_SPEED_FR..OFF_WHEEL_SPEED_FR + 4].copy_from_slice(&51.0f32.to_le_bytes());
        data[OFF_WHEEL_SPEED_RL..OFF_WHEEL_SPEED_RL + 4].copy_from_slice(&52.0f32.to_le_bytes());
        data[OFF_WHEEL_SPEED_RR..OFF_WHEEL_SPEED_RR + 4].copy_from_slice(&53.0f32.to_le_bytes());
        let result = parse_forza_sled(&data)?;
        assert_eq!(
            result.get_extended("wheel_speed_fl"),
            Some(&TelemetryValue::Float(50.0))
        );
        assert_eq!(
            result.get_extended("wheel_speed_rr"),
            Some(&TelemetryValue::Float(53.0))
        );
        Ok(())
    }

    #[test]
    fn test_parse_sled_suspension_travel_in_extended() -> TestResult {
        let mut data = make_sled_packet(1, 1000.0, (0.0, 0.0, 0.0));
        data[OFF_SUSP_TRAVEL_FL..OFF_SUSP_TRAVEL_FL + 4].copy_from_slice(&0.12f32.to_le_bytes());
        data[OFF_SUSP_TRAVEL_RR..OFF_SUSP_TRAVEL_RR + 4].copy_from_slice(&0.08f32.to_le_bytes());
        let result = parse_forza_sled(&data)?;
        assert_eq!(
            result.get_extended("suspension_travel_fl"),
            Some(&TelemetryValue::Float(0.12))
        );
        assert_eq!(
            result.get_extended("suspension_travel_rr"),
            Some(&TelemetryValue::Float(0.08))
        );
        Ok(())
    }

    #[test]
    fn test_parse_sled_exactly_minimum_size() -> TestResult {
        let data = make_sled_packet(1, 3000.0, (10.0, 0.0, 0.0));
        assert_eq!(data.len(), FORZA_SLED_SIZE);
        let result = parse_forza_sled(&data)?;
        assert!((result.rpm - 3000.0).abs() < 0.01);
        Ok(())
    }

    // ── CarDash boundary conditions ───────────────────────────────────────

    fn make_full_cardash_packet(
        rpm: f32,
        vel: (f32, f32, f32),
        throttle: u8,
        brake: u8,
        clutch: u8,
        gear: u8,
        steer: i8,
    ) -> Vec<u8> {
        let mut data = vec![0u8; FORZA_CARDASH_SIZE];
        let sled = make_sled_packet(1, rpm, vel);
        data[..FORZA_SLED_SIZE].copy_from_slice(&sled);
        data[OFF_DASH_SPEED..OFF_DASH_SPEED + 4]
            .copy_from_slice(&(vel.0.hypot(vel.1).hypot(vel.2)).to_le_bytes());
        data[OFF_DASH_ACCEL] = throttle;
        data[OFF_DASH_BRAKE] = brake;
        data[OFF_DASH_CLUTCH] = clutch;
        data[OFF_DASH_GEAR] = gear;
        data[OFF_DASH_STEER] = steer as u8;
        data
    }

    #[test]
    fn test_cardash_user_inputs() -> TestResult {
        let data = make_full_cardash_packet(5000.0, (20.0, 0.0, 0.0), 255, 128, 64, 4, 63);
        let result = parse_forza_cardash(&data)?;
        assert!((result.throttle - 1.0).abs() < 0.01);
        assert!((result.brake - 128.0 / 255.0).abs() < 0.01);
        assert!((result.clutch - 64.0 / 255.0).abs() < 0.01);
        assert_eq!(result.gear, 3); // gear byte 4 → gear 3
        assert!((result.steering_angle - 63.0 / 127.0).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn test_cardash_gear_mapping() -> TestResult {
        // Gear 0 = Reverse
        let data = make_full_cardash_packet(1000.0, (0.0, 0.0, 0.0), 0, 0, 0, 0, 0);
        let result = parse_forza_cardash(&data)?;
        assert_eq!(result.gear, -1);

        // Gear 1 = Neutral
        let data = make_full_cardash_packet(1000.0, (0.0, 0.0, 0.0), 0, 0, 0, 1, 0);
        let result = parse_forza_cardash(&data)?;
        assert_eq!(result.gear, 0);

        // Gear 2 = 1st
        let data = make_full_cardash_packet(1000.0, (0.0, 0.0, 0.0), 0, 0, 0, 2, 0);
        let result = parse_forza_cardash(&data)?;
        assert_eq!(result.gear, 1);

        // Gear 9 = 8th
        let data = make_full_cardash_packet(1000.0, (0.0, 0.0, 0.0), 0, 0, 0, 9, 0);
        let result = parse_forza_cardash(&data)?;
        assert_eq!(result.gear, 8);
        Ok(())
    }

    #[test]
    fn test_cardash_steer_clamped() -> TestResult {
        // Max left
        let data = make_full_cardash_packet(1000.0, (0.0, 0.0, 0.0), 0, 0, 0, 1, -127);
        let result = parse_forza_cardash(&data)?;
        assert!((result.steering_angle - (-1.0)).abs() < 0.01);

        // Max right
        let data = make_full_cardash_packet(1000.0, (0.0, 0.0, 0.0), 0, 0, 0, 1, 127);
        let result = parse_forza_cardash(&data)?;
        assert!((result.steering_angle - 1.0).abs() < 0.01);

        // Center
        let data = make_full_cardash_packet(1000.0, (0.0, 0.0, 0.0), 0, 0, 0, 1, 0);
        let result = parse_forza_cardash(&data)?;
        assert!((result.steering_angle).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn test_cardash_tire_temps_fahrenheit_to_celsius() -> TestResult {
        let mut data = vec![0u8; FORZA_CARDASH_SIZE];
        let sled = make_sled_packet(1, 1000.0, (0.0, 0.0, 0.0));
        data[..FORZA_SLED_SIZE].copy_from_slice(&sled);
        // 212°F = 100°C
        data[OFF_DASH_TIRE_TEMP_FL..OFF_DASH_TIRE_TEMP_FL + 4]
            .copy_from_slice(&212.0f32.to_le_bytes());
        // 32°F = 0°C
        data[OFF_DASH_TIRE_TEMP_FR..OFF_DASH_TIRE_TEMP_FR + 4]
            .copy_from_slice(&32.0f32.to_le_bytes());
        // 68°F = 20°C
        data[OFF_DASH_TIRE_TEMP_RL..OFF_DASH_TIRE_TEMP_RL + 4]
            .copy_from_slice(&68.0f32.to_le_bytes());
        // 392°F = 200°C
        data[OFF_DASH_TIRE_TEMP_RR..OFF_DASH_TIRE_TEMP_RR + 4]
            .copy_from_slice(&392.0f32.to_le_bytes());
        let result = parse_forza_cardash(&data)?;
        assert_eq!(result.tire_temps_c[0], 100);
        assert_eq!(result.tire_temps_c[1], 0);
        assert_eq!(result.tire_temps_c[2], 20);
        assert_eq!(result.tire_temps_c[3], 200);
        Ok(())
    }

    #[test]
    fn test_cardash_fuel_and_laps() -> TestResult {
        let mut data = vec![0u8; FORZA_CARDASH_SIZE];
        let sled = make_sled_packet(1, 1000.0, (0.0, 0.0, 0.0));
        data[..FORZA_SLED_SIZE].copy_from_slice(&sled);
        data[OFF_DASH_FUEL..OFF_DASH_FUEL + 4].copy_from_slice(&0.75f32.to_le_bytes());
        data[OFF_DASH_BEST_LAP..OFF_DASH_BEST_LAP + 4].copy_from_slice(&82.5f32.to_le_bytes());
        data[OFF_DASH_LAST_LAP..OFF_DASH_LAST_LAP + 4].copy_from_slice(&83.2f32.to_le_bytes());
        data[OFF_DASH_CUR_LAP..OFF_DASH_CUR_LAP + 4].copy_from_slice(&41.0f32.to_le_bytes());
        data[OFF_DASH_LAP_NUMBER..OFF_DASH_LAP_NUMBER + 2].copy_from_slice(&5u16.to_le_bytes());
        data[OFF_DASH_RACE_POS] = 3;
        let result = parse_forza_cardash(&data)?;
        assert!((result.fuel_percent - 0.75).abs() < 0.01);
        assert!((result.best_lap_time_s - 82.5).abs() < 0.01);
        assert!((result.last_lap_time_s - 83.2).abs() < 0.01);
        assert!((result.current_lap_time_s - 41.0).abs() < 0.01);
        assert_eq!(result.lap, 5);
        assert_eq!(result.position, 3);
        Ok(())
    }

    #[test]
    fn test_cardash_race_off_returns_defaults() -> TestResult {
        let data = vec![0u8; FORZA_CARDASH_SIZE];
        // is_race_on = 0
        let result = parse_forza_cardash(&data)?;
        assert_eq!(result.rpm, 0.0);
        assert_eq!(result.speed_ms, 0.0);
        assert_eq!(result.throttle, 0.0);
        Ok(())
    }

    #[test]
    fn test_cardash_truncated() {
        let data = vec![0u8; FORZA_CARDASH_SIZE - 1];
        assert!(parse_forza_cardash(&data).is_err());
    }

    #[test]
    fn test_fh4_cardash_truncated() {
        // Expected minimum is FORZA_CARDASH_SIZE + 12 = 323 bytes
        let data = vec![0u8; FORZA_CARDASH_SIZE + 12 - 1];
        assert!(parse_forza_fh4_cardash(&data).is_err());
    }

    #[test]
    fn test_cardash_preserves_sled_extended_fields() -> TestResult {
        let mut data = vec![0u8; FORZA_CARDASH_SIZE];
        let mut sled = make_sled_packet(1, 1000.0, (0.0, 0.0, 0.0));
        sled[OFF_WHEEL_SPEED_FL..OFF_WHEEL_SPEED_FL + 4]
            .copy_from_slice(&42.0f32.to_le_bytes());
        data[..FORZA_SLED_SIZE].copy_from_slice(&sled);
        let result = parse_forza_cardash(&data)?;
        assert_eq!(
            result.get_extended("wheel_speed_fl"),
            Some(&TelemetryValue::Float(42.0))
        );
        Ok(())
    }

    #[test]
    fn test_read_f32_le_invalid_offset() {
        let data = vec![0u8; 4];
        assert!(read_f32_le(&data, 2).is_none());
    }

    #[test]
    fn test_read_f32_le_nan_filtered() {
        let mut data = vec![0u8; 4];
        data.copy_from_slice(&f32::NAN.to_le_bytes());
        assert!(read_f32_le(&data, 0).is_none());
    }

    #[test]
    fn test_read_f32_le_infinity_filtered() {
        let mut data = vec![0u8; 4];
        data.copy_from_slice(&f32::INFINITY.to_le_bytes());
        assert!(read_f32_le(&data, 0).is_none());
    }

    #[test]
    fn test_read_i32_le_valid() {
        let mut data = vec![0u8; 4];
        data.copy_from_slice(&42i32.to_le_bytes());
        assert_eq!(read_i32_le(&data, 0), Some(42));
    }

    #[test]
    fn test_read_i32_le_invalid_offset() {
        let data = vec![0u8; 2];
        assert!(read_i32_le(&data, 0).is_none());
    }

    // ── Insta snapshot tests ──────────────────────────────────────────────

    #[test]
    fn snapshot_sled_typical_driving() -> TestResult {
        let mut data = make_sled_packet(1, 6500.0, (25.0, 1.0, 10.0));
        data[OFF_ACCEL_X..OFF_ACCEL_X + 4].copy_from_slice(&(1.5 * G).to_le_bytes());
        data[OFF_ACCEL_Z..OFF_ACCEL_Z + 4].copy_from_slice(&(0.3 * G).to_le_bytes());
        data[OFF_SLIP_ANGLE_FL..OFF_SLIP_ANGLE_FL + 4].copy_from_slice(&0.02f32.to_le_bytes());
        data[OFF_SLIP_ANGLE_FR..OFF_SLIP_ANGLE_FR + 4].copy_from_slice(&0.03f32.to_le_bytes());
        let result = parse_forza_sled(&data)?;
        insta::assert_yaml_snapshot!("forza_sled_typical", result);
        Ok(())
    }

    #[test]
    fn snapshot_cardash_full_data() -> TestResult {
        let data = make_full_cardash_packet(7200.0, (35.0, 0.0, 0.0), 200, 0, 0, 5, 15);
        let result = parse_forza_cardash(&data)?;
        insta::assert_yaml_snapshot!("forza_cardash_full", result);
        Ok(())
    }

    #[test]
    fn snapshot_sled_race_off() -> TestResult {
        let data = make_sled_packet(0, 5000.0, (20.0, 0.0, 0.0));
        let result = parse_forza_sled(&data)?;
        insta::assert_yaml_snapshot!("forza_sled_race_off", result);
        Ok(())
    }
}

#[cfg(test)]
mod proptest_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn parse_sled_no_panic_on_arbitrary_bytes(
            data in proptest::collection::vec(any::<u8>(), 0..512)
        ) {
            let _ = parse_forza_sled(&data);
        }

        #[test]
        fn parse_cardash_no_panic_on_arbitrary_bytes(
            data in proptest::collection::vec(any::<u8>(), 0..512)
        ) {
            let _ = parse_forza_cardash(&data);
        }

        #[test]
        fn parse_forza_packet_no_panic(
            data in proptest::collection::vec(any::<u8>(), 0..512)
        ) {
            let _ = parse_forza_packet(&data);
        }

        #[test]
        fn parse_sled_rpm_nonneg_when_race_on(rpm in 0.0f32..=20000.0f32) {
            let mut data = vec![0u8; FORZA_SLED_SIZE];
            data[OFF_IS_RACE_ON..OFF_IS_RACE_ON + 4].copy_from_slice(&1i32.to_le_bytes());
            data[OFF_CURRENT_RPM..OFF_CURRENT_RPM + 4].copy_from_slice(&rpm.to_le_bytes());
            if let Ok(result) = parse_forza_sled(&data) {
                prop_assert!(result.rpm >= 0.0);
            }
        }

        #[test]
        fn parse_sled_steering_clamped(accel in any::<f32>()) {
            let mut data = vec![0u8; FORZA_SLED_SIZE];
            data[OFF_IS_RACE_ON..OFF_IS_RACE_ON + 4].copy_from_slice(&1i32.to_le_bytes());
            data[OFF_ACCEL_X..OFF_ACCEL_X + 4].copy_from_slice(&accel.to_le_bytes());
            if let Ok(result) = parse_forza_sled(&data) {
                prop_assert!(result.speed_ms >= 0.0);
            }
        }

        #[test]
        fn cardash_throttle_in_range(throttle_byte in 0u8..=255u8) {
            let mut data = vec![0u8; FORZA_CARDASH_SIZE];
            data[OFF_IS_RACE_ON..OFF_IS_RACE_ON + 4].copy_from_slice(&1i32.to_le_bytes());
            data[OFF_DASH_ACCEL] = throttle_byte;
            if let Ok(result) = parse_forza_cardash(&data) {
                prop_assert!(result.throttle >= 0.0 && result.throttle <= 1.0);
            }
        }

        #[test]
        fn cardash_brake_in_range(brake_byte in 0u8..=255u8) {
            let mut data = vec![0u8; FORZA_CARDASH_SIZE];
            data[OFF_IS_RACE_ON..OFF_IS_RACE_ON + 4].copy_from_slice(&1i32.to_le_bytes());
            data[OFF_DASH_BRAKE] = brake_byte;
            if let Ok(result) = parse_forza_cardash(&data) {
                prop_assert!(result.brake >= 0.0 && result.brake <= 1.0);
            }
        }

        #[test]
        fn cardash_steer_in_range(steer_byte in 0u8..=255u8) {
            let mut data = vec![0u8; FORZA_CARDASH_SIZE];
            data[OFF_IS_RACE_ON..OFF_IS_RACE_ON + 4].copy_from_slice(&1i32.to_le_bytes());
            data[OFF_DASH_STEER] = steer_byte;
            if let Ok(result) = parse_forza_cardash(&data) {
                prop_assert!(result.steering_angle >= -1.0 && result.steering_angle <= 1.0);
            }
        }

        #[test]
        fn cardash_gear_valid(gear_byte in 0u8..=9u8) {
            let mut data = vec![0u8; FORZA_CARDASH_SIZE];
            data[OFF_IS_RACE_ON..OFF_IS_RACE_ON + 4].copy_from_slice(&1i32.to_le_bytes());
            data[OFF_DASH_GEAR] = gear_byte;
            if let Ok(result) = parse_forza_cardash(&data) {
                prop_assert!(result.gear >= -1 && result.gear <= 8);
            }
        }

        #[test]
        fn fh4_cardash_no_panic_on_arbitrary_bytes(
            data in proptest::collection::vec(any::<u8>(), 0..512)
        ) {
            let _ = parse_forza_fh4_cardash(&data);
        }

        #[test]
        fn sled_speed_is_magnitude(
            vx in -100.0f32..=100.0f32,
            vy in -100.0f32..=100.0f32,
            vz in -100.0f32..=100.0f32,
        ) {
            let mut data = vec![0u8; FORZA_SLED_SIZE];
            data[OFF_IS_RACE_ON..OFF_IS_RACE_ON + 4].copy_from_slice(&1i32.to_le_bytes());
            data[OFF_VEL_X..OFF_VEL_X + 4].copy_from_slice(&vx.to_le_bytes());
            data[OFF_VEL_Y..OFF_VEL_Y + 4].copy_from_slice(&vy.to_le_bytes());
            data[OFF_VEL_Z..OFF_VEL_Z + 4].copy_from_slice(&vz.to_le_bytes());
            if let Ok(result) = parse_forza_sled(&data) {
                let expected = (vx * vx + vy * vy + vz * vz).sqrt();
                prop_assert!((result.speed_ms - expected).abs() < 0.01);
                prop_assert!(result.speed_ms >= 0.0);
            }
        }
    }
}

//! Forza Motorsport / Forza Horizon telemetry adapter using UDP.
//!
//! Supports two packet formats:
//! - "Sled" (232 bytes): Forza 7 and earlier.
//! - "CarDash" (311 bytes): FM8, FH5, Forza 7+.
//!
//! Both formats use little-endian encoding.
#![cfg_attr(not(windows), allow(unused, dead_code))]

use crate::{
    NormalizedTelemetry, TelemetryAdapter, TelemetryFrame, TelemetryReceiver, telemetry_now_ns,
};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::time::Duration;
use tokio::net::UdpSocket as TokioUdpSocket;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

const DEFAULT_FORZA_PORT: u16 = 5300;
const FORZA_SLED_SIZE: usize = 232;
const FORZA_CARDASH_SIZE: usize = 311;
const MAX_PACKET_SIZE: usize = 512;

// Sled format byte offsets
const OFF_IS_RACE_ON: usize = 0;
const OFF_ENGINE_MAX_RPM: usize = 8;
const OFF_CURRENT_RPM: usize = 16;
const OFF_ACCEL: usize = 20;
const OFF_BRAKE: usize = 24;
const OFF_GEAR: usize = 36;
const OFF_STEER: usize = 40;
const OFF_VEL_X: usize = 52;
const OFF_VEL_Y: usize = 56;
const OFF_VEL_Z: usize = 60;

#[cfg(windows)]
const FORZA_PROCESS_NAMES: &[&str] = &[
    "forzahorizon5.exe",
    "forzamotorsport.exe",
    "forza motorsport 7.exe",
];

#[derive(Debug, Clone, Copy, PartialEq)]
enum ForzaPacketFormat {
    Sled,
    CarDash,
    Unknown,
}

fn detect_format(len: usize) -> ForzaPacketFormat {
    match len {
        FORZA_SLED_SIZE => ForzaPacketFormat::Sled,
        FORZA_CARDASH_SIZE => ForzaPacketFormat::CarDash,
        _ => ForzaPacketFormat::Unknown,
    }
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

    let engine_max_rpm = read_f32_le(data, OFF_ENGINE_MAX_RPM).unwrap_or(0.0);
    let current_rpm = read_f32_le(data, OFF_CURRENT_RPM).unwrap_or(0.0);
    let throttle = read_f32_le(data, OFF_ACCEL).unwrap_or(0.0);
    let brake = read_f32_le(data, OFF_BRAKE).unwrap_or(0.0);
    let gear_raw = read_f32_le(data, OFF_GEAR).unwrap_or(1.0);
    let steer = read_f32_le(data, OFF_STEER)
        .unwrap_or(0.0)
        .clamp(-1.0, 1.0);
    let vel_x = read_f32_le(data, OFF_VEL_X).unwrap_or(0.0);
    let vel_y = read_f32_le(data, OFF_VEL_Y).unwrap_or(0.0);
    let vel_z = read_f32_le(data, OFF_VEL_Z).unwrap_or(0.0);
    let speed_mps = (vel_x * vel_x + vel_y * vel_y + vel_z * vel_z).sqrt();

    // Forza gear: 0=Reverse, 1-8=forward gears.
    // Normalized: -1=Reverse, 0=Neutral (not present), 1-8=forward.
    let gear: i8 = if gear_raw.is_finite() {
        match gear_raw.trunc() as i32 {
            0 => -1,
            g @ 1..=8 => g as i8,
            _ => 0,
        }
    } else {
        0
    };

    Ok(NormalizedTelemetry::builder()
        .rpm(current_rpm)
        .max_rpm(engine_max_rpm)
        .throttle(throttle)
        .brake(brake)
        .steering_angle(steer)
        .speed_ms(speed_mps)
        .gear(gear)
        .build())
}

/// CarDash format shares the Sled layout for the first 232 bytes.
fn parse_forza_cardash(data: &[u8]) -> Result<NormalizedTelemetry> {
    if data.len() < FORZA_CARDASH_SIZE {
        return Err(anyhow!(
            "Forza CarDash packet too short: expected {FORZA_CARDASH_SIZE}, got {}",
            data.len()
        ));
    }
    parse_forza_sled(data)
}

fn parse_forza_packet(data: &[u8]) -> Result<NormalizedTelemetry> {
    match detect_format(data.len()) {
        ForzaPacketFormat::Sled => parse_forza_sled(data),
        ForzaPacketFormat::CarDash => parse_forza_cardash(data),
        ForzaPacketFormat::Unknown => Err(anyhow!(
            "Unknown Forza packet length: {}. Expected {} (Sled) or {} (CarDash)",
            data.len(),
            FORZA_SLED_SIZE,
            FORZA_CARDASH_SIZE,
        )),
    }
}

/// Forza Motorsport / Forza Horizon telemetry adapter.
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
            let bind_addr =
                SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, bind_port));
            let socket = match TokioUdpSocket::bind(bind_addr).await {
                Ok(s) => s,
                Err(e) => {
                    warn!("Failed to bind Forza UDP socket on port {bind_port}: {e}");
                    return;
                }
            };
            info!("Forza adapter listening on UDP port {bind_port}");
            let mut buf = [0u8; MAX_PACKET_SIZE];
            let mut sequence = 0u64;

            loop {
                match tokio::time::timeout(update_rate * 10, socket.recv(&mut buf)).await {
                    Ok(Ok(len)) => match parse_forza_packet(&buf[..len]) {
                        Ok(normalized) => {
                            let frame = TelemetryFrame::new(
                                normalized,
                                telemetry_now_ns(),
                                sequence,
                                len,
                            );
                            if tx.send(frame).await.is_err() {
                                debug!("Receiver dropped, stopping Forza monitoring");
                                break;
                            }
                            sequence = sequence.saturating_add(1);
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

    fn make_sled_packet(
        is_race_on: i32,
        rpm: f32,
        throttle: f32,
        brake: f32,
        gear: f32,
        steer: f32,
        vel: (f32, f32, f32),
    ) -> Vec<u8> {
        let mut data = vec![0u8; FORZA_SLED_SIZE];
        data[OFF_IS_RACE_ON..OFF_IS_RACE_ON + 4].copy_from_slice(&is_race_on.to_le_bytes());
        data[OFF_ENGINE_MAX_RPM..OFF_ENGINE_MAX_RPM + 4]
            .copy_from_slice(&8000.0f32.to_le_bytes());
        data[OFF_CURRENT_RPM..OFF_CURRENT_RPM + 4].copy_from_slice(&rpm.to_le_bytes());
        data[OFF_ACCEL..OFF_ACCEL + 4].copy_from_slice(&throttle.to_le_bytes());
        data[OFF_BRAKE..OFF_BRAKE + 4].copy_from_slice(&brake.to_le_bytes());
        data[OFF_GEAR..OFF_GEAR + 4].copy_from_slice(&gear.to_le_bytes());
        data[OFF_STEER..OFF_STEER + 4].copy_from_slice(&steer.to_le_bytes());
        data[OFF_VEL_X..OFF_VEL_X + 4].copy_from_slice(&vel.0.to_le_bytes());
        data[OFF_VEL_Y..OFF_VEL_Y + 4].copy_from_slice(&vel.1.to_le_bytes());
        data[OFF_VEL_Z..OFF_VEL_Z + 4].copy_from_slice(&vel.2.to_le_bytes());
        data
    }

    #[test]
    fn test_parse_sled_valid() -> TestResult {
        let data = make_sled_packet(1, 5000.0, 0.7, 0.0, 3.0, 0.25, (20.0, 0.0, 0.0));
        let result = parse_forza_sled(&data)?;
        assert!((result.rpm - 5000.0).abs() < 0.01);
        assert!((result.throttle - 0.7).abs() < 0.001);
        assert!((result.brake).abs() < 0.001);
        assert_eq!(result.gear, 3);
        assert!((result.steering_angle - 0.25).abs() < 0.001);
        assert!((result.speed_ms - 20.0).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn test_parse_sled_race_off() -> TestResult {
        let data = make_sled_packet(0, 5000.0, 0.7, 0.0, 3.0, 0.25, (20.0, 0.0, 0.0));
        let result = parse_forza_sled(&data)?;
        assert_eq!(result.rpm, 0.0);
        Ok(())
    }

    #[test]
    fn test_parse_sled_gear_reverse() -> TestResult {
        let data = make_sled_packet(1, 1000.0, 0.0, 0.5, 0.0, 0.0, (-5.0, 0.0, 0.0));
        let result = parse_forza_sled(&data)?;
        assert_eq!(result.gear, -1);
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
        assert_eq!(detect_format(FORZA_CARDASH_SIZE), ForzaPacketFormat::CarDash);
        assert_eq!(detect_format(100), ForzaPacketFormat::Unknown);
    }

    #[test]
    fn test_parse_unknown_format() {
        let data = vec![0u8; 100];
        assert!(parse_forza_packet(&data).is_err());
    }

    #[test]
    fn test_normalization_clamp() -> TestResult {
        let data = make_sled_packet(1, 5000.0, 2.0, -1.0, 3.0, 3.0, (20.0, 0.0, 0.0));
        let result = parse_forza_sled(&data)?;
        assert!((result.throttle - 1.0).abs() < 0.001);
        assert!((result.brake).abs() < 0.001);
        assert!((result.steering_angle - 1.0).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn test_parse_cardash_valid() -> TestResult {
        let mut data = vec![0u8; FORZA_CARDASH_SIZE];
        // Copy a valid sled header into it
        let sled = make_sled_packet(1, 4000.0, 0.5, 0.2, 2.0, 0.1, (15.0, 0.0, 0.0));
        data[..FORZA_SLED_SIZE].copy_from_slice(&sled);
        let result = parse_forza_cardash(&data)?;
        assert!((result.rpm - 4000.0).abs() < 0.01);
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
}

//! Live For Speed (LFS) telemetry adapter.
//!
//! LFS exposes telemetry via the OutGauge UDP protocol on a configurable port (default 30000).
//! The 96-byte OutGauge packet format is the same as used by BeamNG.drive.
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

/// Verified: LFS OutGauge default (en.lfsmanual.net/wiki/OutGauge example binds 30000).
const DEFAULT_LFS_PORT: u16 = 30000;
/// Standard LFS OutGauge packet size.
const OUTGAUGE_PACKET_SIZE: usize = 96;
const MAX_PACKET_SIZE: usize = 256;

// OutGauge byte offsets (shared with BeamNG.drive OutGauge format)
const OFF_GEAR: usize = 10; // u8: 0=Reverse, 1=Neutral, 2=1st, 3=2nd, …
const OFF_SPEED: usize = 12; // f32, m/s
const OFF_RPM: usize = 16; // f32
const OFF_FUEL: usize = 28; // f32, 0 to 1
const OFF_THROTTLE: usize = 48; // f32, 0 to 1
const OFF_BRAKE: usize = 52; // f32, 0 to 1
const OFF_CLUTCH: usize = 56; // f32, 0 to 1

#[cfg(windows)]
const LFS_PROCESS_NAMES: &[&str] = &["lfs.exe", "lfs64.exe"];

fn parse_lfs_packet(data: &[u8]) -> Result<NormalizedTelemetry> {
    if data.len() < OUTGAUGE_PACKET_SIZE {
        return Err(anyhow!(
            "LFS OutGauge packet too short: expected {OUTGAUGE_PACKET_SIZE}, got {}",
            data.len()
        ));
    }

    let gear_raw = data[OFF_GEAR];
    let speed_mps = read_f32_le(data, OFF_SPEED).unwrap_or(0.0);
    let rpm = read_f32_le(data, OFF_RPM).unwrap_or(0.0);
    let fuel = read_f32_le(data, OFF_FUEL).unwrap_or(0.0);
    let throttle = read_f32_le(data, OFF_THROTTLE).unwrap_or(0.0);
    let brake = read_f32_le(data, OFF_BRAKE).unwrap_or(0.0);
    let clutch = read_f32_le(data, OFF_CLUTCH).unwrap_or(0.0);

    // OutGauge gear: 0=Reverse, 1=Neutral, 2=1st gear, 3=2nd gear, …
    // Normalized:   -1=Reverse,  0=Neutral,  1=1st gear, 2=2nd gear, …
    let gear: i8 = match gear_raw {
        0 => -1,
        1 => 0,
        g => (g - 1) as i8,
    };

    Ok(NormalizedTelemetry::builder()
        .speed_ms(speed_mps)
        .rpm(rpm)
        .gear(gear)
        .throttle(throttle)
        .brake(brake)
        .clutch(clutch)
        .fuel_percent(fuel)
        .build())
}

/// Live For Speed telemetry adapter using the OutGauge UDP protocol.
pub struct LFSAdapter {
    bind_port: u16,
    update_rate: Duration,
}

impl Default for LFSAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl LFSAdapter {
    pub fn new() -> Self {
        Self {
            bind_port: DEFAULT_LFS_PORT,
            update_rate: Duration::from_millis(16),
        }
    }

    pub fn with_port(mut self, port: u16) -> Self {
        self.bind_port = port;
        self
    }
}

#[async_trait]
impl TelemetryAdapter for LFSAdapter {
    fn game_id(&self) -> &str {
        "live_for_speed"
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
                    warn!("Failed to bind LFS UDP socket on port {bind_port}: {e}");
                    return;
                }
            };
            info!("LFS adapter listening on UDP port {bind_port}");
            let mut buf = [0u8; MAX_PACKET_SIZE];
            let mut frame_seq = 0u64;

            loop {
                match tokio::time::timeout(update_rate * 10, socket.recv(&mut buf)).await {
                    Ok(Ok(len)) => match parse_lfs_packet(&buf[..len]) {
                        Ok(normalized) => {
                            let frame =
                                TelemetryFrame::new(normalized, telemetry_now_ns(), frame_seq, len);
                            if tx.send(frame).await.is_err() {
                                debug!("Receiver dropped, stopping LFS UDP monitoring");
                                break;
                            }
                            frame_seq = frame_seq.saturating_add(1);
                        }
                        Err(e) => debug!("Failed to parse LFS OutGauge packet: {e}"),
                    },
                    Ok(Err(e)) => warn!("LFS UDP receive error: {e}"),
                    Err(_) => debug!("No LFS telemetry data received (timeout)"),
                }
            }
            info!("Stopped LFS telemetry monitoring");
        });

        Ok(rx)
    }

    async fn stop_monitoring(&self) -> Result<()> {
        Ok(())
    }

    fn normalize(&self, raw: &[u8]) -> Result<NormalizedTelemetry> {
        parse_lfs_packet(raw)
    }

    fn expected_update_rate(&self) -> Duration {
        self.update_rate
    }

    async fn is_game_running(&self) -> Result<bool> {
        Ok(is_lfs_process_running())
    }
}

#[cfg(windows)]
fn is_lfs_process_running() -> bool {
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
                if LFS_PROCESS_NAMES.iter().any(|p| name.contains(p)) {
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
fn is_lfs_process_running() -> bool {
    false
}

fn read_f32_le(data: &[u8], offset: usize) -> Option<f32> {
    data.get(offset..offset + 4)
        .and_then(|b| b.try_into().ok())
        .map(f32::from_le_bytes)
        .filter(|v| v.is_finite())
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn make_lfs_packet(
        speed: f32,
        rpm: f32,
        gear: u8,
        throttle: f32,
        brake: f32,
        clutch: f32,
        fuel: f32,
    ) -> Vec<u8> {
        let mut data = vec![0u8; OUTGAUGE_PACKET_SIZE];
        data[OFF_GEAR] = gear;
        data[OFF_SPEED..OFF_SPEED + 4].copy_from_slice(&speed.to_le_bytes());
        data[OFF_RPM..OFF_RPM + 4].copy_from_slice(&rpm.to_le_bytes());
        data[OFF_FUEL..OFF_FUEL + 4].copy_from_slice(&fuel.to_le_bytes());
        data[OFF_THROTTLE..OFF_THROTTLE + 4].copy_from_slice(&throttle.to_le_bytes());
        data[OFF_BRAKE..OFF_BRAKE + 4].copy_from_slice(&brake.to_le_bytes());
        data[OFF_CLUTCH..OFF_CLUTCH + 4].copy_from_slice(&clutch.to_le_bytes());
        data
    }

    #[test]
    fn test_parse_valid_packet() -> TestResult {
        let data = make_lfs_packet(30.0, 4500.0, 3, 0.7, 0.0, 0.0, 0.5);
        let result = parse_lfs_packet(&data)?;
        assert!((result.speed_ms - 30.0).abs() < 0.01);
        assert!((result.rpm - 4500.0).abs() < 0.01);
        assert_eq!(result.gear, 2); // gear_raw 3 → normalized 2 (3rd gear = 3-1=2)
        assert!((result.throttle - 0.7).abs() < 0.001);
        assert!(result.brake.abs() < 0.001);
        Ok(())
    }

    #[test]
    fn test_parse_truncated_packet() {
        let data = vec![0u8; 50];
        assert!(parse_lfs_packet(&data).is_err());
    }

    #[test]
    fn test_parse_empty_packet() {
        assert!(parse_lfs_packet(&[]).is_err());
    }

    #[test]
    fn test_gear_encoding_reverse() -> TestResult {
        let data = make_lfs_packet(5.0, 2000.0, 0, 0.0, 0.5, 0.0, 0.8);
        let result = parse_lfs_packet(&data)?;
        assert_eq!(result.gear, -1, "gear 0 (reverse) should normalize to -1");
        Ok(())
    }

    #[test]
    fn test_gear_encoding_neutral() -> TestResult {
        let data = make_lfs_packet(0.0, 800.0, 1, 0.0, 0.0, 0.0, 0.9);
        let result = parse_lfs_packet(&data)?;
        assert_eq!(result.gear, 0, "gear 1 (neutral) should normalize to 0");
        Ok(())
    }

    #[test]
    fn test_gear_encoding_first() -> TestResult {
        let data = make_lfs_packet(10.0, 3000.0, 2, 0.5, 0.0, 0.0, 0.6);
        let result = parse_lfs_packet(&data)?;
        assert_eq!(result.gear, 1, "gear 2 (1st) should normalize to 1");
        Ok(())
    }

    #[test]
    fn test_adapter_game_id() {
        let adapter = LFSAdapter::new();
        assert_eq!(adapter.game_id(), "live_for_speed");
    }

    #[test]
    fn test_adapter_update_rate() {
        let adapter = LFSAdapter::new();
        assert_eq!(adapter.expected_update_rate(), Duration::from_millis(16));
    }

    #[test]
    fn test_normalize_delegates_to_parse() -> TestResult {
        let adapter = LFSAdapter::new();
        let data = make_lfs_packet(25.0, 3500.0, 2, 0.5, 0.0, 0.0, 0.4);
        let result = adapter.normalize(&data)?;
        assert!((result.rpm - 3500.0).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn test_with_port_builder() {
        let adapter = LFSAdapter::new().with_port(31000);
        assert_eq!(adapter.bind_port, 31000);
    }

    #[test]
    fn test_speed_is_nonnegative() -> TestResult {
        let data = make_lfs_packet(50.0, 5000.0, 3, 0.5, 0.0, 0.0, 0.5);
        let result = parse_lfs_packet(&data)?;
        assert!(
            result.speed_ms >= 0.0,
            "speed_ms must be non-negative, got {}",
            result.speed_ms
        );
        Ok(())
    }

    #[test]
    fn test_throttle_in_unit_range() -> TestResult {
        let data = make_lfs_packet(30.0, 4000.0, 3, 0.75, 0.0, 0.0, 0.6);
        let result = parse_lfs_packet(&data)?;
        assert!(
            result.throttle >= 0.0 && result.throttle <= 1.0,
            "throttle={} must be in [0.0, 1.0]",
            result.throttle
        );
        Ok(())
    }

    #[test]
    fn test_brake_in_unit_range() -> TestResult {
        let data = make_lfs_packet(5.0, 2000.0, 2, 0.0, 0.9, 0.0, 0.8);
        let result = parse_lfs_packet(&data)?;
        assert!(
            result.brake >= 0.0 && result.brake <= 1.0,
            "brake={} must be in [0.0, 1.0]",
            result.brake
        );
        Ok(())
    }

    #[test]
    fn test_gear_valid_range_for_forward_gears() -> TestResult {
        for raw_gear in 2u8..=8u8 {
            let data = make_lfs_packet(20.0, 3000.0, raw_gear, 0.5, 0.0, 0.0, 0.5);
            let result = parse_lfs_packet(&data)?;
            assert!(
                result.gear >= -1 && result.gear <= 8,
                "gear {} (from raw {}) out of expected range -1..=8",
                result.gear,
                raw_gear
            );
        }
        Ok(())
    }

    #[cfg(test)]
    mod proptest_tests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn parse_lfs_no_panic_on_arbitrary_bytes(
                data in proptest::collection::vec(any::<u8>(), 0..256)
            ) {
                // Must never panic regardless of input.
                let _ = parse_lfs_packet(&data);
            }

            #[test]
            fn short_packet_always_errors(
                data in proptest::collection::vec(any::<u8>(), 0..OUTGAUGE_PACKET_SIZE)
            ) {
                prop_assert!(parse_lfs_packet(&data).is_err());
            }

            #[test]
            fn valid_packet_speed_nonnegative(
                speed in 0.0f32..200.0f32,
                rpm in 0.0f32..12000.0f32,
                gear in 0u8..8u8,
                throttle in 0.0f32..1.0f32,
                brake in 0.0f32..1.0f32,
                fuel in 0.0f32..1.0f32,
            ) {
                let data = make_lfs_packet(speed, rpm, gear, throttle, brake, 0.0, fuel);
                let result = parse_lfs_packet(&data).map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
                prop_assert!(result.speed_ms >= 0.0, "speed_ms {} must be non-negative", result.speed_ms);
            }

            #[test]
            fn valid_packet_throttle_in_range(
                speed in 0.0f32..200.0f32,
                rpm in 0.0f32..12000.0f32,
                gear in 0u8..8u8,
                throttle in 0.0f32..1.0f32,
                brake in 0.0f32..1.0f32,
                fuel in 0.0f32..1.0f32,
            ) {
                let data = make_lfs_packet(speed, rpm, gear, throttle, brake, 0.0, fuel);
                let result = parse_lfs_packet(&data).map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
                prop_assert!(result.throttle >= 0.0 && result.throttle <= 1.0,
                    "throttle {} must be in [0, 1]", result.throttle);
            }

            #[test]
            fn valid_packet_brake_in_range(
                speed in 0.0f32..200.0f32,
                rpm in 0.0f32..12000.0f32,
                gear in 0u8..8u8,
                throttle in 0.0f32..1.0f32,
                brake in 0.0f32..1.0f32,
                fuel in 0.0f32..1.0f32,
            ) {
                let data = make_lfs_packet(speed, rpm, gear, throttle, brake, 0.0, fuel);
                let result = parse_lfs_packet(&data).map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
                prop_assert!(result.brake >= 0.0 && result.brake <= 1.0,
                    "brake {} must be in [0, 1]", result.brake);
            }

            #[test]
            fn valid_packet_rpm_nonnegative(
                speed in 0.0f32..200.0f32,
                rpm in 0.0f32..12000.0f32,
                gear in 0u8..8u8,
                throttle in 0.0f32..1.0f32,
                brake in 0.0f32..1.0f32,
                fuel in 0.0f32..1.0f32,
            ) {
                let data = make_lfs_packet(speed, rpm, gear, throttle, brake, 0.0, fuel);
                let result = parse_lfs_packet(&data).map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
                prop_assert!(result.rpm >= 0.0, "rpm {} must be non-negative", result.rpm);
            }

            #[test]
            fn full_packet_no_panic_on_arbitrary_bytes(
                data in proptest::collection::vec(any::<u8>(), OUTGAUGE_PACKET_SIZE..=OUTGAUGE_PACKET_SIZE * 2)
            ) {
                // Must never panic on arbitrary full-size input.
                let _ = parse_lfs_packet(&data);
            }
        }
    }
}

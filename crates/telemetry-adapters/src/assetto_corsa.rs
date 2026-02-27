//! Assetto Corsa (original) telemetry adapter using OutGauge-compatible UDP.
//!
//! Implements telemetry via AC's UDP OutGauge protocol (port 9996).
//! Struct layout is little-endian, total 76 bytes.
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

const DEFAULT_AC_PORT: u16 = 9996;
/// Minimum byte length of the AC OutGauge UDP packet.
const AC_PACKET_MIN_SIZE: usize = 76;
const MAX_PACKET_SIZE: usize = 512;

// Byte offsets in the AC OutGauge UDP packet
const OFF_GEAR: usize = 16;
const OFF_SPEED_KMH: usize = 18;
const OFF_RPM: usize = 20;
const OFF_MAX_RPM: usize = 24;
const OFF_STEER: usize = 64;
const OFF_GAS: usize = 68;
const OFF_BRAKE: usize = 72;

/// Assetto Corsa (original) telemetry adapter using OutGauge UDP protocol.
pub struct AssettoCorsaAdapter {
    bind_port: u16,
    update_rate: Duration,
}

impl Default for AssettoCorsaAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl AssettoCorsaAdapter {
    pub fn new() -> Self {
        Self {
            bind_port: DEFAULT_AC_PORT,
            update_rate: Duration::from_millis(16),
        }
    }

    pub fn with_port(mut self, port: u16) -> Self {
        self.bind_port = port;
        self
    }
}

fn parse_ac_packet(data: &[u8]) -> Result<NormalizedTelemetry> {
    if data.len() < AC_PACKET_MIN_SIZE {
        return Err(anyhow!(
            "AC packet too short: expected {AC_PACKET_MIN_SIZE}, got {}",
            data.len()
        ));
    }

    let gear = data[OFF_GEAR] as i8;
    let speed_kmh = read_u16_le(data, OFF_SPEED_KMH).unwrap_or(0);
    let rpm = read_f32_le(data, OFF_RPM).unwrap_or(0.0);
    let max_rpm = read_f32_le(data, OFF_MAX_RPM).unwrap_or(0.0);
    let steer = read_f32_le(data, OFF_STEER).unwrap_or(0.0).clamp(-1.0, 1.0);
    let gas = read_f32_le(data, OFF_GAS).unwrap_or(0.0);
    let brake = read_f32_le(data, OFF_BRAKE).unwrap_or(0.0);

    let speed_mps = f32::from(speed_kmh) / 3.6;

    Ok(NormalizedTelemetry::builder()
        .steering_angle(steer)
        .throttle(gas)
        .brake(brake)
        .speed_ms(speed_mps)
        .rpm(rpm)
        .max_rpm(max_rpm)
        .gear(gear)
        .build())
}

#[async_trait]
impl TelemetryAdapter for AssettoCorsaAdapter {
    fn game_id(&self) -> &str {
        "assetto_corsa"
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
                    warn!("Failed to bind AC UDP socket on port {bind_port}: {e}");
                    return;
                }
            };
            info!("AC adapter listening on UDP port {bind_port}");
            let mut buf = [0u8; MAX_PACKET_SIZE];
            let mut frame_seq = 0u64;

            loop {
                match tokio::time::timeout(update_rate * 10, socket.recv(&mut buf)).await {
                    Ok(Ok(len)) => match parse_ac_packet(&buf[..len]) {
                        Ok(normalized) => {
                            let frame =
                                TelemetryFrame::new(normalized, telemetry_now_ns(), frame_seq, len);
                            if tx.send(frame).await.is_err() {
                                debug!("Receiver dropped, stopping AC monitoring");
                                break;
                            }
                            frame_seq = frame_seq.saturating_add(1);
                        }
                        Err(e) => debug!("Failed to parse AC packet: {e}"),
                    },
                    Ok(Err(e)) => warn!("AC UDP receive error: {e}"),
                    Err(_) => debug!("No AC telemetry data received (timeout)"),
                }
            }
            info!("Stopped AC telemetry monitoring");
        });

        Ok(rx)
    }

    async fn stop_monitoring(&self) -> Result<()> {
        Ok(())
    }

    fn normalize(&self, raw: &[u8]) -> Result<NormalizedTelemetry> {
        parse_ac_packet(raw)
    }

    fn expected_update_rate(&self) -> Duration {
        self.update_rate
    }

    async fn is_game_running(&self) -> Result<bool> {
        Ok(is_ac_process_running())
    }
}

#[cfg(windows)]
fn is_ac_process_running() -> bool {
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
                if name == "acs.exe" {
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
fn is_ac_process_running() -> bool {
    is_process_running_linux("acs")
}

#[cfg(not(windows))]
fn is_process_running_linux(process_name: &str) -> bool {
    use std::fs;
    if let Ok(entries) = fs::read_dir("/proc") {
        for entry in entries.flatten() {
            let comm_path = entry.path().join("comm");
            if let Ok(name) = fs::read_to_string(&comm_path) {
                if name.trim() == process_name {
                    return true;
                }
            }
        }
    }
    false
}

fn read_f32_le(data: &[u8], offset: usize) -> Option<f32> {
    data.get(offset..offset + 4)
        .and_then(|b| b.try_into().ok())
        .map(f32::from_le_bytes)
}

fn read_u16_le(data: &[u8], offset: usize) -> Option<u16> {
    data.get(offset..offset + 2)
        .and_then(|b| b.try_into().ok())
        .map(u16::from_le_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn make_valid_ac_packet() -> Vec<u8> {
        let mut data = vec![0u8; AC_PACKET_MIN_SIZE];
        data[OFF_GEAR] = 3;
        data[OFF_SPEED_KMH..OFF_SPEED_KMH + 2].copy_from_slice(&120u16.to_le_bytes());
        data[OFF_RPM..OFF_RPM + 4].copy_from_slice(&6000.0f32.to_le_bytes());
        data[OFF_MAX_RPM..OFF_MAX_RPM + 4].copy_from_slice(&8000.0f32.to_le_bytes());
        data[OFF_STEER..OFF_STEER + 4].copy_from_slice(&0.3f32.to_le_bytes());
        data[OFF_GAS..OFF_GAS + 4].copy_from_slice(&0.8f32.to_le_bytes());
        data[OFF_BRAKE..OFF_BRAKE + 4].copy_from_slice(&0.1f32.to_le_bytes());
        data
    }

    #[test]
    fn test_parse_valid_packet() -> TestResult {
        let data = make_valid_ac_packet();
        let result = parse_ac_packet(&data)?;
        assert!((result.rpm - 6000.0).abs() < 0.01);
        assert!((result.max_rpm - 8000.0).abs() < 0.01);
        assert_eq!(result.gear, 3);
        assert!((result.speed_ms - 120.0 / 3.6).abs() < 0.01);
        assert!((result.steering_angle - 0.3).abs() < 0.001);
        assert!((result.throttle - 0.8).abs() < 0.001);
        assert!((result.brake - 0.1).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn test_parse_truncated_packet() -> TestResult {
        let data = vec![0u8; 10];
        let result = parse_ac_packet(&data);
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn test_normalization_bounds() -> TestResult {
        let mut data = make_valid_ac_packet();
        data[OFF_STEER..OFF_STEER + 4].copy_from_slice(&2.5f32.to_le_bytes());
        data[OFF_GAS..OFF_GAS + 4].copy_from_slice(&1.5f32.to_le_bytes());
        data[OFF_BRAKE..OFF_BRAKE + 4].copy_from_slice(&(-0.5f32).to_le_bytes());
        let result = parse_ac_packet(&data)?;
        assert!((result.steering_angle - 1.0).abs() < 0.001);
        // Builder clamps throttle to [0,1]
        assert!((result.throttle - 1.0).abs() < 0.001);
        // Builder clamps brake to [0,1], so -0.5 becomes 0.0
        assert!((result.brake - 0.0).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn test_adapter_game_id() {
        let adapter = AssettoCorsaAdapter::new();
        assert_eq!(adapter.game_id(), "assetto_corsa");
    }

    #[test]
    fn test_adapter_expected_update_rate() {
        let adapter = AssettoCorsaAdapter::new();
        assert_eq!(adapter.expected_update_rate(), Duration::from_millis(16));
    }

    #[test]
    fn test_normalize_delegates_to_parse() -> TestResult {
        let adapter = AssettoCorsaAdapter::new();
        let data = make_valid_ac_packet();
        let result = adapter.normalize(&data)?;
        assert!(result.rpm > 0.0);
        Ok(())
    }

    #[test]
    fn test_parse_exact_min_size() -> TestResult {
        let data = vec![0u8; AC_PACKET_MIN_SIZE];
        let result = parse_ac_packet(&data)?;
        assert_eq!(result.rpm, 0.0);
        assert_eq!(result.speed_ms, 0.0);
        Ok(())
    }
}

#[cfg(test)]
mod proptest_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn parse_ac_packet_no_panic_on_arbitrary_bytes(
            data in proptest::collection::vec(any::<u8>(), 0..512)
        ) {
            // Must never panic on arbitrary input.
            let _ = parse_ac_packet(&data);
        }

        #[test]
        fn parse_ac_packet_too_short_always_errors(size in 0usize..AC_PACKET_MIN_SIZE) {
            let data = vec![0u8; size];
            prop_assert!(parse_ac_packet(&data).is_err());
        }

        #[test]
        fn parse_ac_packet_speed_always_nonneg(speed in 0u16..=300u16) {
            let mut data = vec![0u8; AC_PACKET_MIN_SIZE];
            data[OFF_SPEED_KMH..OFF_SPEED_KMH + 2].copy_from_slice(&speed.to_le_bytes());
            let result = parse_ac_packet(&data);
            prop_assert!(result.is_ok());
            prop_assert!(result.unwrap().speed_ms >= 0.0);
        }

        #[test]
        fn parse_ac_packet_steering_clamped(steer in any::<f32>()) {
            let mut data = vec![0u8; AC_PACKET_MIN_SIZE];
            data[OFF_STEER..OFF_STEER + 4].copy_from_slice(&steer.to_le_bytes());
            if let Ok(result) = parse_ac_packet(&data) {
                prop_assert!(result.steering_angle >= -1.0);
                prop_assert!(result.steering_angle <= 1.0);
            }
        }

        #[test]
        fn parse_ac_packet_rpm_nonneg_on_valid_input(rpm in 0.0f32..=20000.0f32) {
            let mut data = vec![0u8; AC_PACKET_MIN_SIZE];
            data[OFF_RPM..OFF_RPM + 4].copy_from_slice(&rpm.to_le_bytes());
            let result = parse_ac_packet(&data);
            prop_assert!(result.is_ok());
            prop_assert!(result.unwrap().rpm >= 0.0);
        }
    }
}

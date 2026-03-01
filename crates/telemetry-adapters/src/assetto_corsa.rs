//! Assetto Corsa (original) telemetry adapter using Remote Telemetry UDP.
//!
//! Implements telemetry via AC's Remote Telemetry UDP protocol (port 9996).
//! Requires a 3-step handshake: connect → response → subscribe.
//! Update packets use the RTCarInfo struct (328 bytes, little-endian).
//!
//! Reference: <https://github.com/vpicon/acudp/blob/master/UDP.md>
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

/// Verified: AC Remote Telemetry handshake port per official SDK (vpicon/acudp).
const DEFAULT_AC_PORT: u16 = 9996;
/// RTCarInfo struct size (AC Remote Telemetry UDP update packet).
const AC_RTCARINFO_SIZE: usize = 328;
const MAX_PACKET_SIZE: usize = 512;

// Handshake operation IDs for AC Remote Telemetry UDP protocol.
const OP_HANDSHAKE: i32 = 0;
const OP_SUBSCRIBE_UPDATE: i32 = 1;

// Byte offsets in the AC RTCarInfo struct (little-endian, naturally aligned).
// Reference: https://github.com/vpicon/acudp/blob/master/UDP.md
#[cfg(test)]
const OFF_SPEED_KMH: usize = 8; // f32 (used in tests only; parse uses speed_Ms)
const OFF_SPEED_MS: usize = 16; // f32
const OFF_GAS: usize = 56; // f32
const OFF_BRAKE: usize = 60; // f32
const OFF_CLUTCH: usize = 64; // f32
const OFF_RPM: usize = 68; // f32
const OFF_STEER: usize = 72; // f32
const OFF_GEAR: usize = 76; // i32 (0=R, 1=N, 2=1st, ...)

/// Assetto Corsa (original) telemetry adapter using Remote Telemetry UDP.
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
    if data.len() < AC_RTCARINFO_SIZE {
        return Err(anyhow!(
            "AC RTCarInfo packet too short: expected {AC_RTCARINFO_SIZE}, got {}",
            data.len()
        ));
    }

    let speed_ms = read_f32_le(data, OFF_SPEED_MS).unwrap_or(0.0);
    let rpm = read_f32_le(data, OFF_RPM).unwrap_or(0.0);
    let steer = read_f32_le(data, OFF_STEER).unwrap_or(0.0).clamp(-1.0, 1.0);
    let gas = read_f32_le(data, OFF_GAS).unwrap_or(0.0);
    let brake = read_f32_le(data, OFF_BRAKE).unwrap_or(0.0);
    let clutch = read_f32_le(data, OFF_CLUTCH).unwrap_or(0.0);

    let gear_raw = read_i32_le(data, OFF_GEAR).unwrap_or(1); // default neutral
    // AC gear: 0=Reverse, 1=Neutral, 2=1st gear, ...
    // Normalized: -1=Reverse, 0=Neutral, 1=1st gear, ...
    let gear: i8 = match gear_raw {
        0 => -1,
        1 => 0,
        g => (g - 1).clamp(i32::from(i8::MIN), i32::from(i8::MAX)) as i8,
    };

    Ok(NormalizedTelemetry::builder()
        .steering_angle(steer)
        .throttle(gas)
        .brake(brake)
        .clutch(clutch)
        .speed_ms(speed_ms)
        .rpm(rpm)
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
        let ac_port = self.bind_port;
        let update_rate = self.update_rate;

        tokio::spawn(async move {
            // Bind to any available local port (AC listens on ac_port).
            let bind_addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0));
            let socket = match TokioUdpSocket::bind(bind_addr).await {
                Ok(s) => s,
                Err(e) => {
                    warn!("Failed to bind AC UDP socket: {e}");
                    return;
                }
            };

            let ac_addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, ac_port));
            if let Err(e) = socket.connect(ac_addr).await {
                warn!("Failed to connect to AC at {ac_addr}: {e}");
                return;
            }

            // AC Remote Telemetry handshake: send HANDSHAKE, receive response, send SUBSCRIBE.
            let handshake = build_handshake_packet(OP_HANDSHAKE);
            if let Err(e) = socket.send(&handshake).await {
                warn!("Failed to send AC handshake: {e}");
                return;
            }

            let mut buf = [0u8; MAX_PACKET_SIZE];
            match tokio::time::timeout(Duration::from_secs(2), socket.recv(&mut buf)).await {
                Ok(Ok(_)) => info!("AC handshake response received"),
                Ok(Err(e)) => {
                    warn!("Failed to receive AC handshake response: {e}");
                    return;
                }
                Err(_) => {
                    warn!("AC handshake response timeout — is Assetto Corsa running?");
                    return;
                }
            }

            let subscribe = build_handshake_packet(OP_SUBSCRIBE_UPDATE);
            if let Err(e) = socket.send(&subscribe).await {
                warn!("Failed to send AC subscribe request: {e}");
                return;
            }

            info!("AC adapter connected and subscribed via port {ac_port}");
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
            if let Ok(name) = fs::read_to_string(&comm_path)
                && name.trim() == process_name
            {
                return true;
            }
        }
    }
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

fn build_handshake_packet(operation_id: i32) -> [u8; 12] {
    let mut packet = [0u8; 12];
    packet[0..4].copy_from_slice(&1i32.to_le_bytes()); // identifier
    packet[4..8].copy_from_slice(&1i32.to_le_bytes()); // version
    packet[8..12].copy_from_slice(&operation_id.to_le_bytes());
    packet
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn make_valid_ac_packet() -> Vec<u8> {
        let mut data = vec![0u8; AC_RTCARINFO_SIZE];
        // identifier = 'a'
        data[0..4].copy_from_slice(&(b'a' as i32).to_le_bytes());
        // size
        data[4..8].copy_from_slice(&(AC_RTCARINFO_SIZE as i32).to_le_bytes());
        // speed_Kmh (float) at offset 8
        data[OFF_SPEED_KMH..OFF_SPEED_KMH + 4].copy_from_slice(&120.0f32.to_le_bytes());
        // speed_Ms (float) at offset 16
        let speed_ms = 120.0f32 / 3.6;
        data[OFF_SPEED_MS..OFF_SPEED_MS + 4].copy_from_slice(&speed_ms.to_le_bytes());
        // gas at offset 56
        data[OFF_GAS..OFF_GAS + 4].copy_from_slice(&0.8f32.to_le_bytes());
        // brake at offset 60
        data[OFF_BRAKE..OFF_BRAKE + 4].copy_from_slice(&0.1f32.to_le_bytes());
        // rpm at offset 68
        data[OFF_RPM..OFF_RPM + 4].copy_from_slice(&6000.0f32.to_le_bytes());
        // steer at offset 72
        data[OFF_STEER..OFF_STEER + 4].copy_from_slice(&0.3f32.to_le_bytes());
        // gear at offset 76 (AC: 3 = 2nd gear; 0=R, 1=N, 2=1st, 3=2nd)
        data[OFF_GEAR..OFF_GEAR + 4].copy_from_slice(&3i32.to_le_bytes());
        data
    }

    #[test]
    fn test_parse_valid_packet() -> TestResult {
        let data = make_valid_ac_packet();
        let result = parse_ac_packet(&data)?;
        assert!((result.rpm - 6000.0).abs() < 0.01);
        assert_eq!(result.gear, 2); // AC gear 3 → normalized 2
        assert!((result.speed_ms - 120.0 / 3.6).abs() < 0.1);
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
        let data = vec![0u8; AC_RTCARINFO_SIZE];
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
        fn parse_ac_packet_too_short_always_errors(size in 0usize..AC_RTCARINFO_SIZE) {
            let data = vec![0u8; size];
            prop_assert!(parse_ac_packet(&data).is_err());
        }

        #[test]
        fn parse_ac_packet_speed_always_nonneg(speed_ms in 0.0f32..=100.0f32) {
            let mut data = vec![0u8; AC_RTCARINFO_SIZE];
            data[OFF_SPEED_MS..OFF_SPEED_MS + 4].copy_from_slice(&speed_ms.to_le_bytes());
            let t = parse_ac_packet(&data).map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
            prop_assert!(t.speed_ms >= 0.0);
        }

        #[test]
        fn parse_ac_packet_steering_clamped(steer in any::<f32>()) {
            let mut data = vec![0u8; AC_RTCARINFO_SIZE];
            data[OFF_STEER..OFF_STEER + 4].copy_from_slice(&steer.to_le_bytes());
            if let Ok(result) = parse_ac_packet(&data) {
                prop_assert!(result.steering_angle >= -1.0);
                prop_assert!(result.steering_angle <= 1.0);
            }
        }

        #[test]
        fn parse_ac_packet_rpm_nonneg_on_valid_input(rpm in 0.0f32..=20000.0f32) {
            let mut data = vec![0u8; AC_RTCARINFO_SIZE];
            data[OFF_RPM..OFF_RPM + 4].copy_from_slice(&rpm.to_le_bytes());
            let result = parse_ac_packet(&data);
            prop_assert!(result.is_ok());
            let t = result.map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
            prop_assert!(t.rpm >= 0.0);
        }
    }
}

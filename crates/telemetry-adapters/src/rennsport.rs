//! Rennsport telemetry adapter using UDP on port 9000.
//!
//! Rennsport (Competition Company, 2023/2024) exposes UDP telemetry with a
//! layout similar to ACC.  Each packet begins with the identifier byte
//! `0x52` ('R') followed by three reserved bytes, then the payload fields:
//!
//! ```text
//! offset  0: u8    identifier (0x52 = 'R')
//! offset  1: [u8; 3] reserved
//! offset  4: f32   speed_kmh
//! offset  8: f32   rpm
//! offset 12: i8    gear  (-1 = reverse, 0 = neutral, 1+ = forward)
//! offset 13: [u8; 3] reserved
//! offset 16: f32   ffb_scalar  (-1.0 to 1.0)
//! offset 20: f32   slip_ratio  (0.0 to 1.0)
//! ```
//!
//! Update rate: 60 Hz.

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

const DEFAULT_RENNSPORT_PORT: u16 = 9000;
/// Minimum packet length to read all documented fields.
const RENNSPORT_MIN_PACKET_SIZE: usize = 24;
const MAX_PACKET_SIZE: usize = 512;

/// Expected identifier byte at the start of every Rennsport telemetry packet.
const RENNSPORT_IDENTIFIER: u8 = 0x52; // 'R'

// Field byte offsets.
const OFF_IDENTIFIER: usize = 0;
const OFF_SPEED_KMH: usize = 4;
const OFF_RPM: usize = 8;
const OFF_GEAR: usize = 12;
const OFF_FFB_SCALAR: usize = 16;
const OFF_SLIP_RATIO: usize = 20;

#[cfg(windows)]
const RENNSPORT_PROCESS_NAMES: &[&str] = &["rennsport.exe", "rennsport-win64-shipping.exe"];

/// Parse a raw Rennsport UDP packet into `NormalizedTelemetry`.
pub fn parse_rennsport_packet(data: &[u8]) -> Result<NormalizedTelemetry> {
    if data.len() < RENNSPORT_MIN_PACKET_SIZE {
        return Err(anyhow!(
            "Rennsport packet too short: expected at least {RENNSPORT_MIN_PACKET_SIZE}, got {}",
            data.len()
        ));
    }

    if data[OFF_IDENTIFIER] != RENNSPORT_IDENTIFIER {
        return Err(anyhow!(
            "Invalid Rennsport identifier: 0x{:02X}, expected 0x{:02X}",
            data[OFF_IDENTIFIER],
            RENNSPORT_IDENTIFIER
        ));
    }

    let speed_kmh = read_f32_le(data, OFF_SPEED_KMH).unwrap_or(0.0).max(0.0);
    let rpm = read_f32_le(data, OFF_RPM).unwrap_or(0.0).max(0.0);
    let gear = data[OFF_GEAR] as i8;
    let ffb_scalar = read_f32_le(data, OFF_FFB_SCALAR)
        .unwrap_or(0.0)
        .clamp(-1.0, 1.0);
    let slip_ratio = read_f32_le(data, OFF_SLIP_RATIO)
        .unwrap_or(0.0)
        .clamp(0.0, 1.0);

    let speed_ms = speed_kmh / 3.6;

    Ok(NormalizedTelemetry::builder()
        .speed_ms(speed_ms)
        .rpm(rpm)
        .gear(gear)
        .ffb_scalar(ffb_scalar)
        .slip_ratio(slip_ratio)
        .build())
}

/// Rennsport UDP telemetry adapter.
pub struct RennsportAdapter {
    bind_port: u16,
    update_rate: Duration,
}

impl Default for RennsportAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl RennsportAdapter {
    pub fn new() -> Self {
        Self {
            bind_port: DEFAULT_RENNSPORT_PORT,
            update_rate: Duration::from_millis(16), // ~60 Hz
        }
    }

    pub fn with_port(mut self, port: u16) -> Self {
        self.bind_port = port;
        self
    }
}

#[async_trait]
impl TelemetryAdapter for RennsportAdapter {
    fn game_id(&self) -> &str {
        "rennsport"
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
                    warn!("Failed to bind Rennsport UDP socket on port {bind_port}: {e}");
                    return;
                }
            };
            info!("Rennsport adapter listening on UDP port {bind_port}");
            let mut buf = [0u8; MAX_PACKET_SIZE];
            let mut frame_seq = 0u64;

            loop {
                match tokio::time::timeout(update_rate * 10, socket.recv(&mut buf)).await {
                    Ok(Ok(len)) => match parse_rennsport_packet(&buf[..len]) {
                        Ok(normalized) => {
                            let frame =
                                TelemetryFrame::new(normalized, telemetry_now_ns(), frame_seq, len);
                            if tx.send(frame).await.is_err() {
                                debug!("Receiver dropped, stopping Rennsport monitoring");
                                break;
                            }
                            frame_seq = frame_seq.saturating_add(1);
                        }
                        Err(e) => debug!("Failed to parse Rennsport packet: {e}"),
                    },
                    Ok(Err(e)) => warn!("Rennsport UDP receive error: {e}"),
                    Err(_) => debug!("No Rennsport telemetry data received (timeout)"),
                }
            }
            info!("Stopped Rennsport telemetry monitoring");
        });

        Ok(rx)
    }

    async fn stop_monitoring(&self) -> Result<()> {
        Ok(())
    }

    fn normalize(&self, raw: &[u8]) -> Result<NormalizedTelemetry> {
        parse_rennsport_packet(raw)
    }

    fn expected_update_rate(&self) -> Duration {
        self.update_rate
    }

    async fn is_game_running(&self) -> Result<bool> {
        Ok(is_rennsport_process_running())
    }
}

#[cfg(windows)]
fn is_rennsport_process_running() -> bool {
    use std::ffi::CStr;
    use std::mem;
    use winapi::um::{
        handleapi::{CloseHandle, INVALID_HANDLE_VALUE},
        tlhelp32::{
            CreateToolhelp32Snapshot, PROCESSENTRY32, Process32First, Process32Next,
            TH32CS_SNAPPROCESS,
        },
    };

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
                if RENNSPORT_PROCESS_NAMES.iter().any(|p| name.contains(p)) {
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
fn is_rennsport_process_running() -> bool {
    false
}

fn read_f32_le(data: &[u8], offset: usize) -> Option<f32> {
    data.get(offset..offset + 4)
        .and_then(|b| b.try_into().ok())
        .map(f32::from_le_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn make_rennsport_packet(
        speed_kmh: f32,
        rpm: f32,
        gear: i8,
        ffb_scalar: f32,
        slip_ratio: f32,
    ) -> Vec<u8> {
        let mut data = vec![0u8; RENNSPORT_MIN_PACKET_SIZE];
        data[OFF_IDENTIFIER] = RENNSPORT_IDENTIFIER;
        data[OFF_SPEED_KMH..OFF_SPEED_KMH + 4].copy_from_slice(&speed_kmh.to_le_bytes());
        data[OFF_RPM..OFF_RPM + 4].copy_from_slice(&rpm.to_le_bytes());
        data[OFF_GEAR] = gear as u8;
        data[OFF_FFB_SCALAR..OFF_FFB_SCALAR + 4].copy_from_slice(&ffb_scalar.to_le_bytes());
        data[OFF_SLIP_RATIO..OFF_SLIP_RATIO + 4].copy_from_slice(&slip_ratio.to_le_bytes());
        data
    }

    #[test]
    fn test_parse_valid_packet() -> TestResult {
        let data = make_rennsport_packet(180.0, 7500.0, 4, 0.6, 0.1);
        let result = parse_rennsport_packet(&data)?;
        // speed: 180 km/h → 50 m/s
        assert!(
            (result.speed_ms - 50.0).abs() < 0.01,
            "speed_ms={}",
            result.speed_ms
        );
        assert!((result.rpm - 7500.0).abs() < 0.1);
        assert_eq!(result.gear, 4);
        assert!((result.ffb_scalar - 0.6).abs() < 0.001);
        assert!((result.slip_ratio - 0.1).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn test_wrong_identifier_rejected() {
        let mut data = make_rennsport_packet(100.0, 5000.0, 3, 0.0, 0.0);
        data[OFF_IDENTIFIER] = 0x41; // 'A'
        assert!(parse_rennsport_packet(&data).is_err());
    }

    #[test]
    fn test_short_packet_rejected() {
        let data = vec![0u8; 8];
        assert!(parse_rennsport_packet(&data).is_err());
    }

    #[test]
    fn test_reverse_gear() -> TestResult {
        let data = make_rennsport_packet(0.0, 1000.0, -1, -0.1, 0.0);
        let result = parse_rennsport_packet(&data)?;
        assert_eq!(result.gear, -1);
        Ok(())
    }

    #[test]
    fn test_ffb_scalar_clamped() -> TestResult {
        // Packet carries a value beyond [-1, 1]; parser should clamp it.
        let data = make_rennsport_packet(200.0, 8000.0, 5, 5.0, 0.0);
        let result = parse_rennsport_packet(&data)?;
        assert!(
            result.ffb_scalar <= 1.0,
            "ffb_scalar not clamped: {}",
            result.ffb_scalar
        );
        Ok(())
    }

    #[test]
    fn test_slip_ratio_clamped() -> TestResult {
        let data = make_rennsport_packet(50.0, 6000.0, 3, 0.3, 2.0);
        let result = parse_rennsport_packet(&data)?;
        assert!(
            result.slip_ratio <= 1.0,
            "slip_ratio not clamped: {}",
            result.slip_ratio
        );
        Ok(())
    }

    #[test]
    fn test_adapter_game_id() {
        let adapter = RennsportAdapter::new();
        assert_eq!(adapter.game_id(), "rennsport");
    }

    #[test]
    fn test_update_rate() {
        let adapter = RennsportAdapter::new();
        assert_eq!(adapter.expected_update_rate(), Duration::from_millis(16));
    }

    #[test]
    fn test_empty_packet() {
        assert!(
            parse_rennsport_packet(&[]).is_err(),
            "empty packet must return an error"
        );
    }

    #[test]
    fn test_speed_is_nonnegative() -> TestResult {
        let data = make_rennsport_packet(144.0, 6500.0, 3, 0.4, 0.05);
        let result = parse_rennsport_packet(&data)?;
        assert!(
            result.speed_ms >= 0.0,
            "speed_ms must be non-negative, got {}",
            result.speed_ms
        );
        Ok(())
    }

    #[test]
    fn test_gear_in_valid_range() -> TestResult {
        for g in -1i8..=8 {
            let data = make_rennsport_packet(50.0, 5000.0, g, 0.3, 0.0);
            let result = parse_rennsport_packet(&data)?;
            assert!(
                result.gear >= -1 && result.gear <= 8,
                "gear {} out of expected range -1..=8",
                result.gear
            );
        }
        Ok(())
    }
}

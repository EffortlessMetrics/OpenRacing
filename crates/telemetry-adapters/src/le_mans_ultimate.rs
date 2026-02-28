//! Le Mans Ultimate telemetry adapter using an rFactor2-compatible UDP bridge.
//!
//! Le Mans Ultimate (Studio 397, 2024) is built on the rFactor 2 engine and exposes
//! telemetry via the rFactor2 UDP bridge plugin. Configure the bridge to send to port 6789.
//!
//! This adapter expects the simplified 5-field bridge format (20 bytes, little-endian f32):
//!
//! ```text
//! offset  0: f32  speed_ms      (m/s, unsigned)
//! offset  4: f32  rpm
//! offset  8: f32  gear          (-1.0 = reverse, 0.0 = neutral, 1+ = forward)
//! offset 12: f32  throttle      (0.0–1.0)
//! offset 16: f32  brake         (0.0–1.0)
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

const DEFAULT_PORT: u16 = 6789;
const MIN_PACKET_SIZE: usize = 20;
const MAX_PACKET_SIZE: usize = 512;

const ENV_PORT: &str = "OPENRACING_LE_MANS_ULTIMATE_UDP_PORT";

// Packet field offsets (f32, little-endian).
const OFF_SPEED: usize = 0;
const OFF_RPM: usize = 4;
const OFF_GEAR: usize = 8;
const OFF_THROTTLE: usize = 12;
const OFF_BRAKE: usize = 16;

/// Parse a raw Le Mans Ultimate rFactor2-bridge UDP packet.
pub fn parse_le_mans_ultimate_packet(data: &[u8]) -> Result<NormalizedTelemetry> {
    if data.len() < MIN_PACKET_SIZE {
        return Err(anyhow!(
            "Le Mans Ultimate packet too short: expected at least {MIN_PACKET_SIZE} bytes, got {}",
            data.len()
        ));
    }

    let speed_ms = read_f32(data, OFF_SPEED).unwrap_or(0.0).max(0.0);
    let rpm = read_f32(data, OFF_RPM).unwrap_or(0.0).max(0.0);
    let gear_raw = read_f32(data, OFF_GEAR).unwrap_or(0.0);
    let throttle = read_f32(data, OFF_THROTTLE).unwrap_or(0.0).clamp(0.0, 1.0);
    let brake = read_f32(data, OFF_BRAKE).unwrap_or(0.0).clamp(0.0, 1.0);

    let gear: i8 = if gear_raw < -0.5 {
        -1
    } else {
        (gear_raw.round() as i8).clamp(-1, 8)
    };

    // Derive a simple FFB scalar from throttle/brake differential (no lat-G in this format).
    let ffb_scalar = (throttle - brake).clamp(-1.0, 1.0);

    Ok(NormalizedTelemetry::builder()
        .speed_ms(speed_ms)
        .rpm(rpm)
        .gear(gear)
        .throttle(throttle)
        .brake(brake)
        .ffb_scalar(ffb_scalar)
        .build())
}

/// Le Mans Ultimate UDP telemetry adapter.
pub struct LeMansUltimateAdapter {
    bind_port: u16,
    update_rate: Duration,
}

impl Default for LeMansUltimateAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl LeMansUltimateAdapter {
    pub fn new() -> Self {
        let bind_port = std::env::var(ENV_PORT)
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .filter(|&p| p > 0)
            .unwrap_or(DEFAULT_PORT);
        Self {
            bind_port,
            update_rate: Duration::from_millis(16),
        }
    }

    pub fn with_port(mut self, port: u16) -> Self {
        self.bind_port = port;
        self
    }
}

#[async_trait]
impl TelemetryAdapter for LeMansUltimateAdapter {
    fn game_id(&self) -> &str {
        "le_mans_ultimate"
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
                    warn!("Failed to bind Le Mans Ultimate UDP socket on port {bind_port}: {e}");
                    return;
                }
            };
            info!("Le Mans Ultimate adapter listening on UDP port {bind_port}");
            let mut buf = [0u8; MAX_PACKET_SIZE];
            let mut frame_seq = 0u64;

            loop {
                match tokio::time::timeout(update_rate * 10, socket.recv(&mut buf)).await {
                    Ok(Ok(len)) => match parse_le_mans_ultimate_packet(&buf[..len]) {
                        Ok(normalized) => {
                            let frame =
                                TelemetryFrame::new(normalized, telemetry_now_ns(), frame_seq, len);
                            if tx.send(frame).await.is_err() {
                                debug!("Receiver dropped, stopping Le Mans Ultimate monitoring");
                                break;
                            }
                            frame_seq = frame_seq.saturating_add(1);
                        }
                        Err(e) => debug!("Failed to parse Le Mans Ultimate packet: {e}"),
                    },
                    Ok(Err(e)) => warn!("Le Mans Ultimate UDP receive error: {e}"),
                    Err(_) => debug!("No Le Mans Ultimate telemetry received (timeout)"),
                }
            }
            info!("Stopped Le Mans Ultimate telemetry monitoring");
        });

        Ok(rx)
    }

    async fn stop_monitoring(&self) -> Result<()> {
        Ok(())
    }

    fn normalize(&self, raw: &[u8]) -> Result<NormalizedTelemetry> {
        parse_le_mans_ultimate_packet(raw)
    }

    fn expected_update_rate(&self) -> Duration {
        self.update_rate
    }

    async fn is_game_running(&self) -> Result<bool> {
        Ok(is_le_mans_process_running())
    }
}

#[cfg(windows)]
fn is_le_mans_process_running() -> bool {
    use std::ffi::CStr;
    use std::mem;
    use winapi::um::{
        handleapi::{CloseHandle, INVALID_HANDLE_VALUE},
        tlhelp32::{
            CreateToolhelp32Snapshot, PROCESSENTRY32, Process32First, Process32Next,
            TH32CS_SNAPPROCESS,
        },
    };
    const PROCESS_NAMES: &[&str] = &["lemansultimate.exe", "le_mans_ultimate.exe", "lmu.exe"];
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
                if PROCESS_NAMES.iter().any(|p| name.contains(p)) {
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
fn is_le_mans_process_running() -> bool {
    false
}

fn read_f32(data: &[u8], offset: usize) -> Option<f32> {
    data.get(offset..offset + 4)
        .and_then(|b| b.try_into().ok())
        .map(f32::from_le_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn make_packet() -> Vec<u8> {
        vec![0u8; MIN_PACKET_SIZE]
    }

    fn write_f32(buf: &mut [u8], offset: usize, value: f32) {
        buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    #[test]
    fn test_parse_valid_packet() -> TestResult {
        let mut data = make_packet();
        write_f32(&mut data, OFF_SPEED, 50.0);
        write_f32(&mut data, OFF_RPM, 7000.0);
        write_f32(&mut data, OFF_GEAR, 5.0);
        write_f32(&mut data, OFF_THROTTLE, 0.9);
        write_f32(&mut data, OFF_BRAKE, 0.0);

        let t = parse_le_mans_ultimate_packet(&data)?;
        assert!((t.speed_ms - 50.0).abs() < 0.01);
        assert!((t.rpm - 7000.0).abs() < 0.1);
        assert_eq!(t.gear, 5);
        assert!((t.throttle - 0.9).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn test_short_packet_rejected() {
        assert!(parse_le_mans_ultimate_packet(&[0u8; 10]).is_err());
    }

    #[test]
    fn test_empty_packet_rejected() {
        assert!(parse_le_mans_ultimate_packet(&[]).is_err());
    }

    #[test]
    fn test_reverse_gear() -> TestResult {
        let mut data = make_packet();
        write_f32(&mut data, OFF_GEAR, -1.0);
        let t = parse_le_mans_ultimate_packet(&data)?;
        assert_eq!(t.gear, -1);
        Ok(())
    }

    #[test]
    fn test_throttle_clamped() -> TestResult {
        let mut data = make_packet();
        write_f32(&mut data, OFF_THROTTLE, 3.0);
        let t = parse_le_mans_ultimate_packet(&data)?;
        assert!(t.throttle <= 1.0);
        Ok(())
    }

    #[test]
    fn test_adapter_game_id() {
        assert_eq!(LeMansUltimateAdapter::new().game_id(), "le_mans_ultimate");
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(500))]

        #[test]
        fn prop_short_packet_returns_err(len in 0usize..MIN_PACKET_SIZE) {
            let data = vec![0u8; len];
            prop_assert!(parse_le_mans_ultimate_packet(&data).is_err());
        }

        #[test]
        fn prop_arbitrary_packet_no_panic(
            data in proptest::collection::vec(any::<u8>(), MIN_PACKET_SIZE..=128)
        ) {
            let _ = parse_le_mans_ultimate_packet(&data);
        }

        #[test]
        fn prop_speed_nonnegative(speed in 0.0f32..=300.0f32) {
            let mut buf = vec![0u8; MIN_PACKET_SIZE];
            buf[OFF_SPEED..OFF_SPEED + 4].copy_from_slice(&speed.to_le_bytes());
            let t = parse_le_mans_ultimate_packet(&buf).map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
            prop_assert!(t.speed_ms >= 0.0);
        }

        #[test]
        fn prop_throttle_brake_clamped(
            throttle in any::<f32>(),
            brake in any::<f32>()
        ) {
            let mut buf = vec![0u8; MIN_PACKET_SIZE];
            buf[OFF_THROTTLE..OFF_THROTTLE + 4].copy_from_slice(&throttle.to_le_bytes());
            buf[OFF_BRAKE..OFF_BRAKE + 4].copy_from_slice(&brake.to_le_bytes());
            if let Ok(t) = parse_le_mans_ultimate_packet(&buf) {
                prop_assert!(t.throttle >= 0.0 && t.throttle <= 1.0);
                prop_assert!(t.brake >= 0.0 && t.brake <= 1.0);
            }
        }
    }
}

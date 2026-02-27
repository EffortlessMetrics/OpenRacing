//! Wreckfest telemetry adapter using UDP telemetry on port 5606.
//!
//! Wreckfest (Bugbear Entertainment) sends UDP datagrams to port 5606.  
//! Each packet begins with the 4-byte magic `WRKF` (0x57 0x52 0x4B 0x46)
//! followed by payload data.  The simplified field layout used here:
//!
//! ```text
//! offset  0: [u8; 4]  magic  "WRKF"
//! offset  4: u32      packet seq number
//! offset  8: f32      speed_ms    (m/s)
//! offset 12: f32      rpm         (rev/min)
//! offset 16: u8       gear        (0 = neutral / reverse, 1+ = forward)
//! offset 17: [u8; 3]  reserved
//! offset 20: f32      lateral_g   (signed)
//! offset 24: f32      longitudinal_g (signed)
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

const DEFAULT_WRECKFEST_PORT: u16 = 5606;
/// Minimum packet length to read all documented fields.
const WRECKFEST_MIN_PACKET_SIZE: usize = 28;
const MAX_PACKET_SIZE: usize = 512;

/// Expected 4-byte magic at the start of every Wreckfest telemetry packet.
const WRECKFEST_MAGIC: [u8; 4] = [0x57, 0x52, 0x4B, 0x46]; // "WRKF"

// Field byte offsets.
const OFF_MAGIC: usize = 0;
const OFF_SPEED: usize = 8;
const OFF_RPM: usize = 12;
const OFF_GEAR: usize = 16;
const OFF_LATERAL_G: usize = 20;
const OFF_LONGITUDINAL_G: usize = 24;

#[cfg(windows)]
const WRECKFEST_PROCESS_NAMES: &[&str] = &["wreckfest.exe", "wreckfest_x64.exe"];

/// Parse a raw Wreckfest UDP packet into `NormalizedTelemetry`.
pub fn parse_wreckfest_packet(data: &[u8]) -> Result<NormalizedTelemetry> {
    if data.len() < WRECKFEST_MIN_PACKET_SIZE {
        return Err(anyhow!(
            "Wreckfest packet too short: expected at least {WRECKFEST_MIN_PACKET_SIZE}, got {}",
            data.len()
        ));
    }

    if data[OFF_MAGIC..OFF_MAGIC + 4] != WRECKFEST_MAGIC {
        return Err(anyhow!(
            "Invalid Wreckfest magic: {:?}",
            &data[OFF_MAGIC..OFF_MAGIC + 4]
        ));
    }

    let speed_ms = read_f32_le(data, OFF_SPEED).unwrap_or(0.0).max(0.0);
    let rpm = read_f32_le(data, OFF_RPM).unwrap_or(0.0).max(0.0);
    let gear_raw = data[OFF_GEAR];
    let lateral_g = read_f32_le(data, OFF_LATERAL_G).unwrap_or(0.0);
    let longitudinal_g = read_f32_le(data, OFF_LONGITUDINAL_G).unwrap_or(0.0);

    // Gear 0 in Wreckfest telemetry means neutral/reverse (no explicit reverse byte);
    // treat it as neutral since we have no separate reverse indicator.
    let gear: i8 = gear_raw.min(12) as i8;

    // Derive FFB scalar from combined lateral and longitudinal G-force.
    let combined_g = lateral_g.hypot(longitudinal_g);
    let ffb_scalar = (combined_g / 3.0).clamp(-1.0, 1.0);

    Ok(NormalizedTelemetry::builder()
        .speed_ms(speed_ms)
        .rpm(rpm)
        .gear(gear)
        .lateral_g(lateral_g)
        .longitudinal_g(longitudinal_g)
        .ffb_scalar(ffb_scalar)
        .build())
}

/// Wreckfest UDP telemetry adapter.
pub struct WreckfestAdapter {
    bind_port: u16,
    update_rate: Duration,
}

impl Default for WreckfestAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl WreckfestAdapter {
    pub fn new() -> Self {
        Self {
            bind_port: DEFAULT_WRECKFEST_PORT,
            update_rate: Duration::from_millis(16), // ~60 Hz
        }
    }

    pub fn with_port(mut self, port: u16) -> Self {
        self.bind_port = port;
        self
    }
}

#[async_trait]
impl TelemetryAdapter for WreckfestAdapter {
    fn game_id(&self) -> &str {
        "wreckfest"
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
                    warn!("Failed to bind Wreckfest UDP socket on port {bind_port}: {e}");
                    return;
                }
            };
            info!("Wreckfest adapter listening on UDP port {bind_port}");
            let mut buf = [0u8; MAX_PACKET_SIZE];
            let mut frame_idx = 0u64;

            loop {
                match tokio::time::timeout(update_rate * 10, socket.recv(&mut buf)).await {
                    Ok(Ok(len)) => match parse_wreckfest_packet(&buf[..len]) {
                        Ok(normalized) => {
                            let frame =
                                TelemetryFrame::new(normalized, telemetry_now_ns(), frame_idx, len);
                            if tx.send(frame).await.is_err() {
                                debug!("Receiver dropped, stopping Wreckfest monitoring");
                                break;
                            }
                            frame_idx = frame_idx.saturating_add(1);
                        }
                        Err(e) => debug!("Failed to parse Wreckfest packet: {e}"),
                    },
                    Ok(Err(e)) => warn!("Wreckfest UDP receive error: {e}"),
                    Err(_) => debug!("No Wreckfest telemetry data received (timeout)"),
                }
            }
            info!("Stopped Wreckfest telemetry monitoring");
        });

        Ok(rx)
    }

    async fn stop_monitoring(&self) -> Result<()> {
        Ok(())
    }

    fn normalize(&self, raw: &[u8]) -> Result<NormalizedTelemetry> {
        parse_wreckfest_packet(raw)
    }

    fn expected_update_rate(&self) -> Duration {
        self.update_rate
    }

    async fn is_game_running(&self) -> Result<bool> {
        Ok(is_wreckfest_process_running())
    }
}

#[cfg(windows)]
fn is_wreckfest_process_running() -> bool {
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
                if WRECKFEST_PROCESS_NAMES.iter().any(|p| name.contains(p)) {
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
fn is_wreckfest_process_running() -> bool {
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

    fn make_wreckfest_packet(
        speed: f32,
        rpm: f32,
        gear: u8,
        lateral_g: f32,
        longitudinal_g: f32,
    ) -> Vec<u8> {
        let mut data = vec![0u8; WRECKFEST_MIN_PACKET_SIZE];
        data[OFF_MAGIC..OFF_MAGIC + 4].copy_from_slice(&WRECKFEST_MAGIC);
        data[OFF_SPEED..OFF_SPEED + 4].copy_from_slice(&speed.to_le_bytes());
        data[OFF_RPM..OFF_RPM + 4].copy_from_slice(&rpm.to_le_bytes());
        data[OFF_GEAR] = gear;
        data[OFF_LATERAL_G..OFF_LATERAL_G + 4].copy_from_slice(&lateral_g.to_le_bytes());
        data[OFF_LONGITUDINAL_G..OFF_LONGITUDINAL_G + 4]
            .copy_from_slice(&longitudinal_g.to_le_bytes());
        data
    }

    #[test]
    fn test_parse_valid_packet() -> TestResult {
        let data = make_wreckfest_packet(30.0, 4000.0, 3, 0.5, 0.2);
        let result = parse_wreckfest_packet(&data)?;
        assert!((result.speed_ms - 30.0).abs() < 0.001);
        assert!((result.rpm - 4000.0).abs() < 0.1);
        assert_eq!(result.gear, 3);
        assert!((result.lateral_g - 0.5).abs() < 0.001);
        assert!((result.longitudinal_g - 0.2).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn test_magic_mismatch_rejected() {
        let mut data = make_wreckfest_packet(10.0, 2000.0, 2, 0.0, 0.0);
        data[0] = 0xFF;
        assert!(parse_wreckfest_packet(&data).is_err());
    }

    #[test]
    fn test_short_packet_rejected() {
        let data = vec![0u8; 10];
        assert!(parse_wreckfest_packet(&data).is_err());
    }

    #[test]
    fn test_ffb_scalar_range() -> TestResult {
        let data = make_wreckfest_packet(60.0, 7000.0, 5, 2.0, 1.5);
        let result = parse_wreckfest_packet(&data)?;
        assert!(
            result.ffb_scalar >= -1.0 && result.ffb_scalar <= 1.0,
            "ffb_scalar out of range: {}",
            result.ffb_scalar
        );
        Ok(())
    }

    #[test]
    fn test_neutral_gear() -> TestResult {
        let data = make_wreckfest_packet(0.0, 800.0, 0, 0.0, 0.0);
        let result = parse_wreckfest_packet(&data)?;
        assert_eq!(result.gear, 0);
        Ok(())
    }

    #[test]
    fn test_adapter_game_id() {
        let adapter = WreckfestAdapter::new();
        assert_eq!(adapter.game_id(), "wreckfest");
    }

    #[test]
    fn test_update_rate() {
        let adapter = WreckfestAdapter::new();
        assert_eq!(adapter.expected_update_rate(), Duration::from_millis(16));
    }

    #[test]
    fn test_empty_packet() {
        assert!(
            parse_wreckfest_packet(&[]).is_err(),
            "empty packet must return an error"
        );
    }

    #[test]
    fn test_speed_is_nonnegative() -> TestResult {
        let data = make_wreckfest_packet(45.0, 5500.0, 4, 0.3, 0.1);
        let result = parse_wreckfest_packet(&data)?;
        assert!(
            result.speed_ms >= 0.0,
            "speed_ms must be non-negative, got {}",
            result.speed_ms
        );
        Ok(())
    }

    #[test]
    fn test_gear_in_valid_range() -> TestResult {
        for g in 0u8..=8 {
            let data = make_wreckfest_packet(20.0, 3000.0, g, 0.1, 0.0);
            let result = parse_wreckfest_packet(&data)?;
            assert!(
                result.gear >= 0 && result.gear <= 8,
                "gear {} out of expected range 0..=8",
                result.gear
            );
        }
        Ok(())
    }
}

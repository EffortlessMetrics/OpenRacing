//! BeamNG.drive telemetry adapter using the LFS OutGauge UDP protocol.
//!
//! BeamNG.drive exposes telemetry via the standard 96-byte OutGauge packet on UDP port 4444.
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

const DEFAULT_BEAMNG_PORT: u16 = 4444;
/// Standard LFS OutGauge packet size.
const OUTGAUGE_PACKET_SIZE: usize = 96;
const MAX_PACKET_SIZE: usize = 256;

// OutGauge byte offsets
const OFF_SPEED: usize = 12; // f32, m/s
const OFF_RPM: usize = 16;   // f32
const OFF_GEAR: usize = 10;  // i8 (char in C): 0=R, 1=N, 2=1st, 3=2nd, …
const OFF_THROTTLE: usize = 48; // f32
const OFF_BRAKE: usize = 52;    // f32
const OFF_CLUTCH: usize = 56;   // f32

#[cfg(windows)]
const BEAMNG_PROCESS_NAMES: &[&str] = &["beamng.drive.x64.exe", "beamng.drive.exe"];

fn parse_outgauge_packet(data: &[u8]) -> Result<NormalizedTelemetry> {
    if data.len() < OUTGAUGE_PACKET_SIZE {
        return Err(anyhow!(
            "BeamNG OutGauge packet too short: expected {OUTGAUGE_PACKET_SIZE}, got {}",
            data.len()
        ));
    }

    let speed_mps = read_f32_le(data, OFF_SPEED).unwrap_or(0.0);
    let rpm = read_f32_le(data, OFF_RPM).unwrap_or(0.0);
    let gear_raw = data[OFF_GEAR]; // u8: 0=R, 1=N, 2=1st, 3=2nd, …
    let throttle = read_f32_le(data, OFF_THROTTLE).unwrap_or(0.0);
    let brake = read_f32_le(data, OFF_BRAKE).unwrap_or(0.0);
    let clutch = read_f32_le(data, OFF_CLUTCH).unwrap_or(0.0);

    // OutGauge gear: 0=Reverse, 1=Neutral, 2=1st gear, 3=2nd gear, …
    // Normalized:   -1=Reverse,  0=Neutral,  1=1st gear, 2=2nd gear, …
    let gear: i8 = match gear_raw {
        0 => -1,
        1 => 0,
        g => (g - 1) as i8, // g is u8 2..=255; g-1 is 1..=254, cast to i8 is safe for realistic gear values
    };

    Ok(NormalizedTelemetry::builder()
        .speed_ms(speed_mps)
        .rpm(rpm)
        .gear(gear)
        .throttle(throttle)
        .brake(brake)
        .clutch(clutch)
        .build())
}

/// BeamNG.drive telemetry adapter (OutGauge UDP).
pub struct BeamNGAdapter {
    bind_port: u16,
    update_rate: Duration,
}

impl Default for BeamNGAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl BeamNGAdapter {
    pub fn new() -> Self {
        Self {
            bind_port: DEFAULT_BEAMNG_PORT,
            update_rate: Duration::from_millis(16),
        }
    }

    pub fn with_port(mut self, port: u16) -> Self {
        self.bind_port = port;
        self
    }
}

#[async_trait]
impl TelemetryAdapter for BeamNGAdapter {
    fn game_id(&self) -> &str {
        "beamng_drive"
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
                    warn!("Failed to bind BeamNG UDP socket on port {bind_port}: {e}");
                    return;
                }
            };
            info!("BeamNG adapter listening on UDP port {bind_port}");
            let mut buf = [0u8; MAX_PACKET_SIZE];
            let mut sequence = 0u64;

            loop {
                match tokio::time::timeout(update_rate * 10, socket.recv(&mut buf)).await {
                    Ok(Ok(len)) => match parse_outgauge_packet(&buf[..len]) {
                        Ok(normalized) => {
                            let frame = TelemetryFrame::new(
                                normalized,
                                telemetry_now_ns(),
                                sequence,
                                len,
                            );
                            if tx.send(frame).await.is_err() {
                                debug!("Receiver dropped, stopping BeamNG monitoring");
                                break;
                            }
                            sequence = sequence.saturating_add(1);
                        }
                        Err(e) => debug!("Failed to parse BeamNG OutGauge packet: {e}"),
                    },
                    Ok(Err(e)) => warn!("BeamNG UDP receive error: {e}"),
                    Err(_) => debug!("No BeamNG telemetry data received (timeout)"),
                }
            }
            info!("Stopped BeamNG telemetry monitoring");
        });

        Ok(rx)
    }

    async fn stop_monitoring(&self) -> Result<()> {
        Ok(())
    }

    fn normalize(&self, raw: &[u8]) -> Result<NormalizedTelemetry> {
        parse_outgauge_packet(raw)
    }

    fn expected_update_rate(&self) -> Duration {
        self.update_rate
    }

    async fn is_game_running(&self) -> Result<bool> {
        Ok(is_beamng_process_running())
    }
}

#[cfg(windows)]
fn is_beamng_process_running() -> bool {
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
                if BEAMNG_PROCESS_NAMES.iter().any(|p| name.contains(p)) {
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
fn is_beamng_process_running() -> bool {
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

    fn make_outgauge_packet(
        speed: f32,
        rpm: f32,
        gear: u8,
        throttle: f32,
        brake: f32,
        clutch: f32,
    ) -> Vec<u8> {
        let mut data = vec![0u8; OUTGAUGE_PACKET_SIZE];
        data[OFF_SPEED..OFF_SPEED + 4].copy_from_slice(&speed.to_le_bytes());
        data[OFF_RPM..OFF_RPM + 4].copy_from_slice(&rpm.to_le_bytes());
        data[OFF_GEAR] = gear;
        data[OFF_THROTTLE..OFF_THROTTLE + 4].copy_from_slice(&throttle.to_le_bytes());
        data[OFF_BRAKE..OFF_BRAKE + 4].copy_from_slice(&brake.to_le_bytes());
        data[OFF_CLUTCH..OFF_CLUTCH + 4].copy_from_slice(&clutch.to_le_bytes());
        data
    }

    #[test]
    fn test_parse_valid_packet() -> TestResult {
        // OutGauge gear 3 = 2nd gear (normalized as 2)
        let data = make_outgauge_packet(30.0, 4500.0, 3, 0.6, 0.0, 0.0);
        let result = parse_outgauge_packet(&data)?;
        assert!((result.speed_ms - 30.0).abs() < 0.01);
        assert!((result.rpm - 4500.0).abs() < 0.01);
        assert_eq!(result.gear, 2);
        assert!((result.throttle - 0.6).abs() < 0.001);
        assert!((result.brake).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn test_gear_reverse() -> TestResult {
        let data = make_outgauge_packet(5.0, 2000.0, 0, 0.0, 0.5, 0.0);
        let result = parse_outgauge_packet(&data)?;
        assert_eq!(result.gear, -1);
        Ok(())
    }

    #[test]
    fn test_gear_neutral() -> TestResult {
        let data = make_outgauge_packet(0.0, 800.0, 1, 0.0, 0.0, 0.0);
        let result = parse_outgauge_packet(&data)?;
        assert_eq!(result.gear, 0);
        Ok(())
    }

    #[test]
    fn test_parse_truncated_packet() {
        let data = vec![0u8; 50];
        assert!(parse_outgauge_packet(&data).is_err());
    }

    #[test]
    fn test_normalization_clamp() -> TestResult {
        let data = make_outgauge_packet(100.0, 6000.0, 4, 1.5, 2.0, 0.0);
        let result = parse_outgauge_packet(&data)?;
        // Builder clamps throttle and brake to [0,1]
        assert!((result.throttle - 1.0).abs() < 0.001);
        assert!((result.brake - 1.0).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn test_adapter_game_id() {
        let adapter = BeamNGAdapter::new();
        assert_eq!(adapter.game_id(), "beamng_drive");
    }

    #[test]
    fn test_adapter_update_rate() {
        let adapter = BeamNGAdapter::new();
        assert_eq!(adapter.expected_update_rate(), Duration::from_millis(16));
    }

    #[test]
    fn test_normalize_delegates_to_parse() -> TestResult {
        let adapter = BeamNGAdapter::new();
        let data = make_outgauge_packet(40.0, 5000.0, 4, 0.8, 0.0, 0.1);
        let result = adapter.normalize(&data)?;
        assert!((result.speed_ms - 40.0).abs() < 0.01);
        Ok(())
    }
}

#[cfg(test)]
mod proptest_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn parse_outgauge_no_panic_on_arbitrary_bytes(
            data in proptest::collection::vec(any::<u8>(), 0..256)
        ) {
            let _ = parse_outgauge_packet(&data);
        }

        #[test]
        fn parse_outgauge_too_short_always_errors(size in 0usize..OUTGAUGE_PACKET_SIZE) {
            let data = vec![0u8; size];
            prop_assert!(parse_outgauge_packet(&data).is_err());
        }

        #[test]
        fn parse_outgauge_speed_nonneg(speed in 0.0f32..=300.0f32) {
            let mut data = vec![0u8; OUTGAUGE_PACKET_SIZE];
            data[OFF_SPEED..OFF_SPEED + 4].copy_from_slice(&speed.to_le_bytes());
            if let Ok(result) = parse_outgauge_packet(&data) {
                prop_assert!(result.speed_ms >= 0.0);
            }
        }

        #[test]
        fn parse_outgauge_throttle_clamped(throttle in any::<f32>()) {
            let mut data = vec![0u8; OUTGAUGE_PACKET_SIZE];
            data[OFF_THROTTLE..OFF_THROTTLE + 4].copy_from_slice(&throttle.to_le_bytes());
            if let Ok(result) = parse_outgauge_packet(&data) {
                prop_assert!(result.throttle >= 0.0);
                prop_assert!(result.throttle <= 1.0);
            }
        }

        #[test]
        fn parse_outgauge_brake_clamped(brake in any::<f32>()) {
            let mut data = vec![0u8; OUTGAUGE_PACKET_SIZE];
            data[OFF_BRAKE..OFF_BRAKE + 4].copy_from_slice(&brake.to_le_bytes());
            if let Ok(result) = parse_outgauge_packet(&data) {
                prop_assert!(result.brake >= 0.0);
                prop_assert!(result.brake <= 1.0);
            }
        }
    }
}

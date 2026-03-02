//! BeamNG.drive telemetry adapter using the LFS OutGauge UDP protocol.
//!
//! BeamNG.drive exposes telemetry via the standard OutGauge packet on a user-configured UDP
//! port (community convention: 4444). The packet is 92 bytes without the optional `id` field,
//! or 96 bytes when OutGauge ID is configured in BeamNG settings.
//!
//! ## Protocol verification (2025-07)
//!
//! Verified against these authoritative sources:
//! - BeamNG official docs: <https://documentation.beamng.com/modding/protocols/>
//! - BeamNG outgauge.lua: `lua/vehicle/protocols/outgauge.lua` (bCDDL-licensed game source)
//! - LFS InSim.txt OutGauge struct: <https://en.lfsmanual.net/wiki/OutGauge>
//! - Race-Element BeamNG provider (community, port 4444): <https://github.com/RiddleTime/Race-Element>
//!
//! BeamNG explicitly states: "It uses the same format used by Live For Speed."
//! The struct layout matches the LFS OutGauge spec exactly; the `id` field is optional.
//!
//! ### BeamNG-specific notes
//! - `time` field: hardcoded to 0 (N/A)
//! - `car[4]` field: always "beam"
//! - `oilPressure`: hardcoded to 0 (N/A)
//! - `display1`/`display2`: hardcoded to "" (N/A)
//! - Gear encoding: `electrics.values.gearIndex + 1` → 0=Reverse, 1=Neutral, 2=1st, …
//! - Port is user-configurable in Options > Other > Protocols; no fixed default in game.
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

/// Verified: BeamNG OutGauge community convention (Race-Element, SimHub, etc.).
/// The port is user-configurable in BeamNG: Options > Other > Protocols.
const DEFAULT_BEAMNG_PORT: u16 = 4444;
/// Base LFS OutGauge packet size (without optional `id` field).
/// Verified against: documentation.beamng.com/modding/protocols/ and LFS InSim.txt.
/// With `id` (i32) the packet is 96 bytes; without it, 92 bytes.
const OUTGAUGE_PACKET_SIZE: usize = 92;
const MAX_PACKET_SIZE: usize = 256;

// OutGauge byte offsets — verified 2025-07 against:
//   - BeamNG official: documentation.beamng.com/modding/protocols/
//   - BeamNG source: lua/vehicle/protocols/outgauge.lua (getStructDefinition)
//   - LFS manual: en.lfsmanual.net/wiki/OutGauge
//   - Race-Element OutGaugePacket C# struct (Pack=1)
// Layout: time(u32@0), car([4]u8@4), flags(u16@8), gear(u8@10), plid(u8@11),
//   speed(f32@12), rpm(f32@16), turbo(f32@20), engTemp(f32@24), fuel(f32@28),
//   oilPressure(f32@32), oilTemp(f32@36), dashLights(u32@40), showLights(u32@44),
//   throttle(f32@48), brake(f32@52), clutch(f32@56), display1([16]u8@60),
//   display2([16]u8@76), id(i32@92 optional). Total: 92 or 96 bytes.
const OFF_SPEED: usize = 12; // f32, m/s
const OFF_RPM: usize = 16; // f32
const OFF_GEAR: usize = 10; // u8: 0=R, 1=N, 2=1st, 3=2nd, … (verified: outgauge.lua sets gearIndex+1)
const OFF_THROTTLE: usize = 48; // f32, 0..1
const OFF_BRAKE: usize = 52; // f32, 0..1
const OFF_CLUTCH: usize = 56; // f32, 0..1

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
            let bind_addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, bind_port));
            let socket = match TokioUdpSocket::bind(bind_addr).await {
                Ok(s) => s,
                Err(e) => {
                    warn!("Failed to bind BeamNG UDP socket on port {bind_port}: {e}");
                    return;
                }
            };
            info!("BeamNG adapter listening on UDP port {bind_port}");
            let mut buf = [0u8; MAX_PACKET_SIZE];
            let mut frame_seq = 0u64;

            loop {
                match tokio::time::timeout(update_rate * 10, socket.recv(&mut buf)).await {
                    Ok(Ok(len)) => match parse_outgauge_packet(&buf[..len]) {
                        Ok(normalized) => {
                            let frame =
                                TelemetryFrame::new(normalized, telemetry_now_ns(), frame_seq, len);
                            if tx.send(frame).await.is_err() {
                                debug!("Receiver dropped, stopping BeamNG monitoring");
                                break;
                            }
                            frame_seq = frame_seq.saturating_add(1);
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
        .filter(|v| v.is_finite())
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

    /// Verify 92-byte packets (no optional `id` field) are accepted.
    /// This is the base OutGauge size per the LFS spec and BeamNG docs.
    #[test]
    fn test_parse_92_byte_packet_without_id() -> TestResult {
        let mut data = vec![0u8; 92];
        data[OFF_GEAR] = 2; // OutGauge 2 = 1st gear
        let result = parse_outgauge_packet(&data)?;
        assert_eq!(result.gear, 1); // normalized: 1st gear
        Ok(())
    }

    /// Verify 96-byte packets (with optional `id` field) are also accepted.
    #[test]
    fn test_parse_96_byte_packet_with_id() -> TestResult {
        let mut data = vec![0u8; 96];
        data[OFF_GEAR] = 1; // Neutral
        let result = parse_outgauge_packet(&data)?;
        assert_eq!(result.gear, 0);
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

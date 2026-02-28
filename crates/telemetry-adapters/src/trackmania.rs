//! Trackmania telemetry adapter using the OpenPlanet JSON-over-UDP bridge.
//!
//! The OpenPlanet plugin `TelemetryBridge` sends JSON-encoded state as UDP datagrams
//! on port 5004. Install the plugin from <https://openplanet.dev/> and enable it.
//!
//! JSON payload example:
//! ```json
//! {
//!   "speed": 83.2,
//!   "gear": 3,
//!   "rpm": 5500.0,
//!   "throttle": 1.0,
//!   "brake": 0.0,
//!   "steerAngle": -0.12,
//!   "engineRunning": true
//! }
//! ```
//!
//! Fields:
//! - `speed`          – vehicle speed in m/s (always ≥ 0)
//! - `gear`           – current gear (-1 = reverse, 0 = neutral, 1+ = forward)
//! - `rpm`            – engine RPM
//! - `throttle`       – 0.0–1.0
//! - `brake`          – 0.0–1.0
//! - `steerAngle`     – –1.0 (full left) to 1.0 (full right)
//! - `engineRunning`  – whether the engine is currently running
//!
//! Update rate: typically 60 Hz from the OpenPlanet bridge.

use crate::{
    NormalizedTelemetry, TelemetryAdapter, TelemetryFrame, TelemetryReceiver, telemetry_now_ns,
};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use serde::Deserialize;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::time::Duration;
use tokio::net::UdpSocket as TokioUdpSocket;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

const DEFAULT_PORT: u16 = 5004;
const MAX_PACKET_SIZE: usize = 4096;

const ENV_PORT: &str = "OPENRACING_TRACKMANIA_UDP_PORT";

/// Raw JSON payload sent by the OpenPlanet Trackmania bridge plugin.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TrackmaniaRaw {
    #[serde(default)]
    speed: f32,
    #[serde(default)]
    gear: i32,
    #[serde(default)]
    rpm: f32,
    #[serde(default)]
    throttle: f32,
    #[serde(default)]
    brake: f32,
    #[serde(default)]
    steer_angle: f32,
    #[serde(default)]
    engine_running: bool,
}

/// Parse a raw Trackmania UDP datagram (UTF-8 JSON) into [`NormalizedTelemetry`].
pub fn parse_trackmania_packet(data: &[u8]) -> Result<NormalizedTelemetry> {
    if data.is_empty() {
        return Err(anyhow!("Trackmania packet is empty"));
    }

    let text = std::str::from_utf8(data)
        .map_err(|e| anyhow!("Trackmania packet is not valid UTF-8: {e}"))?;

    let raw: TrackmaniaRaw =
        serde_json::from_str(text).map_err(|e| anyhow!("Failed to parse Trackmania JSON: {e}"))?;

    let speed_ms = raw.speed.max(0.0);
    let rpm = raw.rpm.max(0.0);
    let gear: i8 = (raw.gear as i8).clamp(-1, 8);
    let throttle = raw.throttle.clamp(0.0, 1.0);
    let brake = raw.brake.clamp(0.0, 1.0);
    let steer = raw.steer_angle.clamp(-1.0, 1.0);

    // FFB scalar derived from steering angle (Trackmania wheels respond to steer input).
    let ffb_scalar = steer;

    let _ = raw.engine_running; // preserved for potential future flag mapping

    Ok(NormalizedTelemetry::builder()
        .speed_ms(speed_ms)
        .rpm(rpm)
        .gear(gear)
        .throttle(throttle)
        .brake(brake)
        .steering_angle(steer)
        .ffb_scalar(ffb_scalar)
        .build())
}

/// Trackmania UDP telemetry adapter (OpenPlanet JSON bridge).
pub struct TrackmaniAdapter {
    bind_port: u16,
    update_rate: Duration,
}

impl Default for TrackmaniAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl TrackmaniAdapter {
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

/// Public alias matching the naming convention of the other adapters.
pub type TrackmaniaAdapter = TrackmaniAdapter;

#[async_trait]
impl TelemetryAdapter for TrackmaniAdapter {
    fn game_id(&self) -> &str {
        "trackmania"
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
                    warn!("Failed to bind Trackmania UDP socket on port {bind_port}: {e}");
                    return;
                }
            };
            info!("Trackmania adapter listening on UDP port {bind_port}");
            let mut buf = vec![0u8; MAX_PACKET_SIZE];
            let mut frame_seq = 0u64;

            loop {
                match tokio::time::timeout(update_rate * 10, socket.recv(&mut buf)).await {
                    Ok(Ok(len)) => match parse_trackmania_packet(&buf[..len]) {
                        Ok(normalized) => {
                            let frame =
                                TelemetryFrame::new(normalized, telemetry_now_ns(), frame_seq, len);
                            if tx.send(frame).await.is_err() {
                                debug!("Receiver dropped, stopping Trackmania monitoring");
                                break;
                            }
                            frame_seq = frame_seq.saturating_add(1);
                        }
                        Err(e) => debug!("Failed to parse Trackmania packet: {e}"),
                    },
                    Ok(Err(e)) => warn!("Trackmania UDP receive error: {e}"),
                    Err(_) => debug!("No Trackmania telemetry received (timeout)"),
                }
            }
            info!("Stopped Trackmania telemetry monitoring");
        });

        Ok(rx)
    }

    async fn stop_monitoring(&self) -> Result<()> {
        Ok(())
    }

    fn normalize(&self, raw: &[u8]) -> Result<NormalizedTelemetry> {
        parse_trackmania_packet(raw)
    }

    fn expected_update_rate(&self) -> Duration {
        self.update_rate
    }

    async fn is_game_running(&self) -> Result<bool> {
        Ok(is_trackmania_process_running())
    }
}

#[cfg(windows)]
fn is_trackmania_process_running() -> bool {
    use std::ffi::CStr;
    use std::mem;
    use winapi::um::{
        handleapi::{CloseHandle, INVALID_HANDLE_VALUE},
        tlhelp32::{
            CreateToolhelp32Snapshot, PROCESSENTRY32, Process32First, Process32Next,
            TH32CS_SNAPPROCESS,
        },
    };
    const PROCESS_NAMES: &[&str] = &["trackmania.exe", "trackmaniagame.exe", "tm2020.exe"];
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
fn is_trackmania_process_running() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn json(speed: f32, gear: i32, rpm: f32, throttle: f32, brake: f32, steer: f32) -> Vec<u8> {
        format!(
            r#"{{"speed":{speed},"gear":{gear},"rpm":{rpm},"throttle":{throttle},"brake":{brake},"steerAngle":{steer},"engineRunning":true}}"#
        )
        .into_bytes()
    }

    #[test]
    fn test_parse_valid_json() -> TestResult {
        let data = json(55.0, 4, 6000.0, 0.8, 0.0, 0.15);
        let t = parse_trackmania_packet(&data)?;
        assert!((t.speed_ms - 55.0).abs() < 0.01);
        assert_eq!(t.gear, 4);
        assert!((t.rpm - 6000.0).abs() < 0.1);
        assert!((t.throttle - 0.8).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn test_empty_packet_rejected() {
        assert!(parse_trackmania_packet(&[]).is_err());
    }

    #[test]
    fn test_invalid_utf8_rejected() {
        assert!(parse_trackmania_packet(&[0xFF, 0xFE, 0x00]).is_err());
    }

    #[test]
    fn test_invalid_json_rejected() {
        assert!(parse_trackmania_packet(b"not json").is_err());
    }

    #[test]
    fn test_reverse_gear() -> TestResult {
        let data = json(0.0, -1, 1000.0, 0.0, 0.5, 0.0);
        let t = parse_trackmania_packet(&data)?;
        assert_eq!(t.gear, -1);
        Ok(())
    }

    #[test]
    fn test_throttle_clamped() -> TestResult {
        let data = json(30.0, 3, 5000.0, 2.5, 0.0, 0.0);
        let t = parse_trackmania_packet(&data)?;
        assert!(t.throttle <= 1.0);
        Ok(())
    }

    #[test]
    fn test_steering_clamped() -> TestResult {
        let data = json(30.0, 3, 5000.0, 0.5, 0.0, 5.0);
        let t = parse_trackmania_packet(&data)?;
        assert!(t.steering_angle <= 1.0 && t.steering_angle >= -1.0);
        Ok(())
    }

    #[test]
    fn test_engine_running_field_parsed() -> TestResult {
        // engine_running is parsed from JSON without error (field is preserved for future use).
        let data = br#"{"speed":0.0,"gear":0,"rpm":0.0,"throttle":0.0,"brake":0.0,"steerAngle":0.0,"engineRunning":true}"#;
        let t = parse_trackmania_packet(data)?;
        assert_eq!(t.speed_ms, 0.0);
        Ok(())
    }

    #[test]
    fn test_adapter_game_id() {
        assert_eq!(TrackmaniaAdapter::new().game_id(), "trackmania");
    }

    #[test]
    fn test_missing_fields_defaults_to_zero() -> TestResult {
        // Only speed provided; other fields default to 0.
        let data = br#"{"speed":42.0}"#;
        let t = parse_trackmania_packet(data)?;
        assert!((t.speed_ms - 42.0).abs() < 0.01);
        assert_eq!(t.gear, 0);
        assert_eq!(t.rpm, 0.0);
        Ok(())
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(200))]

        /// Arbitrary bytes must never panic.
        #[test]
        fn prop_arbitrary_bytes_no_panic(data in proptest::collection::vec(any::<u8>(), 0..=2048)) {
            let _ = parse_trackmania_packet(&data);
        }

        /// Valid JSON with finite floats produces clamped output values.
        #[test]
        fn prop_valid_json_clamped(
            speed in 0.0f32..=300.0f32,
            gear in -1i32..=8i32,
            throttle in -2.0f32..=2.0f32,
            brake in -2.0f32..=2.0f32,
            steer in -2.0f32..=2.0f32,
        ) {
            let s = format!(
                r#"{{"speed":{speed},"gear":{gear},"rpm":5000.0,"throttle":{throttle},"brake":{brake},"steerAngle":{steer},"engineRunning":true}}"#
            );
            let t = parse_trackmania_packet(s.as_bytes()).expect("valid JSON");
            prop_assert!(t.speed_ms >= 0.0);
            prop_assert!(t.throttle >= 0.0 && t.throttle <= 1.0);
            prop_assert!(t.brake >= 0.0 && t.brake <= 1.0);
            prop_assert!(t.steering_angle >= -1.0 && t.steering_angle <= 1.0);
        }
    }
}

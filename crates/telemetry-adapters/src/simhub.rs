//! SimHub generic JSON UDP bridge adapter (port 5555).
//!
//! SimHub (SHWotever) provides a generic JSON UDP output that many games route
//! through.  Packets arrive as UTF-8 JSON objects on port 5555.
//!
//! JSON payload example:
//! ```json
//! {
//!   "SpeedMs": 22.5,
//!   "Rpms": 4500.0,
//!   "MaxRpms": 8000.0,
//!   "Gear": "3",
//!   "Throttle": 75.0,
//!   "Brake": 0.0,
//!   "Clutch": 0.0,
//!   "SteeringAngle": -15.5,
//!   "FuelPercent": 82.3,
//!   "LateralGForce": 1.2,
//!   "LongitudinalGForce": -0.5,
//!   "FFBValue": 0.35,
//!   "IsRunning": true,
//!   "IsInPit": false
//! }
//! ```
//!
//! Fields:
//! - `Rpms` / `Rpm`                      – engine RPM
//! - `MaxRpms`                            – maximum RPM for redline
//! - `Gear`                               – string: "R" = −1, "N"/"" = 0, "1"–"9" = 1–9
//! - `Throttle` / `Brake` / `Clutch`     – 0–100 (divided by 100 to normalise)
//! - `SteeringAngle`                      – degrees; divided by 450 and clamped to −1..1
//! - `Steer`                              – pre-normalised −1..1 form (preferred when non-zero)
//! - `FuelPercent`                        – 0–100 (divided by 100 to normalise)
//! - `LateralGForce` / `LatAcc`           – lateral G-force
//! - `LongitudinalGForce` / `LonAcc`      – longitudinal G-force
//! - `FFBValue`                           – force feedback scalar (already −1..1)
//!
//! Update rate: ~60 Hz.

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

const SIMHUB_PORT: u16 = 5555;
const MAX_PACKET_SIZE: usize = 4096;

/// Half the rotation range of a 900° wheel in degrees (±450°).
const STEER_HALF_RANGE_DEG: f32 = 450.0;

/// Raw JSON payload sent by the SimHub generic JSON UDP bridge.
#[derive(Debug, Deserialize)]
struct SimHubRaw {
    #[serde(default, rename = "SpeedMs")]
    speed_ms: f32,

    #[serde(default, rename = "Rpms", alias = "Rpm")]
    rpms: f32,

    #[serde(default, rename = "MaxRpms")]
    max_rpms: f32,

    #[serde(default, rename = "Gear")]
    gear: String,

    #[serde(default, rename = "Throttle")]
    throttle: f32,

    #[serde(default, rename = "Brake")]
    brake: f32,

    #[serde(default, rename = "Clutch")]
    clutch: f32,

    /// Steering angle in degrees (divide by 450 to normalise for a 900° wheel).
    #[serde(default, rename = "SteeringAngle")]
    steering_angle_deg: f32,

    /// Pre-normalised steering value (−1..1); preferred over `SteeringAngle` when non-zero.
    #[serde(default, rename = "Steer")]
    steer_normalized: f32,

    #[serde(default, rename = "FuelPercent")]
    fuel_percent: f32,

    #[serde(default, rename = "LateralGForce", alias = "LatAcc")]
    lateral_g_force: f32,

    #[serde(default, rename = "LongitudinalGForce", alias = "LonAcc")]
    longitudinal_g_force: f32,

    #[serde(default, rename = "FFBValue")]
    ffb_value: f32,

    #[serde(default, rename = "IsRunning")]
    is_running: bool,

    #[serde(default, rename = "IsInPit")]
    is_in_pit: bool,
}

/// Parse a gear string from SimHub JSON.
///
/// - `"R"` → `-1`
/// - `""` or `"N"` → `0`
/// - `"1"`–`"9"` → `1`–`9`
/// - Anything else → `0`
fn parse_gear(s: &str) -> i8 {
    match s.trim() {
        "R" => -1,
        "" | "N" => 0,
        other => other.parse::<i8>().unwrap_or(0),
    }
}

/// Parse a raw SimHub JSON UDP datagram (UTF-8) into [`NormalizedTelemetry`].
pub fn parse_simhub_packet(data: &[u8]) -> Result<NormalizedTelemetry> {
    if data.is_empty() {
        return Err(anyhow!("SimHub packet is empty"));
    }

    let text =
        std::str::from_utf8(data).map_err(|e| anyhow!("SimHub packet is not valid UTF-8: {e}"))?;

    let raw: SimHubRaw =
        serde_json::from_str(text).map_err(|e| anyhow!("Failed to parse SimHub JSON: {e}"))?;

    let speed_ms = raw.speed_ms.max(0.0);
    let rpm = raw.rpms.max(0.0);
    let max_rpm = raw.max_rpms.max(0.0);
    let gear = parse_gear(&raw.gear);
    let throttle = (raw.throttle / 100.0).clamp(0.0, 1.0);
    let brake = (raw.brake / 100.0).clamp(0.0, 1.0);
    let clutch = (raw.clutch / 100.0).clamp(0.0, 1.0);

    // Prefer pre-normalised Steer if provided; otherwise convert degrees.
    let steer = if raw.steer_normalized != 0.0 {
        raw.steer_normalized.clamp(-1.0, 1.0)
    } else {
        (raw.steering_angle_deg / STEER_HALF_RANGE_DEG).clamp(-1.0, 1.0)
    };

    let fuel_percent = (raw.fuel_percent / 100.0).clamp(0.0, 1.0);
    let ffb_scalar = raw.ffb_value.clamp(-1.0, 1.0);

    let _ = raw.is_running;
    let _ = raw.is_in_pit;

    Ok(NormalizedTelemetry::builder()
        .speed_ms(speed_ms)
        .rpm(rpm)
        .max_rpm(max_rpm)
        .gear(gear)
        .throttle(throttle)
        .brake(brake)
        .clutch(clutch)
        .steering_angle(steer)
        .fuel_percent(fuel_percent)
        .lateral_g(raw.lateral_g_force)
        .longitudinal_g(raw.longitudinal_g_force)
        .ffb_scalar(ffb_scalar)
        .build())
}

/// Generic SimHub JSON UDP bridge adapter.
pub struct SimHubAdapter {
    bind_port: u16,
    update_rate: Duration,
}

impl SimHubAdapter {
    pub fn new() -> Self {
        Self {
            bind_port: SIMHUB_PORT,
            update_rate: Duration::from_millis(16), // ~60 Hz
        }
    }
}

impl Default for SimHubAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TelemetryAdapter for SimHubAdapter {
    fn game_id(&self) -> &str {
        "simhub"
    }

    async fn start_monitoring(&self) -> Result<TelemetryReceiver> {
        let (tx, rx) = mpsc::channel(64);
        let bind_port = self.bind_port;
        let update_rate = self.update_rate;

        tokio::spawn(async move {
            let bind_addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, bind_port));
            let socket = match TokioUdpSocket::bind(bind_addr).await {
                Ok(s) => s,
                Err(e) => {
                    warn!("Failed to bind SimHub UDP socket on port {bind_port}: {e}");
                    return;
                }
            };
            info!("SimHub adapter listening on UDP port {bind_port}");
            let mut buf = [0u8; MAX_PACKET_SIZE];
            let mut frame_idx = 0u64;

            loop {
                match tokio::time::timeout(update_rate * 10, socket.recv(&mut buf)).await {
                    Ok(Ok(len)) => match parse_simhub_packet(&buf[..len]) {
                        Ok(normalized) => {
                            let frame =
                                TelemetryFrame::new(normalized, telemetry_now_ns(), frame_idx, len);
                            if tx.send(frame).await.is_err() {
                                debug!("Receiver dropped, stopping SimHub monitoring");
                                break;
                            }
                            frame_idx = frame_idx.saturating_add(1);
                        }
                        Err(e) => debug!("Failed to parse SimHub packet: {e}"),
                    },
                    Ok(Err(e)) => warn!("SimHub UDP receive error: {e}"),
                    Err(_) => debug!("No SimHub telemetry data received (timeout)"),
                }
            }
            info!("Stopped SimHub telemetry monitoring");
        });

        Ok(rx)
    }

    async fn stop_monitoring(&self) -> Result<()> {
        Ok(())
    }

    fn normalize(&self, raw: &[u8]) -> Result<NormalizedTelemetry> {
        parse_simhub_packet(raw)
    }

    fn expected_update_rate(&self) -> Duration {
        self.update_rate
    }

    async fn is_game_running(&self) -> Result<bool> {
        Ok(is_simhub_running())
    }
}

#[cfg(windows)]
pub fn is_simhub_running() -> bool {
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
                if name.contains("simhubwpf.exe") {
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
pub fn is_simhub_running() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn full_packet() -> &'static [u8] {
        br#"{"SpeedMs":22.5,"Rpms":4500.0,"MaxRpms":8000.0,"Gear":"3","Throttle":75.0,"Brake":10.0,"Clutch":0.0,"SteeringAngle":-90.0,"FuelPercent":82.3,"LateralGForce":1.2,"LongitudinalGForce":-0.5,"FFBValue":0.35,"IsRunning":true,"IsInPit":false}"#
    }

    fn zero_packet() -> &'static [u8] {
        br#"{"SpeedMs":0.0,"Rpms":0.0,"MaxRpms":0.0,"Gear":"N","Throttle":0.0,"Brake":0.0,"Clutch":0.0,"SteeringAngle":0.0,"FuelPercent":0.0,"LateralGForce":0.0,"LongitudinalGForce":0.0,"FFBValue":0.0,"IsRunning":false,"IsInPit":false}"#
    }

    #[test]
    fn test_parse_full_packet() -> TestResult {
        let t = parse_simhub_packet(full_packet())?;
        assert!((t.speed_ms - 22.5).abs() < 0.01, "speed_ms");
        assert!((t.rpm - 4500.0).abs() < 0.1, "rpm");
        assert!((t.max_rpm - 8000.0).abs() < 0.1, "max_rpm");
        assert_eq!(t.gear, 3, "gear");
        assert!((t.throttle - 0.75).abs() < 0.001, "throttle");
        assert!((t.brake - 0.10).abs() < 0.001, "brake");
        assert_eq!(t.clutch, 0.0, "clutch");
        // -90 deg / 450 = -0.2
        assert!((t.steering_angle - (-0.2)).abs() < 0.001, "steering_angle");
        assert!((t.fuel_percent - 0.823).abs() < 0.001, "fuel_percent");
        assert!((t.lateral_g - 1.2).abs() < 0.001, "lateral_g");
        assert!((t.longitudinal_g - (-0.5)).abs() < 0.001, "longitudinal_g");
        assert!((t.ffb_scalar - 0.35).abs() < 0.001, "ffb_scalar");
        Ok(())
    }

    #[test]
    fn test_parse_empty_bytes() {
        assert!(parse_simhub_packet(&[]).is_err());
    }

    #[test]
    fn test_parse_gear_string() {
        assert_eq!(parse_gear("R"), -1);
        assert_eq!(parse_gear("N"), 0);
        assert_eq!(parse_gear(""), 0);
        assert_eq!(parse_gear("3"), 3);
    }

    #[test]
    fn test_parse_throttle_normalized() -> TestResult {
        let data = br#"{"SpeedMs":0.0,"Rpms":0.0,"MaxRpms":0.0,"Gear":"N","Throttle":75.0,"Brake":0.0,"Clutch":0.0,"SteeringAngle":0.0,"FuelPercent":0.0,"LateralGForce":0.0,"LongitudinalGForce":0.0,"FFBValue":0.0,"IsRunning":true,"IsInPit":false}"#;
        let t = parse_simhub_packet(data)?;
        assert!((t.throttle - 0.75).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn test_parse_steer_degrees() -> TestResult {
        let data = br#"{"SpeedMs":0.0,"Rpms":0.0,"MaxRpms":0.0,"Gear":"N","Throttle":0.0,"Brake":0.0,"Clutch":0.0,"SteeringAngle":-450.0,"FuelPercent":0.0,"LateralGForce":0.0,"LongitudinalGForce":0.0,"FFBValue":0.0,"IsRunning":true,"IsInPit":false}"#;
        let t = parse_simhub_packet(data)?;
        assert!((t.steering_angle - (-1.0)).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn test_normalize_method() -> TestResult {
        let adapter = SimHubAdapter::new();
        let t = adapter.normalize(full_packet())?;
        assert!((t.speed_ms - 22.5).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn test_game_id() {
        assert_eq!(SimHubAdapter::new().game_id(), "simhub");
    }

    #[test]
    fn test_rpm_alias() -> TestResult {
        // Some SimHub configs send "Rpm" instead of "Rpms".
        let data = br#"{"SpeedMs":0.0,"Rpm":3000.0,"MaxRpms":0.0,"Gear":"N","Throttle":0.0,"Brake":0.0,"Clutch":0.0,"SteeringAngle":0.0,"FuelPercent":0.0,"LateralGForce":0.0,"LongitudinalGForce":0.0,"FFBValue":0.0,"IsRunning":true,"IsInPit":false}"#;
        let t = parse_simhub_packet(data)?;
        assert!((t.rpm - 3000.0).abs() < 0.1);
        Ok(())
    }

    #[test]
    fn test_zero_packet_parses() -> TestResult {
        let t = parse_simhub_packet(zero_packet())?;
        assert_eq!(t.speed_ms, 0.0);
        assert_eq!(t.gear, 0);
        Ok(())
    }
}

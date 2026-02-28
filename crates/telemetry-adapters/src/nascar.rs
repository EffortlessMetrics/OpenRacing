//! NASCAR Heat 5 / NASCAR 21 Ignition telemetry adapter (Papyrus UDP format).
//!
//! Enable UDP telemetry in-game: Options → Gameplay → Telemetry Output, port 5606.
//!
//! The packet is a fixed-layout binary stream (little-endian f32 at known offsets).
//! Packet structure based on community reverse-engineering of the Papyrus UDP format:
//! - <https://www.racedepartment.com/threads/nascar-heat-evolution-telemetry.152424/>
//! - SimHub dashboard community documentation for NASCAR Heat series
//!
//! Packet layout (all f32, little-endian unless noted):
//! ```text
//! offset  0: f32  time
//! offset  4: f32  pos_x
//! offset  8: f32  pos_y
//! offset 12: f32  pos_z
//! offset 16: f32  speed_ms   (m/s)
//! offset 20: f32  vel_x
//! offset 24: f32  vel_y
//! offset 28: f32  vel_z
//! offset 32: f32  acc_x  (longitudinal G, m/s²)
//! offset 36: f32  acc_y  (lateral G, m/s²)
//! offset 40: f32  acc_z  (vertical G, m/s²)
//! offset 44: f32  rot_x
//! offset 48: f32  rot_y
//! offset 52: f32  rot_z
//! offset 56: f32  yaw_rate
//! offset 60: f32  pitch_rate
//! offset 64: f32  roll_rate
//! offset 68: f32  gear    (float; -1.0 = reverse, 0.0 = neutral, 1+ = forward)
//! offset 72: f32  rpm
//! offset 76: f32  fuel    (litres)
//! offset 80: f32  throttle (0.0–1.0)
//! offset 84: f32  brake    (0.0–1.0)
//! offset 88: f32  steer    (-1.0 to 1.0, left negative)
//! ```

use crate::{
    NormalizedTelemetry, TelemetryAdapter, TelemetryFlags, TelemetryFrame, TelemetryReceiver,
    telemetry_now_ns,
};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::time::Duration;
use tokio::net::UdpSocket as TokioUdpSocket;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

const DEFAULT_PORT: u16 = 5606;
const MIN_PACKET_SIZE: usize = 92;
const MAX_PACKET_SIZE: usize = 512;

const ENV_PORT: &str = "OPENRACING_NASCAR_UDP_PORT";

// Byte offsets for Papyrus UDP packet fields (all f32, little-endian).
const OFF_SPEED: usize = 16;
const OFF_ACC_X: usize = 32;
const OFF_ACC_Y: usize = 36;
const OFF_GEAR: usize = 68;
const OFF_RPM: usize = 72;
const OFF_THROTTLE: usize = 80;
const OFF_BRAKE: usize = 84;
const OFF_STEER: usize = 88;

/// Lateral G normalisation range for FFB scalar (stock cars reach ~2 G in corners).
const FFB_LAT_G_MAX: f32 = 2.0;

/// Parse a raw NASCAR Papyrus UDP packet into [`NormalizedTelemetry`].
pub fn parse_nascar_packet(data: &[u8]) -> Result<NormalizedTelemetry> {
    if data.len() < MIN_PACKET_SIZE {
        return Err(anyhow!(
            "NASCAR packet too short: expected at least {MIN_PACKET_SIZE} bytes, got {}",
            data.len()
        ));
    }

    let speed_ms = read_f32(data, OFF_SPEED).unwrap_or(0.0).max(0.0);
    let acc_x = read_f32(data, OFF_ACC_X).unwrap_or(0.0);
    let acc_y = read_f32(data, OFF_ACC_Y).unwrap_or(0.0);
    let gear_raw = read_f32(data, OFF_GEAR).unwrap_or(0.0);
    let rpm = read_f32(data, OFF_RPM).unwrap_or(0.0).max(0.0);
    let throttle = read_f32(data, OFF_THROTTLE).unwrap_or(0.0).clamp(0.0, 1.0);
    let brake = read_f32(data, OFF_BRAKE).unwrap_or(0.0).clamp(0.0, 1.0);
    let steer = read_f32(data, OFF_STEER).unwrap_or(0.0).clamp(-1.0, 1.0);

    // Gear: -1 = reverse, 0 = neutral, 1+ = forward.
    let gear: i8 = if gear_raw < -0.5 {
        -1
    } else {
        (gear_raw.round() as i8).clamp(-1, 8)
    };

    // acc_y is lateral acceleration in m/s² from the game frame;
    // convert to G (÷9.81) for the normalised field.
    let lateral_g = acc_y / 9.81;
    let longitudinal_g = acc_x / 9.81;
    let ffb_scalar = (lateral_g / FFB_LAT_G_MAX).clamp(-1.0, 1.0);

    Ok(NormalizedTelemetry::builder()
        .speed_ms(speed_ms)
        .rpm(rpm)
        .gear(gear)
        .throttle(throttle)
        .brake(brake)
        .steering_angle(steer)
        .lateral_g(lateral_g)
        .longitudinal_g(longitudinal_g)
        .ffb_scalar(ffb_scalar)
        .flags(TelemetryFlags::default())
        .build())
}

/// NASCAR Heat 5 / NASCAR 21 Ignition UDP telemetry adapter.
pub struct NascarAdapter {
    bind_port: u16,
    update_rate: Duration,
}

impl Default for NascarAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl NascarAdapter {
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
impl TelemetryAdapter for NascarAdapter {
    fn game_id(&self) -> &str {
        "nascar"
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
                    warn!("Failed to bind NASCAR UDP socket on port {bind_port}: {e}");
                    return;
                }
            };
            info!("NASCAR adapter listening on UDP port {bind_port}");
            let mut buf = [0u8; MAX_PACKET_SIZE];
            let mut frame_seq = 0u64;

            loop {
                match tokio::time::timeout(update_rate * 10, socket.recv(&mut buf)).await {
                    Ok(Ok(len)) => match parse_nascar_packet(&buf[..len]) {
                        Ok(normalized) => {
                            let frame =
                                TelemetryFrame::new(normalized, telemetry_now_ns(), frame_seq, len);
                            if tx.send(frame).await.is_err() {
                                debug!("Receiver dropped, stopping NASCAR monitoring");
                                break;
                            }
                            frame_seq = frame_seq.saturating_add(1);
                        }
                        Err(e) => debug!("Failed to parse NASCAR packet: {e}"),
                    },
                    Ok(Err(e)) => warn!("NASCAR UDP receive error: {e}"),
                    Err(_) => debug!("No NASCAR telemetry received (timeout)"),
                }
            }
            info!("Stopped NASCAR telemetry monitoring");
        });

        Ok(rx)
    }

    async fn stop_monitoring(&self) -> Result<()> {
        Ok(())
    }

    fn normalize(&self, raw: &[u8]) -> Result<NormalizedTelemetry> {
        parse_nascar_packet(raw)
    }

    fn expected_update_rate(&self) -> Duration {
        self.update_rate
    }

    async fn is_game_running(&self) -> Result<bool> {
        Ok(is_nascar_process_running())
    }
}

#[cfg(windows)]
fn is_nascar_process_running() -> bool {
    use std::ffi::CStr;
    use std::mem;
    use winapi::um::{
        handleapi::{CloseHandle, INVALID_HANDLE_VALUE},
        tlhelp32::{
            CreateToolhelp32Snapshot, PROCESSENTRY32, Process32First, Process32Next,
            TH32CS_SNAPPROCESS,
        },
    };
    const PROCESS_NAMES: &[&str] = &[
        "nascarheat5.exe",
        "nascar21ignition.exe",
        "nascar2021.exe",
        "heat5.exe",
    ];
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
fn is_nascar_process_running() -> bool {
    false
}

fn read_f32(data: &[u8], offset: usize) -> Option<f32> {
    data.get(offset..offset + 4)
        .and_then(|b| b.try_into().ok())
        .map(f32::from_le_bytes)
        .filter(|v| v.is_finite())
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
        write_f32(&mut data, OFF_SPEED, 40.0);
        write_f32(&mut data, OFF_RPM, 6500.0);
        write_f32(&mut data, OFF_GEAR, 4.0);
        write_f32(&mut data, OFF_THROTTLE, 0.8);
        write_f32(&mut data, OFF_BRAKE, 0.0);
        write_f32(&mut data, OFF_STEER, 0.2);

        let t = parse_nascar_packet(&data)?;
        assert!((t.speed_ms - 40.0).abs() < 0.01);
        assert!((t.rpm - 6500.0).abs() < 0.1);
        assert_eq!(t.gear, 4);
        assert!((t.throttle - 0.8).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn test_short_packet_rejected() {
        assert!(parse_nascar_packet(&[0u8; 10]).is_err());
    }

    #[test]
    fn test_empty_packet_rejected() {
        assert!(parse_nascar_packet(&[]).is_err());
    }

    #[test]
    fn test_reverse_gear() -> TestResult {
        let mut data = make_packet();
        write_f32(&mut data, OFF_GEAR, -1.0);
        let t = parse_nascar_packet(&data)?;
        assert_eq!(t.gear, -1);
        Ok(())
    }

    #[test]
    fn test_neutral_gear() -> TestResult {
        let mut data = make_packet();
        write_f32(&mut data, OFF_GEAR, 0.0);
        let t = parse_nascar_packet(&data)?;
        assert_eq!(t.gear, 0);
        Ok(())
    }

    #[test]
    fn test_throttle_clamped() -> TestResult {
        let mut data = make_packet();
        write_f32(&mut data, OFF_THROTTLE, 2.5);
        let t = parse_nascar_packet(&data)?;
        assert!(t.throttle <= 1.0, "throttle {} must be ≤ 1.0", t.throttle);
        Ok(())
    }

    #[test]
    fn test_ffb_scalar_from_lat_g() -> TestResult {
        let mut data = make_packet();
        // acc_y = 2 * 9.81 = 19.62 m/s² → lateral_g = 2.0 → ffb_scalar = 1.0
        write_f32(&mut data, OFF_ACC_Y, 2.0 * 9.81);
        let t = parse_nascar_packet(&data)?;
        assert!((t.ffb_scalar - 1.0).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn test_adapter_game_id() {
        assert_eq!(NascarAdapter::new().game_id(), "nascar");
    }

    #[test]
    fn test_speed_nonnegative() -> TestResult {
        let mut data = make_packet();
        write_f32(&mut data, OFF_SPEED, 55.0);
        let t = parse_nascar_packet(&data)?;
        assert!(t.speed_ms >= 0.0);
        Ok(())
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(500))]

        /// Any packet shorter than MIN_PACKET_SIZE must return Err, never panic.
        #[test]
        fn prop_short_packet_returns_err(len in 0usize..MIN_PACKET_SIZE) {
            let data = vec![0u8; len];
            prop_assert!(parse_nascar_packet(&data).is_err());
        }

        /// Arbitrary bytes at or above MIN_PACKET_SIZE must never panic.
        #[test]
        fn prop_arbitrary_packet_no_panic(
            data in proptest::collection::vec(any::<u8>(), MIN_PACKET_SIZE..=512)
        ) {
            let _ = parse_nascar_packet(&data);
        }

        /// Speed from non-negative inputs stays non-negative.
        #[test]
        fn prop_speed_nonnegative(speed in 0.0f32..=200.0f32) {
            let mut buf = vec![0u8; MIN_PACKET_SIZE];
            buf[OFF_SPEED..OFF_SPEED + 4].copy_from_slice(&speed.to_le_bytes());
            let t = parse_nascar_packet(&buf).map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
            prop_assert!(t.speed_ms >= 0.0);
        }

        /// Throttle and brake are always in [0, 1].
        #[test]
        fn prop_throttle_brake_clamped(
            throttle in any::<f32>(),
            brake in any::<f32>()
        ) {
            let mut buf = vec![0u8; MIN_PACKET_SIZE];
            buf[OFF_THROTTLE..OFF_THROTTLE + 4].copy_from_slice(&throttle.to_le_bytes());
            buf[OFF_BRAKE..OFF_BRAKE + 4].copy_from_slice(&brake.to_le_bytes());
            if let Ok(t) = parse_nascar_packet(&buf) {
                prop_assert!(t.throttle >= 0.0 && t.throttle <= 1.0);
                prop_assert!(t.brake >= 0.0 && t.brake <= 1.0);
            }
        }

        /// FFB scalar is always in [-1, 1].
        #[test]
        fn prop_ffb_scalar_in_range(acc_y in any::<f32>()) {
            let mut buf = vec![0u8; MIN_PACKET_SIZE];
            buf[OFF_ACC_Y..OFF_ACC_Y + 4].copy_from_slice(&acc_y.to_le_bytes());
            if let Ok(t) = parse_nascar_packet(&buf) {
                prop_assert!(t.ffb_scalar >= -1.0 && t.ffb_scalar <= 1.0);
            }
        }
    }
}

//! Richard Burns Rally telemetry adapter (RBR LiveData UDP plugin).
//!
//! Decodes packets sent by the community RSF/RBR LiveData UDP plugin on port 6776.
//! Supports both the 184-byte current packet format and the older 128-byte format.
#![cfg_attr(not(windows), allow(unused, dead_code))]

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

const DEFAULT_RBR_PORT: u16 = 6776;
/// Minimum packet size supported (older plugin version).
const MIN_PACKET_SIZE: usize = 128;
/// Full packet size (current plugin version).
#[cfg(test)]
const FULL_PACKET_SIZE: usize = 184;
const MAX_PACKET_SIZE: usize = 256;

// Byte offsets — all fields are little-endian f32
const OFF_SPEED_MS: usize = 12;
const OFF_THROTTLE: usize = 52;
const OFF_BRAKE: usize = 56;
const OFF_CLUTCH: usize = 60;
const OFF_GEAR: usize = 64;
const OFF_STEERING: usize = 68;
const OFF_HANDBRAKE: usize = 112;
const OFF_RPM: usize = 116;

fn read_f32(data: &[u8], offset: usize) -> f32 {
    if offset + 4 > data.len() {
        return 0.0;
    }
    f32::from_le_bytes([data[offset], data[offset + 1], data[offset + 2], data[offset + 3]])
}

fn parse_rbr_packet(data: &[u8]) -> Result<NormalizedTelemetry> {
    if data.len() < MIN_PACKET_SIZE {
        return Err(anyhow!(
            "RBR LiveData packet too short: expected at least {MIN_PACKET_SIZE}, got {}",
            data.len()
        ));
    }

    let speed_ms = read_f32(data, OFF_SPEED_MS);
    let throttle = read_f32(data, OFF_THROTTLE);
    let brake = read_f32(data, OFF_BRAKE);
    let clutch = read_f32(data, OFF_CLUTCH);
    let gear_raw = read_f32(data, OFF_GEAR);
    let steering = read_f32(data, OFF_STEERING);
    let handbrake = read_f32(data, OFF_HANDBRAKE);
    let rpm = read_f32(data, OFF_RPM);

    // RBR gear encoding: 0 = reverse, 1..6 = forward gears (no neutral defined in protocol).
    // Normalized: -1 = reverse, 1..6 = forward.
    let gear: i8 = if gear_raw < 0.5 {
        -1
    } else {
        (gear_raw.round() as i8).max(1)
    };

    // Use throttle-brake differential as a force feedback proxy (-1.0..1.0).
    let ffb_scalar = throttle - brake;

    // Handbrake > 0.5 is exposed via session_paused (closest available flag).
    let flags = TelemetryFlags {
        session_paused: handbrake > 0.5,
        ..TelemetryFlags::default()
    };

    Ok(NormalizedTelemetry::builder()
        .speed_ms(speed_ms)
        .rpm(rpm)
        .gear(gear)
        .throttle(throttle)
        .brake(brake)
        .clutch(clutch)
        .steering_angle(steering)
        .ffb_scalar(ffb_scalar)
        .flags(flags)
        .build())
}

/// Richard Burns Rally telemetry adapter (RBR LiveData UDP plugin on port 6776).
pub struct RBRAdapter {
    bind_port: u16,
    update_rate: Duration,
}

impl Default for RBRAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl RBRAdapter {
    pub fn new() -> Self {
        Self {
            bind_port: DEFAULT_RBR_PORT,
            update_rate: Duration::from_millis(17), // ~60 Hz (game framerate)
        }
    }

    pub fn with_port(mut self, port: u16) -> Self {
        self.bind_port = port;
        self
    }
}

#[async_trait]
impl TelemetryAdapter for RBRAdapter {
    fn game_id(&self) -> &str {
        "rbr"
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
                    warn!("Failed to bind RBR UDP socket on port {bind_port}: {e}");
                    return;
                }
            };
            info!("RBR adapter listening on UDP port {bind_port}");
            let mut buf = [0u8; MAX_PACKET_SIZE];
            let mut sequence = 0u64;

            loop {
                match tokio::time::timeout(update_rate * 10, socket.recv(&mut buf)).await {
                    Ok(Ok(len)) => match parse_rbr_packet(&buf[..len]) {
                        Ok(normalized) => {
                            let frame = TelemetryFrame::new(
                                normalized,
                                telemetry_now_ns(),
                                sequence,
                                len,
                            );
                            if tx.send(frame).await.is_err() {
                                debug!("Receiver dropped, stopping RBR monitoring");
                                break;
                            }
                            sequence = sequence.saturating_add(1);
                        }
                        Err(e) => debug!("Failed to parse RBR LiveData packet: {e}"),
                    },
                    Ok(Err(e)) => warn!("RBR UDP receive error: {e}"),
                    Err(_) => debug!("No RBR telemetry data received (timeout)"),
                }
            }
            info!("Stopped RBR telemetry monitoring");
        });

        Ok(rx)
    }

    async fn stop_monitoring(&self) -> Result<()> {
        Ok(())
    }

    fn normalize(&self, raw: &[u8]) -> Result<NormalizedTelemetry> {
        parse_rbr_packet(raw)
    }

    fn expected_update_rate(&self) -> Duration {
        self.update_rate
    }

    async fn is_game_running(&self) -> Result<bool> {
        Ok(is_rbr_process_running())
    }
}

#[cfg(windows)]
fn is_rbr_process_running() -> bool {
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
                if name.contains("rbr.exe") || name.contains("richardburnsrally") {
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
fn is_rbr_process_running() -> bool {
    false
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    fn make_min_packet() -> Vec<u8> {
        vec![0u8; MIN_PACKET_SIZE]
    }

    fn write_f32_le(buf: &mut [u8], offset: usize, value: f32) {
        buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(500))]

        /// Any packet shorter than MIN_PACKET_SIZE must return Err, never panic.
        #[test]
        fn prop_short_packet_returns_err(len in 0usize..MIN_PACKET_SIZE) {
            let data = vec![0u8; len];
            prop_assert!(parse_rbr_packet(&data).is_err());
        }

        /// Arbitrary bytes at or above MIN_PACKET_SIZE must never panic.
        #[test]
        fn prop_arbitrary_packet_no_panic(
            data in proptest::collection::vec(any::<u8>(), MIN_PACKET_SIZE..=256)
        ) {
            let _ = parse_rbr_packet(&data);
        }

        /// Gear is always -1 (reverse) or >= 1 (forward) — never 0 or other invalid values.
        #[test]
        fn prop_gear_is_reverse_or_forward(gear_val in 0.0f32..=10.0f32) {
            let mut buf = make_min_packet();
            write_f32_le(&mut buf, OFF_GEAR, gear_val);
            let t = parse_rbr_packet(&buf).expect("parse must succeed for valid-size packet");
            prop_assert!(
                t.gear == -1 || t.gear >= 1,
                "gear {} must be -1 (reverse) or >= 1 (forward)",
                t.gear
            );
        }

        /// FFB scalar equals throttle minus brake for inputs in [0, 1].
        #[test]
        fn prop_ffb_scalar_matches_throttle_minus_brake(
            throttle in 0.0f32..=1.0f32,
            brake in 0.0f32..=1.0f32,
        ) {
            let mut buf = make_min_packet();
            write_f32_le(&mut buf, OFF_THROTTLE, throttle);
            write_f32_le(&mut buf, OFF_BRAKE, brake);
            let t = parse_rbr_packet(&buf).expect("parse must succeed for valid-size packet");
            let expected = throttle - brake;
            prop_assert!(
                (t.ffb_scalar - expected).abs() < 1e-5,
                "ffb_scalar {} must equal throttle({}) - brake({})",
                t.ffb_scalar,
                throttle,
                brake
            );
        }

        /// Handbrake > 0.5 sets session_paused; <= 0.5 clears it.
        #[test]
        fn prop_handbrake_flag_threshold(handbrake in 0.0f32..=2.0f32) {
            let mut buf = make_min_packet();
            write_f32_le(&mut buf, OFF_HANDBRAKE, handbrake);
            let t = parse_rbr_packet(&buf).expect("parse must succeed for valid-size packet");
            let expected = handbrake > 0.5;
            prop_assert_eq!(
                t.flags.session_paused,
                expected,
                "session_paused must be {} for handbrake={}",
                expected,
                handbrake
            );
        }

        /// Speed from valid finite inputs is finite.
        #[test]
        fn prop_speed_is_finite(speed in 0.0f32..=300.0f32) {
            let mut buf = make_min_packet();
            write_f32_le(&mut buf, OFF_SPEED_MS, speed);
            let t = parse_rbr_packet(&buf).expect("parse must succeed for valid-size packet");
            prop_assert!(t.speed_ms.is_finite(), "speed_ms {} must be finite", t.speed_ms);
        }

        /// RPM from valid finite inputs is finite.
        #[test]
        fn prop_rpm_is_finite(rpm in 0.0f32..=20000.0f32) {
            let mut buf = make_min_packet();
            write_f32_le(&mut buf, OFF_RPM, rpm);
            let t = parse_rbr_packet(&buf).expect("parse must succeed for valid-size packet");
            prop_assert!(t.rpm.is_finite(), "rpm {} must be finite", t.rpm);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn make_packet(size: usize) -> Vec<u8> {
        vec![0u8; size]
    }

    fn write_f32(data: &mut [u8], offset: usize, value: f32) {
        data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    #[test]
    fn test_too_short_packet() {
        let data = make_packet(64);
        assert!(parse_rbr_packet(&data).is_err());
    }

    #[test]
    fn test_128_byte_zero_packet() -> TestResult {
        let data = make_packet(MIN_PACKET_SIZE);
        let result = parse_rbr_packet(&data)?;
        assert_eq!(result.speed_ms, 0.0);
        assert_eq!(result.rpm, 0.0);
        assert_eq!(result.gear, -1); // gear 0.0 → reverse
        Ok(())
    }

    #[test]
    fn test_speed_extraction() -> TestResult {
        let mut data = make_packet(FULL_PACKET_SIZE);
        write_f32(&mut data, OFF_SPEED_MS, 50.0);
        let result = parse_rbr_packet(&data)?;
        assert!((result.speed_ms - 50.0).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn test_known_values_184_byte_packet() -> TestResult {
        let mut data = make_packet(FULL_PACKET_SIZE);
        write_f32(&mut data, OFF_SPEED_MS, 30.5);
        write_f32(&mut data, OFF_RPM, 5500.0);
        write_f32(&mut data, OFF_GEAR, 3.0);
        write_f32(&mut data, OFF_THROTTLE, 0.75);
        write_f32(&mut data, OFF_BRAKE, 0.0);
        write_f32(&mut data, OFF_STEERING, -0.3);
        let result = parse_rbr_packet(&data)?;
        assert!((result.speed_ms - 30.5).abs() < 0.001);
        assert!((result.rpm - 5500.0).abs() < 0.01);
        assert_eq!(result.gear, 3);
        assert!((result.throttle - 0.75).abs() < 0.001);
        assert!(result.brake.abs() < 0.001);
        assert!((result.steering_angle - (-0.3)).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn test_gear_reverse() -> TestResult {
        let mut data = make_packet(FULL_PACKET_SIZE);
        write_f32(&mut data, OFF_GEAR, 0.0);
        let result = parse_rbr_packet(&data)?;
        assert_eq!(result.gear, -1);
        Ok(())
    }

    #[test]
    fn test_gear_forward() -> TestResult {
        let mut data = make_packet(FULL_PACKET_SIZE);
        write_f32(&mut data, OFF_GEAR, 4.0);
        let result = parse_rbr_packet(&data)?;
        assert_eq!(result.gear, 4);
        Ok(())
    }

    #[test]
    fn test_handbrake_flag_extraction() -> TestResult {
        let mut data = make_packet(FULL_PACKET_SIZE);
        write_f32(&mut data, OFF_HANDBRAKE, 1.0);
        let result = parse_rbr_packet(&data)?;
        assert!(result.flags.session_paused);
        Ok(())
    }

    #[test]
    fn test_handbrake_off() -> TestResult {
        let mut data = make_packet(FULL_PACKET_SIZE);
        write_f32(&mut data, OFF_HANDBRAKE, 0.0);
        let result = parse_rbr_packet(&data)?;
        assert!(!result.flags.session_paused);
        Ok(())
    }

    #[test]
    fn test_adapter_game_id() {
        let adapter = RBRAdapter::new();
        assert_eq!(adapter.game_id(), "rbr");
    }

    #[test]
    fn test_adapter_update_rate() {
        let adapter = RBRAdapter::new();
        assert_eq!(adapter.expected_update_rate(), Duration::from_millis(17));
    }

    #[test]
    fn test_normalize_delegates_to_parse() -> TestResult {
        let adapter = RBRAdapter::new();
        let mut data = make_packet(FULL_PACKET_SIZE);
        write_f32(&mut data, OFF_SPEED_MS, 50.0);
        let result = adapter.normalize(&data)?;
        assert!((result.speed_ms - 50.0).abs() < 0.001);
        Ok(())
    }
}

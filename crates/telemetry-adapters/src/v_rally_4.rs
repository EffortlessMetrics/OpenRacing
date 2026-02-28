//! V-Rally 4 (Kylotonn, 2018) telemetry adapter — Kylotonn UDP format on port 64000.
//!
//! V-Rally 4 uses the same community-documented Kylotonn UDP binary stream as
//! WRC 9 / WRC 10, broadcast on UDP port 64000 (little-endian packed struct).
//!
//! | Offset | Type  | Field            |
//! |--------|-------|------------------|
//! | 0      | f32   | stage_progress   |
//! | 4      | f32   | speed_ms         |
//! | 8      | f32   | steering         |
//! | 12     | f32   | throttle         |
//! | 16     | f32   | brake            |
//! | 20     | f32   | hand_brake       |
//! | 24     | f32   | clutch           |
//! | 28     | u32   | gear             |
//! | 32     | f32   | rpm              |
//! | 36     | f32   | max_rpm          |
//! | 40–52  | f32×4 | suspension       |
//! | 56     | f32   | pos_x            |
//! | 60     | f32   | pos_y            |
//! | 64     | f32   | pos_z            |
//! | 68     | f32   | roll             |
//! | 72     | f32   | pitch            |
//! | 76     | f32   | yaw              |
//! | 80     | f32   | vel_x            |
//! | 84     | f32   | vel_y            |
//! | 88     | f32   | vel_z            |
//! | 92     | f32   | wheel_speed_rr   |

use crate::{
    NormalizedTelemetry, TelemetryAdapter, TelemetryFrame, TelemetryReceiver, TelemetryValue,
    telemetry_now_ns,
};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::Duration;
use tokio::net::UdpSocket as TokioUdpSocket;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

const DEFAULT_PORT: u16 = 64000;
const MIN_PACKET_SIZE: usize = 96;
const MAX_PACKET_SIZE: usize = 256;
const DEFAULT_HEARTBEAT_TIMEOUT_MS: u64 = 1_500;

const ENV_PORT: &str = "OPENRACING_V_RALLY_4_UDP_PORT";
const ENV_HEARTBEAT_TIMEOUT_MS: &str = "OPENRACING_V_RALLY_4_HEARTBEAT_TIMEOUT_MS";

// Byte offsets (all fields little-endian).
const OFF_SPEED_MS: usize = 4;
const OFF_STEERING: usize = 8;
const OFF_THROTTLE: usize = 12;
const OFF_BRAKE: usize = 16;
const OFF_CLUTCH: usize = 24;
const OFF_GEAR: usize = 28; // u32
const OFF_RPM: usize = 32;
const OFF_MAX_RPM: usize = 36;
const OFF_POS_X: usize = 56;
const OFF_POS_Y: usize = 60;
const OFF_POS_Z: usize = 64;
const OFF_VEL_X: usize = 80;
const OFF_VEL_Y: usize = 84;
const OFF_VEL_Z: usize = 88;

/// V-Rally 4 telemetry adapter (Kylotonn UDP format, port 64000).
#[derive(Clone)]
pub struct VRally4Adapter {
    bind_port: u16,
    update_rate: Duration,
    heartbeat_timeout: Duration,
    last_packet_ns: Arc<AtomicU64>,
}

impl Default for VRally4Adapter {
    fn default() -> Self {
        Self::new()
    }
}

impl VRally4Adapter {
    pub fn new() -> Self {
        let bind_port = std::env::var(ENV_PORT)
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .filter(|&p| p > 0)
            .unwrap_or(DEFAULT_PORT);

        let heartbeat_ms = std::env::var(ENV_HEARTBEAT_TIMEOUT_MS)
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|&t| t > 0)
            .unwrap_or(DEFAULT_HEARTBEAT_TIMEOUT_MS);

        Self {
            bind_port,
            update_rate: Duration::from_millis(16),
            heartbeat_timeout: Duration::from_millis(heartbeat_ms),
            last_packet_ns: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn with_port(mut self, port: u16) -> Self {
        self.bind_port = port;
        self
    }

    fn is_recent_packet(&self) -> bool {
        let last = self.last_packet_ns.load(Ordering::Relaxed);
        if last == 0 {
            return false;
        }
        let now = u128::from(telemetry_now_ns());
        let elapsed_ns = now.saturating_sub(u128::from(last));
        elapsed_ns <= self.heartbeat_timeout.as_nanos()
    }
}

fn read_f32_le(data: &[u8], offset: usize) -> Option<f32> {
    data.get(offset..offset + 4)
        .and_then(|b| b.try_into().ok())
        .map(f32::from_le_bytes)
        .filter(|v| v.is_finite())
}

fn read_u32_le(data: &[u8], offset: usize) -> Option<u32> {
    data.get(offset..offset + 4)
        .and_then(|b| b.try_into().ok())
        .map(u32::from_le_bytes)
}

fn parse_packet(data: &[u8]) -> Result<NormalizedTelemetry> {
    if data.len() < MIN_PACKET_SIZE {
        return Err(anyhow!(
            "V-Rally 4 packet too short: expected at least {MIN_PACKET_SIZE}, got {}",
            data.len()
        ));
    }

    let speed_ms = read_f32_le(data, OFF_SPEED_MS).unwrap_or(0.0).max(0.0);
    let steering = read_f32_le(data, OFF_STEERING)
        .unwrap_or(0.0)
        .clamp(-1.0, 1.0);
    let throttle = read_f32_le(data, OFF_THROTTLE)
        .unwrap_or(0.0)
        .clamp(0.0, 1.0);
    let brake = read_f32_le(data, OFF_BRAKE).unwrap_or(0.0).clamp(0.0, 1.0);
    let clutch = read_f32_le(data, OFF_CLUTCH).unwrap_or(0.0).clamp(0.0, 1.0);

    // gear: 0 = reverse (-1), 1–7 = forward gears.
    let gear_raw = read_u32_le(data, OFF_GEAR).unwrap_or(0);
    let gear: i8 = if gear_raw == 0 {
        -1
    } else {
        (gear_raw as i8).clamp(1, 7)
    };

    let rpm = read_f32_le(data, OFF_RPM).unwrap_or(0.0).max(0.0);
    let max_rpm = read_f32_le(data, OFF_MAX_RPM).unwrap_or(0.0).max(0.0);

    let pos_x = read_f32_le(data, OFF_POS_X).unwrap_or(0.0);
    let pos_y = read_f32_le(data, OFF_POS_Y).unwrap_or(0.0);
    let pos_z = read_f32_le(data, OFF_POS_Z).unwrap_or(0.0);

    let vel_x = read_f32_le(data, OFF_VEL_X).unwrap_or(0.0);
    let vel_y = read_f32_le(data, OFF_VEL_Y).unwrap_or(0.0);
    let vel_z = read_f32_le(data, OFF_VEL_Z).unwrap_or(0.0);

    let mut builder = NormalizedTelemetry::builder()
        .speed_ms(speed_ms)
        .rpm(rpm)
        .gear(gear)
        .throttle(throttle)
        .brake(brake)
        .steering_angle(steering)
        .extended("clutch".to_string(), TelemetryValue::Float(clutch))
        .extended("pos_x".to_string(), TelemetryValue::Float(pos_x))
        .extended("pos_y".to_string(), TelemetryValue::Float(pos_y))
        .extended("pos_z".to_string(), TelemetryValue::Float(pos_z))
        .extended("vel_x".to_string(), TelemetryValue::Float(vel_x))
        .extended("vel_y".to_string(), TelemetryValue::Float(vel_y))
        .extended("vel_z".to_string(), TelemetryValue::Float(vel_z));

    if max_rpm > 0.0 {
        let rpm_fraction = (rpm / max_rpm).clamp(0.0, 1.0);
        builder = builder.max_rpm(max_rpm).extended(
            "rpm_fraction".to_string(),
            TelemetryValue::Float(rpm_fraction),
        );
    }

    Ok(builder.build())
}

#[async_trait]
impl TelemetryAdapter for VRally4Adapter {
    fn game_id(&self) -> &str {
        "v_rally_4"
    }

    async fn start_monitoring(&self) -> Result<TelemetryReceiver> {
        let bind_port = self.bind_port;
        let update_rate = self.update_rate;
        let last_packet_ns = Arc::clone(&self.last_packet_ns);
        let (tx, rx) = mpsc::channel(100);

        tokio::spawn(async move {
            let bind_addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, bind_port));
            let socket = match TokioUdpSocket::bind(bind_addr).await {
                Ok(s) => s,
                Err(error) => {
                    warn!(
                        error = %error,
                        port = bind_port,
                        "V-Rally 4 UDP socket bind failed"
                    );
                    return;
                }
            };

            info!(port = bind_port, "V-Rally 4 UDP adapter bound");

            let mut frame_idx = 0u64;
            let mut buf = vec![0u8; MAX_PACKET_SIZE];
            let timeout = (update_rate * 4).max(Duration::from_millis(25));

            loop {
                let recv = tokio::time::timeout(timeout, socket.recv(&mut buf)).await;
                let len = match recv {
                    Ok(Ok(len)) => len,
                    Ok(Err(error)) => {
                        warn!(error = %error, "V-Rally 4 UDP receive error");
                        continue;
                    }
                    Err(_) => {
                        debug!("V-Rally 4 UDP receive timeout");
                        continue;
                    }
                };

                let data = &buf[..len];
                let normalized = match parse_packet(data) {
                    Ok(n) => n,
                    Err(error) => {
                        warn!(error = %error, "Failed to parse V-Rally 4 packet");
                        continue;
                    }
                };

                last_packet_ns.store(telemetry_now_ns(), Ordering::Relaxed);

                let frame = TelemetryFrame::new(normalized, telemetry_now_ns(), frame_idx, len);
                if tx.send(frame).await.is_err() {
                    break;
                }

                frame_idx = frame_idx.saturating_add(1);
            }
        });

        Ok(rx)
    }

    async fn stop_monitoring(&self) -> Result<()> {
        Ok(())
    }

    fn normalize(&self, raw: &[u8]) -> Result<NormalizedTelemetry> {
        parse_packet(raw)
    }

    fn expected_update_rate(&self) -> Duration {
        self.update_rate
    }

    async fn is_game_running(&self) -> Result<bool> {
        Ok(self.is_recent_packet())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_packet(size: usize) -> Vec<u8> {
        vec![0u8; size]
    }

    fn write_f32(buf: &mut [u8], offset: usize, value: f32) {
        buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn write_u32(buf: &mut [u8], offset: usize, value: u32) {
        buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    #[test]
    fn rejects_short_packet() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = VRally4Adapter::new();
        let result = adapter.normalize(&[0u8; MIN_PACKET_SIZE - 1]);
        assert!(result.is_err(), "expected error for short packet");
        Ok(())
    }

    #[test]
    fn zero_packet_returns_defaults() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = VRally4Adapter::new();
        let raw = make_packet(MIN_PACKET_SIZE);
        let t = adapter.normalize(&raw)?;
        assert_eq!(t.speed_ms, 0.0);
        assert_eq!(t.rpm, 0.0);
        assert_eq!(t.gear, -1, "zero gear should map to reverse");
        Ok(())
    }

    #[test]
    fn game_id_is_correct() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = VRally4Adapter::new();
        assert_eq!(adapter.game_id(), "v_rally_4");
        Ok(())
    }

    #[test]
    fn speed_and_rpm_extracted() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = VRally4Adapter::new();
        let mut raw = make_packet(MIN_PACKET_SIZE);
        write_f32(&mut raw, OFF_SPEED_MS, 30.0);
        write_f32(&mut raw, OFF_RPM, 6000.0);
        write_f32(&mut raw, OFF_MAX_RPM, 8000.0);
        let t = adapter.normalize(&raw)?;
        assert!((t.speed_ms - 30.0).abs() < 0.001, "speed_ms={}", t.speed_ms);
        assert!((t.rpm - 6000.0).abs() < 0.001, "rpm={}", t.rpm);
        assert!((t.max_rpm - 8000.0).abs() < 0.001, "max_rpm={}", t.max_rpm);
        Ok(())
    }

    #[test]
    fn forward_gear_decoded() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = VRally4Adapter::new();
        let mut raw = make_packet(MIN_PACKET_SIZE);
        write_u32(&mut raw, OFF_GEAR, 3);
        let t = adapter.normalize(&raw)?;
        assert_eq!(t.gear, 3);
        Ok(())
    }

    #[test]
    fn throttle_brake_clamped() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = VRally4Adapter::new();
        let mut raw = make_packet(MIN_PACKET_SIZE);
        write_f32(&mut raw, OFF_THROTTLE, 5.0);
        write_f32(&mut raw, OFF_BRAKE, -1.0);
        let t = adapter.normalize(&raw)?;
        assert_eq!(t.throttle, 1.0);
        assert_eq!(t.brake, 0.0);
        Ok(())
    }

    #[test]
    fn empty_input_returns_error() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = VRally4Adapter::new();
        assert!(adapter.normalize(&[]).is_err());
        Ok(())
    }
}

#[cfg(test)]
mod proptest_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(500))]

        #[test]
        fn parse_no_panic_on_arbitrary(
            data in proptest::collection::vec(any::<u8>(), 0..1024)
        ) {
            let adapter = VRally4Adapter::new();
            let _ = adapter.normalize(&data);
        }
    }
}

//! rFactor 1 engine UDP telemetry adapter.
//!
//! Supports games built on the ISI rFactor 1 engine that expose the community
//! UDP telemetry interface on port 6776:
//!
//!  - rFactor 1 (Image Space Incorporated)
//!  - GTR2: FIA GT Racing Game (SimBin Studios, 2006)
//!  - Race 07 / RACE: The WTCC Game (SimBin Studios)
//!  - Game Stock Car / Stock Car Extreme (Reiza Studios)
//!
//! The wire format is the `TelemInfoV2` struct broadcast over UDP, encoded
//! in little-endian byte order with natural C++ alignment (no pragma-pack).
//!
//! Key byte offsets within `TelemInfoV2`:
//!
//! | Field         | Type | Offset |
//! |---------------|------|--------|
//! | vel_x         | f64  |     24 |
//! | vel_y         | f64  |     32 |
//! | vel_z         | f64  |     40 |
//! | engine_rpm    | f64  |    312 |
//! | steer_input   | f64  |    992 |
//! | throttle      | f64  |   1000 |
//! | brake         | f64  |   1008 |
//! | gear          | i8   |   1024 |
//!
//! Speed is derived as `sqrt(vel_x² + vel_y² + vel_z²)`.
//! Deeper fields are read only when the received packet is long enough.

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

/// Default UDP port for the rFactor 1 telemetry interface.
const DEFAULT_RF1_PORT: u16 = 6776;

/// Minimum packet size required to read the world-velocity vector (speed).
/// Covers vel_z at offset 40 plus its 8-byte f64 value.
const RF1_MIN_PACKET_SIZE: usize = 48;

/// Byte offsets for the velocity vector (world-space, m/s).
const OFF_VEL_X: usize = 24; // f64
const OFF_VEL_Y: usize = 32; // f64
const OFF_VEL_Z: usize = 40; // f64

/// Engine RPM field offset (f64, rev/min).
const OFF_ENGINE_RPM: usize = 312; // f64

/// Steering, throttle, brake, and gear field offsets (after per-wheel data).
const OFF_STEER_INPUT: usize = 992; // f64, −1.0 (left) … +1.0 (right)
const OFF_THROTTLE: usize = 1000; // f64, 0.0 … 1.0
const OFF_BRAKE: usize = 1008; // f64, 0.0 … 1.0
const OFF_GEAR: usize = 1024; // i8, −1 = reverse, 0 = neutral, 1+ = forward

/// Receive buffer headroom above the largest expected packet.
const MAX_PACKET_SIZE: usize = 2048;

// ---------------------------------------------------------------------------
// Variant enum
// ---------------------------------------------------------------------------

/// Which rFactor 1 engine game this adapter targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RFactor1Variant {
    /// rFactor 1 (Image Space Incorporated)
    RFactor1,
    /// GTR2: FIA GT Racing Game (SimBin Studios, 2006)
    Gtr2,
    /// Race 07 / RACE: The WTCC Game (SimBin Studios)
    Race07,
    /// Game Stock Car / Stock Car Extreme (Reiza Studios)
    GameStockCar,
}

impl RFactor1Variant {
    fn game_id(self) -> &'static str {
        match self {
            Self::RFactor1 => "rfactor1",
            Self::Gtr2 => "gtr2",
            Self::Race07 => "race_07",
            Self::GameStockCar => "gsc",
        }
    }
}

// ---------------------------------------------------------------------------
// Packet parser
// ---------------------------------------------------------------------------

/// Parse a raw rFactor 1 UDP telemetry packet into [`NormalizedTelemetry`].
///
/// Returns an error if the packet is shorter than [`RF1_MIN_PACKET_SIZE`].
/// Deeper fields (RPM, gear, inputs) are silently defaulted to zero when the
/// packet does not extend far enough.
pub fn parse_rfactor1_packet(data: &[u8]) -> Result<NormalizedTelemetry> {
    if data.len() < RF1_MIN_PACKET_SIZE {
        return Err(anyhow!(
            "rFactor 1 packet too short: expected at least {RF1_MIN_PACKET_SIZE}, got {}",
            data.len()
        ));
    }

    // World-space velocity → scalar speed.
    let vel_x = read_f64_le(data, OFF_VEL_X).unwrap_or(0.0);
    let vel_y = read_f64_le(data, OFF_VEL_Y).unwrap_or(0.0);
    let vel_z = read_f64_le(data, OFF_VEL_Z).unwrap_or(0.0);
    let speed_ms = (vel_x * vel_x + vel_y * vel_y + vel_z * vel_z).sqrt() as f32;

    // Optional fields — only decoded when the packet is long enough.
    let rpm = read_f64_opt(data, OFF_ENGINE_RPM)
        .map(|v| (v as f32).max(0.0))
        .unwrap_or(0.0);

    let steer = read_f64_opt(data, OFF_STEER_INPUT)
        .map(|v| (v as f32).clamp(-1.0, 1.0))
        .unwrap_or(0.0);

    let throttle = read_f64_opt(data, OFF_THROTTLE)
        .map(|v| (v as f32).clamp(0.0, 1.0))
        .unwrap_or(0.0);

    let brake = read_f64_opt(data, OFF_BRAKE)
        .map(|v| (v as f32).clamp(0.0, 1.0))
        .unwrap_or(0.0);

    let gear: i8 = if data.len() > OFF_GEAR {
        data[OFF_GEAR] as i8
    } else {
        0
    };

    // Approximate FFB from steering weighted by speed.
    let ffb_scalar = (steer * (speed_ms / 30.0).min(1.0)).clamp(-1.0, 1.0);

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

// ---------------------------------------------------------------------------
// Adapter
// ---------------------------------------------------------------------------

/// rFactor 1 engine UDP telemetry adapter (rFactor 1, GTR2, Race 07, GSC).
pub struct RFactor1Adapter {
    variant: RFactor1Variant,
    bind_port: u16,
    update_rate: Duration,
}

impl RFactor1Adapter {
    /// Create a new adapter targeting rFactor 1.
    pub fn new() -> Self {
        Self::with_variant(RFactor1Variant::RFactor1)
    }

    /// Create an adapter for the given rFactor 1 engine variant.
    pub fn with_variant(variant: RFactor1Variant) -> Self {
        Self {
            variant,
            bind_port: DEFAULT_RF1_PORT,
            update_rate: Duration::from_millis(16), // ~60 Hz
        }
    }

    /// Override the UDP bind port.
    pub fn with_port(mut self, port: u16) -> Self {
        self.bind_port = port;
        self
    }
}

impl Default for RFactor1Adapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TelemetryAdapter for RFactor1Adapter {
    fn game_id(&self) -> &str {
        self.variant.game_id()
    }

    async fn start_monitoring(&self) -> Result<TelemetryReceiver> {
        let (tx, rx) = mpsc::channel(100);
        let bind_port = self.bind_port;
        let update_rate = self.update_rate;
        let game_id = self.variant.game_id();

        tokio::spawn(async move {
            let bind_addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, bind_port));
            let socket = match TokioUdpSocket::bind(bind_addr).await {
                Ok(s) => s,
                Err(e) => {
                    warn!("Failed to bind {game_id} UDP socket on port {bind_port}: {e}");
                    return;
                }
            };
            info!("{game_id} adapter listening on UDP port {bind_port}");
            let mut buf = [0u8; MAX_PACKET_SIZE];
            let mut frame_seq = 0u64;

            loop {
                match tokio::time::timeout(update_rate * 10, socket.recv(&mut buf)).await {
                    Ok(Ok(len)) => match parse_rfactor1_packet(&buf[..len]) {
                        Ok(normalized) => {
                            let frame =
                                TelemetryFrame::new(normalized, telemetry_now_ns(), frame_seq, len);
                            if tx.send(frame).await.is_err() {
                                debug!("Receiver dropped, stopping {game_id} UDP monitoring");
                                break;
                            }
                            frame_seq = frame_seq.saturating_add(1);
                        }
                        Err(e) => debug!("Failed to parse {game_id} packet: {e}"),
                    },
                    Ok(Err(e)) => warn!("{game_id} UDP receive error: {e}"),
                    Err(_) => debug!("No {game_id} telemetry data received (timeout)"),
                }
            }
            info!("Stopped {game_id} telemetry monitoring");
        });

        Ok(rx)
    }

    async fn stop_monitoring(&self) -> Result<()> {
        Ok(())
    }

    fn normalize(&self, raw: &[u8]) -> Result<NormalizedTelemetry> {
        parse_rfactor1_packet(raw)
    }

    fn expected_update_rate(&self) -> Duration {
        self.update_rate
    }

    async fn is_game_running(&self) -> Result<bool> {
        Ok(false)
    }
}

// ---------------------------------------------------------------------------
// Byte-level helpers
// ---------------------------------------------------------------------------

fn read_f64_le(data: &[u8], offset: usize) -> Option<f64> {
    data.get(offset..offset + 8)
        .and_then(|b| b.try_into().ok())
        .map(f64::from_le_bytes)
}

/// Read an f64 at `offset` only when the slice is long enough; returns `None` otherwise.
fn read_f64_opt(data: &[u8], offset: usize) -> Option<f64> {
    if data.len() >= offset + 8 {
        read_f64_le(data, offset)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// Build a full-size packet with the given field values.
    fn make_rf1_packet(
        vel_x: f64,
        vel_y: f64,
        vel_z: f64,
        rpm: f64,
        steer: f64,
        throttle: f64,
        brake: f64,
        gear: i8,
    ) -> Vec<u8> {
        // Allocate enough bytes to cover every field.
        let mut data = vec![0u8; OFF_GEAR + 1];
        data[OFF_VEL_X..OFF_VEL_X + 8].copy_from_slice(&vel_x.to_le_bytes());
        data[OFF_VEL_Y..OFF_VEL_Y + 8].copy_from_slice(&vel_y.to_le_bytes());
        data[OFF_VEL_Z..OFF_VEL_Z + 8].copy_from_slice(&vel_z.to_le_bytes());
        data[OFF_ENGINE_RPM..OFF_ENGINE_RPM + 8].copy_from_slice(&rpm.to_le_bytes());
        data[OFF_STEER_INPUT..OFF_STEER_INPUT + 8].copy_from_slice(&steer.to_le_bytes());
        data[OFF_THROTTLE..OFF_THROTTLE + 8].copy_from_slice(&throttle.to_le_bytes());
        data[OFF_BRAKE..OFF_BRAKE + 8].copy_from_slice(&brake.to_le_bytes());
        data[OFF_GEAR] = gear as u8;
        data
    }

    #[test]
    fn test_parse_valid_packet() -> TestResult {
        let data = make_rf1_packet(0.0, 0.0, 30.0, 6000.0, 0.2, 0.8, 0.0, 3);
        let result = parse_rfactor1_packet(&data)?;
        assert!((result.speed_ms - 30.0).abs() < 0.001);
        assert!((result.rpm - 6000.0).abs() < 0.1);
        assert_eq!(result.gear, 3);
        assert!((result.throttle - 0.8).abs() < 0.001);
        assert!(result.brake.abs() < 0.001);
        Ok(())
    }

    #[test]
    fn test_speed_from_3d_velocity() -> TestResult {
        // 3-4-0 triangle → speed = 5.0 m/s
        let data = make_rf1_packet(3.0, 4.0, 0.0, 5000.0, 0.0, 0.5, 0.0, 2);
        let result = parse_rfactor1_packet(&data)?;
        assert!(
            (result.speed_ms - 5.0).abs() < 0.001,
            "expected 5.0 m/s, got {}",
            result.speed_ms
        );
        Ok(())
    }

    #[test]
    fn test_short_packet_rejected() {
        let data = vec![0u8; 20];
        assert!(parse_rfactor1_packet(&data).is_err());
    }

    #[test]
    fn test_empty_packet_rejected() {
        assert!(parse_rfactor1_packet(&[]).is_err());
    }

    #[test]
    fn test_minimum_packet_returns_speed_only() -> TestResult {
        let mut data = vec![0u8; RF1_MIN_PACKET_SIZE];
        data[OFF_VEL_Z..OFF_VEL_Z + 8].copy_from_slice(&10.0f64.to_le_bytes());
        let result = parse_rfactor1_packet(&data)?;
        assert!((result.speed_ms - 10.0).abs() < 0.001);
        assert_eq!(result.gear, 0);
        assert!(result.rpm.abs() < 0.001);
        Ok(())
    }

    #[test]
    fn test_reverse_gear() -> TestResult {
        let data = make_rf1_packet(0.0, 0.0, -5.0, 2000.0, 0.0, 0.0, 0.5, -1);
        let result = parse_rfactor1_packet(&data)?;
        assert_eq!(result.gear, -1);
        Ok(())
    }

    #[test]
    fn test_neutral_gear() -> TestResult {
        let data = make_rf1_packet(0.0, 0.0, 0.0, 800.0, 0.0, 0.0, 0.0, 0);
        let result = parse_rfactor1_packet(&data)?;
        assert_eq!(result.gear, 0);
        Ok(())
    }

    #[test]
    fn test_throttle_clamped() -> TestResult {
        let data = make_rf1_packet(0.0, 0.0, 20.0, 5000.0, 0.0, 1.5, 0.0, 3);
        let result = parse_rfactor1_packet(&data)?;
        assert!(result.throttle <= 1.0, "throttle must not exceed 1.0");
        assert!(result.throttle >= 0.0, "throttle must not be negative");
        Ok(())
    }

    #[test]
    fn test_brake_clamped() -> TestResult {
        let data = make_rf1_packet(0.0, 0.0, 0.0, 2000.0, 0.0, 0.0, 1.5, 0);
        let result = parse_rfactor1_packet(&data)?;
        assert!(result.brake <= 1.0, "brake must not exceed 1.0");
        assert!(result.brake >= 0.0, "brake must not be negative");
        Ok(())
    }

    #[test]
    fn test_ffb_scalar_in_range() -> TestResult {
        let data = make_rf1_packet(0.0, 10.0, 50.0, 7000.0, 0.9, 1.0, 0.0, 5);
        let result = parse_rfactor1_packet(&data)?;
        assert!(
            result.ffb_scalar >= -1.0 && result.ffb_scalar <= 1.0,
            "ffb_scalar out of range: {}",
            result.ffb_scalar
        );
        Ok(())
    }

    #[test]
    fn test_speed_is_nonnegative() -> TestResult {
        // Even with negative velocity components the magnitude must be >= 0.
        let data = make_rf1_packet(-5.0, -3.0, -4.0, 4000.0, 0.0, 0.3, 0.0, 2);
        let result = parse_rfactor1_packet(&data)?;
        assert!(
            result.speed_ms >= 0.0,
            "speed must be non-negative, got {}",
            result.speed_ms
        );
        Ok(())
    }

    #[test]
    fn test_rpm_is_nonnegative() -> TestResult {
        let data = make_rf1_packet(0.0, 0.0, 20.0, 3500.0, 0.0, 0.4, 0.0, 2);
        let result = parse_rfactor1_packet(&data)?;
        assert!(
            result.rpm >= 0.0,
            "rpm must be non-negative, got {}",
            result.rpm
        );
        Ok(())
    }

    #[test]
    fn test_adapter_game_id_rfactor1() {
        let adapter = RFactor1Adapter::with_variant(RFactor1Variant::RFactor1);
        assert_eq!(adapter.game_id(), "rfactor1");
    }

    #[test]
    fn test_adapter_game_id_gtr2() {
        let adapter = RFactor1Adapter::with_variant(RFactor1Variant::Gtr2);
        assert_eq!(adapter.game_id(), "gtr2");
    }

    #[test]
    fn test_adapter_game_id_race07() {
        let adapter = RFactor1Adapter::with_variant(RFactor1Variant::Race07);
        assert_eq!(adapter.game_id(), "race_07");
    }

    #[test]
    fn test_adapter_game_id_gsc() {
        let adapter = RFactor1Adapter::with_variant(RFactor1Variant::GameStockCar);
        assert_eq!(adapter.game_id(), "gsc");
    }

    #[test]
    fn test_update_rate() {
        let adapter = RFactor1Adapter::new();
        assert_eq!(adapter.expected_update_rate(), Duration::from_millis(16));
    }

    #[test]
    fn test_with_port_builder() {
        let adapter = RFactor1Adapter::new().with_port(6777);
        assert_eq!(adapter.bind_port, 6777);
    }

    #[test]
    fn test_normalize_delegates_to_parse() -> TestResult {
        let adapter = RFactor1Adapter::new();
        let data = make_rf1_packet(0.0, 0.0, 25.0, 5500.0, 0.1, 0.6, 0.0, 4);
        let result = adapter.normalize(&data)?;
        assert!((result.speed_ms - 25.0).abs() < 0.001);
        assert!((result.rpm - 5500.0).abs() < 0.1);
        Ok(())
    }

    #[test]
    fn test_gear_range_forward() -> TestResult {
        for g in 1i8..=7 {
            let data = make_rf1_packet(0.0, 0.0, 20.0, 4000.0, 0.0, 0.5, 0.0, g);
            let result = parse_rfactor1_packet(&data)?;
            assert!(
                result.gear >= -1 && result.gear <= 7,
                "gear {} out of expected range",
                result.gear
            );
        }
        Ok(())
    }

    #[test]
    fn test_steering_in_range() -> TestResult {
        let data = make_rf1_packet(0.0, 0.0, 30.0, 6000.0, -0.7, 0.5, 0.0, 3);
        let result = parse_rfactor1_packet(&data)?;
        assert!(
            result.steering_angle >= -1.0 && result.steering_angle <= 1.0,
            "steering_angle {} out of range",
            result.steering_angle
        );
        Ok(())
    }
}

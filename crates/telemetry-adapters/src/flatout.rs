//! FlatOut UC / FlatOut 4 telemetry adapter (bridge UDP on port 7776).
//!
//! FlatOut games do not ship native UDP telemetry; a community bridge reads the
//! game memory and forwards data on a local UDP socket.  Each datagram starts
//! with the 4-byte magic `FOTC` (0x46 0x4F 0x54 0x43) followed by:
//!
//! ```text
//! offset  0: [u8; 4]  magic  "FOTC"
//! offset  4: u32      packet sequence number
//! offset  8: f32      speed_ms        (m/s)
//! offset 12: f32      rpm             (rev/min)
//! offset 16: u8       gear            (0 = neutral, 1+ = forward)
//! offset 17: [u8; 3]  padding
//! offset 20: f32      lateral_g       (signed)
//! offset 24: f32      longitudinal_g  (signed)
//! offset 28: f32      throttle        (0.0 – 1.0)
//! offset 32: f32      brake           (0.0 – 1.0)
//! ```
//!
//! Minimum packet size: 36 bytes.  Update rate: ~60 Hz.

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

const DEFAULT_FLATOUT_PORT: u16 = 7776;
const FLATOUT_MIN_PACKET_SIZE: usize = 36;
const MAX_PACKET_SIZE: usize = 512;

/// Expected 4-byte magic at the start of every FlatOut telemetry packet.
const FLATOUT_MAGIC: [u8; 4] = [0x46, 0x4F, 0x54, 0x43]; // "FOTC"

const OFF_MAGIC: usize = 0;
const OFF_SPEED: usize = 8;
const OFF_RPM: usize = 12;
const OFF_GEAR: usize = 16;
const OFF_LATERAL_G: usize = 20;
const OFF_LONGITUDINAL_G: usize = 24;
const OFF_THROTTLE: usize = 28;
const OFF_BRAKE: usize = 32;

/// Parse a raw FlatOut bridge UDP packet into [`NormalizedTelemetry`].
pub fn parse_flatout_packet(data: &[u8]) -> Result<NormalizedTelemetry> {
    if data.len() < FLATOUT_MIN_PACKET_SIZE {
        return Err(anyhow!(
            "FlatOut packet too short: expected at least {FLATOUT_MIN_PACKET_SIZE}, got {}",
            data.len()
        ));
    }

    if data[OFF_MAGIC..OFF_MAGIC + 4] != FLATOUT_MAGIC {
        return Err(anyhow!(
            "Invalid FlatOut magic: {:?}",
            &data[OFF_MAGIC..OFF_MAGIC + 4]
        ));
    }

    let speed_ms = read_f32_le(data, OFF_SPEED).unwrap_or(0.0).max(0.0);
    let rpm = read_f32_le(data, OFF_RPM).unwrap_or(0.0).max(0.0);
    let gear = data[OFF_GEAR].min(12) as i8;
    let throttle = read_f32_le(data, OFF_THROTTLE)
        .unwrap_or(0.0)
        .clamp(0.0, 1.0);
    let brake = read_f32_le(data, OFF_BRAKE).unwrap_or(0.0).clamp(0.0, 1.0);
    let lateral_g = read_f32_le(data, OFF_LATERAL_G).unwrap_or(0.0);
    let longitudinal_g = read_f32_le(data, OFF_LONGITUDINAL_G).unwrap_or(0.0);

    let combined_g = lateral_g.hypot(longitudinal_g);
    let ffb_scalar = (combined_g / 3.0).clamp(-1.0, 1.0);

    Ok(NormalizedTelemetry::builder()
        .speed_ms(speed_ms)
        .rpm(rpm)
        .gear(gear)
        .throttle(throttle)
        .brake(brake)
        .lateral_g(lateral_g)
        .longitudinal_g(longitudinal_g)
        .ffb_scalar(ffb_scalar)
        .build())
}

/// FlatOut UC / FlatOut 4 UDP bridge telemetry adapter.
pub struct FlatOutAdapter {
    bind_port: u16,
    update_rate: Duration,
}

impl Default for FlatOutAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl FlatOutAdapter {
    pub fn new() -> Self {
        Self {
            bind_port: DEFAULT_FLATOUT_PORT,
            update_rate: Duration::from_millis(16), // ~60 Hz
        }
    }

    pub fn with_port(mut self, port: u16) -> Self {
        self.bind_port = port;
        self
    }
}

#[async_trait]
impl TelemetryAdapter for FlatOutAdapter {
    fn game_id(&self) -> &str {
        "flatout"
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
                    warn!("Failed to bind FlatOut UDP socket on port {bind_port}: {e}");
                    return;
                }
            };
            info!("FlatOut adapter listening on UDP port {bind_port}");
            let mut buf = [0u8; MAX_PACKET_SIZE];
            let mut frame_idx = 0u64;

            loop {
                match tokio::time::timeout(update_rate * 10, socket.recv(&mut buf)).await {
                    Ok(Ok(len)) => match parse_flatout_packet(&buf[..len]) {
                        Ok(normalized) => {
                            let frame =
                                TelemetryFrame::new(normalized, telemetry_now_ns(), frame_idx, len);
                            if tx.send(frame).await.is_err() {
                                debug!("Receiver dropped, stopping FlatOut monitoring");
                                break;
                            }
                            frame_idx = frame_idx.saturating_add(1);
                        }
                        Err(e) => debug!("Failed to parse FlatOut packet: {e}"),
                    },
                    Ok(Err(e)) => warn!("FlatOut UDP receive error: {e}"),
                    Err(_) => debug!("No FlatOut telemetry data received (timeout)"),
                }
            }
            info!("Stopped FlatOut telemetry monitoring");
        });

        Ok(rx)
    }

    async fn stop_monitoring(&self) -> Result<()> {
        Ok(())
    }

    fn normalize(&self, raw: &[u8]) -> Result<NormalizedTelemetry> {
        parse_flatout_packet(raw)
    }

    fn expected_update_rate(&self) -> Duration {
        self.update_rate
    }

    async fn is_game_running(&self) -> Result<bool> {
        Ok(false)
    }
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

    fn make_flatout_packet(
        speed: f32,
        rpm: f32,
        gear: u8,
        throttle: f32,
        brake: f32,
        lateral_g: f32,
        longitudinal_g: f32,
    ) -> Vec<u8> {
        let mut data = vec![0u8; FLATOUT_MIN_PACKET_SIZE];
        data[OFF_MAGIC..OFF_MAGIC + 4].copy_from_slice(&FLATOUT_MAGIC);
        data[OFF_SPEED..OFF_SPEED + 4].copy_from_slice(&speed.to_le_bytes());
        data[OFF_RPM..OFF_RPM + 4].copy_from_slice(&rpm.to_le_bytes());
        data[OFF_GEAR] = gear;
        data[OFF_THROTTLE..OFF_THROTTLE + 4].copy_from_slice(&throttle.to_le_bytes());
        data[OFF_BRAKE..OFF_BRAKE + 4].copy_from_slice(&brake.to_le_bytes());
        data[OFF_LATERAL_G..OFF_LATERAL_G + 4].copy_from_slice(&lateral_g.to_le_bytes());
        data[OFF_LONGITUDINAL_G..OFF_LONGITUDINAL_G + 4]
            .copy_from_slice(&longitudinal_g.to_le_bytes());
        data
    }

    #[test]
    fn test_parse_valid_packet() -> TestResult {
        let data = make_flatout_packet(25.0, 4500.0, 3, 0.8, 0.0, 0.4, 0.2);
        let result = parse_flatout_packet(&data)?;
        assert!((result.speed_ms - 25.0).abs() < 0.001);
        assert!((result.rpm - 4500.0).abs() < 0.1);
        assert_eq!(result.gear, 3);
        assert!((result.lateral_g - 0.4).abs() < 0.001);
        assert!((result.longitudinal_g - 0.2).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn test_magic_mismatch_rejected() {
        let mut data = make_flatout_packet(10.0, 2000.0, 2, 0.5, 0.1, 0.0, 0.0);
        data[0] = 0xFF;
        assert!(parse_flatout_packet(&data).is_err());
    }

    #[test]
    fn test_short_packet_rejected() {
        let data = vec![0u8; 10];
        assert!(parse_flatout_packet(&data).is_err());
    }

    #[test]
    fn test_ffb_scalar_range() -> TestResult {
        let data = make_flatout_packet(60.0, 7000.0, 5, 1.0, 0.5, 2.0, 1.5);
        let result = parse_flatout_packet(&data)?;
        assert!(
            result.ffb_scalar >= -1.0 && result.ffb_scalar <= 1.0,
            "ffb_scalar out of range: {}",
            result.ffb_scalar
        );
        Ok(())
    }

    #[test]
    fn test_adapter_game_id() {
        let adapter = FlatOutAdapter::new();
        assert_eq!(adapter.game_id(), "flatout");
    }

    #[test]
    fn test_update_rate() {
        let adapter = FlatOutAdapter::new();
        assert_eq!(adapter.expected_update_rate(), Duration::from_millis(16));
    }

    #[test]
    fn test_empty_packet_rejected() {
        assert!(
            parse_flatout_packet(&[]).is_err(),
            "empty packet must return an error"
        );
    }

    #[test]
    fn test_speed_is_nonnegative() -> TestResult {
        let data = make_flatout_packet(45.0, 5500.0, 4, 0.7, 0.0, 0.3, 0.1);
        let result = parse_flatout_packet(&data)?;
        assert!(
            result.speed_ms >= 0.0,
            "speed_ms must be non-negative, got {}",
            result.speed_ms
        );
        Ok(())
    }

    #[test]
    fn test_gear_range() -> TestResult {
        for g in 0u8..=8 {
            let data = make_flatout_packet(20.0, 3000.0, g, 0.5, 0.0, 0.1, 0.0);
            let result = parse_flatout_packet(&data)?;
            assert!(
                result.gear >= 0 && result.gear <= 8,
                "gear {} out of expected range 0..=8",
                result.gear
            );
        }
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
            let adapter = FlatOutAdapter::new();
            let _ = adapter.normalize(&data);
        }
    }
}

//! DiRT 3 telemetry adapter for Codemasters Mode 1 UDP format.
//!
//! Enable UDP telemetry in-game: Options → Accessibility → UDP Telemetry, port 20777.
//!
//! The packet layout is the fixed-layout Codemasters Mode 1 legacy binary stream
//! (264 bytes, little-endian `f32` at known byte offsets), shared with DiRT Rally 2.0,
//! GRID Autosport, GRID 2019, and the broader Codemasters series.  Parsing is
//! delegated to [`crate::codemasters_shared`].

use crate::codemasters_shared;
use crate::{
    NormalizedTelemetry, TelemetryAdapter, TelemetryFrame, TelemetryReceiver, telemetry_now_ns,
};
use anyhow::Result;
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

const DEFAULT_PORT: u16 = 20777;
const MAX_PACKET_SIZE: usize = 2048;
const DEFAULT_HEARTBEAT_TIMEOUT_MS: u64 = 1_500;

const ENV_PORT: &str = "OPENRACING_DIRT3_UDP_PORT";
const ENV_HEARTBEAT_TIMEOUT_MS: &str = "OPENRACING_DIRT3_HEARTBEAT_TIMEOUT_MS";

const GAME_LABEL: &str = "DiRT 3";

/// DiRT 3 adapter for Codemasters Mode 1 UDP telemetry.
#[derive(Clone)]
pub struct Dirt3Adapter {
    bind_port: u16,
    update_rate: Duration,
    heartbeat_timeout: Duration,
    last_packet_ns: Arc<AtomicU64>,
}

impl Default for Dirt3Adapter {
    fn default() -> Self {
        Self::new()
    }
}

impl Dirt3Adapter {
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

fn parse_packet(data: &[u8]) -> Result<NormalizedTelemetry> {
    codemasters_shared::parse_codemasters_mode1_common(data, GAME_LABEL)
}

#[async_trait]
impl TelemetryAdapter for Dirt3Adapter {
    fn game_id(&self) -> &str {
        "dirt3"
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
                        "DiRT 3 UDP socket bind failed"
                    );
                    return;
                }
            };

            info!(port = bind_port, "DiRT 3 UDP adapter bound");

            let mut frame_seq = 0u64;
            let mut buf = vec![0u8; MAX_PACKET_SIZE];
            let timeout = (update_rate * 4).max(Duration::from_millis(25));

            loop {
                let recv = tokio::time::timeout(timeout, socket.recv(&mut buf)).await;
                let len = match recv {
                    Ok(Ok(len)) => len,
                    Ok(Err(error)) => {
                        warn!(error = %error, "DiRT 3 UDP receive error");
                        continue;
                    }
                    Err(_) => {
                        debug!("DiRT 3 UDP receive timeout");
                        continue;
                    }
                };

                let data = &buf[..len];
                let normalized = match parse_packet(data) {
                    Ok(n) => n,
                    Err(error) => {
                        warn!(error = %error, "Failed to parse DiRT 3 packet");
                        continue;
                    }
                };

                last_packet_ns.store(telemetry_now_ns(), Ordering::Relaxed);

                let frame = TelemetryFrame::new(normalized, telemetry_now_ns(), frame_seq, len);
                if tx.send(frame).await.is_err() {
                    break;
                }

                frame_seq = frame_seq.saturating_add(1);
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
    use crate::codemasters_shared::MIN_PACKET_SIZE;

    fn make_packet(size: usize) -> Vec<u8> {
        vec![0u8; size]
    }

    #[test]
    fn rejects_short_packet() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = Dirt3Adapter::new();
        let result = adapter.normalize(&[0u8; MIN_PACKET_SIZE - 1]);
        assert!(result.is_err(), "expected error for short packet");
        Ok(())
    }

    #[test]
    fn zero_packet_returns_zero_speed_and_rpm() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = Dirt3Adapter::new();
        let raw = make_packet(MIN_PACKET_SIZE);
        let t = adapter.normalize(&raw)?;
        assert_eq!(t.speed_ms, 0.0);
        assert_eq!(t.rpm, 0.0);
        Ok(())
    }

    #[test]
    fn gear_zero_maps_to_reverse() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = Dirt3Adapter::new();
        let raw = make_packet(MIN_PACKET_SIZE);
        let t = adapter.normalize(&raw)?;
        assert_eq!(t.gear, -1);
        Ok(())
    }

    #[test]
    fn game_id_is_dirt3() {
        assert_eq!(Dirt3Adapter::new().game_id(), "dirt3");
    }

    #[test]
    fn known_values_parsed_correctly() -> Result<(), Box<dyn std::error::Error>> {
        use crate::codemasters_shared::{
            OFF_BRAKE, OFF_GEAR, OFF_MAX_RPM, OFF_RPM, OFF_STEER, OFF_THROTTLE,
            OFF_WHEEL_SPEED_FL, OFF_WHEEL_SPEED_FR, OFF_WHEEL_SPEED_RL, OFF_WHEEL_SPEED_RR,
        };

        let adapter = Dirt3Adapter::new();
        let mut buf = make_packet(MIN_PACKET_SIZE);

        // Set wheel speeds to 25.0 m/s each → average = 25.0
        buf[OFF_WHEEL_SPEED_FL..OFF_WHEEL_SPEED_FL + 4]
            .copy_from_slice(&25.0_f32.to_le_bytes());
        buf[OFF_WHEEL_SPEED_FR..OFF_WHEEL_SPEED_FR + 4]
            .copy_from_slice(&25.0_f32.to_le_bytes());
        buf[OFF_WHEEL_SPEED_RL..OFF_WHEEL_SPEED_RL + 4]
            .copy_from_slice(&25.0_f32.to_le_bytes());
        buf[OFF_WHEEL_SPEED_RR..OFF_WHEEL_SPEED_RR + 4]
            .copy_from_slice(&25.0_f32.to_le_bytes());

        buf[OFF_RPM..OFF_RPM + 4].copy_from_slice(&5000.0_f32.to_le_bytes());
        buf[OFF_MAX_RPM..OFF_MAX_RPM + 4].copy_from_slice(&8000.0_f32.to_le_bytes());
        buf[OFF_GEAR..OFF_GEAR + 4].copy_from_slice(&3.0_f32.to_le_bytes());
        buf[OFF_THROTTLE..OFF_THROTTLE + 4].copy_from_slice(&0.75_f32.to_le_bytes());
        buf[OFF_BRAKE..OFF_BRAKE + 4].copy_from_slice(&0.25_f32.to_le_bytes());
        buf[OFF_STEER..OFF_STEER + 4].copy_from_slice(&0.5_f32.to_le_bytes());

        let t = adapter.normalize(&buf)?;
        assert_eq!(t.speed_ms, 25.0);
        assert_eq!(t.rpm, 5000.0);
        assert_eq!(t.gear, 3);
        assert_eq!(t.throttle, 0.75);
        assert_eq!(t.brake, 0.25);
        assert_eq!(t.steering_angle, 0.5);
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
            let adapter = Dirt3Adapter::new();
            let _ = adapter.normalize(&data);
        }
    }
}

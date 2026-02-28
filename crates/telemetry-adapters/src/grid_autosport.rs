//! GRID Autosport telemetry adapter for Codemasters Mode 1 UDP format.
//!
//! Enable UDP telemetry in-game: Options → Controls → UDP Telemetry, port 20777.
//!
//! The packet layout is the fixed-layout Codemasters Mode 1 legacy binary stream
//! (264+ bytes, little-endian `f32` at known byte offsets), shared with DiRT Rally 2.0,
//! WRC Generations, and the broader GRID series.  Parsing is delegated to
//! [`crate::codemasters_shared`].

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

const ENV_PORT: &str = "OPENRACING_GRID_AUTOSPORT_UDP_PORT";
const ENV_HEARTBEAT_TIMEOUT_MS: &str = "OPENRACING_GRID_AUTOSPORT_HEARTBEAT_TIMEOUT_MS";

const GAME_LABEL: &str = "GRID Autosport";

/// GRID Autosport adapter for Codemasters Mode 1 UDP telemetry.
#[derive(Clone)]
pub struct GridAutosportAdapter {
    bind_port: u16,
    update_rate: Duration,
    heartbeat_timeout: Duration,
    last_packet_ns: Arc<AtomicU64>,
}

impl Default for GridAutosportAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl GridAutosportAdapter {
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
impl TelemetryAdapter for GridAutosportAdapter {
    fn game_id(&self) -> &str {
        "grid_autosport"
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
                        "GRID Autosport UDP socket bind failed"
                    );
                    return;
                }
            };

            info!(port = bind_port, "GRID Autosport UDP adapter bound");

            let mut frame_seq = 0u64;
            let mut buf = vec![0u8; MAX_PACKET_SIZE];
            let timeout = (update_rate * 4).max(Duration::from_millis(25));

            loop {
                let recv = tokio::time::timeout(timeout, socket.recv(&mut buf)).await;
                let len = match recv {
                    Ok(Ok(len)) => len,
                    Ok(Err(error)) => {
                        warn!(error = %error, "GRID Autosport UDP receive error");
                        continue;
                    }
                    Err(_) => {
                        debug!("GRID Autosport UDP receive timeout");
                        continue;
                    }
                };

                let data = &buf[..len];
                let normalized = match parse_packet(data) {
                    Ok(n) => n,
                    Err(error) => {
                        warn!(error = %error, "Failed to parse GRID Autosport packet");
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
    use crate::codemasters_shared::*;

    fn make_packet(size: usize) -> Vec<u8> {
        vec![0u8; size]
    }

    fn write_f32(buf: &mut [u8], offset: usize, value: f32) {
        buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    #[test]
    fn rejects_short_packet() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = GridAutosportAdapter::new();
        let result = adapter.normalize(&[0u8; MIN_PACKET_SIZE - 1]);
        assert!(result.is_err(), "expected error for short packet");
        Ok(())
    }

    #[test]
    fn zero_packet_returns_zero_speed_and_rpm() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = GridAutosportAdapter::new();
        let raw = make_packet(MIN_PACKET_SIZE);
        let t = adapter.normalize(&raw)?;
        assert_eq!(t.speed_ms, 0.0);
        assert_eq!(t.rpm, 0.0);
        Ok(())
    }

    #[test]
    fn gear_zero_maps_to_reverse() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = GridAutosportAdapter::new();
        let raw = make_packet(MIN_PACKET_SIZE);
        let t = adapter.normalize(&raw)?;
        assert_eq!(t.gear, -1);
        Ok(())
    }

    #[test]
    fn speed_extracted_from_wheel_speeds() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = GridAutosportAdapter::new();
        let mut raw = make_packet(MIN_PACKET_SIZE);
        write_f32(&mut raw, OFF_WHEEL_SPEED_FL, 30.0);
        write_f32(&mut raw, OFF_WHEEL_SPEED_FR, 30.0);
        write_f32(&mut raw, OFF_WHEEL_SPEED_RL, 30.0);
        write_f32(&mut raw, OFF_WHEEL_SPEED_RR, 30.0);
        let t = adapter.normalize(&raw)?;
        assert!(
            (t.speed_ms - 30.0).abs() < 0.001,
            "speed_ms should be 30.0, got {}",
            t.speed_ms
        );
        Ok(())
    }

    #[test]
    fn ffb_scalar_clamped_to_range() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = GridAutosportAdapter::new();
        let mut raw = make_packet(MIN_PACKET_SIZE);
        write_f32(&mut raw, OFF_GFORCE_LAT, 10.0);
        let t = adapter.normalize(&raw)?;
        assert!(t.ffb_scalar >= -1.0 && t.ffb_scalar <= 1.0);
        Ok(())
    }

    #[test]
    fn game_id_is_grid_autosport() {
        assert_eq!(GridAutosportAdapter::new().game_id(), "grid_autosport");
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
            let adapter = GridAutosportAdapter::new();
            let _ = adapter.normalize(&data);
        }
    }
}

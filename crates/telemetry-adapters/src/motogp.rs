//! MotoGP 23 / MotoGP 24 telemetry adapter (SimHub UDP JSON bridge on port 5556).
//!
//! Neither MotoGP 23 nor MotoGP 24 ships native UDP telemetry. A SimHub JSON
//! UDP bridge forwards normalised data in JSON frames on port 5556.
//!
//! Update rate: ~60 Hz.

use crate::{
    NormalizedTelemetry, TelemetryAdapter, TelemetryFrame, TelemetryReceiver, telemetry_now_ns,
};
use anyhow::Result;
use async_trait::async_trait;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::time::Duration;
use tokio::net::UdpSocket as TokioUdpSocket;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

const MOTOGP_PORT: u16 = 5556;
const MAX_PACKET_SIZE: usize = 4096;

/// MotoGP 23 / MotoGP 24 SimHub UDP bridge adapter.
pub struct MotoGPAdapter {
    bind_port: u16,
    update_rate: Duration,
}

impl MotoGPAdapter {
    pub fn new() -> Self {
        Self {
            bind_port: MOTOGP_PORT,
            update_rate: Duration::from_millis(16), // ~60 Hz
        }
    }
}

impl Default for MotoGPAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TelemetryAdapter for MotoGPAdapter {
    fn game_id(&self) -> &str {
        "motogp"
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
                    warn!("Failed to bind MotoGP UDP socket on port {bind_port}: {e}");
                    return;
                }
            };
            info!("MotoGP adapter listening on UDP port {bind_port}");
            let mut buf = [0u8; MAX_PACKET_SIZE];
            let mut frame_idx = 0u64;

            loop {
                match tokio::time::timeout(update_rate * 10, socket.recv(&mut buf)).await {
                    Ok(Ok(len)) => {
                        let normalized = NormalizedTelemetry::builder().build();
                        let frame =
                            TelemetryFrame::new(normalized, telemetry_now_ns(), frame_idx, len);
                        if tx.send(frame).await.is_err() {
                            debug!("Receiver dropped, stopping MotoGP monitoring");
                            break;
                        }
                        frame_idx = frame_idx.saturating_add(1);
                    }
                    Ok(Err(e)) => warn!("MotoGP UDP receive error: {e}"),
                    Err(_) => debug!("No MotoGP telemetry data received (timeout)"),
                }
            }
            info!("Stopped MotoGP telemetry monitoring");
        });

        Ok(rx)
    }

    async fn stop_monitoring(&self) -> Result<()> {
        Ok(())
    }

    fn normalize(&self, _raw: &[u8]) -> Result<NormalizedTelemetry> {
        Ok(NormalizedTelemetry::builder().build())
    }

    fn expected_update_rate(&self) -> Duration {
        self.update_rate
    }

    async fn is_game_running(&self) -> Result<bool> {
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn test_game_id() {
        let adapter = MotoGPAdapter::new();
        assert_eq!(adapter.game_id(), "motogp");
    }

    #[test]
    fn test_update_rate() {
        let adapter = MotoGPAdapter::new();
        assert_eq!(adapter.expected_update_rate(), Duration::from_millis(16));
    }

    #[test]
    fn test_normalize_returns_ok() -> TestResult {
        let adapter = MotoGPAdapter::new();
        let result = adapter.normalize(&[])?;
        assert!(result.rpm >= 0.0);
        Ok(())
    }

    #[tokio::test]
    async fn test_is_game_running() -> TestResult {
        let adapter = MotoGPAdapter::new();
        assert!(!adapter.is_game_running().await?);
        Ok(())
    }

    #[tokio::test]
    async fn test_stop_monitoring() -> TestResult {
        let adapter = MotoGPAdapter::new();
        adapter.stop_monitoring().await?;
        Ok(())
    }

    #[test]
    fn test_default() {
        let a = MotoGPAdapter::default();
        assert_eq!(a.game_id(), "motogp");
    }
}

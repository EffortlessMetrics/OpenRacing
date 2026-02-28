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
                    Ok(Ok(len)) => match crate::simhub::parse_simhub_packet(&buf[..len]) {
                        Ok(normalized) => {
                            let frame =
                                TelemetryFrame::new(normalized, telemetry_now_ns(), frame_idx, len);
                            if tx.send(frame).await.is_err() {
                                debug!("Receiver dropped, stopping MotoGP monitoring");
                                break;
                            }
                            frame_idx = frame_idx.saturating_add(1);
                        }
                        Err(e) => debug!("Failed to parse MotoGP SimHub packet: {e}"),
                    },
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

    fn normalize(&self, raw: &[u8]) -> Result<NormalizedTelemetry> {
        crate::simhub::parse_simhub_packet(raw)
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
        let json = br#"{"SpeedMs":0.0,"Rpms":0.0,"MaxRpms":0.0,"Gear":"N","Throttle":0.0,"Brake":0.0,"Clutch":0.0,"SteeringAngle":0.0,"FuelPercent":0.0,"LateralGForce":0.0,"LongitudinalGForce":0.0,"FFBValue":0.0,"IsRunning":false,"IsInPit":false}"#;
        let result = adapter.normalize(json)?;
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

    #[test]
    fn test_port_constants() {
        assert_eq!(MOTOGP_PORT, 5556);
    }

    #[test]
    fn test_empty_input_returns_err() {
        let adapter = MotoGPAdapter::new();
        assert!(adapter.normalize(&[]).is_err());
    }
}

#[cfg(test)]
mod proptest_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(500))]

        #[test]
        fn prop_arbitrary_bytes_no_panic(
            data in proptest::collection::vec(any::<u8>(), 0..4096)
        ) {
            let adapter = MotoGPAdapter::new();
            let _ = adapter.normalize(&data);
        }
    }
}

//! Gravel (Milestone, 2018) telemetry adapter — SimHub JSON bridge.
//!
//! Gravel does not expose a native UDP telemetry API. Telemetry is received
//! through SimHub's generic JSON UDP bridge (port 5555). Packets are parsed
//! using the shared SimHub JSON parser from [`crate::simhub`].

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

const SIMHUB_PORT: u16 = 5555;
const MAX_PACKET_SIZE: usize = 4096;

/// Gravel adapter (SimHub JSON UDP bridge stub, port 5555).
pub struct GravelAdapter {
    bind_port: u16,
    update_rate: Duration,
}

impl GravelAdapter {
    pub fn new() -> Self {
        Self {
            bind_port: SIMHUB_PORT,
            update_rate: Duration::from_millis(16),
        }
    }
}

impl Default for GravelAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TelemetryAdapter for GravelAdapter {
    fn game_id(&self) -> &str {
        "gravel"
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
                    warn!("Gravel: failed to bind SimHub UDP socket on port {bind_port}: {e}");
                    return;
                }
            };
            info!("Gravel adapter listening on SimHub UDP port {bind_port}");
            let mut buf = [0u8; MAX_PACKET_SIZE];
            let mut frame_idx = 0u64;

            loop {
                match tokio::time::timeout(update_rate * 10, socket.recv(&mut buf)).await {
                    Ok(Ok(len)) => match crate::simhub::parse_simhub_packet(&buf[..len]) {
                        Ok(normalized) => {
                            let frame =
                                TelemetryFrame::new(normalized, telemetry_now_ns(), frame_idx, len);
                            if tx.send(frame).await.is_err() {
                                debug!("Receiver dropped, stopping Gravel monitoring");
                                break;
                            }
                            frame_idx = frame_idx.saturating_add(1);
                        }
                        Err(e) => debug!("Failed to parse Gravel SimHub packet: {e}"),
                    },
                    Ok(Err(e)) => warn!("Gravel UDP receive error: {e}"),
                    Err(_) => debug!("No Gravel telemetry data received (timeout)"),
                }
            }
            info!("Stopped Gravel telemetry monitoring");
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

    const VALID_JSON: &[u8] = br#"{"SpeedMs":0.0,"Rpms":0.0,"MaxRpms":0.0,"Gear":"N","Throttle":0.0,"Brake":0.0,"Clutch":0.0,"SteeringAngle":0.0,"FuelPercent":0.0,"LateralGForce":0.0,"LongitudinalGForce":0.0,"FFBValue":0.0,"IsRunning":false,"IsInPit":false}"#;

    #[test]
    fn game_id_is_correct() {
        assert_eq!(GravelAdapter::new().game_id(), "gravel");
    }

    #[test]
    fn normalize_returns_default() -> TestResult {
        let adapter = GravelAdapter::new();
        let t = adapter.normalize(VALID_JSON)?;
        assert_eq!(t.speed_ms, 0.0);
        assert_eq!(t.rpm, 0.0);
        Ok(())
    }

    #[test]
    fn test_port_constants() {
        assert_eq!(SIMHUB_PORT, 5555);
    }

    #[test]
    fn test_empty_input_returns_err() {
        let adapter = GravelAdapter::new();
        assert!(adapter.normalize(&[]).is_err());
    }

    #[test]
    fn test_valid_packet_parses() -> TestResult {
        let adapter = GravelAdapter::new();
        let json = br#"{"SpeedMs":25.0,"Rpms":5500.0,"MaxRpms":8000.0,"Gear":"3","Throttle":80.0,"Brake":0.0,"Clutch":0.0,"SteeringAngle":45.0,"FuelPercent":60.0,"LateralGForce":0.8,"LongitudinalGForce":0.2,"FFBValue":0.3,"IsRunning":true,"IsInPit":false}"#;
        let t = adapter.normalize(json)?;
        assert!((t.speed_ms - 25.0).abs() < 0.01);
        assert!((t.rpm - 5500.0).abs() < 0.1);
        assert_eq!(t.gear, 3);
        Ok(())
    }

    #[test]
    fn test_update_rate() {
        let adapter = GravelAdapter::new();
        assert_eq!(adapter.expected_update_rate(), Duration::from_millis(16));
    }

    #[test]
    fn test_default() {
        let a = GravelAdapter::default();
        assert_eq!(a.game_id(), "gravel");
    }

    #[tokio::test]
    async fn test_is_game_running() -> TestResult {
        let adapter = GravelAdapter::new();
        assert!(!adapter.is_game_running().await?);
        Ok(())
    }

    #[tokio::test]
    async fn test_stop_monitoring() -> TestResult {
        let adapter = GravelAdapter::new();
        adapter.stop_monitoring().await?;
        Ok(())
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
            let adapter = GravelAdapter::new();
            let _ = adapter.normalize(&data);
        }
    }
}

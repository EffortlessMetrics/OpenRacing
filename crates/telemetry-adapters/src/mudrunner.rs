//! MudRunner / SnowRunner telemetry adapter (SimHub UDP JSON bridge on port 8877).
//!
//! Neither MudRunner nor SnowRunner ships native UDP telemetry.  A SimHub JSON
//! UDP bridge listens on port 8877 and forwards normalised data in JSON frames.
//!
//! Update rate: ~20 Hz.

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

const MUDRUNNER_PORT: u16 = 8877;
const MAX_PACKET_SIZE: usize = 2048;

/// Which variant of the MudRunner / SnowRunner franchise this adapter targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MudRunnerVariant {
    /// MudRunner (Focus Entertainment)
    MudRunner,
    /// SnowRunner (Focus Entertainment)
    SnowRunner,
}

impl MudRunnerVariant {
    fn game_id(self) -> &'static str {
        match self {
            Self::MudRunner => "mudrunner",
            Self::SnowRunner => "snowrunner",
        }
    }
}

/// MudRunner / SnowRunner SimHub UDP bridge adapter.
pub struct MudRunnerAdapter {
    variant: MudRunnerVariant,
    bind_port: u16,
    update_rate: Duration,
}

impl MudRunnerAdapter {
    /// Create a MudRunner adapter.
    pub fn new() -> Self {
        Self::with_variant(MudRunnerVariant::MudRunner)
    }

    /// Create an adapter for the given MudRunner variant.
    pub fn with_variant(variant: MudRunnerVariant) -> Self {
        Self {
            variant,
            bind_port: MUDRUNNER_PORT,
            update_rate: Duration::from_millis(50), // ~20 Hz
        }
    }
}

impl Default for MudRunnerAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TelemetryAdapter for MudRunnerAdapter {
    fn game_id(&self) -> &str {
        self.variant.game_id()
    }

    async fn start_monitoring(&self) -> Result<TelemetryReceiver> {
        let (tx, rx) = mpsc::channel(64);
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
            let mut frame_idx = 0u64;

            loop {
                match tokio::time::timeout(update_rate * 10, socket.recv(&mut buf)).await {
                    Ok(Ok(len)) => match crate::simhub::parse_simhub_packet(&buf[..len]) {
                        Ok(normalized) => {
                            let frame =
                                TelemetryFrame::new(normalized, telemetry_now_ns(), frame_idx, len);
                            if tx.send(frame).await.is_err() {
                                debug!("Receiver dropped, stopping {game_id} monitoring");
                                break;
                            }
                            frame_idx = frame_idx.saturating_add(1);
                        }
                        Err(e) => debug!("Failed to parse {game_id} SimHub packet: {e}"),
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
    fn test_port_constants() {
        assert_eq!(MUDRUNNER_PORT, 8877);
    }

    #[test]
    fn test_game_id_mudrunner() {
        let adapter = MudRunnerAdapter::new();
        assert_eq!(adapter.game_id(), "mudrunner");
    }

    #[test]
    fn test_game_id_snowrunner() {
        let adapter = MudRunnerAdapter::with_variant(MudRunnerVariant::SnowRunner);
        assert_eq!(adapter.game_id(), "snowrunner");
    }

    #[test]
    fn test_variant_game_ids() {
        assert_eq!(MudRunnerVariant::MudRunner.game_id(), "mudrunner");
        assert_eq!(MudRunnerVariant::SnowRunner.game_id(), "snowrunner");
    }

    #[test]
    fn test_update_rate() {
        let adapter = MudRunnerAdapter::new();
        assert_eq!(adapter.expected_update_rate(), Duration::from_millis(50));
    }

    #[test]
    fn test_default() {
        let adapter = MudRunnerAdapter::default();
        assert_eq!(adapter.game_id(), "mudrunner");
    }

    #[test]
    fn test_empty_input_returns_err() {
        let adapter = MudRunnerAdapter::new();
        assert!(adapter.normalize(&[]).is_err());
    }

    #[test]
    fn test_valid_packet_parses() -> TestResult {
        let adapter = MudRunnerAdapter::new();
        let t = adapter.normalize(VALID_JSON)?;
        assert_eq!(t.speed_ms, 0.0);
        assert_eq!(t.rpm, 0.0);
        Ok(())
    }

    #[test]
    fn test_valid_packet_with_values() -> TestResult {
        let adapter = MudRunnerAdapter::new();
        let json = br#"{"SpeedMs":8.5,"Rpms":2500.0,"MaxRpms":4500.0,"Gear":"2","Throttle":60.0,"Brake":0.0,"Clutch":0.0,"SteeringAngle":-90.0,"FuelPercent":70.0,"LateralGForce":0.3,"LongitudinalGForce":0.5,"FFBValue":0.2,"IsRunning":true,"IsInPit":false}"#;
        let t = adapter.normalize(json)?;
        assert!((t.speed_ms - 8.5).abs() < 0.01);
        assert!((t.rpm - 2500.0).abs() < 0.1);
        assert_eq!(t.gear, 2);
        Ok(())
    }

    #[tokio::test]
    async fn test_is_game_running() -> TestResult {
        let adapter = MudRunnerAdapter::new();
        assert!(!adapter.is_game_running().await?);
        Ok(())
    }

    #[tokio::test]
    async fn test_stop_monitoring() -> TestResult {
        let adapter = MudRunnerAdapter::new();
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
            data in proptest::collection::vec(any::<u8>(), 0..2048)
        ) {
            let adapter = MudRunnerAdapter::new();
            let _ = adapter.normalize(&data);
        }
    }
}

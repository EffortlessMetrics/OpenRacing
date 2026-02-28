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

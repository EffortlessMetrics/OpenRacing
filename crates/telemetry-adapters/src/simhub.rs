//! SimHub generic JSON UDP bridge adapter (port 5555).
//!
//! SimHub (SHWotever) provides a generic JSON UDP output that many games route
//! through.  Packets arrive as UTF-8 JSON objects on port 5555.
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

const SIMHUB_PORT: u16 = 5555;
const MAX_PACKET_SIZE: usize = 4096;

/// Generic SimHub JSON UDP bridge adapter.
pub struct SimHubAdapter {
    bind_port: u16,
    update_rate: Duration,
}

impl SimHubAdapter {
    pub fn new() -> Self {
        Self {
            bind_port: SIMHUB_PORT,
            update_rate: Duration::from_millis(16), // ~60 Hz
        }
    }
}

impl Default for SimHubAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TelemetryAdapter for SimHubAdapter {
    fn game_id(&self) -> &str {
        "simhub"
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
                    warn!("Failed to bind SimHub UDP socket on port {bind_port}: {e}");
                    return;
                }
            };
            info!("SimHub adapter listening on UDP port {bind_port}");
            let mut buf = [0u8; MAX_PACKET_SIZE];
            let mut frame_idx = 0u64;

            loop {
                match tokio::time::timeout(update_rate * 10, socket.recv(&mut buf)).await {
                    Ok(Ok(len)) => {
                        let normalized = NormalizedTelemetry::builder().build();
                        let frame =
                            TelemetryFrame::new(normalized, telemetry_now_ns(), frame_idx, len);
                        if tx.send(frame).await.is_err() {
                            debug!("Receiver dropped, stopping SimHub monitoring");
                            break;
                        }
                        frame_idx = frame_idx.saturating_add(1);
                    }
                    Ok(Err(e)) => warn!("SimHub UDP receive error: {e}"),
                    Err(_) => debug!("No SimHub telemetry data received (timeout)"),
                }
            }
            info!("Stopped SimHub telemetry monitoring");
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

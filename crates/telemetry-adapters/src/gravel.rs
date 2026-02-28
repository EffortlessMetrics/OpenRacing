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

    #[test]
    fn game_id_is_correct() {
        assert_eq!(GravelAdapter::new().game_id(), "gravel");
    }

    #[test]
    fn normalize_returns_default() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = GravelAdapter::new();
        // Gravel uses the SimHub JSON bridge — provide a minimal zero-value packet.
        let json = br#"{"SpeedMs":0.0,"Rpms":0.0,"MaxRpms":0.0,"Gear":"N","Throttle":0.0,"Brake":0.0,"Clutch":0.0,"SteeringAngle":0.0,"FuelPercent":0.0,"LateralGForce":0.0,"LongitudinalGForce":0.0,"FFBValue":0.0,"IsRunning":false,"IsInPit":false}"#;
        let t = adapter.normalize(json)?;
        assert_eq!(t.speed_ms, 0.0);
        assert_eq!(t.rpm, 0.0);
        Ok(())
    }
}

//! Gran Turismo Sport UDP telemetry adapter.
//!
//! GT Sport uses the identical Salsa20-encrypted "SimulatorInterface" UDP
//! packet format as GT7, with the default port numbers swapped:
//! - **Receive** on port 33739 (GT Sport sends telemetry here)
//! - **Send heartbeats** to port 33740 on the PlayStation
//!
//! Protocol documented by the community:
//! <https://www.gtplanet.net/forum/threads/gt6-is-compatible-with-the-ps4s-remote-play-feature.317250/>

use crate::{
    NormalizedTelemetry, TelemetryAdapter, TelemetryFrame, TelemetryReceiver,
    gran_turismo_7::{PACKET_SIZE, decrypt_and_parse},
    telemetry_now_ns,
};
use anyhow::Result;
use async_trait::async_trait;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::time::Duration;
use tokio::net::UdpSocket as TokioUdpSocket;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

/// UDP port on which GT Sport broadcasts telemetry to the host PC.
pub const GTS_RECV_PORT: u16 = 33739;
/// UDP port on the PlayStation to which heartbeat packets must be sent.
pub const GTS_SEND_PORT: u16 = 33740;

/// Gran Turismo Sport telemetry adapter.
///
/// Listens for Salsa20-encrypted UDP packets on [`GTS_RECV_PORT`] and sends
/// heartbeats back to the source host on [`GTS_SEND_PORT`] to keep the stream
/// alive. Packet parsing is delegated to the GT7 implementation since both
/// games share the same SimulatorInterface format.
pub struct GranTurismo7SportsAdapter {
    recv_port: u16,
    update_rate: Duration,
}

impl Default for GranTurismo7SportsAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl GranTurismo7SportsAdapter {
    pub fn new() -> Self {
        Self {
            recv_port: GTS_RECV_PORT,
            update_rate: Duration::from_millis(17), // ~60 Hz
        }
    }

    /// Override the receive port (useful for testing with ephemeral ports).
    pub fn with_port(mut self, port: u16) -> Self {
        self.recv_port = port;
        self
    }
}

#[async_trait]
impl TelemetryAdapter for GranTurismo7SportsAdapter {
    fn game_id(&self) -> &str {
        "gran_turismo_sport"
    }

    async fn start_monitoring(&self) -> Result<TelemetryReceiver> {
        let (tx, rx) = mpsc::channel(100);
        let recv_port = self.recv_port;

        tokio::spawn(async move {
            let bind_addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, recv_port));
            let socket = match TokioUdpSocket::bind(bind_addr).await {
                Ok(s) => s,
                Err(e) => {
                    warn!("Failed to bind GT Sport UDP socket on port {recv_port}: {e}");
                    return;
                }
            };
            info!("GT Sport adapter listening on UDP port {recv_port}");

            let heartbeat_payload = b"A";
            let mut buf = [0u8; PACKET_SIZE + 16];
            let mut frame_seq = 0u64;
            let mut last_heartbeat = tokio::time::Instant::now();
            let mut source_addr: Option<SocketAddr> = None;

            loop {
                if last_heartbeat.elapsed() >= Duration::from_millis(100) {
                    if let Some(addr) = source_addr {
                        let hb_addr = SocketAddr::new(addr.ip(), GTS_SEND_PORT);
                        let _ = socket.send_to(heartbeat_payload, hb_addr).await;
                    }
                    last_heartbeat = tokio::time::Instant::now();
                }

                match tokio::time::timeout(Duration::from_millis(50), socket.recv_from(&mut buf))
                    .await
                {
                    Ok(Ok((len, src))) => {
                        source_addr = Some(src);
                        match decrypt_and_parse(&buf[..len]) {
                            Ok(normalized) => {
                                let frame = TelemetryFrame::new(
                                    normalized,
                                    telemetry_now_ns(),
                                    frame_seq,
                                    len,
                                );
                                if tx.send(frame).await.is_err() {
                                    debug!("Receiver dropped, stopping GT Sport monitoring");
                                    break;
                                }
                                frame_seq = frame_seq.saturating_add(1);
                            }
                            Err(e) => debug!("Failed to parse GT Sport packet: {e}"),
                        }
                    }
                    Ok(Err(e)) => warn!("GT Sport UDP receive error: {e}"),
                    Err(_) => {} // timeout — keep looping to send heartbeat
                }
            }
            info!("Stopped GT Sport telemetry monitoring");
        });

        Ok(rx)
    }

    async fn stop_monitoring(&self) -> Result<()> {
        Ok(())
    }

    fn normalize(&self, raw: &[u8]) -> Result<NormalizedTelemetry> {
        decrypt_and_parse(raw)
    }

    fn expected_update_rate(&self) -> Duration {
        self.update_rate
    }

    /// GT Sport runs on a PlayStation console; process detection is not applicable.
    async fn is_game_running(&self) -> Result<bool> {
        Ok(false)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gran_turismo_7::{MAGIC, OFF_MAGIC};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn test_adapter_game_id() {
        let adapter = GranTurismo7SportsAdapter::new();
        assert_eq!(adapter.game_id(), "gran_turismo_sport");
    }

    #[test]
    fn test_adapter_update_rate() {
        let adapter = GranTurismo7SportsAdapter::new();
        assert_eq!(adapter.expected_update_rate(), Duration::from_millis(17));
    }

    #[tokio::test]
    async fn test_adapter_is_game_running() -> TestResult {
        let adapter = GranTurismo7SportsAdapter::new();
        let running = adapter.is_game_running().await?;
        assert!(
            !running,
            "GT Sport is a console game; process detection returns false"
        );
        Ok(())
    }

    #[test]
    fn test_normalize_short_data_returns_err() {
        let adapter = GranTurismo7SportsAdapter::new();
        let result = adapter.normalize(&[0u8; 10]);
        assert!(result.is_err());
    }

    #[test]
    fn test_with_port_override() {
        let adapter = GranTurismo7SportsAdapter::new().with_port(12345);
        assert_eq!(adapter.recv_port, 12345);
    }

    #[test]
    fn test_normalize_valid_packet() -> TestResult {
        // Build a minimal 296-byte packet with the magic set so parsing succeeds.
        let mut buf = [0u8; PACKET_SIZE];
        buf[OFF_MAGIC..OFF_MAGIC + 4].copy_from_slice(&MAGIC.to_le_bytes());
        let adapter = GranTurismo7SportsAdapter::new();
        let result = adapter.normalize(&buf);
        assert!(result.is_ok(), "valid packet must parse successfully");
        Ok(())
    }

    #[test]
    fn test_default_recv_port() {
        let adapter = GranTurismo7SportsAdapter::default();
        assert_eq!(adapter.recv_port, GTS_RECV_PORT);
    }
}

//! Forza Horizon 4 and Forza Horizon 5 telemetry adapters.
//!
//! Both games use the same "Forza Data Out" UDP protocol as Forza Motorsport
//! (232-byte Sled or 311-byte CarDash packets). Only the default listen port
//! differs:
//!
//! - **Forza Horizon 4**: port 12350
//! - **Forza Horizon 5**: port 5300
//!
//! Parsing is delegated entirely to [`crate::forza`]; this module provides
//! correctly-identified adapter wrappers with the appropriate default ports.

use crate::{
    NormalizedTelemetry, TelemetryAdapter, TelemetryFrame, TelemetryReceiver, forza,
    telemetry_now_ns,
};
use anyhow::Result;
use async_trait::async_trait;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::time::Duration;
use tokio::net::UdpSocket as TokioUdpSocket;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

/// Verified: SimHub wiki lists FH4 on port 12350.
const DEFAULT_FH4_PORT: u16 = 12350;
/// Verified: SimHub wiki lists FH5 on port 5300 (same as Forza Motorsport).
const DEFAULT_FH5_PORT: u16 = 5300;
const MAX_PACKET_SIZE: usize = 512;

/// Generic adapter used by both Forza Horizon variants.
struct ForzaHorizonAdapter {
    game_id: &'static str,
    bind_port: u16,
    update_rate: Duration,
}

impl ForzaHorizonAdapter {
    fn new(game_id: &'static str, default_port: u16) -> Self {
        Self {
            game_id,
            bind_port: default_port,
            update_rate: Duration::from_millis(16),
        }
    }
}

#[async_trait]
impl TelemetryAdapter for ForzaHorizonAdapter {
    fn game_id(&self) -> &str {
        self.game_id
    }

    async fn start_monitoring(&self) -> Result<TelemetryReceiver> {
        let (tx, rx) = mpsc::channel(100);
        let bind_port = self.bind_port;
        let update_rate = self.update_rate;
        let game_id = self.game_id;

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
            let mut frame_seq = 0u64;

            loop {
                match tokio::time::timeout(update_rate * 10, socket.recv(&mut buf)).await {
                    Ok(Ok(len)) => match forza::parse_forza_packet(&buf[..len]) {
                        Ok(normalized) => {
                            let frame =
                                TelemetryFrame::new(normalized, telemetry_now_ns(), frame_seq, len);
                            if tx.send(frame).await.is_err() {
                                debug!("Receiver dropped, stopping {game_id} monitoring");
                                break;
                            }
                            frame_seq = frame_seq.saturating_add(1);
                        }
                        Err(e) => debug!("Failed to parse {game_id} packet: {e}"),
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
        forza::parse_forza_packet(raw)
    }

    fn expected_update_rate(&self) -> Duration {
        self.update_rate
    }

    async fn is_game_running(&self) -> Result<bool> {
        Ok(false)
    }
}

/// Forza Horizon 4 telemetry adapter (port 12350).
pub struct ForzaHorizon4Adapter(ForzaHorizonAdapter);

impl ForzaHorizon4Adapter {
    pub fn new() -> Self {
        Self(ForzaHorizonAdapter::new(
            "forza_horizon_4",
            DEFAULT_FH4_PORT,
        ))
    }
}

impl Default for ForzaHorizon4Adapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TelemetryAdapter for ForzaHorizon4Adapter {
    fn game_id(&self) -> &str {
        self.0.game_id()
    }

    async fn start_monitoring(&self) -> Result<TelemetryReceiver> {
        self.0.start_monitoring().await
    }

    async fn stop_monitoring(&self) -> Result<()> {
        self.0.stop_monitoring().await
    }

    fn normalize(&self, raw: &[u8]) -> Result<NormalizedTelemetry> {
        self.0.normalize(raw)
    }

    fn expected_update_rate(&self) -> Duration {
        self.0.expected_update_rate()
    }

    async fn is_game_running(&self) -> Result<bool> {
        self.0.is_game_running().await
    }
}

/// Forza Horizon 5 telemetry adapter (port 5300).
pub struct ForzaHorizon5Adapter(ForzaHorizonAdapter);

impl ForzaHorizon5Adapter {
    pub fn new() -> Self {
        Self(ForzaHorizonAdapter::new(
            "forza_horizon_5",
            DEFAULT_FH5_PORT,
        ))
    }
}

impl Default for ForzaHorizon5Adapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TelemetryAdapter for ForzaHorizon5Adapter {
    fn game_id(&self) -> &str {
        self.0.game_id()
    }

    async fn start_monitoring(&self) -> Result<TelemetryReceiver> {
        self.0.start_monitoring().await
    }

    async fn stop_monitoring(&self) -> Result<()> {
        self.0.stop_monitoring().await
    }

    fn normalize(&self, raw: &[u8]) -> Result<NormalizedTelemetry> {
        self.0.normalize(raw)
    }

    fn expected_update_rate(&self) -> Duration {
        self.0.expected_update_rate()
    }

    async fn is_game_running(&self) -> Result<bool> {
        self.0.is_game_running().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// Build a minimum valid Forza Sled packet (232 bytes, is_race_on=1).
    fn make_sled_fixture() -> Vec<u8> {
        let mut data = vec![0u8; 232];
        data[0..4].copy_from_slice(&1i32.to_le_bytes()); // is_race_on = 1
        data[8..12].copy_from_slice(&8000.0f32.to_le_bytes()); // engine_max_rpm
        data[16..20].copy_from_slice(&5000.0f32.to_le_bytes()); // current_rpm
        data[32..36].copy_from_slice(&20.0f32.to_le_bytes()); // vel_x → speed 20 m/s
        data
    }

    /// Build a minimum valid Forza CarDash packet (311 bytes, is_race_on=1).
    fn make_cardash_fixture() -> Vec<u8> {
        let mut data = vec![0u8; 311];
        let sled = make_sled_fixture();
        data[..232].copy_from_slice(&sled);
        data[244..248].copy_from_slice(&20.0f32.to_le_bytes()); // dash_speed
        data
    }

    #[test]
    fn fh4_game_id() {
        assert_eq!(ForzaHorizon4Adapter::new().game_id(), "forza_horizon_4");
    }

    #[test]
    fn fh5_game_id() {
        assert_eq!(ForzaHorizon5Adapter::new().game_id(), "forza_horizon_5");
    }

    #[test]
    fn fh4_rejects_short_packet() {
        let adapter = ForzaHorizon4Adapter::new();
        assert!(adapter.normalize(&[0u8; 10]).is_err());
    }

    #[test]
    fn fh5_rejects_short_packet() {
        let adapter = ForzaHorizon5Adapter::new();
        assert!(adapter.normalize(&[0u8; 10]).is_err());
    }

    #[test]
    fn fh4_update_rate() {
        assert_eq!(
            ForzaHorizon4Adapter::new().expected_update_rate(),
            Duration::from_millis(16)
        );
    }

    #[test]
    fn fh5_update_rate() {
        assert_eq!(
            ForzaHorizon5Adapter::new().expected_update_rate(),
            Duration::from_millis(16)
        );
    }

    #[test]
    fn fh4_rejects_empty_input() -> TestResult {
        let adapter = ForzaHorizon4Adapter::new();
        let result = adapter.normalize(&[]);
        assert!(result.is_err(), "empty input must return an error for FH4");
        Ok(())
    }

    #[test]
    fn fh5_rejects_empty_input() -> TestResult {
        let adapter = ForzaHorizon5Adapter::new();
        let result = adapter.normalize(&[]);
        assert!(result.is_err(), "empty input must return an error for FH5");
        Ok(())
    }

    #[test]
    fn fh4_accepts_sled_packet() -> TestResult {
        let adapter = ForzaHorizon4Adapter::new();
        let normalized = adapter.normalize(&make_sled_fixture())?;
        assert!(
            (normalized.rpm - 5000.0).abs() < 0.01,
            "RPM mismatch: got {}",
            normalized.rpm
        );
        assert!(
            (normalized.speed_ms - 20.0).abs() < 0.01,
            "speed_ms mismatch: got {}",
            normalized.speed_ms
        );
        Ok(())
    }

    #[test]
    fn fh5_accepts_sled_packet() -> TestResult {
        let adapter = ForzaHorizon5Adapter::new();
        let normalized = adapter.normalize(&make_sled_fixture())?;
        assert!(
            (normalized.rpm - 5000.0).abs() < 0.01,
            "RPM mismatch: got {}",
            normalized.rpm
        );
        assert!(
            (normalized.speed_ms - 20.0).abs() < 0.01,
            "speed_ms mismatch: got {}",
            normalized.speed_ms
        );
        Ok(())
    }

    #[test]
    fn fh4_accepts_cardash_packet() -> TestResult {
        let adapter = ForzaHorizon4Adapter::new();
        let normalized = adapter.normalize(&make_cardash_fixture())?;
        assert!(
            (normalized.rpm - 5000.0).abs() < 0.01,
            "RPM mismatch: got {}",
            normalized.rpm
        );
        assert!(
            (normalized.speed_ms - 20.0).abs() < 0.01,
            "speed_ms mismatch: got {}",
            normalized.speed_ms
        );
        Ok(())
    }

    #[test]
    fn fh5_accepts_cardash_packet() -> TestResult {
        let adapter = ForzaHorizon5Adapter::new();
        let normalized = adapter.normalize(&make_cardash_fixture())?;
        assert!(
            (normalized.rpm - 5000.0).abs() < 0.01,
            "RPM mismatch: got {}",
            normalized.rpm
        );
        assert!(
            (normalized.speed_ms - 20.0).abs() < 0.01,
            "speed_ms mismatch: got {}",
            normalized.speed_ms
        );
        Ok(())
    }

    #[test]
    fn fh4_sled_snapshot() -> TestResult {
        let adapter = ForzaHorizon4Adapter::new();
        let normalized = adapter.normalize(&make_sled_fixture())?;
        insta::assert_yaml_snapshot!("fh4_sled", normalized);
        Ok(())
    }
}

#[cfg(test)]
mod proptest_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// Arbitrary byte sequences fed to ForzaHorizon4Adapter must never panic.
        #[test]
        fn prop_fh4_no_panic_on_arbitrary_bytes(
            data in proptest::collection::vec(any::<u8>(), 0..512)
        ) {
            let adapter = ForzaHorizon4Adapter::new();
            let _ = adapter.normalize(&data);
        }

        /// Arbitrary byte sequences fed to ForzaHorizon5Adapter must never panic.
        #[test]
        fn prop_fh5_no_panic_on_arbitrary_bytes(
            data in proptest::collection::vec(any::<u8>(), 0..512)
        ) {
            let adapter = ForzaHorizon5Adapter::new();
            let _ = adapter.normalize(&data);
        }
    }
}

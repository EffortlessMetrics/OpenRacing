//! Dirt 4 telemetry adapter for Codemasters extradata v0 UDP format.
//!
//! Enable in-game: Settings → UDP Telemetry, port 20777, extradata=3.
//!
//! The packet layout follows the Codemasters legacy UDP format shared with DiRT Rally 2.0.
//! All fields are little-endian `f32` at known byte offsets.  Parsing is delegated to
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

const ENV_PORT: &str = "OPENRACING_DIRT4_UDP_PORT";
const ENV_HEARTBEAT_TIMEOUT_MS: &str = "OPENRACING_DIRT4_HEARTBEAT_TIMEOUT_MS";

const GAME_LABEL: &str = "Dirt 4";

/// Dirt 4 adapter for Codemasters extradata v0 UDP telemetry.
#[derive(Clone)]
pub struct Dirt4Adapter {
    bind_port: u16,
    update_rate: Duration,
    heartbeat_timeout: Duration,
    last_packet_ns: Arc<AtomicU64>,
}

impl Default for Dirt4Adapter {
    fn default() -> Self {
        Self::new()
    }
}

impl Dirt4Adapter {
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
impl TelemetryAdapter for Dirt4Adapter {
    fn game_id(&self) -> &str {
        "dirt4"
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
                        "Dirt 4 UDP socket bind failed"
                    );
                    return;
                }
            };

            info!(port = bind_port, "Dirt 4 UDP adapter bound");

            let mut frame_idx = 0u64;
            let mut buf = vec![0u8; MAX_PACKET_SIZE];
            let timeout = (update_rate * 4).max(Duration::from_millis(25));

            loop {
                let recv = tokio::time::timeout(timeout, socket.recv(&mut buf)).await;
                let len = match recv {
                    Ok(Ok(len)) => len,
                    Ok(Err(error)) => {
                        warn!(error = %error, "Dirt 4 UDP receive error");
                        continue;
                    }
                    Err(_) => {
                        debug!("Dirt 4 UDP receive timeout");
                        continue;
                    }
                };

                let data = &buf[..len];
                let normalized = match parse_packet(data) {
                    Ok(n) => n,
                    Err(error) => {
                        warn!(error = %error, "Failed to parse Dirt 4 packet");
                        continue;
                    }
                };

                last_packet_ns.store(telemetry_now_ns(), Ordering::Relaxed);

                let frame = TelemetryFrame::new(normalized, telemetry_now_ns(), frame_idx, len);
                if tx.send(frame).await.is_err() {
                    break;
                }

                frame_idx = frame_idx.saturating_add(1);
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
    use crate::TelemetryValue;
    use crate::codemasters_shared::*;

    fn make_packet(size: usize) -> Vec<u8> {
        vec![0u8; size]
    }

    fn write_f32(buf: &mut [u8], offset: usize, value: f32) {
        buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    #[test]
    fn rejects_short_packet() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = Dirt4Adapter::new();
        let result = adapter.normalize(&[0u8; MIN_PACKET_SIZE - 1]);
        assert!(result.is_err(), "expected error for short packet");
        Ok(())
    }

    #[test]
    fn zero_packet_returns_zero_speed_and_rpm() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = Dirt4Adapter::new();
        let raw = make_packet(MIN_PACKET_SIZE);
        let t = adapter.normalize(&raw)?;
        assert_eq!(t.speed_ms, 0.0);
        assert_eq!(t.rpm, 0.0);
        Ok(())
    }

    #[test]
    fn zero_gear_maps_to_reverse() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = Dirt4Adapter::new();
        let raw = make_packet(MIN_PACKET_SIZE);
        let t = adapter.normalize(&raw)?;
        assert_eq!(t.gear, -1);
        Ok(())
    }

    #[test]
    fn game_id_is_correct() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = Dirt4Adapter::new();
        assert_eq!(adapter.game_id(), "dirt4");
        Ok(())
    }

    #[test]
    fn speed_extracted_from_wheel_speeds() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = Dirt4Adapter::new();
        let mut raw = make_packet(MIN_PACKET_SIZE);
        write_f32(&mut raw, OFF_WHEEL_SPEED_FL, 20.0);
        write_f32(&mut raw, OFF_WHEEL_SPEED_FR, 20.0);
        write_f32(&mut raw, OFF_WHEEL_SPEED_RL, 20.0);
        write_f32(&mut raw, OFF_WHEEL_SPEED_RR, 20.0);
        let t = adapter.normalize(&raw)?;
        assert!(
            (t.speed_ms - 20.0).abs() < 0.001,
            "speed_ms should be 20.0, got {}",
            t.speed_ms
        );
        Ok(())
    }

    #[test]
    fn in_pit_flag_set_when_one() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = Dirt4Adapter::new();
        let mut raw = make_packet(MIN_PACKET_SIZE);
        write_f32(&mut raw, OFF_IN_PIT, 1.0);
        let t = adapter.normalize(&raw)?;
        assert!(t.flags.in_pits, "in_pits should be true");
        Ok(())
    }

    #[test]
    fn rpm_and_rpm_fraction_extracted() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = Dirt4Adapter::new();
        let mut raw = make_packet(MIN_PACKET_SIZE);
        write_f32(&mut raw, OFF_RPM, 5000.0);
        write_f32(&mut raw, OFF_MAX_RPM, 8000.0);
        let t = adapter.normalize(&raw)?;
        assert!((t.rpm - 5000.0).abs() < 0.001);
        assert!((t.max_rpm - 8000.0).abs() < 0.001);
        if let Some(TelemetryValue::Float(fraction)) = t.extended.get("rpm_fraction") {
            assert!(
                (fraction - 0.625).abs() < 0.001,
                "rpm_fraction should be 0.625, got {fraction}"
            );
        } else {
            return Err("rpm_fraction not found in extended telemetry".into());
        }
        Ok(())
    }

    #[test]
    fn empty_input_returns_error() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = Dirt4Adapter::new();
        assert!(
            adapter.normalize(&[]).is_err(),
            "empty input must return an error"
        );
        Ok(())
    }

    #[test]
    fn known_good_payload_throttle_brake_gear() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = Dirt4Adapter::new();
        let mut raw = make_packet(MIN_PACKET_SIZE);
        write_f32(&mut raw, OFF_WHEEL_SPEED_FL, 20.0);
        write_f32(&mut raw, OFF_WHEEL_SPEED_FR, 20.0);
        write_f32(&mut raw, OFF_WHEEL_SPEED_RL, 20.0);
        write_f32(&mut raw, OFF_WHEEL_SPEED_RR, 20.0);
        write_f32(&mut raw, OFF_THROTTLE, 0.7);
        write_f32(&mut raw, OFF_BRAKE, 0.0);
        write_f32(&mut raw, OFF_GEAR, 2.0);
        let t = adapter.normalize(&raw)?;
        assert!((t.speed_ms - 20.0).abs() < 0.001, "speed_ms={}", t.speed_ms);
        assert!((t.throttle - 0.7).abs() < 0.001, "throttle={}", t.throttle);
        assert!(t.brake.abs() < 0.001, "brake={}", t.brake);
        assert_eq!(t.gear, 2, "gear={}", t.gear);
        Ok(())
    }

    #[test]
    fn speed_is_nonnegative() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = Dirt4Adapter::new();
        let mut raw = make_packet(MIN_PACKET_SIZE);
        write_f32(&mut raw, OFF_WHEEL_SPEED_FL, 10.0);
        write_f32(&mut raw, OFF_WHEEL_SPEED_FR, 10.0);
        write_f32(&mut raw, OFF_WHEEL_SPEED_RL, 10.0);
        write_f32(&mut raw, OFF_WHEEL_SPEED_RR, 10.0);
        let t = adapter.normalize(&raw)?;
        assert!(
            t.speed_ms >= 0.0,
            "speed_ms must be non-negative, got {}",
            t.speed_ms
        );
        Ok(())
    }

    #[test]
    fn throttle_clamped_to_unit_range() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = Dirt4Adapter::new();
        let mut raw = make_packet(MIN_PACKET_SIZE);
        write_f32(&mut raw, OFF_THROTTLE, 3.0);
        let t = adapter.normalize(&raw)?;
        assert!(
            t.throttle >= 0.0 && t.throttle <= 1.0,
            "throttle={} must be in [0.0, 1.0]",
            t.throttle
        );
        Ok(())
    }

    #[test]
    fn brake_clamped_to_unit_range() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = Dirt4Adapter::new();
        let mut raw = make_packet(MIN_PACKET_SIZE);
        write_f32(&mut raw, OFF_BRAKE, 5.0);
        let t = adapter.normalize(&raw)?;
        assert!(
            t.brake >= 0.0 && t.brake <= 1.0,
            "brake={} must be in [0.0, 1.0]",
            t.brake
        );
        Ok(())
    }

    #[test]
    fn gear_forward_stays_in_range() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = Dirt4Adapter::new();
        for g in 1i32..=8 {
            let mut raw = make_packet(MIN_PACKET_SIZE);
            write_f32(&mut raw, OFF_GEAR, g as f32);
            let t = adapter.normalize(&raw)?;
            assert!(
                t.gear >= -1 && t.gear <= 8,
                "gear {} out of expected range -1..=8",
                t.gear
            );
        }
        Ok(())
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
            let adapter = Dirt4Adapter::new();
            let _ = adapter.normalize(&data);
        }
    }
}

//! Dirt 5 telemetry adapter for Codemasters-style custom UDP streams.
//!
//! Dirt 5 is treated as a bridge-backed telemetry source using the DiRT 4
//! Codemasters UDP schema model. This adapter is intentionally telemetry-only;
//! no force-feedback scalar is emitted because the protocol family is not known
//! to include a steering torque request.

use crate::codemasters_udp::{CustomUdpSpec, DecodedCodemastersPacket, canonical_channel_id};
use crate::{
    NormalizedTelemetry, TelemetryAdapter, TelemetryFrame, TelemetryReceiver, TelemetryValue,
    telemetry_now_ns,
};
use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use std::collections::HashMap;
use std::f32::consts::PI;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::Duration;
use tokio::net::UdpSocket as TokioUdpSocket;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

const DEFAULT_DIRT5_PORT: u16 = 20777;
const DEFAULT_DIRT5_MODE: u8 = 1;
const DEFAULT_DIRT5_HEARTBEAT_TIMEOUT_MS: u64 = 1_500;
const MAX_PACKET_SIZE: usize = 2048;

const ENV_DIRT5_UDP_PORT: &str = "OPENRACING_DIRT5_UDP_PORT";
const ENV_DIRT5_UDP_MODE: &str = "OPENRACING_DIRT5_UDP_MODE";
const ENV_DIRT5_CUSTOM_UDP_XML: &str = "OPENRACING_DIRT5_CUSTOM_UDP_XML";
const ENV_DIRT5_HEARTBEAT_TIMEOUT_MS: &str = "OPENRACING_DIRT5_HEARTBEAT_TIMEOUT_MS";

/// Dirt 5 adapter bound to Codemasters-compatible UDP telemetry.
#[derive(Clone)]
pub struct Dirt5Adapter {
    bind_port: u16,
    mode: u8,
    custom_udp_xml: Option<PathBuf>,
    update_rate: Duration,
    heartbeat_timeout: Duration,
    last_packet_ns: Arc<AtomicU64>,
}

impl Default for Dirt5Adapter {
    fn default() -> Self {
        Self::new()
    }
}

impl Dirt5Adapter {
    pub fn new() -> Self {
        let bind_port = parse_u16_env(ENV_DIRT5_UDP_PORT, DEFAULT_DIRT5_PORT);
        let mode = parse_u8_env(ENV_DIRT5_UDP_MODE, DEFAULT_DIRT5_MODE);
        let heartbeat_timeout = Duration::from_millis(parse_u64_env(
            ENV_DIRT5_HEARTBEAT_TIMEOUT_MS,
            DEFAULT_DIRT5_HEARTBEAT_TIMEOUT_MS,
        ));
        let custom_udp_xml = std::env::var(ENV_DIRT5_CUSTOM_UDP_XML)
            .ok()
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from);

        Self {
            bind_port,
            mode,
            custom_udp_xml,
            update_rate: Duration::from_millis(16),
            heartbeat_timeout,
            last_packet_ns: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn with_port(mut self, bind_port: u16) -> Self {
        self.bind_port = bind_port;
        self
    }

    pub fn with_mode(mut self, mode: u8) -> Self {
        self.mode = mode;
        self
    }

    pub fn with_custom_udp_xml(mut self, path: PathBuf) -> Self {
        self.custom_udp_xml = Some(path);
        self
    }

    fn expected_packet_size(&self) -> usize {
        self.load_spec()
            .map(|spec| spec.expected_bytes())
            .unwrap_or(0)
    }

    fn load_spec(&self) -> Result<CustomUdpSpec> {
        if let Some(path) = self.custom_udp_xml.as_deref() {
            CustomUdpSpec::from_xml_path(path).with_context(|| {
                format!(
                    "failed to load Dirt 5 custom UDP spec from {}",
                    path.display()
                )
            })
        } else {
            Ok(CustomUdpSpec::from_mode(self.mode))
        }
    }

    fn normalize_decoded(packet: &DecodedCodemastersPacket) -> NormalizedTelemetry {
        let mut telemetry = NormalizedTelemetry::default();
        let lookup = |aliases: &[&str]| -> Option<f32> { packet_f32(&packet.values, aliases) };

        if let Some(speed_ms) = lookup(&["speed"]) {
            telemetry = telemetry.with_speed_ms(speed_ms);
        }

        if let Some(engine_rate_rad_s) = lookup(&["engine_rate", "engine rate", "enginerate"]) {
            let rpm = engine_rate_rad_s * 60.0 / (2.0 * PI);
            telemetry = telemetry.with_rpm(rpm);
        }

        if let Some(gear_raw) = lookup(&["gear"])
            && gear_raw.is_finite()
        {
            let gear = gear_raw.trunc();
            if (-127.0..=127.0).contains(&gear) {
                telemetry = telemetry.with_gear(gear as i8);
            }
        }

        if let Some(slip_ratio) = lookup(&["slip_ratio"]) {
            telemetry = telemetry.with_slip_ratio(slip_ratio);
        } else {
            let patch_channels = [
                "wheel_patch_speed_fl",
                "wheel_patch_speed_fr",
                "wheel_patch_speed_rl",
                "wheel_patch_speed_rr",
            ];
            let patch_speed_max = patch_channels
                .iter()
                .filter_map(|channel| lookup(&[*channel]))
                .filter(|speed| speed.is_finite())
                .map(|speed| speed.abs())
                .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

            if let (Some(speed_ms), Some(patch_speed)) = (telemetry.speed_ms, patch_speed_max) {
                let denominator = speed_ms.max(1.0);
                telemetry = telemetry.with_slip_ratio((patch_speed - speed_ms).abs() / denominator);
            }
        }

        for (channel, value) in &packet.values {
            telemetry = telemetry.with_extended(channel.clone(), TelemetryValue::Float(*value));
        }

        if let Some(fourcc) = &packet.fourcc {
            telemetry = telemetry
                .with_extended("fourcc".to_string(), TelemetryValue::String(fourcc.clone()));
        }

        telemetry.with_extended(
            "decoder_type".to_string(),
            TelemetryValue::String("codemasters_custom_udp".to_string()),
        )
    }

    fn is_recent_packet(&self) -> bool {
        let last = self.last_packet_ns.load(Ordering::Relaxed);
        if last == 0 {
            return false;
        }

        let now = u128::from(telemetry_now_ns());
        let last_u = u128::from(last);
        let elapsed_ns = now.saturating_sub(last_u);
        elapsed_ns <= self.heartbeat_timeout.as_nanos()
    }
}

fn packet_f32(values: &HashMap<String, f32>, aliases: &[&str]) -> Option<f32> {
    aliases.iter().find_map(|alias| {
        let key = canonical_channel_id(alias);
        values.get(&key).copied()
    })
}

#[async_trait]
impl TelemetryAdapter for Dirt5Adapter {
    fn game_id(&self) -> &str {
        "dirt5"
    }

    async fn start_monitoring(&self) -> Result<TelemetryReceiver> {
        let spec = self.load_spec()?;
        let expected_bytes = spec.expected_bytes();
        let bind_port = self.bind_port;
        let update_rate = self.update_rate;
        let last_packet_ns = Arc::clone(&self.last_packet_ns);
        let (tx, rx) = mpsc::channel(100);

        tokio::spawn(async move {
            let bind_addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, bind_port));
            let socket = match TokioUdpSocket::bind(bind_addr).await {
                Ok(socket) => socket,
                Err(error) => {
                    warn!(
                        error = %error,
                        port = bind_port,
                        "Dirt 5 UDP socket bind failed"
                    );
                    return;
                }
            };

            info!(port = bind_port, "Dirt 5 UDP adapter bound");

            let mut sequence = 0u64;
            let mut buf = vec![0u8; MAX_PACKET_SIZE.max(expected_bytes.max(1))];
            let mut timeout = update_rate * 4;
            if timeout == Duration::ZERO {
                timeout = Duration::from_millis(25);
            }

            loop {
                let recv = tokio::time::timeout(timeout, socket.recv(&mut buf)).await;
                let len = match recv {
                    Ok(Ok(len)) => len,
                    Ok(Err(error)) => {
                        warn!(error = %error, "Error receiving Dirt 5 UDP telemetry");
                        continue;
                    }
                    Err(_) => {
                        debug!("Dirt 5 UDP receive timeout waiting for packet");
                        continue;
                    }
                };

                let data = &buf[..len];
                let decoded = match spec.decode(data) {
                    Ok(packet) => packet,
                    Err(error) => {
                        warn!(
                            error = %error,
                            "Failed to decode Dirt 5 UDP packet"
                        );
                        continue;
                    }
                };

                last_packet_ns.store(telemetry_now_ns(), Ordering::Relaxed);

                let normalized = Dirt5Adapter::normalize_decoded(&decoded);
                let frame = TelemetryFrame::new(normalized, telemetry_now_ns(), sequence, len);
                if tx.send(frame).await.is_err() {
                    break;
                }

                sequence = sequence.saturating_add(1);
            }
        });

        Ok(rx)
    }

    async fn stop_monitoring(&self) -> Result<()> {
        Ok(())
    }

    fn normalize(&self, raw: &[u8]) -> Result<NormalizedTelemetry> {
        let expected = self.expected_packet_size();
        if expected > 0 && raw.len() < expected {
            return Err(anyhow!(
                "Dirt 5 packet too short: expected at least {} bytes, got {}",
                expected,
                raw.len()
            ));
        }

        let spec = self.load_spec()?;
        let decoded = spec.decode(raw)?;
        Ok(Self::normalize_decoded(&decoded))
    }

    fn expected_update_rate(&self) -> Duration {
        self.update_rate
    }

    async fn is_game_running(&self) -> Result<bool> {
        Ok(self.is_recent_packet())
    }
}

fn parse_u16_env(name: &str, fallback: u16) -> u16 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(fallback)
}

fn parse_u8_env(name: &str, fallback: u8) -> u8 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<u8>().ok())
        .filter(|value| *value <= 3)
        .unwrap_or(fallback)
}

fn parse_u64_env(name: &str, fallback: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(fallback)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dirt5_adapter_normalization() -> Result<(), Box<dyn std::error::Error>> {
        let spec = CustomUdpSpec::from_mode(1);
        let mut packet = Vec::new();
        packet.extend_from_slice(&20.0f32.to_le_bytes());
        packet.extend_from_slice(&6283.1855f32.to_le_bytes()); // engine_rate
        packet.extend_from_slice(&(3i32.to_le_bytes()));
        packet.extend_from_slice(&(0.25f32.to_le_bytes()));
        packet.extend_from_slice(&(0.7f32.to_le_bytes()));
        packet.extend_from_slice(&(0.0f32.to_le_bytes()));
        packet.extend_from_slice(&(0.0f32.to_le_bytes()));
        packet.extend_from_slice(&18.0f32.to_le_bytes());
        packet.extend_from_slice(&19.0f32.to_le_bytes());
        packet.extend_from_slice(&15.0f32.to_le_bytes());
        packet.extend_from_slice(&14.0f32.to_le_bytes());
        packet.extend_from_slice(&0.02f32.to_le_bytes());
        packet.extend_from_slice(&0.01f32.to_le_bytes());
        packet.extend_from_slice(&0.01f32.to_le_bytes());
        packet.extend_from_slice(&0.03f32.to_le_bytes());

        let decoded = spec.decode(&packet)?;
        let normalized = Dirt5Adapter::normalize_decoded(&decoded);

        assert_eq!(normalized.speed_ms, Some(20.0));
        assert_eq!(normalized.gear, Some(3));
        assert_eq!(normalized.rpm, Some(60000.0));
        assert!(normalized.slip_ratio.is_some());
        assert_eq!(
            normalized.extended.get("wheelpatchspeedfl"),
            Some(&TelemetryValue::Float(18.0))
        );
        Ok(())
    }

    #[test]
    fn test_dirt5_adapter_rejects_short_packet() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = Dirt5Adapter::new();
        let raw = vec![0u8; 4];

        let result = adapter.normalize(&raw);
        assert!(result.is_err());
        Ok(())
    }
}

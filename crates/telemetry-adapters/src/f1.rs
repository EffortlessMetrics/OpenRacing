//! F1 telemetry adapter for Codemasters-style UDP streams.
//!
//! F1 support is currently bridge-backed and uses the shared custom UDP decoder
//! used by other Codemasters-family integrations.

use crate::codemasters_udp::{CustomUdpSpec, DecodedCodemastersPacket, canonical_channel_id};
use crate::{
    NormalizedTelemetry, TelemetryAdapter, TelemetryFlags, TelemetryFrame, TelemetryReceiver,
    TelemetryValue, telemetry_now_ns,
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

const DEFAULT_F1_PORT: u16 = 20777;
const DEFAULT_F1_MODE: u8 = 3;
const DEFAULT_F1_HEARTBEAT_TIMEOUT_MS: u64 = 1_500;
const MAX_PACKET_SIZE: usize = 4096;

const ENV_F1_UDP_PORT: &str = "OPENRACING_F1_UDP_PORT";
const ENV_F1_UDP_MODE: &str = "OPENRACING_F1_UDP_MODE";
const ENV_F1_CUSTOM_UDP_XML: &str = "OPENRACING_F1_CUSTOM_UDP_XML";
const ENV_F1_HEARTBEAT_TIMEOUT_MS: &str = "OPENRACING_F1_HEARTBEAT_TIMEOUT_MS";

/// Bridge-backed F1 adapter bound to Codemasters-compatible UDP telemetry.
#[derive(Clone)]
pub struct F1Adapter {
    bind_port: u16,
    mode: u8,
    custom_udp_xml: Option<PathBuf>,
    update_rate: Duration,
    heartbeat_timeout: Duration,
    last_packet_ns: Arc<AtomicU64>,
}

impl Default for F1Adapter {
    fn default() -> Self {
        Self::new()
    }
}

impl F1Adapter {
    pub fn new() -> Self {
        let bind_port = parse_u16_env(ENV_F1_UDP_PORT, DEFAULT_F1_PORT);
        let mode = parse_u8_env(ENV_F1_UDP_MODE, DEFAULT_F1_MODE);
        let heartbeat_timeout = Duration::from_millis(parse_u64_env(
            ENV_F1_HEARTBEAT_TIMEOUT_MS,
            DEFAULT_F1_HEARTBEAT_TIMEOUT_MS,
        ));
        let custom_udp_xml = std::env::var(ENV_F1_CUSTOM_UDP_XML)
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
                format!("failed to load F1 custom UDP spec from {}", path.display())
            })
        } else {
            Ok(CustomUdpSpec::from_mode(self.mode))
        }
    }

    fn normalize_decoded(packet: &DecodedCodemastersPacket) -> NormalizedTelemetry {
        let mut telemetry = NormalizedTelemetry::default();
        let lookup = |aliases: &[&str]| -> Option<f32> { packet_f32(&packet.values, aliases) };
        let lookup_bool =
            |aliases: &[&str]| -> Option<bool> { packet_bool(&packet.values, aliases) };

        if let Some(speed_ms) = lookup(&["speed", "vehicle_speed", "speed_ms", "speed_mps"]) {
            telemetry = telemetry.with_speed_ms(speed_ms);
        }

        if let Some(rpm) = lookup(&["rpm", "engine_rpm"]) {
            telemetry = telemetry.with_rpm(rpm);
        } else if let Some(engine_rate_rad_s) = lookup(&["engine_rate", "engine_rate_rad_s"]) {
            let rpm = engine_rate_rad_s * 60.0 / (2.0 * PI);
            telemetry = telemetry.with_rpm(rpm);
        }

        if let Some(gear_raw) = lookup(&["gear", "current_gear"])
            && gear_raw.is_finite()
        {
            let gear = gear_raw.trunc();
            if (-127.0..=127.0).contains(&gear) {
                telemetry = telemetry.with_gear(gear as i8);
            }
        }

        if let Some(slip_ratio) = lookup(&["slip_ratio", "tyre_slip_ratio", "wheel_slip_ratio"]) {
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

            if let Some(patch_speed) = patch_speed_max {
                let speed_ms = telemetry.speed_ms;
                if speed_ms > 0.0 {
                    let denominator = speed_ms.max(1.0);
                    telemetry =
                        telemetry.with_slip_ratio((patch_speed - speed_ms).abs() / denominator);
                }
            }
        }

        let pit_limiter = lookup_bool(&["pit_limiter", "pit_limiter_on"]).unwrap_or(false);
        let in_pits = lookup_bool(&["in_pits", "in_pit_lane", "pit_lane"]).unwrap_or(false);
        let drs_available =
            lookup_bool(&["drs_available", "drs_allowed", "drs_enabled"]).unwrap_or(false);
        let drs_active = lookup_bool(&["drs_active", "drs_open", "drs_deployed"]).unwrap_or(false);
        let ers_available = lookup_bool(&["ers_available", "ers_enabled"])
            .or_else(|| lookup(&["ers_deploy_mode"]).map(|value| value > 0.5))
            .unwrap_or(false);
        let traction_control = lookup_bool(&["traction_control", "tc_active"]).unwrap_or(false);
        let abs_active = lookup_bool(&["abs_active", "abs"]).unwrap_or(false);

        let flags = TelemetryFlags {
            pit_limiter,
            in_pits,
            drs_available,
            drs_active,
            ers_available,
            traction_control,
            abs_active,
            ..TelemetryFlags::default()
        };
        telemetry = telemetry.with_flags(flags);

        for (channel, value) in &packet.values {
            telemetry = telemetry.with_extended(channel.clone(), TelemetryValue::Float(*value));
        }

        if let Some(fourcc) = &packet.fourcc {
            telemetry = telemetry
                .with_extended("fourcc".to_string(), TelemetryValue::String(fourcc.clone()));
        }

        if let Some(fuel_remaining_kg) = lookup(&["fuel_remaining_kg", "fuel_remaining", "fuel"]) {
            telemetry = telemetry.with_extended(
                "fuel_remaining_kg".to_string(),
                TelemetryValue::Float(fuel_remaining_kg),
            );
        }
        if let Some(ers_store_energy_j) =
            lookup(&["ers_store_energy", "ers_store_energy_j", "ers_energy"])
        {
            telemetry = telemetry.with_extended(
                "ers_store_energy_j".to_string(),
                TelemetryValue::Float(ers_store_energy_j),
            );
        }
        if let Some(ers_deploy_mode) = lookup(&["ers_deploy_mode"]) {
            telemetry = telemetry.with_extended(
                "ers_deploy_mode".to_string(),
                TelemetryValue::Integer(ers_deploy_mode as i32),
            );
        }
        if let Some(session_type) = lookup(&["session_type", "session", "session_mode"]) {
            telemetry = telemetry.with_extended(
                "session_type".to_string(),
                TelemetryValue::Integer(session_type as i32),
            );
        }

        telemetry = telemetry
            .with_extended(
                "drs_available".to_string(),
                TelemetryValue::Boolean(drs_available),
            )
            .with_extended(
                "drs_active".to_string(),
                TelemetryValue::Boolean(drs_active),
            )
            .with_extended(
                "ers_available".to_string(),
                TelemetryValue::Boolean(ers_available),
            )
            .with_extended(
                "decoder_type".to_string(),
                TelemetryValue::String("f1_codemasters_udp_bridge".to_string()),
            );

        telemetry
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

fn packet_bool(values: &HashMap<String, f32>, aliases: &[&str]) -> Option<bool> {
    packet_f32(values, aliases).and_then(|value| {
        if value.is_finite() {
            Some(value > 0.5)
        } else {
            None
        }
    })
}

#[async_trait]
impl TelemetryAdapter for F1Adapter {
    fn game_id(&self) -> &str {
        "f1"
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
                        "F1 UDP socket bind failed"
                    );
                    return;
                }
            };

            info!(port = bind_port, "F1 UDP adapter bound");

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
                        warn!(error = %error, "Error receiving F1 UDP telemetry");
                        continue;
                    }
                    Err(_) => {
                        debug!("F1 UDP receive timeout waiting for packet");
                        continue;
                    }
                };

                let data = &buf[..len];
                let decoded = match spec.decode(data) {
                    Ok(packet) => packet,
                    Err(error) => {
                        warn!(
                            error = %error,
                            "Failed to decode F1 UDP packet"
                        );
                        continue;
                    }
                };

                last_packet_ns.store(telemetry_now_ns(), Ordering::Relaxed);

                let normalized = F1Adapter::normalize_decoded(&decoded);
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
                "F1 packet too short: expected at least {} bytes, got {}",
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
    fn test_f1_adapter_normalization_maps_core_fields_and_flags() {
        let mut values = HashMap::new();
        values.insert("speed".to_string(), 72.0);
        values.insert("enginerpm".to_string(), 11000.0);
        values.insert("gear".to_string(), 7.0);
        values.insert("slipratio".to_string(), 0.2);
        values.insert("drsavailable".to_string(), 1.0);
        values.insert("drsactive".to_string(), 1.0);
        values.insert("ersdeploymode".to_string(), 2.0);
        values.insert("pitlimiter".to_string(), 1.0);
        values.insert("fuelremainingkg".to_string(), 18.5);
        values.insert("sessiontype".to_string(), 10.0);

        let packet = DecodedCodemastersPacket {
            values,
            fourcc: Some("F125".to_string()),
        };

        let normalized = F1Adapter::normalize_decoded(&packet);

        assert_eq!(normalized.speed_ms, Some(72.0));
        assert_eq!(normalized.rpm, Some(11000.0));
        assert_eq!(normalized.gear, Some(7));
        assert_eq!(normalized.slip_ratio, Some(0.2));
        assert!(normalized.flags.drs_available);
        assert!(normalized.flags.drs_active);
        assert!(normalized.flags.ers_available);
        assert!(normalized.flags.pit_limiter);
        assert_eq!(
            normalized.extended.get("fuel_remaining_kg"),
            Some(&TelemetryValue::Float(18.5))
        );
        assert_eq!(
            normalized.extended.get("session_type"),
            Some(&TelemetryValue::Integer(10))
        );
        assert_eq!(
            normalized.extended.get("fourcc"),
            Some(&TelemetryValue::String("F125".to_string()))
        );
    }

    #[test]
    fn test_f1_adapter_rejects_short_packet() {
        let adapter = F1Adapter::new();
        let raw = vec![0u8; 4];

        let result = adapter.normalize(&raw);
        assert!(result.is_err());
    }
}

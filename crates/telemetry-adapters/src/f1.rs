//! F1 telemetry adapter for Codemasters-style UDP streams.
//!
//! F1 support is currently bridge-backed and uses the shared custom UDP decoder
//! used by other Codemasters-family integrations.
//!
//! ## Verification (2025-07)
//!
//! - **Default port**: 20777 — standard Codemasters/EA F1 UDP port. ✓
//! - **Custom UDP modes**: 0–3 (mode 3 = full telemetry). ✓
//! - **MAX_PACKET_SIZE**: 4096 bytes (sufficient for all known modes). ✓

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
        let lookup = |aliases: &[&str]| -> Option<f32> { packet_f32(&packet.values, aliases) };
        let lookup_bool =
            |aliases: &[&str]| -> Option<bool> { packet_bool(&packet.values, aliases) };

        let speed_ms = lookup(&["speed", "vehicle_speed", "speed_ms", "speed_mps"]);
        let rpm = lookup(&["rpm", "engine_rpm"]).or_else(|| {
            lookup(&["engine_rate", "engine_rate_rad_s"])
                .map(|engine_rate_rad_s| engine_rate_rad_s * 60.0 / (2.0 * PI))
        });
        let gear = lookup(&["gear", "current_gear"]).and_then(|gear_raw| {
            if gear_raw.is_finite() {
                let gear = gear_raw.trunc();
                if (-127.0..=127.0).contains(&gear) {
                    Some(gear as i8)
                } else {
                    None
                }
            } else {
                None
            }
        });
        let slip_ratio =
            lookup(&["slip_ratio", "tyre_slip_ratio", "wheel_slip_ratio"]).or_else(|| {
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

                patch_speed_max.and_then(|patch_speed| {
                    speed_ms.filter(|&s| s > 0.0).map(|s| {
                        let denominator = s.max(1.0);
                        (patch_speed - s).abs() / denominator
                    })
                })
            });

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

        let mut builder = NormalizedTelemetry::builder();

        if let Some(speed) = speed_ms {
            builder = builder.speed_ms(speed);
        }
        if let Some(r) = rpm {
            builder = builder.rpm(r);
        }
        if let Some(g) = gear {
            builder = builder.gear(g);
        }
        if let Some(slip) = slip_ratio {
            builder = builder.slip_ratio(slip);
        }

        builder = builder.flags(flags);

        for (channel, value) in &packet.values {
            builder = builder.extended(channel.clone(), TelemetryValue::Float(*value));
        }

        if let Some(fourcc) = &packet.fourcc {
            builder =
                builder.extended("fourcc".to_string(), TelemetryValue::String(fourcc.clone()));
        }

        if let Some(fuel_remaining_kg) = lookup(&["fuel_remaining_kg", "fuel_remaining", "fuel"]) {
            builder = builder.extended(
                "fuel_remaining_kg".to_string(),
                TelemetryValue::Float(fuel_remaining_kg),
            );
        }
        if let Some(ers_store_energy_j) =
            lookup(&["ers_store_energy", "ers_store_energy_j", "ers_energy"])
        {
            builder = builder.extended(
                "ers_store_energy_j".to_string(),
                TelemetryValue::Float(ers_store_energy_j),
            );
        }
        if let Some(ers_deploy_mode) = lookup(&["ers_deploy_mode"]) {
            builder = builder.extended(
                "ers_deploy_mode".to_string(),
                TelemetryValue::Integer(ers_deploy_mode as i32),
            );
        }
        if let Some(session_type) = lookup(&["session_type", "session", "session_mode"]) {
            builder = builder.extended(
                "session_type".to_string(),
                TelemetryValue::Integer(session_type as i32),
            );
        }

        builder
            .extended(
                "drs_available".to_string(),
                TelemetryValue::Boolean(drs_available),
            )
            .extended(
                "drs_active".to_string(),
                TelemetryValue::Boolean(drs_active),
            )
            .extended(
                "ers_available".to_string(),
                TelemetryValue::Boolean(ers_available),
            )
            .extended(
                "decoder_type".to_string(),
                TelemetryValue::String("f1_codemasters_udp_bridge".to_string()),
            )
            .build()
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

            let mut frame_seq = 0u64;
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
                let frame = TelemetryFrame::new(normalized, telemetry_now_ns(), frame_seq, len);
                if tx.send(frame).await.is_err() {
                    break;
                }

                frame_seq = frame_seq.saturating_add(1);
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

        assert_eq!(normalized.speed_ms, 72.0);
        assert_eq!(normalized.rpm, 11000.0);
        assert_eq!(normalized.gear, 7);
        assert_eq!(normalized.slip_ratio, 0.2);
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

    #[test]
    fn test_game_id_is_f1() {
        assert_eq!(F1Adapter::new().game_id(), "f1");
    }

    #[test]
    fn test_update_rate() {
        let adapter = F1Adapter::new();
        assert_eq!(adapter.expected_update_rate(), Duration::from_millis(16));
    }

    /// `engine_rate` (rad/s) should be converted to RPM via `rate * 60 / 2π`.
    #[test]
    fn test_engine_rate_rad_s_to_rpm_conversion() -> Result<(), Box<dyn std::error::Error>> {
        let mut values = HashMap::new();
        // 100π rad/s → 3000 RPM
        values.insert("enginerate".to_string(), 100.0 * PI);
        let packet = DecodedCodemastersPacket {
            values,
            fourcc: None,
        };
        let t = F1Adapter::normalize_decoded(&packet);
        assert!(
            (t.rpm - 3000.0).abs() < 0.1,
            "expected ~3000 RPM from 100π rad/s, got {}",
            t.rpm
        );
        Ok(())
    }

    /// When both `rpm` and `engine_rate` are present, `rpm` takes priority.
    #[test]
    fn test_rpm_alias_priority_over_engine_rate() -> Result<(), Box<dyn std::error::Error>> {
        let mut values = HashMap::new();
        values.insert("rpm".to_string(), 7000.0);
        values.insert("enginerate".to_string(), 100.0 * PI);
        let packet = DecodedCodemastersPacket {
            values,
            fourcc: None,
        };
        let t = F1Adapter::normalize_decoded(&packet);
        assert!(
            (t.rpm - 7000.0).abs() < 0.1,
            "rpm alias should take priority, got {}",
            t.rpm
        );
        Ok(())
    }

    /// Empty decoded packet should produce default (zero) telemetry with no panic.
    #[test]
    fn test_empty_decoded_packet_defaults() -> Result<(), Box<dyn std::error::Error>> {
        let packet = DecodedCodemastersPacket {
            values: HashMap::new(),
            fourcc: None,
        };
        let t = F1Adapter::normalize_decoded(&packet);
        assert_eq!(t.speed_ms, 0.0);
        assert_eq!(t.rpm, 0.0);
        assert_eq!(t.gear, 0);
        assert_eq!(t.slip_ratio, 0.0);
        assert!(!t.flags.drs_available);
        assert!(!t.flags.drs_active);
        assert!(!t.flags.pit_limiter);
        Ok(())
    }

    /// Gear values outside i8 range should not be mapped.
    #[test]
    fn test_gear_out_of_i8_range_ignored() -> Result<(), Box<dyn std::error::Error>> {
        let mut values = HashMap::new();
        values.insert("gear".to_string(), 200.0);
        let packet = DecodedCodemastersPacket {
            values,
            fourcc: None,
        };
        let t = F1Adapter::normalize_decoded(&packet);
        // 200 > 127, so gear should not be set (stays default 0)
        assert_eq!(t.gear, 0, "gear out of i8 range should be ignored");
        Ok(())
    }

    /// Gear value of NaN should not be mapped.
    #[test]
    fn test_gear_nan_ignored() -> Result<(), Box<dyn std::error::Error>> {
        let mut values = HashMap::new();
        values.insert("gear".to_string(), f32::NAN);
        let packet = DecodedCodemastersPacket {
            values,
            fourcc: None,
        };
        let t = F1Adapter::normalize_decoded(&packet);
        assert_eq!(t.gear, 0, "NaN gear should be ignored");
        Ok(())
    }

    /// Reverse gear (-1.0) should map to -1.
    #[test]
    fn test_reverse_gear_mapping() -> Result<(), Box<dyn std::error::Error>> {
        let mut values = HashMap::new();
        values.insert("gear".to_string(), -1.0);
        let packet = DecodedCodemastersPacket {
            values,
            fourcc: None,
        };
        let t = F1Adapter::normalize_decoded(&packet);
        assert_eq!(t.gear, -1, "raw -1.0 should map to reverse");
        Ok(())
    }

    /// Neutral gear (0.0) should map to 0.
    #[test]
    fn test_neutral_gear_mapping() -> Result<(), Box<dyn std::error::Error>> {
        let mut values = HashMap::new();
        values.insert("gear".to_string(), 0.0);
        let packet = DecodedCodemastersPacket {
            values,
            fourcc: None,
        };
        let t = F1Adapter::normalize_decoded(&packet);
        assert_eq!(t.gear, 0, "raw 0.0 should map to neutral");
        Ok(())
    }

    /// Slip ratio derived from wheel patch speeds when no direct channel exists.
    #[test]
    fn test_slip_ratio_from_wheel_patch_speeds() -> Result<(), Box<dyn std::error::Error>> {
        let mut values = HashMap::new();
        values.insert("speed".to_string(), 20.0);
        values.insert("wheelpatchspeedfl".to_string(), 22.0);
        values.insert("wheelpatchspeedfr".to_string(), 22.0);
        values.insert("wheelpatchspeedrl".to_string(), 22.0);
        values.insert("wheelpatchspeedrr".to_string(), 22.0);
        let packet = DecodedCodemastersPacket {
            values,
            fourcc: None,
        };
        let t = F1Adapter::normalize_decoded(&packet);
        // slip = |22 - 20| / max(20, 1) = 2 / 20 = 0.1
        assert!(
            (t.slip_ratio - 0.1).abs() < 0.01,
            "slip_ratio from patch speeds: expected ~0.1, got {}",
            t.slip_ratio
        );
        Ok(())
    }

    /// Direct slip_ratio channel takes priority over derived value.
    #[test]
    fn test_direct_slip_ratio_over_derived() -> Result<(), Box<dyn std::error::Error>> {
        let mut values = HashMap::new();
        values.insert("speed".to_string(), 20.0);
        values.insert("slipratio".to_string(), 0.5);
        values.insert("wheelpatchspeedfl".to_string(), 22.0);
        let packet = DecodedCodemastersPacket {
            values,
            fourcc: None,
        };
        let t = F1Adapter::normalize_decoded(&packet);
        assert!(
            (t.slip_ratio - 0.5).abs() < 0.001,
            "direct slip_ratio should take priority"
        );
        Ok(())
    }

    /// Flag aliases are canonicalized (underscores/dashes/spaces stripped).
    #[test]
    fn test_flag_alias_canonicalization() -> Result<(), Box<dyn std::error::Error>> {
        let mut values = HashMap::new();
        // "pit_limiter_on" → canonical "pitlimiteron" should match "pit_limiter" alias
        values.insert("inpits".to_string(), 1.0);
        values.insert("tcactive".to_string(), 1.0);
        values.insert("absactive".to_string(), 1.0);
        let packet = DecodedCodemastersPacket {
            values,
            fourcc: None,
        };
        let t = F1Adapter::normalize_decoded(&packet);
        assert!(t.flags.in_pits, "in_pits flag from alias");
        assert!(t.flags.traction_control, "traction_control flag from alias");
        assert!(t.flags.abs_active, "abs_active flag from alias");
        Ok(())
    }

    /// Boolean-valued flags: value > 0.5 → true, ≤ 0.5 → false.
    #[test]
    fn test_flag_threshold() -> Result<(), Box<dyn std::error::Error>> {
        // 0.5 should be false, 0.51 should be true
        let mut values = HashMap::new();
        values.insert("drsavailable".to_string(), 0.5);
        let packet = DecodedCodemastersPacket {
            values,
            fourcc: None,
        };
        let t = F1Adapter::normalize_decoded(&packet);
        assert!(
            !t.flags.drs_available,
            "0.5 should not trigger drs_available"
        );

        let mut values2 = HashMap::new();
        values2.insert("drsavailable".to_string(), 0.51);
        let packet2 = DecodedCodemastersPacket {
            values: values2,
            fourcc: None,
        };
        let t2 = F1Adapter::normalize_decoded(&packet2);
        assert!(t2.flags.drs_available, "0.51 should trigger drs_available");
        Ok(())
    }

    /// ERS available is derived from ers_deploy_mode > 0.5.
    #[test]
    fn test_ers_available_from_deploy_mode() -> Result<(), Box<dyn std::error::Error>> {
        let mut values = HashMap::new();
        values.insert("ersdeploymode".to_string(), 0.0);
        let packet = DecodedCodemastersPacket {
            values,
            fourcc: None,
        };
        let t = F1Adapter::normalize_decoded(&packet);
        assert!(
            !t.flags.ers_available,
            "ers_deploy_mode 0 means ERS not available"
        );

        let mut values2 = HashMap::new();
        values2.insert("ersdeploymode".to_string(), 1.0);
        let packet2 = DecodedCodemastersPacket {
            values: values2,
            fourcc: None,
        };
        let t2 = F1Adapter::normalize_decoded(&packet2);
        assert!(
            t2.flags.ers_available,
            "ers_deploy_mode 1 means ERS available"
        );
        Ok(())
    }

    /// Extended fields (fuel_remaining_kg, ers_store_energy_j, session_type)
    /// are populated from their respective channels.
    #[test]
    fn test_extended_fields_populated() -> Result<(), Box<dyn std::error::Error>> {
        let mut values = HashMap::new();
        values.insert("fuelremainingkg".to_string(), 25.3);
        values.insert("ersstoreenergy".to_string(), 4_000_000.0);
        values.insert("ersdeploymode".to_string(), 3.0);
        values.insert("sessiontype".to_string(), 5.0);
        let packet = DecodedCodemastersPacket {
            values,
            fourcc: Some("F124".to_string()),
        };
        let t = F1Adapter::normalize_decoded(&packet);
        assert_eq!(
            t.extended.get("fuel_remaining_kg"),
            Some(&TelemetryValue::Float(25.3))
        );
        assert_eq!(
            t.extended.get("ers_store_energy_j"),
            Some(&TelemetryValue::Float(4_000_000.0))
        );
        assert_eq!(
            t.extended.get("ers_deploy_mode"),
            Some(&TelemetryValue::Integer(3))
        );
        assert_eq!(
            t.extended.get("session_type"),
            Some(&TelemetryValue::Integer(5))
        );
        assert_eq!(
            t.extended.get("decoder_type"),
            Some(&TelemetryValue::String(
                "f1_codemasters_udp_bridge".to_string()
            ))
        );
        Ok(())
    }

    /// No fourcc in decoded packet should not insert "fourcc" in extended.
    #[test]
    fn test_no_fourcc_no_extended_key() -> Result<(), Box<dyn std::error::Error>> {
        let packet = DecodedCodemastersPacket {
            values: HashMap::new(),
            fourcc: None,
        };
        let t = F1Adapter::normalize_decoded(&packet);
        assert!(
            !t.extended.contains_key("fourcc"),
            "no fourcc should not be in extended"
        );
        Ok(())
    }

    /// NaN flag values should produce `None` from packet_bool → default false.
    #[test]
    fn test_nan_flag_value_treated_as_false() -> Result<(), Box<dyn std::error::Error>> {
        let mut values = HashMap::new();
        values.insert("drsactive".to_string(), f32::NAN);
        let packet = DecodedCodemastersPacket {
            values,
            fourcc: None,
        };
        let t = F1Adapter::normalize_decoded(&packet);
        assert!(!t.flags.drs_active, "NaN flag should be treated as false");
        Ok(())
    }

    /// Alias resolution: `vehicle_speed` should be resolved to speed_ms.
    #[test]
    fn test_speed_alias_vehicle_speed() -> Result<(), Box<dyn std::error::Error>> {
        let mut values = HashMap::new();
        values.insert("vehiclespeed".to_string(), 33.5);
        let packet = DecodedCodemastersPacket {
            values,
            fourcc: None,
        };
        let t = F1Adapter::normalize_decoded(&packet);
        assert!(
            (t.speed_ms - 33.5).abs() < 0.001,
            "vehicle_speed alias: expected 33.5, got {}",
            t.speed_ms
        );
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
            let adapter = F1Adapter::new();
            let _ = adapter.normalize(&data);
        }
    }
}

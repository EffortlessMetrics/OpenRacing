//! EA SPORTS WRC telemetry adapter using schema-driven UDP decoding.

use crate::telemetry::{
    NormalizedTelemetry, TelemetryAdapter, TelemetryFrame, TelemetryReceiver, TelemetryValue,
};
use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use std::fs;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, Instant};
use tokio::net::UdpSocket as TokioUdpSocket;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

const DEFAULT_EAWRC_PORT: u16 = 20778;
const DEFAULT_STRUCTURE_ID: &str = "openracing";
const DEFAULT_PACKET_ID: &str = "session_update";
const SUPPORTED_SCHEMA_VERSION: u32 = 1;
const MAX_PACKET_SIZE: usize = 8192;
const TELEMETRY_DIR_OVERRIDE_ENV: &str = "OPENRACING_EAWRC_TELEMETRY_DIR";

pub struct EAWRCAdapter {
    telemetry_dir: PathBuf,
    update_rate: Duration,
}

impl Default for EAWRCAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl EAWRCAdapter {
    pub fn new() -> Self {
        Self {
            telemetry_dir: telemetry_root_from_environment(),
            update_rate: Duration::from_millis(16),
        }
    }

    pub fn with_telemetry_dir(telemetry_dir: PathBuf) -> Self {
        Self {
            telemetry_dir,
            ..Self::new()
        }
    }

    fn load_bundle(&self) -> Result<DecoderBundle> {
        let channels_path = self.telemetry_dir.join("readme").join("channels.json");
        let channels: ChannelsFile = read_json(&channels_path).with_context(|| {
            format!(
                "failed to read EA WRC channels catalog at {}",
                channels_path.display()
            )
        })?;

        if let Some(schema) = channels.versions.schema
            && schema > SUPPORTED_SCHEMA_VERSION
        {
            return Err(anyhow!(
                "EA WRC telemetry schema {} is newer than supported {}",
                schema,
                SUPPORTED_SCHEMA_VERSION
            ));
        }

        let config_path = self.telemetry_dir.join("config.json");
        let config: Option<ConfigFile> = read_optional_json(&config_path)?;

        let structure_id = config
            .as_ref()
            .and_then(ConfigFile::structure_id)
            .unwrap_or_else(|| DEFAULT_STRUCTURE_ID.to_string());

        let structure_path = self
            .telemetry_dir
            .join("udp")
            .join(format!("{structure_id}.json"));
        let structure: StructureFile = read_json(&structure_path).with_context(|| {
            format!(
                "failed to read EA WRC structure file at {}",
                structure_path.display()
            )
        })?;

        let packets_catalog_path = self.telemetry_dir.join("readme").join("packets.json");
        let packets_catalog: Option<PacketsCatalogFile> =
            read_optional_json(&packets_catalog_path)?;

        let plan = DecoderPlan::compile(
            &channels,
            &structure,
            packets_catalog.as_ref(),
            structure_id.clone(),
        )?;

        let mut assignments = config
            .map(|cfg| cfg.assignments_for_structure(&structure_id))
            .unwrap_or_default();
        assignments.retain(|assignment| plan.layouts.contains_key(&assignment.packet_id));

        if assignments.is_empty() {
            assignments.push(PacketAssignment {
                packet_id: DEFAULT_PACKET_ID.to_string(),
                port: DEFAULT_EAWRC_PORT,
            });
        }

        Ok(DecoderBundle { plan, assignments })
    }

    fn normalize_decoded(packet: &DecodedPacket) -> NormalizedTelemetry {
        let mut telemetry = NormalizedTelemetry::default();

        if let Some(value) = value_f32(
            &packet.values,
            &["ffb_scalar", "steering_force", "steering_torque"],
        ) {
            telemetry = telemetry.with_ffb_scalar(value);
        }

        if let Some(value) = value_f32(
            &packet.values,
            &[
                "rpm",
                "engine_rpm",
                "vehicle_engine_rpm",
                "powertrain_engine_rpm",
            ],
        ) {
            telemetry = telemetry.with_rpm(value);
        }

        if let Some(value) = value_f32(&packet.values, &["speed", "vehicle_speed", "speed_ms"]) {
            telemetry = telemetry.with_speed_ms(value);
        }

        if let Some(value) = value_f32(
            &packet.values,
            &["slip_ratio", "vehicle_slip_ratio", "tyre_slip"],
        ) {
            telemetry = telemetry.with_slip_ratio(value);
        }

        if let Some(value) = value_i8(
            &packet.values,
            &["gear", "vehicle_gear", "gear_index", "vehicle_gear_index"],
        ) {
            telemetry = telemetry.with_gear(value);
        }

        if let Some(value) = value_string(&packet.values, &["car_id", "vehicle_name", "vehicle_id"])
        {
            telemetry = telemetry.with_car_id(value);
        }

        if let Some(value) = value_string(&packet.values, &["track_id", "track_name", "stage_name"])
        {
            telemetry = telemetry.with_track_id(value);
        }

        for (channel_id, value) in &packet.values {
            telemetry = telemetry.with_extended(channel_id.clone(), value.to_telemetry_value());
        }

        telemetry
            .with_extended(
                "packet_id".to_string(),
                TelemetryValue::String(packet.packet_id.clone()),
            )
            .with_extended(
                "decoder_structure_id".to_string(),
                TelemetryValue::String(packet.structure_id.clone()),
            )
    }
}

#[async_trait]
impl TelemetryAdapter for EAWRCAdapter {
    fn game_id(&self) -> &str {
        "eawrc"
    }

    async fn start_monitoring(&self) -> Result<TelemetryReceiver> {
        let bundle = self.load_bundle()?;
        let (tx, rx) = mpsc::channel(100);

        let sequence = Arc::new(AtomicU64::new(0));
        let epoch = Instant::now();

        let mut packet_ids_by_port: HashMap<u16, Vec<String>> = HashMap::new();
        for assignment in bundle.assignments {
            packet_ids_by_port
                .entry(assignment.port)
                .or_default()
                .push(assignment.packet_id);
        }

        for (port, mut packet_ids) in packet_ids_by_port {
            packet_ids.sort_unstable();
            packet_ids.dedup();

            let plan = bundle.plan.clone();
            let tx = tx.clone();
            let sequence = Arc::clone(&sequence);
            let update_rate = self.update_rate;

            tokio::spawn(async move {
                let bind_addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port));
                let socket = match TokioUdpSocket::bind(bind_addr).await {
                    Ok(socket) => socket,
                    Err(error) => {
                        warn!(port = port, error = %error, "Failed to bind EA WRC UDP socket");
                        return;
                    }
                };

                info!(port = port, packet_ids = ?packet_ids, "EA WRC telemetry socket bound");

                let mut buf = [0u8; MAX_PACKET_SIZE];
                loop {
                    let recv = tokio::time::timeout(update_rate * 4, socket.recv(&mut buf)).await;
                    let len = match recv {
                        Ok(Ok(len)) => len,
                        Ok(Err(error)) => {
                            warn!(port = port, error = %error, "EA WRC UDP receive failed");
                            continue;
                        }
                        Err(_) => {
                            debug!(port = port, "EA WRC UDP receive timeout");
                            continue;
                        }
                    };

                    let decoded = match plan.decode_any(&packet_ids, &buf[..len]) {
                        Ok(packet) => packet,
                        Err(error) => {
                            warn!(port = port, error = %error, "Failed to decode EA WRC packet");
                            continue;
                        }
                    };

                    let frame = TelemetryFrame::new(
                        EAWRCAdapter::normalize_decoded(&decoded),
                        monotonic_ns_since(epoch, Instant::now()),
                        sequence.fetch_add(1, Ordering::Relaxed),
                        len,
                    );

                    if tx.send(frame).await.is_err() {
                        break;
                    }
                }
            });
        }

        drop(tx);
        Ok(rx)
    }

    async fn stop_monitoring(&self) -> Result<()> {
        Ok(())
    }

    fn normalize(&self, raw: &[u8]) -> Result<NormalizedTelemetry> {
        let bundle = self.load_bundle()?;
        let packet_id = bundle
            .assignments
            .first()
            .map(|assignment| assignment.packet_id.clone())
            .unwrap_or_else(|| DEFAULT_PACKET_ID.to_string());
        let decoded = bundle.plan.decode(&packet_id, raw)?;
        Ok(Self::normalize_decoded(&decoded))
    }

    fn expected_update_rate(&self) -> Duration {
        self.update_rate
    }

    async fn is_game_running(&self) -> Result<bool> {
        let has_config = self.telemetry_dir.join("config.json").exists();
        let has_channels = self
            .telemetry_dir
            .join("readme")
            .join("channels.json")
            .exists();
        Ok(has_config && has_channels)
    }
}

#[derive(Debug, Clone)]
struct DecoderBundle {
    plan: DecoderPlan,
    assignments: Vec<PacketAssignment>,
}

#[derive(Debug, Clone)]
struct PacketAssignment {
    packet_id: String,
    port: u16,
}

#[derive(Debug, Clone)]
struct DecoderPlan {
    structure_id: String,
    layouts: HashMap<String, PacketLayout>,
    packet_uid_to_id: HashMap<String, String>,
}

#[derive(Debug, Clone)]
struct PacketLayout {
    channels: Vec<ChannelLayout>,
    total_size: usize,
}

#[derive(Debug, Clone)]
struct ChannelLayout {
    id: String,
    ty: ChannelType,
    offset: usize,
}

#[derive(Debug, Clone)]
struct DecodedPacket {
    packet_id: String,
    structure_id: String,
    values: HashMap<String, DecodedValue>,
}

impl DecoderPlan {
    fn compile(
        channels: &ChannelsFile,
        structure: &StructureFile,
        packets_catalog: Option<&PacketsCatalogFile>,
        structure_id: String,
    ) -> Result<Self> {
        let mut channel_types = HashMap::new();
        for channel in &channels.channels {
            let ty = ChannelType::parse(&channel.ty)
                .ok_or_else(|| anyhow!("unsupported EA WRC channel type '{}'", channel.ty))?;
            channel_types.insert(channel.id.clone(), ty);
        }

        let mut layouts = HashMap::new();
        for packet in &structure.packets {
            let mut order = packet.header.channels.clone();
            order.extend(packet.channels.iter().cloned());

            let mut channels = Vec::with_capacity(order.len());
            let mut offset = 0usize;
            for channel_id in order {
                let ty = channel_types.get(&channel_id).copied().ok_or_else(|| {
                    anyhow!("unknown channel '{}' in packet '{}'", channel_id, packet.id)
                })?;
                channels.push(ChannelLayout {
                    id: channel_id,
                    ty,
                    offset,
                });
                offset = offset
                    .checked_add(ty.width())
                    .ok_or_else(|| anyhow!("packet layout overflow"))?;
            }

            layouts.insert(
                packet.id.clone(),
                PacketLayout {
                    channels,
                    total_size: offset,
                },
            );
        }

        if layouts.is_empty() {
            return Err(anyhow!("EA WRC structure file has no packet layouts"));
        }

        let packet_uid_to_id = packets_catalog
            .map(|catalog| {
                catalog
                    .packets
                    .iter()
                    .map(|packet| (packet.four_cc.clone(), packet.id.clone()))
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default();

        info!(
            schema = channels.versions.schema,
            data = channels.versions.data,
            structure_id = %structure_id,
            "Loaded EA WRC telemetry metadata"
        );

        Ok(Self {
            structure_id,
            layouts,
            packet_uid_to_id,
        })
    }

    fn decode_any(&self, packet_ids: &[String], raw: &[u8]) -> Result<DecodedPacket> {
        if packet_ids.is_empty() {
            return self.decode(DEFAULT_PACKET_ID, raw);
        }

        let mut last_error: Option<anyhow::Error> = None;
        for packet_id in packet_ids {
            match self.decode(packet_id, raw) {
                Ok(packet) => return Ok(packet),
                Err(error) => last_error = Some(error),
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow!("unable to decode packet for any known layout")))
    }

    fn decode(&self, packet_id: &str, raw: &[u8]) -> Result<DecodedPacket> {
        let layout = self
            .layouts
            .get(packet_id)
            .ok_or_else(|| anyhow!("no layout for packet '{}'", packet_id))?;

        if raw.len() < layout.total_size {
            return Err(anyhow!(
                "packet '{}' too short: expected at least {} bytes, got {}",
                packet_id,
                layout.total_size,
                raw.len()
            ));
        }

        if raw.len() > layout.total_size {
            debug!(
                packet_id = packet_id,
                expected_size = layout.total_size,
                actual_size = raw.len(),
                "EA WRC packet includes trailing bytes"
            );
        }

        let mut values = HashMap::with_capacity(layout.channels.len());
        for channel in &layout.channels {
            let value = DecodedValue::read(channel.ty, raw, channel.offset).with_context(|| {
                format!(
                    "failed to decode channel '{}' in packet '{}'",
                    channel.id, packet_id
                )
            })?;
            values.insert(channel.id.clone(), value);
        }

        if let Some(DecodedValue::String(packet_uid)) = values.get("packet_uid")
            && !self.packet_uid_to_id.is_empty()
        {
            let expected_packet_id = self.packet_uid_to_id.get(packet_uid).ok_or_else(|| {
                anyhow!(
                    "packet_uid '{}' is not present in packets catalog for structure '{}'",
                    packet_uid,
                    self.structure_id
                )
            })?;

            if expected_packet_id != packet_id {
                return Err(anyhow!(
                    "packet_uid '{}' maps to '{}', but layout '{}' was used",
                    packet_uid,
                    expected_packet_id,
                    packet_id
                ));
            }
        }

        Ok(DecodedPacket {
            packet_id: packet_id.to_string(),
            structure_id: self.structure_id.clone(),
            values,
        })
    }
}

#[derive(Debug, Clone, Copy)]
enum ChannelType {
    Bool,
    U8,
    U16,
    U32,
    U64,
    I8,
    I16,
    I32,
    I64,
    F32,
    F64,
    FourCC,
}

impl ChannelType {
    fn width(self) -> usize {
        match self {
            Self::Bool | Self::U8 | Self::I8 => 1,
            Self::U16 | Self::I16 => 2,
            Self::U32 | Self::I32 | Self::F32 | Self::FourCC => 4,
            Self::U64 | Self::I64 | Self::F64 => 8,
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        let normalized = raw.trim().to_ascii_lowercase().replace([' ', '-', '_'], "");
        let base = normalized.split('[').next().unwrap_or(&normalized);

        Some(match base {
            "bool" | "boolean" => Self::Bool,
            "u8" | "uint8" | "byte" => Self::U8,
            "u16" | "uint16" => Self::U16,
            "u32" | "uint32" => Self::U32,
            "u64" | "uint64" => Self::U64,
            "i8" | "int8" | "s8" => Self::I8,
            "i16" | "int16" | "s16" => Self::I16,
            "i32" | "int32" | "s32" => Self::I32,
            "i64" | "int64" | "s64" => Self::I64,
            "f32" | "float" | "single" => Self::F32,
            "f64" | "double" => Self::F64,
            "fourcc" | "packetuid" => Self::FourCC,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone)]
enum DecodedValue {
    Bool(bool),
    U64(u64),
    I64(i64),
    F64(f64),
    String(String),
}

impl DecodedValue {
    fn read(ty: ChannelType, raw: &[u8], offset: usize) -> Result<Self> {
        let width = ty.width();
        let end = offset
            .checked_add(width)
            .ok_or_else(|| anyhow!("packet offset overflow"))?;
        if end > raw.len() {
            return Err(anyhow!(
                "packet too short: need {} bytes at offset {}, packet has {} bytes",
                width,
                offset,
                raw.len()
            ));
        }

        let slice = &raw[offset..end];
        Ok(match ty {
            ChannelType::Bool => Self::Bool(slice[0] != 0),
            ChannelType::U8 => Self::U64(u64::from(slice[0])),
            ChannelType::U16 => {
                let mut b = [0u8; 2];
                b.copy_from_slice(slice);
                Self::U64(u64::from(u16::from_le_bytes(b)))
            }
            ChannelType::U32 => {
                let mut b = [0u8; 4];
                b.copy_from_slice(slice);
                Self::U64(u64::from(u32::from_le_bytes(b)))
            }
            ChannelType::U64 => {
                let mut b = [0u8; 8];
                b.copy_from_slice(slice);
                Self::U64(u64::from_le_bytes(b))
            }
            ChannelType::I8 => Self::I64(i64::from(i8::from_le_bytes([slice[0]]))),
            ChannelType::I16 => {
                let mut b = [0u8; 2];
                b.copy_from_slice(slice);
                Self::I64(i64::from(i16::from_le_bytes(b)))
            }
            ChannelType::I32 => {
                let mut b = [0u8; 4];
                b.copy_from_slice(slice);
                Self::I64(i64::from(i32::from_le_bytes(b)))
            }
            ChannelType::I64 => {
                let mut b = [0u8; 8];
                b.copy_from_slice(slice);
                Self::I64(i64::from_le_bytes(b))
            }
            ChannelType::F32 => {
                let mut b = [0u8; 4];
                b.copy_from_slice(slice);
                Self::F64(f32::from_le_bytes(b) as f64)
            }
            ChannelType::F64 => {
                let mut b = [0u8; 8];
                b.copy_from_slice(slice);
                Self::F64(f64::from_le_bytes(b))
            }
            ChannelType::FourCC => Self::String(String::from_utf8_lossy(slice).into_owned()),
        })
    }

    fn as_f32(&self) -> Option<f32> {
        match self {
            Self::Bool(value) => Some(if *value { 1.0 } else { 0.0 }),
            Self::U64(value) => Some(*value as f32),
            Self::I64(value) => Some(*value as f32),
            Self::F64(value) => Some(*value as f32),
            Self::String(_) => None,
        }
    }

    fn as_i8(&self) -> Option<i8> {
        match self {
            Self::Bool(value) => Some(if *value { 1 } else { 0 }),
            Self::U64(value) => i8::try_from(*value).ok(),
            Self::I64(value) => i8::try_from(*value).ok(),
            Self::F64(value) => i8::try_from(*value as i64).ok(),
            Self::String(_) => None,
        }
    }

    fn as_string(&self) -> Option<String> {
        match self {
            Self::String(value) => Some(value.clone()),
            Self::Bool(value) => Some(value.to_string()),
            Self::U64(value) => Some(value.to_string()),
            Self::I64(value) => Some(value.to_string()),
            Self::F64(value) => Some(value.to_string()),
        }
    }

    fn to_telemetry_value(&self) -> TelemetryValue {
        match self {
            Self::Bool(value) => TelemetryValue::Boolean(*value),
            Self::U64(value) => TelemetryValue::Integer((*value).min(i32::MAX as u64) as i32),
            Self::I64(value) => {
                TelemetryValue::Integer((*value).clamp(i32::MIN as i64, i32::MAX as i64) as i32)
            }
            Self::F64(value) => TelemetryValue::Float(*value as f32),
            Self::String(value) => TelemetryValue::String(value.clone()),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
struct ChannelsVersions {
    #[serde(default)]
    schema: Option<u32>,
    #[serde(default)]
    data: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct ChannelsFile {
    #[serde(default)]
    versions: ChannelsVersions,
    channels: Vec<ChannelDef>,
}

#[derive(Debug, Deserialize)]
struct ChannelDef {
    id: String,
    #[serde(rename = "type")]
    ty: String,
}

#[derive(Debug, Deserialize)]
struct StructureFile {
    #[serde(default, rename = "id")]
    _id: Option<String>,
    #[serde(default)]
    packets: Vec<PacketDef>,
}

#[derive(Debug, Deserialize)]
struct PacketDef {
    id: String,
    #[serde(default)]
    header: PacketHeader,
    #[serde(default)]
    channels: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
struct PacketHeader {
    #[serde(default)]
    channels: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct PacketsCatalogFile {
    #[serde(default)]
    packets: Vec<PacketCatalogEntry>,
}

#[derive(Debug, Deserialize)]
struct PacketCatalogEntry {
    id: String,
    #[serde(rename = "fourCC")]
    four_cc: String,
}

#[derive(Debug, Default, Deserialize)]
struct UdpConfig {
    #[serde(default, rename = "packetAssignments")]
    packet_assignments: Vec<ConfigAssignment>,
}

#[derive(Debug, Default, Deserialize)]
struct ConfigFile {
    #[serde(default)]
    udp: UdpConfig,
    #[serde(default, rename = "packetAssignments")]
    packet_assignments: Vec<ConfigAssignment>,
}

#[derive(Debug, Default, Deserialize)]
struct ConfigAssignment {
    #[serde(default, rename = "packetId")]
    packet_id: Option<String>,
    #[serde(default, rename = "structureId")]
    structure_id: Option<String>,
    #[serde(default)]
    port: Option<u16>,
    #[serde(default, rename = "bEnabled")]
    enabled: Option<bool>,
}

impl ConfigFile {
    fn all_assignments(&self) -> impl Iterator<Item = &ConfigAssignment> {
        self.udp
            .packet_assignments
            .iter()
            .chain(self.packet_assignments.iter())
    }

    fn structure_id(&self) -> Option<String> {
        self.all_assignments().find_map(|assignment| {
            assignment
                .structure_id
                .as_ref()
                .map(ToOwned::to_owned)
                .filter(|id| !id.is_empty())
        })
    }

    fn assignments_for_structure(&self, structure_id: &str) -> Vec<PacketAssignment> {
        self.all_assignments()
            .filter_map(|assignment| {
                if assignment.enabled == Some(false) {
                    return None;
                }

                let packet_id = assignment.packet_id.as_ref()?.clone();
                if packet_id.is_empty() {
                    return None;
                }

                if let Some(assignment_structure) = assignment.structure_id.as_ref()
                    && assignment_structure != structure_id
                {
                    return None;
                }

                Some(PacketAssignment {
                    packet_id,
                    port: assignment.port.unwrap_or(DEFAULT_EAWRC_PORT),
                })
            })
            .collect()
    }
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let content = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
}

fn read_optional_json<T: DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(read_json(path)?))
}

fn value_f32(values: &HashMap<String, DecodedValue>, aliases: &[&str]) -> Option<f32> {
    find_value(values, aliases).and_then(DecodedValue::as_f32)
}

fn value_i8(values: &HashMap<String, DecodedValue>, aliases: &[&str]) -> Option<i8> {
    find_value(values, aliases).and_then(DecodedValue::as_i8)
}

fn value_string(values: &HashMap<String, DecodedValue>, aliases: &[&str]) -> Option<String> {
    find_value(values, aliases).and_then(DecodedValue::as_string)
}

fn find_value<'a>(
    values: &'a HashMap<String, DecodedValue>,
    aliases: &[&str],
) -> Option<&'a DecodedValue> {
    for alias in aliases {
        if let Some(value) = values.get(*alias) {
            return Some(value);
        }

        if let Some((_, value)) = values
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(alias))
        {
            return Some(value);
        }
    }
    None
}

fn telemetry_root_from_environment() -> PathBuf {
    if let Ok(path) = std::env::var(TELEMETRY_DIR_OVERRIDE_ENV)
        && !path.trim().is_empty()
    {
        return PathBuf::from(path);
    }

    #[cfg(windows)]
    {
        if let Ok(user_profile) = std::env::var("USERPROFILE") {
            return PathBuf::from(user_profile)
                .join("Documents")
                .join("My Games")
                .join("WRC")
                .join("telemetry");
        }
    }

    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home)
            .join("Documents")
            .join("My Games")
            .join("WRC")
            .join("telemetry");
    }

    PathBuf::from("Documents/My Games/WRC/telemetry")
}

fn monotonic_ns_since(epoch: Instant, now: Instant) -> u64 {
    now.checked_duration_since(epoch)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0)
        .min(u64::MAX as u128) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn must<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
        match result {
            Ok(value) => value,
            Err(error) => panic!("unexpected Err: {error:?}"),
        }
    }

    fn channels_json() -> serde_json::Value {
        serde_json::json!({
            "versions": { "schema": 1, "data": 7 },
            "channels": [
                { "id": "packet_uid", "type": "fourCC" },
                { "id": "ffb_scalar", "type": "f32" },
                { "id": "engine_rpm", "type": "f32" },
                { "id": "vehicle_speed", "type": "f32" },
                { "id": "gear", "type": "i8" }
            ]
        })
    }

    fn structure_json() -> serde_json::Value {
        serde_json::json!({
            "id": "openracing",
            "packets": [
                {
                    "id": "session_update",
                    "header": { "channels": ["packet_uid"] },
                    "channels": ["ffb_scalar", "engine_rpm", "vehicle_speed", "gear"]
                }
            ]
        })
    }

    fn packets_json() -> serde_json::Value {
        serde_json::json!({
            "packets": [
                { "id": "session_update", "fourCC": "SU01" }
            ]
        })
    }

    #[test]
    fn test_decoder_compile_and_decode() -> TestResult {
        let channels: ChannelsFile = serde_json::from_value(channels_json())?;
        let structure: StructureFile = serde_json::from_value(structure_json())?;
        let catalog: PacketsCatalogFile = serde_json::from_value(packets_json())?;

        let plan = DecoderPlan::compile(
            &channels,
            &structure,
            Some(&catalog),
            "openracing".to_string(),
        )?;

        let mut packet = Vec::new();
        packet.extend_from_slice(b"SU01");
        packet.extend_from_slice(&0.6f32.to_le_bytes());
        packet.extend_from_slice(&6400.0f32.to_le_bytes());
        packet.extend_from_slice(&51.0f32.to_le_bytes());
        packet.extend_from_slice(&4i8.to_le_bytes());

        let decoded = plan.decode("session_update", &packet)?;
        let telemetry = EAWRCAdapter::normalize_decoded(&decoded);

        assert_eq!(telemetry.ffb_scalar, Some(0.6));
        assert_eq!(telemetry.rpm, Some(6400.0));
        assert_eq!(telemetry.speed_ms, Some(51.0));
        assert_eq!(telemetry.gear, Some(4));
        assert_eq!(
            telemetry.extended.get("packet_uid"),
            Some(&TelemetryValue::String("SU01".to_string()))
        );
        Ok(())
    }

    #[test]
    fn test_decoder_rejects_packet_uid_mismatch() -> TestResult {
        let channels: ChannelsFile = serde_json::from_value(channels_json())?;
        let structure: StructureFile = serde_json::from_value(structure_json())?;
        let catalog: PacketsCatalogFile = serde_json::from_value(packets_json())?;

        let plan = DecoderPlan::compile(
            &channels,
            &structure,
            Some(&catalog),
            "openracing".to_string(),
        )?;

        let mut packet = Vec::new();
        packet.extend_from_slice(b"BAD!");
        packet.extend_from_slice(&0.0f32.to_le_bytes());
        packet.extend_from_slice(&0.0f32.to_le_bytes());
        packet.extend_from_slice(&0.0f32.to_le_bytes());
        packet.extend_from_slice(&0i8.to_le_bytes());

        let result = plan.decode("session_update", &packet);
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn test_adapter_normalize_uses_runtime_schema_files() -> TestResult {
        let temp_dir = must(tempfile::tempdir());
        let telemetry_root = temp_dir.path().join("telemetry");
        let readme_dir = telemetry_root.join("readme");
        let udp_dir = telemetry_root.join("udp");

        must(fs::create_dir_all(&readme_dir));
        must(fs::create_dir_all(&udp_dir));

        must(fs::write(
            readme_dir.join("channels.json"),
            serde_json::to_vec_pretty(&channels_json())?,
        ));
        must(fs::write(
            readme_dir.join("packets.json"),
            serde_json::to_vec_pretty(&packets_json())?,
        ));
        must(fs::write(
            udp_dir.join("openracing.json"),
            serde_json::to_vec_pretty(&structure_json())?,
        ));
        must(fs::write(
            telemetry_root.join("config.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "udp": {
                    "packetAssignments": [
                        {
                            "packetId": "session_update",
                            "structureId": "openracing",
                            "port": 20778,
                            "bEnabled": true
                        }
                    ]
                }
            }))?,
        ));

        let adapter = EAWRCAdapter::with_telemetry_dir(telemetry_root);

        let mut packet = Vec::new();
        packet.extend_from_slice(b"SU01");
        packet.extend_from_slice(&0.25f32.to_le_bytes());
        packet.extend_from_slice(&5000.0f32.to_le_bytes());
        packet.extend_from_slice(&33.0f32.to_le_bytes());
        packet.extend_from_slice(&3i8.to_le_bytes());

        let telemetry = adapter.normalize(&packet)?;
        assert_eq!(telemetry.ffb_scalar, Some(0.25));
        assert_eq!(telemetry.rpm, Some(5000.0));
        assert_eq!(telemetry.speed_ms, Some(33.0));
        assert_eq!(telemetry.gear, Some(3));
        Ok(())
    }
}

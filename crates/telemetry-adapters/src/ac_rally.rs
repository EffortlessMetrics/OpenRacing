//! Assetto Corsa Rally telemetry discovery adapter.
//!
//! AC Rally telemetry transport is not publicly specified, so this adapter is
//! discovery-first:
//! 1) Try ACC-style UDP registration handshake on a configurable endpoint.
//! 2) Run a passive UDP capture window on a configurable local bind address.
//! 3) Emit probe diagnostics as normalized telemetry `extended` fields.

use crate::{
    NormalizedTelemetry, TelemetryAdapter, TelemetryFrame, TelemetryReceiver, TelemetryValue,
    telemetry_now_ns,
};
use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::time::{Duration, Instant};
use tokio::net::UdpSocket as TokioUdpSocket;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

const REGISTER_COMMAND_APPLICATION: u8 = 1;
const PROTOCOL_VERSION: u8 = 4;
const MSG_REGISTRATION_RESULT: u8 = 1;

const DEFAULT_AC_RALLY_ENDPOINT_PORT: u16 = 9000;
const DEFAULT_AC_RALLY_PASSIVE_PORT: u16 = 9000;
const MAX_PACKET_SIZE: usize = 4096;
const DEFAULT_HANDSHAKE_TIMEOUT_MS: u64 = 400;
const DEFAULT_PASSIVE_PROBE_WINDOW_MS: u64 = 2_000;

const ENV_AC_RALLY_ENDPOINT: &str = "OPENRACING_AC_RALLY_ENDPOINT";
const ENV_AC_RALLY_PASSIVE_PORT: &str = "OPENRACING_AC_RALLY_PASSIVE_PORT";
const ENV_AC_RALLY_HANDSHAKE_TIMEOUT_MS: &str = "OPENRACING_AC_RALLY_HANDSHAKE_TIMEOUT_MS";
const ENV_AC_RALLY_PASSIVE_WINDOW_MS: &str = "OPENRACING_AC_RALLY_PASSIVE_WINDOW_MS";

#[derive(Debug, Clone)]
pub struct ACRallyAdapter {
    handshake_endpoint: SocketAddr,
    passive_bind_address: SocketAddr,
    update_rate: Duration,
    handshake_timeout: Duration,
    passive_probe_window: Duration,
}

impl Default for ACRallyAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl ACRallyAdapter {
    pub fn new() -> Self {
        let handshake_endpoint = parse_socket_addr_env(
            ENV_AC_RALLY_ENDPOINT,
            SocketAddr::V4(SocketAddrV4::new(
                Ipv4Addr::LOCALHOST,
                DEFAULT_AC_RALLY_ENDPOINT_PORT,
            )),
        );
        let passive_port = parse_u16_env(ENV_AC_RALLY_PASSIVE_PORT, DEFAULT_AC_RALLY_PASSIVE_PORT);
        let passive_bind_address =
            SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, passive_port));
        let handshake_timeout = Duration::from_millis(parse_u64_env(
            ENV_AC_RALLY_HANDSHAKE_TIMEOUT_MS,
            DEFAULT_HANDSHAKE_TIMEOUT_MS,
        ));
        let passive_probe_window = Duration::from_millis(parse_u64_env(
            ENV_AC_RALLY_PASSIVE_WINDOW_MS,
            DEFAULT_PASSIVE_PROBE_WINDOW_MS,
        ));

        Self {
            handshake_endpoint,
            passive_bind_address,
            update_rate: Duration::from_millis(16),
            handshake_timeout,
            passive_probe_window,
        }
    }

    pub fn with_probe_addresses(
        handshake_endpoint: SocketAddr,
        passive_bind_address: SocketAddr,
    ) -> Self {
        Self {
            handshake_endpoint,
            passive_bind_address,
            ..Self::new()
        }
    }
}

#[async_trait]
impl TelemetryAdapter for ACRallyAdapter {
    fn game_id(&self) -> &str {
        "ac_rally"
    }

    async fn start_monitoring(&self) -> Result<TelemetryReceiver> {
        let (tx, rx) = mpsc::channel(64);
        let adapter = self.clone();

        tokio::spawn(async move {
            let mut frame_seq = 0u64;

            let handshake = probe_udp_handshake(
                adapter.handshake_endpoint,
                adapter.handshake_timeout,
                adapter.update_rate,
            )
            .await;

            let handshake_telemetry =
                telemetry_from_handshake(&handshake, adapter.handshake_endpoint);
            if !send_probe_frame(
                &tx,
                &mut frame_seq,
                handshake_telemetry,
                handshake.raw_size(),
            )
            .await
            {
                return;
            }

            match &handshake {
                HandshakeProbeOutcome::Registration(result) => {
                    info!(
                        endpoint = %adapter.handshake_endpoint,
                        success = result.success,
                        readonly = result.readonly,
                        "AC Rally handshake probe completed"
                    );
                }
                HandshakeProbeOutcome::Response {
                    message_type,
                    raw_size,
                } => {
                    debug!(
                        endpoint = %adapter.handshake_endpoint,
                        message_type = *message_type,
                        raw_size = *raw_size,
                        "AC Rally handshake probe received non-registration response"
                    );
                }
                HandshakeProbeOutcome::Timeout => {
                    debug!(
                        endpoint = %adapter.handshake_endpoint,
                        "AC Rally handshake probe timed out"
                    );
                }
                HandshakeProbeOutcome::Error { message } => {
                    warn!(
                        endpoint = %adapter.handshake_endpoint,
                        error = %message,
                        "AC Rally handshake probe failed"
                    );
                }
            }

            run_passive_udp_probe(
                &tx,
                &mut frame_seq,
                adapter.passive_bind_address,
                adapter.passive_probe_window,
                adapter.update_rate,
            )
            .await;
        });

        Ok(rx)
    }

    async fn stop_monitoring(&self) -> Result<()> {
        Ok(())
    }

    fn normalize(&self, raw: &[u8]) -> Result<NormalizedTelemetry> {
        normalize_probe_packet(raw)
    }

    fn expected_update_rate(&self) -> Duration {
        self.update_rate
    }

    async fn is_game_running(&self) -> Result<bool> {
        let outcome = probe_udp_handshake(
            self.handshake_endpoint,
            self.handshake_timeout,
            self.update_rate,
        )
        .await;
        Ok(matches!(
            outcome,
            HandshakeProbeOutcome::Registration(_) | HandshakeProbeOutcome::Response { .. }
        ))
    }
}

#[derive(Debug, Clone)]
enum HandshakeProbeOutcome {
    Registration(RegistrationResult),
    Response { message_type: u8, raw_size: usize },
    Timeout,
    Error { message: String },
}

impl HandshakeProbeOutcome {
    fn raw_size(&self) -> usize {
        match self {
            HandshakeProbeOutcome::Registration(result) => result.raw_size,
            HandshakeProbeOutcome::Response { raw_size, .. } => *raw_size,
            HandshakeProbeOutcome::Timeout | HandshakeProbeOutcome::Error { .. } => 0,
        }
    }
}

#[derive(Debug, Clone)]
struct RegistrationResult {
    connection_id: i32,
    success: bool,
    readonly: bool,
    error: String,
    raw_size: usize,
}

async fn probe_udp_handshake(
    endpoint: SocketAddr,
    timeout_duration: Duration,
    update_rate: Duration,
) -> HandshakeProbeOutcome {
    let bind_address = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0));
    let socket = match TokioUdpSocket::bind(bind_address).await {
        Ok(socket) => socket,
        Err(error) => {
            return HandshakeProbeOutcome::Error {
                message: format!("bind failed: {error}"),
            };
        }
    };

    if let Err(error) = socket.connect(endpoint).await {
        return HandshakeProbeOutcome::Error {
            message: format!("connect failed: {error}"),
        };
    }

    let packet = match build_register_packet("OpenRacing AC Rally Probe", "", update_rate, "") {
        Ok(packet) => packet,
        Err(error) => {
            return HandshakeProbeOutcome::Error {
                message: format!("register packet encoding failed: {error}"),
            };
        }
    };

    if let Err(error) = socket.send(&packet).await {
        return HandshakeProbeOutcome::Error {
            message: format!("register send failed: {error}"),
        };
    }

    let mut buf = [0u8; MAX_PACKET_SIZE];
    let recv = tokio::time::timeout(timeout_duration, socket.recv(&mut buf)).await;
    let len = match recv {
        Ok(Ok(len)) => len,
        Ok(Err(error)) => {
            return HandshakeProbeOutcome::Error {
                message: format!("receive failed: {error}"),
            };
        }
        Err(_) => return HandshakeProbeOutcome::Timeout,
    };

    if let Ok(result) = parse_registration_result(&buf[..len]) {
        return HandshakeProbeOutcome::Registration(RegistrationResult {
            raw_size: len,
            ..result
        });
    }

    HandshakeProbeOutcome::Response {
        message_type: buf[0],
        raw_size: len,
    }
}

async fn run_passive_udp_probe(
    tx: &mpsc::Sender<TelemetryFrame>,
    frame_seq: &mut u64,
    bind_address: SocketAddr,
    probe_window: Duration,
    update_rate: Duration,
) {
    let socket = match TokioUdpSocket::bind(bind_address).await {
        Ok(socket) => socket,
        Err(error) => {
            let telemetry = NormalizedTelemetry::builder()
                .extended(
                    "probe_stage".to_string(),
                    TelemetryValue::String("udp_passive".to_string()),
                )
                .extended(
                    "probe_status".to_string(),
                    TelemetryValue::String("bind_error".to_string()),
                )
                .extended(
                    "probe_error".to_string(),
                    TelemetryValue::String(error.to_string()),
                )
                .extended(
                    "probe_bind".to_string(),
                    TelemetryValue::String(bind_address.to_string()),
                )
                .build();
            let _ = send_probe_frame(tx, frame_seq, telemetry, 0).await;
            return;
        }
    };

    info!(
        bind = %bind_address,
        window_ms = probe_window.as_millis(),
        "AC Rally passive UDP probe started"
    );

    let probe_deadline = Instant::now() + probe_window;
    let mut packets_seen = 0u64;
    let mut buf = [0u8; MAX_PACKET_SIZE];

    while Instant::now() < probe_deadline {
        let remaining = probe_deadline.saturating_duration_since(Instant::now());
        let timeout = remaining.min(update_rate.saturating_mul(4));
        let recv = tokio::time::timeout(timeout, socket.recv_from(&mut buf)).await;

        match recv {
            Ok(Ok((len, source))) => {
                packets_seen = packets_seen.saturating_add(1);
                let telemetry = match normalize_probe_packet(&buf[..len]) {
                    Ok(base) => base,
                    Err(error) => {
                        debug!(error = %error, "AC Rally passive packet normalization failed");
                        continue;
                    }
                };

                let telemetry = telemetry
                    .with_extended(
                        "probe_stage".to_string(),
                        TelemetryValue::String("udp_passive".to_string()),
                    )
                    .with_extended(
                        "probe_status".to_string(),
                        TelemetryValue::String("packet_received".to_string()),
                    )
                    .with_extended(
                        "probe_source".to_string(),
                        TelemetryValue::String(source.to_string()),
                    )
                    .with_extended(
                        "probe_bind".to_string(),
                        TelemetryValue::String(bind_address.to_string()),
                    );

                if !send_probe_frame(tx, frame_seq, telemetry, len).await {
                    return;
                }
            }
            Ok(Err(error)) => {
                let telemetry = NormalizedTelemetry::builder()
                    .extended(
                        "probe_stage".to_string(),
                        TelemetryValue::String("udp_passive".to_string()),
                    )
                    .extended(
                        "probe_status".to_string(),
                        TelemetryValue::String("receive_error".to_string()),
                    )
                    .extended(
                        "probe_error".to_string(),
                        TelemetryValue::String(error.to_string()),
                    )
                    .extended(
                        "probe_bind".to_string(),
                        TelemetryValue::String(bind_address.to_string()),
                    )
                    .build();
                if !send_probe_frame(tx, frame_seq, telemetry, 0).await {
                    return;
                }
            }
            Err(_) => {
                // Probe window uses periodic timeout polling.
            }
        }
    }

    if packets_seen == 0 {
        let telemetry = NormalizedTelemetry::builder()
            .extended(
                "probe_stage".to_string(),
                TelemetryValue::String("udp_passive".to_string()),
            )
            .extended(
                "probe_status".to_string(),
                TelemetryValue::String("no_packets".to_string()),
            )
            .extended(
                "probe_bind".to_string(),
                TelemetryValue::String(bind_address.to_string()),
            )
            .build();
        let _ = send_probe_frame(tx, frame_seq, telemetry, 0).await;
    }
}

fn telemetry_from_handshake(
    outcome: &HandshakeProbeOutcome,
    endpoint: SocketAddr,
) -> NormalizedTelemetry {
    let mut builder = NormalizedTelemetry::builder()
        .extended(
            "probe_stage".to_string(),
            TelemetryValue::String("udp_handshake".to_string()),
        )
        .extended(
            "probe_endpoint".to_string(),
            TelemetryValue::String(endpoint.to_string()),
        );

    match outcome {
        HandshakeProbeOutcome::Registration(result) => {
            builder = builder
                .extended(
                    "probe_status".to_string(),
                    TelemetryValue::String("registration_result".to_string()),
                )
                .extended(
                    "registration_success".to_string(),
                    TelemetryValue::Boolean(result.success),
                )
                .extended(
                    "registration_readonly".to_string(),
                    TelemetryValue::Boolean(result.readonly),
                )
                .extended(
                    "registration_connection_id".to_string(),
                    TelemetryValue::Integer(result.connection_id),
                )
                .extended(
                    "registration_error".to_string(),
                    TelemetryValue::String(result.error.clone()),
                );
        }
        HandshakeProbeOutcome::Response {
            message_type,
            raw_size,
        } => {
            builder = builder
                .extended(
                    "probe_status".to_string(),
                    TelemetryValue::String("unexpected_response".to_string()),
                )
                .extended(
                    "response_message_type".to_string(),
                    TelemetryValue::Integer(i32::from(*message_type)),
                )
                .extended(
                    "response_size".to_string(),
                    TelemetryValue::Integer(capped_i32(*raw_size)),
                );
        }
        HandshakeProbeOutcome::Timeout => {
            builder = builder.extended(
                "probe_status".to_string(),
                TelemetryValue::String("timeout".to_string()),
            );
        }
        HandshakeProbeOutcome::Error { message } => {
            builder = builder
                .extended(
                    "probe_status".to_string(),
                    TelemetryValue::String("error".to_string()),
                )
                .extended(
                    "probe_error".to_string(),
                    TelemetryValue::String(message.clone()),
                );
        }
    }

    builder.build()
}

async fn send_probe_frame(
    tx: &mpsc::Sender<TelemetryFrame>,
    frame_seq: &mut u64,
    telemetry: NormalizedTelemetry,
    raw_size: usize,
) -> bool {
    let frame = TelemetryFrame::new(telemetry, telemetry_now_ns(), *frame_seq, raw_size);

    if tx.send(frame).await.is_err() {
        return false;
    }

    *frame_seq = frame_seq.saturating_add(1);
    true
}

fn normalize_probe_packet(raw: &[u8]) -> Result<NormalizedTelemetry> {
    if raw.is_empty() {
        return Err(anyhow!("AC Rally probe packet is empty"));
    }

    let mut builder = NormalizedTelemetry::builder()
        .extended(
            "probe_status".to_string(),
            TelemetryValue::String("raw_packet".to_string()),
        )
        .extended(
            "raw_size".to_string(),
            TelemetryValue::Integer(capped_i32(raw.len())),
        )
        .extended(
            "raw_preview_hex".to_string(),
            TelemetryValue::String(hex_preview(raw, 32)),
        )
        .extended(
            "raw_first_byte".to_string(),
            TelemetryValue::Integer(i32::from(raw[0])),
        );

    if let Ok(text) = std::str::from_utf8(raw) {
        builder = builder.extended(
            "raw_utf8_preview".to_string(),
            TelemetryValue::String(text.chars().take(32).collect()),
        );
    }

    Ok(builder.build())
}

fn build_register_packet(
    display_name: &str,
    connection_password: &str,
    update_rate: Duration,
    command_password: &str,
) -> Result<Vec<u8>> {
    let interval_ms = update_rate
        .as_millis()
        .try_into()
        .unwrap_or(i32::MAX)
        .max(1);

    let mut packet = Vec::with_capacity(128);
    packet.push(REGISTER_COMMAND_APPLICATION);
    packet.push(PROTOCOL_VERSION);
    write_acc_string(&mut packet, display_name)?;
    write_acc_string(&mut packet, connection_password)?;
    packet.extend_from_slice(&interval_ms.to_le_bytes());
    write_acc_string(&mut packet, command_password)?;
    Ok(packet)
}

fn parse_registration_result(data: &[u8]) -> Result<RegistrationResult> {
    let mut reader = PacketReader::new(data);
    let message_type = reader.read_u8()?;
    if message_type != MSG_REGISTRATION_RESULT {
        return Err(anyhow!(
            "unexpected message type {message_type}, expected {MSG_REGISTRATION_RESULT}"
        ));
    }

    Ok(RegistrationResult {
        connection_id: reader.read_i32_le()?,
        success: reader.read_bool_u8()?,
        readonly: reader.read_bool_u8()?,
        error: read_acc_string(&mut reader)?,
        raw_size: data.len(),
    })
}

fn write_acc_string(buffer: &mut Vec<u8>, value: &str) -> Result<()> {
    let bytes = value.as_bytes();
    let length = u16::try_from(bytes.len())
        .map_err(|_| anyhow!("probe string length exceeds u16: {} bytes", bytes.len()))?;
    buffer.extend_from_slice(&length.to_le_bytes());
    buffer.extend_from_slice(bytes);
    Ok(())
}

fn read_acc_string(reader: &mut PacketReader<'_>) -> Result<String> {
    let length = usize::from(reader.read_u16_le()?);
    let raw = reader.read_exact(length)?;
    String::from_utf8(raw.to_vec()).context("probe string contains invalid UTF-8")
}

fn parse_socket_addr_env(env_key: &str, fallback: SocketAddr) -> SocketAddr {
    std::env::var(env_key)
        .ok()
        .and_then(|value| value.parse::<SocketAddr>().ok())
        .unwrap_or(fallback)
}

fn parse_u16_env(env_key: &str, fallback: u16) -> u16 {
    std::env::var(env_key)
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(fallback)
}

fn parse_u64_env(env_key: &str, fallback: u64) -> u64 {
    std::env::var(env_key)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(fallback)
}

fn hex_preview(data: &[u8], max_bytes: usize) -> String {
    let preview_len = data.len().min(max_bytes);
    let mut output = String::with_capacity(preview_len * 2);
    for byte in &data[..preview_len] {
        output.push(nibble_to_hex(byte >> 4));
        output.push(nibble_to_hex(byte & 0x0f));
    }
    output
}

fn nibble_to_hex(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        10..=15 => (b'a' + (value - 10)) as char,
        _ => '0',
    }
}

fn capped_i32(value: usize) -> i32 {
    value.min(i32::MAX as usize) as i32
}

struct PacketReader<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> PacketReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    fn read_exact(&mut self, len: usize) -> Result<&'a [u8]> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or_else(|| anyhow!("packet offset overflow"))?;
        if end > self.data.len() {
            return Err(anyhow!(
                "packet too short: need {len} bytes at offset {}, total {}",
                self.offset,
                self.data.len()
            ));
        }

        let slice = &self.data[self.offset..end];
        self.offset = end;
        Ok(slice)
    }

    fn read_u8(&mut self) -> Result<u8> {
        Ok(self.read_exact(1)?[0])
    }

    fn read_bool_u8(&mut self) -> Result<bool> {
        Ok(self.read_u8()? != 0)
    }

    fn read_u16_le(&mut self) -> Result<u16> {
        let bytes = self.read_exact(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    fn read_i32_le(&mut self) -> Result<i32> {
        let bytes = self.read_exact(4)?;
        Ok(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn test_ac_rally_adapter_defaults() {
        let adapter = ACRallyAdapter::new();
        assert_eq!(adapter.game_id(), "ac_rally");
        assert_eq!(adapter.handshake_endpoint.port(), 9000);
        assert_eq!(adapter.passive_bind_address.port(), 9000);
    }

    #[test]
    fn test_build_register_packet_layout() -> TestResult {
        let packet = build_register_packet("OpenRacing", "pw", Duration::from_millis(16), "cmd")?;
        let mut reader = PacketReader::new(&packet);

        assert_eq!(reader.read_u8()?, REGISTER_COMMAND_APPLICATION);
        assert_eq!(reader.read_u8()?, PROTOCOL_VERSION);
        assert_eq!(read_acc_string(&mut reader)?, "OpenRacing");
        assert_eq!(read_acc_string(&mut reader)?, "pw");
        assert_eq!(reader.read_i32_le()?, 16);
        assert_eq!(read_acc_string(&mut reader)?, "cmd");
        Ok(())
    }

    #[test]
    fn test_parse_registration_result_packet() -> TestResult {
        let mut packet = Vec::new();
        packet.push(MSG_REGISTRATION_RESULT);
        packet.extend_from_slice(&42i32.to_le_bytes());
        packet.push(1);
        packet.push(0);
        write_acc_string(&mut packet, "ok")?;

        let result = parse_registration_result(&packet)?;
        assert_eq!(result.connection_id, 42);
        assert!(result.success);
        assert!(!result.readonly);
        assert_eq!(result.error, "ok");
        Ok(())
    }

    #[test]
    fn test_normalize_probe_packet_contains_debug_fields() -> TestResult {
        let raw = [0x11u8, 0x22, 0x33, 0x44];
        let telemetry = normalize_probe_packet(&raw)?;

        assert_eq!(
            telemetry.extended.get("raw_size"),
            Some(&TelemetryValue::Integer(4))
        );
        assert_eq!(
            telemetry.extended.get("raw_first_byte"),
            Some(&TelemetryValue::Integer(0x11))
        );
        Ok(())
    }

    #[test]
    fn test_normalize_probe_packet_rejects_empty_data() {
        let result = normalize_probe_packet(&[]);
        assert!(result.is_err());
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
            let adapter = ACRallyAdapter::new();
            let _ = adapter.normalize(&data);
        }
    }
}

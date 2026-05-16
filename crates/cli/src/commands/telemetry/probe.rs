use super::{
    MAX_PACKET_SIZE, MSG_REGISTRATION_RESULT, PROTOCOL_VERSION, REGISTER_COMMAND_APPLICATION,
};
use crate::error::CliError;
use anyhow::{Context, Result, anyhow};
use serde::Serialize;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;

#[derive(Debug, Serialize)]
struct ProbeAttempt {
    attempt: u32,
    status: String,
    elapsed_ms: u64,
    response_size: usize,
    message_type: Option<u8>,
    registration_connection_id: Option<i32>,
    registration_success: Option<bool>,
    registration_readonly: Option<bool>,
    registration_error: Option<String>,
    pub(super) error: Option<String>,
}

#[derive(Debug, Serialize)]
struct ProbeSummary {
    game_id: String,
    endpoint: String,
    attempts: u32,
    any_response: bool,
    attempts_detail: Vec<ProbeAttempt>,
}

pub(super) async fn probe(
    game_id: &str,
    endpoint: &str,
    timeout_ms: u64,
    attempts: u32,
    json: bool,
) -> Result<()> {
    ensure_probe_game(game_id)?;
    let endpoint_addr: SocketAddr = endpoint.parse().map_err(|error| {
        CliError::InvalidConfiguration(format!("Invalid --endpoint '{}': {}", endpoint, error))
    })?;

    let timeout = Duration::from_millis(timeout_ms.max(1));
    let total_attempts = attempts.max(1);
    let mut detail = Vec::with_capacity(total_attempts as usize);
    let mut any_response = false;

    for attempt in 1..=total_attempts {
        let started = Instant::now();
        let result = probe_once(endpoint_addr, timeout).await;
        let elapsed_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;

        let probe_attempt = match result {
            Ok(ProbeOutcome::Registration(result)) => {
                any_response = true;
                ProbeAttempt {
                    attempt,
                    status: "registration_result".to_string(),
                    elapsed_ms,
                    response_size: result.raw_size,
                    message_type: Some(MSG_REGISTRATION_RESULT),
                    registration_connection_id: Some(result.connection_id),
                    registration_success: Some(result.success),
                    registration_readonly: Some(result.readonly),
                    registration_error: Some(result.error),
                    error: None,
                }
            }
            Ok(ProbeOutcome::Response {
                message_type,
                raw_size,
            }) => {
                any_response = true;
                ProbeAttempt {
                    attempt,
                    status: "response".to_string(),
                    elapsed_ms,
                    response_size: raw_size,
                    message_type: Some(message_type),
                    registration_connection_id: None,
                    registration_success: None,
                    registration_readonly: None,
                    registration_error: None,
                    error: None,
                }
            }
            Ok(ProbeOutcome::Timeout) => ProbeAttempt {
                attempt,
                status: "timeout".to_string(),
                elapsed_ms,
                response_size: 0,
                message_type: None,
                registration_connection_id: None,
                registration_success: None,
                registration_readonly: None,
                registration_error: None,
                error: None,
            },
            Err(error) => ProbeAttempt {
                attempt,
                status: "error".to_string(),
                elapsed_ms,
                response_size: 0,
                message_type: None,
                registration_connection_id: None,
                registration_success: None,
                registration_readonly: None,
                registration_error: None,
                error: Some(error.to_string()),
            },
        };

        detail.push(probe_attempt);
    }

    let summary = ProbeSummary {
        game_id: game_id.to_string(),
        endpoint: endpoint_addr.to_string(),
        attempts: total_attempts,
        any_response,
        attempts_detail: detail,
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&summary)?);
    } else {
        println!(
            "Telemetry probe for {} at {}",
            summary.game_id, summary.endpoint
        );
        println!("Attempts: {}", summary.attempts);
        println!("Any response: {}", summary.any_response);
        for attempt in &summary.attempts_detail {
            println!(
                "  attempt {} -> {} ({} ms)",
                attempt.attempt, attempt.status, attempt.elapsed_ms
            );
            if let Some(error) = &attempt.error {
                println!("    error: {}", error);
            }
            if let Some(message_type) = attempt.message_type {
                println!("    message_type: {}", message_type);
            }
            if let Some(connection_id) = attempt.registration_connection_id {
                println!("    registration_connection_id: {}", connection_id);
            }
            if let Some(success) = attempt.registration_success {
                println!("    registration_success: {}", success);
            }
            if let Some(readonly) = attempt.registration_readonly {
                println!("    registration_readonly: {}", readonly);
            }
            if let Some(error) = &attempt.registration_error
                && !error.is_empty()
            {
                println!("    registration_error: {}", error);
            }
        }
    }

    Ok(())
}
pub(super) fn ensure_probe_game(game_id: &str) -> Result<()> {
    let allowed = ["acc", "ac_rally"];
    if allowed.iter().any(|id| id == &game_id) {
        return Ok(());
    }

    Err(CliError::InvalidConfiguration(format!(
        "Telemetry probe currently supports: {}",
        allowed.join(", ")
    ))
    .into())
}

enum ProbeOutcome {
    Registration(RegistrationResult),
    Response { message_type: u8, raw_size: usize },
    Timeout,
}

#[derive(Debug)]
pub(super) struct RegistrationResult {
    pub(super) connection_id: i32,
    pub(super) success: bool,
    pub(super) readonly: bool,
    pub(super) error: String,
    pub(super) raw_size: usize,
}

async fn probe_once(endpoint: SocketAddr, timeout: Duration) -> Result<ProbeOutcome> {
    let socket = UdpSocket::bind(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0)))
        .await
        .context("probe bind failed")?;
    socket
        .connect(endpoint)
        .await
        .context("probe connect failed")?;

    let packet = build_register_packet("OpenRacing Probe", "", Duration::from_millis(16), "")?;
    socket.send(&packet).await.context("probe send failed")?;

    let mut buf = [0u8; MAX_PACKET_SIZE];
    let recv = tokio::time::timeout(timeout, socket.recv(&mut buf)).await;
    let len = match recv {
        Ok(Ok(len)) => len,
        Ok(Err(error)) => return Err(anyhow!("probe receive failed: {}", error)),
        Err(_) => return Ok(ProbeOutcome::Timeout),
    };

    if let Ok(result) = parse_registration_result(&buf[..len]) {
        return Ok(ProbeOutcome::Registration(RegistrationResult {
            raw_size: len,
            ..result
        }));
    }

    Ok(ProbeOutcome::Response {
        message_type: buf[0],
        raw_size: len,
    })
}

pub(super) fn build_register_packet(
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

pub(super) fn parse_registration_result(data: &[u8]) -> Result<RegistrationResult> {
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

pub(super) fn write_acc_string(buffer: &mut Vec<u8>, value: &str) -> Result<()> {
    let bytes = value.as_bytes();
    let length = u16::try_from(bytes.len())
        .map_err(|_| anyhow!("probe string too long: {} bytes", bytes.len()))?;
    buffer.extend_from_slice(&length.to_le_bytes());
    buffer.extend_from_slice(bytes);
    Ok(())
}

pub(super) fn read_acc_string(reader: &mut PacketReader<'_>) -> Result<String> {
    let len = usize::from(reader.read_u16_le()?);
    let raw = reader.read_exact(len)?;
    String::from_utf8(raw.to_vec()).context("probe string is not valid UTF-8")
}

pub(super) struct PacketReader<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> PacketReader<'a> {
    pub(super) fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    pub(super) fn read_exact(&mut self, len: usize) -> Result<&'a [u8]> {
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

    pub(super) fn read_u8(&mut self) -> Result<u8> {
        Ok(self.read_exact(1)?[0])
    }

    pub(super) fn read_bool_u8(&mut self) -> Result<bool> {
        Ok(self.read_u8()? != 0)
    }

    pub(super) fn read_u16_le(&mut self) -> Result<u16> {
        let bytes = self.read_exact(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    pub(super) fn read_i32_le(&mut self) -> Result<i32> {
        let bytes = self.read_exact(4)?;
        Ok(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }
}

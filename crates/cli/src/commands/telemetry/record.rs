use super::shared::{
    default_recorder_session_id, validated_normalized_snapshots, write_jsonl_values,
};
use super::{MAX_PACKET_SIZE, RECORD_COMMAND};
use crate::error::CliError;
use anyhow::{Context, Result, anyhow};
use racing_wheel_telemetry_adapters::simhub::parse_simhub_packet;
use serde::Serialize;
use serde_json::Value;
use std::fs::File;
use std::io::Write;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::path::Path;
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;

#[derive(Debug, Serialize)]
struct RecordSummary {
    command: &'static str,
    game: String,
    telemetry_source: String,
    input: String,
    output: String,
    recorder_session_id: String,
    normalized_snapshot_count: u64,
    duration_ms: u64,
    hardware_output_enabled: bool,
    no_hid_device_opened: bool,
    no_ffb_writes: bool,
    no_serial_config_commands: bool,
    no_firmware_or_dfu_commands: bool,
}

#[derive(Debug, Serialize)]
struct LiveRecordSummary {
    command: &'static str,
    game: String,
    telemetry_source: String,
    input: String,
    output: String,
    recorder_session_id: String,
    normalized_snapshot_count: u64,
    duration_ms: u64,
    packets_received: u64,
    bytes_received: u64,
    parse_errors: u64,
    hardware_output_enabled: bool,
    no_hid_device_opened: bool,
    no_ffb_writes: bool,
    no_serial_config_commands: bool,
    no_firmware_or_dfu_commands: bool,
}

pub(super) async fn record_normalized_snapshots(
    game_id: &str,
    telemetry_source: &str,
    input_path: &str,
    output_path: &str,
    session_id: Option<&str>,
    duration_ms: u64,
    json: bool,
) -> Result<()> {
    validate_record_metadata(game_id, telemetry_source)?;
    let session_id = session_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| default_recorder_session_id(game_id));

    let mut snapshots = validated_normalized_snapshots(input_path)?;
    for snapshot in &mut snapshots {
        stamp_record_provenance(
            snapshot,
            game_id,
            telemetry_source,
            &session_id,
            duration_ms,
        )?;
    }

    if let Some(parent) = Path::new(output_path)
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create '{}'", parent.display()))?;
    }
    let mut file = File::create(output_path)
        .with_context(|| format!("failed to create recorder output '{}'", output_path))?;

    for snapshot in &snapshots {
        let line = serde_json::to_string(&snapshot)?;
        file.write_all(line.as_bytes())?;
        file.write_all(b"\n")?;
    }
    file.flush()?;

    let normalized_snapshot_count =
        u64::try_from(snapshots.len()).context("too many normalized telemetry records")?;
    let summary = RecordSummary {
        command: RECORD_COMMAND,
        game: game_id.to_string(),
        telemetry_source: telemetry_source.to_string(),
        input: input_path.to_string(),
        output: output_path.to_string(),
        recorder_session_id: session_id,
        normalized_snapshot_count,
        duration_ms,
        hardware_output_enabled: false,
        no_hid_device_opened: true,
        no_ffb_writes: true,
        no_serial_config_commands: true,
        no_firmware_or_dfu_commands: true,
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&summary)?);
    } else {
        println!("Telemetry recording complete");
        println!("  game: {}", summary.game);
        println!("  telemetry_source: {}", summary.telemetry_source);
        println!("  snapshots: {}", summary.normalized_snapshot_count);
        println!("  session: {}", summary.recorder_session_id);
        println!("  output: {}", summary.output);
    }

    Ok(())
}

pub(super) async fn record_live_simhub_snapshots(
    game_id: &str,
    telemetry_source: &str,
    port: u16,
    output_path: &str,
    session_id: Option<&str>,
    duration_ms: u64,
    json: bool,
) -> Result<()> {
    validate_record_metadata(game_id, telemetry_source)?;
    if telemetry_source != "simhub_bridge" {
        return Err(CliError::InvalidConfiguration(
            "--live-simhub requires --telemetry-source simhub_bridge".to_string(),
        )
        .into());
    }
    if duration_ms == 0 {
        return Err(CliError::InvalidConfiguration(
            "--duration-ms must be > 0 for --live-simhub".to_string(),
        )
        .into());
    }

    let bind_addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port));
    let socket = UdpSocket::bind(bind_addr).await.with_context(|| {
        format!(
            "failed to bind SimHub telemetry socket at {} (is another process using this port?)",
            bind_addr
        )
    })?;
    record_live_simhub_snapshots_from_socket(
        socket,
        &format!("udp://{bind_addr}"),
        game_id,
        telemetry_source,
        output_path,
        session_id,
        duration_ms,
        json,
    )
    .await
}

pub(super) async fn record_live_simhub_snapshots_from_socket(
    socket: UdpSocket,
    input_label: &str,
    game_id: &str,
    telemetry_source: &str,
    output_path: &str,
    session_id: Option<&str>,
    duration_ms: u64,
    json: bool,
) -> Result<()> {
    let session_id = session_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| default_recorder_session_id(game_id));
    let start = Instant::now();
    let deadline = start + Duration::from_millis(duration_ms.max(1));
    let mut buf = [0u8; MAX_PACKET_SIZE];
    let mut snapshots = Vec::new();
    let mut packets_received = 0u64;
    let mut bytes_received = 0u64;
    let mut parse_errors = 0u64;
    let mut previous_timestamp_ns = None;

    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let timeout = remaining.min(Duration::from_millis(100));
        let recv = tokio::time::timeout(timeout, socket.recv_from(&mut buf)).await;
        let (len, _) = match recv {
            Ok(Ok(value)) => value,
            Ok(Err(error)) => return Err(anyhow!("SimHub telemetry receive failed: {}", error)),
            Err(_) => continue,
        };
        packets_received = packets_received.saturating_add(1);
        bytes_received = bytes_received
            .saturating_add(u64::try_from(len).context("received SimHub packet length overflow")?);

        let normalized = match parse_simhub_packet(&buf[..len]) {
            Ok(normalized) => normalized,
            Err(_) => {
                parse_errors = parse_errors.saturating_add(1);
                continue;
            }
        };
        let mut snapshot = serde_json::to_value(normalized)?;
        let sequence = u64::try_from(snapshots.len()).context("too many live telemetry records")?;
        let mut timestamp_ns = start.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
        if previous_timestamp_ns
            .map(|previous| timestamp_ns <= previous)
            .unwrap_or(false)
        {
            timestamp_ns = previous_timestamp_ns.unwrap_or(0).saturating_add(1);
        }
        previous_timestamp_ns = Some(timestamp_ns);
        let Some(object) = snapshot.as_object_mut() else {
            return Err(anyhow!("normalized SimHub snapshot is not a JSON object"));
        };
        object.insert("sequence".to_string(), serde_json::json!(sequence));
        object.insert("timestamp_ns".to_string(), serde_json::json!(timestamp_ns));
        stamp_record_provenance(
            &mut snapshot,
            game_id,
            telemetry_source,
            &session_id,
            duration_ms,
        )?;
        snapshots.push(snapshot);
    }

    if snapshots.is_empty() {
        return Err(anyhow!(
            "live SimHub recording received {packets_received} packet(s) but no valid normalized snapshots"
        ));
    }
    write_jsonl_values(output_path, &snapshots)?;

    let normalized_snapshot_count =
        u64::try_from(snapshots.len()).context("too many normalized telemetry records")?;
    let summary = LiveRecordSummary {
        command: RECORD_COMMAND,
        game: game_id.to_string(),
        telemetry_source: telemetry_source.to_string(),
        input: input_label.to_string(),
        output: output_path.to_string(),
        recorder_session_id: session_id,
        normalized_snapshot_count,
        duration_ms,
        packets_received,
        bytes_received,
        parse_errors,
        hardware_output_enabled: false,
        no_hid_device_opened: true,
        no_ffb_writes: true,
        no_serial_config_commands: true,
        no_firmware_or_dfu_commands: true,
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&summary)?);
    } else {
        println!("Live SimHub telemetry recording complete");
        println!("  game: {}", summary.game);
        println!("  telemetry_source: {}", summary.telemetry_source);
        println!("  listen: {}", summary.input);
        println!("  snapshots: {}", summary.normalized_snapshot_count);
        println!("  packets_received: {}", summary.packets_received);
        println!("  parse_errors: {}", summary.parse_errors);
        println!("  session: {}", summary.recorder_session_id);
        println!("  output: {}", summary.output);
    }

    Ok(())
}

pub(super) fn validate_record_metadata(game_id: &str, telemetry_source: &str) -> Result<()> {
    if game_id.trim().is_empty() {
        return Err(CliError::InvalidConfiguration("--game must not be empty".to_string()).into());
    }
    if !matches!(telemetry_source, "real_game" | "simhub_bridge") {
        return Err(CliError::InvalidConfiguration(
            "--telemetry-source must be real_game or simhub_bridge".to_string(),
        )
        .into());
    }
    Ok(())
}

pub(super) fn stamp_record_provenance(
    snapshot: &mut Value,
    game_id: &str,
    telemetry_source: &str,
    session_id: &str,
    duration_ms: u64,
) -> Result<()> {
    let Some(object) = snapshot.as_object_mut() else {
        return Err(anyhow!("validated snapshot is not a JSON object"));
    };
    object.insert(
        "recorder_command".to_string(),
        serde_json::json!(RECORD_COMMAND),
    );
    object.insert(
        "recorder_session_id".to_string(),
        serde_json::json!(session_id),
    );
    object.insert(
        "recording_duration_ms".to_string(),
        serde_json::json!(duration_ms),
    );
    object.insert("game".to_string(), serde_json::json!(game_id));
    object.insert(
        "telemetry_source".to_string(),
        serde_json::json!(telemetry_source),
    );
    object.insert(
        "hardware_output_enabled".to_string(),
        serde_json::json!(false),
    );
    object.insert("no_hid_device_opened".to_string(), serde_json::json!(true));
    object.insert("no_ffb_writes".to_string(), serde_json::json!(true));
    object.insert(
        "no_serial_config_commands".to_string(),
        serde_json::json!(true),
    );
    object.insert(
        "no_firmware_or_dfu_commands".to_string(),
        serde_json::json!(true),
    );
    Ok(())
}

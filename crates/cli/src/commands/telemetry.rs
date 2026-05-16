//! Telemetry discovery and capture commands.

mod capture;
mod probe;
mod record;
mod shared;
mod virtual_ffb;

use crate::commands::TelemetryCommands;
use crate::error::CliError;
use anyhow::Result;

use capture::capture;
use probe::probe;
use record::{record_live_simhub_snapshots, record_normalized_snapshots};
use virtual_ffb::write_virtual_ffb_log;

const REGISTER_COMMAND_APPLICATION: u8 = 1;
const PROTOCOL_VERSION: u8 = 4;
const MSG_REGISTRATION_RESULT: u8 = 1;
const MAX_PACKET_SIZE: usize = 4096;
const CAPTURE_MAGIC: &[u8; 8] = b"ORACAPv1";
const RECORD_COMMAND: &str = "wheelctl telemetry record";
const VIRTUAL_FFB_LOG_COMMAND: &str = "wheelctl telemetry virtual-ffb-log";
const DEFAULT_RECORD_FRAME_PERIOD_NS: u64 = 16_666_667;
#[cfg(test)]
const DEFAULT_SIMHUB_PORT: u16 = 5555;
const VIRTUAL_FFB_REPORT_FORMAT: &str = "openracing_virtual_ffb_v1";
const VIRTUAL_FFB_VENDOR_ID: u16 = 0xFFFF;
const VIRTUAL_FFB_PRODUCT_ID: u16 = 0x0001;

/// Execute telemetry command.
pub async fn execute(cmd: &TelemetryCommands, json: bool) -> Result<()> {
    match cmd {
        TelemetryCommands::Probe {
            game,
            endpoint,
            timeout_ms,
            attempts,
        } => probe(game, endpoint, *timeout_ms, *attempts, json).await,
        TelemetryCommands::Capture {
            game,
            port,
            duration,
            out,
            max_payload,
        } => capture(game, *port, *duration, out, *max_payload, json).await,
        TelemetryCommands::Record {
            game,
            telemetry_source,
            input,
            live_simhub,
            port,
            out,
            session_id,
            duration_ms,
        } => match (live_simhub, input.as_deref()) {
            (true, None) => {
                record_live_simhub_snapshots(
                    game,
                    telemetry_source,
                    *port,
                    out,
                    session_id.as_deref(),
                    *duration_ms,
                    json,
                )
                .await
            }
            (false, Some(input)) => {
                record_normalized_snapshots(
                    game,
                    telemetry_source,
                    input,
                    out,
                    session_id.as_deref(),
                    *duration_ms,
                    json,
                )
                .await
            }
            (true, Some(_)) => Err(CliError::InvalidConfiguration(
                "--input cannot be combined with --live-simhub".to_string(),
            )
            .into()),
            (false, None) => Err(CliError::InvalidConfiguration(
                "--input is required unless --live-simhub is set".to_string(),
            )
            .into()),
        },
        TelemetryCommands::VirtualFfbLog {
            input,
            out,
            session_id,
            max_percent,
            watchdog_timeout_ms,
        } => {
            write_virtual_ffb_log(
                input,
                out,
                session_id.as_deref(),
                *max_percent,
                *watchdog_timeout_ms,
                json,
            )
            .await
        }
    }
}
#[cfg(test)]
mod tests {
    use super::probe::{
        PacketReader, build_register_packet, ensure_probe_game, parse_registration_result,
        read_acc_string, write_acc_string,
    };
    use super::record::{record_live_simhub_snapshots, record_live_simhub_snapshots_from_socket};
    use super::shared::{
        default_recorder_session_id, normalized_telemetry_payload,
        normalized_telemetry_payload_is_valid, read_normalized_telemetry_records,
    };
    use super::*;
    use anyhow::anyhow;
    use serde_json::Value;
    use std::fs;
    use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
    use std::path::{Path, PathBuf};
    use std::time::Duration;
    use tokio::net::UdpSocket;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn normalized_snapshot(sequence: usize) -> Value {
        serde_json::json!({
            "sequence": sequence,
            "timestamp_ns": sequence as u64 * DEFAULT_RECORD_FRAME_PERIOD_NS,
            "speed_ms": 12.5,
            "steering_angle": 0.05,
            "throttle": 0.25,
            "brake": 0.0,
            "rpm": 3200.0,
            "gear": 3,
            "ffb_scalar": 0.2
        })
    }

    fn write_normalized_jsonl(path: &Path, count: usize) -> TestResult {
        let mut lines = String::new();
        for sequence in 0..count {
            lines.push_str(&serde_json::to_string(&normalized_snapshot(sequence))?);
            lines.push('\n');
        }
        fs::write(path, lines)?;
        Ok(())
    }

    fn simhub_packet(sequence: usize) -> String {
        serde_json::json!({
            "SpeedMs": 11.5 + sequence as f32,
            "Rpms": 3200.0 + sequence as f32,
            "MaxRpms": 8000.0,
            "Gear": "3",
            "Throttle": 25.0,
            "Brake": 0.0,
            "Clutch": 0.0,
            "Steer": 0.05,
            "FuelPercent": 81.0,
            "LateralGForce": 0.2,
            "LongitudinalGForce": 0.1,
            "FFBValue": 0.2
        })
        .to_string()
    }

    fn telemetry_fixture_path(relative: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("fixtures")
            .join("telemetry")
            .join(relative)
    }

    fn read_jsonl_values(path: &Path) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
        let contents = fs::read_to_string(path)?;
        let mut values = Vec::new();
        for (line_index, line) in contents.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let value: Value = serde_json::from_str(trimmed)
                .map_err(|error| format!("invalid JSONL line {}: {error}", line_index + 1))?;
            values.push(value);
        }
        Ok(values)
    }

    fn assert_fixture_records_are_synthetic(path: &Path) -> TestResult {
        let records = read_jsonl_values(path)?;
        assert!(!records.is_empty());
        for record in records {
            assert_eq!(
                record.get("fixture_source").and_then(Value::as_str),
                Some("synthetic")
            );
            assert_eq!(
                record
                    .get("real_simulator_validated")
                    .and_then(Value::as_bool),
                Some(false)
            );
        }
        Ok(())
    }

    #[test]
    fn test_ensure_probe_game_accepts_acc_and_ac_rally() {
        assert!(ensure_probe_game("acc").is_ok());
        assert!(ensure_probe_game("ac_rally").is_ok());
    }

    #[test]
    fn test_ensure_probe_game_rejects_unsupported_game() {
        let result = ensure_probe_game("iracing");
        assert!(result.is_err());
    }

    #[test]
    fn test_ensure_probe_game_rejects_empty_string() {
        let result = ensure_probe_game("");
        assert!(result.is_err());
    }

    #[test]
    fn test_ensure_probe_game_error_message_lists_supported() {
        let result = ensure_probe_game("ams2");
        assert!(result.is_err());
        let msg = result
            .as_ref()
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        assert!(msg.contains("acc"));
        assert!(msg.contains("ac_rally"));
    }

    #[tokio::test]
    async fn record_normalized_snapshots_writes_moza_compatible_provenance() -> TestResult {
        let dir = tempfile::tempdir()?;
        let input = dir.path().join("normalized.jsonl");
        let output = dir.path().join("recording.jsonl");
        write_normalized_jsonl(&input, 2)?;

        record_normalized_snapshots(
            "simhub-bridge",
            "simhub_bridge",
            input.to_str().ok_or("input path not UTF-8")?,
            output.to_str().ok_or("output path not UTF-8")?,
            Some("session-001"),
            5000,
            false,
        )
        .await?;

        let contents = fs::read_to_string(&output)?;
        let mut lines = contents.lines();
        let first_line = lines.next().ok_or("missing first record")?;
        let first: Value = serde_json::from_str(first_line)?;
        assert_eq!(
            first.get("recorder_command"),
            Some(&serde_json::json!(RECORD_COMMAND))
        );
        assert_eq!(
            first.get("recorder_session_id"),
            Some(&serde_json::json!("session-001"))
        );
        assert_eq!(first.get("game"), Some(&serde_json::json!("simhub-bridge")));
        assert_eq!(
            first.get("telemetry_source"),
            Some(&serde_json::json!("simhub_bridge"))
        );
        assert_eq!(
            first.get("hardware_output_enabled"),
            Some(&serde_json::json!(false))
        );
        assert_eq!(first.get("no_ffb_writes"), Some(&serde_json::json!(true)));
        assert!(lines.next().is_some());
        assert!(lines.next().is_none());
        Ok(())
    }

    #[test]
    fn checked_in_replay_fixtures_are_synthetic_and_valid() -> TestResult {
        for fixture in [
            "simhub/basic-lap.jsonl",
            "iracing/basic-lap.jsonl",
            "acc/basic-lap.jsonl",
        ] {
            let path = telemetry_fixture_path(fixture);
            assert_fixture_records_are_synthetic(&path)?;
            let records =
                read_normalized_telemetry_records(path.to_str().ok_or("path not UTF-8")?)?;
            assert_eq!(records.len(), 3);
            for record in records {
                let payload =
                    normalized_telemetry_payload(&record).ok_or("missing normalized payload")?;
                assert!(normalized_telemetry_payload_is_valid(payload));
            }
        }
        Ok(())
    }

    #[tokio::test]
    async fn record_normalized_snapshots_accepts_checked_in_replay_fixtures() -> TestResult {
        for (fixture, game, telemetry_source) in [
            ("simhub/basic-lap.jsonl", "simhub-bridge", "simhub_bridge"),
            ("iracing/basic-lap.jsonl", "iracing", "real_game"),
            ("acc/basic-lap.jsonl", "acc", "real_game"),
        ] {
            let dir = tempfile::tempdir()?;
            let input = telemetry_fixture_path(fixture);
            let output = dir.path().join("recording.jsonl");

            record_normalized_snapshots(
                game,
                telemetry_source,
                input.to_str().ok_or("input path not UTF-8")?,
                output.to_str().ok_or("output path not UTF-8")?,
                Some("fixture-session"),
                5000,
                false,
            )
            .await?;

            let records = read_jsonl_values(&output)?;
            assert_eq!(records.len(), 3);
            for (sequence, record) in records.iter().enumerate() {
                assert_eq!(record.get("game").and_then(Value::as_str), Some(game));
                assert_eq!(
                    record.get("telemetry_source").and_then(Value::as_str),
                    Some(telemetry_source)
                );
                assert_eq!(
                    record.get("recorder_session_id").and_then(Value::as_str),
                    Some("fixture-session")
                );
                assert_eq!(
                    record
                        .get("hardware_output_enabled")
                        .and_then(Value::as_bool),
                    Some(false)
                );
                assert_eq!(
                    record.get("no_ffb_writes").and_then(Value::as_bool),
                    Some(true)
                );
                assert_eq!(
                    record.get("sequence").and_then(Value::as_u64),
                    Some(u64::try_from(sequence)?)
                );
            }
        }
        Ok(())
    }

    #[tokio::test]
    async fn record_normalized_snapshots_rejects_fault_fixtures_without_output() -> TestResult {
        for (fixture, expected_error) in [
            (
                "faults/missing-fields.jsonl",
                "missing valid normalized telemetry fields",
            ),
            (
                "faults/stale-frame.jsonl",
                "stale or non-monotonic timestamp_ns",
            ),
        ] {
            let dir = tempfile::tempdir()?;
            let input = telemetry_fixture_path(fixture);
            let output = dir.path().join("recording.jsonl");

            let result = record_normalized_snapshots(
                "simhub-bridge",
                "simhub_bridge",
                input.to_str().ok_or("input path not UTF-8")?,
                output.to_str().ok_or("output path not UTF-8")?,
                Some("fault-session"),
                5000,
                false,
            )
            .await;

            let error = match result {
                Ok(()) => {
                    return Err(format!("fault fixture {fixture} unexpectedly recorded").into());
                }
                Err(error) => error.to_string(),
            };
            assert!(
                error.contains(expected_error),
                "expected error containing '{expected_error}', got '{error}'"
            );
            assert!(!output.exists());
        }
        Ok(())
    }

    #[tokio::test]
    async fn virtual_ffb_log_accepts_checked_in_replay_fixture() -> TestResult {
        let dir = tempfile::tempdir()?;
        let input = telemetry_fixture_path("simhub/basic-lap.jsonl");
        let output = dir.path().join("simulator-ffb-output.virtual.jsonl");

        write_virtual_ffb_log(
            input.to_str().ok_or("input path not UTF-8")?,
            output.to_str().ok_or("output path not UTF-8")?,
            Some("virtual-session-001"),
            2.0,
            100,
            false,
        )
        .await?;

        let records = read_jsonl_values(&output)?;
        assert_eq!(records.len(), 8);
        let mut nonzero = 0usize;
        let mut clear_events = Vec::new();
        for (sequence, record) in records.iter().enumerate() {
            assert_eq!(
                record.get("sequence").and_then(Value::as_u64),
                Some(u64::try_from(sequence)?)
            );
            assert_eq!(
                record.get("hardware_source").and_then(Value::as_str),
                Some("virtual")
            );
            assert_eq!(
                record
                    .get("real_hardware_validated")
                    .and_then(Value::as_bool),
                Some(false)
            );
            assert_eq!(
                record
                    .get("real_simulator_validated")
                    .and_then(Value::as_bool),
                Some(false)
            );
            assert_eq!(
                record
                    .get("hardware_output_enabled")
                    .and_then(Value::as_bool),
                Some(false)
            );
            assert_eq!(
                record.get("no_hid_device_opened").and_then(Value::as_bool),
                Some(true)
            );
            assert_eq!(
                record.get("no_ffb_writes").and_then(Value::as_bool),
                Some(true)
            );
            let percent = record
                .get("output_percent")
                .and_then(Value::as_f64)
                .ok_or("missing output_percent")?;
            assert!(percent.abs() <= 2.0);
            if percent.abs() > f64::EPSILON {
                nonzero += 1;
            }
            if record.get("kind").and_then(Value::as_str) == Some("clear_zero") {
                let event = record
                    .get("clear_event")
                    .and_then(Value::as_str)
                    .ok_or("missing clear event")?;
                clear_events.push(event.to_string());
            }
        }
        assert_eq!(nonzero, 2);
        assert_eq!(
            clear_events,
            vec!["stop", "pause", "game_exit", "mode_mismatch"]
        );
        assert_eq!(
            records
                .last()
                .and_then(|record| record.get("kind"))
                .and_then(Value::as_str),
            Some("final_zero")
        );
        assert_eq!(
            records
                .last()
                .and_then(|record| record.get("virtual_report_hex"))
                .and_then(Value::as_str),
            Some("0000000000000000")
        );
        Ok(())
    }

    #[tokio::test]
    async fn virtual_ffb_log_rejects_fault_fixtures_without_output() -> TestResult {
        for (fixture, expected_error) in [
            (
                "faults/missing-fields.jsonl",
                "missing valid normalized telemetry fields",
            ),
            (
                "faults/stale-frame.jsonl",
                "stale or non-monotonic timestamp_ns",
            ),
        ] {
            let dir = tempfile::tempdir()?;
            let input = telemetry_fixture_path(fixture);
            let output = dir.path().join("simulator-ffb-output.virtual.jsonl");

            let result = write_virtual_ffb_log(
                input.to_str().ok_or("input path not UTF-8")?,
                output.to_str().ok_or("output path not UTF-8")?,
                Some("fault-session"),
                2.0,
                100,
                false,
            )
            .await;

            let error = match result {
                Ok(()) => {
                    return Err(format!("fault fixture {fixture} unexpectedly produced FFB").into());
                }
                Err(error) => error.to_string(),
            };
            assert!(
                error.contains(expected_error),
                "expected error containing '{expected_error}', got '{error}'"
            );
            assert!(!output.exists());
        }
        Ok(())
    }

    #[tokio::test]
    async fn virtual_ffb_log_refuses_ci_hardware_output_paths() -> TestResult {
        let dir = tempfile::tempdir()?;
        let input = telemetry_fixture_path("simhub/basic-lap.jsonl");
        let output = dir
            .path()
            .join("ci")
            .join("hardware")
            .join("moza-r5")
            .join("2026-05-12")
            .join("simulator-ffb-output.virtual.jsonl");

        let result = write_virtual_ffb_log(
            input.to_str().ok_or("input path not UTF-8")?,
            output.to_str().ok_or("output path not UTF-8")?,
            Some("virtual-session-001"),
            2.0,
            100,
            false,
        )
        .await;

        let error = match result {
            Ok(()) => return Err("virtual FFB log unexpectedly wrote under ci/hardware".into()),
            Err(error) => error.to_string(),
        };
        assert!(error.contains("ci/hardware"));
        assert!(!output.exists());
        Ok(())
    }

    #[tokio::test]
    async fn record_normalized_snapshots_rejects_unsupported_source() -> TestResult {
        let dir = tempfile::tempdir()?;
        let input = dir.path().join("normalized.jsonl");
        let output = dir.path().join("recording.jsonl");
        write_normalized_jsonl(&input, 1)?;

        let result = record_normalized_snapshots(
            "simhub-bridge",
            "synthetic",
            input.to_str().ok_or("input path not UTF-8")?,
            output.to_str().ok_or("output path not UTF-8")?,
            Some("session-001"),
            5000,
            false,
        )
        .await;

        assert!(result.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn execute_record_dispatches_to_json_summary() -> TestResult {
        let dir = tempfile::tempdir()?;
        let input = dir.path().join("normalized.jsonl");
        let output = dir.path().join("recording.jsonl");
        write_normalized_jsonl(&input, 1)?;
        let command = TelemetryCommands::Record {
            game: "simhub-bridge".to_string(),
            telemetry_source: "simhub_bridge".to_string(),
            input: Some(input.to_str().ok_or("input path not UTF-8")?.to_string()),
            live_simhub: false,
            port: DEFAULT_SIMHUB_PORT,
            out: output.to_str().ok_or("output path not UTF-8")?.to_string(),
            session_id: None,
            duration_ms: 1000,
        };

        execute(&command, true).await?;

        let contents = fs::read_to_string(&output)?;
        let first_line = contents.lines().next().ok_or("missing first record")?;
        let first: Value = serde_json::from_str(first_line)?;
        let recorder_session_id = first
            .get("recorder_session_id")
            .and_then(Value::as_str)
            .ok_or("missing recorder session id")?;
        assert!(recorder_session_id.starts_with("simhub-bridge-"));
        assert_eq!(
            first.get("telemetry_source").and_then(Value::as_str),
            Some("simhub_bridge")
        );
        assert_eq!(
            first.get("no_ffb_writes").and_then(Value::as_bool),
            Some(true)
        );
        Ok(())
    }

    #[tokio::test]
    async fn record_live_simhub_snapshots_writes_moza_compatible_provenance() -> TestResult {
        let dir = tempfile::tempdir()?;
        let output = dir.path().join("recording.jsonl");
        let listener =
            UdpSocket::bind(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))).await?;
        let listen_addr = listener.local_addr()?;
        let output_for_task = output.clone();
        let input_label = format!("udp://{listen_addr}");

        let task = tokio::spawn(async move {
            record_live_simhub_snapshots_from_socket(
                listener,
                &input_label,
                "simhub-bridge",
                "simhub_bridge",
                output_for_task
                    .to_str()
                    .ok_or_else(|| anyhow!("output path not UTF-8"))?,
                Some("live-session-001"),
                250,
                false,
            )
            .await
        });

        tokio::time::sleep(Duration::from_millis(25)).await;
        let sender =
            UdpSocket::bind(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))).await?;
        for sequence in 0..3 {
            sender
                .send_to(simhub_packet(sequence).as_bytes(), listen_addr)
                .await?;
        }

        task.await??;

        let records = read_jsonl_values(&output)?;
        assert_eq!(records.len(), 3);
        let mut previous_timestamp = None;
        for (sequence, record) in records.iter().enumerate() {
            assert_eq!(
                record.get("recorder_command"),
                Some(&serde_json::json!(RECORD_COMMAND))
            );
            assert_eq!(
                record.get("recorder_session_id").and_then(Value::as_str),
                Some("live-session-001")
            );
            assert_eq!(
                record.get("game").and_then(Value::as_str),
                Some("simhub-bridge")
            );
            assert_eq!(
                record.get("telemetry_source").and_then(Value::as_str),
                Some("simhub_bridge")
            );
            assert_eq!(
                record
                    .get("hardware_output_enabled")
                    .and_then(Value::as_bool),
                Some(false)
            );
            assert_eq!(
                record.get("no_hid_device_opened").and_then(Value::as_bool),
                Some(true)
            );
            assert_eq!(
                record.get("no_ffb_writes").and_then(Value::as_bool),
                Some(true)
            );
            assert_eq!(
                record.get("sequence").and_then(Value::as_u64),
                Some(u64::try_from(sequence)?)
            );
            let timestamp = record
                .get("timestamp_ns")
                .and_then(Value::as_u64)
                .ok_or("missing timestamp_ns")?;
            assert!(
                previous_timestamp
                    .map(|previous| timestamp > previous)
                    .unwrap_or(true)
            );
            previous_timestamp = Some(timestamp);
            assert!(normalized_telemetry_payload_is_valid(record));
        }
        Ok(())
    }

    #[tokio::test]
    async fn execute_record_rejects_missing_input_without_live_simhub() -> TestResult {
        let dir = tempfile::tempdir()?;
        let output = dir.path().join("recording.jsonl");
        let command = TelemetryCommands::Record {
            game: "simhub-bridge".to_string(),
            telemetry_source: "simhub_bridge".to_string(),
            input: None,
            live_simhub: false,
            port: DEFAULT_SIMHUB_PORT,
            out: output.to_str().ok_or("output path not UTF-8")?.to_string(),
            session_id: None,
            duration_ms: 1000,
        };

        let result = execute(&command, false).await;

        let error = match result {
            Ok(()) => return Err("record unexpectedly accepted missing input".into()),
            Err(error) => error.to_string(),
        };
        assert!(error.contains("--input is required"));
        assert!(!output.exists());
        Ok(())
    }

    #[tokio::test]
    async fn execute_record_rejects_input_with_live_simhub() -> TestResult {
        let dir = tempfile::tempdir()?;
        let input = dir.path().join("normalized.jsonl");
        let output = dir.path().join("recording.jsonl");
        write_normalized_jsonl(&input, 1)?;
        let command = TelemetryCommands::Record {
            game: "simhub-bridge".to_string(),
            telemetry_source: "simhub_bridge".to_string(),
            input: Some(input.to_str().ok_or("input path not UTF-8")?.to_string()),
            live_simhub: true,
            port: DEFAULT_SIMHUB_PORT,
            out: output.to_str().ok_or("output path not UTF-8")?.to_string(),
            session_id: None,
            duration_ms: 1000,
        };

        let result = execute(&command, false).await;

        let error = match result {
            Ok(()) => return Err("record unexpectedly accepted input plus live SimHub".into()),
            Err(error) => error.to_string(),
        };
        assert!(error.contains("--input cannot be combined"));
        assert!(!output.exists());
        Ok(())
    }

    #[tokio::test]
    async fn record_live_simhub_requires_simhub_source_and_duration() -> TestResult {
        let dir = tempfile::tempdir()?;
        let output = dir.path().join("recording.jsonl");
        let output_str = output.to_str().ok_or("output path not UTF-8")?;

        let wrong_source = record_live_simhub_snapshots(
            "simhub-bridge",
            "real_game",
            0,
            output_str,
            Some("live-session-001"),
            100,
            false,
        )
        .await;
        assert!(wrong_source.is_err());

        let zero_duration = record_live_simhub_snapshots(
            "simhub-bridge",
            "simhub_bridge",
            0,
            output_str,
            Some("live-session-001"),
            0,
            false,
        )
        .await;
        assert!(zero_duration.is_err());
        assert!(!output.exists());
        Ok(())
    }

    #[tokio::test]
    async fn record_normalized_snapshots_rejects_empty_game() -> TestResult {
        let dir = tempfile::tempdir()?;
        let input = dir.path().join("normalized.jsonl");
        let output = dir.path().join("recording.jsonl");
        write_normalized_jsonl(&input, 1)?;

        let result = record_normalized_snapshots(
            " ",
            "real_game",
            input.to_str().ok_or("input path not UTF-8")?,
            output.to_str().ok_or("output path not UTF-8")?,
            None,
            1000,
            false,
        )
        .await;

        assert!(result.is_err());
        assert!(!output.exists());
        Ok(())
    }

    #[tokio::test]
    async fn record_normalized_snapshots_rejects_empty_input() -> TestResult {
        let dir = tempfile::tempdir()?;
        let input = dir.path().join("empty.jsonl");
        let output = dir.path().join("recording.jsonl");
        fs::write(&input, "\n\n")?;

        let result = record_normalized_snapshots(
            "simhub-bridge",
            "simhub_bridge",
            input.to_str().ok_or("input path not UTF-8")?,
            output.to_str().ok_or("output path not UTF-8")?,
            Some("session-001"),
            1000,
            false,
        )
        .await;

        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn normalized_record_reader_accepts_wrapped_json_shapes() -> TestResult {
        let dir = tempfile::tempdir()?;
        for (file_name, contents) in [
            (
                "array.json",
                serde_json::json!([normalized_snapshot(0), normalized_snapshot(1)]),
            ),
            (
                "frames.json",
                serde_json::json!({"frames": [normalized_snapshot(0)]}),
            ),
            (
                "records.json",
                serde_json::json!({"records": [normalized_snapshot(0)]}),
            ),
            (
                "snapshots.json",
                serde_json::json!({"snapshots": [normalized_snapshot(0)]}),
            ),
        ] {
            let path = dir.path().join(file_name);
            fs::write(&path, serde_json::to_string(&contents)?)?;
            let records =
                read_normalized_telemetry_records(path.to_str().ok_or("path not UTF-8")?)?;
            assert!(!records.is_empty());
        }
        Ok(())
    }

    #[test]
    fn normalized_record_reader_accepts_single_json_object() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("snapshot.json");
        fs::write(&path, serde_json::to_string(&normalized_snapshot(7))?)?;

        let records = read_normalized_telemetry_records(path.to_str().ok_or("path not UTF-8")?)?;

        assert_eq!(records.len(), 1);
        assert_eq!(
            records
                .first()
                .and_then(|record| record.get("gear"))
                .and_then(Value::as_i64),
            Some(3)
        );
        Ok(())
    }

    #[test]
    fn default_recorder_session_id_sanitizes_game_id() {
        let session_id = default_recorder_session_id("sim hub/bridge");

        assert!(session_id.starts_with("sim-hub-bridge-"));
    }

    #[tokio::test]
    async fn record_normalized_snapshots_inserts_sequence_and_timestamp_for_nested_payload()
    -> TestResult {
        let dir = tempfile::tempdir()?;
        let input = dir.path().join("nested.jsonl");
        let output = dir.path().join("recording.jsonl");
        let mut snapshot = normalized_snapshot(0);
        if let Some(object) = snapshot.as_object_mut() {
            object.remove("sequence");
            object.remove("timestamp_ns");
        }
        fs::write(&input, serde_json::json!({"data": snapshot}).to_string())?;

        record_normalized_snapshots(
            "simhub-bridge",
            "simhub_bridge",
            input.to_str().ok_or("input path not UTF-8")?,
            output.to_str().ok_or("output path not UTF-8")?,
            Some("session-001"),
            1000,
            false,
        )
        .await?;

        let contents = fs::read_to_string(&output)?;
        let first_line = contents.lines().next().ok_or("missing first record")?;
        let first: Value = serde_json::from_str(first_line)?;
        assert_eq!(first.get("sequence").and_then(Value::as_u64), Some(0));
        assert_eq!(first.get("timestamp_ns").and_then(Value::as_u64), Some(0));
        Ok(())
    }

    #[tokio::test]
    async fn record_normalized_snapshots_rejects_out_of_range_payload() -> TestResult {
        let dir = tempfile::tempdir()?;
        let input = dir.path().join("invalid.jsonl");
        let output = dir.path().join("recording.jsonl");
        let mut snapshot = normalized_snapshot(0);
        if let Some(object) = snapshot.as_object_mut() {
            object.insert("speed_ms".to_string(), serde_json::json!(999.0));
        }
        fs::write(&input, snapshot.to_string())?;

        let result = record_normalized_snapshots(
            "simhub-bridge",
            "simhub_bridge",
            input.to_str().ok_or("input path not UTF-8")?,
            output.to_str().ok_or("output path not UTF-8")?,
            Some("session-001"),
            1000,
            false,
        )
        .await;

        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn build_register_packet_structure() -> TestResult {
        let packet = build_register_packet("Test", "", Duration::from_millis(16), "")?;
        assert_eq!(packet[0], REGISTER_COMMAND_APPLICATION);
        assert_eq!(packet[1], PROTOCOL_VERSION);
        // display_name "Test" length = 4 as u16 LE
        assert_eq!(packet[2], 4);
        assert_eq!(packet[3], 0);
        assert_eq!(&packet[4..8], b"Test");
        Ok(())
    }

    #[test]
    fn build_register_packet_empty_name() -> TestResult {
        let packet = build_register_packet("", "", Duration::from_millis(16), "")?;
        assert_eq!(packet[0], REGISTER_COMMAND_APPLICATION);
        // name length = 0
        assert_eq!(packet[2], 0);
        assert_eq!(packet[3], 0);
        Ok(())
    }

    #[test]
    fn build_register_packet_interval_encoded() -> TestResult {
        let packet = build_register_packet("X", "", Duration::from_millis(50), "")?;
        // After header (2 bytes), display_name (2+1), connection_password (2+0)
        // interval is at offset 2 + (2+1) + (2+0) = 7
        let interval_offset = 2 + 2 + 1 + 2;
        let interval_bytes = &packet[interval_offset..interval_offset + 4];
        let interval = i32::from_le_bytes([
            interval_bytes[0],
            interval_bytes[1],
            interval_bytes[2],
            interval_bytes[3],
        ]);
        assert_eq!(interval, 50);
        Ok(())
    }

    #[test]
    fn parse_registration_result_valid() -> TestResult {
        let mut data = Vec::new();
        data.push(MSG_REGISTRATION_RESULT);
        data.extend_from_slice(&42i32.to_le_bytes());
        data.push(1); // success
        data.push(0); // readonly
        data.extend_from_slice(&0u16.to_le_bytes()); // empty error string

        let result = parse_registration_result(&data)?;
        assert_eq!(result.connection_id, 42);
        assert!(result.success);
        assert!(!result.readonly);
        assert!(result.error.is_empty());
        Ok(())
    }

    #[test]
    fn parse_registration_result_with_error_string() -> TestResult {
        let mut data = Vec::new();
        data.push(MSG_REGISTRATION_RESULT);
        data.extend_from_slice(&(-1i32).to_le_bytes());
        data.push(0); // not success
        data.push(0); // not readonly
        let error_msg = b"connection limit reached";
        data.extend_from_slice(&(error_msg.len() as u16).to_le_bytes());
        data.extend_from_slice(error_msg);

        let result = parse_registration_result(&data)?;
        assert_eq!(result.connection_id, -1);
        assert!(!result.success);
        assert_eq!(result.error, "connection limit reached");
        Ok(())
    }

    #[test]
    fn parse_registration_result_wrong_message_type() {
        let data = vec![255u8, 0, 0, 0, 0, 0, 0, 0, 0];
        let result = parse_registration_result(&data);
        assert!(result.is_err());
    }

    #[test]
    fn parse_registration_result_truncated() {
        let data = vec![MSG_REGISTRATION_RESULT, 0]; // too short
        let result = parse_registration_result(&data);
        assert!(result.is_err());
    }

    #[test]
    fn packet_reader_read_exact() -> TestResult {
        let data = [1, 2, 3, 4, 5];
        let mut reader = PacketReader::new(&data);
        let chunk = reader.read_exact(3)?;
        assert_eq!(chunk, &[1, 2, 3]);
        let chunk2 = reader.read_exact(2)?;
        assert_eq!(chunk2, &[4, 5]);
        Ok(())
    }

    #[test]
    fn packet_reader_overflow() {
        let data = [1, 2];
        let mut reader = PacketReader::new(&data);
        let result = reader.read_exact(5);
        assert!(result.is_err());
    }

    #[test]
    fn packet_reader_u16_le() -> TestResult {
        let data = [0x34, 0x12];
        let mut reader = PacketReader::new(&data);
        let val = reader.read_u16_le()?;
        assert_eq!(val, 0x1234);
        Ok(())
    }

    #[test]
    fn packet_reader_i32_le() -> TestResult {
        let data = [0x78, 0x56, 0x34, 0x12];
        let mut reader = PacketReader::new(&data);
        let val = reader.read_i32_le()?;
        assert_eq!(val, 0x12345678);
        Ok(())
    }

    #[test]
    fn packet_reader_bool_u8() -> TestResult {
        let data = [0, 1, 255];
        let mut reader = PacketReader::new(&data);
        assert!(!reader.read_bool_u8()?);
        assert!(reader.read_bool_u8()?);
        assert!(reader.read_bool_u8()?);
        Ok(())
    }

    #[test]
    fn write_and_read_acc_string_roundtrip() -> TestResult {
        let mut buf = Vec::new();
        write_acc_string(&mut buf, "hello")?;

        let mut reader = PacketReader::new(&buf);
        let result = read_acc_string(&mut reader)?;
        assert_eq!(result, "hello");
        Ok(())
    }

    #[test]
    fn write_acc_string_empty() -> TestResult {
        let mut buf = Vec::new();
        write_acc_string(&mut buf, "")?;
        assert_eq!(buf.len(), 2); // just the length prefix
        assert_eq!(buf[0], 0);
        assert_eq!(buf[1], 0);
        Ok(())
    }
}

//! Configuration writers for game-specific telemetry setup

use crate::game_service::{ConfigDiff, ConfigWriter, DiffOperation, TelemetryConfig};
use anyhow::Result;
use serde_json::{Map, Value};
use std::fs;
use std::net::SocketAddr;
use std::path::Path;
use tracing::info;

/// iRacing configuration writer
pub struct IRacingConfigWriter;

impl Default for IRacingConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for IRacingConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing iRacing telemetry configuration");

        let app_ini_path = game_path.join("Documents/iRacing/app.ini");
        let telemetry_enabled = if config.enabled { "1" } else { "0" };

        // Read existing app.ini if it exists.
        let existing_content = if app_ini_path.exists() {
            fs::read_to_string(&app_ini_path)?
        } else {
            String::new()
        };

        let (new_content, prior_value, operation) = upsert_ini_value(
            &existing_content,
            "Telemetry",
            "telemetryDiskFile",
            telemetry_enabled,
        );

        if let Some(parent) = app_ini_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&app_ini_path, &new_content)?;

        let diffs = vec![ConfigDiff {
            file_path: app_ini_path.to_string_lossy().to_string(),
            section: Some("Telemetry".to_string()),
            key: "telemetryDiskFile".to_string(),
            old_value: prior_value,
            new_value: telemetry_enabled.to_string(),
            operation,
        }];

        Ok(diffs)
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let app_ini_path = game_path.join("Documents/iRacing/app.ini");

        if !app_ini_path.exists() {
            return Ok(false);
        }

        let content = fs::read_to_string(app_ini_path)?;

        // Check if telemetry is enabled.
        let has_telemetry_section = content.contains("[Telemetry]");
        let has_telemetry_enabled = content
            .lines()
            .any(|line| line.trim().eq_ignore_ascii_case("telemetryDiskFile=1"));

        Ok(has_telemetry_section && has_telemetry_enabled)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let telemetry_enabled = if config.enabled { "1" } else { "0" };

        Ok(vec![ConfigDiff {
            file_path: "Documents/iRacing/app.ini".to_string(),
            section: Some("Telemetry".to_string()),
            key: "telemetryDiskFile".to_string(),
            old_value: None,
            new_value: telemetry_enabled.to_string(),
            operation: DiffOperation::Add,
        }])
    }
}

/// ACC (Assetto Corsa Competizione) configuration writer
pub struct ACCConfigWriter;

impl Default for ACCConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for ACCConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing ACC telemetry configuration");

        let broadcasting_json_path =
            game_path.join("Documents/Assetto Corsa Competizione/Config/broadcasting.json");

        let existed_before = broadcasting_json_path.exists();
        let existing_content = if broadcasting_json_path.exists() {
            Some(fs::read_to_string(&broadcasting_json_path)?)
        } else {
            None
        };

        let mut json_map = existing_content
            .as_deref()
            .and_then(parse_json_object)
            .unwrap_or_default();

        let listener_port = parse_target_port(&config.output_target).unwrap_or(9000);

        json_map.insert("updListenerPort".to_string(), Value::from(listener_port));
        json_map
            .entry("broadcastingPort".to_string())
            .or_insert(Value::from(9000));
        json_map
            .entry("connectionId".to_string())
            .or_insert(Value::String(String::new()));
        json_map
            .entry("connectionPassword".to_string())
            .or_insert(Value::String(String::new()));
        json_map
            .entry("commandPassword".to_string())
            .or_insert(Value::String(String::new()));
        json_map.insert(
            "updateRateHz".to_string(),
            Value::from(config.update_rate_hz),
        );

        let new_content = serde_json::to_string_pretty(&Value::Object(json_map))?;

        if let Some(parent) = broadcasting_json_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&broadcasting_json_path, &new_content)?;

        let diffs = vec![ConfigDiff {
            file_path: broadcasting_json_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }];

        Ok(diffs)
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let broadcasting_json_path =
            game_path.join("Documents/Assetto Corsa Competizione/Config/broadcasting.json");

        if !broadcasting_json_path.exists() {
            return Ok(false);
        }

        let content = fs::read_to_string(broadcasting_json_path)?;
        let config: Value = serde_json::from_str(&content)?;

        // Check if broadcasting is properly configured.
        let has_udp_port = config.get("updListenerPort").is_some();
        let has_broadcast_port = config.get("broadcastingPort").is_some();

        Ok(has_udp_port && has_broadcast_port)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let listener_port = parse_target_port(&config.output_target).unwrap_or(9000);
        let mut broadcasting_config = Map::new();
        broadcasting_config.insert("updListenerPort".to_string(), Value::from(listener_port));
        broadcasting_config.insert("connectionId".to_string(), Value::String(String::new()));
        broadcasting_config.insert(
            "connectionPassword".to_string(),
            Value::String(String::new()),
        );
        broadcasting_config.insert("broadcastingPort".to_string(), Value::from(9000));
        broadcasting_config.insert("commandPassword".to_string(), Value::String(String::new()));
        broadcasting_config.insert(
            "updateRateHz".to_string(),
            Value::from(config.update_rate_hz),
        );

        let new_content = serde_json::to_string_pretty(&Value::Object(broadcasting_config))?;

        Ok(vec![ConfigDiff {
            file_path: "Documents/Assetto Corsa Competizione/Config/broadcasting.json".to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: new_content,
            operation: DiffOperation::Add,
        }])
    }
}

fn upsert_ini_value(
    content: &str,
    section: &str,
    key: &str,
    new_value: &str,
) -> (String, Option<String>, DiffOperation) {
    let section_header = format!("[{section}]");
    let key_prefix = format!("{key}=");

    let mut lines: Vec<String> = if content.is_empty() {
        Vec::new()
    } else {
        content.lines().map(ToOwned::to_owned).collect()
    };

    let mut section_start = None;
    let mut section_end = lines.len();

    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if section_start.is_some() {
                section_end = index;
                break;
            }

            if trimmed.eq_ignore_ascii_case(&section_header) {
                section_start = Some(index);
            }
        }
    }

    let mut previous_value = None;
    if let Some(start) = section_start {
        let search_start = start + 1;
        let mut key_line_index = None;

        for index in search_start..section_end {
            let trimmed = lines[index].trim();
            if trimmed.starts_with(&key_prefix) {
                key_line_index = Some(index);
                previous_value = Some(trimmed[key_prefix.len()..].trim().to_string());
                break;
            }
        }

        if let Some(index) = key_line_index {
            lines[index] = format!("{key}={new_value}");
            let output = normalize_ini_output(lines);
            return (output, previous_value, DiffOperation::Modify);
        }

        lines.insert(section_end, format!("{key}={new_value}"));
        let output = normalize_ini_output(lines);
        return (output, previous_value, DiffOperation::Add);
    }

    if !lines.is_empty()
        && !lines
            .last()
            .map(|line| line.trim().is_empty())
            .unwrap_or(false)
    {
        lines.push(String::new());
    }

    lines.push(section_header);
    lines.push(format!("{key}={new_value}"));
    let output = normalize_ini_output(lines);
    (output, previous_value, DiffOperation::Add)
}

fn normalize_ini_output(lines: Vec<String>) -> String {
    let mut output = lines.join("\n");
    if !output.ends_with('\n') {
        output.push('\n');
    }
    output
}

fn parse_json_object(content: &str) -> Option<Map<String, Value>> {
    serde_json::from_str::<Value>(content)
        .ok()
        .and_then(|value| value.as_object().cloned())
}

fn parse_target_port(target: &str) -> Option<u16> {
    if let Ok(addr) = target.parse::<SocketAddr>() {
        return Some(addr.port());
    }

    let (_, port_part) = target.rsplit_once(':')?;
    port_part.parse::<u16>().ok()
}

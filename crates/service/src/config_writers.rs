//! Configuration writers for game-specific telemetry setup

use crate::game_service::{ConfigDiff, ConfigWriter, DiffOperation, TelemetryConfig};
use anyhow::{Result, anyhow};
use serde_json::{Map, Value};
use std::fs;
use std::net::SocketAddr;
use std::path::Path;
use tracing::info;

const EAWRC_STRUCTURE_ID: &str = "openracing";
const EAWRC_PACKET_ID: &str = "session_update";
const EAWRC_DEFAULT_PORT: u16 = 20778;
const AC_RALLY_DEFAULT_DISCOVERY_PORT: u16 = 9000;
const AC_RALLY_PROBE_RELATIVE_PATH: &str =
    "Documents/Assetto Corsa Rally/Config/openracing_probe.json";

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

        let existing_map = existing_content
            .as_deref()
            .and_then(parse_json_object)
            .unwrap_or_default();

        let listener_port = parse_target_port(&config.output_target).unwrap_or(9000);
        let connection_password = existing_map
            .get("connectionPassword")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let command_password = existing_map
            .get("commandPassword")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();

        let mut broadcasting_config = Map::new();
        broadcasting_config.insert("updListenerPort".to_string(), Value::from(listener_port));
        // Keep compatibility with environments/tools expecting the corrected key.
        broadcasting_config.insert("udpListenerPort".to_string(), Value::from(listener_port));
        broadcasting_config.insert(
            "connectionPassword".to_string(),
            Value::String(connection_password),
        );
        broadcasting_config.insert(
            "commandPassword".to_string(),
            Value::String(command_password),
        );

        let new_content = serde_json::to_string_pretty(&Value::Object(broadcasting_config))?;

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
        let config_value: Value = serde_json::from_str(&content)?;
        let object = match config_value.as_object() {
            Some(obj) => obj,
            None => return Ok(false),
        };

        // Accept both the original ACC key and the corrected compatibility key.
        let has_listener_port = object
            .get("updListenerPort")
            .or_else(|| object.get("udpListenerPort"))
            .and_then(Value::as_u64)
            .is_some();
        let has_connection_password = object
            .get("connectionPassword")
            .and_then(Value::as_str)
            .is_some();
        let has_command_password = object
            .get("commandPassword")
            .and_then(Value::as_str)
            .is_some();

        Ok(has_listener_port && has_connection_password && has_command_password)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let listener_port = parse_target_port(&config.output_target).unwrap_or(9000);
        let mut broadcasting_config = Map::new();
        broadcasting_config.insert("updListenerPort".to_string(), Value::from(listener_port));
        broadcasting_config.insert("udpListenerPort".to_string(), Value::from(listener_port));
        broadcasting_config.insert(
            "connectionPassword".to_string(),
            Value::String(String::new()),
        );
        broadcasting_config.insert("commandPassword".to_string(), Value::String(String::new()));

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

/// Assetto Corsa Rally configuration writer.
///
/// AC Rally telemetry transport is currently discovery-based in OpenRacing.
/// This writer creates a sidecar probe profile consumed by OpenRacing tooling.
pub struct ACRallyConfigWriter;

impl Default for ACRallyConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for ACRallyConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing Assetto Corsa Rally telemetry probe configuration");

        let probe_json_path = game_path.join(AC_RALLY_PROBE_RELATIVE_PATH);
        let existed_before = probe_json_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&probe_json_path)?)
        } else {
            None
        };

        let mut root = existing_content
            .as_deref()
            .and_then(parse_json_object)
            .unwrap_or_default();

        let listener_port =
            parse_target_port(&config.output_target).unwrap_or(AC_RALLY_DEFAULT_DISCOVERY_PORT);
        root.insert("enabled".to_string(), Value::from(config.enabled));
        root.insert("mode".to_string(), Value::String("discovery".to_string()));
        root.insert(
            "updateRateHz".to_string(),
            Value::from(config.update_rate_hz),
        );
        root.insert(
            "outputTarget".to_string(),
            Value::String(config.output_target.clone()),
        );
        root.insert(
            "probeOrder".to_string(),
            Value::Array(vec![
                Value::String("udp_handshake".to_string()),
                Value::String("udp_passive".to_string()),
                Value::String("shared_memory".to_string()),
            ]),
        );
        root.insert(
            "udpCandidates".to_string(),
            Value::Array(vec![Value::from(listener_port)]),
        );
        root.entry("sharedMemoryCandidates".to_string())
            .or_insert(Value::Array(Vec::new()));
        root.insert(
            "note".to_string(),
            Value::String(
                "OpenRacing discovery profile. Populate sharedMemoryCandidates when map names are known."
                    .to_string(),
            ),
        );

        let new_content = serde_json::to_string_pretty(&Value::Object(root))?;

        if let Some(parent) = probe_json_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&probe_json_path, &new_content)?;

        Ok(vec![ConfigDiff {
            file_path: probe_json_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let probe_json_path = game_path.join(AC_RALLY_PROBE_RELATIVE_PATH);
        if !probe_json_path.exists() {
            return Ok(false);
        }

        let content = fs::read_to_string(probe_json_path)?;
        let value: Value = serde_json::from_str(&content)?;

        let mode_discovery = value
            .get("mode")
            .and_then(Value::as_str)
            .map(|mode| mode == "discovery")
            .unwrap_or(false);
        let has_probe_order = value
            .get("probeOrder")
            .and_then(Value::as_array)
            .map(|items| !items.is_empty())
            .unwrap_or(false);
        let has_udp_candidates = value
            .get("udpCandidates")
            .and_then(Value::as_array)
            .map(|items| !items.is_empty())
            .unwrap_or(false);

        Ok(mode_discovery && has_probe_order && has_udp_candidates)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let listener_port =
            parse_target_port(&config.output_target).unwrap_or(AC_RALLY_DEFAULT_DISCOVERY_PORT);
        let content = serde_json::to_string_pretty(&serde_json::json!({
            "enabled": config.enabled,
            "mode": "discovery",
            "updateRateHz": config.update_rate_hz,
            "outputTarget": config.output_target,
            "probeOrder": ["udp_handshake", "udp_passive", "shared_memory"],
            "udpCandidates": [listener_port],
            "sharedMemoryCandidates": [],
            "note": "OpenRacing discovery profile. Populate sharedMemoryCandidates when map names are known."
        }))?;

        Ok(vec![ConfigDiff {
            file_path: AC_RALLY_PROBE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: content,
            operation: DiffOperation::Add,
        }])
    }
}

/// AMS2 (Automobilista 2) configuration writer.
///
/// AMS2 shared-memory telemetry requires an in-game toggle. This writer
/// stores explicit telemetry intent in the player config while preserving
/// existing content.
pub struct AMS2ConfigWriter;

impl Default for AMS2ConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for AMS2ConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing AMS2 telemetry configuration");

        let player_json_path =
            game_path.join("Documents/Automobilista 2/UserData/player/player.json");
        let existed_before = player_json_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&player_json_path)?)
        } else {
            None
        };

        let mut json_map = existing_content
            .as_deref()
            .and_then(parse_json_object)
            .unwrap_or_default();

        json_map.insert(
            "sharedMemoryEnabled".to_string(),
            Value::from(config.enabled),
        );
        json_map.insert(
            "openRacingTelemetry".to_string(),
            Value::Object(Map::from_iter([
                ("enabled".to_string(), Value::from(config.enabled)),
                (
                    "sharedMemoryMap".to_string(),
                    Value::String("$pcars2$".to_string()),
                ),
                (
                    "updateRateHz".to_string(),
                    Value::from(config.update_rate_hz),
                ),
                (
                    "note".to_string(),
                    Value::String(
                        "Enable Project CARS 2 shared memory in AMS2 options.".to_string(),
                    ),
                ),
            ])),
        );

        let new_content = serde_json::to_string_pretty(&Value::Object(json_map))?;

        if let Some(parent) = player_json_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&player_json_path, &new_content)?;

        Ok(vec![ConfigDiff {
            file_path: player_json_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let player_json_path =
            game_path.join("Documents/Automobilista 2/UserData/player/player.json");
        if !player_json_path.exists() {
            return Ok(false);
        }

        let content = fs::read_to_string(player_json_path)?;
        let config: Value = serde_json::from_str(&content)?;

        let top_level_enabled = config
            .get("sharedMemoryEnabled")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let openracing_enabled = config
            .get("openRacingTelemetry")
            .and_then(Value::as_object)
            .and_then(|obj| obj.get("enabled"))
            .and_then(Value::as_bool)
            .unwrap_or(false);

        Ok(top_level_enabled && openracing_enabled)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let mut root = Map::new();
        root.insert(
            "sharedMemoryEnabled".to_string(),
            Value::from(config.enabled),
        );
        root.insert(
            "openRacingTelemetry".to_string(),
            Value::Object(Map::from_iter([
                ("enabled".to_string(), Value::from(config.enabled)),
                (
                    "sharedMemoryMap".to_string(),
                    Value::String("$pcars2$".to_string()),
                ),
                (
                    "updateRateHz".to_string(),
                    Value::from(config.update_rate_hz),
                ),
                (
                    "note".to_string(),
                    Value::String(
                        "Enable Project CARS 2 shared memory in AMS2 options.".to_string(),
                    ),
                ),
            ])),
        );

        Ok(vec![ConfigDiff {
            file_path: "Documents/Automobilista 2/UserData/player/player.json".to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: serde_json::to_string_pretty(&Value::Object(root))?,
            operation: DiffOperation::Add,
        }])
    }
}

/// rFactor 2 configuration writer.
///
/// rFactor 2 telemetry requires the shared-memory plugin. This writer
/// generates an explicit plugin telemetry configuration contract.
pub struct RFactor2ConfigWriter;

impl Default for RFactor2ConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for RFactor2ConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing rFactor 2 telemetry configuration");

        let config_path = game_path.join("UserData/player/OpenRacing.Telemetry.json");
        let existed_before = config_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&config_path)?)
        } else {
            None
        };

        let mut root = existing_content
            .as_deref()
            .and_then(parse_json_object)
            .unwrap_or_default();
        root.insert("enabled".to_string(), Value::from(config.enabled));
        root.insert("requiresSharedMemoryPlugin".to_string(), Value::from(true));
        root.insert(
            "telemetryMap".to_string(),
            Value::String("$rFactor2SMMP_Telemetry$".to_string()),
        );
        root.insert(
            "scoringMap".to_string(),
            Value::String("$rFactor2SMMP_Scoring$".to_string()),
        );
        root.insert(
            "forceFeedbackMap".to_string(),
            Value::String("$rFactor2SMMP_ForceFeedback$".to_string()),
        );
        root.insert(
            "updateRateHz".to_string(),
            Value::from(config.update_rate_hz),
        );

        let new_content = serde_json::to_string_pretty(&Value::Object(root))?;

        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&config_path, &new_content)?;

        Ok(vec![ConfigDiff {
            file_path: config_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before {
                DiffOperation::Modify
            } else {
                DiffOperation::Add
            },
        }])
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let config_path = game_path.join("UserData/player/OpenRacing.Telemetry.json");
        if !config_path.exists() {
            return Ok(false);
        }

        let content = fs::read_to_string(config_path)?;
        let value: Value = serde_json::from_str(&content)?;

        let plugin_required = value
            .get("requiresSharedMemoryPlugin")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let has_telemetry_map = value.get("telemetryMap").and_then(Value::as_str).is_some();
        let has_force_map = value
            .get("forceFeedbackMap")
            .and_then(Value::as_str)
            .is_some();

        Ok(plugin_required && has_telemetry_map && has_force_map)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let mut root = Map::new();
        root.insert("enabled".to_string(), Value::from(config.enabled));
        root.insert("requiresSharedMemoryPlugin".to_string(), Value::from(true));
        root.insert(
            "telemetryMap".to_string(),
            Value::String("$rFactor2SMMP_Telemetry$".to_string()),
        );
        root.insert(
            "scoringMap".to_string(),
            Value::String("$rFactor2SMMP_Scoring$".to_string()),
        );
        root.insert(
            "forceFeedbackMap".to_string(),
            Value::String("$rFactor2SMMP_ForceFeedback$".to_string()),
        );
        root.insert(
            "updateRateHz".to_string(),
            Value::from(config.update_rate_hz),
        );

        Ok(vec![ConfigDiff {
            file_path: "UserData/player/OpenRacing.Telemetry.json".to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: serde_json::to_string_pretty(&Value::Object(root))?,
            operation: DiffOperation::Add,
        }])
    }
}

/// EA SPORTS WRC configuration writer.
///
/// EA WRC telemetry is configured through a generated telemetry folder under
/// `Documents/My Games/WRC/telemetry`.
pub struct EAWRCConfigWriter;

impl Default for EAWRCConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for EAWRCConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing EA WRC telemetry configuration");

        let telemetry_root = game_path.join("Documents/My Games/WRC/telemetry");
        let config_path = telemetry_root.join("config.json");
        let structure_path = telemetry_root
            .join("udp")
            .join(format!("{EAWRC_STRUCTURE_ID}.json"));

        let existed_before = config_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&config_path)?)
        } else {
            None
        };

        let mut root = existing_content
            .as_deref()
            .and_then(parse_json_object)
            .unwrap_or_default();

        let udp_value = root
            .entry("udp".to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        let udp_object = udp_value
            .as_object_mut()
            .ok_or_else(|| anyhow!("EA WRC config field 'udp' is not a JSON object"))?;

        let assignments_value = udp_object
            .entry("packetAssignments".to_string())
            .or_insert_with(|| Value::Array(Vec::new()));
        let assignments = assignments_value.as_array_mut().ok_or_else(|| {
            anyhow!("EA WRC config field 'udp.packetAssignments' is not a JSON array")
        })?;

        let listener_port = parse_target_port(&config.output_target).unwrap_or(EAWRC_DEFAULT_PORT);
        let listener_ip =
            parse_target_host(&config.output_target).unwrap_or_else(|| "127.0.0.1".to_string());

        let assignment = serde_json::json!({
            "packetId": EAWRC_PACKET_ID,
            "structureId": EAWRC_STRUCTURE_ID,
            "ip": listener_ip,
            "port": listener_port,
            "frequencyHz": i64::from(config.update_rate_hz),
            "bEnabled": config.enabled,
        });

        let mut updated_existing = false;
        for existing in assignments.iter_mut() {
            let same_packet = existing
                .get("packetId")
                .and_then(Value::as_str)
                .map(|value| value == EAWRC_PACKET_ID)
                .unwrap_or(false);
            let same_structure = existing
                .get("structureId")
                .and_then(Value::as_str)
                .map(|value| value == EAWRC_STRUCTURE_ID)
                .unwrap_or(false);

            if same_packet && same_structure {
                *existing = assignment.clone();
                updated_existing = true;
                break;
            }
        }

        if !updated_existing {
            assignments.push(assignment);
        }

        let new_config_content = serde_json::to_string_pretty(&Value::Object(root))?;

        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&config_path, &new_config_content)?;

        if let Some(parent) = structure_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let structure_content = serde_json::to_string_pretty(&eawrc_structure_definition())?;
        fs::write(&structure_path, &structure_content)?;

        Ok(vec![
            ConfigDiff {
                file_path: config_path.to_string_lossy().to_string(),
                section: None,
                key: "entire_file".to_string(),
                old_value: existing_content,
                new_value: new_config_content,
                operation: if existed_before {
                    DiffOperation::Modify
                } else {
                    DiffOperation::Add
                },
            },
            ConfigDiff {
                file_path: structure_path.to_string_lossy().to_string(),
                section: None,
                key: "entire_file".to_string(),
                old_value: None,
                new_value: structure_content,
                operation: DiffOperation::Add,
            },
        ])
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let telemetry_root = game_path.join("Documents/My Games/WRC/telemetry");
        let config_path = telemetry_root.join("config.json");
        let structure_path = telemetry_root
            .join("udp")
            .join(format!("{EAWRC_STRUCTURE_ID}.json"));

        if !config_path.exists() || !structure_path.exists() {
            return Ok(false);
        }

        let config_value: Value = serde_json::from_str(&fs::read_to_string(config_path)?)?;
        let assignments = config_value
            .get("udp")
            .and_then(Value::as_object)
            .and_then(|udp| udp.get("packetAssignments"))
            .and_then(Value::as_array)
            .or_else(|| {
                config_value
                    .get("packetAssignments")
                    .and_then(Value::as_array)
            });

        let assignment_ok = assignments
            .map(|entries| {
                entries.iter().any(|entry| {
                    let packet_ok = entry
                        .get("packetId")
                        .and_then(Value::as_str)
                        .map(|value| value == EAWRC_PACKET_ID)
                        .unwrap_or(false);
                    let structure_ok = entry
                        .get("structureId")
                        .and_then(Value::as_str)
                        .map(|value| value == EAWRC_STRUCTURE_ID)
                        .unwrap_or(false);
                    let enabled_ok = entry
                        .get("bEnabled")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    packet_ok && structure_ok && enabled_ok
                })
            })
            .unwrap_or(false);

        Ok(assignment_ok)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let listener_port = parse_target_port(&config.output_target).unwrap_or(EAWRC_DEFAULT_PORT);
        let listener_ip =
            parse_target_host(&config.output_target).unwrap_or_else(|| "127.0.0.1".to_string());

        let config_content = serde_json::to_string_pretty(&serde_json::json!({
            "udp": {
                "packetAssignments": [
                    {
                        "packetId": EAWRC_PACKET_ID,
                        "structureId": EAWRC_STRUCTURE_ID,
                        "ip": listener_ip,
                        "port": listener_port,
                        "frequencyHz": i64::from(config.update_rate_hz),
                        "bEnabled": config.enabled
                    }
                ]
            }
        }))?;
        let structure_content = serde_json::to_string_pretty(&eawrc_structure_definition())?;

        Ok(vec![
            ConfigDiff {
                file_path: "Documents/My Games/WRC/telemetry/config.json".to_string(),
                section: None,
                key: "entire_file".to_string(),
                old_value: None,
                new_value: config_content,
                operation: DiffOperation::Add,
            },
            ConfigDiff {
                file_path: format!(
                    "Documents/My Games/WRC/telemetry/udp/{EAWRC_STRUCTURE_ID}.json"
                ),
                section: None,
                key: "entire_file".to_string(),
                old_value: None,
                new_value: structure_content,
                operation: DiffOperation::Add,
            },
        ])
    }
}

fn eawrc_structure_definition() -> Value {
    serde_json::json!({
        "id": EAWRC_STRUCTURE_ID,
        "packets": [
            {
                "id": EAWRC_PACKET_ID,
                "header": {
                    "channels": ["packet_uid"]
                },
                "channels": [
                    "ffb_scalar",
                    "engine_rpm",
                    "vehicle_speed",
                    "gear",
                    "slip_ratio"
                ]
            }
        ]
    })
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

        for (index, line) in lines
            .iter()
            .enumerate()
            .take(section_end)
            .skip(search_start)
        {
            let trimmed = line.trim();
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

fn parse_target_host(target: &str) -> Option<String> {
    if let Ok(addr) = target.parse::<SocketAddr>() {
        return Some(addr.ip().to_string());
    }

    let (host_part, _) = target.rsplit_once(':')?;
    if host_part.starts_with('[') && host_part.ends_with(']') {
        return Some(
            host_part
                .trim_start_matches('[')
                .trim_end_matches(']')
                .to_string(),
        );
    }

    Some(host_part.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn test_ams2_writer_round_trip() -> TestResult {
        let writer = AMS2ConfigWriter;
        let temp_dir = tempdir()?;
        let config = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "shared_memory".to_string(),
            output_target: "127.0.0.1:12345".to_string(),
            fields: vec!["ffb_scalar".to_string()],
        };

        let diffs = writer.write_config(temp_dir.path(), &config)?;
        assert_eq!(diffs.len(), 1);
        assert!(writer.validate_config(temp_dir.path())?);
        Ok(())
    }

    #[test]
    fn test_rfactor2_writer_round_trip() -> TestResult {
        let writer = RFactor2ConfigWriter;
        let temp_dir = tempdir()?;
        let config = TelemetryConfig {
            enabled: true,
            update_rate_hz: 100,
            output_method: "shared_memory".to_string(),
            output_target: "127.0.0.1:12345".to_string(),
            fields: vec!["ffb_scalar".to_string()],
        };

        let diffs = writer.write_config(temp_dir.path(), &config)?;
        assert_eq!(diffs.len(), 1);
        assert!(writer.validate_config(temp_dir.path())?);
        Ok(())
    }

    #[test]
    fn test_eawrc_writer_round_trip() -> TestResult {
        let writer = EAWRCConfigWriter;
        let temp_dir = tempdir()?;
        let config = TelemetryConfig {
            enabled: true,
            update_rate_hz: 120,
            output_method: "udp_schema".to_string(),
            output_target: "127.0.0.1:20790".to_string(),
            fields: vec!["ffb_scalar".to_string(), "rpm".to_string()],
        };

        let diffs = writer.write_config(temp_dir.path(), &config)?;
        assert_eq!(diffs.len(), 2);
        assert!(writer.validate_config(temp_dir.path())?);

        let expected_structure = temp_dir
            .path()
            .join("Documents/My Games/WRC/telemetry/udp/openracing.json");
        assert!(expected_structure.exists());
        Ok(())
    }

    #[test]
    fn test_ac_rally_writer_round_trip() -> TestResult {
        let writer = ACRallyConfigWriter;
        let temp_dir = tempdir()?;
        let config = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "probe_discovery".to_string(),
            output_target: "127.0.0.1:9000".to_string(),
            fields: vec![],
        };

        let diffs = writer.write_config(temp_dir.path(), &config)?;
        assert_eq!(diffs.len(), 1);
        assert!(writer.validate_config(temp_dir.path())?);

        let probe_config = temp_dir
            .path()
            .join("Documents/Assetto Corsa Rally/Config/openracing_probe.json");
        assert!(probe_config.exists());
        Ok(())
    }

    #[test]
    fn test_acc_writer_round_trip_compat_schema() -> TestResult {
        let writer = ACCConfigWriter;
        let temp_dir = tempdir()?;
        let config = TelemetryConfig {
            enabled: true,
            update_rate_hz: 100,
            output_method: "udp_broadcast".to_string(),
            output_target: "127.0.0.1:9000".to_string(),
            fields: vec!["speed_ms".to_string()],
        };

        let diffs = writer.write_config(temp_dir.path(), &config)?;
        assert_eq!(diffs.len(), 1);
        assert!(writer.validate_config(temp_dir.path())?);

        let value: Value = serde_json::from_str(&diffs[0].new_value)?;
        assert_eq!(value["updListenerPort"], 9000);
        assert_eq!(value["udpListenerPort"], 9000);
        assert_eq!(value["connectionPassword"], "");
        assert_eq!(value["commandPassword"], "");
        assert!(value.get("broadcastingPort").is_none());
        assert!(value.get("updateRateHz").is_none());
        assert!(value.get("connectionId").is_none());
        Ok(())
    }

    #[test]
    fn test_acc_validate_accepts_udp_listener_port_only() -> TestResult {
        let writer = ACCConfigWriter;
        let temp_dir = tempdir()?;
        let config_path = temp_dir
            .path()
            .join("Documents/Assetto Corsa Competizione/Config/broadcasting.json");
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(
            &config_path,
            r#"{
  "udpListenerPort": 9000,
  "connectionPassword": "",
  "commandPassword": ""
}"#,
        )?;

        assert!(writer.validate_config(temp_dir.path())?);
        Ok(())
    }
}

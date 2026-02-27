//! Configuration writers for game-specific telemetry setup

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::fs;
use std::net::SocketAddr;
use std::path::Path;
use tracing::info;

/// Configuration to be applied to a game
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryConfig {
    pub enabled: bool,
    pub update_rate_hz: u32,
    pub output_method: String,
    pub output_target: String,
    pub fields: Vec<String>,
    #[serde(default)]
    pub enable_high_rate_iracing_360hz: bool,
}

/// Represents a configuration change made to a game file
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfigDiff {
    pub file_path: String,
    pub section: Option<String>,
    pub key: String,
    pub old_value: Option<String>,
    pub new_value: String,
    pub operation: DiffOperation,
}

/// Type of configuration operation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DiffOperation {
    Add,
    Modify,
    Remove,
}

/// Configuration writer trait for game-specific config generation
pub trait ConfigWriter {
    /// Write telemetry configuration for the game
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>>;

    /// Validate that configuration was applied correctly
    fn validate_config(&self, game_path: &Path) -> Result<bool>;

    /// Get the expected configuration diffs for testing
    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>>;
}

/// Factory for constructing config writer instances.
pub type ConfigWriterFactory = fn() -> Box<dyn ConfigWriter + Send + Sync>;

fn new_iracing_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(IRacingConfigWriter)
}

fn new_acc_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(ACCConfigWriter)
}

fn new_ac_rally_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(ACRallyConfigWriter)
}

fn new_ams2_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(AMS2ConfigWriter)
}

fn new_rfactor2_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(RFactor2ConfigWriter)
}

fn new_eawrc_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(EAWRCConfigWriter)
}

fn new_dirt5_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(Dirt5ConfigWriter)
}

fn new_dirt_rally_2_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(DirtRally2ConfigWriter)
}

fn new_rbr_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(RBRConfigWriter)
}

fn new_gran_turismo_7_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(GranTurismo7ConfigWriter)
}

fn new_assetto_corsa_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(AssettoCorsaConfigWriter)
}

fn new_forza_motorsport_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(ForzaMotorsportConfigWriter)
}

fn new_beamng_drive_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(BeamNGDriveConfigWriter)
}

fn new_wrc_generations_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(WrcGenerationsConfigWriter)
}

fn new_dirt4_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(Dirt4ConfigWriter)
}

fn new_f1_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(F1ConfigWriter)
}

fn new_f1_25_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(F1_25ConfigWriter)
}

fn new_project_cars_2_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(PCars2ConfigWriter)
}

fn new_live_for_speed_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(LFSConfigWriter)
}

fn new_ets2_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(Ets2ConfigWriter)
}

fn new_ats_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(AtsConfigWriter)
}

fn new_wreckfest_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(WreckfestConfigWriter)
}

fn new_rennsport_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(RennsportConfigWriter)
}

fn new_kartkraft_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(KartKraftConfigWriter)
}

fn new_raceroom_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(RaceRoomConfigWriter)
}

fn new_grid_autosport_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(GridAutosportConfigWriter)
}

fn new_grid_2019_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(Grid2019ConfigWriter)
}

fn new_grid_legends_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(GridLegendsConfigWriter)
}

fn new_automobilista_config_writer() -> Box<dyn ConfigWriter + Send + Sync> {
    Box::new(AutomobilistaConfigWriter)
}

/// Returns the canonical config writer registry for all supported integrations.
pub fn config_writer_factories() -> &'static [(&'static str, ConfigWriterFactory)] {
    &[
        ("iracing", new_iracing_config_writer),
        ("acc", new_acc_config_writer),
        ("ac_rally", new_ac_rally_config_writer),
        ("ams2", new_ams2_config_writer),
        ("rfactor2", new_rfactor2_config_writer),
        ("eawrc", new_eawrc_config_writer),
        ("f1", new_f1_config_writer),
        ("f1_25", new_f1_25_config_writer),
        ("dirt5", new_dirt5_config_writer),
        ("dirt_rally_2", new_dirt_rally_2_config_writer),
        ("rbr", new_rbr_config_writer),
        ("gran_turismo_7", new_gran_turismo_7_config_writer),
        ("assetto_corsa", new_assetto_corsa_config_writer),
        ("forza_motorsport", new_forza_motorsport_config_writer),
        ("beamng_drive", new_beamng_drive_config_writer),
        ("project_cars_2", new_project_cars_2_config_writer),
        ("live_for_speed", new_live_for_speed_config_writer),
        ("wrc_generations", new_wrc_generations_config_writer),
        ("dirt4", new_dirt4_config_writer),
        ("ets2", new_ets2_config_writer),
        ("ats", new_ats_config_writer),
        ("wreckfest", new_wreckfest_config_writer),
        ("rennsport", new_rennsport_config_writer),
        ("raceroom", new_raceroom_config_writer),
        ("kartkraft", new_kartkraft_config_writer),
        ("grid_autosport", new_grid_autosport_config_writer),
        ("grid_2019", new_grid_2019_config_writer),
        ("grid_legends", new_grid_legends_config_writer),
        ("automobilista", new_automobilista_config_writer),
    ]
}

const EAWRC_STRUCTURE_ID: &str = "openracing";
const EAWRC_PACKET_ID: &str = "session_update";
const EAWRC_DEFAULT_PORT: u16 = 20778;
const AC_RALLY_DEFAULT_DISCOVERY_PORT: u16 = 9000;
const AC_RALLY_PROBE_RELATIVE_PATH: &str =
    "Documents/Assetto Corsa Rally/Config/openracing_probe.json";
const IRACING_360HZ_KEY: &str = "irsdkLog360Hz";
const DIRT5_BRIDGE_RELATIVE_PATH: &str = "Documents/OpenRacing/dirt5_bridge_contract.json";
const DIRT5_BRIDGE_PROTOCOL: &str = "codemasters_udp";
const DIRT5_DEFAULT_PORT: u16 = 20777;
const DIRT5_DEFAULT_MODE: u8 = 1;
const DIRT_RALLY_2_BRIDGE_RELATIVE_PATH: &str =
    "Documents/OpenRacing/dirt_rally_2_bridge_contract.json";
const DIRT_RALLY_2_BRIDGE_PROTOCOL: &str = "codemasters_udp";
const DIRT_RALLY_2_DEFAULT_PORT: u16 = 20777;
const DIRT_RALLY_2_DEFAULT_MODE: u8 = 1;
const RBR_BRIDGE_RELATIVE_PATH: &str = "Documents/OpenRacing/rbr_bridge_contract.json";
const RBR_BRIDGE_PROTOCOL: &str = "rbr_livedata_udp";
const RBR_DEFAULT_PORT: u16 = 6776;
const F1_BRIDGE_RELATIVE_PATH: &str = "Documents/OpenRacing/f1_bridge_contract.json";
const F1_BRIDGE_PROTOCOL: &str = "codemasters_udp";
const F1_DEFAULT_PORT: u16 = 20777;
const F1_DEFAULT_MODE: u8 = 3;
const F1_25_CONTRACT_RELATIVE_PATH: &str = "Documents/OpenRacing/f1_25_contract.json";
const F1_25_NATIVE_PROTOCOL: &str = "f1_25_native_udp";
const F1_25_DEFAULT_PORT: u16 = 20777;
const WRC_GENERATIONS_BRIDGE_RELATIVE_PATH: &str =
    "Documents/OpenRacing/wrc_generations_bridge_contract.json";
const WRC_GENERATIONS_BRIDGE_PROTOCOL: &str = "codemasters_udp";
const WRC_GENERATIONS_DEFAULT_PORT: u16 = 6777;
const WRC_GENERATIONS_DEFAULT_MODE: u8 = 1;
const DIRT4_BRIDGE_RELATIVE_PATH: &str = "Documents/OpenRacing/dirt4_bridge_contract.json";
const DIRT4_BRIDGE_PROTOCOL: &str = "codemasters_udp";
const DIRT4_DEFAULT_PORT: u16 = 20777;
const DIRT4_DEFAULT_MODE: u8 = 1;

const ETS2_BRIDGE_RELATIVE_PATH: &str = "Documents/OpenRacing/ets2_bridge_contract.json";
const ETS2_BRIDGE_PROTOCOL: &str = "scs_shared_memory";
const ETS2_DEFAULT_PORT: u16 = 0;

const ATS_BRIDGE_RELATIVE_PATH: &str = "Documents/OpenRacing/ats_bridge_contract.json";
const ATS_BRIDGE_PROTOCOL: &str = "scs_shared_memory";
const ATS_DEFAULT_PORT: u16 = 0;

const WRECKFEST_BRIDGE_RELATIVE_PATH: &str = "Documents/OpenRacing/wreckfest_bridge_contract.json";
const WRECKFEST_BRIDGE_PROTOCOL: &str = "udp_wreckfest";
const WRECKFEST_DEFAULT_PORT: u16 = 5606;

const RENNSPORT_BRIDGE_RELATIVE_PATH: &str = "Documents/OpenRacing/rennsport_bridge_contract.json";
const RENNSPORT_BRIDGE_PROTOCOL: &str = "udp_rennsport";
const RENNSPORT_DEFAULT_PORT: u16 = 9000;

const GRID_AUTOSPORT_BRIDGE_RELATIVE_PATH: &str =
    "Documents/OpenRacing/grid_autosport_bridge_contract.json";
const GRID_AUTOSPORT_BRIDGE_PROTOCOL: &str = "codemasters_udp";
const GRID_AUTOSPORT_DEFAULT_PORT: u16 = 20777;
const GRID_AUTOSPORT_DEFAULT_MODE: u8 = 1;

const GRID_2019_BRIDGE_RELATIVE_PATH: &str =
    "Documents/OpenRacing/grid_2019_bridge_contract.json";
const GRID_2019_BRIDGE_PROTOCOL: &str = "codemasters_udp";
const GRID_2019_DEFAULT_PORT: u16 = 20777;
const GRID_2019_DEFAULT_MODE: u8 = 1;

const GRID_LEGENDS_BRIDGE_RELATIVE_PATH: &str =
    "Documents/OpenRacing/grid_legends_bridge_contract.json";
const GRID_LEGENDS_BRIDGE_PROTOCOL: &str = "codemasters_udp";
const GRID_LEGENDS_DEFAULT_PORT: u16 = 20777;
const GRID_LEGENDS_DEFAULT_MODE: u8 = 1;

const AUTOMOBILISTA_BRIDGE_RELATIVE_PATH: &str =
    "Documents/OpenRacing/automobilista_bridge_contract.json";
const AUTOMOBILISTA_BRIDGE_PROTOCOL: &str = "isi_rf1_shared_memory";

const KARTKRAFT_BRIDGE_RELATIVE_PATH: &str = "Documents/OpenRacing/kartkraft_bridge_contract.json";
const KARTKRAFT_BRIDGE_PROTOCOL: &str = "udp_flatbuffers_kartkraft";
const KARTKRAFT_DEFAULT_PORT: u16 = 5000;

const RACEROOM_BRIDGE_RELATIVE_PATH: &str = "Documents/OpenRacing/raceroom_bridge_contract.json";
const RACEROOM_BRIDGE_PROTOCOL: &str = "r3e_shared_memory";

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

        let existing_content = if app_ini_path.exists() {
            fs::read_to_string(&app_ini_path)?
        } else {
            String::new()
        };

        let (mut new_content, prior_value, operation) = upsert_ini_value(
            &existing_content,
            "Telemetry",
            "telemetryDiskFile",
            telemetry_enabled,
        );

        let mut diffs = vec![ConfigDiff {
            file_path: app_ini_path.to_string_lossy().to_string(),
            section: Some("Telemetry".to_string()),
            key: "telemetryDiskFile".to_string(),
            old_value: prior_value,
            new_value: telemetry_enabled.to_string(),
            operation,
        }];

        if config.enable_high_rate_iracing_360hz {
            let (updated_content, prior_360hz_value, operation_360hz) =
                upsert_ini_value(&new_content, "Telemetry", IRACING_360HZ_KEY, "1");
            new_content = updated_content;
            diffs.push(ConfigDiff {
                file_path: app_ini_path.to_string_lossy().to_string(),
                section: Some("Telemetry".to_string()),
                key: IRACING_360HZ_KEY.to_string(),
                old_value: prior_360hz_value,
                new_value: "1".to_string(),
                operation: operation_360hz,
            });
        }

        if let Some(parent) = app_ini_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&app_ini_path, &new_content)?;

        Ok(diffs)
    }

    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let app_ini_path = game_path.join("Documents/iRacing/app.ini");

        if !app_ini_path.exists() {
            return Ok(false);
        }

        let content = fs::read_to_string(app_ini_path)?;

        let has_telemetry_section = content.contains("[Telemetry]");
        let has_telemetry_enabled = content
            .lines()
            .any(|line| line.trim().eq_ignore_ascii_case("telemetryDiskFile=1"));

        Ok(has_telemetry_section && has_telemetry_enabled)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let telemetry_enabled = if config.enabled { "1" } else { "0" };

        let mut diffs = vec![ConfigDiff {
            file_path: "Documents/iRacing/app.ini".to_string(),
            section: Some("Telemetry".to_string()),
            key: "telemetryDiskFile".to_string(),
            old_value: None,
            new_value: telemetry_enabled.to_string(),
            operation: DiffOperation::Add,
        }];

        if config.enable_high_rate_iracing_360hz {
            diffs.push(ConfigDiff {
                file_path: "Documents/iRacing/app.ini".to_string(),
                section: Some("Telemetry".to_string()),
                key: IRACING_360HZ_KEY.to_string(),
                old_value: None,
                new_value: "1".to_string(),
                operation: DiffOperation::Add,
            });
        }

        Ok(diffs)
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
        let connection_id = existing_map
            .get("connectionId")
            .cloned()
            .unwrap_or_else(|| Value::String(String::new()));
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

        let mut broadcasting_config = existing_map;
        broadcasting_config.insert("updListenerPort".to_string(), Value::from(listener_port));
        broadcasting_config.insert("udpListenerPort".to_string(), Value::from(listener_port));
        broadcasting_config.insert("broadcastingPort".to_string(), Value::from(listener_port));
        broadcasting_config.insert("connectionId".to_string(), connection_id);
        broadcasting_config.insert(
            "connectionPassword".to_string(),
            Value::String(connection_password),
        );
        broadcasting_config.insert(
            "commandPassword".to_string(),
            Value::String(command_password),
        );
        broadcasting_config.insert(
            "updateRateHz".to_string(),
            Value::from(config.update_rate_hz),
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

        let has_listener_port = object
            .get("updListenerPort")
            .or_else(|| object.get("udpListenerPort"))
            .and_then(Value::as_u64)
            .is_some();
        let has_broadcasting_port = object
            .get("broadcastingPort")
            .and_then(Value::as_u64)
            .is_some();
        let has_connection_id = object
            .get("connectionId")
            .map(|value| !value.is_null())
            .unwrap_or(false);
        let has_connection_password = object
            .get("connectionPassword")
            .and_then(Value::as_str)
            .is_some();
        let has_command_password = object
            .get("commandPassword")
            .and_then(Value::as_str)
            .is_some();
        let has_update_rate = object.get("updateRateHz").and_then(Value::as_u64).is_some();

        Ok(has_listener_port
            && has_broadcasting_port
            && has_connection_id
            && has_connection_password
            && has_command_password
            && has_update_rate)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let listener_port = parse_target_port(&config.output_target).unwrap_or(9000);
        let mut broadcasting_config = Map::new();
        broadcasting_config.insert("updListenerPort".to_string(), Value::from(listener_port));
        broadcasting_config.insert("udpListenerPort".to_string(), Value::from(listener_port));
        broadcasting_config.insert("broadcastingPort".to_string(), Value::from(listener_port));
        broadcasting_config.insert("connectionId".to_string(), Value::String(String::new()));
        broadcasting_config.insert(
            "connectionPassword".to_string(),
            Value::String(String::new()),
        );
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

/// Assetto Corsa Rally configuration writer.
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
pub struct EAWRCConfigWriter;

impl Default for EAWRCConfigWriter {
    fn default() -> Self {
        Self
    }
}

/// Dirt 5 configuration writer.
pub struct Dirt5ConfigWriter;

impl Default for Dirt5ConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for Dirt5ConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing Dirt 5 bridge contract configuration");

        let contract_path = game_path.join(DIRT5_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };

        let udp_port = parse_target_port(&config.output_target).unwrap_or(DIRT5_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "dirt5",
            "telemetry_protocol": DIRT5_BRIDGE_PROTOCOL,
            "mode": DIRT5_DEFAULT_MODE,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "Dirt 5 telemetry is bridge-backed; no native game config is modified.",
        });

        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;

        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
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
        let contract_path = game_path.join(DIRT5_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }

        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;

        let valid_protocol = value
            .get("telemetry_protocol")
            .and_then(Value::as_str)
            .map(|value| value == DIRT5_BRIDGE_PROTOCOL)
            .unwrap_or(false);
        let valid_game = value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|value| value == "dirt5")
            .unwrap_or(false);

        Ok(valid_protocol && valid_game)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(DIRT5_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "dirt5",
            "telemetry_protocol": DIRT5_BRIDGE_PROTOCOL,
            "mode": DIRT5_DEFAULT_MODE,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "Dirt 5 telemetry is bridge-backed; no native game config is modified.",
        });
        let expected = serde_json::to_string_pretty(&contract)?;

        Ok(vec![ConfigDiff {
            file_path: DIRT5_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: expected,
            operation: DiffOperation::Add,
        }])
    }
}

/// DiRT Rally 2.0 configuration writer.
///
/// DiRT Rally 2.0 uses the same Codemasters UDP Mode 1 format as DiRT 5.
/// This writer creates a bridge contract file for the OpenRacing telemetry pipeline.
pub struct DirtRally2ConfigWriter;

impl Default for DirtRally2ConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for DirtRally2ConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing DiRT Rally 2.0 bridge contract configuration");

        let contract_path = game_path.join(DIRT_RALLY_2_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };

        let udp_port =
            parse_target_port(&config.output_target).unwrap_or(DIRT_RALLY_2_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "dirt_rally_2",
            "telemetry_protocol": DIRT_RALLY_2_BRIDGE_PROTOCOL,
            "mode": DIRT_RALLY_2_DEFAULT_MODE,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "DiRT Rally 2.0 telemetry uses Codemasters UDP Mode 1. Enable UDP telemetry in the game's hardware settings.",
        });

        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;

        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
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
        let contract_path = game_path.join(DIRT_RALLY_2_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }

        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;

        let valid_protocol = value
            .get("telemetry_protocol")
            .and_then(Value::as_str)
            .map(|v| v == DIRT_RALLY_2_BRIDGE_PROTOCOL)
            .unwrap_or(false);
        let valid_game = value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "dirt_rally_2")
            .unwrap_or(false);

        Ok(valid_protocol && valid_game)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port =
            parse_target_port(&config.output_target).unwrap_or(DIRT_RALLY_2_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "dirt_rally_2",
            "telemetry_protocol": DIRT_RALLY_2_BRIDGE_PROTOCOL,
            "mode": DIRT_RALLY_2_DEFAULT_MODE,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "DiRT Rally 2.0 telemetry uses Codemasters UDP Mode 1. Enable UDP telemetry in the game's hardware settings.",
        });
        let expected = serde_json::to_string_pretty(&contract)?;

        Ok(vec![ConfigDiff {
            file_path: DIRT_RALLY_2_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: expected,
            operation: DiffOperation::Add,
        }])
    }
}

/// Richard Burns Rally configuration writer.
///
/// RBR does not have native UDP telemetry output; it requires the RSF Rallysimfans plugin.
/// This writer creates a bridge contract file documenting the expected UDP connection.
pub struct RBRConfigWriter;

impl Default for RBRConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for RBRConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing RBR bridge contract configuration");

        let contract_path = game_path.join(RBR_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };

        let udp_port = parse_target_port(&config.output_target).unwrap_or(RBR_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "rbr",
            "telemetry_protocol": RBR_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "RBR requires the RSF Rallysimfans plugin for UDP telemetry. Configure the plugin to send LiveData to the OpenRacing port.",
        });

        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;

        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
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
        let contract_path = game_path.join(RBR_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }

        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;

        let valid_protocol = value
            .get("telemetry_protocol")
            .and_then(Value::as_str)
            .map(|v| v == RBR_BRIDGE_PROTOCOL)
            .unwrap_or(false);
        let valid_game = value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "rbr")
            .unwrap_or(false);

        Ok(valid_protocol && valid_game)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(RBR_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "rbr",
            "telemetry_protocol": RBR_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "RBR requires the RSF Rallysimfans plugin for UDP telemetry. Configure the plugin to send LiveData to the OpenRacing port.",
        });
        let expected = serde_json::to_string_pretty(&contract)?;

        Ok(vec![ConfigDiff {
            file_path: RBR_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: expected,
            operation: DiffOperation::Add,
        }])
    }
}

/// Gran Turismo 7 configuration writer.
///
/// GT7 is a PlayStation-exclusive title; there is no PC executable or config file to write.
/// This writer creates a bridge contract that documents the Salsa20-encrypted UDP connection.
pub struct GranTurismo7ConfigWriter;

impl Default for GranTurismo7ConfigWriter {
    fn default() -> Self {
        Self
    }
}

const GT7_BRIDGE_RELATIVE_PATH: &str = "Documents/OpenRacing/gran_turismo_7_bridge_contract.json";
const GT7_BRIDGE_PROTOCOL: &str = "gt7_salsa20_udp";
const GT7_DEFAULT_PORT: u16 = 33740;

impl ConfigWriter for GranTurismo7ConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing Gran Turismo 7 bridge contract configuration");

        let contract_path = game_path.join(GT7_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };

        let udp_port = parse_target_port(&config.output_target).unwrap_or(GT7_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "gran_turismo_7",
            "telemetry_protocol": GT7_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "GT7 sends Salsa20-encrypted UDP packets from the PS4/PS5 to this port. Enable telemetry in GT7 Settings > Options > Machine/Car Settings > Vehicle Data Output.",
        });

        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;

        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
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
        let contract_path = game_path.join(GT7_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }

        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;

        let valid_protocol = value
            .get("telemetry_protocol")
            .and_then(Value::as_str)
            .map(|v| v == GT7_BRIDGE_PROTOCOL)
            .unwrap_or(false);
        let valid_game = value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "gran_turismo_7")
            .unwrap_or(false);

        Ok(valid_protocol && valid_game)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(GT7_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "gran_turismo_7",
            "telemetry_protocol": GT7_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "GT7 sends Salsa20-encrypted UDP packets from the PS4/PS5 to this port. Enable telemetry in GT7 Settings > Options > Machine/Car Settings > Vehicle Data Output.",
        });
        let expected = serde_json::to_string_pretty(&contract)?;

        Ok(vec![ConfigDiff {
            file_path: GT7_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: expected,
            operation: DiffOperation::Add,
        }])
    }
}

/// F1 configuration writer.
pub struct F1ConfigWriter;

impl Default for F1ConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for F1ConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing F1 bridge contract configuration");

        let contract_path = game_path.join(F1_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };

        let udp_port = parse_target_port(&config.output_target).unwrap_or(F1_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "f1",
            "telemetry_protocol": F1_BRIDGE_PROTOCOL,
            "mode": F1_DEFAULT_MODE,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "F1 telemetry is bridge-backed; no native game config is modified.",
        });

        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;

        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
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
        let contract_path = game_path.join(F1_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }

        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;

        let valid_protocol = value
            .get("telemetry_protocol")
            .and_then(Value::as_str)
            .map(|value| value == F1_BRIDGE_PROTOCOL)
            .unwrap_or(false);
        let valid_game = value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|value| value == "f1")
            .unwrap_or(false);

        Ok(valid_protocol && valid_game)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(F1_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "f1",
            "telemetry_protocol": F1_BRIDGE_PROTOCOL,
            "mode": F1_DEFAULT_MODE,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "F1 telemetry is bridge-backed; no native game config is modified.",
        });
        let expected = serde_json::to_string_pretty(&contract)?;

        Ok(vec![ConfigDiff {
            file_path: F1_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: expected,
            operation: DiffOperation::Add,
        }])
    }
}

/// F1 25 native UDP configuration writer.
pub struct F1_25ConfigWriter;

impl Default for F1_25ConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for F1_25ConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing F1 25 native UDP contract configuration");

        let contract_path = game_path.join(F1_25_CONTRACT_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };

        let udp_port = parse_target_port(&config.output_target).unwrap_or(F1_25_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "f1_25",
            "telemetry_protocol": F1_25_NATIVE_PROTOCOL,
            "packet_format": 2025,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "setup_notes": [
                "In F1 25 game settings, enable UDP telemetry:",
                "  UDP Telemetry: On",
                "  UDP Broadcast Mode: Off",
                "  UDP IP Address: 127.0.0.1",
                "  UDP Port: 20777",
                "  UDP Send Rate: 60Hz",
                "  UDP Format: 2025"
            ],
            "supported_packets": ["session (1)", "car_telemetry (6)", "car_status (7)"],
        });

        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;

        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
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
        let contract_path = game_path.join(F1_25_CONTRACT_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }

        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;

        let valid_protocol = value
            .get("telemetry_protocol")
            .and_then(Value::as_str)
            .map(|v| v == F1_25_NATIVE_PROTOCOL)
            .unwrap_or(false);
        let valid_game = value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "f1_25")
            .unwrap_or(false);
        let valid_format = value
            .get("packet_format")
            .and_then(Value::as_u64)
            .map(|v| v == 2025)
            .unwrap_or(false);

        Ok(valid_protocol && valid_game && valid_format)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(F1_25_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "f1_25",
            "telemetry_protocol": F1_25_NATIVE_PROTOCOL,
            "packet_format": 2025,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "setup_notes": [
                "In F1 25 game settings, enable UDP telemetry:",
                "  UDP Telemetry: On",
                "  UDP Broadcast Mode: Off",
                "  UDP IP Address: 127.0.0.1",
                "  UDP Port: 20777",
                "  UDP Send Rate: 60Hz",
                "  UDP Format: 2025"
            ],
            "supported_packets": ["session (1)", "car_telemetry (6)", "car_status (7)"],
        });
        let expected = serde_json::to_string_pretty(&contract)?;

        Ok(vec![ConfigDiff {
            file_path: F1_25_CONTRACT_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: expected,
            operation: DiffOperation::Add,
        }])
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
            "enabled": config.enabled,
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
                        .or_else(|| entry.get("enabled").and_then(Value::as_bool))
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
                        "bEnabled": config.enabled,
                        "enabled": config.enabled
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

/// Assetto Corsa (original) configuration writer.
///
/// AC uses the OutGauge UDP protocol (port 9996). Since the OutGauge output target
/// is configured inside the game, this writer creates a bridge contract that documents
/// the expected UDP listener configuration.
pub struct AssettoCorsaConfigWriter;

impl Default for AssettoCorsaConfigWriter {
    fn default() -> Self {
        Self
    }
}

const AC_BRIDGE_RELATIVE_PATH: &str =
    "Documents/OpenRacing/assetto_corsa_bridge_contract.json";
const AC_BRIDGE_PROTOCOL: &str = "ac_outgauge_udp";
const AC_DEFAULT_PORT: u16 = 9996;

impl ConfigWriter for AssettoCorsaConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing Assetto Corsa bridge contract configuration");

        let contract_path = game_path.join(AC_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };

        let udp_port = parse_target_port(&config.output_target).unwrap_or(AC_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "assetto_corsa",
            "telemetry_protocol": AC_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "setup_notes": [
                "In Assetto Corsa, enable OutGauge in the Documents/Assetto Corsa/cfg/openracing.ini file:",
                "  [OutGauge]",
                "  Mode=2",
                "  IP=127.0.0.1",
                "  Port=9996",
                "  Delay=0",
                "  ID=1"
            ],
        });

        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;

        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
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
        let contract_path = game_path.join(AC_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }

        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;

        let valid_protocol = value
            .get("telemetry_protocol")
            .and_then(Value::as_str)
            .map(|v| v == AC_BRIDGE_PROTOCOL)
            .unwrap_or(false);
        let valid_game = value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "assetto_corsa")
            .unwrap_or(false);

        Ok(valid_protocol && valid_game)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(AC_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "assetto_corsa",
            "telemetry_protocol": AC_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "setup_notes": [
                "In Assetto Corsa, enable OutGauge in the Documents/Assetto Corsa/cfg/openracing.ini file:",
                "  [OutGauge]",
                "  Mode=2",
                "  IP=127.0.0.1",
                "  Port=9996",
                "  Delay=0",
                "  ID=1"
            ],
        });
        let expected = serde_json::to_string_pretty(&contract)?;

        Ok(vec![ConfigDiff {
            file_path: AC_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: expected,
            operation: DiffOperation::Add,
        }])
    }
}

/// Forza Motorsport / Forza Horizon configuration writer.
///
/// Forza's "Data Out" feature is configured in-game only. This writer creates a bridge
/// contract documenting the expected UDP listener on port 5300.
pub struct ForzaMotorsportConfigWriter;

impl Default for ForzaMotorsportConfigWriter {
    fn default() -> Self {
        Self
    }
}

const FORZA_BRIDGE_RELATIVE_PATH: &str =
    "Documents/OpenRacing/forza_motorsport_bridge_contract.json";
const FORZA_BRIDGE_PROTOCOL: &str = "forza_data_out_udp";
const FORZA_DEFAULT_PORT: u16 = 5300;

impl ConfigWriter for ForzaMotorsportConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing Forza Motorsport bridge contract configuration");

        let contract_path = game_path.join(FORZA_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };

        let udp_port = parse_target_port(&config.output_target).unwrap_or(FORZA_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "forza_motorsport",
            "telemetry_protocol": FORZA_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "supported_formats": ["sled_232", "cardash_311"],
            "setup_notes": [
                "In Forza Motorsport / Forza Horizon, enable Data Out in game settings:",
                "  HUD and Gameplay > Data Out > On",
                "  Data Out IP Address: 127.0.0.1",
                "  Data Out IP Port: 5300"
            ],
        });

        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;

        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
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
        let contract_path = game_path.join(FORZA_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }

        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;

        let valid_protocol = value
            .get("telemetry_protocol")
            .and_then(Value::as_str)
            .map(|v| v == FORZA_BRIDGE_PROTOCOL)
            .unwrap_or(false);
        let valid_game = value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "forza_motorsport")
            .unwrap_or(false);

        Ok(valid_protocol && valid_game)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(FORZA_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "forza_motorsport",
            "telemetry_protocol": FORZA_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "supported_formats": ["sled_232", "cardash_311"],
            "setup_notes": [
                "In Forza Motorsport / Forza Horizon, enable Data Out in game settings:",
                "  HUD and Gameplay > Data Out > On",
                "  Data Out IP Address: 127.0.0.1",
                "  Data Out IP Port: 5300"
            ],
        });
        let expected = serde_json::to_string_pretty(&contract)?;

        Ok(vec![ConfigDiff {
            file_path: FORZA_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: expected,
            operation: DiffOperation::Add,
        }])
    }
}

/// BeamNG.drive configuration writer.
///
/// BeamNG.drive exposes telemetry via the OutGauge protocol (port 4444), enabled through
/// its in-game apps system. This writer creates a bridge contract documenting the listener.
pub struct BeamNGDriveConfigWriter;

impl Default for BeamNGDriveConfigWriter {
    fn default() -> Self {
        Self
    }
}

const BEAMNG_BRIDGE_RELATIVE_PATH: &str =
    "Documents/OpenRacing/beamng_drive_bridge_contract.json";
const BEAMNG_BRIDGE_PROTOCOL: &str = "beamng_outgauge_udp";
const BEAMNG_DEFAULT_PORT: u16 = 4444;

impl ConfigWriter for BeamNGDriveConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing BeamNG.drive bridge contract configuration");

        let contract_path = game_path.join(BEAMNG_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };

        let udp_port = parse_target_port(&config.output_target).unwrap_or(BEAMNG_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "beamng_drive",
            "telemetry_protocol": BEAMNG_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "packet_format": "lfs_outgauge_96bytes",
            "setup_notes": [
                "In BeamNG.drive, enable the OutGauge app from the apps menu.",
                "Set the UDP IP to 127.0.0.1 and port to 4444.",
                "Alternatively, edit settings/electrics.json to enable OutGauge."
            ],
        });

        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;

        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
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
        let contract_path = game_path.join(BEAMNG_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }

        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;

        let valid_protocol = value
            .get("telemetry_protocol")
            .and_then(Value::as_str)
            .map(|v| v == BEAMNG_BRIDGE_PROTOCOL)
            .unwrap_or(false);
        let valid_game = value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "beamng_drive")
            .unwrap_or(false);

        Ok(valid_protocol && valid_game)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(BEAMNG_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "beamng_drive",
            "telemetry_protocol": BEAMNG_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "packet_format": "lfs_outgauge_96bytes",
            "setup_notes": [
                "In BeamNG.drive, enable the OutGauge app from the apps menu.",
                "Set the UDP IP to 127.0.0.1 and port to 4444.",
                "Alternatively, edit settings/electrics.json to enable OutGauge."
            ],
        });
        let expected = serde_json::to_string_pretty(&contract)?;

        Ok(vec![ConfigDiff {
            file_path: BEAMNG_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: expected,
            operation: DiffOperation::Add,
        }])
    }
}

const PCARS2_BRIDGE_RELATIVE_PATH: &str = "Documents/OpenRacing/project_cars_2_bridge_contract.json";
const PCARS2_BRIDGE_PROTOCOL: &str = "sms_udp_pcars2";
const PCARS2_DEFAULT_PORT: u16 = 5606;

/// Project CARS 2 configuration writer.
///
/// PCARS2 supports shared memory (`Local\$pcars2$`) on Windows and UDP telemetry on port 5606.
/// This writer creates a bridge contract documenting the UDP listener configuration.
pub struct PCars2ConfigWriter;

impl Default for PCars2ConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for PCars2ConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing Project CARS 2 bridge contract configuration");

        let contract_path = game_path.join(PCARS2_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };

        let udp_port = parse_target_port(&config.output_target).unwrap_or(PCARS2_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "project_cars_2",
            "telemetry_protocol": PCARS2_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "setup_notes": [
                "In Project CARS 2, enable UDP telemetry in Options > Visual > UDP Frequency.",
                "Set UDP IP Address to 127.0.0.1 and Port to 5606.",
                "Alternatively, shared memory is used automatically on Windows."
            ],
        });

        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;

        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
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
        let contract_path = game_path.join(PCARS2_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }

        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;

        let valid_protocol = value
            .get("telemetry_protocol")
            .and_then(Value::as_str)
            .map(|v| v == PCARS2_BRIDGE_PROTOCOL)
            .unwrap_or(false);
        let valid_game = value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "project_cars_2")
            .unwrap_or(false);

        Ok(valid_protocol && valid_game)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(PCARS2_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "project_cars_2",
            "telemetry_protocol": PCARS2_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "setup_notes": [
                "In Project CARS 2, enable UDP telemetry in Options > Visual > UDP Frequency.",
                "Set UDP IP Address to 127.0.0.1 and Port to 5606.",
                "Alternatively, shared memory is used automatically on Windows."
            ],
        });
        let expected = serde_json::to_string_pretty(&contract)?;

        Ok(vec![ConfigDiff {
            file_path: PCARS2_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: expected,
            operation: DiffOperation::Add,
        }])
    }
}

const LFS_BRIDGE_RELATIVE_PATH: &str = "Documents/OpenRacing/live_for_speed_bridge_contract.json";
const LFS_BRIDGE_PROTOCOL: &str = "lfs_outgauge_udp";
const LFS_DEFAULT_PORT: u16 = 30000;

/// Live For Speed configuration writer.
///
/// LFS exposes telemetry via the OutGauge UDP protocol. Enable OutGauge in LFS `cfg.lfs` or
/// via the in-game options. This writer creates a bridge contract documenting the listener.
pub struct LFSConfigWriter;

impl Default for LFSConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for LFSConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing Live For Speed bridge contract configuration");

        let contract_path = game_path.join(LFS_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };

        let udp_port = parse_target_port(&config.output_target).unwrap_or(LFS_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "live_for_speed",
            "telemetry_protocol": LFS_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "packet_format": "lfs_outgauge_96bytes",
            "setup_notes": [
                "In LFS, enable OutGauge in Options > Output or edit cfg.lfs directly.",
                "Set OutGauge IP to 127.0.0.1 and Port to 30000.",
                "Example cfg.lfs entry: OutGauge Mode 1 Addr 127.0.0.1 Port 30000 Id 1 Delay 1"
            ],
        });

        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;

        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
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
        let contract_path = game_path.join(LFS_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }

        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;

        let valid_protocol = value
            .get("telemetry_protocol")
            .and_then(Value::as_str)
            .map(|v| v == LFS_BRIDGE_PROTOCOL)
            .unwrap_or(false);
        let valid_game = value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "live_for_speed")
            .unwrap_or(false);

        Ok(valid_protocol && valid_game)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(LFS_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "live_for_speed",
            "telemetry_protocol": LFS_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "packet_format": "lfs_outgauge_96bytes",
            "setup_notes": [
                "In LFS, enable OutGauge in Options > Output or edit cfg.lfs directly.",
                "Set OutGauge IP to 127.0.0.1 and Port to 30000.",
                "Example cfg.lfs entry: OutGauge Mode 1 Addr 127.0.0.1 Port 30000 Id 1 Delay 1"
            ],
        });
        let expected = serde_json::to_string_pretty(&contract)?;

        Ok(vec![ConfigDiff {
            file_path: LFS_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: expected,
            operation: DiffOperation::Add,
        }])
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

/// WRC Generations / WRC 23 configuration writer.
///
/// WRC Generations uses the Codemasters/RallyEngine UDP Mode 1 format on port 6777.
/// This writer creates a bridge contract file for the OpenRacing telemetry pipeline.
pub struct WrcGenerationsConfigWriter;

impl Default for WrcGenerationsConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for WrcGenerationsConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing WRC Generations bridge contract configuration");

        let contract_path = game_path.join(WRC_GENERATIONS_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };

        let udp_port =
            parse_target_port(&config.output_target).unwrap_or(WRC_GENERATIONS_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "wrc_generations",
            "telemetry_protocol": WRC_GENERATIONS_BRIDGE_PROTOCOL,
            "mode": WRC_GENERATIONS_DEFAULT_MODE,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "WRC Generations / WRC 23 uses the Codemasters/RallyEngine UDP Mode 1 format. Enable UDP telemetry in the game's accessibility settings.",
        });

        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;

        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
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
        let contract_path = game_path.join(WRC_GENERATIONS_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }

        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;

        let valid_protocol = value
            .get("telemetry_protocol")
            .and_then(Value::as_str)
            .map(|v| v == WRC_GENERATIONS_BRIDGE_PROTOCOL)
            .unwrap_or(false);
        let valid_game = value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "wrc_generations")
            .unwrap_or(false);

        Ok(valid_protocol && valid_game)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port =
            parse_target_port(&config.output_target).unwrap_or(WRC_GENERATIONS_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "wrc_generations",
            "telemetry_protocol": WRC_GENERATIONS_BRIDGE_PROTOCOL,
            "mode": WRC_GENERATIONS_DEFAULT_MODE,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "WRC Generations / WRC 23 uses the Codemasters/RallyEngine UDP Mode 1 format. Enable UDP telemetry in the game's accessibility settings.",
        });
        let expected = serde_json::to_string_pretty(&contract)?;

        Ok(vec![ConfigDiff {
            file_path: WRC_GENERATIONS_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: expected,
            operation: DiffOperation::Add,
        }])
    }
}

/// Dirt 4 configuration writer.
///
/// Dirt 4 uses the Codemasters extradata v0 UDP format on port 20777.
/// This writer creates a bridge contract file for the OpenRacing telemetry pipeline.
pub struct Dirt4ConfigWriter;

impl Default for Dirt4ConfigWriter {
    fn default() -> Self {
        Self
    }
}

impl ConfigWriter for Dirt4ConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing Dirt 4 bridge contract configuration");

        let contract_path = game_path.join(DIRT4_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before {
            Some(fs::read_to_string(&contract_path)?)
        } else {
            None
        };

        let udp_port = parse_target_port(&config.output_target).unwrap_or(DIRT4_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "dirt4",
            "telemetry_protocol": DIRT4_BRIDGE_PROTOCOL,
            "mode": DIRT4_DEFAULT_MODE,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "Dirt 4 uses the Codemasters extradata v0 UDP format. Enable UDP telemetry in the game's settings.",
        });

        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&contract_path, &new_content)?;

        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
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
        let contract_path = game_path.join(DIRT4_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() {
            return Ok(false);
        }

        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;

        let valid_protocol = value
            .get("telemetry_protocol")
            .and_then(Value::as_str)
            .map(|v| v == DIRT4_BRIDGE_PROTOCOL)
            .unwrap_or(false);
        let valid_game = value
            .get("game_id")
            .and_then(Value::as_str)
            .map(|v| v == "dirt4")
            .unwrap_or(false);

        Ok(valid_protocol && valid_game)
    }

    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(DIRT4_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "dirt4",
            "telemetry_protocol": DIRT4_BRIDGE_PROTOCOL,
            "mode": DIRT4_DEFAULT_MODE,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "Dirt 4 uses the Codemasters extradata v0 UDP format. Enable UDP telemetry in the game's settings.",
        });
        let expected = serde_json::to_string_pretty(&contract)?;

        Ok(vec![ConfigDiff {
            file_path: DIRT4_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: expected,
            operation: DiffOperation::Add,
        }])
    }
}

/// ETS2/ATS configuration writer (SCS Telemetry SDK shared memory)
pub struct Ets2ConfigWriter;

impl Default for Ets2ConfigWriter {
    fn default() -> Self { Self }
}

impl ConfigWriter for Ets2ConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing ETS2 bridge contract configuration");
        let contract_path = game_path.join(ETS2_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before { Some(fs::read_to_string(&contract_path)?) } else { None };
        let udp_port = parse_target_port(&config.output_target).unwrap_or(ETS2_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "ets2",
            "telemetry_protocol": ETS2_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "ETS2 uses SCS Telemetry SDK shared memory. Install the SCS Telemetry plugin.",
        });
        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() { fs::create_dir_all(parent)?; }
        fs::write(&contract_path, &new_content)?;
        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before { DiffOperation::Modify } else { DiffOperation::Add },
        }])
    }
    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = game_path.join(ETS2_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() { return Ok(false); }
        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;
        Ok(value.get("game_id").and_then(Value::as_str).map(|v| v == "ets2").unwrap_or(false))
    }
    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(ETS2_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "ets2",
            "telemetry_protocol": ETS2_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "ETS2 uses SCS Telemetry SDK shared memory. Install the SCS Telemetry plugin.",
        });
        Ok(vec![ConfigDiff {
            file_path: ETS2_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: serde_json::to_string_pretty(&contract)?,
            operation: DiffOperation::Add,
        }])
    }
}

/// ATS configuration writer (SCS Telemetry SDK shared memory)
pub struct AtsConfigWriter;

impl Default for AtsConfigWriter {
    fn default() -> Self { Self }
}

impl ConfigWriter for AtsConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing ATS bridge contract configuration");
        let contract_path = game_path.join(ATS_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before { Some(fs::read_to_string(&contract_path)?) } else { None };
        let udp_port = parse_target_port(&config.output_target).unwrap_or(ATS_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "ats",
            "telemetry_protocol": ATS_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "ATS uses SCS Telemetry SDK shared memory. Install the SCS Telemetry plugin.",
        });
        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() { fs::create_dir_all(parent)?; }
        fs::write(&contract_path, &new_content)?;
        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before { DiffOperation::Modify } else { DiffOperation::Add },
        }])
    }
    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = game_path.join(ATS_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() { return Ok(false); }
        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;
        Ok(value.get("game_id").and_then(Value::as_str).map(|v| v == "ats").unwrap_or(false))
    }
    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(ATS_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "ats",
            "telemetry_protocol": ATS_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "ATS uses SCS Telemetry SDK shared memory. Install the SCS Telemetry plugin.",
        });
        Ok(vec![ConfigDiff {
            file_path: ATS_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: serde_json::to_string_pretty(&contract)?,
            operation: DiffOperation::Add,
        }])
    }
}

/// Wreckfest configuration writer (UDP on port 5606)
pub struct WreckfestConfigWriter;

impl Default for WreckfestConfigWriter {
    fn default() -> Self { Self }
}

impl ConfigWriter for WreckfestConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing Wreckfest bridge contract configuration");
        let contract_path = game_path.join(WRECKFEST_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before { Some(fs::read_to_string(&contract_path)?) } else { None };
        let udp_port = parse_target_port(&config.output_target).unwrap_or(WRECKFEST_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "wreckfest",
            "telemetry_protocol": WRECKFEST_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "Wreckfest sends UDP telemetry on port 5606. Validated by WRKF magic header.",
        });
        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() { fs::create_dir_all(parent)?; }
        fs::write(&contract_path, &new_content)?;
        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before { DiffOperation::Modify } else { DiffOperation::Add },
        }])
    }
    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = game_path.join(WRECKFEST_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() { return Ok(false); }
        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;
        Ok(value.get("game_id").and_then(Value::as_str).map(|v| v == "wreckfest").unwrap_or(false))
    }
    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(WRECKFEST_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "wreckfest",
            "telemetry_protocol": WRECKFEST_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "Wreckfest sends UDP telemetry on port 5606. Validated by WRKF magic header.",
        });
        Ok(vec![ConfigDiff {
            file_path: WRECKFEST_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: serde_json::to_string_pretty(&contract)?,
            operation: DiffOperation::Add,
        }])
    }
}

/// Rennsport configuration writer (UDP on port 9000)
pub struct RennsportConfigWriter;

impl Default for RennsportConfigWriter {
    fn default() -> Self { Self }
}

impl ConfigWriter for RennsportConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing Rennsport bridge contract configuration");
        let contract_path = game_path.join(RENNSPORT_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before { Some(fs::read_to_string(&contract_path)?) } else { None };
        let udp_port = parse_target_port(&config.output_target).unwrap_or(RENNSPORT_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "rennsport",
            "telemetry_protocol": RENNSPORT_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "Rennsport sends UDP telemetry on port 9000. Validated by 0x52 'R' identifier byte.",
        });
        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() { fs::create_dir_all(parent)?; }
        fs::write(&contract_path, &new_content)?;
        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before { DiffOperation::Modify } else { DiffOperation::Add },
        }])
    }
    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = game_path.join(RENNSPORT_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() { return Ok(false); }
        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;
        Ok(value.get("game_id").and_then(Value::as_str).map(|v| v == "rennsport").unwrap_or(false))
    }
    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(RENNSPORT_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "rennsport",
            "telemetry_protocol": RENNSPORT_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "Rennsport sends UDP telemetry on port 9000. Validated by 0x52 'R' identifier byte.",
        });
        Ok(vec![ConfigDiff {
            file_path: RENNSPORT_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: serde_json::to_string_pretty(&contract)?,
            operation: DiffOperation::Add,
        }])
    }
}

/// KartKraft configuration writer (FlatBuffers UDP on port 5000).
pub struct KartKraftConfigWriter;

impl Default for KartKraftConfigWriter {
    fn default() -> Self { Self }
}

impl ConfigWriter for KartKraftConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing KartKraft bridge contract configuration");
        let contract_path = game_path.join(KARTKRAFT_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before { Some(fs::read_to_string(&contract_path)?) } else { None };
        let udp_port = parse_target_port(&config.output_target).unwrap_or(KARTKRAFT_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "kartkraft",
            "telemetry_protocol": KARTKRAFT_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "KartKraft sends FlatBuffers UDP packets (KKFB identifier) on port 5000.",
        });
        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() { fs::create_dir_all(parent)?; }
        fs::write(&contract_path, &new_content)?;
        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before { DiffOperation::Modify } else { DiffOperation::Add },
        }])
    }
    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = game_path.join(KARTKRAFT_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() { return Ok(false); }
        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;
        Ok(value.get("game_id").and_then(Value::as_str).map(|v| v == "kartkraft").unwrap_or(false))
    }
    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(KARTKRAFT_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "kartkraft",
            "telemetry_protocol": KARTKRAFT_BRIDGE_PROTOCOL,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "KartKraft sends FlatBuffers UDP packets (KKFB identifier) on port 5000.",
        });
        Ok(vec![ConfigDiff {
            file_path: KARTKRAFT_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: serde_json::to_string_pretty(&contract)?,
            operation: DiffOperation::Add,
        }])
    }
}

/// RaceRoom Racing Experience configuration writer (R3E shared memory)
pub struct RaceRoomConfigWriter;

impl Default for RaceRoomConfigWriter {
    fn default() -> Self { Self }
}

impl ConfigWriter for RaceRoomConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing RaceRoom bridge contract configuration");
        let contract_path = game_path.join(RACEROOM_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before { Some(fs::read_to_string(&contract_path)?) } else { None };
        let contract = serde_json::json!({
            "game_id": "raceroom",
            "telemetry_protocol": RACEROOM_BRIDGE_PROTOCOL,
            "shared_memory_name": "Local\\$R3E",
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "R3E shared memory is Windows-only. RaceRoom writes to Local\\$R3E automatically when running. No in-game settings required. Supported SDK version: 2.x",
        });
        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() { fs::create_dir_all(parent)?; }
        fs::write(&contract_path, &new_content)?;
        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before { DiffOperation::Modify } else { DiffOperation::Add },
        }])
    }
    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = game_path.join(RACEROOM_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() { return Ok(false); }
        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;
        Ok(value.get("game_id").and_then(Value::as_str).map(|v| v == "raceroom").unwrap_or(false))
    }
    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let contract = serde_json::json!({
            "game_id": "raceroom",
            "telemetry_protocol": RACEROOM_BRIDGE_PROTOCOL,
            "shared_memory_name": "Local\\$R3E",
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "R3E shared memory is Windows-only. RaceRoom writes to Local\\$R3E automatically when running. No in-game settings required. Supported SDK version: 2.x",
        });
        Ok(vec![ConfigDiff {
            file_path: RACEROOM_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: serde_json::to_string_pretty(&contract)?,
            operation: DiffOperation::Add,
        }])
    }
}

/// GRID Autosport configuration writer (Codemasters UDP Mode 1)
pub struct GridAutosportConfigWriter;

impl Default for GridAutosportConfigWriter {
    fn default() -> Self { Self }
}

impl ConfigWriter for GridAutosportConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing GRID Autosport bridge contract configuration");
        let contract_path = game_path.join(GRID_AUTOSPORT_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before { Some(fs::read_to_string(&contract_path)?) } else { None };
        let udp_port = parse_target_port(&config.output_target).unwrap_or(GRID_AUTOSPORT_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "grid_autosport",
            "telemetry_protocol": GRID_AUTOSPORT_BRIDGE_PROTOCOL,
            "mode": GRID_AUTOSPORT_DEFAULT_MODE,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "GRID Autosport uses Codemasters UDP Mode 1 format on port 20777. Enable UDP telemetry in-game under Options > Controls > UDP Telemetry.",
        });
        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() { fs::create_dir_all(parent)?; }
        fs::write(&contract_path, &new_content)?;
        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before { DiffOperation::Modify } else { DiffOperation::Add },
        }])
    }
    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = game_path.join(GRID_AUTOSPORT_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() { return Ok(false); }
        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;
        Ok(value.get("game_id").and_then(Value::as_str).map(|v| v == "grid_autosport").unwrap_or(false)
            && value.get("telemetry_protocol").and_then(Value::as_str).map(|v| v == GRID_AUTOSPORT_BRIDGE_PROTOCOL).unwrap_or(false))
    }
    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(GRID_AUTOSPORT_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "grid_autosport",
            "telemetry_protocol": GRID_AUTOSPORT_BRIDGE_PROTOCOL,
            "mode": GRID_AUTOSPORT_DEFAULT_MODE,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "GRID Autosport uses Codemasters UDP Mode 1 format on port 20777. Enable UDP telemetry in-game under Options > Controls > UDP Telemetry.",
        });
        Ok(vec![ConfigDiff {
            file_path: GRID_AUTOSPORT_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: serde_json::to_string_pretty(&contract)?,
            operation: DiffOperation::Add,
        }])
    }
}

/// GRID 2019 configuration writer (Codemasters UDP Mode 1)
pub struct Grid2019ConfigWriter;

impl Default for Grid2019ConfigWriter {
    fn default() -> Self { Self }
}

impl ConfigWriter for Grid2019ConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing GRID 2019 bridge contract configuration");
        let contract_path = game_path.join(GRID_2019_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before { Some(fs::read_to_string(&contract_path)?) } else { None };
        let udp_port = parse_target_port(&config.output_target).unwrap_or(GRID_2019_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "grid_2019",
            "telemetry_protocol": GRID_2019_BRIDGE_PROTOCOL,
            "mode": GRID_2019_DEFAULT_MODE,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "GRID (2019) uses Codemasters UDP Mode 1 format on port 20777. Enable UDP telemetry in-game under Options > Controls > UDP Telemetry.",
        });
        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() { fs::create_dir_all(parent)?; }
        fs::write(&contract_path, &new_content)?;
        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before { DiffOperation::Modify } else { DiffOperation::Add },
        }])
    }
    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = game_path.join(GRID_2019_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() { return Ok(false); }
        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;
        Ok(value.get("game_id").and_then(Value::as_str).map(|v| v == "grid_2019").unwrap_or(false)
            && value.get("telemetry_protocol").and_then(Value::as_str).map(|v| v == GRID_2019_BRIDGE_PROTOCOL).unwrap_or(false))
    }
    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(GRID_2019_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "grid_2019",
            "telemetry_protocol": GRID_2019_BRIDGE_PROTOCOL,
            "mode": GRID_2019_DEFAULT_MODE,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "GRID (2019) uses Codemasters UDP Mode 1 format on port 20777. Enable UDP telemetry in-game under Options > Controls > UDP Telemetry.",
        });
        Ok(vec![ConfigDiff {
            file_path: GRID_2019_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: serde_json::to_string_pretty(&contract)?,
            operation: DiffOperation::Add,
        }])
    }
}

/// GRID Legends configuration writer (Codemasters UDP Mode 1)
pub struct GridLegendsConfigWriter;

impl Default for GridLegendsConfigWriter {
    fn default() -> Self { Self }
}

impl ConfigWriter for GridLegendsConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing GRID Legends bridge contract configuration");
        let contract_path = game_path.join(GRID_LEGENDS_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before { Some(fs::read_to_string(&contract_path)?) } else { None };
        let udp_port = parse_target_port(&config.output_target).unwrap_or(GRID_LEGENDS_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "grid_legends",
            "telemetry_protocol": GRID_LEGENDS_BRIDGE_PROTOCOL,
            "mode": GRID_LEGENDS_DEFAULT_MODE,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "GRID Legends uses Codemasters UDP Mode 1 format on port 20777. Enable UDP telemetry in-game under Options > Controls > UDP Telemetry.",
        });
        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() { fs::create_dir_all(parent)?; }
        fs::write(&contract_path, &new_content)?;
        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before { DiffOperation::Modify } else { DiffOperation::Add },
        }])
    }
    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = game_path.join(GRID_LEGENDS_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() { return Ok(false); }
        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;
        Ok(value.get("game_id").and_then(Value::as_str).map(|v| v == "grid_legends").unwrap_or(false)
            && value.get("telemetry_protocol").and_then(Value::as_str).map(|v| v == GRID_LEGENDS_BRIDGE_PROTOCOL).unwrap_or(false))
    }
    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let udp_port = parse_target_port(&config.output_target).unwrap_or(GRID_LEGENDS_DEFAULT_PORT);
        let contract = serde_json::json!({
            "game_id": "grid_legends",
            "telemetry_protocol": GRID_LEGENDS_BRIDGE_PROTOCOL,
            "mode": GRID_LEGENDS_DEFAULT_MODE,
            "udp_port": udp_port,
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "GRID Legends uses Codemasters UDP Mode 1 format on port 20777. Enable UDP telemetry in-game under Options > Controls > UDP Telemetry.",
        });
        Ok(vec![ConfigDiff {
            file_path: GRID_LEGENDS_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: serde_json::to_string_pretty(&contract)?,
            operation: DiffOperation::Add,
        }])
    }
}

/// Automobilista 1 configuration writer (ISI rFactor 1 shared memory)
pub struct AutomobilistaConfigWriter;

impl Default for AutomobilistaConfigWriter {
    fn default() -> Self { Self }
}

impl ConfigWriter for AutomobilistaConfigWriter {
    fn write_config(&self, game_path: &Path, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        info!("Writing Automobilista 1 bridge contract configuration");
        let contract_path = game_path.join(AUTOMOBILISTA_BRIDGE_RELATIVE_PATH);
        let existed_before = contract_path.exists();
        let existing_content = if existed_before { Some(fs::read_to_string(&contract_path)?) } else { None };
        let contract = serde_json::json!({
            "game_id": "automobilista",
            "telemetry_protocol": AUTOMOBILISTA_BRIDGE_PROTOCOL,
            "shared_memory_name": "$rFactor$",
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "Automobilista 1 uses ISI InternalsPlugin SDK 2.3 shared memory ($rFactor$). Windows only. Requires the ISI telemetry plugin to be active.",
        });
        let new_content = serde_json::to_string_pretty(&contract)?;
        if let Some(parent) = contract_path.parent() { fs::create_dir_all(parent)?; }
        fs::write(&contract_path, &new_content)?;
        Ok(vec![ConfigDiff {
            file_path: contract_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: existing_content,
            new_value: new_content,
            operation: if existed_before { DiffOperation::Modify } else { DiffOperation::Add },
        }])
    }
    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let contract_path = game_path.join(AUTOMOBILISTA_BRIDGE_RELATIVE_PATH);
        if !contract_path.exists() { return Ok(false); }
        let content = fs::read_to_string(contract_path)?;
        let value: Value = serde_json::from_str(&content)?;
        Ok(value.get("game_id").and_then(Value::as_str).map(|v| v == "automobilista").unwrap_or(false)
            && value.get("telemetry_protocol").and_then(Value::as_str).map(|v| v == AUTOMOBILISTA_BRIDGE_PROTOCOL).unwrap_or(false))
    }
    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let contract = serde_json::json!({
            "game_id": "automobilista",
            "telemetry_protocol": AUTOMOBILISTA_BRIDGE_PROTOCOL,
            "shared_memory_name": "$rFactor$",
            "update_rate_hz": config.update_rate_hz,
            "enabled": config.enabled,
            "bridge_notes": "Automobilista 1 uses ISI InternalsPlugin SDK 2.3 shared memory ($rFactor$). Windows only. Requires the ISI telemetry plugin to be active.",
        });
        Ok(vec![ConfigDiff {
            file_path: AUTOMOBILISTA_BRIDGE_RELATIVE_PATH.to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: serde_json::to_string_pretty(&contract)?,
            operation: DiffOperation::Add,
        }])
    }
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
            enable_high_rate_iracing_360hz: false,
        };

        let diffs = writer.write_config(temp_dir.path(), &config)?;
        assert_eq!(diffs.len(), 1);
        assert!(writer.validate_config(temp_dir.path())?);
        Ok(())
    }

    #[test]
    fn test_iracing_writer_optional_360hz_setting() -> TestResult {
        let writer = IRacingConfigWriter;
        let temp_dir = tempdir()?;
        let config = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "shared_memory".to_string(),
            output_target: "127.0.0.1:12345".to_string(),
            fields: vec!["ffb_scalar".to_string(), "rpm".to_string()],
            enable_high_rate_iracing_360hz: true,
        };

        let diffs = writer.write_config(temp_dir.path(), &config)?;
        assert_eq!(diffs.len(), 2);
        assert!(writer.validate_config(temp_dir.path())?);

        let first = diffs
            .iter()
            .find(|diff| diff.key == "telemetryDiskFile")
            .expect("telemetryDiskFile diff should be present");
        let second = diffs
            .iter()
            .find(|diff| diff.key == "irsdkLog360Hz")
            .expect("irsdkLog360Hz diff should be present when enabled");

        assert_eq!(first.new_value, "1");
        assert_eq!(second.new_value, "1");

        let expected = writer.get_expected_diffs(&config)?;
        assert_eq!(expected.len(), 2);
        assert!(expected.iter().any(|diff| diff.key == "irsdkLog360Hz"));

        Ok(())
    }

    #[test]
    fn test_iracing_writer_without_360hz_is_idempotent() -> TestResult {
        let writer = IRacingConfigWriter;
        let temp_dir = tempdir()?;
        let config = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "shared_memory".to_string(),
            output_target: "127.0.0.1:12345".to_string(),
            fields: vec!["ffb_scalar".to_string(), "rpm".to_string()],
            enable_high_rate_iracing_360hz: false,
        };

        let first = writer.write_config(temp_dir.path(), &config)?;
        assert_eq!(first.len(), 1);

        let app_ini_path = temp_dir.path().join("Documents/iRacing/app.ini");
        let first_content = std::fs::read_to_string(&app_ini_path)?;
        assert!(first_content.contains("telemetryDiskFile=1"));
        assert!(
            !first_content
                .lines()
                .any(|line| line.starts_with("irsdkLog360Hz="))
        );

        let second = writer.write_config(temp_dir.path(), &config)?;
        assert_eq!(second.len(), 1);
        assert!(
            second
                .iter()
                .all(|diff| diff.key == "telemetryDiskFile" && diff.new_value == "1")
        );

        let second_content = std::fs::read_to_string(&app_ini_path)?;
        assert!(second_content.contains("telemetryDiskFile=1"));
        assert!(
            !second_content
                .lines()
                .any(|line| line.starts_with("irsdkLog360Hz="))
        );

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
            enable_high_rate_iracing_360hz: false,
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
            enable_high_rate_iracing_360hz: false,
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
            enable_high_rate_iracing_360hz: false,
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
    fn test_acc_writer_round_trip() -> TestResult {
        let writer = ACCConfigWriter;
        let temp_dir = tempdir()?;
        let config = TelemetryConfig {
            enabled: true,
            update_rate_hz: 100,
            output_method: "udp_broadcast".to_string(),
            output_target: "127.0.0.1:9000".to_string(),
            fields: vec!["speed_ms".to_string()],
            enable_high_rate_iracing_360hz: false,
        };

        let diffs = writer.write_config(temp_dir.path(), &config)?;
        assert_eq!(diffs.len(), 1);
        assert!(writer.validate_config(temp_dir.path())?);

        let value: Value = serde_json::from_str(&diffs[0].new_value)?;
        assert_eq!(value["updListenerPort"], 9000);
        assert_eq!(value["udpListenerPort"], 9000);
        assert_eq!(value["broadcastingPort"], 9000);
        assert_eq!(value["updateRateHz"], 100);
        Ok(())
    }

    #[test]
    fn test_dirt5_writer_round_trip() -> TestResult {
        let writer = Dirt5ConfigWriter;
        let temp_dir = tempdir()?;
        let config = TelemetryConfig {
            enabled: true,
            update_rate_hz: 120,
            output_method: "udp_custom_codemasters".to_string(),
            output_target: "127.0.0.1:20777".to_string(),
            fields: vec![
                "rpm".to_string(),
                "speed_ms".to_string(),
                "gear".to_string(),
                "slip_ratio".to_string(),
            ],
            enable_high_rate_iracing_360hz: false,
        };

        let diffs = writer.write_config(temp_dir.path(), &config)?;
        assert_eq!(diffs.len(), 1);
        assert!(writer.validate_config(temp_dir.path())?);
        Ok(())
    }

    #[test]
    fn test_f1_writer_round_trip() -> TestResult {
        let writer = F1ConfigWriter;
        let temp_dir = tempdir()?;
        let config = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "udp_custom_codemasters".to_string(),
            output_target: "127.0.0.1:20777".to_string(),
            fields: vec![
                "rpm".to_string(),
                "speed_ms".to_string(),
                "gear".to_string(),
                "slip_ratio".to_string(),
                "flags".to_string(),
            ],
            enable_high_rate_iracing_360hz: false,
        };

        let diffs = writer.write_config(temp_dir.path(), &config)?;
        assert_eq!(diffs.len(), 1);
        assert!(writer.validate_config(temp_dir.path())?);
        Ok(())
    }

    #[test]
    fn test_f1_25_writer_round_trip() -> TestResult {
        let writer = F1_25ConfigWriter;
        let temp_dir = tempdir()?;
        let config = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "f1_25_native_udp".to_string(),
            output_target: "127.0.0.1:20777".to_string(),
            fields: vec!["rpm".to_string(), "speed_ms".to_string()],
            enable_high_rate_iracing_360hz: false,
        };

        let diffs = writer.write_config(temp_dir.path(), &config)?;
        assert_eq!(diffs.len(), 1);
        assert!(writer.validate_config(temp_dir.path())?);

        let value: Value = serde_json::from_str(&diffs[0].new_value)?;
        assert_eq!(value["game_id"], "f1_25");
        assert_eq!(value["telemetry_protocol"], "f1_25_native_udp");
        assert_eq!(value["packet_format"], 2025);
        Ok(())
    }
}

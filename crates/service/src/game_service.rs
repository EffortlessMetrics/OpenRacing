//! Game Integration Service
//! 
//! Handles telemetry configuration, auto-switching, and game-specific integrations
//! according to requirements GI-01 and GI-03.

use crate::config_writers::{IRacingConfigWriter, ACCConfigWriter};
use anyhow::{Result, Context};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Game integration service that manages telemetry configuration and auto-switching
pub struct GameService {
    support_matrix: Arc<RwLock<GameSupportMatrix>>,
    config_writers: HashMap<String, Box<dyn ConfigWriter + Send + Sync>>,
    active_game: Arc<RwLock<Option<String>>>,
}

/// Game support matrix loaded from YAML configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GameSupportMatrix {
    pub games: HashMap<String, GameSupport>,
}

/// Support information for a specific game
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GameSupport {
    pub name: String,
    pub versions: Vec<GameVersion>,
    pub telemetry: TelemetrySupport,
    pub config_writer: String,
    pub auto_detect: AutoDetectConfig,
}

/// Version-specific game support
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GameVersion {
    pub version: String,
    pub config_paths: Vec<String>,
    pub executable_patterns: Vec<String>,
    pub telemetry_method: String,
    pub supported_fields: Vec<String>,
}

/// Telemetry support configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TelemetrySupport {
    pub method: String, // "shared_memory", "udp_broadcast", "file_based"
    pub update_rate_hz: u32,
    pub fields: TelemetryFieldMapping,
}

/// Mapping of normalized telemetry fields to game-specific fields
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TelemetryFieldMapping {
    pub ffb_scalar: Option<String>,
    pub rpm: Option<String>,
    pub speed_ms: Option<String>,
    pub slip_ratio: Option<String>,
    pub gear: Option<String>,
    pub flags: Option<String>,
    pub car_id: Option<String>,
    pub track_id: Option<String>,
}

/// Auto-detection configuration for games
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AutoDetectConfig {
    pub process_names: Vec<String>,
    pub install_registry_keys: Vec<String>,
    pub install_paths: Vec<String>,
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

/// Configuration to be applied to a game
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryConfig {
    pub enabled: bool,
    pub update_rate_hz: u32,
    pub output_method: String,
    pub output_target: String, // IP:port for UDP, file path for file-based, etc.
    pub fields: Vec<String>,
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
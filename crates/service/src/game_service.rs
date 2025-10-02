//! Game Integration Service
//! 
//! Handles telemetry configuration, auto-switching, and game-specific integrations
//! according to requirements GI-01 and GI-03.

use crate::config_writers::{IRacingConfigWriter, ACCConfigWriter};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

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

/// Game status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameStatusInfo {
    pub active_game: Option<String>,
    pub telemetry_active: bool,
    pub car_id: Option<String>,
    pub track_id: Option<String>,
}

impl GameService {
    /// Create new game service with YAML-loaded support matrix
    pub async fn new() -> Result<Self> {
        let support_matrix = Self::load_support_matrix().await?;
        let mut config_writers: HashMap<String, Box<dyn ConfigWriter + Send + Sync>> = HashMap::new();
        
        // Register config writers
        config_writers.insert("iracing".to_string(), Box::new(IRacingConfigWriter::default()));
        config_writers.insert("acc".to_string(), Box::new(ACCConfigWriter::default()));
        
        Ok(Self {
            support_matrix: Arc::new(RwLock::new(support_matrix)),
            config_writers,
            active_game: Arc::new(RwLock::new(None)),
        })
    }
    
    /// Load game support matrix from YAML file
    async fn load_support_matrix() -> Result<GameSupportMatrix> {
        let yaml_content = include_str!("../config/game_support_matrix.yaml");
        let matrix: GameSupportMatrix = serde_yaml::from_str(yaml_content)?;
        info!(games_count = matrix.games.len(), "Loaded game support matrix");
        Ok(matrix)
    }
    
    /// Configure telemetry for a specific game (GI-01)
    pub async fn configure_telemetry(&self, game_id: &str, game_path: &Path) -> Result<Vec<ConfigDiff>> {
        info!(game_id = %game_id, game_path = ?game_path, "Configuring telemetry");
        
        let support_matrix = self.support_matrix.read().await;
        let game_support = support_matrix.games.get(game_id)
            .ok_or_else(|| anyhow::anyhow!("Unsupported game: {}", game_id))?;
        
        let config_writer = self.config_writers.get(game_id)
            .ok_or_else(|| anyhow::anyhow!("No config writer for game: {}", game_id))?;
        
        // Create telemetry configuration
        let telemetry_config = TelemetryConfig {
            enabled: true,
            update_rate_hz: game_support.telemetry.update_rate_hz,
            output_method: game_support.telemetry.method.clone(),
            output_target: "127.0.0.1:12345".to_string(),
            fields: game_support.versions[0].supported_fields.clone(),
        };
        
        // Write configuration and get diffs
        let diffs = config_writer.write_config(game_path, &telemetry_config)?;
        
        info!(game_id = %game_id, diffs_count = diffs.len(), "Telemetry configuration completed");
        Ok(diffs)
    }
    
    /// Get normalized telemetry field mapping for a game (GI-03)
    pub async fn get_telemetry_mapping(&self, game_id: &str) -> Result<TelemetryFieldMapping> {
        let support_matrix = self.support_matrix.read().await;
        let game_support = support_matrix.games.get(game_id)
            .ok_or_else(|| anyhow::anyhow!("Unsupported game: {}", game_id))?;
        
        Ok(game_support.telemetry.fields.clone())
    }
    
    /// Get list of supported games
    pub async fn get_supported_games(&self) -> Vec<String> {
        let support_matrix = self.support_matrix.read().await;
        support_matrix.games.keys().cloned().collect()
    }
    
    /// Get game support information
    pub async fn get_game_support(&self, game_id: &str) -> Result<GameSupport> {
        let support_matrix = self.support_matrix.read().await;
        support_matrix.games.get(game_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Unsupported game: {}", game_id))
    }
    
    /// Get currently active game
    pub async fn get_active_game(&self) -> Option<String> {
        self.active_game.read().await.clone()
    }
    
    /// Set active game for auto-switching
    pub async fn set_active_game(&self, game_id: Option<String>) -> Result<()> {
        let mut active_game = self.active_game.write().await;
        *active_game = game_id.clone();
        
        if let Some(game_id) = game_id {
            info!(game_id = %game_id, "Set active game");
        } else {
            info!("Cleared active game");
        }
        
        Ok(())
    }
    
    /// Validate configuration was applied correctly
    pub async fn validate_telemetry_config(&self, game_id: &str, game_path: &Path) -> Result<bool> {
        let config_writer = self.config_writers.get(game_id)
            .ok_or_else(|| anyhow::anyhow!("No config writer for game: {}", game_id))?;
        
        config_writer.validate_config(game_path)
    }
    
    /// Get expected configuration diffs for testing
    pub async fn get_expected_diffs(&self, game_id: &str, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let config_writer = self.config_writers.get(game_id)
            .ok_or_else(|| anyhow::anyhow!("No config writer for game: {}", game_id))?;
        
        config_writer.get_expected_diffs(config)
    }

    /// Get game status (for IPC service compatibility)
    pub async fn get_game_status(&self) -> Result<GameStatusInfo> {
        let active_game = self.get_active_game().await;
        
        // For now, return basic status information
        // This could be enhanced to detect actual game state, telemetry activity, etc.
        Ok(GameStatusInfo {
            active_game,
            telemetry_active: false, // Would be determined by actual telemetry monitoring
            car_id: None,            // Would be populated from telemetry data
            track_id: None,          // Would be populated from telemetry data
        })
    }
}
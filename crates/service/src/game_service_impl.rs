//! GameService implementation

use crate::game_service::*;
use crate::config_writers::{IRacingConfigWriter, ACCConfigWriter};
use anyhow::{Result, Context};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

impl GameService {
    /// Create new game service instance
    pub async fn new() -> Result<Self> {
        info!("Initializing Game Service");
        
        let support_matrix = Arc::new(RwLock::new(GameSupportMatrix::default()));
        let config_writers = HashMap::new();
        let active_game = Arc::new(RwLock::new(None));
        
        let mut service = Self {
            support_matrix,
            config_writers,
            active_game,
        };
        
        // Load support matrix from configuration
        service.load_support_matrix().await?;
        
        // Initialize config writers
        service.initialize_config_writers().await?;
        
        info!("Game Service initialized successfully");
        Ok(service)
    }
    
    /// Load game support matrix from YAML configuration
    async fn load_support_matrix(&mut self) -> Result<()> {
        // For now, create a default matrix with iRacing and ACC support
        // In production, this would load from a YAML file
        let matrix = GameSupportMatrix::create_default();
        
        let mut support_matrix = self.support_matrix.write().await;
        *support_matrix = matrix;
        
        info!("Game support matrix loaded");
        Ok(())
    }
    
    /// Initialize configuration writers for supported games
    async fn initialize_config_writers(&mut self) -> Result<()> {
        // Register iRacing config writer
        self.config_writers.insert(
            "iracing".to_string(),
            Box::new(IRacingConfigWriter::new())
        );
        
        // Register ACC config writer
        self.config_writers.insert(
            "acc".to_string(),
            Box::new(ACCConfigWriter::new())
        );
        
        info!("Configuration writers initialized");
        Ok(())
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
            output_target: "127.0.0.1:12345".to_string(), // Default UDP target
            fields: game_support.versions[0].supported_fields.clone(),
        };
        
        // Write configuration and get diffs
        let diffs = config_writer.write_config(game_path, &telemetry_config)
            .context("Failed to write telemetry configuration")?;
        
        // Validate configuration was applied
        if !config_writer.validate_config(game_path)? {
            warn!("Configuration validation failed for {}", game_id);
        }
        
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
    
    /// Set active game for auto-switching (GI-02)
    pub async fn set_active_game(&self, game_id: Option<String>) -> Result<()> {
        let mut active_game = self.active_game.write().await;
        *active_game = game_id.clone();
        
        if let Some(game) = game_id {
            info!(game_id = %game, "Active game set");
        } else {
            info!("Active game cleared");
        }
        
        Ok(())
    }
    
    /// Get currently active game
    pub async fn get_active_game(&self) -> Option<String> {
        self.active_game.read().await.clone()
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
}
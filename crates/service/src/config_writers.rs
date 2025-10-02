//! Configuration writers for game-specific telemetry setup

use crate::game_service::{ConfigWriter, TelemetryConfig, ConfigDiff, DiffOperation};
use anyhow::Result;
use std::path::Path;
use std::fs;
use tracing::{info, debug};

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
        let mut diffs = Vec::new();
        
        // Read existing app.ini if it exists
        let existing_content = if app_ini_path.exists() {
            fs::read_to_string(&app_ini_path)?
        } else {
            String::new()
        };
        
        // Parse INI and modify telemetry settings
        let mut new_content = existing_content.clone();
        
        // Enable telemetry output
        if !new_content.contains("[Telemetry]") {
            new_content.push_str("\n[Telemetry]\n");
        }
        
        // Add/modify telemetry settings
        let telemetry_enabled = if config.enabled { "1" } else { "0" };
        
        diffs.push(ConfigDiff {
            file_path: app_ini_path.to_string_lossy().to_string(),
            section: Some("Telemetry".to_string()),
            key: "telemetryDiskFile".to_string(),
            old_value: None,
            new_value: telemetry_enabled.to_string(),
            operation: DiffOperation::Add,
        });
        
        // Write the configuration file
        if let Some(parent) = app_ini_path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        // For demo purposes, we'll just log what we would write
        debug!("Would write iRacing config to: {:?}", app_ini_path);
        debug!("Telemetry enabled: {}", config.enabled);
        debug!("Update rate: {} Hz", config.update_rate_hz);
        
        Ok(diffs)
    }
    
    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let app_ini_path = game_path.join("Documents/iRacing/app.ini");
        
        if !app_ini_path.exists() {
            return Ok(false);
        }
        
        let content = fs::read_to_string(app_ini_path)?;
        
        // Check if telemetry is enabled
        let has_telemetry_section = content.contains("[Telemetry]");
        let has_telemetry_enabled = content.contains("telemetryDiskFile=1");
        
        Ok(has_telemetry_section && has_telemetry_enabled)
    }
    
    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let mut diffs = Vec::new();
        
        let telemetry_enabled = if config.enabled { "1" } else { "0" };
        
        diffs.push(ConfigDiff {
            file_path: "Documents/iRacing/app.ini".to_string(),
            section: Some("Telemetry".to_string()),
            key: "telemetryDiskFile".to_string(),
            old_value: None,
            new_value: telemetry_enabled.to_string(),
            operation: DiffOperation::Add,
        });
        
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
        
        let broadcasting_json_path = game_path.join("Documents/Assetto Corsa Competizione/Config/broadcasting.json");
        let mut diffs = Vec::new();
        
        // Create broadcasting configuration JSON
        let broadcasting_config = serde_json::json!({
            "updListenerPort": 9996,
            "connectionId": "",
            "broadcastingPort": 9000,
            "commandPassword": "",
            "updateRateHz": config.update_rate_hz
        });
        
        // Read existing config if it exists
        let old_value = if broadcasting_json_path.exists() {
            Some(fs::read_to_string(&broadcasting_json_path)?)
        } else {
            None
        };
        
        let new_content = serde_json::to_string_pretty(&broadcasting_config)?;
        
        diffs.push(ConfigDiff {
            file_path: broadcasting_json_path.to_string_lossy().to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value,
            new_value: new_content.clone(),
            operation: if broadcasting_json_path.exists() { 
                DiffOperation::Modify 
            } else { 
                DiffOperation::Add 
            },
        });
        
        // Write the configuration file
        if let Some(parent) = broadcasting_json_path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        // For demo purposes, we'll just log what we would write
        debug!("Would write ACC config to: {:?}", broadcasting_json_path);
        debug!("Broadcasting config: {}", new_content);
        
        Ok(diffs)
    }
    
    fn validate_config(&self, game_path: &Path) -> Result<bool> {
        let broadcasting_json_path = game_path.join("Documents/Assetto Corsa Competizione/Config/broadcasting.json");
        
        if !broadcasting_json_path.exists() {
            return Ok(false);
        }
        
        let content = fs::read_to_string(broadcasting_json_path)?;
        let config: serde_json::Value = serde_json::from_str(&content)?;
        
        // Check if broadcasting is properly configured
        let has_udp_port = config.get("updListenerPort").is_some();
        let has_broadcast_port = config.get("broadcastingPort").is_some();
        
        Ok(has_udp_port && has_broadcast_port)
    }
    
    fn get_expected_diffs(&self, config: &TelemetryConfig) -> Result<Vec<ConfigDiff>> {
        let mut diffs = Vec::new();
        
        let broadcasting_config = serde_json::json!({
            "updListenerPort": 9996,
            "connectionId": "",
            "broadcastingPort": 9000,
            "commandPassword": "",
            "updateRateHz": config.update_rate_hz
        });
        
        let new_content = serde_json::to_string_pretty(&broadcasting_config)?;
        
        diffs.push(ConfigDiff {
            file_path: "Documents/Assetto Corsa Competizione/Config/broadcasting.json".to_string(),
            section: None,
            key: "entire_file".to_string(),
            old_value: None,
            new_value: new_content,
            operation: DiffOperation::Add,
        });
        
        Ok(diffs)
    }
}
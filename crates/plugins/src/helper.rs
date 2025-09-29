//! Helper utilities for plugin system

use std::path::Path;
use uuid::Uuid;

use crate::{PluginError, PluginResult};

/// Helper functions for plugin management
pub struct PluginHelper;

impl PluginHelper {
    /// Validate plugin file exists and is readable
    pub async fn validate_plugin_file(path: &Path) -> PluginResult<()> {
        if !path.exists() {
            return Err(PluginError::LoadingFailed(format!(
                "Plugin file does not exist: {}",
                path.display()
            )));
        }
        
        if !path.is_file() {
            return Err(PluginError::LoadingFailed(format!(
                "Plugin path is not a file: {}",
                path.display()
            )));
        }
        
        // Check if file is readable
        match tokio::fs::metadata(path).await {
            Ok(_) => Ok(()),
            Err(e) => Err(PluginError::LoadingFailed(format!(
                "Cannot read plugin file {}: {}",
                path.display(),
                e
            ))),
        }
    }
    
    /// Generate a unique plugin ID
    pub fn generate_plugin_id() -> Uuid {
        Uuid::new_v4()
    }
    
    /// Validate plugin name
    pub fn validate_plugin_name(name: &str) -> PluginResult<()> {
        if name.is_empty() {
            return Err(PluginError::ManifestValidation(
                "Plugin name cannot be empty".to_string(),
            ));
        }
        
        if name.len() > 100 {
            return Err(PluginError::ManifestValidation(
                "Plugin name too long (max 100 characters)".to_string(),
            ));
        }
        
        // Check for invalid characters
        if name.chars().any(|c| c.is_control() || c == '/' || c == '\\') {
            return Err(PluginError::ManifestValidation(
                "Plugin name contains invalid characters".to_string(),
            ));
        }
        
        Ok(())
    }
}
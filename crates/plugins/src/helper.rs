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
        if name
            .chars()
            .any(|c| c.is_control() || c == '/' || c == '\\')
        {
            return Err(PluginError::ManifestValidation(
                "Plugin name contains invalid characters".to_string(),
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_plugin_id_is_unique() {
        let id1 = PluginHelper::generate_plugin_id();
        let id2 = PluginHelper::generate_plugin_id();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_validate_valid_plugin_name() -> Result<(), PluginError> {
        PluginHelper::validate_plugin_name("My Plugin")?;
        PluginHelper::validate_plugin_name("FFB-Filter-Pro")?;
        PluginHelper::validate_plugin_name("a")?;
        PluginHelper::validate_plugin_name("plugin_v2.1")
    }

    #[test]
    fn test_validate_empty_name_rejected() {
        let result = PluginHelper::validate_plugin_name("");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_name_too_long() {
        let long_name = "a".repeat(101);
        let result = PluginHelper::validate_plugin_name(&long_name);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_name_exactly_100_chars() -> Result<(), PluginError> {
        let name = "a".repeat(100);
        PluginHelper::validate_plugin_name(&name)
    }

    #[test]
    fn test_validate_name_with_forward_slash_rejected() {
        let result = PluginHelper::validate_plugin_name("path/to/plugin");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_name_with_backslash_rejected() {
        let result = PluginHelper::validate_plugin_name("path\\to\\plugin");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_name_with_control_chars_rejected() {
        let result = PluginHelper::validate_plugin_name("plugin\0name");
        assert!(result.is_err());
        let result = PluginHelper::validate_plugin_name("plugin\nnewline");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_validate_nonexistent_file() {
        let result = PluginHelper::validate_plugin_file(std::path::Path::new(
            "/nonexistent/path/plugin.wasm",
        ))
        .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_validate_directory_rejected() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let result = PluginHelper::validate_plugin_file(dir.path()).await;
        assert!(result.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn test_validate_existing_file_accepted() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let file_path = dir.path().join("test_plugin.wasm");
        tokio::fs::write(&file_path, b"fake wasm content").await?;
        PluginHelper::validate_plugin_file(&file_path).await?;
        Ok(())
    }
}

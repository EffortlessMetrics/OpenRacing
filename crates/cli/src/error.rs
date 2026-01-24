//! Error types for wheelctl CLI

use thiserror::Error;
#[derive(Error, Debug)]
pub enum CliError {
    #[error("Device not found: {0}")]
    DeviceNotFound(String),

    #[error("Profile not found: {0}")]
    ProfileNotFound(String),

    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfiguration(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("YAML error: {0}")]
    YamlError(#[from] serde_yaml::Error),

    #[error("Schema error: {0}")]
    SchemaError(#[from] racing_wheel_schemas::config::SchemaError),
}

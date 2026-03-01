//! Plugin manifest validation and loading system

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use uuid::Uuid;

use crate::{PluginClass, PluginError, PluginResult};

/// Plugin manifest structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub id: Uuid,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub license: String,
    pub homepage: Option<String>,
    pub class: PluginClass,
    pub capabilities: Vec<Capability>,
    pub operations: Vec<PluginOperation>,
    pub constraints: PluginConstraints,
    pub entry_points: EntryPoints,
    pub config_schema: Option<serde_json::Value>,
    pub signature: Option<String>,
}

/// Plugin capability requirements
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Capability {
    ReadTelemetry,
    ModifyTelemetry,
    ControlLeds,
    ProcessDsp,
    FileSystem { paths: Vec<String> },
    Network { hosts: Vec<String> },
    InterPluginComm,
}

/// Supported plugin operations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PluginOperation {
    TelemetryProcessor,
    LedMapper,
    DspFilter,
    TelemetrySource,
}

/// Plugin performance constraints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConstraints {
    pub max_execution_time_us: u32,
    pub max_memory_bytes: u64,
    pub update_rate_hz: u32,
    pub cpu_affinity: Option<u64>,
}

/// Plugin entry points
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryPoints {
    pub wasm_module: Option<String>,
    pub native_library: Option<String>,
    pub main_function: String,
    pub init_function: Option<String>,
    pub cleanup_function: Option<String>,
}

/// Plugin manifest validator
pub struct ManifestValidator {
    allowed_capabilities: HashMap<PluginClass, Vec<Capability>>,
    max_constraints: HashMap<PluginClass, PluginConstraints>,
}

impl Default for ManifestValidator {
    fn default() -> Self {
        let mut allowed_capabilities = HashMap::new();
        allowed_capabilities.insert(
            PluginClass::Safe,
            vec![
                Capability::ReadTelemetry,
                Capability::ModifyTelemetry,
                Capability::ControlLeds,
                Capability::InterPluginComm,
            ],
        );
        allowed_capabilities.insert(
            PluginClass::Fast,
            vec![
                Capability::ReadTelemetry,
                Capability::ModifyTelemetry,
                Capability::ControlLeds,
                Capability::ProcessDsp,
                Capability::InterPluginComm,
            ],
        );

        let mut max_constraints = HashMap::new();
        max_constraints.insert(
            PluginClass::Safe,
            PluginConstraints {
                max_execution_time_us: 5000,
                max_memory_bytes: 16 * 1024 * 1024,
                update_rate_hz: 200,
                cpu_affinity: None,
            },
        );
        max_constraints.insert(
            PluginClass::Fast,
            PluginConstraints {
                max_execution_time_us: 200,
                max_memory_bytes: 4 * 1024 * 1024,
                update_rate_hz: 1000,
                cpu_affinity: Some(0xFE),
            },
        );

        Self {
            allowed_capabilities,
            max_constraints,
        }
    }
}

impl ManifestValidator {
    pub fn validate(&self, manifest: &PluginManifest) -> PluginResult<()> {
        if manifest.name.is_empty() {
            return Err(PluginError::ManifestValidation(
                "Plugin name cannot be empty".to_string(),
            ));
        }

        if manifest.author.is_empty() {
            return Err(PluginError::ManifestValidation(
                "Plugin author cannot be empty".to_string(),
            ));
        }

        let allowed_capabilities =
            self.allowed_capabilities
                .get(&manifest.class)
                .ok_or_else(|| {
                    PluginError::ManifestValidation(format!(
                        "No capability policy defined for plugin class {:?}",
                        manifest.class
                    ))
                })?;

        for capability in &manifest.capabilities {
            if !allowed_capabilities.contains(capability) {
                return Err(PluginError::ManifestValidation(format!(
                    "Capability {:?} is not allowed for {:?} plugins",
                    capability, manifest.class
                )));
            }
        }

        let max_constraints = self.max_constraints.get(&manifest.class).ok_or_else(|| {
            PluginError::ManifestValidation(format!(
                "No constraint policy defined for plugin class {:?}",
                manifest.class
            ))
        })?;

        if manifest.constraints.max_execution_time_us > max_constraints.max_execution_time_us {
            return Err(PluginError::ManifestValidation(format!(
                "Execution time budget {}us exceeds max {}us for {:?} plugins",
                manifest.constraints.max_execution_time_us,
                max_constraints.max_execution_time_us,
                manifest.class
            )));
        }

        if manifest.constraints.max_memory_bytes > max_constraints.max_memory_bytes {
            return Err(PluginError::ManifestValidation(format!(
                "Memory budget {} bytes exceeds max {} bytes for {:?} plugins",
                manifest.constraints.max_memory_bytes,
                max_constraints.max_memory_bytes,
                manifest.class
            )));
        }

        if manifest.constraints.update_rate_hz > max_constraints.update_rate_hz {
            return Err(PluginError::ManifestValidation(format!(
                "Update rate {}Hz exceeds max {}Hz for {:?} plugins",
                manifest.constraints.update_rate_hz, max_constraints.update_rate_hz, manifest.class
            )));
        }

        Ok(())
    }
}

/// Load and validate plugin manifest from file
pub async fn load_manifest(path: &Path) -> PluginResult<PluginManifest> {
    let content = tokio::fs::read_to_string(path).await?;
    let manifest: PluginManifest = serde_yaml::from_str(&content)
        .map_err(|e| PluginError::ManifestValidation(format!("YAML parse error: {}", e)))?;

    let validator = ManifestValidator::default();
    validator.validate(&manifest)?;

    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to build a valid test manifest with sensible defaults.
    fn test_manifest(class: PluginClass) -> PluginManifest {
        PluginManifest {
            id: Uuid::new_v4(),
            name: "Test Plugin".to_string(),
            version: "1.0.0".to_string(),
            description: "A test plugin".to_string(),
            author: "Test Author".to_string(),
            license: "MIT".to_string(),
            homepage: None,
            class,
            capabilities: vec![Capability::ReadTelemetry],
            operations: vec![PluginOperation::TelemetryProcessor],
            constraints: PluginConstraints {
                max_execution_time_us: 100,
                max_memory_bytes: 1024 * 1024,
                update_rate_hz: 60,
                cpu_affinity: None,
            },
            entry_points: EntryPoints {
                wasm_module: Some("plugin.wasm".to_string()),
                native_library: None,
                main_function: "process".to_string(),
                init_function: Some("init".to_string()),
                cleanup_function: Some("cleanup".to_string()),
            },
            config_schema: None,
            signature: None,
        }
    }

    #[test]
    fn test_valid_safe_manifest_passes_validation() -> Result<(), PluginError> {
        let validator = ManifestValidator::default();
        let manifest = test_manifest(PluginClass::Safe);
        validator.validate(&manifest)
    }

    #[test]
    fn test_valid_fast_manifest_passes_validation() -> Result<(), PluginError> {
        let validator = ManifestValidator::default();
        let mut manifest = test_manifest(PluginClass::Fast);
        manifest.constraints.max_execution_time_us = 100;
        manifest.constraints.max_memory_bytes = 2 * 1024 * 1024;
        manifest.constraints.update_rate_hz = 1000;
        validator.validate(&manifest)
    }

    #[test]
    fn test_empty_name_rejected() {
        let validator = ManifestValidator::default();
        let mut manifest = test_manifest(PluginClass::Safe);
        manifest.name = String::new();
        let result = validator.validate(&manifest);
        assert!(result.is_err());
        let err_msg = format!("{}", result.err().unwrap_or_else(|| unreachable!()));
        assert!(err_msg.contains("name"));
    }

    #[test]
    fn test_empty_author_rejected() {
        let validator = ManifestValidator::default();
        let mut manifest = test_manifest(PluginClass::Safe);
        manifest.author = String::new();
        let result = validator.validate(&manifest);
        assert!(result.is_err());
    }

    #[test]
    fn test_disallowed_capability_for_safe_plugin() {
        let validator = ManifestValidator::default();
        let mut manifest = test_manifest(PluginClass::Safe);
        // ProcessDsp is NOT allowed for Safe plugins
        manifest.capabilities = vec![Capability::ProcessDsp];
        let result = validator.validate(&manifest);
        assert!(result.is_err());
        let err_msg = format!("{}", result.err().unwrap_or_else(|| unreachable!()));
        assert!(err_msg.contains("ProcessDsp"));
    }

    #[test]
    fn test_process_dsp_allowed_for_fast_plugin() -> Result<(), PluginError> {
        let validator = ManifestValidator::default();
        let mut manifest = test_manifest(PluginClass::Fast);
        manifest.capabilities = vec![Capability::ProcessDsp];
        manifest.constraints.max_execution_time_us = 100;
        manifest.constraints.max_memory_bytes = 2 * 1024 * 1024;
        manifest.constraints.update_rate_hz = 1000;
        validator.validate(&manifest)
    }

    #[test]
    fn test_execution_time_exceeds_safe_limit() {
        let validator = ManifestValidator::default();
        let mut manifest = test_manifest(PluginClass::Safe);
        // Safe max is 5000us
        manifest.constraints.max_execution_time_us = 10_000;
        let result = validator.validate(&manifest);
        assert!(result.is_err());
        let err_msg = format!("{}", result.err().unwrap_or_else(|| unreachable!()));
        assert!(err_msg.contains("Execution time"));
    }

    #[test]
    fn test_memory_exceeds_safe_limit() {
        let validator = ManifestValidator::default();
        let mut manifest = test_manifest(PluginClass::Safe);
        // Safe max is 16MB
        manifest.constraints.max_memory_bytes = 32 * 1024 * 1024;
        let result = validator.validate(&manifest);
        assert!(result.is_err());
        let err_msg = format!("{}", result.err().unwrap_or_else(|| unreachable!()));
        assert!(err_msg.contains("Memory budget"));
    }

    #[test]
    fn test_update_rate_exceeds_safe_limit() {
        let validator = ManifestValidator::default();
        let mut manifest = test_manifest(PluginClass::Safe);
        // Safe max is 200Hz
        manifest.constraints.update_rate_hz = 500;
        let result = validator.validate(&manifest);
        assert!(result.is_err());
        let err_msg = format!("{}", result.err().unwrap_or_else(|| unreachable!()));
        assert!(err_msg.contains("Update rate"));
    }

    #[test]
    fn test_execution_time_exceeds_fast_limit() {
        let validator = ManifestValidator::default();
        let mut manifest = test_manifest(PluginClass::Fast);
        // Fast max is 200us
        manifest.constraints.max_execution_time_us = 500;
        manifest.constraints.max_memory_bytes = 1024 * 1024;
        manifest.constraints.update_rate_hz = 1000;
        let result = validator.validate(&manifest);
        assert!(result.is_err());
    }

    #[test]
    fn test_manifest_serialization_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let manifest = test_manifest(PluginClass::Safe);
        let json = serde_json::to_string(&manifest)?;
        let deserialized: PluginManifest = serde_json::from_str(&json)?;
        assert_eq!(deserialized.name, manifest.name);
        assert_eq!(deserialized.version, manifest.version);
        assert_eq!(deserialized.class, manifest.class);
        assert_eq!(deserialized.capabilities, manifest.capabilities);
        Ok(())
    }

    #[test]
    fn test_manifest_yaml_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let manifest = test_manifest(PluginClass::Fast);
        let yaml = serde_yaml::to_string(&manifest)?;
        let deserialized: PluginManifest = serde_yaml::from_str(&yaml)?;
        assert_eq!(deserialized.name, manifest.name);
        assert_eq!(deserialized.author, manifest.author);
        Ok(())
    }

    #[test]
    fn test_multiple_capabilities_validated() -> Result<(), PluginError> {
        let validator = ManifestValidator::default();
        let mut manifest = test_manifest(PluginClass::Safe);
        manifest.capabilities = vec![
            Capability::ReadTelemetry,
            Capability::ModifyTelemetry,
            Capability::ControlLeds,
            Capability::InterPluginComm,
        ];
        validator.validate(&manifest)
    }

    #[test]
    fn test_network_capability_not_allowed_for_safe() {
        let validator = ManifestValidator::default();
        let mut manifest = test_manifest(PluginClass::Safe);
        manifest.capabilities = vec![Capability::Network {
            hosts: vec!["example.com".to_string()],
        }];
        let result = validator.validate(&manifest);
        assert!(result.is_err());
    }

    #[test]
    fn test_filesystem_capability_not_allowed_for_safe() {
        let validator = ManifestValidator::default();
        let mut manifest = test_manifest(PluginClass::Safe);
        manifest.capabilities = vec![Capability::FileSystem {
            paths: vec!["/tmp".to_string()],
        }];
        let result = validator.validate(&manifest);
        assert!(result.is_err());
    }

    #[test]
    fn test_constraints_at_exact_limit_pass() -> Result<(), PluginError> {
        let validator = ManifestValidator::default();
        let mut manifest = test_manifest(PluginClass::Safe);
        // Exactly at the Safe limits
        manifest.constraints.max_execution_time_us = 5000;
        manifest.constraints.max_memory_bytes = 16 * 1024 * 1024;
        manifest.constraints.update_rate_hz = 200;
        validator.validate(&manifest)
    }

    #[test]
    fn test_plugin_operation_variants_serialize() -> Result<(), Box<dyn std::error::Error>> {
        let ops = vec![
            PluginOperation::TelemetryProcessor,
            PluginOperation::LedMapper,
            PluginOperation::DspFilter,
            PluginOperation::TelemetrySource,
        ];
        let json = serde_json::to_string(&ops)?;
        let deserialized: Vec<PluginOperation> = serde_json::from_str(&json)?;
        assert_eq!(ops, deserialized);
        Ok(())
    }
}

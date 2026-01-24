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

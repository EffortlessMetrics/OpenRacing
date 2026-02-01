//! Racing Wheel Plugin System
//!
//! This crate provides a two-tier plugin system for the racing wheel software:
//! 1. Safe WASM plugins with capability-based sandboxing (60-200Hz operations)
//! 2. Fast native plugins with SPSC shared memory and RT watchdog
//!
//! The system includes crash isolation, budget violation detection, and quarantine
//! policies for repeatedly failing plugins.

#![deny(static_mut_refs)]
#![deny(unused_must_use)]
#![deny(clippy::unwrap_used)]

pub mod abi;
pub mod capability;
pub mod helper;
pub mod host;
pub mod manifest;
pub mod native;
pub mod quarantine;
pub mod sdk;
pub mod wasm;

use racing_wheel_engine::NormalizedTelemetry;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;
use uuid::Uuid;

/// Plugin execution result
pub type PluginResult<T> = Result<T, PluginError>;

/// Plugin system errors
#[derive(Error, Debug)]
pub enum PluginError {
    #[error("Plugin manifest validation failed: {0}")]
    ManifestValidation(String),

    #[error("Plugin loading failed: {0}")]
    LoadingFailed(String),

    #[error("Plugin execution timeout: {duration:?}")]
    ExecutionTimeout { duration: Duration },

    #[error("Plugin budget violation: used {used_us}μs, budget {budget_us}μs")]
    BudgetViolation { used_us: u32, budget_us: u32 },

    #[error("Plugin crashed: {reason}")]
    Crashed { reason: String },

    #[error("Plugin quarantined: {plugin_id}")]
    Quarantined { plugin_id: Uuid },

    #[error("Capability violation: {capability}")]
    CapabilityViolation { capability: String },

    #[error("WASM runtime error: {0}")]
    WasmRuntime(#[from] wasmtime::Error),

    #[error("Native plugin error: {0}")]
    NativePlugin(#[from] libloading::Error),

    #[error("IPC error: {0}")]
    Ipc(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Plugin execution class
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PluginClass {
    /// Safe WASM plugins (60-200Hz, sandboxed)
    Safe,
    /// Fast native plugins (RT, isolated helper process)
    Fast,
}

/// Plugin execution context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginContext {
    pub plugin_id: Uuid,
    pub class: PluginClass,
    pub update_rate_hz: u32,
    pub budget_us: u32,
    pub capabilities: Vec<String>,
}

/// Plugin output for telemetry processing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginTelemetryOutput {
    pub modified_telemetry: Option<NormalizedTelemetry>,
    pub custom_data: serde_json::Value,
}

/// Plugin output for LED mapping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginLedOutput {
    pub led_pattern: Vec<u8>, // RGB values
    pub brightness: f32,      // 0.0-1.0
    pub duration_ms: u32,
}

/// Plugin output for DSP filtering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDspOutput {
    pub modified_ffb: f32, // -1.0 to 1.0
    pub filter_state: serde_json::Value,
}

/// Combined plugin output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PluginOutput {
    Telemetry(PluginTelemetryOutput),
    Led(PluginLedOutput),
    Dsp(PluginDspOutput),
}

/// Plugin execution statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginStats {
    pub executions: u64,
    pub total_time_us: u64,
    pub avg_time_us: f64,
    pub max_time_us: u32,
    pub budget_violations: u32,
    pub crashes: u32,
    pub last_execution: Option<chrono::DateTime<chrono::Utc>>,
}

impl Default for PluginStats {
    fn default() -> Self {
        Self {
            executions: 0,
            total_time_us: 0,
            avg_time_us: 0.0,
            max_time_us: 0,
            budget_violations: 0,
            crashes: 0,
            last_execution: None,
        }
    }
}

/// Plugin trait for both WASM and native plugins
#[async_trait::async_trait]
pub trait Plugin: Send + Sync {
    /// Get plugin metadata
    fn manifest(&self) -> &manifest::PluginManifest;

    /// Initialize plugin with configuration
    async fn initialize(&mut self, config: serde_json::Value) -> PluginResult<()>;

    /// Process telemetry data
    async fn process_telemetry(
        &mut self,
        input: &NormalizedTelemetry,
        context: &PluginContext,
    ) -> PluginResult<PluginOutput>;

    /// Process LED mapping
    async fn process_led_mapping(
        &mut self,
        input: &NormalizedTelemetry,
        context: &PluginContext,
    ) -> PluginResult<PluginOutput>;

    /// Process DSP filtering (fast plugins only)
    async fn process_dsp(
        &mut self,
        ffb_input: f32,
        wheel_speed: f32,
        context: &PluginContext,
    ) -> PluginResult<PluginOutput>;

    /// Shutdown plugin gracefully
    async fn shutdown(&mut self) -> PluginResult<()>;
}

/// Re-export main types
pub use abi::*;
pub use capability::*;
pub use host::*;
pub use manifest::*;
pub use quarantine::*;

#[cfg(test)]
mod wasm_property_tests;

#[cfg(test)]
mod native_property_tests;

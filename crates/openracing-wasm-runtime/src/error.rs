//! Error types for the WASM runtime.
//!
//! This module provides error handling for all WASM runtime operations.

use std::time::Duration;
use thiserror::Error;

/// WASM runtime result type.
pub type WasmResult<T> = Result<T, WasmError>;

/// WASM runtime errors.
#[derive(Error, Debug)]
pub enum WasmError {
    /// Plugin loading failed
    #[error("Plugin loading failed: {0}")]
    LoadingFailed(String),

    /// Plugin execution timeout
    #[error("Plugin execution timeout: {duration:?}")]
    ExecutionTimeout {
        /// Duration of the timeout
        duration: Duration,
    },

    /// Plugin budget violation
    #[error("Plugin budget violation: used {used_us}μs, budget {budget_us}μs")]
    BudgetViolation {
        /// Used time in microseconds
        used_us: u32,
        /// Budget in microseconds
        budget_us: u32,
    },

    /// Plugin crashed (trap)
    #[error("Plugin crashed: {reason}")]
    Crashed {
        /// Reason for the crash
        reason: String,
    },

    /// Plugin not found
    #[error("Plugin not found: {0}")]
    PluginNotFound(String),

    /// Plugin is disabled
    #[error("Plugin is disabled: {reason}")]
    PluginDisabled {
        /// Reason the plugin was disabled
        reason: String,
    },

    /// Plugin not initialized
    #[error("Plugin not initialized")]
    PluginNotInitialized,

    /// Capability violation
    #[error("Capability violation: {capability}")]
    CapabilityViolation {
        /// The capability that was violated
        capability: String,
    },

    /// WASM runtime error from wasmtime
    #[error("WASM runtime error: {0}")]
    WasmRuntime(#[from] wasmtime::Error),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Invalid WASM module
    #[error("Invalid WASM module: {0}")]
    InvalidModule(String),

    /// Missing required export
    #[error("Missing required export: {0}")]
    MissingExport(String),

    /// Maximum instances reached
    #[error("Maximum plugin instances ({0}) reached")]
    MaxInstancesReached(usize),

    /// Module compilation failed
    #[error("Module compilation failed: {0}")]
    CompilationFailed(String),

    /// Module compilation timed out
    #[error("Module compilation timed out after {duration:?}")]
    CompilationTimeout {
        /// How long compilation was allowed to run
        duration: Duration,
    },

    /// Plugin fuel exhausted (instruction budget exceeded)
    #[error("Plugin fuel exhausted (limit: {fuel_limit} instructions)")]
    FuelExhausted {
        /// The fuel limit that was configured
        fuel_limit: u64,
    },
}

impl WasmError {
    /// Check if this error indicates a plugin crash
    #[must_use]
    pub fn is_crash(&self) -> bool {
        matches!(self, Self::Crashed { .. })
    }

    /// Check if this error indicates a timeout
    #[must_use]
    pub fn is_timeout(&self) -> bool {
        matches!(self, Self::ExecutionTimeout { .. })
    }

    /// Check if this error indicates a budget violation
    #[must_use]
    pub fn is_budget_violation(&self) -> bool {
        matches!(
            self,
            Self::BudgetViolation { .. } | Self::FuelExhausted { .. }
        )
    }

    /// Check if this error indicates a capability violation
    #[must_use]
    pub fn is_capability_violation(&self) -> bool {
        matches!(self, Self::CapabilityViolation { .. })
    }

    /// Check if this error indicates fuel exhaustion
    #[must_use]
    pub fn is_fuel_exhausted(&self) -> bool {
        matches!(self, Self::FuelExhausted { .. })
    }

    /// Check if this error indicates a compilation timeout
    #[must_use]
    pub fn is_compilation_timeout(&self) -> bool {
        matches!(self, Self::CompilationTimeout { .. })
    }

    /// Create a loading failed error with context
    #[must_use]
    pub fn loading_failed(msg: impl Into<String>) -> Self {
        Self::LoadingFailed(msg.into())
    }

    /// Create a plugin not found error
    #[must_use]
    pub fn plugin_not_found(id: impl std::fmt::Display) -> Self {
        Self::PluginNotFound(id.to_string())
    }

    /// Create a crashed error
    #[must_use]
    pub fn crashed(reason: impl Into<String>) -> Self {
        Self::Crashed {
            reason: reason.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_is_crash() {
        let error = WasmError::crashed("test crash");
        assert!(error.is_crash());
        assert!(!error.is_timeout());
    }

    #[test]
    fn test_error_is_timeout() {
        let error = WasmError::ExecutionTimeout {
            duration: Duration::from_secs(1),
        };
        assert!(error.is_timeout());
        assert!(!error.is_crash());
    }

    #[test]
    fn test_error_is_budget_violation() {
        let error = WasmError::BudgetViolation {
            used_us: 100,
            budget_us: 50,
        };
        assert!(error.is_budget_violation());
        assert!(!error.is_capability_violation());
    }

    #[test]
    fn test_error_is_capability_violation() {
        let error = WasmError::CapabilityViolation {
            capability: "read_telemetry".to_string(),
        };
        assert!(error.is_capability_violation());
        assert!(!error.is_budget_violation());
    }

    #[test]
    fn test_error_display() {
        let error = WasmError::loading_failed("test error");
        let msg = format!("{}", error);
        assert!(msg.contains("test error"));
    }
}

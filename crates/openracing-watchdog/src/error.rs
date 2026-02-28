//! Error types for the watchdog system.
//!
//! This module provides error handling for watchdog operations with
//! proper error classification and context.

use thiserror::Error;

/// Errors that can occur during watchdog operations.
#[derive(Debug, Clone, Error)]
pub enum WatchdogError {
    /// Plugin not found in the watchdog registry.
    #[error("Plugin not found: {0}")]
    PluginNotFound(String),

    /// Component not registered with the watchdog.
    #[error("Component not registered: {0:?}")]
    ComponentNotFound(crate::health::SystemComponent),

    /// Plugin is already quarantined.
    #[error("Plugin '{0}' is already quarantined")]
    AlreadyQuarantined(String),

    /// Plugin is not currently quarantined.
    #[error("Plugin '{0}' is not quarantined")]
    NotQuarantined(String),

    /// Invalid configuration provided.
    #[error("Invalid configuration: {0}")]
    InvalidConfiguration(String),

    /// Health check failed.
    #[error("Health check failed for {component}: {reason}")]
    HealthCheckFailed {
        /// The component that failed the health check.
        component: crate::health::SystemComponent,
        /// The reason for the failure.
        reason: String,
    },

    /// Quarantine operation failed.
    #[error("Quarantine operation failed: {0}")]
    QuarantineFailed(String),

    /// Timeout exceeded.
    #[error("Timeout exceeded for {0}: {1:?}")]
    TimeoutExceeded(String, std::time::Duration),

    /// Callback registration failed.
    #[error("Failed to register callback: {0}")]
    CallbackRegistrationFailed(String),

    /// Statistics operation failed.
    #[error("Statistics operation failed: {0}")]
    StatsFailed(String),
}

impl WatchdogError {
    /// Create a plugin not found error.
    #[must_use]
    pub fn plugin_not_found(plugin_id: impl Into<String>) -> Self {
        Self::PluginNotFound(plugin_id.into())
    }

    /// Create a component not found error.
    #[must_use]
    pub fn component_not_found(component: crate::health::SystemComponent) -> Self {
        Self::ComponentNotFound(component)
    }

    /// Create an already quarantined error.
    #[must_use]
    pub fn already_quarantined(plugin_id: impl Into<String>) -> Self {
        Self::AlreadyQuarantined(plugin_id.into())
    }

    /// Create a not quarantined error.
    #[must_use]
    pub fn not_quarantined(plugin_id: impl Into<String>) -> Self {
        Self::NotQuarantined(plugin_id.into())
    }

    /// Create an invalid configuration error.
    #[must_use]
    pub fn invalid_configuration(reason: impl Into<String>) -> Self {
        Self::InvalidConfiguration(reason.into())
    }

    /// Create a health check failed error.
    #[must_use]
    pub fn health_check_failed(
        component: crate::health::SystemComponent,
        reason: impl Into<String>,
    ) -> Self {
        Self::HealthCheckFailed {
            component,
            reason: reason.into(),
        }
    }

    /// Create a quarantine failed error.
    #[must_use]
    pub fn quarantine_failed(reason: impl Into<String>) -> Self {
        Self::QuarantineFailed(reason.into())
    }

    /// Create a timeout exceeded error.
    #[must_use]
    pub fn timeout_exceeded(context: impl Into<String>, duration: std::time::Duration) -> Self {
        Self::TimeoutExceeded(context.into(), duration)
    }
}

/// A specialized `Result` type for watchdog operations.
pub type WatchdogResult<T> = std::result::Result<T, WatchdogError>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health::SystemComponent;

    #[test]
    fn test_error_display() {
        let err = WatchdogError::plugin_not_found("test_plugin");
        assert!(err.to_string().contains("test_plugin"));

        let err = WatchdogError::health_check_failed(SystemComponent::RtThread, "timeout");
        assert!(err.to_string().contains("RT Thread"));
        assert!(err.to_string().contains("timeout"));
    }

    #[test]
    fn test_error_constructors() {
        let err = WatchdogError::not_quarantined("plugin_x");
        assert!(matches!(err, WatchdogError::NotQuarantined(_)));

        let err = WatchdogError::invalid_configuration("timeout too low");
        assert!(matches!(err, WatchdogError::InvalidConfiguration(_)));
    }
}

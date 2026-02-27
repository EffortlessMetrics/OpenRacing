//! Configuration types for hardware watchdog.

use crate::error::{HardwareWatchdogError, HardwareWatchdogResult};

/// Hardware watchdog configuration.
///
/// # Real-Time Safety
///
/// This struct is created during initialization and contains only
/// primitive types. It requires no heap allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct WatchdogConfig {
    /// Watchdog timeout in milliseconds.
    ///
    /// Default: 100ms per safety requirements.
    pub timeout_ms: u32,

    /// Maximum response time for safe state transition in microseconds.
    ///
    /// Default: 1000μs (1ms) per safety requirements.
    pub max_response_time_us: u32,

    /// Number of feed failures before triggering safe state.
    ///
    /// Default: 0 (immediate safe state on any failure).
    pub max_feed_failures: u32,

    /// Enable periodic health checks.
    ///
    /// When enabled, the watchdog will periodically verify its own state.
    pub health_check_enabled: bool,

    /// Health check interval in milliseconds.
    ///
    /// Only used when `health_check_enabled` is true.
    pub health_check_interval_ms: u32,
}

impl WatchdogConfig {
    /// Create a new configuration with the specified timeout.
    ///
    /// # Arguments
    ///
    /// * `timeout_ms` - Watchdog timeout in milliseconds (10-5000ms).
    ///
    /// # Errors
    ///
    /// Returns an error if `timeout_ms` is outside the valid range.
    pub fn new(timeout_ms: u32) -> HardwareWatchdogResult<Self> {
        if !(10..=5000).contains(&timeout_ms) {
            return Err(HardwareWatchdogError::invalid_configuration(
                "timeout_ms must be between 10 and 5000",
            ));
        }
        Ok(Self {
            timeout_ms,
            ..Self::default()
        })
    }

    /// Create a configuration builder.
    #[must_use]
    pub fn builder() -> WatchdogConfigBuilder {
        WatchdogConfigBuilder::default()
    }

    /// Validate the configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if any configuration values are invalid.
    pub fn validate(&self) -> HardwareWatchdogResult<()> {
        if !(10..=5000).contains(&self.timeout_ms) {
            return Err(HardwareWatchdogError::invalid_configuration(
                "timeout_ms must be between 10 and 5000",
            ));
        }
        if self.max_response_time_us > 10000 {
            return Err(HardwareWatchdogError::invalid_configuration(
                "max_response_time_us must not exceed 10000",
            ));
        }
        if self.health_check_enabled && self.health_check_interval_ms < 10 {
            return Err(HardwareWatchdogError::invalid_configuration(
                "health_check_interval_ms must be at least 10 when enabled",
            ));
        }
        Ok(())
    }

    /// Get the timeout in microseconds.
    #[must_use]
    pub fn timeout_us(&self) -> u64 {
        u64::from(self.timeout_ms) * 1000
    }

    /// Get the max response time as a Duration (requires std).
    #[cfg(feature = "std")]
    #[must_use]
    pub fn max_response_time(&self) -> core::time::Duration {
        core::time::Duration::from_micros(u64::from(self.max_response_time_us))
    }
}

impl Default for WatchdogConfig {
    fn default() -> Self {
        Self {
            timeout_ms: 100,
            max_response_time_us: 1000,
            max_feed_failures: 0,
            health_check_enabled: true,
            health_check_interval_ms: 100,
        }
    }
}

/// Builder for `WatchdogConfig`.
#[derive(Debug, Default)]
pub struct WatchdogConfigBuilder {
    config: WatchdogConfig,
}

impl WatchdogConfigBuilder {
    /// Set the timeout in milliseconds.
    #[must_use]
    pub fn timeout_ms(mut self, ms: u32) -> Self {
        self.config.timeout_ms = ms;
        self
    }

    /// Set the maximum response time in microseconds.
    #[must_use]
    pub fn max_response_time_us(mut self, us: u32) -> Self {
        self.config.max_response_time_us = us;
        self
    }

    /// Set the maximum feed failures before safe state.
    #[must_use]
    pub fn max_feed_failures(mut self, count: u32) -> Self {
        self.config.max_feed_failures = count;
        self
    }

    /// Enable or disable health checks.
    #[must_use]
    pub fn health_check_enabled(mut self, enabled: bool) -> Self {
        self.config.health_check_enabled = enabled;
        self
    }

    /// Set the health check interval in milliseconds.
    #[must_use]
    pub fn health_check_interval_ms(mut self, ms: u32) -> Self {
        self.config.health_check_interval_ms = ms;
        self
    }

    /// Build the configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration is invalid.
    pub fn build(self) -> HardwareWatchdogResult<WatchdogConfig> {
        self.config.validate()?;
        Ok(self.config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = WatchdogConfig::default();
        assert_eq!(config.timeout_ms, 100);
        assert_eq!(config.max_response_time_us, 1000);
        assert!(config.health_check_enabled);
    }

    #[test]
    fn test_config_validation() {
        let config = WatchdogConfig::new(5);
        assert!(config.is_err());

        let config = WatchdogConfig::new(6000);
        assert!(config.is_err());

        let config = WatchdogConfig::new(100);
        assert!(config.is_ok());
    }

    #[test]
    fn test_config_builder() {
        let result = WatchdogConfig::builder()
            .timeout_ms(200)
            .max_response_time_us(500)
            .max_feed_failures(3)
            .health_check_enabled(false)
            .build();
        assert!(result.is_ok());
        if let Ok(config) = result {
            assert_eq!(config.timeout_ms, 200);
            assert_eq!(config.max_response_time_us, 500);
            assert_eq!(config.max_feed_failures, 3);
            assert!(!config.health_check_enabled);
        }
    }

    #[test]
    fn test_timeout_us() {
        let result = WatchdogConfig::new(100);
        assert!(result.is_ok());
        if let Ok(config) = result {
            assert_eq!(config.timeout_us(), 100_000);
        }
    }
}

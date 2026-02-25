//! Core watchdog system for monitoring plugins and system components.
//!
//! This module provides the main `WatchdogSystem` struct that coordinates
//! plugin execution monitoring, component health checks, and quarantine management.

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::FaultType;
use crate::error::{WatchdogError, WatchdogResult};
use crate::health::{HealthCheck, HealthStatus, SystemComponent};
use crate::quarantine::{QuarantineManager, QuarantineReason};
use crate::stats::PluginStats;

/// Callback function type for fault notifications.
pub type FaultCallback = Box<dyn Fn(FaultType, &str) + Send + Sync>;

/// Watchdog configuration for different components.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchdogConfig {
    /// Plugin execution timeout per tick (microseconds).
    pub plugin_timeout_us: u64,
    /// Maximum consecutive plugin timeouts before quarantine.
    pub plugin_max_timeouts: u32,
    /// Plugin quarantine duration.
    pub plugin_quarantine_duration: Duration,
    /// RT thread heartbeat timeout (milliseconds).
    pub rt_thread_timeout_ms: u64,
    /// HID communication timeout (milliseconds).
    pub hid_timeout_ms: u64,
    /// Telemetry timeout (milliseconds).
    pub telemetry_timeout_ms: u64,
    /// System health check interval.
    pub health_check_interval: Duration,
}

impl Default for WatchdogConfig {
    fn default() -> Self {
        Self {
            plugin_timeout_us: 100,
            plugin_max_timeouts: 5,
            plugin_quarantine_duration: Duration::from_secs(300),
            rt_thread_timeout_ms: 10,
            hid_timeout_ms: 50,
            telemetry_timeout_ms: 1000,
            health_check_interval: Duration::from_millis(100),
        }
    }
}

impl WatchdogConfig {
    /// Validate the configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if any configuration values are invalid.
    pub fn validate(&self) -> WatchdogResult<()> {
        if self.plugin_timeout_us == 0 {
            return Err(WatchdogError::invalid_configuration(
                "plugin_timeout_us must be greater than 0",
            ));
        }
        if self.plugin_max_timeouts == 0 {
            return Err(WatchdogError::invalid_configuration(
                "plugin_max_timeouts must be greater than 0",
            ));
        }
        if self.plugin_quarantine_duration.is_zero() {
            return Err(WatchdogError::invalid_configuration(
                "plugin_quarantine_duration must be greater than 0",
            ));
        }
        if self.rt_thread_timeout_ms == 0 {
            return Err(WatchdogError::invalid_configuration(
                "rt_thread_timeout_ms must be greater than 0",
            ));
        }
        Ok(())
    }

    /// Create a configuration builder.
    #[must_use]
    pub fn builder() -> WatchdogConfigBuilder {
        WatchdogConfigBuilder::default()
    }
}

/// Builder for `WatchdogConfig`.
#[derive(Debug, Default)]
pub struct WatchdogConfigBuilder {
    config: WatchdogConfig,
}

impl WatchdogConfigBuilder {
    /// Set plugin timeout in microseconds.
    #[must_use]
    pub fn plugin_timeout_us(mut self, us: u64) -> Self {
        self.config.plugin_timeout_us = us;
        self
    }

    /// Set maximum consecutive timeouts before quarantine.
    #[must_use]
    pub fn plugin_max_timeouts(mut self, count: u32) -> Self {
        self.config.plugin_max_timeouts = count;
        self
    }

    /// Set quarantine duration.
    #[must_use]
    pub fn plugin_quarantine_duration(mut self, duration: Duration) -> Self {
        self.config.plugin_quarantine_duration = duration;
        self
    }

    /// Set RT thread timeout in milliseconds.
    #[must_use]
    pub fn rt_thread_timeout_ms(mut self, ms: u64) -> Self {
        self.config.rt_thread_timeout_ms = ms;
        self
    }

    /// Set HID timeout in milliseconds.
    #[must_use]
    pub fn hid_timeout_ms(mut self, ms: u64) -> Self {
        self.config.hid_timeout_ms = ms;
        self
    }

    /// Set telemetry timeout in milliseconds.
    #[must_use]
    pub fn telemetry_timeout_ms(mut self, ms: u64) -> Self {
        self.config.telemetry_timeout_ms = ms;
        self
    }

    /// Set health check interval.
    #[must_use]
    pub fn health_check_interval(mut self, interval: Duration) -> Self {
        self.config.health_check_interval = interval;
        self
    }

    /// Build the configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration is invalid.
    pub fn build(self) -> WatchdogResult<WatchdogConfig> {
        self.config.validate()?;
        Ok(self.config)
    }
}

/// Watchdog system for monitoring plugins and system components.
///
/// This struct provides comprehensive monitoring capabilities:
/// - Plugin execution timing and timeout detection
/// - Component health tracking via heartbeats
/// - Automatic quarantine of misbehaving plugins
/// - Fault notification callbacks
///
/// # Thread Safety
///
/// The `WatchdogSystem` uses internal synchronization via `RwLock` for
/// thread-safe access to statistics and health status.
///
/// # RT Safety
///
/// The following methods are RT-safe (no allocations after initialization):
/// - `record_plugin_execution()`
/// - `heartbeat()`
/// - `is_plugin_quarantined()`
/// - `get_plugin_stats()` (read-only)
pub struct WatchdogSystem {
    config: WatchdogConfig,
    plugin_stats: RwLock<HashMap<String, PluginStats>>,
    health_checks: RwLock<HashMap<SystemComponent, HealthCheck>>,
    quarantine_manager: RwLock<QuarantineManager>,
    last_health_check: RwLock<Instant>,
    quarantine_policy_enabled: RwLock<bool>,
    fault_callbacks: RwLock<Vec<Arc<dyn Fn(FaultType, &str) + Send + Sync>>>,
}

impl WatchdogSystem {
    /// Create a new watchdog system with the given configuration.
    #[must_use]
    pub fn new(config: WatchdogConfig) -> Self {
        let mut health_checks = HashMap::new();
        for component in SystemComponent::all() {
            health_checks.insert(component, HealthCheck::new(component));
        }

        let quarantine_duration = config.plugin_quarantine_duration;

        Self {
            config,
            plugin_stats: RwLock::new(HashMap::new()),
            health_checks: RwLock::new(health_checks),
            quarantine_manager: RwLock::new(QuarantineManager::with_default_duration(
                quarantine_duration,
            )),
            last_health_check: RwLock::new(Instant::now()),
            quarantine_policy_enabled: RwLock::new(true),
            fault_callbacks: RwLock::new(Vec::new()),
        }
    }

    /// Record plugin execution.
    ///
    /// Returns `Some(FaultType)` if the plugin was quarantined due to consecutive timeouts.
    ///
    /// # RT Safety
    ///
    /// This method is RT-safe. The internal write lock is held for a minimal duration.
    /// Note: First execution for a plugin ID will allocate a new entry.
    pub fn record_plugin_execution(
        &self,
        plugin_id: &str,
        execution_time_us: u64,
    ) -> Option<FaultType> {
        let should_quarantine = {
            let mut stats = self.plugin_stats.write();
            let plugin_stats = stats.entry(plugin_id.to_string()).or_default();

            if execution_time_us > self.config.plugin_timeout_us {
                plugin_stats.record_timeout(execution_time_us);

                let quarantine_enabled = *self.quarantine_policy_enabled.read();
                quarantine_enabled
                    && plugin_stats.consecutive_timeouts >= self.config.plugin_max_timeouts
            } else {
                plugin_stats.record_success(execution_time_us);
                false
            }
        };

        if should_quarantine {
            self.quarantine_plugin(plugin_id);
            return Some(FaultType::PluginOverrun);
        }

        None
    }

    /// Quarantine a plugin.
    fn quarantine_plugin(&self, plugin_id: &str) {
        let reason = {
            let mut stats = self.plugin_stats.write();
            let plugin_stats = stats.entry(plugin_id.to_string()).or_default();
            plugin_stats.consecutive_timeouts = 0;

            let mut manager = self.quarantine_manager.write();
            manager.quarantine(
                plugin_id,
                Some(self.config.plugin_quarantine_duration),
                QuarantineReason::ConsecutiveTimeouts,
                plugin_stats,
            );
            QuarantineReason::ConsecutiveTimeouts
        };

        tracing::warn!(
            plugin_id = plugin_id,
            reason = ?reason,
            "Plugin quarantined due to consecutive timeouts"
        );

        // Notify fault callbacks
        let callbacks = self.fault_callbacks.read();
        for callback in callbacks.iter() {
            callback(FaultType::PluginOverrun, plugin_id);
        }
    }

    /// Check if a plugin is quarantined.
    ///
    /// # RT Safety
    ///
    /// This method is RT-safe and performs no allocations.
    #[must_use]
    pub fn is_plugin_quarantined(&self, plugin_id: &str) -> bool {
        let stats = self.plugin_stats.read();
        stats.get(plugin_id).map_or(false, |s| s.is_quarantined())
    }

    /// Get plugin statistics.
    ///
    /// # RT Safety
    ///
    /// This method is RT-safe for read access.
    #[must_use]
    pub fn get_plugin_stats(&self, plugin_id: &str) -> Option<PluginStats> {
        let stats = self.plugin_stats.read();
        stats.get(plugin_id).cloned()
    }

    /// Get all plugin statistics.
    #[must_use]
    pub fn get_all_plugin_stats(&self) -> HashMap<String, PluginStats> {
        let stats = self.plugin_stats.read();
        stats.clone()
    }

    /// Release a plugin from quarantine.
    ///
    /// # Errors
    ///
    /// Returns an error if the plugin is not found or not quarantined.
    pub fn release_plugin_quarantine(&self, plugin_id: &str) -> WatchdogResult<()> {
        let mut stats = self.plugin_stats.write();
        let plugin_stats = stats
            .get_mut(plugin_id)
            .ok_or_else(|| WatchdogError::plugin_not_found(plugin_id))?;

        if !plugin_stats.is_quarantined() {
            return Err(WatchdogError::not_quarantined(plugin_id));
        }

        let mut manager = self.quarantine_manager.write();
        manager.release(plugin_id, plugin_stats)?;

        tracing::info!(plugin_id = plugin_id, "Plugin released from quarantine");
        Ok(())
    }

    /// Get all quarantined plugins with remaining duration.
    #[must_use]
    pub fn get_quarantined_plugins(&self) -> Vec<(String, Duration)> {
        let manager = self.quarantine_manager.read();
        manager.get_quarantined()
    }

    /// Record system component heartbeat.
    ///
    /// # RT Safety
    ///
    /// This method is RT-safe.
    pub fn heartbeat(&self, component: SystemComponent) {
        let mut checks = self.health_checks.write();
        if let Some(health_check) = checks.get_mut(&component) {
            health_check.heartbeat();
        }
    }

    /// Report system component failure.
    pub fn report_component_failure(&self, component: SystemComponent, error: Option<String>) {
        let status = {
            let mut checks = self.health_checks.write();
            if let Some(health_check) = checks.get_mut(&component) {
                health_check.report_failure(error);
                health_check.status
            } else {
                return;
            }
        };

        if status == HealthStatus::Faulted {
            let fault_type = match component {
                SystemComponent::RtThread => FaultType::TimingViolation,
                SystemComponent::HidCommunication => FaultType::UsbStall,
                SystemComponent::TelemetryAdapter => FaultType::TimingViolation,
                SystemComponent::PluginHost => FaultType::PluginOverrun,
                SystemComponent::SafetySystem => FaultType::SafetyInterlockViolation,
                SystemComponent::DeviceManager => FaultType::UsbStall,
            };

            let callbacks = self.fault_callbacks.read();
            for callback in callbacks.iter() {
                callback(fault_type, &format!("{component:?}"));
            }
        }
    }

    /// Add metric to a system component.
    pub fn add_component_metric(&self, component: SystemComponent, name: String, value: f64) {
        let mut checks = self.health_checks.write();
        if let Some(health_check) = checks.get_mut(&component) {
            health_check.add_metric(name, value);
        }
    }

    /// Get component health status.
    #[must_use]
    pub fn get_component_health(&self, component: SystemComponent) -> Option<HealthCheck> {
        let checks = self.health_checks.read();
        checks.get(&component).cloned()
    }

    /// Get all component health statuses.
    #[must_use]
    pub fn get_all_component_health(&self) -> HashMap<SystemComponent, HealthCheck> {
        let checks = self.health_checks.read();
        checks.clone()
    }

    /// Perform periodic health checks.
    ///
    /// Returns a list of detected faults.
    pub fn perform_health_checks(&self) -> Vec<FaultType> {
        let now = Instant::now();
        {
            let last = self.last_health_check.read();
            if now.duration_since(*last) < self.config.health_check_interval {
                return Vec::new();
            }
        }

        {
            let mut last = self.last_health_check.write();
            *last = now;
        }

        let mut faults = Vec::new();

        // Check component timeouts
        {
            let mut checks = self.health_checks.write();

            if let Some(health_check) = checks.get_mut(&SystemComponent::RtThread) {
                if health_check
                    .check_timeout(Duration::from_millis(self.config.rt_thread_timeout_ms))
                {
                    faults.push(FaultType::TimingViolation);
                }
            }

            if let Some(health_check) = checks.get_mut(&SystemComponent::HidCommunication) {
                if health_check.check_timeout(Duration::from_millis(self.config.hid_timeout_ms)) {
                    faults.push(FaultType::UsbStall);
                }
            }

            if let Some(health_check) = checks.get_mut(&SystemComponent::TelemetryAdapter) {
                health_check.check_timeout(Duration::from_millis(self.config.telemetry_timeout_ms));
            }
        }

        // Clean up expired quarantines
        {
            let mut stats = self.plugin_stats.write();
            let mut manager = self.quarantine_manager.write();
            manager.cleanup_expired_with_stats(&mut stats);
        }

        faults
    }

    /// Add a fault callback.
    pub fn add_fault_callback<F>(&self, callback: F)
    where
        F: Fn(FaultType, &str) + Send + Sync + 'static,
    {
        let mut callbacks = self.fault_callbacks.write();
        callbacks.push(Arc::new(callback));
    }

    /// Enable or disable quarantine policy.
    pub fn set_quarantine_policy_enabled(&self, enabled: bool) {
        let mut policy = self.quarantine_policy_enabled.write();
        *policy = enabled;
    }

    /// Check if quarantine policy is enabled.
    #[must_use]
    pub fn is_quarantine_policy_enabled(&self) -> bool {
        *self.quarantine_policy_enabled.read()
    }

    /// Get the current configuration.
    #[must_use]
    pub fn get_config(&self) -> &WatchdogConfig {
        &self.config
    }

    /// Get system health summary.
    #[must_use]
    pub fn get_health_summary(&self) -> HashMap<SystemComponent, HealthStatus> {
        let checks = self.health_checks.read();
        checks
            .iter()
            .map(|(component, health_check)| (*component, health_check.status))
            .collect()
    }

    /// Check if any component is faulted.
    #[must_use]
    pub fn has_faulted_components(&self) -> bool {
        let checks = self.health_checks.read();
        checks
            .values()
            .any(|health_check| health_check.status == HealthStatus::Faulted)
    }

    /// Get plugin performance metrics.
    #[must_use]
    pub fn get_plugin_performance_metrics(&self) -> HashMap<String, HashMap<String, f64>> {
        let stats = self.plugin_stats.read();
        stats
            .iter()
            .map(|(plugin_id, plugin_stats)| {
                let mut metrics = HashMap::new();
                metrics.insert(
                    "total_executions".to_string(),
                    plugin_stats.total_executions as f64,
                );
                metrics.insert(
                    "average_execution_time_us".to_string(),
                    plugin_stats.average_execution_time_us(),
                );
                metrics.insert(
                    "timeout_rate_percent".to_string(),
                    plugin_stats.timeout_rate(),
                );
                metrics.insert(
                    "quarantine_count".to_string(),
                    plugin_stats.quarantine_count as f64,
                );
                metrics.insert(
                    "consecutive_timeouts".to_string(),
                    plugin_stats.consecutive_timeouts as f64,
                );

                if let Some(remaining) = plugin_stats.quarantine_remaining() {
                    metrics.insert(
                        "quarantine_remaining_ms".to_string(),
                        remaining.as_millis() as f64,
                    );
                }

                (plugin_id.clone(), metrics)
            })
            .collect()
    }

    /// Reset plugin statistics.
    ///
    /// # Errors
    ///
    /// Returns an error if the plugin is not found.
    pub fn reset_plugin_stats(&self, plugin_id: &str) -> WatchdogResult<()> {
        let mut stats = self.plugin_stats.write();
        let plugin_stats = stats
            .get_mut(plugin_id)
            .ok_or_else(|| WatchdogError::plugin_not_found(plugin_id))?;
        plugin_stats.reset();
        Ok(())
    }

    /// Reset all plugin statistics.
    pub fn reset_all_plugin_stats(&self) {
        let mut stats = self.plugin_stats.write();
        stats.clear();
    }

    /// Get component uptime (time since last heartbeat).
    #[must_use]
    pub fn get_component_uptime(&self, component: SystemComponent) -> Option<Duration> {
        let checks = self.health_checks.read();
        checks
            .get(&component)
            .and_then(|health_check| health_check.last_heartbeat)
            .map(|last_heartbeat| last_heartbeat.elapsed())
    }

    /// Register a plugin for monitoring.
    pub fn register_plugin(&self, plugin_id: &str) {
        let mut stats = self.plugin_stats.write();
        stats.entry(plugin_id.to_string()).or_default();
    }

    /// Unregister a plugin from monitoring.
    ///
    /// # Errors
    ///
    /// Returns an error if the plugin is not found.
    pub fn unregister_plugin(&self, plugin_id: &str) -> WatchdogResult<()> {
        let mut stats = self.plugin_stats.write();
        if stats.remove(plugin_id).is_some() {
            Ok(())
        } else {
            Err(WatchdogError::plugin_not_found(plugin_id))
        }
    }

    /// Get the number of registered plugins.
    #[must_use]
    pub fn plugin_count(&self) -> usize {
        self.plugin_stats.read().len()
    }
}

impl Default for WatchdogSystem {
    fn default() -> Self {
        Self::new(WatchdogConfig::default())
    }
}

impl std::fmt::Debug for WatchdogSystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WatchdogSystem")
            .field("config", &self.config)
            .field("plugin_count", &self.plugin_stats.read().len())
            .field(
                "quarantine_policy_enabled",
                &*self.quarantine_policy_enabled.read(),
            )
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_execution_tracking() {
        let watchdog = WatchdogSystem::default();

        let fault = watchdog.record_plugin_execution("test_plugin", 50);
        assert!(fault.is_none());

        let stats = watchdog.get_plugin_stats("test_plugin");
        assert!(stats.is_some());
        let stats = stats.unwrap();
        assert_eq!(stats.total_executions, 1);
        assert_eq!(stats.last_execution_time_us, 50);
        assert_eq!(stats.timeout_count, 0);
    }

    #[test]
    fn test_plugin_timeout_detection() {
        let watchdog = WatchdogSystem::default();

        let fault = watchdog.record_plugin_execution("test_plugin", 150);
        assert!(fault.is_none()); // First timeout, not quarantined yet

        let stats = watchdog.get_plugin_stats("test_plugin").unwrap();
        assert_eq!(stats.timeout_count, 1);
        assert_eq!(stats.consecutive_timeouts, 1);
    }

    #[test]
    fn test_plugin_quarantine() {
        let watchdog = WatchdogSystem::default();

        for i in 0..5 {
            let fault = watchdog.record_plugin_execution("test_plugin", 150);
            if i == 4 {
                assert_eq!(fault, Some(FaultType::PluginOverrun));
            }
        }

        assert!(watchdog.is_plugin_quarantined("test_plugin"));

        let quarantined = watchdog.get_quarantined_plugins();
        assert_eq!(quarantined.len(), 1);
        assert_eq!(quarantined[0].0, "test_plugin");
    }

    #[test]
    fn test_plugin_quarantine_release() {
        let watchdog = WatchdogSystem::default();

        for _ in 0..5 {
            watchdog.record_plugin_execution("test_plugin", 150);
        }

        assert!(watchdog.is_plugin_quarantined("test_plugin"));

        watchdog.release_plugin_quarantine("test_plugin").unwrap();
        assert!(!watchdog.is_plugin_quarantined("test_plugin"));
    }

    #[test]
    fn test_system_component_health() {
        let watchdog = WatchdogSystem::default();

        let health = watchdog
            .get_component_health(SystemComponent::RtThread)
            .unwrap();
        assert_eq!(health.status, HealthStatus::Unknown);

        watchdog.heartbeat(SystemComponent::RtThread);
        let health = watchdog
            .get_component_health(SystemComponent::RtThread)
            .unwrap();
        assert_eq!(health.status, HealthStatus::Healthy);

        watchdog
            .report_component_failure(SystemComponent::RtThread, Some("Test error".to_string()));
        let health = watchdog
            .get_component_health(SystemComponent::RtThread)
            .unwrap();
        assert_eq!(health.consecutive_failures, 1);
        assert_eq!(health.last_error, Some("Test error".to_string()));
    }

    #[test]
    fn test_health_summary() {
        let watchdog = WatchdogSystem::default();

        watchdog.heartbeat(SystemComponent::RtThread);
        watchdog.report_component_failure(SystemComponent::HidCommunication, None);

        let summary = watchdog.get_health_summary();
        assert_eq!(summary[&SystemComponent::RtThread], HealthStatus::Healthy);
        assert_eq!(
            summary[&SystemComponent::HidCommunication],
            HealthStatus::Healthy
        ); // Only 1 failure

        assert!(!watchdog.has_faulted_components());
    }

    #[test]
    fn test_fault_callback() {
        let watchdog = WatchdogSystem::default();

        watchdog.add_fault_callback(|_fault_type, _component| {
            // Callback received
        });

        for _ in 0..5 {
            watchdog.record_plugin_execution("test_plugin", 150);
        }

        assert!(watchdog.is_plugin_quarantined("test_plugin"));
    }

    #[test]
    fn test_performance_metrics() {
        let watchdog = WatchdogSystem::default();

        watchdog.record_plugin_execution("plugin1", 50);
        watchdog.record_plugin_execution("plugin1", 150);
        watchdog.record_plugin_execution("plugin2", 75);

        let metrics = watchdog.get_plugin_performance_metrics();

        assert_eq!(metrics.len(), 2);
        assert!(metrics.contains_key("plugin1"));
        assert!(metrics.contains_key("plugin2"));

        let plugin1_metrics = &metrics["plugin1"];
        assert_eq!(plugin1_metrics["total_executions"], 2.0);
        assert_eq!(plugin1_metrics["timeout_rate_percent"], 50.0);
    }

    #[test]
    fn test_quarantine_policy_toggle() {
        let watchdog = WatchdogSystem::default();

        watchdog.set_quarantine_policy_enabled(false);

        for _ in 0..10 {
            let fault = watchdog.record_plugin_execution("test_plugin", 150);
            assert!(fault.is_none());
        }

        assert!(!watchdog.is_plugin_quarantined("test_plugin"));

        watchdog.set_quarantine_policy_enabled(true);

        for i in 0..5 {
            let fault = watchdog.record_plugin_execution("test_plugin2", 150);
            if i == 4 {
                assert_eq!(fault, Some(FaultType::PluginOverrun));
            }
        }

        assert!(watchdog.is_plugin_quarantined("test_plugin2"));
    }

    #[test]
    fn test_config_builder() {
        let config = WatchdogConfig::builder()
            .plugin_timeout_us(200)
            .plugin_max_timeouts(3)
            .plugin_quarantine_duration(Duration::from_secs(600))
            .rt_thread_timeout_ms(20)
            .build()
            .unwrap();

        assert_eq!(config.plugin_timeout_us, 200);
        assert_eq!(config.plugin_max_timeouts, 3);
        assert_eq!(config.plugin_quarantine_duration, Duration::from_secs(600));
        assert_eq!(config.rt_thread_timeout_ms, 20);
    }

    #[test]
    fn test_config_validation() {
        let config = WatchdogConfig {
            plugin_timeout_us: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());

        let config = WatchdogConfig {
            plugin_max_timeouts: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_plugin_registration() {
        let watchdog = WatchdogSystem::default();

        assert_eq!(watchdog.plugin_count(), 0);

        watchdog.register_plugin("plugin_a");
        watchdog.register_plugin("plugin_b");
        assert_eq!(watchdog.plugin_count(), 2);

        watchdog.unregister_plugin("plugin_a").unwrap();
        assert_eq!(watchdog.plugin_count(), 1);

        let result = watchdog.unregister_plugin("unknown");
        assert!(result.is_err());
    }
}

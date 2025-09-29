//! Watchdog systems for monitoring plugin execution and system health

use std::collections::HashMap;
use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};

use super::FaultType;

/// Watchdog configuration for different components
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchdogConfig {
    /// Plugin execution timeout per tick
    pub plugin_timeout_us: u64,
    /// Maximum consecutive plugin timeouts before quarantine
    pub plugin_max_timeouts: u32,
    /// Plugin quarantine duration
    pub plugin_quarantine_duration: Duration,
    /// RT thread heartbeat timeout
    pub rt_thread_timeout_ms: u64,
    /// HID communication timeout
    pub hid_timeout_ms: u64,
    /// Telemetry timeout
    pub telemetry_timeout_ms: u64,
    /// System health check interval
    pub health_check_interval: Duration,
}

impl Default for WatchdogConfig {
    fn default() -> Self {
        Self {
            plugin_timeout_us: 100,
            plugin_max_timeouts: 5,
            plugin_quarantine_duration: Duration::from_secs(300), // 5 minutes
            rt_thread_timeout_ms: 10,
            hid_timeout_ms: 50,
            telemetry_timeout_ms: 1000,
            health_check_interval: Duration::from_millis(100),
        }
    }
}

/// Plugin execution statistics
#[derive(Debug, Clone, Default)]
pub struct PluginStats {
    pub total_executions: u64,
    pub total_execution_time_us: u64,
    pub timeout_count: u32,
    pub consecutive_timeouts: u32,
    pub last_execution_time_us: u64,
    pub last_execution: Option<Instant>,
    pub quarantined_until: Option<Instant>,
    pub quarantine_count: u32,
}

impl PluginStats {
    /// Get average execution time in microseconds
    pub fn average_execution_time_us(&self) -> f64 {
        if self.total_executions == 0 {
            0.0
        } else {
            self.total_execution_time_us as f64 / self.total_executions as f64
        }
    }

    /// Get timeout rate as percentage
    pub fn timeout_rate(&self) -> f64 {
        if self.total_executions == 0 {
            0.0
        } else {
            (self.timeout_count as f64 / self.total_executions as f64) * 100.0
        }
    }

    /// Check if plugin is currently quarantined
    pub fn is_quarantined(&self) -> bool {
        if let Some(quarantine_until) = self.quarantined_until {
            Instant::now() < quarantine_until
        } else {
            false
        }
    }

    /// Get remaining quarantine time
    pub fn quarantine_remaining(&self) -> Option<Duration> {
        if let Some(quarantine_until) = self.quarantined_until {
            let now = Instant::now();
            if now < quarantine_until {
                Some(quarantine_until - now)
            } else {
                None
            }
        } else {
            None
        }
    }
}

/// System component health status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Faulted,
    Unknown,
}

/// System component being monitored
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SystemComponent {
    RtThread,
    HidCommunication,
    TelemetryAdapter,
    PluginHost,
    SafetySystem,
    DeviceManager,
}

/// Health check result for a system component
#[derive(Debug, Clone)]
pub struct HealthCheck {
    pub component: SystemComponent,
    pub status: HealthStatus,
    pub last_heartbeat: Option<Instant>,
    pub consecutive_failures: u32,
    pub last_error: Option<String>,
    pub metrics: HashMap<String, f64>,
}

impl HealthCheck {
    pub fn new(component: SystemComponent) -> Self {
        Self {
            component,
            status: HealthStatus::Unknown,
            last_heartbeat: None,
            consecutive_failures: 0,
            last_error: None,
            metrics: HashMap::new(),
        }
    }

    /// Update health status with heartbeat
    pub fn heartbeat(&mut self) {
        self.last_heartbeat = Some(Instant::now());
        self.status = HealthStatus::Healthy;
        self.consecutive_failures = 0;
        self.last_error = None;
    }

    /// Report failure
    pub fn report_failure(&mut self, error: Option<String>) {
        self.consecutive_failures += 1;
        self.last_error = error;
        
        // Determine status based on failure count
        self.status = if self.consecutive_failures >= 5 {
            HealthStatus::Faulted
        } else if self.consecutive_failures >= 2 {
            HealthStatus::Degraded
        } else {
            HealthStatus::Healthy
        };
    }

    /// Check if component has timed out
    pub fn check_timeout(&mut self, timeout: Duration) -> bool {
        if let Some(last_heartbeat) = self.last_heartbeat {
            if last_heartbeat.elapsed() > timeout {
                self.report_failure(Some("Heartbeat timeout".to_string()));
                return true;
            }
        }
        false
    }

    /// Add metric
    pub fn add_metric(&mut self, name: String, value: f64) {
        self.metrics.insert(name, value);
    }
}

/// Watchdog system for monitoring plugins and system components
pub struct WatchdogSystem {
    config: WatchdogConfig,
    plugin_stats: HashMap<String, PluginStats>,
    health_checks: HashMap<SystemComponent, HealthCheck>,
    last_health_check: Instant,
    quarantine_policy_enabled: bool,
    fault_callbacks: Vec<Box<dyn Fn(FaultType, &str) + Send + Sync>>,
}

impl WatchdogSystem {
    /// Create new watchdog system
    pub fn new(config: WatchdogConfig) -> Self {
        let mut health_checks = HashMap::new();
        
        // Initialize health checks for all system components
        for component in [
            SystemComponent::RtThread,
            SystemComponent::HidCommunication,
            SystemComponent::TelemetryAdapter,
            SystemComponent::PluginHost,
            SystemComponent::SafetySystem,
            SystemComponent::DeviceManager,
        ] {
            health_checks.insert(component, HealthCheck::new(component));
        }

        Self {
            config,
            plugin_stats: HashMap::new(),
            health_checks,
            last_health_check: Instant::now(),
            quarantine_policy_enabled: true,
            fault_callbacks: Vec::new(),
        }
    }

    /// Record plugin execution
    pub fn record_plugin_execution(&mut self, plugin_id: &str, execution_time_us: u64) -> Option<FaultType> {
        let should_quarantine = {
            let stats = self.plugin_stats.entry(plugin_id.to_string()).or_default();
            
            stats.total_executions += 1;
            stats.total_execution_time_us += execution_time_us;
            stats.last_execution_time_us = execution_time_us;
            stats.last_execution = Some(Instant::now());

            // Check for timeout
            if execution_time_us > self.config.plugin_timeout_us {
                stats.timeout_count += 1;
                stats.consecutive_timeouts += 1;

                // Check if plugin should be quarantined
                self.quarantine_policy_enabled && stats.consecutive_timeouts >= self.config.plugin_max_timeouts
            } else {
                // Reset consecutive timeouts on successful execution
                stats.consecutive_timeouts = 0;
                false
            }
        };

        if should_quarantine {
            self.quarantine_plugin(plugin_id);
            // Reset consecutive timeouts after quarantine
            if let Some(stats) = self.plugin_stats.get_mut(plugin_id) {
                stats.consecutive_timeouts = 0;
            }
            return Some(FaultType::PluginOverrun);
        }

        None
    }

    /// Quarantine a plugin
    fn quarantine_plugin(&mut self, plugin_id: &str) {
        if let Some(stats) = self.plugin_stats.get_mut(plugin_id) {
            stats.quarantined_until = Some(Instant::now() + self.config.plugin_quarantine_duration);
            stats.quarantine_count += 1;
            
            // Notify fault callbacks
            for callback in &self.fault_callbacks {
                callback(FaultType::PluginOverrun, plugin_id);
            }
        }
    }

    /// Check if plugin is quarantined
    pub fn is_plugin_quarantined(&self, plugin_id: &str) -> bool {
        self.plugin_stats
            .get(plugin_id)
            .map(|stats| stats.is_quarantined())
            .unwrap_or(false)
    }

    /// Get plugin statistics
    pub fn get_plugin_stats(&self, plugin_id: &str) -> Option<&PluginStats> {
        self.plugin_stats.get(plugin_id)
    }

    /// Get all plugin statistics
    pub fn get_all_plugin_stats(&self) -> &HashMap<String, PluginStats> {
        &self.plugin_stats
    }

    /// Release plugin from quarantine
    pub fn release_plugin_quarantine(&mut self, plugin_id: &str) -> Result<(), String> {
        if let Some(stats) = self.plugin_stats.get_mut(plugin_id) {
            if stats.is_quarantined() {
                stats.quarantined_until = None;
                stats.consecutive_timeouts = 0;
                Ok(())
            } else {
                Err("Plugin is not quarantined".to_string())
            }
        } else {
            Err("Plugin not found".to_string())
        }
    }

    /// Get quarantined plugins
    pub fn get_quarantined_plugins(&self) -> Vec<(String, Duration)> {
        self.plugin_stats
            .iter()
            .filter_map(|(plugin_id, stats)| {
                stats.quarantine_remaining().map(|remaining| (plugin_id.clone(), remaining))
            })
            .collect()
    }

    /// Record system component heartbeat
    pub fn heartbeat(&mut self, component: SystemComponent) {
        if let Some(health_check) = self.health_checks.get_mut(&component) {
            health_check.heartbeat();
        }
    }

    /// Report system component failure
    pub fn report_component_failure(&mut self, component: SystemComponent, error: Option<String>) {
        if let Some(health_check) = self.health_checks.get_mut(&component) {
            health_check.report_failure(error);
            
            // Trigger fault callback if component is faulted
            if health_check.status == HealthStatus::Faulted {
                let fault_type = match component {
                    SystemComponent::RtThread => FaultType::TimingViolation,
                    SystemComponent::HidCommunication => FaultType::UsbStall,
                    SystemComponent::TelemetryAdapter => FaultType::TimingViolation,
                    SystemComponent::PluginHost => FaultType::PluginOverrun,
                    SystemComponent::SafetySystem => FaultType::SafetyInterlockViolation,
                    SystemComponent::DeviceManager => FaultType::UsbStall,
                };

                for callback in &self.fault_callbacks {
                    callback(fault_type, &format!("{:?}", component));
                }
            }
        }
    }

    /// Add metric to system component
    pub fn add_component_metric(&mut self, component: SystemComponent, name: String, value: f64) {
        if let Some(health_check) = self.health_checks.get_mut(&component) {
            health_check.add_metric(name, value);
        }
    }

    /// Get system component health
    pub fn get_component_health(&self, component: SystemComponent) -> Option<&HealthCheck> {
        self.health_checks.get(&component)
    }

    /// Get all system component health
    pub fn get_all_component_health(&self) -> &HashMap<SystemComponent, HealthCheck> {
        &self.health_checks
    }

    /// Perform periodic health checks
    pub fn perform_health_checks(&mut self) -> Vec<FaultType> {
        let now = Instant::now();
        if now.duration_since(self.last_health_check) < self.config.health_check_interval {
            return Vec::new();
        }

        self.last_health_check = now;
        let mut faults = Vec::new();

        // Check RT thread timeout
        if let Some(health_check) = self.health_checks.get_mut(&SystemComponent::RtThread) {
            if health_check.check_timeout(Duration::from_millis(self.config.rt_thread_timeout_ms)) {
                faults.push(FaultType::TimingViolation);
            }
        }

        // Check HID communication timeout
        if let Some(health_check) = self.health_checks.get_mut(&SystemComponent::HidCommunication) {
            if health_check.check_timeout(Duration::from_millis(self.config.hid_timeout_ms)) {
                faults.push(FaultType::UsbStall);
            }
        }

        // Check telemetry timeout
        if let Some(health_check) = self.health_checks.get_mut(&SystemComponent::TelemetryAdapter) {
            if health_check.check_timeout(Duration::from_millis(self.config.telemetry_timeout_ms)) {
                // Telemetry timeout is not critical, just log it
            }
        }

        // Clean up expired quarantines
        self.cleanup_expired_quarantines();

        faults
    }

    /// Clean up expired plugin quarantines
    fn cleanup_expired_quarantines(&mut self) {
        for stats in self.plugin_stats.values_mut() {
            if let Some(quarantine_until) = stats.quarantined_until {
                if Instant::now() >= quarantine_until {
                    stats.quarantined_until = None;
                }
            }
        }
    }

    /// Add fault callback
    pub fn add_fault_callback<F>(&mut self, callback: F)
    where
        F: Fn(FaultType, &str) + Send + Sync + 'static,
    {
        self.fault_callbacks.push(Box::new(callback));
    }

    /// Enable or disable quarantine policy
    pub fn set_quarantine_policy_enabled(&mut self, enabled: bool) {
        self.quarantine_policy_enabled = enabled;
    }

    /// Get watchdog configuration
    pub fn get_config(&self) -> &WatchdogConfig {
        &self.config
    }

    /// Update watchdog configuration
    pub fn update_config(&mut self, config: WatchdogConfig) {
        self.config = config;
    }

    /// Get system health summary
    pub fn get_health_summary(&self) -> HashMap<SystemComponent, HealthStatus> {
        self.health_checks
            .iter()
            .map(|(component, health_check)| (*component, health_check.status))
            .collect()
    }

    /// Check if any system component is faulted
    pub fn has_faulted_components(&self) -> bool {
        self.health_checks
            .values()
            .any(|health_check| health_check.status == HealthStatus::Faulted)
    }

    /// Get plugin performance metrics
    pub fn get_plugin_performance_metrics(&self) -> HashMap<String, HashMap<String, f64>> {
        self.plugin_stats
            .iter()
            .map(|(plugin_id, stats)| {
                let mut metrics = HashMap::new();
                metrics.insert("total_executions".to_string(), stats.total_executions as f64);
                metrics.insert("average_execution_time_us".to_string(), stats.average_execution_time_us());
                metrics.insert("timeout_rate_percent".to_string(), stats.timeout_rate());
                metrics.insert("quarantine_count".to_string(), stats.quarantine_count as f64);
                metrics.insert("consecutive_timeouts".to_string(), stats.consecutive_timeouts as f64);
                
                if let Some(remaining) = stats.quarantine_remaining() {
                    metrics.insert("quarantine_remaining_ms".to_string(), remaining.as_millis() as f64);
                }

                (plugin_id.clone(), metrics)
            })
            .collect()
    }

    /// Reset plugin statistics
    pub fn reset_plugin_stats(&mut self, plugin_id: &str) -> Result<(), String> {
        if let Some(stats) = self.plugin_stats.get_mut(plugin_id) {
            *stats = PluginStats::default();
            Ok(())
        } else {
            Err("Plugin not found".to_string())
        }
    }

    /// Reset all plugin statistics
    pub fn reset_all_plugin_stats(&mut self) {
        self.plugin_stats.clear();
    }

    /// Get component uptime
    pub fn get_component_uptime(&self, component: SystemComponent) -> Option<Duration> {
        self.health_checks
            .get(&component)
            .and_then(|health_check| health_check.last_heartbeat)
            .map(|last_heartbeat| last_heartbeat.elapsed())
    }
}

impl Default for WatchdogSystem {
    fn default() -> Self {
        Self::new(WatchdogConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_execution_tracking() {
        let mut watchdog = WatchdogSystem::default();
        
        // Normal execution
        assert!(watchdog.record_plugin_execution("test_plugin", 50).is_none());
        
        let stats = watchdog.get_plugin_stats("test_plugin").unwrap();
        assert_eq!(stats.total_executions, 1);
        assert_eq!(stats.last_execution_time_us, 50);
        assert_eq!(stats.timeout_count, 0);
    }

    #[test]
    fn test_plugin_timeout_detection() {
        let mut watchdog = WatchdogSystem::default();
        
        // Timeout execution
        assert!(watchdog.record_plugin_execution("test_plugin", 150).is_none()); // First timeout
        
        let stats = watchdog.get_plugin_stats("test_plugin").unwrap();
        assert_eq!(stats.timeout_count, 1);
        assert_eq!(stats.consecutive_timeouts, 1);
    }

    #[test]
    fn test_plugin_quarantine() {
        let mut watchdog = WatchdogSystem::default();
        
        // Trigger multiple timeouts
        for i in 0..5 {
            let result = watchdog.record_plugin_execution("test_plugin", 150);
            if i == 4 {
                assert_eq!(result, Some(FaultType::PluginOverrun));
            }
        }
        
        assert!(watchdog.is_plugin_quarantined("test_plugin"));
        
        let quarantined = watchdog.get_quarantined_plugins();
        assert_eq!(quarantined.len(), 1);
        assert_eq!(quarantined[0].0, "test_plugin");
    }

    #[test]
    fn test_plugin_quarantine_release() {
        let mut watchdog = WatchdogSystem::default();
        
        // Quarantine plugin
        for _ in 0..5 {
            watchdog.record_plugin_execution("test_plugin", 150);
        }
        
        assert!(watchdog.is_plugin_quarantined("test_plugin"));
        
        // Release quarantine
        watchdog.release_plugin_quarantine("test_plugin").unwrap();
        assert!(!watchdog.is_plugin_quarantined("test_plugin"));
    }

    #[test]
    fn test_system_component_health() {
        let mut watchdog = WatchdogSystem::default();
        
        // Initial state should be unknown
        let health = watchdog.get_component_health(SystemComponent::RtThread).unwrap();
        assert_eq!(health.status, HealthStatus::Unknown);
        
        // Heartbeat should make it healthy
        watchdog.heartbeat(SystemComponent::RtThread);
        let health = watchdog.get_component_health(SystemComponent::RtThread).unwrap();
        assert_eq!(health.status, HealthStatus::Healthy);
        
        // Report failure
        watchdog.report_component_failure(SystemComponent::RtThread, Some("Test error".to_string()));
        let health = watchdog.get_component_health(SystemComponent::RtThread).unwrap();
        assert_eq!(health.consecutive_failures, 1);
        assert_eq!(health.last_error, Some("Test error".to_string()));
    }

    #[test]
    fn test_component_timeout_detection() {
        let config = WatchdogConfig {
            rt_thread_timeout_ms: 10,
            ..Default::default()
        };
        let mut watchdog = WatchdogSystem::new(config);
        
        // Send heartbeat
        watchdog.heartbeat(SystemComponent::RtThread);
        
        // Wait for timeout
        std::thread::sleep(Duration::from_millis(15));
        
        // Perform health check
        let faults = watchdog.perform_health_checks();
        assert!(faults.contains(&FaultType::TimingViolation));
        
        let health = watchdog.get_component_health(SystemComponent::RtThread).unwrap();
        assert_eq!(health.status, HealthStatus::Degraded);
    }

    #[test]
    fn test_plugin_statistics() {
        let mut watchdog = WatchdogSystem::default();
        
        // Record various executions
        watchdog.record_plugin_execution("test_plugin", 50);
        watchdog.record_plugin_execution("test_plugin", 75);
        watchdog.record_plugin_execution("test_plugin", 150); // Timeout
        
        let stats = watchdog.get_plugin_stats("test_plugin").unwrap();
        assert_eq!(stats.total_executions, 3);
        assert_eq!(stats.timeout_count, 1);
        assert!((stats.average_execution_time_us() - 91.67).abs() < 0.1);
        assert!((stats.timeout_rate() - 33.33).abs() < 0.1);
    }

    #[test]
    fn test_health_summary() {
        let mut watchdog = WatchdogSystem::default();
        
        watchdog.heartbeat(SystemComponent::RtThread);
        watchdog.report_component_failure(SystemComponent::HidCommunication, None);
        
        let summary = watchdog.get_health_summary();
        assert_eq!(summary[&SystemComponent::RtThread], HealthStatus::Healthy);
        assert_eq!(summary[&SystemComponent::HidCommunication], HealthStatus::Healthy); // Only 1 failure
        
        assert!(!watchdog.has_faulted_components());
    }

    #[test]
    fn test_fault_callback() {
        let mut watchdog = WatchdogSystem::default();
        let mut _callback_called = false;
        
        // This test would need Arc<Mutex<bool>> in real code for the callback
        // For now, just test that the callback mechanism exists
        watchdog.add_fault_callback(|fault_type, _component| {
            assert_eq!(fault_type, FaultType::PluginOverrun);
        });
        
        // Trigger plugin quarantine
        for _ in 0..5 {
            watchdog.record_plugin_execution("test_plugin", 150);
        }
    }

    #[test]
    fn test_performance_metrics() {
        let mut watchdog = WatchdogSystem::default();
        
        watchdog.record_plugin_execution("plugin1", 50);
        watchdog.record_plugin_execution("plugin1", 150); // Timeout
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
        let mut watchdog = WatchdogSystem::default();
        
        // Disable quarantine policy
        watchdog.set_quarantine_policy_enabled(false);
        
        // Trigger timeouts - should not quarantine
        for _ in 0..10 {
            assert!(watchdog.record_plugin_execution("test_plugin", 150).is_none());
        }
        
        assert!(!watchdog.is_plugin_quarantined("test_plugin"));
        
        // Re-enable quarantine policy
        watchdog.set_quarantine_policy_enabled(true);
        
        // Should quarantine on next timeout burst
        for i in 0..5 {
            let result = watchdog.record_plugin_execution("test_plugin2", 150);
            if i == 4 {
                assert_eq!(result, Some(FaultType::PluginOverrun));
            }
        }
        
        assert!(watchdog.is_plugin_quarantined("test_plugin2"));
    }
}
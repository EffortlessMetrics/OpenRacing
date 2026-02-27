//! Component health status and check management.
//!
//! This module provides structures for tracking the health of system
//! components including heartbeat tracking and failure detection.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// System component health status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[derive(Default)]
pub enum HealthStatus {
    /// Component is operating normally.
    Healthy,
    /// Component is degraded but functional.
    Degraded,
    /// Component has failed and requires attention.
    Faulted,
    /// Component status is unknown (not yet checked).
    #[default]
    Unknown,
}


impl std::fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HealthStatus::Healthy => write!(f, "Healthy"),
            HealthStatus::Degraded => write!(f, "Degraded"),
            HealthStatus::Faulted => write!(f, "Faulted"),
            HealthStatus::Unknown => write!(f, "Unknown"),
        }
    }
}

/// System component being monitored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SystemComponent {
    /// Real-time processing thread.
    RtThread,
    /// HID device communication.
    HidCommunication,
    /// Telemetry data adapter.
    TelemetryAdapter,
    /// Plugin execution host.
    PluginHost,
    /// Safety system module.
    SafetySystem,
    /// Device manager.
    DeviceManager,
}

impl std::fmt::Display for SystemComponent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SystemComponent::RtThread => write!(f, "RT Thread"),
            SystemComponent::HidCommunication => write!(f, "HID Communication"),
            SystemComponent::TelemetryAdapter => write!(f, "Telemetry Adapter"),
            SystemComponent::PluginHost => write!(f, "Plugin Host"),
            SystemComponent::SafetySystem => write!(f, "Safety System"),
            SystemComponent::DeviceManager => write!(f, "Device Manager"),
        }
    }
}

impl SystemComponent {
    /// Get all system components as an iterator.
    pub fn all() -> impl Iterator<Item = Self> {
        [
            Self::RtThread,
            Self::HidCommunication,
            Self::TelemetryAdapter,
            Self::PluginHost,
            Self::SafetySystem,
            Self::DeviceManager,
        ]
        .into_iter()
    }
}

/// Health check result for a system component.
#[derive(Debug, Clone)]
pub struct HealthCheck {
    /// The component being monitored.
    pub component: SystemComponent,
    /// Current health status.
    pub status: HealthStatus,
    /// Timestamp of last heartbeat.
    pub last_heartbeat: Option<Instant>,
    /// Number of consecutive failures.
    pub consecutive_failures: u32,
    /// Last error message, if any.
    pub last_error: Option<String>,
    /// Additional metrics for the component.
    pub metrics: HashMap<String, f64>,
}

impl HealthCheck {
    /// Create a new health check for the given component.
    #[must_use]
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

    /// Record a heartbeat from the component.
    ///
    /// # RT Safety
    ///
    /// This method is RT-safe. The `HashMap` allocation only occurs for new metrics,
    /// which should be pre-allocated during initialization.
    pub fn heartbeat(&mut self) {
        self.last_heartbeat = Some(Instant::now());
        self.status = HealthStatus::Healthy;
        self.consecutive_failures = 0;
        self.last_error = None;
    }

    /// Report a failure from the component.
    ///
    /// Updates the status based on the number of consecutive failures:
    /// - 1 failure: Still healthy (transient)
    /// - 2-4 failures: Degraded
    /// - 5+ failures: Faulted
    pub fn report_failure(&mut self, error: Option<String>) {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        self.last_error = error;

        self.status = if self.consecutive_failures >= 5 {
            HealthStatus::Faulted
        } else if self.consecutive_failures >= 2 {
            HealthStatus::Degraded
        } else {
            HealthStatus::Healthy
        };
    }

    /// Check if component has timed out.
    ///
    /// Returns `true` if the component has timed out (no heartbeat within the timeout period).
    ///
    /// # RT Safety
    ///
    /// This method is RT-safe and performs no allocations.
    pub fn check_timeout(&mut self, timeout: Duration) -> bool {
        if let Some(last_heartbeat) = self.last_heartbeat
            && last_heartbeat.elapsed() > timeout {
                self.report_failure(Some("Heartbeat timeout".to_string()));
                return true;
            }
        false
    }

    /// Add a metric value.
    ///
    /// Note: This operation may allocate if the metric name is new.
    pub fn add_metric(&mut self, name: String, value: f64) {
        self.metrics.insert(name, value);
    }

    /// Clear the failure count and reset to healthy status.
    pub fn clear_failures(&mut self) {
        self.consecutive_failures = 0;
        self.last_error = None;
        if self.last_heartbeat.is_some() {
            self.status = HealthStatus::Healthy;
        }
    }

    /// Get time since last heartbeat.
    #[must_use]
    pub fn time_since_heartbeat(&self) -> Option<Duration> {
        self.last_heartbeat
            .as_ref()
            .map(std::time::Instant::elapsed)
    }
}

impl Default for HealthCheck {
    fn default() -> Self {
        Self::new(SystemComponent::RtThread)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_health_check_heartbeat() {
        let mut check = HealthCheck::new(SystemComponent::RtThread);
        assert_eq!(check.status, HealthStatus::Unknown);
        assert!(check.last_heartbeat.is_none());

        check.heartbeat();
        assert_eq!(check.status, HealthStatus::Healthy);
        assert!(check.last_heartbeat.is_some());
        assert_eq!(check.consecutive_failures, 0);
    }

    #[test]
    fn test_health_check_failure_progression() {
        let mut check = HealthCheck::new(SystemComponent::HidCommunication);

        // Single failure should stay healthy
        check.report_failure(Some("Error 1".to_string()));
        assert_eq!(check.status, HealthStatus::Healthy);
        assert_eq!(check.consecutive_failures, 1);

        // Second failure: degraded
        check.report_failure(Some("Error 2".to_string()));
        assert_eq!(check.status, HealthStatus::Degraded);

        // Fifth failure: faulted
        check.report_failure(None);
        check.report_failure(None);
        check.report_failure(None);
        assert_eq!(check.status, HealthStatus::Faulted);
    }

    #[test]
    fn test_health_check_timeout() {
        let mut check = HealthCheck::new(SystemComponent::TelemetryAdapter);
        check.heartbeat();

        // Should not timeout immediately
        assert!(!check.check_timeout(Duration::from_millis(100)));

        // Wait and check again
        thread::sleep(Duration::from_millis(50));
        assert!(!check.check_timeout(Duration::from_millis(100)));

        thread::sleep(Duration::from_millis(60));
        assert!(check.check_timeout(Duration::from_millis(100)));
        assert_eq!(check.status, HealthStatus::Healthy); // First failure
    }

    #[test]
    fn test_health_check_metrics() {
        let mut check = HealthCheck::new(SystemComponent::PluginHost);
        check.add_metric("latency_us".to_string(), 50.0);
        check.add_metric("throughput".to_string(), 1000.0);

        assert_eq!(check.metrics.len(), 2);
        assert!((check.metrics["latency_us"] - 50.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_health_check_clear_failures() {
        let mut check = HealthCheck::new(SystemComponent::SafetySystem);
        for _ in 0..5 {
            check.report_failure(Some("Test error".to_string()));
        }
        assert_eq!(check.status, HealthStatus::Faulted);

        check.heartbeat();
        check.clear_failures();
        assert_eq!(check.status, HealthStatus::Healthy);
        assert_eq!(check.consecutive_failures, 0);
    }

    #[test]
    fn test_system_component_all() {
        let components: Vec<_> = SystemComponent::all().collect();
        assert_eq!(components.len(), 6);
        assert!(components.contains(&SystemComponent::RtThread));
        assert!(components.contains(&SystemComponent::DeviceManager));
    }
}

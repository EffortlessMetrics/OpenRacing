//! Plugin quarantine system for repeatedly failing plugins

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::{PluginError, PluginResult};

/// Quarantine policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuarantinePolicy {
    /// Maximum crashes before quarantine
    pub max_crashes: u32,
    /// Maximum budget violations before quarantine
    pub max_budget_violations: u32,
    /// Time window for counting violations (minutes)
    pub violation_window_minutes: i64,
    /// Quarantine duration (minutes)
    pub quarantine_duration_minutes: i64,
    /// Maximum quarantine escalation levels
    pub max_escalation_levels: u32,
}

impl Default for QuarantinePolicy {
    fn default() -> Self {
        Self {
            max_crashes: 3,
            max_budget_violations: 10,
            violation_window_minutes: 60,    // 1 hour
            quarantine_duration_minutes: 60, // Start with 1 hour
            max_escalation_levels: 5,
        }
    }
}

/// Quarantine state for a plugin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuarantineState {
    pub plugin_id: Uuid,
    pub is_quarantined: bool,
    pub quarantine_start: Option<DateTime<Utc>>,
    pub quarantine_end: Option<DateTime<Utc>>,
    pub escalation_level: u32,
    pub total_crashes: u32,
    pub total_budget_violations: u32,
    pub recent_violations: Vec<ViolationRecord>,
}

/// Violation record for tracking plugin failures
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViolationRecord {
    pub timestamp: DateTime<Utc>,
    pub violation_type: ViolationType,
    pub details: String,
}

/// Types of plugin violations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ViolationType {
    Crash,
    BudgetViolation,
    CapabilityViolation,
    TimeoutViolation,
}

/// Plugin quarantine manager
pub struct QuarantineManager {
    policy: QuarantinePolicy,
    quarantine_states: HashMap<Uuid, QuarantineState>,
}

impl QuarantineManager {
    /// Create a new quarantine manager
    pub fn new(policy: QuarantinePolicy) -> Self {
        Self {
            policy,
            quarantine_states: HashMap::new(),
        }
    }

    /// Record a plugin violation
    pub fn record_violation(
        &mut self,
        plugin_id: Uuid,
        violation_type: ViolationType,
        details: String,
    ) -> PluginResult<()> {
        // First, ensure the state exists and record the violation
        {
            let state =
                self.quarantine_states
                    .entry(plugin_id)
                    .or_insert_with(|| QuarantineState {
                        plugin_id,
                        is_quarantined: false,
                        quarantine_start: None,
                        quarantine_end: None,
                        escalation_level: 0,
                        total_crashes: 0,
                        total_budget_violations: 0,
                        recent_violations: Vec::new(),
                    });

            // Record the violation
            let violation = ViolationRecord {
                timestamp: Utc::now(),
                violation_type: violation_type.clone(),
                details,
            };

            state.recent_violations.push(violation);

            // Update counters
            match violation_type {
                ViolationType::Crash => state.total_crashes += 1,
                ViolationType::BudgetViolation => state.total_budget_violations += 1,
                _ => {}
            }
        }

        // Now handle cleanup and quarantine check separately
        self.cleanup_old_violations_for_plugin(plugin_id);

        if self.should_quarantine_plugin(plugin_id) {
            self.quarantine_plugin_by_id(plugin_id)?;
        }

        Ok(())
    }

    /// Check if a plugin is currently quarantined
    pub fn is_quarantined(&mut self, plugin_id: Uuid) -> bool {
        if let Some(state) = self.quarantine_states.get_mut(&plugin_id) {
            // Check if quarantine period has expired
            if let Some(end_time) = state.quarantine_end
                && Utc::now() > end_time
            {
                state.is_quarantined = false;
                state.quarantine_start = None;
                state.quarantine_end = None;
                return false;
            }
            state.is_quarantined
        } else {
            false
        }
    }

    /// Get quarantine state for a plugin
    pub fn get_quarantine_state(&self, plugin_id: Uuid) -> Option<&QuarantineState> {
        self.quarantine_states.get(&plugin_id)
    }

    /// Get quarantine statistics for all plugins
    pub fn get_quarantine_stats(&self) -> HashMap<Uuid, QuarantineState> {
        self.quarantine_states.clone()
    }

    /// Manually quarantine a plugin for a specified duration
    pub fn manual_quarantine(
        &mut self,
        plugin_id: Uuid,
        duration_minutes: i64,
    ) -> PluginResult<()> {
        let state = self
            .quarantine_states
            .entry(plugin_id)
            .or_insert_with(|| QuarantineState {
                plugin_id,
                is_quarantined: false,
                quarantine_start: None,
                quarantine_end: None,
                escalation_level: 0,
                total_crashes: 0,
                total_budget_violations: 0,
                recent_violations: Vec::new(),
            });

        let now = Utc::now();
        state.is_quarantined = true;
        state.quarantine_start = Some(now);
        state.quarantine_end = Some(now + Duration::minutes(duration_minutes));

        Ok(())
    }

    /// Release a plugin from quarantine
    pub fn release_from_quarantine(&mut self, plugin_id: Uuid) -> PluginResult<()> {
        if let Some(state) = self.quarantine_states.get_mut(&plugin_id) {
            state.is_quarantined = false;
            state.quarantine_start = None;
            state.quarantine_end = None;
            Ok(())
        } else {
            Err(PluginError::ManifestValidation(format!(
                "Plugin {} not found in quarantine system",
                plugin_id
            )))
        }
    }

    /// Clean up old violations outside the time window for a specific plugin
    fn cleanup_old_violations_for_plugin(&mut self, plugin_id: Uuid) {
        if let Some(state) = self.quarantine_states.get_mut(&plugin_id) {
            let cutoff = Utc::now() - Duration::minutes(self.policy.violation_window_minutes);
            state.recent_violations.retain(|v| v.timestamp > cutoff);
        }
    }

    /// Check if a plugin should be quarantined based on recent violations
    fn should_quarantine_plugin(&self, plugin_id: Uuid) -> bool {
        if let Some(state) = self.quarantine_states.get(&plugin_id) {
            if state.is_quarantined {
                return false; // Already quarantined
            }

            let recent_crashes = state
                .recent_violations
                .iter()
                .filter(|v| v.violation_type == ViolationType::Crash)
                .count() as u32;

            let recent_budget_violations = state
                .recent_violations
                .iter()
                .filter(|v| v.violation_type == ViolationType::BudgetViolation)
                .count() as u32;

            recent_crashes >= self.policy.max_crashes
                || recent_budget_violations >= self.policy.max_budget_violations
        } else {
            false
        }
    }

    /// Quarantine a plugin by ID with escalating duration
    fn quarantine_plugin_by_id(&mut self, plugin_id: Uuid) -> PluginResult<()> {
        if let Some(state) = self.quarantine_states.get_mut(&plugin_id) {
            let now = Utc::now();
            state.is_quarantined = true;
            state.quarantine_start = Some(now);
            state.escalation_level += 1;

            // Escalating quarantine duration: base * 2^level
            let duration_minutes = self.policy.quarantine_duration_minutes
                * (2_i64.pow(
                    state
                        .escalation_level
                        .min(self.policy.max_escalation_levels),
                ));

            state.quarantine_end = Some(now + Duration::minutes(duration_minutes));

            tracing::warn!(
                plugin_id = %state.plugin_id,
                escalation_level = state.escalation_level,
                duration_minutes = duration_minutes,
                "Plugin quarantined due to repeated violations"
            );
        }

        Ok(())
    }
}

/// Tracker for plugin execution statistics
pub struct FailureTracker {
    stats: HashMap<Uuid, crate::PluginStats>,
}

impl Default for FailureTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl FailureTracker {
    pub fn new() -> Self {
        Self {
            stats: HashMap::new(),
        }
    }

    pub fn record_execution(&mut self, plugin_id: Uuid, duration_us: u32, success: bool) {
        let stats = self.stats.entry(plugin_id).or_default();

        stats.executions += 1;
        stats.total_time_us += duration_us as u64;
        stats.avg_time_us = stats.total_time_us as f64 / stats.executions as f64;

        if duration_us > stats.max_time_us {
            stats.max_time_us = duration_us;
        }

        if !success {
            stats.crashes += 1;
        }
    }

    pub fn get_stats(&self, plugin_id: Uuid) -> Option<&crate::PluginStats> {
        self.stats.get(&plugin_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_quarantine_policy() {
        let policy = QuarantinePolicy::default();
        assert_eq!(policy.max_crashes, 3);
        assert_eq!(policy.max_budget_violations, 10);
        assert_eq!(policy.violation_window_minutes, 60);
        assert_eq!(policy.quarantine_duration_minutes, 60);
        assert_eq!(policy.max_escalation_levels, 5);
    }

    #[test]
    fn test_new_plugin_not_quarantined() {
        let policy = QuarantinePolicy::default();
        let mut manager = QuarantineManager::new(policy);
        let plugin_id = Uuid::new_v4();
        assert!(!manager.is_quarantined(plugin_id));
    }

    #[test]
    fn test_single_violation_does_not_quarantine() -> Result<(), PluginError> {
        let policy = QuarantinePolicy::default();
        let mut manager = QuarantineManager::new(policy);
        let plugin_id = Uuid::new_v4();

        manager.record_violation(plugin_id, ViolationType::Crash, "test crash".to_string())?;
        assert!(!manager.is_quarantined(plugin_id));
        Ok(())
    }

    #[test]
    fn test_max_crashes_triggers_quarantine() -> Result<(), PluginError> {
        let policy = QuarantinePolicy {
            max_crashes: 3,
            ..QuarantinePolicy::default()
        };
        let mut manager = QuarantineManager::new(policy);
        let plugin_id = Uuid::new_v4();

        for i in 0..3 {
            manager.record_violation(plugin_id, ViolationType::Crash, format!("crash {i}"))?;
        }

        assert!(manager.is_quarantined(plugin_id));
        Ok(())
    }

    #[test]
    fn test_budget_violations_trigger_quarantine() -> Result<(), PluginError> {
        let policy = QuarantinePolicy {
            max_budget_violations: 3,
            ..QuarantinePolicy::default()
        };
        let mut manager = QuarantineManager::new(policy);
        let plugin_id = Uuid::new_v4();

        for i in 0..3 {
            manager.record_violation(
                plugin_id,
                ViolationType::BudgetViolation,
                format!("budget violation {i}"),
            )?;
        }

        assert!(manager.is_quarantined(plugin_id));
        Ok(())
    }

    #[test]
    fn test_manual_quarantine() -> Result<(), PluginError> {
        let policy = QuarantinePolicy::default();
        let mut manager = QuarantineManager::new(policy);
        let plugin_id = Uuid::new_v4();

        manager.manual_quarantine(plugin_id, 30)?;
        assert!(manager.is_quarantined(plugin_id));

        let state = manager.get_quarantine_state(plugin_id);
        assert!(state.is_some());
        let state = state.unwrap_or_else(|| unreachable!());
        assert!(state.quarantine_start.is_some());
        assert!(state.quarantine_end.is_some());
        Ok(())
    }

    #[test]
    fn test_release_from_quarantine() -> Result<(), PluginError> {
        let policy = QuarantinePolicy::default();
        let mut manager = QuarantineManager::new(policy);
        let plugin_id = Uuid::new_v4();

        manager.manual_quarantine(plugin_id, 30)?;
        assert!(manager.is_quarantined(plugin_id));

        manager.release_from_quarantine(plugin_id)?;
        assert!(!manager.is_quarantined(plugin_id));
        Ok(())
    }

    #[test]
    fn test_release_unknown_plugin_fails() {
        let policy = QuarantinePolicy::default();
        let mut manager = QuarantineManager::new(policy);
        let plugin_id = Uuid::new_v4();

        let result = manager.release_from_quarantine(plugin_id);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_quarantine_stats_empty() {
        let policy = QuarantinePolicy::default();
        let manager = QuarantineManager::new(policy);
        let stats = manager.get_quarantine_stats();
        assert!(stats.is_empty());
    }

    #[test]
    fn test_quarantine_state_tracks_totals() -> Result<(), PluginError> {
        let policy = QuarantinePolicy::default();
        let mut manager = QuarantineManager::new(policy);
        let plugin_id = Uuid::new_v4();

        manager.record_violation(plugin_id, ViolationType::Crash, "crash 1".to_string())?;
        manager.record_violation(
            plugin_id,
            ViolationType::BudgetViolation,
            "budget 1".to_string(),
        )?;

        let state = manager.get_quarantine_state(plugin_id);
        assert!(state.is_some());
        let state = state.unwrap_or_else(|| unreachable!());
        assert_eq!(state.total_crashes, 1);
        assert_eq!(state.total_budget_violations, 1);
        Ok(())
    }

    #[test]
    fn test_capability_violation_does_not_increment_crash_counter() -> Result<(), PluginError> {
        let policy = QuarantinePolicy::default();
        let mut manager = QuarantineManager::new(policy);
        let plugin_id = Uuid::new_v4();

        manager.record_violation(
            plugin_id,
            ViolationType::CapabilityViolation,
            "cap violation".to_string(),
        )?;

        let state = manager.get_quarantine_state(plugin_id);
        assert!(state.is_some());
        let state = state.unwrap_or_else(|| unreachable!());
        assert_eq!(state.total_crashes, 0);
        assert_eq!(state.total_budget_violations, 0);
        Ok(())
    }

    #[test]
    fn test_failure_tracker_new_is_empty() {
        let tracker = FailureTracker::new();
        let plugin_id = Uuid::new_v4();
        assert!(tracker.get_stats(plugin_id).is_none());
    }

    #[test]
    fn test_failure_tracker_records_execution() {
        let mut tracker = FailureTracker::new();
        let plugin_id = Uuid::new_v4();

        tracker.record_execution(plugin_id, 100, true);
        tracker.record_execution(plugin_id, 200, true);
        tracker.record_execution(plugin_id, 300, false);

        let stats = tracker.get_stats(plugin_id);
        assert!(stats.is_some());
        let stats = stats.unwrap_or_else(|| unreachable!());
        assert_eq!(stats.executions, 3);
        assert_eq!(stats.total_time_us, 600);
        assert_eq!(stats.max_time_us, 300);
        assert_eq!(stats.crashes, 1);
        assert!((stats.avg_time_us - 200.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_failure_tracker_max_time_tracks_peak() {
        let mut tracker = FailureTracker::new();
        let plugin_id = Uuid::new_v4();

        tracker.record_execution(plugin_id, 500, true);
        tracker.record_execution(plugin_id, 100, true);
        tracker.record_execution(plugin_id, 300, true);

        let stats = tracker.get_stats(plugin_id);
        assert!(stats.is_some());
        assert_eq!(stats.unwrap_or_else(|| unreachable!()).max_time_us, 500);
    }

    #[test]
    fn test_violation_type_equality() {
        assert_eq!(ViolationType::Crash, ViolationType::Crash);
        assert_ne!(ViolationType::Crash, ViolationType::BudgetViolation);
        assert_ne!(
            ViolationType::CapabilityViolation,
            ViolationType::TimeoutViolation
        );
    }

    #[test]
    fn test_quarantine_policy_serialization() -> Result<(), serde_json::Error> {
        let policy = QuarantinePolicy::default();
        let json = serde_json::to_string(&policy)?;
        let deserialized: QuarantinePolicy = serde_json::from_str(&json)?;
        assert_eq!(deserialized.max_crashes, policy.max_crashes);
        assert_eq!(
            deserialized.max_budget_violations,
            policy.max_budget_violations
        );
        Ok(())
    }
}

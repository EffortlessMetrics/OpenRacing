//! Plugin quarantine system for repeatedly failing plugins

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::{PluginError, PluginResult, PluginStats};

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
            violation_window_minutes: 60, // 1 hour
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
}/// Plu
gin quarantine manager
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
        let state = self.quarantine_states.entry(plugin_id).or_insert_with(|| {
            QuarantineState {
                plugin_id,
                is_quarantined: false,
                quarantine_start: None,
                quarantine_end: None,
                escalation_level: 0,
                total_crashes: 0,
                total_budget_violations: 0,
                recent_violations: Vec::new(),
            }
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
        
        // Clean up old violations outside the window
        self.cleanup_old_violations(state);
        
        // Check if quarantine is needed
        if self.should_quarantine(state) {
            self.quarantine_plugin(state)?;
        }
        
        Ok(())
    }
    
    /// Check if a plugin is currently quarantined
    pub fn is_quarantined(&mut self, plugin_id: Uuid) -> bool {
        if let Some(state) = self.quarantine_states.get_mut(&plugin_id) {
            // Check if quarantine period has expired
            if let Some(end_time) = state.quarantine_end {
                if Utc::now() > end_time {
                    state.is_quarantined = false;
                    state.quarantine_start = None;
                    state.quarantine_end = None;
                    return false;
                }
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
}
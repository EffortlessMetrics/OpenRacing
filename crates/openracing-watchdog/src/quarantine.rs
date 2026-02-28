//! Plugin quarantine management.
//!
//! This module provides structures for managing plugin quarantine state,
//! including tracking quarantined plugins and their release conditions.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::WatchdogError;
use crate::stats::PluginStats;

/// Manages plugin quarantine state.
///
/// Provides methods for quarantining, releasing, and querying plugin quarantine status.
#[derive(Debug, Default)]
pub struct QuarantineManager {
    /// Plugins currently in quarantine.
    quarantined: HashMap<String, QuarantineEntry>,
    /// Default quarantine duration.
    default_duration: Duration,
}

/// Entry representing a quarantined plugin.
#[derive(Debug, Clone)]
pub struct QuarantineEntry {
    /// Plugin identifier.
    pub plugin_id: String,
    /// When the quarantine was applied.
    pub quarantined_at: Instant,
    /// When the quarantine expires.
    pub expires_at: Instant,
    /// Duration of the quarantine.
    pub duration: Duration,
    /// Reason for quarantine.
    pub reason: QuarantineReason,
    /// Number of times this plugin has been quarantined.
    pub quarantine_count: u32,
}

/// Reason for plugin quarantine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuarantineReason {
    /// Too many consecutive timeouts.
    ConsecutiveTimeouts,
    /// Exceeded timing budget repeatedly.
    TimingViolation,
    /// Caused a system crash or panic.
    Crash,
    /// Manual quarantine by operator.
    Manual,
    /// Unknown reason.
    Unknown,
}

impl std::fmt::Display for QuarantineReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QuarantineReason::ConsecutiveTimeouts => write!(f, "Consecutive timeouts"),
            QuarantineReason::TimingViolation => write!(f, "Timing violation"),
            QuarantineReason::Crash => write!(f, "Crash"),
            QuarantineReason::Manual => write!(f, "Manual"),
            QuarantineReason::Unknown => write!(f, "Unknown"),
        }
    }
}

impl QuarantineManager {
    /// Create a new quarantine manager with default settings.
    #[must_use]
    pub fn new() -> Self {
        Self {
            quarantined: HashMap::new(),
            default_duration: Duration::from_mins(5), // 5 minutes
        }
    }

    /// Create a quarantine manager with a custom default duration.
    #[must_use]
    pub fn with_default_duration(duration: Duration) -> Self {
        Self {
            quarantined: HashMap::new(),
            default_duration: duration,
        }
    }

    /// Quarantine a plugin.
    ///
    /// If the plugin is already quarantined, updates the expiration time.
    pub fn quarantine(
        &mut self,
        plugin_id: &str,
        duration: Option<Duration>,
        reason: QuarantineReason,
        stats: &mut PluginStats,
    ) {
        let duration = duration.unwrap_or(self.default_duration);
        let now = Instant::now();

        let entry = QuarantineEntry {
            plugin_id: plugin_id.to_string(),
            quarantined_at: now,
            expires_at: now + duration,
            duration,
            reason,
            quarantine_count: stats.quarantine_count.saturating_add(1),
        };

        stats.apply_quarantine(duration);
        self.quarantined.insert(plugin_id.to_string(), entry);
    }

    /// Release a plugin from quarantine.
    ///
    /// # Errors
    ///
    /// Returns `WatchdogError::NotQuarantined` if the plugin is not quarantined.
    pub fn release(
        &mut self,
        plugin_id: &str,
        stats: &mut PluginStats,
    ) -> crate::WatchdogResult<()> {
        if self.quarantined.remove(plugin_id).is_some() {
            stats.clear_quarantine();
            Ok(())
        } else {
            Err(WatchdogError::not_quarantined(plugin_id))
        }
    }

    /// Check if a plugin is currently quarantined.
    #[must_use]
    pub fn is_quarantined(&self, plugin_id: &str) -> bool {
        self.quarantined
            .get(plugin_id)
            .is_some_and(|entry| Instant::now() < entry.expires_at)
    }

    /// Get quarantine entry for a plugin.
    #[must_use]
    pub fn get_entry(&self, plugin_id: &str) -> Option<&QuarantineEntry> {
        self.quarantined.get(plugin_id)
    }

    /// Get all currently quarantined plugins.
    ///
    /// Returns a list of `(plugin_id, remaining_duration)` tuples.
    #[must_use]
    pub fn get_quarantined(&self) -> Vec<(String, Duration)> {
        let now = Instant::now();
        self.quarantined
            .iter()
            .filter_map(|(id, entry)| {
                if now < entry.expires_at {
                    Some((id.clone(), entry.expires_at - now))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get count of quarantined plugins.
    #[must_use]
    pub fn quarantined_count(&self) -> usize {
        self.quarantined.len()
    }

    /// Clean up expired quarantines.
    ///
    /// Returns the number of quarantines that were cleaned up.
    pub fn cleanup_expired(&mut self) -> usize {
        let now = Instant::now();
        let expired: Vec<_> = self
            .quarantined
            .iter()
            .filter(|(_, entry)| now >= entry.expires_at)
            .map(|(id, _)| id.clone())
            .collect();

        let count = expired.len();
        for id in expired {
            self.quarantined.remove(&id);
        }
        count
    }

    /// Clean up expired quarantines and update stats.
    ///
    /// Returns the plugin IDs that were cleaned up.
    pub fn cleanup_expired_with_stats(
        &mut self,
        stats_map: &mut HashMap<String, PluginStats>,
    ) -> Vec<String> {
        let now = Instant::now();
        let expired: Vec<_> = self
            .quarantined
            .iter()
            .filter(|(_, entry)| now >= entry.expires_at)
            .map(|(id, _)| id.clone())
            .collect();

        for id in &expired {
            self.quarantined.remove(id);
            if let Some(stats) = stats_map.get_mut(id) {
                stats.quarantined_until = None;
            }
        }

        expired
    }

    /// Clear all quarantines.
    pub fn clear_all(&mut self) {
        self.quarantined.clear();
    }

    /// Set the default quarantine duration.
    pub fn set_default_duration(&mut self, duration: Duration) {
        self.default_duration = duration;
    }

    /// Get the default quarantine duration.
    #[must_use]
    pub fn default_duration(&self) -> Duration {
        self.default_duration
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_quarantine_and_release() {
        let mut manager = QuarantineManager::new();
        let mut stats = PluginStats::new();

        assert!(!manager.is_quarantined("plugin_a"));

        manager.quarantine(
            "plugin_a",
            Some(Duration::from_millis(100)),
            QuarantineReason::ConsecutiveTimeouts,
            &mut stats,
        );

        assert!(manager.is_quarantined("plugin_a"));
        assert!(stats.is_quarantined());
        assert_eq!(manager.quarantined_count(), 1);

        let result = manager.release("plugin_a", &mut stats);
        assert!(result.is_ok());
        assert!(!manager.is_quarantined("plugin_a"));
        assert!(!stats.is_quarantined());
    }

    #[test]
    fn test_release_not_quarantined() {
        let mut manager = QuarantineManager::new();
        let mut stats = PluginStats::new();

        let result = manager.release("unknown_plugin", &mut stats);
        assert!(result.is_err());
    }

    #[test]
    fn test_quarantine_expiry() {
        let mut manager = QuarantineManager::new();
        let mut stats = PluginStats::new();

        manager.quarantine(
            "plugin_a",
            Some(Duration::from_millis(500)),
            QuarantineReason::TimingViolation,
            &mut stats,
        );

        assert!(manager.is_quarantined("plugin_a"));

        thread::sleep(Duration::from_millis(600));

        let cleaned = manager.cleanup_expired();
        assert_eq!(cleaned, 1);
        assert!(!manager.is_quarantined("plugin_a"));
    }

    #[test]
    fn test_get_quarantined() {
        let mut manager = QuarantineManager::new();
        let mut stats1 = PluginStats::new();
        let mut stats2 = PluginStats::new();

        manager.quarantine(
            "plugin_a",
            Some(Duration::from_mins(1)),
            QuarantineReason::ConsecutiveTimeouts,
            &mut stats1,
        );
        manager.quarantine(
            "plugin_b",
            Some(Duration::from_mins(2)),
            QuarantineReason::Manual,
            &mut stats2,
        );

        let quarantined = manager.get_quarantined();
        assert_eq!(quarantined.len(), 2);

        let plugin_a = quarantined.iter().find(|(id, _)| id == "plugin_a");
        assert!(plugin_a.is_some());
    }

    #[test]
    fn test_quarantine_entry() {
        let mut manager = QuarantineManager::new();
        let mut stats = PluginStats::new();

        manager.quarantine(
            "plugin_a",
            Some(Duration::from_mins(5)),
            QuarantineReason::Crash,
            &mut stats,
        );

        let entry = manager.get_entry("plugin_a");
        assert!(entry.is_some());
        if let Some(entry) = entry {
            assert_eq!(entry.plugin_id, "plugin_a");
            assert_eq!(entry.reason, QuarantineReason::Crash);
            assert_eq!(entry.duration, Duration::from_mins(5));
        }
    }

    #[test]
    fn test_default_duration() {
        let mut manager = QuarantineManager::with_default_duration(Duration::from_mins(10));
        let mut stats = PluginStats::new();

        assert_eq!(manager.default_duration(), Duration::from_mins(10));

        manager.set_default_duration(Duration::from_mins(2));
        assert_eq!(manager.default_duration(), Duration::from_mins(2));

        // Use default duration
        manager.quarantine("plugin_a", None, QuarantineReason::Unknown, &mut stats);
        let entry = manager.get_entry("plugin_a");
        assert!(entry.is_some());
        if let Some(entry) = entry {
            assert_eq!(entry.duration, Duration::from_mins(2));
        }
    }
}

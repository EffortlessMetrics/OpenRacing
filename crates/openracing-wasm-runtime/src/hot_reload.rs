//! Hot-reload support for WASM plugins.
//!
//! This module provides hot-reload functionality that allows plugins to be
//! updated at runtime while preserving their state.

use std::collections::HashMap;

/// State preserved during hot-reload.
///
/// This struct captures the state that should be preserved when
/// reloading a plugin, including custom data and statistics.
#[derive(Debug, Clone)]
pub struct PreservedPluginState {
    /// Custom plugin data (key-value pairs)
    pub plugin_data: HashMap<String, Vec<u8>>,
    /// Number of successful process() calls
    pub process_count: u64,
    /// Total processing time in microseconds
    pub total_process_time_us: u64,
}

impl Default for PreservedPluginState {
    fn default() -> Self {
        Self::new()
    }
}

impl PreservedPluginState {
    /// Create a new empty preserved state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            plugin_data: HashMap::new(),
            process_count: 0,
            total_process_time_us: 0,
        }
    }

    /// Check if the state is empty (no data preserved).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.plugin_data.is_empty() && self.process_count == 0 && self.total_process_time_us == 0
    }

    /// Get the average processing time in microseconds.
    #[must_use]
    pub fn average_process_time_us(&self) -> f64 {
        if self.process_count == 0 {
            0.0
        } else {
            self.total_process_time_us as f64 / self.process_count as f64
        }
    }
}

/// Hot-reload manager for WASM plugins.
///
/// This struct provides utilities for managing hot-reload operations,
/// including state preservation and restoration.
#[derive(Debug, Default)]
pub struct HotReloader {
    /// Number of successful reloads
    reload_count: u64,
    /// Number of failed reloads
    failed_reload_count: u64,
}

impl HotReloader {
    /// Create a new hot-reload manager.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a successful reload.
    pub fn record_success(&mut self) {
        self.reload_count += 1;
    }

    /// Record a failed reload.
    pub fn record_failure(&mut self) {
        self.failed_reload_count += 1;
    }

    /// Get the number of successful reloads.
    #[must_use]
    pub fn reload_count(&self) -> u64 {
        self.reload_count
    }

    /// Get the number of failed reloads.
    #[must_use]
    pub fn failed_reload_count(&self) -> u64 {
        self.failed_reload_count
    }

    /// Get the total number of reload attempts.
    #[must_use]
    pub fn total_attempts(&self) -> u64 {
        self.reload_count + self.failed_reload_count
    }

    /// Get the success rate as a percentage.
    #[must_use]
    pub fn success_rate(&self) -> f64 {
        let total = self.total_attempts();
        if total == 0 {
            100.0
        } else {
            (self.reload_count as f64 / total as f64) * 100.0
        }
    }

    /// Reset statistics.
    pub fn reset_stats(&mut self) {
        self.reload_count = 0;
        self.failed_reload_count = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preserved_plugin_state_new() {
        let state = PreservedPluginState::new();
        assert!(state.is_empty());
        assert_eq!(state.process_count, 0);
        assert_eq!(state.total_process_time_us, 0);
    }

    #[test]
    fn test_preserved_plugin_state_with_data() {
        let mut data = HashMap::new();
        data.insert("key1".to_string(), vec![1, 2, 3]);
        data.insert("key2".to_string(), vec![4, 5, 6]);

        let state = PreservedPluginState {
            plugin_data: data.clone(),
            process_count: 100,
            total_process_time_us: 5000,
        };

        assert!(!state.is_empty());
        assert_eq!(state.process_count, 100);
        assert_eq!(state.total_process_time_us, 5000);
        assert_eq!(state.plugin_data.len(), 2);
        assert_eq!(state.average_process_time_us(), 50.0);
    }

    #[test]
    fn test_hot_reloader_stats() {
        let mut reloader = HotReloader::new();

        reloader.record_success();
        reloader.record_success();
        reloader.record_failure();
        reloader.record_success();

        assert_eq!(reloader.reload_count(), 3);
        assert_eq!(reloader.failed_reload_count(), 1);
        assert_eq!(reloader.total_attempts(), 4);
        assert!((reloader.success_rate() - 75.0).abs() < f64::EPSILON);

        reloader.reset_stats();
        assert_eq!(reloader.reload_count(), 0);
        assert_eq!(reloader.failed_reload_count(), 0);
    }
}

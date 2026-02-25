//! Plugin execution statistics tracking.
//!
//! This module provides structures for tracking plugin execution metrics
//! including timing, timeouts, and quarantine status.

use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// Plugin execution statistics.
///
/// Tracks execution metrics for a single plugin including:
/// - Total execution count and time
/// - Timeout tracking
/// - Quarantine status and history
///
/// # RT Safety
///
/// All methods in this struct are RT-safe and perform no heap allocations.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginStats {
    /// Total number of executions.
    pub total_executions: u64,
    /// Total execution time in microseconds.
    pub total_execution_time_us: u64,
    /// Number of timeouts detected.
    pub timeout_count: u32,
    /// Consecutive timeout count (resets on successful execution).
    pub consecutive_timeouts: u32,
    /// Last execution time in microseconds.
    pub last_execution_time_us: u64,
    /// Timestamp of last execution.
    #[serde(skip)]
    pub last_execution: Option<Instant>,
    /// Quarantine expiration time, if quarantined.
    #[serde(skip)]
    pub quarantined_until: Option<Instant>,
    /// Total number of quarantines for this plugin.
    pub quarantine_count: u32,
}

impl PluginStats {
    /// Create new empty plugin statistics.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Get average execution time in microseconds.
    ///
    /// Returns 0.0 if no executions have been recorded.
    #[must_use]
    pub fn average_execution_time_us(&self) -> f64 {
        if self.total_executions == 0 {
            0.0
        } else {
            self.total_execution_time_us as f64 / self.total_executions as f64
        }
    }

    /// Get timeout rate as percentage.
    ///
    /// Returns 0.0 if no executions have been recorded.
    #[must_use]
    pub fn timeout_rate(&self) -> f64 {
        if self.total_executions == 0 {
            0.0
        } else {
            (self.timeout_count as f64 / self.total_executions as f64) * 100.0
        }
    }

    /// Check if plugin is currently quarantined.
    ///
    /// # RT Safety
    ///
    /// This method is RT-safe and performs no allocations.
    #[must_use]
    pub fn is_quarantined(&self) -> bool {
        self.quarantined_until
            .map_or(false, |quarantine_until| Instant::now() < quarantine_until)
    }

    /// Get remaining quarantine time.
    ///
    /// Returns `None` if not quarantined or quarantine has expired.
    #[must_use]
    pub fn quarantine_remaining(&self) -> Option<Duration> {
        self.quarantined_until.and_then(|quarantine_until| {
            let now = Instant::now();
            if now < quarantine_until {
                Some(quarantine_until - now)
            } else {
                None
            }
        })
    }

    /// Record a successful execution.
    ///
    /// # RT Safety
    ///
    /// This method is RT-safe and performs no allocations.
    pub fn record_success(&mut self, execution_time_us: u64) {
        self.total_executions = self.total_executions.saturating_add(1);
        self.total_execution_time_us = self
            .total_execution_time_us
            .saturating_add(execution_time_us);
        self.last_execution_time_us = execution_time_us;
        self.last_execution = Some(Instant::now());
        self.consecutive_timeouts = 0;
    }

    /// Record a timeout.
    ///
    /// # RT Safety
    ///
    /// This method is RT-safe and performs no allocations.
    pub fn record_timeout(&mut self, execution_time_us: u64) {
        self.total_executions = self.total_executions.saturating_add(1);
        self.total_execution_time_us = self
            .total_execution_time_us
            .saturating_add(execution_time_us);
        self.last_execution_time_us = execution_time_us;
        self.last_execution = Some(Instant::now());
        self.timeout_count = self.timeout_count.saturating_add(1);
        self.consecutive_timeouts = self.consecutive_timeouts.saturating_add(1);
    }

    /// Apply quarantine for the specified duration.
    ///
    /// Increments the quarantine count and sets the quarantine expiration.
    pub fn apply_quarantine(&mut self, duration: Duration) {
        self.quarantined_until = Some(Instant::now() + duration);
        self.quarantine_count = self.quarantine_count.saturating_add(1);
    }

    /// Clear quarantine status.
    pub fn clear_quarantine(&mut self) {
        self.quarantined_until = None;
        self.consecutive_timeouts = 0;
    }

    /// Check if the quarantine has expired and clear it if so.
    ///
    /// Returns `true` if the quarantine was cleared.
    pub fn check_quarantine_expiry(&mut self) -> bool {
        if let Some(quarantine_until) = self.quarantined_until {
            if Instant::now() >= quarantine_until {
                self.quarantined_until = None;
                return true;
            }
        }
        false
    }

    /// Reset all statistics to default values.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

impl PartialEq for PluginStats {
    fn eq(&self, other: &Self) -> bool {
        // Compare all fields except Instant ones which don't implement PartialEq
        self.total_executions == other.total_executions
            && self.total_execution_time_us == other.total_execution_time_us
            && self.timeout_count == other.timeout_count
            && self.consecutive_timeouts == other.consecutive_timeouts
            && self.last_execution_time_us == other.last_execution_time_us
            && self.quarantine_count == other.quarantine_count
            && self.is_quarantined() == other.is_quarantined()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_record_success() {
        let mut stats = PluginStats::new();
        stats.record_success(50);
        stats.record_success(75);

        assert_eq!(stats.total_executions, 2);
        assert_eq!(stats.total_execution_time_us, 125);
        assert_eq!(stats.last_execution_time_us, 75);
        assert_eq!(stats.consecutive_timeouts, 0);
        assert_eq!(stats.timeout_count, 0);
    }

    #[test]
    fn test_record_timeout() {
        let mut stats = PluginStats::new();
        stats.record_success(50);
        stats.record_timeout(150);

        assert_eq!(stats.total_executions, 2);
        assert_eq!(stats.timeout_count, 1);
        assert_eq!(stats.consecutive_timeouts, 1);

        stats.record_success(40);
        assert_eq!(stats.consecutive_timeouts, 0);
    }

    #[test]
    fn test_average_and_timeout_rate() {
        let mut stats = PluginStats::new();
        assert!((stats.average_execution_time_us() - 0.0).abs() < f64::EPSILON);
        assert!((stats.timeout_rate() - 0.0).abs() < f64::EPSILON);

        stats.record_success(50);
        stats.record_success(100);
        stats.record_timeout(200);

        assert!((stats.average_execution_time_us() - 116.67).abs() < 0.1);
        assert!((stats.timeout_rate() - 33.33).abs() < 0.1);
    }

    #[test]
    fn test_quarantine() {
        let mut stats = PluginStats::new();
        assert!(!stats.is_quarantined());
        assert!(stats.quarantine_remaining().is_none());

        stats.apply_quarantine(Duration::from_millis(100));
        assert!(stats.is_quarantined());
        assert!(stats.quarantine_remaining().is_some());
        assert_eq!(stats.quarantine_count, 1);

        stats.clear_quarantine();
        assert!(!stats.is_quarantined());
    }

    #[test]
    fn test_quarantine_expiry() {
        let mut stats = PluginStats::new();
        stats.apply_quarantine(Duration::from_millis(10));

        thread::sleep(Duration::from_millis(20));

        assert!(stats.check_quarantine_expiry());
        assert!(!stats.is_quarantined());
    }

    #[test]
    fn test_reset() {
        let mut stats = PluginStats::new();
        stats.record_success(50);
        stats.record_timeout(150);
        stats.apply_quarantine(Duration::from_secs(10));

        stats.reset();

        assert_eq!(stats.total_executions, 0);
        assert_eq!(stats.timeout_count, 0);
        assert!(!stats.is_quarantined());
    }
}

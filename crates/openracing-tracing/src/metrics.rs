//! Tracing metrics for observability

use core::time::Duration;

/// Metrics collected for tracing observability
///
/// Provides insight into the tracing subsystem's performance and health.
/// All counters are monotonically increasing.
#[derive(Debug, Clone, Default)]
pub struct TracingMetrics {
    /// Total number of RT events emitted
    pub rt_events_emitted: u64,

    /// Total number of app events emitted
    pub app_events_emitted: u64,

    /// Number of events dropped due to buffer full
    pub events_dropped: u64,

    /// Number of RT deadline misses detected
    pub deadline_misses: u64,

    /// Number of pipeline faults detected
    pub pipeline_faults: u64,

    /// Total processing time for RT events (nanoseconds)
    pub total_rt_processing_ns: u64,

    /// Number of times the provider was reinitialized
    pub reinitializations: u64,
}

impl TracingMetrics {
    /// Create new metrics with zero values
    pub fn new() -> Self {
        Self::default()
    }

    /// Record an RT event emission
    #[inline]
    pub fn record_rt_event(&mut self) {
        self.rt_events_emitted = self.rt_events_emitted.saturating_add(1);
    }

    /// Record an app event emission
    #[inline]
    pub fn record_app_event(&mut self) {
        self.app_events_emitted = self.app_events_emitted.saturating_add(1);
    }

    /// Record a dropped event
    #[inline]
    pub fn record_dropped_event(&mut self) {
        self.events_dropped = self.events_dropped.saturating_add(1);
    }

    /// Record a deadline miss
    #[inline]
    pub fn record_deadline_miss(&mut self) {
        self.deadline_misses = self.deadline_misses.saturating_add(1);
    }

    /// Record a pipeline fault
    #[inline]
    pub fn record_pipeline_fault(&mut self) {
        self.pipeline_faults = self.pipeline_faults.saturating_add(1);
    }

    /// Record RT processing time
    #[inline]
    pub fn record_processing_time(&mut self, ns: u64) {
        self.total_rt_processing_ns = self.total_rt_processing_ns.saturating_add(ns);
    }

    /// Record a reinitialization
    #[inline]
    pub fn record_reinitialization(&mut self) {
        self.reinitializations = self.reinitializations.saturating_add(1);
    }

    /// Calculate average RT processing time
    pub fn average_rt_processing_time(&self) -> Duration {
        if self.rt_events_emitted == 0 {
            return Duration::ZERO;
        }
        Duration::from_nanos(self.total_rt_processing_ns / self.rt_events_emitted)
    }

    /// Calculate event drop rate
    pub fn drop_rate(&self) -> f64 {
        let total = self
            .rt_events_emitted
            .saturating_add(self.app_events_emitted);
        if total == 0 {
            return 0.0;
        }
        (self.events_dropped as f64) / (total as f64)
    }

    /// Check if health indicators are within acceptable bounds
    pub fn is_healthy(&self) -> bool {
        self.drop_rate() < 0.01 && self.pipeline_faults == 0
    }

    /// Reset all metrics to zero
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Merge metrics from another instance
    pub fn merge(&mut self, other: &TracingMetrics) {
        self.rt_events_emitted = self
            .rt_events_emitted
            .saturating_add(other.rt_events_emitted);
        self.app_events_emitted = self
            .app_events_emitted
            .saturating_add(other.app_events_emitted);
        self.events_dropped = self.events_dropped.saturating_add(other.events_dropped);
        self.deadline_misses = self.deadline_misses.saturating_add(other.deadline_misses);
        self.pipeline_faults = self.pipeline_faults.saturating_add(other.pipeline_faults);
        self.total_rt_processing_ns = self
            .total_rt_processing_ns
            .saturating_add(other.total_rt_processing_ns);
        self.reinitializations = self
            .reinitializations
            .saturating_add(other.reinitializations);
    }
}

impl core::fmt::Display for TracingMetrics {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "TracingMetrics(rt={}, app={}, dropped={}, misses={}, faults={}, drop_rate={:.4}%)",
            self.rt_events_emitted,
            self.app_events_emitted,
            self.events_dropped,
            self.deadline_misses,
            self.pipeline_faults,
            self.drop_rate() * 100.0
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_default() {
        let m = TracingMetrics::default();
        assert_eq!(m.rt_events_emitted, 0);
        assert_eq!(m.app_events_emitted, 0);
        assert!(m.is_healthy());
    }

    #[test]
    fn test_metrics_recording() {
        let mut m = TracingMetrics::new();

        m.record_rt_event();
        m.record_rt_event();
        m.record_app_event();
        m.record_deadline_miss();

        assert_eq!(m.rt_events_emitted, 2);
        assert_eq!(m.app_events_emitted, 1);
        assert_eq!(m.deadline_misses, 1);
    }

    #[test]
    fn test_metrics_drop_rate() {
        let mut m = TracingMetrics::new();

        assert_eq!(m.drop_rate(), 0.0);

        m.rt_events_emitted = 100;
        m.events_dropped = 1;

        assert!((m.drop_rate() - 0.01).abs() < 0.0001);
    }

    #[test]
    fn test_metrics_average_processing_time() {
        let mut m = TracingMetrics::new();

        assert_eq!(m.average_rt_processing_time(), Duration::ZERO);

        m.rt_events_emitted = 10;
        m.total_rt_processing_ns = 1000;

        assert_eq!(m.average_rt_processing_time(), Duration::from_nanos(100));
    }

    #[test]
    fn test_metrics_health() {
        let mut m = TracingMetrics::new();
        assert!(m.is_healthy());

        m.events_dropped = 99;
        m.rt_events_emitted = 10000;
        assert!(m.is_healthy());

        m.events_dropped = 200;
        assert!(!m.is_healthy());

        m.events_dropped = 0;
        m.pipeline_faults = 1;
        assert!(!m.is_healthy());
    }

    #[test]
    fn test_metrics_merge() {
        let mut m1 = TracingMetrics::new();
        m1.rt_events_emitted = 100;
        m1.deadline_misses = 5;

        let m2 = TracingMetrics {
            rt_events_emitted: 50,
            deadline_misses: 3,
            ..Default::default()
        };

        m1.merge(&m2);

        assert_eq!(m1.rt_events_emitted, 150);
        assert_eq!(m1.deadline_misses, 8);
    }

    #[test]
    fn test_metrics_saturating_add() {
        let mut m = TracingMetrics::new();
        m.rt_events_emitted = u64::MAX;

        m.record_rt_event();

        assert_eq!(m.rt_events_emitted, u64::MAX);
    }
}

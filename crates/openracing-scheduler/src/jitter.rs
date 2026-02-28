//! Jitter metrics collection and analysis.
//!
//! This module provides comprehensive jitter tracking for real-time systems,
//! including percentile calculations and deadline miss tracking.

use std::vec::Vec;

/// Jitter metrics collection and analysis.
///
/// Tracks timing statistics including:
/// - Total and missed ticks
/// - Maximum observed jitter
/// - Running variance calculation
/// - Percentile estimation (p99)
///
/// # RT-Safety
///
/// - `record_tick` is O(1) amortized
/// - Uses a bounded ring buffer for samples
/// - Reuses scratch storage for percentile calculation
/// - No allocations in the hot path after initialization
#[derive(Debug, Clone)]
pub struct JitterMetrics {
    /// Total number of ticks recorded
    pub total_ticks: u64,

    /// Number of missed deadlines
    pub missed_ticks: u64,

    /// Maximum observed jitter in nanoseconds
    pub max_jitter_ns: u64,

    /// Running sum of squared jitter for variance calculation
    jitter_sum_squared: f64,

    /// Last observed jitter sample
    pub last_jitter_ns: u64,

    /// Recent jitter samples for percentile calculation (ring buffer)
    recent_jitter_samples: Vec<u64>,

    /// Maximum samples to keep for percentile calculation
    max_samples: usize,

    /// Ring buffer write index
    next_sample_index: usize,

    /// Reused scratch storage for percentile selection
    percentile_scratch: Vec<u64>,
}

impl Default for JitterMetrics {
    fn default() -> Self {
        const DEFAULT_MAX_SAMPLES: usize = 10_000;
        Self {
            total_ticks: 0,
            missed_ticks: 0,
            max_jitter_ns: 0,
            jitter_sum_squared: 0.0,
            last_jitter_ns: 0,
            recent_jitter_samples: Vec::with_capacity(DEFAULT_MAX_SAMPLES),
            max_samples: DEFAULT_MAX_SAMPLES,
            next_sample_index: 0,
            percentile_scratch: Vec::with_capacity(DEFAULT_MAX_SAMPLES),
        }
    }
}

impl JitterMetrics {
    /// Create new jitter metrics collector with default capacity.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create jitter metrics with custom sample capacity.
    ///
    /// # Arguments
    ///
    /// * `max_samples` - Maximum number of samples to retain for percentile calculation
    pub fn with_capacity(max_samples: usize) -> Self {
        Self {
            total_ticks: 0,
            missed_ticks: 0,
            max_jitter_ns: 0,
            jitter_sum_squared: 0.0,
            last_jitter_ns: 0,
            recent_jitter_samples: Vec::with_capacity(max_samples),
            max_samples,
            next_sample_index: 0,
            percentile_scratch: Vec::with_capacity(max_samples),
        }
    }

    /// Record a tick with its jitter measurement.
    ///
    /// # Arguments
    ///
    /// * `jitter_ns` - The jitter observed for this tick in nanoseconds
    /// * `missed_deadline` - Whether this tick missed its deadline
    ///
    /// # RT-Safety
    ///
    /// This method is O(1) amortized. The ring buffer avoids per-tick allocations
    /// once the initial capacity is filled.
    pub fn record_tick(&mut self, jitter_ns: u64, missed_deadline: bool) {
        self.total_ticks += 1;

        if missed_deadline {
            self.missed_ticks += 1;
        }

        self.max_jitter_ns = self.max_jitter_ns.max(jitter_ns);
        self.jitter_sum_squared += (jitter_ns as f64).powi(2);
        self.last_jitter_ns = jitter_ns;

        if self.max_samples == 0 {
            return;
        }

        // Ring buffer management
        if self.recent_jitter_samples.len() < self.max_samples {
            self.recent_jitter_samples.push(jitter_ns);
            if self.recent_jitter_samples.len() == self.max_samples {
                self.next_sample_index = 0;
            }
        } else {
            self.recent_jitter_samples[self.next_sample_index] = jitter_ns;
            self.next_sample_index = (self.next_sample_index + 1) % self.max_samples;
        }
    }

    /// Calculate p99 jitter in nanoseconds.
    ///
    /// Uses quickselect algorithm for O(n) average-case performance.
    ///
    /// # Returns
    ///
    /// The 99th percentile jitter value, or 0 if no samples have been recorded.
    pub fn p99_jitter_ns(&mut self) -> u64 {
        self.percentile_jitter_ns(0.99)
    }

    /// Calculate p95 jitter in nanoseconds.
    pub fn p95_jitter_ns(&mut self) -> u64 {
        self.percentile_jitter_ns(0.95)
    }

    /// Calculate p50 (median) jitter in nanoseconds.
    pub fn p50_jitter_ns(&mut self) -> u64 {
        self.percentile_jitter_ns(0.50)
    }

    /// Calculate arbitrary percentile jitter in nanoseconds.
    ///
    /// # Arguments
    ///
    /// * `percentile` - Percentile to calculate (0.0 to 1.0)
    pub fn percentile_jitter_ns(&mut self, percentile: f64) -> u64 {
        if self.recent_jitter_samples.is_empty() {
            return 0;
        }

        let percentile = percentile.clamp(0.0, 1.0);

        // Ensure scratch buffer has capacity
        if self.percentile_scratch.capacity() < self.recent_jitter_samples.len() {
            self.percentile_scratch
                .reserve(self.recent_jitter_samples.len() - self.percentile_scratch.capacity());
        }

        self.percentile_scratch.clear();
        self.percentile_scratch
            .extend_from_slice(&self.recent_jitter_samples);

        let len = self.percentile_scratch.len();
        let index = ((len as f64 * percentile) as usize).min(len.saturating_sub(1));
        let (_, value, _) = self.percentile_scratch.select_nth_unstable(index);
        *value
    }

    /// Calculate the variance of jitter samples.
    ///
    /// This is an approximation using the running sum of squares.
    pub fn jitter_variance(&self) -> f64 {
        if self.total_ticks == 0 {
            return 0.0;
        }
        self.jitter_sum_squared / self.total_ticks as f64
    }

    /// Calculate standard deviation of jitter in nanoseconds.
    pub fn jitter_std_dev_ns(&self) -> f64 {
        self.jitter_variance().sqrt()
    }

    /// Calculate missed tick rate (0.0 to 1.0).
    pub fn missed_tick_rate(&self) -> f64 {
        if self.total_ticks == 0 {
            0.0
        } else {
            self.missed_ticks as f64 / self.total_ticks as f64
        }
    }

    /// Check if metrics meet performance requirements.
    ///
    /// Requirements:
    /// - p99 jitter ≤ 0.25ms (250_000 ns)
    /// - missed tick rate ≤ 0.001% (0.00001)
    pub fn meets_requirements(&mut self) -> bool {
        self.p99_jitter_ns() <= 250_000 && self.missed_tick_rate() <= 0.00001
    }

    /// Check if metrics meet custom requirements.
    ///
    /// # Arguments
    ///
    /// * `max_p99_jitter_ns` - Maximum allowed p99 jitter
    /// * `max_missed_rate` - Maximum allowed missed tick rate (0.0 to 1.0)
    pub fn meets_custom_requirements(
        &mut self,
        max_p99_jitter_ns: u64,
        max_missed_rate: f64,
    ) -> bool {
        self.p99_jitter_ns() <= max_p99_jitter_ns && self.missed_tick_rate() <= max_missed_rate
    }

    /// Reset all metrics.
    pub fn reset(&mut self) {
        self.total_ticks = 0;
        self.missed_ticks = 0;
        self.max_jitter_ns = 0;
        self.jitter_sum_squared = 0.0;
        self.last_jitter_ns = 0;
        self.recent_jitter_samples.clear();
        self.next_sample_index = 0;
        self.percentile_scratch.clear();
    }

    /// Get the number of samples currently stored.
    pub fn sample_count(&self) -> usize {
        self.recent_jitter_samples.len()
    }

    /// Get average jitter in nanoseconds.
    ///
    /// Note: This is an approximation. For exact average, use the raw samples.
    pub fn average_jitter_ns(&self) -> f64 {
        if self.total_ticks == 0 {
            return 0.0;
        }
        // We don't track sum directly, so approximate from RMS
        self.jitter_variance().sqrt()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jitter_metrics_creation() {
        let metrics = JitterMetrics::new();
        assert_eq!(metrics.total_ticks, 0);
        assert_eq!(metrics.missed_ticks, 0);
        assert_eq!(metrics.max_jitter_ns, 0);
    }

    #[test]
    fn test_record_tick() {
        let mut metrics = JitterMetrics::new();

        metrics.record_tick(100_000, false);
        metrics.record_tick(200_000, false);
        metrics.record_tick(300_000, true);

        assert_eq!(metrics.total_ticks, 3);
        assert_eq!(metrics.missed_ticks, 1);
        assert_eq!(metrics.max_jitter_ns, 300_000);
        assert_eq!(metrics.last_jitter_ns, 300_000);
    }

    #[test]
    fn test_missed_tick_rate() {
        let mut metrics = JitterMetrics::new();

        assert_eq!(metrics.missed_tick_rate(), 0.0);

        metrics.record_tick(100_000, false);
        metrics.record_tick(100_000, true);
        metrics.record_tick(100_000, false);

        assert!((metrics.missed_tick_rate() - 1.0 / 3.0).abs() < 1e-10);
    }

    #[test]
    fn test_p99_calculation() {
        let mut metrics = JitterMetrics::new();

        // Add 100 samples with known distribution
        for i in 0..100u64 {
            metrics.record_tick(i * 1000, false);
        }

        let p99 = metrics.p99_jitter_ns();
        // p99 of 0-99 should be around 98-99
        assert!(p99 >= 97_000, "p99 was {}", p99);
        assert!(p99 <= 99_000, "p99 was {}", p99);
    }

    #[test]
    fn test_ring_buffer_behavior() {
        let mut metrics = JitterMetrics::with_capacity(3);

        for i in 1..=5u64 {
            metrics.record_tick(i * 1_000, false);
        }

        assert_eq!(metrics.sample_count(), 3);
        assert_eq!(metrics.last_jitter_ns, 5_000);

        // Should contain last 3 values
        let mut sorted = metrics.recent_jitter_samples.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, vec![3_000, 4_000, 5_000]);
    }

    #[test]
    fn test_meets_requirements() {
        let mut metrics = JitterMetrics::new();

        // Good metrics
        for _ in 0..1000 {
            metrics.record_tick(100_000, false);
        }
        assert!(metrics.meets_requirements());

        // High jitter
        metrics.reset();
        for _ in 0..1000 {
            metrics.record_tick(300_000, false);
        }
        assert!(!metrics.meets_requirements());

        // High missed rate
        metrics.reset();
        for _ in 0..1000 {
            metrics.record_tick(100_000, true);
        }
        assert!(!metrics.meets_requirements());
    }

    #[test]
    fn test_percentile_bounds() {
        let mut metrics = JitterMetrics::with_capacity(100);

        // All same value
        for _ in 0..100 {
            metrics.record_tick(50_000, false);
        }

        assert_eq!(metrics.p50_jitter_ns(), 50_000);
        assert_eq!(metrics.p95_jitter_ns(), 50_000);
        assert_eq!(metrics.p99_jitter_ns(), 50_000);
    }

    #[test]
    fn test_custom_capacity() {
        let metrics = JitterMetrics::with_capacity(100);
        assert_eq!(metrics.max_samples, 100);
    }

    #[test]
    fn test_zero_capacity() {
        let mut metrics = JitterMetrics::with_capacity(0);
        metrics.record_tick(100_000, false);

        assert_eq!(metrics.total_ticks, 1);
        assert_eq!(metrics.sample_count(), 0);
    }

    #[test]
    fn test_reset() {
        let mut metrics = JitterMetrics::new();

        for i in 1..=10 {
            metrics.record_tick(i * 1000, i % 2 == 0);
        }

        metrics.reset();

        assert_eq!(metrics.total_ticks, 0);
        assert_eq!(metrics.missed_ticks, 0);
        assert_eq!(metrics.max_jitter_ns, 0);
        assert_eq!(metrics.sample_count(), 0);
    }

    #[test]
    fn test_variance_calculation() {
        let mut metrics = JitterMetrics::new();

        // Record samples with known variance
        metrics.record_tick(100, false);
        metrics.record_tick(200, false);
        metrics.record_tick(300, false);

        let variance = metrics.jitter_variance();
        assert!(variance > 0.0);
    }
}

//! Statistics structures for RT metrics.
//!
//! This module provides POD (Plain Old Data) structures for representing
//! statistics collected from the RT system. These are snapshot types that
//! capture the state at a point in time.

/// Jitter statistics in nanoseconds.
///
/// Represents percentile-based jitter measurements from the RT loop.
/// All values are in nanoseconds.
///
/// # RT Safety
///
/// This is a POD type with no heap allocations. Creating and copying
/// instances is RT-safe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct JitterStats {
    /// Median (p50) jitter in nanoseconds.
    pub p50_ns: u64,
    /// 99th percentile jitter in nanoseconds.
    pub p99_ns: u64,
    /// Maximum observed jitter in nanoseconds.
    pub max_ns: u64,
}

impl JitterStats {
    /// Create a new `JitterStats` with all values set to zero.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            p50_ns: 0,
            p99_ns: 0,
            max_ns: 0,
        }
    }

    /// Create a new `JitterStats` with the given values.
    #[must_use]
    pub const fn from_values(p50_ns: u64, p99_ns: u64, max_ns: u64) -> Self {
        Self {
            p50_ns,
            p99_ns,
            max_ns,
        }
    }

    /// Check if jitter exceeds a threshold (in nanoseconds).
    #[must_use]
    pub const fn exceeds_threshold(&self, threshold_ns: u64) -> bool {
        self.p99_ns > threshold_ns
    }

    /// Convert to microseconds.
    #[must_use]
    pub const fn to_micros(&self) -> Self {
        Self {
            p50_ns: self.p50_ns / 1000,
            p99_ns: self.p99_ns / 1000,
            max_ns: self.max_ns / 1000,
        }
    }
}

/// Latency statistics in microseconds.
///
/// Represents percentile-based latency measurements.
/// All values are in microseconds.
///
/// # RT Safety
///
/// This is a POD type with no heap allocations. Creating and copying
/// instances is RT-safe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LatencyStats {
    /// Median (p50) latency in microseconds.
    pub p50_us: u64,
    /// 99th percentile latency in microseconds.
    pub p99_us: u64,
    /// Maximum observed latency in microseconds.
    pub max_us: u64,
}

impl LatencyStats {
    /// Create a new `LatencyStats` with all values set to zero.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            p50_us: 0,
            p99_us: 0,
            max_us: 0,
        }
    }

    /// Create a new `LatencyStats` with the given values.
    #[must_use]
    pub const fn from_values(p50_us: u64, p99_us: u64, max_us: u64) -> Self {
        Self {
            p50_us,
            p99_us,
            max_us,
        }
    }

    /// Check if latency exceeds a threshold (in microseconds).
    #[must_use]
    pub const fn exceeds_threshold(&self, threshold_us: u64) -> bool {
        self.p99_us > threshold_us
    }

    /// Convert from nanoseconds to microseconds.
    #[must_use]
    pub const fn from_nanos(p50_ns: u64, p99_ns: u64, max_ns: u64) -> Self {
        Self {
            p50_us: p50_ns / 1000,
            p99_us: p99_ns / 1000,
            max_us: max_ns / 1000,
        }
    }
}

/// Snapshot of RT metrics.
///
/// Captures the state of RT performance metrics at a point in time.
/// This is a snapshot type suitable for transmission over queues or
/// storage in metrics systems.
///
/// # RT Safety
///
/// This is a POD type with no heap allocations. Creating and copying
/// instances is RT-safe.
#[derive(Debug, Clone, Copy)]
pub struct RTMetricsSnapshot {
    /// Total number of RT ticks processed.
    pub total_ticks: u64,
    /// Number of missed ticks (deadline violations).
    pub missed_ticks: u64,
    /// Jitter statistics.
    pub jitter: JitterStats,
    /// HID write latency statistics.
    pub hid_latency: LatencyStats,
    /// Processing time statistics.
    pub processing_time: LatencyStats,
    /// CPU usage percentage (0-100).
    pub cpu_usage_percent: f32,
    /// Memory usage in bytes.
    pub memory_usage_bytes: u64,
}

impl Default for RTMetricsSnapshot {
    fn default() -> Self {
        Self {
            total_ticks: 0,
            missed_ticks: 0,
            jitter: JitterStats::new(),
            hid_latency: LatencyStats::new(),
            processing_time: LatencyStats::new(),
            cpu_usage_percent: 0.0,
            memory_usage_bytes: 0,
        }
    }
}

impl RTMetricsSnapshot {
    /// Create a new `RTMetricsSnapshot` with all values set to default.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            total_ticks: 0,
            missed_ticks: 0,
            jitter: JitterStats::new(),
            hid_latency: LatencyStats::new(),
            processing_time: LatencyStats::new(),
            cpu_usage_percent: 0.0,
            memory_usage_bytes: 0,
        }
    }

    /// Calculate the missed tick rate as a percentage.
    ///
    /// Returns 0.0 if `total_ticks` is 0.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn missed_tick_rate(&self) -> f64 {
        if self.total_ticks == 0 {
            return 0.0;
        }
        (self.missed_ticks as f64 / self.total_ticks as f64) * 100.0
    }

    /// Check if any RT performance threshold is exceeded.
    #[must_use]
    pub const fn has_violations(&self, thresholds: &RTThresholds) -> bool {
        self.jitter.p99_ns > thresholds.max_jitter_ns
            || self.processing_time.p99_us > thresholds.max_processing_time_us
            || self.hid_latency.p99_us > thresholds.max_hid_latency_us
    }
}

/// RT performance thresholds for validation.
#[derive(Debug, Clone, Copy)]
pub struct RTThresholds {
    /// Maximum allowed jitter in nanoseconds (p99).
    pub max_jitter_ns: u64,
    /// Maximum allowed processing time in microseconds (p99).
    pub max_processing_time_us: u64,
    /// Maximum allowed HID latency in microseconds (p99).
    pub max_hid_latency_us: u64,
    /// Maximum allowed CPU usage percentage.
    pub max_cpu_usage_percent: f32,
    /// Maximum allowed memory usage in bytes.
    pub max_memory_usage_bytes: u64,
}

impl Default for RTThresholds {
    fn default() -> Self {
        Self {
            max_jitter_ns: 250_000,
            max_processing_time_us: 200,
            max_hid_latency_us: 300,
            max_cpu_usage_percent: 3.0,
            max_memory_usage_bytes: 150 * 1024 * 1024,
        }
    }
}

/// Snapshot of application-level metrics.
///
/// Captures the state of application metrics at a point in time.
///
/// # RT Safety
///
/// This is a POD type. The `active_game` field uses a fixed-size array
/// instead of a heap-allocated string for RT safety.
#[derive(Debug, Clone, Copy)]
pub struct AppMetricsSnapshot {
    /// Number of connected devices.
    pub connected_devices: u32,
    /// Torque saturation percentage (0-100).
    pub torque_saturation_percent: f32,
    /// Telemetry packet loss percentage (0-100).
    pub telemetry_packet_loss_percent: f32,
    /// Number of safety events triggered.
    pub safety_events: u64,
    /// Number of profile switches.
    pub profile_switches: u64,
}

impl Default for AppMetricsSnapshot {
    fn default() -> Self {
        Self {
            connected_devices: 0,
            torque_saturation_percent: 0.0,
            telemetry_packet_loss_percent: 0.0,
            safety_events: 0,
            profile_switches: 0,
        }
    }
}

impl AppMetricsSnapshot {
    /// Create a new `AppMetricsSnapshot` with all values set to default.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            connected_devices: 0,
            torque_saturation_percent: 0.0,
            telemetry_packet_loss_percent: 0.0,
            safety_events: 0,
            profile_switches: 0,
        }
    }

    /// Check if any application threshold is exceeded.
    #[must_use]
    pub const fn has_violations(&self, thresholds: &AppThresholds) -> bool {
        self.torque_saturation_percent > thresholds.max_torque_saturation_percent
            || self.telemetry_packet_loss_percent > thresholds.max_telemetry_loss_percent
    }
}

/// Application metrics thresholds for validation.
#[derive(Debug, Clone, Copy)]
pub struct AppThresholds {
    /// Maximum allowed torque saturation percentage.
    pub max_torque_saturation_percent: f32,
    /// Maximum allowed telemetry packet loss percentage.
    pub max_telemetry_loss_percent: f32,
}

impl Default for AppThresholds {
    fn default() -> Self {
        Self {
            max_torque_saturation_percent: 95.0,
            max_telemetry_loss_percent: 5.0,
        }
    }
}

/// Helper for computing statistics from samples.
///
/// This is a simple streaming statistics calculator that can be used
/// to compute percentiles without storing all samples.
///
/// Note: For accurate percentile calculations, use hdrhistogram in
/// the non-RT path. This is provided for simple use cases.
#[derive(Debug, Clone, Copy, Default)]
pub struct StreamingStats {
    count: u64,
    sum: u64,
    min: u64,
    max: u64,
}

impl StreamingStats {
    /// Create a new empty `StreamingStats`.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            count: 0,
            sum: 0,
            min: u64::MAX,
            max: 0,
        }
    }

    /// Record a sample.
    #[inline]
    pub fn record(&mut self, value: u64) {
        self.count = self.count.saturating_add(1);
        self.sum = self.sum.saturating_add(value);
        self.min = self.min.min(value);
        self.max = self.max.max(value);
    }

    /// Get the number of samples recorded.
    #[must_use]
    pub const fn count(&self) -> u64 {
        self.count
    }

    /// Get the minimum value recorded.
    ///
    /// Returns `u64::MAX` if no samples have been recorded.
    #[must_use]
    pub const fn min(&self) -> u64 {
        self.min
    }

    /// Get the maximum value recorded.
    #[must_use]
    pub const fn max(&self) -> u64 {
        self.max
    }

    /// Get the mean value.
    ///
    /// Returns 0 if no samples have been recorded.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn mean(&self) -> f64 {
        if self.count == 0 {
            return 0.0;
        }
        self.sum as f64 / self.count as f64
    }

    /// Reset all statistics.
    #[inline]
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Check if any samples have been recorded.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.count == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jitter_stats_threshold() {
        let stats = JitterStats::from_values(100_000, 300_000, 500_000);
        assert!(stats.exceeds_threshold(250_000));
        assert!(!stats.exceeds_threshold(400_000));
    }

    #[test]
    fn test_jitter_stats_to_micros() {
        let stats = JitterStats::from_values(100_000, 300_000, 500_000);
        let micros = stats.to_micros();
        assert_eq!(micros.p50_ns, 100);
        assert_eq!(micros.p99_ns, 300);
        assert_eq!(micros.max_ns, 500);
    }

    #[test]
    fn test_latency_stats_threshold() {
        let stats = LatencyStats::from_values(100, 250, 400);
        assert!(stats.exceeds_threshold(200));
        assert!(!stats.exceeds_threshold(300));
    }

    #[test]
    fn test_latency_stats_from_nanos() {
        let stats = LatencyStats::from_nanos(100_000, 250_000, 400_000);
        assert_eq!(stats.p50_us, 100);
        assert_eq!(stats.p99_us, 250);
        assert_eq!(stats.max_us, 400);
    }

    #[test]
    fn test_rt_metrics_missed_tick_rate() {
        let mut metrics = RTMetricsSnapshot::new();
        assert!((metrics.missed_tick_rate() - 0.0).abs() < f64::EPSILON);

        metrics.total_ticks = 1000;
        metrics.missed_ticks = 10;
        let rate = metrics.missed_tick_rate();
        assert!((rate - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_rt_metrics_has_violations() {
        let mut metrics = RTMetricsSnapshot::new();
        let thresholds = RTThresholds::default();

        assert!(!metrics.has_violations(&thresholds));

        metrics.jitter.p99_ns = 300_000;
        assert!(metrics.has_violations(&thresholds));
    }

    #[test]
    fn test_app_metrics_has_violations() {
        let mut metrics = AppMetricsSnapshot::new();
        let thresholds = AppThresholds::default();

        assert!(!metrics.has_violations(&thresholds));

        metrics.torque_saturation_percent = 96.0;
        assert!(metrics.has_violations(&thresholds));
    }

    #[test]
    fn test_streaming_stats() {
        let mut stats = StreamingStats::new();
        assert!(stats.is_empty());

        stats.record(10);
        stats.record(20);
        stats.record(30);

        assert_eq!(stats.count(), 3);
        assert_eq!(stats.min(), 10);
        assert_eq!(stats.max(), 30);
        assert!((stats.mean() - 20.0).abs() < f64::EPSILON);

        stats.reset();
        assert!(stats.is_empty());
    }
}

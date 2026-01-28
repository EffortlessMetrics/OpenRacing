//! Rate limiter to protect RT thread from telemetry parsing overhead
//!
//! Requirements: GI-04

use std::time::{Duration, Instant};

/// Rate limiter to protect the RT thread from telemetry parsing overhead
///
/// This ensures that telemetry processing doesn't overwhelm the system
/// and maintains the strict timing requirements of the force feedback loop.
pub struct RateLimiter {
    max_rate_hz: u32,
    min_interval: Duration,
    last_processed: Option<Instant>,
    dropped_count: u64,
    processed_count: u64,
}

impl RateLimiter {
    /// Create a new rate limiter with maximum rate in Hz
    pub fn new(max_rate_hz: u32) -> Self {
        let min_interval = Duration::from_nanos(1_000_000_000 / max_rate_hz as u64);

        Self {
            max_rate_hz,
            min_interval,
            last_processed: None,
            dropped_count: 0,
            processed_count: 0,
        }
    }

    /// Check if processing is allowed at this time
    /// Returns true if processing should proceed, false if rate limited
    pub fn should_process(&mut self) -> bool {
        let now = Instant::now();

        if let Some(last) = self.last_processed {
            let elapsed = now.duration_since(last);
            if elapsed < self.min_interval {
                self.dropped_count += 1;
                return false;
            }
        }

        self.last_processed = Some(now);
        self.processed_count += 1;
        true
    }

    /// Async version that waits until processing is allowed
    pub async fn wait_for_slot(&mut self) {
        let now = Instant::now();

        if let Some(last) = self.last_processed {
            let elapsed = now.duration_since(last);
            if elapsed < self.min_interval {
                let wait_time = self.min_interval - elapsed;
                tokio::time::sleep(wait_time).await;
            }
        }

        self.last_processed = Some(Instant::now());
        self.processed_count += 1;
    }

    /// Get the number of dropped frames due to rate limiting
    pub fn dropped_count(&self) -> u64 {
        self.dropped_count
    }

    /// Get the number of processed frames
    pub fn processed_count(&self) -> u64 {
        self.processed_count
    }

    /// Get the current drop rate as a percentage
    pub fn drop_rate_percent(&self) -> f32 {
        let total = self.dropped_count + self.processed_count;
        if total == 0 {
            0.0
        } else {
            (self.dropped_count as f32 / total as f32) * 100.0
        }
    }

    /// Reset statistics
    pub fn reset_stats(&mut self) {
        self.dropped_count = 0;
        self.processed_count = 0;
    }

    /// Get the configured maximum rate
    pub fn max_rate_hz(&self) -> u32 {
        self.max_rate_hz
    }

    /// Update the maximum rate
    pub fn set_max_rate_hz(&mut self, max_rate_hz: u32) {
        self.max_rate_hz = max_rate_hz;
        self.min_interval = Duration::from_nanos(1_000_000_000 / max_rate_hz as u64);
    }
}

/// Rate limiter statistics for monitoring
#[derive(Debug, Clone)]
pub struct RateLimiterStats {
    pub max_rate_hz: u32,
    pub processed_count: u64,
    pub dropped_count: u64,
    pub drop_rate_percent: f32,
}

impl From<&RateLimiter> for RateLimiterStats {
    fn from(limiter: &RateLimiter) -> Self {
        Self {
            max_rate_hz: limiter.max_rate_hz,
            processed_count: limiter.processed_count,
            dropped_count: limiter.dropped_count,
            drop_rate_percent: limiter.drop_rate_percent(),
        }
    }
}

/// Adaptive rate limiter that adjusts based on system load
pub struct AdaptiveRateLimiter {
    base_limiter: RateLimiter,
    initial_rate_hz: u32,
    target_cpu_percent: f32,
    current_cpu_percent: f32,
    adjustment_factor: f32,
}

impl AdaptiveRateLimiter {
    /// Create a new adaptive rate limiter
    pub fn new(initial_rate_hz: u32, target_cpu_percent: f32) -> Self {
        Self {
            base_limiter: RateLimiter::new(initial_rate_hz),
            initial_rate_hz,
            target_cpu_percent,
            current_cpu_percent: 0.0,
            adjustment_factor: 1.0,
        }
    }

    /// Update CPU usage and adjust rate accordingly
    pub fn update_cpu_usage(&mut self, cpu_percent: f32) {
        self.current_cpu_percent = cpu_percent;

        // Adjust rate based on CPU usage
        if cpu_percent > self.target_cpu_percent {
            // Reduce rate if CPU usage is too high
            self.adjustment_factor *= 0.95;
        } else if cpu_percent < self.target_cpu_percent * 0.8 {
            // Increase rate if CPU usage is low
            self.adjustment_factor *= 1.05;
        }

        // Clamp adjustment factor
        self.adjustment_factor = self.adjustment_factor.clamp(0.1, 2.0);

        // Apply adjustment to the original initial rate (not the current adjusted rate)
        let adjusted_rate = (self.initial_rate_hz as f32 * self.adjustment_factor) as u32;
        self.base_limiter.set_max_rate_hz(adjusted_rate.max(1));
    }

    /// Check if processing is allowed
    pub fn should_process(&mut self) -> bool {
        self.base_limiter.should_process()
    }

    /// Async wait for processing slot
    pub async fn wait_for_slot(&mut self) {
        self.base_limiter.wait_for_slot().await;
    }

    /// Get statistics
    pub fn stats(&self) -> RateLimiterStats {
        RateLimiterStats::from(&self.base_limiter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::Instant;

    #[test]
    fn test_rate_limiter_creation() {
        let limiter = RateLimiter::new(1000);
        assert_eq!(limiter.max_rate_hz(), 1000);
        assert_eq!(limiter.processed_count(), 0);
        assert_eq!(limiter.dropped_count(), 0);
    }

    #[test]
    fn test_rate_limiting() {
        let mut limiter = RateLimiter::new(10); // 10 Hz = 100ms interval

        // First call should be allowed
        assert!(limiter.should_process());
        assert_eq!(limiter.processed_count(), 1);

        // Immediate second call should be dropped
        assert!(!limiter.should_process());
        assert_eq!(limiter.dropped_count(), 1);
        assert_eq!(limiter.processed_count(), 1);
    }

    #[test]
    fn test_drop_rate_calculation() {
        let mut limiter = RateLimiter::new(10);

        // Process one, drop one
        assert!(limiter.should_process());
        assert!(!limiter.should_process());

        assert_eq!(limiter.drop_rate_percent(), 50.0);
    }

    #[test]
    fn test_stats_reset() {
        let mut limiter = RateLimiter::new(10);

        limiter.should_process();
        limiter.should_process(); // This will be dropped

        assert_eq!(limiter.processed_count(), 1);
        assert_eq!(limiter.dropped_count(), 1);

        limiter.reset_stats();

        assert_eq!(limiter.processed_count(), 0);
        assert_eq!(limiter.dropped_count(), 0);
    }

    #[tokio::test]
    async fn test_async_rate_limiting() {
        let mut limiter = RateLimiter::new(100); // 100 Hz = 10ms interval

        let start = Instant::now();

        // First call should be immediate
        limiter.wait_for_slot().await;
        let first_elapsed = start.elapsed();

        // Second call should wait
        limiter.wait_for_slot().await;
        let second_elapsed = start.elapsed();

        // Should have waited at least the minimum interval
        assert!(second_elapsed >= first_elapsed + Duration::from_millis(8)); // Allow some tolerance
        assert_eq!(limiter.processed_count(), 2);
    }

    #[test]
    fn test_adaptive_rate_limiter() {
        let mut adaptive = AdaptiveRateLimiter::new(1000, 50.0);

        // High CPU usage should reduce rate
        adaptive.update_cpu_usage(80.0);
        let stats_high = adaptive.stats();

        // Low CPU usage should increase rate
        adaptive.update_cpu_usage(20.0);
        let stats_low = adaptive.stats();

        // Rate should have been adjusted (though exact values depend on adjustment logic)
        assert!(stats_low.max_rate_hz >= stats_high.max_rate_hz);
    }

    #[test]
    fn test_rate_limiter_stats() {
        let mut limiter = RateLimiter::new(100);

        limiter.should_process();
        limiter.should_process(); // Dropped
        limiter.should_process(); // Dropped

        let stats = RateLimiterStats::from(&limiter);

        assert_eq!(stats.max_rate_hz, 100);
        assert_eq!(stats.processed_count, 1);
        assert_eq!(stats.dropped_count, 2);
        assert!((stats.drop_rate_percent - 66.67).abs() < 0.1);
    }
}

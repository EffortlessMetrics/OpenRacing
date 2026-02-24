//! Telemetry rate limiting utilities.
//!
//! Extracted from service telemetry runtime to keep rate control as a small,
//! reusable and independently versioned crate.

use std::time::{Duration, Instant};

/// Rate limiter to protect RT-adjacent paths from telemetry parsing bursts.
pub struct RateLimiter {
    max_rate_hz: u32,
    min_interval: Duration,
    last_processed: Option<Instant>,
    dropped_count: u64,
    processed_count: u64,
}

impl RateLimiter {
    /// Create a new rate limiter with maximum rate in Hz.
    pub fn new(max_rate_hz: u32) -> Self {
        let divisor = max_rate_hz.max(1) as u64;
        let min_interval = Duration::from_nanos(1_000_000_000 / divisor);

        Self {
            max_rate_hz,
            min_interval,
            last_processed: None,
            dropped_count: 0,
            processed_count: 0,
        }
    }

    /// Returns true if processing should proceed at this instant.
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

    /// Async variant that waits until a processing slot is available.
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

    /// Number of frames dropped for rate limiting.
    pub fn dropped_count(&self) -> u64 {
        self.dropped_count
    }

    /// Number of frames processed.
    pub fn processed_count(&self) -> u64 {
        self.processed_count
    }

    /// Current drop rate in percent.
    pub fn drop_rate_percent(&self) -> f32 {
        let total = self.dropped_count + self.processed_count;
        if total == 0 {
            0.0
        } else {
            (self.dropped_count as f32 / total as f32) * 100.0
        }
    }

    /// Reset collected statistics.
    pub fn reset_stats(&mut self) {
        self.dropped_count = 0;
        self.processed_count = 0;
    }

    /// Current max configured rate.
    pub fn max_rate_hz(&self) -> u32 {
        self.max_rate_hz
    }

    /// Update the max configured rate.
    pub fn set_max_rate_hz(&mut self, max_rate_hz: u32) {
        let effective = max_rate_hz.max(1);
        self.max_rate_hz = effective;
        self.min_interval = Duration::from_nanos(1_000_000_000 / effective as u64);
    }
}

/// Rate limiter statistics for monitoring.
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

/// Adaptive limiter that adjusts based on observed CPU usage.
pub struct AdaptiveRateLimiter {
    base_limiter: RateLimiter,
    initial_rate_hz: u32,
    target_cpu_percent: f32,
    current_cpu_percent: f32,
    adjustment_factor: f32,
}

impl AdaptiveRateLimiter {
    /// Create a new adaptive limiter.
    pub fn new(initial_rate_hz: u32, target_cpu_percent: f32) -> Self {
        Self {
            base_limiter: RateLimiter::new(initial_rate_hz),
            initial_rate_hz,
            target_cpu_percent,
            current_cpu_percent: 0.0,
            adjustment_factor: 1.0,
        }
    }

    /// Update the observed CPU usage and rebalance limiter behavior.
    pub fn update_cpu_usage(&mut self, cpu_percent: f32) {
        self.current_cpu_percent = cpu_percent;

        if cpu_percent > self.target_cpu_percent {
            self.adjustment_factor *= 0.95;
        } else if cpu_percent < self.target_cpu_percent * 0.8 {
            self.adjustment_factor *= 1.05;
        }

        self.adjustment_factor = self.adjustment_factor.clamp(0.1, 2.0);

        let adjusted_rate = (self.initial_rate_hz as f32 * self.adjustment_factor) as u32;
        self.base_limiter.set_max_rate_hz(adjusted_rate.max(1));
    }

    /// Returns true if processing should proceed.
    pub fn should_process(&mut self) -> bool {
        self.base_limiter.should_process()
    }

    /// Async variant that waits for the next processing slot.
    pub async fn wait_for_slot(&mut self) {
        self.base_limiter.wait_for_slot().await;
    }

    /// Snapshot limiter stats.
    pub fn stats(&self) -> RateLimiterStats {
        RateLimiterStats::from(&self.base_limiter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Duration;

    #[test]
    fn test_rate_limiter_creation() {
        let limiter = RateLimiter::new(1000);
        assert_eq!(limiter.max_rate_hz(), 1000);
        assert_eq!(limiter.processed_count(), 0);
        assert_eq!(limiter.dropped_count(), 0);
    }

    #[test]
    fn test_rate_limiting() {
        let mut limiter = RateLimiter::new(10);

        assert!(limiter.should_process());
        assert!(!limiter.should_process());
        assert_eq!(limiter.processed_count(), 1);
        assert_eq!(limiter.dropped_count(), 1);
    }

    #[test]
    fn test_drop_rate_calculation() {
        let mut limiter = RateLimiter::new(10);
        assert!(limiter.should_process());
        assert!(!limiter.should_process());
        assert!((limiter.drop_rate_percent() - 50.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_stats_reset() {
        let mut limiter = RateLimiter::new(10);
        assert!(limiter.should_process());
        assert!(!limiter.should_process());
        limiter.reset_stats();
        assert_eq!(limiter.processed_count(), 0);
        assert_eq!(limiter.dropped_count(), 0);
    }

    #[tokio::test]
    async fn test_async_rate_limiting() {
        let mut limiter = RateLimiter::new(100);

        limiter.wait_for_slot().await;
        let first = std::time::Instant::now();

        limiter.wait_for_slot().await;
        let second = std::time::Instant::now();

        assert!(second.duration_since(first) >= Duration::from_millis(8));
        assert_eq!(limiter.processed_count(), 2);
    }

    #[test]
    fn test_adaptive_rate_limiter() {
        let mut adaptive = AdaptiveRateLimiter::new(1000, 50.0);
        adaptive.update_cpu_usage(80.0);
        let high = adaptive.stats();
        adaptive.update_cpu_usage(20.0);
        let low = adaptive.stats();
        assert!(low.max_rate_hz >= high.max_rate_hz);
    }

    #[test]
    fn test_rate_limiter_stats() {
        let mut limiter = RateLimiter::new(100);
        assert!(limiter.should_process());
        assert!(!limiter.should_process());
        assert!(!limiter.should_process());

        let stats = RateLimiterStats::from(&limiter);
        assert_eq!(stats.max_rate_hz, 100);
        assert_eq!(stats.processed_count, 1);
        assert_eq!(stats.dropped_count, 2);
        assert!(stats.drop_rate_percent > 0.0);
    }

    #[test]
    fn test_rate_limiter_zero_rate() {
        let mut limiter = RateLimiter::new(0);
        assert_eq!(limiter.max_rate_hz(), 0);
        assert!(limiter.should_process());
    }

    #[test]
    fn test_rate_limiter_set_rate() {
        let mut limiter = RateLimiter::new(100);
        assert_eq!(limiter.max_rate_hz(), 100);

        limiter.set_max_rate_hz(200);
        assert_eq!(limiter.max_rate_hz(), 200);

        limiter.set_max_rate_hz(0);
        assert_eq!(limiter.max_rate_hz(), 1);
    }

    #[test]
    fn test_adaptive_limiter_adjustment_bounds() {
        let mut adaptive = AdaptiveRateLimiter::new(100, 50.0);

        for _ in 0..100 {
            adaptive.update_cpu_usage(100.0);
        }
        let stats = adaptive.stats();
        assert!(stats.max_rate_hz >= 10);

        for _ in 0..100 {
            adaptive.update_cpu_usage(0.0);
        }
        let stats = adaptive.stats();
        assert!(stats.max_rate_hz <= 200);
    }
}

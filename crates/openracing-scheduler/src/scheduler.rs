//! Absolute scheduler for precise timing with PLL and jitter metrics.
//!
//! This module provides the main scheduler implementation that combines
//! PLL drift correction, jitter tracking, and platform-specific high-precision sleep.

use crate::adaptive::{AdaptiveSchedulingConfig, AdaptiveSchedulingState};
use crate::error::{RTError, RTResult};
use crate::jitter::JitterMetrics;
use crate::pll::PLL;
use crate::rt_setup::RTSetup;
use std::boxed::Box;
use std::time::{Duration, Instant};

#[cfg(target_os = "windows")]
use crate::windows::PlatformSleep;

#[cfg(target_os = "linux")]
use crate::linux::PlatformSleep;

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
use crate::fallback::PlatformSleep;

/// Absolute scheduler for precise timing with PLL correction.
///
/// This scheduler maintains accurate timing by:
/// 1. Using absolute wake times to prevent drift
/// 2. Applying PLL correction based on actual tick intervals
/// 3. Tracking jitter metrics for monitoring
/// 4. Supporting adaptive period adjustment under load
///
/// # RT-Safety
///
/// - `wait_for_tick` is the main hot path and is O(1)
/// - Platform-specific sleep uses busy-spin for final precision
/// - All allocations occur during initialization only
///
/// # Example
///
/// ```no_run
/// use openracing_scheduler::{AbsoluteScheduler, RTSetup};
///
/// let mut scheduler = AbsoluteScheduler::new_1khz();
/// let setup = RTSetup::default();
/// scheduler.apply_rt_setup(&setup).expect("RT setup failed");
///
/// loop {
///     let tick = scheduler.wait_for_tick().expect("Timing violation");
///     // Process real-time work
/// }
/// ```
pub struct AbsoluteScheduler {
    /// Target period in nanoseconds
    period_ns: u64,

    /// Next scheduled tick time
    next_tick: Box<Instant>,

    /// Total tick count
    tick_count: u64,

    /// Phase-locked loop for drift correction
    pll: PLL,

    /// Jitter metrics collection
    metrics: JitterMetrics,

    /// Adaptive scheduling configuration
    adaptive_config: AdaptiveSchedulingConfig,

    /// Current adaptive target period
    adaptive_period_ns: u64,

    /// Last processing time reported (microseconds)
    last_processing_time_us: u64,

    /// EMA of processing time (microseconds)
    processing_time_ema_us: f64,

    /// RT setup applied flag
    rt_setup_applied: bool,

    /// Platform-specific sleep implementation
    platform_sleep: PlatformSleep,
}

impl AbsoluteScheduler {
    /// Create new scheduler with 1kHz (1ms) period.
    ///
    /// # RT-Safety
    ///
    /// This constructor performs heap allocations. Call during initialization only.
    pub fn new_1khz() -> Self {
        Self::with_period(crate::PERIOD_1KHZ_NS)
    }

    /// Create new scheduler with custom period.
    ///
    /// # Arguments
    ///
    /// * `period_ns` - Target period in nanoseconds
    ///
    /// # RT-Safety
    ///
    /// This constructor performs heap allocations. Call during initialization only.
    pub fn with_period(period_ns: u64) -> Self {
        let period_ns = period_ns.max(1);
        Self {
            period_ns,
            next_tick: Box::new(Instant::now()),
            tick_count: 0,
            pll: PLL::new(period_ns),
            metrics: JitterMetrics::new(),
            adaptive_config: AdaptiveSchedulingConfig::default(),
            adaptive_period_ns: period_ns,
            last_processing_time_us: 0,
            processing_time_ema_us: 0.0,
            rt_setup_applied: false,
            platform_sleep: PlatformSleep::new(),
        }
    }

    /// Apply real-time setup for optimal performance.
    ///
    /// This should be called once during initialization before the main loop.
    ///
    /// # Platform-Specific Behavior
    ///
    /// - **Windows**: Sets TIME_CRITICAL thread priority
    /// - **Linux**: Sets SCHED_FIFO priority and locks memory
    /// - **Other**: No-op
    pub fn apply_rt_setup(&mut self, setup: &RTSetup) -> RTResult {
        if self.rt_setup_applied {
            return Ok(());
        }

        self.platform_sleep.apply_rt_setup(setup)?;
        self.rt_setup_applied = true;
        Ok(())
    }

    /// Wait for next tick (RT-safe) with PLL correction and jitter measurement.
    ///
    /// This is the main hot path for the real-time loop. It:
    /// 1. Measures jitter from the expected wake time
    /// 2. Records timing metrics
    /// 3. Sleeps until the next scheduled tick
    /// 4. Applies PLL correction
    /// 5. Updates adaptive scheduling if enabled
    ///
    /// # Returns
    ///
    /// The current tick count on success.
    ///
    /// # Errors
    ///
    /// Returns `RTError::TimingViolation` if jitter exceeds the maximum threshold.
    ///
    /// # RT-Safety
    ///
    /// This method is O(1) and allocation-free in the steady state.
    pub fn wait_for_tick(&mut self) -> RTResult<u64> {
        let tick_start = Instant::now();

        // Calculate jitter
        let missed_deadline = tick_start >= *self.next_tick;
        let jitter_ns = if missed_deadline {
            tick_start.duration_since(*self.next_tick).as_nanos() as u64
        } else {
            self.next_tick.duration_since(tick_start).as_nanos() as u64
        };

        // Record metrics
        self.metrics.record_tick(jitter_ns, missed_deadline);

        // Sleep if not already past deadline
        if !missed_deadline {
            self.platform_sleep.sleep_until(*self.next_tick)?;
        }

        // Update adaptive scheduling
        let adaptive_target = self.update_adaptive_target(jitter_ns, missed_deadline);
        self.pll.set_target_period_ns(adaptive_target);

        // Update PLL with actual timing
        let actual_tick_time = Instant::now();
        let corrected_period = self.pll.update(
            actual_tick_time
                .duration_since(*self.next_tick - Duration::from_nanos(self.period_ns))
                .as_nanos() as u64,
        );

        // Schedule next tick
        self.tick_count += 1;
        *self.next_tick += corrected_period;

        // Check for severe timing violations
        #[cfg(test)]
        let max_jitter = crate::MAX_JITTER_TEST_NS;
        #[cfg(not(test))]
        let max_jitter = crate::MAX_JITTER_NS;

        if jitter_ns > max_jitter {
            return Err(RTError::TimingViolation);
        }

        Ok(self.tick_count)
    }

    /// Update adaptive target period based on current load signals.
    fn update_adaptive_target(&mut self, jitter_ns: u64, missed_deadline: bool) -> u64 {
        if !self.adaptive_config.enabled {
            self.adaptive_period_ns = self.period_ns;
            return self.period_ns;
        }

        let jitter_overloaded =
            missed_deadline || jitter_ns >= self.adaptive_config.jitter_relax_threshold_ns;
        let jitter_healthy =
            !missed_deadline && jitter_ns <= self.adaptive_config.jitter_tighten_threshold_ns;

        let has_signal = self.last_processing_time_us > 0;
        let processing_overloaded = has_signal
            && self.processing_time_ema_us
                >= self.adaptive_config.processing_relax_threshold_us as f64;
        let processing_healthy = has_signal
            && self.processing_time_ema_us
                <= self.adaptive_config.processing_tighten_threshold_us as f64;

        if jitter_overloaded || processing_overloaded {
            self.adaptive_period_ns = self
                .adaptive_period_ns
                .saturating_add(self.adaptive_config.increase_step_ns);
        } else if jitter_healthy && processing_healthy {
            self.adaptive_period_ns = self
                .adaptive_period_ns
                .saturating_sub(self.adaptive_config.decrease_step_ns);
        }

        self.adaptive_period_ns = self.adaptive_period_ns.clamp(
            self.adaptive_config.min_period_ns,
            self.adaptive_config.max_period_ns,
        );

        self.adaptive_period_ns
    }

    /// Configure adaptive scheduling.
    ///
    /// The configuration is normalized to maintain safe, bounded behavior.
    pub fn set_adaptive_scheduling(&mut self, mut config: AdaptiveSchedulingConfig) {
        config.normalize();

        self.adaptive_config = config;
        self.adaptive_period_ns = self.period_ns.clamp(
            self.adaptive_config.min_period_ns,
            self.adaptive_config.max_period_ns,
        );

        let pll_target = if self.adaptive_config.enabled {
            self.adaptive_period_ns
        } else {
            self.period_ns
        };
        self.pll.set_target_period_ns(pll_target);
    }

    /// Get adaptive scheduling runtime state.
    pub fn adaptive_scheduling(&self) -> AdaptiveSchedulingState {
        AdaptiveSchedulingState {
            enabled: self.adaptive_config.enabled,
            target_period_ns: self.adaptive_period_ns,
            min_period_ns: self.adaptive_config.min_period_ns,
            max_period_ns: self.adaptive_config.max_period_ns,
            last_processing_time_us: self.last_processing_time_us,
            processing_time_ema_us: self.processing_time_ema_us,
        }
    }

    /// Report per-tick processing time in microseconds.
    ///
    /// This signal is consumed by adaptive scheduling to estimate current load.
    pub fn record_processing_time_us(&mut self, processing_time_us: u64) {
        self.last_processing_time_us = processing_time_us;

        if self.processing_time_ema_us <= f64::EPSILON {
            self.processing_time_ema_us = processing_time_us as f64;
            return;
        }

        let alpha = self.adaptive_config.processing_ema_alpha;
        self.processing_time_ema_us =
            (1.0 - alpha) * self.processing_time_ema_us + alpha * processing_time_us as f64;
    }

    /// Get current tick count.
    #[inline]
    pub fn tick_count(&self) -> u64 {
        self.tick_count
    }

    /// Get jitter metrics.
    #[inline]
    pub fn metrics(&self) -> &JitterMetrics {
        &self.metrics
    }

    /// Get mutable jitter metrics for percentile calculations.
    #[inline]
    pub fn metrics_mut(&mut self) -> &mut JitterMetrics {
        &mut self.metrics
    }

    /// Get PLL phase error in nanoseconds.
    #[inline]
    pub fn phase_error_ns(&self) -> f64 {
        self.pll.phase_error_ns()
    }

    /// Get target period in nanoseconds.
    #[inline]
    pub fn period_ns(&self) -> u64 {
        self.period_ns
    }

    /// Reset scheduler state.
    pub fn reset(&mut self) {
        *self.next_tick = Instant::now();
        self.tick_count = 0;
        self.pll.reset();
        self.pll.set_target_period_ns(self.period_ns);
        self.metrics.reset();
        self.adaptive_period_ns = self.period_ns;
        self.last_processing_time_us = 0;
        self.processing_time_ema_us = 0.0;
    }

    /// Check if RT setup has been applied.
    #[inline]
    pub fn is_rt_setup_applied(&self) -> bool {
        self.rt_setup_applied
    }
}

impl Default for AbsoluteScheduler {
    fn default() -> Self {
        Self::new_1khz()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scheduler_creation() {
        let scheduler = AbsoluteScheduler::new_1khz();
        assert_eq!(scheduler.period_ns(), 1_000_000);
        assert_eq!(scheduler.tick_count(), 0);
        assert!(!scheduler.is_rt_setup_applied());
    }

    #[test]
    fn test_scheduler_with_custom_period() {
        let scheduler = AbsoluteScheduler::with_period(500_000);
        assert_eq!(scheduler.period_ns(), 500_000);
    }

    #[test]
    fn test_scheduler_zero_period_handled() {
        let scheduler = AbsoluteScheduler::with_period(0);
        assert_eq!(scheduler.period_ns(), 1);
    }

    #[test]
    fn test_adaptive_scheduling_defaults_disabled() {
        let scheduler = AbsoluteScheduler::new_1khz();
        let state = scheduler.adaptive_scheduling();

        assert!(!state.enabled);
        assert_eq!(state.target_period_ns, 1_000_000);
    }

    #[test]
    fn test_record_processing_time() {
        let mut scheduler = AbsoluteScheduler::new_1khz();
        scheduler.set_adaptive_scheduling(AdaptiveSchedulingConfig {
            processing_ema_alpha: 0.5,
            ..Default::default()
        });

        scheduler.record_processing_time_us(100);
        scheduler.record_processing_time_us(200);

        let state = scheduler.adaptive_scheduling();
        assert_eq!(state.last_processing_time_us, 200);
        assert!((state.processing_time_ema_us - 150.0).abs() < 1e-6);
    }

    #[test]
    fn test_reset() {
        let mut scheduler = AbsoluteScheduler::new_1khz();
        scheduler.tick_count = 100;
        scheduler.metrics.record_tick(100_000, false);

        scheduler.reset();

        assert_eq!(scheduler.tick_count(), 0);
        assert_eq!(scheduler.metrics().total_ticks, 0);
    }

    #[test]
    fn test_rt_setup_default() {
        let setup = RTSetup::default();
        assert!(setup.high_priority);
        assert!(setup.lock_memory);
    }

    #[test]
    fn test_default_impl() {
        let scheduler = AbsoluteScheduler::default();
        assert_eq!(scheduler.period_ns(), 1_000_000);
    }
}

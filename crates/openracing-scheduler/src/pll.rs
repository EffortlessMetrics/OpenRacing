//! Phase-Locked Loop for drift correction in real-time scheduling.
//!
//! The PLL maintains accurate timing by measuring actual tick intervals and
//! applying corrections to compensate for system clock drift. This ensures
//! that long-running real-time loops maintain their target frequency.

use std::time::Duration;

/// Phase-Locked Loop for drift correction.
///
/// The PLL tracks the difference between expected and actual tick times,
/// applying proportional-integral control to adjust the period and maintain
/// accurate timing over extended operation.
///
/// # RT-Safety
///
/// - All operations are O(1) and allocation-free
/// - No system calls in the hot path
/// - Bounded correction limits prevent runaway behavior
#[derive(Debug, Clone)]
pub struct PLL {
    /// Target period in nanoseconds
    target_period_ns: u64,

    /// Current estimated period in nanoseconds
    estimated_period_ns: f64,

    /// PLL gain factor (lower = more stable, higher = faster correction)
    gain: f64,

    /// Integral gain factor for accumulated phase error
    integral_gain: f64,

    /// Accumulated phase error in nanoseconds
    phase_error_ns: f64,

    /// Number of samples collected
    sample_count: u64,
}

impl PLL {
    /// Create new PLL with target period.
    ///
    /// # Arguments
    ///
    /// * `target_period_ns` - Target period in nanoseconds (must be > 0)
    ///
    /// # Panics
    ///
    /// Panics in debug mode if `target_period_ns` is 0.
    pub fn new(target_period_ns: u64) -> Self {
        debug_assert!(target_period_ns > 0, "target_period_ns must be > 0");

        Self {
            target_period_ns: target_period_ns.max(1),
            estimated_period_ns: target_period_ns.max(1) as f64,
            gain: 0.01,
            integral_gain: 0.1,
            phase_error_ns: 0.0,
            sample_count: 0,
        }
    }

    /// Create PLL with custom gains.
    ///
    /// # Arguments
    ///
    /// * `target_period_ns` - Target period in nanoseconds
    /// * `gain` - Proportional gain (0.0 to 1.0)
    /// * `integral_gain` - Integral gain (0.0 to 1.0)
    pub fn with_gains(target_period_ns: u64, gain: f64, integral_gain: f64) -> Self {
        let mut pll = Self::new(target_period_ns);
        pll.gain = gain.clamp(0.0, 1.0);
        pll.integral_gain = integral_gain.clamp(0.0, 1.0);
        pll
    }

    /// Calculate corrected period from actual interval.
    ///
    /// This method computes the phase error between expected and actual timing,
    /// then applies PI control to adjust the estimated period.
    ///
    /// # Arguments
    ///
    /// * `actual_interval_ns` - The actual time interval since the last tick
    ///
    /// # Returns
    ///
    /// The corrected period duration to use for scheduling the next tick.
    ///
    /// # RT-Safety
    ///
    /// This method is O(1) and allocation-free.
    pub fn update(&mut self, actual_interval_ns: u64) -> Duration {
        let actual_ns = actual_interval_ns as f64;

        // Calculate phase error (deviation from target)
        let period_error = actual_ns - self.target_period_ns as f64;

        // Accumulate phase error for integral term
        self.phase_error_ns += period_error;
        self.sample_count += 1;

        // Apply PI control
        // Correction = Kp * error + Ki * integral_error
        let correction =
            self.gain * period_error + self.integral_gain * self.gain * self.phase_error_ns;

        // Apply correction to estimated period
        self.estimated_period_ns = self.target_period_ns as f64 - correction;

        // Clamp to reasonable bounds (±10% of target)
        self.clamp_period();

        Duration::from_nanos(self.estimated_period_ns as u64)
    }

    /// Get current phase error in nanoseconds.
    ///
    /// Positive values indicate the system is running slow (behind schedule).
    /// Negative values indicate the system is running fast (ahead of schedule).
    #[inline]
    pub fn phase_error_ns(&self) -> f64 {
        self.phase_error_ns
    }

    /// Get average phase error over all samples.
    #[inline]
    pub fn average_phase_error_ns(&self) -> f64 {
        if self.sample_count == 0 {
            0.0
        } else {
            self.phase_error_ns / self.sample_count as f64
        }
    }

    /// Get current estimated period in nanoseconds.
    #[inline]
    pub fn estimated_period_ns(&self) -> u64 {
        self.estimated_period_ns as u64
    }

    /// Get target period in nanoseconds.
    #[inline]
    pub fn target_period_ns(&self) -> u64 {
        self.target_period_ns
    }

    /// Reset PLL state.
    ///
    /// Clears accumulated phase error and resets estimated period to target.
    pub fn reset(&mut self) {
        self.estimated_period_ns = self.target_period_ns as f64;
        self.phase_error_ns = 0.0;
        self.sample_count = 0;
    }

    /// Update the target period.
    ///
    /// This is used by adaptive scheduling to change the loop period based on
    /// observed system load while keeping PLL drift correction behavior.
    pub fn set_target_period_ns(&mut self, target_period_ns: u64) {
        self.target_period_ns = target_period_ns.max(1);
        self.clamp_period();
    }

    /// Clamp estimated period to ±10% of target.
    fn clamp_period(&mut self) {
        let min_period = self.target_period_ns as f64 * 0.9;
        let max_period = self.target_period_ns as f64 * 1.1;
        self.estimated_period_ns = self.estimated_period_ns.clamp(min_period, max_period);
    }

    /// Check if the PLL estimate is within acceptable bounds.
    #[inline]
    pub fn is_stable(&self) -> bool {
        let ratio = self.estimated_period_ns / self.target_period_ns as f64;
        (0.95..=1.05).contains(&ratio)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pll_creation() {
        let pll = PLL::new(1_000_000);
        assert_eq!(pll.target_period_ns(), 1_000_000);
        assert_eq!(pll.estimated_period_ns(), 1_000_000);
        assert_eq!(pll.phase_error_ns(), 0.0);
    }

    #[test]
    fn test_pll_update_within_bounds() {
        let mut pll = PLL::new(1_000_000);

        // Simulate slightly slow tick (1.05ms instead of 1ms)
        let corrected = pll.update(1_050_000);

        // Corrected period should be less than target to catch up
        assert!(corrected.as_nanos() <= 1_100_000);
        assert!(corrected.as_nanos() >= 900_000);
    }

    #[test]
    fn test_pll_clamp_to_bounds() {
        let mut pll = PLL::new(1_000_000);

        // Simulate very slow tick (should be clamped)
        let _ = pll.update(2_000_000);

        // Period should still be within ±10% bounds
        let period = pll.estimated_period_ns();
        assert!(period >= 900_000, "Period {} should be >= 900_000", period);
        assert!(
            period <= 1_100_000,
            "Period {} should be <= 1_100_000",
            period
        );
    }

    #[test]
    fn test_pll_reset() {
        let mut pll = PLL::new(1_000_000);
        let _ = pll.update(1_050_000);

        assert_ne!(pll.phase_error_ns(), 0.0);

        pll.reset();

        assert_eq!(pll.estimated_period_ns(), 1_000_000);
        assert_eq!(pll.phase_error_ns(), 0.0);
        assert_eq!(pll.sample_count, 0);
    }

    #[test]
    fn test_pll_set_target_period() {
        let mut pll = PLL::new(1_000_000);

        pll.set_target_period_ns(2_000_000);

        assert_eq!(pll.target_period_ns(), 2_000_000);
        // Estimated period should be clamped to new bounds
        assert!(pll.estimated_period_ns() >= 1_800_000);
        assert!(pll.estimated_period_ns() <= 2_200_000);
    }

    #[test]
    fn test_pll_stability_check() {
        let mut pll = PLL::new(1_000_000);

        // Fresh PLL should be stable
        assert!(pll.is_stable());

        // After small correction, should still be stable
        let _ = pll.update(1_010_000);
        assert!(pll.is_stable());
    }

    #[test]
    fn test_pll_custom_gains() {
        let pll = PLL::with_gains(1_000_000, 0.5, 0.2);

        assert_eq!(pll.gain, 0.5);
        assert_eq!(pll.integral_gain, 0.2);
    }

    #[test]
    #[cfg_attr(debug_assertions, ignore)]
    fn test_pll_zero_target_handled() {
        // Should not panic in release mode (debug_assertions disabled)
        let pll = PLL::new(0);
        assert_eq!(pll.target_period_ns(), 1);
    }

    #[test]
    fn test_average_phase_error() {
        let mut pll = PLL::new(1_000_000);

        // No samples yet
        assert_eq!(pll.average_phase_error_ns(), 0.0);

        // Add some samples
        let _ = pll.update(1_010_000); // +10us error
        let _ = pll.update(990_000); // -10us error

        // Total error: +10us + (-10us) = 0
        // Average should be 0
        let avg = pll.average_phase_error_ns();
        assert!(
            avg.abs() < 1.0,
            "Average error should be near 0, got {}",
            avg
        );
    }
}

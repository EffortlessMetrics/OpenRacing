//! Soft-stop controller for graceful torque ramping.
//!
//! The soft-stop mechanism provides safe torque reduction by ramping torque
//! to zero over a configurable duration, rather than an abrupt cutoff.

use core::time::Duration;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Soft-stop mechanism for torque ramping.
///
/// Provides linear torque ramp from current value to zero over a configurable
/// duration. This prevents sudden force changes that could cause injury or
/// mechanical stress.
///
/// # RT-Safety
///
/// All methods in this struct are RT-safe:
/// - No heap allocations
/// - No blocking operations
/// - Bounded execution time
/// - Deterministic behavior
///
/// # Example
///
/// ```rust
/// use openracing_fmea::SoftStopController;
/// use core::time::Duration;
///
/// let mut controller = SoftStopController::new();
///
/// // Start soft-stop from 10Nm with custom duration
/// controller.start_soft_stop_with_duration(10.0, Duration::from_millis(50));
///
/// // Update each tick - torque ramps down
/// while controller.is_active() {
///     let current_torque = controller.update(Duration::from_millis(1));
///     // Apply current_torque...
/// }
/// ```
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct SoftStopController {
    /// Whether soft-stop is currently active.
    active: bool,
    /// Elapsed time since soft-stop started.
    elapsed: Duration,
    /// Starting torque value.
    start_torque: f32,
    /// Target torque (typically 0.0).
    target_torque: f32,
    /// Duration of the ramp.
    ramp_duration: Duration,
    /// Current torque value.
    current_torque: f32,
    /// Default ramp duration.
    default_duration: Duration,
}

impl Default for SoftStopController {
    fn default() -> Self {
        Self::new()
    }
}

impl SoftStopController {
    /// Default soft-stop duration (50ms).
    pub const DEFAULT_RAMP_DURATION_MS: u64 = 50;

    /// Create a new soft-stop controller with default settings.
    pub fn new() -> Self {
        Self {
            active: false,
            elapsed: Duration::ZERO,
            start_torque: 0.0,
            target_torque: 0.0,
            ramp_duration: Duration::from_millis(Self::DEFAULT_RAMP_DURATION_MS),
            current_torque: 0.0,
            default_duration: Duration::from_millis(Self::DEFAULT_RAMP_DURATION_MS),
        }
    }

    /// Create a soft-stop controller with a custom default duration.
    pub fn with_duration(default_duration: Duration) -> Self {
        Self {
            active: false,
            elapsed: Duration::ZERO,
            start_torque: 0.0,
            target_torque: 0.0,
            ramp_duration: default_duration,
            current_torque: 0.0,
            default_duration,
        }
    }

    /// Start soft-stop from current torque to zero.
    ///
    /// Uses the default ramp duration.
    ///
    /// # Arguments
    ///
    /// * `current_torque` - The current torque value to ramp from.
    pub fn start_soft_stop(&mut self, current_torque: f32) {
        self.start_soft_stop_with_duration(current_torque, self.default_duration);
    }

    /// Start soft-stop with a specific ramp duration.
    ///
    /// # Arguments
    ///
    /// * `current_torque` - The current torque value to ramp from.
    /// * `duration` - The duration of the ramp.
    pub fn start_soft_stop_with_duration(&mut self, current_torque: f32, duration: Duration) {
        self.active = true;
        self.elapsed = Duration::ZERO;
        self.start_torque = current_torque;
        self.target_torque = 0.0;
        self.ramp_duration = duration;
        self.current_torque = current_torque;
    }

    /// Start soft-stop to a specific target torque.
    ///
    /// # Arguments
    ///
    /// * `current_torque` - The current torque value to ramp from.
    /// * `target_torque` - The target torque value to ramp to.
    /// * `duration` - The duration of the ramp.
    pub fn start_ramp_to(&mut self, current_torque: f32, target_torque: f32, duration: Duration) {
        self.active = true;
        self.elapsed = Duration::ZERO;
        self.start_torque = current_torque;
        self.target_torque = target_torque;
        self.ramp_duration = duration;
        self.current_torque = current_torque;
    }

    /// Update the soft-stop state with elapsed time and return current torque.
    ///
    /// This method should be called on each RT tick with the time delta since
    /// the last update.
    ///
    /// # Arguments
    ///
    /// * `delta` - Time elapsed since the last update.
    ///
    /// # Returns
    ///
    /// The current torque value after the update.
    pub fn update(&mut self, delta: Duration) -> f32 {
        if !self.active {
            return self.current_torque;
        }

        self.elapsed = self.elapsed.saturating_add(delta);

        if self.elapsed >= self.ramp_duration {
            self.active = false;
            self.current_torque = self.target_torque;
            return self.current_torque;
        }

        let progress = if self.ramp_duration.is_zero() {
            1.0
        } else {
            self.elapsed.as_secs_f32() / self.ramp_duration.as_secs_f32()
        };

        self.current_torque =
            self.start_torque + (self.target_torque - self.start_torque) * progress;

        self.current_torque
    }

    /// Get the current torque multiplier (0.0 to 1.0).
    ///
    /// Returns the ratio of current torque to start torque.
    /// Returns 0.0 if start torque was zero.
    pub fn current_multiplier(&self) -> f32 {
        if self.start_torque.abs() <= f32::EPSILON {
            return 0.0;
        }

        (self.current_torque / self.start_torque)
            .abs()
            .clamp(0.0, 1.0)
    }

    /// Check if soft-stop is currently active.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Get remaining time in the ramp.
    ///
    /// Returns `None` if soft-stop is not active.
    pub fn remaining_time(&self) -> Option<Duration> {
        if !self.active {
            return None;
        }

        let remaining = self.ramp_duration.saturating_sub(self.elapsed);
        if remaining.is_zero() {
            None
        } else {
            Some(remaining)
        }
    }

    /// Get the progress of the ramp (0.0 to 1.0).
    pub fn progress(&self) -> f32 {
        if !self.active {
            return if self.current_torque.abs() <= f32::EPSILON {
                1.0
            } else {
                0.0
            };
        }

        if self.ramp_duration.is_zero() {
            return 1.0;
        }

        self.elapsed.as_secs_f32() / self.ramp_duration.as_secs_f32()
    }

    /// Get the start torque value.
    pub fn start_torque(&self) -> f32 {
        self.start_torque
    }

    /// Get the target torque value.
    pub fn target_torque(&self) -> f32 {
        self.target_torque
    }

    /// Get the current torque value.
    pub fn current_torque(&self) -> f32 {
        self.current_torque
    }

    /// Get the ramp duration.
    pub fn ramp_duration(&self) -> Duration {
        self.ramp_duration
    }

    /// Force stop immediately (set torque to zero).
    ///
    /// This is used for emergency stops where immediate torque cutoff is required.
    pub fn force_stop(&mut self) {
        self.active = false;
        self.current_torque = 0.0;
    }

    /// Cancel the soft-stop and maintain current torque.
    pub fn cancel(&mut self) {
        self.active = false;
    }

    /// Reset the controller to initial state.
    pub fn reset(&mut self) {
        self.active = false;
        self.elapsed = Duration::ZERO;
        self.start_torque = 0.0;
        self.target_torque = 0.0;
        self.current_torque = 0.0;
    }

    /// Set the default ramp duration.
    pub fn set_default_duration(&mut self, duration: Duration) {
        self.default_duration = duration;
    }

    /// Get the default ramp duration.
    pub fn default_duration(&self) -> Duration {
        self.default_duration
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;

    #[test]
    fn test_soft_stop_creation() {
        let controller = SoftStopController::new();
        assert!(!controller.is_active());
        assert_eq!(controller.current_torque(), 0.0);
        assert_eq!(
            controller.default_duration(),
            Duration::from_millis(SoftStopController::DEFAULT_RAMP_DURATION_MS)
        );
    }

    #[test]
    fn test_soft_stop_with_duration() {
        let controller = SoftStopController::with_duration(Duration::from_millis(100));
        assert_eq!(controller.default_duration(), Duration::from_millis(100));
    }

    #[test]
    fn test_soft_stop_start() {
        let mut controller = SoftStopController::new();
        controller.start_soft_stop(10.0);

        assert!(controller.is_active());
        assert_eq!(controller.start_torque(), 10.0);
        assert_eq!(controller.target_torque(), 0.0);
        assert_eq!(controller.current_torque(), 10.0);
    }

    #[test]
    fn test_soft_stop_ramp() {
        let mut controller = SoftStopController::new();
        let duration = Duration::from_millis(100);
        controller.start_soft_stop_with_duration(10.0, duration);

        // At start, torque should be at starting value
        assert_eq!(controller.current_torque(), 10.0);
        assert_eq!(controller.progress(), 0.0);

        // After 25ms (25% progress), torque should be 7.5
        let torque = controller.update(Duration::from_millis(25));
        assert!(torque > 7.4 && torque < 7.6);
        assert!(controller.progress() > 0.24 && controller.progress() < 0.26);

        // After another 25ms (50% progress), torque should be 5.0
        let torque = controller.update(Duration::from_millis(25));
        assert!(torque > 4.9 && torque < 5.1);

        // After 100ms total, soft-stop should be complete
        let torque = controller.update(Duration::from_millis(50));
        assert!(!controller.is_active());
        assert_eq!(torque, 0.0);
        assert_eq!(controller.current_torque(), 0.0);
    }

    #[test]
    fn test_soft_stop_multiplier() {
        let mut controller = SoftStopController::new();
        controller.start_soft_stop(10.0);

        assert_eq!(controller.current_multiplier(), 1.0);

        controller.update(Duration::from_millis(25));
        let multiplier = controller.current_multiplier();
        // After 50% of the ramp, multiplier should be approximately 0.5
        assert!(multiplier > 0.49 && multiplier < 0.51);
    }

    #[test]
    fn test_soft_stop_zero_start() {
        let mut controller = SoftStopController::new();
        controller.start_soft_stop(0.0);

        // Zero start torque should result in zero multiplier
        assert_eq!(controller.current_multiplier(), 0.0);
    }

    #[test]
    fn test_soft_stop_force_stop() {
        let mut controller = SoftStopController::new();
        controller.start_soft_stop(10.0);
        assert!(controller.is_active());

        controller.force_stop();
        assert!(!controller.is_active());
        assert_eq!(controller.current_torque(), 0.0);
    }

    #[test]
    fn test_soft_stop_cancel() {
        let mut controller = SoftStopController::new();
        controller.start_soft_stop(10.0);
        controller.update(Duration::from_millis(25));
        let current = controller.current_torque();

        controller.cancel();
        assert!(!controller.is_active());
        assert_eq!(controller.current_torque(), current);
    }

    #[test]
    fn test_soft_stop_reset() {
        let mut controller = SoftStopController::new();
        controller.start_soft_stop(10.0);
        controller.update(Duration::from_millis(25));

        controller.reset();
        assert!(!controller.is_active());
        assert_eq!(controller.current_torque(), 0.0);
        assert_eq!(controller.start_torque(), 0.0);
    }

    #[test]
    fn test_soft_stop_remaining_time() {
        let mut controller = SoftStopController::new();
        controller.start_soft_stop_with_duration(10.0, Duration::from_millis(100));

        let remaining = controller.remaining_time();
        assert_eq!(remaining, Some(Duration::from_millis(100)));

        controller.update(Duration::from_millis(25));
        let remaining = controller.remaining_time();
        assert_eq!(remaining, Some(Duration::from_millis(75)));

        // Complete the ramp
        controller.update(Duration::from_millis(75));
        let remaining = controller.remaining_time();
        assert!(remaining.is_none());
    }

    #[test]
    fn test_soft_stop_ramp_to() {
        let mut controller = SoftStopController::new();
        controller.start_ramp_to(0.0, 5.0, Duration::from_millis(100));

        assert!(controller.is_active());
        assert_eq!(controller.start_torque(), 0.0);
        assert_eq!(controller.target_torque(), 5.0);

        // Complete ramp
        controller.update(Duration::from_millis(100));
        assert_eq!(controller.current_torque(), 5.0);
    }

    #[test]
    fn test_soft_stop_instant_duration() {
        let mut controller = SoftStopController::new();
        controller.start_soft_stop_with_duration(10.0, Duration::ZERO);

        // Zero duration should complete immediately
        let torque = controller.update(Duration::ZERO);
        assert!(!controller.is_active());
        assert_eq!(torque, 0.0);
    }
}

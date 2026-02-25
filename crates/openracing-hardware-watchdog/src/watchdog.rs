//! Hardware watchdog trait definition.
//!
//! This module provides the core `HardwareWatchdog` trait for implementing
//! RT-safe watchdog implementations.

use crate::config::WatchdogConfig;
use crate::error::HardwareWatchdogResult;
use crate::state::{WatchdogMetrics, WatchdogStatus};

/// Hardware watchdog trait for safety-critical torque control.
///
/// Implementations must ensure that if `feed()` is not called within
/// the timeout period, the device enters a safe state (zero torque).
///
/// # Real-Time Safety
///
/// All trait methods must be RT-safe:
/// - No heap allocations
/// - No blocking operations
/// - Bounded WCET
///
/// Implementations should document their WCET bounds for each method.
///
/// # State Machine
///
/// ```text
/// Disarmed ──arm()──► Armed
///     ▲                  │
///     │            feed()│ timeout()
///     │                  ▼
///     │            TimedOut
///     │                  │
///  reset()          trigger_safe()
///     │                  │
///     └──────────────────┤
///                        ▼
///                    SafeState
/// ```
///
/// # Implementation Requirements
///
/// 1. `feed()` MUST be callable from RT contexts at 1kHz
/// 2. `has_timed_out()` MUST return true after timeout period without feed
/// 3. `trigger_safe_state()` MUST put hardware in safe state (zero torque)
/// 4. All state transitions MUST be atomic
pub trait HardwareWatchdog: Send + Sync {
    /// Feed the watchdog to prevent timeout.
    ///
    /// This method should be called from the RT loop on every tick
    /// to prevent watchdog timeout.
    ///
    /// # Real-Time Safety
    ///
    /// WCET: Implementation-defined, typically < 100ns
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Watchdog is not armed
    /// - Watchdog has timed out
    /// - Safe state was triggered
    fn feed(&mut self) -> HardwareWatchdogResult<()>;

    /// Get the watchdog timeout in milliseconds.
    ///
    /// # Real-Time Safety
    ///
    /// WCET: < 50ns
    fn timeout_ms(&self) -> u32;

    /// Check if the watchdog is armed (active and monitoring).
    ///
    /// # Real-Time Safety
    ///
    /// WCET: < 50ns
    fn is_armed(&self) -> bool;

    /// Arm the watchdog (start monitoring).
    ///
    /// After arming, `feed()` must be called within the timeout period
    /// to prevent the watchdog from timing out.
    ///
    /// # Real-Time Safety
    ///
    /// WCET: Implementation-defined, typically < 1μs
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Watchdog is already armed
    fn arm(&mut self) -> HardwareWatchdogResult<()>;

    /// Disarm the watchdog (stop monitoring).
    ///
    /// After disarming, the watchdog will not time out.
    ///
    /// # Real-Time Safety
    ///
    /// WCET: Implementation-defined, typically < 1μs
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Watchdog is not armed
    fn disarm(&mut self) -> HardwareWatchdogResult<()>;

    /// Trigger immediate safe state (zero torque).
    ///
    /// This method MUST put the hardware in a safe state immediately.
    /// After calling this method, the watchdog enters a terminal state
    /// and must be reset before being armed again.
    ///
    /// # Real-Time Safety
    ///
    /// WCET: Implementation-defined, MUST be < 1ms per safety requirements
    ///
    /// # Errors
    ///
    /// Returns an error if safe state was already triggered.
    fn trigger_safe_state(&mut self) -> HardwareWatchdogResult<()>;

    /// Check if the watchdog has timed out.
    ///
    /// # Real-Time Safety
    ///
    /// WCET: < 100ns
    fn has_timed_out(&self) -> bool;

    /// Check if safe state was triggered.
    ///
    /// # Real-Time Safety
    ///
    /// WCET: < 50ns
    fn is_safe_state_triggered(&self) -> bool;

    /// Get the current watchdog status.
    ///
    /// # Real-Time Safety
    ///
    /// WCET: < 50ns
    fn status(&self) -> WatchdogStatus;

    /// Get time since last feed.
    ///
    /// Returns `None` if the watchdog has never been fed.
    ///
    /// # Real-Time Safety
    ///
    /// WCET: < 100ns
    fn time_since_last_feed_us(&self) -> Option<u64>;

    /// Reset the watchdog to the disarmed state.
    ///
    /// This clears all state and allows the watchdog to be armed again.
    ///
    /// # Real-Time Safety
    ///
    /// WCET: Implementation-defined, typically < 1μs
    fn reset(&mut self);

    /// Get the watchdog configuration.
    ///
    /// # Real-Time Safety
    ///
    /// WCET: < 50ns
    fn config(&self) -> &WatchdogConfig;

    /// Get the watchdog metrics.
    ///
    /// Returns a snapshot of the current metrics.
    ///
    /// # Real-Time Safety
    ///
    /// WCET: < 100ns
    fn metrics(&self) -> WatchdogMetrics;

    /// Check the watchdog health.
    ///
    /// Performs a health check and returns true if the watchdog is healthy.
    ///
    /// # Real-Time Safety
    ///
    /// WCET: Implementation-defined
    fn is_healthy(&self) -> bool {
        !self.has_timed_out() && !self.is_safe_state_triggered()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trait_bounds() {
        fn assert_send_sync<T: Send + Sync + ?Sized>() {}
        assert_send_sync::<dyn HardwareWatchdog>();
    }
}

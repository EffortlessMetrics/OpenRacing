//! Software watchdog implementation.
//!
//! This module provides `SoftwareWatchdog`, a software-based implementation
//! of the `HardwareWatchdog` trait for testing and hardware-free environments.

use crate::config::WatchdogConfig;
use crate::error::{HardwareWatchdogError, HardwareWatchdogResult};
use crate::state::{WatchdogMetrics, WatchdogState, WatchdogStatus};
use crate::watchdog::HardwareWatchdog;
use portable_atomic::{AtomicBool, AtomicU64, Ordering};

/// Software-based hardware watchdog implementation.
///
/// This implementation provides a software watchdog that can be used
/// when hardware watchdog is not available, or for testing purposes.
///
/// # Real-Time Safety
///
/// All methods are RT-safe:
/// - No heap allocations after initialization
/// - No blocking operations
/// - All state transitions are atomic
///
/// # WCET Bounds
///
/// - `feed()`: < 100ns
/// - `is_armed()`: < 50ns
/// - `has_timed_out()`: < 100ns
/// - `arm()`: < 500ns
/// - `disarm()`: < 500ns
/// - `trigger_safe_state()`: < 200ns
/// - `status()`: < 50ns
///
/// # Example
///
/// ```rust
/// use openracing_hardware_watchdog::{SoftwareWatchdog, WatchdogConfig, HardwareWatchdog};
///
/// let config = WatchdogConfig::new(100).expect("Valid config");
/// let mut watchdog = SoftwareWatchdog::new(config);
///
/// watchdog.arm().expect("Failed to arm");
/// watchdog.feed().expect("Failed to feed");
/// assert!(watchdog.is_armed());
/// assert!(!watchdog.has_timed_out());
/// ```
#[derive(Debug)]
pub struct SoftwareWatchdog {
    /// Watchdog configuration.
    config: WatchdogConfig,
    /// Watchdog state machine.
    state: WatchdogState,
    /// Last feed timestamp in microseconds.
    last_feed_us: AtomicU64,
    /// Time source start point (for elapsed time calculation).
    start_time_us: AtomicU64,
    /// Safe state triggered flag.
    safe_state_triggered: AtomicBool,
    /// Metrics (not atomic, requires &mut for updates).
    metrics: core::cell::UnsafeCell<WatchdogMetrics>,
}

impl SoftwareWatchdog {
    /// Create a new software watchdog with the specified configuration.
    ///
    /// # Arguments
    ///
    /// * `config` - Watchdog configuration.
    #[must_use]
    pub fn new(config: WatchdogConfig) -> Self {
        Self {
            config,
            state: WatchdogState::new(),
            last_feed_us: AtomicU64::new(0),
            start_time_us: AtomicU64::new(0),
            safe_state_triggered: AtomicBool::new(false),
            metrics: core::cell::UnsafeCell::new(WatchdogMetrics::new()),
        }
    }

    /// Create a new software watchdog with a timeout in milliseconds.
    ///
    /// # Arguments
    ///
    /// * `timeout_ms` - Watchdog timeout in milliseconds.
    ///
    /// # Errors
    ///
    /// Returns an error if the timeout is outside the valid range.
    pub fn with_timeout(timeout_ms: u32) -> HardwareWatchdogResult<Self> {
        let config = WatchdogConfig::new(timeout_ms)?;
        Ok(Self::new(config))
    }

    /// Create a new software watchdog with default 100ms timeout.
    #[must_use]
    pub fn with_default_timeout() -> Self {
        Self::new(WatchdogConfig::default())
    }

    /// Get elapsed time in microseconds since start.
    ///
    /// In `no_std` environments without a time source, this returns 0.
    /// Use `set_elapsed_us()` to provide time from an external source.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn elapsed_us(&self) -> u64 {
        #[cfg(feature = "std")]
        {
            use std::time::{SystemTime, UNIX_EPOCH};
            SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| {
                let micros = d.as_micros();
                if micros > u128::from(u64::MAX) {
                    u64::MAX
                } else {
                    micros as u64
                }
            })
        }
        #[cfg(not(feature = "std"))]
        {
            0
        }
    }

    /// Set the elapsed time from an external source.
    ///
    /// Use this in `no_std` environments to provide time from an external
    /// source (e.g., a hardware timer).
    pub fn set_elapsed_us(&self, elapsed_us: u64) {
        self.start_time_us.store(elapsed_us, Ordering::Release);
    }

    /// Get the current timestamp for feeding.
    #[must_use]
    fn current_timestamp_us(&self) -> u64 {
        self.elapsed_us()
    }

    /// Check if the watchdog has timed out based on elapsed time.
    fn check_timeout(&self) -> bool {
        let status = self.state.status();
        if status != WatchdogStatus::Armed {
            return false;
        }

        let last_feed = self.last_feed_us.load(Ordering::Acquire);
        if last_feed == 0 {
            return false;
        }

        let current = self.current_timestamp_us();
        let elapsed = current.saturating_sub(last_feed);
        let timeout_us = self.config.timeout_us();

        elapsed > timeout_us
    }

    /// Update metrics (internal helper).
    fn update_metrics(&self, f: impl FnOnce(&mut WatchdogMetrics)) {
        unsafe {
            f(&mut *self.metrics.get());
        }
    }

    /// Manually trigger a timeout for testing purposes.
    ///
    /// This method forces the watchdog into the timed out state,
    /// which is useful for testing timeout handling logic.
    ///
    /// # Real-Time Safety
    ///
    /// WCET: < 200ns
    ///
    /// # Errors
    ///
    /// Returns an error if the watchdog is not in the Armed state.
    pub fn trigger_timeout(&self) -> HardwareWatchdogResult<()> {
        self.state.timeout()?;
        self.update_metrics(WatchdogMetrics::record_timeout);
        Ok(())
    }
}

impl HardwareWatchdog for SoftwareWatchdog {
    fn feed(&mut self) -> HardwareWatchdogResult<()> {
        let status = self.state.status();

        match status {
            WatchdogStatus::Armed => {
                let timestamp = self.current_timestamp_us();
                self.last_feed_us.store(timestamp, Ordering::Release);
                self.state.feed()?;
                self.update_metrics(|m| m.record_feed(timestamp));
                Ok(())
            }
            WatchdogStatus::TimedOut => Err(HardwareWatchdogError::TimedOut),
            WatchdogStatus::Disarmed => Err(HardwareWatchdogError::NotArmed),
            WatchdogStatus::SafeState => Err(HardwareWatchdogError::SafeStateAlreadyTriggered),
        }
    }

    fn timeout_ms(&self) -> u32 {
        self.config.timeout_ms
    }

    fn is_armed(&self) -> bool {
        self.state.status() == WatchdogStatus::Armed
    }

    fn arm(&mut self) -> HardwareWatchdogResult<()> {
        self.state.arm()?;
        self.start_time_us
            .store(self.current_timestamp_us(), Ordering::Release);
        self.last_feed_us
            .store(self.current_timestamp_us(), Ordering::Release);
        self.update_metrics(WatchdogMetrics::record_arm);
        Ok(())
    }

    fn disarm(&mut self) -> HardwareWatchdogResult<()> {
        self.state.disarm()
    }

    fn trigger_safe_state(&mut self) -> HardwareWatchdogResult<()> {
        self.state.trigger_safe_state()?;
        self.safe_state_triggered.store(true, Ordering::Release);
        self.update_metrics(WatchdogMetrics::record_safe_state);
        Ok(())
    }

    fn has_timed_out(&self) -> bool {
        if self.state.status() == WatchdogStatus::TimedOut {
            return true;
        }

        if self.check_timeout() {
            let _ = self.state.timeout();
            self.update_metrics(WatchdogMetrics::record_timeout);
            return true;
        }

        false
    }

    fn is_safe_state_triggered(&self) -> bool {
        self.safe_state_triggered.load(Ordering::Acquire)
    }

    fn status(&self) -> WatchdogStatus {
        self.state.status()
    }

    fn time_since_last_feed_us(&self) -> Option<u64> {
        let last_feed = self.last_feed_us.load(Ordering::Acquire);
        if last_feed == 0 {
            return None;
        }
        let current = self.current_timestamp_us();
        Some(current.saturating_sub(last_feed))
    }

    fn reset(&mut self) {
        self.state.reset();
        self.last_feed_us.store(0, Ordering::Release);
        self.start_time_us.store(0, Ordering::Release);
        self.safe_state_triggered.store(false, Ordering::Release);
        self.update_metrics(WatchdogMetrics::reset);
    }

    fn config(&self) -> &WatchdogConfig {
        &self.config
    }

    fn metrics(&self) -> WatchdogMetrics {
        unsafe { *self.metrics.get() }
    }
}

impl Default for SoftwareWatchdog {
    fn default() -> Self {
        Self::with_default_timeout()
    }
}

unsafe impl Send for SoftwareWatchdog {}
unsafe impl Sync for SoftwareWatchdog {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_software_watchdog_creation() {
        let config = WatchdogConfig::new(100).expect("Valid config");
        let watchdog = SoftwareWatchdog::new(config);

        assert_eq!(watchdog.timeout_ms(), 100);
        assert!(!watchdog.is_armed());
        assert!(!watchdog.has_timed_out());
    }

    #[test]
    fn test_software_watchdog_default() {
        let watchdog = SoftwareWatchdog::with_default_timeout();
        assert_eq!(watchdog.timeout_ms(), 100);
    }

    #[test]
    fn test_arm_disarm() {
        let config = WatchdogConfig::new(100).expect("Valid config");
        let mut watchdog = SoftwareWatchdog::new(config);

        assert!(!watchdog.is_armed());

        watchdog.arm().expect("Arm should succeed");
        assert!(watchdog.is_armed());

        let result = watchdog.arm();
        assert!(result.is_err());

        watchdog.disarm().expect("Disarm should succeed");
        assert!(!watchdog.is_armed());

        let result = watchdog.disarm();
        assert!(result.is_err());
    }

    #[test]
    fn test_feed_when_disarmed() {
        let config = WatchdogConfig::new(100).expect("Valid config");
        let mut watchdog = SoftwareWatchdog::new(config);

        let result = watchdog.feed();
        assert!(result.is_err());
    }

    #[test]
    fn test_feed_when_armed() {
        let config = WatchdogConfig::new(100).expect("Valid config");
        let mut watchdog = SoftwareWatchdog::new(config);

        watchdog.arm().expect("Arm should succeed");
        let result = watchdog.feed();
        assert!(result.is_ok());

        let metrics = watchdog.metrics();
        assert_eq!(metrics.feed_count, 1);
    }

    #[test]
    fn test_trigger_safe_state() {
        let config = WatchdogConfig::new(100).expect("Valid config");
        let mut watchdog = SoftwareWatchdog::new(config);

        watchdog
            .trigger_safe_state()
            .expect("Safe state should succeed");
        assert!(watchdog.is_safe_state_triggered());
        assert_eq!(watchdog.status(), WatchdogStatus::SafeState);

        let result = watchdog.trigger_safe_state();
        assert!(result.is_err());
    }

    #[test]
    fn test_reset() {
        let config = WatchdogConfig::new(100).expect("Valid config");
        let mut watchdog = SoftwareWatchdog::new(config);

        watchdog.arm().expect("Arm should succeed");
        watchdog.feed().expect("Feed should succeed");

        watchdog.reset();

        assert!(!watchdog.is_armed());
        assert!(!watchdog.is_safe_state_triggered());
    }

    #[test]
    fn test_status() {
        let config = WatchdogConfig::new(100).expect("Valid config");
        let mut watchdog = SoftwareWatchdog::new(config);

        assert_eq!(watchdog.status(), WatchdogStatus::Disarmed);

        watchdog.arm().expect("Arm should succeed");
        assert_eq!(watchdog.status(), WatchdogStatus::Armed);
    }

    #[test]
    fn test_is_healthy() {
        let config = WatchdogConfig::new(100).expect("Valid config");
        let mut watchdog = SoftwareWatchdog::new(config);

        assert!(watchdog.is_healthy());

        watchdog.arm().expect("Arm should succeed");
        assert!(watchdog.is_healthy());
    }
}

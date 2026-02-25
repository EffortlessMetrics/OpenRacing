//! Watchdog state machine and metrics.
//!
//! This module provides the state machine for hardware watchdog management
//! with deterministic, atomic state transitions.

use portable_atomic::{AtomicU32, Ordering};

/// Watchdog operational status.
///
/// This enum represents the current state of the watchdog state machine.
/// All transitions are atomic and RT-safe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum WatchdogStatus {
    /// Watchdog is not armed (inactive).
    #[default]
    Disarmed = 0,
    /// Watchdog is armed and monitoring.
    Armed = 1,
    /// Watchdog has timed out.
    TimedOut = 2,
    /// Safe state has been triggered (terminal state).
    SafeState = 3,
}

impl WatchdogStatus {
    /// Convert from raw u32 value.
    ///
    /// # Safety
    ///
    /// The value must be a valid `WatchdogStatus` discriminant.
    #[must_use]
    pub fn from_raw(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::Disarmed),
            1 => Some(Self::Armed),
            2 => Some(Self::TimedOut),
            3 => Some(Self::SafeState),
            _ => None,
        }
    }

    /// Convert to raw u32 value.
    #[must_use]
    pub fn to_raw(self) -> u32 {
        self as u32
    }

    /// Check if the watchdog is in a terminal state.
    ///
    /// Terminal states cannot transition to other states without a reset.
    #[must_use]
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::SafeState)
    }

    /// Check if the watchdog is active (armed or timed out).
    #[must_use]
    pub fn is_active(self) -> bool {
        matches!(self, Self::Armed | Self::TimedOut)
    }

    /// Get the status as a string slice.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Disarmed => "Disarmed",
            Self::Armed => "Armed",
            Self::TimedOut => "TimedOut",
            Self::SafeState => "SafeState",
        }
    }
}

impl core::fmt::Display for WatchdogStatus {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Atomic watchdog state for RT-safe state machine.
///
/// This struct provides atomic state transitions for the watchdog state machine.
/// All operations are lock-free and suitable for real-time contexts.
///
/// # Real-Time Safety
///
/// All methods in this struct are RT-safe:
/// - No heap allocations
/// - No blocking operations
/// - Bounded WCET (< 50ns on modern hardware)
///
/// # State Transition Diagram
///
/// ```text
/// Disarmed ──arm()──► Armed
///     ▲                  │
///     │                  │
///     │            ┌─────┴─────┐
///     │            │           │
///  reset()      feed()    timeout()
///     │            │           │
///     │            ▼           ▼
///     │      (stay Armed)  TimedOut
///     │                          │
///     │                      trigger_safe()
///     │                          │
///     └──────────────────────────┘
///                            │
///                            ▼
///                        SafeState
/// ```
#[derive(Debug)]
#[repr(C)]
pub struct WatchdogState {
    /// Current status (atomic for lock-free access).
    status: AtomicU32,
    /// Number of times armed.
    arm_count: AtomicU32,
    /// Number of times fed.
    feed_count: AtomicU32,
    /// Number of timeouts.
    timeout_count: AtomicU32,
    /// Number of safe state triggers.
    safe_state_count: AtomicU32,
}

impl WatchdogState {
    /// Create a new watchdog state in the Disarmed status.
    #[must_use]
    pub fn new() -> Self {
        Self {
            status: AtomicU32::new(WatchdogStatus::Disarmed.to_raw()),
            arm_count: AtomicU32::new(0),
            feed_count: AtomicU32::new(0),
            timeout_count: AtomicU32::new(0),
            safe_state_count: AtomicU32::new(0),
        }
    }

    /// Get the current status.
    ///
    /// # Real-Time Safety
    ///
    /// WCET: < 50ns
    #[must_use]
    pub fn status(&self) -> WatchdogStatus {
        let raw = self.status.load(Ordering::Acquire);
        WatchdogStatus::from_raw(raw).unwrap_or(WatchdogStatus::Disarmed)
    }

    /// Attempt to transition from Disarmed to Armed.
    ///
    /// # Real-Time Safety
    ///
    /// WCET: < 50ns
    ///
    /// # Errors
    ///
    /// Returns an error if the current state is not `Disarmed`.
    pub fn arm(&self) -> Result<(), crate::error::HardwareWatchdogError> {
        let previous = self.status.compare_exchange(
            WatchdogStatus::Disarmed.to_raw(),
            WatchdogStatus::Armed.to_raw(),
            Ordering::AcqRel,
            Ordering::Acquire,
        );

        match previous {
            Ok(_) => {
                self.arm_count.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
            Err(current) => {
                let current_status =
                    WatchdogStatus::from_raw(current).unwrap_or(WatchdogStatus::Disarmed);
                Err(crate::error::HardwareWatchdogError::invalid_transition(
                    current_status.as_str(),
                    "Armed",
                ))
            }
        }
    }

    /// Attempt to transition from Armed to Disarmed.
    ///
    /// # Real-Time Safety
    ///
    /// WCET: < 50ns
    ///
    /// # Errors
    ///
    /// Returns an error if the current state is not `Armed`.
    pub fn disarm(&self) -> Result<(), crate::error::HardwareWatchdogError> {
        let previous = self.status.compare_exchange(
            WatchdogStatus::Armed.to_raw(),
            WatchdogStatus::Disarmed.to_raw(),
            Ordering::AcqRel,
            Ordering::Acquire,
        );

        match previous {
            Ok(_) => Ok(()),
            Err(current) => {
                let current_status =
                    WatchdogStatus::from_raw(current).unwrap_or(WatchdogStatus::Disarmed);
                Err(crate::error::HardwareWatchdogError::invalid_transition(
                    current_status.as_str(),
                    "Disarmed",
                ))
            }
        }
    }

    /// Record a feed operation (Armed state only).
    ///
    /// # Real-Time Safety
    ///
    /// WCET: < 50ns
    ///
    /// # Errors
    ///
    /// Returns an error if the current state is not `Armed`.
    pub fn feed(&self) -> Result<(), crate::error::HardwareWatchdogError> {
        let current = self.status.load(Ordering::Acquire);
        let current_status = WatchdogStatus::from_raw(current).unwrap_or(WatchdogStatus::Disarmed);

        match current_status {
            WatchdogStatus::Armed => {
                self.feed_count.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
            WatchdogStatus::TimedOut => Err(crate::error::HardwareWatchdogError::TimedOut),
            WatchdogStatus::Disarmed => Err(crate::error::HardwareWatchdogError::NotArmed),
            WatchdogStatus::SafeState => {
                Err(crate::error::HardwareWatchdogError::SafeStateAlreadyTriggered)
            }
        }
    }

    /// Transition to `TimedOut` state.
    ///
    /// # Real-Time Safety
    ///
    /// WCET: < 50ns
    ///
    /// # Errors
    ///
    /// Returns an error if the current state is not `Armed`.
    pub fn timeout(&self) -> Result<(), crate::error::HardwareWatchdogError> {
        let previous = self.status.compare_exchange(
            WatchdogStatus::Armed.to_raw(),
            WatchdogStatus::TimedOut.to_raw(),
            Ordering::AcqRel,
            Ordering::Acquire,
        );

        match previous {
            Ok(_) => {
                self.timeout_count.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
            Err(current) => {
                let current_status =
                    WatchdogStatus::from_raw(current).unwrap_or(WatchdogStatus::Disarmed);
                Err(crate::error::HardwareWatchdogError::invalid_transition(
                    current_status.as_str(),
                    "TimedOut",
                ))
            }
        }
    }

    /// Transition to `SafeState` (terminal).
    ///
    /// # Real-Time Safety
    ///
    /// WCET: < 50ns
    ///
    /// # Errors
    ///
    /// Returns an error if already in `SafeState`.
    pub fn trigger_safe_state(&self) -> Result<(), crate::error::HardwareWatchdogError> {
        let current = self.status.load(Ordering::Acquire);
        let current_status = WatchdogStatus::from_raw(current).unwrap_or(WatchdogStatus::Disarmed);

        if current_status == WatchdogStatus::SafeState {
            Err(crate::error::HardwareWatchdogError::SafeStateAlreadyTriggered)
        } else {
            self.status
                .store(WatchdogStatus::SafeState.to_raw(), Ordering::Release);
            self.safe_state_count.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }
    }

    /// Reset to Disarmed state.
    ///
    /// # Real-Time Safety
    ///
    /// WCET: < 50ns
    pub fn reset(&self) {
        self.status
            .store(WatchdogStatus::Disarmed.to_raw(), Ordering::Release);
    }

    /// Get the arm count.
    #[must_use]
    pub fn arm_count(&self) -> u32 {
        self.arm_count.load(Ordering::Acquire)
    }

    /// Get the feed count.
    #[must_use]
    pub fn feed_count(&self) -> u32 {
        self.feed_count.load(Ordering::Acquire)
    }

    /// Get the timeout count.
    #[must_use]
    pub fn timeout_count(&self) -> u32 {
        self.timeout_count.load(Ordering::Acquire)
    }

    /// Get the safe state trigger count.
    #[must_use]
    pub fn safe_state_count(&self) -> u32 {
        self.safe_state_count.load(Ordering::Acquire)
    }
}

impl Default for WatchdogState {
    fn default() -> Self {
        Self::new()
    }
}

/// Watchdog metrics for monitoring and diagnostics.
///
/// This struct contains counters and timing information for watchdog
/// operations. It is designed for RT-safe access without allocations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct WatchdogMetrics {
    /// Total number of feed operations.
    pub feed_count: u64,
    /// Total number of arm operations.
    pub arm_count: u64,
    /// Total number of timeout events.
    pub timeout_count: u64,
    /// Total number of safe state triggers.
    pub safe_state_count: u64,
    /// Number of consecutive feed failures.
    pub consecutive_failures: u32,
    /// Maximum time between feeds (in microseconds).
    pub max_feed_interval_us: u64,
    /// Last feed timestamp (in microseconds since start).
    pub last_feed_timestamp_us: u64,
}

impl WatchdogMetrics {
    /// Create a new metrics instance with default values.
    #[must_use]
    pub fn new() -> Self {
        Self {
            feed_count: 0,
            arm_count: 0,
            timeout_count: 0,
            safe_state_count: 0,
            consecutive_failures: 0,
            max_feed_interval_us: 0,
            last_feed_timestamp_us: 0,
        }
    }

    /// Record a successful feed.
    pub fn record_feed(&mut self, timestamp_us: u64) {
        if self.last_feed_timestamp_us > 0 {
            let interval = timestamp_us.saturating_sub(self.last_feed_timestamp_us);
            if interval > self.max_feed_interval_us {
                self.max_feed_interval_us = interval;
            }
        }
        self.last_feed_timestamp_us = timestamp_us;
        self.feed_count = self.feed_count.saturating_add(1);
        self.consecutive_failures = 0;
    }

    /// Record a feed failure.
    pub fn record_failure(&mut self) {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
    }

    /// Record an arm operation.
    pub fn record_arm(&mut self) {
        self.arm_count = self.arm_count.saturating_add(1);
    }

    /// Record a timeout event.
    pub fn record_timeout(&mut self) {
        self.timeout_count = self.timeout_count.saturating_add(1);
    }

    /// Record a safe state trigger.
    pub fn record_safe_state(&mut self) {
        self.safe_state_count = self.safe_state_count.saturating_add(1);
    }

    /// Reset all metrics.
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Get the feed success rate (0.0 to 1.0).
    #[allow(clippy::cast_precision_loss)]
    #[must_use]
    pub fn success_rate(&self) -> f32 {
        let total = self
            .feed_count
            .saturating_add(u64::from(self.consecutive_failures));
        if total == 0 {
            1.0
        } else {
            self.feed_count as f32 / total as f32
        }
    }
}

impl Default for WatchdogMetrics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_transitions() {
        let state = WatchdogState::new();
        assert_eq!(state.status(), WatchdogStatus::Disarmed);

        state.arm().expect("Arm should succeed");
        assert_eq!(state.status(), WatchdogStatus::Armed);

        state.disarm().expect("Disarm should succeed");
        assert_eq!(state.status(), WatchdogStatus::Disarmed);
    }

    #[test]
    fn test_arm_from_wrong_state() {
        let state = WatchdogState::new();
        state.arm().expect("Arm should succeed");

        let result = state.arm();
        assert!(result.is_err());
    }

    #[test]
    fn test_feed_in_armed_state() {
        let state = WatchdogState::new();
        state.arm().expect("Arm should succeed");

        state.feed().expect("Feed should succeed");
        assert_eq!(state.feed_count(), 1);
    }

    #[test]
    fn test_feed_in_disarmed_state() {
        let state = WatchdogState::new();

        let result = state.feed();
        assert!(result.is_err());
    }

    #[test]
    fn test_timeout_transition() {
        let state = WatchdogState::new();
        state.arm().expect("Arm should succeed");

        state.timeout().expect("Timeout should succeed");
        assert_eq!(state.status(), WatchdogStatus::TimedOut);
        assert_eq!(state.timeout_count(), 1);
    }

    #[test]
    fn test_safe_state_transition() {
        let state = WatchdogState::new();

        state
            .trigger_safe_state()
            .expect("Safe state should succeed");
        assert_eq!(state.status(), WatchdogStatus::SafeState);
        assert_eq!(state.safe_state_count(), 1);

        let result = state.trigger_safe_state();
        assert!(result.is_err());
    }

    #[test]
    fn test_reset() {
        let state = WatchdogState::new();
        state.arm().expect("Arm should succeed");
        state.timeout().expect("Timeout should succeed");

        state.reset();
        assert_eq!(state.status(), WatchdogStatus::Disarmed);
    }

    #[test]
    fn test_metrics() {
        let mut metrics = WatchdogMetrics::new();

        metrics.record_arm();
        assert_eq!(metrics.arm_count, 1);

        metrics.record_feed(1000);
        metrics.record_feed(2000);
        assert_eq!(metrics.feed_count, 2);
        assert_eq!(metrics.max_feed_interval_us, 1000);

        metrics.record_failure();
        assert_eq!(metrics.consecutive_failures, 1);
        assert!((metrics.success_rate() - 0.666).abs() < 0.1);

        metrics.record_timeout();
        assert_eq!(metrics.timeout_count, 1);

        metrics.record_safe_state();
        assert_eq!(metrics.safe_state_count, 1);
    }

    #[test]
    fn test_metrics_reset() {
        let mut metrics = WatchdogMetrics::new();
        metrics.record_feed(1000);
        metrics.record_timeout();

        metrics.reset();

        assert_eq!(metrics.feed_count, 0);
        assert_eq!(metrics.timeout_count, 0);
    }
}

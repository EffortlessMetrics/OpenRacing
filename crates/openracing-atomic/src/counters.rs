//! Atomic counters for RT-safe metrics collection.
//!
//! This module provides [`AtomicCounters`], a collection of atomic counters that can
//! be safely incremented from the RT hot path without allocations or blocking.
//!
//! # RT Safety
//!
//! All methods on [`AtomicCounters`] are RT-safe:
//! - Use `Ordering::Relaxed` for performance (correctness is maintained)
//! - No heap allocations
//! - No syscalls
//! - Bounded execution time (single atomic instruction)

use core::sync::atomic::{AtomicU64, Ordering};

/// Counter snapshot returned by [`AtomicCounters::snapshot`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CounterSnapshot {
    /// Total number of RT ticks processed
    pub total_ticks: u64,
    /// Number of missed ticks (deadline violations)
    pub missed_ticks: u64,
    /// Number of safety events triggered
    pub safety_events: u64,
    /// Number of profile switches
    pub profile_switches: u64,
    /// Number of telemetry packets received
    pub telemetry_packets_received: u64,
    /// Number of telemetry packets lost
    pub telemetry_packets_lost: u64,
    /// Number of torque saturation samples recorded
    pub torque_saturation_samples: u64,
    /// Number of samples where torque was saturated
    pub torque_saturation_count: u64,
    /// Number of HID write errors
    pub hid_write_errors: u64,
}

/// Atomic counters for RT-safe metrics collection.
///
/// This struct provides a collection of atomic counters that can be safely
/// incremented from the RT hot path. All operations are lock-free and
/// allocation-free after initialization.
///
/// # Thread Safety
///
/// All counters use `AtomicU64` with `Ordering::Relaxed` semantics, which is
/// sufficient for metrics counters where:
/// - We don't need synchronization with other memory operations
/// - Counter values are eventually consistent
/// - Individual counter increments don't need to be atomic with each other
///
/// # RT Safety
///
/// All `inc_*` and `record_*` methods are RT-safe:
/// - Single atomic instruction per call
/// - No heap allocation
/// - No blocking
/// - No syscalls
///
/// # Example
///
/// ```rust
/// use openracing_atomic::AtomicCounters;
///
/// let counters = AtomicCounters::new();
///
/// // RT hot path - these are safe to call
/// counters.inc_tick();
/// counters.inc_missed_tick();
/// counters.record_torque_saturation(true);
///
/// // Non-RT path - read snapshot
/// let snapshot = counters.snapshot();
/// assert_eq!(snapshot.total_ticks, 1);
/// ```
#[derive(Debug)]
pub struct AtomicCounters {
    total_ticks: AtomicU64,
    missed_ticks: AtomicU64,
    safety_events: AtomicU64,
    profile_switches: AtomicU64,
    telemetry_packets_received: AtomicU64,
    telemetry_packets_lost: AtomicU64,
    torque_saturation_samples: AtomicU64,
    torque_saturation_count: AtomicU64,
    hid_write_errors: AtomicU64,
}

impl Default for AtomicCounters {
    fn default() -> Self {
        Self::new()
    }
}

impl AtomicCounters {
    /// Create a new `AtomicCounters` with all counters initialized to zero.
    ///
    /// This is an initialization-time operation and allocates the atomic storage.
    /// After creation, no further allocations occur.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            total_ticks: AtomicU64::new(0),
            missed_ticks: AtomicU64::new(0),
            safety_events: AtomicU64::new(0),
            profile_switches: AtomicU64::new(0),
            telemetry_packets_received: AtomicU64::new(0),
            telemetry_packets_lost: AtomicU64::new(0),
            torque_saturation_samples: AtomicU64::new(0),
            torque_saturation_count: AtomicU64::new(0),
            hid_write_errors: AtomicU64::new(0),
        }
    }

    /// Create a new `AtomicCounters` with initial values.
    ///
    /// Useful for testing or for resuming from a previous state.
    #[must_use]
    pub fn with_values(snapshot: CounterSnapshot) -> Self {
        Self {
            total_ticks: AtomicU64::new(snapshot.total_ticks),
            missed_ticks: AtomicU64::new(snapshot.missed_ticks),
            safety_events: AtomicU64::new(snapshot.safety_events),
            profile_switches: AtomicU64::new(snapshot.profile_switches),
            telemetry_packets_received: AtomicU64::new(snapshot.telemetry_packets_received),
            telemetry_packets_lost: AtomicU64::new(snapshot.telemetry_packets_lost),
            torque_saturation_samples: AtomicU64::new(snapshot.torque_saturation_samples),
            torque_saturation_count: AtomicU64::new(snapshot.torque_saturation_count),
            hid_write_errors: AtomicU64::new(snapshot.hid_write_errors),
        }
    }

    /// Increment the tick counter.
    ///
    /// # RT Safety
    ///
    /// This method is RT-safe. It performs a single atomic fetch-add with
    /// `Ordering::Relaxed`, which compiles to a single CPU instruction on
    /// most architectures.
    #[inline]
    pub fn inc_tick(&self) {
        self.total_ticks.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment the tick counter by a specific amount.
    ///
    /// # RT Safety
    ///
    /// RT-safe. Single atomic fetch-add instruction.
    #[inline]
    pub fn inc_tick_by(&self, amount: u64) {
        self.total_ticks.fetch_add(amount, Ordering::Relaxed);
    }

    /// Increment the missed tick counter.
    ///
    /// Call this when an RT deadline is missed.
    ///
    /// # RT Safety
    ///
    /// RT-safe. Single atomic fetch-add instruction.
    #[inline]
    pub fn inc_missed_tick(&self) {
        self.missed_ticks.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment the missed tick counter by a specific amount.
    ///
    /// # RT Safety
    ///
    /// RT-safe. Single atomic fetch-add instruction.
    #[inline]
    pub fn inc_missed_tick_by(&self, amount: u64) {
        self.missed_ticks.fetch_add(amount, Ordering::Relaxed);
    }

    /// Increment the safety event counter.
    ///
    /// Call this when a safety interlock is triggered.
    ///
    /// # RT Safety
    ///
    /// RT-safe. Single atomic fetch-add instruction.
    #[inline]
    pub fn inc_safety_event(&self) {
        self.safety_events.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment the profile switch counter.
    ///
    /// Call this when the active profile is changed.
    ///
    /// # RT Safety
    ///
    /// RT-safe. Single atomic fetch-add instruction.
    #[inline]
    pub fn inc_profile_switch(&self) {
        self.profile_switches.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a telemetry packet as received.
    ///
    /// # RT Safety
    ///
    /// RT-safe. Single atomic fetch-add instruction.
    #[inline]
    pub fn inc_telemetry_received(&self) {
        self.telemetry_packets_received
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Record a telemetry packet as lost.
    ///
    /// # RT Safety
    ///
    /// RT-safe. Single atomic fetch-add instruction.
    #[inline]
    pub fn inc_telemetry_lost(&self) {
        self.telemetry_packets_lost.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a torque saturation sample.
    ///
    /// Increments the samples counter, and if `is_saturated` is true,
    /// also increments the saturation count counter.
    ///
    /// # RT Safety
    ///
    /// RT-safe. One or two atomic fetch-add instructions depending on
    /// the `is_saturated` parameter.
    #[inline]
    pub fn record_torque_saturation(&self, is_saturated: bool) {
        self.torque_saturation_samples
            .fetch_add(1, Ordering::Relaxed);
        if is_saturated {
            self.torque_saturation_count.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Increment the HID write error counter.
    ///
    /// Call this when a HID write operation fails.
    ///
    /// # RT Safety
    ///
    /// RT-safe. Single atomic fetch-add instruction.
    #[inline]
    pub fn inc_hid_write_error(&self) {
        self.hid_write_errors.fetch_add(1, Ordering::Relaxed);
    }

    /// Get a snapshot of all counter values.
    ///
    /// This reads all counters without resetting them. The values are
    /// eventually consistent - there's no atomic snapshot across all counters.
    ///
    /// # RT Safety
    ///
    /// RT-safe but typically called from non-RT code. Performs multiple
    /// atomic loads, each of which is individually RT-safe.
    #[inline]
    #[must_use]
    pub fn snapshot(&self) -> CounterSnapshot {
        CounterSnapshot {
            total_ticks: self.total_ticks.load(Ordering::Relaxed),
            missed_ticks: self.missed_ticks.load(Ordering::Relaxed),
            safety_events: self.safety_events.load(Ordering::Relaxed),
            profile_switches: self.profile_switches.load(Ordering::Relaxed),
            telemetry_packets_received: self.telemetry_packets_received.load(Ordering::Relaxed),
            telemetry_packets_lost: self.telemetry_packets_lost.load(Ordering::Relaxed),
            torque_saturation_samples: self.torque_saturation_samples.load(Ordering::Relaxed),
            torque_saturation_count: self.torque_saturation_count.load(Ordering::Relaxed),
            hid_write_errors: self.hid_write_errors.load(Ordering::Relaxed),
        }
    }

    /// Get a snapshot and reset all counters to zero.
    ///
    /// This is typically called by the metrics collector to get the
    /// counter values for the current collection interval.
    ///
    /// # RT Safety
    ///
    /// **NOT RT-safe**. This method performs multiple atomic swap operations
    /// which could introduce jitter. Call from non-RT code only.
    #[inline]
    #[must_use]
    pub fn snapshot_and_reset(&self) -> CounterSnapshot {
        CounterSnapshot {
            total_ticks: self.total_ticks.swap(0, Ordering::Relaxed),
            missed_ticks: self.missed_ticks.swap(0, Ordering::Relaxed),
            safety_events: self.safety_events.swap(0, Ordering::Relaxed),
            profile_switches: self.profile_switches.swap(0, Ordering::Relaxed),
            telemetry_packets_received: self.telemetry_packets_received.swap(0, Ordering::Relaxed),
            telemetry_packets_lost: self.telemetry_packets_lost.swap(0, Ordering::Relaxed),
            torque_saturation_samples: self.torque_saturation_samples.swap(0, Ordering::Relaxed),
            torque_saturation_count: self.torque_saturation_count.swap(0, Ordering::Relaxed),
            hid_write_errors: self.hid_write_errors.swap(0, Ordering::Relaxed),
        }
    }

    /// Reset all counters to zero.
    ///
    /// # RT Safety
    ///
    /// **NOT RT-safe**. This method performs multiple atomic store operations.
    /// Call from non-RT code only.
    #[inline]
    pub fn reset(&self) {
        self.total_ticks.store(0, Ordering::Relaxed);
        self.missed_ticks.store(0, Ordering::Relaxed);
        self.safety_events.store(0, Ordering::Relaxed);
        self.profile_switches.store(0, Ordering::Relaxed);
        self.telemetry_packets_received.store(0, Ordering::Relaxed);
        self.telemetry_packets_lost.store(0, Ordering::Relaxed);
        self.torque_saturation_samples.store(0, Ordering::Relaxed);
        self.torque_saturation_count.store(0, Ordering::Relaxed);
        self.hid_write_errors.store(0, Ordering::Relaxed);
    }

    /// Get the current total ticks value.
    #[inline]
    #[must_use]
    pub fn total_ticks(&self) -> u64 {
        self.total_ticks.load(Ordering::Relaxed)
    }

    /// Get the current missed ticks value.
    #[inline]
    #[must_use]
    pub fn missed_ticks(&self) -> u64 {
        self.missed_ticks.load(Ordering::Relaxed)
    }

    /// Get the current safety events value.
    #[inline]
    #[must_use]
    pub fn safety_events(&self) -> u64 {
        self.safety_events.load(Ordering::Relaxed)
    }

    /// Calculate the torque saturation percentage.
    ///
    /// Returns 0.0 if no samples have been recorded.
    #[inline]
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn torque_saturation_percent(&self) -> f32 {
        let samples = self.torque_saturation_samples.load(Ordering::Relaxed);
        if samples == 0 {
            return 0.0;
        }
        let saturated = self.torque_saturation_count.load(Ordering::Relaxed);
        (saturated as f32 / samples as f32) * 100.0
    }

    /// Calculate the telemetry packet loss percentage.
    ///
    /// Returns 0.0 if no packets have been received.
    #[inline]
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn telemetry_loss_percent(&self) -> f32 {
        let received = self.telemetry_packets_received.load(Ordering::Relaxed);
        let lost = self.telemetry_packets_lost.load(Ordering::Relaxed);
        let total = received.saturating_add(lost);
        if total == 0 {
            return 0.0;
        }
        (lost as f32 / total as f32) * 100.0
    }
}

impl CounterSnapshot {
    /// Calculate the torque saturation percentage.
    ///
    /// Returns 0.0 if no samples have been recorded.
    #[inline]
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn torque_saturation_percent(&self) -> f32 {
        if self.torque_saturation_samples == 0 {
            return 0.0;
        }
        (self.torque_saturation_count as f32 / self.torque_saturation_samples as f32) * 100.0
    }

    /// Calculate the telemetry packet loss percentage.
    ///
    /// Returns 0.0 if no packets have been received.
    #[inline]
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn telemetry_loss_percent(&self) -> f32 {
        let total = self
            .telemetry_packets_received
            .saturating_add(self.telemetry_packets_lost);
        if total == 0 {
            return 0.0;
        }
        (self.telemetry_packets_lost as f32 / total as f32) * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_counters_are_zero() {
        let counters = AtomicCounters::new();
        let snapshot = counters.snapshot();
        assert_eq!(snapshot.total_ticks, 0);
        assert_eq!(snapshot.missed_ticks, 0);
        assert_eq!(snapshot.safety_events, 0);
        assert_eq!(snapshot.profile_switches, 0);
        assert_eq!(snapshot.telemetry_packets_received, 0);
        assert_eq!(snapshot.telemetry_packets_lost, 0);
        assert_eq!(snapshot.torque_saturation_samples, 0);
        assert_eq!(snapshot.torque_saturation_count, 0);
        assert_eq!(snapshot.hid_write_errors, 0);
    }

    #[test]
    fn test_inc_tick() {
        let counters = AtomicCounters::new();
        counters.inc_tick();
        counters.inc_tick();
        counters.inc_tick();
        assert_eq!(counters.total_ticks(), 3);
    }

    #[test]
    fn test_inc_tick_by() {
        let counters = AtomicCounters::new();
        counters.inc_tick_by(10);
        assert_eq!(counters.total_ticks(), 10);
    }

    #[test]
    fn test_inc_missed_tick() {
        let counters = AtomicCounters::new();
        counters.inc_missed_tick();
        assert_eq!(counters.missed_ticks(), 1);
    }

    #[test]
    fn test_record_torque_saturation() {
        let counters = AtomicCounters::new();
        counters.record_torque_saturation(true);
        counters.record_torque_saturation(false);
        counters.record_torque_saturation(true);

        let snapshot = counters.snapshot();
        assert_eq!(snapshot.torque_saturation_samples, 3);
        assert_eq!(snapshot.torque_saturation_count, 2);
    }

    #[test]
    fn test_snapshot_and_reset() {
        let counters = AtomicCounters::new();
        counters.inc_tick();
        counters.inc_missed_tick();
        counters.inc_safety_event();

        let snapshot = counters.snapshot_and_reset();
        assert_eq!(snapshot.total_ticks, 1);
        assert_eq!(snapshot.missed_ticks, 1);
        assert_eq!(snapshot.safety_events, 1);

        let after = counters.snapshot();
        assert_eq!(after.total_ticks, 0);
        assert_eq!(after.missed_ticks, 0);
        assert_eq!(after.safety_events, 0);
    }

    #[test]
    fn test_torque_saturation_percent() {
        let counters = AtomicCounters::new();
        assert_eq!(counters.torque_saturation_percent(), 0.0);

        counters.record_torque_saturation(true);
        counters.record_torque_saturation(true);
        counters.record_torque_saturation(false);
        counters.record_torque_saturation(false);

        let pct = counters.torque_saturation_percent();
        assert!((pct - 50.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_telemetry_loss_percent() {
        let counters = AtomicCounters::new();
        assert_eq!(counters.telemetry_loss_percent(), 0.0);

        for _ in 0..90 {
            counters.inc_telemetry_received();
        }
        for _ in 0..10 {
            counters.inc_telemetry_lost();
        }

        let pct = counters.telemetry_loss_percent();
        assert!((pct - 10.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_with_values() {
        let initial = CounterSnapshot {
            total_ticks: 100,
            missed_ticks: 5,
            safety_events: 2,
            profile_switches: 3,
            telemetry_packets_received: 1000,
            telemetry_packets_lost: 50,
            torque_saturation_samples: 500,
            torque_saturation_count: 25,
            hid_write_errors: 0,
        };

        let counters = AtomicCounters::with_values(initial);
        let snapshot = counters.snapshot();

        assert_eq!(snapshot.total_ticks, 100);
        assert_eq!(snapshot.missed_ticks, 5);
        assert_eq!(snapshot.safety_events, 2);
    }
}

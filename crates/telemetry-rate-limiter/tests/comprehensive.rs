#![allow(clippy::redundant_closure)]
//! Comprehensive tests for the telemetry rate limiter.
//!
//! Tests rate limiting accuracy, burst handling, and proptest invariants
//! ensuring the limiter never exceeds its configured rate and applies
//! correct backpressure.

use std::time::{Duration, Instant};

use proptest::prelude::*;
use racing_wheel_telemetry_rate_limiter::{
    AdaptiveRateLimiter, RateLimiter, RateLimiterStats,
};

// ---------------------------------------------------------------------------
// 1. Rate limiting accuracy
// ---------------------------------------------------------------------------

#[test]
fn accuracy_first_call_always_accepted_across_rates() {
    for rate in [1, 10, 30, 60, 120, 240, 360, 500, 1000, 10_000] {
        let mut limiter = RateLimiter::new(rate);
        assert!(
            limiter.should_process(),
            "first call must be accepted for rate {rate}"
        );
    }
}

#[test]
fn accuracy_immediate_second_call_rejected_across_rates() {
    for rate in [1, 10, 60, 120, 360, 1000] {
        let mut limiter = RateLimiter::new(rate);
        assert!(limiter.should_process());
        assert!(
            !limiter.should_process(),
            "immediate second call must be rejected for rate {rate}"
        );
    }
}

#[test]
fn accuracy_60hz_accepts_after_interval() {
    let mut limiter = RateLimiter::new(60);
    assert!(limiter.should_process());
    // Wait more than 1/60s ≈ 17ms
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(18) {
        std::hint::spin_loop();
    }
    assert!(
        limiter.should_process(),
        "should accept after waiting >16.7ms for 60Hz"
    );
    assert_eq!(limiter.processed_count(), 2);
}

#[test]
fn accuracy_u32_max_rate_accepts_all() {
    let mut limiter = RateLimiter::new(u32::MAX);
    for _ in 0..50 {
        assert!(limiter.should_process());
    }
    assert_eq!(limiter.dropped_count(), 0);
    assert_eq!(limiter.processed_count(), 50);
}

#[test]
fn accuracy_stats_from_snapshot() {
    let mut limiter = RateLimiter::new(60);
    assert!(limiter.should_process());
    for _ in 0..9 {
        assert!(!limiter.should_process());
    }
    let stats = RateLimiterStats::from(&limiter);
    assert_eq!(stats.max_rate_hz, 60);
    assert_eq!(stats.processed_count, 1);
    assert_eq!(stats.dropped_count, 9);
    assert!((stats.drop_rate_percent - 90.0).abs() < f32::EPSILON);
}

#[tokio::test]
async fn accuracy_async_slot_spacing_100hz() {
    let mut limiter = RateLimiter::new(100);
    limiter.wait_for_slot().await;
    let before = Instant::now();
    limiter.wait_for_slot().await;
    let elapsed = before.elapsed();
    // 100 Hz → ~10ms interval; allow OS slack
    assert!(
        elapsed >= Duration::from_millis(8),
        "100Hz interval too short: {elapsed:?}"
    );
}

// ---------------------------------------------------------------------------
// 2. Burst handling
// ---------------------------------------------------------------------------

#[test]
fn burst_1000_calls_only_first_accepted() {
    let mut limiter = RateLimiter::new(60);
    let mut accepted = 0u64;
    for _ in 0..1000 {
        if limiter.should_process() {
            accepted += 1;
        }
    }
    assert_eq!(accepted, 1, "only first call should pass in a burst");
    assert_eq!(limiter.processed_count(), 1);
    assert_eq!(limiter.dropped_count(), 999);
}

#[test]
fn burst_drop_rate_is_high() {
    let mut limiter = RateLimiter::new(10);
    assert!(limiter.should_process());
    for _ in 0..9999 {
        let _ = limiter.should_process();
    }
    assert!(limiter.drop_rate_percent() > 99.0);
}

#[test]
fn burst_separated_by_interval_all_accepted() {
    let mut limiter = RateLimiter::new(100);
    let interval = Duration::from_millis(11); // > 10ms for 100Hz
    let mut accepted = 0u64;
    for _ in 0..5 {
        if limiter.should_process() {
            accepted += 1;
        }
        let start = Instant::now();
        while start.elapsed() < interval {
            std::hint::spin_loop();
        }
    }
    assert_eq!(accepted, 5);
    assert_eq!(limiter.dropped_count(), 0);
}

#[test]
fn burst_interleaved_with_waits() {
    let mut limiter = RateLimiter::new(100);
    // Burst of 5
    assert!(limiter.should_process());
    for _ in 0..4 {
        assert!(!limiter.should_process());
    }
    // Wait for interval
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(12) {
        std::hint::spin_loop();
    }
    // Should accept again
    assert!(limiter.should_process());
    assert_eq!(limiter.processed_count(), 2);
}

// ---------------------------------------------------------------------------
// 3. Proptest: rate limit invariants
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// First call is ALWAYS accepted for any positive rate.
    #[test]
    fn prop_first_always_accepted(rate in 1u32..=1_000_000u32) {
        let mut limiter = RateLimiter::new(rate);
        prop_assert!(limiter.should_process());
    }

    /// Immediate second call is ALWAYS rejected for rates that produce
    /// a non-zero interval (i.e., rate < ~1 billion).
    #[test]
    fn prop_second_immediate_rejected(rate in 1u32..=1_000_000u32) {
        let mut limiter = RateLimiter::new(rate);
        let _ = limiter.should_process();
        prop_assert!(!limiter.should_process());
    }

    /// processed + dropped == total calls (conservation invariant).
    #[test]
    fn prop_conservation_invariant(rate in 1u32..=10_000u32, burst in 1u32..=500u32) {
        let mut limiter = RateLimiter::new(rate);
        for _ in 0..burst {
            let _ = limiter.should_process();
        }
        prop_assert_eq!(
            limiter.processed_count() + limiter.dropped_count(),
            u64::from(burst),
        );
    }

    /// Drop rate is always in [0, 100].
    #[test]
    fn prop_drop_rate_bounded(rate in 1u32..=100_000u32, burst in 1u32..=500u32) {
        let mut limiter = RateLimiter::new(rate);
        for _ in 0..burst {
            let _ = limiter.should_process();
        }
        let pct = limiter.drop_rate_percent();
        prop_assert!((0.0..=100.0).contains(&pct),
            "drop_rate_percent={pct} out of bounds");
    }

    /// set_max_rate_hz round-trips for positive values.
    #[test]
    fn prop_set_rate_roundtrip(rate in 1u32..=100_000u32) {
        let mut limiter = RateLimiter::new(1);
        limiter.set_max_rate_hz(rate);
        prop_assert_eq!(limiter.max_rate_hz(), rate);
    }

    /// set_max_rate_hz(0) clamps to 1.
    #[test]
    fn prop_zero_rate_clamps(rate in 0u32..=1u32) {
        let mut limiter = RateLimiter::new(100);
        limiter.set_max_rate_hz(rate);
        prop_assert!(limiter.max_rate_hz() >= 1);
    }

    /// reset_stats zeroes all counters.
    #[test]
    fn prop_reset_zeroes_counters(rate in 1u32..=10_000u32, burst in 1u32..=200u32) {
        let mut limiter = RateLimiter::new(rate);
        for _ in 0..burst {
            let _ = limiter.should_process();
        }
        limiter.reset_stats();
        prop_assert_eq!(limiter.processed_count(), 0);
        prop_assert_eq!(limiter.dropped_count(), 0);
        prop_assert_eq!(limiter.drop_rate_percent(), 0.0);
    }

    /// Stats snapshot is consistent with limiter state.
    #[test]
    fn prop_stats_consistent(rate in 1u32..=10_000u32, burst in 1u32..=200u32) {
        let mut limiter = RateLimiter::new(rate);
        for _ in 0..burst {
            let _ = limiter.should_process();
        }
        let stats = RateLimiterStats::from(&limiter);
        prop_assert_eq!(stats.max_rate_hz, limiter.max_rate_hz());
        prop_assert_eq!(stats.processed_count, limiter.processed_count());
        prop_assert_eq!(stats.dropped_count, limiter.dropped_count());
        let expected_pct = limiter.drop_rate_percent();
        prop_assert!((stats.drop_rate_percent - expected_pct).abs() < f32::EPSILON);
    }

    /// Adaptive rate never drops to zero.
    #[test]
    fn prop_adaptive_never_zero(
        initial in 1u32..=10_000u32,
        target_cpu in 1.0f32..=99.0f32,
        observed_cpu in 0.0f32..=100.0f32,
        iters in 1u32..=100u32,
    ) {
        let mut adaptive = AdaptiveRateLimiter::new(initial, target_cpu);
        for _ in 0..iters {
            adaptive.update_cpu_usage(observed_cpu);
        }
        prop_assert!(adaptive.stats().max_rate_hz >= 1);
    }

    /// Adaptive limiter: processed + dropped == total calls.
    #[test]
    fn prop_adaptive_conservation(
        initial in 1u32..=1_000u32,
        burst in 1u32..=200u32,
    ) {
        let mut adaptive = AdaptiveRateLimiter::new(initial, 50.0);
        for _ in 0..burst {
            let _ = adaptive.should_process();
        }
        let stats = adaptive.stats();
        prop_assert_eq!(
            stats.processed_count + stats.dropped_count,
            u64::from(burst),
        );
    }
}

// ---------------------------------------------------------------------------
// 4. Backpressure: rate limiter never exceeds configured limit
// ---------------------------------------------------------------------------

#[test]
fn backpressure_at_60hz_respects_interval() {
    let mut limiter = RateLimiter::new(60);
    let start = Instant::now();
    let mut accepted = 0u64;
    // Tight loop for 100ms
    while start.elapsed() < Duration::from_millis(100) {
        if limiter.should_process() {
            accepted += 1;
        }
    }
    // At 60Hz, max ~6 frames in 100ms (first is immediate)
    assert!(
        accepted <= 10,
        "accepted {accepted} frames in 100ms at 60Hz, expected <=10"
    );
    assert!(accepted >= 1, "should have accepted at least 1 frame");
}

#[test]
fn backpressure_reconfigure_enforced() {
    let mut limiter = RateLimiter::new(1000);
    assert!(limiter.should_process());
    // Switch to 1 Hz
    limiter.set_max_rate_hz(1);
    // Tight loop: should not accept any more immediately
    let mut accepted_after = 0u64;
    for _ in 0..1000 {
        if limiter.should_process() {
            accepted_after += 1;
        }
    }
    assert_eq!(accepted_after, 0, "should not accept after switching to 1Hz");
}

#[test]
fn backpressure_adaptive_high_cpu_reduces_throughput() {
    let mut adaptive = AdaptiveRateLimiter::new(1000, 50.0);
    let initial_rate = adaptive.stats().max_rate_hz;
    for _ in 0..50 {
        adaptive.update_cpu_usage(95.0);
    }
    let reduced_rate = adaptive.stats().max_rate_hz;
    assert!(
        reduced_rate < initial_rate,
        "high CPU should reduce rate: initial={initial_rate}, reduced={reduced_rate}"
    );
}

#[test]
fn backpressure_adaptive_low_cpu_increases_throughput() {
    let mut adaptive = AdaptiveRateLimiter::new(500, 50.0);
    // First decrease
    for _ in 0..30 {
        adaptive.update_cpu_usage(90.0);
    }
    let low = adaptive.stats().max_rate_hz;
    // Then recover
    for _ in 0..60 {
        adaptive.update_cpu_usage(5.0);
    }
    let recovered = adaptive.stats().max_rate_hz;
    assert!(
        recovered > low,
        "low CPU should recover rate: low={low}, recovered={recovered}"
    );
}

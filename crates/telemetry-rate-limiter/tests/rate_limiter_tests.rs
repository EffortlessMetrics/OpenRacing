//! Integration tests for the telemetry-rate-limiter crate.
//!
//! Covers rate limiting at various frequencies, burst handling, timing
//! precision, edge cases, frame dropping behaviour, adaptive limiting,
//! and statistics reporting.

use std::time::{Duration, Instant};

use racing_wheel_telemetry_rate_limiter::{AdaptiveRateLimiter, RateLimiter, RateLimiterStats};

// ---------------------------------------------------------------------------
// 1. Rate limiting at various frequencies
// ---------------------------------------------------------------------------

#[test]
fn rate_60hz_min_interval_is_roughly_16ms() {
    let limiter = RateLimiter::new(60);
    assert_eq!(limiter.max_rate_hz(), 60);
    // First call always accepted
    let mut lim = limiter;
    assert!(lim.should_process());
    // Immediate second call rejected
    assert!(!lim.should_process());
}

#[test]
fn rate_120hz_min_interval_is_roughly_8ms() {
    let limiter = RateLimiter::new(120);
    assert_eq!(limiter.max_rate_hz(), 120);
    let mut lim = limiter;
    assert!(lim.should_process());
    assert!(!lim.should_process());
}

#[test]
fn rate_360hz_min_interval_is_roughly_2ms() {
    let limiter = RateLimiter::new(360);
    assert_eq!(limiter.max_rate_hz(), 360);
    let mut lim = limiter;
    assert!(lim.should_process());
    assert!(!lim.should_process());
}

#[test]
fn various_rates_first_always_accepted() {
    for rate in [1, 10, 30, 60, 120, 240, 360, 500, 1000] {
        let mut limiter = RateLimiter::new(rate);
        assert!(
            limiter.should_process(),
            "first call must be accepted for rate {rate}"
        );
    }
}

#[test]
fn rate_1hz_allows_after_one_second() {
    let mut limiter = RateLimiter::new(1);
    assert!(limiter.should_process());
    // Spin-wait at least 1 second
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(1050) {
        std::hint::spin_loop();
    }
    assert!(limiter.should_process());
    assert_eq!(limiter.processed_count(), 2);
}

// ---------------------------------------------------------------------------
// 2. Burst handling
// ---------------------------------------------------------------------------

#[test]
fn burst_of_calls_only_first_accepted() {
    let mut limiter = RateLimiter::new(60);
    assert!(limiter.should_process());
    for _ in 0..100 {
        assert!(!limiter.should_process());
    }
    assert_eq!(limiter.processed_count(), 1);
    assert_eq!(limiter.dropped_count(), 100);
}

#[test]
fn burst_drop_rate_approaches_100_percent() {
    let mut limiter = RateLimiter::new(60);
    assert!(limiter.should_process());
    for _ in 0..999 {
        let _ = limiter.should_process();
    }
    // 1 processed, 999 dropped → 99.9%
    assert!(limiter.drop_rate_percent() > 99.0);
}

#[test]
fn repeated_bursts_separated_by_interval_all_accepted() {
    let mut limiter = RateLimiter::new(100);
    let interval = Duration::from_millis(11); // >10ms for 100 Hz
    for _ in 0..5 {
        assert!(limiter.should_process());
        let start = Instant::now();
        while start.elapsed() < interval {
            std::hint::spin_loop();
        }
    }
    assert_eq!(limiter.processed_count(), 5);
    assert_eq!(limiter.dropped_count(), 0);
}

// ---------------------------------------------------------------------------
// 3. Timing precision
// ---------------------------------------------------------------------------

#[tokio::test]
async fn async_slot_respects_min_interval_at_60hz() {
    let mut limiter = RateLimiter::new(60);
    limiter.wait_for_slot().await;
    let before = Instant::now();
    limiter.wait_for_slot().await;
    let elapsed = before.elapsed();
    // 60 Hz → ~16.67ms interval; allow some OS scheduling slack
    assert!(
        elapsed >= Duration::from_millis(14),
        "elapsed {elapsed:?} too short for 60 Hz"
    );
    assert_eq!(limiter.processed_count(), 2);
}

#[tokio::test]
async fn async_slot_respects_min_interval_at_120hz() {
    let mut limiter = RateLimiter::new(120);
    limiter.wait_for_slot().await;
    let before = Instant::now();
    limiter.wait_for_slot().await;
    let elapsed = before.elapsed();
    // 120 Hz → ~8.33ms
    assert!(
        elapsed >= Duration::from_millis(6),
        "elapsed {elapsed:?} too short for 120 Hz"
    );
}

#[tokio::test]
async fn async_slot_processes_multiple_sequential() {
    let mut limiter = RateLimiter::new(200);
    let start = Instant::now();
    for _ in 0..5 {
        limiter.wait_for_slot().await;
    }
    let total = start.elapsed();
    // 4 waits of ~5ms each ≥ 16ms
    assert!(
        total >= Duration::from_millis(16),
        "5 slots at 200 Hz should take ≥16ms, took {total:?}"
    );
    assert_eq!(limiter.processed_count(), 5);
}

// ---------------------------------------------------------------------------
// 4. Edge cases
// ---------------------------------------------------------------------------

#[test]
fn zero_rate_clamps_to_one_hz() {
    let mut limiter = RateLimiter::new(0);
    // Constructor clamps divisor to 1 → min_interval = 1s
    assert!(limiter.should_process());
    assert!(!limiter.should_process());
}

#[test]
fn very_high_rate_does_not_panic() {
    let mut limiter = RateLimiter::new(u32::MAX);
    assert!(limiter.should_process());
    // At u32::MAX Hz the min_interval is ~0ns, so the second call may pass
    // We only care that it doesn't panic
    let _ = limiter.should_process();
}

#[test]
fn u32_max_rate_has_near_zero_interval() {
    let mut limiter = RateLimiter::new(u32::MAX);
    assert!(limiter.should_process());
    // min_interval ≈ 0ns, so immediate second call should succeed
    assert!(limiter.should_process());
    assert_eq!(limiter.processed_count(), 2);
    assert_eq!(limiter.dropped_count(), 0);
}

#[test]
fn set_max_rate_hz_zero_clamps_to_one() {
    let mut limiter = RateLimiter::new(100);
    limiter.set_max_rate_hz(0);
    assert_eq!(limiter.max_rate_hz(), 1);
}

#[test]
fn set_max_rate_hz_updates_interval() {
    let mut limiter = RateLimiter::new(100);
    assert!(limiter.should_process());
    // Immediately switch to 1 Hz – next call should be rejected
    limiter.set_max_rate_hz(1);
    assert!(!limiter.should_process());
}

#[test]
fn set_max_rate_hz_to_high_value_allows_immediate() {
    let mut limiter = RateLimiter::new(1);
    assert!(limiter.should_process());
    // Switch to very high rate
    limiter.set_max_rate_hz(u32::MAX);
    assert!(limiter.should_process());
}

// ---------------------------------------------------------------------------
// 5. Frame dropping behaviour when over budget
// ---------------------------------------------------------------------------

#[test]
fn continuous_calls_all_dropped_except_first() {
    let mut limiter = RateLimiter::new(10);
    let mut processed = 0u64;
    let mut dropped = 0u64;
    for _ in 0..1000 {
        if limiter.should_process() {
            processed += 1;
        } else {
            dropped += 1;
        }
    }
    assert_eq!(processed, 1);
    assert_eq!(dropped, 999);
    assert_eq!(limiter.processed_count(), processed);
    assert_eq!(limiter.dropped_count(), dropped);
}

#[test]
fn drop_rate_is_zero_when_all_frames_processed() {
    let mut limiter = RateLimiter::new(100);
    let interval = Duration::from_millis(11);
    for _ in 0..3 {
        assert!(limiter.should_process());
        let start = Instant::now();
        while start.elapsed() < interval {
            std::hint::spin_loop();
        }
    }
    assert_eq!(limiter.drop_rate_percent(), 0.0);
}

#[test]
fn drop_rate_percent_bounded_between_0_and_100() {
    let mut limiter = RateLimiter::new(10);
    assert!(limiter.should_process());
    for _ in 0..500 {
        let _ = limiter.should_process();
    }
    let rate = limiter.drop_rate_percent();
    assert!((0.0..=100.0).contains(&rate));
}

#[test]
fn drop_rate_is_zero_with_no_events() {
    let limiter = RateLimiter::new(100);
    assert_eq!(limiter.drop_rate_percent(), 0.0);
}

// ---------------------------------------------------------------------------
// 6. Statistics and reset
// ---------------------------------------------------------------------------

#[test]
fn reset_stats_clears_counters_but_preserves_rate() {
    let mut limiter = RateLimiter::new(60);
    assert!(limiter.should_process());
    assert!(!limiter.should_process());
    limiter.reset_stats();
    assert_eq!(limiter.processed_count(), 0);
    assert_eq!(limiter.dropped_count(), 0);
    assert_eq!(limiter.max_rate_hz(), 60);
}

#[test]
fn stats_snapshot_matches_limiter_state() {
    let mut limiter = RateLimiter::new(120);
    assert!(limiter.should_process());
    for _ in 0..4 {
        let _ = limiter.should_process();
    }
    let stats = RateLimiterStats::from(&limiter);
    assert_eq!(stats.max_rate_hz, 120);
    assert_eq!(stats.processed_count, 1);
    assert_eq!(stats.dropped_count, 4);
    assert!((stats.drop_rate_percent - 80.0).abs() < f32::EPSILON);
}

#[test]
fn stats_clone_is_independent() {
    let mut limiter = RateLimiter::new(60);
    assert!(limiter.should_process());
    let stats = RateLimiterStats::from(&limiter);
    let cloned = stats.clone();
    // Mutate original limiter – clone must remain unchanged
    assert!(limiter.should_process() || !limiter.should_process());
    assert_eq!(cloned.processed_count, 1);
}

#[test]
fn stats_debug_format_is_not_empty() {
    let limiter = RateLimiter::new(100);
    let stats = RateLimiterStats::from(&limiter);
    let debug = format!("{stats:?}");
    assert!(!debug.is_empty());
}

// ---------------------------------------------------------------------------
// 7. Adaptive rate limiter
// ---------------------------------------------------------------------------

#[test]
fn adaptive_reduces_rate_under_high_cpu() {
    let mut adaptive = AdaptiveRateLimiter::new(1000, 50.0);
    let initial = adaptive.stats().max_rate_hz;
    for _ in 0..20 {
        adaptive.update_cpu_usage(90.0);
    }
    let reduced = adaptive.stats().max_rate_hz;
    assert!(
        reduced < initial,
        "rate should decrease: initial={initial}, after={reduced}"
    );
}

#[test]
fn adaptive_increases_rate_under_low_cpu() {
    let mut adaptive = AdaptiveRateLimiter::new(500, 50.0);
    // First push down so there's room to grow
    for _ in 0..20 {
        adaptive.update_cpu_usage(90.0);
    }
    let low = adaptive.stats().max_rate_hz;
    for _ in 0..40 {
        adaptive.update_cpu_usage(10.0);
    }
    let recovered = adaptive.stats().max_rate_hz;
    assert!(
        recovered > low,
        "rate should recover: low={low}, recovered={recovered}"
    );
}

#[test]
fn adaptive_clamps_factor_at_lower_bound() {
    let mut adaptive = AdaptiveRateLimiter::new(100, 50.0);
    for _ in 0..500 {
        adaptive.update_cpu_usage(100.0);
    }
    let stats = adaptive.stats();
    // Factor clamped to 0.1 → rate ≥ 10 (100 * 0.1), clamped to ≥ 1
    assert!(stats.max_rate_hz >= 1);
}

#[test]
fn adaptive_clamps_factor_at_upper_bound() {
    let mut adaptive = AdaptiveRateLimiter::new(100, 50.0);
    for _ in 0..500 {
        adaptive.update_cpu_usage(0.0);
    }
    let stats = adaptive.stats();
    // Factor clamped to 2.0 → rate ≤ 200
    assert!(stats.max_rate_hz <= 200);
}

#[test]
fn adaptive_should_process_delegates_correctly() {
    let mut adaptive = AdaptiveRateLimiter::new(60, 50.0);
    assert!(adaptive.should_process());
    assert!(!adaptive.should_process());
}

#[tokio::test]
async fn adaptive_wait_for_slot_delegates_correctly() {
    let mut adaptive = AdaptiveRateLimiter::new(100, 50.0);
    let start = Instant::now();
    adaptive.wait_for_slot().await;
    adaptive.wait_for_slot().await;
    let elapsed = start.elapsed();
    assert!(
        elapsed >= Duration::from_millis(8),
        "adaptive wait_for_slot should respect interval"
    );
}

#[test]
fn adaptive_at_target_cpu_does_not_change_rate() {
    let mut adaptive = AdaptiveRateLimiter::new(100, 50.0);
    // CPU exactly at target – factor stays at 1.0
    adaptive.update_cpu_usage(50.0);
    assert_eq!(adaptive.stats().max_rate_hz, 100);
}

#[test]
fn adaptive_cpu_in_hysteresis_band_does_not_increase() {
    let mut adaptive = AdaptiveRateLimiter::new(100, 50.0);
    // CPU at 45% is above 50% * 0.8 = 40% → neither branch fires
    adaptive.update_cpu_usage(45.0);
    assert_eq!(adaptive.stats().max_rate_hz, 100);
}

// ---------------------------------------------------------------------------
// 8. Property-style coverage via parameterised loops
// ---------------------------------------------------------------------------

#[test]
fn all_rates_first_accepted_second_immediate_rejected() {
    for rate in (1..=1000).step_by(17) {
        let mut limiter = RateLimiter::new(rate);
        assert!(
            limiter.should_process(),
            "first call must be accepted for rate {rate}"
        );
        assert!(
            !limiter.should_process(),
            "immediate second call must be rejected for rate {rate}"
        );
    }
}

#[test]
fn set_max_rate_hz_roundtrips() {
    for rate in [1, 10, 60, 120, 360, 1000, 100_000] {
        let mut limiter = RateLimiter::new(1);
        limiter.set_max_rate_hz(rate);
        assert_eq!(limiter.max_rate_hz(), rate, "roundtrip failed for {rate}");
    }
}

// ---------------------------------------------------------------------------
// 9. Reconfiguration mid-stream
// ---------------------------------------------------------------------------

#[test]
fn reconfigure_slower_rejects_immediate() {
    let mut limiter = RateLimiter::new(1000);
    assert!(limiter.should_process());
    limiter.set_max_rate_hz(1);
    // 1 Hz → 1s interval; immediate call rejected
    assert!(!limiter.should_process());
}

#[test]
fn reconfigure_faster_after_slow() {
    let mut limiter = RateLimiter::new(1); // 1 Hz
    assert!(limiter.should_process());
    limiter.set_max_rate_hz(u32::MAX);
    // Near-zero interval → immediate call accepted
    assert!(limiter.should_process());
}

#[test]
fn reconfigure_preserves_counters() {
    let mut limiter = RateLimiter::new(60);
    assert!(limiter.should_process());
    assert!(!limiter.should_process());
    let p_before = limiter.processed_count();
    let d_before = limiter.dropped_count();
    limiter.set_max_rate_hz(120);
    assert_eq!(limiter.processed_count(), p_before);
    assert_eq!(limiter.dropped_count(), d_before);
}

#[test]
fn repeated_reconfigurations_stable() {
    let mut limiter = RateLimiter::new(60);
    for rate in [1, 10, 100, 1000, u32::MAX, 1, 60, 0] {
        limiter.set_max_rate_hz(rate);
        assert!(
            limiter.max_rate_hz() >= 1,
            "rate must be >= 1 after set_max_rate_hz({rate})"
        );
    }
}

// ---------------------------------------------------------------------------
// 10. Boundary values
// ---------------------------------------------------------------------------

#[test]
fn boundary_power_of_two_rates() {
    for rate in [1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024] {
        let mut limiter = RateLimiter::new(rate);
        assert!(
            limiter.should_process(),
            "first call accepted for rate {rate}"
        );
        assert!(
            !limiter.should_process(),
            "immediate second rejected for rate {rate}"
        );
    }
}

#[test]
fn boundary_u32_max_no_drops() {
    let mut limiter = RateLimiter::new(u32::MAX);
    for _ in 0..100 {
        assert!(limiter.should_process());
    }
    assert_eq!(limiter.dropped_count(), 0);
    assert_eq!(limiter.processed_count(), 100);
}

#[test]
fn boundary_processed_plus_dropped_invariant() {
    let mut limiter = RateLimiter::new(10);
    let total_calls = 500u64;
    for _ in 0..total_calls {
        let _ = limiter.should_process();
    }
    assert_eq!(
        limiter.processed_count() + limiter.dropped_count(),
        total_calls
    );
}

// ---------------------------------------------------------------------------
// 11. Reset and statistics edge cases
// ---------------------------------------------------------------------------

#[test]
fn reset_then_rate_still_enforced() {
    let mut limiter = RateLimiter::new(60);
    assert!(limiter.should_process());
    limiter.reset_stats();
    // Timing state is preserved, so immediate call is still rejected
    assert!(!limiter.should_process());
    assert_eq!(limiter.dropped_count(), 1);
}

#[test]
fn consecutive_resets_are_idempotent() {
    let mut limiter = RateLimiter::new(100);
    assert!(limiter.should_process());
    limiter.reset_stats();
    limiter.reset_stats();
    limiter.reset_stats();
    assert_eq!(limiter.processed_count(), 0);
    assert_eq!(limiter.dropped_count(), 0);
}

#[test]
fn stats_independent_of_future_mutations() {
    let mut limiter = RateLimiter::new(100);
    assert!(limiter.should_process());
    let snap1 = RateLimiterStats::from(&limiter);
    // Mutate limiter after snapshot
    assert!(!limiter.should_process());
    assert!(!limiter.should_process());
    // Snapshot must be unchanged
    assert_eq!(snap1.processed_count, 1);
    assert_eq!(snap1.dropped_count, 0);
}

// ---------------------------------------------------------------------------
// 12. Adaptive rate limiter: extended scenarios
// ---------------------------------------------------------------------------

#[test]
fn adaptive_converges_under_constant_high_cpu() {
    let mut adaptive = AdaptiveRateLimiter::new(1000, 50.0);
    let mut prev = adaptive.stats().max_rate_hz;
    for _ in 0..200 {
        adaptive.update_cpu_usage(90.0);
        let curr = adaptive.stats().max_rate_hz;
        assert!(curr <= prev || curr == prev);
        prev = curr;
    }
    // Should have settled at the lower clamp
    assert!(adaptive.stats().max_rate_hz >= 1);
}

#[test]
fn adaptive_converges_under_constant_low_cpu() {
    let mut adaptive = AdaptiveRateLimiter::new(500, 50.0);
    let mut prev = adaptive.stats().max_rate_hz;
    for _ in 0..200 {
        adaptive.update_cpu_usage(0.0);
        let curr = adaptive.stats().max_rate_hz;
        assert!(curr >= prev || curr == prev);
        prev = curr;
    }
    // Should have settled at the upper clamp
    assert!(adaptive.stats().max_rate_hz <= 1000);
}

#[test]
fn adaptive_stats_track_processing() {
    let mut adaptive = AdaptiveRateLimiter::new(100, 50.0);
    assert!(adaptive.should_process());
    assert!(!adaptive.should_process());
    let stats = adaptive.stats();
    assert_eq!(stats.processed_count, 1);
    assert_eq!(stats.dropped_count, 1);
    assert!((stats.drop_rate_percent - 50.0).abs() < f32::EPSILON);
}

// ---------------------------------------------------------------------------
// 13. Timer precision edge cases
// ---------------------------------------------------------------------------

#[tokio::test]
async fn async_first_slot_is_nearly_immediate() {
    let mut limiter = RateLimiter::new(60);
    let start = Instant::now();
    limiter.wait_for_slot().await;
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_millis(10),
        "first slot should be immediate, took {elapsed:?}"
    );
}

#[tokio::test]
async fn async_slot_at_u32_max_rate_is_fast() {
    let mut limiter = RateLimiter::new(u32::MAX);
    limiter.wait_for_slot().await;
    let start = Instant::now();
    limiter.wait_for_slot().await;
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_millis(5),
        "u32::MAX rate should have near-zero interval, took {elapsed:?}"
    );
}

#[tokio::test]
async fn async_adaptive_wait_first_slot_immediate() {
    let mut adaptive = AdaptiveRateLimiter::new(60, 50.0);
    let start = Instant::now();
    adaptive.wait_for_slot().await;
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_millis(10),
        "first adaptive slot should be immediate, took {elapsed:?}"
    );
}

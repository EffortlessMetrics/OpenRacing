//! Deep tests for telemetry-rate-limiter: exact drop-rate arithmetic,
//! multi-cycle burst-rest patterns, adaptive limiter boundary conditions,
//! stats snapshot isolation, and combined processing + CPU update sequences.

use std::time::{Duration, Instant};

use racing_wheel_telemetry_rate_limiter::{
    AdaptiveRateLimiter, RateLimiter, RateLimiterStats,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ===========================================================================
// Exact drop-rate arithmetic
// ===========================================================================

#[test]
fn drop_rate_one_processed_one_dropped() -> TestResult {
    let mut limiter = RateLimiter::new(10);
    assert!(limiter.should_process()); // processed=1
    assert!(!limiter.should_process()); // dropped=1
    // 1/(1+1) * 100 = 50.0
    assert!((limiter.drop_rate_percent() - 50.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn drop_rate_one_processed_three_dropped() -> TestResult {
    let mut limiter = RateLimiter::new(10);
    assert!(limiter.should_process());
    for _ in 0..3 {
        assert!(!limiter.should_process());
    }
    // 3/(1+3) * 100 = 75.0
    assert!((limiter.drop_rate_percent() - 75.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn drop_rate_one_processed_nine_dropped() -> TestResult {
    let mut limiter = RateLimiter::new(10);
    assert!(limiter.should_process());
    for _ in 0..9 {
        assert!(!limiter.should_process());
    }
    // 9/(1+9) * 100 = 90.0
    assert!((limiter.drop_rate_percent() - 90.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn drop_rate_100_percent_not_reachable_with_first_accepted() -> TestResult {
    // The first call is always accepted, so drop rate < 100% in practice
    let mut limiter = RateLimiter::new(1);
    assert!(limiter.should_process());
    for _ in 0..999 {
        assert!(!limiter.should_process());
    }
    let rate = limiter.drop_rate_percent();
    assert!(rate < 100.0);
    assert!(rate > 99.0);
    Ok(())
}

// ===========================================================================
// Multi-cycle burst-rest patterns
// ===========================================================================

#[test]
fn burst_rest_burst_counters_accumulate() -> TestResult {
    let mut limiter = RateLimiter::new(100);

    // Cycle 1: burst
    assert!(limiter.should_process());
    for _ in 0..4 {
        assert!(!limiter.should_process());
    }
    assert_eq!(limiter.processed_count(), 1);
    assert_eq!(limiter.dropped_count(), 4);

    // Rest (wait for interval)
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(12) {
        std::hint::spin_loop();
    }

    // Cycle 2: burst
    assert!(limiter.should_process());
    for _ in 0..4 {
        assert!(!limiter.should_process());
    }
    assert_eq!(limiter.processed_count(), 2);
    assert_eq!(limiter.dropped_count(), 8);
    Ok(())
}

#[test]
fn burst_rest_reset_burst_fresh_counters() -> TestResult {
    let mut limiter = RateLimiter::new(100);
    assert!(limiter.should_process());
    assert!(!limiter.should_process());
    limiter.reset_stats();

    // Wait for interval
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(12) {
        std::hint::spin_loop();
    }

    // New burst with fresh counters
    assert!(limiter.should_process());
    assert!(!limiter.should_process());
    assert_eq!(limiter.processed_count(), 1);
    assert_eq!(limiter.dropped_count(), 1);
    Ok(())
}

#[test]
fn three_cycle_burst_rest_invariant() -> TestResult {
    let mut limiter = RateLimiter::new(100);
    let interval = Duration::from_millis(12);

    for cycle in 0..3u64 {
        assert!(
            limiter.should_process(),
            "cycle {cycle}: first call should be accepted"
        );
        assert!(
            !limiter.should_process(),
            "cycle {cycle}: immediate second should be rejected"
        );
        let start = Instant::now();
        while start.elapsed() < interval {
            std::hint::spin_loop();
        }
    }
    assert_eq!(limiter.processed_count(), 3);
    assert_eq!(limiter.dropped_count(), 3);
    assert_eq!(
        limiter.processed_count() + limiter.dropped_count(),
        6
    );
    Ok(())
}

// ===========================================================================
// Stats Debug format content
// ===========================================================================

#[test]
fn stats_debug_contains_field_names() -> TestResult {
    let mut limiter = RateLimiter::new(60);
    assert!(limiter.should_process());
    assert!(!limiter.should_process());
    let stats = RateLimiterStats::from(&limiter);
    let debug = format!("{stats:?}");
    assert!(debug.contains("max_rate_hz"));
    assert!(debug.contains("processed_count"));
    assert!(debug.contains("dropped_count"));
    assert!(debug.contains("drop_rate_percent"));
    Ok(())
}

#[test]
fn stats_debug_contains_values() -> TestResult {
    let mut limiter = RateLimiter::new(42);
    assert!(limiter.should_process());
    let stats = RateLimiterStats::from(&limiter);
    let debug = format!("{stats:?}");
    assert!(debug.contains("42")); // max_rate_hz
    Ok(())
}

// ===========================================================================
// RateLimiterStats clone independence
// ===========================================================================

#[test]
fn stats_clone_is_independent_from_further_limiter_mutations() -> TestResult {
    let mut limiter = RateLimiter::new(100);
    assert!(limiter.should_process());
    let stats = RateLimiterStats::from(&limiter);
    let cloned = stats.clone();

    // Mutate limiter after snapshot
    for _ in 0..10 {
        let _ = limiter.should_process();
    }

    // Clone must reflect the original snapshot values
    assert_eq!(cloned.max_rate_hz, 100);
    assert_eq!(cloned.processed_count, 1);
    assert_eq!(cloned.dropped_count, 0);
    assert_eq!(cloned.drop_rate_percent, 0.0);
    Ok(())
}

#[test]
fn stats_clone_field_equality() -> TestResult {
    let mut limiter = RateLimiter::new(250);
    assert!(limiter.should_process());
    for _ in 0..3 {
        let _ = limiter.should_process();
    }
    let stats = RateLimiterStats::from(&limiter);
    let cloned = stats.clone();
    assert_eq!(stats.max_rate_hz, cloned.max_rate_hz);
    assert_eq!(stats.processed_count, cloned.processed_count);
    assert_eq!(stats.dropped_count, cloned.dropped_count);
    assert!((stats.drop_rate_percent - cloned.drop_rate_percent).abs() < f32::EPSILON);
    Ok(())
}

// ===========================================================================
// Adaptive limiter: boundary CPU values
// ===========================================================================

#[test]
fn adaptive_cpu_exactly_at_target_no_change() -> TestResult {
    let mut adaptive = AdaptiveRateLimiter::new(1000, 50.0);
    // CPU == target → triggers the ">" branch (false), and "< target*0.8" → 50 < 40 is false
    // So neither branch fires, factor stays 1.0
    for _ in 0..50 {
        adaptive.update_cpu_usage(50.0);
    }
    assert_eq!(adaptive.stats().max_rate_hz, 1000);
    Ok(())
}

#[test]
fn adaptive_cpu_exactly_at_hysteresis_lower_bound() -> TestResult {
    let mut adaptive = AdaptiveRateLimiter::new(1000, 50.0);
    // target * 0.8 = 40.0. CPU at exactly 40.0 → "< 40.0" is false.
    // Also "> 50.0" is false. So factor stays 1.0.
    for _ in 0..50 {
        adaptive.update_cpu_usage(40.0);
    }
    assert_eq!(adaptive.stats().max_rate_hz, 1000);
    Ok(())
}

#[test]
fn adaptive_cpu_just_below_hysteresis_lower_bound() -> TestResult {
    let mut adaptive = AdaptiveRateLimiter::new(1000, 50.0);
    // target * 0.8 = 40.0. CPU at 39.9 → triggers increase (1.05× per iter)
    for _ in 0..10 {
        adaptive.update_cpu_usage(39.9);
    }
    assert!(adaptive.stats().max_rate_hz > 1000);
    Ok(())
}

#[test]
fn adaptive_cpu_just_above_target() -> TestResult {
    let mut adaptive = AdaptiveRateLimiter::new(1000, 50.0);
    // CPU at 50.1 → triggers decrease (0.95× per iter)
    for _ in 0..10 {
        adaptive.update_cpu_usage(50.1);
    }
    assert!(adaptive.stats().max_rate_hz < 1000);
    Ok(())
}

// ===========================================================================
// Adaptive limiter: combined processing + CPU updates
// ===========================================================================

#[test]
fn adaptive_process_then_adjust_then_process() -> TestResult {
    let mut adaptive = AdaptiveRateLimiter::new(100, 50.0);

    // Process some frames
    assert!(adaptive.should_process());
    assert!(!adaptive.should_process());

    // Adjust CPU down → rate increases
    for _ in 0..20 {
        adaptive.update_cpu_usage(10.0);
    }
    let stats_after_adjust = adaptive.stats();
    assert!(stats_after_adjust.max_rate_hz > 100);

    // Stats still reflect prior processing
    assert_eq!(stats_after_adjust.processed_count, 1);
    assert_eq!(stats_after_adjust.dropped_count, 1);
    Ok(())
}

#[test]
fn adaptive_interleaved_process_and_cpu_updates() -> TestResult {
    let mut adaptive = AdaptiveRateLimiter::new(100, 50.0);

    // Interleave: process → adjust → process → adjust
    assert!(adaptive.should_process());
    adaptive.update_cpu_usage(80.0); // decrease
    assert!(!adaptive.should_process()); // still rate-limited from first call
    adaptive.update_cpu_usage(80.0); // decrease more

    let stats = adaptive.stats();
    assert_eq!(stats.processed_count, 1);
    assert_eq!(stats.dropped_count, 1);
    // Rate should have decreased
    assert!(stats.max_rate_hz < 100);
    Ok(())
}

// ===========================================================================
// Adaptive limiter: factor clamping precision
// ===========================================================================

#[test]
fn adaptive_factor_lower_clamp_produces_tenth_of_initial_rate() -> TestResult {
    let initial = 1000u32;
    let mut adaptive = AdaptiveRateLimiter::new(initial, 50.0);
    // Drive factor to 0.1 with many high CPU updates
    for _ in 0..500 {
        adaptive.update_cpu_usage(100.0);
    }
    let stats = adaptive.stats();
    // 1000 * 0.1 = 100, clamped to >=1
    assert_eq!(stats.max_rate_hz, 100);
    Ok(())
}

#[test]
fn adaptive_factor_upper_clamp_produces_double_initial_rate() -> TestResult {
    let initial = 500u32;
    let mut adaptive = AdaptiveRateLimiter::new(initial, 50.0);
    // Drive factor to 2.0 with many low CPU updates
    for _ in 0..500 {
        adaptive.update_cpu_usage(0.0);
    }
    let stats = adaptive.stats();
    // 500 * 2.0 = 1000
    assert_eq!(stats.max_rate_hz, 1000);
    Ok(())
}

// ===========================================================================
// Adaptive limiter: oscillation convergence
// ===========================================================================

#[test]
fn adaptive_rapid_alternation_stays_bounded() -> TestResult {
    let mut adaptive = AdaptiveRateLimiter::new(500, 50.0);
    // Alternate every call between high and low CPU
    for i in 0..200 {
        let cpu = if i % 2 == 0 { 90.0 } else { 10.0 };
        adaptive.update_cpu_usage(cpu);
    }
    let stats = adaptive.stats();
    // 0.95 * 1.05 ≈ 0.9975, so factor slowly decreases
    // Rate should be somewhere between 0.1× and 2.0× initial
    assert!(stats.max_rate_hz >= 50); // 500 * 0.1
    assert!(stats.max_rate_hz <= 1000); // 500 * 2.0
    Ok(())
}

#[test]
fn adaptive_gradual_ramp_up() -> TestResult {
    let mut adaptive = AdaptiveRateLimiter::new(100, 50.0);
    // Gradually increase CPU from 0 to 100
    let mut prev_rate = adaptive.stats().max_rate_hz;
    let mut rate_ever_decreased = false;
    for i in 0..100 {
        let cpu = i as f32;
        adaptive.update_cpu_usage(cpu);
        let rate = adaptive.stats().max_rate_hz;
        if rate < prev_rate {
            rate_ever_decreased = true;
        }
        prev_rate = rate;
    }
    // At some point the rate must have decreased (when CPU crossed the target)
    assert!(rate_ever_decreased);
    Ok(())
}

// ===========================================================================
// set_max_rate_hz preserves last_processed timing
// ===========================================================================

#[test]
fn set_max_rate_preserves_last_processed_timestamp() -> TestResult {
    let mut limiter = RateLimiter::new(1); // 1 Hz = 1s interval
    assert!(limiter.should_process()); // sets last_processed

    // Reconfigure to 100 Hz = 10ms interval
    limiter.set_max_rate_hz(100);
    // The last_processed was set above, elapsed should be < 10ms
    // So this should still be rejected (timing preserved)
    assert!(!limiter.should_process());

    // Wait for the new 100 Hz interval
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(12) {
        std::hint::spin_loop();
    }
    assert!(limiter.should_process());
    Ok(())
}

// ===========================================================================
// Reset stats does NOT reset last_processed timing
// ===========================================================================

#[test]
fn reset_stats_preserves_timing_enforcement() -> TestResult {
    let mut limiter = RateLimiter::new(60);
    assert!(limiter.should_process());
    limiter.reset_stats();

    // Counters are zero
    assert_eq!(limiter.processed_count(), 0);
    assert_eq!(limiter.dropped_count(), 0);

    // But timing is still enforced (last_processed preserved)
    assert!(!limiter.should_process());
    assert_eq!(limiter.dropped_count(), 1);
    Ok(())
}

// ===========================================================================
// Constructor: new(0) stores max_rate_hz=0 but clamps divisor
// ===========================================================================

#[test]
fn constructor_zero_stores_zero_rate() -> TestResult {
    let limiter = RateLimiter::new(0);
    // The stored max_rate_hz is 0 (not clamped), but divisor was clamped
    assert_eq!(limiter.max_rate_hz(), 0);
    Ok(())
}

#[test]
fn constructor_zero_vs_set_max_rate_hz_zero_differ() -> TestResult {
    // new(0) stores 0 but set_max_rate_hz(0) clamps and stores 1
    let limiter_new = RateLimiter::new(0);
    let mut limiter_set = RateLimiter::new(100);
    limiter_set.set_max_rate_hz(0);
    assert_eq!(limiter_new.max_rate_hz(), 0);
    assert_eq!(limiter_set.max_rate_hz(), 1);
    Ok(())
}

// ===========================================================================
// Adaptive limiter: initial state
// ===========================================================================

#[test]
fn adaptive_initial_stats_match_initial_rate() -> TestResult {
    let adaptive = AdaptiveRateLimiter::new(500, 75.0);
    let stats = adaptive.stats();
    assert_eq!(stats.max_rate_hz, 500);
    assert_eq!(stats.processed_count, 0);
    assert_eq!(stats.dropped_count, 0);
    assert_eq!(stats.drop_rate_percent, 0.0);
    Ok(())
}

#[test]
fn adaptive_first_process_always_accepted() -> TestResult {
    for rate in [1, 10, 60, 100, 500, 1000] {
        let mut adaptive = AdaptiveRateLimiter::new(rate, 50.0);
        assert!(
            adaptive.should_process(),
            "first call should be accepted for initial rate {rate}"
        );
    }
    Ok(())
}

// ===========================================================================
// Adaptive limiter: target_cpu_percent edge values
// ===========================================================================

#[test]
fn adaptive_very_low_target_cpu() -> TestResult {
    let mut adaptive = AdaptiveRateLimiter::new(100, 1.0);
    // Even moderate CPU (5%) exceeds target → rate decreases
    for _ in 0..20 {
        adaptive.update_cpu_usage(5.0);
    }
    assert!(adaptive.stats().max_rate_hz < 100);
    Ok(())
}

#[test]
fn adaptive_very_high_target_cpu() -> TestResult {
    let mut adaptive = AdaptiveRateLimiter::new(100, 99.0);
    // Hysteresis lower = 99 * 0.8 = 79.2. CPU at 50% < 79.2 → rate increases
    for _ in 0..20 {
        adaptive.update_cpu_usage(50.0);
    }
    assert!(adaptive.stats().max_rate_hz > 100);
    Ok(())
}

// ===========================================================================
// Async: adaptive wait_for_slot
// ===========================================================================

#[tokio::test]
async fn adaptive_async_wait_first_slot_immediate() -> TestResult {
    let mut adaptive = AdaptiveRateLimiter::new(60, 50.0);
    let start = Instant::now();
    adaptive.wait_for_slot().await;
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_millis(10),
        "first adaptive slot should be immediate, took {elapsed:?}"
    );
    Ok(())
}

#[tokio::test]
async fn adaptive_async_wait_second_slot_respects_interval() -> TestResult {
    let mut adaptive = AdaptiveRateLimiter::new(100, 50.0);
    adaptive.wait_for_slot().await;
    let start = Instant::now();
    adaptive.wait_for_slot().await;
    let elapsed = start.elapsed();
    assert!(
        elapsed >= Duration::from_millis(8),
        "second adaptive slot should wait ~10ms at 100Hz, took {elapsed:?}"
    );
    Ok(())
}

// ===========================================================================
// Multiple reconfiguration cycles
// ===========================================================================

#[test]
fn reconfigure_cycle_slow_fast_slow() -> TestResult {
    let mut limiter = RateLimiter::new(1); // 1 Hz
    assert!(limiter.should_process());
    assert!(!limiter.should_process()); // rejected at 1 Hz

    // Speed up
    limiter.set_max_rate_hz(u32::MAX);
    assert!(limiter.should_process()); // accepted (near-zero interval)

    // Slow down again
    limiter.set_max_rate_hz(1);
    assert!(!limiter.should_process()); // rejected at 1 Hz

    assert_eq!(limiter.processed_count(), 2);
    Ok(())
}

#[test]
fn reconfigure_does_not_reset_counters() -> TestResult {
    let mut limiter = RateLimiter::new(60);
    assert!(limiter.should_process());
    assert!(!limiter.should_process());
    assert!(!limiter.should_process());

    limiter.set_max_rate_hz(120);
    assert_eq!(limiter.processed_count(), 1);
    assert_eq!(limiter.dropped_count(), 2);
    Ok(())
}

// ===========================================================================
// Stats: drop_rate_percent numerical consistency
// ===========================================================================

#[test]
fn drop_rate_consistent_with_manual_calculation() -> TestResult {
    let mut limiter = RateLimiter::new(10);
    assert!(limiter.should_process()); // processed=1
    for _ in 0..19 {
        assert!(!limiter.should_process()); // dropped=19
    }
    let expected = (19.0f32 / 20.0) * 100.0;
    assert!((limiter.drop_rate_percent() - expected).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn drop_rate_zero_all_processed() -> TestResult {
    let mut limiter = RateLimiter::new(u32::MAX);
    for _ in 0..50 {
        assert!(limiter.should_process());
    }
    assert_eq!(limiter.drop_rate_percent(), 0.0);
    assert_eq!(limiter.processed_count(), 50);
    assert_eq!(limiter.dropped_count(), 0);
    Ok(())
}

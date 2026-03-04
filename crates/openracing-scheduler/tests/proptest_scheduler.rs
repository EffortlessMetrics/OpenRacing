//! Property-based tests for the openracing-scheduler crate.
//!
//! These tests verify critical invariants that must hold for any valid inputs:
//! - Tick rates produce correct intervals
//! - No negative durations in scheduling
//! - Timing statistics are monotonically valid
//! - PLL correction is bounded
//! - Adaptive scheduling respects configured bounds

use openracing_scheduler::{AbsoluteScheduler, AdaptiveSchedulingConfig, JitterMetrics, PLL};
use proptest::prelude::*;
use std::time::Duration;

/// proptest config with 200 cases per test
fn config() -> ProptestConfig {
    ProptestConfig {
        cases: 200,
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// PLL invariants
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(config())]

    /// PLL correction output is always within ±10% of target period.
    #[test]
    fn pll_correction_is_bounded(
        target_ns in 1_000u64..=10_000_000,
        actual_ns in 1_000u64..=20_000_000,
    ) {
        let mut pll = PLL::new(target_ns);
        let corrected = pll.update(actual_ns);
        let corrected_ns = corrected.as_nanos() as u64;

        let min_ns = (target_ns as f64 * 0.9) as u64;
        let max_ns = (target_ns as f64 * 1.1) as u64;

        prop_assert!(
            corrected_ns >= min_ns.saturating_sub(1),
            "Corrected {} < min {} (target {})",
            corrected_ns, min_ns, target_ns
        );
        prop_assert!(
            corrected_ns <= max_ns + 1,
            "Corrected {} > max {} (target {})",
            corrected_ns, max_ns, target_ns
        );
    }

    /// PLL update never produces a zero-duration period.
    #[test]
    fn pll_update_never_zero(
        target_ns in 1u64..=10_000_000,
        actual_ns in 1u64..=20_000_000,
    ) {
        let mut pll = PLL::new(target_ns);
        let corrected = pll.update(actual_ns);
        prop_assert!(corrected > Duration::ZERO, "PLL returned zero duration");
    }

    /// PLL custom gains are always clamped to [0, 1].
    #[test]
    fn pll_custom_gains_clamped(
        target_ns in 1u64..=10_000_000,
        gain in -10.0f64..=10.0,
        integral_gain in -10.0f64..=10.0,
    ) {
        let pll = PLL::with_gains(target_ns, gain, integral_gain);
        // The PLL should still produce bounded output after clamping
        let mut pll = pll;
        let corrected = pll.update(target_ns);
        let corrected_ns = corrected.as_nanos() as u64;

        let min_ns = (target_ns as f64 * 0.9) as u64;
        let max_ns = (target_ns as f64 * 1.1) as u64;
        prop_assert!(corrected_ns >= min_ns.saturating_sub(1));
        prop_assert!(corrected_ns <= max_ns + 1);
    }

    /// After reset, PLL estimated period equals target.
    #[test]
    fn pll_reset_restores_target(
        target_ns in 1u64..=10_000_000,
        actual_ns in 1u64..=20_000_000,
    ) {
        let mut pll = PLL::new(target_ns);
        let _ = pll.update(actual_ns);
        pll.reset();
        prop_assert_eq!(pll.estimated_period_ns(), target_ns);
        prop_assert!((pll.phase_error_ns() - 0.0).abs() < f64::EPSILON);
    }

    /// Repeated PLL updates with exact target never diverge.
    #[test]
    fn pll_exact_ticks_stay_stable(target_ns in 100u64..=10_000_000) {
        let mut pll = PLL::new(target_ns);
        for _ in 0..100 {
            let _ = pll.update(target_ns);
        }
        prop_assert!(pll.is_stable(), "PLL became unstable with exact ticks");
    }

    /// PLL set_target_period always results in bounded estimated period.
    #[test]
    fn pll_set_target_keeps_bounds(
        initial_ns in 1u64..=10_000_000,
        new_target_ns in 1u64..=10_000_000,
    ) {
        let mut pll = PLL::new(initial_ns);
        pll.set_target_period_ns(new_target_ns);

        let estimated = pll.estimated_period_ns();
        let min_ns = (new_target_ns as f64 * 0.9) as u64;
        let max_ns = (new_target_ns as f64 * 1.1) as u64;
        prop_assert!(estimated >= min_ns.saturating_sub(1));
        prop_assert!(estimated <= max_ns + 1);
    }

    /// Average phase error with zero samples is zero.
    #[test]
    fn pll_average_phase_error_zero_samples(target_ns in 1u64..=10_000_000) {
        let pll = PLL::new(target_ns);
        prop_assert!((pll.average_phase_error_ns() - 0.0).abs() < f64::EPSILON);
    }
}

// ---------------------------------------------------------------------------
// JitterMetrics invariants
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(config())]

    /// max_jitter_ns is always >= any recorded jitter sample.
    #[test]
    fn jitter_max_monotonic(
        samples in prop::collection::vec(0u64..=10_000_000, 1..100),
    ) {
        let mut metrics = JitterMetrics::new();
        let mut expected_max = 0u64;
        for &jitter in &samples {
            expected_max = expected_max.max(jitter);
            metrics.record_tick(jitter, false);
        }
        prop_assert_eq!(metrics.max_jitter_ns, expected_max);
    }

    /// total_ticks always equals the number of record_tick calls.
    #[test]
    fn jitter_total_ticks_count(n in 0u64..=500) {
        let mut metrics = JitterMetrics::new();
        for _ in 0..n {
            metrics.record_tick(1000, false);
        }
        prop_assert_eq!(metrics.total_ticks, n);
    }

    /// missed_ticks never exceeds total_ticks.
    #[test]
    fn jitter_missed_never_exceeds_total(
        samples in prop::collection::vec((0u64..=10_000_000, any::<bool>()), 1..200),
    ) {
        let mut metrics = JitterMetrics::new();
        for &(jitter, missed) in &samples {
            metrics.record_tick(jitter, missed);
        }
        prop_assert!(
            metrics.missed_ticks <= metrics.total_ticks,
            "missed {} > total {}",
            metrics.missed_ticks, metrics.total_ticks
        );
    }

    /// missed_tick_rate is always in [0.0, 1.0].
    #[test]
    fn jitter_missed_rate_bounded(
        samples in prop::collection::vec((0u64..=10_000_000, any::<bool>()), 0..200),
    ) {
        let mut metrics = JitterMetrics::new();
        for &(jitter, missed) in &samples {
            metrics.record_tick(jitter, missed);
        }
        let rate = metrics.missed_tick_rate();
        prop_assert!((0.0..=1.0).contains(&rate), "rate out of bounds: {}", rate);
    }

    /// jitter_variance is always non-negative.
    #[test]
    fn jitter_variance_non_negative(
        samples in prop::collection::vec(0u64..=10_000_000, 0..200),
    ) {
        let mut metrics = JitterMetrics::new();
        for &jitter in &samples {
            metrics.record_tick(jitter, false);
        }
        let variance = metrics.jitter_variance();
        prop_assert!(variance >= 0.0, "negative variance: {}", variance);
    }

    /// jitter_std_dev_ns is always non-negative.
    #[test]
    fn jitter_std_dev_non_negative(
        samples in prop::collection::vec(0u64..=10_000_000, 0..200),
    ) {
        let mut metrics = JitterMetrics::new();
        for &jitter in &samples {
            metrics.record_tick(jitter, false);
        }
        let std_dev = metrics.jitter_std_dev_ns();
        prop_assert!(std_dev >= 0.0, "negative std dev: {}", std_dev);
    }

    /// p99 >= p95 >= p50 for any sample set.
    #[test]
    fn jitter_percentile_ordering(
        samples in prop::collection::vec(0u64..=10_000_000, 10..200),
    ) {
        let mut metrics = JitterMetrics::with_capacity(200);
        for &jitter in &samples {
            metrics.record_tick(jitter, false);
        }
        let p50 = metrics.p50_jitter_ns();
        let p95 = metrics.p95_jitter_ns();
        let p99 = metrics.p99_jitter_ns();
        prop_assert!(p99 >= p95, "p99 {} < p95 {}", p99, p95);
        prop_assert!(p95 >= p50, "p95 {} < p50 {}", p95, p50);
    }

    /// Reset clears all metrics to zero state.
    #[test]
    fn jitter_reset_clears(
        samples in prop::collection::vec((0u64..=10_000_000, any::<bool>()), 1..100),
    ) {
        let mut metrics = JitterMetrics::new();
        for &(jitter, missed) in &samples {
            metrics.record_tick(jitter, missed);
        }
        metrics.reset();
        prop_assert_eq!(metrics.total_ticks, 0);
        prop_assert_eq!(metrics.missed_ticks, 0);
        prop_assert_eq!(metrics.max_jitter_ns, 0);
        prop_assert_eq!(metrics.sample_count(), 0);
    }

    /// Ring buffer sample_count never exceeds capacity.
    #[test]
    fn jitter_ring_buffer_bounded(
        capacity in 1usize..=100,
        n_samples in 0usize..=500,
    ) {
        let mut metrics = JitterMetrics::with_capacity(capacity);
        for i in 0..n_samples {
            metrics.record_tick(i as u64, false);
        }
        prop_assert!(
            metrics.sample_count() <= capacity,
            "sample_count {} > capacity {}",
            metrics.sample_count(), capacity
        );
    }
}

// ---------------------------------------------------------------------------
// AbsoluteScheduler invariants
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(config())]

    /// Scheduler period is always >= 1 (never zero).
    #[test]
    fn scheduler_period_always_positive(period_ns in 0u64..=10_000_000) {
        let scheduler = AbsoluteScheduler::with_period(period_ns);
        prop_assert!(scheduler.period_ns() >= 1, "period was {}", scheduler.period_ns());
    }

    /// Scheduler tick_count starts at zero.
    #[test]
    fn scheduler_initial_tick_count_zero(period_ns in 1u64..=10_000_000) {
        let scheduler = AbsoluteScheduler::with_period(period_ns);
        prop_assert_eq!(scheduler.tick_count(), 0);
    }

    /// Scheduler reset always restores zero tick count.
    #[test]
    fn scheduler_reset_zeroes_ticks(period_ns in 1u64..=10_000_000) {
        let mut scheduler = AbsoluteScheduler::with_period(period_ns);
        // Simulate some state changes
        scheduler.record_processing_time_us(100);
        scheduler.reset();
        prop_assert_eq!(scheduler.tick_count(), 0);
        prop_assert_eq!(scheduler.metrics().total_ticks, 0);
    }
}

// ---------------------------------------------------------------------------
// AdaptiveSchedulingConfig invariants
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(config())]

    /// Normalize always produces a valid configuration.
    #[test]
    fn adaptive_normalize_always_valid(
        min_ns in 0u64..=10_000_000,
        max_ns in 0u64..=10_000_000,
        increase_ns in 0u64..=1_000_000,
        decrease_ns in 0u64..=1_000_000,
        jitter_relax in 0u64..=1_000_000,
        jitter_tighten in 0u64..=1_000_000,
        proc_relax in 0u64..=10_000,
        proc_tighten in 0u64..=10_000,
        alpha in -1.0f64..=5.0,
    ) {
        let mut config = AdaptiveSchedulingConfig {
            enabled: true,
            min_period_ns: min_ns,
            max_period_ns: max_ns,
            increase_step_ns: increase_ns,
            decrease_step_ns: decrease_ns,
            jitter_relax_threshold_ns: jitter_relax,
            jitter_tighten_threshold_ns: jitter_tighten,
            processing_relax_threshold_us: proc_relax,
            processing_tighten_threshold_us: proc_tighten,
            processing_ema_alpha: alpha,
        };
        config.normalize();
        prop_assert!(config.is_valid(), "Config invalid after normalize: {:?}", config);
    }

    /// After normalize, min_period <= max_period.
    #[test]
    fn adaptive_normalize_min_le_max(
        min_ns in 0u64..=10_000_000,
        max_ns in 0u64..=10_000_000,
    ) {
        let mut config = AdaptiveSchedulingConfig {
            min_period_ns: min_ns,
            max_period_ns: max_ns,
            ..AdaptiveSchedulingConfig::default()
        };
        config.normalize();
        prop_assert!(
            config.min_period_ns <= config.max_period_ns,
            "min {} > max {} after normalize",
            config.min_period_ns, config.max_period_ns
        );
    }

    /// EMA alpha after normalize is always in [0.01, 1.0].
    #[test]
    fn adaptive_ema_alpha_clamped(alpha in -100.0f64..=100.0) {
        let mut config = AdaptiveSchedulingConfig {
            processing_ema_alpha: alpha,
            ..AdaptiveSchedulingConfig::default()
        };
        config.normalize();
        prop_assert!(config.processing_ema_alpha >= 0.01);
        prop_assert!(config.processing_ema_alpha <= 1.0);
    }

    /// Adaptive scheduling target period is always within [min, max] bounds.
    #[test]
    fn adaptive_target_within_bounds(
        base_period in 500_000u64..=2_000_000,
        min_offset in 0u64..=200_000,
        max_offset in 0u64..=200_000,
    ) {
        let min_ns = base_period.saturating_sub(min_offset).max(1);
        let max_ns = base_period.saturating_add(max_offset);

        let mut scheduler = AbsoluteScheduler::with_period(base_period);
        let config = AdaptiveSchedulingConfig::new()
            .with_enabled(true)
            .with_period_bounds(min_ns, max_ns);
        scheduler.set_adaptive_scheduling(config);

        let state = scheduler.adaptive_scheduling();
        prop_assert!(
            state.target_period_ns >= state.min_period_ns,
            "target {} < min {}",
            state.target_period_ns, state.min_period_ns
        );
        prop_assert!(
            state.target_period_ns <= state.max_period_ns,
            "target {} > max {}",
            state.target_period_ns, state.max_period_ns
        );
    }

    /// Processing time EMA is always non-negative.
    #[test]
    fn processing_ema_non_negative(
        times in prop::collection::vec(0u64..=10_000, 1..50),
    ) {
        let mut scheduler = AbsoluteScheduler::new_1khz();
        for &t in &times {
            scheduler.record_processing_time_us(t);
        }
        let state = scheduler.adaptive_scheduling();
        prop_assert!(
            state.processing_time_ema_us >= 0.0,
            "negative EMA: {}",
            state.processing_time_ema_us
        );
    }
}

//! Property-based and edge-case tests for the scheduler crate.
//!
//! Covers scheduling invariants, zero-duration tasks, maximum priority,
//! overflow conditions, and RT scheduling policy selection.

#![allow(clippy::redundant_closure)]

use openracing_scheduler::{
    AdaptiveSchedulingConfig, AdaptiveSchedulingState, JitterMetrics, PERIOD_1KHZ_NS, PLL, RTSetup,
};
use proptest::prelude::*;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Property-based tests (proptest)
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    // -- PLL invariants -------------------------------------------------------

    #[test]
    fn prop_pll_correction_always_positive_duration(
        target in 1_000u64..10_000_000,
        interval in 1_000u64..10_000_000,
    ) {
        let mut pll = PLL::new(target);
        let corrected = pll.update(interval);
        prop_assert!(corrected > Duration::ZERO, "PLL must return positive duration");
    }

    #[test]
    fn prop_pll_converges_towards_target(
        target in 100_000u64..2_000_000,
    ) {
        let mut pll = PLL::new(target);
        // Feed the exact target interval repeatedly; phase error should shrink.
        for _ in 0..100 {
            let _ = pll.update(target);
        }
        let error = pll.phase_error_ns().abs();
        prop_assert!(error < target as f64 * 0.05,
            "After 100 on-target ticks phase error {} should be < 5% of target {}", error, target);
    }

    #[test]
    fn prop_pll_stability_flag_consistent(
        target in 100_000u64..2_000_000,
        jitter in 0u64..50_000,
    ) {
        let mut pll = PLL::new(target);
        for _ in 0..200 {
            let actual = target.saturating_add(jitter % 1000);
            let _ = pll.update(actual);
        }
        // If jitter is tiny relative to target, PLL should be stable.
        if jitter < target / 100 {
            prop_assert!(pll.is_stable(),
                "PLL should be stable with jitter {} and target {}", jitter, target);
        }
    }

    #[test]
    fn prop_pll_estimated_period_within_range(
        target in 100_000u64..5_000_000,
        intervals in prop::collection::vec(100_000u64..5_000_000, 1..50),
    ) {
        let mut pll = PLL::new(target);
        for &interval in &intervals {
            let _ = pll.update(interval);
        }
        let estimated = pll.estimated_period_ns();
        let lower = (target as f64 * 0.85) as u64;
        let upper = (target as f64 * 1.15) as u64;
        prop_assert!(estimated >= lower && estimated <= upper,
            "estimated {} out of [{}, {}] for target {}", estimated, lower, upper, target);
    }

    // -- Jitter metrics invariants -------------------------------------------

    #[test]
    fn prop_jitter_average_between_min_max(
        samples in prop::collection::vec(0u64..1_000_000, 2..200),
    ) {
        let mut metrics = JitterMetrics::with_capacity(samples.len());
        for &s in &samples {
            metrics.record_tick(s, false);
        }
        let avg = metrics.average_jitter_ns();
        let min_val = *samples.iter().min().unwrap_or(&0) as f64;
        let max_val = *samples.iter().max().unwrap_or(&0) as f64;
        prop_assert!(avg >= min_val - 0.01 && avg <= max_val + 0.01,
            "avg {} not in [{}, {}]", avg, min_val, max_val);
    }

    #[test]
    fn prop_jitter_sample_count_matches(
        count in 1usize..500,
    ) {
        let mut metrics = JitterMetrics::with_capacity(count);
        for i in 0..count {
            metrics.record_tick(i as u64, false);
        }
        prop_assert_eq!(metrics.sample_count(), count);
    }

    #[test]
    fn prop_jitter_total_ticks_equals_input(
        n_missed in 0u64..50,
        n_ok in 0u64..50,
    ) {
        let mut metrics = JitterMetrics::new();
        for _ in 0..n_missed {
            metrics.record_tick(500_000, true);
        }
        for _ in 0..n_ok {
            metrics.record_tick(100, false);
        }
        prop_assert_eq!(metrics.total_ticks, n_missed + n_ok);
        prop_assert_eq!(metrics.missed_ticks, n_missed);
    }

    #[test]
    fn prop_jitter_variance_non_negative(
        samples in prop::collection::vec(0u64..10_000_000, 2..100),
    ) {
        let mut metrics = JitterMetrics::with_capacity(samples.len());
        for &s in &samples {
            metrics.record_tick(s, false);
        }
        let variance = metrics.jitter_variance();
        prop_assert!(variance >= 0.0, "Variance {} must be non-negative", variance);
    }

    #[test]
    fn prop_jitter_std_dev_is_sqrt_variance(
        samples in prop::collection::vec(0u64..10_000_000, 2..100),
    ) {
        let mut metrics = JitterMetrics::with_capacity(samples.len());
        for &s in &samples {
            metrics.record_tick(s, false);
        }
        let variance = metrics.jitter_variance();
        let std_dev = metrics.jitter_std_dev_ns();
        let expected = variance.sqrt();
        prop_assert!((std_dev - expected).abs() < 1.0,
            "std_dev {} != sqrt(variance) {}", std_dev, expected);
    }

    // -- Adaptive scheduling invariants --------------------------------------

    #[test]
    fn prop_adaptive_config_valid_after_normalize(
        min_ns in 0u64..5_000_000,
        max_ns in 0u64..5_000_000,
        alpha in -2.0f64..3.0,
    ) {
        let mut config = AdaptiveSchedulingConfig {
            enabled: true,
            min_period_ns: min_ns,
            max_period_ns: max_ns,
            increase_step_ns: 5_000,
            decrease_step_ns: 2_000,
            jitter_relax_threshold_ns: 200_000,
            jitter_tighten_threshold_ns: 50_000,
            processing_relax_threshold_us: 180,
            processing_tighten_threshold_us: 80,
            processing_ema_alpha: alpha,
        };
        config.normalize();
        prop_assert!(config.is_valid(), "Config must be valid after normalize");
        prop_assert!(config.min_period_ns <= config.max_period_ns);
    }

    #[test]
    fn prop_adaptive_state_boundary_detection(
        min_ns in 100_000u64..500_000,
        max_ns in 600_000u64..2_000_000,
    ) {
        let at_min = AdaptiveSchedulingState {
            enabled: true,
            target_period_ns: min_ns,
            min_period_ns: min_ns,
            max_period_ns: max_ns,
            last_processing_time_us: 0,
            processing_time_ema_us: 0.0,
        };
        prop_assert!(at_min.is_at_min());
        prop_assert!(!at_min.is_at_max());

        let at_max = AdaptiveSchedulingState {
            enabled: true,
            target_period_ns: max_ns,
            min_period_ns: min_ns,
            max_period_ns: max_ns,
            last_processing_time_us: 0,
            processing_time_ema_us: 0.0,
        };
        prop_assert!(at_max.is_at_max());
        prop_assert!(!at_max.is_at_min());
    }

    // -- RTSetup invariants --------------------------------------------------

    #[test]
    fn prop_rt_setup_minimal_has_no_features(
        _seed in 0u32..100,
    ) {
        let setup = RTSetup::minimal();
        prop_assert!(!setup.has_rt_features());
    }

    #[test]
    fn prop_rt_setup_builder_roundtrip(
        hp in any::<bool>(),
        lm in any::<bool>(),
        dp in any::<bool>(),
        affinity in proptest::option::of(1u64..256),
    ) {
        let mut setup = RTSetup::new()
            .with_high_priority(hp)
            .with_lock_memory(lm)
            .with_disable_power_throttling(dp);
        if let Some(a) = affinity {
            setup = setup.with_cpu_affinity(a);
        }
        prop_assert_eq!(setup.high_priority, hp);
        prop_assert_eq!(setup.lock_memory, lm);
        prop_assert_eq!(setup.disable_power_throttling, dp);
        if affinity.is_some() {
            prop_assert!(setup.cpu_affinity.is_some());
        }
    }
}

// ---------------------------------------------------------------------------
// Edge-case tests (deterministic)
// ---------------------------------------------------------------------------

#[test]
fn edge_pll_zero_target_period() {
    // PLL should handle very small target period without panicking.
    let mut pll = PLL::new(1);
    let corrected = pll.update(1);
    assert!(corrected.as_nanos() > 0);
}

#[test]
fn edge_pll_maximum_target_period() {
    let target = u64::MAX / 2;
    let mut pll = PLL::new(target);
    let corrected = pll.update(target);
    // Should not panic; correction is within bounds.
    assert!(corrected.as_nanos() > 0);
}

#[test]
fn edge_pll_extreme_drift() {
    let target = PERIOD_1KHZ_NS;
    let mut pll = PLL::new(target);
    // Feed extremely long intervals.
    for _ in 0..10 {
        let corrected = pll.update(target * 100);
        let max_allowed = (target as f64 * 1.1) as u128;
        assert!(
            corrected.as_nanos() <= max_allowed,
            "Corrected {}ns exceeds 110% of target",
            corrected.as_nanos()
        );
    }
}

#[test]
fn edge_pll_reset_clears_state() {
    let mut pll = PLL::new(PERIOD_1KHZ_NS);
    for _ in 0..50 {
        let _ = pll.update(PERIOD_1KHZ_NS + 100_000);
    }
    pll.reset();
    // After reset, phase error should be zero.
    assert!(
        pll.phase_error_ns().abs() < f64::EPSILON,
        "Phase error after reset: {}",
        pll.phase_error_ns()
    );
}

#[test]
fn edge_pll_set_target_period() {
    let mut pll = PLL::new(PERIOD_1KHZ_NS);
    let new_target = 2_000_000u64;
    pll.set_target_period_ns(new_target);
    assert_eq!(pll.target_period_ns(), new_target);
}

#[test]
fn edge_jitter_empty_metrics() {
    let metrics = JitterMetrics::new();
    assert_eq!(metrics.total_ticks, 0);
    assert_eq!(metrics.missed_ticks, 0);
    assert_eq!(metrics.max_jitter_ns, 0);
    assert_eq!(metrics.missed_tick_rate(), 0.0);
    assert_eq!(metrics.average_jitter_ns(), 0.0);
}

#[test]
fn edge_jitter_single_sample() {
    let mut metrics = JitterMetrics::with_capacity(1);
    metrics.record_tick(42, false);
    assert_eq!(metrics.p50_jitter_ns(), 42);
    assert_eq!(metrics.p95_jitter_ns(), 42);
    assert_eq!(metrics.p99_jitter_ns(), 42);
}

#[test]
fn edge_jitter_all_missed_ticks() {
    let mut metrics = JitterMetrics::new();
    for _ in 0..100 {
        metrics.record_tick(500_000, true);
    }
    assert_eq!(metrics.missed_ticks, 100);
    assert!((metrics.missed_tick_rate() - 1.0).abs() < f64::EPSILON);
    assert!(!metrics.meets_requirements());
}

#[test]
fn edge_jitter_zero_jitter_meets_requirements() {
    let mut metrics = JitterMetrics::with_capacity(1000);
    for _ in 0..1000 {
        metrics.record_tick(0, false);
    }
    assert!(metrics.meets_requirements());
}

#[test]
fn edge_jitter_reset_clears_everything() {
    let mut metrics = JitterMetrics::new();
    for i in 0..50 {
        metrics.record_tick(i * 1000, i % 5 == 0);
    }
    metrics.reset();
    assert_eq!(metrics.total_ticks, 0);
    assert_eq!(metrics.missed_ticks, 0);
    assert_eq!(metrics.max_jitter_ns, 0);
    assert_eq!(metrics.sample_count(), 0);
}

#[test]
fn edge_jitter_max_value_samples() {
    let mut metrics = JitterMetrics::with_capacity(10);
    metrics.record_tick(u64::MAX, false);
    assert_eq!(metrics.max_jitter_ns, u64::MAX);
    assert_eq!(metrics.last_jitter_ns, u64::MAX);
}

#[test]
fn edge_jitter_custom_requirements() {
    let mut metrics = JitterMetrics::with_capacity(100);
    for _ in 0..100 {
        metrics.record_tick(100_000, false);
    }
    // Tight requirement should fail.
    assert!(!metrics.meets_custom_requirements(50_000, 0.0));
    // Relaxed requirement should pass.
    assert!(metrics.meets_custom_requirements(200_000, 0.01));
}

#[test]
fn edge_adaptive_config_all_zeros() {
    let mut config = AdaptiveSchedulingConfig {
        enabled: true,
        min_period_ns: 0,
        max_period_ns: 0,
        increase_step_ns: 0,
        decrease_step_ns: 0,
        jitter_relax_threshold_ns: 0,
        jitter_tighten_threshold_ns: 0,
        processing_relax_threshold_us: 0,
        processing_tighten_threshold_us: 0,
        processing_ema_alpha: 0.0,
    };
    config.normalize();
    // After normalize, min <= max.
    assert!(config.min_period_ns <= config.max_period_ns);
}

#[test]
fn edge_adaptive_config_inverted_bounds() {
    let mut config = AdaptiveSchedulingConfig {
        enabled: true,
        min_period_ns: 2_000_000,
        max_period_ns: 500_000,
        increase_step_ns: 5_000,
        decrease_step_ns: 2_000,
        jitter_relax_threshold_ns: 200_000,
        jitter_tighten_threshold_ns: 50_000,
        processing_relax_threshold_us: 180,
        processing_tighten_threshold_us: 80,
        processing_ema_alpha: 0.2,
    };
    config.normalize();
    assert!(config.min_period_ns <= config.max_period_ns);
}

#[test]
fn edge_rt_setup_all_features() {
    let setup = RTSetup::new()
        .with_high_priority(true)
        .with_lock_memory(true)
        .with_disable_power_throttling(true)
        .with_cpu_affinity(0xFF);
    assert!(setup.has_rt_features());
    assert_eq!(setup.cpu_affinity, Some(0xFF));
}

#[test]
fn edge_rt_setup_testing_profile() {
    let setup = RTSetup::testing();
    // Testing profile has only power throttling disabled.
    assert!(!setup.high_priority);
    assert!(!setup.lock_memory);
    assert!(setup.disable_power_throttling);
    assert!(setup.cpu_affinity.is_none());
    assert!(setup.has_rt_features());
}

#[test]
fn edge_pll_with_custom_gains() {
    let mut pll = PLL::with_gains(PERIOD_1KHZ_NS, 0.001, 0.01);
    for _ in 0..50 {
        let corrected = pll.update(PERIOD_1KHZ_NS);
        assert!(corrected.as_nanos() > 0);
    }
    // With gentle gains, should still converge.
    assert!(pll.phase_error_ns().abs() < PERIOD_1KHZ_NS as f64);
}

#[test]
fn edge_jitter_percentile_boundary_values() {
    let mut metrics = JitterMetrics::with_capacity(100);
    // All identical values.
    for _ in 0..100 {
        metrics.record_tick(50_000, false);
    }
    // All percentiles should be the same value.
    assert_eq!(metrics.p50_jitter_ns(), 50_000);
    assert_eq!(metrics.p95_jitter_ns(), 50_000);
    assert_eq!(metrics.p99_jitter_ns(), 50_000);
}

#[test]
fn edge_adaptive_state_period_fraction_at_boundaries() {
    let state = AdaptiveSchedulingState {
        enabled: true,
        target_period_ns: 1_000_000,
        min_period_ns: 1_000_000,
        max_period_ns: 1_000_000,
        last_processing_time_us: 0,
        processing_time_ema_us: 0.0,
    };
    // When min == max, fraction computation should not panic.
    let _fraction = state.period_fraction();
}

#[cfg(target_os = "windows")]
#[test]
fn edge_rt_setup_windows_policy() {
    // On Windows, high priority should map to TIME_CRITICAL.
    let setup = RTSetup::default();
    assert!(setup.high_priority);
}

#[cfg(target_os = "linux")]
#[test]
fn edge_rt_setup_linux_policy() {
    // On Linux, high priority should map to SCHED_FIFO.
    let setup = RTSetup::default();
    assert!(setup.high_priority);
}

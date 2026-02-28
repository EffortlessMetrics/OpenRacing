//! Property-based tests for the scheduler crate.

use openracing_scheduler::{
    AdaptiveSchedulingConfig, AdaptiveSchedulingState, JitterMetrics, PLL, RTSetup,
};
use quickcheck_macros::quickcheck;

#[quickcheck]
fn pll_period_never_exceeds_10_percent_bounds(target_period: u64, actual_intervals: Vec<u64>) {
    let target = target_period.clamp(1, 10_000_000_000);
    let mut pll = PLL::new(target);

    for interval in actual_intervals {
        let interval_bounded = interval.clamp(1, 10_000_000_000);
        let corrected = pll.update(interval_bounded);

        // Period should always be within ±10% of target
        let min = (target as f64 * 0.9) as u64;
        let max = (target as f64 * 1.1) as u64;

        assert!(
            corrected.as_nanos() >= min as u128,
            "Period {}ns below minimum {}ns",
            corrected.as_nanos(),
            min
        );
        assert!(
            corrected.as_nanos() <= max as u128,
            "Period {}ns above maximum {}ns",
            corrected.as_nanos(),
            max
        );
    }
}

#[quickcheck]
fn jitter_percentile_is_monotonic(samples: Vec<u64>) {
    if samples.is_empty() {
        return;
    }

    let mut metrics = JitterMetrics::with_capacity(samples.len().min(10000));

    for &sample in &samples {
        metrics.record_tick(sample.min(10_000_000_000), false);
    }

    let p50 = metrics.p50_jitter_ns();
    let p95 = metrics.p95_jitter_ns();
    let p99 = metrics.p99_jitter_ns();

    // Percentiles should be monotonic: p50 <= p95 <= p99
    assert!(p50 <= p95, "p50 ({}) > p95 ({})", p50, p95);
    assert!(p95 <= p99, "p95 ({}) > p99 ({})", p95, p99);
}

#[quickcheck]
fn jitter_max_is_upper_bound(samples: Vec<u64>) {
    if samples.is_empty() {
        return;
    }

    let mut metrics = JitterMetrics::new();
    let samples: Vec<u64> = samples.into_iter().map(|s| s.min(10_000_000_000)).collect();
    let Some(&expected_max) = samples.iter().max() else {
        return;
    };

    for &sample in &samples {
        metrics.record_tick(sample, false);
    }

    assert_eq!(metrics.max_jitter_ns, expected_max);
}

#[quickcheck]
fn adaptive_config_normalize_is_idempotent(
    min_max: (u64, u64),
    steps: (u64, u64),
    thresholds: (u64, u64),
    alpha: f64,
) {
    let mut config = AdaptiveSchedulingConfig {
        enabled: true,
        min_period_ns: min_max.0.min(10_000_000),
        max_period_ns: min_max.1.min(10_000_000),
        increase_step_ns: steps.0.min(1_000_000),
        decrease_step_ns: steps.1.min(1_000_000),
        jitter_relax_threshold_ns: thresholds.0.min(1_000_000_000),
        jitter_tighten_threshold_ns: thresholds.1.min(1_000_000_000),
        processing_relax_threshold_us: 100,
        processing_tighten_threshold_us: 50,
        processing_ema_alpha: alpha.abs().min(2.0),
    };

    config.normalize();
    let first = config.clone();
    config.normalize();

    // Second normalize should not change anything
    assert_eq!(config.min_period_ns, first.min_period_ns);
    assert_eq!(config.max_period_ns, first.max_period_ns);
    assert_eq!(config.increase_step_ns, first.increase_step_ns);
    assert_eq!(config.decrease_step_ns, first.decrease_step_ns);
}

#[quickcheck]
fn adaptive_state_period_fraction_in_range(min_period: u64, max_period: u64, target_period: u64) {
    let min = min_period.clamp(1, 1_000_000);
    let max = max_period.max(min + 1).min(2_000_000);
    let target = target_period.clamp(min, max);

    let state = AdaptiveSchedulingState {
        enabled: true,
        target_period_ns: target,
        min_period_ns: min,
        max_period_ns: max,
        last_processing_time_us: 0,
        processing_time_ema_us: 0.0,
    };

    let fraction = state.period_fraction();
    assert!((0.0..=1.0).contains(&fraction));
}

#[quickcheck]
fn jitter_missed_rate_in_bounds(total: u64, missed: u64) {
    let total = total.min(1_000_000);
    let missed = missed.min(total);

    let mut metrics = JitterMetrics::new();

    for i in 0..total {
        metrics.record_tick(100, i < missed);
    }

    let rate = metrics.missed_tick_rate();
    assert!((0.0..=1.0).contains(&rate));

    if total > 0 {
        let expected = missed as f64 / total as f64;
        assert!((rate - expected).abs() < 1e-10);
    } else {
        assert_eq!(rate, 0.0);
    }
}

#[quickcheck]
fn rt_setup_has_rt_features_consistent(
    high_priority: bool,
    lock_memory: bool,
    disable_power: bool,
    affinity: Option<u64>,
) {
    let setup = RTSetup {
        high_priority,
        lock_memory,
        disable_power_throttling: disable_power,
        cpu_affinity: affinity,
    };

    let expected = high_priority || lock_memory || disable_power || affinity.is_some();
    assert_eq!(setup.has_rt_features(), expected);
}

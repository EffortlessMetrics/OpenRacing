//! Tests for scheduler metrics collection accuracy, reset behavior,
//! export format, and histogram bucket distribution.

use openracing_scheduler::{AbsoluteScheduler, AdaptiveSchedulingConfig, JitterMetrics, RTSetup};

// ===========================================================================
// 1. Metrics collection accuracy
// ===========================================================================

#[test]
fn metrics_total_ticks_accurate() -> Result<(), Box<dyn std::error::Error>> {
    let mut metrics = JitterMetrics::new();
    let n = 500u64;
    for i in 0..n {
        metrics.record_tick(i * 100, false);
    }
    assert_eq!(metrics.total_ticks, n);
    assert_eq!(metrics.missed_ticks, 0);
    Ok(())
}

#[test]
fn metrics_missed_ticks_accurate() -> Result<(), Box<dyn std::error::Error>> {
    let mut metrics = JitterMetrics::new();
    for i in 0..100u64 {
        metrics.record_tick(i * 1_000, i >= 80);
    }
    assert_eq!(metrics.total_ticks, 100);
    assert_eq!(metrics.missed_ticks, 20);
    assert!((metrics.missed_tick_rate() - 0.2).abs() < 1e-10);
    Ok(())
}

#[test]
fn metrics_max_jitter_tracks_maximum() -> Result<(), Box<dyn std::error::Error>> {
    let mut metrics = JitterMetrics::new();
    let values = [100u64, 500, 200, 999, 300, 999, 50];
    for &v in &values {
        metrics.record_tick(v, false);
    }
    assert_eq!(metrics.max_jitter_ns, 999);
    Ok(())
}

#[test]
fn metrics_last_jitter_is_most_recent() -> Result<(), Box<dyn std::error::Error>> {
    let mut metrics = JitterMetrics::new();
    metrics.record_tick(111, false);
    metrics.record_tick(222, false);
    metrics.record_tick(333, false);
    assert_eq!(metrics.last_jitter_ns, 333);
    Ok(())
}

#[test]
fn metrics_variance_increases_with_spread() -> Result<(), Box<dyn std::error::Error>> {
    let mut narrow = JitterMetrics::new();
    let mut wide = JitterMetrics::new();
    for _ in 0..100 {
        narrow.record_tick(1000, false);
    }
    for i in 0..100u64 {
        wide.record_tick(i * 10_000, false);
    }
    assert!(
        wide.jitter_variance() > narrow.jitter_variance(),
        "wide variance {} should exceed narrow {}",
        wide.jitter_variance(),
        narrow.jitter_variance()
    );
    Ok(())
}

#[test]
fn metrics_std_dev_is_sqrt_of_variance() -> Result<(), Box<dyn std::error::Error>> {
    let mut metrics = JitterMetrics::new();
    for i in 0..200u64 {
        metrics.record_tick(i * 500, false);
    }
    let var = metrics.jitter_variance();
    let std_dev = metrics.jitter_std_dev_ns();
    assert!(
        (std_dev - var.sqrt()).abs() < 0.01,
        "std_dev {std_dev} != sqrt(var) {}",
        var.sqrt()
    );
    Ok(())
}

#[test]
fn metrics_missed_rate_zero_when_all_on_time() -> Result<(), Box<dyn std::error::Error>> {
    let mut metrics = JitterMetrics::new();
    for _ in 0..1000 {
        metrics.record_tick(50_000, false);
    }
    assert!((metrics.missed_tick_rate()).abs() < f64::EPSILON);
    Ok(())
}

#[test]
fn metrics_missed_rate_one_when_all_missed() -> Result<(), Box<dyn std::error::Error>> {
    let mut metrics = JitterMetrics::new();
    for _ in 0..1000 {
        metrics.record_tick(500_000, true);
    }
    assert!((metrics.missed_tick_rate() - 1.0).abs() < f64::EPSILON);
    Ok(())
}

// ===========================================================================
// 2. Metrics reset behavior
// ===========================================================================

#[test]
fn metrics_reset_clears_all_counters() -> Result<(), Box<dyn std::error::Error>> {
    let mut metrics = JitterMetrics::with_capacity(100);
    for i in 0..100u64 {
        metrics.record_tick(i * 1_000, i % 3 == 0);
    }

    metrics.reset();

    assert_eq!(metrics.total_ticks, 0);
    assert_eq!(metrics.missed_ticks, 0);
    assert_eq!(metrics.max_jitter_ns, 0);
    assert_eq!(metrics.last_jitter_ns, 0);
    assert_eq!(metrics.sample_count(), 0);
    assert_eq!(metrics.missed_tick_rate(), 0.0);
    assert_eq!(metrics.average_jitter_ns(), 0.0);
    assert!((metrics.jitter_variance()).abs() < f64::EPSILON);
    Ok(())
}

#[test]
fn metrics_usable_after_reset() -> Result<(), Box<dyn std::error::Error>> {
    let mut metrics = JitterMetrics::with_capacity(50);
    for _ in 0..50 {
        metrics.record_tick(100_000, false);
    }
    metrics.reset();

    // Record new data
    for _ in 0..30 {
        metrics.record_tick(200_000, true);
    }
    assert_eq!(metrics.total_ticks, 30);
    assert_eq!(metrics.missed_ticks, 30);
    assert_eq!(metrics.max_jitter_ns, 200_000);
    Ok(())
}

#[test]
fn scheduler_reset_clears_metrics_and_ticks() -> Result<(), Box<dyn std::error::Error>> {
    let mut sched = AbsoluteScheduler::with_period(5_000_000);
    sched.apply_rt_setup(&RTSetup::minimal())?;
    for _ in 0..10 {
        let _ = sched.wait_for_tick();
    }

    sched.reset();

    assert_eq!(sched.tick_count(), 0);
    assert_eq!(sched.metrics().total_ticks, 0);
    assert_eq!(sched.metrics().missed_ticks, 0);
    assert_eq!(sched.metrics().max_jitter_ns, 0);
    Ok(())
}

// ===========================================================================
// 3. Metrics export format (meets_requirements / meets_custom_requirements)
// ===========================================================================

#[test]
fn metrics_meets_requirements_with_low_jitter() -> Result<(), Box<dyn std::error::Error>> {
    let mut metrics = JitterMetrics::with_capacity(1000);
    for _ in 0..1000 {
        metrics.record_tick(100_000, false); // 100µs, well under 250µs
    }
    assert!(metrics.meets_requirements());
    Ok(())
}

#[test]
fn metrics_fails_requirements_high_p99() -> Result<(), Box<dyn std::error::Error>> {
    let mut metrics = JitterMetrics::with_capacity(1000);
    for _ in 0..1000 {
        metrics.record_tick(300_000, false); // 300µs > 250µs p99 limit
    }
    assert!(!metrics.meets_requirements());
    Ok(())
}

#[test]
fn metrics_fails_requirements_high_missed_rate() -> Result<(), Box<dyn std::error::Error>> {
    let mut metrics = JitterMetrics::with_capacity(1000);
    for i in 0..1000u64 {
        // Low jitter but high miss rate
        metrics.record_tick(10_000, i % 2 == 0);
    }
    assert!(!metrics.meets_requirements());
    Ok(())
}

#[test]
fn metrics_custom_requirements_strict() -> Result<(), Box<dyn std::error::Error>> {
    let mut metrics = JitterMetrics::with_capacity(500);
    for _ in 0..500 {
        metrics.record_tick(50_000, false);
    }
    assert!(metrics.meets_custom_requirements(100_000, 0.001));
    assert!(!metrics.meets_custom_requirements(10_000, 0.001));
    Ok(())
}

// ===========================================================================
// 4. Histogram bucket distribution (ring buffer / percentile)
// ===========================================================================

#[test]
fn histogram_uniform_distribution_percentiles() -> Result<(), Box<dyn std::error::Error>> {
    let mut metrics = JitterMetrics::with_capacity(1000);
    // Uniform distribution: 0, 1000, 2000, ..., 999_000
    for i in 0..1000u64 {
        metrics.record_tick(i * 1_000, false);
    }

    let p50 = metrics.p50_jitter_ns();
    let p95 = metrics.p95_jitter_ns();
    let p99 = metrics.p99_jitter_ns();

    // p50 should be around 500k, p95 ~950k, p99 ~990k
    assert!(
        (400_000..=600_000).contains(&p50),
        "p50 {p50} should be ~500k"
    );
    assert!(
        (900_000..=999_000).contains(&p95),
        "p95 {p95} should be ~950k"
    );
    assert!(p99 >= 980_000, "p99 {p99} should be >= 980k");
    Ok(())
}

#[test]
fn histogram_bimodal_distribution() -> Result<(), Box<dyn std::error::Error>> {
    let mut metrics = JitterMetrics::with_capacity(200);
    // 150 low samples, 50 high samples
    for _ in 0..150 {
        metrics.record_tick(10_000, false);
    }
    for _ in 0..50 {
        metrics.record_tick(500_000, true);
    }

    let p50 = metrics.p50_jitter_ns();
    let p95 = metrics.p95_jitter_ns();

    // p50 should be in the low cluster
    assert!(p50 <= 100_000, "p50 {p50} should be in low cluster");
    // p95 should capture some of the high cluster
    assert!(p95 >= 10_000, "p95 {p95} should capture high tail");
    Ok(())
}

#[test]
fn histogram_single_outlier() -> Result<(), Box<dyn std::error::Error>> {
    let mut metrics = JitterMetrics::with_capacity(100);
    for _ in 0..99 {
        metrics.record_tick(1_000, false);
    }
    metrics.record_tick(1_000_000, true); // single outlier

    let p50 = metrics.p50_jitter_ns();
    let p99 = metrics.p99_jitter_ns();

    assert_eq!(p50, 1_000, "p50 should be the common value");
    assert_eq!(p99, 1_000_000, "p99 should capture the outlier");
    Ok(())
}

#[test]
fn histogram_capacity_limits_samples() -> Result<(), Box<dyn std::error::Error>> {
    let cap = 50;
    let mut metrics = JitterMetrics::with_capacity(cap);
    for i in 0..200u64 {
        metrics.record_tick(i * 100, false);
    }
    assert_eq!(metrics.sample_count(), cap);
    assert_eq!(metrics.total_ticks, 200);
    Ok(())
}

#[test]
fn histogram_zero_capacity_no_percentiles() -> Result<(), Box<dyn std::error::Error>> {
    let mut metrics = JitterMetrics::with_capacity(0);
    for _ in 0..100 {
        metrics.record_tick(50_000, false);
    }
    // With zero capacity, percentile returns 0
    assert_eq!(metrics.p50_jitter_ns(), 0);
    assert_eq!(metrics.p99_jitter_ns(), 0);
    // But counters still work
    assert_eq!(metrics.total_ticks, 100);
    Ok(())
}

#[test]
fn histogram_empty_percentile_returns_zero() -> Result<(), Box<dyn std::error::Error>> {
    let mut metrics = JitterMetrics::new();
    assert_eq!(metrics.p50_jitter_ns(), 0);
    assert_eq!(metrics.p95_jitter_ns(), 0);
    assert_eq!(metrics.p99_jitter_ns(), 0);
    Ok(())
}

#[test]
fn scheduler_metrics_via_accessor() -> Result<(), Box<dyn std::error::Error>> {
    let mut sched = AbsoluteScheduler::with_period(5_000_000);
    sched.apply_rt_setup(&RTSetup::minimal())?;

    for _ in 0..20 {
        let _ = sched.wait_for_tick();
    }

    let m = sched.metrics();
    assert!(m.total_ticks >= 10, "expected at least 10 ticks");
    assert!(m.max_jitter_ns > 0 || m.total_ticks > 0, "should have data");

    // Mutable access for percentiles
    let m = sched.metrics_mut();
    let _p99 = m.p99_jitter_ns();
    // Just verifying it doesn't panic
    Ok(())
}

#[test]
fn processing_time_ema_initial_seed() -> Result<(), Box<dyn std::error::Error>> {
    let mut sched = AbsoluteScheduler::new_1khz();
    sched.set_adaptive_scheduling(AdaptiveSchedulingConfig::new().with_ema_alpha(0.5));

    // First sample seeds the EMA
    sched.record_processing_time_us(100);
    let state = sched.adaptive_scheduling();
    assert!(
        (state.processing_time_ema_us - 100.0).abs() < 0.01,
        "first EMA should equal first sample, got {}",
        state.processing_time_ema_us
    );

    // Second sample blends
    sched.record_processing_time_us(200);
    let state = sched.adaptive_scheduling();
    assert!(
        (state.processing_time_ema_us - 150.0).abs() < 0.01,
        "second EMA should be 150 with alpha=0.5, got {}",
        state.processing_time_ema_us
    );
    Ok(())
}

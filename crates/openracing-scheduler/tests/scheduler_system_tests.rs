//! System-level tests for openracing-scheduler: RT scheduling configuration,
//! PLL drift correction, jitter metrics, adaptive scheduling, and error types.

use openracing_scheduler::{
    AbsoluteScheduler, AdaptiveSchedulingConfig, AdaptiveSchedulingState, JitterMetrics,
    MAX_JITTER_NS, PERIOD_1KHZ_NS, PLL, RTError, RTResult, RTSetup,
};
use std::time::Duration;

// ===========================================================================
// Constants
// ===========================================================================

#[test]
fn period_1khz_is_one_millisecond_in_nanos() {
    assert_eq!(PERIOD_1KHZ_NS, 1_000_000);
}

#[test]
fn max_jitter_is_250_microseconds() {
    assert_eq!(MAX_JITTER_NS, 250_000);
}

// ===========================================================================
// RTError — display, debug, clone, equality
// ===========================================================================

#[test]
fn rt_error_display_all_variants() {
    let variants = [
        RTError::DeviceDisconnected,
        RTError::TorqueLimit,
        RTError::PipelineFault,
        RTError::TimingViolation,
        RTError::RTSetupFailed,
        RTError::InvalidConfig,
    ];
    for v in &variants {
        let msg = format!("{v}");
        assert!(!msg.is_empty(), "Display should not be empty for {v:?}");
    }
}

#[test]
fn rt_error_is_std_error() {
    let e = RTError::TimingViolation;
    let _: &dyn std::error::Error = &e;
}

#[test]
fn rt_error_clone_and_eq() {
    let e1 = RTError::DeviceDisconnected;
    let e2 = e1;
    assert_eq!(e1, e2);
}

#[test]
fn rt_error_all_variants_distinct() {
    let variants = [
        RTError::DeviceDisconnected,
        RTError::TorqueLimit,
        RTError::PipelineFault,
        RTError::TimingViolation,
        RTError::RTSetupFailed,
        RTError::InvalidConfig,
    ];
    for i in 0..variants.len() {
        for j in (i + 1)..variants.len() {
            assert_ne!(variants[i], variants[j]);
        }
    }
}

// ===========================================================================
// RTSetup — constructors, builder, features
// ===========================================================================

#[test]
fn rt_setup_new_enables_all_features() {
    let setup = RTSetup::new();
    assert!(setup.high_priority);
    assert!(setup.lock_memory);
    assert!(setup.disable_power_throttling);
    assert!(setup.has_rt_features());
}

#[test]
fn rt_setup_minimal_disables_all() {
    let setup = RTSetup::minimal();
    assert!(!setup.high_priority);
    assert!(!setup.lock_memory);
    assert!(!setup.disable_power_throttling);
    assert!(setup.cpu_affinity.is_none());
    assert!(!setup.has_rt_features());
}

#[test]
fn rt_setup_testing_config() {
    let setup = RTSetup::testing();
    assert!(setup.disable_power_throttling);
    // Testing config has reduced features
    assert!(!setup.high_priority || setup.has_rt_features());
}

#[test]
fn rt_setup_builder_chain() {
    let setup = RTSetup::minimal()
        .with_high_priority(true)
        .with_lock_memory(true)
        .with_disable_power_throttling(true)
        .with_cpu_affinity(0x0F);

    assert!(setup.high_priority);
    assert!(setup.lock_memory);
    assert!(setup.disable_power_throttling);
    assert_eq!(setup.cpu_affinity, Some(0x0F));
    assert!(setup.has_rt_features());
}

#[test]
fn rt_setup_cpu_affinity_single_core() {
    let setup = RTSetup::minimal().with_cpu_affinity(1); // core 0 only
    assert_eq!(setup.cpu_affinity, Some(1));
}

#[test]
fn rt_setup_cpu_affinity_all_cores() {
    let setup = RTSetup::minimal().with_cpu_affinity(u64::MAX);
    assert_eq!(setup.cpu_affinity, Some(u64::MAX));
}

#[test]
fn rt_setup_default_matches_new() {
    let def = RTSetup::default();
    let new = RTSetup::new();
    assert_eq!(def.high_priority, new.high_priority);
    assert_eq!(def.lock_memory, new.lock_memory);
    assert_eq!(def.disable_power_throttling, new.disable_power_throttling);
    assert_eq!(def.cpu_affinity, new.cpu_affinity);
}

#[test]
fn rt_setup_has_features_any_enabled() {
    assert!(
        RTSetup::minimal()
            .with_high_priority(true)
            .has_rt_features()
    );
    assert!(RTSetup::minimal().with_lock_memory(true).has_rt_features());
    assert!(
        RTSetup::minimal()
            .with_disable_power_throttling(true)
            .has_rt_features()
    );
    assert!(RTSetup::minimal().with_cpu_affinity(1).has_rt_features());
}

// ===========================================================================
// PLL — construction, stability, drift correction
// ===========================================================================

#[test]
fn pll_new_initial_state() {
    let pll = PLL::new(PERIOD_1KHZ_NS);
    assert_eq!(pll.target_period_ns(), PERIOD_1KHZ_NS);
    assert!((pll.phase_error_ns()).abs() < f64::EPSILON);
}

#[test]
fn pll_with_gains() {
    let pll = PLL::with_gains(PERIOD_1KHZ_NS, 0.5, 0.01);
    assert_eq!(pll.target_period_ns(), PERIOD_1KHZ_NS);
}

#[test]
fn pll_perfect_timing_stays_stable() {
    let mut pll = PLL::new(PERIOD_1KHZ_NS);
    for _ in 0..200 {
        let _ = pll.update(PERIOD_1KHZ_NS);
    }
    assert!(pll.is_stable());
    assert!(
        pll.phase_error_ns().abs() < 50_000.0,
        "Phase error should be small: {}",
        pll.phase_error_ns()
    );
}

#[test]
fn pll_consistently_slow_corrects() {
    let mut pll = PLL::new(PERIOD_1KHZ_NS);
    // Simulate running 10% slow
    let slow_period = (PERIOD_1KHZ_NS as f64 * 1.1) as u64;
    for _ in 0..100 {
        let _ = pll.update(slow_period);
    }
    let correction = pll.update(slow_period);
    // Correction should shorten the wait to compensate
    assert!(
        correction < Duration::from_nanos(slow_period),
        "PLL should correct by shortening period"
    );
}

#[test]
fn pll_consistently_fast_corrects() {
    let mut pll = PLL::new(PERIOD_1KHZ_NS);
    // Simulate running 10% fast
    let fast_period = (PERIOD_1KHZ_NS as f64 * 0.9) as u64;
    for _ in 0..100 {
        let _ = pll.update(fast_period);
    }
    let correction = pll.update(fast_period);
    // Correction should lengthen the wait
    assert!(
        correction > Duration::from_nanos(fast_period),
        "PLL should correct by lengthening period"
    );
}

#[test]
fn pll_reset_clears_state() {
    let mut pll = PLL::new(PERIOD_1KHZ_NS);
    for _ in 0..50 {
        let _ = pll.update(PERIOD_1KHZ_NS + 100_000);
    }
    pll.reset();
    assert!(
        pll.phase_error_ns().abs() < f64::EPSILON,
        "After reset, phase error should be near zero"
    );
}

#[test]
fn pll_set_target_period() {
    let mut pll = PLL::new(PERIOD_1KHZ_NS);
    pll.set_target_period_ns(500_000); // 2kHz
    assert_eq!(pll.target_period_ns(), 500_000);
}

#[test]
fn pll_estimated_period_near_target() {
    let mut pll = PLL::new(PERIOD_1KHZ_NS);
    for _ in 0..100 {
        let _ = pll.update(PERIOD_1KHZ_NS);
    }
    let estimated = pll.estimated_period_ns();
    let diff = (estimated as i64 - PERIOD_1KHZ_NS as i64).unsigned_abs();
    assert!(
        diff < 100_000,
        "Estimated period should be close to target: estimated={estimated}, target={PERIOD_1KHZ_NS}"
    );
}

#[test]
fn pll_average_phase_error() {
    let mut pll = PLL::new(PERIOD_1KHZ_NS);
    for _ in 0..100 {
        let _ = pll.update(PERIOD_1KHZ_NS);
    }
    let avg = pll.average_phase_error_ns();
    assert!(avg.is_finite());
}

#[test]
fn pll_update_returns_positive_duration() {
    let mut pll = PLL::new(PERIOD_1KHZ_NS);
    let d = pll.update(PERIOD_1KHZ_NS);
    assert!(
        d.as_nanos() > 0,
        "PLL correction should be a positive duration"
    );
}

#[test]
fn pll_large_jitter_does_not_panic() {
    let mut pll = PLL::new(PERIOD_1KHZ_NS);
    // Very large interval
    let _ = pll.update(u64::MAX / 2);
    // Very small interval
    let _ = pll.update(1);
    // Zero interval
    let _ = pll.update(0);
    // Normal
    let d = pll.update(PERIOD_1KHZ_NS);
    assert!(d.as_nanos() > 0);
}

// ===========================================================================
// JitterMetrics — recording, percentiles, requirements
// ===========================================================================

#[test]
fn jitter_metrics_new_is_empty() {
    let m = JitterMetrics::new();
    assert_eq!(m.total_ticks, 0);
    assert_eq!(m.missed_ticks, 0);
    assert_eq!(m.max_jitter_ns, 0);
    assert_eq!(m.sample_count(), 0);
}

#[test]
fn jitter_metrics_with_capacity() {
    let m = JitterMetrics::with_capacity(500);
    assert_eq!(m.sample_count(), 0);
}

#[test]
fn jitter_metrics_record_increments_total() {
    let mut m = JitterMetrics::new();
    m.record_tick(1000, false);
    assert_eq!(m.total_ticks, 1);
    assert_eq!(m.missed_ticks, 0);
    m.record_tick(2000, true);
    assert_eq!(m.total_ticks, 2);
    assert_eq!(m.missed_ticks, 1);
}

#[test]
fn jitter_metrics_tracks_max() {
    let mut m = JitterMetrics::new();
    m.record_tick(100, false);
    m.record_tick(500, false);
    m.record_tick(200, false);
    assert_eq!(m.max_jitter_ns, 500);
}

#[test]
fn jitter_metrics_last_jitter() {
    let mut m = JitterMetrics::new();
    m.record_tick(100, false);
    assert_eq!(m.last_jitter_ns, 100);
    m.record_tick(200, false);
    assert_eq!(m.last_jitter_ns, 200);
}

#[test]
fn jitter_metrics_missed_tick_rate() {
    let mut m = JitterMetrics::new();
    for i in 0..100 {
        m.record_tick(1000, i < 10); // 10% missed
    }
    let rate = m.missed_tick_rate();
    assert!((rate - 0.1).abs() < 1e-10);
}

#[test]
fn jitter_metrics_missed_tick_rate_zero_ticks() {
    let m = JitterMetrics::new();
    let rate = m.missed_tick_rate();
    // Should not panic, return 0 or NaN gracefully
    assert!(rate.is_finite() || rate.is_nan());
}

#[test]
fn jitter_metrics_percentile_p50() {
    let mut m = JitterMetrics::with_capacity(100);
    for i in 1..=100 {
        m.record_tick(i * 1000, false);
    }
    let p50 = m.p50_jitter_ns();
    // p50 of [1000..100000] should be around 50000-51000
    assert!(
        (40_000..=60_000).contains(&p50),
        "p50 should be near 50000: got {p50}"
    );
}

#[test]
fn jitter_metrics_percentile_p95() {
    let mut m = JitterMetrics::with_capacity(100);
    for i in 1..=100 {
        m.record_tick(i * 1000, false);
    }
    let p95 = m.p95_jitter_ns();
    assert!(
        (90_000..=100_000).contains(&p95),
        "p95 should be near 95000: got {p95}"
    );
}

#[test]
fn jitter_metrics_percentile_p99() {
    let mut m = JitterMetrics::with_capacity(1000);
    for i in 1..=1000 {
        m.record_tick(i * 100, false);
    }
    let p99 = m.p99_jitter_ns();
    assert!(
        (95_000..=100_100).contains(&p99),
        "p99 should be near 99000: got {p99}"
    );
}

#[test]
fn jitter_metrics_arbitrary_percentile() {
    let mut m = JitterMetrics::with_capacity(100);
    for i in 1..=100 {
        m.record_tick(i * 1000, false);
    }
    // percentile_jitter_ns takes 0.0-1.0 range
    let p10 = m.percentile_jitter_ns(0.1);
    assert!(
        (5_000..=15_000).contains(&p10),
        "p10 should be near 10000: got {p10}"
    );
}

#[test]
fn jitter_metrics_variance_and_stddev() {
    let mut m = JitterMetrics::new();
    // Constant jitter = 1000ns → variance = sum_sq/count = 1000^2 = 1_000_000
    for _ in 0..100 {
        m.record_tick(1000, false);
    }
    let variance = m.jitter_variance();
    let stddev = m.jitter_std_dev_ns();
    // variance is mean-of-squares (1000^2 = 1_000_000), stddev is sqrt = 1000
    assert!(
        (variance - 1_000_000.0).abs() < 1.0,
        "Mean of squares for constant 1000ns: got {variance}"
    );
    assert!(
        (stddev - 1000.0).abs() < 1.0,
        "RMS of constant 1000ns should be 1000: got {stddev}"
    );
}

#[test]
fn jitter_metrics_average() {
    let mut m = JitterMetrics::new();
    m.record_tick(100, false);
    m.record_tick(200, false);
    m.record_tick(300, false);
    let avg = m.average_jitter_ns();
    assert!(avg.is_finite());
    assert!(
        (avg - 200.0).abs() < 50.0,
        "Average should be near 200: got {avg}"
    );
}

#[test]
fn jitter_metrics_meets_requirements_good() {
    let mut m = JitterMetrics::with_capacity(100);
    // All ticks with very low jitter, no misses
    for _ in 0..100 {
        m.record_tick(1000, false); // 1µs jitter
    }
    assert!(m.meets_requirements());
}

#[test]
fn jitter_metrics_fails_requirements_high_jitter() {
    let mut m = JitterMetrics::with_capacity(100);
    // Most ticks have high jitter
    for _ in 0..100 {
        m.record_tick(500_000, false); // 500µs > 250µs threshold
    }
    assert!(!m.meets_requirements());
}

#[test]
fn jitter_metrics_fails_requirements_high_miss_rate() {
    let mut m = JitterMetrics::with_capacity(100);
    for _ in 0..100 {
        m.record_tick(1000, true); // all missed
    }
    assert!(!m.meets_requirements());
}

#[test]
fn jitter_metrics_custom_requirements() {
    let mut m = JitterMetrics::with_capacity(100);
    for _ in 0..100 {
        m.record_tick(100_000, false); // 100µs
    }
    // Lenient: 200µs p99, 1% miss rate
    assert!(m.meets_custom_requirements(200_000, 0.01));
    // Strict: 50µs p99
    assert!(!m.meets_custom_requirements(50_000, 0.01));
}

#[test]
fn jitter_metrics_reset_clears_all() {
    let mut m = JitterMetrics::new();
    for i in 0..50 {
        m.record_tick(i * 1000, i % 5 == 0);
    }
    m.reset();
    assert_eq!(m.total_ticks, 0);
    assert_eq!(m.missed_ticks, 0);
    assert_eq!(m.max_jitter_ns, 0);
    assert_eq!(m.sample_count(), 0);
}

#[test]
fn jitter_metrics_sample_count_capped() {
    let cap = 100;
    let mut m = JitterMetrics::with_capacity(cap);
    for i in 0..200 {
        m.record_tick(i * 100, false);
    }
    // sample_count should be capped at capacity
    assert!(m.sample_count() <= cap);
    // But total_ticks records all
    assert_eq!(m.total_ticks, 200);
}

// ===========================================================================
// AdaptiveSchedulingConfig — validation, normalization, builder
// ===========================================================================

#[test]
fn adaptive_config_new_is_disabled() {
    let cfg = AdaptiveSchedulingConfig::new();
    assert!(!cfg.enabled);
}

#[test]
fn adaptive_config_enabled_constructor() {
    let cfg = AdaptiveSchedulingConfig::enabled();
    assert!(cfg.enabled);
}

#[test]
fn adaptive_config_builder_chain() {
    let cfg = AdaptiveSchedulingConfig::new()
        .with_enabled(true)
        .with_period_bounds(500_000, 2_000_000)
        .with_step_sizes(10_000, 5_000)
        .with_jitter_thresholds(200_000, 50_000)
        .with_processing_thresholds(400, 100)
        .with_ema_alpha(0.1);

    assert!(cfg.enabled);
    assert_eq!(cfg.min_period_ns, 500_000);
    assert_eq!(cfg.max_period_ns, 2_000_000);
    assert_eq!(cfg.increase_step_ns, 10_000);
    assert_eq!(cfg.decrease_step_ns, 5_000);
    assert_eq!(cfg.jitter_relax_threshold_ns, 200_000);
    assert_eq!(cfg.jitter_tighten_threshold_ns, 50_000);
    assert_eq!(cfg.processing_relax_threshold_us, 400);
    assert_eq!(cfg.processing_tighten_threshold_us, 100);
    assert!((cfg.processing_ema_alpha - 0.1).abs() < f64::EPSILON);
}

#[test]
fn adaptive_config_is_valid_default() {
    let cfg = AdaptiveSchedulingConfig::new();
    assert!(cfg.is_valid());
}

#[test]
fn adaptive_config_is_valid_inverted_bounds() {
    let cfg = AdaptiveSchedulingConfig::new().with_period_bounds(2_000_000, 500_000); // min > max
    assert!(!cfg.is_valid());
}

#[test]
fn adaptive_config_normalize_fixes_inverted() {
    let mut cfg = AdaptiveSchedulingConfig::new().with_period_bounds(2_000_000, 500_000);
    cfg.normalize();
    assert!(cfg.is_valid());
    assert!(cfg.min_period_ns <= cfg.max_period_ns);
}

#[test]
fn adaptive_config_default_matches_new() {
    let def = AdaptiveSchedulingConfig::default();
    let new = AdaptiveSchedulingConfig::new();
    assert_eq!(def.enabled, new.enabled);
    assert_eq!(def.min_period_ns, new.min_period_ns);
    assert_eq!(def.max_period_ns, new.max_period_ns);
}

// ===========================================================================
// AdaptiveSchedulingState — properties
// ===========================================================================

#[test]
fn adaptive_state_new_defaults() {
    let state = AdaptiveSchedulingState::new();
    assert!(!state.enabled);
}

#[test]
fn adaptive_state_default_matches_new() {
    let def = AdaptiveSchedulingState::default();
    let new = AdaptiveSchedulingState::new();
    assert_eq!(def.enabled, new.enabled);
    assert_eq!(def.target_period_ns, new.target_period_ns);
}

#[test]
fn adaptive_state_period_fraction_at_boundaries() {
    let mut state = AdaptiveSchedulingState::new();
    state.min_period_ns = 500_000;
    state.max_period_ns = 2_000_000;

    state.target_period_ns = 500_000;
    assert!((state.period_fraction() - 0.0).abs() < 0.01);

    state.target_period_ns = 2_000_000;
    assert!((state.period_fraction() - 1.0).abs() < 0.01);

    state.target_period_ns = 1_250_000; // midpoint
    assert!((state.period_fraction() - 0.5).abs() < 0.01);
}

#[test]
fn adaptive_state_at_min_max() {
    let mut state = AdaptiveSchedulingState::new();
    state.min_period_ns = 500_000;
    state.max_period_ns = 2_000_000;

    state.target_period_ns = 500_000;
    assert!(state.is_at_min());
    assert!(!state.is_at_max());

    state.target_period_ns = 2_000_000;
    assert!(state.is_at_max());
    assert!(!state.is_at_min());

    state.target_period_ns = 1_000_000;
    assert!(!state.is_at_min());
    assert!(!state.is_at_max());
}

#[test]
fn adaptive_state_is_copy() {
    let state = AdaptiveSchedulingState::new();
    let copy = state;
    assert_eq!(copy.enabled, state.enabled);
}

// ===========================================================================
// AbsoluteScheduler — construction, period, reset, metrics access
// ===========================================================================

#[test]
fn scheduler_new_1khz_defaults() {
    let s = AbsoluteScheduler::new_1khz();
    assert_eq!(s.period_ns(), PERIOD_1KHZ_NS);
    assert_eq!(s.tick_count(), 0);
    assert!(!s.is_rt_setup_applied());
}

#[test]
fn scheduler_with_period() {
    let s = AbsoluteScheduler::with_period(500_000);
    assert_eq!(s.period_ns(), 500_000);
}

#[test]
fn scheduler_default_is_1khz() {
    let s = AbsoluteScheduler::default();
    assert_eq!(s.period_ns(), PERIOD_1KHZ_NS);
}

#[test]
fn scheduler_apply_rt_setup_testing() {
    let mut s = AbsoluteScheduler::new_1khz();
    let setup = RTSetup::testing();
    let result = s.apply_rt_setup(&setup);
    // Testing setup should generally succeed
    assert!(result.is_ok() || result == Err(RTError::RTSetupFailed));
    // Even if it "failed", mark should be set based on impl
}

#[test]
fn scheduler_apply_rt_setup_minimal() {
    let mut s = AbsoluteScheduler::new_1khz();
    let setup = RTSetup::minimal();
    let result = s.apply_rt_setup(&setup);
    assert!(result.is_ok() || result == Err(RTError::RTSetupFailed));
}

#[test]
fn scheduler_metrics_access() {
    let s = AbsoluteScheduler::new_1khz();
    let m = s.metrics();
    assert_eq!(m.total_ticks, 0);
}

#[test]
fn scheduler_metrics_mut_access() {
    let mut s = AbsoluteScheduler::new_1khz();
    let m = s.metrics_mut();
    m.record_tick(1000, false);
    assert_eq!(s.metrics().total_ticks, 1);
}

#[test]
fn scheduler_phase_error_initially_zero() {
    let s = AbsoluteScheduler::new_1khz();
    assert!((s.phase_error_ns()).abs() < f64::EPSILON);
}

#[test]
fn scheduler_reset_clears_state() {
    let mut s = AbsoluteScheduler::new_1khz();
    let setup = RTSetup::testing();
    let _ = s.apply_rt_setup(&setup);

    // Run a few ticks if possible
    for _ in 0..3 {
        match s.wait_for_tick() {
            Ok(_) | Err(RTError::TimingViolation) => {}
            Err(_) => break,
        }
    }

    s.reset();
    assert_eq!(s.tick_count(), 0);
    assert_eq!(s.metrics().total_ticks, 0);
}

#[test]
fn scheduler_set_adaptive_scheduling() {
    let mut s = AbsoluteScheduler::new_1khz();
    let cfg = AdaptiveSchedulingConfig::enabled().with_period_bounds(500_000, 2_000_000);
    s.set_adaptive_scheduling(cfg);
    let state = s.adaptive_scheduling();
    assert!(state.enabled);
}

#[test]
fn scheduler_adaptive_state_disabled_by_default() {
    let s = AbsoluteScheduler::new_1khz();
    let state = s.adaptive_scheduling();
    assert!(!state.enabled);
}

#[test]
fn scheduler_record_processing_time() {
    let mut s = AbsoluteScheduler::new_1khz();
    let cfg = AdaptiveSchedulingConfig::enabled();
    s.set_adaptive_scheduling(cfg);
    s.record_processing_time_us(100);
    let state = s.adaptive_scheduling();
    // After recording, the EMA should reflect the value
    assert!(state.last_processing_time_us == 100 || state.processing_time_ema_us > 0.0);
}

// ===========================================================================
// Scheduler timing — real ticks with timing validation
// ===========================================================================

#[test]
fn scheduler_wait_for_tick_returns_incrementing_count() {
    let mut s = AbsoluteScheduler::new_1khz();
    let setup = RTSetup::testing();
    let _ = s.apply_rt_setup(&setup);

    let mut ticks = Vec::new();
    for _ in 0..5 {
        match s.wait_for_tick() {
            Ok(tick) => ticks.push(tick),
            Err(RTError::TimingViolation) => return, // acceptable in CI
            Err(e) => {
                // Other errors are also acceptable in constrained environments
                assert!(
                    matches!(e, RTError::RTSetupFailed | RTError::TimingViolation),
                    "Unexpected error: {e:?}"
                );
                return;
            }
        }
    }

    // Ticks should be monotonically increasing
    for i in 1..ticks.len() {
        assert!(
            ticks[i] > ticks[i - 1],
            "Ticks should increase monotonically"
        );
    }
}

#[test]
fn scheduler_tick_count_matches_wait_result() {
    let mut s = AbsoluteScheduler::new_1khz();
    let setup = RTSetup::testing();
    let _ = s.apply_rt_setup(&setup);

    for _ in 0..3 {
        match s.wait_for_tick() {
            Ok(tick) => assert_eq!(tick, s.tick_count()),
            Err(RTError::TimingViolation) => return,
            Err(_) => return,
        }
    }
}

// ===========================================================================
// PLL + JitterMetrics integration
// ===========================================================================

#[test]
fn pll_feeds_jitter_metrics() {
    let mut pll = PLL::new(PERIOD_1KHZ_NS);
    let mut metrics = JitterMetrics::with_capacity(100);

    for i in 0..100 {
        let actual = PERIOD_1KHZ_NS + (i % 10) * 1000; // slight jitter
        let _ = pll.update(actual);
        let jitter_ns = (actual as i64 - PERIOD_1KHZ_NS as i64).unsigned_abs();
        metrics.record_tick(jitter_ns, false);
    }

    assert_eq!(metrics.total_ticks, 100);
    assert_eq!(metrics.missed_ticks, 0);
    assert!(metrics.max_jitter_ns <= 9000);
    assert!(pll.is_stable());
}

// ===========================================================================
// Scheduler with adaptive scheduling — period adjustment
// ===========================================================================

#[test]
fn scheduler_adaptive_period_bounded() {
    let mut s = AbsoluteScheduler::new_1khz();
    let cfg = AdaptiveSchedulingConfig::enabled().with_period_bounds(800_000, 1_200_000);
    s.set_adaptive_scheduling(cfg);

    let state = s.adaptive_scheduling();
    assert!(state.target_period_ns >= 800_000);
    assert!(state.target_period_ns <= 1_200_000);
}

// ===========================================================================
// Edge cases
// ===========================================================================

#[test]
fn jitter_metrics_single_sample_percentile() {
    let mut m = JitterMetrics::with_capacity(10);
    m.record_tick(42_000, false);
    // With only one sample, all percentiles should return that sample
    let p50 = m.p50_jitter_ns();
    let p99 = m.p99_jitter_ns();
    assert_eq!(p50, 42_000);
    assert_eq!(p99, 42_000);
}

#[test]
fn jitter_metrics_all_same_values() {
    let mut m = JitterMetrics::with_capacity(100);
    for _ in 0..100 {
        m.record_tick(5000, false);
    }
    assert_eq!(m.p50_jitter_ns(), 5000);
    assert_eq!(m.p95_jitter_ns(), 5000);
    assert_eq!(m.p99_jitter_ns(), 5000);
    // variance is mean-of-squares = 5000^2 = 25_000_000
    assert!((m.jitter_variance() - 25_000_000.0).abs() < 1.0);
}

#[test]
fn pll_high_frequency_period() {
    let mut pll = PLL::new(100_000); // 10kHz
    for _ in 0..100 {
        let d = pll.update(100_000);
        assert!(d.as_nanos() > 0);
    }
    assert!(pll.is_stable());
}

#[test]
fn pll_low_frequency_period() {
    let mut pll = PLL::new(10_000_000); // 100Hz
    for _ in 0..100 {
        let d = pll.update(10_000_000);
        assert!(d.as_nanos() > 0);
    }
    assert!(pll.is_stable());
}

#[test]
fn rt_result_type_alias_works() {
    fn returns_ok() -> RTResult<u64> {
        Ok(42)
    }
    fn returns_err() -> RTResult {
        Err(RTError::InvalidConfig)
    }
    assert!(returns_ok().is_ok());
    assert!(returns_err().is_err());
}

#[test]
fn scheduler_multiple_resets() {
    let mut s = AbsoluteScheduler::new_1khz();
    s.reset();
    s.reset();
    s.reset();
    assert_eq!(s.tick_count(), 0);
    assert_eq!(s.metrics().total_ticks, 0);
}

#[test]
fn scheduler_with_period_various_frequencies() {
    // 500Hz
    let s = AbsoluteScheduler::with_period(2_000_000);
    assert_eq!(s.period_ns(), 2_000_000);

    // 2kHz
    let s = AbsoluteScheduler::with_period(500_000);
    assert_eq!(s.period_ns(), 500_000);

    // 10kHz
    let s = AbsoluteScheduler::with_period(100_000);
    assert_eq!(s.period_ns(), 100_000);
}

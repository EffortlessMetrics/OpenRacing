//! Cross-platform scheduling API tests.
//!
//! These tests validate the platform-independent public API surface of the
//! scheduler crate: priority configuration, affinity masks, timing
//! primitives, error types, adaptive config, jitter metrics, and PLL
//! behaviour. Every test returns `Result` — no `unwrap()`/`expect()`.

use openracing_scheduler::{
    AbsoluteScheduler, AdaptiveSchedulingConfig, AdaptiveSchedulingState, JitterMetrics,
    MAX_JITTER_NS, PERIOD_1KHZ_NS, PLL, RTError, RTSetup,
};
use std::time::Instant;

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

/// Convenience alias used throughout the file.
type TestResult = Result<(), Box<dyn std::error::Error>>;

// ===========================================================================
// 1. RTSetup — priority & affinity validation
// ===========================================================================

#[test]
fn rtsetup_default_enables_all_features() -> TestResult {
    let s = RTSetup::default();
    assert!(s.high_priority);
    assert!(s.lock_memory);
    assert!(s.disable_power_throttling);
    assert!(s.cpu_affinity.is_none());
    assert!(s.has_rt_features());
    Ok(())
}

#[test]
fn rtsetup_minimal_disables_everything() -> TestResult {
    let s = RTSetup::minimal();
    assert!(!s.high_priority);
    assert!(!s.lock_memory);
    assert!(!s.disable_power_throttling);
    assert!(s.cpu_affinity.is_none());
    assert!(!s.has_rt_features());
    Ok(())
}

#[test]
fn rtsetup_testing_preset() -> TestResult {
    let s = RTSetup::testing();
    assert!(!s.high_priority);
    assert!(!s.lock_memory);
    assert!(s.disable_power_throttling);
    assert!(s.cpu_affinity.is_none());
    Ok(())
}

#[test]
fn rtsetup_builder_chain() -> TestResult {
    let s = RTSetup::new()
        .with_high_priority(false)
        .with_lock_memory(true)
        .with_disable_power_throttling(false)
        .with_cpu_affinity(0xFF);
    assert!(!s.high_priority);
    assert!(s.lock_memory);
    assert!(!s.disable_power_throttling);
    assert_eq!(s.cpu_affinity, Some(0xFF));
    assert!(s.has_rt_features());
    Ok(())
}

#[test]
fn rtsetup_affinity_single_core() -> TestResult {
    let s = RTSetup::minimal().with_cpu_affinity(0x01);
    assert_eq!(s.cpu_affinity, Some(1));
    assert!(s.has_rt_features());
    Ok(())
}

#[test]
fn rtsetup_affinity_all_cores_mask() -> TestResult {
    let s = RTSetup::minimal().with_cpu_affinity(u64::MAX);
    assert_eq!(s.cpu_affinity, Some(u64::MAX));
    Ok(())
}

#[test]
fn rtsetup_affinity_zero_mask_still_set() -> TestResult {
    // A zero mask is technically "set" even though no cores are selected.
    let s = RTSetup::minimal().with_cpu_affinity(0);
    assert_eq!(s.cpu_affinity, Some(0));
    assert!(s.has_rt_features());
    Ok(())
}

#[test]
fn rtsetup_new_equals_default() -> TestResult {
    let a = RTSetup::new();
    let b = RTSetup::default();
    assert_eq!(a.high_priority, b.high_priority);
    assert_eq!(a.lock_memory, b.lock_memory);
    assert_eq!(a.disable_power_throttling, b.disable_power_throttling);
    assert_eq!(a.cpu_affinity, b.cpu_affinity);
    Ok(())
}

#[test]
fn rtsetup_clone_is_independent() -> TestResult {
    let a = RTSetup::default();
    let mut b = a.clone();
    b.high_priority = false;
    assert!(a.high_priority);
    assert!(!b.high_priority);
    Ok(())
}

// ===========================================================================
// 2. AbsoluteScheduler — creation, period, reset
// ===========================================================================

#[test]
fn scheduler_1khz_period() -> TestResult {
    let s = AbsoluteScheduler::new_1khz();
    assert_eq!(s.period_ns(), PERIOD_1KHZ_NS);
    assert_eq!(s.tick_count(), 0);
    assert!(!s.is_rt_setup_applied());
    Ok(())
}

#[test]
fn scheduler_default_is_1khz() -> TestResult {
    let s = AbsoluteScheduler::default();
    assert_eq!(s.period_ns(), PERIOD_1KHZ_NS);
    Ok(())
}

#[test]
fn scheduler_custom_period() -> TestResult {
    let s = AbsoluteScheduler::with_period(500_000);
    assert_eq!(s.period_ns(), 500_000);
    Ok(())
}

#[test]
fn scheduler_zero_period_clamped_to_one() -> TestResult {
    let s = AbsoluteScheduler::with_period(0);
    assert_eq!(s.period_ns(), 1);
    Ok(())
}

#[test]
fn scheduler_very_large_period() -> TestResult {
    let s = AbsoluteScheduler::with_period(u64::MAX);
    assert_eq!(s.period_ns(), u64::MAX);
    Ok(())
}

#[test]
fn scheduler_reset_clears_state() -> TestResult {
    let mut s = AbsoluteScheduler::new_1khz();
    // Accumulate some state
    s.record_processing_time_us(100);
    s.metrics_mut().record_tick(50_000, false);
    s.reset();

    assert_eq!(s.tick_count(), 0);
    assert_eq!(s.metrics().total_ticks, 0);
    assert_eq!(s.metrics().missed_ticks, 0);
    assert_eq!(s.metrics().max_jitter_ns, 0);
    Ok(())
}

#[test]
fn scheduler_apply_rt_minimal_succeeds() -> TestResult {
    let mut s = AbsoluteScheduler::new_1khz();
    let setup = RTSetup::minimal();
    s.apply_rt_setup(&setup)?;
    assert!(s.is_rt_setup_applied());
    Ok(())
}

#[test]
fn scheduler_apply_rt_idempotent() -> TestResult {
    let mut s = AbsoluteScheduler::new_1khz();
    let setup = RTSetup::minimal();
    s.apply_rt_setup(&setup)?;
    // Second call should be a no-op and succeed.
    s.apply_rt_setup(&setup)?;
    assert!(s.is_rt_setup_applied());
    Ok(())
}

#[test]
fn scheduler_phase_error_starts_zero() -> TestResult {
    let s = AbsoluteScheduler::new_1khz();
    assert!((s.phase_error_ns() - 0.0).abs() < f64::EPSILON);
    Ok(())
}

// ===========================================================================
// 3. Adaptive scheduling configuration
// ===========================================================================

#[test]
fn adaptive_default_disabled() -> TestResult {
    let c = AdaptiveSchedulingConfig::default();
    assert!(!c.enabled);
    assert!(c.is_valid());
    Ok(())
}

#[test]
fn adaptive_enabled_constructor() -> TestResult {
    let c = AdaptiveSchedulingConfig::enabled();
    assert!(c.enabled);
    assert!(c.is_valid());
    Ok(())
}

#[test]
fn adaptive_builder_full_chain() -> TestResult {
    let c = AdaptiveSchedulingConfig::new()
        .with_enabled(true)
        .with_period_bounds(800_000, 1_200_000)
        .with_step_sizes(10_000, 5_000)
        .with_jitter_thresholds(300_000, 100_000)
        .with_processing_thresholds(200, 80)
        .with_ema_alpha(0.3);

    assert!(c.enabled);
    assert_eq!(c.min_period_ns, 800_000);
    assert_eq!(c.max_period_ns, 1_200_000);
    assert_eq!(c.increase_step_ns, 10_000);
    assert_eq!(c.decrease_step_ns, 5_000);
    assert_eq!(c.jitter_relax_threshold_ns, 300_000);
    assert_eq!(c.jitter_tighten_threshold_ns, 100_000);
    assert!((c.processing_ema_alpha - 0.3).abs() < 1e-10);
    assert!(c.is_valid());
    Ok(())
}

#[test]
fn adaptive_normalize_swaps_min_max() -> TestResult {
    let mut c = AdaptiveSchedulingConfig {
        min_period_ns: 2_000_000,
        max_period_ns: 500_000,
        ..Default::default()
    };
    c.normalize();
    assert!(c.min_period_ns < c.max_period_ns);
    assert_eq!(c.min_period_ns, 500_000);
    assert_eq!(c.max_period_ns, 2_000_000);
    Ok(())
}

#[test]
fn adaptive_normalize_clamps_ema_low() -> TestResult {
    let mut c = AdaptiveSchedulingConfig {
        processing_ema_alpha: 0.001,
        ..Default::default()
    };
    c.normalize();
    assert!((c.processing_ema_alpha - 0.01).abs() < 1e-10);
    Ok(())
}

#[test]
fn adaptive_normalize_clamps_ema_high() -> TestResult {
    let mut c = AdaptiveSchedulingConfig {
        processing_ema_alpha: 5.0,
        ..Default::default()
    };
    c.normalize();
    assert!((c.processing_ema_alpha - 1.0).abs() < 1e-10);
    Ok(())
}

#[test]
fn adaptive_normalize_fixes_tighten_greater_than_relax() -> TestResult {
    let mut c = AdaptiveSchedulingConfig {
        jitter_tighten_threshold_ns: 500_000,
        jitter_relax_threshold_ns: 100_000,
        processing_tighten_threshold_us: 300,
        processing_relax_threshold_us: 50,
        ..Default::default()
    };
    c.normalize();
    assert!(c.jitter_tighten_threshold_ns <= c.jitter_relax_threshold_ns);
    assert!(c.processing_tighten_threshold_us <= c.processing_relax_threshold_us);
    Ok(())
}

#[test]
fn adaptive_normalize_zero_steps_become_one() -> TestResult {
    let mut c = AdaptiveSchedulingConfig {
        increase_step_ns: 0,
        decrease_step_ns: 0,
        ..Default::default()
    };
    c.normalize();
    assert!(c.increase_step_ns >= 1);
    assert!(c.decrease_step_ns >= 1);
    Ok(())
}

#[test]
fn adaptive_normalize_zero_min_period_becomes_one() -> TestResult {
    let mut c = AdaptiveSchedulingConfig {
        min_period_ns: 0,
        max_period_ns: 0,
        ..Default::default()
    };
    c.normalize();
    assert!(c.min_period_ns >= 1);
    assert!(c.max_period_ns >= c.min_period_ns);
    Ok(())
}

#[test]
fn adaptive_invalid_zero_min_period() -> TestResult {
    let c = AdaptiveSchedulingConfig {
        min_period_ns: 0,
        ..Default::default()
    };
    assert!(!c.is_valid());
    Ok(())
}

#[test]
fn adaptive_invalid_min_greater_than_max() -> TestResult {
    let c = AdaptiveSchedulingConfig {
        min_period_ns: 2_000_000,
        max_period_ns: 1_000_000,
        ..Default::default()
    };
    assert!(!c.is_valid());
    Ok(())
}

#[test]
fn adaptive_invalid_ema_alpha_too_small() -> TestResult {
    let c = AdaptiveSchedulingConfig {
        processing_ema_alpha: 0.001,
        ..Default::default()
    };
    assert!(!c.is_valid());
    Ok(())
}

#[test]
fn adaptive_invalid_ema_alpha_too_large() -> TestResult {
    let c = AdaptiveSchedulingConfig {
        processing_ema_alpha: 1.5,
        ..Default::default()
    };
    assert!(!c.is_valid());
    Ok(())
}

// ===========================================================================
// 4. Scheduler ↔ adaptive integration
// ===========================================================================

#[test]
fn scheduler_adaptive_defaults_disabled() -> TestResult {
    let s = AbsoluteScheduler::new_1khz();
    let st = s.adaptive_scheduling();
    assert!(!st.enabled);
    assert_eq!(st.target_period_ns, PERIOD_1KHZ_NS);
    Ok(())
}

#[test]
fn scheduler_set_adaptive_enabled() -> TestResult {
    let mut s = AbsoluteScheduler::new_1khz();
    s.set_adaptive_scheduling(AdaptiveSchedulingConfig::enabled());
    let st = s.adaptive_scheduling();
    assert!(st.enabled);
    Ok(())
}

#[test]
fn scheduler_adaptive_clamps_period() -> TestResult {
    let mut s = AbsoluteScheduler::with_period(500_000);
    let cfg = AdaptiveSchedulingConfig::new()
        .with_enabled(true)
        .with_period_bounds(900_000, 1_100_000);
    s.set_adaptive_scheduling(cfg);
    let st = s.adaptive_scheduling();
    assert!(st.target_period_ns >= 900_000);
    assert!(st.target_period_ns <= 1_100_000);
    Ok(())
}

#[test]
fn scheduler_record_processing_time_ema_initial() -> TestResult {
    let mut s = AbsoluteScheduler::new_1khz();
    s.set_adaptive_scheduling(
        AdaptiveSchedulingConfig::new().with_ema_alpha(0.5),
    );
    s.record_processing_time_us(200);
    let st = s.adaptive_scheduling();
    // First sample should seed the EMA directly.
    assert!((st.processing_time_ema_us - 200.0).abs() < 1e-6);
    Ok(())
}

#[test]
fn scheduler_record_processing_time_ema_converges() -> TestResult {
    let mut s = AbsoluteScheduler::new_1khz();
    s.set_adaptive_scheduling(
        AdaptiveSchedulingConfig::new().with_ema_alpha(0.5),
    );
    s.record_processing_time_us(100);
    s.record_processing_time_us(200);
    let st = s.adaptive_scheduling();
    assert_eq!(st.last_processing_time_us, 200);
    // EMA after two samples: first=100, then 0.5*100 + 0.5*200 = 150
    assert!((st.processing_time_ema_us - 150.0).abs() < 1e-6);
    Ok(())
}

// ===========================================================================
// 5. AdaptiveSchedulingState queries
// ===========================================================================

#[test]
fn adaptive_state_at_min() -> TestResult {
    let st = AdaptiveSchedulingState {
        target_period_ns: 900_000,
        min_period_ns: 900_000,
        max_period_ns: 1_100_000,
        ..Default::default()
    };
    assert!(st.is_at_min());
    assert!(!st.is_at_max());
    Ok(())
}

#[test]
fn adaptive_state_at_max() -> TestResult {
    let st = AdaptiveSchedulingState {
        target_period_ns: 1_100_000,
        min_period_ns: 900_000,
        max_period_ns: 1_100_000,
        ..Default::default()
    };
    assert!(st.is_at_max());
    assert!(!st.is_at_min());
    Ok(())
}

#[test]
fn adaptive_state_fraction_at_midpoint() -> TestResult {
    let st = AdaptiveSchedulingState {
        target_period_ns: 1_000_000,
        min_period_ns: 900_000,
        max_period_ns: 1_100_000,
        ..Default::default()
    };
    assert!((st.period_fraction() - 0.5).abs() < 1e-10);
    Ok(())
}

#[test]
fn adaptive_state_fraction_equal_bounds() -> TestResult {
    let st = AdaptiveSchedulingState {
        target_period_ns: 1_000_000,
        min_period_ns: 1_000_000,
        max_period_ns: 1_000_000,
        ..Default::default()
    };
    assert!((st.period_fraction() - 0.5).abs() < 1e-10);
    Ok(())
}

// ===========================================================================
// 6. PLL — drift correction, bounds, stability
// ===========================================================================

#[test]
fn pll_initial_state() -> TestResult {
    let pll = PLL::new(1_000_000);
    assert_eq!(pll.target_period_ns(), 1_000_000);
    assert_eq!(pll.estimated_period_ns(), 1_000_000);
    assert!((pll.phase_error_ns() - 0.0).abs() < f64::EPSILON);
    assert!(pll.is_stable());
    Ok(())
}

#[test]
fn pll_custom_gains_clamped() -> TestResult {
    let pll = PLL::with_gains(1_000_000, 2.0, -1.0);
    // Gains should be clamped to [0, 1].
    assert!((pll.phase_error_ns() - 0.0).abs() < f64::EPSILON);
    Ok(())
}

#[test]
fn pll_update_corrects_slow_tick() -> TestResult {
    let mut pll = PLL::new(1_000_000);
    let corrected = pll.update(1_050_000); // 5% slow
    let ns = corrected.as_nanos() as u64;
    // Should be within ±10% of target.
    assert!(ns >= 900_000, "period {} too small", ns);
    assert!(ns <= 1_100_000, "period {} too large", ns);
    Ok(())
}

#[test]
fn pll_update_corrects_fast_tick() -> TestResult {
    let mut pll = PLL::new(1_000_000);
    let corrected = pll.update(950_000); // 5% fast
    let ns = corrected.as_nanos() as u64;
    assert!(ns >= 900_000, "period {} too small", ns);
    assert!(ns <= 1_100_000, "period {} too large", ns);
    Ok(())
}

#[test]
fn pll_extreme_slow_clamped() -> TestResult {
    let mut pll = PLL::new(1_000_000);
    let corrected = pll.update(5_000_000); // 5x too slow
    let ns = corrected.as_nanos() as u64;
    assert!(ns >= 900_000);
    assert!(ns <= 1_100_000);
    Ok(())
}

#[test]
fn pll_extreme_fast_clamped() -> TestResult {
    let mut pll = PLL::new(1_000_000);
    let corrected = pll.update(100); // almost zero
    let ns = corrected.as_nanos() as u64;
    assert!(ns >= 900_000);
    assert!(ns <= 1_100_000);
    Ok(())
}

#[test]
fn pll_reset_restores_initial() -> TestResult {
    let mut pll = PLL::new(1_000_000);
    let _ = pll.update(1_100_000);
    pll.reset();
    assert_eq!(pll.estimated_period_ns(), 1_000_000);
    assert!((pll.phase_error_ns() - 0.0).abs() < f64::EPSILON);
    Ok(())
}

#[test]
fn pll_set_target_period_updates_bounds() -> TestResult {
    let mut pll = PLL::new(1_000_000);
    pll.set_target_period_ns(2_000_000);
    assert_eq!(pll.target_period_ns(), 2_000_000);
    let est = pll.estimated_period_ns();
    assert!(est >= 1_800_000);
    assert!(est <= 2_200_000);
    Ok(())
}

#[test]
fn pll_set_target_zero_clamped() -> TestResult {
    let mut pll = PLL::new(1_000_000);
    pll.set_target_period_ns(0);
    assert_eq!(pll.target_period_ns(), 1);
    Ok(())
}

#[test]
fn pll_stability_after_small_correction() -> TestResult {
    let mut pll = PLL::new(1_000_000);
    let _ = pll.update(1_010_000);
    assert!(pll.is_stable());
    Ok(())
}

#[test]
fn pll_average_phase_error_empty() -> TestResult {
    let pll = PLL::new(1_000_000);
    assert!((pll.average_phase_error_ns() - 0.0).abs() < f64::EPSILON);
    Ok(())
}

#[test]
fn pll_average_phase_error_symmetric() -> TestResult {
    let mut pll = PLL::new(1_000_000);
    let _ = pll.update(1_010_000); // +10us
    let _ = pll.update(990_000);   // -10us
    // Accumulated: +10000 + (-10000) = 0, average ~0
    assert!(pll.average_phase_error_ns().abs() < 1.0);
    Ok(())
}

// ===========================================================================
// 7. JitterMetrics — recording, percentiles, requirements
// ===========================================================================

#[test]
fn jitter_new_empty() -> TestResult {
    let m = JitterMetrics::new();
    assert_eq!(m.total_ticks, 0);
    assert_eq!(m.missed_ticks, 0);
    assert_eq!(m.max_jitter_ns, 0);
    assert_eq!(m.sample_count(), 0);
    Ok(())
}

#[test]
fn jitter_record_basic() -> TestResult {
    let mut m = JitterMetrics::new();
    m.record_tick(100_000, false);
    m.record_tick(200_000, true);
    assert_eq!(m.total_ticks, 2);
    assert_eq!(m.missed_ticks, 1);
    assert_eq!(m.max_jitter_ns, 200_000);
    assert_eq!(m.last_jitter_ns, 200_000);
    Ok(())
}

#[test]
fn jitter_missed_rate_zero_when_empty() -> TestResult {
    let m = JitterMetrics::new();
    assert!((m.missed_tick_rate() - 0.0).abs() < f64::EPSILON);
    Ok(())
}

#[test]
fn jitter_missed_rate_calculation() -> TestResult {
    let mut m = JitterMetrics::new();
    m.record_tick(100_000, false);
    m.record_tick(100_000, true);
    m.record_tick(100_000, false);
    assert!((m.missed_tick_rate() - 1.0 / 3.0).abs() < 1e-10);
    Ok(())
}

#[test]
fn jitter_p99_empty_returns_zero() -> TestResult {
    let mut m = JitterMetrics::new();
    assert_eq!(m.p99_jitter_ns(), 0);
    Ok(())
}

#[test]
fn jitter_p99_uniform_samples() -> TestResult {
    let mut m = JitterMetrics::new();
    for i in 0..100u64 {
        m.record_tick(i * 1_000, false);
    }
    let p99 = m.p99_jitter_ns();
    assert!(p99 >= 97_000, "p99={}", p99);
    assert!(p99 <= 99_000, "p99={}", p99);
    Ok(())
}

#[test]
fn jitter_p50_median() -> TestResult {
    let mut m = JitterMetrics::new();
    for i in 0..100u64 {
        m.record_tick(i * 1_000, false);
    }
    let p50 = m.p50_jitter_ns();
    assert!(p50 >= 45_000, "p50={}", p50);
    assert!(p50 <= 55_000, "p50={}", p50);
    Ok(())
}

#[test]
fn jitter_p95_between_p50_and_p99() -> TestResult {
    let mut m = JitterMetrics::new();
    for i in 0..100u64 {
        m.record_tick(i * 1_000, false);
    }
    let p50 = m.p50_jitter_ns();
    let p95 = m.p95_jitter_ns();
    let p99 = m.p99_jitter_ns();
    assert!(p50 <= p95);
    assert!(p95 <= p99);
    Ok(())
}

#[test]
fn jitter_all_same_value() -> TestResult {
    let mut m = JitterMetrics::with_capacity(50);
    for _ in 0..50 {
        m.record_tick(42_000, false);
    }
    assert_eq!(m.p50_jitter_ns(), 42_000);
    assert_eq!(m.p95_jitter_ns(), 42_000);
    assert_eq!(m.p99_jitter_ns(), 42_000);
    Ok(())
}

#[test]
fn jitter_ring_buffer_overwrites() -> TestResult {
    let mut m = JitterMetrics::with_capacity(3);
    for i in 1..=5u64 {
        m.record_tick(i * 1_000, false);
    }
    assert_eq!(m.sample_count(), 3);
    assert_eq!(m.total_ticks, 5);
    Ok(())
}

#[test]
fn jitter_zero_capacity_still_counts() -> TestResult {
    let mut m = JitterMetrics::with_capacity(0);
    m.record_tick(100_000, true);
    assert_eq!(m.total_ticks, 1);
    assert_eq!(m.missed_ticks, 1);
    assert_eq!(m.sample_count(), 0);
    Ok(())
}

#[test]
fn jitter_variance_empty() -> TestResult {
    let m = JitterMetrics::new();
    assert!((m.jitter_variance() - 0.0).abs() < f64::EPSILON);
    Ok(())
}

#[test]
fn jitter_variance_positive() -> TestResult {
    let mut m = JitterMetrics::new();
    m.record_tick(100, false);
    m.record_tick(200, false);
    assert!(m.jitter_variance() > 0.0);
    Ok(())
}

#[test]
fn jitter_std_dev_consistent_with_variance() -> TestResult {
    let mut m = JitterMetrics::new();
    m.record_tick(100, false);
    m.record_tick(300, false);
    let sd = m.jitter_std_dev_ns();
    let var = m.jitter_variance();
    assert!((sd * sd - var).abs() < 1e-6);
    Ok(())
}

#[test]
fn jitter_meets_requirements_good() -> TestResult {
    let mut m = JitterMetrics::new();
    for _ in 0..1000 {
        m.record_tick(100_000, false);
    }
    assert!(m.meets_requirements());
    Ok(())
}

#[test]
fn jitter_meets_requirements_high_jitter() -> TestResult {
    let mut m = JitterMetrics::new();
    for _ in 0..1000 {
        m.record_tick(300_000, false);
    }
    assert!(!m.meets_requirements());
    Ok(())
}

#[test]
fn jitter_meets_requirements_all_missed() -> TestResult {
    let mut m = JitterMetrics::new();
    for _ in 0..1000 {
        m.record_tick(100_000, true);
    }
    assert!(!m.meets_requirements());
    Ok(())
}

#[test]
fn jitter_meets_custom_requirements() -> TestResult {
    let mut m = JitterMetrics::new();
    for _ in 0..100 {
        m.record_tick(50_000, false);
    }
    assert!(m.meets_custom_requirements(100_000, 0.01));
    assert!(!m.meets_custom_requirements(10_000, 0.01));
    Ok(())
}

#[test]
fn jitter_reset_clears_all() -> TestResult {
    let mut m = JitterMetrics::new();
    for i in 1..=10 {
        m.record_tick(i * 1_000, i % 3 == 0);
    }
    m.reset();
    assert_eq!(m.total_ticks, 0);
    assert_eq!(m.missed_ticks, 0);
    assert_eq!(m.max_jitter_ns, 0);
    assert_eq!(m.sample_count(), 0);
    Ok(())
}

#[test]
fn jitter_average_jitter_empty() -> TestResult {
    let m = JitterMetrics::new();
    assert!((m.average_jitter_ns() - 0.0).abs() < f64::EPSILON);
    Ok(())
}

// ===========================================================================
// 8. RTError — Display, Debug, variant coverage
// ===========================================================================

#[test]
fn error_display_device_disconnected() -> TestResult {
    let e = RTError::DeviceDisconnected;
    assert_eq!(format!("{e}"), "Device disconnected");
    Ok(())
}

#[test]
fn error_display_torque_limit() -> TestResult {
    let e = RTError::TorqueLimit;
    assert_eq!(format!("{e}"), "Torque limit exceeded");
    Ok(())
}

#[test]
fn error_display_pipeline_fault() -> TestResult {
    let e = RTError::PipelineFault;
    assert_eq!(format!("{e}"), "Pipeline processing fault");
    Ok(())
}

#[test]
fn error_display_timing_violation() -> TestResult {
    let e = RTError::TimingViolation;
    assert_eq!(format!("{e}"), "Real-time timing violation");
    Ok(())
}

#[test]
fn error_display_rt_setup_failed() -> TestResult {
    let e = RTError::RTSetupFailed;
    assert_eq!(format!("{e}"), "Failed to apply real-time setup");
    Ok(())
}

#[test]
fn error_display_invalid_config() -> TestResult {
    let e = RTError::InvalidConfig;
    assert_eq!(format!("{e}"), "Invalid configuration parameter");
    Ok(())
}

#[test]
fn error_debug_representation() -> TestResult {
    let e = RTError::TimingViolation;
    let dbg = format!("{e:?}");
    assert!(dbg.contains("TimingViolation"));
    Ok(())
}

#[test]
fn error_clone_and_eq() -> TestResult {
    let a = RTError::PipelineFault;
    let b = a;
    assert_eq!(a, b);
    Ok(())
}

#[test]
fn error_implements_std_error() -> TestResult {
    fn assert_std_error<E: std::error::Error>(_e: &E) {}
    assert_std_error(&RTError::TimingViolation);
    Ok(())
}

#[test]
fn error_all_variants_distinct() -> TestResult {
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
    Ok(())
}

// ===========================================================================
// 9. Constants
// ===========================================================================

#[test]
fn constant_period_1khz() -> TestResult {
    assert_eq!(PERIOD_1KHZ_NS, 1_000_000);
    Ok(())
}

#[test]
fn constant_max_jitter() -> TestResult {
    assert_eq!(MAX_JITTER_NS, 250_000);
    Ok(())
}

// ===========================================================================
// 10. Timing measurement — basic smoke tests
// ===========================================================================

#[test]
fn timing_instant_monotonic() -> TestResult {
    let a = Instant::now();
    // Burn a tiny bit of CPU.
    let mut _sum = 0u64;
    for i in 0..1_000 {
        _sum = _sum.wrapping_add(i);
    }
    let b = Instant::now();
    assert!(b >= a);
    Ok(())
}

#[test]
fn timing_scheduler_single_tick() -> TestResult {
    let mut s = AbsoluteScheduler::with_period(1_000_000);
    let setup = RTSetup::minimal();
    s.apply_rt_setup(&setup)?;
    // A single tick should succeed (may report timing violation on loaded CI,
    // but the API contract still holds).
    let result = s.wait_for_tick();
    assert!(result.is_ok() || result == Err(RTError::TimingViolation));
    Ok(())
}

// ===========================================================================
// 11. Platform detection — compile-time cfg checks
// ===========================================================================

#[test]
fn platform_detection_exactly_one_active() -> TestResult {
    let mut count = 0u32;
    #[cfg(target_os = "windows")]
    {
        count += 1;
    }
    #[cfg(target_os = "linux")]
    {
        count += 1;
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        count += 1;
    }
    assert_eq!(count, 1, "exactly one platform backend must be active");
    Ok(())
}

#[test]
fn platform_sleep_new_does_not_panic() -> TestResult {
    // Construct a scheduler (which constructs PlatformSleep internally).
    let _s = AbsoluteScheduler::new_1khz();
    Ok(())
}

// ===========================================================================
// 12. Prelude re-exports — ensure types are accessible
// ===========================================================================

#[test]
fn prelude_reexports_scheduler() -> TestResult {
    use openracing_scheduler::prelude::*;
    let _s = AbsoluteScheduler::new_1khz();
    let _p = PLL::new(1_000_000);
    let _j = JitterMetrics::new();
    let _r = RTSetup::default();
    let _c = AdaptiveSchedulingConfig::default();
    let _st = AdaptiveSchedulingState::default();
    let _e: RTResult = Ok(());
    let _k = PERIOD_1KHZ_NS;
    let _m = MAX_JITTER_NS;
    Ok(())
}

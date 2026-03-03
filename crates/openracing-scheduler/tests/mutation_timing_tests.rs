//! Mutation-targeted timing tests for the scheduler crate.
//!
//! Each test is designed to catch a specific class of mutation that
//! cargo-mutants might introduce in timing-critical code paths:
//!
//! - Sign/direction errors in PLL correction
//! - Off-by-one in tick counters and percentile calculations
//! - Removed bounds checks on period or jitter
//! - Swapped comparisons in deadline detection
//! - Replaced constants (0 vs 1, true vs false)

use openracing_scheduler::{
    AbsoluteScheduler, AdaptiveSchedulingConfig, JitterMetrics, PERIOD_1KHZ_NS, PLL,
};

// ===========================================================================
// 1. PLL — target period identity
// ===========================================================================

#[test]
fn pll_new_has_correct_target() -> Result<(), Box<dyn std::error::Error>> {
    let pll = PLL::new(PERIOD_1KHZ_NS);
    assert_eq!(
        pll.target_period_ns(),
        PERIOD_1KHZ_NS,
        "target period must match constructor argument"
    );
    Ok(())
}

#[test]
fn pll_estimated_period_starts_at_target() -> Result<(), Box<dyn std::error::Error>> {
    let pll = PLL::new(PERIOD_1KHZ_NS);
    assert_eq!(
        pll.estimated_period_ns(),
        PERIOD_1KHZ_NS,
        "estimated period should start at target"
    );
    Ok(())
}

// ===========================================================================
// 2. PLL — convergence direction (sign mutation)
// ===========================================================================

#[test]
fn pll_positive_drift_moves_estimate_down() -> Result<(), Box<dyn std::error::Error>> {
    let mut pll = PLL::new(PERIOD_1KHZ_NS);
    // Feed intervals that are 5% too long — PLL should lower its estimated period
    // to compensate (correction is positive, estimated = target - correction)
    let drift_ns = PERIOD_1KHZ_NS + PERIOD_1KHZ_NS / 20;
    for _ in 0..100 {
        let _ = pll.update(drift_ns);
    }
    let estimated = pll.estimated_period_ns();
    assert!(
        estimated < PERIOD_1KHZ_NS,
        "PLL with slow intervals should lower estimate: got {estimated}"
    );
    Ok(())
}

#[test]
fn pll_negative_drift_moves_estimate_up() -> Result<(), Box<dyn std::error::Error>> {
    let mut pll = PLL::new(PERIOD_1KHZ_NS);
    // Feed intervals that are 5% too short — PLL correction goes negative,
    // so estimated = target - negative = above target
    let drift_ns = PERIOD_1KHZ_NS - PERIOD_1KHZ_NS / 20;
    for _ in 0..100 {
        let _ = pll.update(drift_ns);
    }
    let estimated = pll.estimated_period_ns();
    assert!(
        estimated > PERIOD_1KHZ_NS,
        "PLL with fast intervals should raise estimate: got {estimated}"
    );
    Ok(())
}

// ===========================================================================
// 3. PLL — stability check
// ===========================================================================

#[test]
fn pll_exact_intervals_reach_stable() -> Result<(), Box<dyn std::error::Error>> {
    let mut pll = PLL::new(PERIOD_1KHZ_NS);
    for _ in 0..500 {
        let _ = pll.update(PERIOD_1KHZ_NS);
    }
    assert!(pll.is_stable(), "PLL fed exact target must be stable");
    Ok(())
}

#[test]
fn pll_opposite_drift_directions_produce_opposite_estimates() -> Result<(), Box<dyn std::error::Error>> {
    let mut pll_slow = PLL::new(PERIOD_1KHZ_NS);
    let mut pll_fast = PLL::new(PERIOD_1KHZ_NS);
    let slow_ns = PERIOD_1KHZ_NS + PERIOD_1KHZ_NS / 10;
    let fast_ns = PERIOD_1KHZ_NS - PERIOD_1KHZ_NS / 10;
    for _ in 0..100 {
        let _ = pll_slow.update(slow_ns);
        let _ = pll_fast.update(fast_ns);
    }
    assert!(
        pll_slow.estimated_period_ns() < pll_fast.estimated_period_ns(),
        "slow drift estimate ({}) must be < fast drift estimate ({})",
        pll_slow.estimated_period_ns(),
        pll_fast.estimated_period_ns()
    );
    Ok(())
}

// ===========================================================================
// 4. PLL — reset zeroes state
// ===========================================================================

#[test]
fn pll_reset_clears_error() -> Result<(), Box<dyn std::error::Error>> {
    let mut pll = PLL::new(PERIOD_1KHZ_NS);
    for _ in 0..100 {
        let _ = pll.update(PERIOD_1KHZ_NS + 50_000);
    }
    pll.reset();
    assert!(
        pll.phase_error_ns().abs() < 1.0,
        "phase error should be ~0 after reset: got {}",
        pll.phase_error_ns()
    );
    Ok(())
}

// ===========================================================================
// 5. JitterMetrics — missed tick counting
// ===========================================================================

#[test]
fn jitter_metrics_no_misses_yields_zero_rate() -> Result<(), Box<dyn std::error::Error>> {
    let mut m = JitterMetrics::new();
    for _ in 0..1000 {
        m.record_tick(100, false);
    }
    assert!(
        m.missed_tick_rate() < f64::EPSILON,
        "no missed ticks should yield zero rate: got {}",
        m.missed_tick_rate()
    );
    Ok(())
}

#[test]
fn jitter_metrics_all_missed_yields_one() -> Result<(), Box<dyn std::error::Error>> {
    let mut m = JitterMetrics::new();
    for _ in 0..100 {
        m.record_tick(100, true);
    }
    assert!(
        (m.missed_tick_rate() - 1.0).abs() < f64::EPSILON,
        "all missed should yield rate=1.0: got {}",
        m.missed_tick_rate()
    );
    Ok(())
}

#[test]
fn jitter_metrics_half_missed() -> Result<(), Box<dyn std::error::Error>> {
    let mut m = JitterMetrics::new();
    for i in 0..1000 {
        m.record_tick(100, i % 2 == 0);
    }
    let rate = m.missed_tick_rate();
    assert!(
        (rate - 0.5).abs() < 0.01,
        "50% missed should yield ~0.5: got {rate}"
    );
    Ok(())
}

// ===========================================================================
// 6. JitterMetrics — percentile monotonicity
// ===========================================================================

#[test]
fn jitter_percentiles_are_monotonic() -> Result<(), Box<dyn std::error::Error>> {
    let mut m = JitterMetrics::new();
    // Record increasing jitter values
    for i in 0..1000u64 {
        m.record_tick(i * 100, false);
    }
    let p50 = m.p50_jitter_ns();
    let p95 = m.p95_jitter_ns();
    let p99 = m.p99_jitter_ns();
    assert!(
        p50 <= p95,
        "p50 ({p50}) must be <= p95 ({p95})"
    );
    assert!(
        p95 <= p99,
        "p95 ({p95}) must be <= p99 ({p99})"
    );
    Ok(())
}

// ===========================================================================
// 7. JitterMetrics — max jitter tracks worst case
// ===========================================================================

#[test]
fn jitter_max_tracks_worst_case() -> Result<(), Box<dyn std::error::Error>> {
    let mut m = JitterMetrics::new();
    m.record_tick(100, false);
    m.record_tick(999_999, false);
    m.record_tick(200, false);
    assert_eq!(
        m.max_jitter_ns, 999_999,
        "max jitter must track worst case"
    );
    Ok(())
}

#[test]
fn jitter_last_jitter_is_most_recent() -> Result<(), Box<dyn std::error::Error>> {
    let mut m = JitterMetrics::new();
    m.record_tick(100, false);
    m.record_tick(42_000, false);
    assert_eq!(m.last_jitter_ns, 42_000, "last_jitter must be most recent");
    Ok(())
}

// ===========================================================================
// 8. JitterMetrics — meets_requirements boundary
// ===========================================================================

#[test]
fn jitter_meets_requirements_with_low_jitter() -> Result<(), Box<dyn std::error::Error>> {
    let mut m = JitterMetrics::new();
    // All ticks well under 250µs = 250_000ns
    for _ in 0..1000 {
        m.record_tick(10_000, false);
    }
    assert!(
        m.meets_requirements(),
        "low jitter with no misses should meet requirements"
    );
    Ok(())
}

#[test]
fn jitter_fails_requirements_with_high_jitter() -> Result<(), Box<dyn std::error::Error>> {
    let mut m = JitterMetrics::new();
    // All ticks at 500µs — above p99 limit of 250µs
    for _ in 0..1000 {
        m.record_tick(500_000, false);
    }
    assert!(
        !m.meets_requirements(),
        "500µs jitter should fail the 250µs p99 requirement"
    );
    Ok(())
}

// ===========================================================================
// 9. JitterMetrics — reset clears state
// ===========================================================================

#[test]
fn jitter_reset_clears_all_state() -> Result<(), Box<dyn std::error::Error>> {
    let mut m = JitterMetrics::new();
    for _ in 0..100 {
        m.record_tick(100_000, true);
    }
    m.reset();
    assert_eq!(m.total_ticks, 0, "total_ticks must be zero after reset");
    assert_eq!(m.missed_ticks, 0, "missed_ticks must be zero after reset");
    assert_eq!(m.max_jitter_ns, 0, "max_jitter must be zero after reset");
    assert_eq!(m.last_jitter_ns, 0, "last_jitter must be zero after reset");
    Ok(())
}

// ===========================================================================
// 10. JitterMetrics — variance / std dev
// ===========================================================================

#[test]
fn jitter_constant_samples_have_lower_variance_than_varied() -> Result<(), Box<dyn std::error::Error>> {
    let mut constant = JitterMetrics::new();
    for _ in 0..100 {
        constant.record_tick(50_000, false);
    }
    let mut varied = JitterMetrics::new();
    for i in 0..100u64 {
        varied.record_tick(i * 5000, false);
    }
    // Constant samples should have lower std dev than varied samples
    let constant_std = constant.jitter_std_dev_ns();
    let varied_std = varied.jitter_std_dev_ns();
    assert!(
        varied_std > constant_std,
        "varied jitter std dev ({varied_std:.0}) must exceed constant ({constant_std:.0})"
    );
    Ok(())
}

#[test]
fn jitter_varied_samples_have_positive_variance() -> Result<(), Box<dyn std::error::Error>> {
    let mut m = JitterMetrics::new();
    for i in 0..100u64 {
        m.record_tick(i * 1000, false);
    }
    let var = m.jitter_variance();
    assert!(
        var > 0.0,
        "varied jitter should have positive variance: got {var}"
    );
    Ok(())
}

// ===========================================================================
// 11. AbsoluteScheduler — period preservation
// ===========================================================================

#[test]
fn scheduler_1khz_has_correct_period() -> Result<(), Box<dyn std::error::Error>> {
    let sched = AbsoluteScheduler::new_1khz();
    assert_eq!(
        sched.period_ns(),
        PERIOD_1KHZ_NS,
        "1kHz scheduler must have 1ms period"
    );
    Ok(())
}

#[test]
fn scheduler_custom_period_preserved() -> Result<(), Box<dyn std::error::Error>> {
    let period = 2_000_000u64; // 2ms = 500Hz
    let sched = AbsoluteScheduler::with_period(period);
    assert_eq!(
        sched.period_ns(),
        period,
        "custom period must be preserved"
    );
    Ok(())
}

// ===========================================================================
// 12. AbsoluteScheduler — tick count starts at zero
// ===========================================================================

#[test]
fn scheduler_tick_count_starts_at_zero() -> Result<(), Box<dyn std::error::Error>> {
    let sched = AbsoluteScheduler::new_1khz();
    assert_eq!(sched.tick_count(), 0, "tick count must start at zero");
    Ok(())
}

// ===========================================================================
// 13. AdaptiveSchedulingConfig — validation
// ===========================================================================

#[test]
fn adaptive_config_default_is_valid() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = AdaptiveSchedulingConfig::new();
    assert!(cfg.is_valid(), "default config must be valid");
    Ok(())
}

#[test]
fn adaptive_config_inverted_bounds_is_invalid() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = AdaptiveSchedulingConfig::new()
        .with_period_bounds(1_100_000, 900_000); // min > max
    assert!(!cfg.is_valid(), "inverted bounds must be invalid");
    Ok(())
}

#[test]
fn adaptive_config_normalize_fixes_inverted_thresholds() -> Result<(), Box<dyn std::error::Error>> {
    let mut cfg = AdaptiveSchedulingConfig::new()
        .with_period_bounds(1_100_000, 900_000);
    cfg.normalize();
    assert!(cfg.is_valid(), "normalize should fix inverted bounds");
    Ok(())
}

// ===========================================================================
// 14. AdaptiveSchedulingState — boundary detection
// ===========================================================================

#[test]
fn adaptive_state_period_fraction_boundaries() -> Result<(), Box<dyn std::error::Error>> {
    let mut cfg = AdaptiveSchedulingConfig::enabled();
    cfg.normalize();

    // Create a scheduler with adaptive config and check the state
    let mut sched = AbsoluteScheduler::new_1khz();
    sched.set_adaptive_scheduling(cfg);
    let state = sched.adaptive_scheduling();

    let frac = state.period_fraction();
    assert!(
        (0.0..=1.0).contains(&frac),
        "period fraction must be in [0, 1]: got {frac}"
    );
    Ok(())
}

// ===========================================================================
// 15. JitterMetrics — custom requirements
// ===========================================================================

#[test]
fn jitter_custom_requirements_strict() -> Result<(), Box<dyn std::error::Error>> {
    let mut m = JitterMetrics::new();
    for _ in 0..1000 {
        m.record_tick(100_000, false); // 100µs jitter
    }
    // With a very strict threshold (50µs p99), should fail
    assert!(
        !m.meets_custom_requirements(50_000, 0.001),
        "100µs jitter should fail 50µs requirement"
    );
    // With relaxed threshold (200µs p99), should pass
    assert!(
        m.meets_custom_requirements(200_000, 0.001),
        "100µs jitter should pass 200µs requirement"
    );
    Ok(())
}

// ===========================================================================
// 16. JitterMetrics — empty metrics edge case
// ===========================================================================

#[test]
fn jitter_empty_metrics_do_not_panic() -> Result<(), Box<dyn std::error::Error>> {
    let mut m = JitterMetrics::new();
    // These should all handle zero samples gracefully
    assert_eq!(m.total_ticks, 0);
    assert_eq!(m.missed_ticks, 0);
    let _ = m.p50_jitter_ns();
    let _ = m.p95_jitter_ns();
    let _ = m.p99_jitter_ns();
    let _ = m.jitter_variance();
    let _ = m.jitter_std_dev_ns();
    let _ = m.average_jitter_ns();
    Ok(())
}

// ===========================================================================
// 17. PERIOD_1KHZ_NS constant — must be exactly 1ms
// ===========================================================================

#[test]
fn period_1khz_ns_is_one_millisecond() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        PERIOD_1KHZ_NS, 1_000_000,
        "PERIOD_1KHZ_NS must be exactly 1,000,000 ns (1ms)"
    );
    Ok(())
}

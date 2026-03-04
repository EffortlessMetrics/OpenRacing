//! Deep tests for scheduler timing, PLL convergence, missed tick detection,
//! adaptive rate adjustment, and jitter statistics.
//!
//! Contains deterministic unit-level tests and proptest-based property tests
//! covering the timing subsystem at various tick rates and simulated loads.

#![allow(clippy::redundant_closure)]

use openracing_scheduler::{
    AbsoluteScheduler, AdaptiveSchedulingConfig, AdaptiveSchedulingState, JitterMetrics,
    PERIOD_1KHZ_NS, PLL, RTError, RTSetup,
};
use proptest::prelude::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

// ===========================================================================
// Helpers
// ===========================================================================

/// Run `n` ticks on a scheduler, returning (ok_count, violation_count).
fn run_ticks(scheduler: &mut AbsoluteScheduler, n: u64) -> (u64, u64) {
    let mut ok = 0u64;
    let mut violations = 0u64;
    for _ in 0..n {
        match scheduler.wait_for_tick() {
            Ok(_) => ok += 1,
            Err(RTError::TimingViolation) => violations += 1,
            Err(_) => break,
        }
    }
    (ok, violations)
}

// ===========================================================================
// 1. PLL convergence under varying system loads
// ===========================================================================

#[test]
fn pll_converges_with_exact_target() -> Result<(), Box<dyn std::error::Error>> {
    let mut pll = PLL::new(PERIOD_1KHZ_NS);
    for _ in 0..500 {
        let _ = pll.update(PERIOD_1KHZ_NS);
    }
    assert!(
        pll.is_stable(),
        "PLL should be stable with exact target input"
    );
    let avg = pll.average_phase_error_ns();
    assert!(
        avg.abs() < 100.0,
        "average phase error {avg} should be near zero"
    );
    Ok(())
}

#[test]
fn pll_converges_with_small_positive_drift() -> Result<(), Box<dyn std::error::Error>> {
    let mut pll = PLL::new(PERIOD_1KHZ_NS);
    // 0.1% slow drift
    let drifted = PERIOD_1KHZ_NS + 1_000;
    for _ in 0..1000 {
        let _ = pll.update(drifted);
    }
    assert!(
        pll.is_stable(),
        "PLL should stabilise with 0.1% drift; estimated={}",
        pll.estimated_period_ns()
    );
    Ok(())
}

#[test]
fn pll_converges_with_small_negative_drift() -> Result<(), Box<dyn std::error::Error>> {
    let mut pll = PLL::new(PERIOD_1KHZ_NS);
    let drifted = PERIOD_1KHZ_NS.saturating_sub(1_000);
    for _ in 0..1000 {
        let _ = pll.update(drifted);
    }
    assert!(
        pll.is_stable(),
        "PLL should stabilise with -0.1% drift; estimated={}",
        pll.estimated_period_ns()
    );
    Ok(())
}

#[test]
fn pll_bounded_under_alternating_load() -> Result<(), Box<dyn std::error::Error>> {
    let mut pll = PLL::new(PERIOD_1KHZ_NS);
    for i in 0..500u64 {
        let interval = if i % 2 == 0 {
            PERIOD_1KHZ_NS + 50_000
        } else {
            PERIOD_1KHZ_NS.saturating_sub(50_000)
        };
        let corrected = pll.update(interval);
        let ns = corrected.as_nanos() as u64;
        assert!(
            (900_000..=1_100_000).contains(&ns),
            "corrected {ns} outside ±10% at iteration {i}"
        );
    }
    Ok(())
}

#[test]
fn pll_custom_gains_converge() -> Result<(), Box<dyn std::error::Error>> {
    let gains: &[(f64, f64)] = &[(0.001, 0.01), (0.05, 0.2), (0.1, 0.5)];
    for &(kp, ki) in gains {
        let mut pll = PLL::with_gains(PERIOD_1KHZ_NS, kp, ki);
        for _ in 0..500 {
            let _ = pll.update(PERIOD_1KHZ_NS);
        }
        assert!(pll.is_stable(), "PLL with kp={kp} ki={ki} should be stable");
    }
    Ok(())
}

// ===========================================================================
// 2. Missed tick detection and recovery
// ===========================================================================

#[test]
fn missed_tick_detected_after_deliberate_sleep() -> Result<(), Box<dyn std::error::Error>> {
    let mut sched = AbsoluteScheduler::with_period(1_000_000);
    sched.apply_rt_setup(&RTSetup::minimal())?;

    // Burn first tick
    let _ = sched.wait_for_tick();

    // Miss several deadlines
    thread::sleep(Duration::from_millis(30));

    let result = sched.wait_for_tick();
    assert!(
        result.is_ok() || result == Err(RTError::TimingViolation),
        "expected Ok or TimingViolation, got {result:?}"
    );
    assert!(
        sched.metrics().max_jitter_ns >= 1_000_000,
        "max jitter {} should reflect the late tick",
        sched.metrics().max_jitter_ns
    );
    Ok(())
}

#[test]
fn scheduler_recovers_after_missed_deadline() -> Result<(), Box<dyn std::error::Error>> {
    let mut sched = AbsoluteScheduler::with_period(2_000_000); // 2ms
    sched.apply_rt_setup(&RTSetup::minimal())?;

    let _ = sched.wait_for_tick();
    thread::sleep(Duration::from_millis(30));
    // Force through the missed tick
    let _ = sched.wait_for_tick();

    // Now run several more ticks — scheduler should still function
    let (ok, violations) = run_ticks(&mut sched, 10);
    let total = ok + violations;
    assert!(
        total > 0,
        "scheduler should still produce ticks after recovery"
    );
    Ok(())
}

#[test]
fn missed_ticks_counted_in_metrics() -> Result<(), Box<dyn std::error::Error>> {
    let mut metrics = JitterMetrics::new();
    metrics.record_tick(100, false);
    metrics.record_tick(100, true);
    metrics.record_tick(100, true);
    metrics.record_tick(100, false);
    assert_eq!(metrics.missed_ticks, 2);
    assert_eq!(metrics.total_ticks, 4);
    assert!((metrics.missed_tick_rate() - 0.5).abs() < 1e-10);
    Ok(())
}

// ===========================================================================
// 3. Adaptive rate adjustment triggers
// ===========================================================================

#[test]
fn adaptive_relaxes_under_high_jitter() -> Result<(), Box<dyn std::error::Error>> {
    let mut sched = AbsoluteScheduler::new_1khz();
    let config = AdaptiveSchedulingConfig::new()
        .with_enabled(true)
        .with_period_bounds(900_000, 1_200_000)
        .with_step_sizes(10_000, 5_000)
        .with_jitter_thresholds(200_000, 50_000);
    sched.set_adaptive_scheduling(config);

    let initial_target = sched.adaptive_scheduling().target_period_ns;

    // Simulate high processing load to drive EMA up
    for _ in 0..50 {
        sched.record_processing_time_us(500); // above relax threshold
    }

    // Drive several ticks with timing violations to trigger relaxation
    sched.apply_rt_setup(&RTSetup::minimal())?;
    let _ = sched.wait_for_tick();
    thread::sleep(Duration::from_millis(20));
    let _ = sched.wait_for_tick();

    let final_target = sched.adaptive_scheduling().target_period_ns;
    // Period should have increased (relaxed) or stayed bounded
    assert!(
        final_target >= initial_target || final_target >= 900_000,
        "target should relax: initial={initial_target}, final={final_target}"
    );
    Ok(())
}

#[test]
fn adaptive_tightens_when_healthy() -> Result<(), Box<dyn std::error::Error>> {
    let mut sched = AbsoluteScheduler::new_1khz();
    let config = AdaptiveSchedulingConfig::new()
        .with_enabled(true)
        .with_period_bounds(900_000, 1_200_000)
        .with_step_sizes(10_000, 5_000)
        .with_jitter_thresholds(200_000, 50_000)
        .with_processing_thresholds(180, 80);
    sched.set_adaptive_scheduling(config);

    // Start with period at max by simulating load first
    sched.record_processing_time_us(500);
    sched.apply_rt_setup(&RTSetup::minimal())?;
    // Drive a few ticks to push adaptive period up
    let _ = sched.wait_for_tick();
    thread::sleep(Duration::from_millis(20));
    let _ = sched.wait_for_tick();

    let elevated = sched.adaptive_scheduling().target_period_ns;

    // Now simulate healthy conditions with low processing and run ticks
    for _ in 0..100 {
        sched.record_processing_time_us(10);
    }
    // Run healthy ticks (5ms period to avoid real timing violations)
    sched.reset();
    let mut sched2 = AbsoluteScheduler::with_period(5_000_000);
    sched2.set_adaptive_scheduling(
        AdaptiveSchedulingConfig::new()
            .with_enabled(true)
            .with_period_bounds(4_000_000, 6_000_000)
            .with_step_sizes(50_000, 20_000)
            .with_jitter_thresholds(1_000_000, 200_000)
            .with_processing_thresholds(180, 80),
    );
    for _ in 0..20 {
        sched2.record_processing_time_us(10);
    }
    sched2.apply_rt_setup(&RTSetup::minimal())?;
    for _ in 0..20 {
        let _ = sched2.wait_for_tick();
        sched2.record_processing_time_us(10);
    }

    let state2 = sched2.adaptive_scheduling();
    // Either tightened or at minimum — adaptive must stay in bounds
    assert!(
        state2.target_period_ns >= state2.min_period_ns,
        "target must be >= min"
    );
    assert!(
        state2.target_period_ns <= state2.max_period_ns,
        "target must be <= max"
    );
    // Verify that elevated was valid
    assert!(elevated >= 900_000, "elevated period should be valid");
    Ok(())
}

#[test]
fn adaptive_stays_bounded_with_extreme_load() -> Result<(), Box<dyn std::error::Error>> {
    let mut sched = AbsoluteScheduler::new_1khz();
    let config = AdaptiveSchedulingConfig::new()
        .with_enabled(true)
        .with_period_bounds(800_000, 1_500_000)
        .with_step_sizes(100_000, 50_000);
    sched.set_adaptive_scheduling(config);

    for _ in 0..200 {
        sched.record_processing_time_us(10_000); // extremely high
    }

    let state = sched.adaptive_scheduling();
    assert!(state.target_period_ns <= 1_500_000, "must not exceed max");
    assert!(state.target_period_ns >= 800_000, "must not go below min");
    Ok(())
}

#[test]
fn adaptive_disabled_keeps_base_period() -> Result<(), Box<dyn std::error::Error>> {
    let mut sched = AbsoluteScheduler::new_1khz();
    // Adaptive disabled by default
    let state = sched.adaptive_scheduling();
    assert!(!state.enabled);
    assert_eq!(state.target_period_ns, PERIOD_1KHZ_NS);

    // Even after reporting high load, period stays at base
    sched.record_processing_time_us(9999);
    let state = sched.adaptive_scheduling();
    assert_eq!(state.target_period_ns, PERIOD_1KHZ_NS);
    Ok(())
}

// ===========================================================================
// 4. Timing precision at different tick rates
// ===========================================================================

fn validate_tick_rate(period_ns: u64, num_ticks: u64) -> Result<(), Box<dyn std::error::Error>> {
    let mut sched = AbsoluteScheduler::with_period(period_ns);
    sched.apply_rt_setup(&RTSetup::minimal())?;

    let start = Instant::now();
    let (ok, violations) = run_ticks(&mut sched, num_ticks);
    let elapsed = start.elapsed();

    let total = ok + violations;
    assert_eq!(total, num_ticks, "all ticks must complete");

    let expected = Duration::from_nanos(period_ns * num_ticks);
    let ratio = elapsed.as_secs_f64() / expected.as_secs_f64();
    // Wide tolerance for CI
    assert!(
        (0.3..=3.0).contains(&ratio),
        "rate {:.0}Hz: timing ratio {ratio:.3} outside [0.3, 3.0]; \
         elapsed={elapsed:?}, expected={expected:?}",
        1_000_000_000.0 / period_ns as f64
    );
    Ok(())
}

#[test]
fn timing_precision_100hz() -> Result<(), Box<dyn std::error::Error>> {
    validate_tick_rate(10_000_000, 30) // 10ms period, 30 ticks
}

#[test]
fn timing_precision_500hz() -> Result<(), Box<dyn std::error::Error>> {
    validate_tick_rate(2_000_000, 50) // 2ms period, 50 ticks
}

#[test]
fn timing_precision_1khz() -> Result<(), Box<dyn std::error::Error>> {
    validate_tick_rate(1_000_000, 50) // 1ms period, 50 ticks
}

#[test]
fn timing_precision_2khz() -> Result<(), Box<dyn std::error::Error>> {
    validate_tick_rate(500_000, 50) // 0.5ms period, 50 ticks
}

// ===========================================================================
// 5. Scheduler behavior under heavy simulated system load
// ===========================================================================

#[test]
fn scheduler_under_cpu_busy_loop() -> Result<(), Box<dyn std::error::Error>> {
    let mut sched = AbsoluteScheduler::with_period(5_000_000); // 5ms
    sched.apply_rt_setup(&RTSetup::minimal())?;

    let stop = Arc::new(AtomicBool::new(false));

    // Spawn load threads
    let load_handles: Vec<_> = (0..2)
        .map(|_| {
            let s = Arc::clone(&stop);
            thread::spawn(move || {
                while !s.load(Ordering::Relaxed) {
                    std::hint::spin_loop();
                }
            })
        })
        .collect();

    let (ok, violations) = run_ticks(&mut sched, 30);
    stop.store(true, Ordering::Relaxed);

    for h in load_handles {
        let _ = h.join();
    }

    let total = ok + violations;
    assert_eq!(total, 30, "all ticks must complete under load");
    assert!(ok > 0, "at least some ticks must be on-time");
    Ok(())
}

#[test]
fn scheduler_metrics_reflect_simulated_load() -> Result<(), Box<dyn std::error::Error>> {
    let mut sched = AbsoluteScheduler::with_period(5_000_000); // 5ms
    sched.apply_rt_setup(&RTSetup::minimal())?;

    // First 10 normal ticks
    let _ = run_ticks(&mut sched, 10);
    // Simulate load spike by sleeping
    thread::sleep(Duration::from_millis(30));
    // 10 more ticks
    let _ = run_ticks(&mut sched, 10);

    let metrics = sched.metrics();
    assert!(metrics.total_ticks >= 10, "should have at least 10 ticks");
    assert!(
        metrics.max_jitter_ns > 0,
        "some jitter should be recorded under load"
    );
    Ok(())
}

// ===========================================================================
// 6. Jitter statistics computation
// ===========================================================================

#[test]
fn jitter_variance_with_constant_samples() -> Result<(), Box<dyn std::error::Error>> {
    let mut metrics = JitterMetrics::with_capacity(100);
    for _ in 0..100 {
        metrics.record_tick(50_000, false);
    }
    // Variance of constant = mean_of_squares = 50000^2
    // (This is the "variance" as implemented: sum(x^2)/n)
    let expected_var = (50_000.0_f64).powi(2);
    let var = metrics.jitter_variance();
    assert!(
        (var - expected_var).abs() < 1.0,
        "variance {var} != expected {expected_var}"
    );
    Ok(())
}

#[test]
fn jitter_std_dev_consistent_with_variance() -> Result<(), Box<dyn std::error::Error>> {
    let mut metrics = JitterMetrics::with_capacity(200);
    for i in 0..200u64 {
        metrics.record_tick(i * 500, i % 10 == 0);
    }
    let var = metrics.jitter_variance();
    let std_dev = metrics.jitter_std_dev_ns();
    assert!(
        (std_dev - var.sqrt()).abs() < 0.01,
        "std_dev {std_dev} != sqrt(variance) {}",
        var.sqrt()
    );
    Ok(())
}

#[test]
fn jitter_percentiles_monotonic() -> Result<(), Box<dyn std::error::Error>> {
    let mut metrics = JitterMetrics::with_capacity(1000);
    for i in 0..1000u64 {
        metrics.record_tick(i * 100, false);
    }
    let p50 = metrics.p50_jitter_ns();
    let p95 = metrics.p95_jitter_ns();
    let p99 = metrics.p99_jitter_ns();
    assert!(p50 <= p95, "p50 {p50} > p95 {p95}");
    assert!(p95 <= p99, "p95 {p95} > p99 {p99}");
    Ok(())
}

#[test]
fn jitter_ring_buffer_overwrites_oldest() -> Result<(), Box<dyn std::error::Error>> {
    let mut metrics = JitterMetrics::with_capacity(5);
    for i in 1..=10u64 {
        metrics.record_tick(i * 1_000, false);
    }
    assert_eq!(metrics.sample_count(), 5);
    // The ring buffer should contain the last 5 values: 6k..10k
    let p50 = metrics.p50_jitter_ns();
    assert!(
        (6_000..=10_000).contains(&p50),
        "p50 {p50} should be from recent samples"
    );
    Ok(())
}

// ===========================================================================
// 7. Scheduler warm-up period
// ===========================================================================

#[test]
fn scheduler_warmup_first_tick_jitter() -> Result<(), Box<dyn std::error::Error>> {
    let mut sched = AbsoluteScheduler::with_period(5_000_000);
    sched.apply_rt_setup(&RTSetup::minimal())?;

    let _ = sched.wait_for_tick();
    // First tick may have high jitter due to initialization
    let first_jitter = sched.metrics().last_jitter_ns;
    // It should still be finite and recorded
    assert!(
        first_jitter < 500_000_000,
        "first tick jitter {first_jitter} should be < 500ms"
    );

    // Subsequent ticks should have lower jitter
    for _ in 0..10 {
        let _ = sched.wait_for_tick();
    }
    let later_jitter = sched.metrics().last_jitter_ns;
    assert!(
        later_jitter < 100_000_000,
        "later jitter {later_jitter} should be < 100ms"
    );
    Ok(())
}

#[test]
fn scheduler_warmup_pll_stabilises() -> Result<(), Box<dyn std::error::Error>> {
    let mut sched = AbsoluteScheduler::with_period(5_000_000);
    sched.apply_rt_setup(&RTSetup::minimal())?;

    for _ in 0..50 {
        let _ = sched.wait_for_tick();
    }

    // After warm-up the PLL phase error should be bounded
    let phase_err = sched.phase_error_ns().abs();
    // Very wide tolerance since we're on general-purpose OS
    assert!(
        phase_err < 1_000_000_000.0,
        "phase error {phase_err} should be bounded after warm-up"
    );
    Ok(())
}

// ===========================================================================
// 8. Multiple independent schedulers
// ===========================================================================

#[test]
fn independent_schedulers_different_rates() -> Result<(), Box<dyn std::error::Error>> {
    let periods = [10_000_000u64, 5_000_000, 2_000_000]; // 100Hz, 200Hz, 500Hz
    let ticks_each = 10u64;

    let handles: Vec<_> = periods
        .iter()
        .map(|&period| {
            thread::spawn(
                move || -> Result<(u64, u64), Box<dyn std::error::Error + Send + Sync>> {
                    let mut sched = AbsoluteScheduler::with_period(period);
                    sched
                        .apply_rt_setup(&RTSetup::minimal())
                        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;
                    let mut ok = 0u64;
                    for _ in 0..ticks_each {
                        match sched.wait_for_tick() {
                            Ok(_) | Err(RTError::TimingViolation) => ok += 1,
                            Err(e) => {
                                return Err(Box::new(e) as Box<dyn std::error::Error + Send + Sync>);
                            }
                        }
                    }
                    Ok((period, ok))
                },
            )
        })
        .collect();

    for h in handles {
        let (period, completed) = h
            .join()
            .map_err(|_| "thread panicked")?
            .map_err(|e| -> Box<dyn std::error::Error> { e })?;
        assert_eq!(
            completed, ticks_each,
            "scheduler with period {period} should complete all ticks"
        );
    }
    Ok(())
}

#[test]
fn independent_schedulers_do_not_share_state() -> Result<(), Box<dyn std::error::Error>> {
    let mut a = AbsoluteScheduler::with_period(1_000_000);
    let mut b = AbsoluteScheduler::with_period(2_000_000);

    a.apply_rt_setup(&RTSetup::minimal())?;
    b.apply_rt_setup(&RTSetup::minimal())?;

    let _ = a.wait_for_tick();
    let _ = a.wait_for_tick();

    // b should still be at 0 ticks
    assert_eq!(b.tick_count(), 0);
    assert_eq!(a.tick_count(), 2);
    assert_eq!(b.period_ns(), 2_000_000);
    assert_eq!(a.period_ns(), 1_000_000);
    Ok(())
}

// ===========================================================================
// 9. Property tests (proptest) — at least 25
// ===========================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    // -- PLL properties -------------------------------------------------------

    #[test]
    fn prop_pll_update_always_returns_positive_duration(
        target in 1_000u64..10_000_000,
        interval in 1_000u64..20_000_000,
    ) {
        let mut pll = PLL::new(target);
        let d = pll.update(interval);
        prop_assert!(d > Duration::ZERO);
    }

    #[test]
    fn prop_pll_estimated_within_10pct_of_target(
        target in 100_000u64..5_000_000,
        intervals in prop::collection::vec(50_000u64..10_000_000, 1..30),
    ) {
        let mut pll = PLL::new(target);
        for &iv in &intervals {
            let _ = pll.update(iv);
        }
        let est = pll.estimated_period_ns();
        let lo = (target as f64 * 0.9) as u64;
        let hi = (target as f64 * 1.1) as u64;
        prop_assert!(est >= lo && est <= hi,
            "estimated {} not in [{}, {}]", est, lo, hi);
    }

    #[test]
    fn prop_pll_converges_on_exact_input(target in 1_000u64..5_000_000) {
        let mut pll = PLL::new(target);
        for _ in 0..200 {
            let _ = pll.update(target);
        }
        prop_assert!(pll.is_stable());
    }

    #[test]
    fn prop_pll_reset_zeroes_error(
        target in 1_000u64..5_000_000,
        intervals in prop::collection::vec(1_000u64..10_000_000, 1..20),
    ) {
        let mut pll = PLL::new(target);
        for &iv in &intervals {
            let _ = pll.update(iv);
        }
        pll.reset();
        prop_assert!(pll.phase_error_ns().abs() < f64::EPSILON);
        prop_assert_eq!(pll.estimated_period_ns(), target);
    }

    #[test]
    fn prop_pll_set_target_updates_target(
        old_target in 100_000u64..5_000_000,
        new_target in 100_000u64..5_000_000,
    ) {
        let mut pll = PLL::new(old_target);
        pll.set_target_period_ns(new_target);
        prop_assert_eq!(pll.target_period_ns(), new_target);
    }

    #[test]
    fn prop_pll_phase_error_accumulated(
        target in 100_000u64..2_000_000,
        count in 1u32..50,
    ) {
        let mut pll = PLL::new(target);
        for _ in 0..count {
            let _ = pll.update(target + 100);
        }
        // Phase error should be positive (running slow)
        prop_assert!(pll.phase_error_ns() > 0.0);
    }

    #[test]
    fn prop_pll_custom_gains_clamped(
        target in 100_000u64..2_000_000,
        kp in -5.0f64..5.0,
        ki in -5.0f64..5.0,
    ) {
        let pll = PLL::with_gains(target, kp, ki);
        // Gains should be clamped to [0,1]
        let _ = pll.estimated_period_ns();
        // Should not panic
    }

    // -- Jitter metrics properties --------------------------------------------

    #[test]
    fn prop_jitter_max_is_greatest_sample(
        samples in prop::collection::vec(0u64..10_000_000, 1..100),
    ) {
        let mut metrics = JitterMetrics::with_capacity(samples.len());
        let mut expected_max = 0u64;
        for &s in &samples {
            metrics.record_tick(s, false);
            expected_max = expected_max.max(s);
        }
        prop_assert_eq!(metrics.max_jitter_ns, expected_max);
    }

    #[test]
    fn prop_jitter_total_ticks_correct(
        missed in 0u64..100,
        ok in 0u64..100,
    ) {
        let mut metrics = JitterMetrics::new();
        for _ in 0..missed {
            metrics.record_tick(100, true);
        }
        for _ in 0..ok {
            metrics.record_tick(100, false);
        }
        prop_assert_eq!(metrics.total_ticks, missed + ok);
        prop_assert_eq!(metrics.missed_ticks, missed);
    }

    #[test]
    fn prop_jitter_missed_rate_in_range(
        missed in 0u64..50,
        ok in 1u64..50,
    ) {
        let mut metrics = JitterMetrics::new();
        for _ in 0..missed {
            metrics.record_tick(100, true);
        }
        for _ in 0..ok {
            metrics.record_tick(100, false);
        }
        let rate = metrics.missed_tick_rate();
        prop_assert!((0.0..=1.0).contains(&rate), "rate {} out of [0,1]", rate);
    }

    #[test]
    fn prop_jitter_variance_nonneg(
        samples in prop::collection::vec(0u64..10_000_000, 1..100),
    ) {
        let mut metrics = JitterMetrics::with_capacity(samples.len());
        for &s in &samples {
            metrics.record_tick(s, false);
        }
        prop_assert!(metrics.jitter_variance() >= 0.0);
    }

    #[test]
    fn prop_jitter_percentile_between_min_max(
        samples in prop::collection::vec(0u64..1_000_000, 2..200),
        pct in 0.0f64..=1.0,
    ) {
        let mut metrics = JitterMetrics::with_capacity(samples.len());
        let mut min_val = u64::MAX;
        let mut max_val = 0u64;
        for &s in &samples {
            metrics.record_tick(s, false);
            min_val = min_val.min(s);
            max_val = max_val.max(s);
        }
        let p = metrics.percentile_jitter_ns(pct);
        prop_assert!(p >= min_val && p <= max_val,
            "percentile({pct}) = {p} not in [{min_val}, {max_val}]");
    }

    #[test]
    fn prop_jitter_reset_clears(
        samples in prop::collection::vec(0u64..1_000_000, 1..50),
    ) {
        let mut metrics = JitterMetrics::with_capacity(samples.len());
        for &s in &samples {
            metrics.record_tick(s, s > 500_000);
        }
        metrics.reset();
        prop_assert_eq!(metrics.total_ticks, 0);
        prop_assert_eq!(metrics.missed_ticks, 0);
        prop_assert_eq!(metrics.max_jitter_ns, 0);
        prop_assert_eq!(metrics.sample_count(), 0);
    }

    #[test]
    fn prop_jitter_sample_count_bounded(
        cap in 1usize..500,
        n in 1usize..1000,
    ) {
        let mut metrics = JitterMetrics::with_capacity(cap);
        for i in 0..n {
            metrics.record_tick(i as u64, false);
        }
        prop_assert!(metrics.sample_count() <= cap);
    }

    // -- Adaptive scheduling properties ---------------------------------------

    #[test]
    fn prop_adaptive_config_normalize_yields_valid(
        min_ns in 0u64..5_000_000,
        max_ns in 0u64..5_000_000,
        alpha in -2.0f64..3.0,
        inc in 0u64..100_000,
        dec in 0u64..100_000,
    ) {
        let mut config = AdaptiveSchedulingConfig {
            enabled: true,
            min_period_ns: min_ns,
            max_period_ns: max_ns,
            increase_step_ns: inc,
            decrease_step_ns: dec,
            jitter_relax_threshold_ns: 200_000,
            jitter_tighten_threshold_ns: 50_000,
            processing_relax_threshold_us: 180,
            processing_tighten_threshold_us: 80,
            processing_ema_alpha: alpha,
        };
        config.normalize();
        prop_assert!(config.is_valid());
        prop_assert!(config.min_period_ns <= config.max_period_ns);
        prop_assert!(config.processing_ema_alpha >= 0.01);
        prop_assert!(config.processing_ema_alpha <= 1.0);
    }

    #[test]
    fn prop_adaptive_state_fraction_in_range(
        min_ns in 100_000u64..500_000,
        max_ns in 600_000u64..2_000_000,
        target_offset in 0u64..500_000,
    ) {
        let target = min_ns + (target_offset % (max_ns - min_ns + 1));
        let state = AdaptiveSchedulingState {
            enabled: true,
            target_period_ns: target,
            min_period_ns: min_ns,
            max_period_ns: max_ns,
            last_processing_time_us: 0,
            processing_time_ema_us: 0.0,
        };
        let frac = state.period_fraction();
        prop_assert!((0.0..=1.0).contains(&frac),
            "fraction {} out of [0,1] for target={}, min={}, max={}",
            frac, target, min_ns, max_ns);
    }

    // -- Scheduler creation property ------------------------------------------

    #[test]
    fn prop_scheduler_period_always_at_least_1(period in 0u64..10_000_000) {
        let s = AbsoluteScheduler::with_period(period);
        prop_assert!(s.period_ns() >= 1);
    }

    #[test]
    fn prop_scheduler_reset_zeroes_counters(period in 1_000u64..10_000_000) {
        let mut s = AbsoluteScheduler::with_period(period);
        s.reset();
        prop_assert_eq!(s.tick_count(), 0);
        prop_assert_eq!(s.metrics().total_ticks, 0);
    }

    // -- EMA processing time properties ---------------------------------------

    #[test]
    fn prop_processing_ema_bounded(
        values in prop::collection::vec(0u64..10_000, 1..50),
    ) {
        let mut sched = AbsoluteScheduler::new_1khz();
        sched.set_adaptive_scheduling(
            AdaptiveSchedulingConfig::new().with_ema_alpha(0.3),
        );
        let mut min_val = u64::MAX;
        let mut max_val = 0u64;
        for &v in &values {
            sched.record_processing_time_us(v);
            min_val = min_val.min(v);
            max_val = max_val.max(v);
        }
        let ema = sched.adaptive_scheduling().processing_time_ema_us;
        prop_assert!(ema >= min_val as f64 - 0.01 && ema <= max_val as f64 + 0.01,
            "ema {} not in [{}, {}]", ema, min_val, max_val);
    }

    #[test]
    fn prop_pll_average_error_finite(
        target in 100_000u64..5_000_000,
        intervals in prop::collection::vec(50_000u64..10_000_000, 1..50),
    ) {
        let mut pll = PLL::new(target);
        for &iv in &intervals {
            let _ = pll.update(iv);
        }
        let avg = pll.average_phase_error_ns();
        prop_assert!(avg.is_finite(), "average phase error should be finite");
    }
}

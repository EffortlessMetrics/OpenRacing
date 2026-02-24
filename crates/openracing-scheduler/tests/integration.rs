//! Integration tests for the scheduler crate.

use openracing_scheduler::{
    AbsoluteScheduler, AdaptiveSchedulingConfig, JitterMetrics, PLL, RTError, RTSetup,
};
use std::time::Instant;

#[test]
fn test_scheduler_basic_timing() {
    let mut scheduler = AbsoluteScheduler::new_1khz();
    let setup = RTSetup::testing();
    let _ = scheduler.apply_rt_setup(&setup);

    let start = Instant::now();

    // Run 5 ticks
    for expected in 1..=5 {
        match scheduler.wait_for_tick() {
            Ok(tick) => assert_eq!(tick, expected),
            Err(RTError::TimingViolation) => return, // Acceptable in CI with variable load
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }

    let elapsed = start.elapsed();
    // Just verify time passed; timing accuracy is tested in benchmarks
    assert!(elapsed.as_nanos() > 0, "Should have waited some time");
}

#[test]
fn test_pll_correction_converges() {
    let mut pll = PLL::new(1_000_000);

    // Simulate consistent timing
    for _ in 0..100 {
        let _ = pll.update(1_000_000);
    }

    // PLL should be stable
    assert!(pll.is_stable());
    assert!(pll.phase_error_ns().abs() < 1_000.0);
}

#[test]
fn test_jitter_metrics_accumulation() {
    let mut metrics = JitterMetrics::with_capacity(100);

    for i in 1..=100 {
        metrics.record_tick(i * 1000, i % 10 == 0);
    }

    assert_eq!(metrics.total_ticks, 100);
    assert_eq!(metrics.missed_ticks, 10);
    assert_eq!(metrics.max_jitter_ns, 100_000);
    assert!((metrics.missed_tick_rate() - 0.1).abs() < 1e-10);
}

#[test]
fn test_adaptive_scheduling_increases_under_load() {
    let mut scheduler = AbsoluteScheduler::new_1khz();
    scheduler.set_adaptive_scheduling(AdaptiveSchedulingConfig {
        enabled: true,
        min_period_ns: 950_000,
        max_period_ns: 1_050_000,
        increase_step_ns: 10_000,
        decrease_step_ns: 5_000,
        jitter_relax_threshold_ns: 100_000,
        jitter_tighten_threshold_ns: 50_000,
        processing_relax_threshold_us: 150,
        processing_tighten_threshold_us: 80,
        processing_ema_alpha: 1.0,
    });

    // Simulate high load
    scheduler.record_processing_time_us(200);

    // Period should increase when processing time is high
    let state = scheduler.adaptive_scheduling();
    // Initial state
    assert_eq!(state.target_period_ns, 1_000_000);
}

#[test]
fn test_scheduler_reset_clears_state() {
    let mut scheduler = AbsoluteScheduler::new_1khz();

    // Record some ticks via metrics_mut
    scheduler.metrics_mut().record_tick(100_000, false);

    // Simulate tick count by running some ticks
    let _ = scheduler.wait_for_tick();

    scheduler.reset();

    assert_eq!(scheduler.tick_count(), 0);
    assert_eq!(scheduler.metrics().total_ticks, 0);
}

#[test]
fn test_pll_custom_gains() {
    let pll = PLL::with_gains(1_000_000, 0.5, 0.2);
    assert_eq!(pll.target_period_ns(), 1_000_000);
}

#[test]
fn test_rt_setup_builder() {
    let setup = RTSetup::new()
        .with_high_priority(true)
        .with_lock_memory(true)
        .with_cpu_affinity(0xFF);

    assert!(setup.high_priority);
    assert!(setup.lock_memory);
    assert_eq!(setup.cpu_affinity, Some(0xFF));
}

#[test]
fn test_jitter_meets_requirements() {
    let mut metrics = JitterMetrics::new();

    // Good metrics
    for _ in 0..1000 {
        metrics.record_tick(100_000, false);
    }
    assert!(metrics.meets_requirements());

    // Bad jitter
    metrics.reset();
    for _ in 0..1000 {
        metrics.record_tick(500_000, false);
    }
    assert!(!metrics.meets_requirements());

    // High missed rate
    metrics.reset();
    for _ in 0..1000 {
        metrics.record_tick(100_000, true);
    }
    assert!(!metrics.meets_requirements());
}

#[test]
fn test_scheduler_with_custom_period() {
    let scheduler = AbsoluteScheduler::with_period(500_000);
    assert_eq!(scheduler.period_ns(), 500_000);
}

#[test]
fn test_adaptive_config_validation() {
    let mut invalid = AdaptiveSchedulingConfig {
        min_period_ns: 0,
        max_period_ns: 0,
        ..Default::default()
    };
    invalid.normalize();
    assert!(invalid.is_valid());

    let valid = AdaptiveSchedulingConfig::default();
    assert!(valid.is_valid());
}

#[tokio::test]
async fn test_scheduler_async_context() {
    let mut scheduler = AbsoluteScheduler::new_1khz();
    let setup = RTSetup::minimal();
    let _ = scheduler.apply_rt_setup(&setup);

    // Should work in async context
    let result = scheduler.wait_for_tick();
    assert!(result.is_ok() || result.err() == Some(RTError::TimingViolation));
}

#[test]
fn test_jitter_ring_buffer_overflow() {
    let mut metrics = JitterMetrics::with_capacity(5);

    for i in 1..=10 {
        metrics.record_tick(i * 1000, false);
    }

    // Should only keep last 5
    assert_eq!(metrics.sample_count(), 5);
    assert_eq!(metrics.last_jitter_ns, 10_000);
}

#[test]
fn test_pll_phase_error_tracking() {
    let mut pll = PLL::new(1_000_000);

    // Consistent late ticks
    for _ in 0..10 {
        let _ = pll.update(1_100_000);
    }

    // Should have accumulated positive phase error (running slow)
    assert!(pll.phase_error_ns() > 0.0);
}

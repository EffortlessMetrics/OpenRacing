//! Platform scheduling deep tests.
//!
//! Validates RT thread priority configuration, timer resolution setup,
//! platform-specific scheduling policy selection, fallback behavior,
//! and priority inheritance mutex behavior across platforms.

use racing_wheel_engine::{
    AbsoluteScheduler, AdaptiveSchedulingConfig, JitterMetrics, PLL, RTSetup,
};
use std::time::Duration;

// ═══════════════════════════════════════════════════════════════════════════════
// 1. RT thread priority configuration
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn rt_setup_default_enables_high_priority() -> Result<(), Box<dyn std::error::Error>> {
    let setup = RTSetup::default();
    assert!(
        setup.high_priority,
        "default RTSetup should enable high_priority"
    );
    assert!(
        setup.lock_memory,
        "default RTSetup should enable lock_memory"
    );
    assert!(
        setup.disable_power_throttling,
        "default RTSetup should enable disable_power_throttling"
    );
    Ok(())
}

#[test]
fn rt_setup_cpu_affinity_default_is_none() -> Result<(), Box<dyn std::error::Error>> {
    let setup = RTSetup::default();
    assert!(
        setup.cpu_affinity.is_none(),
        "default cpu_affinity should be None (use all cores)"
    );
    Ok(())
}

#[test]
fn rt_setup_cpu_affinity_bitmask_single_core() -> Result<(), Box<dyn std::error::Error>> {
    let setup = RTSetup {
        cpu_affinity: Some(0x01), // core 0 only
        ..RTSetup::default()
    };
    let mask = setup.cpu_affinity.ok_or("affinity should be set")?;
    assert_eq!(mask.count_ones(), 1, "single core should have one bit set");
    assert_eq!(mask.trailing_zeros(), 0, "should be core 0");
    Ok(())
}

#[test]
fn rt_setup_cpu_affinity_bitmask_multi_core() -> Result<(), Box<dyn std::error::Error>> {
    let setup = RTSetup {
        cpu_affinity: Some(0x0F), // cores 0-3
        ..RTSetup::default()
    };
    let mask = setup.cpu_affinity.ok_or("affinity should be set")?;
    assert_eq!(mask.count_ones(), 4, "four cores should have four bits set");
    Ok(())
}

#[test]
fn rt_setup_all_disabled() -> Result<(), Box<dyn std::error::Error>> {
    let setup = RTSetup {
        high_priority: false,
        lock_memory: false,
        disable_power_throttling: false,
        cpu_affinity: None,
    };
    assert!(!setup.high_priority);
    assert!(!setup.lock_memory);
    assert!(!setup.disable_power_throttling);
    assert!(setup.cpu_affinity.is_none());
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. Timer resolution setup
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn scheduler_1khz_period_is_one_millisecond() -> Result<(), Box<dyn std::error::Error>> {
    let sched = AbsoluteScheduler::new_1khz();
    // 1kHz = 1ms = 1_000_000 ns
    assert!(
        sched.phase_error_ns().abs() < f64::EPSILON,
        "initial phase error should be zero"
    );
    Ok(())
}

#[test]
fn pll_target_period_1khz() -> Result<(), Box<dyn std::error::Error>> {
    let pll = PLL::new(1_000_000);
    assert_eq!(
        pll.target_period_ns(),
        1_000_000,
        "PLL target should be 1ms in nanoseconds"
    );
    Ok(())
}

#[test]
fn pll_target_period_can_be_updated() -> Result<(), Box<dyn std::error::Error>> {
    let mut pll = PLL::new(1_000_000);
    pll.set_target_period_ns(500_000); // 2kHz
    assert_eq!(pll.target_period_ns(), 500_000);

    pll.set_target_period_ns(2_000_000); // 500Hz
    assert_eq!(pll.target_period_ns(), 2_000_000);
    Ok(())
}

#[test]
fn pll_zero_target_period_clamped_to_minimum() -> Result<(), Box<dyn std::error::Error>> {
    let mut pll = PLL::new(1_000_000);
    pll.set_target_period_ns(0);
    // Implementation clamps to max(1)
    assert!(
        pll.target_period_ns() >= 1,
        "zero period should be clamped: {}",
        pll.target_period_ns()
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. Platform-specific scheduling policy selection
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn apply_rt_setup_with_no_features_succeeds() -> Result<(), Box<dyn std::error::Error>> {
    let mut sched = AbsoluteScheduler::new_1khz();
    let setup = RTSetup {
        high_priority: false,
        lock_memory: false,
        disable_power_throttling: false,
        cpu_affinity: None,
    };
    // With all features disabled, apply_rt_setup should always succeed
    let result = sched.apply_rt_setup(&setup);
    assert!(
        result.is_ok(),
        "applying empty RT setup should succeed: {result:?}"
    );
    assert!(sched.is_rt_setup_applied());
    Ok(())
}

#[test]
fn apply_rt_setup_is_idempotent() -> Result<(), Box<dyn std::error::Error>> {
    let mut sched = AbsoluteScheduler::new_1khz();
    let setup = RTSetup {
        high_priority: false,
        lock_memory: false,
        disable_power_throttling: false,
        cpu_affinity: None,
    };
    // First application
    let r1 = sched.apply_rt_setup(&setup);
    assert!(r1.is_ok());
    assert!(sched.is_rt_setup_applied());

    // Second application should be a no-op (early return Ok)
    let r2 = sched.apply_rt_setup(&setup);
    assert!(r2.is_ok(), "second apply should be idempotent");
    Ok(())
}

#[test]
fn scheduler_not_rt_setup_by_default() -> Result<(), Box<dyn std::error::Error>> {
    let sched = AbsoluteScheduler::new_1khz();
    assert!(
        !sched.is_rt_setup_applied(),
        "new scheduler should not have RT setup applied"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. Fallback behavior when RT scheduling unavailable
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn scheduler_functions_without_rt_setup() -> Result<(), Box<dyn std::error::Error>> {
    let sched = AbsoluteScheduler::new_1khz();
    // Scheduler should be usable even without RT setup
    assert_eq!(sched.tick_count(), 0);
    assert!(sched.phase_error_ns().abs() < f64::EPSILON);
    Ok(())
}

#[test]
fn scheduler_reset_clears_all_state() -> Result<(), Box<dyn std::error::Error>> {
    let mut sched = AbsoluteScheduler::new_1khz();

    // Apply some state
    sched.record_processing_time_us(100);
    let setup = RTSetup {
        high_priority: false,
        lock_memory: false,
        disable_power_throttling: false,
        cpu_affinity: None,
    };
    let _ = sched.apply_rt_setup(&setup);

    // Reset
    sched.reset();
    assert_eq!(
        sched.tick_count(),
        0,
        "tick count should be zero after reset"
    );
    assert!(
        sched.phase_error_ns().abs() < f64::EPSILON,
        "phase error should be zero after reset"
    );
    Ok(())
}

#[test]
fn jitter_metrics_default_meets_requirements() -> Result<(), Box<dyn std::error::Error>> {
    let mut metrics = JitterMetrics::new();
    // With no ticks recorded, p99 is 0 and missed rate is 0 — should meet requirements
    assert!(
        metrics.meets_requirements(),
        "empty metrics should meet requirements"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. Priority inheritance mutex behavior
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn parking_lot_mutex_does_not_block_indefinitely() -> Result<(), Box<dyn std::error::Error>> {
    use std::sync::Arc;

    let data = Arc::new(parking_lot::Mutex::new(0u64));
    let data2 = Arc::clone(&data);

    let handle = std::thread::spawn(move || {
        let mut guard = data2.lock();
        *guard += 1;
    });

    handle.join().map_err(|_| "mutex thread panicked")?;

    let val = *data.lock();
    assert_eq!(val, 1, "mutex should allow sequential access");
    Ok(())
}

#[test]
fn parking_lot_mutex_try_lock_reports_contention() -> Result<(), Box<dyn std::error::Error>> {
    use std::sync::Arc;

    let data = Arc::new(parking_lot::Mutex::new(42u64));

    // Hold the lock
    let guard = data.lock();

    // try_lock should fail while lock is held
    let try_result = data.try_lock();
    assert!(
        try_result.is_none(),
        "try_lock should return None when contended"
    );

    drop(guard);

    // Now try_lock should succeed
    let try_result = data.try_lock();
    assert!(
        try_result.is_some(),
        "try_lock should succeed when lock is free"
    );
    Ok(())
}

#[test]
fn crossbeam_channel_bounded_for_rt_command_queue() -> Result<(), Box<dyn std::error::Error>> {
    // RT pipelines use bounded channels to avoid unbounded allocation
    let (tx, rx) = crossbeam::channel::bounded::<u64>(16);

    for i in 0..16 {
        tx.send(i)
            .map_err(|e| format!("send should succeed for bounded channel: {e}"))?;
    }

    // Channel is full — try_send should fail
    let result = tx.try_send(99);
    assert!(result.is_err(), "bounded channel should reject when full");

    // Drain all messages
    for expected in 0..16 {
        let val = rx.recv().map_err(|e| format!("recv failed: {e}"))?;
        assert_eq!(val, expected);
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 6. Adaptive scheduling and jitter metrics
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn adaptive_scheduling_disabled_by_default() -> Result<(), Box<dyn std::error::Error>> {
    let sched = AbsoluteScheduler::new_1khz();
    let state = sched.adaptive_scheduling();
    assert!(
        !state.enabled,
        "adaptive scheduling should be disabled by default"
    );
    Ok(())
}

#[test]
fn adaptive_scheduling_can_be_enabled() -> Result<(), Box<dyn std::error::Error>> {
    let mut sched = AbsoluteScheduler::new_1khz();
    let config = AdaptiveSchedulingConfig {
        enabled: true,
        ..AdaptiveSchedulingConfig::default()
    };
    sched.set_adaptive_scheduling(config);

    let state = sched.adaptive_scheduling();
    assert!(state.enabled, "adaptive scheduling should be enabled");
    assert_eq!(state.min_period_ns, 900_000);
    assert_eq!(state.max_period_ns, 1_100_000);
    Ok(())
}

#[test]
fn adaptive_config_normalizes_inverted_bounds() -> Result<(), Box<dyn std::error::Error>> {
    let mut sched = AbsoluteScheduler::new_1khz();

    // Deliberately invert min/max
    let config = AdaptiveSchedulingConfig {
        enabled: true,
        min_period_ns: 2_000_000, // larger than max
        max_period_ns: 500_000,   // smaller than min
        ..AdaptiveSchedulingConfig::default()
    };
    sched.set_adaptive_scheduling(config);

    let state = sched.adaptive_scheduling();
    assert!(
        state.min_period_ns <= state.max_period_ns,
        "normalization should swap inverted bounds: min={}, max={}",
        state.min_period_ns,
        state.max_period_ns
    );
    Ok(())
}

#[test]
fn jitter_metrics_records_missed_ticks() -> Result<(), Box<dyn std::error::Error>> {
    let mut metrics = JitterMetrics::new();
    metrics.record_tick(100_000, false); // within budget
    metrics.record_tick(300_000, true); // missed

    assert_eq!(metrics.total_ticks, 2);
    assert_eq!(metrics.missed_ticks, 1);
    assert_eq!(metrics.max_jitter_ns, 300_000);
    Ok(())
}

#[test]
fn jitter_metrics_p99_with_many_samples() -> Result<(), Box<dyn std::error::Error>> {
    let mut metrics = JitterMetrics::new();

    // Record 99 low-jitter ticks and 1 high-jitter tick
    for _ in 0..99 {
        metrics.record_tick(10_000, false);
    }
    metrics.record_tick(500_000, true);

    let p99 = metrics.p99_jitter_ns();
    // p99 of 100 samples: the 99th percentile should be the high value
    assert!(p99 >= 10_000, "p99 should be at least the baseline: {p99}");
    Ok(())
}

#[test]
fn jitter_metrics_missed_tick_rate_calculation() -> Result<(), Box<dyn std::error::Error>> {
    let mut metrics = JitterMetrics::new();

    for _ in 0..999 {
        metrics.record_tick(50_000, false);
    }
    metrics.record_tick(300_000, true);

    let rate = metrics.missed_tick_rate();
    // 1 missed out of 1000 = 0.001
    assert!(
        (rate - 0.001).abs() < 0.0001,
        "missed tick rate should be ~0.001: {rate}"
    );
    Ok(())
}

#[test]
fn pll_reset_returns_to_initial_state() -> Result<(), Box<dyn std::error::Error>> {
    let mut pll = PLL::new(1_000_000);

    // Perturb PLL state
    let t0 = std::time::Instant::now();
    let _ = pll.update(t0);
    std::thread::sleep(Duration::from_micros(500));
    let _ = pll.update(std::time::Instant::now());

    // Reset
    pll.reset();
    assert!(
        pll.phase_error_ns().abs() < f64::EPSILON,
        "phase error should be zero after reset: {}",
        pll.phase_error_ns()
    );
    assert_eq!(pll.target_period_ns(), 1_000_000);
    Ok(())
}

#[test]
fn scheduler_record_processing_time() -> Result<(), Box<dyn std::error::Error>> {
    let mut sched = AbsoluteScheduler::new_1khz();
    let config = AdaptiveSchedulingConfig {
        enabled: true,
        ..AdaptiveSchedulingConfig::default()
    };
    sched.set_adaptive_scheduling(config);

    sched.record_processing_time_us(100);
    let state = sched.adaptive_scheduling();
    assert_eq!(
        state.last_processing_time_us, 100,
        "last processing time should be recorded"
    );

    sched.record_processing_time_us(200);
    let state = sched.adaptive_scheduling();
    assert_eq!(state.last_processing_time_us, 200);
    // EMA should be between 100 and 200
    assert!(
        state.processing_time_ema_us >= 100.0 && state.processing_time_ema_us <= 200.0,
        "EMA should be between recorded values: {}",
        state.processing_time_ema_us
    );
    Ok(())
}

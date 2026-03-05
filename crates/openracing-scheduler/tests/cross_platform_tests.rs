//! Cross-platform correctness tests for the scheduler crate.
//!
//! Validates that the platform abstraction layer (PlatformSleep), RTSetup,
//! AbsoluteScheduler, PLL, JitterMetrics, and adaptive scheduling behave
//! consistently across Windows, Linux, and macOS (fallback).
//!
//! Every test returns `Result` — no `unwrap()` / `expect()`.

use openracing_scheduler::{
    AbsoluteScheduler, AdaptiveSchedulingConfig, JitterMetrics, MAX_JITTER_NS, PERIOD_1KHZ_NS, PLL,
    RTError, RTSetup,
};
use std::time::{Duration, Instant};

/// Convenience alias.
type R = Result<(), Box<dyn std::error::Error>>;

// ═══════════════════════════════════════════════════════════════════════════════
// 1. PlatformSleep — construction and basic operation
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn scheduler_construction_succeeds_on_all_platforms() -> R {
    // PlatformSleep is created internally; if this compiles and runs,
    // the platform module is correctly selected.
    let scheduler = AbsoluteScheduler::new_1khz();
    assert_eq!(scheduler.tick_count(), 0);
    Ok(())
}

#[test]
fn scheduler_custom_period_accepted() -> R {
    let scheduler = AbsoluteScheduler::with_period(500_000); // 2kHz
    assert_eq!(scheduler.tick_count(), 0);
    Ok(())
}

#[test]
fn scheduler_minimum_period_clamped() -> R {
    // Period of 0 should be clamped to at least 1ns
    let scheduler = AbsoluteScheduler::with_period(0);
    // Should not panic; tick_count starts at 0
    assert_eq!(scheduler.tick_count(), 0);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. RTSetup — apply_rt_setup on current platform
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn apply_minimal_rt_setup_succeeds() -> R {
    let mut scheduler = AbsoluteScheduler::new_1khz();
    let setup = RTSetup::minimal();
    let result = scheduler.apply_rt_setup(&setup);
    assert!(
        result.is_ok(),
        "minimal RT setup (no special config) should succeed on all platforms: {result:?}"
    );
    Ok(())
}

#[test]
fn apply_testing_rt_setup_succeeds() -> R {
    let mut scheduler = AbsoluteScheduler::new_1khz();
    let setup = RTSetup::testing();
    let result = scheduler.apply_rt_setup(&setup);
    assert!(
        result.is_ok(),
        "testing RT setup should succeed on all platforms: {result:?}"
    );
    Ok(())
}

#[test]
fn apply_rt_setup_is_idempotent() -> R {
    let mut scheduler = AbsoluteScheduler::new_1khz();
    let setup = RTSetup::minimal();
    scheduler.apply_rt_setup(&setup)?;
    // Second apply should be a no-op (not an error)
    let result = scheduler.apply_rt_setup(&setup);
    assert!(
        result.is_ok(),
        "re-applying RT setup should be idempotent: {result:?}"
    );
    Ok(())
}

#[test]
fn apply_rt_setup_with_high_priority_does_not_panic() -> R {
    let mut scheduler = AbsoluteScheduler::new_1khz();
    let setup = RTSetup::new().with_high_priority(true);
    // On non-privileged environments this may fail, but must not panic
    let _result = scheduler.apply_rt_setup(&setup);
    Ok(())
}

#[test]
fn apply_rt_setup_with_memory_lock_does_not_panic() -> R {
    let mut scheduler = AbsoluteScheduler::new_1khz();
    let setup = RTSetup::new()
        .with_high_priority(false)
        .with_lock_memory(true);
    // May fail without privilege, must not panic
    let _result = scheduler.apply_rt_setup(&setup);
    Ok(())
}

#[test]
fn apply_rt_setup_with_cpu_affinity_does_not_panic() -> R {
    let mut scheduler = AbsoluteScheduler::new_1khz();
    let setup = RTSetup::minimal().with_cpu_affinity(0x01);
    // Affinity to core 0 — may or may not succeed depending on platform
    let _result = scheduler.apply_rt_setup(&setup);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. RTSetup — builder pattern and field access
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn rt_setup_builder_all_fields() -> R {
    let setup = RTSetup::new()
        .with_high_priority(true)
        .with_lock_memory(true)
        .with_disable_power_throttling(false)
        .with_cpu_affinity(0xFF);
    assert!(setup.high_priority);
    assert!(setup.lock_memory);
    assert!(!setup.disable_power_throttling);
    assert_eq!(setup.cpu_affinity, Some(0xFF));
    assert!(setup.has_rt_features());
    Ok(())
}

#[test]
fn rt_setup_minimal_has_no_features() -> R {
    let setup = RTSetup::minimal();
    assert!(!setup.has_rt_features());
    Ok(())
}

#[test]
fn rt_setup_clone_preserves_fields() -> R {
    let original = RTSetup::new().with_cpu_affinity(0x03);
    let cloned = original.clone();
    assert_eq!(cloned.cpu_affinity, Some(0x03));
    assert_eq!(cloned.high_priority, original.high_priority);
    assert_eq!(cloned.lock_memory, original.lock_memory);
    Ok(())
}

#[test]
fn rt_setup_debug_output_is_non_empty() -> R {
    let setup = RTSetup::default();
    let debug = format!("{setup:?}");
    assert!(!debug.is_empty(), "Debug output should be non-empty");
    assert!(
        debug.contains("high_priority"),
        "Debug should include field names: {debug}"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. PLL — drift correction is platform-independent
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn pll_initial_state() -> R {
    let pll = PLL::new(PERIOD_1KHZ_NS);
    assert_eq!(pll.target_period_ns(), PERIOD_1KHZ_NS);
    assert!(
        pll.phase_error_ns().abs() < f64::EPSILON,
        "initial phase error should be zero"
    );
    Ok(())
}

#[test]
fn pll_update_produces_correction() -> R {
    let mut pll = PLL::new(PERIOD_1KHZ_NS);
    // Simulate a tick that took exactly the target period
    let corrected = pll.update(PERIOD_1KHZ_NS);
    let corrected_ns = corrected.as_nanos() as u64;
    // Should be close to target period (PLL correction is small)
    let diff = (corrected_ns as i64 - PERIOD_1KHZ_NS as i64).unsigned_abs();
    assert!(
        diff < 100_000, // within 100µs
        "PLL correction should be close to target: got {corrected_ns}ns, expected ~{PERIOD_1KHZ_NS}ns"
    );
    Ok(())
}

#[test]
fn pll_reset_clears_accumulated_error() -> R {
    let mut pll = PLL::new(PERIOD_1KHZ_NS);
    // Run several updates with drift
    for _ in 0..100 {
        pll.update(PERIOD_1KHZ_NS + 1000); // slight drift
    }
    pll.reset();
    assert!(
        pll.phase_error_ns().abs() < f64::EPSILON,
        "phase error should be zero after reset"
    );
    Ok(())
}

#[test]
fn pll_target_period_can_be_changed() -> R {
    let mut pll = PLL::new(PERIOD_1KHZ_NS);
    pll.set_target_period_ns(500_000); // 2kHz
    assert_eq!(pll.target_period_ns(), 500_000);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. JitterMetrics — deterministic on all platforms
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn jitter_metrics_initial_state() -> R {
    let metrics = JitterMetrics::new();
    assert_eq!(metrics.total_ticks, 0);
    assert_eq!(metrics.missed_ticks, 0);
    assert_eq!(metrics.max_jitter_ns, 0);
    Ok(())
}

#[test]
fn jitter_metrics_records_normal_ticks() -> R {
    let mut metrics = JitterMetrics::new();
    for _ in 0..50 {
        metrics.record_tick(10_000, false); // 10µs jitter, not missed
    }
    assert_eq!(metrics.total_ticks, 50);
    assert_eq!(metrics.missed_ticks, 0);
    assert_eq!(metrics.max_jitter_ns, 10_000);
    Ok(())
}

#[test]
fn jitter_metrics_records_missed_ticks() -> R {
    let mut metrics = JitterMetrics::new();
    metrics.record_tick(300_000, true); // 300µs, missed
    metrics.record_tick(10_000, false);
    assert_eq!(metrics.total_ticks, 2);
    assert_eq!(metrics.missed_ticks, 1);
    assert_eq!(metrics.max_jitter_ns, 300_000);
    Ok(())
}

#[test]
fn jitter_metrics_missed_tick_rate_is_correct() -> R {
    let mut metrics = JitterMetrics::new();
    for _ in 0..99 {
        metrics.record_tick(1_000, false);
    }
    metrics.record_tick(500_000, true);
    let rate = metrics.missed_tick_rate();
    assert!(
        (rate - 0.01).abs() < 0.001,
        "1 missed in 100 should give ~0.01 rate: {rate}"
    );
    Ok(())
}

#[test]
fn jitter_metrics_max_jitter_tracks_worst_case() -> R {
    let mut metrics = JitterMetrics::new();
    let jitters = [100, 500, 250_000, 50, 1_000];
    for &j in &jitters {
        metrics.record_tick(j, j > MAX_JITTER_NS);
    }
    assert_eq!(metrics.max_jitter_ns, 250_000);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 6. AbsoluteScheduler — wait_for_tick
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn scheduler_wait_for_tick_advances_count() -> R {
    // Use a relaxed period so the first tick is unlikely to exceed the jitter threshold
    let mut scheduler = AbsoluteScheduler::with_period(10_000_000); // 10ms
    scheduler.apply_rt_setup(&RTSetup::minimal())?;

    match scheduler.wait_for_tick() {
        Ok(tick) => assert_eq!(tick, 1),
        Err(RTError::TimingViolation) => {
            // Acceptable in non-RT test environments
        }
        Err(e) => return Err(e.into()),
    }
    Ok(())
}

#[test]
fn scheduler_wait_for_tick_measures_jitter() -> R {
    let mut scheduler = AbsoluteScheduler::with_period(10_000_000); // 10ms
    scheduler.apply_rt_setup(&RTSetup::minimal())?;

    // Run a few ticks — timing violations are acceptable under load
    for _ in 0..5 {
        match scheduler.wait_for_tick() {
            Ok(_) => {}
            Err(RTError::TimingViolation) => break,
            Err(e) => return Err(e.into()),
        }
    }

    let metrics = scheduler.metrics();
    // total_ticks includes violation ticks (recorded before the error check)
    assert!(
        metrics.total_ticks >= 1,
        "should have at least one tick recorded"
    );
    Ok(())
}

#[test]
fn scheduler_phase_error_starts_near_zero() -> R {
    let scheduler = AbsoluteScheduler::new_1khz();
    let error = scheduler.phase_error_ns();
    assert!(
        error.abs() < 1_000_000.0, // within 1ms
        "initial phase error should be near zero: {error}"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 7. AdaptiveScheduling — config normalization
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn adaptive_config_default_is_disabled() -> R {
    let config = AdaptiveSchedulingConfig::default();
    assert!(!config.enabled);
    Ok(())
}

#[test]
fn adaptive_config_inverted_bounds_are_normalized() -> R {
    let mut scheduler = AbsoluteScheduler::new_1khz();
    let config = AdaptiveSchedulingConfig {
        enabled: true,
        min_period_ns: 2_000_000, // deliberately inverted
        max_period_ns: 500_000,
        ..AdaptiveSchedulingConfig::default()
    };
    scheduler.set_adaptive_scheduling(config);
    let state = scheduler.adaptive_scheduling();
    assert!(
        state.min_period_ns <= state.max_period_ns,
        "normalization should correct inverted bounds: min={}, max={}",
        state.min_period_ns,
        state.max_period_ns
    );
    Ok(())
}

#[test]
fn adaptive_scheduling_state_reflects_config() -> R {
    let mut scheduler = AbsoluteScheduler::new_1khz();
    let config = AdaptiveSchedulingConfig {
        enabled: true,
        min_period_ns: 800_000,
        max_period_ns: 1_200_000,
        ..AdaptiveSchedulingConfig::default()
    };
    scheduler.set_adaptive_scheduling(config);
    let state = scheduler.adaptive_scheduling();
    assert_eq!(state.min_period_ns, 800_000);
    assert_eq!(state.max_period_ns, 1_200_000);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 8. RTError — error types compile on all platforms
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn rt_error_display_is_non_empty() -> R {
    let err = RTError::TimingViolation;
    let msg = format!("{err}");
    assert!(!msg.is_empty());
    assert!(
        msg.contains("iming") || msg.contains("iolation"),
        "error message should describe the violation: {msg}"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 9. Constants — correctness
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn period_1khz_is_one_millisecond() -> R {
    assert_eq!(PERIOD_1KHZ_NS, 1_000_000);
    Ok(())
}

#[test]
fn max_jitter_is_250_microseconds() -> R {
    assert_eq!(MAX_JITTER_NS, 250_000);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 10. Platform timing — Instant precision
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn instant_monotonicity() -> R {
    let a = Instant::now();
    // Burn some time
    let mut sum = 0u64;
    for i in 0..10_000 {
        sum = sum.wrapping_add(i);
    }
    let _ = sum;
    let b = Instant::now();
    assert!(b >= a, "Instant should be monotonic");
    Ok(())
}

#[test]
fn instant_duration_since_is_non_negative() -> R {
    let start = Instant::now();
    std::thread::sleep(Duration::from_micros(100));
    let elapsed = start.elapsed();
    assert!(
        elapsed.as_nanos() > 0,
        "elapsed time should be positive after sleep"
    );
    Ok(())
}

#[test]
fn thread_sleep_respects_minimum_duration() -> R {
    let start = Instant::now();
    std::thread::sleep(Duration::from_millis(1));
    let elapsed = start.elapsed();
    // On all platforms, sleeping 1ms should take at least 500µs
    assert!(
        elapsed >= Duration::from_micros(500),
        "1ms sleep should take at least 500µs: {elapsed:?}"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 11. Platform detection — cfg predicates
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn exactly_one_platform_module_selected() -> R {
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
        count += 1; // fallback
    }
    assert_eq!(count, 1, "exactly one platform module should be selected");
    Ok(())
}

#[test]
fn platform_family_is_consistent() -> R {
    #[cfg(windows)]
    const {
        assert!(cfg!(target_os = "windows"));
        assert!(!cfg!(unix));
    }
    #[cfg(unix)]
    const {
        assert!(!cfg!(windows));
        assert!(
            cfg!(target_os = "linux") || cfg!(target_os = "macos") || cfg!(target_os = "freebsd")
        );
    }
    Ok(())
}

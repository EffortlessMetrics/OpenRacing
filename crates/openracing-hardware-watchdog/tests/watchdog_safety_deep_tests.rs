#![allow(clippy::result_large_err)]
//! Deep tests for hardware watchdog and safety interlock system.
//!
//! Covers:
//! 1. Watchdog feed timing at boundary conditions
//! 2. Rapid feed/starve cycles
//! 3. Watchdog recovery after fault
//! 4. Concurrent watchdog feed attempts
//! 5. Different timeout configurations
//! 6. Exhaustive state machine transitions
//! 7. Invalid transition rejection
//! 8. State persistence across reset scenarios
//! 9. Diagnostic output for each state

use openracing_hardware_watchdog::prelude::*;
use std::sync::{Arc, Barrier};
use std::time::Duration;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn make_watchdog_ms(timeout_ms: u32) -> Result<SoftwareWatchdog, HardwareWatchdogError> {
    let config = WatchdogConfig::new(timeout_ms)?;
    Ok(SoftwareWatchdog::new(config))
}

// ===========================================================================
// 1. Watchdog feed timing at boundary conditions
// ===========================================================================

/// Feed just before the timeout expires — watchdog must remain armed.
#[test]
fn feed_just_before_timeout_keeps_armed() -> TestResult {
    let mut wd = make_watchdog_ms(20)?;
    wd.arm()?;

    // Sleep just under the timeout then feed
    std::thread::sleep(Duration::from_millis(12));
    wd.feed()?;
    assert!(!wd.has_timed_out());
    assert!(wd.is_armed());
    assert_eq!(wd.status(), WatchdogStatus::Armed);
    Ok(())
}

/// Let the timeout expire without feeding — watchdog must detect timeout.
#[test]
fn timeout_fires_after_period_elapses() -> TestResult {
    let mut wd = make_watchdog_ms(10)?;
    wd.arm()?;

    std::thread::sleep(Duration::from_millis(15));
    assert!(wd.has_timed_out());
    assert_eq!(wd.status(), WatchdogStatus::TimedOut);
    Ok(())
}

/// Feed exactly at boundary — feed resets the clock.
#[test]
fn feed_resets_timeout_clock() -> TestResult {
    let mut wd = make_watchdog_ms(15)?;
    wd.arm()?;

    // Feed at ~10ms, then wait another 10ms (total 20ms > 15ms from arm,
    // but only 10ms from last feed) — should NOT time out.
    std::thread::sleep(Duration::from_millis(10));
    wd.feed()?;
    std::thread::sleep(Duration::from_millis(10));
    assert!(!wd.has_timed_out());
    Ok(())
}

/// After timeout, feeding must fail with an error.
#[test]
fn feed_after_timeout_returns_error() -> TestResult {
    let mut wd = make_watchdog_ms(10)?;
    wd.arm()?;

    // Force a timeout
    wd.trigger_timeout()?;
    let result = wd.feed();
    assert!(result.is_err());
    assert!(
        matches!(result, Err(HardwareWatchdogError::TimedOut)),
        "Expected TimedOut error"
    );
    Ok(())
}

// ===========================================================================
// 2. Rapid feed/starve cycles
// ===========================================================================

/// Rapidly alternate between feeding and small sleeps — never should timeout.
#[test]
fn rapid_feed_cycle_never_times_out() -> TestResult {
    let mut wd = make_watchdog_ms(10)?;
    wd.arm()?;

    for _ in 0..50 {
        std::thread::sleep(Duration::from_millis(2));
        wd.feed()?;
        assert!(!wd.has_timed_out());
    }
    assert!(wd.is_armed());
    assert_eq!(wd.metrics().feed_count, 50);
    Ok(())
}

/// Feed rapidly, then starve — must timeout after starvation period.
#[test]
fn feed_then_starve_causes_timeout() -> TestResult {
    let mut wd = make_watchdog_ms(10)?;
    wd.arm()?;

    // Rapid feeds
    for _ in 0..10 {
        wd.feed()?;
    }
    assert!(!wd.has_timed_out());

    // Now starve
    std::thread::sleep(Duration::from_millis(15));
    assert!(wd.has_timed_out());
    Ok(())
}

/// Alternate: 5 rapid feeds, starve to near-timeout, feed again — should survive.
#[test]
fn near_timeout_rescue_cycle() -> TestResult {
    let mut wd = make_watchdog_ms(20)?;
    wd.arm()?;

    for cycle in 0..5 {
        // Rapid feeds
        for _ in 0..3 {
            wd.feed()?;
        }
        // Sleep close to timeout (but under)
        std::thread::sleep(Duration::from_millis(15));
        // Rescue feed
        wd.feed()?;
        assert!(
            !wd.has_timed_out(),
            "Timed out unexpectedly on cycle {cycle}"
        );
    }
    Ok(())
}

// ===========================================================================
// 3. Watchdog recovery after fault
// ===========================================================================

/// After timeout → safe-state → reset, watchdog must be fully reusable.
#[test]
fn full_recovery_cycle_after_fault() -> TestResult {
    let mut wd = make_watchdog_ms(100)?;

    // Normal operation
    wd.arm()?;
    wd.feed()?;

    // Simulate fault
    wd.trigger_timeout()?;
    assert!(wd.has_timed_out());
    wd.trigger_safe_state()?;
    assert!(wd.is_safe_state_triggered());
    assert_eq!(wd.status(), WatchdogStatus::SafeState);

    // Recovery
    wd.reset();
    assert_eq!(wd.status(), WatchdogStatus::Disarmed);
    assert!(!wd.is_safe_state_triggered());
    assert!(!wd.has_timed_out());

    // Re-arm and operate normally
    wd.arm()?;
    wd.feed()?;
    assert!(wd.is_armed());
    assert!(wd.is_healthy());
    Ok(())
}

/// Multiple consecutive fault-recovery cycles.
#[test]
fn repeated_fault_recovery_cycles() -> TestResult {
    let mut wd = make_watchdog_ms(100)?;

    for i in 0..10 {
        wd.arm()?;
        wd.feed()?;
        wd.trigger_timeout()?;
        wd.trigger_safe_state()?;
        assert!(wd.is_safe_state_triggered(), "Cycle {i}: not in safe state");
        wd.reset();
        assert_eq!(
            wd.status(),
            WatchdogStatus::Disarmed,
            "Cycle {i}: not disarmed after reset"
        );
    }
    Ok(())
}

/// After reset, metrics are cleared.
#[test]
fn metrics_cleared_after_reset() -> TestResult {
    let mut wd = make_watchdog_ms(100)?;
    wd.arm()?;
    for _ in 0..5 {
        wd.feed()?;
    }
    assert_eq!(wd.metrics().feed_count, 5);

    wd.reset();
    let m = wd.metrics();
    assert_eq!(m.feed_count, 0);
    assert_eq!(m.arm_count, 0);
    assert_eq!(m.timeout_count, 0);
    assert_eq!(m.safe_state_count, 0);
    assert_eq!(m.consecutive_failures, 0);
    Ok(())
}

/// Reset from armed state (without fault) is valid.
#[test]
fn reset_from_armed_state() -> TestResult {
    let mut wd = make_watchdog_ms(100)?;
    wd.arm()?;
    wd.feed()?;
    wd.reset();
    assert_eq!(wd.status(), WatchdogStatus::Disarmed);
    // Can re-arm immediately
    wd.arm()?;
    assert!(wd.is_armed());
    Ok(())
}

// ===========================================================================
// 4. Concurrent watchdog feed attempts (WatchdogState is atomic)
// ===========================================================================

/// Multiple threads reading state concurrently see a consistent snapshot.
#[test]
fn concurrent_state_reads_are_consistent() -> TestResult {
    let state = Arc::new(WatchdogState::new());
    state.arm().map_err(|e| format!("{e}"))?;

    let barrier = Arc::new(Barrier::new(4));
    let mut handles = Vec::new();

    for _ in 0..4 {
        let s = Arc::clone(&state);
        let b = Arc::clone(&barrier);
        handles.push(std::thread::spawn(move || {
            b.wait();
            for _ in 0..100 {
                let status = s.status();
                // State should always be Armed (no other thread mutates)
                assert_eq!(status, WatchdogStatus::Armed);
            }
        }));
    }

    for h in handles {
        h.join().map_err(|_| "thread panicked")?;
    }
    Ok(())
}

/// Concurrent feeds on WatchdogState — feed_count should be consistent.
#[test]
fn concurrent_feeds_on_atomic_state() -> TestResult {
    let state = Arc::new(WatchdogState::new());
    state.arm().map_err(|e| format!("{e}"))?;

    let barrier = Arc::new(Barrier::new(4));
    let feeds_per_thread = 100u32;
    let mut handles = Vec::new();

    for _ in 0..4 {
        let s = Arc::clone(&state);
        let b = Arc::clone(&barrier);
        handles.push(std::thread::spawn(move || {
            b.wait();
            for _ in 0..feeds_per_thread {
                let _ = s.feed();
            }
        }));
    }

    for h in handles {
        h.join().map_err(|_| "thread panicked")?;
    }

    // All feeds should have been counted
    assert_eq!(state.feed_count(), 4 * feeds_per_thread);
    Ok(())
}

/// One thread arms, another tries to arm concurrently — exactly one succeeds.
#[test]
fn concurrent_arm_exactly_one_succeeds() -> TestResult {
    let state = Arc::new(WatchdogState::new());
    let barrier = Arc::new(Barrier::new(2));
    let mut handles = Vec::new();

    for _ in 0..2 {
        let s = Arc::clone(&state);
        let b = Arc::clone(&barrier);
        handles.push(std::thread::spawn(move || -> bool {
            b.wait();
            s.arm().is_ok()
        }));
    }

    let mut successes = 0u32;
    for h in handles {
        if h.join().map_err(|_| "thread panicked")? {
            successes += 1;
        }
    }
    assert_eq!(successes, 1, "Exactly one arm() should succeed");
    assert_eq!(state.status(), WatchdogStatus::Armed);
    Ok(())
}

// ===========================================================================
// 5. Different timeout configurations
// ===========================================================================

/// Minimum valid timeout (10ms).
#[test]
fn config_minimum_timeout() -> TestResult {
    let config = WatchdogConfig::new(10)?;
    assert_eq!(config.timeout_ms, 10);
    assert_eq!(config.timeout_us(), 10_000);

    let mut wd = SoftwareWatchdog::new(config);
    wd.arm()?;
    wd.feed()?;
    assert!(wd.is_armed());
    Ok(())
}

/// Maximum valid timeout (5000ms).
#[test]
fn config_maximum_timeout() -> TestResult {
    let config = WatchdogConfig::new(5000)?;
    assert_eq!(config.timeout_ms, 5000);
    assert_eq!(config.timeout_us(), 5_000_000);

    let mut wd = SoftwareWatchdog::new(config);
    wd.arm()?;
    wd.feed()?;
    assert!(wd.is_armed());
    Ok(())
}

/// Below minimum timeout is rejected.
#[test]
fn config_below_minimum_rejected() {
    assert!(WatchdogConfig::new(9).is_err());
    assert!(WatchdogConfig::new(0).is_err());
    assert!(WatchdogConfig::new(1).is_err());
}

/// Above maximum timeout is rejected.
#[test]
fn config_above_maximum_rejected() {
    assert!(WatchdogConfig::new(5001).is_err());
    assert!(WatchdogConfig::new(10_000).is_err());
    assert!(WatchdogConfig::new(u32::MAX).is_err());
}

/// Builder with all custom fields.
#[test]
fn config_builder_full_custom() -> TestResult {
    let config = WatchdogConfig::builder()
        .timeout_ms(200)
        .max_response_time_us(500)
        .max_feed_failures(5)
        .health_check_enabled(true)
        .health_check_interval_ms(50)
        .build()?;

    assert_eq!(config.timeout_ms, 200);
    assert_eq!(config.max_response_time_us, 500);
    assert_eq!(config.max_feed_failures, 5);
    assert!(config.health_check_enabled);
    assert_eq!(config.health_check_interval_ms, 50);
    Ok(())
}

/// Builder with health check disabled allows any health check interval.
#[test]
fn config_builder_health_check_disabled() -> TestResult {
    let config = WatchdogConfig::builder()
        .timeout_ms(100)
        .health_check_enabled(false)
        .health_check_interval_ms(0)
        .build()?;
    assert!(!config.health_check_enabled);
    Ok(())
}

/// Builder rejects invalid max_response_time_us.
#[test]
fn config_builder_rejects_excessive_response_time() {
    let result = WatchdogConfig::builder()
        .timeout_ms(100)
        .max_response_time_us(10_001)
        .build();
    assert!(result.is_err());
}

/// Multiple timeouts from different configs behave correctly.
#[test]
fn multiple_watchdogs_different_configs() -> TestResult {
    let mut wd_short = make_watchdog_ms(10)?;
    let mut wd_long = make_watchdog_ms(5000)?;

    wd_short.arm()?;
    wd_long.arm()?;

    // Short timeout fires
    std::thread::sleep(Duration::from_millis(15));
    assert!(wd_short.has_timed_out());
    // Long timeout does not
    assert!(!wd_long.has_timed_out());

    wd_long.disarm()?;
    Ok(())
}

// ===========================================================================
// 6. Exhaustive state machine transitions (all valid paths)
// ===========================================================================

/// Disarmed → Armed → Disarmed (normal arm/disarm cycle).
#[test]
fn transition_disarmed_armed_disarmed() -> TestResult {
    let state = WatchdogState::new();
    assert_eq!(state.status(), WatchdogStatus::Disarmed);

    state.arm().map_err(|e| format!("{e}"))?;
    assert_eq!(state.status(), WatchdogStatus::Armed);

    state.disarm().map_err(|e| format!("{e}"))?;
    assert_eq!(state.status(), WatchdogStatus::Disarmed);
    Ok(())
}

/// Disarmed → Armed → TimedOut (timeout from armed).
#[test]
fn transition_armed_to_timed_out() -> TestResult {
    let state = WatchdogState::new();
    state.arm().map_err(|e| format!("{e}"))?;
    state.timeout().map_err(|e| format!("{e}"))?;
    assert_eq!(state.status(), WatchdogStatus::TimedOut);
    Ok(())
}

/// Armed → feed succeeds and stays Armed.
#[test]
fn transition_armed_feed_stays_armed() -> TestResult {
    let state = WatchdogState::new();
    state.arm().map_err(|e| format!("{e}"))?;
    state.feed().map_err(|e| format!("{e}"))?;
    assert_eq!(state.status(), WatchdogStatus::Armed);
    assert_eq!(state.feed_count(), 1);
    Ok(())
}

/// Any non-SafeState → SafeState is valid.
#[test]
fn transition_any_to_safe_state() -> TestResult {
    // From Disarmed
    let state = WatchdogState::new();
    state.trigger_safe_state().map_err(|e| format!("{e}"))?;
    assert_eq!(state.status(), WatchdogStatus::SafeState);

    // From Armed
    let state = WatchdogState::new();
    state.arm().map_err(|e| format!("{e}"))?;
    state.trigger_safe_state().map_err(|e| format!("{e}"))?;
    assert_eq!(state.status(), WatchdogStatus::SafeState);

    // From TimedOut
    let state = WatchdogState::new();
    state.arm().map_err(|e| format!("{e}"))?;
    state.timeout().map_err(|e| format!("{e}"))?;
    state.trigger_safe_state().map_err(|e| format!("{e}"))?;
    assert_eq!(state.status(), WatchdogStatus::SafeState);
    Ok(())
}

/// Reset from any state goes to Disarmed.
#[test]
fn reset_from_every_state() -> TestResult {
    // Armed
    let state = WatchdogState::new();
    state.arm().map_err(|e| format!("{e}"))?;
    state.reset();
    assert_eq!(state.status(), WatchdogStatus::Disarmed);

    // TimedOut
    let state = WatchdogState::new();
    state.arm().map_err(|e| format!("{e}"))?;
    state.timeout().map_err(|e| format!("{e}"))?;
    state.reset();
    assert_eq!(state.status(), WatchdogStatus::Disarmed);

    // SafeState
    let state = WatchdogState::new();
    state.trigger_safe_state().map_err(|e| format!("{e}"))?;
    state.reset();
    assert_eq!(state.status(), WatchdogStatus::Disarmed);
    Ok(())
}

// ===========================================================================
// 7. Invalid transitions are rejected
// ===========================================================================

/// Disarmed → feed fails.
#[test]
fn invalid_feed_when_disarmed() {
    let state = WatchdogState::new();
    let result = state.feed();
    assert!(matches!(result, Err(HardwareWatchdogError::NotArmed)));
}

/// Disarmed → disarm fails.
#[test]
fn invalid_disarm_when_disarmed() {
    let state = WatchdogState::new();
    let result = state.disarm();
    assert!(result.is_err());
}

/// Disarmed → timeout fails.
#[test]
fn invalid_timeout_when_disarmed() {
    let state = WatchdogState::new();
    let result = state.timeout();
    assert!(result.is_err());
}

/// Armed → arm fails (double arm).
#[test]
fn invalid_double_arm() {
    let state = WatchdogState::new();
    assert!(state.arm().is_ok());
    let result = state.arm();
    assert!(result.is_err());
}

/// TimedOut → arm fails.
#[test]
fn invalid_arm_when_timed_out() {
    let state = WatchdogState::new();
    assert!(state.arm().is_ok());
    assert!(state.timeout().is_ok());
    let result = state.arm();
    assert!(result.is_err());
}

/// TimedOut → feed fails.
#[test]
fn invalid_feed_when_timed_out() {
    let state = WatchdogState::new();
    assert!(state.arm().is_ok());
    assert!(state.timeout().is_ok());
    let result = state.feed();
    assert!(matches!(result, Err(HardwareWatchdogError::TimedOut)));
}

/// TimedOut → disarm fails.
#[test]
fn invalid_disarm_when_timed_out() {
    let state = WatchdogState::new();
    assert!(state.arm().is_ok());
    assert!(state.timeout().is_ok());
    let result = state.disarm();
    assert!(result.is_err());
}

/// TimedOut → timeout fails (already timed out).
#[test]
fn invalid_double_timeout() {
    let state = WatchdogState::new();
    assert!(state.arm().is_ok());
    assert!(state.timeout().is_ok());
    let result = state.timeout();
    assert!(result.is_err());
}

/// SafeState → everything except reset and trigger_safe_state fails.
#[test]
fn invalid_operations_in_safe_state() {
    let state = WatchdogState::new();
    assert!(state.trigger_safe_state().is_ok());

    assert!(state.arm().is_err());
    assert!(state.disarm().is_err());
    assert!(state.feed().is_err());
    assert!(state.timeout().is_err());
    assert!(state.trigger_safe_state().is_err());
}

// ===========================================================================
// 8. State persistence across restart (reset) scenarios
// ===========================================================================

/// Full lifecycle: arm → feed → timeout → safe → reset → arm → feed → disarm.
#[test]
fn full_lifecycle_round_trip() -> TestResult {
    let mut wd = make_watchdog_ms(100)?;

    // Phase 1: normal → fault
    wd.arm()?;
    for _ in 0..5 {
        wd.feed()?;
    }
    wd.trigger_timeout()?;
    assert_eq!(wd.status(), WatchdogStatus::TimedOut);
    wd.trigger_safe_state()?;
    assert_eq!(wd.status(), WatchdogStatus::SafeState);

    // Phase 2: recovery
    wd.reset();
    assert_eq!(wd.status(), WatchdogStatus::Disarmed);

    // Phase 3: normal operation resumes
    wd.arm()?;
    wd.feed()?;
    assert!(wd.is_armed());
    assert!(wd.is_healthy());
    wd.disarm()?;
    assert_eq!(wd.status(), WatchdogStatus::Disarmed);
    Ok(())
}

/// State counters accumulate across arm/disarm cycles (without reset).
#[test]
fn counters_accumulate_without_reset() -> TestResult {
    let state = WatchdogState::new();

    for _ in 0..3 {
        state.arm().map_err(|e| format!("{e}"))?;
        state.feed().map_err(|e| format!("{e}"))?;
        state.disarm().map_err(|e| format!("{e}"))?;
    }

    assert_eq!(state.arm_count(), 3);
    assert_eq!(state.feed_count(), 3);
    Ok(())
}

/// After reset, counters in the WatchdogState are NOT cleared
/// (only SoftwareWatchdog.reset() clears metrics).
#[test]
fn watchdog_state_reset_does_not_clear_counters() -> TestResult {
    let state = WatchdogState::new();
    state.arm().map_err(|e| format!("{e}"))?;
    state.feed().map_err(|e| format!("{e}"))?;

    state.reset();
    // WatchdogState.reset() only resets the status, not counters
    assert_eq!(state.status(), WatchdogStatus::Disarmed);
    assert_eq!(state.arm_count(), 1);
    assert_eq!(state.feed_count(), 1);
    Ok(())
}

/// SoftwareWatchdog.reset() clears metrics completely.
#[test]
fn software_watchdog_reset_clears_metrics() -> TestResult {
    let mut wd = make_watchdog_ms(100)?;
    wd.arm()?;
    wd.feed()?;
    wd.feed()?;
    wd.feed()?;
    assert_eq!(wd.metrics().feed_count, 3);

    wd.reset();
    let m = wd.metrics();
    assert_eq!(m.feed_count, 0);
    assert_eq!(m.arm_count, 0);
    assert_eq!(m.max_feed_interval_us, 0);
    assert_eq!(m.last_feed_timestamp_us, 0);
    Ok(())
}

// ===========================================================================
// 9. Diagnostic output for each state
// ===========================================================================

/// Status display strings are correct for all variants.
#[test]
fn status_display_strings() {
    assert_eq!(format!("{}", WatchdogStatus::Disarmed), "Disarmed");
    assert_eq!(format!("{}", WatchdogStatus::Armed), "Armed");
    assert_eq!(format!("{}", WatchdogStatus::TimedOut), "TimedOut");
    assert_eq!(format!("{}", WatchdogStatus::SafeState), "SafeState");
}

/// Status from_raw covers all valid values and rejects invalid.
#[test]
fn status_from_raw_exhaustive() {
    assert_eq!(WatchdogStatus::from_raw(0), Some(WatchdogStatus::Disarmed));
    assert_eq!(WatchdogStatus::from_raw(1), Some(WatchdogStatus::Armed));
    assert_eq!(WatchdogStatus::from_raw(2), Some(WatchdogStatus::TimedOut));
    assert_eq!(WatchdogStatus::from_raw(3), Some(WatchdogStatus::SafeState));
    assert_eq!(WatchdogStatus::from_raw(4), None);
    assert_eq!(WatchdogStatus::from_raw(u32::MAX), None);
}

/// is_terminal is true only for SafeState.
#[test]
fn status_is_terminal_only_safe_state() {
    let non_terminal = [
        WatchdogStatus::Disarmed,
        WatchdogStatus::Armed,
        WatchdogStatus::TimedOut,
    ];
    for s in &non_terminal {
        assert!(!s.is_terminal(), "{s} should not be terminal");
    }
    assert!(WatchdogStatus::SafeState.is_terminal());
}

/// is_active is true only for Armed and TimedOut.
#[test]
fn status_is_active_for_armed_and_timed_out() {
    assert!(!WatchdogStatus::Disarmed.is_active());
    assert!(WatchdogStatus::Armed.is_active());
    assert!(WatchdogStatus::TimedOut.is_active());
    assert!(!WatchdogStatus::SafeState.is_active());
}

/// Metrics record_feed tracks max interval monotonically.
#[test]
fn metrics_max_interval_monotonic() {
    let mut m = WatchdogMetrics::new();

    m.record_feed(1000);
    m.record_feed(2000);
    assert_eq!(m.max_feed_interval_us, 1000);

    m.record_feed(5000);
    assert_eq!(m.max_feed_interval_us, 3000);

    // Smaller interval doesn't reduce max
    m.record_feed(5500);
    assert_eq!(m.max_feed_interval_us, 3000);
}

/// Metrics success_rate is always in [0.0, 1.0].
#[test]
fn metrics_success_rate_bounded() {
    let mut m = WatchdogMetrics::new();
    assert!((0.0..=1.0).contains(&m.success_rate()));

    m.record_feed(1000);
    assert!((0.0..=1.0).contains(&m.success_rate()));

    for _ in 0..100 {
        m.record_failure();
    }
    assert!((0.0..=1.0).contains(&m.success_rate()));
}

/// Error display messages are non-empty for all variants.
#[test]
fn error_display_all_variants() {
    let errors: Vec<HardwareWatchdogError> = vec![
        HardwareWatchdogError::NotArmed,
        HardwareWatchdogError::AlreadyArmed,
        HardwareWatchdogError::TimedOut,
        HardwareWatchdogError::hardware_error("test hw err"),
        HardwareWatchdogError::invalid_configuration("bad config"),
        HardwareWatchdogError::invalid_transition("Armed", "Armed"),
        HardwareWatchdogError::SafeStateAlreadyTriggered,
        HardwareWatchdogError::WcetExceeded,
    ];

    for err in &errors {
        let msg = format!("{err}");
        assert!(!msg.is_empty(), "Empty display for {err:?}");
    }
}

/// SoftwareWatchdog is_healthy returns false after timeout or safe state.
#[test]
fn is_healthy_reflects_fault_state() -> TestResult {
    let mut wd = make_watchdog_ms(100)?;
    assert!(wd.is_healthy());

    wd.arm()?;
    assert!(wd.is_healthy());

    wd.trigger_timeout()?;
    assert!(!wd.is_healthy());

    wd.trigger_safe_state()?;
    assert!(!wd.is_healthy());
    Ok(())
}

/// time_since_last_feed_us returns None before any feed.
#[test]
fn time_since_last_feed_none_initially() {
    let wd = SoftwareWatchdog::with_default_timeout();
    assert!(wd.time_since_last_feed_us().is_none());
}

/// time_since_last_feed_us returns Some after feeding.
#[test]
fn time_since_last_feed_some_after_feed() -> TestResult {
    let mut wd = make_watchdog_ms(100)?;
    wd.arm()?;
    wd.feed()?;
    // Should have a value now (may be 0 or small, depending on clock)
    assert!(wd.time_since_last_feed_us().is_some());
    Ok(())
}

/// Config access through the trait.
#[test]
fn config_accessible_through_trait() -> TestResult {
    let config = WatchdogConfig::builder()
        .timeout_ms(250)
        .max_response_time_us(800)
        .build()?;
    let wd = SoftwareWatchdog::new(config);
    assert_eq!(wd.config().timeout_ms, 250);
    assert_eq!(wd.config().max_response_time_us, 800);
    Ok(())
}

// ===========================================================================
// 10. SoftwareWatchdog trigger_timeout (test-only helper)
// ===========================================================================

/// trigger_timeout fails if not armed.
#[test]
fn trigger_timeout_fails_when_disarmed() {
    let wd = SoftwareWatchdog::with_default_timeout();
    let result = wd.trigger_timeout();
    assert!(result.is_err());
}

/// trigger_timeout transitions from Armed to TimedOut.
#[test]
fn trigger_timeout_from_armed() -> TestResult {
    let mut wd = make_watchdog_ms(100)?;
    wd.arm()?;
    wd.trigger_timeout()?;
    assert_eq!(wd.status(), WatchdogStatus::TimedOut);
    assert!(wd.has_timed_out());
    Ok(())
}

/// After trigger_timeout, feed must fail.
#[test]
fn no_feed_after_trigger_timeout() -> TestResult {
    let mut wd = make_watchdog_ms(100)?;
    wd.arm()?;
    wd.trigger_timeout()?;
    let result = wd.feed();
    assert!(result.is_err());
    Ok(())
}

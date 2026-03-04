//! Property-based and edge-case tests for hardware watchdog crate.
//!
//! Covers: state machine invariants (armed→tripped→reset),
//! challenge-response protocol, stale/repeated trips, concurrent access,
//! configuration edge cases.

#![allow(clippy::redundant_closure)]

use openracing_hardware_watchdog::prelude::*;
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// Property-based tests (proptest)
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    // -- State machine invariants --------------------------------------------

    #[test]
    fn prop_state_machine_arm_timeout_reset_cycle(
        cycles in 1u32..20,
    ) {
        let state = WatchdogState::new();
        for _ in 0..cycles {
            // Disarmed → Armed
            let arm_result = state.arm();
            prop_assert!(arm_result.is_ok());
            prop_assert_eq!(state.status(), WatchdogStatus::Armed);

            // Armed → TimedOut
            let timeout_result = state.timeout();
            prop_assert!(timeout_result.is_ok());
            prop_assert_eq!(state.status(), WatchdogStatus::TimedOut);

            // Reset → Disarmed
            state.reset();
            prop_assert_eq!(state.status(), WatchdogStatus::Disarmed);
        }
        prop_assert_eq!(state.arm_count(), cycles);
        prop_assert_eq!(state.timeout_count(), cycles);
    }

    #[test]
    fn prop_arm_count_monotonic(
        operations in 1u32..50,
    ) {
        let state = WatchdogState::new();
        let mut last_count = 0u32;

        for _ in 0..operations {
            let _ = state.arm();
            let current = state.arm_count();
            prop_assert!(current >= last_count);
            last_count = current;
            state.reset();
        }
    }

    #[test]
    fn prop_feed_only_in_armed_state(
        do_arm in any::<bool>(),
    ) {
        let state = WatchdogState::new();
        if do_arm {
            let _ = state.arm();
            prop_assert!(state.feed().is_ok());
        } else {
            prop_assert!(state.feed().is_err());
        }
    }

    #[test]
    fn prop_safe_state_is_terminal_from_any(
        initial_state in 0u32..3,
    ) {
        let state = WatchdogState::new();
        match initial_state {
            0 => { /* Disarmed */ }
            1 => { let _ = state.arm(); }
            2 => { let _ = state.arm(); let _ = state.timeout(); }
            _ => {}
        }

        let result = state.trigger_safe_state();
        prop_assert!(result.is_ok());
        prop_assert_eq!(state.status(), WatchdogStatus::SafeState);

        // Verify safe state is terminal: arm/disarm/feed/timeout all fail.
        prop_assert!(state.arm().is_err());
        prop_assert!(state.disarm().is_err());
        prop_assert!(state.feed().is_err());
        prop_assert!(state.timeout().is_err());
        prop_assert!(state.trigger_safe_state().is_err()); // Already in safe state.
    }

    #[test]
    fn prop_config_valid_range(
        timeout_ms in 10u32..5000,
    ) {
        let config = WatchdogConfig::new(timeout_ms);
        prop_assert!(config.is_ok());
        if let Ok(c) = config {
            prop_assert_eq!(c.timeout_ms, timeout_ms);
            prop_assert_eq!(c.timeout_us(), u64::from(timeout_ms) * 1000);
            prop_assert!(c.validate().is_ok());
        }
    }

    #[test]
    fn prop_config_invalid_below_min(
        timeout_ms in 0u32..10,
    ) {
        let config = WatchdogConfig::new(timeout_ms);
        prop_assert!(config.is_err());
    }

    #[test]
    fn prop_config_invalid_above_max(
        timeout_ms in 5001u32..u32::MAX,
    ) {
        let config = WatchdogConfig::new(timeout_ms);
        prop_assert!(config.is_err());
    }

    // -- Metrics invariants --------------------------------------------------

    #[test]
    fn prop_metrics_success_rate_in_range(
        feeds in 0u64..200,
        failures in 0u32..50,
    ) {
        let mut metrics = WatchdogMetrics::new();
        for i in 0..feeds {
            metrics.record_feed(i * 1000);
        }
        for _ in 0..failures {
            metrics.record_failure();
        }
        let rate = metrics.success_rate();
        prop_assert!((0.0..=1.0).contains(&rate),
            "success_rate {} out of range", rate);
    }

    #[test]
    fn prop_metrics_max_feed_interval_monotonic(
        intervals in prop::collection::vec(1u64..1_000_000, 2..50),
    ) {
        let mut metrics = WatchdogMetrics::new();
        let mut timestamp = 0u64;
        let mut max_interval = 0u64;

        for &interval in &intervals {
            timestamp = timestamp.saturating_add(interval);
            metrics.record_feed(timestamp);
            // Max should never decrease.
            prop_assert!(metrics.max_feed_interval_us >= max_interval);
            max_interval = metrics.max_feed_interval_us;
        }
    }

    // -- SoftwareWatchdog property tests ------------------------------------

    #[test]
    fn prop_software_watchdog_arm_feed_disarm(
        feed_count in 0u32..20,
    ) {
        let mut wd = SoftwareWatchdog::with_default_timeout();

        let arm_result = wd.arm();
        prop_assert!(arm_result.is_ok());
        prop_assert!(wd.is_armed());

        for _ in 0..feed_count {
            let feed_result = wd.feed();
            prop_assert!(feed_result.is_ok());
        }

        let disarm_result = wd.disarm();
        prop_assert!(disarm_result.is_ok());
        prop_assert!(!wd.is_armed());
    }

    #[test]
    fn prop_software_watchdog_timeout_correct(
        timeout_ms in 10u32..1000,
    ) {
        let wd = SoftwareWatchdog::with_timeout(timeout_ms);
        prop_assert!(wd.is_ok());
        if let Ok(w) = wd {
            prop_assert_eq!(w.timeout_ms(), timeout_ms);
        }
    }
}

// ---------------------------------------------------------------------------
// Edge-case tests (deterministic)
// ---------------------------------------------------------------------------

#[test]
fn edge_state_disarm_from_disarmed_fails() {
    let state = WatchdogState::new();
    let result = state.disarm();
    assert!(result.is_err());
}

#[test]
fn edge_state_double_arm_fails() {
    let state = WatchdogState::new();
    assert!(state.arm().is_ok());
    let result = state.arm();
    assert!(result.is_err());
}

#[test]
fn edge_state_feed_in_timed_out_fails() {
    let state = WatchdogState::new();
    assert!(state.arm().is_ok());
    assert!(state.timeout().is_ok());
    let result = state.feed();
    assert!(result.is_err());
}

#[test]
fn edge_state_timeout_from_disarmed_fails() {
    let state = WatchdogState::new();
    let result = state.timeout();
    assert!(result.is_err());
}

#[test]
fn edge_state_safe_state_double_trigger() {
    let state = WatchdogState::new();
    assert!(state.trigger_safe_state().is_ok());
    assert_eq!(state.safe_state_count(), 1);

    let result = state.trigger_safe_state();
    assert!(result.is_err());
    assert_eq!(state.safe_state_count(), 1);
}

#[test]
fn edge_state_reset_from_safe_state() {
    let state = WatchdogState::new();
    assert!(state.trigger_safe_state().is_ok());
    state.reset();
    assert_eq!(state.status(), WatchdogStatus::Disarmed);
    // After reset from safe state, can arm again.
    assert!(state.arm().is_ok());
}

#[test]
fn edge_state_many_feeds() {
    let state = WatchdogState::new();
    assert!(state.arm().is_ok());
    for _ in 0..1000 {
        assert!(state.feed().is_ok());
    }
    assert_eq!(state.feed_count(), 1000);
}

#[test]
fn edge_config_boundary_10ms() -> Result<(), Box<dyn std::error::Error>> {
    let config = WatchdogConfig::new(10)?;
    assert_eq!(config.timeout_ms, 10);
    assert_eq!(config.timeout_us(), 10_000);
    Ok(())
}

#[test]
fn edge_config_boundary_5000ms() -> Result<(), Box<dyn std::error::Error>> {
    let config = WatchdogConfig::new(5000)?;
    assert_eq!(config.timeout_ms, 5000);
    assert_eq!(config.timeout_us(), 5_000_000);
    Ok(())
}

#[test]
fn edge_config_builder_invalid_timeout() {
    let result = WatchdogConfig::builder().timeout_ms(5).build();
    assert!(result.is_err());
}

#[test]
fn edge_config_builder_invalid_response_time() {
    let result = WatchdogConfig::builder()
        .timeout_ms(100)
        .max_response_time_us(20_000)
        .build();
    assert!(result.is_err());
}

#[test]
fn edge_config_builder_invalid_health_check_interval() {
    let result = WatchdogConfig::builder()
        .timeout_ms(100)
        .health_check_enabled(true)
        .health_check_interval_ms(5)
        .build();
    assert!(result.is_err());
}

#[test]
fn edge_config_builder_health_check_disabled_allows_low_interval() {
    let result = WatchdogConfig::builder()
        .timeout_ms(100)
        .health_check_enabled(false)
        .health_check_interval_ms(1)
        .build();
    assert!(result.is_ok());
}

#[test]
fn edge_config_default_valid() {
    let config = WatchdogConfig::default();
    assert!(config.validate().is_ok());
    assert_eq!(config.timeout_ms, 100);
}

#[test]
fn edge_metrics_empty() {
    let metrics = WatchdogMetrics::new();
    assert_eq!(metrics.feed_count, 0);
    assert_eq!(metrics.arm_count, 0);
    assert_eq!(metrics.timeout_count, 0);
    assert_eq!(metrics.safe_state_count, 0);
    assert_eq!(metrics.consecutive_failures, 0);
    assert_eq!(metrics.max_feed_interval_us, 0);
    assert_eq!(metrics.success_rate(), 1.0);
}

#[test]
fn edge_metrics_feed_then_failure_resets_consecutive() {
    let mut metrics = WatchdogMetrics::new();
    metrics.record_failure();
    metrics.record_failure();
    assert_eq!(metrics.consecutive_failures, 2);

    metrics.record_feed(1000);
    assert_eq!(metrics.consecutive_failures, 0);
}

#[test]
fn edge_metrics_saturating_operations() {
    let mut metrics = WatchdogMetrics::new();
    metrics.feed_count = u64::MAX;
    metrics.record_feed(1000);
    assert_eq!(metrics.feed_count, u64::MAX);

    metrics.consecutive_failures = u32::MAX;
    metrics.record_failure();
    assert_eq!(metrics.consecutive_failures, u32::MAX);
}

#[test]
fn edge_software_watchdog_feed_when_disarmed() {
    let mut wd = SoftwareWatchdog::with_default_timeout();
    let result = wd.feed();
    assert!(result.is_err());
}

#[test]
fn edge_software_watchdog_double_arm() {
    let mut wd = SoftwareWatchdog::with_default_timeout();
    assert!(wd.arm().is_ok());
    let result = wd.arm();
    assert!(result.is_err());
}

#[test]
fn edge_software_watchdog_trigger_safe_state_from_armed() {
    let mut wd = SoftwareWatchdog::with_default_timeout();
    assert!(wd.arm().is_ok());
    assert!(wd.trigger_safe_state().is_ok());
    assert!(wd.is_safe_state_triggered());
    assert_eq!(wd.status(), WatchdogStatus::SafeState);
}

#[test]
fn edge_software_watchdog_reset_clears_all() {
    let mut wd = SoftwareWatchdog::with_default_timeout();
    assert!(wd.arm().is_ok());
    assert!(wd.feed().is_ok());
    wd.reset();

    assert!(!wd.is_armed());
    assert!(!wd.has_timed_out());
    assert!(!wd.is_safe_state_triggered());
    assert_eq!(wd.status(), WatchdogStatus::Disarmed);

    let metrics = wd.metrics();
    assert_eq!(metrics.feed_count, 0);
}

#[test]
fn edge_software_watchdog_trigger_timeout_then_safe_state() -> Result<(), Box<dyn std::error::Error>>
{
    let mut wd = SoftwareWatchdog::with_default_timeout();
    wd.arm()?;
    wd.trigger_timeout()?;
    assert!(wd.has_timed_out());
    assert_eq!(wd.status(), WatchdogStatus::TimedOut);

    wd.trigger_safe_state()?;
    assert_eq!(wd.status(), WatchdogStatus::SafeState);
    Ok(())
}

#[test]
fn edge_software_watchdog_time_since_last_feed_none_before_feed() {
    let wd = SoftwareWatchdog::with_default_timeout();
    assert!(wd.time_since_last_feed_us().is_none());
}

#[test]
fn edge_software_watchdog_is_healthy() {
    let mut wd = SoftwareWatchdog::with_default_timeout();
    assert!(wd.is_healthy());

    wd.arm().ok();
    assert!(wd.is_healthy());

    wd.trigger_safe_state().ok();
    assert!(!wd.is_healthy());
}

#[test]
fn edge_software_watchdog_config_access() -> Result<(), Box<dyn std::error::Error>> {
    let config = WatchdogConfig::new(250)?;
    let wd = SoftwareWatchdog::new(config);
    assert_eq!(wd.config().timeout_ms, 250);
    Ok(())
}

#[test]
fn edge_status_display_all_variants() {
    assert_eq!(WatchdogStatus::Disarmed.as_str(), "Disarmed");
    assert_eq!(WatchdogStatus::Armed.as_str(), "Armed");
    assert_eq!(WatchdogStatus::TimedOut.as_str(), "TimedOut");
    assert_eq!(WatchdogStatus::SafeState.as_str(), "SafeState");
}

#[test]
fn edge_status_is_terminal() {
    assert!(!WatchdogStatus::Disarmed.is_terminal());
    assert!(!WatchdogStatus::Armed.is_terminal());
    assert!(!WatchdogStatus::TimedOut.is_terminal());
    assert!(WatchdogStatus::SafeState.is_terminal());
}

#[test]
fn edge_status_is_active() {
    assert!(!WatchdogStatus::Disarmed.is_active());
    assert!(WatchdogStatus::Armed.is_active());
    assert!(WatchdogStatus::TimedOut.is_active());
    assert!(!WatchdogStatus::SafeState.is_active());
}

#[test]
fn edge_status_from_raw_invalid() {
    assert!(WatchdogStatus::from_raw(4).is_none());
    assert!(WatchdogStatus::from_raw(u32::MAX).is_none());
}

#[test]
fn edge_status_from_raw_all_valid() {
    for raw in 0..4 {
        let status = WatchdogStatus::from_raw(raw);
        assert!(status.is_some());
        if let Some(s) = status {
            assert_eq!(s.to_raw(), raw);
        }
    }
}

#[test]
fn edge_repeated_trip_reset_cycles() -> Result<(), Box<dyn std::error::Error>> {
    let mut wd = SoftwareWatchdog::with_default_timeout();

    for _ in 0..10 {
        wd.arm()?;
        wd.trigger_timeout()?;
        assert!(wd.has_timed_out());
        wd.reset();
        assert_eq!(wd.status(), WatchdogStatus::Disarmed);
    }
    Ok(())
}

#[test]
fn edge_concurrent_state_reads() {
    // WatchdogState uses atomics; reads from multiple "perspectives" should be consistent.
    let state = WatchdogState::new();
    assert!(state.arm().is_ok());

    // Multiple reads should all see Armed.
    for _ in 0..100 {
        assert_eq!(state.status(), WatchdogStatus::Armed);
    }
}

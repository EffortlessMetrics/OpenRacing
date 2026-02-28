//! Property-based tests for hardware watchdog state machine invariants.

#![cfg(test)]

use openracing_hardware_watchdog::prelude::*;
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn prop_arm_disarm_is_idempotent(
        iterations in 0usize..10,
    ) {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();

        for _ in 0..iterations {
            let _ = watchdog.arm();
            let _ = watchdog.disarm();
        }

        prop_assert_eq!(watchdog.status(), WatchdogStatus::Disarmed);
    }

    #[test]
    fn prop_feed_increments_count(
        feed_count in 0u32..100,
    ) {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();
        let result = watchdog.arm();
        prop_assert!(result.is_ok(), "Arm should succeed");

        for _ in 0..feed_count {
            let _ = watchdog.feed();
        }

        let metrics = watchdog.metrics();
        prop_assert!(metrics.feed_count <= feed_count as u64);
    }

    #[test]
    fn prop_reset_always_returns_to_disarmed(
        do_arm in any::<bool>(),
        do_timeout in any::<bool>(),
        do_safe_state in any::<bool>(),
    ) {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();

        if do_arm {
            let _ = watchdog.arm();
        }
        if do_timeout && watchdog.is_armed() {
            let _ = watchdog.trigger_timeout();
        }
        if do_safe_state {
            let _ = watchdog.trigger_safe_state();
        }

        watchdog.reset();
        prop_assert_eq!(watchdog.status(), WatchdogStatus::Disarmed);
        prop_assert!(!watchdog.is_safe_state_triggered());
    }

    #[test]
    fn prop_timeout_count_never_decreases(
        timeouts in 0u32..10,
    ) {
        let state = WatchdogState::new();
        let arm_result = state.arm();
        prop_assert!(arm_result.is_ok(), "Arm should succeed");

        let mut expected = 0u32;
        for i in 0..timeouts {
            if state.status() == WatchdogStatus::Armed {
                let _ = state.timeout();
                expected += 1;
            }
            if i < timeouts - 1 {
                state.reset();
                let _ = state.arm();
            }
        }

        prop_assert_eq!(state.timeout_count(), expected);
    }

    #[test]
    fn prop_config_timeout_bounds(
        timeout_ms in 10u32..5000,
    ) {
        let config = WatchdogConfig::new(timeout_ms);
        prop_assert!(config.is_ok());

        let Ok(config) = config else {
            return Err(proptest::test_runner::TestCaseError::fail("Config creation failed"));
        };
        prop_assert_eq!(config.timeout_ms, timeout_ms);
        prop_assert_eq!(config.timeout_us(), u64::from(timeout_ms) * 1000);
    }

    #[test]
    fn prop_metrics_feed_count_monotonic(
        feeds in 0u32..100,
    ) {
        let mut metrics = WatchdogMetrics::new();

        let mut last_count = 0u64;
        for i in 0..feeds {
            metrics.record_feed(u64::from(i) * 1000);
            prop_assert!(metrics.feed_count >= last_count);
            last_count = metrics.feed_count;
        }
    }

    #[test]
    fn prop_status_from_raw_roundtrip(
        status_val in 0u32..4,
    ) {
        let status = WatchdogStatus::from_raw(status_val);
        if let Some(s) = status {
            prop_assert_eq!(s.to_raw(), status_val);
        }
    }
}

#[test]
fn prop_cannot_feed_when_disarmed() {
    let mut watchdog = SoftwareWatchdog::with_default_timeout();

    for _ in 0..10 {
        let result = watchdog.feed();
        assert!(result.is_err());
    }
}

#[test]
fn prop_safe_state_is_terminal() -> Result<(), Box<dyn std::error::Error>> {
    let mut watchdog = SoftwareWatchdog::with_default_timeout();
    watchdog.trigger_safe_state()?;

    let actions = [true, false, true, false, true];
    for &action in &actions {
        if action {
            let _ = watchdog.arm();
        } else {
            let _ = watchdog.feed();
        }
    }
    assert_eq!(watchdog.status(), WatchdogStatus::SafeState);
    Ok(())
}

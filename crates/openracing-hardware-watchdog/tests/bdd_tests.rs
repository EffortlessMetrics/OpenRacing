//! BDD tests for hardware watchdog safety scenarios.
//!
//! Feature: watchdog_safety.feature

#![cfg(test)]

use openracing_hardware_watchdog::prelude::*;
use std::time::Instant;

mod watchdog_safety_scenarios {
    use super::*;

    /// Scenario: Watchdog arms successfully
    #[test]
    fn scenario_watchdog_arms_successfully() {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();
        let result = watchdog.arm();
        assert!(result.is_ok());
        assert!(watchdog.is_armed());
        assert_eq!(watchdog.status(), WatchdogStatus::Armed);
    }

    /// Scenario: Watchdog feeds successfully when armed
    #[test]
    fn scenario_watchdog_feeds_successfully_when_armed() {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();
        watchdog.arm().expect("Arm should succeed");
        let result = watchdog.feed();
        assert!(result.is_ok());
        let metrics = watchdog.metrics();
        assert_eq!(metrics.feed_count, 1);
    }

    /// Scenario: Watchdog refuses feed when disarmed
    #[test]
    fn scenario_watchdog_refuses_feed_when_disarmed() {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();
        let result = watchdog.feed();
        assert!(result.is_err());
        assert!(matches!(result, Err(HardwareWatchdogError::NotArmed)));
    }

    /// Scenario: Watchdog triggers safe state on demand
    #[test]
    fn scenario_watchdog_triggers_safe_state_on_demand() {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();
        let result = watchdog.trigger_safe_state();
        assert!(result.is_ok());
        assert!(watchdog.is_safe_state_triggered());
        assert_eq!(watchdog.status(), WatchdogStatus::SafeState);
    }

    /// Scenario: Watchdog cannot be armed twice
    #[test]
    fn scenario_watchdog_cannot_be_armed_twice() {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();
        watchdog.arm().expect("Arm should succeed");
        let result = watchdog.arm();
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(HardwareWatchdogError::InvalidTransition { .. })
        ));
    }

    /// Scenario: Watchdog reset clears all state
    #[test]
    fn scenario_watchdog_reset_clears_all_state() {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();
        watchdog
            .trigger_safe_state()
            .expect("Safe state should succeed");
        watchdog.reset();
        assert_eq!(watchdog.status(), WatchdogStatus::Disarmed);
        assert!(!watchdog.is_safe_state_triggered());
    }

    /// Scenario: Metrics are tracked correctly
    #[test]
    fn scenario_metrics_are_tracked_correctly() {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();
        watchdog.arm().expect("Arm should succeed");
        for _ in 0..5 {
            watchdog.feed().expect("Feed should succeed");
        }
        watchdog.disarm().expect("Disarm should succeed");
        watchdog.arm().expect("Arm should succeed");
        watchdog.feed().expect("Feed should succeed");

        let metrics = watchdog.metrics();
        assert_eq!(metrics.arm_count, 2);
        assert_eq!(metrics.feed_count, 6);
    }

    /// Scenario: Timeout state prevents feeding
    #[test]
    fn scenario_timeout_state_prevents_feeding() {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();
        watchdog.arm().expect("Arm should succeed");
        watchdog.trigger_timeout().expect("Timeout should succeed");
        let result = watchdog.feed();
        assert!(result.is_err());
        assert!(matches!(result, Err(HardwareWatchdogError::TimedOut)));
    }

    /// Scenario: Safe state is terminal
    #[test]
    fn scenario_safe_state_is_terminal() {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();
        watchdog
            .trigger_safe_state()
            .expect("Safe state should succeed");

        let arm_result = watchdog.arm();
        let feed_result = watchdog.feed();

        assert!(arm_result.is_err());
        assert!(feed_result.is_err());
    }

    /// Scenario: Health check works correctly
    #[test]
    fn scenario_health_check_works_correctly() {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();
        assert!(watchdog.is_healthy());

        watchdog.arm().expect("Arm should succeed");
        watchdog.trigger_timeout().expect("Timeout should succeed");

        assert!(!watchdog.is_healthy());
    }
}

mod wcet_requirements {
    use super::*;

    #[test]
    fn scenario_feed_wcet_under_1us() {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();
        watchdog.arm().expect("Arm should succeed");

        let start = Instant::now();
        for _ in 0..1000 {
            let _ = watchdog.feed();
        }
        let elapsed = start.elapsed();

        let avg_ns = elapsed.as_nanos() / 1000;
        assert!(
            avg_ns < 1000,
            "Average feed time {}ns exceeded 1μs WCET budget",
            avg_ns
        );
    }

    #[test]
    fn scenario_status_check_wcet_under_500ns() {
        let watchdog = SoftwareWatchdog::with_default_timeout();

        let start = Instant::now();
        for _ in 0..1000 {
            let _ = watchdog.status();
        }
        let elapsed = start.elapsed();

        let avg_ns = elapsed.as_nanos() / 1000;
        assert!(
            avg_ns < 500,
            "Average status check time {}ns exceeded 500ns WCET budget",
            avg_ns
        );
    }

    #[test]
    fn scenario_arm_wcet_under_10us() {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();

        let times: Vec<_> = (0..100)
            .map(|_| {
                watchdog.reset();
                let start = Instant::now();
                let _ = watchdog.arm();
                start.elapsed()
            })
            .collect();

        let avg_us = times.iter().map(|d| d.as_micros()).sum::<u128>() / 100;
        assert!(
            avg_us < 10,
            "Average arm time {}μs exceeded 10μs WCET budget",
            avg_us
        );
    }
}

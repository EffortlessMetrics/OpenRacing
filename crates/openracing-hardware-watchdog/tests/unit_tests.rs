//! Unit tests for hardware watchdog.

#![cfg(test)]

use openracing_hardware_watchdog::prelude::*;

mod state_transitions {
    use super::*;

    #[test]
    fn test_initial_state_is_disarmed() {
        let watchdog = SoftwareWatchdog::with_default_timeout();
        assert_eq!(watchdog.status(), WatchdogStatus::Disarmed);
        assert!(!watchdog.is_armed());
    }

    #[test]
    fn test_arm_transition() {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();
        watchdog.arm().expect("Arm should succeed");
        assert_eq!(watchdog.status(), WatchdogStatus::Armed);
        assert!(watchdog.is_armed());
    }

    #[test]
    fn test_disarm_transition() {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();
        watchdog.arm().expect("Arm should succeed");
        watchdog.disarm().expect("Disarm should succeed");
        assert_eq!(watchdog.status(), WatchdogStatus::Disarmed);
        assert!(!watchdog.is_armed());
    }

    #[test]
    fn test_cannot_arm_twice() {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();
        watchdog.arm().expect("First arm should succeed");
        let result = watchdog.arm();
        assert!(matches!(
            result,
            Err(HardwareWatchdogError::InvalidTransition { .. })
        ));
    }

    #[test]
    fn test_cannot_disarm_when_not_armed() {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();
        let result = watchdog.disarm();
        assert!(matches!(
            result,
            Err(HardwareWatchdogError::InvalidTransition { .. })
        ));
    }
}

mod feeding {
    use super::*;

    #[test]
    fn test_feed_when_armed() {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();
        watchdog.arm().expect("Arm should succeed");
        let result = watchdog.feed();
        assert!(result.is_ok());
    }

    #[test]
    fn test_feed_when_disarmed() {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();
        let result = watchdog.feed();
        assert!(matches!(result, Err(HardwareWatchdogError::NotArmed)));
    }

    #[test]
    fn test_feed_updates_metrics() {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();
        watchdog.arm().expect("Arm should succeed");

        watchdog.feed().expect("Feed should succeed");
        watchdog.feed().expect("Feed should succeed");
        watchdog.feed().expect("Feed should succeed");

        let metrics = watchdog.metrics();
        assert_eq!(metrics.feed_count, 3);
    }

    #[test]
    fn test_feed_count_increments() {
        let state = WatchdogState::new();
        state.arm().expect("Arm should succeed");

        state.feed().expect("Feed should succeed");
        state.feed().expect("Feed should succeed");

        assert_eq!(state.feed_count(), 2);
    }
}

mod timeout {
    use super::*;

    #[test]
    fn test_has_timed_out_when_armed() {
        let watchdog = SoftwareWatchdog::with_default_timeout();
        assert!(!watchdog.has_timed_out());
    }

    #[test]
    fn test_timeout_state_transition() {
        let state = WatchdogState::new();
        state.arm().expect("Arm should succeed");

        state.timeout().expect("Timeout should succeed");
        assert_eq!(state.status(), WatchdogStatus::TimedOut);
        assert_eq!(state.timeout_count(), 1);
    }

    #[test]
    fn test_cannot_feed_after_timeout() {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();
        watchdog.arm().expect("Arm should succeed");
        watchdog.trigger_timeout().expect("Timeout should succeed");

        let result = watchdog.feed();
        assert!(matches!(result, Err(HardwareWatchdogError::TimedOut)));
    }
}

mod safe_state {
    use super::*;

    #[test]
    fn test_trigger_safe_state() {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();
        watchdog
            .trigger_safe_state()
            .expect("Safe state should succeed");
        assert!(watchdog.is_safe_state_triggered());
        assert_eq!(watchdog.status(), WatchdogStatus::SafeState);
    }

    #[test]
    fn test_safe_state_is_terminal() {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();
        watchdog
            .trigger_safe_state()
            .expect("Safe state should succeed");

        let result = watchdog.arm();
        assert!(result.is_err());

        let result = watchdog.feed();
        assert!(result.is_err());
    }

    #[test]
    fn test_cannot_trigger_safe_state_twice() {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();
        watchdog
            .trigger_safe_state()
            .expect("First trigger should succeed");

        let result = watchdog.trigger_safe_state();
        assert!(matches!(
            result,
            Err(HardwareWatchdogError::SafeStateAlreadyTriggered)
        ));
    }
}

mod reset {
    use super::*;

    #[test]
    fn test_reset_from_armed() {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();
        watchdog.arm().expect("Arm should succeed");
        watchdog.feed().expect("Feed should succeed");

        watchdog.reset();

        assert_eq!(watchdog.status(), WatchdogStatus::Disarmed);
        assert!(!watchdog.is_armed());
    }

    #[test]
    fn test_reset_from_timed_out() {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();
        watchdog.arm().expect("Arm should succeed");
        watchdog.trigger_timeout().expect("Timeout should succeed");

        watchdog.reset();

        assert_eq!(watchdog.status(), WatchdogStatus::Disarmed);
    }

    #[test]
    fn test_reset_from_safe_state() {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();
        watchdog
            .trigger_safe_state()
            .expect("Safe state should succeed");

        watchdog.reset();

        assert_eq!(watchdog.status(), WatchdogStatus::Disarmed);
        assert!(!watchdog.is_safe_state_triggered());
    }

    #[test]
    fn test_reset_clears_metrics() {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();
        watchdog.arm().expect("Arm should succeed");
        watchdog.feed().expect("Feed should succeed");
        watchdog.feed().expect("Feed should succeed");

        watchdog.reset();

        let metrics = watchdog.metrics();
        assert_eq!(metrics.feed_count, 0);
    }
}

mod config {
    use super::*;

    #[test]
    fn test_default_timeout_is_100ms() {
        let config = WatchdogConfig::default();
        assert_eq!(config.timeout_ms, 100);
    }

    #[test]
    fn test_config_validation() {
        let result = WatchdogConfig::new(5);
        assert!(result.is_err());

        let result = WatchdogConfig::new(100);
        assert!(result.is_ok());
    }

    #[test]
    fn test_timeout_us_conversion() {
        let config = WatchdogConfig::new(100).expect("Valid config");
        assert_eq!(config.timeout_us(), 100_000);
    }
}

mod metrics {
    use super::*;

    #[test]
    fn test_metrics_initial_state() {
        let metrics = WatchdogMetrics::new();
        assert_eq!(metrics.feed_count, 0);
        assert_eq!(metrics.arm_count, 0);
        assert_eq!(metrics.timeout_count, 0);
        assert_eq!(metrics.safe_state_count, 0);
    }

    #[test]
    fn test_metrics_record_feed() {
        let mut metrics = WatchdogMetrics::new();
        metrics.record_feed(1000);
        metrics.record_feed(2000);
        metrics.record_feed(3000);

        assert_eq!(metrics.feed_count, 3);
        assert_eq!(metrics.max_feed_interval_us, 1000);
    }

    #[test]
    fn test_metrics_record_failure() {
        let mut metrics = WatchdogMetrics::new();
        metrics.record_failure();
        metrics.record_failure();
        metrics.record_failure();

        assert_eq!(metrics.consecutive_failures, 3);
    }

    #[test]
    fn test_metrics_success_rate() {
        let mut metrics = WatchdogMetrics::new();
        metrics.record_feed(1000);
        metrics.record_feed(2000);
        metrics.record_failure();

        let rate = metrics.success_rate();
        assert!((rate - 0.666).abs() < 0.01);
    }
}

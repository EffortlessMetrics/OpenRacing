//! Unit tests for hardware watchdog.

#![cfg(test)]

use openracing_hardware_watchdog::prelude::*;

mod state_transitions {
    use super::*;

    #[test]
    fn test_initial_state_is_disarmed() -> Result<(), Box<dyn std::error::Error>> {
        let watchdog = SoftwareWatchdog::with_default_timeout();
        assert_eq!(watchdog.status(), WatchdogStatus::Disarmed);
        assert!(!watchdog.is_armed());
        Ok(())
    }

    #[test]
    fn test_arm_transition() -> Result<(), Box<dyn std::error::Error>> {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();
        watchdog.arm()?;
        assert_eq!(watchdog.status(), WatchdogStatus::Armed);
        assert!(watchdog.is_armed());
        Ok(())
    }

    #[test]
    fn test_disarm_transition() -> Result<(), Box<dyn std::error::Error>> {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();
        watchdog.arm()?;
        watchdog.disarm()?;
        assert_eq!(watchdog.status(), WatchdogStatus::Disarmed);
        assert!(!watchdog.is_armed());
        Ok(())
    }

    #[test]
    fn test_cannot_arm_twice() -> Result<(), Box<dyn std::error::Error>> {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();
        watchdog.arm()?;
        let result = watchdog.arm();
        assert!(matches!(
            result,
            Err(HardwareWatchdogError::InvalidTransition { .. })
        ));
        Ok(())
    }

    #[test]
    fn test_cannot_disarm_when_not_armed() -> Result<(), Box<dyn std::error::Error>> {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();
        let result = watchdog.disarm();
        assert!(matches!(
            result,
            Err(HardwareWatchdogError::InvalidTransition { .. })
        ));
        Ok(())
    }
}

mod feeding {
    use super::*;

    #[test]
    fn test_feed_when_armed() -> Result<(), Box<dyn std::error::Error>> {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();
        watchdog.arm()?;
        let result = watchdog.feed();
        assert!(result.is_ok());
        Ok(())
    }

    #[test]
    fn test_feed_when_disarmed() -> Result<(), Box<dyn std::error::Error>> {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();
        let result = watchdog.feed();
        assert!(matches!(result, Err(HardwareWatchdogError::NotArmed)));
        Ok(())
    }

    #[test]
    fn test_feed_updates_metrics() -> Result<(), Box<dyn std::error::Error>> {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();
        watchdog.arm()?;

        watchdog.feed()?;
        watchdog.feed()?;
        watchdog.feed()?;

        let metrics = watchdog.metrics();
        assert_eq!(metrics.feed_count, 3);
        Ok(())
    }

    #[test]
    fn test_feed_count_increments() -> Result<(), Box<dyn std::error::Error>> {
        let state = WatchdogState::new();
        state.arm()?;

        state.feed()?;
        state.feed()?;

        assert_eq!(state.feed_count(), 2);
        Ok(())
    }
}

mod timeout {
    use super::*;

    #[test]
    fn test_has_timed_out_when_armed() -> Result<(), Box<dyn std::error::Error>> {
        let watchdog = SoftwareWatchdog::with_default_timeout();
        assert!(!watchdog.has_timed_out());
        Ok(())
    }

    #[test]
    fn test_timeout_state_transition() -> Result<(), Box<dyn std::error::Error>> {
        let state = WatchdogState::new();
        state.arm()?;

        state.timeout()?;
        assert_eq!(state.status(), WatchdogStatus::TimedOut);
        assert_eq!(state.timeout_count(), 1);
        Ok(())
    }

    #[test]
    fn test_cannot_feed_after_timeout() -> Result<(), Box<dyn std::error::Error>> {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();
        watchdog.arm()?;
        watchdog.trigger_timeout()?;

        let result = watchdog.feed();
        assert!(matches!(result, Err(HardwareWatchdogError::TimedOut)));
        Ok(())
    }
}

mod safe_state {
    use super::*;

    #[test]
    fn test_trigger_safe_state() -> Result<(), Box<dyn std::error::Error>> {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();
        watchdog.trigger_safe_state()?;
        assert!(watchdog.is_safe_state_triggered());
        assert_eq!(watchdog.status(), WatchdogStatus::SafeState);
        Ok(())
    }

    #[test]
    fn test_safe_state_is_terminal() -> Result<(), Box<dyn std::error::Error>> {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();
        watchdog.trigger_safe_state()?;

        let result = watchdog.arm();
        assert!(result.is_err());

        let result = watchdog.feed();
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn test_cannot_trigger_safe_state_twice() -> Result<(), Box<dyn std::error::Error>> {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();
        watchdog.trigger_safe_state()?;

        let result = watchdog.trigger_safe_state();
        assert!(matches!(
            result,
            Err(HardwareWatchdogError::SafeStateAlreadyTriggered)
        ));
        Ok(())
    }
}

mod reset {
    use super::*;

    #[test]
    fn test_reset_from_armed() -> Result<(), Box<dyn std::error::Error>> {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();
        watchdog.arm()?;
        watchdog.feed()?;

        watchdog.reset();

        assert_eq!(watchdog.status(), WatchdogStatus::Disarmed);
        assert!(!watchdog.is_armed());
        Ok(())
    }

    #[test]
    fn test_reset_from_timed_out() -> Result<(), Box<dyn std::error::Error>> {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();
        watchdog.arm()?;
        watchdog.trigger_timeout()?;

        watchdog.reset();

        assert_eq!(watchdog.status(), WatchdogStatus::Disarmed);
        Ok(())
    }

    #[test]
    fn test_reset_from_safe_state() -> Result<(), Box<dyn std::error::Error>> {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();
        watchdog.trigger_safe_state()?;

        watchdog.reset();

        assert_eq!(watchdog.status(), WatchdogStatus::Disarmed);
        assert!(!watchdog.is_safe_state_triggered());
        Ok(())
    }

    #[test]
    fn test_reset_clears_metrics() -> Result<(), Box<dyn std::error::Error>> {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();
        watchdog.arm()?;
        watchdog.feed()?;
        watchdog.feed()?;

        watchdog.reset();

        let metrics = watchdog.metrics();
        assert_eq!(metrics.feed_count, 0);
        Ok(())
    }
}

mod config {
    use super::*;

    #[test]
    fn test_default_timeout_is_100ms() -> Result<(), Box<dyn std::error::Error>> {
        let config = WatchdogConfig::default();
        assert_eq!(config.timeout_ms, 100);
        Ok(())
    }

    #[test]
    fn test_config_validation() -> Result<(), Box<dyn std::error::Error>> {
        let result = WatchdogConfig::new(5);
        assert!(result.is_err());

        let result = WatchdogConfig::new(100);
        assert!(result.is_ok());
        Ok(())
    }

    #[test]
    fn test_timeout_us_conversion() -> Result<(), Box<dyn std::error::Error>> {
        let config = WatchdogConfig::new(100)?;
        assert_eq!(config.timeout_us(), 100_000);
        Ok(())
    }
}

mod metrics {
    use super::*;

    #[test]
    fn test_metrics_initial_state() -> Result<(), Box<dyn std::error::Error>> {
        let metrics = WatchdogMetrics::new();
        assert_eq!(metrics.feed_count, 0);
        assert_eq!(metrics.arm_count, 0);
        assert_eq!(metrics.timeout_count, 0);
        assert_eq!(metrics.safe_state_count, 0);
        Ok(())
    }

    #[test]
    fn test_metrics_record_feed() -> Result<(), Box<dyn std::error::Error>> {
        let mut metrics = WatchdogMetrics::new();
        metrics.record_feed(1000);
        metrics.record_feed(2000);
        metrics.record_feed(3000);

        assert_eq!(metrics.feed_count, 3);
        assert_eq!(metrics.max_feed_interval_us, 1000);
        Ok(())
    }

    #[test]
    fn test_metrics_record_failure() -> Result<(), Box<dyn std::error::Error>> {
        let mut metrics = WatchdogMetrics::new();
        metrics.record_failure();
        metrics.record_failure();
        metrics.record_failure();

        assert_eq!(metrics.consecutive_failures, 3);
        Ok(())
    }

    #[test]
    fn test_metrics_success_rate() -> Result<(), Box<dyn std::error::Error>> {
        let mut metrics = WatchdogMetrics::new();
        metrics.record_feed(1000);
        metrics.record_feed(2000);
        metrics.record_failure();

        let rate = metrics.success_rate();
        assert!((rate - 0.666).abs() < 0.01);
        Ok(())
    }
}

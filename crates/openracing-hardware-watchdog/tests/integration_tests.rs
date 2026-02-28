//! Integration tests for full watchdog lifecycle.

#![cfg(test)]

use openracing_hardware_watchdog::prelude::*;
use std::sync::{Arc, Mutex};
use std::thread;

type TestResult = Result<(), Box<dyn std::error::Error>>;

mod full_lifecycle {
    use super::*;

    #[test]
    fn test_complete_lifecycle() -> TestResult {
        let config = WatchdogConfig::builder()
            .timeout_ms(200)
            .max_response_time_us(500)
            .build()?;

        let mut watchdog = SoftwareWatchdog::new(config);

        assert_eq!(watchdog.status(), WatchdogStatus::Disarmed);
        assert!(!watchdog.is_armed());
        assert!(watchdog.is_healthy());

        watchdog.arm()?;
        assert!(watchdog.is_armed());
        assert_eq!(watchdog.status(), WatchdogStatus::Armed);

        for _ in 0..10 {
            watchdog.feed()?;
        }

        let metrics = watchdog.metrics();
        assert_eq!(metrics.feed_count, 10);
        assert_eq!(metrics.arm_count, 1);

        watchdog.disarm()?;
        assert!(!watchdog.is_armed());

        watchdog.reset();
        let metrics = watchdog.metrics();
        assert_eq!(metrics.feed_count, 0);
        Ok(())
    }

    #[test]
    fn test_timeout_lifecycle() -> TestResult {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();

        watchdog.arm()?;
        watchdog.feed()?;

        watchdog.trigger_timeout()?;
        assert!(watchdog.has_timed_out());

        watchdog.reset();
        watchdog.arm()?;
        assert!(watchdog.is_armed());
        Ok(())
    }

    #[test]
    fn test_safe_state_lifecycle() -> TestResult {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();

        watchdog.arm()?;
        watchdog.trigger_safe_state()?;

        assert!(watchdog.is_safe_state_triggered());
        assert_eq!(watchdog.status(), WatchdogStatus::SafeState);
        assert!(!watchdog.is_healthy());

        watchdog.reset();

        watchdog.arm()?;
        assert!(watchdog.is_healthy());
        Ok(())
    }

    #[test]
    fn test_multiple_arm_disarm_cycles() -> TestResult {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();

        for _ in 0..5 {
            watchdog.arm()?;
            for _ in 0..3 {
                watchdog.feed()?;
            }
            watchdog.disarm()?;
        }

        let metrics = watchdog.metrics();
        assert_eq!(metrics.arm_count, 5);
        assert_eq!(metrics.feed_count, 15);
        Ok(())
    }
}

mod concurrent_access {
    use super::*;

    #[test]
    fn test_concurrent_status_checks() -> TestResult {
        let watchdog = Arc::new(Mutex::new(SoftwareWatchdog::with_default_timeout()));
        watchdog.lock().map_err(|e| e.to_string())?.arm()?;

        let handles: Vec<_> = (0..4)
            .map(|_| {
                let w = Arc::clone(&watchdog);
                thread::spawn(move || {
                    for _ in 0..100 {
                        if let Ok(guard) = w.lock() {
                            let status = guard.status();
                            assert!(matches!(
                                status,
                                WatchdogStatus::Armed | WatchdogStatus::TimedOut
                            ));
                        }
                    }
                })
            })
            .collect();

        for handle in handles {
            assert!(handle.join().is_ok(), "Thread should not panic");
        }
        Ok(())
    }

    #[test]
    fn test_concurrent_feed_operations() -> TestResult {
        let watchdog = Arc::new(Mutex::new(SoftwareWatchdog::with_default_timeout()));
        watchdog.lock().map_err(|e| e.to_string())?.arm()?;

        let handles: Vec<_> = (0..4)
            .map(|_| {
                let w = Arc::clone(&watchdog);
                thread::spawn(move || {
                    for _ in 0..10 {
                        if let Ok(mut guard) = w.lock() {
                            let _ = guard.feed();
                        }
                    }
                })
            })
            .collect();

        for handle in handles {
            assert!(handle.join().is_ok(), "Thread should not panic");
        }

        let metrics = watchdog.lock().map_err(|e| e.to_string())?.metrics();
        assert!(metrics.feed_count > 0);
        Ok(())
    }
}

mod error_handling {
    use super::*;
    use std::string::ToString;

    #[test]
    fn test_error_display() {
        assert_eq!(
            HardwareWatchdogError::NotArmed.to_string(),
            "Watchdog is not armed"
        );
        assert_eq!(
            HardwareWatchdogError::AlreadyArmed.to_string(),
            "Watchdog is already armed"
        );
        assert_eq!(
            HardwareWatchdogError::TimedOut.to_string(),
            "Watchdog has timed out"
        );
    }

    #[test]
    fn test_all_errors_are_recoverable() {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();

        let _ = watchdog.feed();
        let _ = watchdog.arm();
        let _ = watchdog.arm();
        let _ = watchdog.feed();
        let _ = watchdog.disarm();
        let _ = watchdog.disarm();

        watchdog.reset();
        assert_eq!(watchdog.status(), WatchdogStatus::Disarmed);
    }
}

mod time_tracking {
    use super::*;

    #[test]
    fn test_time_since_last_feed() -> TestResult {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();

        assert!(watchdog.time_since_last_feed_us().is_none());

        watchdog.arm()?;
        watchdog.feed()?;

        assert!(watchdog.time_since_last_feed_us().is_some());
        Ok(())
    }

    #[test]
    fn test_external_time_source() -> TestResult {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();

        watchdog.arm()?;
        watchdog.set_elapsed_us(100_000);
        watchdog.feed()?;

        watchdog.set_elapsed_us(150_000);
        let elapsed = watchdog.time_since_last_feed_us();
        assert!(elapsed.is_some());
        Ok(())
    }
}

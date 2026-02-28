//! Integration tests for full watchdog lifecycle.

#![cfg(test)]

use openracing_hardware_watchdog::prelude::*;
use std::sync::{Arc, Mutex};
use std::thread;

mod full_lifecycle {
    use super::*;

    #[test]
    fn test_complete_lifecycle() {
        let config = WatchdogConfig::builder()
            .timeout_ms(200)
            .max_response_time_us(500)
            .build()
            .expect("Valid config");

        let mut watchdog = SoftwareWatchdog::new(config);

        assert_eq!(watchdog.status(), WatchdogStatus::Disarmed);
        assert!(!watchdog.is_armed());
        assert!(watchdog.is_healthy());

        watchdog.arm().expect("Arm should succeed");
        assert!(watchdog.is_armed());
        assert_eq!(watchdog.status(), WatchdogStatus::Armed);

        for _ in 0..10 {
            watchdog.feed().expect("Feed should succeed");
        }

        let metrics = watchdog.metrics();
        assert_eq!(metrics.feed_count, 10);
        assert_eq!(metrics.arm_count, 1);

        watchdog.disarm().expect("Disarm should succeed");
        assert!(!watchdog.is_armed());

        watchdog.reset();
        let metrics = watchdog.metrics();
        assert_eq!(metrics.feed_count, 0);
    }

    #[test]
    fn test_timeout_lifecycle() {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();

        watchdog.arm().expect("Arm should succeed");
        watchdog.feed().expect("Feed should succeed");

        watchdog.trigger_timeout().expect("Timeout should succeed");
        assert!(watchdog.has_timed_out());

        watchdog.reset();
        watchdog.arm().expect("Arm after reset should succeed");
        assert!(watchdog.is_armed());
    }

    #[test]
    fn test_safe_state_lifecycle() {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();

        watchdog.arm().expect("Arm should succeed");
        watchdog
            .trigger_safe_state()
            .expect("Safe state should succeed");

        assert!(watchdog.is_safe_state_triggered());
        assert_eq!(watchdog.status(), WatchdogStatus::SafeState);
        assert!(!watchdog.is_healthy());

        watchdog.reset();

        watchdog.arm().expect("Arm after reset should succeed");
        assert!(watchdog.is_healthy());
    }

    #[test]
    fn test_multiple_arm_disarm_cycles() {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();

        for cycle in 0..5 {
            watchdog
                .arm()
                .unwrap_or_else(|_| panic!("Arm cycle {cycle} should succeed"));
            for _ in 0..3 {
                watchdog.feed().expect("Feed should succeed");
            }
            watchdog
                .disarm()
                .unwrap_or_else(|_| panic!("Disarm cycle {cycle} should succeed"));
        }

        let metrics = watchdog.metrics();
        assert_eq!(metrics.arm_count, 5);
        assert_eq!(metrics.feed_count, 15);
    }
}

mod concurrent_access {
    use super::*;

    #[test]
    fn test_concurrent_status_checks() {
        let watchdog = Arc::new(Mutex::new(SoftwareWatchdog::with_default_timeout()));
        watchdog
            .lock()
            .expect("Lock should succeed")
            .arm()
            .expect("Arm should succeed");

        let handles: Vec<_> = (0..4)
            .map(|_| {
                let w = Arc::clone(&watchdog);
                thread::spawn(move || {
                    for _ in 0..100 {
                        let status = w.lock().expect("Lock should succeed").status();
                        assert!(matches!(
                            status,
                            WatchdogStatus::Armed | WatchdogStatus::TimedOut
                        ));
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().expect("Thread should not panic");
        }
    }

    #[test]
    fn test_concurrent_feed_operations() {
        let watchdog = Arc::new(Mutex::new(SoftwareWatchdog::with_default_timeout()));
        watchdog
            .lock()
            .expect("Lock should succeed")
            .arm()
            .expect("Arm should succeed");

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
            handle.join().expect("Thread should not panic");
        }

        let metrics = watchdog.lock().expect("Lock should succeed").metrics();
        assert!(metrics.feed_count > 0);
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
    fn test_time_since_last_feed() {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();

        assert!(watchdog.time_since_last_feed_us().is_none());

        watchdog.arm().expect("Arm should succeed");
        watchdog.feed().expect("Feed should succeed");

        assert!(watchdog.time_since_last_feed_us().is_some());
    }

    #[test]
    fn test_external_time_source() {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();

        watchdog.arm().expect("Arm should succeed");
        watchdog.set_elapsed_us(100_000);
        watchdog.feed().expect("Feed should succeed");

        watchdog.set_elapsed_us(150_000);
        let elapsed = watchdog.time_since_last_feed_us();
        assert!(elapsed.is_some());
    }
}

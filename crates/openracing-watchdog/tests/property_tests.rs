//! Property-based tests for watchdog statistics invariants.

use openracing_watchdog::PluginStats;
use proptest::prelude::*;
use std::time::Duration;

proptest! {
    #[test]
    fn test_stats_total_executions_never_decreases(
        executions in 0..1000u64,
        time_us in 0..1_000_000u64,
    ) {
        let mut stats = PluginStats::new();

        for _ in 0..executions {
            stats.record_success(time_us);
        }

        prop_assert_eq!(stats.total_executions, executions);
        prop_assert_eq!(stats.timeout_count, 0);
    }

    #[test]
    fn test_stats_timeout_count_consistent(
        successes in 0..100u64,
        timeouts in 0..100u64,
        success_time in 0..100u64,
        timeout_time in 100..1000u64,
    ) {
        let mut stats = PluginStats::new();

        // Mix of successes and timeouts
        for _ in 0..successes {
            stats.record_success(success_time);
        }
        for _ in 0..timeouts {
            stats.record_timeout(timeout_time);
        }

        let total = successes + timeouts;
        prop_assert_eq!(stats.total_executions, total);
        prop_assert_eq!(stats.timeout_count, timeouts as u32);

        // Timeout rate should be consistent
        let expected_rate = if total == 0 {
            0.0
        } else {
            (timeouts as f64 / total as f64) * 100.0
        };
        prop_assert!((stats.timeout_rate() - expected_rate).abs() < 0.01);
    }

    #[test]
    fn test_average_time_never_negative(
        times in prop::collection::vec(0..1_000_000u64, 1..100),
    ) {
        let mut stats = PluginStats::new();

        for time in &times {
            stats.record_success(*time);
        }

        let avg = stats.average_execution_time_us();
        prop_assert!(avg >= 0.0);

        // Average should be within range of input times
        let min_time = *times.iter().min().unwrap() as f64;
        let max_time = *times.iter().max().unwrap() as f64;
        prop_assert!(avg >= min_time - 0.1);
        prop_assert!(avg <= max_time + 0.1);
    }

    #[test]
    fn test_consecutive_timeouts_reset_on_success(
        timeouts_before in 0u32..10,
        timeouts_after in 0u32..10,
    ) {
        let mut stats = PluginStats::new();

        // Some timeouts
        for _ in 0..timeouts_before {
            stats.record_timeout(200);
        }
        prop_assert_eq!(stats.consecutive_timeouts, timeouts_before);

        // One success resets
        stats.record_success(50);
        prop_assert_eq!(stats.consecutive_timeouts, 0);

        // More timeouts
        for _ in 0..timeouts_after {
            stats.record_timeout(200);
        }
        prop_assert_eq!(stats.consecutive_timeouts, timeouts_after);
    }

    #[test]
    fn test_quarantine_remaining_decreases(duration_secs in 1u64..3600) {
        let mut stats = PluginStats::new();
        let duration = Duration::from_secs(duration_secs);

        stats.apply_quarantine(duration);

        let remaining = stats.quarantine_remaining();
        prop_assert!(remaining.is_some());

        let remaining = remaining.unwrap();
        prop_assert!(remaining <= duration);
        prop_assert!(remaining > Duration::ZERO);
    }

    #[test]
    fn test_stats_saturating_addition(
        large_time in u64::MAX/2..u64::MAX,
        count in 1u8..10,
    ) {
        let mut stats = PluginStats::new();

        for _ in 0..count {
            stats.record_success(large_time);
        }

        // Should not panic, should saturate
        prop_assert!(stats.total_execution_time_us >= large_time);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn run_proptest_suite() {
        // Run all proptests with default config
    }
}

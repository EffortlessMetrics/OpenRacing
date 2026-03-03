//! Deep tests for the watchdog subsystem.
//!
//! Covers timeout handling, re-arm behavior, cascading watchdogs,
//! metrics tracking, and property-based tests for feed sequences.

use openracing_watchdog::prelude::*;
use proptest::prelude::*;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Helper: build a WatchdogSystem with a low timeout and low max-timeouts
// ---------------------------------------------------------------------------
fn system_with_timeout(timeout_us: u64, max_timeouts: u32) -> WatchdogSystem {
    let config = WatchdogConfig {
        plugin_timeout_us: timeout_us,
        plugin_max_timeouts: max_timeouts,
        plugin_quarantine_duration: Duration::from_millis(500),
        ..Default::default()
    };
    WatchdogSystem::new(config)
}

// ===== 1. Watchdog timeout: arm → miss deadline → triggers timeout handler =====

#[test]
fn watchdog_timeout_triggers_quarantine_and_callback() -> Result<(), Box<dyn std::error::Error>> {
    let wd = system_with_timeout(100, 3);
    let triggered = Arc::new(AtomicU32::new(0));
    let t = Arc::clone(&triggered);
    wd.add_fault_callback(move |ft, _plugin| {
        if ft == FaultType::PluginOverrun {
            t.fetch_add(1, Ordering::SeqCst);
        }
    });

    // Three overruns should trigger quarantine
    let mut last_fault = None;
    for _ in 0..3 {
        last_fault = wd.record_plugin_execution("slow_plugin", 200);
    }

    assert_eq!(last_fault, Some(FaultType::PluginOverrun));
    assert!(wd.is_plugin_quarantined("slow_plugin"));
    assert_eq!(triggered.load(Ordering::SeqCst), 1);
    Ok(())
}

// ===== 2. Watchdog re-arm: feed within timeout → no trigger =====

#[test]
fn watchdog_rearm_no_trigger_when_fed_in_time() -> Result<(), Box<dyn std::error::Error>> {
    let wd = system_with_timeout(100, 5);

    // Alternate: one slow, then one fast — consecutive counter should reset
    for _ in 0..20 {
        let fault = wd.record_plugin_execution("plugin_a", 200); // over
        assert!(fault.is_none()); // only 1 consecutive
        let fault = wd.record_plugin_execution("plugin_a", 50); // under — resets
        assert!(fault.is_none());
    }

    assert!(!wd.is_plugin_quarantined("plugin_a"));
    let stats = wd.get_plugin_stats("plugin_a");
    assert!(stats.is_some());
    let stats = stats.ok_or("missing stats")?;
    assert_eq!(stats.consecutive_timeouts, 0);
    Ok(())
}

// ===== 3. Watchdog cascading: multiple watchdogs, one times out =====

#[test]
fn watchdog_cascading_only_faulty_plugin_quarantined() -> Result<(), Box<dyn std::error::Error>> {
    let wd = system_with_timeout(100, 3);

    wd.register_plugin("good_plugin");
    wd.register_plugin("bad_plugin");
    wd.register_plugin("neutral_plugin");

    // Good plugin always fast
    for _ in 0..10 {
        let f = wd.record_plugin_execution("good_plugin", 50);
        assert!(f.is_none());
    }

    // Bad plugin always slow → quarantined after 3
    for i in 0..3 {
        let f = wd.record_plugin_execution("bad_plugin", 200);
        if i < 2 {
            assert!(f.is_none());
        } else {
            assert_eq!(f, Some(FaultType::PluginOverrun));
        }
    }

    // Neutral never executed beyond registration
    assert!(wd.is_plugin_quarantined("bad_plugin"));
    assert!(!wd.is_plugin_quarantined("good_plugin"));
    assert!(!wd.is_plugin_quarantined("neutral_plugin"));
    Ok(())
}

// ===== 4. Watchdog metrics: track trigger count, max lateness =====

#[test]
fn watchdog_metrics_track_trigger_count_and_lateness() -> Result<(), Box<dyn std::error::Error>> {
    let wd = system_with_timeout(100, 3);

    // Record a mix of executions
    wd.record_plugin_execution("metrics_plugin", 50);
    wd.record_plugin_execution("metrics_plugin", 80);
    wd.record_plugin_execution("metrics_plugin", 200); // timeout
    wd.record_plugin_execution("metrics_plugin", 300); // timeout
    wd.record_plugin_execution("metrics_plugin", 60);

    let stats = wd
        .get_plugin_stats("metrics_plugin")
        .ok_or("missing stats")?;
    assert_eq!(stats.total_executions, 5);
    assert_eq!(stats.timeout_count, 2);
    // After the success at 60, consecutive_timeouts resets
    assert_eq!(stats.consecutive_timeouts, 0);
    assert_eq!(stats.last_execution_time_us, 60);

    // Performance metrics map
    let perf = wd.get_plugin_performance_metrics();
    let pm = perf.get("metrics_plugin").ok_or("missing perf metrics")?;
    assert!((pm["total_executions"] - 5.0).abs() < f64::EPSILON);
    assert!((pm["timeout_rate_percent"] - 40.0).abs() < f64::EPSILON);
    Ok(())
}

// ===== 5. Property test: any feed sequence within timeout never triggers =====

proptest! {
    #[test]
    fn prop_within_timeout_never_triggers(
        executions in prop::collection::vec(1_u64..=100, 1..200),
    ) {
        let wd = system_with_timeout(100, 5);
        for exec_time in &executions {
            let fault = wd.record_plugin_execution("prop_plugin", *exec_time);
            prop_assert!(fault.is_none(), "Should never quarantine within budget");
        }
        prop_assert!(!wd.is_plugin_quarantined("prop_plugin"));
    }
}

// ===== 6. Property test: any sequence exceeding timeout always triggers =====

proptest! {
    #[test]
    fn prop_exceeding_timeout_always_triggers(
        max_timeouts in 1_u32..=10,
        excess in 101_u64..=500,
    ) {
        let wd = system_with_timeout(100, max_timeouts);
        let mut triggered = false;
        for _ in 0..max_timeouts {
            if let Some(FaultType::PluginOverrun) =
                wd.record_plugin_execution("prop_bad", excess)
            {
                triggered = true;
            }
        }
        prop_assert!(triggered, "Must quarantine after {} overruns", max_timeouts);
        prop_assert!(wd.is_plugin_quarantined("prop_bad"));
    }
}

// ===== 7. Component health heartbeat timeout integration =====

#[test]
fn watchdog_component_heartbeat_timeout() -> Result<(), Box<dyn std::error::Error>> {
    let config = WatchdogConfig {
        rt_thread_timeout_ms: 10,
        health_check_interval: Duration::from_millis(0), // always eligible
        ..Default::default()
    };
    let wd = WatchdogSystem::new(config);

    // Send an initial heartbeat, then wait for it to expire
    wd.heartbeat(SystemComponent::RtThread);
    std::thread::sleep(Duration::from_millis(20));

    let faults = wd.perform_health_checks();
    assert!(
        faults.contains(&FaultType::TimingViolation),
        "Expected TimingViolation after heartbeat timeout"
    );
    Ok(())
}

// ===== Additional: quarantine release and re-quarantine cycle =====

#[test]
fn watchdog_quarantine_release_and_requarantine() -> Result<(), Box<dyn std::error::Error>> {
    let wd = system_with_timeout(100, 2);

    // Quarantine
    wd.record_plugin_execution("cycle_plugin", 200);
    let fault = wd.record_plugin_execution("cycle_plugin", 200);
    assert_eq!(fault, Some(FaultType::PluginOverrun));
    assert!(wd.is_plugin_quarantined("cycle_plugin"));

    // Release
    wd.release_plugin_quarantine("cycle_plugin")?;
    assert!(!wd.is_plugin_quarantined("cycle_plugin"));

    // Re-quarantine
    wd.record_plugin_execution("cycle_plugin", 200);
    let fault = wd.record_plugin_execution("cycle_plugin", 200);
    assert_eq!(fault, Some(FaultType::PluginOverrun));
    assert!(wd.is_plugin_quarantined("cycle_plugin"));
    Ok(())
}

// ===== Additional: quarantine policy disabled prevents quarantine =====

#[test]
fn watchdog_quarantine_policy_disabled() -> Result<(), Box<dyn std::error::Error>> {
    let wd = system_with_timeout(100, 2);
    wd.set_quarantine_policy_enabled(false);

    for _ in 0..20 {
        let fault = wd.record_plugin_execution("no_quarantine", 200);
        assert!(fault.is_none());
    }

    assert!(!wd.is_plugin_quarantined("no_quarantine"));
    assert!(!wd.is_quarantine_policy_enabled());
    Ok(())
}

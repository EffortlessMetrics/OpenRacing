//! Concurrency tests for the watchdog system.

use openracing_watchdog::prelude::*;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn test_concurrent_plugin_registration() {
    let watchdog = Arc::new(WatchdogSystem::default());
    let mut handles = vec![];

    // Spawn multiple threads registering plugins
    for i in 0..10 {
        let watchdog_clone = watchdog.clone();
        let handle = thread::spawn(move || {
            let plugin_id = format!("plugin_{}", i);
            watchdog_clone.register_plugin(&plugin_id);
            watchdog_clone.record_plugin_execution(&plugin_id, 50);
        });
        handles.push(handle);
    }

    for handle in handles {
        assert!(handle.join().is_ok(), "Thread should not panic");
    }

    // All plugins should be registered
    assert_eq!(watchdog.plugin_count(), 10);
}

#[test]
fn test_concurrent_heartbeats() {
    let watchdog = Arc::new(WatchdogSystem::default());
    let mut handles = vec![];

    // All components should be registered
    let components: Vec<SystemComponent> = SystemComponent::all().collect();

    for component in components {
        let watchdog_clone = watchdog.clone();
        let handle = thread::spawn(move || {
            for _ in 0..100 {
                watchdog_clone.heartbeat(component);
                thread::sleep(Duration::from_micros(10));
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        assert!(handle.join().is_ok(), "Thread should not panic");
    }

    // All components should be healthy
    let summary = watchdog.get_health_summary();
    for (_, status) in summary {
        assert_eq!(status, HealthStatus::Healthy);
    }
}

#[test]
fn test_concurrent_execution_recording() -> TestResult {
    let watchdog = Arc::new(WatchdogSystem::default());
    let mut handles = vec![];

    // Multiple threads recording to the same plugin
    for _ in 0..4 {
        let watchdog_clone = watchdog.clone();
        let handle = thread::spawn(move || {
            for i in 0..100 {
                let time_us = if i % 10 == 0 { 150 } else { 50 };
                watchdog_clone.record_plugin_execution("shared_plugin", time_us);
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        assert!(handle.join().is_ok(), "Thread should not panic");
    }

    // Should have recorded all executions
    let stats = watchdog
        .get_plugin_stats("shared_plugin")
        .ok_or("Expected plugin stats")?;
    assert_eq!(stats.total_executions, 400);
    // ~40 timeouts (10% from each thread)
    assert!(stats.timeout_count >= 35 && stats.timeout_count <= 45);
    Ok(())
}

#[test]
fn test_concurrent_quarantine_release() {
    let watchdog = Arc::new(WatchdogSystem::default());

    // Trigger quarantine
    for _ in 0..5 {
        watchdog.record_plugin_execution("test_plugin", 200);
    }
    assert!(watchdog.is_plugin_quarantined("test_plugin"));

    // Multiple threads trying to release quarantine
    let mut handles = vec![];
    let success_count = Arc::new(std::sync::atomic::AtomicU32::new(0));

    for _ in 0..5 {
        let watchdog_clone = watchdog.clone();
        let count_clone = success_count.clone();
        let handle = thread::spawn(move || {
            if watchdog_clone
                .release_plugin_quarantine("test_plugin")
                .is_ok()
            {
                count_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        assert!(handle.join().is_ok(), "Thread should not panic");
    }

    // Only one thread should succeed
    assert_eq!(success_count.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert!(!watchdog.is_plugin_quarantined("test_plugin"));
}

#[test]
fn test_concurrent_health_checks() -> TestResult {
    let watchdog = Arc::new(WatchdogSystem::default());
    let mut handles = vec![];

    // Send heartbeats while performing health checks
    for _ in 0..3 {
        let watchdog_clone = watchdog.clone();
        let handle = thread::spawn(move || {
            for _ in 0..100 {
                watchdog_clone.heartbeat(SystemComponent::RtThread);
                thread::sleep(Duration::from_micros(50));
            }
        });
        handles.push(handle);
    }

    for _ in 0..2 {
        let watchdog_clone = watchdog.clone();
        let handle = thread::spawn(move || {
            for _ in 0..50 {
                watchdog_clone.perform_health_checks();
                thread::sleep(Duration::from_micros(100));
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        assert!(handle.join().is_ok(), "Thread should not panic");
    }

    // Component should remain healthy
    let health = watchdog
        .get_component_health(SystemComponent::RtThread)
        .ok_or("Expected component health")?;
    assert_eq!(health.status, HealthStatus::Healthy);
    Ok(())
}

#[test]
fn test_concurrent_stats_access() -> TestResult {
    let watchdog = Arc::new(WatchdogSystem::default());

    // Record initial stats
    for _ in 0..10 {
        watchdog.record_plugin_execution("test_plugin", 50);
    }

    let mut handles = vec![];

    // Read stats while recording more
    for _ in 0..5 {
        let watchdog_clone = watchdog.clone();
        let handle = thread::spawn(move || {
            for _ in 0..100 {
                let _ = watchdog_clone.get_plugin_stats("test_plugin");
                thread::sleep(Duration::from_micros(10));
            }
        });
        handles.push(handle);
    }

    for _ in 0..2 {
        let watchdog_clone = watchdog.clone();
        let handle = thread::spawn(move || {
            for _ in 0..100 {
                watchdog_clone.record_plugin_execution("test_plugin", 50);
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        assert!(handle.join().is_ok(), "Thread should not panic");
    }

    // Final count should be consistent
    let stats = watchdog
        .get_plugin_stats("test_plugin")
        .ok_or("Expected plugin stats")?;
    assert_eq!(stats.total_executions, 210); // 10 initial + 200 from threads
    Ok(())
}

#[test]
fn test_concurrent_policy_toggle() -> TestResult {
    let watchdog = Arc::new(WatchdogSystem::default());
    let mut handles = vec![];

    // One thread toggles policy
    {
        let watchdog_clone = watchdog.clone();
        let handle = thread::spawn(move || {
            for i in 0..100 {
                watchdog_clone.set_quarantine_policy_enabled(i % 2 == 0);
                thread::sleep(Duration::from_micros(100));
            }
        });
        handles.push(handle);
    }

    // Other threads record executions
    for _ in 0..3 {
        let watchdog_clone = watchdog.clone();
        let handle = thread::spawn(move || {
            for _ in 0..100 {
                watchdog_clone.record_plugin_execution("test_plugin", 200);
                thread::sleep(Duration::from_micros(50));
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        assert!(handle.join().is_ok(), "Thread should not panic");
    }

    // Stats should still be valid
    let stats = watchdog
        .get_plugin_stats("test_plugin")
        .ok_or("Expected plugin stats")?;
    assert!(stats.total_executions > 0);
    assert!(stats.timeout_count > 0);
    Ok(())
}

#[test]
fn test_stress_many_plugins() {
    let watchdog = Arc::new(WatchdogSystem::default());
    let mut handles = vec![];

    // Create 100 plugins concurrently
    for i in 0..100 {
        let watchdog_clone = watchdog.clone();
        let handle = thread::spawn(move || {
            let plugin_id = format!("plugin_{}", i);
            watchdog_clone.register_plugin(&plugin_id);
            for j in 0..10 {
                let time_us = if j % 5 == 0 { 150 } else { 50 };
                watchdog_clone.record_plugin_execution(&plugin_id, time_us);
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        assert!(handle.join().is_ok(), "Thread should not panic");
    }

    assert_eq!(watchdog.plugin_count(), 100);

    // Check some stats
    let metrics = watchdog.get_plugin_performance_metrics();
    assert_eq!(metrics.len(), 100);

    // Each plugin should have 10 executions
    for (_, plugin_metrics) in metrics {
        assert_eq!(plugin_metrics["total_executions"], 10.0);
    }
}

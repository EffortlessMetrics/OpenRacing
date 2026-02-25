//! Performance benchmarks for the watchdog system.

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use openracing_watchdog::prelude::*;
use std::hint::black_box;
use std::time::Duration;

fn bench_record_plugin_execution(c: &mut Criterion) {
    let watchdog = WatchdogSystem::default();

    c.bench_function("record_plugin_execution_success", |b| {
        b.iter(|| watchdog.record_plugin_execution(black_box("test_plugin"), black_box(50)));
    });

    c.bench_function("record_plugin_execution_timeout", |b| {
        let watchdog2 = WatchdogSystem::default();
        b.iter(|| watchdog2.record_plugin_execution(black_box("test_plugin"), black_box(150)));
    });
}

fn bench_heartbeat(c: &mut Criterion) {
    let watchdog = WatchdogSystem::default();

    c.bench_function("heartbeat_rt_thread", |b| {
        b.iter(|| watchdog.heartbeat(black_box(SystemComponent::RtThread)));
    });

    c.bench_function("heartbeat_all_components", |b| {
        b.iter(|| {
            watchdog.heartbeat(SystemComponent::RtThread);
            watchdog.heartbeat(SystemComponent::HidCommunication);
            watchdog.heartbeat(SystemComponent::TelemetryAdapter);
            watchdog.heartbeat(SystemComponent::PluginHost);
            watchdog.heartbeat(SystemComponent::SafetySystem);
            watchdog.heartbeat(SystemComponent::DeviceManager);
        });
    });
}

fn bench_quarantine_check(c: &mut Criterion) {
    let watchdog = WatchdogSystem::default();

    // Pre-register plugin
    watchdog.register_plugin("test_plugin");

    c.bench_function("is_plugin_quarantined_not_quarantined", |b| {
        b.iter(|| watchdog.is_plugin_quarantined(black_box("test_plugin")));
    });

    // Quarantine the plugin
    for _ in 0..5 {
        watchdog.record_plugin_execution("test_plugin", 200);
    }

    c.bench_function("is_plugin_quarantined_quarantined", |b| {
        b.iter(|| watchdog.is_plugin_quarantined(black_box("test_plugin")));
    });
}

fn bench_get_stats(c: &mut Criterion) {
    let watchdog = WatchdogSystem::default();

    // Pre-register and record some executions
    for i in 0..100 {
        let plugin_id = format!("plugin_{}", i);
        watchdog.register_plugin(&plugin_id);
        for _ in 0..10 {
            watchdog.record_plugin_execution(&plugin_id, 50);
        }
    }

    c.bench_function("get_plugin_stats_single", |b| {
        b.iter(|| watchdog.get_plugin_stats(black_box("plugin_50")));
    });

    c.bench_function("get_all_plugin_stats", |b| {
        b.iter(|| watchdog.get_all_plugin_stats());
    });

    c.bench_function("get_plugin_performance_metrics", |b| {
        b.iter(|| watchdog.get_plugin_performance_metrics());
    });
}

fn bench_health_checks(c: &mut Criterion) {
    let config = WatchdogConfig::builder()
        .health_check_interval(Duration::from_millis(0))
        .build()
        .unwrap();
    let watchdog = WatchdogSystem::new(config);

    // Send heartbeats
    for component in SystemComponent::all() {
        watchdog.heartbeat(component);
    }

    c.bench_function("perform_health_checks", |b| {
        b.iter(|| watchdog.perform_health_checks());
    });
}

fn bench_concurrent_access(c: &mut Criterion) {
    let watchdog = std::sync::Arc::new(WatchdogSystem::default());
    let watchdog_clone = watchdog.clone();

    c.bench_function("concurrent_heartbeat_and_record", |b| {
        b.iter(|| {
            watchdog_clone.heartbeat(SystemComponent::RtThread);
            watchdog_clone.record_plugin_execution("test_plugin", 50);
        });
    });
}

fn bench_plugin_registration(c: &mut Criterion) {
    c.bench_function("register_plugin", |b| {
        let watchdog = WatchdogSystem::default();
        let mut counter = 0u64;
        b.iter(|| {
            let plugin_id = format!("plugin_{}", counter);
            counter = counter.wrapping_add(1);
            watchdog.register_plugin(black_box(&plugin_id));
        });
    });
}

fn bench_many_plugins(c: &mut Criterion) {
    let mut group = c.benchmark_group("many_plugins");

    let sizes: [usize; 4] = [10, 50, 100, 500];
    for &size in &sizes {
        group.bench_with_input(
            BenchmarkId::new("record_execution", size),
            &size,
            |b: &mut criterion::Bencher, &size| {
                let watchdog = WatchdogSystem::default();
                for i in 0..size {
                    let plugin_id = format!("plugin_{}", i);
                    watchdog.register_plugin(&plugin_id);
                }

                b.iter(|| {
                    for i in 0..size {
                        let plugin_id = format!("plugin_{}", i);
                        watchdog.record_plugin_execution(&plugin_id, 50);
                    }
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_record_plugin_execution,
    bench_heartbeat,
    bench_quarantine_check,
    bench_get_stats,
    bench_health_checks,
    bench_concurrent_access,
    bench_plugin_registration,
    bench_many_plugins,
);

criterion_main!(benches);

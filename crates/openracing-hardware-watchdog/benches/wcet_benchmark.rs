//! WCET benchmarks for hardware watchdog operations.

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use openracing_hardware_watchdog::prelude::*;

fn bench_feed(c: &mut Criterion) {
    let mut group = c.benchmark_group("feed");

    group.bench_function("feed_armed", |b| {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();
        watchdog.arm().expect("Arm should succeed");
        b.iter(|| black_box(watchdog.feed()));
    });

    group.bench_function("feed_disarmed", |b| {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();
        b.iter(|| black_box(watchdog.feed()));
    });

    group.finish();
}

fn bench_status(c: &mut Criterion) {
    let mut group = c.benchmark_group("status");

    let watchdog = SoftwareWatchdog::with_default_timeout();

    group.bench_function("status_check", |b| {
        b.iter(|| black_box(watchdog.status()));
    });

    group.bench_function("is_armed", |b| {
        b.iter(|| black_box(watchdog.is_armed()));
    });

    group.bench_function("has_timed_out", |b| {
        b.iter(|| black_box(watchdog.has_timed_out()));
    });

    group.finish();
}

fn bench_state_transitions(c: &mut Criterion) {
    let mut group = c.benchmark_group("transitions");

    group.bench_function("arm", |b| {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();
        b.iter(|| {
            watchdog.reset();
            black_box(watchdog.arm())
        });
    });

    group.bench_function("disarm", |b| {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();
        b.iter(|| {
            watchdog.arm().expect("Arm should succeed");
            black_box(watchdog.disarm())
        });
    });

    group.bench_function("reset", |b| {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();
        watchdog.arm().expect("Arm should succeed");
        b.iter(|| {
            watchdog.reset();
            black_box(())
        });
    });

    group.finish();
}

fn bench_metrics(c: &mut Criterion) {
    let mut group = c.benchmark_group("metrics");

    let watchdog = SoftwareWatchdog::with_default_timeout();

    group.bench_function("get_metrics", |b| {
        b.iter(|| black_box(watchdog.metrics()));
    });

    group.finish();
}

fn bench_combined_workflow(c: &mut Criterion) {
    let mut group = c.benchmark_group("workflow");

    group.bench_function("typical_tick", |b| {
        let mut watchdog = SoftwareWatchdog::with_default_timeout();
        watchdog.arm().expect("Arm should succeed");
        b.iter(|| {
            let status = watchdog.status();
            if status == WatchdogStatus::Armed {
                let _ = watchdog.feed();
            }
            black_box(status)
        });
    });

    group.bench_function("full_lifecycle", |b| {
        b.iter(|| {
            let mut watchdog = SoftwareWatchdog::with_default_timeout();
            watchdog.arm().expect("Arm should succeed");
            for _ in 0..10 {
                let _ = watchdog.feed();
            }
            watchdog.disarm().expect("Disarm should succeed");
            watchdog.reset();
            black_box(())
        });
    });

    group.finish();
}

fn bench_atomic_state(c: &mut Criterion) {
    let mut group = c.benchmark_group("atomic_state");

    let state = WatchdogState::new();

    group.bench_function("status_load", |b| {
        b.iter(|| black_box(state.status()));
    });

    group.bench_function("arm_count_load", |b| {
        b.iter(|| black_box(state.arm_count()));
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_feed,
    bench_status,
    bench_state_transitions,
    bench_metrics,
    bench_combined_workflow,
    bench_atomic_state,
);

criterion_main!(benches);

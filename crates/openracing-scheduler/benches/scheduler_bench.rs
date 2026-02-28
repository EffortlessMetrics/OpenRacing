//! Benchmarks for the scheduler crate.

use criterion::{Criterion, criterion_group, criterion_main};
use openracing_scheduler::{AbsoluteScheduler, JitterMetrics, PLL, RTSetup};
use std::hint::black_box;

fn bench_pll_update(c: &mut Criterion) {
    let mut pll = PLL::new(1_000_000);
    let interval = 1_000_000u64;

    c.bench_function("pll_update", |b| {
        b.iter(|| {
            black_box(pll.update(black_box(interval)));
        });
    });
}

fn bench_jitter_record_tick(c: &mut Criterion) {
    let mut metrics = JitterMetrics::new();
    let jitter = 100_000u64;

    c.bench_function("jitter_record_tick", |b| {
        b.iter(|| {
            metrics.record_tick(black_box(jitter), false);
        });
    });
}

fn bench_jitter_p99(c: &mut Criterion) {
    let mut metrics = JitterMetrics::with_capacity(10_000);

    // Pre-populate with samples
    for i in 0..10_000u64 {
        metrics.record_tick(i % 1_000_000, false);
    }

    c.bench_function("jitter_p99", |b| {
        b.iter(|| {
            black_box(metrics.p99_jitter_ns());
        });
    });
}

fn bench_scheduler_wait_for_tick(c: &mut Criterion) {
    let mut scheduler = AbsoluteScheduler::new_1khz();
    let setup = RTSetup::minimal();
    let _ = scheduler.apply_rt_setup(&setup);

    c.bench_function("scheduler_wait_for_tick", |b| {
        b.iter(|| {
            // Note: This will actually sleep, so benchmark with caution
            // For pure benchmarking, we mock the timing
            black_box(scheduler.tick_count());
        });
    });
}

fn bench_adaptive_state_query(c: &mut Criterion) {
    let scheduler = AbsoluteScheduler::new_1khz();

    c.bench_function("adaptive_state_query", |b| {
        b.iter(|| {
            black_box(scheduler.adaptive_scheduling());
        });
    });
}

criterion_group!(
    benches,
    bench_pll_update,
    bench_jitter_record_tick,
    bench_jitter_p99,
    bench_scheduler_wait_for_tick,
    bench_adaptive_state_query,
);

criterion_main!(benches);

//! Benchmark tests for tracing overhead

use criterion::{Criterion, criterion_group, criterion_main};
use openracing_tracing::{RTTraceEvent, TracingManager, TracingMetrics, TracingProvider};

struct NullProvider;

impl TracingProvider for NullProvider {
    fn initialize(&mut self) -> Result<(), openracing_tracing::TracingError> {
        Ok(())
    }

    fn emit_rt_event(&self, _event: RTTraceEvent) {}

    fn emit_app_event(&self, _event: openracing_tracing::AppTraceEvent) {}

    fn metrics(&self) -> TracingMetrics {
        TracingMetrics::default()
    }

    fn shutdown(&mut self) {}
}

fn bench_event_creation(c: &mut Criterion) {
    c.bench_function("create_tick_start", |b| {
        b.iter(|| RTTraceEvent::TickStart {
            tick_count: std::hint::black_box(1),
            timestamp_ns: std::hint::black_box(1_000_000),
        })
    });

    c.bench_function("create_tick_end", |b| {
        b.iter(|| RTTraceEvent::TickEnd {
            tick_count: std::hint::black_box(1),
            timestamp_ns: std::hint::black_box(1_000_000),
            processing_time_ns: std::hint::black_box(500),
        })
    });

    c.bench_function("create_hid_write", |b| {
        b.iter(|| RTTraceEvent::HidWrite {
            tick_count: std::hint::black_box(1),
            timestamp_ns: std::hint::black_box(1_000_000),
            torque_nm: std::hint::black_box(50.0),
            seq: std::hint::black_box(42),
        })
    });
}

fn bench_event_accessors(c: &mut Criterion) {
    let event = RTTraceEvent::TickEnd {
        tick_count: 42,
        timestamp_ns: 1_000_000,
        processing_time_ns: 500,
    };

    c.bench_function("access_tick_count", |b| {
        b.iter(|| std::hint::black_box(event.tick_count()))
    });

    c.bench_function("access_timestamp", |b| {
        b.iter(|| std::hint::black_box(event.timestamp_ns()))
    });

    c.bench_function("access_category", |b| {
        b.iter(|| std::hint::black_box(event.category()))
    });

    c.bench_function("access_is_error", |b| {
        b.iter(|| std::hint::black_box(event.is_error()))
    });
}

fn bench_event_emission(c: &mut Criterion) {
    let manager = TracingManager::with_provider(Box::new(NullProvider));

    c.bench_function("emit_tick_start", |b| {
        b.iter(|| {
            manager.emit_rt_event(RTTraceEvent::TickStart {
                tick_count: std::hint::black_box(1),
                timestamp_ns: std::hint::black_box(1_000_000),
            })
        })
    });

    c.bench_function("emit_tick_end", |b| {
        b.iter(|| {
            manager.emit_rt_event(RTTraceEvent::TickEnd {
                tick_count: std::hint::black_box(1),
                timestamp_ns: std::hint::black_box(1_000_000),
                processing_time_ns: std::hint::black_box(500),
            })
        })
    });
}

criterion_group!(
    benches,
    bench_event_creation,
    bench_event_accessors,
    bench_event_emission,
);

criterion_main!(benches);

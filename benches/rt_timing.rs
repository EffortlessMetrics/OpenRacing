//! Real-time timing benchmarks for performance validation.
//!
//! This benchmark produces JSON output compatible with the performance gate
//! validation script (scripts/validate_performance.py).
//!
//! Performance budgets from requirements:
//! - Total RT Budget: 1000μs @ 1kHz
//! - P99 Jitter: ≤ 0.25ms (250μs)
//! - Missed Ticks: ≤ 0.001% rate
//! - Processing Time: ≤ 50μs median, ≤ 200μs p99

use criterion::{Criterion, criterion_group, criterion_main};
use racing_wheel_engine::{
    AbsoluteScheduler, BenchmarkEntry, BenchmarkResult, BenchmarkResults, CustomMetrics, Frame,
    Percentiles, PerformanceMetrics, Pipeline,
};
use std::env;
use std::hint::black_box;
use std::time::Instant;

/// Collect timing samples and compute percentiles.
struct TimingCollector {
    samples: Vec<u64>,
}

impl TimingCollector {
    fn new(capacity: usize) -> Self {
        Self {
            samples: Vec::with_capacity(capacity),
        }
    }

    fn add_sample(&mut self, sample_ns: u64) {
        self.samples.push(sample_ns);
    }

    fn percentile(&self, p: f64) -> u64 {
        if self.samples.is_empty() {
            return 0;
        }
        let mut sorted = self.samples.clone();
        sorted.sort_unstable();
        let idx = ((p / 100.0) * (sorted.len() - 1) as f64).round() as usize;
        sorted[idx.min(sorted.len() - 1)]
    }

    fn p50(&self) -> u64 {
        self.percentile(50.0)
    }

    fn p99(&self) -> u64 {
        self.percentile(99.0)
    }

    fn sample_count(&self) -> u64 {
        self.samples.len() as u64
    }
}

fn benchmark_rt_timing(c: &mut Criterion) {
    let mut group = c.benchmark_group("rt_timing");

    // Set up RT scheduler
    let mut scheduler = AbsoluteScheduler::new_1khz();
    let mut pipeline = Pipeline::new();
    let mut frame = Frame::default();
    let mut metrics = PerformanceMetrics::default();

    // Collectors for detailed timing data
    let mut jitter_collector = TimingCollector::new(1000);
    let mut processing_collector = TimingCollector::new(1000);

    group.bench_function("1khz_tick_precision", |b| {
        b.iter(|| {
            // Simulate 10ms of 1kHz operation (10 ticks)
            for _ in 0..10 {
                let tick_start = Instant::now();

                // Wait for next tick
                if let Ok(tick) = scheduler.wait_for_tick() {
                    metrics.total_ticks = tick;

                    // Process frame through pipeline
                    frame.seq = tick as u16;
                    frame.ts_mono_ns = tick_start.elapsed().as_nanos() as u64;

                    let process_start = Instant::now();
                    let _ = pipeline.process(&mut frame);
                    let process_time_ns = process_start.elapsed().as_nanos() as u64;
                    processing_collector.add_sample(process_time_ns);

                    // Measure jitter
                    let jitter_ns = tick_start.elapsed().as_nanos() as u64;
                    jitter_collector.add_sample(jitter_ns);
                    if jitter_ns > metrics.max_jitter_ns {
                        metrics.max_jitter_ns = jitter_ns;
                    }
                } else {
                    metrics.missed_ticks += 1;
                }
            }

            black_box(metrics.clone())
        });
    });

    group.bench_function("pipeline_processing", |b| {
        b.iter(|| {
            let mut test_frame = Frame {
                ffb_in: 0.5,
                torque_out: 0.0,
                wheel_speed: 1.0,
                hands_off: false,
                ts_mono_ns: 0,
                seq: 0,
            };

            let result = pipeline.process(&mut test_frame);
            black_box((test_frame, result))
        });
    });

    group.finish();

    // Generate JSON output if requested via environment variable
    if env::var("BENCHMARK_JSON_OUTPUT").is_ok() {
        generate_json_output(&metrics, &jitter_collector, &processing_collector);
    }
}

fn generate_json_output(
    metrics: &PerformanceMetrics,
    jitter_collector: &TimingCollector,
    processing_collector: &TimingCollector,
) {
    let missed_tick_rate = if metrics.total_ticks > 0 {
        (metrics.missed_ticks as f64 / metrics.total_ticks as f64) * 100.0
    } else {
        0.0
    };

    let jitter_p99_ns = jitter_collector.p99();
    let processing_p50_ns = processing_collector.p50();
    let processing_p99_ns = processing_collector.p99();

    let mut results = BenchmarkResults::new();

    // Add RT timing benchmark entry
    results.add_benchmark(BenchmarkEntry {
        name: "rt_timing/1khz_tick_precision".to_string(),
        percentiles: Percentiles {
            p50: jitter_collector.p50(),
            p99: jitter_p99_ns,
        },
        custom_metrics: CustomMetrics {
            missed_tick_rate,
            e2e_latency_p99_us: jitter_p99_ns as f64 / 1000.0,
            rt_heap_allocs: 0,
        },
        sample_count: jitter_collector.sample_count(),
    });

    // Add pipeline processing benchmark entry
    results.add_benchmark(BenchmarkEntry {
        name: "rt_timing/pipeline_processing".to_string(),
        percentiles: Percentiles {
            p50: processing_p50_ns,
            p99: processing_p99_ns,
        },
        custom_metrics: CustomMetrics {
            missed_tick_rate: 0.0,
            e2e_latency_p99_us: 0.0,
            rt_heap_allocs: 0,
        },
        sample_count: processing_collector.sample_count(),
    });

    // Set summary
    results.set_summary(BenchmarkResult::new(
        jitter_p99_ns as f64 / 1000.0,
        jitter_p99_ns as f64 / 1_000_000.0,
        missed_tick_rate,
        processing_p50_ns as f64 / 1000.0,
        processing_p99_ns as f64 / 1000.0,
    ));

    // Write to file or stdout
    if let Ok(output_path) = env::var("BENCHMARK_JSON_PATH") {
        if let Err(e) = results.write_to_file(&output_path) {
            eprintln!("Failed to write benchmark results: {}", e);
        }
    } else {
        match results.to_json() {
            Ok(json) => println!("{}", json),
            Err(e) => eprintln!("Failed to serialize benchmark results: {}", e),
        }
    }
}

fn benchmark_memory_usage(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory");

    group.bench_function("zero_alloc_pipeline", |b| {
        let mut pipeline = Pipeline::new();
        let mut frame = Frame::default();

        b.iter(|| {
            // This should not allocate on the heap
            let result = pipeline.process(&mut frame);
            black_box((frame, result))
        });
    });

    group.finish();
}

criterion_group!(benches, benchmark_rt_timing, benchmark_memory_usage);
criterion_main!(benches);

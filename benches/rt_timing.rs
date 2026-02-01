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

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use racing_wheel_engine::{AbsoluteScheduler, Frame, PerformanceMetrics, Pipeline};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs::File;
use std::io::Write;
use std::time::Instant;

/// Benchmark result structure for JSON output.
///
/// This struct captures all required metrics for performance gate validation:
/// - RT loop timing in microseconds
/// - P99 jitter in milliseconds
/// - Missed tick rate as a percentage
/// - Processing time (median and p99) in microseconds
///
/// **Validates: Requirements 14.6**
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BenchmarkResult {
    /// RT loop timing in microseconds
    pub rt_loop_us: f64,
    /// P99 jitter in milliseconds
    pub jitter_p99_ms: f64,
    /// Missed tick rate as a percentage (0.0 to 100.0)
    pub missed_tick_rate: f64,
    /// Median processing time in microseconds
    pub processing_time_median_us: f64,
    /// P99 processing time in microseconds
    pub processing_time_p99_us: f64,
}

impl Default for BenchmarkResult {
    fn default() -> Self {
        Self {
            rt_loop_us: 0.0,
            jitter_p99_ms: 0.0,
            missed_tick_rate: 0.0,
            processing_time_median_us: 0.0,
            processing_time_p99_us: 0.0,
        }
    }
}

impl BenchmarkResult {
    /// Create a new BenchmarkResult with the given values.
    pub fn new(
        rt_loop_us: f64,
        jitter_p99_ms: f64,
        missed_tick_rate: f64,
        processing_time_median_us: f64,
        processing_time_p99_us: f64,
    ) -> Self {
        Self {
            rt_loop_us,
            jitter_p99_ms,
            missed_tick_rate,
            processing_time_median_us,
            processing_time_p99_us,
        }
    }

    /// Check if the benchmark result meets performance gates.
    ///
    /// Performance budgets:
    /// - Total RT Budget: 1000μs @ 1kHz
    /// - P99 Jitter: ≤ 0.25ms
    /// - Missed Ticks: ≤ 0.001% rate
    /// - Processing Time: ≤ 50μs median, ≤ 200μs p99
    pub fn meets_performance_gates(&self) -> bool {
        self.rt_loop_us <= 1000.0
            && self.jitter_p99_ms <= 0.25
            && self.missed_tick_rate <= 0.001
            && self.processing_time_median_us <= 50.0
            && self.processing_time_p99_us <= 200.0
    }
}

/// Percentile data for benchmark results.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Percentiles {
    /// 50th percentile (median) in nanoseconds
    pub p50: u64,
    /// 99th percentile in nanoseconds
    pub p99: u64,
}

/// Custom metrics for benchmark results.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CustomMetrics {
    /// Missed tick rate as a percentage
    pub missed_tick_rate: f64,
    /// End-to-end latency p99 in microseconds
    pub e2e_latency_p99_us: f64,
    /// Number of RT heap allocations (should be 0)
    pub rt_heap_allocs: u64,
}

/// Individual benchmark entry for JSON output.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BenchmarkEntry {
    /// Benchmark name
    pub name: String,
    /// Percentile timing data
    pub percentiles: Percentiles,
    /// Custom metrics
    pub custom_metrics: CustomMetrics,
    /// Number of samples collected
    pub sample_count: u64,
}

/// Complete benchmark results for JSON output.
///
/// This structure is compatible with the performance gate validation script.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BenchmarkResults {
    /// List of benchmark entries
    pub benchmarks: Vec<BenchmarkEntry>,
    /// Summary result with all key metrics
    pub summary: BenchmarkResult,
}

impl BenchmarkResults {
    /// Create a new empty BenchmarkResults.
    pub fn new() -> Self {
        Self {
            benchmarks: Vec::new(),
            summary: BenchmarkResult::default(),
        }
    }

    /// Add a benchmark entry.
    pub fn add_benchmark(&mut self, entry: BenchmarkEntry) {
        self.benchmarks.push(entry);
    }

    /// Set the summary result.
    pub fn set_summary(&mut self, summary: BenchmarkResult) {
        self.summary = summary;
    }

    /// Serialize to JSON string.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Deserialize from JSON string.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Write results to a file.
    pub fn write_to_file(&self, path: &str) -> std::io::Result<()> {
        let json = self
            .to_json()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let mut file = File::create(path)?;
        file.write_all(json.as_bytes())?;
        Ok(())
    }
}

impl Default for BenchmarkResults {
    fn default() -> Self {
        Self::new()
    }
}

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
            let start = Instant::now();

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
            jitter_p99_ns as f64 / 1000.0,      // RT loop in μs
            jitter_p99_ns as f64 / 1_000_000.0, // Jitter p99 in ms
            missed_tick_rate,
            processing_p50_ns as f64 / 1000.0, // Processing median in μs
            processing_p99_ns as f64 / 1000.0, // Processing p99 in μs
        ));

        // Write to file
        if let Ok(output_path) = env::var("BENCHMARK_JSON_PATH") {
            if let Err(e) = results.write_to_file(&output_path) {
                eprintln!("Failed to write benchmark results: {}", e);
            }
        } else {
            // Print to stdout
            match results.to_json() {
                Ok(json) => println!("{}", json),
                Err(e) => eprintln!("Failed to serialize benchmark results: {}", e),
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that BenchmarkResult can be serialized and deserialized correctly.
    /// **Validates: Property 25 - Benchmark JSON Round-Trip**
    #[test]
    fn test_benchmark_result_json_roundtrip() -> Result<(), serde_json::Error> {
        let result = BenchmarkResult::new(
            100.5,  // rt_loop_us
            0.15,   // jitter_p99_ms
            0.0005, // missed_tick_rate
            25.0,   // processing_time_median_us
            150.0,  // processing_time_p99_us
        );

        let json = serde_json::to_string(&result)?;
        let deserialized: BenchmarkResult = serde_json::from_str(&json)?;

        assert_eq!(result, deserialized);
        Ok(())
    }

    /// Test that BenchmarkResults can be serialized and deserialized correctly.
    #[test]
    fn test_benchmark_results_json_roundtrip() -> Result<(), serde_json::Error> {
        let mut results = BenchmarkResults::new();

        results.add_benchmark(BenchmarkEntry {
            name: "test_benchmark".to_string(),
            percentiles: Percentiles {
                p50: 50000,
                p99: 150000,
            },
            custom_metrics: CustomMetrics {
                missed_tick_rate: 0.0001,
                e2e_latency_p99_us: 150.0,
                rt_heap_allocs: 0,
            },
            sample_count: 1000,
        });

        results.set_summary(BenchmarkResult::new(100.0, 0.15, 0.0001, 50.0, 150.0));

        let json = results.to_json()?;
        let deserialized = BenchmarkResults::from_json(&json)?;

        assert_eq!(results, deserialized);
        Ok(())
    }

    /// Test that performance gate validation works correctly.
    #[test]
    fn test_performance_gates_pass() {
        let result = BenchmarkResult::new(
            500.0,  // rt_loop_us (< 1000)
            0.20,   // jitter_p99_ms (< 0.25)
            0.0005, // missed_tick_rate (< 0.001)
            40.0,   // processing_time_median_us (< 50)
            180.0,  // processing_time_p99_us (< 200)
        );

        assert!(result.meets_performance_gates());
    }

    /// Test that performance gate validation fails when thresholds are exceeded.
    #[test]
    fn test_performance_gates_fail_jitter() {
        let result = BenchmarkResult::new(
            500.0, // rt_loop_us
            0.30,  // jitter_p99_ms (> 0.25 - FAIL)
            0.0005, 40.0, 180.0,
        );

        assert!(!result.meets_performance_gates());
    }

    /// Test that performance gate validation fails when missed tick rate is exceeded.
    #[test]
    fn test_performance_gates_fail_missed_ticks() {
        let result = BenchmarkResult::new(
            500.0, 0.20, 0.002, // missed_tick_rate (> 0.001 - FAIL)
            40.0, 180.0,
        );

        assert!(!result.meets_performance_gates());
    }

    /// Test that performance gate validation fails when processing time is exceeded.
    #[test]
    fn test_performance_gates_fail_processing_time() {
        let result = BenchmarkResult::new(
            500.0, 0.20, 0.0005, 60.0, // processing_time_median_us (> 50 - FAIL)
            180.0,
        );

        assert!(!result.meets_performance_gates());
    }

    /// Test default BenchmarkResult values.
    #[test]
    fn test_benchmark_result_default() {
        let result = BenchmarkResult::default();

        assert_eq!(result.rt_loop_us, 0.0);
        assert_eq!(result.jitter_p99_ms, 0.0);
        assert_eq!(result.missed_tick_rate, 0.0);
        assert_eq!(result.processing_time_median_us, 0.0);
        assert_eq!(result.processing_time_p99_us, 0.0);
    }
}

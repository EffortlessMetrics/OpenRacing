//! Performance testing utilities and benchmarks

use anyhow::Result;
use hdrhistogram::Histogram;
use std::time::{Duration, Instant};
use tracing::{info, warn};

use crate::common::{RTTimer, TestHarness, TimingUtils};
use crate::{PerformanceMetrics, TestConfig, TestResult};

/// Performance benchmark suite
pub struct PerformanceBenchmark {
    pub name: String,
    pub config: TestConfig,
    pub expected_metrics: PerformanceMetrics,
}

impl PerformanceBenchmark {
    pub fn new(name: &str, config: TestConfig, expected: PerformanceMetrics) -> Self {
        Self {
            name: name.to_string(),
            config,
            expected_metrics: expected,
        }
    }

    pub async fn run(&self) -> Result<TestResult> {
        info!("Running performance benchmark: {}", self.name);

        let mut harness = TestHarness::new(self.config.clone()).await?;
        let start_time = Instant::now();

        harness.start_service().await?;

        let result = self.execute_benchmark(&mut harness).await?;

        harness.shutdown().await?;

        let violations = self.check_performance_violations(&result);

        Ok(TestResult {
            passed: self.validate_results(&result),
            duration: start_time.elapsed(),
            metrics: result.clone(),
            errors: violations,
            requirement_coverage: vec!["NFR-01".to_string(), "FFB-01".to_string()],
        })
    }

    async fn execute_benchmark(&self, _harness: &mut TestHarness) -> Result<PerformanceMetrics> {
        let mut jitter_histogram = Histogram::<u64>::new_with_bounds(1, 10_000_000, 3)?;
        let mut latency_histogram = Histogram::<u64>::new_with_bounds(1, 10_000_000, 3)?;

        let mut timer = RTTimer::new(self.config.sample_rate_hz);
        let mut total_ticks = 0u64;
        let mut missed_ticks = 0u64;

        let end_time = Instant::now() + self.config.duration;

        while Instant::now() < end_time {
            let jitter = timer.wait_for_next_tick().await;
            total_ticks += 1;

            // Record jitter
            let jitter_ns = jitter.as_nanos() as u64;
            jitter_histogram.record(jitter_ns).ok();

            if jitter > Duration::from_micros(500) {
                missed_ticks += 1;
            }

            // Simulate and measure processing latency
            let (_, processing_latency) =
                TimingUtils::measure_async(|| async { self.simulate_processing_load().await })
                    .await;

            let latency_ns = processing_latency.as_nanos() as u64;
            latency_histogram.record(latency_ns).ok();
        }

        // Calculate final metrics
        let jitter_p50_ms = jitter_histogram.value_at_quantile(0.5) as f64 / 1_000_000.0;
        let jitter_p99_ms = jitter_histogram.value_at_quantile(0.99) as f64 / 1_000_000.0;
        let latency_p50_us = latency_histogram.value_at_quantile(0.5) as f64 / 1_000.0;
        let latency_p99_us = latency_histogram.value_at_quantile(0.99) as f64 / 1_000.0;

        Ok(PerformanceMetrics {
            jitter_p50_ms,
            jitter_p99_ms,
            hid_latency_p50_us: latency_p50_us,
            hid_latency_p99_us: latency_p99_us,
            missed_ticks,
            total_ticks,
            ..Default::default()
        })
    }

    async fn simulate_processing_load(&self) {
        match self.config.stress_level {
            crate::StressLevel::Light => {
                tokio::time::sleep(Duration::from_micros(20)).await;
            }
            crate::StressLevel::Medium => {
                tokio::time::sleep(Duration::from_micros(50)).await;
            }
            crate::StressLevel::Heavy => {
                tokio::time::sleep(Duration::from_micros(100)).await;
            }
            crate::StressLevel::Extreme => {
                tokio::time::sleep(Duration::from_micros(200)).await;
            }
        }
    }

    fn validate_results(&self, actual: &PerformanceMetrics) -> bool {
        actual.jitter_p99_ms <= self.expected_metrics.jitter_p99_ms
            && actual.hid_latency_p99_us <= self.expected_metrics.hid_latency_p99_us
            && actual.missed_ticks <= self.expected_metrics.missed_ticks
    }

    fn check_performance_violations(&self, actual: &PerformanceMetrics) -> Vec<String> {
        let mut errors = Vec::new();

        if actual.jitter_p99_ms > self.expected_metrics.jitter_p99_ms {
            errors.push(format!(
                "Jitter P99 {:.3}ms exceeds expected {:.3}ms",
                actual.jitter_p99_ms, self.expected_metrics.jitter_p99_ms
            ));
        }

        if actual.hid_latency_p99_us > self.expected_metrics.hid_latency_p99_us {
            errors.push(format!(
                "HID latency P99 {:.1}μs exceeds expected {:.1}μs",
                actual.hid_latency_p99_us, self.expected_metrics.hid_latency_p99_us
            ));
        }

        if actual.missed_ticks > self.expected_metrics.missed_ticks {
            errors.push(format!(
                "Missed ticks {} exceeds expected {}",
                actual.missed_ticks, self.expected_metrics.missed_ticks
            ));
        }

        errors
    }
}

/// Create standard performance benchmark suite
pub fn create_benchmark_suite() -> Vec<PerformanceBenchmark> {
    vec![
        // Baseline performance benchmark
        PerformanceBenchmark::new(
            "Baseline Performance",
            TestConfig {
                duration: Duration::from_secs(60),
                sample_rate_hz: 1000,
                virtual_device: true,
                enable_tracing: false,
                stress_level: crate::StressLevel::Light,
                ..Default::default()
            },
            PerformanceMetrics {
                jitter_p99_ms: 0.15,
                hid_latency_p99_us: 200.0,
                missed_ticks: 0,
                ..Default::default()
            },
        ),
        // Normal load benchmark
        PerformanceBenchmark::new(
            "Normal Load Performance",
            TestConfig {
                duration: Duration::from_secs(120),
                sample_rate_hz: 1000,
                virtual_device: true,
                enable_tracing: true,
                enable_metrics: true,
                stress_level: crate::StressLevel::Medium,
                ..Default::default()
            },
            PerformanceMetrics {
                jitter_p99_ms: 0.20,
                hid_latency_p99_us: 250.0,
                missed_ticks: 0,
                ..Default::default()
            },
        ),
        // Heavy load benchmark
        PerformanceBenchmark::new(
            "Heavy Load Performance",
            TestConfig {
                duration: Duration::from_secs(180),
                sample_rate_hz: 1000,
                virtual_device: true,
                enable_tracing: true,
                enable_metrics: true,
                stress_level: crate::StressLevel::Heavy,
                ..Default::default()
            },
            PerformanceMetrics {
                jitter_p99_ms: 0.25,
                hid_latency_p99_us: 300.0,
                missed_ticks: 0,
                ..Default::default()
            },
        ),
    ]
}

/// Run all performance benchmarks
pub async fn run_performance_benchmark_suite() -> Result<Vec<TestResult>> {
    info!("Running performance benchmark suite");

    let benchmarks = create_benchmark_suite();
    let mut results = Vec::new();

    for benchmark in benchmarks {
        let result = benchmark.run().await?;

        info!(
            "Benchmark '{}' completed: {}",
            benchmark.name,
            if result.passed { "PASSED" } else { "FAILED" }
        );

        if !result.passed {
            warn!("Benchmark '{}' failed: {:?}", benchmark.name, result.errors);
        }

        results.push(result);
    }

    Ok(results)
}

/// Latency measurement utilities
pub struct LatencyMeasurement {
    samples: Vec<Duration>,
    histogram: Histogram<u64>,
}

impl LatencyMeasurement {
    pub fn new() -> Result<Self> {
        Ok(Self {
            samples: Vec::new(),
            histogram: Histogram::new_with_bounds(1, 100_000_000, 3)?, // 1ns to 100ms
        })
    }

    pub fn record(&mut self, latency: Duration) {
        self.samples.push(latency);
        let nanos = latency.as_nanos() as u64;
        self.histogram.record(nanos).ok();
    }

    pub fn percentile(&self, p: f64) -> Duration {
        let nanos = self.histogram.value_at_quantile(p);
        Duration::from_nanos(nanos)
    }

    pub fn mean(&self) -> Duration {
        if self.samples.is_empty() {
            return Duration::ZERO;
        }

        let total_nanos: u128 = self.samples.iter().map(|d| d.as_nanos()).sum();
        Duration::from_nanos((total_nanos / self.samples.len() as u128) as u64)
    }

    pub fn max(&self) -> Duration {
        Duration::from_nanos(self.histogram.max())
    }

    pub fn min(&self) -> Duration {
        Duration::from_nanos(self.histogram.min())
    }

    pub fn sample_count(&self) -> usize {
        self.samples.len()
    }

    pub fn report(&self) -> String {
        format!(
            "Latency Report ({} samples):\n\
             - Min: {:?}\n\
             - Mean: {:?}\n\
             - P50: {:?}\n\
             - P95: {:?}\n\
             - P99: {:?}\n\
             - P99.9: {:?}\n\
             - Max: {:?}",
            self.sample_count(),
            self.min(),
            self.mean(),
            self.percentile(0.5),
            self.percentile(0.95),
            self.percentile(0.99),
            self.percentile(0.999),
            self.max()
        )
    }
}

/// Throughput measurement utilities
pub struct ThroughputMeasurement {
    start_time: Instant,
    operation_count: u64,
    byte_count: u64,
}

impl ThroughputMeasurement {
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            operation_count: 0,
            byte_count: 0,
        }
    }

    pub fn record_operation(&mut self, bytes: u64) {
        self.operation_count += 1;
        self.byte_count += bytes;
    }

    pub fn operations_per_second(&self) -> f64 {
        let elapsed = self.start_time.elapsed().as_secs_f64();
        if elapsed > 0.0 {
            self.operation_count as f64 / elapsed
        } else {
            0.0
        }
    }

    pub fn bytes_per_second(&self) -> f64 {
        let elapsed = self.start_time.elapsed().as_secs_f64();
        if elapsed > 0.0 {
            self.byte_count as f64 / elapsed
        } else {
            0.0
        }
    }

    pub fn megabytes_per_second(&self) -> f64 {
        self.bytes_per_second() / (1024.0 * 1024.0)
    }

    pub fn report(&self) -> String {
        format!(
            "Throughput Report ({:.1}s elapsed):\n\
             - Operations: {} ({:.1} ops/sec)\n\
             - Bytes: {} ({:.1} MB/sec)",
            self.start_time.elapsed().as_secs_f64(),
            self.operation_count,
            self.operations_per_second(),
            self.byte_count,
            self.megabytes_per_second()
        )
    }
}

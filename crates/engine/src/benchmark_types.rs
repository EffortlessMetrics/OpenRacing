//! Benchmark result types for JSON output.
//!
//! This module provides types for serializing benchmark results to JSON format,
//! compatible with the performance gate validation script (scripts/validate_performance.py).
//!
//! **Validates: Requirements 14.6**

use serde::{Deserialize, Serialize};

/// Benchmark result structure for JSON output.
///
/// This struct captures all required metrics for performance gate validation:
/// - RT loop timing in microseconds
/// - P99 jitter in milliseconds
/// - Missed tick rate as a ratio (0.0 to 1.0)
/// - Processing time (median and p99) in microseconds
///
/// **Validates: Requirements 14.6**
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BenchmarkResult {
    /// RT loop timing in microseconds
    pub rt_loop_us: f64,
    /// P99 jitter in milliseconds
    pub jitter_p99_ms: f64,
    /// Missed tick rate as a ratio (0.0 to 1.0)
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
            && self.missed_tick_rate <= 0.00001
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
    /// Missed tick rate as a ratio (0.0 to 1.0)
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
        use std::fs::File;
        use std::io::Write;

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

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that BenchmarkResult can be serialized and deserialized correctly.
    /// Feature: release-roadmap-v1, Property 25: Benchmark JSON Round-Trip
    /// **Validates: Property 25 - Benchmark JSON Round-Trip**
    #[test]
    fn test_benchmark_result_json_roundtrip() -> Result<(), serde_json::Error> {
        let result = BenchmarkResult::new(
            100.5,    // rt_loop_us
            0.15,     // jitter_p99_ms
            0.000005, // missed_tick_rate
            25.0,     // processing_time_median_us
            150.0,    // processing_time_p99_us
        );

        let json = serde_json::to_string(&result)?;
        let deserialized: BenchmarkResult = serde_json::from_str(&json)?;

        assert_eq!(result, deserialized);
        Ok(())
    }

    /// Test that BenchmarkResults can be serialized and deserialized correctly.
    /// Feature: release-roadmap-v1, Property 25: Benchmark JSON Round-Trip
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
                missed_tick_rate: 0.000001,
                e2e_latency_p99_us: 150.0,
                rt_heap_allocs: 0,
            },
            sample_count: 1000,
        });

        results.set_summary(BenchmarkResult::new(100.0, 0.15, 0.000001, 50.0, 150.0));

        let json = results.to_json()?;
        let deserialized = BenchmarkResults::from_json(&json)?;

        assert_eq!(results, deserialized);
        Ok(())
    }

    /// Test that performance gate validation works correctly.
    #[test]
    fn test_performance_gates_pass() {
        let result = BenchmarkResult::new(
            500.0,    // rt_loop_us (< 1000)
            0.20,     // jitter_p99_ms (< 0.25)
            0.000005, // missed_tick_rate (< 0.00001)
            40.0,     // processing_time_median_us (< 50)
            180.0,    // processing_time_p99_us (< 200)
        );

        assert!(result.meets_performance_gates());
    }

    /// Test that performance gate validation fails when jitter threshold is exceeded.
    #[test]
    fn test_performance_gates_fail_jitter() {
        let result = BenchmarkResult::new(
            500.0, // rt_loop_us
            0.30,  // jitter_p99_ms (> 0.25 - FAIL)
            0.000005, 40.0, 180.0,
        );

        assert!(!result.meets_performance_gates());
    }

    /// Test that performance gate validation fails when missed tick rate is exceeded.
    #[test]
    fn test_performance_gates_fail_missed_ticks() {
        let result = BenchmarkResult::new(
            500.0, 0.20, 0.00002, // missed_tick_rate (> 0.00001 - FAIL)
            40.0, 180.0,
        );

        assert!(!result.meets_performance_gates());
    }

    /// Test that performance gate validation fails when processing time is exceeded.
    #[test]
    fn test_performance_gates_fail_processing_time() {
        let result = BenchmarkResult::new(
            500.0, 0.20, 0.000005, 60.0, // processing_time_median_us (> 50 - FAIL)
            180.0,
        );

        assert!(!result.meets_performance_gates());
    }

    /// Test that performance gate validation fails when RT loop time is exceeded.
    #[test]
    fn test_performance_gates_fail_rt_loop() {
        let result = BenchmarkResult::new(
            1100.0, // rt_loop_us (> 1000 - FAIL)
            0.20, 0.000005, 40.0, 180.0,
        );

        assert!(!result.meets_performance_gates());
    }

    /// Test that performance gate validation fails when p99 processing time is exceeded.
    #[test]
    fn test_performance_gates_fail_processing_p99() {
        let result = BenchmarkResult::new(
            500.0, 0.20, 0.000005, 40.0, 250.0, // processing_time_p99_us (> 200 - FAIL)
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

    /// Test default BenchmarkResults values.
    #[test]
    fn test_benchmark_results_default() {
        let results = BenchmarkResults::default();

        assert!(results.benchmarks.is_empty());
        assert_eq!(results.summary, BenchmarkResult::default());
    }
}

/// Property-based tests for benchmark JSON round-trip.
///
/// These tests validate that benchmark results can be serialized to JSON
/// and deserialized back without loss of information.
///
/// **Validates: Requirements 14.6**
#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    /// Maximum relative error allowed for floating-point comparisons after JSON round-trip.
    /// JSON uses decimal representation which can introduce small precision differences.
    const F64_RELATIVE_EPSILON: f64 = 1e-14;

    /// Check if two f64 values are approximately equal within relative tolerance.
    /// Handles special cases like zero and very small numbers.
    fn approx_eq(a: f64, b: f64) -> bool {
        if a == b {
            return true;
        }
        if a == 0.0 || b == 0.0 {
            return (a - b).abs() < F64_RELATIVE_EPSILON;
        }
        let relative_diff = ((a - b) / a.abs().max(b.abs())).abs();
        relative_diff < F64_RELATIVE_EPSILON
    }

    /// Check if two BenchmarkResult instances are approximately equal.
    fn benchmark_result_approx_eq(a: &BenchmarkResult, b: &BenchmarkResult) -> bool {
        approx_eq(a.rt_loop_us, b.rt_loop_us)
            && approx_eq(a.jitter_p99_ms, b.jitter_p99_ms)
            && approx_eq(a.missed_tick_rate, b.missed_tick_rate)
            && approx_eq(a.processing_time_median_us, b.processing_time_median_us)
            && approx_eq(a.processing_time_p99_us, b.processing_time_p99_us)
    }

    /// Check if two CustomMetrics instances are approximately equal.
    fn custom_metrics_approx_eq(a: &CustomMetrics, b: &CustomMetrics) -> bool {
        approx_eq(a.missed_tick_rate, b.missed_tick_rate)
            && approx_eq(a.e2e_latency_p99_us, b.e2e_latency_p99_us)
            && a.rt_heap_allocs == b.rt_heap_allocs
    }

    /// Check if two BenchmarkEntry instances are approximately equal.
    fn benchmark_entry_approx_eq(a: &BenchmarkEntry, b: &BenchmarkEntry) -> bool {
        a.name == b.name
            && a.percentiles == b.percentiles
            && custom_metrics_approx_eq(&a.custom_metrics, &b.custom_metrics)
            && a.sample_count == b.sample_count
    }

    /// Check if two BenchmarkResults instances are approximately equal.
    fn benchmark_results_approx_eq(a: &BenchmarkResults, b: &BenchmarkResults) -> bool {
        if !benchmark_result_approx_eq(&a.summary, &b.summary) {
            return false;
        }
        if a.benchmarks.len() != b.benchmarks.len() {
            return false;
        }
        a.benchmarks
            .iter()
            .zip(b.benchmarks.iter())
            .all(|(x, y)| benchmark_entry_approx_eq(x, y))
    }

    /// Strategy for generating valid f64 values for benchmark metrics.
    ///
    /// We constrain to finite, non-negative values that are reasonable for benchmarks.
    fn benchmark_f64_strategy() -> impl Strategy<Value = f64> {
        // Generate values in a reasonable range for benchmark metrics
        // Avoid NaN, Infinity, and negative values
        (0.0f64..1_000_000.0).prop_filter("Value must be finite", |v| v.is_finite())
    }

    /// Strategy for generating valid u64 values for benchmark metrics.
    fn benchmark_u64_strategy() -> impl Strategy<Value = u64> {
        0u64..1_000_000_000
    }

    /// Strategy for generating valid benchmark names.
    fn benchmark_name_strategy() -> impl Strategy<Value = String> {
        "[a-z][a-z0-9_]{0,30}".prop_map(|s| s.to_string())
    }

    /// Strategy for generating valid BenchmarkResult instances.
    fn benchmark_result_strategy() -> impl Strategy<Value = BenchmarkResult> {
        (
            benchmark_f64_strategy(), // rt_loop_us
            benchmark_f64_strategy(), // jitter_p99_ms
            benchmark_f64_strategy(), // missed_tick_rate
            benchmark_f64_strategy(), // processing_time_median_us
            benchmark_f64_strategy(), // processing_time_p99_us
        )
            .prop_map(
                |(rt_loop_us, jitter_p99_ms, missed_tick_rate, median_us, p99_us)| {
                    BenchmarkResult::new(
                        rt_loop_us,
                        jitter_p99_ms,
                        missed_tick_rate,
                        median_us,
                        p99_us,
                    )
                },
            )
    }

    /// Strategy for generating valid Percentiles instances.
    fn percentiles_strategy() -> impl Strategy<Value = Percentiles> {
        (benchmark_u64_strategy(), benchmark_u64_strategy())
            .prop_map(|(p50, p99)| Percentiles { p50, p99 })
    }

    /// Strategy for generating valid CustomMetrics instances.
    fn custom_metrics_strategy() -> impl Strategy<Value = CustomMetrics> {
        (
            benchmark_f64_strategy(), // missed_tick_rate
            benchmark_f64_strategy(), // e2e_latency_p99_us
            benchmark_u64_strategy(), // rt_heap_allocs
        )
            .prop_map(|(missed_tick_rate, e2e_latency_p99_us, rt_heap_allocs)| {
                CustomMetrics {
                    missed_tick_rate,
                    e2e_latency_p99_us,
                    rt_heap_allocs,
                }
            })
    }

    /// Strategy for generating valid BenchmarkEntry instances.
    fn benchmark_entry_strategy() -> impl Strategy<Value = BenchmarkEntry> {
        (
            benchmark_name_strategy(),
            percentiles_strategy(),
            custom_metrics_strategy(),
            benchmark_u64_strategy(), // sample_count
        )
            .prop_map(
                |(name, percentiles, custom_metrics, sample_count)| BenchmarkEntry {
                    name,
                    percentiles,
                    custom_metrics,
                    sample_count,
                },
            )
    }

    /// Strategy for generating valid BenchmarkResults instances.
    fn benchmark_results_strategy() -> impl Strategy<Value = BenchmarkResults> {
        (
            prop::collection::vec(benchmark_entry_strategy(), 0..10),
            benchmark_result_strategy(),
        )
            .prop_map(|(benchmarks, summary)| BenchmarkResults {
                benchmarks,
                summary,
            })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// Feature: release-roadmap-v1, Property 25: Benchmark JSON Round-Trip
        ///
        /// *For any* benchmark result, serializing to JSON and deserializing
        /// SHALL produce an equivalent result structure.
        ///
        /// **Validates: Requirements 14.6**
        #[test]
        fn prop_benchmark_result_json_roundtrip(result in benchmark_result_strategy()) {
            // Serialize to JSON
            let json_result = serde_json::to_string(&result);
            prop_assert!(
                json_result.is_ok(),
                "Failed to serialize BenchmarkResult to JSON: {:?}",
                json_result.err()
            );

            let json = json_result.map_err(|e| TestCaseError::fail(format!("{}", e)))?;

            // Deserialize from JSON
            let parsed_result: Result<BenchmarkResult, _> = serde_json::from_str(&json);
            prop_assert!(
                parsed_result.is_ok(),
                "Failed to deserialize BenchmarkResult from JSON: {:?}\nJSON:\n{}",
                parsed_result.err(),
                json
            );

            let parsed = parsed_result.map_err(|e| TestCaseError::fail(format!("{}", e)))?;

            // Verify equivalence using approximate equality for floating-point values
            // JSON uses decimal representation which can introduce small precision differences
            prop_assert!(
                benchmark_result_approx_eq(&result, &parsed),
                "BenchmarkResult not approximately equal after JSON roundtrip.\n\
                 Original: {:?}\nParsed: {:?}",
                result,
                parsed
            );
        }

        /// Feature: release-roadmap-v1, Property 25: Benchmark JSON Round-Trip
        ///
        /// *For any* BenchmarkResults collection, serializing to JSON and deserializing
        /// SHALL produce an equivalent result structure.
        ///
        /// **Validates: Requirements 14.6**
        #[test]
        fn prop_benchmark_results_json_roundtrip(results in benchmark_results_strategy()) {
            // Serialize to JSON using the to_json method
            let json_result = results.to_json();
            prop_assert!(
                json_result.is_ok(),
                "Failed to serialize BenchmarkResults to JSON: {:?}",
                json_result.err()
            );

            let json = json_result.map_err(|e| TestCaseError::fail(format!("{}", e)))?;

            // Deserialize from JSON using the from_json method
            let parsed_result = BenchmarkResults::from_json(&json);
            prop_assert!(
                parsed_result.is_ok(),
                "Failed to deserialize BenchmarkResults from JSON: {:?}\nJSON:\n{}",
                parsed_result.err(),
                json
            );

            let parsed = parsed_result.map_err(|e| TestCaseError::fail(format!("{}", e)))?;

            // Verify equivalence using approximate equality for floating-point values
            prop_assert!(
                benchmark_results_approx_eq(&results, &parsed),
                "BenchmarkResults not approximately equal after JSON roundtrip.\n\
                 Original: {:?}\nParsed: {:?}",
                results,
                parsed
            );
        }

        /// Feature: release-roadmap-v1, Property 25: Benchmark JSON Round-Trip
        ///
        /// *For any* BenchmarkEntry, serializing to JSON and deserializing
        /// SHALL produce an equivalent result structure.
        ///
        /// **Validates: Requirements 14.6**
        #[test]
        fn prop_benchmark_entry_json_roundtrip(entry in benchmark_entry_strategy()) {
            // Serialize to JSON
            let json_result = serde_json::to_string(&entry);
            prop_assert!(
                json_result.is_ok(),
                "Failed to serialize BenchmarkEntry to JSON: {:?}",
                json_result.err()
            );

            let json = json_result.map_err(|e| TestCaseError::fail(format!("{}", e)))?;

            // Deserialize from JSON
            let parsed_result: Result<BenchmarkEntry, _> = serde_json::from_str(&json);
            prop_assert!(
                parsed_result.is_ok(),
                "Failed to deserialize BenchmarkEntry from JSON: {:?}\nJSON:\n{}",
                parsed_result.err(),
                json
            );

            let parsed = parsed_result.map_err(|e| TestCaseError::fail(format!("{}", e)))?;

            // Verify equivalence using approximate equality for floating-point values
            prop_assert!(
                benchmark_entry_approx_eq(&entry, &parsed),
                "BenchmarkEntry not approximately equal after JSON roundtrip.\n\
                 Original: {:?}\nParsed: {:?}",
                entry,
                parsed
            );
        }

        /// Feature: release-roadmap-v1, Property 25: Benchmark JSON Round-Trip
        ///
        /// *For any* Percentiles, serializing to JSON and deserializing
        /// SHALL produce an equivalent result structure.
        ///
        /// **Validates: Requirements 14.6**
        #[test]
        fn prop_percentiles_json_roundtrip(percentiles in percentiles_strategy()) {
            // Serialize to JSON
            let json_result = serde_json::to_string(&percentiles);
            prop_assert!(
                json_result.is_ok(),
                "Failed to serialize Percentiles to JSON: {:?}",
                json_result.err()
            );

            let json = json_result.map_err(|e| TestCaseError::fail(format!("{}", e)))?;

            // Deserialize from JSON
            let parsed_result: Result<Percentiles, _> = serde_json::from_str(&json);
            prop_assert!(
                parsed_result.is_ok(),
                "Failed to deserialize Percentiles from JSON: {:?}\nJSON:\n{}",
                parsed_result.err(),
                json
            );

            let parsed = parsed_result.map_err(|e| TestCaseError::fail(format!("{}", e)))?;

            // Percentiles uses u64, so exact equality is expected
            prop_assert_eq!(
                percentiles,
                parsed,
                "Percentiles not equal after JSON roundtrip"
            );
        }

        /// Feature: release-roadmap-v1, Property 25: Benchmark JSON Round-Trip
        ///
        /// *For any* CustomMetrics, serializing to JSON and deserializing
        /// SHALL produce an equivalent result structure.
        ///
        /// **Validates: Requirements 14.6**
        #[test]
        fn prop_custom_metrics_json_roundtrip(metrics in custom_metrics_strategy()) {
            // Serialize to JSON
            let json_result = serde_json::to_string(&metrics);
            prop_assert!(
                json_result.is_ok(),
                "Failed to serialize CustomMetrics to JSON: {:?}",
                json_result.err()
            );

            let json = json_result.map_err(|e| TestCaseError::fail(format!("{}", e)))?;

            // Deserialize from JSON
            let parsed_result: Result<CustomMetrics, _> = serde_json::from_str(&json);
            prop_assert!(
                parsed_result.is_ok(),
                "Failed to deserialize CustomMetrics from JSON: {:?}\nJSON:\n{}",
                parsed_result.err(),
                json
            );

            let parsed = parsed_result.map_err(|e| TestCaseError::fail(format!("{}", e)))?;

            // Verify equivalence using approximate equality for floating-point values
            prop_assert!(
                custom_metrics_approx_eq(&metrics, &parsed),
                "CustomMetrics not approximately equal after JSON roundtrip.\n\
                 Original: {:?}\nParsed: {:?}",
                metrics,
                parsed
            );
        }
    }
}

/// Property-based tests for performance gate validation.
///
/// These tests validate that the performance gate validation correctly identifies
/// violations of: RT loop >1000μs, p99 jitter >0.25ms, missed ticks >0.00001,
/// processing time >50μs median or >200μs p99.
///
/// **Property 24: Performance Gate Validation (Consolidated)**
/// **Validates: Requirements 14.2, 14.3, 14.4, 14.5**
#[cfg(test)]
mod performance_gate_property_tests {
    use super::*;
    use proptest::prelude::*;

    // Performance gate thresholds (from design document and code)
    const RT_LOOP_THRESHOLD_US: f64 = 1000.0;
    const JITTER_P99_THRESHOLD_MS: f64 = 0.25;
    const MISSED_TICK_RATE_THRESHOLD: f64 = 0.00001;
    const PROCESSING_TIME_MEDIAN_THRESHOLD_US: f64 = 50.0;
    const PROCESSING_TIME_P99_THRESHOLD_US: f64 = 200.0;

    /// Strategy for generating values strictly within a threshold (passing).
    /// Uses a small epsilon to ensure we're strictly below the threshold.
    fn within_threshold(max: f64) -> impl Strategy<Value = f64> {
        let epsilon = max * 0.001; // 0.1% below max to ensure we're within
        (0.0f64..=(max - epsilon)).prop_filter("Value must be finite", |v| v.is_finite())
    }

    /// Strategy for generating values strictly exceeding a threshold (failing).
    /// Generates values from just above the threshold to a reasonable upper bound.
    fn exceeds_threshold(threshold: f64) -> impl Strategy<Value = f64> {
        let epsilon = threshold * 0.001; // 0.1% above threshold to ensure we exceed
        let upper_bound = threshold * 10.0; // Reasonable upper bound
        ((threshold + epsilon)..=upper_bound).prop_filter("Value must be finite", |v| v.is_finite())
    }

    /// Strategy for generating BenchmarkResult with ALL metrics within thresholds.
    /// This should always pass performance gates.
    fn passing_benchmark_result_strategy() -> impl Strategy<Value = BenchmarkResult> {
        (
            within_threshold(RT_LOOP_THRESHOLD_US),
            within_threshold(JITTER_P99_THRESHOLD_MS),
            within_threshold(MISSED_TICK_RATE_THRESHOLD),
            within_threshold(PROCESSING_TIME_MEDIAN_THRESHOLD_US),
            within_threshold(PROCESSING_TIME_P99_THRESHOLD_US),
        )
            .prop_map(
                |(rt_loop_us, jitter_p99_ms, missed_tick_rate, median_us, p99_us)| {
                    BenchmarkResult::new(
                        rt_loop_us,
                        jitter_p99_ms,
                        missed_tick_rate,
                        median_us,
                        p99_us,
                    )
                },
            )
    }

    /// Strategy for generating BenchmarkResult with RT loop exceeding threshold.
    fn failing_rt_loop_strategy() -> impl Strategy<Value = BenchmarkResult> {
        (
            exceeds_threshold(RT_LOOP_THRESHOLD_US),
            within_threshold(JITTER_P99_THRESHOLD_MS),
            within_threshold(MISSED_TICK_RATE_THRESHOLD),
            within_threshold(PROCESSING_TIME_MEDIAN_THRESHOLD_US),
            within_threshold(PROCESSING_TIME_P99_THRESHOLD_US),
        )
            .prop_map(
                |(rt_loop_us, jitter_p99_ms, missed_tick_rate, median_us, p99_us)| {
                    BenchmarkResult::new(
                        rt_loop_us,
                        jitter_p99_ms,
                        missed_tick_rate,
                        median_us,
                        p99_us,
                    )
                },
            )
    }

    /// Strategy for generating BenchmarkResult with jitter p99 exceeding threshold.
    fn failing_jitter_strategy() -> impl Strategy<Value = BenchmarkResult> {
        (
            within_threshold(RT_LOOP_THRESHOLD_US),
            exceeds_threshold(JITTER_P99_THRESHOLD_MS),
            within_threshold(MISSED_TICK_RATE_THRESHOLD),
            within_threshold(PROCESSING_TIME_MEDIAN_THRESHOLD_US),
            within_threshold(PROCESSING_TIME_P99_THRESHOLD_US),
        )
            .prop_map(
                |(rt_loop_us, jitter_p99_ms, missed_tick_rate, median_us, p99_us)| {
                    BenchmarkResult::new(
                        rt_loop_us,
                        jitter_p99_ms,
                        missed_tick_rate,
                        median_us,
                        p99_us,
                    )
                },
            )
    }

    /// Strategy for generating BenchmarkResult with missed tick rate exceeding threshold.
    fn failing_missed_ticks_strategy() -> impl Strategy<Value = BenchmarkResult> {
        (
            within_threshold(RT_LOOP_THRESHOLD_US),
            within_threshold(JITTER_P99_THRESHOLD_MS),
            exceeds_threshold(MISSED_TICK_RATE_THRESHOLD),
            within_threshold(PROCESSING_TIME_MEDIAN_THRESHOLD_US),
            within_threshold(PROCESSING_TIME_P99_THRESHOLD_US),
        )
            .prop_map(
                |(rt_loop_us, jitter_p99_ms, missed_tick_rate, median_us, p99_us)| {
                    BenchmarkResult::new(
                        rt_loop_us,
                        jitter_p99_ms,
                        missed_tick_rate,
                        median_us,
                        p99_us,
                    )
                },
            )
    }

    /// Strategy for generating BenchmarkResult with processing time median exceeding threshold.
    fn failing_processing_median_strategy() -> impl Strategy<Value = BenchmarkResult> {
        (
            within_threshold(RT_LOOP_THRESHOLD_US),
            within_threshold(JITTER_P99_THRESHOLD_MS),
            within_threshold(MISSED_TICK_RATE_THRESHOLD),
            exceeds_threshold(PROCESSING_TIME_MEDIAN_THRESHOLD_US),
            within_threshold(PROCESSING_TIME_P99_THRESHOLD_US),
        )
            .prop_map(
                |(rt_loop_us, jitter_p99_ms, missed_tick_rate, median_us, p99_us)| {
                    BenchmarkResult::new(
                        rt_loop_us,
                        jitter_p99_ms,
                        missed_tick_rate,
                        median_us,
                        p99_us,
                    )
                },
            )
    }

    /// Strategy for generating BenchmarkResult with processing time p99 exceeding threshold.
    fn failing_processing_p99_strategy() -> impl Strategy<Value = BenchmarkResult> {
        (
            within_threshold(RT_LOOP_THRESHOLD_US),
            within_threshold(JITTER_P99_THRESHOLD_MS),
            within_threshold(MISSED_TICK_RATE_THRESHOLD),
            within_threshold(PROCESSING_TIME_MEDIAN_THRESHOLD_US),
            exceeds_threshold(PROCESSING_TIME_P99_THRESHOLD_US),
        )
            .prop_map(
                |(rt_loop_us, jitter_p99_ms, missed_tick_rate, median_us, p99_us)| {
                    BenchmarkResult::new(
                        rt_loop_us,
                        jitter_p99_ms,
                        missed_tick_rate,
                        median_us,
                        p99_us,
                    )
                },
            )
    }

    /// Strategy for selecting which metric to violate (0-4).
    fn violation_selector() -> impl Strategy<Value = usize> {
        0usize..5
    }

    /// Strategy for generating BenchmarkResult with at least one metric exceeding threshold.
    /// Randomly selects which metric to violate.
    fn failing_benchmark_result_strategy() -> impl Strategy<Value = BenchmarkResult> {
        violation_selector().prop_flat_map(|selector| match selector {
            0 => failing_rt_loop_strategy().boxed(),
            1 => failing_jitter_strategy().boxed(),
            2 => failing_missed_ticks_strategy().boxed(),
            3 => failing_processing_median_strategy().boxed(),
            4 => failing_processing_p99_strategy().boxed(),
            _ => unreachable!(),
        })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// Feature: release-roadmap-v1, Property 24: Performance Gate Validation (Consolidated)
        ///
        /// *For any* benchmark result set with all metrics within thresholds,
        /// the performance validator SHALL return true (pass).
        ///
        /// **Validates: Requirements 14.2, 14.3, 14.4, 14.5**
        #[test]
        fn prop_all_metrics_within_thresholds_passes(result in passing_benchmark_result_strategy()) {
            prop_assert!(
                result.meets_performance_gates(),
                "BenchmarkResult with all metrics within thresholds should pass.\n\
                 Result: {:?}\n\
                 Thresholds: RT loop ≤{}μs, jitter p99 ≤{}ms, missed ticks ≤{}, \
                 processing median ≤{}μs, processing p99 ≤{}μs",
                result,
                RT_LOOP_THRESHOLD_US,
                JITTER_P99_THRESHOLD_MS,
                MISSED_TICK_RATE_THRESHOLD,
                PROCESSING_TIME_MEDIAN_THRESHOLD_US,
                PROCESSING_TIME_P99_THRESHOLD_US
            );
        }

        /// Feature: release-roadmap-v1, Property 24: Performance Gate Validation (Consolidated)
        ///
        /// *For any* benchmark result set with at least one metric exceeding its threshold,
        /// the performance validator SHALL return false (fail).
        ///
        /// **Validates: Requirements 14.2, 14.3, 14.4, 14.5**
        #[test]
        fn prop_any_metric_exceeding_threshold_fails(result in failing_benchmark_result_strategy()) {
            prop_assert!(
                !result.meets_performance_gates(),
                "BenchmarkResult with at least one metric exceeding threshold should fail.\n\
                 Result: {:?}\n\
                 Thresholds: RT loop ≤{}μs, jitter p99 ≤{}ms, missed ticks ≤{}, \
                 processing median ≤{}μs, processing p99 ≤{}μs",
                result,
                RT_LOOP_THRESHOLD_US,
                JITTER_P99_THRESHOLD_MS,
                MISSED_TICK_RATE_THRESHOLD,
                PROCESSING_TIME_MEDIAN_THRESHOLD_US,
                PROCESSING_TIME_P99_THRESHOLD_US
            );
        }

        /// Feature: release-roadmap-v1, Property 24: Performance Gate Validation (Consolidated)
        ///
        /// *For any* benchmark result with RT loop exceeding 1000μs,
        /// the performance validator SHALL correctly identify the violation.
        ///
        /// **Validates: Requirements 14.3**
        #[test]
        fn prop_rt_loop_threshold_enforced(result in failing_rt_loop_strategy()) {
            prop_assert!(
                !result.meets_performance_gates(),
                "RT loop exceeding {}μs should fail performance gates.\n\
                 RT loop value: {}μs\nResult: {:?}",
                RT_LOOP_THRESHOLD_US,
                result.rt_loop_us,
                result
            );
        }

        /// Feature: release-roadmap-v1, Property 24: Performance Gate Validation (Consolidated)
        ///
        /// *For any* benchmark result with p99 jitter exceeding 0.25ms,
        /// the performance validator SHALL correctly identify the violation.
        ///
        /// **Validates: Requirements 14.3**
        #[test]
        fn prop_jitter_p99_threshold_enforced(result in failing_jitter_strategy()) {
            prop_assert!(
                !result.meets_performance_gates(),
                "P99 jitter exceeding {}ms should fail performance gates.\n\
                 Jitter p99 value: {}ms\nResult: {:?}",
                JITTER_P99_THRESHOLD_MS,
                result.jitter_p99_ms,
                result
            );
        }

        /// Feature: release-roadmap-v1, Property 24: Performance Gate Validation (Consolidated)
        ///
        /// *For any* benchmark result with missed tick rate exceeding 0.00001,
        /// the performance validator SHALL correctly identify the violation.
        ///
        /// **Validates: Requirements 14.3**
        #[test]
        fn prop_missed_tick_rate_threshold_enforced(result in failing_missed_ticks_strategy()) {
            prop_assert!(
                !result.meets_performance_gates(),
                "Missed tick rate exceeding {} should fail performance gates.\n\
                 Missed tick rate value: {}\nResult: {:?}",
                MISSED_TICK_RATE_THRESHOLD,
                result.missed_tick_rate,
                result
            );
        }

        /// Feature: release-roadmap-v1, Property 24: Performance Gate Validation (Consolidated)
        ///
        /// *For any* benchmark result with processing time median exceeding 50μs,
        /// the performance validator SHALL correctly identify the violation.
        ///
        /// **Validates: Requirements 14.4**
        #[test]
        fn prop_processing_time_median_threshold_enforced(result in failing_processing_median_strategy()) {
            prop_assert!(
                !result.meets_performance_gates(),
                "Processing time median exceeding {}μs should fail performance gates.\n\
                 Processing time median value: {}μs\nResult: {:?}",
                PROCESSING_TIME_MEDIAN_THRESHOLD_US,
                result.processing_time_median_us,
                result
            );
        }

        /// Feature: release-roadmap-v1, Property 24: Performance Gate Validation (Consolidated)
        ///
        /// *For any* benchmark result with processing time p99 exceeding 200μs,
        /// the performance validator SHALL correctly identify the violation.
        ///
        /// **Validates: Requirements 14.4**
        #[test]
        fn prop_processing_time_p99_threshold_enforced(result in failing_processing_p99_strategy()) {
            prop_assert!(
                !result.meets_performance_gates(),
                "Processing time p99 exceeding {}μs should fail performance gates.\n\
                 Processing time p99 value: {}μs\nResult: {:?}",
                PROCESSING_TIME_P99_THRESHOLD_US,
                result.processing_time_p99_us,
                result
            );
        }

        /// Feature: release-roadmap-v1, Property 24: Performance Gate Validation (Consolidated)
        ///
        /// *For any* benchmark result at exactly the threshold values,
        /// the performance validator SHALL return true (pass) since thresholds are inclusive (≤).
        ///
        /// **Validates: Requirements 14.2, 14.3, 14.4, 14.5**
        #[test]
        fn prop_exact_threshold_values_pass(_seed in 0u64..1000) {
            // Test with exact threshold values - should pass since thresholds are ≤
            let result = BenchmarkResult::new(
                RT_LOOP_THRESHOLD_US,
                JITTER_P99_THRESHOLD_MS,
                MISSED_TICK_RATE_THRESHOLD,
                PROCESSING_TIME_MEDIAN_THRESHOLD_US,
                PROCESSING_TIME_P99_THRESHOLD_US,
            );

            prop_assert!(
                result.meets_performance_gates(),
                "BenchmarkResult at exact threshold values should pass (thresholds are inclusive ≤).\n\
                 Result: {:?}",
                result
            );
        }
    }
}

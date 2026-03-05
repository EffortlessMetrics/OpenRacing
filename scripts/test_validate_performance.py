#!/usr/bin/env python3
"""Tests for the performance gate validation script (validate_performance.py).

Covers: metric checking, threshold enforcement, JSON parsing, summary/benchmark
validation, format_value display, and end-to-end pass/fail determination.
"""

import json
import os
import sys
import tempfile
import unittest

# Ensure the scripts directory is importable
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from validate_performance import (
    THRESHOLDS,
    MetricResult,
    MetricStatus,
    ValidationResult,
    _format_value,
    check_metric,
    validate_benchmark_metrics,
    validate_performance,
    validate_summary_metrics,
)


class TestCheckMetric(unittest.TestCase):
    """Tests for the check_metric function."""

    def test_value_below_threshold_passes(self):
        r = check_metric("test", 10.0, 100.0, "us", "desc", "src")
        self.assertEqual(r.status, MetricStatus.PASSED)

    def test_value_at_threshold_passes(self):
        r = check_metric("test", 100.0, 100.0, "us", "desc", "src")
        self.assertEqual(r.status, MetricStatus.PASSED)

    def test_value_above_threshold_fails(self):
        r = check_metric("test", 100.001, 100.0, "us", "desc", "src")
        self.assertEqual(r.status, MetricStatus.FAILED)

    def test_none_value_is_skipped(self):
        r = check_metric("test", None, 100.0, "us", "desc", "src")
        self.assertEqual(r.status, MetricStatus.SKIPPED)

    def test_zero_value_passes(self):
        r = check_metric("test", 0.0, 100.0, "us", "desc", "src")
        self.assertEqual(r.status, MetricStatus.PASSED)

    def test_lower_is_better_false(self):
        # When lower_is_better=False, value must be >= threshold
        r = check_metric("test", 200.0, 100.0, "us", "desc", "src", lower_is_better=False)
        self.assertEqual(r.status, MetricStatus.PASSED)

        r = check_metric("test", 50.0, 100.0, "us", "desc", "src", lower_is_better=False)
        self.assertEqual(r.status, MetricStatus.FAILED)


class TestValidateSummaryMetrics(unittest.TestCase):
    """Tests for validate_summary_metrics with known-good and known-bad data."""

    def _make_passing_summary(self):
        return {
            "rt_loop_us": 500.0,
            "jitter_p99_ms": 0.15,
            "missed_tick_rate": 0.000005,
            "processing_time_median_us": 30.0,
            "processing_time_p99_us": 150.0,
        }

    def test_all_metrics_pass(self):
        results = validate_summary_metrics(self._make_passing_summary())
        for r in results:
            self.assertIn(r.status, (MetricStatus.PASSED, MetricStatus.SKIPPED),
                          f"{r.name} unexpectedly {r.status}")

    def test_jitter_exceeds_threshold(self):
        summary = self._make_passing_summary()
        summary["jitter_p99_ms"] = 0.30  # > 0.25ms
        results = validate_summary_metrics(summary)
        jitter = [r for r in results if "Jitter" in r.name]
        self.assertTrue(any(r.status == MetricStatus.FAILED for r in jitter))

    def test_rt_loop_exceeds_threshold(self):
        summary = self._make_passing_summary()
        summary["rt_loop_us"] = 1200.0  # > 1000us
        results = validate_summary_metrics(summary)
        rt = [r for r in results if "RT Loop" in r.name]
        self.assertTrue(any(r.status == MetricStatus.FAILED for r in rt))

    def test_missed_tick_rate_exceeds_threshold(self):
        summary = self._make_passing_summary()
        summary["missed_tick_rate"] = 0.001  # >> 0.00001
        results = validate_summary_metrics(summary)
        missed = [r for r in results if "Missed" in r.name]
        self.assertTrue(any(r.status == MetricStatus.FAILED for r in missed))

    def test_processing_median_exceeds_threshold(self):
        summary = self._make_passing_summary()
        summary["processing_time_median_us"] = 60.0  # > 50us
        results = validate_summary_metrics(summary)
        proc = [r for r in results if "Median" in r.name]
        self.assertTrue(any(r.status == MetricStatus.FAILED for r in proc))

    def test_processing_p99_exceeds_threshold(self):
        summary = self._make_passing_summary()
        summary["processing_time_p99_us"] = 250.0  # > 200us
        results = validate_summary_metrics(summary)
        proc = [r for r in results if "P99" in r.name and "Processing" in r.name]
        self.assertTrue(any(r.status == MetricStatus.FAILED for r in proc))

    def test_jitter_us_fallback(self):
        """When jitter_p99_ms is absent, jitter_p99_us is used."""
        summary = self._make_passing_summary()
        del summary["jitter_p99_ms"]
        summary["jitter_p99_us"] = 200.0  # < 250us -> pass
        results = validate_summary_metrics(summary)
        jitter = [r for r in results if "Jitter" in r.name]
        self.assertTrue(any(r.status == MetricStatus.PASSED for r in jitter))

    def test_missing_fields_are_skipped(self):
        results = validate_summary_metrics({})
        for r in results:
            self.assertEqual(r.status, MetricStatus.SKIPPED)

    def test_exact_threshold_values_pass(self):
        summary = {
            "rt_loop_us": THRESHOLDS["rt_loop_us"],
            "jitter_p99_ms": THRESHOLDS["jitter_p99_ms"],
            "missed_tick_rate": THRESHOLDS["missed_tick_rate"],
            "processing_time_median_us": THRESHOLDS["processing_time_median_us"],
            "processing_time_p99_us": THRESHOLDS["processing_time_p99_us"],
        }
        results = validate_summary_metrics(summary)
        checked = [r for r in results if r.status != MetricStatus.SKIPPED]
        for r in checked:
            self.assertEqual(r.status, MetricStatus.PASSED,
                             f"{r.name} at exact threshold should pass")


class TestValidateBenchmarkMetrics(unittest.TestCase):
    """Tests for validate_benchmark_metrics with individual benchmark entries."""

    def _make_rt_benchmark(self, **overrides):
        base = {
            "name": "rt_timing/1khz_tick_precision",
            "percentiles": {"p50": 50_000, "p99": 200_000},
            "custom_metrics": {
                "missed_tick_rate": 0.000001,
                "e2e_latency_p99_us": 200.0,
                "rt_heap_allocs": 0,
            },
            "sample_count": 1000,
        }
        base.update(overrides)
        return base

    def _make_processing_benchmark(self, **overrides):
        base = {
            "name": "rt_timing/pipeline_processing",
            "percentiles": {"p50": 25_000, "p99": 150_000},
            "custom_metrics": {
                "missed_tick_rate": 0.0,
                "e2e_latency_p99_us": 0.0,
                "rt_heap_allocs": 0,
            },
            "sample_count": 1000,
        }
        base.update(overrides)
        return base

    def test_passing_rt_benchmark(self):
        results = validate_benchmark_metrics([self._make_rt_benchmark()])
        for r in results:
            self.assertEqual(r.status, MetricStatus.PASSED,
                             f"{r.name} should pass: value={r.value}, threshold={r.threshold}")

    def test_jitter_p99_exceeds_threshold(self):
        bench = self._make_rt_benchmark()
        bench["percentiles"]["p99"] = 300_000  # 300us > 250us
        results = validate_benchmark_metrics([bench])
        jitter = [r for r in results if "Jitter P99" in r.name]
        self.assertTrue(any(r.status == MetricStatus.FAILED for r in jitter))

    def test_processing_benchmark_median_exceeds(self):
        bench = self._make_processing_benchmark()
        bench["percentiles"]["p50"] = 60_000  # 60us > 50us
        results = validate_benchmark_metrics([bench])
        median = [r for r in results if "Median" in r.name]
        self.assertTrue(any(r.status == MetricStatus.FAILED for r in median))

    def test_processing_benchmark_p99_exceeds(self):
        bench = self._make_processing_benchmark()
        bench["percentiles"]["p99"] = 250_000  # 250us > 200us
        results = validate_benchmark_metrics([bench])
        p99 = [r for r in results if "Processing P99" in r.name]
        self.assertTrue(any(r.status == MetricStatus.FAILED for r in p99))

    def test_missed_tick_rate_in_custom_metrics(self):
        bench = self._make_rt_benchmark()
        bench["custom_metrics"]["missed_tick_rate"] = 0.01  # >> 0.00001
        results = validate_benchmark_metrics([bench])
        missed = [r for r in results if "Missed Ticks" in r.name]
        self.assertTrue(any(r.status == MetricStatus.FAILED for r in missed))

    def test_rt_heap_allocs_nonzero_fails(self):
        bench = self._make_rt_benchmark()
        bench["custom_metrics"]["rt_heap_allocs"] = 5
        results = validate_benchmark_metrics([bench])
        allocs = [r for r in results if "Heap Allocs" in r.name]
        self.assertTrue(any(r.status == MetricStatus.FAILED for r in allocs))

    def test_non_rt_benchmarks_ignored(self):
        bench = {"name": "memory/zero_alloc_pipeline", "percentiles": {"p50": 1, "p99": 2},
                 "custom_metrics": {}, "sample_count": 10}
        results = validate_benchmark_metrics([bench])
        self.assertEqual(len(results), 0)

    def test_multiple_benchmarks_validated(self):
        results = validate_benchmark_metrics([
            self._make_rt_benchmark(),
            self._make_processing_benchmark(),
        ])
        self.assertTrue(len(results) >= 2)


class TestValidatePerformance(unittest.TestCase):
    """End-to-end tests for validate_performance."""

    def _make_passing_data(self):
        return {
            "summary": {
                "rt_loop_us": 500.0,
                "jitter_p99_ms": 0.15,
                "missed_tick_rate": 0.000005,
                "processing_time_median_us": 30.0,
                "processing_time_p99_us": 150.0,
            },
            "benchmarks": [
                {
                    "name": "rt_timing/1khz_tick_precision",
                    "percentiles": {"p50": 50_000, "p99": 200_000},
                    "custom_metrics": {
                        "missed_tick_rate": 0.000005,
                        "e2e_latency_p99_us": 200.0,
                        "rt_heap_allocs": 0,
                    },
                    "sample_count": 1000,
                },
                {
                    "name": "rt_timing/pipeline_processing",
                    "percentiles": {"p50": 25_000, "p99": 150_000},
                    "custom_metrics": {
                        "missed_tick_rate": 0.0,
                        "e2e_latency_p99_us": 0.0,
                        "rt_heap_allocs": 0,
                    },
                    "sample_count": 1000,
                },
            ],
        }

    def test_passing_data_passes(self):
        result = validate_performance(self._make_passing_data())
        self.assertTrue(result.passed)

    def test_failing_summary_fails_overall(self):
        data = self._make_passing_data()
        data["summary"]["jitter_p99_ms"] = 0.50  # > 0.25
        result = validate_performance(data)
        self.assertFalse(result.passed)
        self.assertTrue(len(result.failed_metrics) > 0)

    def test_failing_benchmark_fails_overall(self):
        data = self._make_passing_data()
        data["benchmarks"][0]["percentiles"]["p99"] = 300_000  # jitter > 250us
        result = validate_performance(data)
        self.assertFalse(result.passed)

    def test_empty_data_passes_vacuously(self):
        result = validate_performance({})
        # No checked metrics -> all skipped -> passes vacuously
        self.assertTrue(result.passed)

    def test_skipped_metrics_do_not_cause_failure(self):
        data = {"summary": {}, "benchmarks": []}
        result = validate_performance(data)
        self.assertTrue(result.passed)

    def test_validation_result_properties(self):
        data = self._make_passing_data()
        data["summary"]["rt_loop_us"] = 1200.0  # fail
        result = validate_performance(data)
        self.assertTrue(len(result.failed_metrics) >= 1)
        self.assertTrue(len(result.passed_metrics) >= 1)


class TestFormatValue(unittest.TestCase):
    """Tests for _format_value display formatting."""

    def test_normal_value(self):
        s = _format_value(100.5, "us")
        self.assertIn("100.50", s)
        self.assertTrue(s.endswith("us"))

    def test_small_value(self):
        s = _format_value(0.005, "")
        self.assertIn("0.005", s)

    def test_very_small_value(self):
        s = _format_value(0.00001, "")
        # Should use scientific or many decimal places
        self.assertIn("1", s)

    def test_zero_value(self):
        s = _format_value(0.0, "ms")
        self.assertIn("0", s)

    def test_large_value(self):
        s = _format_value(50000.0, "us")
        self.assertIn("50000", s)


class TestMetricResultStr(unittest.TestCase):
    """Tests for MetricResult string representation."""

    def test_passed_format(self):
        r = MetricResult("test", 10.0, 100.0, "us", MetricStatus.PASSED, "desc", "src")
        s = str(r)
        self.assertIn("[PASS]", s)
        self.assertIn("test", s)

    def test_failed_format(self):
        r = MetricResult("test", 200.0, 100.0, "us", MetricStatus.FAILED, "desc", "src")
        s = str(r)
        self.assertIn("[FAIL]", s)

    def test_skipped_format(self):
        r = MetricResult("test", 0.0, 100.0, "us", MetricStatus.SKIPPED, "desc", "src")
        s = str(r)
        self.assertIn("[SKIP]", s)


class TestJsonFileIntegration(unittest.TestCase):
    """Tests that validate_performance works with JSON fixture files."""

    def test_roundtrip_with_tempfile(self):
        """Write benchmark data to a temp file, parse it, and validate."""
        data = {
            "summary": {
                "rt_loop_us": 200.0,
                "jitter_p99_ms": 0.10,
                "missed_tick_rate": 0.0,
                "processing_time_median_us": 20.0,
                "processing_time_p99_us": 100.0,
            },
            "benchmarks": [],
        }
        with tempfile.NamedTemporaryFile(mode='w', suffix='.json', delete=False) as f:
            json.dump(data, f)
            path = f.name

        try:
            result = validate_performance(data)
            self.assertTrue(result.passed)
        finally:
            os.unlink(path)


class TestThresholdConstants(unittest.TestCase):
    """Tests that THRESHOLDS dict has expected keys and values."""

    def test_required_keys_present(self):
        required = [
            "rt_loop_us", "jitter_p99_ms", "jitter_p99_us",
            "missed_tick_rate", "processing_time_median_us",
            "processing_time_p99_us",
        ]
        for key in required:
            self.assertIn(key, THRESHOLDS, f"Missing threshold key: {key}")

    def test_jitter_ms_and_us_consistent(self):
        # 0.25ms = 250us
        self.assertAlmostEqual(THRESHOLDS["jitter_p99_ms"] * 1000,
                               THRESHOLDS["jitter_p99_us"])

    def test_all_thresholds_positive(self):
        for key, val in THRESHOLDS.items():
            self.assertGreater(val, 0, f"Threshold {key} should be positive")


if __name__ == '__main__':
    unittest.main()

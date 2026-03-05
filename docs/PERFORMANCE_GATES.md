# Performance Gates

This document describes the performance gate enforcement process for OpenRacing's real-time (RT) force feedback pipeline. The RT path runs at 1kHz (1ms tick budget) and must meet strict timing, allocation, and reliability requirements.

## Thresholds

| Metric | Threshold | Unit | Requirement |
|--------|-----------|------|-------------|
| RT Loop Total | ≤ 1000 | µs | NFR-01 / 14.3 |
| P99 Jitter | ≤ 0.25 | ms | NFR-01 / 14.3 |
| Missed Tick Rate | ≤ 0.001% | (0.00001) | NFR-01 / 14.3 |
| Processing Time (median) | ≤ 50 | µs | NFR-01 / 14.4 |
| Processing Time (P99) | ≤ 200 | µs | NFR-01 / 14.4 |
| RT Heap Allocations | 0 | count | NFR-01 |
| E2E Latency (P99) | ≤ 2000 | µs | optional |

### RT Pipeline Budget Breakdown

```
Input → Filter Pipeline → Safety Checks → Output
 50µs    200µs (median)      50µs        100µs
         800µs (p99)
```

Total budget: 1000µs at 1kHz. Jitter P99 must be ≤ 0.25ms.

## What Is Measured

### Benchmarks (`cargo bench --bench rt_timing`)

The `rt_timing` benchmark produces `bench_results.json` with:

- **Per-benchmark percentiles** (p50, p99) in nanoseconds
- **Custom metrics**: `missed_tick_rate`, `e2e_latency_p99_us`, `rt_heap_allocs`
- **Summary section**: aggregated `rt_loop_us`, `jitter_p99_ms`, `missed_tick_rate`, `processing_time_median_us`, `processing_time_p99_us`

### Allocation Tests (`cargo test`)

The `racing-wheel-engine` crate uses a `TrackingAllocator` as the global allocator in test builds. Tests in `tests/perf_gate_tests.rs`, `tests/rt_allocation_tests.rs`, and `src/rt_alloc_verify_tests.rs` verify:

- **Full RT tick path** (input → filter → safety → output) is allocation-free
- **Individual stages** (filter pipeline, safety clamp, curve lookup) are allocation-free
- **Sustained operation** (10,000+ ticks) triggers no allocations
- **Pipeline swap** at tick boundary is allocation-free
- **Edge cases** (NaN, infinity, sign flips, hands-off toggling) are allocation-free

### Complexity Bound Tests

Deterministic tests in `tests/perf_gate_tests.rs` verify:

- Pipeline traversal is O(N) in the number of filter nodes
- CurveLut lookup is O(1) via fixed-size [f32; 256] table
- Safety clamp is O(1) with no iteration
- Frame is a fixed-size, stack-only, Copy type (≤ 64 bytes)
- FilterNodeFn is a thin function pointer (direct dispatch, no vtable)

## How to Run Locally

### 1. Run performance benchmarks and validate

```bash
# Generate benchmark results
BENCHMARK_JSON_OUTPUT=1 BENCHMARK_JSON_PATH=bench_results.json cargo bench --bench rt_timing

# Validate against thresholds (text output)
python scripts/validate_performance.py bench_results.json --strict --verbose

# Validate with JSON output (for CI integration)
python scripts/validate_performance.py bench_results.json --output-format json

# Generate detailed report
python scripts/validate_performance.py bench_results.json --report perf_report.md --verbose
```

### 2. Run allocation and complexity tests

```bash
# All performance gate tests
cargo test --package racing-wheel-engine --test perf_gate_tests

# All RT allocation tests
cargo test --package racing-wheel-engine --test rt_allocation_tests

# In-crate allocation verification tests
cargo test --package racing-wheel-engine rt_alloc_verify
```

### 3. Run the full test suite

```bash
cargo test --all-features --workspace
```

## CI Enforcement

The CI pipeline enforces performance gates in two ways:

### Gate 1: Benchmark Validation

After running `cargo bench --bench rt_timing`, CI runs:

```bash
python scripts/validate_performance.py bench_results.json --strict
```

If any metric exceeds its threshold, the script exits with code 1 and the build fails.

### Gate 2: Allocation and Complexity Tests

Standard `cargo test` runs the performance gate tests. Any allocation detected in the RT path fails the test.

## Validator Output

### Text Output (default)

```
[INFO] Validating performance gates...
   Input: bench_results.json
   Mode: strict
   Found: 2 benchmark(s), summary: yes

============================================================
Performance Gate Validation Report
============================================================

[PASS] PASSED METRICS:
----------------------------------------
  [PASS] RT Loop Total: 950.00us (limit: 1000.00us)
  [PASS] P99 Jitter: 0.2200ms (limit: 0.2500ms)
  [PASS] Missed Tick Rate: 0.000005 (limit: 1.00e-05)
  [PASS] Processing Time Median: 35.00us (limit: 50.00us)
  [PASS] Processing Time P99: 180.00us (limit: 200.00us)

============================================================
Summary: 5/5 metrics passed

[PASS] All performance gates PASSED!
```

### JSON Output (`--output-format json`)

```json
{
  "timestamp": "2025-01-15T10:30:00.000000",
  "passed": true,
  "total_metrics": 5,
  "passed_count": 5,
  "failed_count": 0,
  "metrics": [
    {
      "name": "RT Loop Total",
      "value": 950.0,
      "threshold": 1000.0,
      "unit": "us",
      "status": "passed",
      "description": "Total RT loop time @ 1kHz",
      "source": "summary.rt_loop_us"
    }
  ]
}
```

## Adding New Metrics

1. Add the threshold to `THRESHOLDS` in `scripts/validate_performance.py`
2. Add extraction logic in `validate_summary_metrics()` or `validate_benchmark_metrics()`
3. Add a corresponding test in `crates/engine/tests/perf_gate_tests.rs`
4. Update this document

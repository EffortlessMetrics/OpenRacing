#!/usr/bin/env python3
"""
Performance gate validation script.
Validates that RT timing benchmarks meet the requirements from NFR-01 and tech.md:
- RT loop total <= 1000us @ 1kHz
- P99 jitter <= 0.25ms (250us)
- Missed ticks <= 0.001%
- Processing time <= 50us median, <= 200us p99

Requirements: 14.2, 14.3, 14.4, 14.5
"""

import json
import sys
import argparse
import os
from dataclasses import dataclass
from typing import Dict, Any, List, Optional
from enum import Enum


class MetricStatus(Enum):
    """Status of a performance metric check."""
    PASSED = "passed"
    FAILED = "failed"
    SKIPPED = "skipped"


@dataclass
class MetricResult:
    """Result of checking a single performance metric."""
    name: str
    value: float
    threshold: float
    unit: str
    status: MetricStatus
    description: str
    source: str

    def __str__(self) -> str:
        status_icon = {
            MetricStatus.PASSED: "[PASS]",
            MetricStatus.FAILED: "[FAIL]",
            MetricStatus.SKIPPED: "[SKIP]",
        }[self.status]
        value_str = _format_value(self.value, self.unit)
        threshold_str = _format_value(self.threshold, self.unit)
        return f"{status_icon} {self.name}: {value_str} (limit: {threshold_str})"


def _format_value(value: float, unit: str) -> str:
    """Format a metric value with appropriate precision for display.
    
    Args:
        value: The numeric value to format.
        unit: The unit string.
        
    Returns:
        Formatted string representation with unit.
    """
    # For very small values (like missed tick rate), use scientific notation or more decimals
    if abs(value) < 0.0001 and value != 0:
        return f"{value:.2e}{unit}"
    elif abs(value) < 0.01:
        return f"{value:.6f}{unit}"
    elif abs(value) < 1:
        return f"{value:.4f}{unit}"
    else:
        return f"{value:.2f}{unit}"


@dataclass
class ValidationResult:
    """Overall validation result."""
    metrics: List[MetricResult]
    passed: bool

    @property
    def failed_metrics(self) -> List[MetricResult]:
        return [m for m in self.metrics if m.status == MetricStatus.FAILED]

    @property
    def passed_metrics(self) -> List[MetricResult]:
        return [m for m in self.metrics if m.status == MetricStatus.PASSED]


# Performance thresholds from requirements (NFR-01) and tech.md
# Requirement 14.3: RT loop <=1000us total, p99 jitter <=0.25ms, missed ticks <=0.001%
# Requirement 14.4: processing time <=50us median, <=200us p99
THRESHOLDS = {
    "rt_loop_us": 1000.0,           # Total RT Budget: 1000us @ 1kHz
    "jitter_p99_ms": 0.25,          # P99 Jitter: <= 0.25ms
    "jitter_p99_us": 250.0,         # P99 Jitter: <= 250us (same as above, different unit)
    "missed_tick_rate": 0.00001,    # Missed Ticks: <= 0.001% (0.00001 as decimal)
    "processing_time_median_us": 50.0,   # Processing Time: <= 50us median
    "processing_time_p99_us": 200.0,     # Processing Time: <= 200us p99
    "e2e_latency_p99_us": 2000.0,        # E2E latency: <= 2ms p99 (optional)
}


def resolve_benchmark_path(file_path: str) -> str:
    """Resolve benchmark result path across common workspace locations."""
    if os.path.exists(file_path):
        return file_path

    candidates = []
    if not os.path.isabs(file_path):
        candidates.append(os.path.join("crates", "engine", file_path))

    for candidate in candidates:
        if os.path.exists(candidate):
            print(f"[INFO] Benchmark file not found at {file_path}; using {candidate}")
            return candidate

    return file_path


def parse_benchmark_results(file_path: str) -> Dict[str, Any]:
    """Parse benchmark results from JSON file.
    
    Args:
        file_path: Path to the JSON benchmark results file.
        
    Returns:
        Parsed JSON data as a dictionary.
        
    Raises:
        SystemExit: If file cannot be read or parsed.
    """
    resolved_path = resolve_benchmark_path(file_path)

    if not os.path.exists(resolved_path):
        print(f"[ERROR] Benchmark file not found: {file_path}")
        sys.exit(1)
        
    try:
        with open(resolved_path, 'r') as f:
            data = json.load(f)
        return data
    except json.JSONDecodeError as e:
        print(f"[ERROR] Error parsing benchmark JSON: {e}")
        sys.exit(1)
    except IOError as e:
        print(f"[ERROR] Error reading benchmark file: {e}")
        sys.exit(1)


def check_metric(
    name: str,
    value: Optional[float],
    threshold: float,
    unit: str,
    description: str,
    source: str,
    lower_is_better: bool = True
) -> MetricResult:
    """Check a single metric against its threshold.
    
    Args:
        name: Name of the metric.
        value: Measured value (None if not available).
        threshold: Maximum allowed value.
        unit: Unit string for display.
        description: Human-readable description.
        lower_is_better: If True, value must be <= threshold.
        
    Returns:
        MetricResult with pass/fail status.
    """
    if value is None:
        return MetricResult(
            name=name,
            value=0.0,
            threshold=threshold,
            unit=unit,
            status=MetricStatus.SKIPPED,
            description=f"{description} (not available)",
            source=source,
        )
    
    if lower_is_better:
        passed = value <= threshold
    else:
        passed = value >= threshold
        
    return MetricResult(
        name=name,
        value=value,
        threshold=threshold,
        unit=unit,
        status=MetricStatus.PASSED if passed else MetricStatus.FAILED,
        description=description,
        source=source,
    )


def validate_summary_metrics(summary: Dict[str, Any]) -> List[MetricResult]:
    """Validate metrics from the summary section.
    
    Args:
        summary: The 'summary' section from benchmark results.
        
    Returns:
        List of MetricResult for each checked metric.
    """
    results = []
    
    # RT loop total (Requirement 14.3)
    results.append(check_metric(
        name="RT Loop Total",
        value=summary.get("rt_loop_us"),
        threshold=THRESHOLDS["rt_loop_us"],
        unit="us",
        description="Total RT loop time @ 1kHz",
        source="summary.rt_loop_us",
    ))

    # P99 Jitter (Requirement 14.3)
    # Accept either ms or us summary fields.
    jitter_ms = summary.get("jitter_p99_ms")
    jitter_us = summary.get("jitter_p99_us")
    if jitter_ms is not None:
        results.append(check_metric(
            name="P99 Jitter",
            value=jitter_ms,
            threshold=THRESHOLDS["jitter_p99_ms"],
            unit="ms",
            description="P99 jitter at 1kHz",
            source="summary.jitter_p99_ms",
        ))
    elif jitter_us is not None:
        results.append(check_metric(
            name="P99 Jitter",
            value=jitter_us,
            threshold=THRESHOLDS["jitter_p99_us"],
            unit="us",
            description="P99 jitter at 1kHz",
            source="summary.jitter_p99_us",
        ))

    # Missed tick rate (Requirement 14.3)
    results.append(check_metric(
        name="Missed Tick Rate",
        value=summary.get("missed_tick_rate"),
        threshold=THRESHOLDS["missed_tick_rate"],
        unit="",
        description="Missed tick rate ratio (0.001% = 0.00001)",
        source="summary.missed_tick_rate",
    ))
    
    # Processing time median (Requirement 14.4)
    results.append(check_metric(
        name="Processing Time Median",
        value=summary.get("processing_time_median_us"),
        threshold=THRESHOLDS["processing_time_median_us"],
        unit="us",
        description="Median processing time per tick",
        source="summary.processing_time_median_us",
    ))
    
    # Processing time P99 (Requirement 14.4)
    results.append(check_metric(
        name="Processing Time P99",
        value=summary.get("processing_time_p99_us"),
        threshold=THRESHOLDS["processing_time_p99_us"],
        unit="us",
        description="P99 processing time per tick",
        source="summary.processing_time_p99_us",
    ))
    
    return results


def validate_benchmark_metrics(benchmarks: List[Dict[str, Any]]) -> List[MetricResult]:
    """Validate metrics from individual benchmark entries.
    
    Args:
        benchmarks: List of benchmark entries.
        
    Returns:
        List of MetricResult for each checked metric.
    """
    results = []

    # Find RT timing benchmarks
    rt_benchmarks = [
        bench for bench in benchmarks
        if 'rt_timing' in bench.get('name', '') or '1khz' in bench.get('name', '').lower()
    ]

    for bench in rt_benchmarks:
        name = bench.get('name', 'unknown')
        lower_name = name.lower()
        percentiles = bench.get('percentiles', {})
        custom_metrics = bench.get('custom_metrics', {})

        p50_ns = percentiles.get('p50')
        p99_ns = percentiles.get('p99')

        is_jitter_benchmark = ('tick_precision' in lower_name) or ('jitter' in lower_name)
        is_processing_benchmark = (
            'pipeline_processing' in lower_name
            or ('processing' in lower_name and not is_jitter_benchmark)
        )

        # Metric mapping must match benchmark intent:
        # - tick/jitter benchmark percentiles -> jitter thresholds
        # - processing benchmark percentiles -> processing thresholds
        if is_jitter_benchmark and p99_ns is not None:
            p99_us = p99_ns / 1000.0
            results.append(check_metric(
                name=f"{name} - Jitter P99",
                value=p99_us,
                threshold=THRESHOLDS["jitter_p99_us"],
                unit="us",
                description=f"P99 jitter for {name}",
                source=f"benchmarks[{name}].percentiles.p99",
            ))

        if is_processing_benchmark:
            if p50_ns is not None:
                p50_us = p50_ns / 1000.0
                results.append(check_metric(
                    name=f"{name} - Processing Median",
                    value=p50_us,
                    threshold=THRESHOLDS["processing_time_median_us"],
                    unit="us",
                    description=f"Median processing time for {name}",
                    source=f"benchmarks[{name}].percentiles.p50",
                ))

            if p99_ns is not None:
                p99_us = p99_ns / 1000.0
                results.append(check_metric(
                    name=f"{name} - Processing P99",
                    value=p99_us,
                    threshold=THRESHOLDS["processing_time_p99_us"],
                    unit="us",
                    description=f"P99 processing time for {name}",
                    source=f"benchmarks[{name}].percentiles.p99",
                ))

        # Missed tick rate from custom metrics (ratio: 0.0 to 1.0)
        missed_rate = custom_metrics.get('missed_tick_rate')
        if missed_rate is not None:
            results.append(check_metric(
                name=f"{name} - Missed Ticks",
                value=missed_rate,
                threshold=THRESHOLDS["missed_tick_rate"],
                unit="",
                description=f"Missed tick rate ratio for {name}",
                source=f"benchmarks[{name}].custom_metrics.missed_tick_rate",
            ))

        # E2E latency from custom metrics
        e2e_latency = custom_metrics.get('e2e_latency_p99_us')
        if e2e_latency is not None:
            results.append(check_metric(
                name=f"{name} - E2E Latency P99",
                value=e2e_latency,
                threshold=THRESHOLDS["e2e_latency_p99_us"],
                unit="us",
                description=f"E2E latency P99 for {name}",
                source=f"benchmarks[{name}].custom_metrics.e2e_latency_p99_us",
            ))

        # RT heap allocations (should be 0)
        rt_allocs = custom_metrics.get('rt_heap_allocs')
        if rt_allocs is not None:
            results.append(check_metric(
                name=f"{name} - RT Heap Allocs",
                value=float(rt_allocs),
                threshold=0.0,
                unit="",
                description=f"RT heap allocations for {name} (must be 0)",
                source=f"benchmarks[{name}].custom_metrics.rt_heap_allocs",
            ))

    return results


def validate_performance(results: Dict[str, Any]) -> ValidationResult:
    """Validate all performance metrics against thresholds.
    
    Args:
        results: Parsed benchmark results.
        
    Returns:
        ValidationResult with all metric checks.
    """
    all_metrics = []
    
    # Check summary metrics if available
    summary = results.get('summary', {})
    if summary:
        all_metrics.extend(validate_summary_metrics(summary))
    
    # Check individual benchmark metrics
    benchmarks = results.get('benchmarks', [])
    if benchmarks:
        all_metrics.extend(validate_benchmark_metrics(benchmarks))
    
    # Filter out skipped metrics for pass/fail determination
    checked_metrics = [m for m in all_metrics if m.status != MetricStatus.SKIPPED]
    
    # All checked metrics must pass
    passed = all(m.status == MetricStatus.PASSED for m in checked_metrics)
    
    return ValidationResult(metrics=all_metrics, passed=passed)


def print_validation_report(result: ValidationResult, verbose: bool = False) -> None:
    """Print a formatted validation report.
    
    Args:
        result: The validation result to report.
        verbose: If True, print all metrics; otherwise only failures.
    """
    print("\n" + "=" * 60)
    print("Performance Gate Validation Report")
    print("=" * 60)
    
    # Group metrics by status
    passed = result.passed_metrics
    failed = result.failed_metrics
    skipped = [m for m in result.metrics if m.status == MetricStatus.SKIPPED]
    
    # Always show failed metrics
    if failed:
        print("\n[FAIL] FAILED METRICS:")
        print("-" * 40)
        for metric in failed:
            print(f"  {metric}")
            print(f"     -> Source: {metric.source}")
            print(f"     -> {metric.description}")
    
    # Show passed metrics in verbose mode
    if verbose and passed:
        print("\n[PASS] PASSED METRICS:")
        print("-" * 40)
        for metric in passed:
            print(f"  {metric}")
    
    # Show skipped metrics in verbose mode
    if verbose and skipped:
        print("\n[SKIP] SKIPPED METRICS (not available in input):")
        print("-" * 40)
        for metric in skipped:
            print(f"  {metric.name}")
    
    # Summary
    print("\n" + "=" * 60)
    total = len(passed) + len(failed)
    print(f"Summary: {len(passed)}/{total} metrics passed")
    
    if result.passed:
        print("\n[PASS] All performance gates PASSED!")
        print("   Requirements validated:")
        print("   - 14.3: RT loop <=1000us, p99 jitter <=0.25ms, missed ticks <=0.001%")
        print("   - 14.4: Processing time <=50us median, <=200us p99")
    else:
        print("\n[FAIL] Performance gate validation FAILED!")
        print(f"   {len(failed)} metric(s) exceeded threshold(s)")
        print("\n   Failed requirements:")
        # Report which specific requirements failed (Requirement 14.5)
        for metric in failed:
            value_str = _format_value(metric.value, metric.unit)
            threshold_str = _format_value(metric.threshold, metric.unit)
            if "RT Loop" in metric.name or "Jitter" in metric.name or "Missed" in metric.name:
                print(
                    f"   - 14.3: {metric.name} = {value_str} > {threshold_str} "
                    f"(source: {metric.source})"
                )
            elif "Processing" in metric.name:
                print(
                    f"   - 14.4: {metric.name} = {value_str} > {threshold_str} "
                    f"(source: {metric.source})"
                )
            else:
                print(
                    f"   - {metric.name} = {value_str} > {threshold_str} "
                    f"(source: {metric.source})"
                )


def generate_performance_report(results: Dict[str, Any], validation: ValidationResult, output_file: str) -> None:
    """Generate a detailed performance report to file.
    
    Args:
        results: Raw benchmark results.
        validation: Validation result.
        output_file: Path to write the report.
    """
    from datetime import datetime
    
    lines = [
        "# OpenRacing Performance Report",
        f"Generated: {datetime.now().isoformat()}",
        "",
        "## Performance Gates Status",
        "",
        f"**Overall Status**: {'PASSED' if validation.passed else 'FAILED'}",
        "",
        "### Thresholds (from tech.md and Requirements 14.3, 14.4)",
        "",
        "| Metric | Threshold | Unit |",
        "|--------|-----------|------|",
        f"| RT Loop Total | {THRESHOLDS['rt_loop_us']} | us |",
        f"| P99 Jitter | {THRESHOLDS['jitter_p99_ms']} | ms |",
        f"| Missed Tick Rate | {THRESHOLDS['missed_tick_rate']} | (0.001%) |",
        f"| Processing Median | {THRESHOLDS['processing_time_median_us']} | us |",
        f"| Processing P99 | {THRESHOLDS['processing_time_p99_us']} | us |",
        "",
        "### Results",
        "",
        "| Metric | Source | Value | Threshold | Status |",
        "|--------|--------|-------|-----------|--------|",
    ]
    
    for metric in validation.metrics:
        if metric.status == MetricStatus.SKIPPED:
            continue
        status = "PASS" if metric.status == MetricStatus.PASSED else "FAIL"
        # Use appropriate precision for the value
        value_str = _format_value(metric.value, metric.unit)
        threshold_str = _format_value(metric.threshold, metric.unit)
        lines.append(
            f"| {metric.name} | `{metric.source}` | {value_str} | {threshold_str} | {status} |"
        )
    
    lines.extend([
        "",
        "### Raw Benchmark Data",
        "",
    ])
    
    # Add summary if available
    summary = results.get('summary', {})
    if summary:
        lines.append("#### Summary")
        lines.append("```json")
        lines.append(json.dumps(summary, indent=2))
        lines.append("```")
        lines.append("")
    
    # Add benchmark details
    benchmarks = results.get('benchmarks', [])
    if benchmarks:
        lines.append("#### Benchmarks")
        for bench in benchmarks:
            lines.append(f"\n**{bench.get('name', 'unknown')}**")
            lines.append(f"- Sample count: {bench.get('sample_count', 'N/A')}")
            percentiles = bench.get('percentiles', {})
            if percentiles:
                lines.append(f"- P50: {percentiles.get('p50', 'N/A')} ns")
                lines.append(f"- P99: {percentiles.get('p99', 'N/A')} ns")
    
    report_content = "\n".join(lines)
    
    try:
        with open(output_file, 'w', encoding='utf-8') as f:
            f.write(report_content)
        print(f"[INFO] Performance report written to {output_file}")
    except IOError as e:
        print(f"[WARN] Could not write report to {output_file}: {e}")


def main() -> int:
    """Main entry point for performance validation.
    
    Returns:
        Exit code: 0 for success, 1 for failure.
    """
    parser = argparse.ArgumentParser(
        description='Validate performance gate requirements (Requirements 14.2-14.5)',
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Performance Thresholds (from tech.md):
  - RT Loop Total:        <= 1000us @ 1kHz
  - P99 Jitter:           <= 0.25ms (250us)
  - Missed Tick Rate:     <= 0.001%% (0.00001)
  - Processing Median:    <= 50us
  - Processing P99:       <= 200us

Examples:
  %(prog)s bench_results.json
  %(prog)s bench_results.json --strict
  %(prog)s bench_results.json --report perf_report.md --verbose
        """
    )
    parser.add_argument(
        'benchmark_file',
        help='Path to benchmark results JSON file'
    )
    parser.add_argument(
        '--strict',
        action='store_true',
        help='Strict mode: exit with error on any threshold violation (default behavior)'
    )
    parser.add_argument(
        '--warn-only',
        action='store_true',
        help='Warning mode: report violations but exit with success'
    )
    parser.add_argument(
        '--report',
        metavar='FILE',
        help='Generate detailed performance report to FILE'
    )
    parser.add_argument(
        '--verbose', '-v',
        action='store_true',
        help='Verbose output: show all metrics including passed ones'
    )
    
    args = parser.parse_args()
    
    print("[INFO] Validating performance gates...")
    print(f"   Input: {args.benchmark_file}")
    print(f"   Mode: {'warn-only' if args.warn_only else 'strict'}")
    
    # Parse benchmark results
    results = parse_benchmark_results(args.benchmark_file)
    
    # Count benchmarks found
    benchmark_count = len(results.get('benchmarks', []))
    has_summary = bool(results.get('summary'))
    print(f"   Found: {benchmark_count} benchmark(s), summary: {'yes' if has_summary else 'no'}")
    
    # Validate all metrics
    validation = validate_performance(results)
    
    # Print report
    print_validation_report(validation, verbose=args.verbose)
    
    # Generate file report if requested
    if args.report:
        generate_performance_report(results, validation, args.report)
    
    # Determine exit code
    # Requirement 14.2: WHEN benchmark results exceed thresholds, THE CI_Pipeline SHALL fail the build
    if args.warn_only:
        return 0
    else:
        return 0 if validation.passed else 1


if __name__ == '__main__':
    sys.exit(main())

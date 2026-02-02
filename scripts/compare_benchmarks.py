#!/usr/bin/env python3
"""
Benchmark comparison tool for detecting performance regressions.

Compares two benchmark runs and flags any metrics that regressed beyond a tolerance.
Designed for CI integration to catch performance regressions in pull requests.

Example usage:
    python scripts/compare_benchmarks.py baseline.json current.json
    python scripts/compare_benchmarks.py baseline.json current.json --tolerance 0.15
    python scripts/compare_benchmarks.py baseline.json current.json --output report.md
"""

import argparse
import json
import sys
from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path
from typing import Any, Dict, List, Optional


class MetricChange(Enum):
    """Classification of how a metric changed between runs."""
    REGRESSION = "regression"
    IMPROVEMENT = "improvement"
    UNCHANGED = "unchanged"


@dataclass
class MetricComparison:
    """Comparison result for a single metric.

    Attributes:
        name: Human-readable name of the metric.
        baseline_value: Value from the baseline benchmark run.
        current_value: Value from the current benchmark run.
        unit: Unit of measurement for the metric.
        change_percent: Percentage change from baseline (positive = worse for lower-is-better).
        change_type: Classification of the change (regression, improvement, unchanged).
        lower_is_better: Whether lower values indicate better performance.
    """
    name: str
    baseline_value: float
    current_value: float
    unit: str
    change_percent: float
    change_type: MetricChange
    lower_is_better: bool = True

    def __str__(self) -> str:
        """Return a formatted string representation of the comparison."""
        direction = "+" if self.change_percent > 0 else ""
        return (
            f"{self.name}: {self.baseline_value:.4f}{self.unit} -> "
            f"{self.current_value:.4f}{self.unit} ({direction}{self.change_percent:.1f}%)"
        )


@dataclass
class ComparisonResult:
    """Complete result of comparing two benchmark runs.

    Attributes:
        baseline_file: Path to the baseline benchmark JSON file.
        current_file: Path to the current benchmark JSON file.
        tolerance: Tolerance percentage used for regression detection.
        metrics: List of all metric comparisons.
        regressions: List of metrics that exceeded the tolerance (performance worsened).
        improvements: List of metrics that improved beyond the tolerance.
    """
    baseline_file: Path
    current_file: Path
    tolerance: float
    metrics: List[MetricComparison] = field(default_factory=list)
    regressions: List[MetricComparison] = field(default_factory=list)
    improvements: List[MetricComparison] = field(default_factory=list)

    @property
    def has_regressions(self) -> bool:
        """Return True if any regressions were detected."""
        return len(self.regressions) > 0


def load_benchmark_file(file_path: Path) -> Dict[str, Any]:
    """Load and parse a benchmark JSON file.

    Args:
        file_path: Path to the benchmark JSON file.

    Returns:
        Parsed JSON data as a dictionary.

    Raises:
        SystemExit: If the file cannot be read or parsed.
    """
    if not file_path.exists():
        print(f"Error: Benchmark file not found: {file_path}")
        sys.exit(1)

    try:
        with open(file_path, 'r', encoding='utf-8') as f:
            return json.load(f)
    except json.JSONDecodeError as e:
        print(f"Error: Failed to parse JSON in {file_path}: {e}")
        sys.exit(1)
    except IOError as e:
        print(f"Error: Failed to read {file_path}: {e}")
        sys.exit(1)


def extract_summary_metrics(data: Dict[str, Any]) -> Dict[str, tuple]:
    """Extract key metrics from the benchmark summary section.

    Args:
        data: Parsed benchmark JSON data.

    Returns:
        Dictionary mapping metric names to (value, unit, lower_is_better) tuples.
    """
    metrics = {}
    summary = data.get('summary', {})

    # RT loop time (lower is better)
    if 'rt_loop_us' in summary:
        metrics['RT Loop Time'] = (summary['rt_loop_us'], 'us', True)

    # P99 Jitter (lower is better)
    if 'jitter_p99_ms' in summary:
        metrics['P99 Jitter'] = (summary['jitter_p99_ms'], 'ms', True)

    # Missed tick rate (lower is better)
    if 'missed_tick_rate' in summary:
        metrics['Missed Tick Rate'] = (summary['missed_tick_rate'], '', True)

    # Processing time median (lower is better)
    if 'processing_time_median_us' in summary:
        metrics['Processing Time (Median)'] = (summary['processing_time_median_us'], 'us', True)

    # Processing time P99 (lower is better)
    if 'processing_time_p99_us' in summary:
        metrics['Processing Time (P99)'] = (summary['processing_time_p99_us'], 'us', True)

    return metrics


def extract_benchmark_metrics(data: Dict[str, Any]) -> Dict[str, tuple]:
    """Extract metrics from individual benchmark entries.

    Args:
        data: Parsed benchmark JSON data.

    Returns:
        Dictionary mapping metric names to (value, unit, lower_is_better) tuples.
    """
    metrics = {}
    benchmarks = data.get('benchmarks', [])

    for bench in benchmarks:
        name = bench.get('name', 'unknown')
        percentiles = bench.get('percentiles', {})
        custom = bench.get('custom_metrics', {})

        # P50 (median) in nanoseconds - convert to microseconds
        if 'p50' in percentiles:
            p50_us = percentiles['p50'] / 1000.0
            metrics[f'{name}/p50'] = (p50_us, 'us', True)

        # P99 in nanoseconds - convert to microseconds
        if 'p99' in percentiles:
            p99_us = percentiles['p99'] / 1000.0
            metrics[f'{name}/p99'] = (p99_us, 'us', True)

        # Missed tick rate from custom metrics
        if 'missed_tick_rate' in custom:
            metrics[f'{name}/missed_tick_rate'] = (custom['missed_tick_rate'], '', True)

        # E2E latency P99
        if 'e2e_latency_p99_us' in custom:
            metrics[f'{name}/e2e_latency_p99'] = (custom['e2e_latency_p99_us'], 'us', True)

        # RT heap allocations (should be 0)
        if 'rt_heap_allocs' in custom:
            metrics[f'{name}/rt_heap_allocs'] = (float(custom['rt_heap_allocs']), '', True)

    return metrics


def calculate_change_percent(
    baseline: float,
    current: float,
    lower_is_better: bool
) -> float:
    """Calculate the percentage change between baseline and current values.

    Args:
        baseline: The baseline value.
        current: The current value.
        lower_is_better: If True, positive change indicates regression.

    Returns:
        Percentage change. For lower_is_better metrics, positive means regression.
    """
    if baseline == 0:
        if current == 0:
            return 0.0
        # Baseline was 0 but current is not - significant change
        return 100.0 if lower_is_better else -100.0

    raw_change = ((current - baseline) / abs(baseline)) * 100.0
    return raw_change


def classify_change(
    change_percent: float,
    tolerance: float,
    lower_is_better: bool
) -> MetricChange:
    """Classify a metric change as regression, improvement, or unchanged.

    Args:
        change_percent: The percentage change from baseline.
        tolerance: The tolerance threshold for detecting significant changes.
        lower_is_better: If True, positive change indicates regression.

    Returns:
        MetricChange classification.
    """
    tolerance_percent = tolerance * 100.0

    if lower_is_better:
        # For lower-is-better metrics (timing, etc.):
        # Positive change = regression (value increased)
        # Negative change = improvement (value decreased)
        if change_percent > tolerance_percent:
            return MetricChange.REGRESSION
        elif change_percent < -tolerance_percent:
            return MetricChange.IMPROVEMENT
    else:
        # For higher-is-better metrics (throughput, etc.):
        # Positive change = improvement (value increased)
        # Negative change = regression (value decreased)
        if change_percent < -tolerance_percent:
            return MetricChange.REGRESSION
        elif change_percent > tolerance_percent:
            return MetricChange.IMPROVEMENT

    return MetricChange.UNCHANGED


def compare_benchmarks(
    baseline: Path,
    current: Path,
    tolerance: float = 0.10
) -> ComparisonResult:
    """Compare two benchmark runs and identify regressions.

    Args:
        baseline: Path to the baseline benchmark JSON file.
        current: Path to the current benchmark JSON file.
        tolerance: Tolerance as a decimal (0.10 = 10%). Metrics that change
                   more than this amount are flagged.

    Returns:
        ComparisonResult with all metric comparisons and regression/improvement lists.
    """
    baseline_data = load_benchmark_file(baseline)
    current_data = load_benchmark_file(current)

    # Extract metrics from both files
    baseline_summary = extract_summary_metrics(baseline_data)
    current_summary = extract_summary_metrics(current_data)

    baseline_benchmarks = extract_benchmark_metrics(baseline_data)
    current_benchmarks = extract_benchmark_metrics(current_data)

    # Merge all metrics
    baseline_metrics = {**baseline_summary, **baseline_benchmarks}
    current_metrics = {**current_summary, **current_benchmarks}

    result = ComparisonResult(
        baseline_file=baseline,
        current_file=current,
        tolerance=tolerance
    )

    # Compare metrics that exist in both runs
    common_metrics = set(baseline_metrics.keys()) & set(current_metrics.keys())

    for metric_name in sorted(common_metrics):
        baseline_value, unit, lower_is_better = baseline_metrics[metric_name]
        current_value, _, _ = current_metrics[metric_name]

        change_percent = calculate_change_percent(
            baseline_value, current_value, lower_is_better
        )
        change_type = classify_change(change_percent, tolerance, lower_is_better)

        comparison = MetricComparison(
            name=metric_name,
            baseline_value=baseline_value,
            current_value=current_value,
            unit=unit,
            change_percent=change_percent,
            change_type=change_type,
            lower_is_better=lower_is_better
        )

        result.metrics.append(comparison)

        if change_type == MetricChange.REGRESSION:
            result.regressions.append(comparison)
        elif change_type == MetricChange.IMPROVEMENT:
            result.improvements.append(comparison)

    return result


def format_value(value: float, unit: str) -> str:
    """Format a metric value with appropriate precision.

    Args:
        value: The numeric value to format.
        unit: The unit string.

    Returns:
        Formatted string with unit.
    """
    if abs(value) < 0.0001 and value != 0:
        return f"{value:.2e}{unit}"
    elif abs(value) < 0.01:
        return f"{value:.6f}{unit}"
    elif abs(value) < 1:
        return f"{value:.4f}{unit}"
    else:
        return f"{value:.2f}{unit}"


def generate_comparison_report(result: ComparisonResult) -> str:
    """Generate a markdown report suitable for PR comments.

    Args:
        result: The ComparisonResult to format.

    Returns:
        Markdown-formatted report string.
    """
    lines = [
        "# Benchmark Comparison Report",
        "",
        f"**Baseline:** `{result.baseline_file.name}`",
        f"**Current:** `{result.current_file.name}`",
        f"**Tolerance:** {result.tolerance * 100:.0f}%",
        "",
    ]

    # Summary section
    if result.has_regressions:
        lines.append(f"## Summary: {len(result.regressions)} Regression(s) Detected")
        lines.append("")
    else:
        lines.append("## Summary: No Regressions Detected")
        lines.append("")

    # Regressions table
    if result.regressions:
        lines.append("### Regressions")
        lines.append("")
        lines.append("| Metric | Baseline | Current | Change |")
        lines.append("|--------|----------|---------|--------|")

        for metric in result.regressions:
            baseline_str = format_value(metric.baseline_value, metric.unit)
            current_str = format_value(metric.current_value, metric.unit)
            change_sign = "+" if metric.change_percent > 0 else ""
            lines.append(
                f"| {metric.name} | {baseline_str} | {current_str} | "
                f"{change_sign}{metric.change_percent:.1f}% |"
            )
        lines.append("")

    # Improvements table
    if result.improvements:
        lines.append("### Improvements")
        lines.append("")
        lines.append("| Metric | Baseline | Current | Change |")
        lines.append("|--------|----------|---------|--------|")

        for metric in result.improvements:
            baseline_str = format_value(metric.baseline_value, metric.unit)
            current_str = format_value(metric.current_value, metric.unit)
            change_sign = "+" if metric.change_percent > 0 else ""
            lines.append(
                f"| {metric.name} | {baseline_str} | {current_str} | "
                f"{change_sign}{metric.change_percent:.1f}% |"
            )
        lines.append("")

    # All metrics table
    lines.append("### All Metrics")
    lines.append("")
    lines.append("| Status | Metric | Baseline | Current | Change |")
    lines.append("|--------|--------|----------|---------|--------|")

    for metric in result.metrics:
        if metric.change_type == MetricChange.REGRESSION:
            status = "Regression"
        elif metric.change_type == MetricChange.IMPROVEMENT:
            status = "Improvement"
        else:
            status = "Unchanged"

        baseline_str = format_value(metric.baseline_value, metric.unit)
        current_str = format_value(metric.current_value, metric.unit)
        change_sign = "+" if metric.change_percent > 0 else ""

        lines.append(
            f"| {status} | {metric.name} | {baseline_str} | {current_str} | "
            f"{change_sign}{metric.change_percent:.1f}% |"
        )

    lines.append("")

    return "\n".join(lines)


def print_console_summary(result: ComparisonResult) -> None:
    """Print a summary of the comparison to the console.

    Args:
        result: The ComparisonResult to summarize.
    """
    print("\n" + "=" * 60)
    print("Benchmark Comparison Results")
    print("=" * 60)
    print(f"Baseline: {result.baseline_file}")
    print(f"Current:  {result.current_file}")
    print(f"Tolerance: {result.tolerance * 100:.0f}%")
    print()

    if result.regressions:
        print("REGRESSIONS DETECTED:")
        print("-" * 40)
        for metric in result.regressions:
            baseline_str = format_value(metric.baseline_value, metric.unit)
            current_str = format_value(metric.current_value, metric.unit)
            change_sign = "+" if metric.change_percent > 0 else ""
            print(f"  {metric.name}")
            print(f"    Baseline: {baseline_str}")
            print(f"    Current:  {current_str}")
            print(f"    Change:   {change_sign}{metric.change_percent:.1f}%")
        print()

    if result.improvements:
        print("IMPROVEMENTS:")
        print("-" * 40)
        for metric in result.improvements:
            baseline_str = format_value(metric.baseline_value, metric.unit)
            current_str = format_value(metric.current_value, metric.unit)
            change_sign = "+" if metric.change_percent > 0 else ""
            print(f"  {metric.name}")
            print(f"    Baseline: {baseline_str}")
            print(f"    Current:  {current_str}")
            print(f"    Change:   {change_sign}{metric.change_percent:.1f}%")
        print()

    # Summary
    print("=" * 60)
    total = len(result.metrics)
    unchanged = total - len(result.regressions) - len(result.improvements)
    print(f"Total metrics compared: {total}")
    print(f"  Regressions:   {len(result.regressions)}")
    print(f"  Improvements:  {len(result.improvements)}")
    print(f"  Unchanged:     {unchanged}")
    print()

    if result.has_regressions:
        print("RESULT: FAILED - Performance regressions detected")
    else:
        print("RESULT: PASSED - No performance regressions detected")


def main() -> int:
    """Main entry point for the benchmark comparison tool.

    Returns:
        Exit code: 0 if no regressions, 1 if regressions found.
    """
    parser = argparse.ArgumentParser(
        description='Compare two benchmark runs and detect performance regressions.',
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  %(prog)s baseline.json current.json
  %(prog)s baseline.json current.json --tolerance 0.15
  %(prog)s baseline.json current.json --output report.md

Exit codes:
  0 - No regressions detected
  1 - Regressions detected (or error)

The script compares these metrics:
  - RT Loop Time (us)
  - P99 Jitter (ms)
  - Missed Tick Rate
  - Processing Time Median (us)
  - Processing Time P99 (us)
  - Per-benchmark percentiles and custom metrics
        """
    )

    parser.add_argument(
        'baseline',
        type=Path,
        help='Path to the baseline benchmark JSON file'
    )
    parser.add_argument(
        'current',
        type=Path,
        help='Path to the current benchmark JSON file'
    )
    parser.add_argument(
        '--tolerance',
        type=float,
        default=0.10,
        help='Tolerance for detecting regressions as a decimal (default: 0.10 = 10%%)'
    )
    parser.add_argument(
        '--output',
        type=Path,
        metavar='FILE',
        help='Write markdown report to FILE'
    )

    args = parser.parse_args()

    # Validate tolerance
    if args.tolerance < 0 or args.tolerance > 1:
        print("Error: Tolerance must be between 0 and 1 (e.g., 0.10 for 10%)")
        return 1

    # Run comparison
    result = compare_benchmarks(args.baseline, args.current, args.tolerance)

    # Print console summary
    print_console_summary(result)

    # Generate and write markdown report if requested
    if args.output:
        report = generate_comparison_report(result)
        try:
            with open(args.output, 'w', encoding='utf-8') as f:
                f.write(report)
            print(f"\nMarkdown report written to: {args.output}")
        except IOError as e:
            print(f"Warning: Could not write report to {args.output}: {e}")

    # Return appropriate exit code
    return 1 if result.has_regressions else 0


if __name__ == '__main__':
    sys.exit(main())

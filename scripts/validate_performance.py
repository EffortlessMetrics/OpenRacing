#!/usr/bin/env python3
"""
Performance gate validation script.
Validates that RT timing benchmarks meet the requirements from NFR-01:
- p99 jitter ≤ 0.25ms at 1kHz
- Missed ticks ≤ 0.001%
- Added E2E latency ≤ 2ms p99
- Processing ≤ 50μs median, ≤ 200μs p99 per tick
"""

import json
import sys
import argparse
import os
from typing import Dict, Any, List

# Performance thresholds from requirements (NFR-01)
MAX_P99_JITTER_US = 250.0  # 0.25ms in microseconds
MAX_MISSED_TICK_RATE = 0.00001  # 0.001%
MAX_E2E_LATENCY_P99_US = 2000.0  # 2ms in microseconds
MAX_PROCESSING_MEDIAN_US = 50.0  # 50μs median
MAX_PROCESSING_P99_US = 200.0  # 200μs p99

def parse_benchmark_results(file_path: str) -> Dict[str, Any]:
    """Parse Criterion benchmark results from JSON."""
    try:
        with open(file_path, 'r') as f:
            data = json.load(f)
        return data
    except (FileNotFoundError, json.JSONDecodeError) as e:
        print(f"Error reading benchmark results: {e}")
        sys.exit(1)

def validate_rt_timing(results: Dict[str, Any]) -> bool:
    """Validate real-time timing requirements from NFR-01."""
    success = True
    
    # Look for RT timing benchmark results
    rt_benchmarks = [
        bench for bench in results.get('benchmarks', [])
        if 'rt_timing' in bench.get('name', '') or '1khz' in bench.get('name', '')
    ]
    
    if not rt_benchmarks:
        print("❌ No RT timing benchmarks found")
        return False
    
    for bench in rt_benchmarks:
        name = bench.get('name', 'unknown')
        print(f"📊 {name}:")
        
        # Check jitter (should be in percentiles)
        percentiles = bench.get('percentiles', {})
        p99_jitter_ns = percentiles.get('p99', 0)
        p99_jitter_us = p99_jitter_ns / 1000.0  # Convert ns to us
        
        print(f"   P99 jitter: {p99_jitter_us:.2f}μs (limit: {MAX_P99_JITTER_US}μs)")
        
        if p99_jitter_us > MAX_P99_JITTER_US:
            print(f"❌ P99 jitter exceeds limit: {p99_jitter_us:.2f}μs > {MAX_P99_JITTER_US}μs")
            success = False
        else:
            print(f"✅ P99 jitter within limits")
        
        # Check processing time
        median_ns = percentiles.get('p50', 0)
        median_us = median_ns / 1000.0
        p99_processing_ns = percentiles.get('p99', 0)
        p99_processing_us = p99_processing_ns / 1000.0
        
        print(f"   Processing median: {median_us:.2f}μs (limit: {MAX_PROCESSING_MEDIAN_US}μs)")
        print(f"   Processing p99: {p99_processing_us:.2f}μs (limit: {MAX_PROCESSING_P99_US}μs)")
        
        if median_us > MAX_PROCESSING_MEDIAN_US:
            print(f"❌ Processing median exceeds limit: {median_us:.2f}μs > {MAX_PROCESSING_MEDIAN_US}μs")
            success = False
        
        if p99_processing_us > MAX_PROCESSING_P99_US:
            print(f"❌ Processing p99 exceeds limit: {p99_processing_us:.2f}μs > {MAX_PROCESSING_P99_US}μs")
            success = False
        
        # Check missed tick rate (custom metric)
        custom_metrics = bench.get('custom_metrics', {})
        missed_ticks = custom_metrics.get('missed_tick_rate', 0)
        print(f"   Missed tick rate: {missed_ticks:.6f}% (limit: {MAX_MISSED_TICK_RATE:.6f}%)")
        
        if missed_ticks > MAX_MISSED_TICK_RATE:
            print(f"❌ Missed tick rate exceeds limit: {missed_ticks:.6f}% > {MAX_MISSED_TICK_RATE:.6f}%")
            success = False
        else:
            print(f"✅ Missed tick rate within limits")
        
        # Check E2E latency if available
        e2e_latency_us = custom_metrics.get('e2e_latency_p99_us', 0)
        if e2e_latency_us > 0:
            print(f"   E2E latency p99: {e2e_latency_us:.2f}μs (limit: {MAX_E2E_LATENCY_P99_US}μs)")
            if e2e_latency_us > MAX_E2E_LATENCY_P99_US:
                print(f"❌ E2E latency exceeds limit: {e2e_latency_us:.2f}μs > {MAX_E2E_LATENCY_P99_US}μs")
                success = False
            else:
                print(f"✅ E2E latency within limits")
    
    return success

def validate_memory_usage(results: Dict[str, Any]) -> bool:
    """Validate memory usage requirements."""
    success = True
    
    # Look for memory benchmarks
    memory_benchmarks = [
        bench for bench in results.get('benchmarks', [])
        if 'memory' in bench.get('name', '')
    ]
    
    for bench in memory_benchmarks:
        name = bench.get('name', 'unknown')
        
        # Check for heap allocations in RT path
        rt_allocs = bench.get('custom_metrics', {}).get('rt_heap_allocs', 0)
        
        print(f"📊 {name}:")
        print(f"   RT heap allocations: {rt_allocs}")
        
        if rt_allocs > 0:
            print(f"❌ RT path has heap allocations: {rt_allocs}")
            success = False
        else:
            print(f"✅ No RT heap allocations")
    
    return success

def generate_performance_report(results: Dict[str, Any], output_file: str = None):
    """Generate a detailed performance report."""
    report_lines = [
        "# Racing Wheel Software Performance Report",
        f"Generated: {os.popen('date').read().strip()}",
        "",
        "## Performance Gates Status",
        ""
    ]
    
    # Add benchmark results summary
    for bench in results.get('benchmarks', []):
        name = bench.get('name', 'unknown')
        percentiles = bench.get('percentiles', {})
        
        report_lines.extend([
            f"### {name}",
            f"- Mean: {percentiles.get('p50', 0)/1000:.2f}μs",
            f"- P99: {percentiles.get('p99', 0)/1000:.2f}μs",
            f"- Samples: {bench.get('sample_count', 0)}",
            ""
        ])
    
    report_content = "\n".join(report_lines)
    
    if output_file:
        with open(output_file, 'w') as f:
            f.write(report_content)
        print(f"📄 Performance report written to {output_file}")
    else:
        print("\n" + report_content)

def main():
    parser = argparse.ArgumentParser(description='Validate performance gate requirements (NFR-01)')
    parser.add_argument('benchmark_file', help='Path to benchmark results JSON file')
    parser.add_argument('--strict', action='store_true', help='Fail on any performance regression')
    parser.add_argument('--report', help='Generate performance report to file')
    parser.add_argument('--verbose', '-v', action='store_true', help='Verbose output')
    
    args = parser.parse_args()
    
    print("🚀 Validating performance gates (NFR-01: RT timing requirements)...")
    
    if not os.path.exists(args.benchmark_file):
        print(f"❌ Benchmark file not found: {args.benchmark_file}")
        sys.exit(1)
    
    results = parse_benchmark_results(args.benchmark_file)
    
    if args.verbose:
        print(f"📊 Found {len(results.get('benchmarks', []))} benchmark results")
    
    timing_ok = validate_rt_timing(results)
    memory_ok = validate_memory_usage(results)
    
    if args.report:
        generate_performance_report(results, args.report)
    
    if timing_ok and memory_ok:
        print("\n✅ All performance gates passed!")
        print("   - P99 jitter ≤ 0.25ms ✓")
        print("   - Missed ticks ≤ 0.001% ✓") 
        print("   - No RT heap allocations ✓")
        sys.exit(0)
    else:
        print("\n❌ Performance gate validation failed!")
        print("   See requirements NFR-01 for timing specifications")
        sys.exit(1)

if __name__ == '__main__':
    main()
#!/usr/bin/env python3
"""
Track compilation progress by comparing current state to baseline.
"""

import json
import subprocess
from pathlib import Path
from datetime import datetime

def load_baseline():
    """Load the baseline from file."""
    baseline_path = Path("docs/build/compile-baseline.json")
    if not baseline_path.exists():
        print("No baseline found. Run create_error_baseline.py first.")
        return None
    
    with open(baseline_path) as f:
        return json.load(f)

def get_current_state():
    """Get current compilation state."""
    # Import the baseline creation function
    import sys
    sys.path.append('scripts')
    from create_error_baseline import create_baseline
    
    return create_baseline()

def compare_states(baseline, current):
    """Compare current state to baseline."""
    comparison = {
        "timestamp": datetime.now().isoformat(),
        "baseline_errors": baseline["total_errors"],
        "current_errors": current["total_errors"],
        "error_delta": current["total_errors"] - baseline["total_errors"],
        "baseline_warnings": baseline["total_warnings"],
        "current_warnings": current["total_warnings"],
        "warning_delta": current["total_warnings"] - baseline["total_warnings"],
        "progress": {},
        "regressions": {},
        "improvements": {}
    }
    
    # Compare by crate
    for crate in baseline["crate_errors"]:
        baseline_count = len(baseline["crate_errors"][crate]["errors"])
        current_count = len(current["crate_errors"].get(crate, {}).get("errors", []))
        
        delta = current_count - baseline_count
        
        if delta < 0:
            comparison["improvements"][crate] = abs(delta)
        elif delta > 0:
            comparison["regressions"][crate] = delta
        
        comparison["progress"][crate] = {
            "baseline": baseline_count,
            "current": current_count,
            "delta": delta
        }
    
    return comparison

def main():
    print("=== COMPILATION PROGRESS TRACKER ===")
    print()
    
    baseline = load_baseline()
    if not baseline:
        return 1
    
    print("Loading current state...")
    current = get_current_state()
    
    comparison = compare_states(baseline, current)
    
    print(f"Baseline: {comparison['baseline_errors']} errors, {comparison['baseline_warnings']} warnings")
    print(f"Current:  {comparison['current_errors']} errors, {comparison['current_warnings']} warnings")
    print(f"Delta:    {comparison['error_delta']:+d} errors, {comparison['warning_delta']:+d} warnings")
    print()
    
    if comparison["improvements"]:
        print("✅ IMPROVEMENTS:")
        for crate, reduction in comparison["improvements"].items():
            print(f"  {crate}: -{reduction} errors")
        print()
    
    if comparison["regressions"]:
        print("❌ REGRESSIONS:")
        for crate, increase in comparison["regressions"].items():
            print(f"  {crate}: +{increase} errors")
        print()
    
    print("📊 CRATE STATUS:")
    for crate, progress in comparison["progress"].items():
        status = "✓" if progress["current"] == 0 else "✗"
        delta_str = f"({progress['delta']:+d})" if progress["delta"] != 0 else ""
        print(f"  {status} {crate}: {progress['current']} errors {delta_str}")
    
    # Save comparison
    output_dir = Path("docs/build")
    with open(output_dir / "compile-progress.json", 'w') as f:
        json.dump(comparison, f, indent=2)
    
    print(f"\nProgress saved to {output_dir / 'compile-progress.json'}")
    
    # Return success if no errors
    return 0 if comparison["current_errors"] == 0 else 1

if __name__ == "__main__":
    exit(main())
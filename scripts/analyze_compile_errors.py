#!/usr/bin/env python3
"""
Analyze compilation errors from cargo JSON output and create a baseline report.
"""

import json
import sys
from collections import defaultdict
from pathlib import Path

def analyze_compile_json(json_file):
    """Parse cargo JSON output and extract error information."""
    errors_by_crate = defaultdict(list)
    warnings_by_crate = defaultdict(list)
    
    try:
        with open(json_file, 'r') as f:
            for line in f:
                try:
                    data = json.loads(line.strip())
                    
                    # Look for compiler messages
                    if data.get('reason') == 'compiler-message':
                        message = data.get('message', {})
                        level = message.get('level', '')
                        text = message.get('message', '')
                        
                        # Extract crate name from target
                        target = data.get('target', {})
                        crate_name = target.get('name', 'unknown')
                        
                        if level == 'error':
                            errors_by_crate[crate_name].append(text)
                        elif level == 'warning':
                            warnings_by_crate[crate_name].append(text)
                            
                except json.JSONDecodeError:
                    # Skip non-JSON lines (like cargo output)
                    continue
                    
    except FileNotFoundError:
        print(f"File {json_file} not found")
        return {}, {}
        
    return dict(errors_by_crate), dict(warnings_by_crate)

def create_baseline_report(errors, warnings):
    """Create a baseline report with error counts and categories."""
    baseline = {
        "timestamp": "2024-09-30T00:00:00Z",
        "total_errors": sum(len(errs) for errs in errors.values()),
        "total_warnings": sum(len(warns) for warns in warnings.values()),
        "errors_by_crate": {},
        "warnings_by_crate": {},
        "error_categories": {
            "dependency_issues": 0,
            "type_mismatches": 0,
            "missing_imports": 0,
            "async_trait_issues": 0,
            "other": 0
        }
    }
    
    # Count errors by crate
    for crate, error_list in errors.items():
        baseline["errors_by_crate"][crate] = len(error_list)
        
        # Categorize errors
        for error in error_list:
            error_lower = error.lower()
            if any(keyword in error_lower for keyword in ['dependency', 'version', 'conflict']):
                baseline["error_categories"]["dependency_issues"] += 1
            elif any(keyword in error_lower for keyword in ['type', 'mismatch', 'expected']):
                baseline["error_categories"]["type_mismatches"] += 1
            elif any(keyword in error_lower for keyword in ['import', 'module', 'not found']):
                baseline["error_categories"]["missing_imports"] += 1
            elif any(keyword in error_lower for keyword in ['async', 'trait', 'future']):
                baseline["error_categories"]["async_trait_issues"] += 1
            else:
                baseline["error_categories"]["other"] += 1
    
    # Count warnings by crate
    for crate, warning_list in warnings.items():
        baseline["warnings_by_crate"][crate] = len(warning_list)
    
    return baseline

def main():
    # Analyze the main compilation output
    errors, warnings = analyze_compile_json("target/compile.json")
    
    # Also check individual crate outputs if they exist
    for crate_file in ["target/schemas-errors.json", "target/cli-errors.json", "target/service-errors.json"]:
        if Path(crate_file).exists():
            crate_errors, crate_warnings = analyze_compile_json(crate_file)
            errors.update(crate_errors)
            warnings.update(crate_warnings)
    
    # Create baseline report
    baseline = create_baseline_report(errors, warnings)
    
    # Output the baseline
    print("=== COMPILATION ERROR BASELINE ===")
    print(f"Total Errors: {baseline['total_errors']}")
    print(f"Total Warnings: {baseline['total_warnings']}")
    print()
    
    print("Errors by Crate:")
    for crate, count in baseline["errors_by_crate"].items():
        print(f"  {crate}: {count}")
    print()
    
    print("Error Categories:")
    for category, count in baseline["error_categories"].items():
        print(f"  {category}: {count}")
    print()
    
    print("Warnings by Crate:")
    for crate, count in baseline["warnings_by_crate"].items():
        print(f"  {crate}: {count}")
    
    # Save to JSON file
    output_dir = Path("docs/build")
    output_dir.mkdir(parents=True, exist_ok=True)
    
    with open(output_dir / "compile-baseline.json", 'w') as f:
        json.dump(baseline, f, indent=2)
    
    print(f"\nBaseline saved to {output_dir / 'compile-baseline.json'}")
    
    # Also save detailed error information
    detailed_report = {
        "errors": errors,
        "warnings": warnings,
        "baseline": baseline
    }
    
    with open(output_dir / "compile-errors-detailed.json", 'w') as f:
        json.dump(detailed_report, f, indent=2)
    
    print(f"Detailed report saved to {output_dir / 'compile-errors-detailed.json'}")

if __name__ == "__main__":
    main()
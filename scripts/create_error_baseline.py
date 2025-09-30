#!/usr/bin/env python3
"""
Create a compilation error baseline by attempting to build each crate individually
and capturing the specific errors.
"""

import subprocess
import json
import sys
from pathlib import Path
from datetime import datetime

def run_cargo_check(crate_name=None, capture_json=True):
    """Run cargo check and capture output."""
    cmd = ["cargo", "check"]
    if crate_name:
        cmd.extend(["-p", crate_name])
    
    if capture_json:
        cmd.extend(["--message-format=json"])
    
    try:
        result = subprocess.run(
            cmd, 
            capture_output=True, 
            text=True, 
            timeout=300  # 5 minute timeout
        )
        return result.returncode, result.stdout, result.stderr
    except subprocess.TimeoutExpired:
        return -1, "", "Timeout expired"
    except Exception as e:
        return -1, "", str(e)

def parse_cargo_errors(stdout, stderr):
    """Parse cargo output to extract errors and warnings."""
    errors = []
    warnings = []
    
    # Parse JSON output
    for line in stdout.split('\n'):
        if line.strip():
            try:
                data = json.loads(line)
                if data.get('reason') == 'compiler-message':
                    message = data.get('message', {})
                    level = message.get('level', '')
                    text = message.get('message', '')
                    
                    if level == 'error':
                        errors.append(text)
                    elif level == 'warning':
                        warnings.append(text)
            except json.JSONDecodeError:
                continue
    
    # Also parse stderr for build script errors
    stderr_lines = stderr.split('\n')
    for line in stderr_lines:
        if 'error:' in line.lower():
            errors.append(line.strip())
        elif 'warning:' in line.lower():
            warnings.append(line.strip())
    
    return errors, warnings

def categorize_error(error_text):
    """Categorize an error based on its content."""
    error_lower = error_text.lower()
    
    if any(keyword in error_lower for keyword in ['cmake', 'nasm', 'build command', 'aws-lc-sys']):
        return 'build_dependencies'
    elif any(keyword in error_lower for keyword in ['dependency', 'version', 'conflict']):
        return 'dependency_skew'
    elif any(keyword in error_lower for keyword in ['type', 'mismatch', 'expected', 'found']):
        return 'type_mismatch'
    elif any(keyword in error_lower for keyword in ['import', 'module', 'not found', 'unresolved']):
        return 'missing_imports'
    elif any(keyword in error_lower for keyword in ['async', 'trait', 'future', 'lifetime']):
        return 'async_trait_issues'
    elif any(keyword in error_lower for keyword in ['field', 'struct', 'missing']):
        return 'missing_fields'
    else:
        return 'other'

def create_baseline():
    """Create a comprehensive error baseline."""
    baseline = {
        "timestamp": datetime.now().isoformat(),
        "workspace_errors": {},
        "crate_errors": {},
        "error_categories": {
            "build_dependencies": 0,
            "dependency_skew": 0,
            "type_mismatch": 0,
            "missing_imports": 0,
            "async_trait_issues": 0,
            "missing_fields": 0,
            "other": 0
        },
        "total_errors": 0,
        "total_warnings": 0,
        "reproducible": True
    }
    
    # List of crates to check
    crates = [
        "racing-wheel-schemas",
        "racing-wheel-engine", 
        "racing-wheel-service",
        "wheelctl",
        "racing-wheel-ui",
        "racing-wheel-plugins",
        "racing-wheel-integration-tests",
        "racing-wheel-compat"
    ]
    
    print("=== CREATING COMPILATION ERROR BASELINE ===")
    print()
    
    # First, try workspace build
    print("Checking workspace build...")
    returncode, stdout, stderr = run_cargo_check()
    workspace_errors, workspace_warnings = parse_cargo_errors(stdout, stderr)
    
    baseline["workspace_errors"] = {
        "returncode": returncode,
        "errors": workspace_errors,
        "warnings": workspace_warnings
    }
    
    print(f"Workspace build: {len(workspace_errors)} errors, {len(workspace_warnings)} warnings")
    
    # Then check individual crates
    for crate in crates:
        print(f"Checking crate: {crate}")
        returncode, stdout, stderr = run_cargo_check(crate)
        errors, warnings = parse_cargo_errors(stdout, stderr)
        
        baseline["crate_errors"][crate] = {
            "returncode": returncode,
            "errors": errors,
            "warnings": warnings
        }
        
        print(f"  {crate}: {len(errors)} errors, {len(warnings)} warnings")
        
        # Categorize errors
        for error in errors:
            category = categorize_error(error)
            baseline["error_categories"][category] += 1
            baseline["total_errors"] += 1
        
        baseline["total_warnings"] += len(warnings)
    
    return baseline

def main():
    baseline = create_baseline()
    
    print()
    print("=== BASELINE SUMMARY ===")
    print(f"Total Errors: {baseline['total_errors']}")
    print(f"Total Warnings: {baseline['total_warnings']}")
    print()
    
    print("Error Categories:")
    for category, count in baseline["error_categories"].items():
        if count > 0:
            print(f"  {category}: {count}")
    print()
    
    print("Crate Status:")
    for crate, data in baseline["crate_errors"].items():
        status = "✓" if data["returncode"] == 0 else "✗"
        print(f"  {status} {crate}: {len(data['errors'])} errors")
    
    # Save baseline
    output_dir = Path("docs/build")
    output_dir.mkdir(parents=True, exist_ok=True)
    
    with open(output_dir / "compile-baseline.json", 'w') as f:
        json.dump(baseline, f, indent=2)
    
    print(f"\nBaseline saved to {output_dir / 'compile-baseline.json'}")
    
    # Create a summary for tracking progress
    summary = {
        "total_errors": baseline["total_errors"],
        "total_warnings": baseline["total_warnings"],
        "error_categories": baseline["error_categories"],
        "crates_with_errors": sum(1 for data in baseline["crate_errors"].values() if data["returncode"] != 0),
        "total_crates": len(baseline["crate_errors"])
    }
    
    with open(output_dir / "compile-summary.json", 'w') as f:
        json.dump(summary, f, indent=2)
    
    print(f"Summary saved to {output_dir / 'compile-summary.json'}")
    
    return baseline["total_errors"] == 0

if __name__ == "__main__":
    success = main()
    sys.exit(0 if success else 1)
#!/usr/bin/env python3
"""
Check for API violations in integration tests.
This script enforces public API usage and prevents regression.
"""

import os
import re
import sys
from pathlib import Path

def check_api_violations():
    """Check for various API violations."""
    violations = []
    
    # Check 1: Private module imports
    violations.extend(check_private_imports())
    
    # Check 2: Deprecated field names
    violations.extend(check_deprecated_fields())
    
    # Check 3: Glob re-exports (if any)
    violations.extend(check_glob_reexports())
    
    if violations:
        print("❌ Found API violations:")
        for violation in violations:
            print(f"  {violation}")
        return 1
    else:
        print("✅ No API violations found")
        return 0

def check_private_imports():
    """Check for private module imports."""
    violations = []
    integration_tests_dir = Path("crates/integration-tests")
    
    if not integration_tests_dir.exists():
        return violations
    
    private_patterns = [
        r'use\s+.*::(tests|internal|private)',
    ]
    
    for rust_file in integration_tests_dir.rglob("*.rs"):
        with open(rust_file, 'r', encoding='utf-8') as f:
            content = f.read()
            
        for line_num, line in enumerate(content.splitlines(), 1):
            for pattern in private_patterns:
                if re.search(pattern, line):
                    violations.append(f"{rust_file}:{line_num}: Private import: {line.strip()}")
    
    return violations

def check_deprecated_fields():
    """Check for deprecated field names."""
    violations = []
    integration_tests_dir = Path("crates/integration-tests")
    
    if not integration_tests_dir.exists():
        return violations
    
    # Deprecated field patterns
    deprecated_patterns = [
        r'\bwheel_angle_mdeg\b',
        r'\btemp_c\b',
        r'\bsequence\b',
        r'\.faults\b',  # Should be fault_flags
    ]
    
    for rust_file in integration_tests_dir.rglob("*.rs"):
        with open(rust_file, 'r', encoding='utf-8') as f:
            content = f.read()
            
        for line_num, line in enumerate(content.splitlines(), 1):
            for pattern in deprecated_patterns:
                if re.search(pattern, line):
                    violations.append(f"{rust_file}:{line_num}: Deprecated field: {line.strip()}")
    
    return violations

def check_glob_reexports():
    """Check for glob re-exports."""
    violations = []
    integration_tests_dir = Path("crates/integration-tests")
    
    if not integration_tests_dir.exists():
        return violations
    
    glob_pattern = r'pub\s+use\s+.*::\*'
    
    for rust_file in integration_tests_dir.rglob("*.rs"):
        with open(rust_file, 'r', encoding='utf-8') as f:
            content = f.read()
            
        for line_num, line in enumerate(content.splitlines(), 1):
            if re.search(glob_pattern, line):
                violations.append(f"{rust_file}:{line_num}: Glob re-export: {line.strip()}")
    
    return violations

if __name__ == "__main__":
    sys.exit(check_api_violations())
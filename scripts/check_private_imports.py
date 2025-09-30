#!/usr/bin/env python3
"""
Check for private module imports in integration tests.
This script enforces that integration tests only use public APIs.
"""

import os
import re
import sys
from pathlib import Path

def check_private_imports():
    """Check for private module imports in integration tests."""
    integration_tests_dir = Path("crates/integration-tests")
    
    if not integration_tests_dir.exists():
        print("Integration tests directory not found")
        return 1
    
    # Patterns that indicate private module access
    private_patterns = [
        r'use\s+.*::(tests|internal|private)',
        r'use\s+crate::.*::(tests|internal|private)',
    ]
    
    violations = []
    
    for rust_file in integration_tests_dir.rglob("*.rs"):
        with open(rust_file, 'r', encoding='utf-8') as f:
            content = f.read()
            
        for line_num, line in enumerate(content.splitlines(), 1):
            for pattern in private_patterns:
                if re.search(pattern, line):
                    violations.append(f"{rust_file}:{line_num}: {line.strip()}")
    
    if violations:
        print("❌ Found private module imports in integration tests:")
        for violation in violations:
            print(f"  {violation}")
        return 1
    else:
        print("✅ No private module imports found in integration tests")
        return 0

if __name__ == "__main__":
    sys.exit(check_private_imports())
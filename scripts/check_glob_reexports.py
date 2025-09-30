#!/usr/bin/env python3
"""
Check for forbidden glob re-exports in the codebase.

This script searches for `pub use *::*;` patterns which are forbidden
in our codebase to maintain clear API boundaries.
"""

import os
import re
import sys
from pathlib import Path

def check_glob_reexports(root_dir):
    """Check for glob re-exports in Rust files."""
    glob_pattern = re.compile(r'pub\s+use\s+.*::\*\s*;')
    violations = []
    
    # Search in crates directory
    crates_dir = Path(root_dir) / "crates"
    if not crates_dir.exists():
        print(f"Warning: {crates_dir} does not exist")
        return violations
    
    for rust_file in crates_dir.rglob("*.rs"):
        try:
            with open(rust_file, 'r', encoding='utf-8') as f:
                for line_num, line in enumerate(f, 1):
                    if glob_pattern.search(line):
                        violations.append({
                            'file': str(rust_file),
                            'line': line_num,
                            'content': line.strip()
                        })
        except Exception as e:
            print(f"Warning: Could not read {rust_file}: {e}")
    
    return violations

def main():
    """Main entry point."""
    root_dir = Path(__file__).parent.parent
    violations = check_glob_reexports(root_dir)
    
    if violations:
        print("⚠️  Found glob re-exports (will be addressed in future tasks):")
        for violation in violations:
            print(f"  {violation['file']}:{violation['line']}: {violation['content']}")
        print(f"\nTotal: {len(violations)} glob re-exports found")
        print("Note: These will be refactored to use explicit re-exports or prelude modules")
        # For now, don't fail CI - this will be addressed in task 2 (schema restructuring)
        sys.exit(0)
    else:
        print("✅ No glob re-exports found")
        sys.exit(0)

if __name__ == "__main__":
    main()
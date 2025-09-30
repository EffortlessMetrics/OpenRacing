#!/usr/bin/env python3
"""
Track compatibility layer usage count to ensure it doesn't increase over time.

This script searches for usage of the TelemetryCompat trait methods in the codebase
and reports the count. It's designed to be used in CI to ensure that compat usage
trends downward as code is migrated to use new field names.
"""

import os
import re
import sys
import json
from pathlib import Path

# Compatibility method patterns to search for
COMPAT_PATTERNS = [
    r'\.temp_c\(\)',           # Old temperature field
    r'\.faults\(\)',           # Old faults field  
    r'\.wheel_angle_mdeg\(\)', # Old wheel angle field
    r'\.wheel_speed_mrad_s\(\)', # Old wheel speed field
    r'\.sequence\(\)',         # Removed sequence field
]

def count_compat_usage(root_dir):
    """Count usage of compatibility methods in the codebase."""
    usage_count = 0
    usage_details = []
    
    # Search in crates directory (exclude compat crate itself)
    crates_dir = Path(root_dir) / 'crates'
    
    for rust_file in crates_dir.rglob('*.rs'):
        # Skip the compat crate itself
        if 'compat' in rust_file.parts:
            continue
            
        try:
            with open(rust_file, 'r', encoding='utf-8') as f:
                content = f.read()
                
            for line_num, line in enumerate(content.splitlines(), 1):
                for pattern in COMPAT_PATTERNS:
                    matches = re.findall(pattern, line)
                    if matches:
                        usage_count += len(matches)
                        for match in matches:
                            usage_details.append({
                                'file': str(rust_file.relative_to(root_dir)),
                                'line': line_num,
                                'method': match,
                                'context': line.strip()
                            })
                            
        except (UnicodeDecodeError, IOError) as e:
            print(f"Warning: Could not read {rust_file}: {e}", file=sys.stderr)
            continue
    
    return usage_count, usage_details

def main():
    """Main entry point."""
    if len(sys.argv) > 1:
        root_dir = sys.argv[1]
    else:
        root_dir = os.getcwd()
    
    usage_count, usage_details = count_compat_usage(root_dir)
    
    # Output results
    print(f"Compatibility layer usage count: {usage_count}")
    
    if usage_details:
        print("\nUsage details:")
        for detail in usage_details:
            print(f"  {detail['file']}:{detail['line']} - {detail['method']} in: {detail['context']}")
    
    # Output JSON for CI consumption
    result = {
        'usage_count': usage_count,
        'usage_details': usage_details
    }
    
    # Write to file for CI artifact
    with open('compat_usage_report.json', 'w') as f:
        json.dump(result, f, indent=2)
    
    print(f"\nDetailed report written to compat_usage_report.json")
    
    # Exit with count as exit code (for CI thresholds)
    return min(usage_count, 255)  # Cap at 255 for shell exit codes

if __name__ == '__main__':
    sys.exit(main())
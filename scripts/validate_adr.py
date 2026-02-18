#!/usr/bin/env python3
"""
ADR validation script.
Validates that ADR files follow the required format and reference requirements properly.
"""

import os
import re
import sys
import argparse
from pathlib import Path
from typing import List, Dict, Set

def find_adr_files(adr_dir: Path) -> List[Path]:
    """Find all ADR files in the directory."""
    adr_files = []
    for file_path in adr_dir.glob("*.md"):
        if file_path.name != "template.md" and file_path.name != "README.md":
            if re.match(r'\d{4}-.*\.md', file_path.name):
                adr_files.append(file_path)
    return sorted(adr_files)

def validate_adr_format(adr_path: Path) -> List[str]:
    """Validate that an ADR follows the required format."""
    errors = []
    
    try:
        content = adr_path.read_text(encoding='utf-8')
    except Exception as e:
        return [f"Could not read file: {e}"]
    
    lines = content.split('\n')
    
    # Check for required sections
    required_sections = [
        r'^# ADR-\d{4}: .+',  # Title
        r'^\*\*Status:\*\*',   # Status
        r'^\*\*Date:\*\*',     # Date
        r'^\*\*Authors:\*\*',  # Authors
        r'## Context',         # Context section
        r'## Decision',        # Decision section
        r'## Rationale',       # Rationale section
        r'## Consequences',    # Consequences section
        r'## References',      # References section
    ]
    
    found_sections = set()
    for line in lines:
        for i, pattern in enumerate(required_sections):
            if re.match(pattern, line):
                found_sections.add(i)
    
    missing_sections = []
    section_names = [
        "Title (# ADR-XXXX: Title)",
        "Status metadata",
        "Date metadata", 
        "Authors metadata",
        "Context section",
        "Decision section",
        "Rationale section",
        "Consequences section",
        "References section"
    ]
    
    for i, section in enumerate(section_names):
        if i not in found_sections:
            missing_sections.append(section)
    
    if missing_sections:
        errors.append(f"Missing required sections: {', '.join(missing_sections)}")
    
    # Check status values
    status_line = next((line for line in lines if line.startswith('**Status:**')), None)
    if status_line:
        valid_statuses = ['Proposed', 'Accepted', 'Deprecated', 'Superseded']
        status_match = re.search(r'\*\*Status:\*\* (.+)', status_line)
        if status_match:
            status = status_match.group(1).strip()
            if status not in valid_statuses:
                errors.append(f"Invalid status '{status}'. Must be one of: {', '.join(valid_statuses)}")
    
    # Check for requirement references
    if 'Requirements:' not in content:
        errors.append("No requirement references found. ADRs should reference specific requirement IDs.")
    
    return errors

def extract_requirement_references(adr_path: Path) -> Set[str]:
    """Extract requirement IDs referenced in the ADR."""
    try:
        content = adr_path.read_text(encoding='utf-8')
    except:
        return set()
    
    # Look for requirement patterns like NFR-01, FFB-02, etc.
    req_pattern = r'\b([A-Z]{2,}-\d{2})\b'
    requirements = set(re.findall(req_pattern, content))
    return requirements

def validate_requirement_references(adr_files: List[Path], requirements_file: Path) -> Dict[str, List[str]]:
    """Validate that ADRs reference valid requirements."""
    errors = {}
    
    if not requirements_file.exists():
        return {"global": ["Requirements file not found"]}
    
    try:
        req_content = requirements_file.read_text(encoding='utf-8')
        # Extract all requirement IDs from requirements.md
        valid_reqs = set(re.findall(r'\b([A-Z]{2,}-\d{2})\b', req_content))
    except Exception as e:
        return {"global": [f"Could not read requirements file: {e}"]}
    
    for adr_path in adr_files:
        adr_errors = []
        referenced_reqs = extract_requirement_references(adr_path)
        
        for req_id in referenced_reqs:
            if req_id not in valid_reqs:
                adr_errors.append(f"References invalid requirement: {req_id}")
        
        if adr_errors:
            errors[adr_path.name] = adr_errors
    
    return errors

def main():
    parser = argparse.ArgumentParser(description='Validate ADR format and content')
    parser.add_argument('--adr-dir', default='docs/adr', help='Path to ADR directory')
    parser.add_argument('--requirements', default='.kiro/specs/racing-wheel-suite/requirements.md', 
                       help='Path to requirements file')
    parser.add_argument('--verbose', '-v', action='store_true', help='Verbose output')
    
    args = parser.parse_args()
    
    adr_dir = Path(args.adr_dir)
    requirements_file = Path(args.requirements)
    
    if not adr_dir.exists():
        print(f"[ERROR] ADR directory not found: {adr_dir}")
        sys.exit(1)

    print("[INFO] Validating ADR files...")
    
    adr_files = find_adr_files(adr_dir)
    
    if not adr_files:
        print("[ERROR] No ADR files found")
        sys.exit(1)

    if args.verbose:
        print(f"[INFO] Found {len(adr_files)} ADR files")
    
    total_errors = 0
    
    # Validate format
    for adr_path in adr_files:
        errors = validate_adr_format(adr_path)
        if errors:
            print(f"\n[ERROR] {adr_path.name}:")
            for error in errors:
                print(f"   - {error}")
            total_errors += len(errors)
        elif args.verbose:
            print(f"[OK] {adr_path.name}: Format OK")
    
    # Validate requirement references
    req_errors = validate_requirement_references(adr_files, requirements_file)
    for file_name, errors in req_errors.items():
        if errors:
            print(f"\n[ERROR] {file_name} (requirements):")
            for error in errors:
                print(f"   - {error}")
            total_errors += len(errors)
    
    if total_errors == 0:
        print(f"\n[OK] All {len(adr_files)} ADR files are valid!")
        sys.exit(0)
    else:
        print(f"\n[ERROR] Found {total_errors} validation errors")
        sys.exit(1)

if __name__ == '__main__':
    main()

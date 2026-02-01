#!/usr/bin/env python3
"""
CHANGELOG validation script.
Validates that CHANGELOG.md follows the Keep a Changelog format.

This script checks:
1. CHANGELOG.md file exists
2. Required header and format description
3. [Unreleased] section exists
4. Version entries follow semantic versioning
5. Dates are in ISO 8601 format (YYYY-MM-DD)
6. Required sections (Added, Changed, Deprecated, Removed, Fixed, Security)
"""

import os
import re
import sys
import argparse
from pathlib import Path
from typing import List, Tuple, Optional


def validate_changelog_exists(changelog_path: Path) -> List[str]:
    """Check that CHANGELOG.md exists."""
    if not changelog_path.exists():
        return [f"CHANGELOG.md not found at: {changelog_path}"]
    return []


def validate_header(content: str) -> List[str]:
    """Validate the changelog header and format description."""
    errors = []
    lines = content.split('\n')
    
    # Check for title
    if not lines or not lines[0].strip().startswith('# Changelog'):
        errors.append("Missing '# Changelog' title at the beginning of the file")
    
    # Check for Keep a Changelog reference
    if 'Keep a Changelog' not in content:
        errors.append("Missing reference to Keep a Changelog format")
    
    # Check for Semantic Versioning reference
    if 'Semantic Versioning' not in content:
        errors.append("Missing reference to Semantic Versioning")
    
    return errors


def validate_unreleased_section(content: str) -> List[str]:
    """Validate that [Unreleased] section exists."""
    errors = []
    
    # Look for [Unreleased] section
    unreleased_pattern = r'^## \[Unreleased\]'
    if not re.search(unreleased_pattern, content, re.MULTILINE):
        errors.append("Missing [Unreleased] section")
    
    return errors


def extract_version_entries(content: str) -> List[Tuple[str, str, int]]:
    """Extract version entries from the changelog.
    
    Returns list of (version, date, line_number) tuples.
    """
    entries = []
    lines = content.split('\n')
    
    # Pattern for version headers: ## [X.Y.Z] - YYYY-MM-DD
    version_pattern = r'^## \[(\d+\.\d+\.\d+(?:-[a-zA-Z0-9.]+)?)\]\s*-\s*(\d{4}-\d{2}-\d{2})'
    
    for i, line in enumerate(lines, 1):
        match = re.match(version_pattern, line)
        if match:
            version = match.group(1)
            date = match.group(2)
            entries.append((version, date, i))
    
    return entries


def validate_version_format(version: str, line_num: int) -> List[str]:
    """Validate that version follows semantic versioning."""
    errors = []
    
    # Basic semver pattern (with optional prerelease)
    semver_pattern = r'^\d+\.\d+\.\d+(-[a-zA-Z0-9.]+)?$'
    if not re.match(semver_pattern, version):
        errors.append(f"Line {line_num}: Invalid version format '{version}'. Expected semantic versioning (X.Y.Z or X.Y.Z-prerelease)")
    
    return errors


def validate_date_format(date: str, line_num: int) -> List[str]:
    """Validate that date is in ISO 8601 format (YYYY-MM-DD)."""
    errors = []
    
    # ISO 8601 date pattern
    iso_date_pattern = r'^\d{4}-\d{2}-\d{2}$'
    if not re.match(iso_date_pattern, date):
        errors.append(f"Line {line_num}: Invalid date format '{date}'. Expected ISO 8601 format (YYYY-MM-DD)")
        return errors
    
    # Validate date components
    try:
        year, month, day = map(int, date.split('-'))
        if month < 1 or month > 12:
            errors.append(f"Line {line_num}: Invalid month '{month}' in date '{date}'")
        if day < 1 or day > 31:
            errors.append(f"Line {line_num}: Invalid day '{day}' in date '{date}'")
    except ValueError:
        errors.append(f"Line {line_num}: Could not parse date '{date}'")
    
    return errors


def validate_version_entries(content: str) -> List[str]:
    """Validate all version entries in the changelog."""
    errors = []
    
    entries = extract_version_entries(content)
    
    if not entries:
        errors.append("No version entries found. At least one version entry is expected.")
        return errors
    
    for version, date, line_num in entries:
        errors.extend(validate_version_format(version, line_num))
        errors.extend(validate_date_format(date, line_num))
    
    return errors


def validate_section_format(content: str) -> List[str]:
    """Validate that change sections use correct headers."""
    errors = []
    
    # Valid section headers per Keep a Changelog
    valid_sections = {'Added', 'Changed', 'Deprecated', 'Removed', 'Fixed', 'Security'}
    
    # Find all ### headers
    section_pattern = r'^### (.+)$'
    lines = content.split('\n')
    
    for i, line in enumerate(lines, 1):
        match = re.match(section_pattern, line)
        if match:
            section_name = match.group(1).strip()
            if section_name not in valid_sections:
                errors.append(
                    f"Line {i}: Unknown section '{section_name}'. "
                    f"Valid sections are: {', '.join(sorted(valid_sections))}"
                )
    
    return errors


def validate_list_items(content: str) -> List[str]:
    """Validate that list items are properly formatted."""
    errors = []
    lines = content.split('\n')
    
    in_section = False
    current_section_line = 0
    
    for i, line in enumerate(lines, 1):
        # Track when we enter a section
        if re.match(r'^### ', line):
            in_section = True
            current_section_line = i
            continue
        
        # Track when we leave a section (new version or end of file)
        if re.match(r'^## ', line):
            in_section = False
            continue
        
        # Skip empty lines
        if not line.strip():
            continue
        
        # If we're in a section, lines should be list items or continuations
        if in_section:
            stripped = line.strip()
            # Allow list items starting with -
            if stripped.startswith('-'):
                continue
            # Allow indented continuation lines (for multi-line items)
            if line.startswith('  ') or line.startswith('\t'):
                continue
            # Otherwise, it's an error
            if stripped and not stripped.startswith('#'):
                errors.append(
                    f"Line {i}: Content in section should be a list item (starting with '-')"
                )
    
    return errors


def validate_breaking_changes(content: str) -> List[str]:
    """Validate that breaking changes are properly marked."""
    warnings = []
    
    # Look for breaking change indicators
    breaking_patterns = [
        r'\*\*BREAKING\*\*',
        r'\*\*BREAKING:\*\*',
        r'\*\*BREAKING CHANGE\*\*',
        r'BREAKING:',
    ]
    
    # This is informational - we just check if breaking changes exist
    # and are in the Changed section
    lines = content.split('\n')
    in_changed_section = False
    
    for i, line in enumerate(lines, 1):
        if '### Changed' in line:
            in_changed_section = True
            continue
        if re.match(r'^### ', line) and '### Changed' not in line:
            in_changed_section = False
            continue
        
        # Check if breaking change marker is outside Changed section
        for pattern in breaking_patterns:
            if re.search(pattern, line) and not in_changed_section:
                warnings.append(
                    f"Line {i}: Breaking change marker found outside 'Changed' section. "
                    "Consider moving to 'Changed' section for consistency."
                )
    
    return warnings


def validate_changelog(changelog_path: Path, verbose: bool = False) -> Tuple[List[str], List[str]]:
    """
    Validate the changelog file.
    
    Returns:
        Tuple of (errors, warnings)
    """
    errors = []
    warnings = []
    
    # Check file exists
    existence_errors = validate_changelog_exists(changelog_path)
    if existence_errors:
        return existence_errors, warnings
    
    # Read content
    try:
        content = changelog_path.read_text(encoding='utf-8')
    except Exception as e:
        return [f"Could not read CHANGELOG.md: {e}"], warnings
    
    if verbose:
        print(f"📄 Validating: {changelog_path}")
        print(f"   File size: {len(content)} bytes")
    
    # Run all validations
    errors.extend(validate_header(content))
    errors.extend(validate_unreleased_section(content))
    errors.extend(validate_version_entries(content))
    errors.extend(validate_section_format(content))
    errors.extend(validate_list_items(content))
    
    # Breaking change validation produces warnings, not errors
    warnings.extend(validate_breaking_changes(content))
    
    return errors, warnings


def main():
    parser = argparse.ArgumentParser(
        description='Validate CHANGELOG.md follows Keep a Changelog format'
    )
    parser.add_argument(
        '--changelog',
        default='CHANGELOG.md',
        help='Path to CHANGELOG.md file (default: CHANGELOG.md)'
    )
    parser.add_argument(
        '--verbose', '-v',
        action='store_true',
        help='Verbose output'
    )
    parser.add_argument(
        '--strict',
        action='store_true',
        help='Treat warnings as errors'
    )
    
    args = parser.parse_args()
    
    changelog_path = Path(args.changelog)
    
    print("🔍 Validating CHANGELOG format...")
    
    errors, warnings = validate_changelog(changelog_path, args.verbose)
    
    # Print warnings
    if warnings:
        print(f"\n⚠️  Warnings ({len(warnings)}):")
        for warning in warnings:
            print(f"   - {warning}")
    
    # Print errors
    if errors:
        print(f"\n❌ Errors ({len(errors)}):")
        for error in errors:
            print(f"   - {error}")
    
    # Determine exit code
    if errors:
        print(f"\n❌ CHANGELOG validation failed with {len(errors)} error(s)")
        sys.exit(1)
    elif warnings and args.strict:
        print(f"\n❌ CHANGELOG validation failed with {len(warnings)} warning(s) (strict mode)")
        sys.exit(1)
    else:
        print("\n✅ CHANGELOG validation passed!")
        if warnings:
            print(f"   ({len(warnings)} warning(s) - consider addressing them)")
        sys.exit(0)


if __name__ == '__main__':
    main()

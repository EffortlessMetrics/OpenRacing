#!/usr/bin/env python3
"""
Create GitHub issue for removing compatibility shims in next minor version.

This script automatically creates a GitHub issue to track the removal of
compatibility shims that have completed their deprecation window.
"""

import argparse
import json
import os
import sys
from datetime import datetime, timedelta
from pathlib import Path

def get_current_usage():
    """Get current compatibility usage count."""
    try:
        # Run the usage tracking script
        import subprocess
        result = subprocess.run(
            [sys.executable, 'scripts/track_compat_usage.py'],
            capture_output=True,
            text=True,
            cwd=Path.cwd()
        )
        
        if result.returncode == 0:
            # Parse usage count from output
            for line in result.stdout.split('\n'):
                if 'usage count:' in line:
                    return int(line.split(':')[1].strip())
        return 0
    except Exception as e:
        print(f"Warning: Could not get current usage count: {e}")
        return 0

def create_issue_body(version, deprecated_in, current_usage):
    """Create the issue body with current data."""
    
    # Calculate target date (assuming quarterly releases)
    target_date = datetime.now() + timedelta(days=90)
    
    template = f"""## Compatibility Shim Removal

This issue tracks the removal of compatibility shims that have completed their deprecation window.

### Compatibility Layer Details

**Deprecated in version:** v{deprecated_in}  
**Scheduled removal:** v{version}  
**Current usage count:** {current_usage} occurrences  

### Items to Remove

#### Struct Fields
- [ ] `TelemetryData.temp_c` → use `temperature_c`
- [ ] `TelemetryData.wheel_angle_mdeg` → use `wheel_angle_deg`  
- [ ] `TelemetryData.wheel_speed_mrad_s` → use `wheel_speed_rad_s`
- [ ] `TelemetryData.faults` → use `fault_flags`

#### Compatibility Traits
- [ ] `TelemetryCompat` trait in `crates/compat/src/telemetry_compat.rs`
- [ ] Associated implementation blocks
- [ ] Test-only re-exports

#### Functions/Methods
- [ ] `create_device(String)` → use `create_device_with_config(DeviceId, DeviceConfig)`
- [ ] `legacy_field_accessor()` methods

### Pre-Removal Checklist

#### Usage Verification
- [ ] **Current usage count:** Run `python scripts/track_compat_usage.py` 
- [ ] **Usage trend:** Verify usage has been decreasing over deprecation window
- [ ] **Remaining usage:** Identify and migrate any remaining usage

#### Migration Status
- [ ] **Critical paths migrated:** All production code uses new APIs
- [ ] **Test code migrated:** Test code uses new field names or is updated
- [ ] **Examples migrated:** All examples and documentation use new APIs
- [ ] **External dependencies:** Check if any external crates depend on deprecated items

#### Documentation Updates
- [ ] **Migration guide:** Ensure migration documentation is complete
- [ ] **Changelog:** Prepare breaking change entry for changelog
- [ ] **API docs:** Remove deprecated items from documentation
- [ ] **Examples:** Update all code examples

### Removal Tasks

#### Code Changes
- [ ] **Remove deprecated struct fields**
  ```rust
  // Remove these fields from structs
  #[deprecated] pub temp_c: u8,
  #[deprecated] pub wheel_angle_mdeg: i32,
  // etc.
  ```

- [ ] **Remove compatibility traits**
  ```bash
  # Delete compatibility layer files
  rm crates/compat/src/telemetry_compat.rs
  # Update Cargo.toml to remove compat dependency from test builds
  ```

- [ ] **Remove deprecated functions**
  ```rust
  // Remove deprecated function implementations
  #[deprecated] pub fn create_device(id: String) -> Result<Device>
  ```

- [ ] **Update constructors**
  ```rust
  // Remove deprecated field initialization
  impl TelemetryData {{
      pub fn new(...) -> Self {{
          Self {{
              // Remove: temp_c: temperature_c,
              temperature_c,
              // etc.
          }}
      }}
  }}
  ```

#### CI/Build Updates
- [ ] **Remove compat tracking:** Update CI to stop tracking removed compatibility usage
- [ ] **Update lint rules:** Remove deprecated-specific lint exceptions
- [ ] **Build verification:** Ensure all crates build after removal

#### Testing Updates  
- [ ] **Remove compat tests:** Delete tests that specifically test deprecated APIs
- [ ] **Update integration tests:** Ensure integration tests use new APIs
- [ ] **Verify test coverage:** Maintain test coverage for new APIs

### Validation Checklist

#### Compilation
- [ ] **Clean build:** `cargo build --workspace` succeeds
- [ ] **No warnings:** No deprecation warnings remain
- [ ] **All tests pass:** `cargo test --workspace` succeeds
- [ ] **Examples build:** All examples compile and run

#### Runtime Verification
- [ ] **Integration tests:** Full integration test suite passes
- [ ] **Performance tests:** RT performance benchmarks unchanged
- [ ] **Virtual device:** Virtual device demo still works
- [ ] **Real hardware:** Test with actual hardware if available

#### Documentation
- [ ] **API docs:** Generated docs don't reference removed items
- [ ] **Migration guide:** Guide updated to reflect completion
- [ ] **Changelog:** Breaking changes documented

### Communication Plan

#### Internal Communication
- [ ] **Team notification:** Notify team of upcoming removal 1 week before
- [ ] **PR review:** Ensure removal PR gets thorough review
- [ ] **Release notes:** Include in release notes with migration guidance

#### External Communication  
- [ ] **Breaking change notice:** Document in changelog and release notes
- [ ] **Migration timeline:** Communicate any extended timelines if needed
- [ ] **Support:** Provide support for users migrating from deprecated APIs

### Rollback Plan

If issues are discovered after removal:

1. **Immediate rollback:** Revert the removal PR if critical issues found
2. **Hotfix release:** Issue hotfix with reverted changes
3. **Extended timeline:** Extend deprecation window if needed
4. **Improved migration:** Enhance migration tooling/documentation

### Success Criteria

- [ ] **Zero compatibility usage:** `python scripts/track_compat_usage.py` reports 0 usage
- [ ] **Clean compilation:** No deprecation warnings in build output
- [ ] **All tests pass:** Full test suite passes without compatibility layer
- [ ] **Documentation updated:** All docs reference only new APIs
- [ ] **Performance maintained:** RT performance benchmarks unchanged

### Timeline

**Target removal date:** {target_date.strftime('%Y-%m-%d')}  
**Release version:** v{version}  
**Deprecation period:** 3 months

---

### Governance Policy

This removal follows our [Schema Governance Policy](../../docs/SCHEMA_GOVERNANCE.md):
- ✅ Deprecation window of one minor version completed
- ✅ Migration documentation provided
- ✅ Usage tracking shows downward trend
- ✅ Breaking change properly communicated

### Current Status

**Usage trend:** {'🔴 Needs migration' if current_usage > 0 else '✅ Ready for removal'}  
**Estimated effort:** {'High' if current_usage > 20 else 'Medium' if current_usage > 5 else 'Low'}  
**Risk level:** {'High' if current_usage > 50 else 'Medium' if current_usage > 10 else 'Low'}
"""
    
    return template

def create_issue_via_gh_cli(title, body, labels):
    """Create GitHub issue using gh CLI."""
    try:
        import subprocess
        
        # Create issue using gh CLI
        cmd = [
            'gh', 'issue', 'create',
            '--title', title,
            '--body', body,
            '--label', ','.join(labels)
        ]
        
        result = subprocess.run(cmd, capture_output=True, text=True)
        
        if result.returncode == 0:
            issue_url = result.stdout.strip()
            print(f"✅ Created issue: {issue_url}")
            return True
        else:
            print(f"❌ Failed to create issue: {result.stderr}")
            return False
            
    except FileNotFoundError:
        print("❌ GitHub CLI (gh) not found. Please install it or create the issue manually.")
        return False
    except Exception as e:
        print(f"❌ Error creating issue: {e}")
        return False

def save_issue_template(title, body, output_file):
    """Save issue content to file for manual creation."""
    issue_content = f"# {title}\n\n{body}"
    
    with open(output_file, 'w') as f:
        f.write(issue_content)
    
    print(f"📝 Issue template saved to: {output_file}")
    print("You can copy this content to create the issue manually on GitHub.")

def main():
    """Main entry point."""
    parser = argparse.ArgumentParser(description='Create compatibility shim removal issue')
    parser.add_argument('--version', required=True, help='Target version for removal (e.g., 1.3.0)')
    parser.add_argument('--deprecated-in', required=True, help='Version when items were deprecated (e.g., 1.2.0)')
    parser.add_argument('--output', help='Output file for issue template (if not using gh CLI)')
    parser.add_argument('--dry-run', action='store_true', help='Generate template without creating issue')
    
    args = parser.parse_args()
    
    # Get current usage count
    current_usage = get_current_usage()
    
    # Create issue content
    title = f"Remove compatibility shims for TelemetryData in v{args.version}"
    body = create_issue_body(args.version, args.deprecated_in, current_usage)
    labels = ['breaking-change', 'compatibility', 'governance']
    
    if args.dry_run or args.output:
        # Save to file
        output_file = args.output or f"removal_issue_v{args.version}.md"
        save_issue_template(title, body, output_file)
    else:
        # Try to create via GitHub CLI
        if not create_issue_via_gh_cli(title, body, labels):
            # Fallback to file output
            output_file = f"removal_issue_v{args.version}.md"
            save_issue_template(title, body, output_file)
    
    # Print summary
    print(f"\n📊 Summary:")
    print(f"   Target version: v{args.version}")
    print(f"   Deprecated in: v{args.deprecated_in}")
    print(f"   Current usage: {current_usage} occurrences")
    print(f"   Priority: {'High' if current_usage > 20 else 'Medium' if current_usage > 5 else 'Low'}")
    
    if current_usage > 0:
        print(f"\n⚠️  Migration needed: {current_usage} compatibility usages must be migrated before removal")
        print(f"   Run: python scripts/track_compat_usage.py")
        print(f"   See: docs/MIGRATION_PATTERNS.md")

if __name__ == '__main__':
    main()
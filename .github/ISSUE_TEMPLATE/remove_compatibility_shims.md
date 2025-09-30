---
name: Remove Compatibility Shims
about: Track removal of compatibility layer in next minor version
title: 'Remove compatibility shims for [FEATURE] in v[VERSION]'
labels: ['breaking-change', 'compatibility', 'governance']
assignees: ''
---

## Compatibility Shim Removal

This issue tracks the removal of compatibility shims that have completed their deprecation window.

### Compatibility Layer Details

**Deprecated in version:** [e.g., v1.2.0]  
**Scheduled removal:** [e.g., v1.3.0]  
**Current usage count:** [e.g., 15 occurrences]  

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
  impl TelemetryData {
      pub fn new(...) -> Self {
          Self {
              // Remove: temp_c: temperature_c,
              temperature_c,
              // etc.
          }
      }
  }
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

### Related Issues

- Deprecation issue: #[ISSUE_NUMBER]
- Migration tracking: #[ISSUE_NUMBER]  
- Documentation updates: #[ISSUE_NUMBER]

### Timeline

**Target removal date:** [e.g., 2025-02-01]  
**Release version:** [e.g., v1.3.0]  
**Deprecation period:** [e.g., 3 months]

---

### Governance Policy

This removal follows our [Schema Governance Policy](../../docs/SCHEMA_GOVERNANCE.md):
- ✅ Deprecation window of one minor version completed
- ✅ Migration documentation provided
- ✅ Usage tracking shows downward trend
- ✅ Breaking change properly communicated

### Automation

This issue can be automatically created by running:
```bash
# Create removal issue for next minor version
python scripts/create_removal_issue.py --version 1.3.0 --deprecated-in 1.2.0
```
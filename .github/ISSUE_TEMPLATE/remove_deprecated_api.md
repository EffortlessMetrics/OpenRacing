---
name: Remove Deprecated API
about: Track removal of deprecated APIs after deprecation window
title: 'Remove deprecated API: [API_NAME] in v[VERSION]'
labels: ['breaking-change', 'deprecation', 'technical-debt']
assignees: ['@owner-sch']
---

## Deprecated API Removal

**API Name**: `[API_NAME]`  
**Deprecated In**: v[DEPRECATED_VERSION]  
**Scheduled Removal**: v[REMOVE_VERSION]  
**Migration Guide**: [Link to migration documentation]  
**Deprecation Window**: [X] minor versions  

## Description

Brief description of the deprecated API and why it's being removed.

## Migration Path

### Before (Deprecated)
```rust
// Example of old usage
let device = create_device_id("wheel-1".to_string());
let temp = telemetry.temp_c;
```

### After (New API)
```rust
// Example of new usage  
let device = DeviceId::from_str("wheel-1")?;
let temp = telemetry.temperature_c;
```

## Pre-Removal Checklist

### Usage Verification
- [ ] **Workspace scan**: No usage found with `rg -n "[SEARCH_PATTERN]" crates/`
- [ ] **Integration tests**: No usage in test code
- [ ] **Documentation**: No references in docs or examples
- [ ] **External usage**: Checked if this is a public API with external consumers

### Compatibility Tracking
- [ ] **Compat usage count**: Current count is 0 (run `scripts/track_compat_usage.py`)
- [ ] **Trend verification**: Usage has been trending down during deprecation window
- [ ] **Migration complete**: All known consumers have migrated

### Code Removal
- [ ] **Remove deprecated code**: Delete the deprecated implementation
- [ ] **Remove compat shims**: Delete compatibility layer entries
- [ ] **Update re-exports**: Remove deprecated re-exports
- [ ] **Clean up imports**: Remove unused imports

### Testing
- [ ] **Compilation**: `cargo build --workspace` passes
- [ ] **Tests**: `cargo test --workspace` passes  
- [ ] **Trybuild guards**: Compile-fail tests prevent reintroduction
- [ ] **Feature combinations**: All feature flags still work

### Documentation Updates
- [ ] **CHANGELOG**: Document breaking change with migration notes
- [ ] **Migration guide**: Update or archive migration documentation
- [ ] **API docs**: Remove deprecated items from documentation
- [ ] **Examples**: Update all code examples

### CI Verification
- [ ] **Lint gates**: All lint checks pass
- [ ] **Cross-platform**: Linux and Windows builds pass
- [ ] **Isolation builds**: All crates compile independently
- [ ] **Schema validation**: Protobuf/JSON schema checks pass

## Rollback Plan

If issues are discovered after removal:

1. **Immediate**: Revert the removal commit
2. **Short-term**: Extend deprecation window by 1 minor version
3. **Long-term**: Improve migration tooling or documentation

## Impact Assessment

### Breaking Change Scope
- [ ] **Internal only**: Only affects internal workspace code
- [ ] **Public API**: Affects external consumers (requires major version bump)
- [ ] **Plugin ABI**: Affects plugin compatibility
- [ ] **Configuration**: Affects config file formats

### Migration Complexity
- [ ] **Automatic**: Can be migrated with find/replace
- [ ] **Semi-automatic**: Requires simple code changes
- [ ] **Manual**: Requires significant refactoring

### Risk Level
- [ ] **Low**: Simple rename or signature change
- [ ] **Medium**: Logic changes but clear migration path
- [ ] **High**: Complex changes affecting multiple systems

## Verification Commands

```bash
# Check for remaining usage
rg -n "[SEARCH_PATTERN]" crates/

# Verify compat debt is zero
python scripts/track_compat_usage.py --current

# Test compilation
cargo build --workspace --all-targets

# Test all feature combinations  
cargo build --workspace --all-features
cargo build --workspace --no-default-features

# Run isolation builds
cargo build -p wheelctl
cargo build -p racing-wheel-service  
cargo build -p racing-wheel-plugins

# Verify trybuild guards
cargo test -p racing-wheel-schemas --test trybuild
```

## Related Issues

- Deprecation issue: #[DEPRECATION_ISSUE]
- Migration tracking: #[MIGRATION_ISSUE]  
- Documentation updates: #[DOCS_ISSUE]

## Definition of Done

- [ ] All checklist items completed
- [ ] CI passes on all platforms
- [ ] No usage of deprecated API remains in workspace
- [ ] Trybuild guards prevent reintroduction
- [ ] Documentation updated
- [ ] CHANGELOG entry added

---

**Note**: This issue should only be closed when the deprecated API has been completely removed and all verification steps pass. If any issues are discovered, extend the deprecation window rather than rushing the removal.
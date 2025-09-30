# Pull Request

## Description

Brief description of the changes in this PR.

## Type of Change

- [ ] Bug fix (non-breaking change which fixes an issue)
- [ ] New feature (non-breaking change which adds functionality)
- [ ] Breaking change (fix or feature that would cause existing functionality to not work as expected)
- [ ] Documentation update
- [ ] Refactoring (no functional changes)
- [ ] Performance improvement
- [ ] Test coverage improvement

## Schema/API Changes

**⚠️ Required for all PRs that modify public APIs, struct fields, or schemas**

### Breaking Changes Assessment

- [ ] **No breaking changes** - This PR does not modify public APIs, struct fields, or schemas
- [ ] **Breaking changes present** - This PR includes breaking changes (complete section below)

### If Breaking Changes Are Present

- [ ] **Deprecation window followed** - Old items marked `#[deprecated]` for one minor version before removal
- [ ] **Migration documentation provided** - Clear migration path documented with examples
- [ ] **Compatibility layer created** - Test-only compatibility layer added if needed (using `#[cfg(test)]`)
- [ ] **CI tracking updated** - Compatibility usage tracking configured to trend downward
- [ ] **Version bump planned** - Minor version bump scheduled for these changes

### Schema Stability Checklist

#### Struct Field Changes
- [ ] **Field renames** follow the rename → alias → remove pattern
- [ ] **Field additions** include appropriate defaults
- [ ] **Field removals** went through deprecation window
- [ ] **Type changes** are backward compatible or properly deprecated

#### Function Signature Changes  
- [ ] **Parameter changes** follow add new → deprecate old → remove pattern
- [ ] **Return type changes** maintain backward compatibility
- [ ] **New functions** are properly documented
- [ ] **Deprecated functions** include migration notes

#### Protobuf Schema Changes
- [ ] **Field numbers** are never reused
- [ ] **Message structure** changes are backward compatible
- [ ] **Breaking changes** detected by `buf breaking` check
- [ ] **Field renames** preserve wire compatibility

#### JSON Schema Changes
- [ ] **Required fields** not removed without deprecation
- [ ] **Schema version** updated for breaking changes  
- [ ] **Validation tests** updated for new schema
- [ ] **Migration path** documented for consumers

## Testing

### Test Coverage
- [ ] **Unit tests** added/updated for new functionality
- [ ] **Integration tests** cover the change
- [ ] **Property tests** maintain parametric nature (no conversion to zero-arg functions)
- [ ] **Regression tests** added for bug fixes

### Schema/API Testing
- [ ] **Compatibility tests** verify old and new APIs work during transition
- [ ] **Migration tests** validate upgrade path
- [ ] **Round-trip tests** for schema changes
- [ ] **Compile-time tests** for struct instantiation

### Performance Testing
- [ ] **RT performance** validated (p99 jitter ≤ 0.25ms for RT changes)
- [ ] **Memory usage** impact assessed
- [ ] **Build time** impact measured

## Compatibility Impact

### Compatibility Layer Usage
- [ ] **Usage count tracked** - CI will verify compat usage doesn't increase
- [ ] **Migration timeline** - Plan to remove compat layer in next minor version
- [ ] **Test-only scope** - Compatibility layer is `#[cfg(test)]` only

### Deprecation Timeline
- [ ] **Current version** - Items marked deprecated in this version
- [ ] **Removal version** - Items will be removed in version X.Y.0
- [ ] **Communication plan** - Breaking changes communicated to users

## Code Quality

### Linting and Formatting
- [ ] **Clippy** passes with no new warnings
- [ ] **Rustfmt** applied to all changed code
- [ ] **Unused imports** removed
- [ ] **Dead code** addressed with `#[allow(dead_code)]` or removal

### Safety and Best Practices
- [ ] **No unsafe code** added (or justified if necessary)
- [ ] **Error handling** follows project patterns
- [ ] **Documentation** updated for public APIs
- [ ] **Examples** updated if affected

## CI Checks

The following automated checks must pass:

- [ ] **Compilation** - All crates compile without errors
- [ ] **Tests** - Full test suite passes
- [ ] **Linting** - Clippy and rustfmt checks pass
- [ ] **Schema validation** - Protobuf and JSON schema checks pass
- [ ] **Compatibility tracking** - Compat usage doesn't increase
- [ ] **Breaking change detection** - `buf breaking` passes or changes are intentional
- [ ] **Performance** - RT performance benchmarks pass

## Documentation

- [ ] **Code comments** added for complex logic
- [ ] **API documentation** updated for public changes
- [ ] **Migration guide** created for breaking changes
- [ ] **Changelog** entry added
- [ ] **README** updated if needed

## Reviewer Checklist

### For Reviewers

- [ ] **Schema changes** follow governance policy
- [ ] **Breaking changes** are justified and properly handled
- [ ] **Migration path** is clear and well-documented
- [ ] **Test coverage** is adequate
- [ ] **Performance impact** is acceptable
- [ ] **Code quality** meets project standards

### Schema/API Review (Required for breaking changes)

- [ ] **Deprecation policy** followed correctly
- [ ] **Compatibility layer** properly scoped to tests
- [ ] **Migration documentation** is comprehensive
- [ ] **Timeline** for removal is reasonable
- [ ] **Impact assessment** is accurate

## Additional Notes

<!-- Add any additional context, concerns, or notes for reviewers -->

---

### Schema Governance Policy

This PR template enforces our [Schema Governance Policy](../docs/SCHEMA_GOVERNANCE.md). For detailed migration patterns, see [Migration Patterns](../docs/MIGRATION_PATTERNS.md).

**Key Requirements:**
- Breaking changes require minor version bump
- Deprecation window of one minor version
- Compatibility layer must be test-only (`#[cfg(test)]`)
- CI must track compatibility usage trending downward
- Migration documentation required for all breaking changes
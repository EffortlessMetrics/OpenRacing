# Schema and API Governance Policy

## Overview

This document establishes the governance policy for schema and API changes in the racing wheel software to prevent compilation breakage and ensure smooth evolution of the codebase. The policy defines deprecation windows, migration patterns, and enforcement mechanisms.

## Scope

This policy applies to:
- **Public APIs** in `engine` and `schemas` crates
- **Protobuf schemas** used for external communication
- **JSON schemas** for configuration and profiles
- **Struct field names** and function signatures in public interfaces

## Semantic Versioning Policy

### Breaking Changes

The following changes are considered **breaking** and require a minor version bump:

- Renaming public struct fields
- Removing public struct fields
- Changing function signatures in public APIs
- Modifying protobuf message structure
- Changing JSON schema required fields

### Non-Breaking Changes

The following changes are **non-breaking** and can be included in patch releases:

- Adding new optional fields with defaults
- Adding new functions to public APIs
- Extending enums with new variants (when using `#[non_exhaustive]`)
- Adding new protobuf fields with proper field numbers

## Deprecation Window Policy

### Standard Deprecation Process

1. **Mark as deprecated** - Add `#[deprecated]` attribute with migration guidance
2. **Deprecation window** - Keep deprecated items for **one minor version**
3. **Removal** - Remove deprecated items in the next minor version

### Example Deprecation

```rust
// Version 1.2.0 - Add new field and deprecate old
pub struct TelemetryData {
    #[deprecated(since = "1.2.0", note = "Use `temperature_c` instead")]
    pub temp_c: u8,
    pub temperature_c: u8,
    // ... other fields
}

// Version 1.3.0 - Remove deprecated field
pub struct TelemetryData {
    pub temperature_c: u8,
    // ... other fields
}
```

## Migration Patterns

### Field Rename Pattern

**Phase 1: Add new field, deprecate old**
```rust
pub struct Config {
    #[deprecated(since = "1.2.0", note = "Use `new_field_name` instead")]
    pub old_field_name: Type,
    pub new_field_name: Type,
}
```

**Phase 2: Create compatibility layer (test-only)**
```rust
#[cfg(test)]
pub trait ConfigCompat {
    fn old_field_name(&self) -> Type;
}

#[cfg(test)]
impl ConfigCompat for Config {
    #[inline]
    fn old_field_name(&self) -> Type {
        self.new_field_name
    }
}
```

**Phase 3: Remove deprecated field**
```rust
pub struct Config {
    pub new_field_name: Type,
}
```

### Function Signature Change Pattern

**Phase 1: Add new function, deprecate old**
```rust
#[deprecated(since = "1.2.0", note = "Use `new_function` instead")]
pub fn old_function(old_params: Type) -> Result<Type> {
    new_function(convert_params(old_params))
}

pub fn new_function(new_params: NewType) -> Result<Type> {
    // New implementation
}
```

**Phase 2: Remove deprecated function**
```rust
pub fn new_function(new_params: NewType) -> Result<Type> {
    // Implementation
}
```

## Enforcement Mechanisms

### CI Checks

The following CI checks enforce this policy:

1. **Deprecation Detection** - Fail if deprecated field usage increases
2. **Schema Break Detection** - Use `buf breaking` for protobuf changes
3. **JSON Schema Validation** - Round-trip tests for schema compatibility
4. **Compatibility Usage Tracking** - Monitor compat layer usage trending down

### Lint Rules

Required lint rules in non-test crates:
```rust
#![deny(deprecated)]
#![warn(clippy::deprecated_semver)]
```

### PR Requirements

All PRs that modify public APIs must:
- [ ] Follow the deprecation window policy
- [ ] Update compatibility layer if needed
- [ ] Add migration documentation
- [ ] Include tests for both old and new APIs during transition
- [ ] Update changelog with breaking change notes

## Compatibility Layer Guidelines

### Purpose

The compatibility layer serves as a **temporary bridge** during migration periods to:
- Allow gradual migration of existing code
- Prevent compilation breakage during transitions
- Provide clear migration paths

### Implementation Rules

1. **Test-only dependency** - Compatibility layer must be `#[cfg(test)]` only
2. **Inline forwarding** - Use `#[inline]` for zero-cost forwarding
3. **Temporary nature** - Remove in next minor version
4. **Usage tracking** - Monitor usage count trending downward

### Example Implementation

```rust
// crates/compat/src/telemetry_compat.rs
#[cfg(test)]
pub trait TelemetryCompat {
    /// Deprecated: Use `temperature_c` instead
    fn temp_c(&self) -> u8;
    /// Deprecated: Use `fault_flags` instead  
    fn faults(&self) -> u8;
}

#[cfg(test)]
impl TelemetryCompat for TelemetryData {
    #[inline]
    fn temp_c(&self) -> u8 {
        self.temperature_c
    }
    
    #[inline]
    fn faults(&self) -> u8 {
        self.fault_flags
    }
}
```

## Migration Tracking

### Usage Metrics

The CI system tracks:
- **Compatibility layer usage count** - Must trend downward
- **Deprecated field usage** - Must not increase
- **Schema breaking changes** - Must be intentional and documented

### Reporting

Weekly reports include:
- Current compatibility usage count
- Trend analysis (increasing/decreasing)
- Migration progress by module
- Upcoming deprecation removals

## Schema Stability Requirements

### Protobuf Schemas

- **Field numbers** are permanent once assigned
- **Field names** can be changed with deprecation window
- **Message structure** changes require minor version bump
- **Backward compatibility** must be maintained for one minor version

### JSON Schemas

- **Required fields** cannot be removed without minor version bump
- **Optional fields** can be added freely
- **Field renames** follow standard deprecation process
- **Schema version** must be updated for breaking changes

## Rollback Procedures

### Emergency Rollback

If a breaking change causes critical issues:

1. **Immediate revert** of the breaking change
2. **Hotfix release** with reverted changes
3. **Post-mortem** to understand failure
4. **Improved testing** before re-attempting change

### Planned Rollback

If migration is not proceeding as expected:

1. **Extend deprecation window** by one minor version
2. **Improve migration tooling** or documentation
3. **Communicate timeline** to affected teams
4. **Re-evaluate** migration approach

## Governance Roles

### Schema Maintainers

Responsible for:
- Reviewing schema changes
- Enforcing deprecation policy
- Managing compatibility layers
- Coordinating breaking changes

### Release Managers

Responsible for:
- Version planning
- Breaking change coordination
- Migration timeline management
- Communication of changes

## Documentation Requirements

### Change Documentation

All schema/API changes must include:
- **Migration guide** with before/after examples
- **Timeline** for deprecation and removal
- **Impact assessment** on existing code
- **Testing strategy** for the change

### Version Documentation

Each release must document:
- **Breaking changes** with migration paths
- **Deprecated items** with removal timeline
- **New features** and their stability guarantees
- **Compatibility notes** for integrators

## Compliance Monitoring

### Automated Checks

- **Daily** compatibility usage reports
- **Per-PR** breaking change detection
- **Weekly** deprecation timeline reviews
- **Monthly** governance policy compliance audits

### Manual Reviews

- **Quarterly** policy effectiveness review
- **Semi-annual** deprecation cleanup
- **Annual** governance policy updates

## Policy Updates

This governance policy is a living document that should be updated as the project evolves. Changes to this policy require:

1. **RFC process** for significant changes
2. **Team consensus** on policy modifications
3. **Migration plan** for policy changes
4. **Documentation updates** across all relevant docs

---

**Policy Version:** 1.0  
**Last Updated:** 2025-01-01  
**Next Review:** 2025-04-01
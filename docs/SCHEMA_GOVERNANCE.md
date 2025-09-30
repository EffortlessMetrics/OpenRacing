# Schema Governance and API Stability

This document defines the governance policies, deprecation procedures, and stability requirements for schemas, APIs, and cross-crate interfaces in the racing wheel project.

## Table of Contents

- [Overview](#overview)
- [API Stability Levels](#api-stability-levels)
- [Deprecation Policy](#deprecation-policy)
- [Migration Patterns](#migration-patterns)
- [Breaking Change Procedures](#breaking-change-procedures)
- [Schema Versioning](#schema-versioning)
- [Compatibility Layer Management](#compatibility-layer-management)
- [Enforcement and Automation](#enforcement-and-automation)

## Overview

### Principles

1. **Stability First**: Public APIs should be stable and backward compatible
2. **Explicit Migration**: Breaking changes require clear migration paths
3. **Mechanical Enforcement**: Governance rules are enforced by CI, not just documentation
4. **Debt Tracking**: Compatibility debt is measured and must trend downward
5. **Owner Accountability**: Each API surface has a designated owner responsible for stability

### Scope

This governance applies to:
- Public APIs in `racing-wheel-schemas` crate
- IPC contracts in `racing-wheel-ipc` (generated from protobuf)
- Plugin ABI in `racing-wheel-plugins`
- Cross-crate trait definitions and type signatures
- Configuration file formats and CLI interfaces

## API Stability Levels

### Stable APIs
**Commitment**: Backward compatibility maintained for at least 2 minor versions

**Examples**:
- `DeviceId`, `TorqueNm`, `BaseSettings` types
- IPC service trait definitions
- Plugin ABI structs and constants
- CLI command structure and major flags

**Change Process**: Requires deprecation window and migration guide

### Unstable APIs  
**Commitment**: May change without notice, marked with `#[doc(hidden)]` or feature flags

**Examples**:
- Internal helper functions
- Experimental features behind feature flags
- Debug/development-only interfaces

**Change Process**: Can change freely, but must not break stable API consumers

### Internal APIs
**Commitment**: Private to crate, no stability guarantees

**Examples**:
- Private module functions
- Implementation details
- Test utilities

**Change Process**: Free to change, but cross-crate usage is forbidden

## Deprecation Policy

### Deprecation Window

**Standard Window**: 2 minor versions (e.g., deprecated in 1.2.0, removed in 1.4.0)

**Extended Window**: 4 minor versions for widely-used APIs or complex migrations

**Emergency Removal**: Security vulnerabilities may be removed immediately with patch release

### Deprecation Process

#### 1. Mark as Deprecated
```rust
#[deprecated(since = "1.2.0", note = "Use `new_function()` instead. Will be removed in 1.4.0")]
pub fn old_function() -> Result<(), Error> {
    // Implementation that delegates to new function
    new_function()
}
```

#### 2. Add Compatibility Shim
```rust
// In compat layer (if complex migration needed)
pub use crate::new_module::NewType as OldType;

impl From<OldType> for NewType {
    fn from(old: OldType) -> Self {
        // Lossless conversion
    }
}
```

#### 3. Update Documentation
- Add migration guide to docs/MIGRATION_PATTERNS.md
- Update CHANGELOG.md with deprecation notice
- Add examples showing new usage patterns

#### 4. Track Usage
```bash
# Measure compat layer usage
scripts/track_compat_usage.py --baseline
# Usage count must trend downward over deprecation window
```

#### 5. Create Removal Issue
```bash
# Auto-generate removal tracking issue
scripts/create_removal_issue.py --deprecated-in=1.2.0 --remove-in=1.4.0
```

## Migration Patterns

### Pattern 1: Rename → Alias → Remove

**Use Case**: Simple renames without logic changes

**Timeline**: 2 minor versions

```rust
// Version 1.1.0 - Original
pub struct TelemetryData {
    pub temp_c: f32,
}

// Version 1.2.0 - Add new field, alias old
pub struct TelemetryData {
    pub temperature_c: f32,
    #[deprecated(since = "1.2.0", note = "Use temperature_c instead")]
    pub temp_c: f32,  // Computed property or alias
}

impl TelemetryData {
    #[deprecated(since = "1.2.0")]
    pub fn temp_c(&self) -> f32 {
        self.temperature_c
    }
}

// Version 1.4.0 - Remove old field
pub struct TelemetryData {
    pub temperature_c: f32,
    // temp_c removed
}
```

### Pattern 2: Type Evolution → Conversion → Migration

**Use Case**: Type signature changes requiring validation

**Timeline**: 2-4 minor versions depending on complexity

```rust
// Version 1.1.0 - Original (unsafe)
pub fn create_device_id(id: String) -> DeviceId {
    DeviceId(id)  // No validation
}

// Version 1.2.0 - Add safe constructor, deprecate unsafe
pub fn create_device_id(id: String) -> DeviceId {
    id.parse().expect("Invalid device ID - use try_create_device_id")
}

#[deprecated(since = "1.2.0", note = "Use DeviceId::from_str() instead")]
pub fn create_device_id_unsafe(id: String) -> DeviceId {
    DeviceId(id)
}

pub fn try_create_device_id(id: String) -> Result<DeviceId, DeviceIdError> {
    id.parse()
}

// Version 1.4.0 - Remove unsafe constructor
// Only safe constructors remain
```

### Pattern 3: Module Restructure → Re-export → Cleanup

**Use Case**: Moving types between modules or crates

**Timeline**: 2 minor versions

```rust
// Version 1.1.0 - Original location
// racing-wheel-schemas/src/lib.rs
pub mod telemetry {
    pub struct TelemetryData { /* ... */ }
}

// Version 1.2.0 - Move to new location, re-export from old
// racing-wheel-schemas/src/domain/telemetry.rs
pub struct TelemetryData { /* ... */ }

// racing-wheel-schemas/src/lib.rs
pub mod telemetry {
    #[deprecated(since = "1.2.0", note = "Use racing_wheel_schemas::domain::TelemetryData")]
    pub use crate::domain::telemetry::TelemetryData;
}

pub mod domain {
    pub mod telemetry {
        pub use super::TelemetryData;
    }
}

// Version 1.4.0 - Remove old re-export
// Only new location available
```

## Breaking Change Procedures

### Pre-Change Analysis

1. **Impact Assessment**
   ```bash
   # Find all usages across workspace
   rg -n "OldType|old_function" crates/
   
   # Check external dependencies (if public crate)
   cargo search racing-wheel-schemas --limit 100
   ```

2. **Migration Complexity**
   - **Simple**: Rename or signature change with automatic conversion
   - **Complex**: Logic changes requiring manual code updates
   - **Breaking**: No backward compatibility possible

### Change Implementation

#### For Simple Changes
1. Add new API alongside old API
2. Implement automatic conversion between old and new
3. Deprecate old API with clear migration instructions
4. Track usage and ensure it trends downward

#### For Complex Changes  
1. Design migration strategy with examples
2. Implement compatibility layer if needed
3. Create detailed migration guide
4. Provide migration tools/scripts if applicable
5. Extended deprecation window (4 minor versions)

#### For Breaking Changes
1. **Justification Required**: Document why breaking change is necessary
2. **Version Bump**: Increment major version (1.x.y → 2.0.0)
3. **Migration Guide**: Comprehensive guide with before/after examples
4. **Tool Support**: Automated migration tools where possible

### Change Approval Process

1. **RFC Phase** (for major changes)
   - Create RFC document outlining change rationale
   - Gather feedback from affected crate owners
   - Estimate migration effort for consumers

2. **Implementation Phase**
   - Implement change with compatibility layer
   - Add comprehensive tests for migration path
   - Update documentation and examples

3. **Review Phase**
   - Schema owner (@owner-sch) approval required
   - Affected crate owners review migration impact
   - CI validation of all migration examples

## Schema Versioning

### Protobuf Schema Versioning

**Package Naming**: `wheel.v1`, `wheel.v2`, etc.

**Breaking Change Triggers**:
- Field removal or rename
- Type changes (string → int, optional → required)
- Service method signature changes
- Enum value removal

**Compatible Change Examples**:
- Adding optional fields
- Adding new enum values (with unknown handling)
- Adding new service methods
- Deprecating fields (but not removing)

**Validation**:
```bash
# Automated breaking change detection
buf breaking --against main

# Manual review for complex changes
buf diff --against main
```

### JSON Schema Versioning

**Version Field**: All JSON configs include `"schema_version": "1.2"`

**Breaking Change Triggers**:
- Required field addition
- Field type changes
- Validation rule changes (stricter)
- Format changes

**Migration Support**:
```rust
#[derive(Deserialize)]
#[serde(tag = "schema_version")]
enum ConfigVersions {
    #[serde(rename = "1.1")]
    V1_1(ConfigV1_1),
    #[serde(rename = "1.2")]  
    V1_2(ConfigV1_2),
}

impl From<ConfigV1_1> for ConfigV1_2 {
    fn from(old: ConfigV1_1) -> Self {
        // Lossless migration logic
    }
}
```

## Compatibility Layer Management

### Compat Debt Tracking

**Measurement**: Count of deprecated API usages across workspace

```bash
# Baseline measurement
scripts/track_compat_usage.py --baseline > compat_baseline.json

# Current measurement  
scripts/track_compat_usage.py --current

# Trend analysis (must be decreasing)
scripts/track_compat_usage.py --trend --fail-if-increasing
```

**CI Integration**:
```yaml
- name: Track Compat Debt
  run: |
    python scripts/track_compat_usage.py --trend --fail-if-increasing
    echo "Compat debt trend: $(python scripts/track_compat_usage.py --summary)"
```

### Compat Layer Structure

```rust
// crates/compat/src/lib.rs
//! Compatibility layer for deprecated APIs
//! 
//! This crate provides backward compatibility for deprecated APIs.
//! Usage of this crate should trend downward over time.

#[deprecated(since = "1.2.0", note = "Use racing_wheel_schemas::domain::DeviceId")]
pub type DeviceId = racing_wheel_schemas::domain::DeviceId;

#[deprecated(since = "1.2.0", note = "Use TelemetryData::temperature_c")]
pub fn get_temp_c(data: &TelemetryData) -> f32 {
    data.temperature_c
}
```

### Removal Planning

**Automated Issue Creation**:
```bash
# Create removal tracking issue
scripts/create_removal_issue.py \
  --deprecated-in=1.2.0 \
  --remove-in=1.4.0 \
  --api="TelemetryData::temp_c" \
  --migration-guide="docs/MIGRATION_PATTERNS.md#telemetry-field-rename"
```

**Issue Template**:
```markdown
## Remove Deprecated API: {api_name}

**Deprecated In**: v{deprecated_version}  
**Remove In**: v{remove_version}  
**Migration Guide**: {migration_guide_link}

### Checklist
- [ ] Verify no usage in workspace: `rg -n "{api_pattern}"`
- [ ] Check external usage (if public crate)
- [ ] Remove deprecated code
- [ ] Update tests
- [ ] Update documentation
- [ ] Verify CI passes

### Usage Tracking
Current usage count: {current_usage_count}
Target: 0 usages before removal
```

## Enforcement and Automation

### CI Gates

**Compilation Guards**:
```bash
# Prevent deprecated token reintroduction
rg -n 'temp_c|wheel_angle_mdeg|sequence' crates/ && exit 1

# Prevent glob re-exports
rg -n 'pub use .*::\*;' crates/ && exit 1

# Prevent cross-crate private imports
grep -r "use crate::" integration-tests/ && exit 1
```

**Trybuild Guards**:
```rust
// tests/compile-fail/deprecated_fields.rs
use racing_wheel_schemas::TelemetryData;

fn main() {
    let data = TelemetryData::default();
    let _ = data.temp_c; // Should fail to compile
}
```

**Dependency Governance**:
```bash
# No version conflicts
cargo tree --duplicates && exit 1

# Feature unification
cargo hakari generate && git diff --exit-code

# Minimal versions compatibility
cargo +nightly -Z minimal-versions build
```

### Automated Checks

**PR Template Validation**:
- Migration notes required for API changes
- Schema version bumps justified
- Compat debt delta measured and explained

**CODEOWNERS Enforcement**:
- Schema changes require @owner-sch approval
- Service IPC changes require @owner-svc approval  
- Plugin ABI changes require @owner-plug approval

**Lint Rules**:
```toml
# clippy.toml
disallowed-methods = [
    { path = "std::panic::panic", reason = "Use Result<T, E> instead" },
    { path = "std::unimplemented", reason = "Use todo!() for temporary code" },
]

disallowed-types = [
    { path = "std::collections::HashMap", reason = "Use IndexMap for deterministic iteration" },
]
```

### Metrics and Reporting

**Compat Debt Dashboard** (printed in CI):
```
Compatibility Debt Report
=========================
Total deprecated API usages: 23 (-5 from last week)
Trending: ⬇️ DOWN (target: 0 by v1.4.0)

By Category:
- TelemetryData fields: 12 usages (-3)
- DeviceId constructors: 8 usages (-2)  
- Async trait patterns: 3 usages (0)

Next Removal Milestone: v1.4.0 (6 weeks)
At-Risk APIs: temp_c (12 usages), create_device_id (8 usages)
```

**Schema Stability Report**:
```
Schema Stability Report  
======================
Protobuf: wheel.v1 (stable since v1.0.0)
JSON Schema: v1.2 (last breaking change: v1.2.0)

Recent Changes:
- Added optional telemetry.fault_flags field (compatible)
- Deprecated telemetry.temp_c field (remove in v1.4.0)

Breaking Change Budget: 0/1 per minor version (good)
```

This governance framework ensures that API changes are deliberate, well-communicated, and mechanically enforced, while providing clear migration paths for consumers and maintaining system stability.
# Schema Migration Patterns

## Overview

This document provides concrete patterns and examples for safely migrating schemas and APIs while maintaining backward compatibility and following the deprecation window policy.

## Migration Pattern: Field Rename

### Pattern: Rename → Alias → Remove

This is the standard three-phase approach for renaming struct fields.

#### Phase 1: Add New Field, Deprecate Old

**Timeline:** Version N (e.g., 1.2.0)

```rust
// Before migration
pub struct TelemetryData {
    pub temp_c: u8,
    pub wheel_angle_mdeg: i32,
}

// After Phase 1
pub struct TelemetryData {
    #[deprecated(since = "1.2.0", note = "Use `temperature_c` instead")]
    pub temp_c: u8,
    pub temperature_c: u8,
    
    #[deprecated(since = "1.2.0", note = "Use `wheel_angle_deg` instead")]  
    pub wheel_angle_mdeg: i32,
    pub wheel_angle_deg: i32,
}

// Constructor ensures both fields have same value
impl TelemetryData {
    pub fn new(temperature_c: u8, wheel_angle_deg: i32) -> Self {
        Self {
            temp_c: temperature_c,           // Keep old field in sync
            temperature_c,
            wheel_angle_mdeg: wheel_angle_deg, // Keep old field in sync
            wheel_angle_deg,
        }
    }
}
```

#### Phase 2: Create Compatibility Layer (Test-Only)

**Timeline:** Version N (same as Phase 1)

```rust
// crates/compat/src/telemetry_compat.rs
#[cfg(test)]
pub trait TelemetryCompat {
    /// Deprecated: Use `temperature_c` field directly
    fn temp_c(&self) -> u8;
    /// Deprecated: Use `wheel_angle_deg` field directly
    fn wheel_angle_mdeg(&self) -> i32;
}

#[cfg(test)]
impl TelemetryCompat for TelemetryData {
    #[inline]
    fn temp_c(&self) -> u8 {
        self.temperature_c
    }
    
    #[inline] 
    fn wheel_angle_mdeg(&self) -> i32 {
        self.wheel_angle_deg
    }
}
```

**Usage in tests during migration:**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::compat::TelemetryCompat;
    
    #[test]
    fn test_legacy_field_access() {
        let data = TelemetryData::new(25, 180);
        
        // New code should use new fields
        assert_eq!(data.temperature_c, 25);
        assert_eq!(data.wheel_angle_deg, 180);
        
        // Legacy test code can use compat layer temporarily
        assert_eq!(data.temp_c(), 25);
        assert_eq!(data.wheel_angle_mdeg(), 180);
    }
}
```

#### Phase 3: Remove Deprecated Fields

**Timeline:** Version N+1 (e.g., 1.3.0)

```rust
// Final state - only new fields remain
pub struct TelemetryData {
    pub temperature_c: u8,
    pub wheel_angle_deg: i32,
}

impl TelemetryData {
    pub fn new(temperature_c: u8, wheel_angle_deg: i32) -> Self {
        Self {
            temperature_c,
            wheel_angle_deg,
        }
    }
}
```

**Remove compatibility layer:**
```rust
// Delete crates/compat/src/telemetry_compat.rs
// Update all remaining test code to use new field names
```

## Migration Pattern: Function Signature Change

### Pattern: Add New Function → Deprecate Old → Remove

#### Phase 1: Add New Function, Deprecate Old

```rust
// Old function signature
#[deprecated(since = "1.2.0", note = "Use `create_device_with_config` instead")]
pub fn create_device(id: String) -> Result<Device, Error> {
    create_device_with_config(DeviceId::from(id), DeviceConfig::default())
}

// New function signature  
pub fn create_device_with_config(id: DeviceId, config: DeviceConfig) -> Result<Device, Error> {
    // Implementation
}
```

#### Phase 2: Migration Period

**Encourage migration in documentation:**
```rust
/// Creates a new device with the specified configuration.
/// 
/// # Migration from `create_device`
/// 
/// ```rust
/// // Old way (deprecated)
/// let device = create_device("wheel-1".to_string())?;
/// 
/// // New way
/// let device = create_device_with_config(
///     DeviceId::from("wheel-1"), 
///     DeviceConfig::default()
/// )?;
/// ```
pub fn create_device_with_config(id: DeviceId, config: DeviceConfig) -> Result<Device, Error> {
    // Implementation
}
```

#### Phase 3: Remove Deprecated Function

```rust
// Only new function remains
pub fn create_device_with_config(id: DeviceId, config: DeviceConfig) -> Result<Device, Error> {
    // Implementation
}
```

## Migration Pattern: Enum Variant Changes

### Pattern: Add New Variants → Deprecate Old → Remove

#### Phase 1: Add New Variants

```rust
#[non_exhaustive]
pub enum FFBMode {
    #[deprecated(since = "1.2.0", note = "Use `DirectDrive` instead")]
    Direct,
    DirectDrive,
    
    #[deprecated(since = "1.2.0", note = "Use `BeltDrive` instead")]  
    Belt,
    BeltDrive,
    
    Gear,  // Unchanged
}

// Conversion helpers
impl FFBMode {
    #[deprecated(since = "1.2.0")]
    pub fn from_legacy(legacy: &str) -> Option<Self> {
        match legacy {
            "direct" => Some(Self::DirectDrive),
            "belt" => Some(Self::BeltDrive), 
            "gear" => Some(Self::Gear),
            _ => None,
        }
    }
}
```

#### Phase 2: Update Match Statements

```rust
// Handle both old and new variants during transition
match mode {
    FFBMode::Direct | FFBMode::DirectDrive => {
        // Handle direct drive
    },
    FFBMode::Belt | FFBMode::BeltDrive => {
        // Handle belt drive  
    },
    FFBMode::Gear => {
        // Handle gear drive
    },
}
```

#### Phase 3: Remove Deprecated Variants

```rust
#[non_exhaustive]
pub enum FFBMode {
    DirectDrive,
    BeltDrive, 
    Gear,
}
```

## Migration Pattern: Protobuf Schema Changes

### Pattern: Add New Fields → Deprecate Old → Remove

#### Phase 1: Add New Fields

```protobuf
// telemetry.proto
message TelemetryData {
  // Deprecated fields (keep for compatibility)
  uint32 temp_c = 1 [deprecated = true];
  int32 wheel_angle_mdeg = 2 [deprecated = true];
  
  // New fields
  uint32 temperature_c = 10;
  int32 wheel_angle_deg = 11;
  
  // Other fields...
  uint64 timestamp = 20;
}
```

#### Phase 2: Update Code Generation

```rust
// Generated code handles both old and new fields
impl TelemetryData {
    pub fn migrate_from_legacy(legacy: &LegacyTelemetryData) -> Self {
        Self {
            temperature_c: legacy.temp_c,
            wheel_angle_deg: legacy.wheel_angle_mdeg,
            timestamp: legacy.timestamp,
            ..Default::default()
        }
    }
}
```

#### Phase 3: Remove Deprecated Fields

```protobuf
// telemetry.proto - final state
message TelemetryData {
  uint32 temperature_c = 10;
  int32 wheel_angle_deg = 11;
  uint64 timestamp = 20;
}
```

## Migration Pattern: JSON Schema Evolution

### Pattern: Version Schema → Migrate Data → Update

#### Phase 1: Version the Schema

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$id": "https://racing-wheel.dev/schemas/profile/v1.1.json",
  "title": "Racing Wheel Profile",
  "type": "object",
  "properties": {
    "schema": {
      "type": "string", 
      "enum": ["wheel.profile/1.1"]
    },
    "base": {
      "type": "object",
      "properties": {
        "ffbGain": {"type": "number"},
        "torqueCapNm": {"type": "number"},
        "dorDeg": {"type": "number", "deprecated": true},
        "rotationDeg": {"type": "number"}
      },
      "required": ["ffbGain", "torqueCapNm", "rotationDeg"]
    }
  }
}
```

#### Phase 2: Support Both Versions

```rust
#[derive(Deserialize)]
#[serde(tag = "schema")]
pub enum ProfileVersion {
    #[serde(rename = "wheel.profile/1.0")]
    V1_0(ProfileV1_0),
    #[serde(rename = "wheel.profile/1.1")]  
    V1_1(ProfileV1_1),
}

impl ProfileVersion {
    pub fn migrate_to_latest(self) -> ProfileV1_1 {
        match self {
            ProfileVersion::V1_0(v1_0) => v1_0.migrate_to_v1_1(),
            ProfileVersion::V1_1(v1_1) => v1_1,
        }
    }
}

impl ProfileV1_0 {
    fn migrate_to_v1_1(self) -> ProfileV1_1 {
        ProfileV1_1 {
            schema: "wheel.profile/1.1".to_string(),
            base: BaseConfigV1_1 {
                ffb_gain: self.base.ffb_gain,
                torque_cap_nm: self.base.torque_cap_nm,
                rotation_deg: self.base.dor_deg, // Migrate field name
            }
        }
    }
}
```

## Migration Checklist

### Pre-Migration

- [ ] Identify all affected code paths
- [ ] Plan deprecation timeline (one minor version)
- [ ] Create migration documentation
- [ ] Set up compatibility tracking

### During Migration

- [ ] Add new fields/functions with proper documentation
- [ ] Mark old items as `#[deprecated]` with migration notes
- [ ] Create compatibility layer for tests if needed
- [ ] Update CI to track usage trending down
- [ ] Communicate changes to team

### Post-Migration

- [ ] Monitor compatibility usage decreasing
- [ ] Remove deprecated items in next minor version
- [ ] Clean up compatibility layer
- [ ] Update documentation
- [ ] Validate no regressions

## Common Pitfalls

### Pitfall 1: Breaking Changes Without Deprecation

**Wrong:**
```rust
// Version 1.2.0 - BREAKING without warning
pub struct Config {
    pub new_field_name: Type,  // Old field just removed
}
```

**Right:**
```rust
// Version 1.2.0 - Proper deprecation
pub struct Config {
    #[deprecated(since = "1.2.0", note = "Use `new_field_name` instead")]
    pub old_field_name: Type,
    pub new_field_name: Type,
}
```

### Pitfall 2: Compatibility Layer in Production

**Wrong:**
```rust
// Compatibility layer available in production builds
pub trait ConfigCompat {
    fn old_method(&self) -> Type;
}
```

**Right:**
```rust
// Compatibility layer only in tests
#[cfg(test)]
pub trait ConfigCompat {
    fn old_method(&self) -> Type;
}
```

### Pitfall 3: Forgetting to Remove Deprecated Items

**Wrong:**
```rust
// Version 1.4.0 - Still has deprecated items from 1.2.0
pub struct Config {
    #[deprecated(since = "1.2.0")]  // Should have been removed in 1.3.0
    pub old_field: Type,
    pub new_field: Type,
}
```

**Right:**
```rust
// Version 1.3.0 - Deprecated items removed after one minor version
pub struct Config {
    pub new_field: Type,
}
```

## Migration Tools

### Automated Migration Script

```bash
#!/bin/bash
# scripts/migrate-fields.sh

# Find and replace deprecated field usage
find crates/ -name "*.rs" -exec sed -i 's/\.temp_c/\.temperature_c/g' {} \;
find crates/ -name "*.rs" -exec sed -i 's/\.wheel_angle_mdeg/\.wheel_angle_deg/g' {} \;

# Run tests to verify migration
cargo test --workspace

echo "Migration complete. Please review changes and run full test suite."
```

### Usage Tracking Query

```bash
# Check current compatibility usage
python scripts/track_compat_usage.py

# Check for deprecated field usage
grep -r "\.temp_c\|\.faults\|\.wheel_angle_mdeg" crates/ --include="*.rs" | wc -l
```

---

**Document Version:** 1.0  
**Last Updated:** 2025-01-01  
**Related:** [Schema Governance Policy](SCHEMA_GOVERNANCE.md)
# Racing Wheel Schemas

This crate provides schema-first contracts and validation for the racing wheel software suite. It includes:

- **Protobuf schemas** for IPC service contracts
- **JSON Schema** for profile validation with migration support
- **Domain types** with unit safety and validation
- **Code generation** from schemas during compilation
- **Compatibility checks** to prevent breaking changes

## Features

### JSON Schema Validation

The crate provides comprehensive JSON Schema validation for racing wheel profiles:

```rust
use racing_wheel_schemas::config::ProfileValidator;

let validator = ProfileValidator::new()?;
let profile = validator.validate_json(json_string)?;
```

### Protobuf Service Contracts

Generated Rust types from protobuf definitions for gRPC services:

```rust
use racing_wheel_schemas::wheel::v1::{WheelService, DeviceInfo, Profile};
```

### Domain Types with Unit Safety

Strongly-typed domain objects that prevent common errors:

```rust
use racing_wheel_schemas::{TorqueNm, Degrees, DeviceId};

let torque = TorqueNm::new(15.0)?; // Validates range
let dor = Degrees::new_dor(900.0)?; // Validates DOR range
let device_id = DeviceId::new("wheel-base-1".to_string())?; // Validates format
```

## Schema Validation

### Profile Schema (wheel.profile/1)

The profile schema validates racing wheel configurations with the following structure:

```json
{
  "schema": "wheel.profile/1",
  "scope": {
    "game": "iracing",
    "car": "gt3"
  },
  "base": {
    "ffbGain": 0.75,
    "dorDeg": 900,
    "torqueCapNm": 15.0,
    "filters": {
      "reconstruction": 4,
      "friction": 0.12,
      "damper": 0.18,
      "inertia": 0.08,
      "notchFilters": [],
      "slewRate": 0.85,
      "curvePoints": [
        {"input": 0.0, "output": 0.0},
        {"input": 1.0, "output": 1.0}
      ]
    }
  },
  "leds": {
    "rpmBands": [0.75, 0.82, 0.88, 0.92, 0.96],
    "pattern": "progressive",
    "brightness": 0.8
  },
  "haptics": {
    "enabled": true,
    "intensity": 0.6,
    "frequencyHz": 80.0
  }
}
```

### Validation Features

- **Range validation**: All numeric values are validated against realistic ranges
- **Monotonic curves**: Force feedback curves must be monotonically increasing
- **Sorted arrays**: RPM bands must be in ascending order
- **Business rules**: Additional validation beyond JSON Schema
- **Line/column errors**: Precise error reporting for invalid JSON

### Migration Support

The crate supports schema migration for backward compatibility:

```rust
use racing_wheel_schemas::config::ProfileMigrator;

let migrated_profile = ProfileMigrator::migrate_profile(old_json)?;
```

## Protobuf Integration

### Service Definition

The `wheel.proto` file defines the gRPC service interface:

```protobuf
service WheelService {
  rpc ListDevices(google.protobuf.Empty) returns (stream DeviceInfo);
  rpc GetActiveProfile(DeviceId) returns (Profile);
  rpc ApplyProfile(ApplyProfileRequest) returns (OpResult);
  // ... more methods
}
```

### Code Generation

Protobuf types are generated during compilation via `build.rs`. If `protoc` is not available, stub types are generated to allow compilation.

### Compatibility Checks

Use `buf` for schema compatibility validation:

```bash
cd crates/schemas
buf lint
buf breaking --against '.git#branch=main'
```

## Development

### Prerequisites

For full protobuf support:
- Install `protoc` (Protocol Buffer Compiler)
- Install `buf` CLI tool for schema management

### Building

```bash
cargo build -p racing-wheel-schemas
```

### Testing

```bash
cargo test -p racing-wheel-schemas
```

### Running Examples

```bash
cargo run -p racing-wheel-schemas --example schema_validation
```

### Schema Compatibility

Run compatibility checks:

```bash
# Linux/macOS
./scripts/check-compatibility.sh

# Windows
.\scripts\check-compatibility.ps1
```

## CI Integration

The crate includes GitHub Actions workflows for:
- Schema validation and compatibility checks
- Protobuf compilation verification
- JSON Schema syntax validation
- Generated code freshness checks

## Error Handling

The crate provides detailed error types for different validation scenarios:

```rust
use racing_wheel_schemas::config::SchemaError;

match validator.validate_json(json) {
    Ok(profile) => { /* use profile */ },
    Err(SchemaError::ValidationError { path, message }) => {
        eprintln!("Validation error at {}: {}", path, message);
    },
    Err(SchemaError::NonMonotonicCurve) => {
        eprintln!("Curve points must be monotonically increasing");
    },
    Err(e) => {
        eprintln!("Other error: {}", e);
    }
}
```

## Architecture

The crate follows clean architecture principles:

- **Domain layer**: Pure domain types with no external dependencies
- **Schema layer**: JSON Schema and protobuf definitions
- **Validation layer**: Business rule validation and migration logic
- **Generated layer**: Auto-generated types from schemas

This ensures that core business logic remains independent of serialization formats and external protocols.
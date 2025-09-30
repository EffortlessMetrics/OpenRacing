# Design Document

## Overview

This design document outlines the technical approach for resolving the remaining compilation issues in the racing wheel project workspace. The solution focuses on establishing clear API ownership, enforcing dependency governance, and implementing systematic verification to prevent regression.

The core strategy involves creating a single source of truth for shared types, standardizing async patterns, implementing workspace-level dependency management, and establishing comprehensive CI gates to catch compilation issues early.

## Architecture

### API Ownership Model

```
racing-wheel-schemas (Source of Truth)
├── Core domain types (DeviceId, TorqueNm, etc.)
├── Configuration types (BaseSettings, FilterConfig)
├── Telemetry types (with new field names)
└── Public prelude module

Generated IPC Crate (from Protobuf)
├── Service trait definitions
├── Request/Response types
└── gRPC client/server stubs

Consumer Crates
├── racing-wheel-service (IPC server impl)
├── wheelctl (IPC client + CLI logic)
├── racing-wheel-plugins (stable ABI consumer)
└── racing-wheel-integration-tests (public API only)
```

### Dependency Management Strategy

**Workspace-Level Governance:**
- All shared dependencies defined in `[workspace.dependencies]`
- Version pinning for critical async/serialization crates
- Feature flag consistency across dependent crates
- MSRV enforcement via rust-toolchain.toml

**Critical Dependencies to Pin:**
- tokio = "1.39" (async runtime)
- tonic = "0.12" (gRPC framework)
- prost = "0.13" (protobuf serialization)
- async-trait = "0.1" (trait object compatibility)
- serde = "1.0" (serialization)

### Async Pattern Standardization

**Trait Object Pattern:**
```rust
#[async_trait]
pub trait ServiceTrait {
    async fn method(&self) -> Result<Response, Error>;
}

// Implementation
#[async_trait]
impl ServiceTrait for ConcreteService {
    async fn method(&self) -> Result<Response, Error> {
        // Implementation
    }
}
```

**Forbidden Patterns:**
- Manual `Box<dyn Future<...>>` in public APIs
- GATs in cross-crate trait definitions
- Ad-hoc async closures in trait bounds

## Components and Interfaces

### Schema Crate Restructuring

**Public API Surface:**
```rust
// racing-wheel-schemas/src/lib.rs
pub mod prelude {
    pub use crate::domain::*;
    pub use crate::config::*;
    pub use crate::telemetry::*;
}

// Explicit re-exports only
pub use prelude::*;
```

**Type Consolidation:**
- Single `DeviceId` type with conversion traits
- Unified `BaseSettings` with all required fields
- Updated `TelemetryData` with new field names only
- `FilterConfig` with proper Default implementation

### Service Layer Refactoring

**IPC Contract Alignment:**
```rust
// Generated from protobuf
pub trait WheelService {
    async fn get_devices(&self) -> Result<DeviceList, ServiceError>;
    async fn apply_profile(&self, req: ProfileRequest) -> Result<(), ServiceError>;
}

// Implementation must match exactly
#[async_trait]
impl WheelService for WheelServiceImpl {
    async fn get_devices(&self) -> Result<DeviceList, ServiceError> {
        // Implementation using schema types
    }
}
```

**Dependency Injection Pattern:**
```rust
pub struct WheelServiceImpl<D, P> 
where
    D: DeviceRepository + Send + Sync,
    P: ProfileRepository + Send + Sync,
{
    device_repo: Arc<D>,
    profile_repo: Arc<P>,
}
```

### CLI Tool Restructuring

**Schema Import Pattern:**
```rust
// wheelctl/src/main.rs
use racing_wheel_schemas::prelude::*;
use racing_wheel_ipc::WheelServiceClient;

// No direct crate::internal imports
// All types from public prelude
```

**Configuration Handling:**
```rust
fn create_filter_config() -> FilterConfig {
    FilterConfig {
        bumpstop: BumpstopConfig::default(),
        hands_off: HandsOffConfig::default(),
        torque_cap: Some(TorqueNm(10.0)),
        ..Default::default()
    }
}
```

### Plugin System ABI Design

**Stable ABI Definition:**
```rust
// racing-wheel-plugins/src/abi.rs
pub const PLUG_ABI_VERSION: &str = "1.0";

#[repr(C)]
pub struct PluginManifest {
    pub abi_version: [u8; 8],
    pub name: [u8; 64],
    pub capabilities: u32,
}

#[repr(C)]
pub struct TelemetryFrame {
    pub timestamp_us: u64,
    pub wheel_angle_deg: f32,
    pub wheel_speed_rad_s: f32,
    pub temperature_c: f32,
    pub fault_flags: u32,
}
```

**WASM Host Interface:**
```rust
pub trait PluginHost {
    fn load_plugin(&mut self, manifest: &PluginManifest) -> Result<PluginId, PluginError>;
    fn process_telemetry(&mut self, plugin_id: PluginId, frame: &TelemetryFrame) -> Result<(), PluginError>;
    fn unload_plugin(&mut self, plugin_id: PluginId) -> Result<(), PluginError>;
}
```

## Data Models

### Schema Type Definitions

**Core Domain Types:**
```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DeviceId(String);

impl DeviceId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl Display for DeviceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for DeviceId {
    type Err = DeviceIdError;
    
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err(DeviceIdError::Empty);
        }
        Ok(DeviceId(s.to_string()))
    }
}
```

**Configuration Types:**
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseSettings {
    pub device_id: DeviceId,
    pub profile_id: ProfileId,
    pub safety_enabled: bool,
    pub max_torque: TorqueNm,
}

impl Default for BaseSettings {
    fn default() -> Self {
        Self {
            device_id: DeviceId::new("default"),
            profile_id: ProfileId::new("default"),
            safety_enabled: true,
            max_torque: TorqueNm(10.0),
        }
    }
}
```

**Updated Telemetry Types:**
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryData {
    pub timestamp_us: u64,
    pub wheel_angle_deg: f32,        // Updated from wheel_angle_mdeg
    pub wheel_speed_rad_s: f32,      // Updated from wheel_speed_mrad_s
    pub temperature_c: f32,          // Updated from temp_c
    pub fault_flags: u32,            // Updated from faults
    // sequence field removed
}
```

### Workspace Dependency Configuration

**Cargo.toml Structure:**
```toml
[workspace]
members = [
    "crates/racing-wheel-schemas",
    "crates/racing-wheel-engine", 
    "crates/racing-wheel-service",
    "crates/wheelctl",
    "crates/racing-wheel-plugins",
    "crates/racing-wheel-integration-tests",
    "crates/compat"
]

[workspace.dependencies]
tokio = { version = "1.39", features = ["full"] }
tonic = "0.12"
prost = "0.13"
async-trait = "0.1"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
thiserror = "1.0"
tracing = "0.1"
```

## Error Handling

### Compilation Error Categories

**Type Mismatch Errors:**
- Root cause: Duplicate type definitions across crates
- Solution: Single source of truth in schemas crate
- Prevention: Trybuild tests for removed tokens

**Async Trait Errors:**
- Root cause: Inconsistent async patterns
- Solution: Standardize on async-trait for all trait objects
- Prevention: Clippy rules and compile-time checks

**Dependency Version Conflicts:**
- Root cause: Per-crate dependency specifications
- Solution: Workspace-level dependency management
- Prevention: cargo udeps and version consistency checks

**Missing Import Errors:**
- Root cause: Private module access across crates
- Solution: Public prelude with explicit re-exports
- Prevention: Integration tests using public API only

### Error Recovery Strategies

**Incremental Compilation:**
```bash
# Isolate failures by crate
cargo build -p racing-wheel-schemas
cargo build -p wheelctl
cargo build -p racing-wheel-service
cargo build -p racing-wheel-plugins
```

**Dependency Resolution:**
```bash
# Check for version conflicts
cargo tree --duplicates
cargo udeps --all-targets
```

**Schema Validation:**
```bash
# Verify protobuf compatibility
buf breaking --against main
# Verify JSON schema round-trip
cargo test -p racing-wheel-schemas schema_roundtrip
```

## Testing Strategy

### Compilation Verification

**Isolation Testing:**
- Each crate must compile independently
- Feature flag combinations tested separately
- No-default-features builds verified

**Cross-Crate Integration:**
- Public API surface testing only
- No private module imports in tests
- Schema compatibility validation

**Trybuild Guards:**
```rust
#[test]
fn test_removed_telemetry_fields() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile-fail/wheel_angle_mdeg.rs");
    t.compile_fail("tests/compile-fail/temp_c.rs");
    t.compile_fail("tests/compile-fail/sequence_field.rs");
}
```

### Regression Prevention

**CI Pipeline Structure:**
```yaml
jobs:
  workspace-compile:
    - cargo build --workspace
    - cargo build --workspace --all-features
    - cargo build --workspace --no-default-features
  
  isolation-compile:
    - cargo build -p wheelctl
    - cargo build -p racing-wheel-service
    - cargo build -p racing-wheel-plugins
  
  schema-validation:
    - buf breaking --against main
    - cargo test -p racing-wheel-schemas
  
  lint-gates:
    - cargo clippy --workspace -- -D warnings
    - cargo fmt --check
    - cargo udeps --all-targets
```

**Automated Verification:**
- Deprecated token detection via grep/trybuild
- API signature consistency checks
- Dependency version drift detection
- Plugin ABI compatibility validation

### Performance Considerations

**Compilation Time:**
- Parallel crate compilation where possible
- Incremental builds for CI efficiency
- Feature flag optimization to reduce build matrix

**Runtime Impact:**
- No performance impact from compilation fixes
- Async trait overhead acceptable for non-RT code
- Plugin ABI designed for zero-copy where possible

This design ensures systematic resolution of compilation issues while establishing governance to prevent future regressions. The approach prioritizes maintainability and clear ownership boundaries between crates.
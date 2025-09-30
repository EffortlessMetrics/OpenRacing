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

**ONLY Pattern for Public Cross-Crate Traits:**
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

**Forbidden Patterns (enforced by trybuild):**
- Manual `Box<dyn Future<...>>` in public APIs
- `-> impl Future<Output=T>` in public trait definitions
- GATs in cross-crate trait definitions
- Ad-hoc async closures in trait bounds

**Trybuild Guard:**
```rust
// tests/compile-fail/public_future_trait.rs
trait BadPublicApi<T> {
    // should fail: public trait cannot expose impl Future
    fn do_it(&self) -> impl core::future::Future<Output=T>;
}
```

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

// NO root re-export - force explicit prelude usage
// Removed: pub use prelude::*;
```

**Domain vs Wire Type Separation:**
```rust
// Domain types (internal business logic)
pub mod domain {
    pub struct DeviceId(String);
    pub struct TelemetryData { /* business fields */ }
}

// Wire types (generated from protobuf in racing-wheel-ipc)
// Keep prost types separate, use conversion layer
impl From<ipc::DeviceInfo> for domain::Device {
    fn from(wire: ipc::DeviceInfo) -> Self {
        // Lossless conversion with validation
    }
}
```

**Type Consolidation:**
- Single `DeviceId` type with safe construction and validation
- Unified `BaseSettings` with all required fields
- Updated `TelemetryData` with new field names only
- `FilterConfig` with stable 1kHz-safe defaults

### Service Layer Refactoring

**IPC Contract with Conversion Layer:**
```rust
// Generated from protobuf (racing-wheel-ipc crate)
pub trait WheelService {
    async fn get_devices(&self) -> Result<ipc::DeviceList, ServiceError>;
    async fn apply_profile(&self, req: ipc::ProfileRequest) -> Result<(), ServiceError>;
}

// Service implementation with domain/wire separation
#[async_trait]
impl WheelService for WheelServiceImpl {
    async fn get_devices(&self) -> Result<ipc::DeviceList, ServiceError> {
        let domain_devices = self.device_repo.list_devices().await?;
        Ok(domain_devices.into()) // Convert domain -> wire
    }
}

// Conversion layer (lossless, validated)
impl From<domain::Device> for ipc::DeviceInfo {
    fn from(device: domain::Device) -> Self {
        // Lossless conversion with unit documentation
    }
}

impl TryFrom<ipc::TelemetryData> for domain::TelemetryData {
    type Error = ValidationError;
    
    fn try_from(wire: ipc::TelemetryData) -> Result<Self, Self::Error> {
        // Validate units and ranges
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

**Schema Import Pattern (Public API Only):**
```rust
// wheelctl/src/main.rs
use racing_wheel_schemas::prelude::*;  // Explicit prelude usage
use racing_wheel_ipc::WheelServiceClient;

// FORBIDDEN: use racing_wheel_schemas::internal::*;
// FORBIDDEN: use crate::foo::bar across crate boundaries
// CI enforces: grep -r "use crate::" integration-tests/ && exit 1
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

**Stable ABI Definition with Handshake:**
```rust
// racing-wheel-plugins/src/abi.rs
pub const PLUG_ABI_VERSION: u32 = 0x0001_0000; // major<<16 | minor
pub const PLUG_ABI_MAGIC: u32 = 0x57574C31;    // 'WWL1' in LE

bitflags::bitflags! {
    #[repr(transparent)]
    pub struct Cap: u32 {
        const TELEMETRY    = 0b0001;
        const LEDS         = 0b0010;
        const HAPTICS      = 0b0100;
        const RESERVED     = 0xFFFF_FFF8;
    }
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct PluginHeader {
    pub magic: u32,        // LE
    pub abi_version: u32,  // LE
    pub caps: u32,         // Cap bits
    pub reserved: u32,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct TelemetryFrame {
    pub timestamp_us: u64,       // LE, microseconds
    pub wheel_angle_deg: f32,    // degrees (not millidegrees)
    pub wheel_speed_rad_s: f32,  // rad/s (not mrad/s)
    pub temperature_c: f32,      // °C (not temp_c)
    pub fault_flags: u32,        // bitfield, LE (not faults)
    pub _pad: u32,               // align to 8 bytes
}

// Compile-time size assertion
static_assertions::const_assert!(std::mem::size_of::<TelemetryFrame>() == 32);
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

**Core Domain Types (Safe and Ergonomic):**
```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DeviceId(String); // Keep inner field private

impl std::str::FromStr for DeviceId {
    type Err = DeviceIdError;
    
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err(DeviceIdError::Empty);
        }
        // Add validation and normalization here
        Ok(DeviceId(s.to_string()))
    }
}

impl std::fmt::Display for DeviceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TryFrom<String> for DeviceId {
    type Error = DeviceIdError;
    
    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl AsRef<str> for DeviceId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

// Remove the infallible new() constructor - force validation
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
            device_id: "default".parse().expect("valid default device id"),
            profile_id: "default".parse().expect("valid default profile id"),
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

### Workspace Dependency Configuration with Governance

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
# Pin critical async/serialization crates to prevent skew
tokio = { version = "1.39", features = ["full"] }
tonic = "0.12"
prost = "0.13"
async-trait = "0.1"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
thiserror = "1.0"
tracing = "0.1"

# Add cargo-hakari for feature unification
[workspace.metadata.hakari]
resolver = "2"
```

**Toolchain Pinning:**
```toml
# rust-toolchain.toml
[toolchain]
channel = "1.76"
components = ["rustfmt", "clippy"]
```

**FilterConfig Stable Defaults:**
```rust
impl Default for FilterConfig {
    fn default() -> Self {
        Self {
            // Stable at 1kHz - no oscillation or clipping
            reconstruction: 0.0,
            friction: 0.0,
            damper: 0.0,
            inertia: 0.0,
            slew_rate: 1.0,
            notch_filters: Vec::new(),
            bumpstop: BumpstopConfig::default(),
            hands_off: HandsOffConfig::default(),
            torque_cap: Some(TorqueNm(10.0)), // Explicit for tests
        }
    }
}
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

**CI Pipeline Structure (Cross-Platform):**
```yaml
strategy:
  matrix:
    os: [ubuntu-latest, windows-latest]
runs-on: ${{ matrix.os }}

jobs:
  workspace-compile:
    - cargo build --workspace --locked
    - cargo build --workspace --all-features
    - cargo build --workspace --no-default-features
  
  isolation-compile:
    # Run isolation builds first for faster feedback
    - cargo build -p wheelctl
    - cargo build -p racing-wheel-service
    - cargo build -p racing-wheel-plugins
  
  dependency-governance:
    - cargo tree --duplicates && exit 1  # Fail on version conflicts
    - cargo hakari generate && git diff --exit-code  # Feature unification
    - cargo +nightly -Z minimal-versions update
    - cargo +nightly build --workspace -Z minimal-versions
  
  schema-validation:
    - buf breaking --against main
    - cargo test -p racing-wheel-schemas schema_roundtrip
    - cargo test -p racing-wheel-schemas trybuild
  
  lint-gates:
    - RUSTFLAGS="-D warnings -D unused_must_use" cargo clippy --workspace
    - cargo fmt --check
    - cargo udeps --all-targets
    - rg -n 'pub use .*::\*;' crates/ && exit 1  # Ban glob re-exports
  
  compile-time-guards:
    - cargo test -p racing-wheel-schemas --test trybuild
    - cargo test defaults_are_stable_at_1khz
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

## PR Template and Governance

**Required PR Template Sections:**
```markdown
## Migration Notes
- [ ] What changed in types/traits?
- [ ] Breaking changes to public APIs?
- [ ] Field name updates or removals?

## Schema/API Versioning  
- [ ] Protobuf package version (wheel.v1) - bump only on break
- [ ] JSON schema version updated if needed
- [ ] Backward compatibility maintained?

## Compat Impact
- [ ] Delta of compat usage count: +/- X usages
- [ ] Compat debt trending down?
- [ ] Removal ticket created for next minor?

## Verification
- [ ] All isolation builds pass (-p wheelctl, -p service, -p plugins)
- [ ] Cross-platform CI green (Linux + Windows)
- [ ] Trybuild guards updated for removed tokens
- [ ] Doc tests compile and pass
```

**Automated Governance Enforcement:**
- Deprecated token detection: `rg -n 'wheel_angle_mdeg|temp_c|sequence' && exit 1`
- Glob re-export ban: `rg -n 'pub use .*::\*;' crates/ && exit 1`
- Cross-crate private imports: `grep -r "use crate::" integration-tests/ && exit 1`
- Compat usage trending: emit count, fail if increasing commit-over-commit

This design ensures systematic resolution of compilation issues while establishing mechanical governance to prevent future regressions. The approach prioritizes maintainability, clear ownership boundaries, and automated verification of API consistency across the workspace.
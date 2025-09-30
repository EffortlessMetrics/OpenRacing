# Requirements Document

## Header

**Status:** Draft for review  
**Owners:** CLI (@owner-cli), Service (@owner-svc), Plugins (@owner-plug), Schemas (@owner-sch)  
**MSRV:** 1.76 (pin in rust-toolchain.toml)  
**Toolchain:** tokio 1.39, tonic 0.12, prost 0.13, async-trait 0.1.x  
**Platforms:** Windows 10+ (x86-64), Linux (x86-64)  
**Build policy:** non-test crates build with -D warnings -D unused_must_use  

## Introduction

This specification addresses the remaining compilation issues discovered during system integration validation of the racing wheel project. While the core engine and schema components compile successfully, significant compilation errors remain in the CLI tools, service layer, plugin system, and integration tests.

The goal is to systematically resolve all remaining compilation errors to achieve a fully compilable workspace with enforced governance, API ownership, and regression prevention.

## Principles & Ownership

**Single source of truth:**
- Types shared across crates live in racing-wheel-schemas and generated IPC crates
- Service owns IPC server traits; CLI owns thin client; Plugins consume stable PLUG-ABI
- Async pattern: async_trait for trait objects; GATs only in internal code where needed

**Version governance:**
- All workspace crates inherit versions via [workspace.dependencies]
- tonic/prost/tokio versions are identical across crates
- MSRV pinned; OnceLock permitted

## Build & Test Matrix

| Job | Command | Features/Targets | Purpose |
|-----|---------|------------------|---------|
| WS-all | cargo build --workspace --all-targets | default | Baseline compile |
| WS-all-features | cargo build --workspace --all-targets --all-features | all | Feature drift |
| WS-no-default | cargo build --workspace --no-default-features | none | Optional deps |
| CLI-only | -p wheelctl | default | CLI isolation |
| SVC-only | -p racing-wheel-service | default | Service isolation |
| PLUG-only | -p racing-wheel-plugins | default | ABI/host isolation |
| Tests | cargo test --workspace | default | End-to-end compile |
| Trybuild | cargo test -p racing-wheel-schemas --tests | trybuild | compile-fail API guards |

## Requirements

### CLI (wheelctl)

**CLI-01 (MUST, Test, @owner-cli):** wheelctl compiles (cargo build -p wheelctl)

**User Story:** As a developer, I want the wheelctl CLI tool to compile successfully, so that I can use command-line interfaces to interact with the racing wheel system.

#### Acceptance Criteria
1. WHEN building wheelctl in isolation THEN it SHALL compile without errors
2. WHEN wheelctl references BaseSettings THEN it SHALL use correct import paths from racing-wheel-schemas
3. WHEN wheelctl uses FilterConfig THEN it SHALL handle all required fields with proper defaults

**CLI-02 (MUST, Inspection, @owner-cli):** Uses BaseSettings, FilterConfig from racing-wheel-schemas with correct paths and defaults

**CLI-03 (MUST, Test, @owner-cli):** New schema fields used (temperature_c, fault_flags, etc.); no deprecated tokens

**CLI-04 (SHOULD, Test, @owner-cli):** cargo clippy -p wheelctl -D warnings clean

### Service (racing-wheel-service)

**SVC-01 (MUST, Test, @owner-svc):** Compiles in isolation (-p racing-wheel-service)

**User Story:** As a developer, I want the racing-wheel-service crate to compile successfully, so that the main service daemon can be built and deployed.

#### Acceptance Criteria
1. WHEN building the racing-wheel-service crate THEN it SHALL compile without errors
2. WHEN service layer references schema types THEN it SHALL use correct API signatures between crates
3. WHEN service layer uses async traits THEN it SHALL resolve trait object compatibility issues using async_trait

**SVC-02 (MUST, Test, @owner-svc):** IPC service impls match generated tonic signatures (no manual type aliases)

**SVC-03 (MUST, Test, @owner-svc):** Async trait objects compile without lifetime or object-safety errors (async_trait)

**SVC-04 (MUST, Test, @owner-svc):** Dependency versions (tokio/tonic/prost) match workspace pins

**SVC-05 (SHOULD, Demo, @owner-svc):** Service binary links; --help runs

### Plugins (racing-wheel-plugins)

**PLUG-ABI-01 (MUST, Analysis, @owner-plug):** Define PLUG_ABI = "1.0" (semver)

**PLUG-01 (MUST, Test, @owner-plug):** Build -p racing-wheel-plugins (WASM host + native helper with feature flags)

**User Story:** As a developer, I want the racing-wheel-plugins crate to compile successfully, so that the plugin architecture can be used to extend system functionality.

#### Acceptance Criteria
1. WHEN building the racing-wheel-plugins crate THEN it SHALL compile without errors
2. WHEN plugin system references core types THEN it SHALL use compatible type signatures across crate boundaries
3. WHEN plugin system implements WASM hosting THEN it SHALL resolve trait and lifetime issues

**PLUG-02 (MUST, Test, @owner-plug):** WASM host compiles with declared capability manifest types from schemas

**PLUG-03 (MUST, Test, @owner-plug):** Native helper compiles; IPC/shm types match host crate (no mismatched repr)

**PLUG-04 (SHOULD, Test, @owner-plug):** Sample plugin compiles and loads behind feature flag

### Integration Tests

**IT-01 (MUST, Test, @owner-svc):** racing-wheel-integration-tests compiles; resolves workspace dev-deps

**User Story:** As a developer, I want the integration tests to compile successfully, so that I can validate end-to-end system functionality.

#### Acceptance Criteria
1. WHEN building racing-wheel-integration-tests THEN it SHALL compile without errors
2. WHEN integration tests reference dependencies THEN it SHALL resolve version conflicts and missing imports
3. WHEN integration tests use test fixtures THEN it SHALL handle schema compatibility correctly

**IT-02 (MUST, Test, @owner-svc):** Fixtures use schema compatibility (no deprecated field names)

**IT-03 (SHOULD, Test, @owner-svc):** Tests link against public APIs only (no private module reach-through)

### Cross-crate API

**API-01 (MUST, Analysis, @owner-sch):** No duplicate types across crates (one DeviceId, one BaseSettings)

**User Story:** As a developer, I want consistent APIs between crates, so that components can integrate without signature mismatches.

#### Acceptance Criteria
1. WHEN one crate exports a type THEN other crates SHALL use the same type signature when importing
2. WHEN schema types are updated THEN all consuming crates SHALL use the updated field names and structures
3. WHEN trait definitions change THEN all implementations SHALL match the updated signatures

**API-02 (MUST, Trybuild, @owner-sch):** Compile-fail tests guard removed tokens (e.g., wheel_angle_mdeg)

**API-03 (MUST, Test, @owner-sch):** FromStr/TryFrom implemented where IDs need validation (e.g., DeviceId)

**API-04 (SHOULD, Inspection, @owner-sch):** Public re-exports via prelude only; no pub use *

### Dependencies & Versions

**DEP-01 (MUST, Test, @owner-sch):** [workspace.dependencies] pins common deps; no per-crate drift

**User Story:** As a developer, I want consistent dependency versions across the workspace, so that there are no version conflicts during compilation.

#### Acceptance Criteria
1. WHEN multiple crates use the same dependency THEN they SHALL use compatible versions from workspace.dependencies
2. WHEN feature flags are enabled THEN they SHALL be consistent across dependent crates
3. WHEN optional dependencies are used THEN they SHALL not create compilation conflicts

**DEP-02 (MUST, CI, @owner-sch):** cargo udeps passes; no unused deps

**DEP-03 (MUST, CI, @owner-sch):** buf breaking against main for protobuf; JSON Schema round-trip test passes

**DEP-04 (SHOULD, CI, @owner-sch):** rustfmt and clippy -D warnings pass for non-test crates

### Workspace Compile

**WS-01 (MUST, Test, @owner-svc):** cargo build --workspace passes

**User Story:** As a developer, I want the entire workspace to compile cleanly, so that I can build and test the complete system.

#### Acceptance Criteria
1. WHEN running cargo build --workspace THEN it SHALL complete without compilation errors
2. WHEN running cargo test --workspace THEN it SHALL compile all test code successfully
3. WHEN building with different feature combinations THEN compilation SHALL succeed for all valid configurations

**WS-02 (MUST, Test, @owner-svc):** cargo test --workspace compiles all tests (execution optional for this spec)

**WS-03 (SHOULD, Test, @owner-svc):** All defined feature matrices compile (--all-features, --no-default-features)

### Regression Prevention

**REG-01 (MUST, CI, @owner-sch):** Deprecated token grep/trybuild: fail if reintroduced

**User Story:** As a developer, I want compilation regression prevention, so that future changes don't reintroduce the same compilation issues.

#### Acceptance Criteria
1. WHEN CI runs compilation checks THEN it SHALL catch API signature mismatches early
2. WHEN schema changes are made THEN automated checks SHALL verify cross-crate compatibility
3. WHEN deprecated tokens are reintroduced THEN trybuild tests SHALL fail compilation

**REG-02 (MUST, CI, @owner-sch):** Lint gates (-D warnings -D unused_must_use, deny clippy::unwrap_used in non-test)

**REG-03 (MUST, CI, @owner-sch):** Schema/API governance in PR template (migration notes, deprecation window)

**REG-04 (SHOULD, CI, @owner-sch):** Compat usage metric (if compat layer exists) must not increase commit-over-commit

## Verification Details

**CLI/Service/Plugins isolation builds:** use -p filters in CI to surface crate-local failures quickly

**Generated code parity:** tonic/prost regeneration runs in CI (no stale generated files)

**Async trait pattern:** standardize on #[async_trait] for trait objects; forbid ad-hoc boxed future types in public APIs

**Plugin ABI:** #[repr(C)] for shared structs; #[cfg(test)] layout asserts; version string exported from crate root

## Acceptance Matrix

| Req ID | CI Job / Test | Status |
|--------|---------------|--------|
| CLI-01 | cargo build -p wheelctl | ☐ |
| SVC-02 | svc_tonic_sig_check (unit) | ☐ |
| PLUG-ABI-01 | plugins_abi_version_check (unit) | ☐ |
| API-02 | trybuild::telemetry_removed_fields | ☐ |
| DEP-03 | buf breaking --against main | ☐ |
| WS-01 | cargo build --workspace | ☐ |
| REG-02 | clippy -D warnings | ☐ |
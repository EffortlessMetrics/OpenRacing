# Implementation Plan

This implementation plan converts the remaining compilation fixes design into discrete, manageable coding steps that establish API ownership, enforce dependency governance, and implement systematic verification to prevent regression.

The plan follows a specific execution order: establish workspace governance first, then fix type boundaries, standardize async patterns, resolve cross-crate issues, and finally implement comprehensive CI gates.

## Task List

- [ ] 1. Establish workspace dependency governance and toolchain pinning
  - Pin rust-toolchain.toml to MSRV 1.89 and edition 2024 with required components
  - Move all shared dependencies to [workspace.dependencies] in root Cargo.toml
  - Pin critical async/serialization crates: tokio 1.39, tonic 0.12, prost 0.13, async-trait 0.1
  - Add cargo-hakari configuration for feature unification
  - Update all crate Cargo.toml files to inherit from workspace dependencies
  - Verify no version conflicts with `cargo tree --duplicates`
  - **DoD:** All crates use workspace deps; no version conflicts; toolchain pinned; hakari configured
  - _Requirements: DEP-01, DEP-02, REG-02_

- [ ] 2. Restructure schemas crate with proper API boundaries
  - Remove root `pub use prelude::*` re-export from racing-wheel-schemas/lib.rs
  - Force explicit prelude usage: consumers must use `racing_wheel_schemas::prelude::*`
  - Separate domain types from wire types (keep prost types in racing-wheel-ipc only)
  - Implement safe DeviceId with private inner field and validation via FromStr/TryFrom
  - Add AsRef<str> and Display traits for DeviceId ergonomics
  - Remove infallible DeviceId::new() constructor to force validation
  - **DoD:** No root re-exports; DeviceId validates input; prelude explicit; domain/wire separated
  - _Requirements: API-01, API-04, CLI-02_

- [ ] 3. Update FilterConfig with stable 1kHz-safe defaults
  - Implement FilterConfig::default() with stable values (reconstruction=0, friction=0, damper=0, inertia=0, slew_rate=1.0)
  - Set explicit torque_cap default to Some(TorqueNm(10.0)) for test predictability
  - Add empty Vec::new() for notch_filters default
  - Use BumpstopConfig::default() and HandsOffConfig::default() for complex fields
  - Write unit test `defaults_are_stable_at_1khz()` to verify no oscillation
  - **DoD:** FilterConfig::default() compiles; stable at 1kHz; test validates stability
  - _Requirements: CLI-02, CLI-03_

- [ ] 4. Fix TelemetryData field references with new schema names
  - Update all references from wheel_angle_mdeg to wheel_angle_deg
  - Update all references from wheel_speed_mrad_s to wheel_speed_rad_s  
  - Update all references from temp_c to temperature_c
  - Update all references from faults to fault_flags
  - Remove all references to removed sequence field
  - Update CLI code to use new field names consistently
  - **DoD:** All deprecated field names removed; new names used; CLI compiles with new fields
  - _Requirements: CLI-03, IT-02_

- [ ] 5. Standardize async trait patterns across all crates
  - Add #[async_trait] to all public cross-crate trait definitions
  - Remove any manual Box<dyn Future<...>> patterns from public APIs
  - Forbid `-> impl Future<Output=T>` in public trait definitions
  - Update service layer to use async_trait consistently for IPC contracts
  - Add trybuild compile-fail test for forbidden async patterns
  - **DoD:** All public traits use async_trait; no manual Future boxing; trybuild guards active
  - _Requirements: SVC-03, API-02_

- [ ] 6. Implement IPC conversion layer between domain and wire types
  - Create From/TryFrom implementations between domain types and prost-generated types
  - Remove direct prost type usage from service layer internals
  - Add validation in TryFrom implementations for unit conversions and range checks
  - Update WheelService implementation to use conversion layer
  - Ensure service layer only exposes wire types in IPC contracts
  - **DoD:** Domain/wire types separated; conversion layer implemented; service uses conversions
  - _Requirements: SVC-02, SVC-04_

- [ ] 7. Fix wheelctl CLI compilation with proper schema imports
  - Update wheelctl to use `racing_wheel_schemas::prelude::*` explicitly
  - Remove any direct internal module imports across crate boundaries
  - Fix BaseSettings import paths and usage
  - Update FilterConfig initialization to use new defaults and field names
  - Ensure CLI compiles in isolation with `cargo build -p wheelctl`
  - **DoD:** wheelctl compiles independently; uses public prelude only; no internal imports
  - _Requirements: CLI-01, CLI-02, CLI-04_

- [ ] 8. Resolve racing-wheel-service compilation issues
  - Fix IPC service trait implementations to match generated tonic signatures
  - Update async trait usage to use async_trait pattern consistently
  - Resolve dependency version conflicts by using workspace dependencies
  - Fix any type signature mismatches between service and schema crates
  - Ensure service compiles in isolation with `cargo build -p racing-wheel-service`
  - **DoD:** Service compiles independently; IPC signatures match; async traits consistent
  - _Requirements: SVC-01, SVC-02, SVC-03, SVC-05_

- [ ] 9. Implement plugin system ABI with versioning and handshake
  - Define PLUG_ABI_VERSION constant and PLUG_ABI_MAGIC for handshake
  - Create capability bitflags (TELEMETRY, LEDS, HAPTICS) with reserved bits
  - Implement #[repr(C)] PluginHeader with magic, version, caps, and padding
  - Update TelemetryFrame to use new field names with proper alignment and size assertions
  - Add static_assertions for frame size (32 bytes) and layout compatibility
  - Replace fixed-width name arrays with out-of-band manifest metadata
  - **DoD:** Plugin ABI versioned; handshake implemented; frame layout verified; size assertions pass
  - _Requirements: PLUG-ABI-01, PLUG-02, PLUG-03_

- [ ] 10. Fix racing-wheel-plugins compilation with stable ABI
  - Update WASM host to use declared capability manifest types from schemas
  - Fix native helper IPC/shared memory types to match host crate representations
  - Ensure plugin system compiles with feature flags for WASM and native helpers
  - Add sample plugin compilation behind feature flag (no runtime execution required)
  - Ensure plugins crate compiles in isolation with `cargo build -p racing-wheel-plugins`
  - **DoD:** Plugins crate compiles; WASM host works; native helper types match; sample plugin compiles
  - _Requirements: PLUG-01, PLUG-04_

- [ ] 11. Resolve integration test compilation and dependency issues
  - Fix racing-wheel-integration-tests dependency resolution and missing imports
  - Update test fixtures to use new schema field names (no deprecated tokens)
  - Ensure integration tests only import from public APIs (no private module access)
  - Resolve workspace dev-dependency conflicts and version mismatches
  - Add CI enforcement to prevent cross-crate private imports in tests
  - **DoD:** Integration tests compile; use public APIs only; fixtures updated; deps resolved
  - _Requirements: IT-01, IT-02, IT-03_

- [ ] 12. Add trybuild compile-fail guards for removed tokens and patterns
  - Create trybuild tests that fail compilation for deprecated field names (wheel_angle_mdeg, temp_c, sequence)
  - Add compile-fail test for forbidden async patterns (-> impl Future in public traits)
  - Add compile-fail test for glob re-exports (pub use *::*)
  - Add compile-fail test for cross-crate private imports
  - Integrate trybuild tests into CI pipeline with `cargo test -p racing-wheel-schemas --test trybuild`
  - **DoD:** Trybuild guards prevent regression; deprecated tokens fail compilation; async patterns enforced
  - _Requirements: API-02, REG-01_

- [ ] 13. Implement comprehensive CI build matrix with cross-platform support
  - Add build matrix for ubuntu-latest and windows-latest
  - Implement isolation builds that run first for fast feedback (-p wheelctl, -p service, -p plugins)
  - Add workspace compilation jobs (default, all-features, no-default-features)
  - Add dependency governance checks (cargo tree --duplicates, cargo hakari, minimal-versions)
  - Add schema validation (buf breaking, JSON schema round-trip tests)
  - **DoD:** CI runs on Linux and Windows; isolation builds first; all feature combinations tested
  - _Requirements: WS-01, WS-02, WS-03, DEP-03_

- [ ] 14. Add lint gates and automated governance enforcement
  - Set RUSTFLAGS="-D warnings -D unused_must_use" for non-test crates
  - Add clippy denial of unwrap_used in non-test code
  - Implement automated checks for deprecated tokens, glob re-exports, and cross-crate private imports
  - Add cargo udeps check for unused dependencies
  - Add rustfmt --check and cargo clippy --workspace -- -D warnings to CI
  - **DoD:** Lint gates active; deprecated patterns fail CI; unused deps detected; formatting enforced
  - _Requirements: REG-02, DEP-02, DEP-04_

- [ ] 15. Create PR template and establish schema governance policy
  - Add PR template with required sections: Migration Notes, Schema/API Versioning, Compat Impact
  - Document deprecation window policy and migration patterns (rename → alias → remove)
  - Add schema stability requirements and breaking change procedures
  - Implement compat usage tracking (if compat layer exists) with trending metrics
  - Create issue template for removing compatibility shims in next minor version
  - **DoD:** PR template enforced; governance policy documented; compat tracking active; migration process defined
  - _Requirements: REG-03, REG-04_

- [ ] 16. Validate complete workspace compilation and integration
  - Verify `cargo build --workspace` passes without errors or warnings
  - Verify `cargo test --workspace` compiles all test code successfully
  - Run all isolation builds to ensure crate independence
  - Validate cross-platform compilation on both Linux and Windows
  - Confirm all feature flag combinations compile successfully
  - Run comprehensive test suite to ensure no runtime regressions
  - **DoD:** Full workspace compiles cleanly; all tests compile; cross-platform verified; feature combinations work
  - _Requirements: WS-01, WS-02, CLI-01, SVC-01, PLUG-01, IT-01_

## Critical Path Dependencies

```
1 (Workspace Deps) → 2 (Schema API) → 3 (FilterConfig) → 4 (TelemetryData)
                                                              ↓
5 (Async Patterns) → 6 (IPC Conversion) → 7 (CLI) → 8 (Service) → 9 (Plugin ABI)
                                                              ↓
10 (Plugins) → 11 (Integration Tests) → 12 (Trybuild) → 13 (CI Matrix)
                                                              ↓
14 (Lint Gates) → 15 (Governance) → 16 (Final Validation)
```

## Definition of Done (DoD) Criteria

**Workspace Compilation DoD:** `cargo build --workspace` passes; `cargo test --workspace` compiles; no version conflicts; all feature combinations work

**API Boundaries DoD:** No root re-exports; explicit prelude usage; domain/wire separation; DeviceId validates input; trybuild guards active

**Async Patterns DoD:** All public traits use async_trait; no manual Future boxing; compile-fail tests prevent regression

**Cross-Crate Integration DoD:** CLI, service, plugins compile in isolation; integration tests use public APIs only; no deprecated field names

**Governance DoD:** PR template enforced; lint gates active; deprecated patterns fail CI; schema governance documented; compat tracking implemented
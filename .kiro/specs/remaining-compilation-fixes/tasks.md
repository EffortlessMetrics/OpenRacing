# Implementation Plan

This implementation plan converts the remaining compilation fixes design into discrete, manageable coding steps that establish API ownership, enforce dependency governance, and implement systematic verification to prevent regression.

The plan follows a specific execution order: establish workspace governance first, then fix type boundaries, standardize async patterns, resolve cross-crate issues, and finally implement comprehensive CI gates.

## Task List

- [x] 0. Error triage and baseline establishment





  - Capture rustc JSON diagnostics: `cargo build --workspace --message-format=json > target/compile.json`
  - Summarize top error kinds per crate (API mismatch, missing fields, dependency skew)
  - Record baseline counts (errors by crate/kind) for burndown tracking
  - Create reproducible error analysis loop for progress measurement
  - Document findings in docs/build/compile-baseline.json with counts by crate/kind
  - **DoD:** Baseline written with quantified error counts; repeated run is deterministic; error categories identified
  - _Requirements: All (baseline for tracking)_

- [x] 1. Establish workspace dependency governance and toolchain pinning





  - Pin rust-toolchain.toml to MSRV 1.76 with required components (rustfmt, clippy)
  - Move all shared dependencies to [workspace.dependencies] in root Cargo.toml
  - Pin critical async/serialization crates: tokio 1.39, tonic 0.12, prost 0.13, async-trait 0.1
  - Add cargo-hakari configuration for feature unification and workspace-hack crate
  - Document crate features and ownership (service-ipc, cli-client, plugins-wasm, plugins-native)
  - Update all crate Cargo.toml files to inherit from workspace dependencies
  - Add --locked to CI builds and optional nightly -Z minimal-versions lane
  - Create central build.rs in IPC crate for deterministic protobuf/tonic generation
  - Add CI job that regenerates and fails if git diff shows uncommitted generated files
  - **DoD:** cargo tree --duplicates → 0 dupes; cargo hakari generate no diff; cargo +nightly -Z minimal-versions build passes; codegen deterministic
  - _Requirements: DEP-01, DEP-02, REG-02_

- [x] 2. Restructure schemas crate with proper API boundaries





  - Remove root `pub use prelude::*` re-export from racing-wheel-schemas/lib.rs
  - Force explicit prelude usage: consumers must use `racing_wheel_schemas::prelude::*`
  - Separate domain types from wire types (keep prost types in racing-wheel-ipc only)
  - Implement safe DeviceId with private inner field, normalization (trim, lowercase), and validation via FromStr/TryFrom only
  - Add AsRef<str> and Display traits for DeviceId ergonomics; avoid From<String> to force validation
  - Remove all infallible DeviceId constructors to make invalid input impossible
  - Add trybuild test that fails on DeviceId("...") literal hacks
  - **DoD:** No pub use prelude::* at root; DeviceId construction is fallible only; trybuild rejects literal hacks; normalization implemented
  - _Requirements: API-01, API-04, CLI-02_


- [x] 3. Update FilterConfig with stable 1kHz-safe defaults




  - Implement FilterConfig::default() with stable values (reconstruction=0, friction=0, damper=0, inertia=0, slew_rate=1.0)
  - Set explicit torque_cap default to Some(TorqueNm(10.0)) for test predictability
  - Add empty Vec::new() for notch_filters default
  - Use BumpstopConfig::default() and HandsOffConfig::default() for complex fields
  - Write unit test `defaults_are_stable_at_1khz()` to verify no oscillation
  - **DoD:** FilterConfig::default() compiles; stable at 1kHz; test validates stability
  - _Requirements: CLI-02, CLI-03_


- [x] 4. Fix TelemetryData field references with explicit units



  - Confirm wheel_angle_deg is degrees (not milli-degrees); rename to _mdeg if needed
  - Confirm wheel_speed_rad_s is rad/s (not mrad/s); rename to _mrad_s if needed
  - Update all references from temp_c to temperature_c with °C unit documentation
  - Update all references from faults to fault_flags with bitfield documentation
  - Remove all references to removed sequence field
  - Add doc comments on each field with explicit units (degrees, rad/s, °C)
  - Update CLI code to use new field names consistently
  - **DoD:** No deprecated tokens in workspace (rg check); unit docstrings show degrees/rad/s/°C; CLI compiles with new fields
  - _Requirements: CLI-03, IT-02_

- [x] 5. Standardize async trait patterns with compile-time guards





  - Add #[async_trait] to all public cross-crate trait definitions
  - Remove any manual Box<dyn Future<...>> patterns from public APIs
  - Forbid `-> impl Future<Output=T>` in public trait definitions
  - Update service layer to use async_trait consistently for IPC contracts
  - Add trybuild compile-fail test for public traits returning impl Future or exposing BoxFuture
  - Add CI grep to block `pub use .*::\*;` (glob re-exports) across workspace
  - **DoD:** All public traits use #[async_trait]; trybuild forbids BoxFuture/impl Future; grep guard blocks glob re-exports
  - _Requirements: SVC-03, API-02_

- [-] 6. Implement IPC conversion layer between domain and wire types


  - Create From/TryFrom implementations between domain types and prost-generated types
  - Remove direct prost type usage from service layer internals
  - Add validation in TryFrom implementations for unit conversions and range checks
  - Add unit conversion tests (degrees/rad/s/°C) and range validation tests
  - Update WheelService implementation to use conversion layer
  - Ensure service layer only exposes wire types in IPC contracts
  - **DoD:** No prost types in service internals; From/TryFrom conversions tested (unit conversion + range); service uses conversions
  - _Requirements: SVC-02, SVC-04_

- [ ] 7. Fix wheelctl CLI compilation with proper schema imports (isolation first)
  - Update wheelctl to use `racing_wheel_schemas::prelude::*` explicitly
  - Remove any direct internal module imports across crate boundaries
  - Fix BaseSettings import paths and usage
  - Update FilterConfig initialization to use new defaults and field names
  - Ensure CLI compiles in isolation with `cargo build -p wheelctl`
  - Add tiny smoke test that `wheelctl --help` runs (link check, no runtime required)
  - **DoD:** cargo build -p wheelctl compiles; wheelctl --help runs; uses public prelude only; no internal imports
  - _Requirements: CLI-01, CLI-02, CLI-04_

- [ ] 8. Resolve racing-wheel-service compilation issues (isolation first)
  - Fix IPC service trait implementations to match generated tonic signatures
  - Update async trait usage to use async_trait pattern consistently
  - Resolve dependency version conflicts by using workspace dependencies
  - Fix any type signature mismatches between service and schema crates
  - Ensure service compiles in isolation with `cargo build -p racing-wheel-service`
  - Add tiny smoke test that `racing-wheel-service --help` runs (link check, no runtime required)
  - **DoD:** cargo build -p racing-wheel-service compiles; service --help runs; IPC signatures match; async traits consistent
  - _Requirements: SVC-01, SVC-02, SVC-03, SVC-05_

- [ ] 9. Implement plugin system ABI with versioning and endianness
  - Define PLUG_ABI_VERSION constant and PLUG_ABI_MAGIC for handshake
  - Create capability bitflags (TELEMETRY, LEDS, HAPTICS) with reserved bits
  - Implement #[repr(C)] PluginHeader with magic, version, caps, and padding
  - Document little-endian for all integers; keep large metadata out-of-band (JSON/TOML manifest)
  - Update TelemetryFrame to use new field names with proper alignment and size assertions
  - Add static_assertions for align_of::<TelemetryFrame>() and size_of::<PluginHeader>()
  - Add unit test that writes header to byte slice and checks exact bytes (portable)
  - Replace fixed-width name arrays with out-of-band manifest metadata
  - **DoD:** Size/align asserts pass; byte-exact header test passes; endianness documented; ABI versioned
  - _Requirements: PLUG-ABI-01, PLUG-02, PLUG-03_

- [ ] 10. Fix racing-wheel-plugins compilation with stable ABI
  - Update WASM host to use declared capability manifest types from schemas
  - Fix native helper IPC/shared memory types to match host crate representations
  - Ensure plugin system compiles with feature flags for WASM and native helpers
  - Add sample plugin compilation behind feature flag (no runtime execution required)
  - Ensure plugins crate compiles in isolation with `cargo build -p racing-wheel-plugins`
  - **DoD:** Plugins crate compiles; WASM host works; native helper types match; sample plugin compiles
  - _Requirements: PLUG-01, PLUG-04_

- [ ] 11. Resolve integration test compilation with public API enforcement
  - Fix racing-wheel-integration-tests dependency resolution and missing imports
  - Update test fixtures to use new schema field names (no deprecated tokens)
  - Ensure integration tests only import from public APIs (no private module access)
  - Add CI grep: fail on `use .*::(tests|internal|private)` from outside the crate
  - Add #![deny(rust_2018_idioms)] and -D warnings on integration tests crate
  - Resolve workspace dev-dependency conflicts and version mismatches
  - **DoD:** Integration tests import only prelude/public modules; grep guard active; fixtures updated; deps resolved
  - _Requirements: IT-01, IT-02, IT-03_

- [ ] 12. Add trybuild compile-fail guards for removed tokens and patterns
  - Create trybuild tests that fail compilation for deprecated field names (wheel_angle_mdeg, temp_c, sequence)
  - Add compile-fail test for forbidden async patterns (-> impl Future in public traits)
  - Add compile-fail test for glob re-exports (pub use *::*)
  - Add compile-fail test for cross-crate private imports
  - Integrate trybuild tests into CI pipeline with `cargo test -p racing-wheel-schemas --test trybuild`
  - **DoD:** Trybuild guards prevent regression; deprecated tokens fail compilation; async patterns enforced
  - _Requirements: API-02, REG-01_

- [ ] 13. Implement comprehensive CI build matrix with optimized ordering
  - Add build matrix for ubuntu-latest and windows-latest
  - Run jobs in optimized order: isolation builds (CLI/SVC/PLUG) in parallel first, then schemas + trybuild, then workspace default, then all-features/no-default, then minimal-versions (nightly), finally lint gates
  - Enable sccache in CI to keep latency manageable
  - Add dependency governance checks (cargo tree --duplicates, cargo hakari, minimal-versions)
  - Add schema validation (buf breaking, JSON schema round-trip tests)
  - **DoD:** Mean CI wall time reduced; isolation failures show in <3-4 minutes; CI runs on Linux and Windows; all feature combinations tested
  - _Requirements: WS-01, WS-02, WS-03, DEP-03_

- [ ] 14. Add comprehensive lint gates and automated governance enforcement
  - Set RUSTFLAGS="-D warnings -D unused_must_use" for non-test crates
  - Deny clippy::unwrap_used, clippy::print_stdout, and static_mut_refs in non-test code
  - Implement automated checks for deprecated tokens, glob re-exports, and cross-crate private imports
  - Add cargo udeps check for unused dependencies
  - Add rustfmt --check and cargo clippy --workspace -- -D warnings to CI
  - **DoD:** Lints deny warnings, unused_must_use, unwrap_used, print_stdout, static_mut_refs; violations fail CI; unused deps detected
  - _Requirements: REG-02, DEP-02, DEP-04_

- [ ] 15. Create PR template and establish schema governance with enforcement
  - Add PR template with required sections: Migration Notes (call-site actions per crate), Schema/API change (prost package/JSON schema bump reason), Compat debt delta, CI checklist (isolation builds & schema break check links)
  - Add CODEOWNERS: schemas → @owner-sch; service IPC → @owner-svc; plugins ABI → @owner-plug
  - Document deprecation window policy and migration patterns (rename → alias → remove)
  - Add schema stability requirements and breaking change procedures
  - Implement compat usage tracking (if compat layer exists) with trending metrics printed in CI
  - Create issue template for removing compatibility shims in next minor version
  - **DoD:** Template merged; CODEOWNERS enforced; governance policy documented; compat debt trend printed in CI
  - _Requirements: REG-03, REG-04_

- [ ] 16. Validate complete workspace compilation and integration
  - Verify `cargo build --workspace` passes without errors or warnings
  - Verify `cargo test --workspace` compiles all test code successfully
  - Run all isolation builds to ensure crate independence on both Linux and Windows
  - Validate cross-platform compilation on both Linux and Windows
  - Confirm all feature flag combinations compile successfully
  - Run comprehensive test suite to ensure no runtime regressions
  - **DoD:** Workspace build/test pass; feature combos compile; isolation builds green on both OSes; no runtime regressions
  - _Requirements: WS-01, WS-02, CLI-01, SVC-01, PLUG-01, IT-01_

## Critical Path Dependencies

```
0 (Triage) → 1 (Workspace Deps) → 2 (Schema API) → 3 (FilterConfig) → 4 (TelemetryData)
                                                                            ↓
5 (Async Patterns) → 6 (IPC Conversion) → 7 (CLI) → 8 (Service) → 9 (Plugin ABI)
                                                                            ↓
10 (Plugins) → 11 (Integration Tests) → 12 (Trybuild) → 13 (CI Matrix)
                                                                            ↓
14 (Lint Gates) → 15 (Governance) → 16 (Final Validation)
```

**Execution Order for Speed:**
- Tasks 0-6: Foundation (sequential)
- Tasks 7-8: CLI and Service (can run in parallel after 6)
- Tasks 9-10: Plugin system (after 8)
- Tasks 11-12: Tests and guards (after 10)
- Tasks 13-16: CI and validation (sequential)

## Definition of Done (DoD) Criteria

**Baseline & Triage DoD:** Error baseline quantified; reproducible analysis loop; progress trackable by error count reduction

**Workspace Compilation DoD:** cargo build --workspace passes; cargo test --workspace compiles; cargo tree --duplicates → 0 dupes; all feature combinations work

**API Boundaries DoD:** No pub use prelude::* at root; DeviceId construction is fallible only; trybuild rejects literal hacks; domain/wire separated

**Async Patterns DoD:** All public traits use #[async_trait]; trybuild forbids BoxFuture/impl Future; grep guard blocks glob re-exports

**Cross-Crate Integration DoD:** cargo build -p wheelctl and -p racing-wheel-service clean; service --help runs; integration tests import only prelude/public modules

**Plugin ABI DoD:** Size/align asserts pass; byte-exact header test passes; endianness documented; ABI versioned with handshake

**Governance DoD:** Template merged; CODEOWNERS enforced; lints deny warnings/unwrap_used/print_stdout/static_mut_refs; compat debt trend printed in CI
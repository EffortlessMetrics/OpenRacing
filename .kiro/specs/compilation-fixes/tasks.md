# Implementation Plan

This implementation plan converts the compilation error fixes into a series of discrete, manageable coding steps that build incrementally to restore the codebase to a clean compilation state while establishing governance to prevent future regressions.

## Task List

- [x] 1. Resolve root export conflicts at source with canonical types





  - Keep single public FFBMode (in rt module) as canonical export
  - Rename internal enum in ffb module to PipelineMode and make it pub(crate)
  - Replace glob re-exports with explicit item lists to avoid future collisions
  - Create small public prelude module with intended surface if needed
  - Verify no duplicate symbol errors remain and prelude compiles
  - **DoD:** cargo check shows no duplicate symbols; only one public FFBMode; no glob exports; prelude compiles
  - _Requirements: 1.1, 1.2, 1.3, 1.4_

- [x] 2. Add missing standard library imports with atomic preferences





  - Add std::sync::atomic::{AtomicBool, Ordering} for boolean flags in test modules
  - Use std::sync::{Arc, Mutex} only when multi-field mutation is needed
  - Add std::time::Duration import to metrics.rs and other test modules as needed
  - Scope imports within #[cfg(test)] mod tests blocks to avoid production pollution
  - Verify all missing symbol errors are resolved
  - **DoD:** No E0433 in tests; boolean flags use AtomicBool unless locking is needed
  - _Requirements: 2.1, 2.2, 2.3, 2.4_

- [x] 3. Fix HealthEventType import path resolution





  - Add explicit import for HealthEventType in diagnostic/streams.rs
  - Use crate::diagnostic::HealthEventType or add proper use statement
  - Verify enum variant access compiles correctly without super:: ambiguities
  - **DoD:** streams.rs compiles with explicit path; no super:: ambiguities
  - _Requirements: 2.1, 2.4_

- [x] 4. Create test-only schema compatibility layer





  - Create crates/compat with TelemetryCompat trait mapping old → new field names
  - Gate compatibility layer with #[cfg(test)] so it never ships in release builds
  - Add CI metric to ensure compat usage count does not increase over time
  - Implement inline forwarding methods for all renamed fields
  - **DoD:** Compat shim exists; usage count tracked and does not increase in CI
  - _Requirements: 3.1, 3.2, 3.3, 3.4 (risk-reduced migration)_

- [ ] 5. Update TelemetryData field references using compatibility layer
  - Replace wheel_angle_mdeg with wheel_angle_deg in critical paths first
  - Replace wheel_speed_mrad_s with wheel_speed_rad_s in critical paths first
  - Replace temp_c with temperature_c in critical paths first
  - Replace faults with fault_flags in critical paths first
  - Use compatibility layer for non-critical test code during transition
  - Remove or update assertions on removed sequence field
  - **DoD:** All references use new names or compat layer; no compilation errors
  - _Requirements: 3.1, 3.2, 3.3, 3.4, 3.6_

- [ ] 6. Add missing FilterConfig fields with proper defaults
  - Add bumpstop field using BumpstopConfig::default() in initializations
  - Add hands_off field using HandsOffConfig::default() in initializations
  - Add torque_cap field with Some(TorqueNm(10.0)) or appropriate default
  - Use FilterConfig::default() with struct update syntax (..Default::default())
  - Verify all FilterConfig initializations compile successfully
  - **DoD:** All initializers compile; either ..Default::default() used or fields explicitly set
  - _Requirements: 3.5_

- [ ] 7. Fix DeviceId construction and add conversion traits
  - Remove incorrect DeviceId::new() wrapping where id is already DeviceId type
  - Implement Display and From<String>/From<DeviceId> traits for ergonomic conversions
  - Update string comparisons to use .to_string() method consistently
  - Add serde support if needed for test serialization
  - Verify DeviceId usage is consistent throughout codebase
  - **DoD:** No E0308 on DeviceId; string compares use .to_string(); conversion traits implemented
  - _Requirements: 4.1, 4.5_

- [ ] 8. Restore parametric property tests with deterministic seeds
  - Keep property tests parametric using proptest! or QuickCheck with parameters
  - Ensure wrappers forward parameters and return the property result
  - Configure tests to emit failure seeds and enable shrinking for reproducibility
  - Fix function name typos (create_test_telemetry → create_varying_telemetry)
  - Store failing seeds as CI artifacts for debugging
  - **DoD:** Properties parametric; wrappers return results; seeds logged on failure
  - _Requirements: 4.2, 4.3, 5.1, 5.4_

- [ ] 9. Handle write_ffb_report Result with RT-safe error counting
  - Use AtomicU64 counters with Ordering::Relaxed for error accounting
  - Implement non-allocating error counting in RT loop (no logging/allocation)
  - Publish aggregated counts off-thread at 1-2 Hz for diagnostics
  - Send lossy diagnostic signals to side threads using non-blocking channels
  - Verify RT performance (p99 jitter) is unchanged with error handling
  - **DoD:** Hot path increments atomic counters; no logging/allocations; perf test unchanged
  - _Requirements: 4.4, 6.3_

- [ ] 10. Replace unsafe static mut with OnceLock and add lint guard
  - Replace static mut CACHED_INFO with OnceLock<DeviceInfoCache>
  - Implement get_cached_info() function using OnceLock::get_or_init
  - Add #![deny(static_mut_refs)] at crate root (non-test) to prevent regression
  - Update all references to use safe accessor function
  - Document the rule in code style guidelines
  - **DoD:** No static_mut_refs warnings; cache access through accessor; lint guard active
  - _Requirements: 6.1, 6.2, 6.4_

- [ ] 11. Remove unused imports and variables with proper scoping
  - Remove unused imports in test modules (PI, HashMap, sleep, TelemetryData, etc.)
  - Prefix intentionally unused variables with underscore or remove them
  - Add #[cfg(test)] guards to test-only re-exports to prevent production pollution
  - Clean up comparison warnings for type limits appropriately
  - Verify no unused import warnings remain
  - **DoD:** Clean compilation; test-only imports properly scoped; no unused warnings
  - _Requirements: 7.1, 7.2, 7.5_

- [ ] 12. Address dead code with appropriate allow attributes
  - Add #[allow(dead_code)] to intentionally kept helper functions with TODO links
  - Remove truly unused code or document why it's preserved for future use
  - Address unused struct fields that are part of public API appropriately
  - Verify dead code warnings are handled without hiding real issues
  - **DoD:** Dead code either removed or #[allow(dead_code)] with justification
  - _Requirements: 7.3_

- [ ] 13. Add comprehensive regression prevention and CI gates
  - Add RUSTFLAGS="-D warnings -D unused_must_use" for non-test crates
  - Implement Protobuf breaking-change checks (buf breaking --against main)
  - Add JSON Schema round-trip tests and required field validation
  - Create trybuild compile-fail tests for removed schema tokens
  - Add cargo-udeps, clippy, and rustfmt checks to CI pipeline
  - Deny clippy::unwrap_used in non-test code
  - **DoD:** CI gates active; schema break detection; lint enforcement; trybuild guards
  - _Requirements: 8.1, 8.2, 8.3, 8.4_

- [ ] 14. Establish schema/API governance and migration policy
  - Document deprecation window policy (old names marked #[deprecated] for one minor)
  - Create migration pattern documentation (rename → alias → remove)
  - Add schema stability requirements to PR template
  - Implement CI metrics to track compat layer usage trending down
  - Create issue to remove compatibility shims in next minor version
  - **DoD:** Governance policy documented; PR template updated; migration tracking active
  - _Requirements: 8.4_

- [ ] 15. Validate complete system integration and performance
  - Verify entire codebase compiles without errors or warnings
  - Run existing test suite to ensure no runtime regressions
  - Validate that virtual device integration still works correctly
  - Confirm RT engine can be instantiated and maintains performance guarantees
  - Check that all examples compile and execute properly
  - Verify p99 jitter remains ≤0.25ms with new error handling
  - **DoD:** cargo test --workspace green; examples build; virtual device demo runs; RT performance maintained
  - _Requirements: All requirements integration_
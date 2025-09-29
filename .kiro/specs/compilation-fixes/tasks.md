# Implementation Plan

This implementation plan converts the compilation error fixes into a series of discrete, manageable coding steps that build incrementally to restore the codebase to a clean compilation state.

## Task List

- [ ] 1. Resolve root export conflicts and namespace collisions
  - Replace glob re-exports (pub use rt::*; pub use ffb::*;) with explicit exports
  - Create canonical export for FFBMode from rt module
  - Add alias for conflicting FFBMode as PipelineFFBMode if needed
  - Replace remaining glob exports with explicit item lists
  - Verify no duplicate symbol errors remain
  - _Requirements: 1.1, 1.2, 1.3, 1.4_

- [ ] 2. Add missing standard library imports in test modules
  - Add std::sync::{Arc, Mutex} imports to safety/fault_injection.rs test module
  - Add std::time::Duration import to metrics.rs test module
  - Add std::time::Duration import to other test modules as needed
  - Scope imports within #[cfg(test)] mod tests blocks to avoid production pollution
  - Verify all missing symbol errors are resolved
  - _Requirements: 2.1, 2.2, 2.3, 2.4_

- [ ] 3. Fix HealthEventType import path resolution
  - Add explicit import for HealthEventType in diagnostic/streams.rs
  - Use either crate::diagnostic::HealthEventType or add use statement
  - Verify enum variant access compiles correctly
  - _Requirements: 2.1, 2.4_

- [ ] 4. Update TelemetryData field references throughout codebase
  - Replace wheel_angle_mdeg with wheel_angle_deg in all files
  - Replace wheel_speed_mrad_s with wheel_speed_rad_s in all files  
  - Replace temp_c with temperature_c in all files
  - Replace faults with fault_flags in all files
  - Remove or update assertions on removed sequence field
  - Update examples/virtual_device_demo.rs with correct field names
  - Update all test files with correct field names
  - _Requirements: 3.1, 3.2, 3.3, 3.4, 3.6_

- [ ] 5. Add missing FilterConfig fields in struct initializations
  - Add bumpstop field with appropriate default value in test initializations
  - Add hands_off field with appropriate default value in test initializations
  - Add torque_cap field with appropriate default value in test initializations
  - Use FilterConfig::default() with struct update syntax where possible
  - Verify all FilterConfig initializations compile successfully
  - _Requirements: 3.5_

- [ ] 6. Fix DeviceId construction and type mismatches
  - Remove incorrect DeviceId::new() wrapping where id is already DeviceId type
  - Fix device_list[0].id.clone() usage in examples and tests
  - Update string comparisons to use proper DeviceId methods or convert to string
  - Verify DeviceId usage is consistent throughout codebase
  - _Requirements: 4.1, 4.5_

- [ ] 7. Correct property test function signatures and return types
  - Remove parameters from property test wrapper functions to match zero-arg implementations
  - Fix return types to match TestResult or bool as declared
  - Update function calls to use correct function names (fix typos like create_test_telemetry)
  - Ensure property test functions return their helper function results
  - Verify all property tests compile and can be executed
  - _Requirements: 4.2, 4.3, 5.1, 5.4_

- [ ] 8. Handle write_ffb_report Result return value properly
  - Add proper error handling for write_ffb_report calls in RT loop
  - Use error counting instead of propagating errors in hot path
  - Implement non-allocating error accounting with atomic counters
  - Avoid logging in RT path, use side-channel diagnostics
  - Verify RT performance is not impacted by error handling
  - _Requirements: 4.4, 6.3_

- [ ] 9. Replace unsafe static mut with safe alternatives
  - Replace static mut CACHED_INFO with OnceLock<DeviceInfoCache>
  - Implement get_cached_info() function using OnceLock::get_or_init
  - Update all references to use safe accessor function
  - Verify no undefined behavior warnings remain
  - _Requirements: 6.1, 6.2, 6.4_

- [ ] 10. Remove unused imports and variables
  - Remove unused imports in test modules (PI, HashMap, sleep, TelemetryData, etc.)
  - Prefix intentionally unused variables with underscore
  - Remove completely unused variables where appropriate
  - Add #[cfg(test)] guards to test-only re-exports
  - Clean up comparison warnings for type limits
  - _Requirements: 7.1, 7.2, 7.5_

- [ ] 11. Address dead code and add appropriate allow attributes
  - Add #[allow(dead_code)] to intentionally kept helper functions
  - Remove truly unused code or document why it's kept
  - Address unused struct fields that are part of public API
  - Verify dead code warnings are appropriately handled
  - _Requirements: 7.3_

- [ ] 12. Add regression prevention measures
  - Create CI script to detect deprecated field names in codebase
  - Add compile-time test that instantiates current struct definitions
  - Set up clippy and rustfmt enforcement in CI
  - Add schema compatibility checks to prevent future breakage
  - Document field migration patterns for future schema changes
  - _Requirements: 8.1, 8.2, 8.3, 8.4_

- [ ] 13. Validate complete compilation and test execution
  - Verify entire codebase compiles without errors or warnings
  - Run existing test suite to ensure no runtime regressions
  - Validate that virtual device integration still works
  - Confirm RT engine can be instantiated and runs correctly
  - Check that all examples compile and execute properly
  - _Requirements: All requirements integration_
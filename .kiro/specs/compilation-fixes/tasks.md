# Implementation Plan

- [x] 1. Fix trait compatibility and async issues





  - Fix `DiagnosticTest` trait to be dyn-compatible by using proper async trait patterns
  - Resolve async trait object compilation errors in diagnostic service
  - _Requirements: 2.1, 2.3_

- [x] 2. Resolve import and dependency issues





  - [x] 2.1 Fix missing `ApplicationProfileService` import in ipc_service.rs


    - Correct the import path to reference the proper service type
    - _Requirements: 4.1, 4.2_
  
  - [x] 2.2 Add missing tracing_subscriber Layer import for observability


    - Import `tracing_subscriber::Layer` trait to enable `boxed()` method
    - _Requirements: 4.1, 4.3_
  
  - [x] 2.3 Remove unused imports and fix import warnings


    - Clean up unused imports across service modules
    - _Requirements: 4.1_

- [x] 3. Fix type system errors and missing fields





  - [x] 3.1 Fix ambiguous numeric type in diagnostic service


    - Specify explicit type for jitter calculation (f64)
    - Add type annotations for vector collections
    - _Requirements: 3.1, 3.2_
  
  - [x] 3.2 Add missing fields to FilterConfig struct


    - Add `bumpstop`, `hands_off`, and `torque_cap` fields with appropriate default values
    - _Requirements: 3.3_
  
  - [x] 3.3 Fix CalibrationData struct field names


    - Update `timestamp` field to `calibrated_at` to match schema
    - _Requirements: 3.3_
  
  - [x] 3.4 Fix TorqueNm construction calls


    - Replace incorrect `TorqueNm::from(0.0)` with proper constructor
    - _Requirements: 3.1, 3.2_

- [x] 4. Update configuration structures





  - [x] 4.1 Fix IpcConfig struct usage


    - Update struct initialization to match current schema fields
    - Replace `port` and `transport_type` with proper `transport` field
    - _Requirements: 3.3_
  
  - [x] 4.2 Fix TransportType enum variants


    - Update enum variant references to match current definitions
    - Replace missing `Tcp` and `Native` variants with available ones
    - _Requirements: 3.2, 3.3_

- [x] 5. Implement missing service methods





  - [x] 5.1 Add missing device service methods


    - Implement `list_devices()` method or delegate to existing functionality
    - Add `get_device_status()` method using existing device service capabilities
    - _Requirements: 2.1, 2.2_
  
  - [x] 5.2 Add missing safety service methods


    - Implement `start_high_torque()` method or delegate to `request_high_torque()`
    - _Requirements: 2.1, 2.2_
  
  - [x] 5.3 Add missing game service methods


    - Implement `get_game_status()` method for game service
    - _Requirements: 2.1, 2.2_

- [x] 6. Fix enum variant and method name issues





  - [x] 6.1 Fix SafetyViolation enum variants


    - Replace missing `HandsOff`, `OverTemperature`, `RecentFaults` variants with available ones
    - _Requirements: 3.2_
  
  - [x] 6.2 Fix FaultType enum variants


    - Replace missing `EmergencyStop` variant with available alternative
    - _Requirements: 3.2_
  

  - [x] 6.3 Fix SafetyPolicy method calls

    - Replace `default_safe_torque_nm()` and `max_torque_nm()` with `get_max_torque()`
    - _Requirements: 2.1, 2.2_

- [x] 7. Fix duration and time-related issues





  - [x] 7.1 Replace Duration::from_minutes with Duration::from_secs


    - Update duration construction to use available API methods
    - _Requirements: 3.1_
  
  - [x] 7.2 Fix device health status method calls


    - Remove incorrect `map_err` call on `DeviceHealthStatus`
    - Fix pattern matching on health status results
    - _Requirements: 3.1, 3.2_

- [x] 8. Fix async lifetime and ownership issues




  - [x] 8.1 Fix async task spawning lifetime issues





    - Resolve borrowed data escaping method body in device enumeration
    - _Requirements: 2.3_

  
  - [x] 8.2 Fix Arc cloning for service run methods

    - Add proper cloning for wheel service run method
    - _Requirements: 2.3_

- [x] 9. Add missing Default implementations







  - [x] 9.1 Add Default implementation for GameSupportMatrix

    - Implement Default trait or use alternative construction method
    - _Requirements: 2.1_
  
  - [x] 9.2 Fix ProfileId constructor calls


    - Replace `ProfileId::new()` calls with proper constructor or factory method
    - _Requirements: 3.1, 3.2_

- [x] 10. Fix remaining unwrap() usage errors





  - [x] 10.1 Replace unwrap() calls in ipc_service.rs


    - Replace `TorqueNm::new(10.0).unwrap()` with proper error handling or expect()
    - _Requirements: 6.1_
  
  - [x] 10.2 Replace unwrap() calls in profile_repository.rs


    - Replace `json_for_verification.as_object_mut().unwrap()` calls with proper error handling
    - _Requirements: 6.1_
  


  - [x] 10.3 Replace unwrap() calls in device_service.rs

    - Replace `TorqueNm::new(0.0).unwrap()` and duration unwrap calls with proper error handling
    - _Requirements: 6.1_
  
  - [x] 10.4 Replace unwrap() calls in telemetry adapters


    - Replace `"127.0.0.1:9996".parse().unwrap()` with proper error handling
    - _Requirements: 6.1_
  

  - [x] 10.5 Replace unwrap() calls in remaining modules

    - Fix unwrap calls in recorder.rs, observability.rs, and diagnostic_service.rs
    - _Requirements: 6.1_

- [x] 11. Implement proper Default traits





  - [x] 11.1 Replace custom default() methods in entities.rs


    - Convert custom `default()` methods to proper `Default` trait implementations
    - Fix `should_implement_trait` clippy warnings
    - _Requirements: 6.2_
  

  - [x] 11.2 Add Default implementations for service structs

    - Add Default trait for IRacingConfigWriter, ACCConfigWriter, and other structs with new() methods
    - _Requirements: 6.2_

- [x] 12. Fix range checks and conditional patterns





  - [x] 12.1 Replace manual range checks with contains()


    - Replace `band < 0.0 || band > 1.0` with `!(0.0..=1.0).contains(&band)`
    - Apply to all manual range checks in entities.rs, ipc_conversion.rs, and telemetry adapters
    - _Requirements: 6.3_
  

  - [x] 12.2 Collapse nested if statements

    - Simplify nested if statements using `&&` patterns in entities.rs and safety_service.rs
    - _Requirements: 6.4_

- [x] 13. Clean up unused imports and variables







  - [x] 13.1 Remove unused imports in schemas crate

    - Remove unused `BumpstopConfig` and `HandsOffConfig` imports
    - _Requirements: 6.5_

  
  - [x] 13.2 Fix unused variables

    - Prefix unused variables with underscore (e.g., `_state` in ipc_conversion.rs)
    - _Requirements: 6.5_
  

  - [x] 13.3 Remove dead code fields

    - Address unused fields in service structs or mark them as intentionally unused
    - _Requirements: 6.5_

- [ ] 14. Fix additional clippy warnings
  - [ ] 14.1 Fix needless borrows and conversions
    - Replace unnecessary `.into()` calls and borrowed expressions
    - Fix `needless_borrows_for_generic_args` warnings
    - _Requirements: 6.1_
  
  - [ ] 14.2 Improve string and character usage
    - Replace `push_str("\n")` with `push('\n')`
    - Fix unnecessary `to_string()` calls
    - _Requirements: 6.1_
  
  - [ ] 14.3 Use better comparison patterns
    - Replace manual absolute difference with `abs_diff()`
    - Use `clamp()` instead of `max().min()` patterns
    - _Requirements: 6.1_

- [ ] 15. Final verification and quality gates
  - [ ] 15.1 Verify isolation build success
    - Execute `cargo build -p racing-wheel-service --locked` to confirm compilation
    - _Requirements: 1.1, 1.2, 1.3_
  
  - [ ] 15.2 Verify strict clippy compliance
    - Execute `cargo clippy -p racing-wheel-service -- -D warnings -D clippy::unwrap_used -D clippy::expect_used`
    - _Requirements: 6.1_
  
  - [ ] 15.3 Verify workspace build compatibility
    - Execute workspace builds: `--workspace`, `--workspace --all-features`, `--workspace --no-default-features`
    - _Requirements: 7.1, 7.2, 7.3_
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

- [ ] 4. Update configuration structures
  - [ ] 4.1 Fix IpcConfig struct usage
    - Update struct initialization to match current schema fields
    - Replace `port` and `transport_type` with proper `transport` field
    - _Requirements: 3.3_
  
  - [ ] 4.2 Fix TransportType enum variants
    - Update enum variant references to match current definitions
    - Replace missing `Tcp` and `Native` variants with available ones
    - _Requirements: 3.2, 3.3_

- [ ] 5. Implement missing service methods
  - [ ] 5.1 Add missing device service methods
    - Implement `list_devices()` method or delegate to existing functionality
    - Add `get_device_status()` method using existing device service capabilities
    - _Requirements: 2.1, 2.2_
  
  - [ ] 5.2 Add missing safety service methods
    - Implement `start_high_torque()` method or delegate to `request_high_torque()`
    - _Requirements: 2.1, 2.2_
  
  - [ ] 5.3 Add missing game service methods
    - Implement `get_game_status()` method for game service
    - _Requirements: 2.1, 2.2_

- [ ] 6. Fix enum variant and method name issues
  - [ ] 6.1 Fix SafetyViolation enum variants
    - Replace missing `HandsOff`, `OverTemperature`, `RecentFaults` variants with available ones
    - _Requirements: 3.2_
  
  - [ ] 6.2 Fix FaultType enum variants
    - Replace missing `EmergencyStop` variant with available alternative
    - _Requirements: 3.2_
  
  - [ ] 6.3 Fix SafetyPolicy method calls
    - Replace `default_safe_torque_nm()` and `max_torque_nm()` with `get_max_torque()`
    - _Requirements: 2.1, 2.2_

- [ ] 7. Fix duration and time-related issues
  - [ ] 7.1 Replace Duration::from_minutes with Duration::from_secs
    - Update duration construction to use available API methods
    - _Requirements: 3.1_
  
  - [ ] 7.2 Fix device health status method calls
    - Remove incorrect `map_err` call on `DeviceHealthStatus`
    - Fix pattern matching on health status results
    - _Requirements: 3.1, 3.2_

- [ ] 8. Fix async lifetime and ownership issues
  - [ ] 8.1 Fix async task spawning lifetime issues
    - Resolve borrowed data escaping method body in device enumeration
    - _Requirements: 2.3_
  
  - [ ] 8.2 Fix Arc cloning for service run methods
    - Add proper cloning for wheel service run method
    - _Requirements: 2.3_

- [ ] 9. Add missing Default implementations
  - [ ] 9.1 Add Default implementation for GameSupportMatrix
    - Implement Default trait or use alternative construction method
    - _Requirements: 2.1_
  
  - [ ] 9.2 Fix ProfileId constructor calls
    - Replace `ProfileId::new()` calls with proper constructor or factory method
    - _Requirements: 3.1, 3.2_

- [ ] 10. Final compilation verification and cleanup
  - [ ] 10.1 Run cargo build to verify all errors are resolved
    - Execute `cargo build -p racing-wheel-service` to confirm compilation success
    - _Requirements: 1.1, 1.2_
  
  - [ ] 10.2 Address remaining warnings
    - Fix non-critical warnings where possible without breaking functionality
    - _Requirements: 1.3_
  
  - [ ]* 10.3 Run cargo clippy for additional code quality checks
    - Execute clippy to identify potential improvements
    - _Requirements: 1.3_
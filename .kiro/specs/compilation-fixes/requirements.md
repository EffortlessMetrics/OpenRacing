# Compilation Error Fixes — Requirements

## Introduction

This document outlines the requirements for systematically fixing compilation errors in the racing wheel software codebase. The software currently has multiple categories of compilation errors preventing successful builds, including namespace conflicts, missing imports, schema drift, and function signature mismatches that need to be resolved to restore the codebase to a buildable state while maintaining runtime behavior.

## Requirements

### 1. Root Export Conflict Resolution

**User Story:** As a developer, I want namespace conflicts to be resolved so that the codebase compiles without symbol collision errors

#### Acceptance Criteria

1. WHEN compiling the engine crate THEN FFBMode SHALL have a single canonical export path
2. WHEN using glob re-exports THEN they SHALL NOT create symbol name collisions
3. WHEN accessing exported types THEN the import path SHALL be unambiguous
4. WHEN resolving export conflicts THEN public API compatibility SHALL be maintained

### 2. Standard Library Import Resolution

**User Story:** As a developer, I want all required standard library imports to be properly declared so that the code compiles without missing symbol errors

#### Acceptance Criteria

1. WHEN compiling test modules THEN Arc and Mutex types SHALL be imported from std::sync
2. WHEN compiling safety and metrics modules THEN Duration types SHALL be imported from std::time
3. WHEN importing in test scopes THEN imports SHALL NOT shadow existing test helper types
4. WHEN adding imports THEN they SHALL be scoped to avoid production code pollution

### 3. Schema Field Migration

**User Story:** As a developer, I want struct field access to use current field names so that schema evolution doesn't break existing code

#### Acceptance Criteria

1. WHEN accessing TelemetryData fields THEN wheel_angle_deg SHALL be used instead of wheel_angle_mdeg
2. WHEN accessing TelemetryData fields THEN wheel_speed_rad_s SHALL be used instead of wheel_speed_mrad_s
3. WHEN accessing TelemetryData fields THEN temperature_c SHALL be used instead of temp_c
4. WHEN accessing TelemetryData fields THEN fault_flags SHALL be used instead of faults
5. WHEN initializing FilterConfig structs THEN bumpstop, hands_off, and torque_cap fields SHALL be provided
6. WHEN accessing removed fields THEN references SHALL be updated or removed appropriately

### 4. Function Signature Compatibility

**User Story:** As a developer, I want function calls to match their signatures so that type mismatches and arity errors don't prevent compilation

#### Acceptance Criteria

1. WHEN calling DeviceId::new THEN the parameter SHALL be a String type, not an existing DeviceId
2. WHEN calling property test functions THEN the argument count SHALL match the function signature
3. WHEN property test functions return values THEN the return type SHALL match the declared type
4. WHEN calling write_ffb_report THEN the Result return value SHALL be handled appropriately
5. WHEN accessing DeviceInfo fields THEN only existing fields SHALL be referenced

### 5. Test Code Modernization

**User Story:** As a developer, I want test code to compile and run successfully so that I can validate functionality without compilation barriers

#### Acceptance Criteria

1. WHEN running property tests THEN function signatures SHALL match between wrappers and implementations
2. WHEN tests access struct fields THEN current field names SHALL be used
3. WHEN tests call helper functions THEN function names SHALL exist and be spelled correctly
4. WHEN tests assert on struct fields THEN assertions SHALL reference existing fields
5. WHEN tests initialize structs THEN all required fields SHALL be provided

### 6. Memory Safety and Modern Rust

**User Story:** As a developer, I want the codebase to use safe, modern Rust patterns so that undefined behavior is eliminated

#### Acceptance Criteria

1. WHEN using static mutable data THEN OnceLock SHALL be used instead of static mut
2. WHEN creating shared references to mutable statics THEN safe alternatives SHALL be implemented
3. WHEN handling Results in RT paths THEN errors SHALL be counted rather than ignored
4. WHEN using unsafe patterns THEN they SHALL be replaced with safe alternatives where possible

### 7. Code Quality and Warning Elimination

**User Story:** As a developer, I want the codebase to be free of compilation warnings so that real issues are not hidden by noise

#### Acceptance Criteria

1. WHEN compiling the codebase THEN unused imports SHALL be removed or conditionally compiled
2. WHEN compiling the codebase THEN unused variables SHALL be prefixed with underscore or removed
3. WHEN compiling the codebase THEN dead code warnings SHALL be addressed with allow attributes or removal
4. WHEN compiling the codebase THEN comparison warnings for type limits SHALL be addressed
5. WHEN re-exporting test modules THEN they SHALL be conditionally compiled with cfg(test)

### 8. Regression Prevention

**User Story:** As a developer, I want safeguards in place so that similar compilation errors don't reoccur in the future

#### Acceptance Criteria

1. WHEN CI runs THEN it SHALL fail if deprecated field names are detected in the codebase
2. WHEN schemas change THEN compile-time tests SHALL validate struct instantiation
3. WHEN adding new code THEN clippy and rustfmt SHALL enforce consistent patterns
4. WHEN making API changes THEN breaking changes SHALL be detected by CI checks
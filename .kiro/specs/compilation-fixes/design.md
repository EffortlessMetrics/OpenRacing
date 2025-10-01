# Design Document

## Overview

This design addresses the 41 compilation errors in the racing-wheel-service crate by systematically fixing API mismatches, type errors, missing implementations, and structural issues. The approach focuses on maintaining existing functionality while ensuring all components compile successfully.

## Architecture

The compilation fixes are organized into several categories:

1. **Trait Compatibility Issues** - Fix async trait object compatibility
2. **API Method Mismatches** - Align service implementations with trait definitions
3. **Type System Corrections** - Fix type mismatches and missing fields
4. **Import and Dependency Resolution** - Fix module imports and external dependencies
5. **Configuration Structure Updates** - Update config structs to match current schemas

## Components and Interfaces

### 1. Diagnostic Service Fixes

**Issue**: `DiagnosticTest` trait is not dyn compatible due to async methods
**Solution**: 
- Convert async trait methods to return `Pin<Box<dyn Future>>` for dyn compatibility
- Or use `async-trait` crate properly with boxed futures
- Fix type annotation issues for numeric types and vector collections

**Key Changes**:
- Make `DiagnosticTest` trait dyn-compatible
- Fix ambiguous numeric type in jitter calculation (specify `f64`)
- Add missing methods to `SafetyPolicy` or use existing alternatives
- Fix vector type annotations

### 2. IPC Service Implementation

**Issue**: Missing method implementations and incorrect service trait usage
**Solution**:
- Implement missing trait methods or delegate to existing service methods
- Fix import paths for service types
- Correct configuration struct field mappings

**Key Changes**:
- Add missing `list_devices()`, `get_device_status()`, `start_high_torque()`, `get_game_status()` methods
- Fix `ApplicationProfileService` import path
- Update `IpcConfig` struct usage to match current schema

### 3. Safety Service Corrections

**Issue**: Missing enum variants and incorrect method calls
**Solution**:
- Add missing enum variants or use existing alternatives
- Fix `TorqueNm` construction calls
- Update duration method calls to use correct API

**Key Changes**:
- Fix `FaultType::EmergencyStop` and `SafetyViolation` enum variants
- Correct `TorqueNm::from()` usage with proper type construction
- Replace `Duration::from_minutes()` with `Duration::from_secs()`

### 4. Configuration and Schema Updates

**Issue**: Missing fields in config structs and enum variants
**Solution**:
- Add missing fields to configuration structs
- Update enum variant references to match current definitions
- Fix transport type configurations

**Key Changes**:
- Add `bumpstop`, `hands_off`, and `torque_cap` fields to `FilterConfig`
- Fix `TransportType` enum variant references
- Update `CalibrationData` struct field names

### 5. External Dependency Fixes

**Issue**: Incorrect usage of external crate APIs
**Solution**:
- Update tracing-subscriber API usage
- Fix prometheus encoder usage
- Correct async trait implementations

**Key Changes**:
- Import `tracing_subscriber::Layer` trait for `boxed()` method
- Remove unused imports and fix method calls
- Update observability layer configuration

## Data Models

### Error Categories and Fixes

```rust
// Category 1: Trait Compatibility
trait DiagnosticTest: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn run(&self, system_info: &SystemInfo) -> Pin<Box<dyn Future<Output = Result<DiagnosticResult>> + Send + '_>>;
}

// Category 2: Type Corrections
let max_jitter = jitters.iter().fold(0.0f64, |a, &b| a.max(b));
let suggested_actions: Vec<String> = Vec::new();

// Category 3: Configuration Updates
let config = IpcConfig {
    transport: TransportConfig::Tcp {
        bind_address: Some("127.0.0.1".to_string()),
        port: 50051,
    },
    max_connections: 100,
    connection_timeout: Duration::from_secs(30),
    enable_acl: false,
};

// Category 4: Service Method Implementations
impl WheelService for WheelServiceImpl {
    async fn list_devices(&self, _request: Request<()>) -> Result<Response<DeviceList>, Status> {
        // Implementation using existing device service methods
    }
}
```

## Error Handling

### Compilation Error Resolution Strategy

1. **Immediate Fixes**: Address syntax errors and missing imports first
2. **Type System**: Fix type mismatches and missing fields
3. **API Alignment**: Ensure service implementations match trait definitions
4. **Integration**: Verify all components work together after fixes

### Error Categories

- **E0038**: Trait object compatibility issues
- **E0599**: Missing methods or incorrect method names
- **E0308**: Type mismatches
- **E0560**: Missing struct fields
- **E0432**: Import resolution failures

## Testing Strategy

### Validation Approach

1. **Compilation Verification**: Ensure `cargo build -p racing-wheel-service` succeeds
2. **Warning Reduction**: Address non-critical warnings where possible
3. **Functionality Preservation**: Verify existing behavior is maintained
4. **Integration Testing**: Ensure services work together correctly

### Test Categories

- **Build Tests**: Verify compilation succeeds
- **Unit Tests**: Ensure individual components function correctly
- **Integration Tests**: Verify service interactions work properly
- **Regression Tests**: Ensure fixes don't break existing functionality

## Implementation Notes

### Priority Order

1. Fix trait compatibility issues (async trait objects)
2. Resolve import and dependency issues
3. Correct type mismatches and missing fields
4. Implement missing service methods
5. Update configuration structures
6. Address remaining warnings

### Compatibility Considerations

- Maintain existing API contracts where possible
- Use deprecation warnings for breaking changes
- Ensure backward compatibility for configuration files
- Preserve existing error handling patterns

### Performance Impact

- Minimal performance impact expected
- Async trait object changes may have slight overhead
- Configuration updates should not affect runtime performance
- Service method implementations should maintain existing performance characteristics